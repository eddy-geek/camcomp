import type { DisplayDynamicRange } from "../sources/jpeg-hdr";

export const JPEG_HDR_POLICY_DATE = "2026-08-27";

export type JpegGainMapRuntimeCapability = "native-supported" | "sdr-only" | "unverified";

export type DynamicRangeMode = "automatic" | "sdr";

export type HdrPresentationStatus =
  | "hdr-path-active"
  | "sdr-source"
  | "sdr-forced"
  | "sdr-display"
  | "sdr-codec"
  | "heic-hdr-source"
  | "unverified-runtime"
  | "unverified-source";

export interface RuntimeEvidence {
  readonly runtime?: "browser" | "tauri";
  readonly userAgent?: string;
}

export function detectJpegGainMapRuntimeCapability(
  evidence: RuntimeEvidence = defaultRuntimeEvidence(),
): JpegGainMapRuntimeCapability {
  if (evidence.runtime === "tauri") return "unverified";
  const userAgent = evidence.userAgent ?? "";
  if (!userAgent) return "unverified";
  if (/Firefox\//.test(userAgent)) return "sdr-only";
  if (/Edg\//.test(userAgent)) return "native-supported";
  if (/OPR\//.test(userAgent)) return "native-supported";
  if (/(?:Chrome|Chromium)\//.test(userAgent)) return "native-supported";
  if (/Version\//.test(userAgent) && /Safari\//.test(userAgent)) return "native-supported";
  return "unverified";
}

export function evaluateHdrPresentationStatus(
  displayDynamicRange: DisplayDynamicRange,
  runtimeCapability: JpegGainMapRuntimeCapability,
  mode: DynamicRangeMode = "automatic",
  displayHasHighRange = false,
  forcedSdrSupported = true,
): HdrPresentationStatus {
  if (displayDynamicRange.kind === "sdr") return "sdr-source";
  if (displayDynamicRange.kind === "heic-pq" || displayDynamicRange.kind === "heic-hlg") {
    return "heic-hdr-source";
  }
  if (displayDynamicRange.kind === "unverified") {
    return displayDynamicRange.reason === "not-jpeg" ? "sdr-source" : "unverified-source";
  }
  if (mode === "sdr") return forcedSdrSupported ? "sdr-forced" : "unverified-runtime";
  if (runtimeCapability === "sdr-only") return "sdr-codec";
  if (runtimeCapability === "unverified") return "unverified-runtime";
  return displayHasHighRange ? "hdr-path-active" : "sdr-display";
}

export function presentationStatusText(
  status: HdrPresentationStatus,
  runtimeCapability?: JpegGainMapRuntimeCapability,
): string | undefined {
  switch (status) {
    case "hdr-path-active":
      return "HDR path active";
    case "sdr-display":
      return "HDR source · SDR display";
    case "sdr-codec":
      return runtimeCapability === "sdr-only"
        ? "SDR fallback · Firefox does not apply this JPEG gain map"
        : "SDR fallback · this runtime does not apply this JPEG gain map";
    case "sdr-forced":
      return "HDR source · comparing in SDR";
    case "heic-hdr-source":
      return "HEIC HDR source · display not tone-mapped";
    case "unverified-runtime":
      return "HDR output unverified in this desktop runtime";
    case "unverified-source":
      return "HDR metadata unverified";
    case "sdr-source":
      return undefined;
  }
}

export function dynamicRangeLimitValue(mode: DynamicRangeMode): "no-limit" | "standard" {
  return mode === "sdr" ? "standard" : "no-limit";
}

export function supportsDynamicRangeLimit(
  scope: Pick<typeof globalThis, "CSS"> = globalThis,
): boolean {
  return (
    typeof scope.CSS?.supports === "function" &&
    scope.CSS.supports("dynamic-range-limit", "standard")
  );
}

function defaultRuntimeEvidence(): RuntimeEvidence {
  if (typeof globalThis === "undefined") return {};
  const scope = globalThis as typeof globalThis & {
    readonly __TAURI_INTERNALS__?: unknown;
    readonly navigator?: Navigator;
  };
  if (scope.__TAURI_INTERNALS__) return { runtime: "tauri" };
  return { runtime: "browser", userAgent: scope.navigator?.userAgent ?? "" };
}
