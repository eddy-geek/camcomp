# Drop 3 HDR Implementation Notes

- **Status:** Automated Drop 3 delivery complete. JPEG gain-map inspection and
  HEIC CICP colour-parameter probing are both implemented. The frontend
  unit/component suite, three-engine E2E matrix, production build, contract
  check, Tauri compile-check, and Rust formatting pass. Physical HDR output,
  packaged Tauri output, and named hardware performance gates remain pending.
- **Date:** 2026-09-03 (HEIC CICP addition 2026-09-04)
- **Sources:** `PLAN-DROP3-HDR.md` and `ADR-DROP3-HDR.md`
- **Purpose:** Record the implemented JPEG gain-map path, HEIC HDR detection,
  automated evidence, and limitations without claiming unverified display output.

## 1. Delivered behavior

### 1.1 Bounded JPEG inspection

`apps/web/src/sources/jpeg-hdr.ts` provides one injected-fetch inspector for browser and Tauri display URLs. It:

- reads at most the 512 KiB metadata ceiling;
- validates `206` `Content-Range` responses and bounded `200` bodies;
- parses JPEG markers only before SOS;
- recognizes Ultra HDR v1 XMP, MPF, and ISO 21496-1 namespace signaling;
- validates the declared gain-map JPEG with bounded range reads, including files with trailing padding;
- reads the embedded gain-map XMP head for a rough `HDRCapacityMax`/`GainMapMax` stop count;
- returns typed SDR, gain-map, or unverified states without decoding pixels;
- rejects malformed XML/markers, unsafe lengths, duplicate metadata, unsupported ranges, and truncated gain maps without making the photo unavailable;
- propagates cancellation while preserving source release behavior.

The current 18-file JPEG corpus classifies as 16 validated gain-map JPEGs and 2 SDR controls. The 16 gain-map files are all Pixel (`PXL_*`) and OPPO (`IMG*`) JPEGs; the 2 SDR controls are OPPO Peakfinder screenshots (`g3-20260822_*`) that carry no gain-map XMP. Three OPPO files place bounded padding after the declared secondary JPEG and now validate from the declared length plus a bounded backward search instead of being rejected as `invalid-trailing-jpeg`. All 16 gain maps expose a rough stop count from their embedded XMP. ISO-only metadata remains explicitly unverified; synthetic dual metadata covers the parser branch, not codec interoperability.

### 1.2 Shared source and runtime contracts

Resolved photo sources now carry transient `displayFormat`, `displayDynamicRange`, and `jpegGainMapRuntimeCapability` fields. Browser and Tauri adapters annotate only magic-byte-classified JPEG display sources. Tauri remains unverified by policy; no native HDR parser, IPC image transfer, schema field, or dependency was added. Known PNG/decoded derivatives are classified as SDR before transport inspection, and genuine HEIC bypasses the JPEG inspector instead of replacing its definitive format classification with a transport error. Tauri also gives source magic bytes precedence over extensions, so JPEG data stored under an OPPO `.heic` filename remains an original JPEG and can retain and expose any gain map it contains.

### 1.3 HEIC CICP colour-parameter probing

`apps/web/src/sources/worker-heic.ts` now exports `probeHeifCicp()`, a bounded
byte-scan over the first 4 MiB of a HEIF container that locates `colr` boxes
with an `nclx` profile type and returns the CICP primaries, transfer
characteristics, matrix coefficients, and full-range flag.

The HEIC source resolver (`heic.ts`) probes CICP before display resolution and
maps the transfer characteristic to `DisplayDynamicRange`:

- transfer 16 (SMPTE ST 2084 / PQ) → `{ kind: "heic-pq" }`
- transfer 18 (ARIB STD-B67 / HLG) → `{ kind: "heic-hlg" }`
- anything else, or no `colr`/`nclx` box → `{ kind: "sdr" }`

`evaluateHdrPresentationStatus()` returns `"heic-hdr-source"` for PQ/HLG
sources. The viewer shows an `HDR` badge, a `PQ` or `HLG` metadata tag, and
the status text `HEIC HDR source · display not tone-mapped`. The display is not
tone-mapped because neither browser native decode nor the `heif-oxide` worker
applies a PQ/HLG OETF inverse; pixels are clamped to SDR. Correct HDR display
of HEIC PQ/HLG content remains a manual gate.

All current OPPO HEIC files in the corpus use ICC profiles (`prof`) rather than
CICP (`nclx`), so they classify as SDR. This is correct: the OPPO's true HEIC
output is 8-bit HEVC with a Display P3 ICC profile — wide-gamut SDR, not HDR.
Wide-gamut P3 with sRGB or BT.709 transfer is intentionally classified as SDR
because it does not require HDR display capability.

`annotateResolvedPhotoSource()` now preserves the HEIC resolver's classification
instead of overwriting it with a generic `unverified("not-jpeg")`. The Tauri
adapter sets `unverified("not-jpeg")` for native HEIC sources (CICP probing is
not yet available on the Tauri path) and `unverified("range-unsupported")` for
JPEG sources pending inspection.

### 1.4 Runtime recognition

Runtime recognition is isolated in `viewer/hdr.ts`:

- Firefox/Gecko is an explicit SDR-only fallback;
- current Chromium-family and standalone Safari user agents follow the accepted native-capable policy snapshot;
- Tauri, unknown, empty, and unrecognized user agents remain unverified;
- `(dynamic-range: high)` is considered only after source and runtime checks.

### 1.5 Viewer and UI

OpenSeadragon is explicitly configured with the HTML drawer and retains the original image URL. The group toolbar owns an album-lifetime `Automatic / HDR when available` versus `Compare in SDR` mode and applies the inherited `dynamic-range-limit` policy. Each gain-map photo exposes a text status for active HDR path, SDR display, Firefox fallback, forced SDR, unverified runtime, or unverified metadata. Its bottom-metadata popup also shows `GM` and, when embedded metadata supplies it, a one-decimal approximate HDR stop count. The number is metadata-derived headroom, not a pixel measurement or physical-display claim. Media-query listeners are scoped to each photo frame and removed during teardown.

Slider mode forwards both photo statuses into a visible, filename-keyed summary outside the `aria-hidden` trailing layer. Ordinary source-kind badges and current HEIC resolution behavior remain intact.

### 1.6 Rust build hygiene

All 14 `unused` warnings in `camcomp-desktop` were eliminated by gating the
non-macOS HEIC decode-to-cache machinery behind `#[cfg(not(target_os = "macos"))]`.
On macOS, WKWebView handles HEIC natively, so the decode gate, sidecar finder,
PNG encoder, and their supporting constants and imports were dead code. Each item
now carries the platform gate. `cargo build` and `cargo fmt --check` pass
without warnings.

## 2. Automated evidence

The following commands passed after implementation:

```text
npm --prefix apps/web run format:check
npm --prefix apps/web run lint
npm --prefix apps/web run typecheck
npm --prefix apps/web test                 # 51 passed
npm --prefix apps/web run build
npm --prefix apps/web run test:e2e         # 22 passed, 5 skipped
npm --prefix apps/web run contract:check
cargo check -p camcomp-desktop --all-features
cargo fmt --all -- --check
```

The three-engine E2E coverage verifies exact bounded `206` ranges from generated local Blob URLs, actual `<img>` descendants, no actual `<canvas>` descendants, a real gain-map JPEG opening in Chromium and WebKit, the HTML viewer path, and the explicit SDR mode. Firefox’s real gain-map viewer flow is skipped pending M2; its SDR-only policy is covered by unit tests. Headless media-query values are not treated as physical display evidence.

The OPPO calibration corpus, including the distinction between capture mode and
Pro XDR/HDR source markers, is documented in [`SPEC-OPPO.md`](SPEC-OPPO.md).
Unknown mode bits remain visible in the classification and metadata note, while
analysis and grouping continue; the Rust calibration invariant checks that
behavior.

## 3. Manual gates still open

- M1 Chrome/Edge/Safari direct-image parity on a named HDR display;
- M2 Firefox SDR-base fallback on a named desktop release;
- M3 packaged Tauri host/range/MIME/output verification;
- M4 two-to-four-photo memory, interaction, and cleanup measurements;
- M5 HEIC PQ/HLG display: verify that a real PQ or HLG HEIF renders
  meaningfully in Safari, Chrome, and the Tauri macOS native path, and that
  the `display not tone-mapped` warning is accurate;
- a real externally generated dual Ultra HDR v1 plus ISO 21496-1 fixture;
- a real PQ or HLG HEIF fixture to exercise the CICP detection path.

Until those gates are recorded, the application must use `HDR output unverified in this desktop runtime` where the runtime policy cannot establish native output, and documentation must not claim physical HDR luminance or packaged Tauri HDR support. On macOS Tauri, the original HEIC now reaches WKWebView's HTML image path without an SDR cache conversion; this is a preservation path, not yet verified HDR output. Non-macOS Tauri fallback remains current wide-gamut SDR display support, not an HDR implementation.
