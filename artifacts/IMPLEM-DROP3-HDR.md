# Drop 3 JPEG HDR Implementation Notes

- **Status:** Automated Drop 3 delivery complete. The frontend unit/component suite, three-engine E2E matrix, production build, contract check, Tauri compile-check, and Rust formatting pass. Physical HDR output, packaged Tauri output, and named hardware performance gates remain pending.
- **Date:** 2026-08-27
- **Sources:** `PLAN-DROP3-HDR.md` and `ADR-DROP3-HDR.md`
- **Purpose:** Record the implemented JPEG gain-map path, automated evidence, and limitations without claiming unverified display output.

## 1. Delivered behavior

### 1.1 Bounded JPEG inspection

`apps/web/src/sources/jpeg-hdr.ts` provides one injected-fetch inspector for browser and Tauri display URLs. It:

- reads at most the 512 KiB metadata ceiling;
- validates `206` `Content-Range` responses and bounded `200` bodies;
- parses JPEG markers only before SOS;
- recognizes Ultra HDR v1 XMP, MPF, and ISO 21496-1 namespace signaling;
- validates the declared trailing gain-map JPEG with two tiny range reads;
- returns typed SDR, gain-map, or unverified states without decoding pixels;
- rejects malformed XML/markers, unsafe lengths, duplicate metadata, unsupported ranges, and truncated gain maps without making the photo unavailable;
- propagates cancellation while preserving source release behavior.

The existing 12 baseline JPEG fixtures classify as 10 gain-map JPEGs and 2 SDR controls. ISO-only metadata remains explicitly unverified; synthetic dual metadata covers the parser branch, not codec interoperability.

### 1.2 Shared source and runtime contracts

Resolved photo sources now carry transient `displayDynamicRange` and `jpegGainMapRuntimeCapability` fields. Browser and Tauri adapters annotate the actual display URL after source resolution. Tauri remains unverified by policy; no native parser, IPC image transfer, schema field, or dependency was added. Known PNG/decoded derivatives remain SDR, and direct HEIC remains outside the HDR claim.

Runtime recognition is isolated in `viewer/hdr.ts`:

- Firefox/Gecko is an explicit SDR-only fallback;
- current Chromium-family and standalone Safari user agents follow the accepted native-capable policy snapshot;
- Tauri, unknown, empty, and unrecognized user agents remain unverified;
- `(dynamic-range: high)` is considered only after source and runtime checks.

### 1.3 Viewer and UI

OpenSeadragon is explicitly configured with the HTML drawer and retains the original image URL. The group toolbar owns an album-lifetime `Automatic / HDR when available` versus `Compare in SDR` mode and applies the inherited `dynamic-range-limit` policy. Each gain-map photo exposes a text status for active HDR path, SDR display, Firefox fallback, forced SDR, unverified runtime, or unverified metadata. Media-query listeners are scoped to each photo frame and removed during teardown.

Slider mode forwards both photo statuses into a visible, filename-keyed summary outside the `aria-hidden` trailing layer. Ordinary source-kind badges and current HEIC resolution behavior remain intact.

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
- a real externally generated dual Ultra HDR v1 plus ISO 21496-1 fixture.

Until those gates are recorded, the application must use `HDR output unverified in this desktop runtime` where the runtime policy cannot establish native output, and documentation must not claim physical HDR luminance or packaged Tauri HDR support. HEIC remains current wide-gamut SDR display support, not an HDR implementation.
