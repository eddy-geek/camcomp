const MAX_COMPRESSED_BYTES = 256 * 1024 * 1024;
const MAX_DECODED_PIXELS = 60_000_000;
const HEIF_DIMENSION_PROBE_BYTES = 4 * 1024 * 1024;

export const MEASURED_HEIC_PEAK_MIB = 2120;
export const MEASURED_HEIC_DELTA_MIB = 1828;

interface WorkerDecodeResult {
  readonly blob: Blob;
  readonly width: number;
  readonly height: number;
}

type WorkerResponse =
  | { readonly ok: true; readonly blob: Blob; readonly width: number; readonly height: number }
  | { readonly ok: false; readonly message: string };

interface HeicWorkerScope {
  readonly Worker?: typeof Worker;
  readonly OffscreenCanvas?: typeof OffscreenCanvas;
  readonly createImageBitmap?: typeof createImageBitmap;
}

let decodeQueue = Promise.resolve();

export function browserHeicWorkerAvailable(scope: HeicWorkerScope = globalThis): boolean {
  return (
    typeof scope.Worker === "function" &&
    typeof scope.OffscreenCanvas === "function" &&
    typeof scope.createImageBitmap === "function"
  );
}

export interface HeifDimensions {
  readonly width: number;
  readonly height: number;
}

export async function probeHeifDimensions(file: Blob): Promise<HeifDimensions | undefined> {
  const bytes = await probeHeifBytes(file);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let largest: HeifDimensions | undefined;
  for (let offset = 4; offset + 16 <= bytes.length; offset += 1) {
    if (
      bytes[offset] !== 0x69 ||
      bytes[offset + 1] !== 0x73 ||
      bytes[offset + 2] !== 0x70 ||
      bytes[offset + 3] !== 0x65
    ) {
      continue;
    }
    const boxStart = offset - 4;
    const boxSize = view.getUint32(boxStart, false);
    if (boxSize < 20 || boxStart + boxSize > bytes.length) continue;
    const width = view.getUint32(offset + 8, false);
    const height = view.getUint32(offset + 12, false);
    if (!width || !height) continue;
    if (!largest || width * height > largest.width * largest.height) largest = { width, height };
  }
  return largest;
}

/**
 * CICP colour parameters from a HEIF `colr` box with `nclx` profile type.
 * Used to detect HDR transfer functions (PQ, HLG) without decoding pixels.
 */
export interface HeifCicp {
  /** CICP colour primaries (1 = BT.709, 9 = BT.2020, 12 = Display P3). */
  readonly primaries: number;
  /** CICP transfer characteristics (1 = BT.709, 13 = sRGB, 16 = PQ, 18 = HLG). */
  readonly transfer: number;
  /** CICP matrix coefficients. */
  readonly matrix: number;
  /** `true` when full-range samples are signalled. */
  readonly fullRange: boolean;
}

/**
 * Scan the metadata region of a HEIF file for a `colr` box carrying an `nclx`
 * colour description. The returned CICP transfer characteristic distinguishes
 * PQ (16) and HLG (18) HDR from SDR sources without pixel decoding.
 *
 * The scan is a bounded byte search over the first 4 MiB — identical to
 * {@link probeHeifDimensions} — and never allocates a full decode buffer.
 */
export async function probeHeifCicp(file: Blob): Promise<HeifCicp | undefined> {
  const bytes = await probeHeifBytes(file);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  // Scan for 'colr' box type (0x636F6C72) followed by 'nclx' (0x6E636C78).
  // A colr/nclx box is: [4-byte size][colr][nclx][2-byte primaries]
  //   [2-byte transfer][2-byte matrix][1-byte full_range].
  // Minimum useful box size: 8 (header) + 4 (nclx) + 7 (fields) = 19 bytes.
  for (let offset = 4; offset + 15 <= bytes.length; offset += 1) {
    if (
      bytes[offset] !== 0x63 || // 'c'
      bytes[offset + 1] !== 0x6f || // 'o'
      bytes[offset + 2] !== 0x6c || // 'l'
      bytes[offset + 3] !== 0x72 // 'r'
    ) {
      continue;
    }
    const boxStart = offset - 4;
    const boxSize = view.getUint32(boxStart, false);
    if (boxSize < 19 || boxStart + boxSize > bytes.length) continue;
    // Check profile type immediately after the box type.
    const profileOffset = offset + 4;
    if (
      bytes[profileOffset] !== 0x6e || // 'n'
      bytes[profileOffset + 1] !== 0x63 || // 'c'
      bytes[profileOffset + 2] !== 0x6c || // 'l'
      bytes[profileOffset + 3] !== 0x78 // 'x'
    ) {
      continue;
    }
    const primaries = view.getUint16(profileOffset + 4, false);
    const transfer = view.getUint16(profileOffset + 6, false);
    const matrix = view.getUint16(profileOffset + 8, false);
    const fullRange = (bytes[profileOffset + 10]! & 0x80) !== 0;
    return { primaries, transfer, matrix, fullRange };
  }
  return undefined;
}

async function probeHeifBytes(file: Blob): Promise<Uint8Array> {
  const probe = file.slice(0, Math.min(file.size, HEIF_DIMENSION_PROBE_BYTES));
  return new Uint8Array(await blobArrayBuffer(probe));
}

export async function decodeHeicInWorker(
  file: File,
  expected: readonly number[],
  signal?: AbortSignal,
): Promise<WorkerDecodeResult> {
  const width = expected[0];
  const height = expected[1];
  if (!width || !height || !browserHeicWorkerAvailable()) {
    throw new Error("A CSP-compatible HEIC worker is unavailable in this browser.");
  }
  if (file.size > MAX_COMPRESSED_BYTES) {
    throw new Error("The compressed HEIC exceeds the browser decoder input guard.");
  }
  const encoded = await probeHeifDimensions(file);
  signal?.throwIfAborted();
  if (!encoded) throw new Error("The HEIC dimensions could not be verified before decoding.");
  const encodedPixels = encoded.width * encoded.height;
  if (!Number.isSafeInteger(encodedPixels) || encodedPixels > MAX_DECODED_PIXELS) {
    throw new Error("The encoded HEIC dimensions exceed the browser decoder allocation guard.");
  }
  if (encoded.width !== width || encoded.height !== height) {
    throw new Error(
      `The encoded HEIC dimensions ${encoded.width}×${encoded.height} do not match ${width}×${height}.`,
    );
  }
  const run = () => runWorker(file, width, height, signal);
  const result = decodeQueue.then(run, run);
  decodeQueue = result.then(
    () => undefined,
    () => undefined,
  );
  return result;
}

function blobArrayBuffer(blob: Blob): Promise<ArrayBuffer> {
  if (typeof blob.arrayBuffer === "function") return blob.arrayBuffer();
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as ArrayBuffer);
    reader.onerror = () => reject(reader.error);
    reader.readAsArrayBuffer(blob);
  });
}

function runWorker(
  file: File,
  expectedWidth: number,
  expectedHeight: number,
  signal?: AbortSignal,
): Promise<WorkerDecodeResult> {
  signal?.throwIfAborted();
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL("../workers/heic-decoder.worker.ts", import.meta.url), {
      type: "module",
      name: "camcomp-heic-decoder",
    });
    let settled = false;
    const finish = (operation: () => void) => {
      if (settled) return;
      settled = true;
      signal?.removeEventListener("abort", abort);
      worker.terminate();
      operation();
    };
    const abort = () =>
      finish(() => reject(new DOMException("The HEIC decode was cancelled.", "AbortError")));
    worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
      const response = event.data;
      if (response.ok) {
        finish(() =>
          resolve({ blob: response.blob, width: response.width, height: response.height }),
        );
      } else {
        finish(() => reject(new Error(response.message)));
      }
    };
    worker.onerror = (event) =>
      finish(() => reject(new Error(event.message || "The HEIC worker failed.")));
    signal?.addEventListener("abort", abort, { once: true });
    worker.postMessage({
      file,
      expectedWidth,
      expectedHeight,
      maxCompressedBytes: MAX_COMPRESSED_BYTES,
      maxPixels: MAX_DECODED_PIXELS,
    });
  });
}
