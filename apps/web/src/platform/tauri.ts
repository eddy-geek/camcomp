import {
  normalizeAnalysisAlbum,
  type AnalysisDocument,
  type DeepReadonly,
  type Photo,
} from "../contracts";
import type {
  AnalysisOptions,
  AnalysisProgressEvent,
  AnalysisRunCallbacks,
  PhotoResolutionOptions,
  PlatformAdapter,
  ResolvedPhotoSource,
  RuntimeCapabilities,
  SourceSelection,
} from "./types";
import { annotateResolvedPhotoSource } from "../sources/jpeg-hdr";
import { PlatformOperationError } from "./types";

interface NativeSelectionRecord {
  readonly selection: SourceSelection;
  readonly paths: readonly string[];
  readonly analysisPath?: string;
  readonly preflight?: NativePreflight;
}

interface NativePreflight {
  readonly sourceRootLabel: string;
  readonly writeTargetLabel: string;
  readonly hasCamcompJson: boolean;
  readonly photoCount: number;
}

interface NativeAnalyzeResponse {
  readonly document: AnalysisDocument;
  readonly written?: string | null;
}

interface NativeResolvedPhoto {
  readonly photoId: string;
  readonly sourceKind: "native" | "sidecar" | "decoded";
  readonly displayPath: string;
  readonly originalPath: string;
  readonly warnings: readonly string[];
}

type NativeProgressEvent = AnalysisProgressEvent;

export const TAURI_RUNTIME_CAPABILITIES: RuntimeCapabilities = Object.freeze({
  runtime: "tauri",
  folderAccess: "native-dialog",
  fileSelection: true,
  dragAndDrop: true,
  persistentDirectoryPermission: false,
  analysis: true,
  writesAnalysis: true,
  nativePhotoUrls: true,
  heicDisplay: "native-cache",
  workerHeic: false,
  pwaInstalled: false,
});

export class TauriPlatformAdapter implements PlatformAdapter {
  readonly capabilities = TAURI_RUNTIME_CAPABILITIES;
  readonly #records = new Map<string, NativeSelectionRecord>();
  #nextSelectionId = 1;

  async chooseFolder(): Promise<SourceSelection | undefined> {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const path = await open({ directory: true, multiple: false, recursive: false });
    return typeof path === "string" ? this.#register([path], "folder") : undefined;
  }

  async chooseFiles(): Promise<SourceSelection | undefined> {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const paths = await open({
      directory: false,
      multiple: true,
      filters: [
        { name: "camcomp sources", extensions: ["json", "jpg", "jpeg", "png", "heic", "heif"] },
      ],
    });
    if (typeof paths === "string") return this.#register([paths], "files");
    return paths && paths.length > 0 ? this.#register(paths, "files") : undefined;
  }

  async acceptDrop(_dataTransfer: DataTransfer): Promise<never> {
    throw new PlatformOperationError(
      "native-drop-event-required",
      "Native drops are delivered through listenForNativeDrops().",
    );
  }

  async listenForNativeDrops(
    onSelection: (selection: SourceSelection) => void,
    onError?: (error: unknown) => void,
  ): Promise<() => void> {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    const unlisten = await getCurrentWebview().onDragDropEvent(async (event) => {
      if (event.payload.type !== "drop" || event.payload.paths.length === 0) return;
      try {
        onSelection(await this.#register(event.payload.paths, "files"));
      } catch (error) {
        onError?.(error);
      }
    });
    return unlisten;
  }

  async restorePersistedFolder(): Promise<undefined> {
    return undefined;
  }

  async clearRetainedData(): Promise<void> {}

  async reauthorize(selection: SourceSelection): Promise<SourceSelection> {
    this.#record(selection);
    return selection;
  }

  async loadAnalysis(selection: SourceSelection, signal?: AbortSignal) {
    const record = this.#record(selection);
    signal?.throwIfAborted();
    const { invoke } = await import("@tauri-apps/api/core");
    const path = record.analysisPath ?? record.paths[0];
    if (!path || !record.selection.hasAnalysis) return undefined;
    const document = await invoke<AnalysisDocument>("load_analysis", { path });
    if (record.analysisPath && record.preflight) {
      await invoke<NativePreflight>("preflight_inputs", { inputs: record.paths });
    }
    signal?.throwIfAborted();
    return normalizeAnalysisAlbum(document);
  }

  async analyze(
    selection: SourceSelection,
    options: AnalysisOptions = {},
    callbacks: AnalysisRunCallbacks = {},
  ) {
    const record = this.#record(selection);
    callbacks.signal?.throwIfAborted();
    const [{ invoke }, { listen }] = await Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
    ]);
    const unlisten = callbacks.onProgress
      ? await listen<NativeProgressEvent>("camcomp://progress", (event) => {
          if (!callbacks.signal?.aborted) callbacks.onProgress?.(normalizeProgress(event.payload));
        })
      : undefined;
    const abort = () => void invoke<boolean>("cancel_analysis");
    callbacks.signal?.addEventListener("abort", abort, { once: true });
    try {
      const response = await invoke<NativeAnalyzeResponse>("analyze", {
        request: {
          inputs: record.paths,
          persist: options.persist ?? true,
          reuseCache: options.reuseCache ?? true,
          sessionGapSeconds: options.sessionGapSeconds,
          fieldOfViewTolerance: options.fieldOfViewTolerance,
          compatibilityThreshold: options.compatibilityThreshold,
          analysisLongestEdge: options.analysisLongestEdge,
        },
      });
      callbacks.signal?.throwIfAborted();
      return normalizeAnalysisAlbum(response.document);
    } finally {
      callbacks.signal?.removeEventListener("abort", abort);
      unlisten?.();
    }
  }

  async cancelAnalysis(): Promise<boolean> {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("cancel_analysis");
  }

  async resolvePhoto(
    selection: SourceSelection,
    photo: DeepReadonly<Photo>,
    signal?: AbortSignal,
    _options?: PhotoResolutionOptions,
  ): Promise<ResolvedPhotoSource> {
    this.#record(selection);
    signal?.throwIfAborted();
    const { invoke, convertFileSrc } = await import("@tauri-apps/api/core");
    const resolved = await invoke<NativeResolvedPhoto>("resolve_photo_source", { photo });
    signal?.throwIfAborted();
    const source = Object.freeze({
      photoId: resolved.photoId,
      sourceKind: resolved.sourceKind,
      displayUrl: convertFileSrc(resolved.displayPath),
      originalPath: resolved.originalPath,
      displayPath: resolved.displayPath,
      warnings: Object.freeze([...resolved.warnings]),
      displayDynamicRange: { kind: "unverified", reason: "range-unsupported" } as const,
      jpegGainMapRuntimeCapability: "unverified" as const,
      release: () => undefined,
    });
    return annotateResolvedPhotoSource(source, "unverified", { signal });
  }

  releaseSelection(selection: SourceSelection): void {
    this.#records.delete(selection.id);
  }

  async #register(
    paths: readonly string[],
    requestedKind: "folder" | "files",
  ): Promise<SourceSelection> {
    const { invoke } = await import("@tauri-apps/api/core");
    const analysisPaths =
      requestedKind === "files" ? paths.filter((path) => path.toLowerCase().endsWith(".json")) : [];
    if (analysisPaths.length > 1) {
      throw new PlatformOperationError(
        "multiple-analysis-files",
        "Select only one JSON analysis at a time.",
      );
    }
    const analysisPath = analysisPaths[0];
    const sourcePaths = paths.filter((path) => path !== analysisPath);
    const preflight =
      sourcePaths.length > 0
        ? await invoke<NativePreflight>("preflight_inputs", { inputs: sourcePaths })
        : undefined;
    const hasAnalysis = analysisPath !== undefined || preflight?.hasCamcompJson === true;
    const kind =
      requestedKind === "folder"
        ? "folder"
        : analysisPath
          ? sourcePaths.length > 0
            ? "analysis-with-files"
            : "analysis-only"
          : "files";
    const selection = Object.freeze({
      id: `tauri-${this.#nextSelectionId++}`,
      kind,
      label:
        preflight?.sourceRootLabel ??
        (analysisPath ? nativeBasename(analysisPath) : "Selected source"),
      destinationLabel: preflight?.writeTargetLabel,
      hasAnalysis,
      needsReauthorization: false,
      writeTargetLabel: preflight?.writeTargetLabel,
    });
    const analysisInputs =
      sourcePaths.length > 0
        ? sourcePaths
        : analysisPath
          ? [nativeParentPath(analysisPath)]
          : paths;
    this.#records.set(selection.id, {
      selection,
      paths: Object.freeze([...analysisInputs]),
      analysisPath,
      preflight,
    });
    return selection;
  }

  #record(selection: SourceSelection): NativeSelectionRecord {
    const record = this.#records.get(selection.id);
    if (!record || record.selection !== selection) {
      throw new PlatformOperationError(
        "invalid-selection",
        "The native source selection is no longer active.",
      );
    }
    return record;
  }
}

function normalizeProgress(event: NativeProgressEvent): AnalysisProgressEvent {
  return event;
}

function nativeBasename(path: string): string {
  return path.split(/[\\/]/).at(-1) || "Selected source";
}

function nativeParentPath(path: string): string {
  const separator = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return separator > 0 ? path.slice(0, separator) : path;
}
