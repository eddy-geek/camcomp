import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
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
      warnings: [],
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
    expect(await rendered.findByText("HDR path active")).toBeVisible();
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
});
