# ADR-DROP3-HDR: HDR-Preserving Photo Display

- **Status:** Accepted
- **Date:** 2026-08-27
- **Scope:** Drop 3 JPEG gain-map HDR rendering; HEIC/HEIF HDR is audited but explicitly deferred
- **Decision type:** Audit report and follow-on rendering decision; no implementation is included
- **Sources:** `README.md`, `ADR-DROP2.md`, `IMPLEM-DROP2.md`, the implemented browser/Tauri source resolvers, OpenSeadragon 6.1.0, `heic-to` 1.5.2, `heif-oxide` 0.1.0, committed photo fixtures, and the external references in §13

## 1. Executive conclusion

The current viewer does **not** deliver HDR to the display on any audited path.
This remains true when camcomp forwards an original JPEG Ultra HDR file or a
native-decodable HEIC file without changing its bytes. Source-byte preservation is
necessary, but OpenSeadragon then draws the decoded image through an 8-bit WebGL or
Canvas 2D surface. That final rendering step collapses the path to SDR.

The audit found four separate concerns that must not be conflated:

1. whether the selected file contains an HDR rendition or gain map;
2. whether source resolution preserves that representation;
3. whether the browser/runtime can decode it as HDR;
4. whether every subsequent rendering and compositing primitive preserves extended
   dynamic range until scan-out to an HDR-capable display.

All four must pass. A high-dynamic-range source shown on an SDR display is correctly
adapted to SDR, but an HDR source silently reduced by camcomp, its decoder, its viewer,
or its browser is a product fallback that must be disclosed.

**10 of the 12 JPEG fixtures** are already impacted, as they are gain-map JPEGs with
`XMP-hdrgm:Version=1.0`, a `Primary, GainMap` container directory, and a second MPF
JPEG image.

The least invasive viable direction is:

- retain the exact encoded gain-map JPEG or an explicitly verified HDR JPEG sidecar;
- render known HDR sources through a real HTML image element, provisionally
  OpenSeadragon's `html` drawer, rather than its default WebGL/Canvas drawers;
- preserve the existing CSS clip-path slider only after an HDR-display acceptance
  test confirms that the browser compositor keeps the image in HDR;
- detect and record JPEG gain-map characteristics separately from output capability;
- show a persistent, specific **SDR fallback** warning whenever any required stage is
  unavailable, including Firefox's base-JPEG fallback;
- keep each currently generated PNG as a clearly labelled SDR derivative, not an HDR
  or color-faithful substitute.

All 18 HEIC files in the repository's complete current corpus were inspected with
`heif-info` and ExifTool 13.55. Every file is an 8-bit YCbCr primary with a Display-P3
ICC profile; none reports PQ/HLG transfer signaling, an HDR/gain-map marker, or an
alpha/depth/HDR auxiliary channel. They are confirmed wide-gamut SDR for this delivery.

The accepted implementation scope is therefore **JPEG gain-map HDR plus macOS
Tauri HEIC preservation**. Existing non-macOS HEIC display and SDR conversion behavior
remains unchanged. On macOS Tauri, the original HEIC may now reach WKWebView's native
HTML image path without a PNG/JPEG intermediate; packaged HDR rendering remains
unverified until Gate M3 passes. HEIC HDR extraction, conversion, and cross-platform
fixtures remain deferred. The HEIC audit remains in this document to prevent the
portable `heif-oxide` PNG path from being mistaken for HDR-capable output.

## 2. Terminology and required invariants

### 2.1 HDR capture is not necessarily an HDR display file

A camera mode called `HDR+`, exposure fusion, local tone mapping, Display-P3 color, a
10/16-bit channel, and a displayable HDR rendition are different properties. An
8-bit SDR JPEG produced by an HDR capture pipeline is still an SDR display file.
Conversely, a JPEG can carry an 8-bit SDR base plus a gain map that reconstructs an
HDR rendition.

Camcomp must therefore derive display status from the file structure and the active
rendering path, not from `modeLabel`, filename, extension, bit depth alone, or the
presence of a wide-gamut profile.

### 2.2 Gain-map JPEG

Android Ultra HDR v1 stores a conventional SDR JPEG, a compressed gain-map image, and
metadata in one backward-compatible file. A legacy decoder displays the SDR base. A
compatible decoder combines the base and gain map according to image metadata and
available display headroom. ISO 21496-1 defines interoperable gain-map metadata; the
Android guidance recommends carrying both Ultra HDR v1 and ISO metadata when maximum
cross-platform compatibility is needed.

The gain map is not EXIF decoration that can be copied after pixels are flattened. It
is an additional image plus rendering parameters. Decoding to an ordinary 8-bit
bitmap or re-encoding to an ordinary PNG permanently removes the HDR rendition.

### 2.3 HEIC/HEIF HDR

HEIF may represent HDR as an intrinsically HDR primary image, an auxiliary HDR gain
map, or other platform-defined metadata. Apple documents both 10-bit HEIC HDR and an
Apple HDR gain-map auxiliary image. A correct conversion must understand the actual
variant, color primaries, transfer function, matrix/range, ICC profile where present,
orientation, and auxiliary relationships.

### 2.4 Wide color gamut is not high dynamic range

Display P3 expands chromaticity but does not by itself encode luminance above HDR
reference white. An 8-bit Display-P3 HEIC can be wide-gamut SDR. Likewise, a
`display-p3` Canvas 2D or WebGL drawing buffer is not automatically an HDR buffer.

### 2.5 Preservation invariant

For an HDR result, camcomp must preserve all of the following until final
composition:

- the SDR base and gain-map image plus gain-map metadata, or an equivalent intrinsic
  HDR representation;
- color-space and transfer-function semantics;
- precision and values above SDR reference white;
- runtime permission to use available display headroom;
- a compositor path that does not insert an SDR intermediate surface.

## 3. Implemented rendering-path audit

### 3.1 End-to-end matrix

| Runtime and source path                 | Encoded HDR reaches source URL?                                      | Conversion before viewer                                                                  | Final OpenSeadragon path                                          | Current result                                                   |
| --------------------------------------- | -------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------------------------------- |
| Browser exact JPEG Ultra HDR            | Yes; the original `File` becomes an object URL                       | None                                                                                      | Default WebGL, normally; Canvas on iOS-like platforms or fallback | **SDR; gain-map result is flattened**                            |
| Tauri exact JPEG Ultra HDR              | Yes; the original is served by Tauri's asset protocol                | None                                                                                      | Same shared OpenSeadragon frontend                                | **SDR; gain-map result is flattened**                            |
| Browser native HEIC, expected on Safari | Yes; the exact HEIC object URL is retained after a decode probe      | None                                                                                      | WebGL on macOS Safari; Canvas on iOS-like platforms               | **SDR in camcomp; native decode alone is insufficient**          |
| Browser HEIC with JPEG sidecar          | Yes, if the sidecar itself carries HDR                               | Sidecar substitution                                                                      | WebGL/Canvas                                                      | **SDR**                                                          |
| Browser HEIC with PNG sidecar           | Unknown; PNG is not inspected and the HEIC gain map is not forwarded | Sidecar substitution                                                                      | WebGL/Canvas                                                      | **SDR through the current viewer**                               |
| Browser HEIC worker                     | No                                                                   | `heic-to` primary image → 8-bit `ImageData`/`ImageBitmap` → default OffscreenCanvas → PNG | WebGL/Canvas                                                      | **Irreversible SDR derivative**                                  |
| Tauri HEIC with JPEG sidecar            | Yes, only if that sidecar is independently verified as HDR           | Sidecar substitution                                                                      | WebGL/Canvas                                                      | **SDR**                                                          |
| Tauri HEIC with PNG sidecar             | Unknown; PNG is not inspected and the HEIC gain map is not forwarded | Sidecar substitution                                                                      | WebGL/Canvas                                                      | **SDR through the current viewer**                               |
| Tauri HEIC decode                       | No                                                                   | `heif-oxide` primary decode → bounded sRGB raster → 8/16-bit PNG without HDR semantics    | WebGL/Canvas                                                      | **Irreversible SDR and potentially color-inaccurate derivative** |
| Grid versus two-photo slider            | Same as source row                                                   | No additional image encoding                                                              | Two viewer surfaces; slider clips one outer DOM layer             | **No new conversion, but cannot restore HDR already lost**       |

### 3.2 Browser exact JPEG and PNG

`BrowserSourceResolver.resolvePhoto` passes every non-HEIC file directly to
`URL.createObjectURL` and reports `sourceKind: "exact"`
(`apps/web/src/sources/resolver.ts:102-143`). This correctly preserves every byte of a
JPEG Ultra HDR file up to the viewer boundary. There is no special Ultra HDR
recognition, status, or warning.

An exact PNG is also forwarded unchanged. That fact does not make it HDR. The current
application produces no HDR-tagged PNG, and an ordinary 8/16-bit PNG does not retain a
JPEG or HEIC auxiliary gain map. Sixteen-bit integer depth increases precision; it
does not define values above reference white.

### 3.3 Browser HEIC native/sidecar cascade

`resolveHeicPhotoSource` first probes the exact HEIC with `createImageBitmap` and then
an HTML `Image`; if either path reports valid dimensions, it returns the original
object URL (`apps/web/src/sources/heic.ts:69-92,143-175`). The probe establishes that
some image can be decoded. It does **not** establish that an auxiliary gain map was
recognized, that an intrinsic HDR transfer function was preserved, or that final
output is HDR.

If native decode fails, same-stem JPEG/JPEG/PNG sidecars are accepted solely by
oriented dimensions (`apps/web/src/sources/heic.ts:94-117`). Dimensions cannot prove
identical content, color, or dynamic range. The UI says `Sidecar` and gives a generic
substitution warning, but does not say whether HDR was preserved.

The sidecar order prefers JPEG over PNG, which is directionally useful because a JPEG
may carry a compatible gain map. It is not sufficient: the resolver does not inspect
that JPEG for a gain map.

### 3.4 Browser HEIC worker

The browser fallback is unambiguously SDR:

1. `heic-to` 1.5.2 decodes `data[0]`, the first image returned by libheif, into a
   standard `ImageData` backed by `Uint8ClampedArray`
   (`node_modules/heic-to/src/worker.js:5-35`). It does not expose or combine an HDR
   auxiliary image in this API.
2. Camcomp asks `heic-to` for an `ImageBitmap`, draws it to an `OffscreenCanvas` using
   a default 2D context, and calls `convertToBlob({ type: "image/png" })`
   (`apps/web/src/workers/heic-decoder.worker.ts:30-58`).
3. No HDR color space, float backing store, gain-map metadata, ICC profile, or transfer
   function is requested or written.

Calling this result a `lossless worker decode` or `Decoded copy` is true only in the
narrow sense that PNG adds no further lossy compression to the already flattened
8-bit bitmap. It is **not** lossless with respect to HDR, auxiliary images, original
color semantics, or source encoding. Future UI wording should say `SDR decoded copy`.

The existing 2.1 GiB peak-memory warning is orthogonal. User consent to high memory is
not consent to an undisclosed SDR/color fallback.

### 3.5 Tauri exact and sidecar sources

At the application layer, Tauri returns the exact JPEG/PNG or accepted sidecar path
without re-encoding (`apps/desktop/src-tauri/src/commands.rs:796-844` and
`apps/web/src/platform/tauri.ts:177-197`). The source of pinned Tauri 2.11.5's asset
protocol was also audited: it opens the selected file, derives `Content-Type` from
magic bytes/path, and returns the original complete or ranged bytes without image
decoding or transcoding. As in browser mode, this preserves encoded Ultra HDR bytes
only until OpenSeadragon consumes the URL. Gate H2 still verifies byte identity, MIME,
and behavior in each packaged runtime.

Tauri checks sidecar dimensions and warns that compression, color, crop, or content
may differ (`apps/desktop/src-tauri/src/commands.rs:630-670`). It does not classify HDR
or prefer a proven HDR sidecar over an unproven same-size one.

### 3.6 Tauri HEIC decode

Tauri decodes HEIC using `heif-oxide` 0.1.0 and writes a digest-keyed PNG through the
`image` crate. PNG encoding is in `apps/desktop/src-tauri/src/commands.rs:696-733`;
decode, cache, and validation are in `apps/desktop/src-tauri/src/commands.rs:735-794`.
The encoder retains 8- versus 16-bit integer storage from the returned raster but
writes no gain map or HDR transfer semantics.

The pinned decoder's own contract makes the limitation explicit:

- output is bounded, display-ready sRGB;
- an ICC profile is noted but not applied;
- Display-P3/BT.2020 primaries are converted to sRGB and clamped;
- PQ and HLG transfer functions are **not** tone-mapped;
- transfer handling assumes an sRGB-like curve;
- only a supported alpha auxiliary is considered; an HDR gain-map auxiliary is not
  decoded or applied.

See `heif-oxide-0.1.0/src/lib.rs:9-15,23-32,61-76,134-164` and
`heif-oxide-0.1.0/src/color.rs:1-15,191-220,284-319` in the local Cargo registry for
the audited package source.

Consequences are more severe than metadata loss. An intrinsic PQ/HLG HEIC can be
converted with the wrong transfer function and clipped. An ICC-only wide-gamut HEIC
can be interpreted without applying that profile. The output PNG may therefore be
neither HDR nor a faithful SDR rendition.

The current cache suffix, `heif-oxide-v1.png`, does at least permit deterministic
invalidation when a future renderer changes, but current derivatives must not be
reclassified as HDR.

### 3.7 OpenSeadragon is the decisive SDR boundary

Camcomp creates `OpenSeadragon.ImageTileSource({ url, buildPyramid: false })` and does
not configure a drawer (`apps/web/src/viewer/openseadragon.ts:11-42`). OpenSeadragon
6.1.0 therefore uses its default candidate order:

- non-iOS-like platforms expand `auto` to WebGL then Canvas;
- iOS-like platforms choose Canvas for `auto`;
- HTML is only a late fallback after renderers that are normally available.

The simple source initially loads the URL into an `HTMLImageElement`; with
`buildPyramid: false`, it hands that image to the drawer without generating lower
levels (`openseadragon.js:17687-17720,17780-17813`). That is still not the final
primitive.

The default WebGL drawer:

- creates a normal WebGL2/WebGL context without an HDR output configuration;
- allocates render textures as `RGBA`/`UNSIGNED_BYTE`;
- uploads source images as `RGBA`/`UNSIGNED_BYTE`;
- ultimately composites to an ordinary output canvas
  (`openseadragon.js:25070-25085,25226-25249,25263-25277,25281-25312`).

The Canvas drawer obtains `canvas.getContext("2d")` with no color-space or float
options and draws image data into that canvas (`openseadragon.js:23989-24054`). The
OpenSeadragon conversion graph also creates default 2D canvases when converting image
or bitmap data (`openseadragon.js:18598-18603,18651-18667`).

Both are SDR intermediates. The gain-map container may have reached the first hidden
`Image`, and a compatible browser may have decoded an HDR rendition internally, but
this normalized sRGB/default output path has no HDR transfer or output configuration
with which to preserve values above SDR reference white.

OpenSeadragon's HTML drawer is materially different. It clones the
`HTMLImageElement`, sizes it with CSS, and places it in positioned DOM elements without
copying pixels through Canvas (`openseadragon.js:23695-23754`). This is the best
low-change candidate for preserving browser-native HDR while retaining current
OpenSeadragon navigation. It is a candidate, not an accepted result, until real HDR
hardware proves:

- gain-map JPEG and native HEIC remain HDR after clone/insertion and CSS transforms;
- pan, zoom, interpolation, resizing, and cleanup remain acceptable at 50 MP;
- grid and clip-path slider composition do not cause an SDR intermediate surface;
- browser/runtime differences are surfaced correctly.

If the HTML drawer fails those gates, a camcomp-owned `<img>` transform renderer is
preferred over claiming HDR from an opaque canvas path.

### 3.8 Slider and alignment

The two-photo slider applies `clip-path: inset(...)` to an outer viewer layer
(`apps/web/src/components/ComparisonSlider.svelte:19-42`). It neither reads nor
re-encodes pixels. Global alignment maps viewport centers only; it does not warp image
pixels (`ADR-DROP2.md:357-377`). These layers are not the present cause of SDR.

They still require an end-to-end HDR test. Browsers may allocate intermediate
compositing surfaces for clipping, transforms, opacity, or effects. A source-level
argument cannot prove final display luminance.

## 4. Fixture evidence and missing coverage

### 4.1 JPEG fixtures

ExifTool 13.55 inspected all 12 committed JPEG fixtures:

- 10 Pixel JPEGs contain `XMP-hdrgm:Version=1.0`;
- those 10 report `XMP-GContainer:DirectoryItemSemantic = Primary, GainMap`;
- those 10 have two MPF JPEG images and an extracted `Google:GainMapImage`;
- the two labelled PeakFinder derivatives in `tstgrp2/g3-*` have no reported gain map.

This is useful deterministic test material. At minimum, format inspection tests should
classify the 10/2 split without decoding 50 MP pixels.

It does not prove ISO 21496-1 metadata is present. The observed files demonstrate the
Ultra HDR v1/XMP variant and should be supplemented with a dual-metadata ISO 21496-1
fixture.

A bounded classifier spike found the Ultra HDR namespace, `hdrgm:Version`, gain-map
container item, and MPF declaration within the first 128 KiB of every affected fixture.
A 256 KiB head probe plus the declared gain-map length and a tiny tail probe classified
all 10 gain-map JPEGs and both SDR JPEGs correctly while confirming a complete trailing
JPEG. The implementation should use a marker-aware parser and a 512 KiB metadata cap,
not an unbounded whole-file read or blind unrestricted string search.

A separate no-code browser spike forced OpenSeadragon 6.1.0's HTML drawer in Chromium,
WebKit, and Firefox. Each engine opened the representative 4080×3072 Ultra HDR JPEG as
one `<img>` with the original URL, created zero `<canvas>` elements, retained the outer
35% `clip-path`, and removed the image on destroy. WebKit also opened an 8192×6144 HEIC
through the same one-image/zero-canvas path. This closes DOM-path feasibility only;
headless output cannot prove physical HDR luminance or production memory behavior.

### 4.2 HEIC fixtures

`heif-info` and ExifTool inspected all 18 committed HEIC fixtures—the complete HEIC
corpus currently present in the repository. Every file reports an 8-bit YCbCr primary,
8-bit luma/chroma, and a Display-P3 ICC profile. None reports PQ/HLG transfer signaling,
an HDR/gain-map marker, or an alpha/depth/HDR auxiliary channel. These fixtures are
confirmed wide-gamut SDR for the current delivery, not HDR-display files.

The existing HEIC tests therefore remain tests of dimensions, decoding, memory
consent, viewer lifecycle, and SDR presentation. Worker/Tauri color-profile fidelity
remains a separate limitation already acknowledged in
`IMPLEM-DROP2.md:185-197,268-275`; it is not part of the JPEG HDR implementation plan.

If HEIC HDR is requested later, add at least these independent cases before accepting
an HDR conversion path:

- Apple auxiliary gain-map HEIC;
- 10-bit PQ HEIC;
- 10-bit HLG HEIC;
- wide-gamut SDR HEIC with ICC-only color information;
- malformed/missing auxiliary metadata;
- orientation plus an HDR auxiliary relationship.

Fixtures must have documented provenance and expected color/headroom values, not only
an image that appears bright.

## 5. Browser and display capability

### 5.1 Current practical matrix

This matrix describes current intended desktop targets and must be kept under manual
acceptance because codec support, operating-system HDR state, GPU composition, and
physical display all participate.

| Runtime                         | JPEG Ultra HDR / gain-map behavior through native `<img>`                                                 | HEIC through native `<img>`                                                                     | Required camcomp behavior                                                                                      |
| ------------------------------- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Current Chrome/Edge/Brave/Opera | Gain-map HDR is supported on compatible systems/displays; SDR base otherwise                              | Do not assume cross-platform native HEIC                                                        | Use HTML image path for a known HDR JPEG; show active/SDR status                                               |
| Current Safari                  | Current Safari supports JPEG gain-map HDR on compatible Apple systems; HEIC has the strongest native path | Supported where an actual selected-file probe succeeds, but HDR variant still needs a real test | Use HTML image path; retain probe; test JPEG and HEIC independently                                            |
| Firefox                         | JPEG decodes successfully but the gain map is not currently applied; the SDR base is shown                | No native HEIC product assumption                                                               | Always warn `HDR source shown in SDR: this Firefox does not apply the JPEG gain map` for a known gain-map JPEG |
| Tauri macOS                     | WKWebView capability is OS/WebKit-version dependent                                                       | Native webview decode must not be assumed equivalent to ImageIO APIs                            | Test packaged runtime; current shared OpenSeadragon path is SDR                                                |
| Tauri Windows                   | WebView2 follows installed Chromium runtime and OS/GPU HDR state                                          | No general HEIC guarantee                                                                       | Test packaged runtime; current shared OpenSeadragon path is SDR                                                |
| Tauri Linux                     | WebKitGTK/display-stack capability varies                                                                 | No general HEIC guarantee                                                                       | Treat HDR as unconfirmed unless a named target passes the gate                                                 |

Chromium-family support should not be duplicated as five independent rendering
implementations: Edge, Brave, and Opera inherit the relevant Blink/Skia behavior, but
release lag and flags still justify smoke tests.

### 5.2 Output-device detection is necessary but insufficient

`matchMedia("(dynamic-range: high)")` is available across current Chromium, Safari,
and Firefox according to MDN browser-compat data. It reports that the user-agent and
output combination can display a high dynamic range. It does not report that:

- a file contains HDR;
- a JPEG gain map was recognized;
- HEIC decoded as HDR;
- the viewer retained HDR;
- the operating system will allocate the same headroom to every window.

A browser that only displays the SDR base may still return `high` because the monitor
can display other HDR content.

### 5.3 Dynamic-range CSS

The CSS Color HDR draft defines inherited `dynamic-range-limit`, initially
`no-limit`. MDN browser-compat data currently records it in Chrome 136+ and Safari
26+, but not Firefox. Setting `dynamic-range-limit: no-limit` can make camcomp's intent
explicit on its HDR image subtree; it cannot turn an SDR canvas or unsupported codec
into HDR.

### 5.4 There is no sufficient gain-map-rendered feature probe

The following probes are individually insufficient:

- image `load`/`decode()` succeeds: Firefox correctly decodes the SDR base;
- dimensions match: base and HDR rendition normally have the same output dimensions;
- `createImageBitmap` succeeds: it says nothing about retained HDR semantics;
- `CSS.supports("dynamic-range-limit: no-limit")`: CSS support is not image-codec
  support;
- `(dynamic-range: high)`: display capability is not current-image behavior.

No audited standards API directly answers “the gain map for this element is being
applied to the final HDR surface.” Until one exists, camcomp needs a maintained,
versioned runtime capability policy backed by a known-gain-map fixture and manual
checks on named hardware. Unknown must be reported as unknown/SDR-risk, never silently
as confirmed HDR.

## 6. Required product status model and warnings

A single `sourceKind` (`exact`, `sidecar`, `decoded`, `native`) cannot express HDR.
Future implementation should model these independent facts:

- **source dynamic range:** `sdr | gain-map-hdr | intrinsic-hdr | unknown`;
- **source color:** declared primaries/profile and transfer function where known;
- **representation:** `original | sidecar | generated-derivative`;
- **HDR preservation:** `preserved | converted-with-proof | lost | unknown`;
- **renderer:** `html-image | canvas-sdr | canvas-hdr | native | unknown`;
- **output:** `hdr-active | sdr-display | sdr-codec | sdr-conversion | sdr-renderer |
unverified`.

The visible UI can collapse these into concise badges/details:

- `HDR active` — only after every required stage is known to pass;
- `HDR source · SDR display` — renderer/codec is capable but the current output has no
  HDR headroom;
- `SDR fallback · Firefox does not apply this JPEG gain map`;
- `SDR fallback · HEIC was decoded to PNG; HDR data was not preserved`;
- `SDR fallback · PNG sidecar has no verified HDR rendition`;
- `HDR output unverified in this desktop runtime`.

Warnings must be per-photo and remain visible during comparison. A transient import
message is insufficient because two members can take different paths. Accessibility
requires text, not color or a badge alone.

Do not warn merely because a source is JPEG/HEIC: warn only when HDR is detected or
reasonably indicated and the output is not active. If file inspection returns
`unknown`, say so rather than asserting the source was HDR.

## 7. Alternatives

### 7.1 Native HTML image rendering, with OpenSeadragon HTML drawer first

**Description:** Keep the encoded HDR file intact and make the final visual primitive
an `HTMLImageElement`. Configure each HDR viewer instance to use OpenSeadragon's HTML
drawer, or replace only that adapter with a camcomp-owned `<img>` transform renderer if
the drawer fails.

**Advantages**

- preserves JPEG/HEIC container data for the browser's native HDR decoder;
- smallest architectural change; current normalized navigation can remain;
- avoids implementing gain-map math and color management;
- CSS clip-path can retain the current slider design;
- no new codec dependency or derivative cache;
- SDR base fallback remains inherently available for gain-map JPEG.

**Disadvantages and risks**

- OpenSeadragon's HTML drawer is less commonly used than its GPU/Canvas drawers;
- full-frame decode and DOM transforms retain the existing 50 MP memory risk;
- pan/zoom smoothness, resampling, clipping, and compositor HDR behavior require
  measurement;
- browser-native HDR remains engine/OS/display dependent;
- no custom tone-map comparison or pixel-difference shader.

**Assessment:** Preferred immediate path. Accept only after Gate H1 in §10.

### 7.2 Dual viewer: HTML for HDR, current WebGL/Canvas for SDR

**Description:** Select the HTML drawer only for a detected HDR source and retain the
existing renderer for SDR.

**Advantages**

- limits performance/regression risk for ordinary images;
- preserves current GPU path where HDR is irrelevant;
- each photo already owns an independent viewer adapter.

**Disadvantages**

- two rendering paths can differ in interpolation and motion, undermining comparison;
- source classification must complete before viewer creation;
- mixed HDR/SDR groups need deliberate dynamic-range policy and clear status;
- twice the acceptance matrix and lifecycle surface.

**Assessment:** Reasonable optimization after validating an all-HTML baseline; not a
shortcut around validation.

### 7.3 Camcomp-owned `<img>` pan/zoom renderer

**Description:** Use CSS transforms on exact image elements and implement the limited
navigation adapter directly.

**Advantages**

- strongest control that no canvas is inserted;
- direct styling with `dynamic-range-limit`;
- simple clipping and easy inspection of the final DOM primitive;
- removes OpenSeadragon HDR ambiguity.

**Disadvantages**

- camcomp owns pointer, wheel, touch, resize, bounds, interpolation, teardown, and
  synchronized-navigation behavior rejected in the original viewer ADR;
- no tile eviction or progressive level of detail;
- substantial regression/testing burden.

**Assessment:** Fallback if OpenSeadragon's HTML drawer cannot pass H1.

### 7.4 HEIC to dual-metadata gain-map JPEG

**Description:** Extract or reconstruct the HEIC HDR rendition and encode an SDR JPEG
base plus gain map carrying both Ultra HDR v1 and ISO 21496-1 metadata. Cache the
result by source digest and converter/version parameters.

Current `libultrahdr` 2.x is MIT/Apache-2.0 and exposes JPEG, HEIF, and AVIF gain-map
codecs, but its HEIF path depends on a suitable HEIF implementation. `libheif` offers
broad HDR, profile, auxiliary-image, and codec support under LGPL-3.0 and brings native
codec/security/packaging obligations. Apple ImageIO is a strong macOS-only alternative.
The current `heif-oxide` and `heic-to` public paths do not expose enough HDR data for a
faithful conversion.

**Advantages**

- uses the best-supported web HDR still-image path in current Chromium/Safari;
- retains an embedded SDR base for Firefox and SDR displays;
- can move expensive conversion out of the live viewer and cache it;
- one encoded artifact can serve browser and Tauri frontends.

**Disadvantages**

- extracting a gain map is format/vendor-specific; reconstructing one requires both
  correct HDR and SDR renditions;
- JPEG encoding can alter the base and gain map; `lossless PNG` reasoning no longer
  applies;
- orientation, ICC/CICP, headroom, XMP/ISO metadata, and visual intent need conformance
  tests;
- browser-only conversion is likely to repeat the current extreme memory cost;
- native dependencies, LGPL replacement rights, codec patents, signing, and security
  maintenance need review;
- Firefox still displays only the SDR base.

**Assessment:** Most practical non-native HEIC HDR derivative, but deferred pending a
separate conversion ADR and Gate H3.

### 7.5 Intrinsic HDR AVIF

AVIF is not a currently supported camcomp input or sidecar format. Selecting this
alternative would require discovery, metadata, decoder, resolver, MIME, contract, and
test changes in addition to the HDR conversion work.

**Description:** Decode HEIC to linear/high-bit-depth HDR and encode AVIF with correct
BT.2020/P3 primaries and PQ/HLG transfer metadata, or use a gain-map AVIF variant where
interoperable.

**Advantages**

- efficient compression, high bit depth, wide gamut, and intrinsic HDR capability;
- ordinary AVIF image-format support exists in all three target browser families;
- permissively licensed encoders are available.

**Disadvantages**

- ordinary AVIF **format support does not prove HDR scan-out support** on every
  browser/OS/display;
- intrinsic HDR lacks gain-map JPEG's universally useful authored SDR base unless a
  separate fallback is retained;
- converting HEIC correctly still requires an HDR-capable decoder and color pipeline;
- gain-map AVIF browser interoperability is less established than gain-map JPEG;
- full-frame encoding is expensive for 50 MP inputs.

**Assessment:** Worth a measured spike, not the default recommendation and not a
universal Firefox HDR solution without real evidence.

### 7.6 JPEG XL

**Advantages:** High bit depth, HDR, wide gamut, efficient compression, and strong
Safari support.

**Disadvantages:** Chromium support is only returning through a Chrome 145 origin
trial in the current research snapshot, and Firefox is not a stable cross-browser
product path. It would need fallbacks and does not solve HEIC decoding.

**Assessment:** Do not select for the required browser matrix now; reassess if stable
support converges.

### 7.7 HDR Canvas 2D custom drawer

Modern Canvas APIs can request a Display-P3 color space, and current Chromium 137+
adds a `float16` 2D context color type. The pinned OpenSeadragon drawer requests
neither. Display-P3 plus the default `unorm8` color type remains wide-gamut SDR.
Firefox and Safari do not currently expose the `colorType: "float16"` option according
to MDN browser-compat data.

A custom float Canvas drawer would still need proof that `drawImage` expands each
supported gain-map/HEIC source into extended-range values and that the browser
compositor presents the canvas as HDR.

**Advantages:** Retains Canvas transforms and may allow future pixel operations.

**Disadvantages:** Chromium-only capability today, high memory, OpenSeadragon fork or
custom drawer, no direct gain-map metadata API, and difficult end-to-end validation.

**Assessment:** Experimental Chromium path only; inferior to native `<img>` for
pass-through HDR.

### 7.8 WebGL/WebGPU gain-map renderer

**Description:** Parse base/gain-map images and metadata, upload separate textures,
apply ISO/vendor gain-map math and color conversion in a shader, and present through
an HDR-capable canvas.

**Advantages:** Full control over interpolation, comparison, tone mapping, and future
warps/differences; can make the same policy explicit across codecs.

**Disadvantages:** Current OpenSeadragon WebGL is 8-bit SDR and cannot be adapted by a
flag. A replacement must implement extraction, transfer functions, color management,
display headroom, float textures, HDR canvas presentation, context loss, tiling, and
fallbacks. HEIC decoding remains unsolved. Browser HDR-canvas support is less uniform
than native `<img>` HDR.

**Assessment:** Not justified for pass-through viewing. Reconsider only if future
requirements demand pixel-domain HDR operations.

### 7.9 Native platform image views

**Description:** In Tauri, place a platform-native HDR image view/layer beside or
behind the web UI: ImageIO/Core Image/Metal on macOS, and equivalent Windows/Linux
pipelines.

**Advantages:** Best access to platform HEIC, gain maps, display headroom, and native
color management; Apple explicitly provides HDR image-view and ImageIO APIs.

**Disadvantages:** Three platform implementations, difficult embedding and synchronized
input, separate clipping/composition behavior, and a divergence from the shared
frontend architecture. Linux HDR support remains especially environment-dependent.

**Assessment:** Strong macOS-only fidelity fallback; not the first cross-platform
choice.

### 7.10 CICP-tagged intrinsic HDR PNG

PNG Third Edition can signal BT.2100 PQ or HLG using `cICP` and can carry mastering
and content-light metadata in `mDCV`/`cLLI`. That is an intrinsic HDR PNG, not an
ordinary 16-bit PNG and not a way to forward a JPEG/HEIC gain map. Neither current
camcomp PNG encoder writes those chunks.

**Advantages:** Lossless integer samples, no JPEG artifacts, and standardized color/
transfer signaling.

**Disadvantages:** Browser HDR rendering interoperability must be proven separately;
legacy readers do not receive an authored SDR base as they do with gain-map JPEG;
HEIC conversion still needs a correct expanded HDR raster and an explicit SDR
fallback; lossless 50 MP files and decode buffers can be very large.

**Assessment:** Valid research alternative, but not selected without cross-browser HDR
and memory evidence. Merely changing the current PNG to 16-bit is insufficient.

### 7.11 Explicit SDR derivatives

**Description:** Continue producing a color-managed, tone-mapped SDR JPEG/PNG when no
HDR path exists.

**Advantages:** Universal display, predictable comparison, simpler caching, and
backward compatibility.

**Disadvantages:** Loses display headroom and may alter highlight detail, local
contrast, gamut, and camera rendering. Sixteen-bit PNG does not by itself preserve HDR
semantics. Current HEIC conversions are not proven color-managed SDR conversions.

**Assessment:** Required fallback, but only with a specific warning and a defined,
tested tone-map/color policy. It must never be called HDR or unqualified `lossless`.

## 8. Decision

Subject to implementation and acceptance gates, adopt the following policy.

1. **The original encoded representation is authoritative.** Never transcode an exact
   JPEG Ultra HDR merely to display it. Preserve both Ultra HDR v1 and ISO 21496-1
   metadata when present.
2. **Use an HTML image as the final primitive for browser-native HDR.** First test
   OpenSeadragon's HTML drawer with simple sources. If it fails, implement a narrow
   camcomp-owned image-element adapter rather than using the current SDR drawers.
3. **Do not use OpenSeadragon's default WebGL or Canvas drawer for HDR sources.** They
   may remain available for confirmed SDR sources after mixed-renderer equivalence is
   tested.
4. **Keep the current CSS slider provisionally.** It is format-preserving in source
   code, but cannot be accepted for HDR until actual Chrome/Safari/Tauri composition
   on HDR hardware matches a direct `<img>` reference.
5. **Classify source HDR independently.** In this delivery, detect known JPEG gain-map
   metadata only. Do not infer HDR from extension, P3, bit depth, or camera mode; HEIF
   HDR classification is deferred.
6. **No silent SDR fallback.** A known gain-map JPEG must show a persistent per-photo
   reason whenever output is SDR or unverified. Firefox's SDR-base decode, an SDR
   display, or an unverified embedded runtime are mandatory status cases. This also
   applies when the JPEG is used as a HEIC sidecar.
7. **Treat existing generated PNGs as SDR.** Rename future UI language from
   unqualified `lossless decode` to `SDR decoded copy`; invalidate old cache entries
   when color/HDR conversion behavior changes.
8. **Prefer a verified HDR sidecar, not merely the first dimension match.** If multiple
   same-stem candidates exist, exact source identity and explicit user choice remain
   more important than extension order. Never claim a sidecar is equivalent based on
   dimensions alone.
9. **Do not claim HEIC HDR support from native decode success.** Safari/direct HEIC and
   any future derivative each need format-specific and final-output proof.
10. **Defer HEIC-to-HDR conversion.** Evaluate dual-metadata gain-map JPEG first if
    non-native HEIC HDR becomes required. AVIF, custom Canvas/GPU, and native views are
    alternatives, not accepted implementations.
11. **Preserve an SDR viewing mode.** Comparison fairness may benefit from forcing all
    group members to SDR even on an HDR display. The UI should distinguish deliberate
    `Compare in SDR` from an unavoidable fallback.
12. **Never mix unlabelled output states.** If one photo is HDR-active and another is
    SDR, disclose it prominently; provide a one-action all-SDR comparison mode.

## 9. Rationale

The decision minimizes transformations before display and delegates format-specific
HDR reconstruction to the browser path that already implements it. This is both less
code and more likely to match the authored rendition than reconstructing gain maps in
camcomp.

It also retains the useful boundaries accepted by `ADR-DROP2.md`: source resolution,
viewer adapter, normalized navigation, and clip composition remain separate. The
change is a capability-aware viewer selection rather than a rewrite of grouping,
alignment, or import.

A custom GPU renderer would offer theoretical control but begins after the hardest
problems—HEIC auxiliary extraction, color semantics, browser HDR scan-out, and
hardware validation—have already been solved. It is not a cheaper preservation path.

## 10. Acceptance gates

No HDR support claim is complete until all applicable gates pass.

### Gate H0 — source classification

- classify all 10 committed gain-map JPEGs as HDR and both other JPEGs as not detected;
- classify a dual Ultra HDR v1 + ISO 21496-1 JPEG fixture;
- validate the declared gain-map item against a bounded trailing-JPEG probe;
- reject malformed/truncated/oversized metadata without an unbounded whole-file read;
- preserve source identity and never infer sidecar equivalence from dimensions.

HEIC HDR classification is not required for this delivery.

### Gate H1 — HTML viewer

On named HDR hardware with OS HDR enabled, compare camcomp against a direct
untransformed `<img>` reference for:

- Ultra HDR v1 JPEG;
- dual-metadata ISO 21496-1 JPEG;
- grid and slider modes at representative zoom levels;
- CSS transform, clip-path, resize, full-screen, and group-remount transitions;
- two, three, and four approximately 50 MP images;
- memory peak, interaction smoothness, cleanup, and recovery.

Acceptance requires visually and instrumentally credible highlight/headroom parity,
not merely matching dimensions or screenshots captured through an SDR path.

### Gate H2 — browser/runtime status

For each supported release channel and packaged Tauri runtime:

- validate HDR display on a named display/GPU/OS combination;
- validate SDR-display adaptation;
- validate Firefox's SDR-base warning;
- test standalone Safari separately from the macOS Tauri WKWebView rather than
  transferring a Safari result to the embedded runtime;
- verify that Tauri's asset protocol returns the expected image MIME type and
  byte-identical complete/range content for a known Ultra HDR JPEG;
- validate browser update/version-policy behavior;
- ensure `dynamic-range: high` is not treated as proof of image HDR;
- ensure unknown capability is shown as unverified, not HDR-active.

Core matrix: Chrome, Edge, Safari, Firefox, and Tauri on available target hosts. Brave
and Opera require Chromium-derivative smoke tests before being documented as
supported.

### Gate H3 — any HEIC conversion (deferred reopening gate)

Gate H3 is not part of the current definition of done. If actual HDR HEIC support is
requested later, a converted derivative may say HDR only after all of the following:

- decode real auxiliary gain-map, PQ, and HLG fixtures correctly;
- preserve or deliberately transform ICC/CICP, primaries, transfer, matrix, range,
  orientation, and headroom metadata;
- define whether the SDR base is copied, color-converted, tone-mapped, or recompressed;
- encode interoperable Ultra HDR v1 and ISO 21496-1 metadata where JPEG is selected;
- compare source and derivative numerically in linear light and visually on HDR/SDR
  displays;
- record peak memory/time/cache size at 50 MP;
- review codec licenses, patents, native distribution, security updates, and cache
  invalidation;
- retain an explicit warning if conversion is only an SDR rendition.

### Gate H4 — regression and accessibility

- unit tests for source/output-state transitions and exact warning reasons;
- component tests for persistent text status in every viewer and slider mode;
- E2E DOM tests that assert the selected drawer/primitive rather than only
  `.openseadragon-canvas` existence;
- manual screen-reader review of HDR/SDR status;
- existing object-URL, HEIC memory-consent, navigation, slider, and cleanup tests pass;
- the full repository check passes.

Headless Playwright cannot close H1 or H2: virtual displays and screenshots do not
prove physical HDR luminance.

## 11. Consequences

### Positive

- existing Ultra HDR JPEGs can use their authored gain maps without transcoding;
- Firefox and SDR displays retain the intentional SDR base rather than failing to open;
- users know when comparisons mix HDR and SDR paths;
- current navigation/alignment contracts survive;
- no new codec dependency is required for the first HDR-capable path;
- later HEIC conversion has explicit fidelity and licensing gates.

### Negative

- current OpenSeadragon performance evidence must be repeated with its HTML drawer;
- browser capability cannot be determined from one clean feature probe;
- per-photo state and a maintained runtime policy are required;
- mixed HDR/SDR groups introduce a comparison-policy choice;
- current HEIC worker and Tauri derivatives must carry more alarming wording;
- cross-platform HEIC HDR remains unavailable until native decode or a conforming
  conversion path passes H3.

### Explicitly impossible with current components

- preserving a HEIC auxiliary gain map through `heic-to`'s 8-bit `ImageData` result;
- preserving PQ/HLG HEIC through `heif-oxide` 0.1.0's bounded sRGB conversion;
- reconstructing a discarded gain map from the generated PNG alone;
- producing HDR from OpenSeadragon 6.1.0's current `RGBA/UNSIGNED_BYTE` WebGL drawer;
- proving gain-map application from successful image decode or
  `(dynamic-range: high)`;
- making Firefox apply an unsupported JPEG gain map from application code without
  implementing an entirely separate decoder/renderer.

## 12. Follow-on implementation sequence

This is sequencing guidance, not implementation in this ADR.

1. Add bounded JPEG gain-map inspection and typed source/output status.
2. Add warnings first, so Firefox, SDR-display, and unverified-runtime fallbacks are
   honest before claiming HDR.
3. Configure OpenSeadragon's HTML drawer for all current simple-image sources; the
   pre-plan three-engine DOM/clip/lifecycle spike has passed.
4. Run H1 on real HDR hardware and compare against a direct `<img>` reference.
5. Add deliberate all-SDR comparison mode and mixed-state disclosure.
6. Defer HEIC HDR work. If a real requirement appears, acquire qualified fixtures and
   write a separate conversion/rendering ADR against H3.

## 13. External references

Research snapshot: 2026-08-27.

- Android, **Ultra HDR Image Format v1.1** — format lifecycle, backward-compatible SDR
  base, gain-map reconstruction, and ISO 21496-1 compatibility:
  https://developer.android.com/media/platform/hdr-image-format
- Google, **libultrahdr** — MIT/Apache-2.0 gain-map codec and current JPEG/HEIF/AVIF
  API surface: https://github.com/google/libultrahdr
- Apple, **Supporting HDR images in your app** — gain-map expansion, preferred image
  dynamic range, and 10-bit HEIC output:
  https://developer.apple.com/documentation/uikit/supporting-hdr-images-in-your-app
- Apple, **Applying Apple HDR effect to your photos** — Apple auxiliary gain-map
  structure, headroom, and application:
  https://developer.apple.com/documentation/appkit/applying-apple-hdr-effect-to-your-photos
- Apple ImageIO, `kCGImageAuxiliaryDataTypeHDRGainMap`:
  https://developer.apple.com/documentation/imageio/kcgimageauxiliarydatatypehdrgainmap
- W3C, **CSS Color HDR Module Level 1** (Working Draft) — HDR color spaces,
  compositing, and `dynamic-range-limit`: https://www.w3.org/TR/css-color-hdr-1/
- W3C, **Portable Network Graphics (PNG) Third Edition**, `cICP`, `mDCV`, and
  `cLLI` HDR signaling: https://www.w3.org/TR/png-3/#cICP-chunk
- MDN, **HTMLCanvasElement.getContext()** — 2D context `colorSpace`/`colorType`
  options: https://developer.mozilla.org/en-US/docs/Web/API/HTMLCanvasElement/getContext
- MDN, **CanvasRenderingContext2D.getImageData()** — P3 and float16 image-data
  semantics:
  https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/getImageData
- MDN, **`dynamic-range` media feature**:
  https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/At-rules/@media/dynamic-range
- MDN browser-compat data, `HTMLCanvasElement` — current color-space/color-type
  implementation matrix:
  https://github.com/mdn/browser-compat-data/blob/main/api/HTMLCanvasElement.json
- MDN browser-compat data, `dynamic-range-limit`:
  https://github.com/mdn/browser-compat-data/blob/main/css/properties/dynamic-range-limit.json
- MDN, **Image file type and format guide** — HEIC, AVIF, high bit depth, wide gamut,
  and HDR format capabilities:
  https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/Image_types
- Apple/WebKit, **Safari 17.0 features** — HEIC/JPEG XL web image support:
  https://webkit.org/blog/14445/webkit-features-in-safari-17-0/
- Chrome 145 release notes — current JPEG XL origin trial status:
  https://developer.chrome.com/release-notes/145
- OpenSeadragon 6.1.0 audited source, installed at
  `apps/web/node_modules/openseadragon/build/openseadragon/openseadragon.js`.
- `heic-to` 1.5.2 audited source, installed at
  `apps/web/node_modules/heic-to/src/worker.js`.
- `heif-oxide` 0.1.0 audited source, installed in the Cargo registry and pinned by
  `Cargo.lock`: https://docs.rs/crate/heif-oxide/0.1.0/source/src/lib.rs and
  https://docs.rs/crate/heif-oxide/0.1.0/source/src/color.rs
- Tauri 2.11.5 asset protocol audited source, installed in the Cargo registry and
  pinned by `Cargo.lock`:
  https://github.com/tauri-apps/tauri/blob/tauri-v2.11.5/crates/tauri/src/protocol/asset.rs

## 14. Review triggers

Revisit this ADR when any of the following occurs:

- Firefox ships JPEG gain-map HDR;
- OpenSeadragon adds a documented HDR drawer/output contract;
- Canvas HDR output becomes interoperable across Chromium, Safari, and Firefox;
- Tauri exposes a stable cross-platform HDR surface;
- `heif-oxide` gains correct ICC/PQ/HLG and HDR auxiliary support;
- the product requires pixel-domain HDR differences, warping, or tone-map controls;
- measured 50 MP HTML-drawer performance fails the accepted memory/interaction budget.
