import {
  normalizeAnalysisAlbum,
  parseAnalysisDocumentJson,
  type DeepReadonly,
  type Photo,
} from "../contracts";
import { stripSingleDirectoryRoot } from "../sources/paths";
import { annotateResolvedPhotoSource } from "../sources/jpeg-hdr";
import { browserHeicWorkerAvailable } from "../sources/worker-heic";
import {
  BrowserSourceResolver,
  classifyBrowserFiles,
  SourceResolutionError,
} from "../sources/resolver";
import type {
  AnalysisOptions,
  AnalysisRunCallbacks,
  PhotoResolutionOptions,
  PlatformAdapter,
  ResolvedPhotoSource,
  RuntimeCapabilities,
  SourceSelection,
} from "./types";
import { detectJpegGainMapRuntimeCapability } from "../viewer/hdr";
import { PlatformOperationError } from "./types";

const DIRECTORY_DATABASE = "camcomp-directory-handles";
const DIRECTORY_STORE = "handles";
const LAST_DIRECTORY_KEY = "last-source";

interface BrowserSelectionRecord {
  readonly selection: SourceSelection;
  readonly resolver: BrowserSourceResolver;
  readonly analysisFile?: File;
  readonly directory?: FileSystemDirectoryHandle;
}

export function detectBrowserCapabilities(scope: Window = window): RuntimeCapabilities {
  const directoryHandles = typeof scope.showDirectoryPicker === "function";
  return Object.freeze({
    runtime: "browser",
    folderAccess: directoryHandles ? "directory-handle" : "recursive-file-input",
    fileSelection: typeof document !== "undefined" && typeof File !== "undefined",
    dragAndDrop: typeof DataTransfer !== "undefined",
    persistentDirectoryPermission: directoryHandles && typeof indexedDB !== "undefined",
    analysis: false,
    writesAnalysis: false,
    nativePhotoUrls: false,
    heicDisplay: "probe-on-selection",
    workerHeic: browserHeicWorkerAvailable(scope),
    pwaInstalled: scope.matchMedia?.("(display-mode: standalone)").matches ?? false,
  });
}

export class BrowserPlatformAdapter implements PlatformAdapter {
  readonly capabilities: RuntimeCapabilities;
  readonly #scope: Window;
  readonly #jpegGainMapRuntimeCapability: ReturnType<typeof detectJpegGainMapRuntimeCapability>;
  readonly #records = new Map<string, BrowserSelectionRecord>();
  #nextSelectionId = 1;

  constructor(scope: Window = window) {
    this.#scope = scope;
    this.#jpegGainMapRuntimeCapability = detectJpegGainMapRuntimeCapability({
      runtime: "browser",
      userAgent: scope.navigator?.userAgent ?? "",
    });
    this.capabilities = detectBrowserCapabilities(scope);
  }

  async chooseFolder(): Promise<SourceSelection | undefined> {
    if (this.#scope.showDirectoryPicker) {
      try {
        const directory = await this.#scope.showDirectoryPicker();
        const selection = await this.#registerDirectory(directory, false);
        await persistDirectoryHandle(directory).catch(() => undefined);
        return selection;
      } catch (error) {
        if (isUserCancellation(error)) return undefined;
        if (!isPermissionDenial(error)) throw error;
      }
    }
    const files = await chooseFilesWithInput(true);
    if (!files) return undefined;
    const classified = classifyBrowserFiles(files, { stripDirectoryRoot: true });
    const rootRelativePaths = stripSingleDirectoryRoot(
      files.map((file) => file.webkitRelativePath || file.name),
    );
    const analysisIndex = classified.analysisFile ? files.indexOf(classified.analysisFile) : -1;
    if (analysisIndex >= 0 && rootRelativePaths[analysisIndex] !== "camcomp.json") {
      throw new SourceResolutionError(
        "ambiguous-analysis-file",
        "A folder import accepts only a root camcomp.json analysis.",
      );
    }
    return this.#register({
      kind: "folder",
      label: selectedFolderLabel(files),
      analysisFile: classified.analysisFile,
      resolver: new BrowserSourceResolver({ files: classified.sourceFiles }),
    });
  }

  async chooseFiles(): Promise<SourceSelection | undefined> {
    const files = await chooseFilesWithInput(false);
    return files ? this.#registerClassified(files) : undefined;
  }

  async acceptDrop(dataTransfer: DataTransfer): Promise<SourceSelection | undefined> {
    const handles = await Promise.all(
      [...dataTransfer.items]
        .filter((item) => item.kind === "file" && item.getAsFileSystemHandle)
        .map((item) => item.getAsFileSystemHandle!()),
    );
    const presentHandles = handles.filter((handle): handle is FileSystemHandle => handle !== null);
    const directories = presentHandles.filter(
      (handle): handle is FileSystemDirectoryHandle => handle.kind === "directory",
    );
    if (directories.length > 1 || (directories.length === 1 && presentHandles.length > 1)) {
      throw new PlatformOperationError(
        "ambiguous-drop",
        "Drop one folder, or one analysis with its photo files.",
      );
    }
    if (directories[0]) return this.#registerDirectory(directories[0], false);

    const handleFiles = await Promise.all(
      presentHandles
        .filter((handle): handle is FileSystemFileHandle => handle.kind === "file")
        .map((handle) => handle.getFile()),
    );
    const files = handleFiles.length > 0 ? handleFiles : [...dataTransfer.files];
    return files.length > 0 ? this.#registerClassified(files) : undefined;
  }

  async restorePersistedFolder(): Promise<SourceSelection | undefined> {
    const directory = await readPersistedDirectoryHandle().catch(() => undefined);
    if (!directory) return undefined;
    const permission = await queryReadPermission(directory);
    return this.#registerDirectory(directory, permission !== "granted");
  }

  async clearRetainedData(): Promise<void> {
    if (typeof indexedDB === "undefined") return;
    await new Promise<void>((resolve, reject) => {
      const request = indexedDB.deleteDatabase(DIRECTORY_DATABASE);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error("The retained folder database is in use."));
    });
  }

  async reauthorize(selection: SourceSelection): Promise<SourceSelection> {
    const record = this.#record(selection);
    if (!record.directory && record.analysisFile) {
      if (this.#scope.showDirectoryPicker) {
        try {
          const directory = await this.#scope.showDirectoryPicker();
          const replacement = this.#register({
            kind: "analysis-with-files",
            label: directory.name,
            analysisFile: record.analysisFile,
            directory,
            hasAnalysis: true,
            resolver: new BrowserSourceResolver({ directory }),
          });
          this.releaseSelection(selection);
          return replacement;
        } catch (error) {
          if (isUserCancellation(error)) return selection;
          throw error;
        }
      }
      const files = await chooseFilesWithInput(true);
      if (!files) return selection;
      const classified = classifyBrowserFiles(files, { stripDirectoryRoot: true });
      const replacement = this.#register({
        kind: "analysis-with-files",
        label: selectedFolderLabel(files),
        analysisFile: record.analysisFile,
        hasAnalysis: true,
        resolver: new BrowserSourceResolver({ files: classified.sourceFiles }),
      });
      this.releaseSelection(selection);
      return replacement;
    }
    if (!record.directory?.requestPermission) return selection;
    const permission = await record.directory.requestPermission({ mode: "read" });
    if (permission !== "granted") {
      throw new PlatformOperationError(
        "permission-denied",
        "Folder read permission was not granted.",
      );
    }
    this.releaseSelection(selection);
    return this.#registerDirectory(record.directory, false);
  }

  async loadAnalysis(selection: SourceSelection, signal?: AbortSignal) {
    const record = this.#record(selection);
    signal?.throwIfAborted();
    let file = record.analysisFile;
    if (!file && record.directory) {
      try {
        file = await (await record.directory.getFileHandle("camcomp.json")).getFile();
      } catch (error) {
        if (!isNotFound(error)) throw error;
      }
    }
    if (!file) return undefined;
    const text = await file.text();
    signal?.throwIfAborted();
    return normalizeAnalysisAlbum(parseAnalysisDocumentJson(text));
  }

  async analyze(
    _selection: SourceSelection,
    _options?: AnalysisOptions,
    _callbacks?: AnalysisRunCallbacks,
  ): Promise<never> {
    throw new PlatformOperationError(
      "native-analysis-required",
      "New analysis requires the camcomp desktop application. Browser imports are read-only.",
    );
  }

  async cancelAnalysis(): Promise<boolean> {
    return false;
  }

  async resolvePhoto(
    selection: SourceSelection,
    photo: DeepReadonly<Photo>,
    signal?: AbortSignal,
    options?: PhotoResolutionOptions,
  ): Promise<ResolvedPhotoSource> {
    const source = await this.#record(selection).resolver.resolvePhoto(photo, signal, options);
    try {
      return await annotateResolvedPhotoSource(source, this.#jpegGainMapRuntimeCapability, {
        signal,
      });
    } catch (error) {
      source.release();
      throw error;
    }
  }

  releaseSelection(selection: SourceSelection): void {
    const record = this.#records.get(selection.id);
    if (!record) return;
    record.resolver.release();
    this.#records.delete(selection.id);
  }

  async #registerDirectory(
    directory: FileSystemDirectoryHandle,
    needsReauthorization: boolean,
  ): Promise<SourceSelection> {
    let hasAnalysis = false;
    if (!needsReauthorization) {
      try {
        await directory.getFileHandle("camcomp.json");
        hasAnalysis = true;
      } catch (error) {
        if (!isNotFound(error)) throw error;
      }
    }
    return this.#register({
      kind: "folder",
      label: directory.name,
      hasAnalysis,
      needsReauthorization,
      directory,
      resolver: new BrowserSourceResolver({ directory }),
    });
  }

  #registerClassified(files: readonly File[]): SourceSelection {
    const classified = classifyBrowserFiles(files);
    return this.#register({
      kind: classified.kind,
      label: files.length === 1 ? files[0]!.name : `${files.length} selected files`,
      analysisFile: classified.analysisFile,
      resolver: new BrowserSourceResolver({ files: classified.sourceFiles }),
    });
  }

  #register(input: {
    readonly kind: SourceSelection["kind"];
    readonly label: string;
    readonly resolver: BrowserSourceResolver;
    readonly analysisFile?: File;
    readonly directory?: FileSystemDirectoryHandle;
    readonly hasAnalysis?: boolean;
    readonly needsReauthorization?: boolean;
  }): SourceSelection {
    const selection = Object.freeze({
      id: `browser-${this.#nextSelectionId++}`,
      kind: input.kind,
      label: input.label,
      hasAnalysis: input.hasAnalysis ?? input.analysisFile !== undefined,
      needsReauthorization: input.needsReauthorization ?? false,
    });
    this.#records.set(selection.id, {
      selection,
      resolver: input.resolver,
      analysisFile: input.analysisFile,
      directory: input.directory,
    });
    return selection;
  }

  #record(selection: SourceSelection): BrowserSelectionRecord {
    const record = this.#records.get(selection.id);
    if (!record || record.selection !== selection) {
      throw new PlatformOperationError(
        "invalid-selection",
        "The source selection is no longer active.",
      );
    }
    return record;
  }
}

async function chooseFilesWithInput(directory: boolean): Promise<readonly File[] | undefined> {
  if (typeof document === "undefined") {
    throw new PlatformOperationError("picker-unavailable", "A file picker is not available.");
  }
  const input = document.createElement("input");
  input.type = "file";
  input.multiple = true;
  if (directory) input.setAttribute("webkitdirectory", "");
  input.accept = directory
    ? ""
    : "application/json,image/jpeg,image/png,image/heic,image/heif,.json,.heic,.heif";
  return new Promise((resolve) => {
    let settled = false;
    const finish = (files?: readonly File[]) => {
      if (settled) return;
      settled = true;
      input.remove();
      resolve(files && files.length > 0 ? files : undefined);
    };
    input.addEventListener("change", () => finish(input.files ? [...input.files] : undefined), {
      once: true,
    });
    input.addEventListener("cancel", () => finish(), { once: true });
    input.hidden = true;
    document.body.append(input);
    input.click();
  });
}

function selectedFolderLabel(files: readonly File[]): string {
  const first = files[0]?.webkitRelativePath.replaceAll("\\", "/").split("/")[0];
  return first || "Selected folder";
}

async function queryReadPermission(directory: FileSystemDirectoryHandle): Promise<PermissionState> {
  return directory.queryPermission ? directory.queryPermission({ mode: "read" }) : "prompt";
}

async function persistDirectoryHandle(directory: FileSystemDirectoryHandle): Promise<void> {
  if (typeof indexedDB === "undefined") return;
  const database = await openDirectoryDatabase();
  await transactionPromise(database, "readwrite", (store) =>
    store.put(directory, LAST_DIRECTORY_KEY),
  );
  database.close();
}

async function readPersistedDirectoryHandle(): Promise<FileSystemDirectoryHandle | undefined> {
  if (typeof indexedDB === "undefined") return undefined;
  const database = await openDirectoryDatabase();
  const handle = await transactionPromise<FileSystemDirectoryHandle | undefined>(
    database,
    "readonly",
    (store) => store.get(LAST_DIRECTORY_KEY),
  );
  database.close();
  return handle;
}

function openDirectoryDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DIRECTORY_DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(DIRECTORY_STORE);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function transactionPromise<T = IDBValidKey>(
  database: IDBDatabase,
  mode: IDBTransactionMode,
  operation: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return new Promise((resolve, reject) => {
    const transaction = database.transaction(DIRECTORY_STORE, mode);
    const request = operation(transaction.objectStore(DIRECTORY_STORE));
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
    transaction.onerror = () => reject(transaction.error);
  });
}

function isUserCancellation(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function isPermissionDenial(error: unknown): boolean {
  return (
    error instanceof DOMException &&
    (error.name === "NotAllowedError" || error.name === "SecurityError")
  );
}

function isNotFound(error: unknown): boolean {
  return error instanceof DOMException && error.name === "NotFoundError";
}
