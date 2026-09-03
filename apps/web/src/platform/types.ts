import type { DeepReadonly, IndexedAlbum, Photo } from "../contracts";
import type { DisplayDynamicRange } from "../sources/jpeg-hdr";
import type { JpegGainMapRuntimeCapability } from "../viewer/hdr";

export type RuntimeKind = "browser" | "tauri";
export type ApplicationMenuAction = "open-folder" | "refresh";
export type FolderAccessKind =
  "directory-handle" | "recursive-file-input" | "native-dialog" | "none";

export interface RuntimeCapabilities {
  readonly runtime: RuntimeKind;
  readonly folderAccess: FolderAccessKind;
  readonly fileSelection: boolean;
  readonly dragAndDrop: boolean;
  readonly persistentDirectoryPermission: boolean;
  readonly analysis: boolean;
  readonly writesAnalysis: boolean;
  readonly nativePhotoUrls: boolean;
  readonly heicDisplay: "probe-on-selection" | "native-cache";
  readonly workerHeic: boolean;
  readonly pwaInstalled: boolean;
}

export type SourceSelectionKind = "folder" | "files" | "analysis-with-files" | "analysis-only";

export interface SourceSelection {
  readonly id: string;
  readonly kind: SourceSelectionKind;
  readonly label: string;
  readonly destinationLabel?: string;
  readonly hasAnalysis: boolean;
  readonly needsReauthorization: boolean;
  readonly writeTargetLabel?: string;
}

export type ResolvedPhotoSourceKind = "exact" | "sidecar" | "decoded" | "native";
export type ResolvedPhotoDisplayFormat = "jpeg" | "png" | "heic";

export interface PhotoResolutionOptions {
  readonly allowHighMemoryHeic?: boolean;
}

export interface ResolvedPhotoSource {
  readonly photoId: string;
  readonly sourceKind: ResolvedPhotoSourceKind;
  readonly displayFormat: ResolvedPhotoDisplayFormat;
  readonly displayUrl: string;
  readonly originalPath: string;
  readonly displayPath: string;
  readonly width?: number;
  readonly height?: number;
  readonly warnings: readonly string[];
  readonly displayDynamicRange: DisplayDynamicRange;
  readonly jpegGainMapRuntimeCapability: JpegGainMapRuntimeCapability;
  release(): void;
}

export type AnalysisStage =
  "discover" | "inspect" | "candidates" | "features" | "pairs" | "grouping" | "persist";

export type AnalysisProgressEvent =
  | { readonly kind: "stage-started"; readonly stage: AnalysisStage; readonly total: number | null }
  | {
      readonly kind: "advanced";
      readonly stage: AnalysisStage;
      readonly done: number;
      readonly total: number | null;
      readonly detail: string | null;
    }
  | { readonly kind: "stage-finished"; readonly stage: AnalysisStage; readonly elapsedMs: number }
  | { readonly kind: "warning"; readonly stage: AnalysisStage; readonly message: string };

export interface AnalysisOptions {
  readonly persist?: boolean;
  readonly reuseCache?: boolean;
  readonly sessionGapSeconds?: number;
  readonly fieldOfViewTolerance?: number;
  readonly compatibilityThreshold?: number;
  readonly analysisLongestEdge?: number;
}

export interface AnalysisRunCallbacks {
  readonly signal?: AbortSignal;
  readonly onProgress?: (event: AnalysisProgressEvent) => void;
}

export interface PlatformAdapter {
  readonly capabilities: RuntimeCapabilities;
  chooseFolder(): Promise<SourceSelection | undefined>;
  chooseFiles(): Promise<SourceSelection | undefined>;
  acceptDrop(dataTransfer: DataTransfer): Promise<SourceSelection | undefined>;
  listenForNativeDrops?(
    onSelection: (selection: SourceSelection) => void,
    onError?: (error: unknown) => void,
  ): Promise<() => void>;
  listenForApplicationMenu?(onAction: (action: ApplicationMenuAction) => void): Promise<() => void>;
  restorePersistedFolder(): Promise<SourceSelection | undefined>;
  clearRetainedData(): Promise<void>;
  reauthorize(selection: SourceSelection): Promise<SourceSelection>;
  loadAnalysis(selection: SourceSelection, signal?: AbortSignal): Promise<IndexedAlbum | undefined>;
  analyze(
    selection: SourceSelection,
    options?: AnalysisOptions,
    callbacks?: AnalysisRunCallbacks,
  ): Promise<IndexedAlbum>;
  cancelAnalysis(): Promise<boolean>;
  resolvePhoto(
    selection: SourceSelection,
    photo: DeepReadonly<Photo>,
    signal?: AbortSignal,
    options?: PhotoResolutionOptions,
  ): Promise<ResolvedPhotoSource>;
  releaseSelection(selection: SourceSelection): void;
}

export class PlatformOperationError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "PlatformOperationError";
    this.code = code;
  }
}
