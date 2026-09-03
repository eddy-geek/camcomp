import type { ResolvedPhotoSource } from "../platform/types";
import type { JpegGainMapRuntimeCapability } from "../viewer/hdr";

export const JPEG_HDR_METADATA_LIMIT = 512 * 1024;
export const XMP_PACKET_SIGNATURE = "http://ns.adobe.com/xap/1.0/\0";
export const HDR_GAIN_MAP_NAMESPACE = "http://ns.adobe.com/hdr-gain-map/1.0/";
export const CONTAINER_NAMESPACE = "http://ns.google.com/photos/1.0/container/";
export const ITEM_NAMESPACE = "http://ns.google.com/photos/1.0/container/item/";
export const ISO_21496_NAMESPACE = "urn:iso:std:iso:ts:21496:-1";

export type JpegGainMapEncoding = "ultra-hdr-v1" | "iso-21496-1" | "dual";
export type GainMapValidation = "declared" | "trailing-jpeg-validated";

export type JpegHdrInspectionReason =
  | "not-jpeg"
  | "range-unsupported"
  | "invalid-content-range"
  | "metadata-limit"
  | "malformed-jpeg"
  | "malformed-xmp"
  | "conflicting-metadata"
  | "invalid-gain-map-length"
  | "missing-mpf"
  | "invalid-trailing-jpeg"
  | "iso-only-not-validated";

export type DisplayDynamicRange =
  | { readonly kind: "sdr" }
  | {
      readonly kind: "jpeg-gain-map";
      readonly encoding: JpegGainMapEncoding;
      readonly validation: GainMapValidation;
      readonly hdrStops?: number;
    }
  | { readonly kind: "unverified"; readonly reason: JpegHdrInspectionReason };

export interface JpegHdrInspectionOptions {
  readonly fetch?: typeof globalThis.fetch;
  readonly signal?: AbortSignal;
  readonly metadataLimit?: number;
  readonly domParserFactory?: () => DOMParser;
}

export type ResolvedPhotoSourceForAnnotation = Omit<
  ResolvedPhotoSource,
  "displayDynamicRange" | "jpegGainMapRuntimeCapability"
>;

export async function inspectJpegGainMapUrl(
  url: string,
  options: JpegHdrInspectionOptions = {},
): Promise<DisplayDynamicRange> {
  const metadataLimit = options.metadataLimit ?? JPEG_HDR_METADATA_LIMIT;
  if (!Number.isSafeInteger(metadataLimit) || metadataLimit < 1) {
    return unverified("metadata-limit");
  }
  options.signal?.throwIfAborted();
  const fetchImpl = options.fetch ?? globalThis.fetch;
  if (typeof fetchImpl !== "function") return unverified("range-unsupported");

  let head: HeadRead;
  try {
    head = await readHead(url, metadataLimit, fetchImpl, options.signal);
  } catch (error) {
    if (isAbortError(error)) throw error;
    if (error instanceof InspectionFailure) return unverified(error.reason);
    return unverified("range-unsupported");
  }
  options.signal?.throwIfAborted();

  if (head.bytes.length < 2 || head.bytes[0] !== 0xff || head.bytes[1] !== 0xd8) {
    return unverified("not-jpeg");
  }

  const domParserFactory =
    options.domParserFactory ??
    (() => (typeof DOMParser === "function" ? new DOMParser() : undefined));
  const parsed = parseJpegMetadata(head.bytes, head.totalSize, head.complete, domParserFactory);
  if (parsed.kind === "error") return unverified(parsed.reason);
  if (!parsed.reachedImageData) return unverified("malformed-jpeg");
  if (!parsed.xmpFound && !parsed.isoFound) return { kind: "sdr" };
  if (parsed.xmpError) return unverified(parsed.xmpError);
  if (parsed.xmpDeclarations.length > 1) return unverified("conflicting-metadata");
  if (parsed.xmpDeclarations.length === 0 && parsed.isoFound) {
    return unverified("iso-only-not-validated");
  }

  const declaration = parsed.xmpDeclarations[0];
  if (!declaration) return unverified("malformed-xmp");
  if (!parsed.mpfFound) return unverified("missing-mpf");
  if (!Number.isSafeInteger(declaration.gainMapLength) || declaration.gainMapLength <= 0) {
    return unverified("invalid-gain-map-length");
  }
  if (declaration.gainMapLength >= head.totalSize) {
    return unverified("invalid-gain-map-length");
  }

  const validation = await validateTrailingJpeg(
    url,
    head,
    declaration.gainMapLength,
    metadataLimit,
    fetchImpl,
    options.signal,
  );
  if (typeof validation === "string") return unverified(validation);
  const hdrStops = await readGainMapHdrStops(
    url,
    head,
    validation.start,
    declaration.gainMapLength,
    metadataLimit,
    fetchImpl,
    domParserFactory,
    options.signal,
  );
  return {
    kind: "jpeg-gain-map",
    encoding: parsed.isoFound ? "dual" : "ultra-hdr-v1",
    validation: "trailing-jpeg-validated",
    ...(hdrStops === undefined ? {} : { hdrStops }),
  };
}

export async function annotateResolvedPhotoSource(
  source: ResolvedPhotoSourceForAnnotation,
  runtimeCapability: JpegGainMapRuntimeCapability,
  options: JpegHdrInspectionOptions = {},
): Promise<ResolvedPhotoSource> {
  let displayDynamicRange: DisplayDynamicRange;
  if (source.displayFormat === "png") {
    displayDynamicRange = { kind: "sdr" };
  } else if (source.displayFormat === "heic") {
    displayDynamicRange = unverified("not-jpeg");
  } else {
    try {
      displayDynamicRange = await inspectJpegGainMapUrl(source.displayUrl, options);
    } catch (error) {
      if (isAbortError(error)) throw error;
      displayDynamicRange = unverified("range-unsupported");
    }
  }
  return Object.freeze({
    ...source,
    displayDynamicRange,
    jpegGainMapRuntimeCapability: runtimeCapability,
  });
}

interface HeadRead {
  readonly bytes: Uint8Array;
  readonly totalSize: number;
  readonly complete: boolean;
}

interface ParsedJpegMetadata {
  readonly kind: "metadata";
  readonly reachedImageData: boolean;
  readonly xmpFound: boolean;
  readonly xmpError?: JpegHdrInspectionReason;
  readonly xmpDeclarations: readonly XmpDeclaration[];
  readonly mpfFound: boolean;
  readonly isoFound: boolean;
  readonly hdrStops?: number;
}

interface XmpDeclaration {
  readonly gainMapLength: number;
}

async function readHead(
  url: string,
  metadataLimit: number,
  fetchImpl: typeof globalThis.fetch,
  signal?: AbortSignal,
): Promise<HeadRead> {
  const response = await fetchImpl(url, {
    headers: { Range: `bytes=0-${metadataLimit - 1}` },
    signal,
  });
  if (response.status === 206) {
    const contentRange = parseContentRange(response.headers.get("Content-Range"));
    if (!contentRange || contentRange.start !== 0 || contentRange.end >= contentRange.total) {
      throw new InspectionFailure("invalid-content-range");
    }
    const expectedLength = contentRange.end - contentRange.start + 1;
    const bytes = await readResponseBytes(response, expectedLength, "range-unsupported");
    if (bytes.length !== expectedLength) throw new InspectionFailure("invalid-content-range");
    return {
      bytes,
      totalSize: contentRange.total,
      complete: contentRange.end === contentRange.total - 1,
    };
  }
  if (response.status !== 200) throw new InspectionFailure("range-unsupported");
  const contentLength = parseSafeInteger(response.headers.get("Content-Length"));
  if (contentLength === undefined || contentLength > metadataLimit) {
    throw new InspectionFailure(
      contentLength === undefined ? "range-unsupported" : "metadata-limit",
    );
  }
  const bytes = await readResponseBytes(response, metadataLimit);
  if (bytes.length !== contentLength) throw new InspectionFailure("range-unsupported");
  return { bytes, totalSize: contentLength, complete: true };
}

async function readRange(
  url: string,
  start: number,
  end: number,
  totalSize: number,
  fetchImpl: typeof globalThis.fetch,
  signal?: AbortSignal,
): Promise<Uint8Array | JpegHdrInspectionReason> {
  let response: Response;
  try {
    response = await fetchImpl(url, {
      headers: { Range: `bytes=${start}-${end}` },
      signal,
    });
  } catch (error) {
    if (isAbortError(error)) throw error;
    return "range-unsupported";
  }
  if (response.status !== 206) return "range-unsupported";
  const contentRange = parseContentRange(response.headers.get("Content-Range"));
  if (
    !contentRange ||
    contentRange.start !== start ||
    contentRange.end !== end ||
    contentRange.total !== totalSize
  ) {
    return "invalid-content-range";
  }
  try {
    const bytes = await readResponseBytes(response, end - start + 1);
    return bytes.length === end - start + 1 ? bytes : "invalid-content-range";
  } catch (error) {
    if (isAbortError(error)) throw error;
    return "range-unsupported";
  }
}

async function validateTrailingJpeg(
  url: string,
  head: HeadRead,
  gainMapLength: number,
  metadataLimit: number,
  fetchImpl: typeof globalThis.fetch,
  signal?: AbortSignal,
): Promise<{ readonly start: number } | JpegHdrInspectionReason> {
  const expectedStart = head.totalSize - gainMapLength;
  const exact = await validateJpegAt(url, head, expectedStart, gainMapLength, fetchImpl, signal);
  if (exact === true) return { start: expectedStart };
  if (exact !== "invalid-trailing-jpeg") return exact;

  const searchStart = Math.max(0, expectedStart - metadataLimit);
  const searchEnd = Math.min(head.totalSize - 1, expectedStart + 2);
  const search = await readSmallRange(url, head, searchStart, searchEnd, fetchImpl, signal);
  if (typeof search === "string") return search;
  for (let offset = search.length - 3; offset >= 0; offset -= 1) {
    if (search[offset] !== 0xff || search[offset + 1] !== 0xd8 || search[offset + 2] !== 0xff) {
      continue;
    }
    const start = searchStart + offset;
    const candidate = await validateJpegAt(url, head, start, gainMapLength, fetchImpl, signal);
    if (candidate === true) return { start };
    if (candidate !== "invalid-trailing-jpeg") return candidate;
  }
  return "invalid-trailing-jpeg";
}

async function validateJpegAt(
  url: string,
  head: HeadRead,
  start: number,
  length: number,
  fetchImpl: typeof globalThis.fetch,
  signal?: AbortSignal,
): Promise<true | JpegHdrInspectionReason> {
  const end = start + length - 1;
  const first = await readSmallRange(url, head, start, start + 2, fetchImpl, signal);
  if (typeof first === "string") return first;
  if (first[0] !== 0xff || first[1] !== 0xd8 || first[2] !== 0xff) {
    return "invalid-trailing-jpeg";
  }
  const last = await readSmallRange(url, head, end - 1, end, fetchImpl, signal);
  if (typeof last === "string") return last;
  return last[0] === 0xff && last[1] === 0xd9 ? true : "invalid-trailing-jpeg";
}

async function readGainMapHdrStops(
  url: string,
  head: HeadRead,
  start: number,
  length: number,
  metadataLimit: number,
  fetchImpl: typeof globalThis.fetch,
  domParserFactory: () => DOMParser | undefined,
  signal?: AbortSignal,
): Promise<number | undefined> {
  const end = start + Math.min(length, metadataLimit) - 1;
  const bytes = await readSmallRange(url, head, start, end, fetchImpl, signal);
  if (typeof bytes === "string") return undefined;
  const parsed = parseJpegMetadata(bytes, length, bytes.length === length, domParserFactory);
  return parsed.kind === "metadata" ? parsed.hdrStops : undefined;
}

async function readSmallRange(
  url: string,
  head: HeadRead,
  start: number,
  end: number,
  fetchImpl: typeof globalThis.fetch,
  signal?: AbortSignal,
): Promise<Uint8Array | JpegHdrInspectionReason> {
  if (start < 0 || end < start || end >= head.totalSize) return "invalid-gain-map-length";
  if (head.complete) return head.bytes.slice(start, end + 1);
  return readRange(url, start, end, head.totalSize, fetchImpl, signal);
}

function parseJpegMetadata(
  bytes: Uint8Array,
  totalSize: number,
  complete: boolean,
  domParserFactory: () => DOMParser | undefined,
): ParsedJpegMetadata | { readonly kind: "error"; readonly reason: JpegHdrInspectionReason } {
  const app1: Uint8Array[] = [];
  const app2: Uint8Array[] = [];
  let cursor = 2;
  let reachedImageData = false;
  while (cursor < bytes.length) {
    if (bytes[cursor] !== 0xff) return { kind: "error", reason: "malformed-jpeg" };
    while (cursor < bytes.length && bytes[cursor] === 0xff) cursor += 1;
    if (cursor >= bytes.length) return boundaryError(bytes, totalSize, complete);
    const marker = bytes[cursor++]!;
    if (marker === 0x00) return { kind: "error", reason: "malformed-jpeg" };
    if (marker === 0xda) {
      reachedImageData = true;
      break;
    }
    if (marker === 0xd9) {
      return collectMetadata(app1, app2, true, domParserFactory);
    }
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7) || marker === 0xd8) continue;
    if (cursor + 2 > bytes.length) return boundaryError(bytes, totalSize, complete);
    const length = (bytes[cursor]! << 8) | bytes[cursor + 1]!;
    if (length < 2) return { kind: "error", reason: "malformed-jpeg" };
    const payloadStart = cursor + 2;
    const segmentEnd = cursor + length;
    if (segmentEnd > bytes.length) return boundaryError(bytes, totalSize, complete);
    if (marker === 0xe1) app1.push(bytes.slice(payloadStart, segmentEnd));
    if (marker === 0xe2) app2.push(bytes.slice(payloadStart, segmentEnd));
    cursor = segmentEnd;
  }
  if (!reachedImageData && cursor >= bytes.length && !complete) {
    return { kind: "error", reason: "metadata-limit" };
  }
  if (cursor > bytes.length) return { kind: "error", reason: "malformed-jpeg" };
  return collectMetadata(app1, app2, reachedImageData, domParserFactory);
}

function collectMetadata(
  app1: readonly Uint8Array[],
  app2: readonly Uint8Array[],
  reachedImageData: boolean,
  domParserFactory: () => DOMParser | undefined,
): ParsedJpegMetadata {
  const xmpDeclarations: XmpDeclaration[] = [];
  let xmpFound = false;
  let xmpError: JpegHdrInspectionReason | undefined;
  for (const payload of app1) {
    const xmp = parseXmp(payload, domParserFactory);
    if (!xmp) continue;
    xmpFound = true;
    if ("kind" in xmp) xmpError ??= xmp.reason;
    else xmpDeclarations.push(xmp);
  }
  return {
    kind: "metadata",
    reachedImageData,
    xmpFound,
    xmpError,
    xmpDeclarations,
    mpfFound: app2.some((payload) => startsWithAscii(payload, "MPF\0")),
    isoFound: app2.some((payload) => containsNulTerminatedAscii(payload, ISO_21496_NAMESPACE)),
    hdrStops: app1.reduce<number | undefined>(
      (stops, payload) => stops ?? parseHdrStops(payload, domParserFactory),
      undefined,
    ),
  };
}

function parseXmp(
  payload: Uint8Array,
  domParserFactory: () => DOMParser | undefined,
):
  | XmpDeclaration
  | { readonly kind: "error"; readonly reason: JpegHdrInspectionReason }
  | undefined {
  const text = decodeUtf8(payload);
  if (text === undefined || !text.includes(HDR_GAIN_MAP_NAMESPACE)) return undefined;
  if (/<!DOCTYPE|<!ENTITY/i.test(text)) return { kind: "error", reason: "malformed-xmp" };
  const xml = text.startsWith(XMP_PACKET_SIGNATURE)
    ? text.slice(XMP_PACKET_SIGNATURE.length)
    : text;
  const parser = domParserFactory();
  if (!parser) return { kind: "error", reason: "malformed-xmp" };
  const document = parser.parseFromString(xml.replace(/\0+$/u, "").trim(), "application/xml");
  if (document.getElementsByTagName("parsererror").length > 0) {
    return { kind: "error", reason: "malformed-xmp" };
  }
  const allElements = document.getElementsByTagName("*");
  const version = [...allElements].find((element) =>
    element.getAttributeNS(HDR_GAIN_MAP_NAMESPACE, "Version"),
  );
  if (!version || version.getAttributeNS(HDR_GAIN_MAP_NAMESPACE, "Version") !== "1.0") {
    return { kind: "error", reason: "malformed-xmp" };
  }
  const items = [...document.getElementsByTagNameNS(CONTAINER_NAMESPACE, "Item")];
  const gainItems = items.filter(
    (item) =>
      item.getAttributeNS(ITEM_NAMESPACE, "Semantic") === "GainMap" &&
      item.getAttributeNS(ITEM_NAMESPACE, "Mime") === "image/jpeg",
  );
  if (gainItems.length === 0) return { kind: "error", reason: "malformed-xmp" };
  if (gainItems.length > 1) return { kind: "error", reason: "conflicting-metadata" };
  const lengthText = gainItems[0]!.getAttributeNS(ITEM_NAMESPACE, "Length") ?? "";
  const gainMapLength = parseSafeDecimal(lengthText);
  if (gainMapLength === undefined || gainMapLength <= 0) {
    return { kind: "error", reason: "invalid-gain-map-length" };
  }
  return { gainMapLength };
}

function parseHdrStops(
  payload: Uint8Array,
  domParserFactory: () => DOMParser | undefined,
): number | undefined {
  const text = decodeUtf8(payload);
  if (
    text === undefined ||
    !text.includes(HDR_GAIN_MAP_NAMESPACE) ||
    /<!DOCTYPE|<!ENTITY/i.test(text)
  ) {
    return undefined;
  }
  const xml = text.startsWith(XMP_PACKET_SIGNATURE)
    ? text.slice(XMP_PACKET_SIGNATURE.length)
    : text;
  const parser = domParserFactory();
  if (!parser) return undefined;
  const document = parser.parseFromString(xml.replace(/\0+$/u, "").trim(), "application/xml");
  if (document.getElementsByTagName("parsererror").length > 0) return undefined;
  const elements = [...document.getElementsByTagName("*")];
  for (const attribute of ["HDRCapacityMax", "GainMapMax"]) {
    const value = elements
      .map((element) => element.getAttributeNS(HDR_GAIN_MAP_NAMESPACE, attribute))
      .find((candidate) => candidate !== null);
    if (value === undefined) continue;
    const stops = Number(value);
    if (Number.isFinite(stops) && stops >= 0 && stops <= 32) return stops;
  }
  return undefined;
}

function boundaryError(
  bytes: Uint8Array,
  totalSize: number,
  complete: boolean,
): { readonly kind: "error"; readonly reason: JpegHdrInspectionReason } {
  return {
    kind: "error",
    reason: complete || bytes.length >= totalSize ? "malformed-jpeg" : "metadata-limit",
  };
}

async function readResponseBytes(
  response: Response,
  limit: number,
  overflowReason: JpegHdrInspectionReason = "metadata-limit",
): Promise<Uint8Array> {
  if (response.body) {
    const reader = response.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    try {
      while (true) {
        const next = await reader.read();
        if (next.done) break;
        const chunk = next.value;
        size += chunk.byteLength;
        if (size > limit) {
          await reader.cancel();
          throw new InspectionFailure(overflowReason);
        }
        chunks.push(chunk);
      }
    } finally {
      reader.releaseLock();
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.byteLength;
    }
    return bytes;
  }
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length > limit) throw new InspectionFailure(overflowReason);
  return bytes;
}

function parseContentRange(
  value: string | null,
): { start: number; end: number; total: number } | undefined {
  if (!value) return undefined;
  const match = /^bytes ([0-9]+)-([0-9]+)\/([0-9]+)$/u.exec(value.trim());
  if (!match) return undefined;
  const start = parseSafeDecimal(match[1]!);
  const end = parseSafeDecimal(match[2]!);
  const total = parseSafeDecimal(match[3]!);
  if (
    start === undefined ||
    end === undefined ||
    total === undefined ||
    end < start ||
    total <= end
  ) {
    return undefined;
  }
  return { start, end, total };
}

function parseSafeInteger(value: string | null): number | undefined {
  if (!value || !/^[0-9]+$/u.test(value.trim())) return undefined;
  return parseSafeDecimal(value.trim());
}

function parseSafeDecimal(value: string): number | undefined {
  if (!/^[0-9]+$/u.test(value)) return undefined;
  const number = Number(value);
  return Number.isSafeInteger(number) ? number : undefined;
}

function decodeUtf8(bytes: Uint8Array): string | undefined {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return undefined;
  }
}

function startsWithAscii(bytes: Uint8Array, value: string): boolean {
  return matchesAsciiAt(bytes, 0, value);
}

function containsNulTerminatedAscii(bytes: Uint8Array, value: string): boolean {
  for (let start = 0; start + value.length < bytes.length; start += 1) {
    if (matchesAsciiAt(bytes, start, value) && bytes[start + value.length] === 0) return true;
  }
  return false;
}

function matchesAsciiAt(bytes: Uint8Array, start: number, value: string): boolean {
  if (start + value.length > bytes.length) return false;
  for (let index = 0; index < value.length; index += 1) {
    if (bytes[start + index] !== value.charCodeAt(index)) return false;
  }
  return true;
}

function unverified(reason: JpegHdrInspectionReason): DisplayDynamicRange {
  return { kind: "unverified", reason };
}

class InspectionFailure extends Error {
  constructor(readonly reason: JpegHdrInspectionReason) {
    super(reason);
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}
