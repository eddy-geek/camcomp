# Drop 3 JPEG HDR Implementation Plan

- **Status:** Ready for implementation
- **Date:** 2026-08-27
- **Scope:** Preserve and display JPEG gain-map HDR in the shared browser/Tauri viewer
- **Decision source:** Accepted `ADR-DROP3-HDR.md`
- **Frontend:** Existing Svelte 5, TypeScript, Vite, and OpenSeadragon 6.1.0 stack
- **Browser targets:** Current desktop Chromium family, Safari, and Firefox
- **Native target:** Existing shared frontend in Tauri; HDR output remains unverified until a packaged-runtime gate passes
- **HEIC decision:** Current corpus is confirmed wide-gamut SDR; HEIC HDR detection, rendering, and conversion are deferred
- **Dependency decision:** Add no runtime dependency and make no `camcomp.json` schema change

## 1. Objective

Deliver an honest, testable JPEG HDR path that:

- recognizes the Ultra HDR v1 gain-map JPEGs already present in the repository;
- recognizes dual Ultra HDR v1 + ISO 21496-1 signaling when a fixture is added;
- preserves the original encoded JPEG without decoding or transcoding it;
- renders simple images through OpenSeadragon's HTML drawer so the final image primitive is an `HTMLImageElement`, not an SDR WebGL/Canvas surface;
- allows compatible current Chromium/Safari runtimes and HDR displays to apply the gain map natively;
- explicitly reports Firefox's SDR-base fallback;
- distinguishes an SDR display from an unsupported codec and an unverified runtime;
- keeps output states visible per photo during grid and slider comparison;
- provides an explicit group-level **Compare in SDR** mode;
- does not break ordinary JPEG, PNG, or current SDR HEIC viewing;
- remains local, bounded, abortable, and free of image uploads.

The implementation must never claim that successful JPEG decoding proves HDR. A gain-map declaration, an HDR-capable native image path, a supported runtime policy, and current display headroom are separate facts.

## 2. Confirmed baseline and completed risk reduction

### 2.1 Existing JPEG corpus

ExifTool 13.55 and a bounded binary probe inspected all 12 committed JPEG fixtures:

- 10 Pixel JPEGs contain the Ultra HDR v1 namespace,
  `hdrgm:Version="1.0"`, a `GainMap` container item, MPF metadata, and a complete
  trailing JPEG at the declared gain-map length;
- the two `test/tstgrp2/g3-*` JPEGs contain none of those gain-map declarations;
- every required declaration in the 10 affected files occurs within the first 128 KiB;
- a 256 KiB head plus tiny tail probes classified the current 10/2 split correctly;
- none of the current files contains the ISO namespace
  `urn:iso:std:iso:ts:21496:-1`.

Implementation uses a 512 KiB metadata ceiling to retain margin over the observed 128 KiB maximum. The ceiling is a classification limit, not an image-display limit: an oversized or unfamiliar file remains displayable with `unverified` HDR status.

### 2.2 Existing HEIC corpus

The repository contains exactly 18 HEIC/HEIF files, all under `test/tstgrp*`.
`heif-info` and ExifTool report the same properties for every one:

- 8-bit luma and chroma;
- one 8-bit YCbCr primary image;
- Display-P3 ICC profile;
- no PQ or HLG transfer signaling;
- no HDR/gain-map marker;
- no alpha, depth, or HDR auxiliary channel.

These files are confirmed wide-gamut SDR for this delivery. Their existing native,
sidecar, worker, and Tauri display behavior stays in scope for regression testing, but
HEIC HDR is not an implementation requirement. `heif-oxide`/`heic-to` HDR work,
HDR-HEIC fixtures, conversion choices, and ADR Gate H3 remain deferred.

### 2.3 OpenSeadragon HTML drawer spike

A no-code Playwright spike loaded the representative 4080×3072 Ultra HDR JPEG through
OpenSeadragon 6.1.0 with `drawer: "html"` in Chromium, WebKit, and Firefox. Every engine:

- selected the HTML drawer;
- inserted exactly one `<img>` carrying the original URL;
- inserted zero `<canvas>` elements;
- preserved an outer `clip-path: inset(... 35%)`;
- retained a valid OpenSeadragon world item through zoom;
- removed the image on viewer destruction.

WebKit also opened a current 8192×6144 HEIC using one `<img>` and zero canvases. A
headless screenshot forced visible composition without changing that DOM path.

This proves source/DOM/clip/lifecycle feasibility. It does not prove physical HDR
luminance, production GPU composition, or peak memory.

### 2.4 Shared bounded URL reads

A second Playwright spike created a 1 MiB JPEG Blob URL and requested its head and tail
with HTTP `Range` in Chromium, WebKit, and Firefox. All three returned:

- status `206`;
- the exact requested byte count and values;
- a correct `Content-Range` carrying total size.

Pinned Tauri 2.11.5's audited asset protocol likewise serves original complete/ranged
file bytes and returns `Content-Range`. One TypeScript URL inspector can therefore
classify browser Blob URLs and Tauri asset URLs. A native Rust classifier is not
planned unless packaged Tauri disproves this behavior.

The spike commands are session evidence, not permanent acceptance tests. Phase 1 must
first reproduce Blob range behavior in committed three-engine Playwright coverage, and
Phase 2 must reproduce the HTML-drawer/clip/lifecycle assertions before product logic
is allowed to rely on either result. Packaged Tauri range behavior remains Gate M3.

## 3. Scope boundaries

### 3.1 Included

- Ultra HDR v1/XMP gain-map detection for current JPEGs;
- recognition of ISO 21496-1 APP2 namespace signaling and dual-metadata classification;
- bounded trailing gain-map JPEG validation for Ultra HDR v1;
- transient display-source HDR metadata in frontend-owned contracts;
- one shared range-based inspector for browser and Tauri URLs;
- HTML OpenSeadragon drawer for all simple-image sources initially;
- explicit inherited `dynamic-range-limit` policy where supported;
- per-photo HDR/SDR/unverified status and accessible explanatory text;
- Firefox SDR fallback warning for detected gain-map JPEGs;
- SDR-display status using `(dynamic-range: high)` as one input only;
- current Chromium/Safari support policy isolated behind a tested capability function;
- Tauri status kept unverified until manually accepted per host/runtime;
- group-level automatic/HDR versus forced-SDR comparison mode;
- grid/slider/remount state preservation;
- unit, component, three-engine E2E, real-fixture, build, and manual HDR-display checks;
- user and implementation documentation after verification.

### 3.2 Explicitly deferred

- HEIC/HEIF HDR inspection, rendering, gain-map extraction, or conversion;
- changes to `heic-to`, `heif-oxide`, HEIC worker memory behavior, or HEIC cache format;
- CICP/PQ/HLG HDR PNG generation;
- AVIF or JPEG XL input/output support;
- custom Canvas, WebGL, WebGPU, shader, or native platform HDR renderers;
- pixel-domain tone mapping, difference, blending, or warping;
- exposing authored gain-map controls;
- claiming packaged Tauri HDR before a named host/runtime passes the manual gate;
- mobile/tablet acceptance;
- changing persisted analysis/schema contracts;
- modifying source photos or writing display metadata beside them.

### 3.3 Non-goals and terminology

- `HDR+` camera mode is not used as an HDR-file signal.
- Display P3, 10/16-bit storage, and HDR display output remain separate concepts.
- A browser's successful `img.decode()` is not gain-map support evidence.
- `(dynamic-range: high)` is not codec or renderer evidence.
- `HDR path active` means camcomp retained the gain-map JPEG and selected the accepted
  native image path; physical output remains subject to the tested runtime/display
  policy.

## 4. Fixed implementation decisions

1. **Use HTML for all current simple-image sources initially.** Set OpenSeadragon
   `drawer: "html"`; do not select the current WebGL/Canvas path for HDR-capable
   viewing.
2. **Inspect the display URL, not the analysis metadata.** This correctly handles exact
   JPEGs and JPEG sidecars without changing `camcomp.json`.
3. **Use bounded URL ranges.** Read at most 512 KiB of leading metadata plus tiny
   validation ranges near the declared gain map/end.
4. **Do not decode pixels during classification.** Classification is metadata/container
   work only.
5. **Keep classification non-fatal.** Malformed, over-limit, aborted, unsupported-range,
   or unfamiliar metadata yields `unverified`; the photo still opens.
6. **Centralize runtime policy.** User-agent/engine recognition is allowed only in the
   JPEG HDR capability module because no sufficient gain-map feature probe exists.
7. **Firefox is explicit SDR.** A detected gain-map JPEG in Firefox receives a
   persistent SDR fallback status; the ordinary JPEG base remains viewable.
8. **Tauri starts unverified.** Do not infer WKWebView/WebView2/WebKitGTK behavior from
   standalone browser results.
9. **Keep status transient.** Do not add HDR fields to Rust schema, generated TypeScript
   analysis contracts, or `camcomp.json`.
10. **Force SDR through CSS where accepted.** The group-level SDR mode applies inherited
    `dynamic-range-limit: standard`; automatic mode applies `no-limit`. Unsupported or
    unverified enforcement must be disclosed rather than silently claimed.
11. **No dependency is required.** Parse only bounded JPEG marker/XMP/APP2 data needed
    by this feature.
12. **Preserve failure isolation.** HDR inspection failure must not become a missing
    source or viewer-open failure.

## 5. Transient contracts

Use fixed ownership to avoid circular or catch-all modules:

- `apps/web/src/sources/jpeg-hdr.ts` owns JPEG marker/XMP types, stable inspection
  reasons, range I/O, and source annotation;
- `apps/web/src/viewer/hdr.ts` owns `DynamicRangeMode`, presentation statuses, runtime
  capability evaluation, and the pure status matrix;
- `apps/web/src/platform/types.ts` imports the minimum source-owned types needed by
  `ResolvedPhotoSource`; it does not implement HDR policy.

Names may be adjusted to existing naming style, but the states and separation must
remain:

```text
JpegGainMapEncoding
  ultra-hdr-v1 | iso-21496-1 | dual

DisplayDynamicRange
  sdr
  jpeg-gain-map { encoding, validation }
  unverified { reason }

GainMapValidation
  declared | trailing-jpeg-validated

JpegGainMapRuntimeCapability
  native-supported | sdr-only | unverified

DynamicRangeMode
  automatic | sdr

HdrPresentationStatus
  hdr-path-active
  sdr-source
  sdr-forced
  sdr-display
  sdr-codec
  unverified-runtime
  unverified-source

JpegHdrInspectionReason
  not-jpeg | range-unsupported | invalid-content-range | metadata-limit
  malformed-jpeg | malformed-xmp | conflicting-metadata
  invalid-gain-map-length | missing-mpf | invalid-trailing-jpeg
```

`ResolvedPhotoSource` gains immutable transient fields equivalent to:

```text
displayDynamicRange: DisplayDynamicRange
jpegGainMapRuntimeCapability: JpegGainMapRuntimeCapability
```

These fields describe the actual `displayUrl`, not necessarily the immutable original
photo. A gain-map JPEG selected as a HEIC sidecar is therefore classified as a gain-map
display source while `sourceKind` remains `sidecar`.

Ordinary JPEG/PNG display sources may report `sdr`. Direct/native HEIC remains
`unverified-source` for HDR purposes but does not show an HDR warning in this delivery;
HEIC is outside the HDR support claim. Generated HEIC PNG derivatives remain SDR and
keep their existing source-kind warning.

## 6. Bounded JPEG inspector design

### 6.1 Public boundary

Create `apps/web/src/sources/jpeg-hdr.ts` and export it from
`apps/web/src/sources/index.ts`. Keep I/O injection available for deterministic tests:

```text
inspectJpegGainMapUrl(url, { fetch, signal, metadataLimit?, domParserFactory? })
  -> Promise<DisplayDynamicRange>
```

The default metadata limit is `512 * 1024` bytes. No caller may increase it from file
metadata. Tests can lower it to exercise the over-limit path.

### 6.2 Request algorithm

1. Request `Range: bytes=0-524287` with the active `AbortSignal`.
2. Accept a `206` response only when `Content-Range` matches
   `bytes START-END/TOTAL`, `START` is zero, `END >= START`, `END < TOTAL`, the body
   length equals `END - START + 1`, and all parsed integers are safe.
3. Accept a `200` response only when `Content-Length` is present and no larger than the
   cap. Read at most `limit + 1` streamed bytes; if the header is absent/wrong or one
   extra byte arrives, cancel the reader and return `range-unsupported`. Never call an
   unbounded `arrayBuffer()` on a `200` response.
4. Obtain total file size from validated `Content-Range` or the bounded complete body.
5. Parse a `Uint8Array` from offset 0. Require SOI `FF D8`, then iterate marker by marker:
   - consume one or more `FF` fill bytes and reject a missing marker byte;
   - treat `01`, `D0..D7`, `D8`, and `D9` as standalone markers with no length;
   - for every other marker require two length bytes, decode unsigned big-endian length,
     require `length >= 2`, and compute `payloadEnd = lengthFieldOffset + length` with
     safe/bounded arithmetic;
   - if a segment crosses the fetched metadata boundary before SOS, return
     `metadata-limit`; if it crosses the real end of a complete small file, return
     `malformed-jpeg`;
   - stop before entropy-coded data at SOS `FF DA`; do not search after SOS.
6. Retain only bounded APP1 `FF E1` and APP2 `FF E2` payload views. Detect MPF only when
   an APP2 payload begins `MPF\0`; detect ISO only from the specified NUL-terminated
   namespace at the defined APP2 payload position.
7. Return `sdr` when a structurally valid JPEG reaches SOS/EOI with no supported
   gain-map declaration. Return `not-jpeg` for other formats so callers can map known
   PNG to SDR and HEIC to out-of-scope/uninspected.
8. Return `unverified` rather than throwing for malformed/conflicting/over-limit HDR
   metadata. Throw only cancellation; source resolution retains normal error behavior.

### 6.3 Ultra HDR v1 recognition

Within bounded APP1 XMP payloads require all of:

- namespace `http://ns.adobe.com/hdr-gain-map/1.0/`;
- `hdrgm:Version="1.0"` independent of whitespace/quote style;
- a bounded `<Container:Item ...>` whose attributes include
  `Item:Semantic="GainMap"` and `Item:Mime="image/jpeg"`;
- a positive decimal `Item:Length` that is smaller than total file size;
- an APP2 MPF declaration.

For an APP1 payload beginning with the standard
`http://ns.adobe.com/xap/1.0/\0` signature, decode only the bounded bytes after that
signature as UTF-8 and trim permitted terminal NUL padding. Reject `DOCTYPE`/`ENTITY`
declarations, then parse the complete remaining payload with the browser's inert
`DOMParser` in `application/xml` mode. Reject a `parsererror`. Query
elements/attributes by these namespace URIs rather than document prefix or attribute
order:

- HDR gain map: `http://ns.adobe.com/hdr-gain-map/1.0/`;
- container: `http://ns.google.com/photos/1.0/container/`;
- item: `http://ns.google.com/photos/1.0/container/item/`.

Require exactly one compatible gain-map item; reject duplicate/conflicting lengths or
MIME/semantic values. `DOMParser` must remain an injected parsing boundary for Vitest;
it does not execute scripts, and the precheck prevents entity/DTD expansion. Do not
concatenate unbounded extended XMP chunks. Current fixtures keep required gain-map
metadata in standard XMP; an extended-XMP-only declaration returns `unverified` until a
bounded fixture requires support.

Validate the declared gain map with tiny range reads:

- the first three bytes at `totalSize - gainMapLength` are JPEG `FF D8 FF`;
- the final two file bytes are `FF D9`.

When declarations are present but validation fails, return `unverified` with a stable
machine reason; do not call the source SDR.

### 6.4 ISO 21496-1 recognition

Recognize the NUL-terminated APP2 namespace
`urn:iso:std:iso:ts:21496:-1`. For this delivery:

- Ultra HDR v1 plus ISO signaling and a validated Ultra HDR trailing JPEG is `dual`;
- ISO-only signaling returns `unverified-source: iso-only-not-validated` in this
  delivery; do not claim ISO-only support without a real fixture and a separately
  specified bounded relation/metadata parser;
- contradictory duplicate records return `unverified`;
- no ISO metadata is rewritten.

Phase 0 builds a small synthetic dual packet around the same marker rules to verify
namespace recognition and dual classification. The manual HDR gate still requires a
real externally generated dual-metadata JPEG before documentation claims ISO-tested
output. Synthetic structure verifies parser branching, not codec interoperability.

### 6.5 Source annotation

Add a helper that takes a frozen `ResolvedPhotoSource`, runs the inspector against its
`displayUrl`, and returns a new frozen source preserving:

- ID/path/source-kind fields;
- warnings;
- the original idempotent `release()` function;
- inspection status;
- runtime capability supplied by the platform adapter.

Call this helper in both platform adapters after their existing source resolver returns:

- `BrowserPlatformAdapter.resolvePhoto` for Blob URLs;
- `TauriPlatformAdapter.resolvePhoto` after `convertFileSrc` for asset URLs.

Do not change `resolveHeicPhotoSource` just to inspect a JPEG sidecar: inspecting the
resolved display URL covers exact JPEGs, JPEG sidecars, PNG derivatives, and direct HEIC
through one boundary.

If annotation fails for a non-cancellation reason, return the usable source with
`unverified-source`; never leak or double-release its URL.

## 7. Runtime and output policy

### 7.1 Capability function

Implement one pure, injected function that maps runtime evidence to
`JpegGainMapRuntimeCapability`:

- recognized current Firefox/Gecko: `sdr-only`;
- accepted current Chromium-family or Safari standalone: `native-supported`;
- Tauri and unknown engines: `unverified` until a named manual result is recorded.

Keep user-agent parsing in this one module. Match in exclusion order: Firefox
(`Firefox/`), Edge (`Edg/`), Opera (`OPR/`), Chrome/Chromium (`Chrome/` or
`Chromium/`), then Safari only when both `Version/` and `Safari/` are present and no
Chromium token matched. Brave's normal Chromium UA follows the accepted Chromium
engine policy; `navigator.brave` is not required for correctness. Cover empty,
malformed, bot, WebKit-like unknown, and injected Tauri values. Do not use browser
names elsewhere in components or resolvers.

Browser-family recognition is a policy snapshot, not proof. Include the policy date in
a constant/test and make its review a release checklist item. Before M1 is recorded,
recognized Chromium/Safari builds may be shown as `HDR path unverified`; M1 promotes
the tested release policy to `native-supported`. Do not invent minimum versions from
UA strings without recorded evidence.

### 7.2 Display headroom

For a gain-map source, observe `matchMedia("(dynamic-range: high)")` when available.
Subscribe to `change` while the photo frame is mounted and remove the listener on
unmount. The media query may change when the window moves between displays or OS HDR
settings change.

Interpret it only after source, renderer, and runtime capability:

| Source                        | Runtime          | Dynamic range mode | `dynamic-range: high` | Visible result                                                               |
| ----------------------------- | ---------------- | ------------------ | --------------------- | ---------------------------------------------------------------------------- |
| ordinary SDR                  | any              | any                | any                   | `SDR source` or no prominent HDR badge                                       |
| gain-map                      | Firefox/SDR-only | automatic          | any                   | `SDR fallback`; Firefox does not apply the gain map                          |
| gain-map                      | supported        | automatic          | false/unavailable     | `HDR source · SDR display`                                                   |
| gain-map                      | supported        | automatic          | true                  | `HDR path active`                                                            |
| gain-map                      | unverified       | automatic          | any                   | `HDR output unverified in this runtime`                                      |
| gain-map                      | any              | forced SDR         | any                   | `HDR source · comparing in SDR`, or unverified if enforcement is unavailable |
| malformed/unknown declaration | any              | any                | any                   | `HDR metadata unverified`; image remains visible                             |

Do not display `HDR active` based only on `dynamic-range: high`.

### 7.3 Group-level SDR mode

Add `Dynamic range` to the existing GroupViewer toolbar:

- `Automatic / HDR when available` (default);
- `Compare in SDR`.

Apply the chosen value to the common `.viewer-container` so grid and slider inherit the
same policy:

```text
automatic -> dynamic-range-limit: no-limit
sdr       -> dynamic-range-limit: standard
```

Own the mode in `AlbumShell.svelte`, which survives the keyed `GroupViewer` remount
when the selected group changes. Pass the value/change callback into `GroupViewer` and
the current mode into photo/slider status presentation. The mode resets to `automatic`
only when the album shell is destroyed for a new import/reset. Do not add localStorage
or a new global preference system for this delivery.

If `CSS.supports("dynamic-range-limit", "standard")` is false while a runtime is
otherwise expected to render HDR, do not claim forced SDR. Show that SDR enforcement is
unverified. Firefox remains SDR through its base-image behavior.

### 7.4 HTML drawer

In `apps/web/src/viewer/openseadragon.ts`:

- explicitly configure `drawer: "html"`;
- retain `buildPyramid: false` and existing navigation settings;
- do not allow silent fallback to WebGL/Canvas for this accepted path;
- preserve open/open-failed event cleanup and destroy semantics.

Expose the selected drawer type or final primitive through a narrow test hook only if
DOM assertions cannot establish it without production API pollution. Prefer assertions
against the actual descendant `img` and absence of actual `canvas` elements.

Do not use the `.openseadragon-canvas` class as proof of Canvas rendering: OpenSeadragon
uses that class name for a general drawing container.

## 8. UI and accessibility behavior

### 8.1 Per-photo status

Extend `PhotoFrame.svelte` with a compact text badge near the existing source-kind
badge and a detailed status line in the warnings/status region.

Required wording or semantically equivalent text:

- `HDR path active`;
- `HDR source · SDR display`;
- `SDR fallback · Firefox does not apply this JPEG gain map`;
- `HDR output unverified in this desktop runtime`;
- `HDR source · comparing in SDR`;
- `HDR metadata unverified`.

Use `role="status"` for capability changes that do not block viewing. Use `role="alert"`
only for existing blocking decode/source errors; changing monitors must not repeatedly
interrupt assistive technology with alerts.

The state must be visible as text and not rely only on color. Preserve the current
`Sidecar` and `Decoded copy` badges. When a JPEG sidecar is gain-map HDR, show both
`Sidecar` and its dynamic-range status.

### 8.2 Mixed groups

Every detected gain-map photo reports its own status. The group toolbar provides the
single all-SDR action; it does not silently force ordinary SDR members into an HDR
label. If one photo is HDR-path-active and another is SDR, their badges make the mixed
state explicit.

The clipped trailing frame is `aria-hidden`, so slider status cannot rely on its child
content. `PhotoFrame` exposes a narrow status-change callback carrying `photo.id` and
the computed presentation status. `ComparisonSlider` forwards/collects both callbacks
in a map keyed by photo ID and renders one visible textual summary naming both files and
their states outside the hidden layer. Clear the map when the slider is destroyed or
its photo IDs change. Grid mode continues to use each visible frame's own status; it
does not render a duplicate aggregate banner.

### 8.3 Lifecycle

- HDR range requests use the same album abort signal where available.
- Superseded imports release the underlying source exactly once.
- In `PhotoFrame.onMount`, create/read one media-query list, register one `change`
  listener, and retain its remover. `onDestroy` removes it before destroying the viewer
  and releasing the source. Every grid/slider remount creates exactly one replacement
  listener even when the same photo ID remounts.
- Grid/slider remounts retain canonical navigation; album-owned state retains the
  selected dynamic-range mode across groups.
- An inspector failure cannot leave a loading overlay indefinitely.

## 9. Implementation phases

Each phase follows test-first order: add a failing focused test, implement the smallest
complete behavior, run focused checks, then run the phase verification commands.

### Phase 0 — Fixtures, parser contract, and transient types

#### Tasks

1. Add the transient unions/interfaces from §5 without touching generated analysis
   contracts.
2. Add `jpeg-hdr.ts` constants and pure marker/XMP parsing helpers.
3. Add synthetic byte builders in tests for:
   - plain JPEG;
   - current Ultra HDR v1 metadata shape;
   - dual Ultra HDR v1 + ISO APP2 namespace;
   - ISO-only declaration;
   - reordered XML attributes and quote/whitespace variants;
   - malformed marker lengths;
   - missing MPF;
   - zero/negative/overflowing/too-large gain-map lengths;
   - missing or truncated trailing JPEG;
   - conflicting duplicate metadata;
   - metadata beyond the cap.
4. Add a real-fixture test that classifies the committed 10/2 JPEG split using bounded
   reads.
5. Record that current fixtures are Ultra HDR v1 only; do not rewrite them merely to add
   ISO metadata.

#### Exit criteria

- every parser branch returns a typed state/reason;
- malformed metadata never causes an unbounded allocation or source failure;
- all 12 current JPEG fixtures match the documented 10/2 result;
- no HEIC file is read by the HDR classifier tests;
- no new dependency or schema output appears.

### Phase 1 — Shared range I/O and source annotation

#### Tasks

1. Implement bounded `Range` fetch with injected fetch and abort support.
2. Parse and validate `Content-Range`/total size and cap response bodies.
3. Implement head metadata parse and tiny trailing-JPEG reads.
4. Add immutable source annotation preserving idempotent release.
5. Wire annotation into browser and Tauri `resolvePhoto` after URL creation.
6. Isolate runtime capability detection and attach its result to resolved sources.
7. Preserve photo display when range fetch is unsupported, malformed, or denied.

#### Tests

- `206` head/tail success;
- committed Playwright contract: a generated 1 MiB Blob URL returns exact `206` head
  and tail ranges with total size in Chromium, WebKit, and Firefox;
- bounded `200` small-file success;
- oversized/full-body `200` cancellation and `unverified` result;
- missing/invalid `Content-Range`;
- network error versus `AbortError`;
- exact browser JPEG, PNG, HEIC, and JPEG sidecar annotation;
- Tauri asset URL annotation with mocked range responses;
- object URL revoked exactly once after success, failure, and stale-run cancellation;
- unknown source remains viewable.

#### Exit criteria

- browser and Tauri adapters expose the same transient status contract;
- no native Rust parser or IPC image transfer is added;
- source resolution behavior and HEIC consent remain unchanged;
- classification reads no more than the documented cap/tiny tail ranges.

### Phase 2 — HTML renderer migration

#### Tasks

1. Set OpenSeadragon `drawer: "html"` explicitly.
2. Update E2E assertions to target actual `<img>`/`<canvas>` elements.
3. Exercise fit, zoom, pan synchronization, aligned mode, grid/slider switching, and
   cleanup with the HTML drawer.
4. Retain current simple-source and object-URL lifecycle.
5. Keep a single code-level rollback point for the contingency in §12; do not ship a
   hidden per-browser drawer branch.

#### Tests

- synthetic two-photo viewer in Chromium, WebKit, and Firefox has one `img` and zero
  `canvas` descendants per viewer;
- original Blob URL remains the image source;
- real SDR JPEG comparison still opens;
- representative Ultra HDR JPEG opens in each engine;
- WebKit current 8192×6144 HEIC still opens through the HTML drawer;
- navigation and slider tests pass unchanged in behavior;
- leaving a group destroys image elements and releases URLs.

#### Exit criteria

- no final image canvas exists in automated target engines;
- all current viewer interactions pass;
- ordinary JPEG/PNG and current HEIC regressions pass;
- no physical HDR claim is made yet.

### Phase 3 — Status policy, warnings, and SDR mode

#### Tasks

1. Implement pure presentation-status evaluation from source/runtime/display/mode.
2. Add media-query subscription and cleanup.
3. Add per-photo accessible badge/detail text.
4. Add group-level dynamic-range selector and inherited CSS property.
5. Ensure slider mode exposes both photos' status despite its trailing visual layer being
   `aria-hidden`.
6. Preserve album-owned dynamic-range mode across grid/slider and group remounts.

#### Tests

- complete policy table from §7.2;
- Firefox detected gain-map warning;
- supported runtime with low/high display range;
- Tauri/unknown runtime unverified status;
- automatic versus forced-SDR text and CSS;
- unsupported forced-SDR enforcement is not claimed;
- media-query change updates status and listener is removed on destroy;
- sidecar + HDR badges coexist;
- grid/slider status remains perceivable and keyboard accessible;
- no warning is invented from `HDR+` mode, P3, extension, or bit depth.

#### Exit criteria

- every detected gain-map JPEG has one truthful visible status;
- Firefox never silently presents a detected gain-map JPEG as HDR;
- forced SDR affects the whole comparison group or reports enforcement as unverified;
- no current SDR HEIC is labelled HDR or shown an HDR fallback warning.

### Phase 4 — Automated integration and regression

#### Tasks

1. Add one browser flow using a real committed gain-map JPEG.
2. Run deterministic injected capability cases rather than expecting headless HDR
   hardware.
3. Update existing tests that assume `.openseadragon-canvas` means a real canvas.
4. Verify CSP, PWA privacy, object URLs, worker consent, and Tauri compile behavior.
5. Verify production bundling adds no remote request or runtime dependency.

#### Automated matrix

| Case                                  | Chromium |   WebKit |           Firefox |
| ------------------------------------- | -------: | -------: | ----------------: |
| HTML drawer `img`, no actual canvas   | required | required |          required |
| Current Ultra HDR file opens          | required | required | required SDR base |
| Injected supported/high policy        | required | required |    unit/component |
| Injected supported/SDR-display policy | required | required |    unit/component |
| Native Firefox policy                 |      n/a |      n/a |  required warning |
| Grid/slider/remount cleanup           | required | required |          required |
| Current WebKit HEIC regression        |      n/a | required |               n/a |

Headless WebKit may report `(dynamic-range: high)` despite having no physical HDR
screen. Tests must inject policy for semantic assertions and must not treat that media
query result as manual acceptance.

#### Exit criteria

- focused unit/component/E2E tests pass;
- production build and Tauri compile check pass;
- no external network request handles selected images;
- existing 42-test frontend baseline and all updated tests pass;
- implementation is ready for manual HDR gates.

### Phase 5 — Manual HDR acceptance and documentation

#### Required hardware evidence

Record browser version, OS version, GPU, internal/external display model, OS HDR state,
and whether the window is on the tested display.

Use at least:

- one committed Ultra HDR v1 Pixel JPEG;
- one real dual Ultra HDR v1 + ISO 21496-1 JPEG with documented provenance;
- its direct standalone `<img>` reference outside OpenSeadragon;
- one ordinary SDR JPEG control.

#### Gate M1 — Chromium/Safari direct parity

For current Chrome and Safari on an HDR-capable display:

- direct `<img>` visibly produces expected highlight headroom;
- camcomp automatic mode matches the direct reference in grid mode;
- camcomp slider mode retains headroom on both layers;
- pan/zoom/resize/remount/full-screen do not collapse output to SDR;
- `Compare in SDR` visibly constrains both sources;
- moving to an SDR display or disabling HDR updates status honestly.

Edge is required before documenting it as accepted. Brave and Opera are smoke-tested if
they are named in user-facing support claims.

#### Gate M2 — Firefox fallback

- the same gain-map JPEG opens as its SDR base;
- the per-photo Firefox fallback is visible and accessible;
- ordinary SDR JPEG controls do not receive a false HDR warning;
- forced-SDR group mode remains coherent.

#### Gate M3 — Tauri

Test macOS WKWebView separately from Safari. Test other available hosts without
transferring conclusions between WebView2/WebKitGTK/WKWebView.

Until a host passes:

- show `HDR output unverified in this desktop runtime`;
- do not document that Tauri host as HDR-capable.

Verify asset-protocol head/tail ranges and MIME in the packaged runtime as well as
visual output.

#### Gate M4 — performance and cleanup

On named hardware:

- compare two, three, and four representative images;
- include the current 8192×6144 HEIC to exercise the all-HTML renderer at 50 MP even
  though it is SDR;
- record time to first visible image, interaction quality, peak memory, group-switch
  cleanup, and recovery;
- compare against the current implementation evidence without claiming that a headless
  open time is a production measurement.

If M1 cannot be run before merge, the implementation may merge only with user-facing
wording `HDR path unverified` and must not mark HDR acceptance complete.

#### Documentation

After gates:

- update `README.md` known limitations/support summary;
- update `docs/GUIDE.md` with badge meanings, Firefox fallback, display requirements,
  and Compare in SDR;
- add `artifacts/IMPLEM-DROP3-HDR.md` with exact tests, fixture results, browser/hardware
  evidence, unresolved gates, and honest limitations;
- update `ADR-DROP3-HDR.md` only if measured evidence changes a decision;
- do not rewrite `IMPLEM-DROP2.md` history except for a short forward reference if
  needed.

## 10. Expected file changes

### New

- `apps/web/src/sources/jpeg-hdr.ts` — bounded range reader, JPEG marker/XMP/ISO
  inspector, stable reasons, and source annotation;
- `apps/web/src/viewer/hdr.ts` — runtime capability policy, dynamic-range mode, and pure
  presentation-status matrix;
- `apps/web/tests/hdr.test.ts` — synthetic parser/range/policy tests and committed 10/2
  fixture classification;
- `apps/web/tests/e2e/hdr.spec.ts` — reproducible three-engine Blob range and HTML
  drawer/clip/lifecycle contracts plus HDR status flows;
- `artifacts/IMPLEM-DROP3-HDR.md` — created during implementation wrap-up, not before
  evidence exists.

### Modified

- `apps/web/src/sources/index.ts` — export HDR source helpers/types;
- `apps/web/src/platform/types.ts` — transient resolved-source/runtime capability fields;
- `apps/web/src/platform/browser.ts` — central browser capability snapshot and source
  annotation;
- `apps/web/src/platform/tauri.ts` — asset-URL annotation and unverified default;
- `apps/web/src/viewer/openseadragon.ts` — explicit HTML drawer;
- `apps/web/src/components/AlbumShell.svelte` — album-lifetime dynamic-range mode;
- `apps/web/src/components/PhotoFrame.svelte` — per-photo status/media-query lifecycle;
- `apps/web/src/components/GroupViewer.svelte` — controlled mode and toolbar selector;
- `apps/web/src/components/ComparisonSlider.svelte` — accessible two-source status only
  if needed after component testing;
- `apps/web/tests/resolver.test.ts` — source annotation/release/error cases;
- `apps/web/tests/runtime.test.ts` — centralized runtime policy;
- `apps/web/tests/components.test.ts` — status, media query, SDR mode, lifecycle;
- `apps/web/tests/e2e/viewer.spec.ts` — HTML primitive, HDR fixture, grid/slider checks;
- `apps/web/tests/e2e/heic.spec.ts` — HTML-drawer HEIC regression only; no HEIC HDR
  behavior change;
- `README.md` and `docs/GUIDE.md` after measured evidence.

### Not expected on the accepted path

- Rust schema/core/CLI files;
- `schema/camcomp.schema.json` or generated frontend analysis contracts;
- HEIC decoder/worker implementation;
- Cargo or npm dependency manifests;
- source photos or `camcomp.json` fixture contents;
- service-worker runtime cache rules.

The Tauri range-failure contingency is the sole anticipated exception: it may modify
`apps/desktop/src-tauri/src/commands.rs` and command registration plus
`apps/web/src/platform/tauri.ts`. It must not modify core/schema/CLI contracts or
transfer full image bytes through JSON IPC. Record that deviation in implementation
notes before proceeding.

## 11. Verification commands

### Focused inner loop

```sh
mise run web:fmt
mise run web:lint
mise run web:typecheck
mise run web:test
mise run web:build
```

Run focused Vitest files during each phase before the full frontend suite. Use existing
Playwright project selection for Chromium/WebKit/Firefox while iterating, then run the
full matrix.

### Integration

```sh
mise run web:e2e
mise run tauri:check
mise run web:contract:check
```

### Final repository gate

```sh
mise run check
```

Inspect Playwright `test-results`/report output on any failure before changing product
behavior or tests. HDR physical-output gates remain manual and are recorded separately
from the passing headless suite.

## 12. Contingencies and rollback criteria

### 12.1 HTML drawer interaction/performance failure

If automated behavior or Gate M4 fails:

1. retain source classification/status work;
2. use HTML only for detected gain-map JPEGs;
3. retain the current WebGL/Canvas renderer for confirmed SDR display sources;
4. rerun navigation/interpolation parity and the complete mixed-renderer matrix;
5. disclose mixed renderer behavior.

Do not silently restore the SDR drawer for gain-map JPEGs.

### 12.2 Clip-path collapses HDR

If direct/grid HTML is HDR but slider clipping is SDR:

- keep grid HDR support;
- disable slider while automatic HDR is selected for a group containing a detected
  gain-map JPEG;
- keep slider available only after the user selects `Compare in SDR`, with that state
  visible;
- do not add opacity/blending experiments in this delivery or call the slider
  HDR-capable until it independently passes M1.

### 12.3 Tauri range fetch failure

If asset URL ranges fail only in packaged Tauri:

- keep image display functional and return `unverified-source` until the fallback lands;
- add a bounded Rust helper inside `apps/desktop/src-tauri/src/commands.rs` and invoke it
  from the existing `resolve_photo_source_sync` after the actual display path is chosen;
- extend `ResolvePhotoSourceResponse` with the same encoding/validation/reason values;
- make `TauriPlatformAdapter.resolvePhoto` trust that bounded native result and skip URL
  inspection;
- run the same synthetic metadata cases and 10/2 fixture expectation in Rust and
  TypeScript to prevent drift;
- change only the existing response shape; add no second path-taking command and send no
  image buffer over IPC.

### 12.4 Forced-SDR CSS unsupported

If a runtime can show HDR but cannot enforce `dynamic-range-limit: standard`:

- report SDR enforcement as unavailable/unverified;
- consider a group-level Canvas SDR renderer only as an explicit user-selected mode;
- never represent ignored CSS as a successful all-SDR comparison.

### 12.5 Classifier disagreement with native rendering

If a valid native HDR JPEG is not recognized:

- preserve HTML pass-through and show `unverified`, not `SDR source`;
- add a legally usable minimal fixture for that metadata variant;
- extend only the marker parser required by the fixture;
- update the capability table and ADR if the format model changes.

If metadata is recognized but native output remains SDR, downgrade runtime capability;
do not weaken parser validation to force a positive status.

## 13. Security, privacy, and resource requirements

- All requests target existing local Blob/Tauri asset URLs.
- No source bytes, metadata, filenames, or status leave the machine.
- Metadata body is capped at 512 KiB.
- Gain-map length parsing uses safe integers and total-size bounds.
- Tail validation reads only the minimum bytes required.
- JPEG marker iteration rejects overflow, zero/invalid segment lengths, and truncation.
- Parser does not resolve XML entities or external resources.
- Inspection accepts and propagates cancellation.
- Failed inspection does not leak object URLs or duplicate source release.
- Selected photos remain absent from service-worker caches.
- No browser name or file path is logged as part of capability detection.

## 14. Definition of done

### Automated

- [ ] All 10 committed gain-map JPEGs and two SDR controls classify correctly.
- [ ] Synthetic Ultra HDR v1, ISO-only, and dual metadata cases pass.
- [ ] Malformed/oversized/truncated metadata is bounded and non-fatal.
- [ ] Browser Blob and mocked Tauri range paths share one inspector.
- [ ] OpenSeadragon uses one `img` and zero actual canvases per source in all three E2E engines.
- [ ] Grid, slider, fit, zoom, aligned navigation, remount, cleanup, and HEIC regression pass.
- [ ] Firefox displays a persistent SDR fallback only for detected gain-map JPEGs.
- [ ] SDR-display, forced-SDR, active-path, and unverified-runtime states are covered.
- [ ] No HEIC fixture is classified or claimed as HDR.
- [ ] Formatting, lint, typecheck, unit/component tests, E2E, builds, contracts, Tauri check, and `mise run check` pass.

### Manual

- [ ] M1 Chrome HDR parity passes against direct `<img>`.
- [ ] M1 Safari HDR parity passes independently.
- [ ] Grid and slider retain HDR or the slider contingency is invoked.
- [ ] Compare in SDR visibly constrains the full group where claimed.
- [ ] Firefox M2 fallback wording and accessibility pass.
- [ ] Every documented Tauri host passes M3; otherwise it remains explicitly unverified.
- [ ] M4 memory/interaction/cleanup evidence is recorded on named hardware.
- [ ] A real dual Ultra HDR v1 + ISO 21496-1 fixture is manually checked before ISO is documented as tested output.

### Documentation

- [ ] `IMPLEM-DROP3-HDR.md` records exact automated and manual evidence.
- [ ] README and guide describe only capabilities that passed their gates.
- [ ] HEIC remains documented as SDR/current-display support, not HDR support.
- [ ] Known limitations distinguish source HDR, renderer path, runtime codec, and display headroom.

## 15. Requirement traceability

| Accepted ADR requirement                     | Plan coverage                         |
| -------------------------------------------- | ------------------------------------- |
| Preserve original gain-map JPEG              | §§4, 6; no transcode                  |
| Use native HTML image primitive              | §§2.3, 7.4; Phase 2                   |
| No default OSD WebGL/Canvas for HDR          | §7.4; E2E primitive assertions        |
| Keep slider only with proof                  | Phase 5 M1; §12.2 contingency         |
| Classify HDR independently of camera mode/P3 | §§3.3, 5, 6; Phase 0                  |
| Warn on Firefox/SDR/unverified output        | §7.2; Phase 3                         |
| Prefer verified HDR JPEG sidecars            | §§5, 6.5                              |
| Preserve deliberate SDR comparison           | §§7.2–7.3; Phase 3                    |
| Never mix unlabelled output states           | §8; Phase 3                           |
| Defer HEIC HDR                               | §§2.2, 3.2; no HEIC HDR phase         |
| Keep local/privacy contract                  | §13                                   |
| Require real HDR hardware evidence           | Phase 5 and manual definition of done |
