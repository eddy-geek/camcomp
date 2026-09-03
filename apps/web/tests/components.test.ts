import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import AlbumShell from "../src/components/AlbumShell.svelte";
import GroupViewer from "../src/components/GroupViewer.svelte";
import ImportPanel from "../src/components/ImportPanel.svelte";
import PhotoFrame from "../src/components/PhotoFrame.svelte";
import ProgressPanel from "../src/components/ProgressPanel.svelte";
import { normalizeAnalysisAlbum } from "../src/contracts";
import type { ResolvedPhotoSource, RuntimeCapabilities, SourceSelection } from "../src/platform";
import { NavigationController } from "../src/viewer";
import { validAnalysisDocument, validPhoto } from "./fixtures";

vi.mock("../src/viewer/openseadragon", () => ({
  OpenSeadragonViewerAdapter: class {
    open = vi.fn(async () => undefined);
    viewport = vi.fn(() => null);
    applyViewport = vi.fn();
    applyPixelRatio = vi.fn();
    fit = vi.fn();
    zoomBy = vi.fn();
    onInteraction = vi.fn(() => () => undefined);
    destroy = vi.fn();
  },
}));

const browserCapabilities: RuntimeCapabilities = {
  runtime: "browser",
  folderAccess: "recursive-file-input",
  fileSelection: true,
  dragAndDrop: true,
  persistentDirectoryPermission: false,
  analysis: false,
  writesAnalysis: false,
  nativePhotoUrls: false,
  heicDisplay: "probe-on-selection",
  workerHeic: false,
  pwaInstalled: false,
};

const selection: SourceSelection = {
  id: "fixture",
  kind: "folder",
  label: "Fixture album",
  hasAnalysis: true,
  needsReauthorization: false,
};

afterEach(() => cleanup());

describe("import interface", () => {
  it("exposes keyboard-reachable acquisition actions and local capability details", async () => {
    const chooseFolder = vi.fn();
    render(ImportPanel, {
      capabilities: browserCapabilities,
      onChooseFolder: chooseFolder,
      onChooseFiles: vi.fn(),
      onRestore: vi.fn(),
      onClearRetained: vi.fn(),
      onDrop: vi.fn(),
    });
    await fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
    expect(chooseFolder).toHaveBeenCalledOnce();
    expect(screen.getByText("Reselect after restart")).toBeVisible();
    expect(screen.getByText("Local only")).toBeVisible();
  });

  it("keeps accumulated progress warnings visible", () => {
    render(ProgressPanel, {
      phase: "analyzing",
      progress: { kind: "advanced", stage: "pairs", done: 3, total: 9, detail: null },
      warnings: ["One source is missing metadata."],
      selectionLabel: "Fixture album",
      writeTargetLabel: "Fixture album/camcomp.json",
      onCancel: vi.fn(),
    });
    expect(screen.getByText("One source is missing metadata.")).toBeVisible();
    expect(screen.getByText("Result:")).toHaveTextContent("Fixture album/camcomp.json");
  });
});

describe("high-memory HEIC confirmation", () => {
  it("requires an explicit action before starting the measured worker path", async () => {
    const confirmation = Object.assign(
      new Error("Browser HEIC decoding measured about 2,120 MiB peak RAM."),
      { code: "high-memory-heic-confirmation" as const },
    );
    const resolved: ResolvedPhotoSource = {
      photoId: "photo-a",
      sourceKind: "decoded",
      displayUrl: "blob:decoded",
      originalPath: "a.heic",
      displayPath: "a.heic (lossless worker decode)",
      warnings: ["High-memory HEIC worker enabled."],
      displayDynamicRange: { kind: "sdr" },
      jpegGainMapRuntimeCapability: "unverified",
      release: vi.fn(),
    };
    const resolveSource = vi
      .fn()
      .mockRejectedValueOnce(confirmation)
      .mockResolvedValueOnce(resolved);
    const controller = new NavigationController({ anchorPhotoId: "photo-a" });
    render(PhotoFrame, {
      photo: { ...validPhoto(), path: "a.heic" },
      controller,
      resolveSource,
      releaseSource: vi.fn(),
      alignmentStatus: "Anchor",
    });

    expect(await screen.findByText(/2,120 MiB peak RAM/i)).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "Decode HEIC anyway" }));
    await waitFor(() =>
      expect(resolveSource).toHaveBeenLastCalledWith(expect.objectContaining({ id: "photo-a" }), {
        allowHighMemoryHeic: true,
      }),
    );
    expect(await screen.findByText("Decoded copy")).toBeVisible();
    controller.destroy();
  });
});

describe("photo details", () => {
  it("shows structured native errors without stringifying the object", async () => {
    render(PhotoFrame, {
      photo: validPhoto(),
      controller: new NavigationController({ anchorPhotoId: "photo-a" }),
      resolveSource: vi
        .fn()
        .mockRejectedValue({ code: "io", message: "The source photo could not be opened." }),
      releaseSource: vi.fn(),
      alignmentStatus: "Anchor",
    });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Missing source: The source photo could not be opened.",
    );
    expect(screen.queryByText(/\[object Object\]/u)).not.toBeInTheDocument();
  });

  it("exposes complete values for every ellipsized photo field", async () => {
    const photo = validPhoto();
    photo.path = "photos/a-very-long-comparison-photo-name.jpg";
    photo.metadata.effectiveFocalLength35mm = 23;
    photo.metadata.cameraMake = "OPPO";
    photo.metadata.cameraModel = "Find X9 Pro Ultra Long Camera Name";
    photo.metadata.modeLabel = "master";
    const resolved: ResolvedPhotoSource = {
      photoId: photo.id,
      sourceKind: "exact",
      displayUrl: "blob:photo",
      originalPath: photo.path,
      displayPath: photo.path,
      warnings: [],
      displayDynamicRange: { kind: "sdr" },
      jpegGainMapRuntimeCapability: "native-supported",
      release: vi.fn(),
    };
    const alignmentStatus = "direct-pair · 93% · detailed alignment provenance";
    const rendered = render(PhotoFrame, {
      photo,
      controller: new NavigationController({ anchorPhotoId: photo.id }),
      resolveSource: vi.fn().mockResolvedValue(resolved),
      releaseSource: vi.fn(),
      alignmentStatus,
    });

    expect(
      await rendered.findByRole("heading", { name: "a-very-long-comparison-photo-name.jpg" }),
    ).toHaveAttribute("title", "a-very-long-comparison-photo-name.jpg");
    expect(rendered.getByText(alignmentStatus)).toHaveAttribute("title", alignmentStatus);
    expect(rendered.getByText("23 mm equivalent")).toHaveAttribute("title", "23 mm equivalent");
    expect(rendered.getByText("4 × 3")).toHaveAttribute("title", "4 × 3");
    expect(rendered.getByText("OPPO Find X9 Pro Ultra Long Camera Name")).toHaveAttribute(
      "title",
      "OPPO Find X9 Pro Ultra Long Camera Name",
    );
    expect(rendered.getByText("master")).toHaveAttribute("title", "master");
  });
});

describe("HDR presentation", () => {
  it("shows the computed status and removes the display listener on destroy", async () => {
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    vi.stubGlobal("matchMedia", () => ({
      matches: true,
      addEventListener,
      removeEventListener,
    }));
    const resolved: ResolvedPhotoSource = {
      photoId: "photo-a",
      sourceKind: "exact",
      displayUrl: "blob:hdr",
      originalPath: "a.jpg",
      displayPath: "a.jpg",
      warnings: ["HDR metadata unverified"],
      displayDynamicRange: {
        kind: "jpeg-gain-map",
        encoding: "ultra-hdr-v1",
        validation: "trailing-jpeg-validated",
      },
      jpegGainMapRuntimeCapability: "native-supported",
      release: vi.fn(),
    };
    const rendered = render(PhotoFrame, {
      photo: validPhoto(),
      controller: new NavigationController({ anchorPhotoId: "photo-a" }),
      resolveSource: vi.fn().mockResolvedValue(resolved),
      releaseSource: vi.fn(),
      alignmentStatus: "Anchor",
    });
    const statusMessage = await rendered.findByText("HDR path active");
    const statusPopover = statusMessage.closest(".status-popover");
    expect(statusPopover).not.toBeNull();
    expect(statusPopover).toHaveStyle({ position: "fixed" });
    expect(
      rendered.getByRole("button", {
        name: /photo status: hdr path active hdr metadata unverified/i,
      }),
    ).toBeVisible();
    const details = rendered.getByRole("heading", { name: "a.jpg" }).closest(".photo-details");
    expect(details).not.toBeNull();
    expect(details).toHaveTextContent("Anchor");
    expect(details).toHaveTextContent("Focal length");
    expect(addEventListener).toHaveBeenCalledOnce();
    rendered.unmount();
    expect(removeEventListener).toHaveBeenCalledOnce();
    vi.unstubAllGlobals();
  });

  it("exposes the group-wide Compare in SDR control", async () => {
    const document = validAnalysisDocument();
    const second = structuredClone(validPhoto());
    second.id = "photo-b";
    second.path = "photos/b.jpg";
    document.photos.push(second);
    document.groups = [
      {
        id: "group-1",
        anchorPhotoId: "photo-a",
        photoIds: ["photo-a", "photo-b"],
        evidence: {
          compatibleMemberPairs: 1,
          incompatibleMemberPairs: 0,
          unverifiedMemberPairs: 0,
        },
      },
    ];
    document.ungrouped = [];
    const album = normalizeAnalysisAlbum(document);
    const modeChange = vi.fn();
    const pixelZoom = vi.spyOn(NavigationController.prototype, "zoomToPixelRatio");
    const pan = vi.spyOn(NavigationController.prototype, "panBy");
    const zoom = vi.spyOn(NavigationController.prototype, "zoomBy");
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    render(GroupViewer, {
      album,
      group: album.document.groups[0]!,
      resolveSource: vi.fn(),
      releaseSource: vi.fn(),
      dynamicRangeMode: "automatic",
      onDynamicRangeModeChange: modeChange,
    });
    await fireEvent.change(screen.getByLabelText("Dynamic range"), { target: { value: "sdr" } });
    expect(modeChange).toHaveBeenCalledWith("sdr");
    expect(screen.getByRole("option", { name: "Compare in SDR" })).toBeVisible();
    await fireEvent.click(screen.getByRole("button", { name: "100%" }));
    await fireEvent.click(screen.getByRole("button", { name: "200%" }));
    expect(pixelZoom).toHaveBeenNthCalledWith(1, 1);
    expect(pixelZoom).toHaveBeenNthCalledWith(2, 2);
    await fireEvent.keyDown(window, { key: "ArrowLeft" });
    await fireEvent.keyDown(window, { key: "+" });
    expect(pan).toHaveBeenCalledWith(-0.1, 0);
    expect(zoom).toHaveBeenCalledWith(1.25);
    await fireEvent.keyDown(screen.getByLabelText("Navigation"), { key: "ArrowRight" });
    expect(pan).toHaveBeenCalledTimes(1);
    pixelZoom.mockRestore();
    pan.mockRestore();
    zoom.mockRestore();
    vi.unstubAllGlobals();
  });
});

describe("album result", () => {
  it("keeps ungrouped inputs and reasons visible", () => {
    render(AlbumShell, {
      album: normalizeAnalysisAlbum(validAnalysisDocument()),
      selection,
      canRefresh: false,
      onRefresh: vi.fn(),
      onReset: vi.fn(),
      onReauthorize: vi.fn(),
      resolveSource: vi.fn(),
      releaseSource: vi.fn<(source: ResolvedPhotoSource) => void>(),
    });
    expect(screen.getByText("photos/a.jpg")).toBeVisible();
    expect(screen.getByText("no-session-peer")).toBeVisible();
    expect(screen.getByRole("heading", { name: "No comparison groups" })).toBeVisible();
  });

  it("sorts groups by capture time and separates capture days", async () => {
    const document = validAnalysisDocument();
    const groupFixtures = [
      { id: "next", correctedAt: "2026-08-23T09:00:00Z" },
      { id: "missing", correctedAt: undefined },
      { id: "late", correctedAt: "2026-08-22T14:00:00Z" },
      { id: "early", correctedAt: "2026-08-22T10:00:00Z" },
    ];
    document.photos = groupFixtures.flatMap(({ id, correctedAt }) =>
      ["a", "b"].map((suffix) => ({
        ...structuredClone(validPhoto()),
        id: `photo-${id}-${suffix}`,
        path: `photos/${id}-${suffix}.jpg`,
        metadata: {
          ...validPhoto().metadata,
          correctedAt,
          effectiveFocalLength35mm: id === "early" && suffix === "a" ? 24 : undefined,
        },
      })),
    );
    document.groups = groupFixtures.map(({ id }) => ({
      id: `group-${id}`,
      anchorPhotoId: `photo-${id}-a`,
      photoIds: [`photo-${id}-a`, `photo-${id}-b`],
      flags: id === "early" ? ["near-threshold"] : undefined,
      evidence: {
        compatibleMemberPairs: 0,
        incompatibleMemberPairs: 0,
        unverifiedMemberPairs: 0,
      },
    }));
    document.ungrouped = [];
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );

    const rendered = render(AlbumShell, {
      album: normalizeAnalysisAlbum(document),
      selection,
      canRefresh: false,
      onRefresh: vi.fn(),
      onReset: vi.fn(),
      onReauthorize: vi.fn(),
      resolveSource: vi.fn(),
      releaseSource: vi.fn<(source: ResolvedPhotoSource) => void>(),
    });

    expect(screen.getByRole("heading", { name: "early-a.jpg" })).toBeVisible();
    expect(
      [...rendered.container.querySelectorAll("nav time")].map((time) =>
        time.getAttribute("datetime"),
      ),
    ).toEqual([
      "2026-08-22T10:00:00.000Z",
      "2026-08-22T14:00:00.000Z",
      "2026-08-23T09:00:00.000Z",
      null,
    ]);
    expect(rendered.container.querySelectorAll(".group-day")).toHaveLength(3);
    expect(rendered.container.querySelectorAll(".group-button .group-row")).toHaveLength(8);
    expect(screen.getByText("24 mm eq.")).toBeVisible();
    expect(screen.getByText("near-threshold")).toHaveAttribute("title", "near-threshold");
    expect(screen.getAllByText("Focal unavailable")).toHaveLength(3);
    expect(screen.getByText("Time unavailable")).toBeVisible();

    await fireEvent.keyDown(window, { key: "PageDown" });
    expect(screen.getByRole("heading", { name: "late-a.jpg" })).toBeVisible();
    await fireEvent.keyDown(window, { key: " " });
    expect(screen.getByRole("heading", { name: "next-a.jpg" })).toBeVisible();
    await fireEvent.keyDown(window, { key: "PageUp" });
    expect(screen.getByRole("heading", { name: "late-a.jpg" })).toBeVisible();
    vi.unstubAllGlobals();
  });
});
