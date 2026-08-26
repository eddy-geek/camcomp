# ADR-DROP3: Web-First Import and Large-Image Viewer Architecture

- **Status:** Accepted
- **Date:** 2026-08-24
- **Scope:** Drop 2 import/reload interface and Drop 3.1 comparison viewer
- **Sources:** `REQUIREMENTS.md`, `SPEC.md`, `ADR-DROP1.md`, `IMPLEM-DROP1.md`, and `docs/GUIDE.md`

## 1. Context

Drop 1 produced a native Rust analysis engine, a versioned `camcomp.json`, a CLI, and a minimal Tauri 2 integration spike. The spike proves direct in-process analysis, typed progress, cancellation, cache loading, and atomic writes, but its plain-JavaScript page is explicitly not the product UI.

The next delivery should be web-first and statically hostable while retaining Tauri where native analysis or filesystem access is required. The same frontend must support:

- Drop 2 folder selection, file selection, drag-and-drop, and previous-analysis reload;
- native analysis and `camcomp.json` persistence through Tauri;
- browser-only reload of an existing analysis without uploading photos or metadata;
- Drop 3.1 adaptive display of two to four photos;
- synchronized pan and zoom;
- a switchable comparison-slider mode when a group has exactly two photos;
- default EXIF display for focal length, resolution, and camera model;
- JPEG, PNG, and, where the selected browser/runtime can decode it, HEIC;
- source photos that may be approximately 50 megapixels each.

Drop 3.2 configurable tags, random order, blind mode, and Drop 4 scoring are outside this ADR's delivery scope. The architecture should not make them difficult to add.

## 2. Decisions already fixed

The following are product decisions, not options in this ADR:

1. **Web-first static application.** The frontend should run as an installable static PWA where browser capabilities permit.
2. **Tauri-only analysis for this delivery.** The actual workspace passed `mise run check-wasm` for `camcomp-core` on 2026-08-24, including its `heif-oxide` dependency, but this proves target compilation only—not a browser binding, runtime HEIC decode, or acceptable memory behavior. The browser does not run new grouping analyses in Drops 2/3.1. Tauri remains the adapter for native path access, analysis, progress, cancellation, and writing `camcomp.json`.
3. **Browser reload/viewing.** The standalone PWA may open an existing `camcomp.json`, resolve its photos from a user-granted folder or file selection, and display the result locally.
4. **Load without automatic rescan.** A valid existing `camcomp.json` is shown immediately and is not automatically reconciled against current source files. Refresh/re-analysis is an explicit native action.
5. **No graphical group editing in Drop 2.** The completed CLI/core override operations remain available, but add/remove/split/merge/re-anchor UI is deferred.
6. **One combined implementation plan.** `PLAN-DROP2.md` will cover Drop 2 and Drop 3.1 after this ADR's remaining decisions are made.
7. **No browser analysis export.** Because new analysis is native-only, the standalone PWA has no newly generated `camcomp.json` save flow in this delivery.
8. **Cross-platform native code, no packaging.** Tauri adapters and the shared frontend remain portable across macOS, Windows, and Linux, but installers, signing, notarization, and release bundles are not acceptance criteria for this delivery.
9. **Desktop-class browsers.** Safari, Chromium, and Firefox desktop are required; responsive layout is retained, but tablet and phone memory/touch acceptance is deferred.

## 3. Decision drivers

In priority order:

1. Keep photos and metadata local by default.
2. Make the main frontend deployable as static files with no application server.
3. Reuse one frontend in the browser and Tauri rather than maintaining separate products.
4. Keep folder acquisition usable for large nested photo sets.
5. Display JPEG, PNG, and HEIC with explicit capability boundaries.
6. Keep pan and zoom responsive with two to four approximately 50 MP sources.
7. Preserve the normalized-oriented coordinate contract and future use of Drop 1 alignments.
8. Minimize custom gesture, tiling, accessibility, and lifecycle code when a maintained library fits.
9. Keep dependencies permissively licensed and actively maintained.
10. Make browser/runtime limitations visible rather than silently degrading or uploading data.
11. Provide deterministic, automatable tests for import, mapping, layout, and viewer state.

## 4. Existing contracts that constrain the viewer

### 4.1 Analysis document

`camcomp.json` already contains:

- a source root and relative photo paths;
- original, oriented, and analysis dimensions;
- extracted metadata and configurations;
- groups and anchor photo IDs;
- a normalized-oriented photo-to-anchor homography and persisted inverse for each aligned non-anchor member;
- approximately 100–300 spatially distributed correspondences;
- alignment confidence and provenance;
- ungrouped photos and reasons.

The frontend should consume generated TypeScript types or a checked schema-derived equivalent. It must not maintain a hand-written partial interpretation of the document that can drift from the Rust schema. Where initial `SPEC.md` examples differ from completed Drop 1 behavior, the checked schema, Rust contracts, `IMPLEM-DROP1.md`, and `docs/GUIDE.md` are authoritative for this integration ADR; Drop 2/3.1 does not reopen algorithm defaults.

### 4.2 Coordinate system

All viewer navigation should use normalized oriented coordinates, not raw-file pixels or EXIF-unadjusted dimensions. Rendering adapters may convert normalized positions to library-specific image or viewport coordinates at their boundary.

### 4.3 Source immutability

Source photos are never modified, moved, or renamed. Tauri may write only `camcomp.json` in the source folder. Thumbnails, tile pyramids, and other display caches must live in application cache storage or browser OPFS, keyed by the photo content digest and renderer parameters.

## 5. Frontend framework options

### 5.1 Continue with plain JavaScript

**Advantages**

- no migration cost for the small Drop 1 spike;
- minimal runtime and build tooling;
- direct compatibility with framework-neutral viewer libraries.

**Disadvantages**

- the spike has no static types, component boundaries, state model, or frontend tests;
- environment adapters, import state, progress state, group navigation, two rendering modes, and later blind/scoring state would be managed manually;
- preserving event and object-URL lifecycles becomes increasingly error-prone.

This is acceptable for an integration spike, not the recommended product foundation.

### 5.2 Vanilla TypeScript with Vite

**Advantages**

- small output and few abstractions;
- compile-time checking of the analysis and adapter contracts;
- direct use of OpenSeadragon or Web Components;
- simple static output and Tauri integration.

**Disadvantages**

- still requires a home-grown component and reactive state convention;
- DOM and event teardown for multiple long-lived viewers remains manual;
- likely framework migration when Drop 3.2 and Drop 4 introduce more interaction state.

This is viable if minimizing framework use is more important than avoiding future UI migration.

### 5.3 React with TypeScript and Vite

**Advantages**

- largest component and testing ecosystem;
- established patterns for complex state and accessibility;
- many image comparison wrappers.

**Disadvantages**

- more runtime and conceptual overhead than this application currently needs;
- most useful viewer capability here is framework-neutral, so React's larger ecosystem does not directly solve deep zoom, tiling, or homography mapping;
- third-party React wrappers can lag the underlying viewer and obscure cleanup or imperative viewport APIs.

React is reasonable, but its principal advantage is not decisive for this use case.

### 5.4 Svelte with TypeScript and Vite

**Advantages**

- compact component and state code for import, progress, group navigation, and multiple coordinated viewer instances;
- small runtime and static output suitable for a PWA;
- straightforward integration with imperative framework-neutral libraries through actions/component lifecycle;
- enough structure for Drop 3.2 and Drop 4 without adopting an application server framework.

**Disadvantages**

- a framework and compiler are added where the current spike has none;
- the ecosystem is smaller than React's;
- viewer integrations should use the underlying library directly rather than depend on a thin Svelte wrapper with separate maintenance risk.

### 5.5 Frontend recommendation

Adopt **Svelte + TypeScript + Vite**, without SvelteKit. Use Vitest and Testing Library for unit/component tests and Playwright for browser flows. Keep viewer and platform boundaries as plain TypeScript interfaces so they can be tested without Svelte and replaced independently.

Dependency versions should be pinned from reviewed releases at least seven days old when implementation begins. The research snapshot found active, permissively licensed releases of Svelte, Vite, Vitest, OpenSeadragon, and `img-comparison-slider`; this is evidence of availability, not permission to use floating versions.

## 6. Runtime and file-acquisition decision

Browser capabilities were checked against current MDN browser-compat-data and Can I Use data on 2026-08-24.

| Capability | Chromium | Safari | Firefox | Consequence |
|---|---|---|---|---|
| `showDirectoryPicker()` | Chrome 86+ | unavailable through Safari 26 | unavailable | Chromium receives the richer retained-directory-handle flow |
| `<input webkitdirectory>` | Chrome 7+ | Safari 11.1+ | Firefox 50+ | all three can read a complete nested folder and retain `webkitRelativePath` for the page lifetime |
| user-visible writable file handles | available through picker handles | no picker path | no picker path | only Chromium has the direct user-folder path, which is not required while analysis is native-only |
| OPFS | Chrome 86+ | Safari 15.2+ | Firefox 111+ | all three can keep private derived display caches |
| HEIF/HEIC image decode | unavailable through the current compatibility table | Safari 17+ | unavailable | Safari uses native decode; Chromium and Firefox need a sidecar, JavaScript/WASM decode, or Tauri |
| service workers/static offline app | available | available | available | all three can run the static app; install UI remains browser-specific |

`FileSystemFileHandle.createWritable()` exists in Safari 26 for handles such as OPFS entries, but Safari does not expose `showDirectoryPicker()` or an equivalent user-folder picker. It therefore does not provide Chromium-equivalent in-place access to a user's selected photo directory.

### 6.1 Accepted capability-based browser contract

Use one `SourceProvider` interface with browser-specific capability adapters:

- `DirectoryHandleSourceProvider` for Chromium's best folder UX;
- `DirectoryInputSourceProvider` for Safari, Firefox, and fallback Chromium using `<input webkitdirectory>`;
- drag/drop adapters that preserve relative paths where the runtime exposes them and otherwise request the containing folder;
- `TauriSourceProvider` for native paths, analysis, and persistence.

The application feature-detects behavior rather than branching on browser names. Chromium may retain a directory handle and request permission again. Safari and Firefox reconstruct the selected tree from `webkitRelativePath` and require folder reselection after restart. The static application remains usable in all three; it does not promise identical filesystem or installation UX where browser capabilities differ.

### 6.2 HEIC source-resolution cascade

Resolve each document photo deterministically:

1. use the exact referenced HEIC/HEIF when the runtime passes an actual decode capability test, as expected on Safari;
2. when native HEIC decode is unavailable, look for an explicitly user-provided same-stem `.jpg`, `.jpeg`, then `.png` sidecar before invoking a decoder;
3. accept a sidecar only when its oriented dimensions match the `camcomp.json` record, label the substitution in the UI, and retain the original photo identity and EXIF from the analysis document;
4. otherwise decode the exact HEIC/HEIF in a worker through a separately audited JavaScript/WASM codec;
5. if decoding is unsupported, invalid, too large for the memory guard, or fails, preserve the rest of the group and direct that photo to the Tauri fallback.

A sidecar has different bytes and may have different compression or color rendering. It is therefore an explicit viewing substitution, never cache identity and never silent evidence that the source is unchanged.

The full-detail decoder must return a bitmap or lossless display source. Converting to JPEG at quality `0.8` would add artifacts to the material being compared and is not acceptable for full-detail viewing. A bounded lossy preview is permitted only when identified as a preview and replaced before detail comparison.

[`heic-to`](https://github.com/hoppergee/heic-to) is an active candidate with worker and `ImageBitmap` paths, but the researched package is approximately 24 MB unpacked and LGPL-3.0 because it embeds libheif. The existing MIT/Apache-2.0 `heif-oxide` decoder already handles the Drop 1 fixtures and compiles as part of `camcomp-core` for WASM. The implementation plan must compare a narrow `heif-oxide` WASM wrapper with `heic-to` for fixture coverage, color/orientation correctness, CSP compatibility, bundle size, runtime, peak memory, and license obligations before selecting either.

### 6.3 Import resolution behavior

- Opening a folder enumerates enough entries to find `camcomp.json` and resolve paths, but does not trigger a metadata/fingerprint rescan.
- Dropping/opening `camcomp.json` alone loads and validates it, then asks for the source folder or files required to resolve photo paths.
- Missing paths and cheap size/mtime mismatches are reported during resolution. Content hashing is reserved for an explicit verify/refresh action, so loading a cached analysis does not become an automatic rescan. Browser mode does not silently repair or overwrite the document.
- Selecting photos without an analysis is accepted in Tauri and starts native analysis. In the standalone PWA it explains that new analysis requires the desktop application.
- Browser object URLs are scoped to the active album/group and revoked when no longer used.

## 7. Large-image rendering options

A 50 MP image occupies roughly 200 MB as 8-bit RGBA after decode. Four full decoded images can therefore approach 800 MB before browser, canvas, texture, tile, and duplicate-buffer overhead. A viewer library cannot remove that cost if it receives each original as one simple image.

### 7.1 Native `<img>` elements with CSS transforms

**Advantages**

- smallest implementation and dependency surface;
- browser-native decoding, color handling, and compositing;
- easy clipping for a two-image slider.

**Disadvantages**

- each source is normally decoded as one full image;
- custom wheel, drag, touch, bounds, resize, synchronization, and teardown code;
- no level-of-detail loading or tile eviction;
- likely memory pressure for four 50 MP photos.

This is useful for a first functional spike and bounded previews, not a sufficient unmeasured production decision.

### 7.2 Canvas or WebGL renderer built by camcomp

**Advantages**

- complete control over clipping, interpolation, texture lifetimes, and future overlays;
- a common canvas could implement both slider and grid modes;
- WebGL could later warp or blend pixels if product requirements expand.

**Disadvantages**

- camcomp would own gesture handling, level selection, tile scheduling, cache eviction, context loss, accessibility, and browser-specific limits;
- source decode and pyramid generation are still required;
- this duplicates mature deep-zoom infrastructure before a unique rendering need is proven.

A custom renderer is the fallback if a library cannot satisfy the mapping or local-source requirements, not the preferred starting point.

### 7.3 OpenSeadragon with simple image sources

**Advantages**

- mature pan/zoom interactions, bounds handling, coordinate conversion, touch input, and viewport APIs;
- straightforward synchronization of two to four viewers;
- framework-neutral and BSD-3-Clause licensed;
- local Blob/object URLs can be presented as image sources;
- keeps the viewer API compatible with a later tiled source without paying the pyramid cost before measurements require it.

**Disadvantages**

- a simple source still decodes the entire 50 MP image;
- multiple viewer instances require an explicit shared-navigation controller;
- OpenSeadragon does not understand `camcomp.json` homographies by itself;
- decoded-memory pressure may still make groups of three or four impractical on some machines.

### 7.4 OpenSeadragon with tiled pyramids

**Advantages**

- visible-resolution tiles are loaded on demand and evicted;
- the viewer is designed for images substantially larger than 50 MP;
- the navigation and composition layers can stay unchanged when simple sources are replaced with pyramids.

**Disadvantages**

- local originals do not arrive as Deep Zoom pyramids;
- pyramid generation has a one-time full-decode cost and requires progress, cancellation, storage, and invalidation;
- Tauri and browser environments need different tile-cache adapters;
- browser-generated HEIC pyramids are practical only after HEIC decode succeeds.

### 7.5 Accepted rendering decision

Adopt an **OpenSeadragon-based viewer with simple image sources first**, behind `PhotoSourceProvider` and `ViewerAdapter` interfaces:

1. resolve an exact, sidecar, native-decoded, or worker-decoded local display source through `PhotoSourceProvider`;
2. open that source through OpenSeadragon's simple-image path;
3. keep only the active group resident and release viewer, bitmap, Blob URL, and decoder resources when leaving it;
4. measure Safari, Chromium, Firefox, and Tauri with real JPEG, PNG, and HEIC groups near 50 MP;
5. preserve source and viewer boundaries that permit a later tiled provider without changing Svelte components, navigation state, slider composition, or alignment mapping.

Tile-pyramid generation is a contingency, not part of the initial delivery. Trigger it only if the measured simple-source path exceeds the accepted memory budget, crashes, cannot keep interactions responsive, or cannot display the required group sizes. A pyramid may then be cached in Tauri application storage or browser OPFS, never beside source photos.

## 8. Two-image comparison slider

### 8.1 Knight Lab Juxtapose

Juxtapose provides the desired before/after interaction, but its published `juxtaposejs` package remains an older standalone slider, uses MPL-2.0, and does not provide synchronized deep zoom. The repository has recent maintenance activity, but the package does not solve camcomp's multi-view, local-tile, or alignment requirements.

**Decision:** use its interaction as design reference, not as the viewer foundation.

### 8.2 `img-comparison-slider`

This maintained MIT-licensed Web Component provides an accessible two-slot comparison control and is framework-neutral. It may be able to clip two OpenSeadragon containers while the camcomp navigation controller synchronizes their viewports.

**Risk:** slider components are generally tested with static images, not nested interactive viewers. Pointer capture, wheel delivery, touch gestures, focus, and resize behavior must be proven before adoption.

### 8.3 Small camcomp-owned clip control

Two overlaid viewer containers, one clipped by a range-controlled divider, are a limited custom component. It avoids an integration dependency and can share the same viewer instances and canonical navigation state as grid mode.

**Recommendation:** run a narrow integration spike with `img-comparison-slider`. Adopt it only if nested OpenSeadragon interaction and keyboard accessibility pass. Otherwise implement the small clipping/divider shell locally; do not fork Juxtapose. Check also https://github.com/locomo/openseadragon-curtain-sync

## 9. Improve-ImgSLI audit

[`Improve-ImgSLI`](https://github.com/Loganavter/Improve-ImgSLI) is functionally relevant, not merely another static before/after widget. Its current application includes:

- a two-image split comparison workspace;
- a synchronized N-image multi-compare workspace;
- GPU-accelerated pan/zoom and composition;
- tiled live rendering and a `TiledPixelStore`;
- drag-and-drop and file lists;
- image metadata, export, magnifiers, and diff modes;
- extensive pytest coverage around rendering, tiling, interaction, and projects.

It also documents the same fundamental codec constraint camcomp faces: the live renderer is tiled, but codecs still perform a full-frame decode once. Inputs above 65,536 pixels on one side are rejected.

### 9.1 Why a fork does not reduce camcomp implementation effort

- It is a Python/PySide6/Qt desktop application with QRhi shaders, not a JavaScript component or static web application.
- Its dependencies include PySide6, Pillow, NumPy, scikit-image, and imagecodecs; adopting it would reverse the accepted Tauri/Rust/web frontend architecture.
- Its workspace, rendering, input, and project models are not `camcomp.json` adapters.
- Integrating the Rust core would require a new sidecar/FFI boundary and a separate browser viewer would still be needed.
- The repository is GPL-3.0-or-later. Forking it or porting covered source into camcomp would require GPL-compliant distribution and is incompatible with retaining camcomp's current MIT/Apache-2.0 licensing for that combined code.

### 9.2 What should be reused

Use Improve-ImgSLI as independent evidence and UX/test inspiration for:

- split orientation and divider interactions;
- synchronized multi-image navigation;
- fit, 100%, zoom-toward-pointer, and pan gestures;
- focus/fullscreen behavior per grid cell;
- progressive preview and tiled-store lifecycle;
- memory guards and oversized-input errors;
- rendering, tiling, and interaction test cases.

Do not copy source. Any similar implementation should be written against camcomp's own requirements and web APIs.

## 10. Navigation and alignment options

### 10.1 Mode A — normalized synchronized navigation

All viewers use the same normalized-oriented center and visible span. This implements the initial Drop 3 requirement and does not use persisted alignment evidence.

**Advantages**

- deterministic and easy to explain;
- always available, including unaligned/manual group members;
- low implementation and test risk.

**Limitations**

- moderate camera translation, rotation, and perspective move the same scene feature away from the shared center;
- the viewer does not yet realize the principal reason Drop 1 persisted homographies.

### 10.2 Mode B — global-homography feature-centered navigation

The leading viewer's normalized point is mapped into anchor space, then through each target photo's persisted inverse homography:

```text
anchorPoint = photoToAnchor × sourcePoint
targetPoint = anchorToTarget × anchorPoint
```

Only viewport centers are mapped. The full-resolution images are not warped, so this does not require Canvas/WebGL rendering or a different tile format.

**Additional work over Mode A**

- nine-value projective point transforms and finite/denominator checks;
- a canonical anchor-space navigation state;
- mapped-point bounds and overlap handling;
- confidence/provenance-aware fallback;
- clear behavior when one member lacks an alignment;
- decisions about whether zoom synchronizes normalized visible span or adjusts by the homography's local scale.

This is a moderate state/mapping feature, not a rendering-system rewrite.

### 10.3 Mode C — locally refined feature-centered navigation

Use nearby persisted correspondences to refine the source-to-anchor and anchor-to-target point mapping, falling back to the global homography outside reliable regions. A Delaunay/piecewise-affine or similarly bounded interpolation can move viewport centers without warping image pixels.

**Additional work over Mode B**

- spatial index or triangulation of correspondences;
- quality, reprojection-error, density, and extrapolation gates;
- continuity near triangle/reliability boundaries;
- confidence visualization and fallback tests;
- difficult behavior around parallax and moving subjects.

This is materially harder than Mode B but still does not force a custom pixel renderer. Per-pixel local warping would be a separate, currently unrequested feature.

### 10.4 Architecture required even if only Mode A ships

Define a renderer-independent navigation model from the beginning:

```text
NavigationState
  mode: normalized | global-aligned | local-aligned
  leaderPhotoId
  sceneCenter in normalized anchor coordinates
  normalizedVisibleSpan

PointMapper
  sourceToScene(photoId, point) -> scene point or unavailable
  sceneToTarget(photoId, scene point) -> target point or unavailable
  confidence(photoId, scene point) -> mapping confidence

ViewerAdapter
  read interaction in normalized image coordinates
  apply target center and visible span
  report bounds and lifecycle events
```

The normalized mapper is identity-like, the global mapper applies persisted matrices, and a future local mapper consumes correspondences. OpenSeadragon-specific coordinates remain inside `ViewerAdapter`.

If Mode A instead stores one viewer's raw library viewport as global state, adding Mode B later will require substantial rework. The canonical scene model is therefore part of the initial architecture regardless of feature scope.

### 10.5 Alignment recommendation

Ship Mode A first as the required fallback, then include Mode B in the same combined plan if the user accepts the additional scope. Mode B is the best cost/value use of data already produced by Drop 1 and validates the persisted alignment contract before later drops depend on it. Keep it switchable because normalized navigation remains more predictable for low-confidence, parallax-heavy, or manually assembled groups.

Defer Mode C until real use shows that global homographies are insufficient. Preserve the mapper interface and fixtures now so this deferral does not dictate a renderer rewrite.

## 11. Proposed component boundaries

```text
Svelte application
├── album/import state
├── group navigation and metadata presentation
├── platform capability presentation
└── viewer composition (grid or two-photo slider)

Plain TypeScript services
├── AnalysisRepository
│   ├── BrowserAnalysisRepository (read/validate only)
│   └── TauriAnalysisRepository (load/analyze/refresh/save)
├── SourceProvider
│   ├── DirectoryHandleSourceProvider (Chromium)
│   ├── DirectoryInputSourceProvider (Safari/fallback)
│   └── TauriSourceProvider
├── PhotoSourceProvider
│   ├── NativeBrowserPhotoSourceProvider
│   ├── SidecarPhotoSourceProvider
│   ├── WorkerHeicPhotoSourceProvider
│   ├── TauriPhotoSourceProvider
│   └── TiledPhotoSourceProvider (contingency)
├── PointMapper
│   ├── NormalizedPointMapper
│   ├── GlobalHomographyPointMapper
│   └── LocalCorrespondencePointMapper (future)
└── ViewerAdapter
    └── OpenSeadragonViewerAdapter (provisional)
```

The adapters exchange camcomp-owned IDs, normalized coordinates, typed progress, and object/blob URLs. UI components do not branch directly on `window.__TAURI__`, browser names, native paths, or OpenSeadragon internals.

## 12. Security and privacy consequences

- The static application has no upload or telemetry path by default.
- The service worker caches only versioned application assets, never selected photos, EXIF, `camcomp.json`, Blob URLs, or generated tiles.
- OPFS caches are origin-private, digest-keyed, size-bounded, and explicitly clearable.
- Object URLs are revoked when a group/album closes.
- Tauri filesystem commands accept only paths acquired through the native import flow and return structured errors without exposing unrelated absolute paths.
- Content Security Policy permits only required local/static sources and does not add remote viewer/CDN scripts.
- Unsupported or malformed JSON is never overwritten automatically.

## 13. Verification strategy

### 13.1 Contract and unit tests

- generate/check TypeScript analysis types against `camcomp.schema.json`;
- validate schema-major rejection and tolerant minor/unknown fields;
- path normalization and traversal rejection for relative photo paths;
- normalized/global point mapping against existing Rust synthetic matrices;
- non-finite, near-zero projective denominator, out-of-bounds, and missing-alignment cases;
- canonical navigation state independent of leader photo;
- deterministic 2/3/4-photo layout selection.

### 13.2 Component and browser tests

- folder, files, and drag/drop state transitions;
- valid, malformed, incompatible, and missing `camcomp.json`;
- standalone JSON followed by source-folder resolution;
- cached analysis loads without invoking native analysis or refresh;
- progress/cancel/error behavior in Tauri;
- keyboard and screen-reader behavior for import, grid, zoom, group navigation, and slider divider;
- object URL and viewer instance cleanup;
- PWA offline app-shell startup without caching private inputs.

Playwright should cover Chromium, WebKit, and Firefox. File-picker calls require injected adapters for deterministic CI plus manual smoke tests against real browser pickers and Tauri dialogs.

Main coverage on Chromium with just sanity checks on the others.

### 13.3 Large-image acceptance gate

Use representative JPEG, PNG, and HEIC sources near 50 MP and measure:

- time to first bounded preview;
- time to first zoomable full-detail region;
- peak browser and Tauri memory for groups of two, three, and four;
- interaction frame consistency during pan/zoom;
- simple-source decode duplication, release behavior, and group-to-group memory recovery;
- color/orientation parity with the Drop 1 oriented dimensions;
- if the pyramid contingency is triggered, tile-cache size, eviction, cancellation, and invalidation after digest change;
- no retained decoded buffers or object URLs after changing groups.

No renderer is accepted based only on a small demo image.

## 14. Decision gates

### Gate A — frontend foundation

Accept Svelte/TypeScript/Vite when the same production build runs as static PWA assets and inside Tauri, with typed environment adapters and frontend tests.

### Gate B — browser acquisition

Accept each browser target only when a nested real folder can load `camcomp.json`, resolve all supported paths, report missing files, and reopen after the browser-specific permission/reselection flow.

### Gate C — large-image viewer

Accept OpenSeadragon with simple sources when the 50 MP matrix in §13.3 stays within a recorded memory budget, releases memory between groups, and remains responsive. If it fails, keep OpenSeadragon and evaluate a tiled `PhotoSourceProvider` first; a custom renderer is the second contingency behind the same adapter interfaces.

### Gate D — slider composition

Accept `img-comparison-slider` only if nested viewers preserve pointer, wheel, touch, keyboard, resize, and cleanup behavior. Otherwise use a camcomp-owned clipping shell.

### Gate E — aligned navigation

Compare mapped centers against Drop 1 synthetic controls and independent real-scene landmark probes labelled before implementation. Mode B must meet predefined mapping-error/improvement thresholds and fall back visibly and deterministically when alignment is absent, low-confidence, invalid, or outside reliable coverage.

## 15. Decision

1. Replace the Drop 1 page with **Svelte + TypeScript + Vite** and produce one static frontend for PWA and Tauri.
2. Keep analysis native through a typed `AnalysisRepository`; the PWA only loads existing analyses in this delivery.
3. Support Chromium, Safari, and Firefox through capability-based source providers and the best folder UX each exposes.
4. Use native Safari HEIC decode where available; otherwise prefer a validated same-stem sidecar, then an audited worker decoder, then Tauri fallback.
5. Compare `heif-oxide` WASM with `heic-to` before selecting the non-native HEIC decoder; do not use lossy conversion as the full-detail source.
6. Use **OpenSeadragon with simple image sources first** and preserve a tiled-source contingency if measured 50 MP behavior fails.
7. Test `img-comparison-slider` as the two-photo composition shell and keep a small custom clipping shell as the fallback.
8. Treat Improve-ImgSLI and Juxtapose as UX/test references, not dependencies or fork bases.
9. Build navigation around canonical normalized anchor-space state and a pluggable `PointMapper`.
10. Ship normalized synchronized navigation first and switchable global-homography feature-centered navigation second; defer local correspondence refinement.
11. Keep all derived image data outside source folders and prevent the PWA service worker from caching private inputs.

## 16. Resolved decision record

- **D1 — browser contract:** capability-based Chromium, Safari, and Firefox support, combining native HEIC where available with sidecar and audited worker-decoder fallbacks.
- **D2 — alignment scope:** normalized synchronized navigation plus switchable global-homography feature-centered navigation; local correspondence refinement is deferred.
- **D3 — frontend:** Svelte + TypeScript + Vite accepted.
- **D4 — large-image source:** OpenSeadragon simple-image sources first; tile pyramids are triggered only by measured failures at the large-image gate.

## 17. References

- [MDN: File System Access API](https://developer.mozilla.org/en-US/docs/Web/API/File_System_Access_API)
- [MDN: `showDirectoryPicker`](https://developer.mozilla.org/en-US/docs/Web/API/Window/showDirectoryPicker)
- [MDN: `webkitdirectory`](https://developer.mozilla.org/en-US/docs/Web/API/HTMLInputElement/webkitdirectory)
- [MDN: Origin private file system](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system)
- [Can I Use: HEIF/HEIC](https://caniuse.com/heif)
- [OpenSeadragon](https://openseadragon.github.io/)
- [OpenSeadragon repository](https://github.com/openseadragon/openseadragon)
- [`img-comparison-slider`](https://github.com/sneas/img-comparison-slider)
- [`heic-to`](https://github.com/hoppergee/heic-to)
- [`heif-oxide`](https://github.com/dan335/heif-oxide)
- [Knight Lab Juxtapose](https://github.com/NUKnightLab/juxtapose)
- [Improve-ImgSLI](https://github.com/Loganavter/Improve-ImgSLI)
