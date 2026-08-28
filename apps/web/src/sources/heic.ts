import type { DeepReadonly, Photo } from "../contracts";
import type { ResolvedPhotoSource } from "../platform/types";
import { normalizeRelativePath, replaceRelativePathExtension } from "./paths";
import { MEASURED_HEIC_DELTA_MIB, MEASURED_HEIC_PEAK_MIB } from "./worker-heic";

const HEIF_BRANDS = new Set(["heic", "heix", "hevc", "hevx", "heim", "heis", "mif1", "msf1"]);

export interface ImageDimensions {
  readonly width: number;
  readonly height: number;
}

export interface WorkerHeicResult extends ImageDimensions {
  readonly blob: Blob;
}

export interface HeicResolutionOptions {
  readonly photo: DeepReadonly<Photo>;
  readonly exactFile: File;
  readonly getFile: (relativePath: string) => Promise<File | undefined>;
  readonly createObjectUrl: (blob: Blob) => string;
  readonly revokeObjectUrl: (url: string) => void;
  readonly signal?: AbortSignal;
  readonly decodeProbe?: (file: File, url: string) => Promise<ImageDimensions | undefined>;
  readonly dimensionProbe?: (file: File, url: string) => Promise<ImageDimensions | undefined>;
  readonly decodeWithWorker?: (
    file: File,
    expected: readonly number[],
    signal?: AbortSignal,
  ) => Promise<WorkerHeicResult>;
  readonly allowHighMemoryWorker?: boolean;
}

export class HighMemoryHeicConfirmationRequired extends Error {
  readonly code = "high-memory-heic-confirmation";
  readonly peakMemoryMiB = MEASURED_HEIC_PEAK_MIB;
  readonly additionalMemoryMiB = MEASURED_HEIC_DELTA_MIB;

  constructor() {
    super(
      `Browser HEIC decoding measured about ${MEASURED_HEIC_PEAK_MIB.toLocaleString()} MiB peak RAM (${MEASURED_HEIC_DELTA_MIB.toLocaleString()} MiB additional) for one 50 MP photo.`,
    );
    this.name = "HighMemoryHeicConfirmationRequired";
  }
}

export class UnsupportedHeicError extends Error {
  readonly code = "unsupported-heic";
  readonly relativePath: string;

  constructor(relativePath: string) {
    super(
      `“${relativePath}” cannot be displayed by this browser and no dimension-matched JPEG or PNG sidecar was found.`,
    );
    this.name = "UnsupportedHeicError";
    this.relativePath = relativePath;
  }
}

export async function hasHeicMagicBytes(blob: Blob): Promise<boolean> {
  const bytes = await readBlobBytes(blob.slice(0, 64));
  if (bytes.length < 12 || ascii(bytes, 4, 8) !== "ftyp") return false;
  for (let offset = 8; offset + 4 <= bytes.length; offset += 4) {
    if (HEIF_BRANDS.has(ascii(bytes, offset, offset + 4))) return true;
  }
  return false;
}

export async function resolveHeicPhotoSource(
  options: HeicResolutionOptions,
): Promise<ResolvedPhotoSource> {
  const originalPath = normalizeRelativePath(options.photo.path);
  options.signal?.throwIfAborted();
  const hasHeicSignature = await hasHeicMagicBytes(options.exactFile);
  options.signal?.throwIfAborted();
  if (!hasHeicSignature) throw new UnsupportedHeicError(originalPath);

  const exactUrl = options.createObjectUrl(options.exactFile);
  const decodeProbe = options.decodeProbe ?? probeBrowserImageDimensions;
  try {
    const dimensions = await decodeProbe(options.exactFile, exactUrl);
    options.signal?.throwIfAborted();
    if (dimensions) {
      return resolvedSource(options, "exact", exactUrl, originalPath, originalPath, dimensions, []);
    }
  } catch (error) {
    if (isAbortError(error)) {
      options.revokeObjectUrl(exactUrl);
      throw error;
    }
  }
  options.revokeObjectUrl(exactUrl);

  const expected = options.photo.dimensions.oriented;
  const dimensionProbe = options.dimensionProbe ?? probeBrowserImageDimensions;
  for (const extension of ["jpg", "jpeg", "png", "JPG", "JPEG", "PNG"] as const) {
    options.signal?.throwIfAborted();
    const sidecarPath = replaceRelativePathExtension(originalPath, extension);
    const sidecar = await options.getFile(sidecarPath);
    if (!sidecar) continue;
    const sidecarUrl = options.createObjectUrl(sidecar);
    let dimensions: ImageDimensions | undefined;
    try {
      dimensions = await dimensionProbe(sidecar, sidecarUrl);
      options.signal?.throwIfAborted();
    } catch (error) {
      options.revokeObjectUrl(sidecarUrl);
      if (isAbortError(error)) throw error;
      continue;
    }
    if (dimensions && dimensions.width === expected[0] && dimensions.height === expected[1]) {
      return resolvedSource(options, "sidecar", sidecarUrl, originalPath, sidecarPath, dimensions, [
        "Displaying a same-directory sidecar because native HEIC decoding is unavailable.",
      ]);
    }
    options.revokeObjectUrl(sidecarUrl);
  }

  if (options.decodeWithWorker) {
    if (!options.allowHighMemoryWorker) throw new HighMemoryHeicConfirmationRequired();
    const decoded = await options.decodeWithWorker(options.exactFile, expected, options.signal);
    options.signal?.throwIfAborted();
    if (decoded.width !== expected[0] || decoded.height !== expected[1]) {
      throw new UnsupportedHeicError(originalPath);
    }
    const decodedUrl = options.createObjectUrl(decoded.blob);
    return resolvedSource(
      options,
      "decoded",
      decodedUrl,
      originalPath,
      `${originalPath} (lossless worker decode)`,
      decoded,
      [
        `High-memory HEIC worker enabled; a 50 MP decode measured about ${MEASURED_HEIC_PEAK_MIB.toLocaleString()} MiB peak RAM.`,
      ],
    );
  }

  throw new UnsupportedHeicError(originalPath);
}

export async function probeBrowserImageDimensions(
  file: File,
  objectUrl: string,
): Promise<ImageDimensions | undefined> {
  if (typeof createImageBitmap === "function") {
    try {
      const bitmap = await createImageBitmap(file);
      const dimensions = { width: bitmap.width, height: bitmap.height };
      bitmap.close();
      if (dimensions.width > 0 && dimensions.height > 0) return dimensions;
    } catch {
      // The HTML image path is an independent native decoder capability.
    }
  }
  if (typeof Image === "undefined") return undefined;
  return new Promise((resolve) => {
    const image = new Image();
    image.onload = () => {
      const dimensions = { width: image.naturalWidth, height: image.naturalHeight };
      image.onload = null;
      image.onerror = null;
      image.src = "";
      resolve(dimensions.width > 0 && dimensions.height > 0 ? dimensions : undefined);
    };
    image.onerror = () => {
      image.onload = null;
      image.onerror = null;
      image.src = "";
      resolve(undefined);
    };
    image.src = objectUrl;
  });
}

function resolvedSource(
  options: HeicResolutionOptions,
  sourceKind: "exact" | "sidecar" | "decoded",
  displayUrl: string,
  originalPath: string,
  displayPath: string,
  dimensions: ImageDimensions,
  warnings: readonly string[],
): ResolvedPhotoSource {
  let released = false;
  return Object.freeze({
    photoId: options.photo.id,
    sourceKind,
    displayUrl,
    originalPath,
    displayPath,
    width: dimensions.width,
    height: dimensions.height,
    warnings: Object.freeze([...warnings]),
    displayDynamicRange: { kind: "unverified", reason: "not-jpeg" } as const,
    jpegGainMapRuntimeCapability: "unverified",
    release: () => {
      if (released) return;
      released = true;
      options.revokeObjectUrl(displayUrl);
    },
  });
}

async function readBlobBytes(blob: Blob): Promise<Uint8Array> {
  if (typeof blob.arrayBuffer === "function") return new Uint8Array(await blob.arrayBuffer());
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(new Uint8Array(reader.result as ArrayBuffer));
    reader.onerror = () => reject(reader.error);
    reader.readAsArrayBuffer(blob);
  });
}

function ascii(bytes: Uint8Array, start: number, end: number): string {
  return String.fromCharCode(...bytes.slice(start, end));
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}
