# ADR-DROP3: Web-First Import and Large-Image Viewer Architecture

- **Status:** Proposed — two decisions remain in §16
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
2. **Tauri-only analysis for this delivery.** The Rust core compiles for `wasm32-unknown-unknown`, but the browser does not run new grouping analyses in Drops 2/3.1. Tauri remains the adapter for native path access, analysis, progress, cancellation, and writing `camcomp.json`.
3. **Browser reload/viewing.** The standalone PWA may open an existing `camcomp.json`, resolve its photos from a user-granted folder or file selection, and display the result locally.
4. **Load without automatic rescan.** A valid existing `camcomp.json` is shown immediately and is not automatically reconciled against current source files. Refresh/re-analysis is an explicit native action.
5. **No graphical group editing in Drop 2.** The completed CLI/core override operations remain available, but add/remove/split/merge/re-anchor UI is deferred.
6. **One combined implementation plan.** `PLAN-DROP2.md` will cover Drop 2 and Drop 3.1 after this ADR's remaining decisions are made.
7. **No browser analysis export.** Because new analysis is native-only, the standalone PWA has no newly generated `camcomp.json` save flow in this delivery.

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

The frontend should consume generated TypeScript types or a checked schema-derived equivalent. It must not maintain a hand-written partial interpretation of the document that can drift from the Rust schema.

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

## 6. Runtime and file-acquisition options

Browser capabilities were checked against current MDN browser-compat-data and Can I Use data on 2026-08-24.

| Capability | Chromium | Safari | Consequence |
|---|---|---|---|
| `showDirectoryPicker()` | Chrome 86+ | unavailable through Safari 26 | Chromium can retain a directory handle; Safari needs a file-input fallback |
| `<input webkitdirectory>` | Chrome 7+ | Safari 11.1+ | both can read a complete nested folder and retain `webkitRelativePath` for the page lifetime |
| user-visible writable file handles | available through picker handles | no picker path | Safari cannot update a selected source folder in place |
| OPFS | Chrome 86+ | Safari 15.2+ | both can keep private derived display caches |
| HEIF/HEIC image decode | unavailable in Chromium through the current compatibility table | Safari 17+ | Safari can be the browser HEIC path without adding a decoder |
| service workers/PWA | available | available | both can run a static installable app shell |

`FileSystemFileHandle.createWritable()` exists in Safari 26 for handles such as OPFS entries, but Safari does not expose `showDirectoryPicker()` or an equivalent user-folder picker. It therefore does not provide Chromium-equivalent in-place access to a user's selected photo directory.

### 6.1 Chromium-only PWA

**Advantages**

- best folder workflow through `showDirectoryPicker()`;
- handles can be stored in IndexedDB and permission can be requested again;
- `camcomp.json` can be found directly without flattening a selection;
- strongest path to later in-place browser writes if browser analysis is revisited.

**Disadvantages**

- no native HEIC image display;
- a browser HEIC decoder or a Tauri fallback is required for a supported Drop 1 format;
- committing to Chromium for folder access does not solve the 50 MP rendering problem.

### 6.2 Safari-only PWA

**Advantages**

- native HEIC/HEIF display in Safari 17+;
- recursive folder import works through `<input webkitdirectory>`;
- static PWA, Blob URLs, Canvas, workers, and OPFS are available;
- no browser HEIC decoder dependency is needed.

**Disadvantages**

- no `showDirectoryPicker()` and no retained user-directory handle;
- users must select the folder again after page/application restart;
- the file list is flattened and its tree must be reconstructed from `webkitRelativePath`;
- no direct user-folder write path, although that is not required while analysis is Tauri-only.

Safari-only is viable for the decided read-only browser role, but its folder UX is less persistent.

### 6.3 Capability-based Safari and Chromium PWA

Use one `SourceProvider` interface with two browser implementations:

- `DirectoryHandleSourceProvider` for Chromium;
- `DirectoryInputSourceProvider` for Safari and other browsers with `webkitdirectory`;
- `TauriSourceProvider` for native paths and analysis.

The application feature-detects behavior rather than user-agent strings. Safari provides the no-extra-decoder HEIC path. Chromium provides the best directory-handle path and displays JPEG/PNG; an HEIC group there can direct the user to Safari or Tauri until a separately gated decoder is added.

**Recommendation:** support both Safari and Chromium through capability adapters. This avoids choosing between good folder access and native HEIC, while keeping all browser behavior read-only. If only one browser is accepted initially, Safari is defensible because HEIC is already a Drop 1 format and direct browser writes are not needed.

### 6.4 Import resolution behavior

- Opening a folder enumerates enough entries to find `camcomp.json` and resolve paths, but does not trigger a metadata/fingerprint rescan.
- Dropping/opening `camcomp.json` alone loads and validates it, then asks for the source folder or files required to resolve photo paths.
- Missing, renamed, or digest-mismatched source files are reported; browser mode does not silently repair or overwrite the document.
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
- local Blob/object URLs can be presented as image sources.

**Disadvantages**

- a simple source still decodes the entire 50 MP image;
- multiple viewer instances require an explicit shared-navigation controller;
- OpenSeadragon does not understand `camcomp.json` homographies by itself.

This is useful to validate interaction and mapping, but it does not by itself satisfy the large-image memory constraint.

### 7.4 OpenSeadragon with tiled pyramids

**Advantages**

- visible-resolution tiles are loaded on demand and evicted;
- the viewer is designed for images substantially larger than 50 MP;
- one renderer can support grid, synchronized navigation, overlays, and a clipped two-viewer slider;
- application code can focus on camcomp groups and coordinate mapping.

**Disadvantages**

- local originals do not arrive as Deep Zoom pyramids;
- pyramid generation has a one-time full-decode cost and requires progress, cancellation, storage, and invalidation;
- Tauri and browser environments need different tile-cache adapters;
- browser-generated HEIC pyramids are practical only where the browser can decode HEIC.

### 7.5 Rendering recommendation

Adopt an **OpenSeadragon-based viewer behind `PhotoPyramidProvider` and `ViewerAdapter` interfaces**, conditional on an early 50 MP gate:

1. render bounded previews first for fast group display;
2. generate/load a digest-keyed tile pyramid on demand when the user zooms;
3. keep Tauri pyramids in the application cache, never beside source photos;
4. keep browser pyramids in OPFS and process one source at a time;
5. prove Safari JPEG/PNG/HEIC and Chromium JPEG/PNG behavior on real 50 MP fixtures;
6. reject the library if measured memory, local tile delivery, or multi-view synchronization is unacceptable.

The gate must compare this path with a native `<img>`/CSS baseline. OpenSeadragon is selected for the work it avoids only when used with actual tiled sources.

## 8. Two-image comparison slider

### 8.1 Knight Lab Juxtapose

Juxtapose provides the desired before/after interaction, but its published `juxtaposejs` package remains an older standalone slider, uses MPL-2.0, and does not provide synchronized deep zoom. The repository has recent maintenance activity, but the package does not solve camcomp's multi-view, local-tile, or alignment requirements.

**Decision:** use its interaction as design reference, not as the viewer foundation.

### 8.2 `img-comparison-slider`

This maintained MIT-licensed Web Component provides an accessible two-slot comparison control and is framework-neutral. It may be able to clip two OpenSeadragon containers while the camcomp navigation controller synchronizes their viewports.

**Risk:** slider components are generally tested with static images, not nested interactive viewers. Pointer capture, wheel delivery, touch gestures, focus, and resize behavior must be proven before adoption.

### 8.3 Small camcomp-owned clip control

Two overlaid viewer containers, one clipped by a range-controlled divider, are a limited custom component. It avoids an integration dependency and can share the same viewer instances and canonical navigation state as grid mode.

**Recommendation:** run a narrow integration spike with `img-comparison-slider`. Adopt it only if nested OpenSeadragon interaction and keyboard accessibility pass. Otherwise implement the small clipping/divider shell locally; do not fork Juxtapose.

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
├── PhotoPyramidProvider
│   ├── BrowserOpfsPyramidProvider
│   └── TauriCachePyramidProvider
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

Playwright should cover Chromium and WebKit. File-picker calls require injected adapters for deterministic CI plus manual smoke tests against real browser pickers and Tauri dialogs.

### 13.3 Large-image acceptance gate

Use representative JPEG, PNG, and HEIC sources near 50 MP and measure:

- time to first bounded preview;
- time to first zoomable full-detail region;
- peak browser and Tauri memory for groups of two, three, and four;
- interaction frame consistency during pan/zoom;
- tile-cache size, eviction, cancellation, and invalidation after digest change;
- color/orientation parity with the Drop 1 oriented dimensions;
- no retained decoded buffers or object URLs after changing groups.

No renderer is accepted based only on a small demo image.

## 14. Decision gates

### Gate A — frontend foundation

Accept Svelte/TypeScript/Vite when the same production build runs as static PWA assets and inside Tauri, with typed environment adapters and frontend tests.

### Gate B — browser acquisition

Accept each browser target only when a nested real folder can load `camcomp.json`, resolve all supported paths, report missing files, and reopen after the browser-specific permission/reselection flow.

### Gate C — large-image viewer

Accept OpenSeadragon with tiled providers only when the 50 MP matrix in §13.3 stays within a recorded memory budget and remains responsive. Otherwise evaluate the custom renderer contingency behind the same provider/adapter interfaces.

### Gate D — slider composition

Accept `img-comparison-slider` only if nested viewers preserve pointer, wheel, touch, keyboard, resize, and cleanup behavior. Otherwise use a camcomp-owned clipping shell.

### Gate E — aligned navigation

If Mode B is included, compare mapped centers against Drop 1 synthetic control points and real groups. It must fall back visibly and deterministically when alignment is absent, low-confidence, invalid, or outside reliable coverage.

## 15. Proposed decision

Subject to the choices in §16:

1. Replace the Drop 1 page with **Svelte + TypeScript + Vite** and produce one static frontend for PWA and Tauri.
2. Keep analysis native through a typed `AnalysisRepository`; the PWA only loads existing analyses in this delivery.
3. Use capability-based source providers rather than browser-specific application branches.
4. Support Safari's recursive directory input/native HEIC path and Chromium's directory-handle/JPEG/PNG path unless a narrower initial browser target is chosen.
5. Provisionally use **OpenSeadragon with real tile pyramids**, not simple-image sources, after the 50 MP gate.
6. Test `img-comparison-slider` as the two-photo composition shell and keep a small custom clipping shell as the fallback.
7. Treat Improve-ImgSLI and Juxtapose as UX/test references, not dependencies or fork bases.
8. Build navigation around canonical normalized anchor-space state and a pluggable `PointMapper`.
9. Always provide normalized synchronized navigation; optionally add global-homography feature-centered navigation in the same plan; defer local correspondence refinement.
10. Keep all derived image data outside source folders and prevent the PWA service worker from caching private inputs.

## 16. Decisions required before `PLAN-DROP2.md`

### D1 — initial standalone browser contract

Choose one:

- **D1-A (recommended): Safari + Chromium adapters.** Safari supports HEIC with folder reselection; Chromium supports persistent directory handles for JPEG/PNG; Tauri handles analysis and all formats.
- **D1-B: Safari-first.** Require Safari 17+ for the standalone viewer, use `webkitdirectory`, and accept folder reselection. Add Chromium later.
- **D1-C: Chromium-first.** Require Chromium for folder handles, support JPEG/PNG in-browser, and direct HEIC users to Tauri. Add Safari later.
- **D1-D: Chromium plus browser HEIC decoder.** Add a separately audited/benchmarked HEIC WASM decoder and retain Chromium-only browser UX. This adds codec, licensing, memory, and bundle risk.

### D2 — Drop 3.1 alignment scope

Choose one:

- **D2-A: normalized only.** Ship Mode A, but retain canonical scene state and mapper interfaces for later modes.
- **D2-B (recommended): normalized plus global alignment.** Ship Mode A as fallback and Mode B as a switchable second stage in the combined plan.
- **D2-C: all point-mapping modes.** Also implement local correspondence refinement in this plan; this expands Drop 3.1 beyond its stated first-version requirement.

Once D1 and D2 are chosen, this ADR can move from Proposed to Accepted and the combined Drop 2/Drop 3.1 implementation plan can be written without unresolved architecture branches.

## 17. References

- [MDN: File System Access API](https://developer.mozilla.org/en-US/docs/Web/API/File_System_Access_API)
- [MDN: `showDirectoryPicker`](https://developer.mozilla.org/en-US/docs/Web/API/Window/showDirectoryPicker)
- [MDN: `webkitdirectory`](https://developer.mozilla.org/en-US/docs/Web/API/HTMLInputElement/webkitdirectory)
- [MDN: Origin private file system](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system)
- [Can I Use: HEIF/HEIC](https://caniuse.com/heif)
- [OpenSeadragon](https://openseadragon.github.io/)
- [OpenSeadragon repository](https://github.com/openseadragon/openseadragon)
- [`img-comparison-slider`](https://github.com/sneas/img-comparison-slider)
- [Knight Lab Juxtapose](https://github.com/NUKnightLab/juxtapose)
- [Improve-ImgSLI](https://github.com/Loganavter/Improve-ImgSLI)
