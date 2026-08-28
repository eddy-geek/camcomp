import type { DeepReadonly, Photo } from "../contracts";
import type { PhotoResolutionOptions, ResolvedPhotoSource } from "../platform/types";
import { hasHeicMagicBytes, resolveHeicPhotoSource } from "./heic";
import { indexUniqueRelativePaths, normalizeRelativePath, stripSingleDirectoryRoot } from "./paths";
import { browserHeicWorkerAvailable, decodeHeicInWorker } from "./worker-heic";

export interface BrowserFileEntry {
  readonly path: string;
  readonly file: File;
}

export interface ClassifiedBrowserSelection {
  readonly analysisFile?: File;
  readonly sourceFiles: readonly BrowserFileEntry[];
  readonly kind: "files" | "analysis-with-files" | "analysis-only";
}

export interface BrowserSourceResolverOptions {
  readonly files?: readonly BrowserFileEntry[];
  readonly directory?: FileSystemDirectoryHandle;
  readonly createObjectUrl?: (blob: Blob) => string;
  readonly revokeObjectUrl?: (url: string) => void;
}

export function classifyBrowserFiles(
  files: readonly File[],
  options: { readonly stripDirectoryRoot?: boolean } = {},
): ClassifiedBrowserSelection {
  const rawPaths = files.map((file) => file.webkitRelativePath || file.name);
  const paths = options.stripDirectoryRoot
    ? stripSingleDirectoryRoot(rawPaths)
    : rawPaths.map(normalizeRelativePath);
  const entries = files.map((file, index) => ({ path: paths[index]!, file }));
  indexUniqueRelativePaths(entries.map((entry) => [entry.path, entry] as const));

  const analyses = entries.filter((entry) =>
    entry.path.split("/").at(-1)?.toLowerCase().endsWith(".json"),
  );
  if (analyses.length > 1) {
    throw new SourceResolutionError(
      "multiple-analysis-files",
      "Select only one JSON analysis at a time.",
    );
  }
  const analysis = analyses[0];
  const sourceFiles = entries.filter((entry) => entry !== analysis);
  return Object.freeze({
    analysisFile: analysis?.file,
    sourceFiles: Object.freeze(sourceFiles),
    kind: analysis ? (sourceFiles.length > 0 ? "analysis-with-files" : "analysis-only") : "files",
  });
}

export class SourceResolutionError extends Error {
  readonly code: string;
  readonly relativePath?: string;

  constructor(code: string, message: string, relativePath?: string) {
    super(message);
    this.name = "SourceResolutionError";
    this.code = code;
    this.relativePath = relativePath;
  }
}

export class BrowserSourceResolver {
  readonly #files: ReadonlyMap<string, File>;
  readonly #directory?: FileSystemDirectoryHandle;
  readonly #createObjectUrl: (blob: Blob) => string;
  readonly #revokeObjectUrl: (url: string) => void;
  readonly #activeUrls = new Set<string>();
  readonly #highMemoryHeicConsent = new Set<string>();

  constructor(options: BrowserSourceResolverOptions) {
    this.#files = indexUniqueRelativePaths(
      (options.files ?? []).map(({ path, file }) => [path, file] as const),
    );
    this.#directory = options.directory;
    this.#createObjectUrl = options.createObjectUrl ?? URL.createObjectURL.bind(URL);
    this.#revokeObjectUrl = options.revokeObjectUrl ?? URL.revokeObjectURL.bind(URL);
  }

  async getFile(relativePath: string): Promise<File | undefined> {
    const normalized = normalizeRelativePath(relativePath);
    const selected = this.#files.get(normalized);
    if (selected) return selected;
    if (!this.#directory) return undefined;

    const segments = normalized.split("/");
    let directory = this.#directory;
    try {
      for (const segment of segments.slice(0, -1)) {
        directory = await directory.getDirectoryHandle(segment);
      }
      return await (await directory.getFileHandle(segments.at(-1)!)).getFile();
    } catch (error) {
      if (isNotFoundError(error)) return undefined;
      throw error;
    }
  }

  async resolvePhoto(
    photo: DeepReadonly<Photo>,
    signal?: AbortSignal,
    options: PhotoResolutionOptions = {},
  ): Promise<ResolvedPhotoSource> {
    signal?.throwIfAborted();
    const file = await this.getFile(photo.path);
    signal?.throwIfAborted();
    if (!file) {
      throw new SourceResolutionError(
        "missing-photo",
        `The source photo “${normalizeRelativePath(photo.path)}” is missing.`,
        normalizeRelativePath(photo.path),
      );
    }

    if (await isHeicCandidate(file)) {
      const relativePath = normalizeRelativePath(photo.path);
      if (options.allowHighMemoryHeic) this.#highMemoryHeicConsent.add(relativePath);
      return resolveHeicPhotoSource({
        photo,
        exactFile: file,
        getFile: (path) => this.getFile(path),
        createObjectUrl: (blob) => this.#trackObjectUrl(blob),
        revokeObjectUrl: (url) => this.#releaseObjectUrl(url),
        decodeWithWorker: browserHeicWorkerAvailable() ? decodeHeicInWorker : undefined,
        allowHighMemoryWorker: this.#highMemoryHeicConsent.has(relativePath),
        signal,
      });
    }

    const displayUrl = this.#trackObjectUrl(file);
    return Object.freeze({
      photoId: photo.id,
      sourceKind: "exact" as const,
      displayUrl,
      originalPath: normalizeRelativePath(photo.path),
      displayPath: normalizeRelativePath(photo.path),
      warnings: Object.freeze(cheapFingerprintWarnings(photo, file)),
      displayDynamicRange: { kind: "unverified", reason: "range-unsupported" } as const,
      jpegGainMapRuntimeCapability: "unverified",
      release: () => this.#releaseObjectUrl(displayUrl),
    });
  }

  release(): void {
    for (const url of this.#activeUrls) this.#revokeObjectUrl(url);
    this.#activeUrls.clear();
    this.#highMemoryHeicConsent.clear();
  }

  #trackObjectUrl(blob: Blob): string {
    const url = this.#createObjectUrl(blob);
    this.#activeUrls.add(url);
    return url;
  }

  #releaseObjectUrl(url: string): void {
    if (!this.#activeUrls.delete(url)) return;
    this.#revokeObjectUrl(url);
  }
}

function cheapFingerprintWarnings(photo: DeepReadonly<Photo>, file: File): string[] {
  const warnings: string[] = [];
  if (file.size !== photo.fingerprint.size)
    warnings.push("The file size differs from the analyzed source.");
  if (photo.fingerprint.modifiedAt && file.lastModified > 0) {
    const expected = Date.parse(photo.fingerprint.modifiedAt);
    if (Number.isFinite(expected) && Math.abs(expected - file.lastModified) >= 1000) {
      warnings.push("The file modification time differs from the analyzed source.");
    }
  }
  return warnings;
}

async function isHeicCandidate(file: File): Promise<boolean> {
  const name = file.name.toLowerCase();
  return /\.(heic|heif)$/.test(name) || hasHeicMagicBytes(file);
}

function isNotFoundError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "NotFoundError";
}
