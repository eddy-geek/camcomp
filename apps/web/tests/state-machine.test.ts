import { describe, expect, it, vi } from "vitest";

import { normalizeAnalysisAlbum } from "../src/contracts";
import { ImportController, initialImportState, reduceImportState } from "../src/import";
import type { PlatformAdapter, SourceSelection } from "../src/platform";
import { validAnalysisDocument } from "./fixtures";

const cachedSelection: SourceSelection = Object.freeze({
  id: "cached",
  kind: "folder",
  label: "Album",
  hasAnalysis: true,
  needsReauthorization: false,
});

describe("import state isolation", () => {
  it("ignores events from stale asynchronous runs", () => {
    const current = reduceImportState(initialImportState, {
      type: "acquisition-started",
      runId: 2,
    });
    const stale = reduceImportState(current, {
      type: "failed",
      runId: 1,
      code: "late",
      message: "late failure",
    });
    expect(stale).toBe(current);
  });

  it("keeps cancellation terminal for late completion", () => {
    let state = reduceImportState(initialImportState, { type: "acquisition-started", runId: 1 });
    state = reduceImportState(state, {
      type: "analysis-started",
      runId: 1,
      selection: cachedSelection,
    });
    state = reduceImportState(state, { type: "cancellation-requested", runId: 1 });
    state = reduceImportState(state, { type: "cancelled", runId: 1 });
    const late = reduceImportState(state, {
      type: "ready",
      runId: 1,
      selection: cachedSelection,
      album: normalizeAnalysisAlbum(validAnalysisDocument()),
    });
    expect(late.phase).toBe("cancelled");
  });

  it("loads a valid cache without invoking analysis", async () => {
    const album = normalizeAnalysisAlbum(validAnalysisDocument());
    const analyze = vi.fn();
    const adapter = {
      capabilities: { analysis: true },
      loadAnalysis: vi.fn().mockResolvedValue(album),
      analyze,
      releaseSelection: vi.fn(),
    } as unknown as PlatformAdapter;
    const controller = new ImportController(adapter);
    await controller.acquire(async () => cachedSelection);
    expect(controller.state.phase).toBe("ready");
    expect(adapter.loadAnalysis).toHaveBeenCalledOnce();
    expect(analyze).not.toHaveBeenCalled();
  });

  it("surfaces browser-only photo imports as requiring native analysis", async () => {
    const selection: SourceSelection = {
      ...cachedSelection,
      id: "photos",
      kind: "files",
      hasAnalysis: false,
    };
    const adapter = {
      capabilities: { analysis: false },
      releaseSelection: vi.fn(),
    } as unknown as PlatformAdapter;
    const controller = new ImportController(adapter);
    await controller.acquire(async () => selection);
    expect(controller.state).toMatchObject({
      phase: "error",
      code: "native-analysis-required",
    });
  });

  it("requires confirmation after showing the native write target", async () => {
    const selection: SourceSelection = {
      ...cachedSelection,
      id: "native-photos",
      kind: "files",
      hasAnalysis: false,
      writeTargetLabel: "album/camcomp.json",
    };
    const analyze = vi.fn();
    const adapter = {
      capabilities: { analysis: true },
      analyze,
      releaseSelection: vi.fn(),
    } as unknown as PlatformAdapter;
    const controller = new ImportController(adapter);
    await controller.acquire(async () => selection);
    expect(controller.state).toMatchObject({
      phase: "error",
      code: "analysis-confirmation-required",
      message: expect.stringContaining("album/camcomp.json"),
    });
    expect(analyze).not.toHaveBeenCalled();
  });

  it("refreshes through incremental cache reuse", async () => {
    const album = normalizeAnalysisAlbum(validAnalysisDocument());
    const analyze = vi.fn().mockResolvedValue(album);
    const adapter = {
      capabilities: { analysis: true },
      loadAnalysis: vi.fn().mockResolvedValue(album),
      analyze,
      releaseSelection: vi.fn(),
    } as unknown as PlatformAdapter;
    const controller = new ImportController(adapter);
    await controller.acquire(async () => cachedSelection);
    await controller.refresh();
    expect(analyze).toHaveBeenCalledWith(
      cachedSelection,
      expect.objectContaining({ reuseCache: true }),
      expect.any(Object),
    );
  });
});
