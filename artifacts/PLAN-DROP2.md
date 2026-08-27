# Drops 2 and 3.1 Implementation Plan

- **Status:** Proposed
- **Date:** 2026-08-24
- **Scope:** Drop 2 import/reload interface and Drop 3.1 comparison viewer
- **Sources:** `REQUIREMENTS.md`, `SPEC.md`, `ADR-DROP1.md`, `IMPLEM-DROP1.md`, `ADR-DROP3.md`, and `docs/GUIDE.md`
- **Frontend:** Svelte, TypeScript, and Vite
- **Viewer:** OpenSeadragon with simple image sources; tile pyramids only if the large-image gate fails
- **Analysis:** Existing native `camcomp-core` through Tauri; no browser analysis in this delivery
- **Browser targets:** Desktop Safari, Chromium, and Firefox through capability adapters
- **Native target:** Cross-platform source and checks; no installer, signing, notarization, or packaging work

## 1. Objective

Deliver one tested static frontend that runs as a desktop-browser PWA and inside Tauri and that:

- opens a local folder;
- selects or accepts dragged photo files;
- detects and reloads an existing `camcomp.json`;
- accepts a dropped or selected `camcomp.json` from a previous run;
- shows a valid cached analysis immediately without automatically rescanning source photos;
- runs new analysis, progress, cancellation, explicit refresh, and JSON persistence through Tauri;
- explains that new analysis requires Tauri when the standalone browser receives photos without an analysis;
- presents groups and ungrouped photos without dropping inputs;
- displays a selected group of photos in an adaptive layout;
- supports synchronized pan and zoom in normalized-coordinate mode;
- supports a switchable global-homography-aligned navigation mode with deterministic fallback;
- offers a before/after comparison slider when a group contains exactly two photos;
- shows focal length, resolution, and camera model by default;
- uses the best available folder and HEIC behavior in Safari, Chromium, Firefox, and Tauri;
- remains local and never uploads photos or metadata.

The implementation shall consume the completed Drop 1 core and schema rather than reproduce analysis behavior in TypeScript. Where the initial `SPEC.md` prose or illustrative JSON differs from the completed implementation, the generated schema, Rust public contracts, `IMPLEM-DROP1.md`, and observed `docs/GUIDE.md` behavior are authoritative for integration. This plan does not reopen or silently translate Drop 1 algorithm defaults. Do make any modification needed to ease integration.

## 2. Scope boundaries

### 2.1 Included

- replacement of the Drop 1 plain-JavaScript spike UI;
- a static installable PWA app shell;
- one shared Svelte frontend for browser and Tauri;
- generated or schema-checked TypeScript contracts for `camcomp.json`;
- browser read-only analysis reload and source resolution;
- native Tauri analysis and persistence;
- folder, file, and drag/drop acquisition;
- explicit cache refresh/re-analysis in Tauri;
- browser-specific capability and fallback messages;
- JPEG and PNG display;
- native Safari HEIC display where supported;
- same-stem JPEG/PNG sidecar resolution for HEIC sources;
- a worker-based HEIC decoder decision spike and accepted fallback implementation;
- OpenSeadragon simple-image viewing;
- adaptive layouts for groups of two, three, and four, with a non-lossy generic layout for larger groups;
- normalized and global-homography navigation modes;
- a two-photo comparison slider;
- fixed default EXIF presentation;
- desktop-browser accessibility and keyboard interaction;
- cross-platform native checks without release packaging.

### 2.2 Explicitly deferred

- browser/WASM grouping analysis;
- server-side analysis, uploads, accounts, or telemetry;
- graphical add/remove/split/merge/re-anchor group editing;
- Drop 3.2 configurable tags, randomized ordering, and blind mode;
- Drop 4 preference scoring and persistence;
- local correspondence-refined navigation;
- per-pixel image warping or overlays;
- tile-pyramid generation unless the simple-source gate fails;
- RAW formats;
- mobile/tablet browser acceptance;
- installers, signing, notarization, app-store delivery, and auto-update;
- calibration or behavioral changes to the completed Drop 1 grouping algorithm.

## 3. Delivery strategy

Work is divided into four tracks with explicit integration gates.

### 3.1 Shared frontend track

Build the static Svelte application, schema contract, state model, import UX, group browser, viewer composition, metadata display, accessibility, and PWA behavior. UI components depend on camcomp-owned TypeScript service interfaces, not browser globals, Tauri globals, or OpenSeadragon directly.

### 3.2 Platform adapter track

Implement source and analysis adapters for:

- Chromium directory handles;
- Safari/Firefox recursive directory inputs;
- browser file selection and drag/drop;
- Tauri native dialogs, dropped paths, analysis, progress, cancellation, refresh, and persistence.

The adapters expose equivalent application concepts while preserving visible capability differences.

### 3.3 Image and viewer track

Establish deterministic photo resolution, native/sidecar/worker HEIC behavior, OpenSeadragon simple-image sources, resource lifetime management, normalized navigation, global mapping, and slider composition. Large-image measurements decide whether the tile-pyramid contingency is activated.

### 3.4 Verification track

Add contract, unit, component, browser, native-command, integration, accessibility, privacy, and performance tests with each phase. No phase is complete on a small demo image when its behavior concerns 50 MP sources.

## 4. Proposed repository shape

Migrate the current generated-looking `apps/desktop/dist` spike into a source-owned web application:

```text
camcomp/
├── apps/
│   ├── web/
│   │   ├── package.json
│   │   ├── package-lock.json
│   │   ├── tsconfig.json
│   │   ├── vite.config.ts
│   │   ├── static/
│   │   │   ├── manifest.webmanifest
│   │   │   └── icons/
│   │   ├── src/
│   │   │   ├── app/
│   │   │   ├── components/
│   │   │   ├── contracts/
│   │   │   ├── import/
│   │   │   ├── platform/
│   │   │   ├── sources/
│   │   │   ├── viewer/
│   │   │   └── workers/
│   │   ├── tests/
│   │   └── e2e/
│   └── desktop/
│       └── src-tauri/
├── crates/
│   ├── camcomp-core/
│   └── camcomp-cli/
├── schema/
│   └── camcomp.schema.json
├── artifacts/
│   ├── ADR-DROP3.md
│   └── PLAN-DROP2.md
└── mise.toml
```

`apps/web/dist` is build output and shall not be hand-edited. Tauri points to that output and uses the same development server during local development. The exact module subdivision may be adjusted during scaffolding, but platform, source, navigation, and renderer boundaries must remain independently testable.

## 5. Quality and verification policy

Every implementation phase follows this order:

1. define expected behavior, errors, and cleanup;
2. add a failing unit, component, integration, or browser test where practical;
3. implement the smallest complete behavior;
4. run focused frontend and/or Rust tests;
5. run the combined repository checks;
6. record measured capability or performance evidence for decision gates.

### 5.1 Frontend quality baseline

Use pinned, reviewed dependency versions at least seven days old when added. Establish tasks equivalent to:

```text
mise run web:fmt
mise run web:lint
mise run web:typecheck
mise run web:test
mise run web:e2e
mise run web:build
mise run tauri:check
```

Integrate them into `mise run check` without removing the existing Rust formatting, Clippy, test, release-build, fixture, or schema-freshness checks.

The frontend baseline includes:

- TypeScript strict mode;
- Svelte diagnostics/type checking;
- ESLint with warnings denied;
- deterministic formatting;
- Vitest and Testing Library;
- Playwright projects for Chromium, WebKit, and Firefox;
- production static build verification;
- dependency license and advisory review.

### 5.2 Test privacy

Commit synthetic, generated, or redistributable frontend fixtures only. Keep private real photos out of git. A local manifest may point at representative private 50 MP JPEG, PNG, and HEIC sets for memory and visual smoke tests.

### 5.3 No hidden browser behavior

Browser capability tests use feature detection and injected adapters. User-agent strings do not select filesystem or decoder behavior. Test doubles cover deterministic CI, but every real picker/permission/drag path also receives a manual browser smoke test before acceptance.

## 6. Phase 0 — Acceptance contract and fixtures

### 6.1 Tasks

1. Convert the requirements and accepted ADR into a traceability matrix covering every Drop 2 and Drop 3.1 behavior.
2. Define the product state machine:
   - idle;
   - acquiring source;
   - validating analysis;
   - resolving photos;
   - ready;
   - analyzing;
   - cancelling;
   - cancelled;
   - error.
3. Define fixtures for:
   - valid empty/minimal analysis documents;
   - two-, three-, four-, and more-than-four-photo groups;
   - portrait, landscape, and mixed-aspect groups;
   - grouped and ungrouped photos;
   - direct, composed, missing, low-confidence, and invalid alignment evidence;
   - malformed JSON, missing schema version, unsupported schema major, and unknown minor fields;
   - missing source paths and size/mtime mismatches;
   - exact HEIC, same-stem sidecar, and decoder-fallback resolution;
   - same-stem sidecar with wrong dimensions;
   - Unicode, mixed-case, and nested relative paths.
4. Prepare real local viewer sets near 50 MP for:
   - two JPEGs;
   - three or four JPEG/PNG images;
   - Safari-native HEIC;
   - non-native-browser HEIC decoding;
   - mixed HEIC and JPEG groups.
5. Label independent real-scene landmark probes for aligned-navigation acceptance. Keep them separate from correspondences used to estimate the homographies and record normalized expected positions for each group member.
6. Before measuring, define and record on named reference hardware:
   - acceptable time to first group preview;
   - acceptable time to first interactive image;
   - peak-memory ceiling for groups of two, three, and four;
   - interaction responsiveness criterion;
   - required recovery after leaving a group;
   - maximum aligned-navigation landmark error and required improvement over normalized mapping.
7. Define the supported desktop browser versions at implementation time from current stable Safari, Chromium, and Firefox releases.
8. Define manual smoke-test checklists for browser pickers, drag/drop, permissions, native Tauri dialogs, and cross-platform path behavior.

### 6.2 Tests and evidence

- Fixture documents validate against the checked schema.
- Every photo path referenced by a viewer fixture has a deterministic source-resolution result.
- Synthetic homographies have expected mapped control points.
- Independent real-scene landmark probes are labelled before global-alignment implementation.
- Private fixture manifests and generated caches are excluded by `.gitignore`.
- Performance and mapping thresholds are written before renderer/alignment decisions are selected by measurement.

### 6.3 Exit criteria

- No Drop 2 or Drop 3.1 requirement lacks a planned acceptance check.
- Representative group sizes, source formats, error states, and alignment states have fixtures.
- The large-image gate has explicit measurements rather than “looks responsive” as its criterion.

## 7. Phase 1 — Shared frontend foundation

### 7.1 Tasks

1. Create `apps/web` with Svelte, TypeScript, and Vite; do not introduce SvelteKit or server rendering.
2. Add a lockfile and pin reviewed dependencies without floating ranges.
3. Configure static production output for both a web origin and Tauri's frontend distribution path.
4. Add strict TypeScript, Svelte checking, ESLint, formatting, Vitest, Testing Library, and Playwright.
5. Add a minimal PWA manifest, icons, and app-shell service worker.
6. Ensure the service worker precaches only versioned static application assets.
7. Define an injected runtime capability object rather than reading `window.__TAURI__` throughout components.
8. Establish top-level error boundaries and typed user-visible error presentation.
9. Preserve keyboard focus and live-region behavior from the spike while replacing its DOM implementation.
10. Point Tauri development and production configuration at the new frontend without changing core analysis behavior.
11. Define CSP targets early: no remote scripts and no broad `script-src 'unsafe-eval'`; allow only the style directives emitted by the production Svelte build and a narrowly tested `wasm-unsafe-eval` directive if the selected decoder requires WebAssembly compilation.
12. Add frontend and native checks to `mise.toml` and CI-equivalent tasks.

### 7.2 Tests

- Static build loads at a non-root base path where supported by configuration.
- The same compiled application boots under browser and mocked Tauri capabilities.
- Runtime selection is feature/capability based and independently injectable.
- The service worker cache manifest contains no selected-photo, JSON, object-URL, or OPFS entry.
- Keyboard focus is visible and initial status messages are announced.
- Tauri command tests from Drop 1 still pass.

### 7.3 Exit criteria

- `apps/desktop/dist` is no longer hand-authored source.
- One production frontend build serves browser and Tauri.
- Frontend format, lint, typecheck, unit-test, E2E, and build tasks run from `mise`.
- Existing Rust verification remains green.

## 8. Phase 2 — `camcomp.json` browser contract

### 8.1 Tasks

1. Generate TypeScript types from `schema/camcomp.schema.json`, or adopt another reproducible schema-derived contract with a freshness check.
2. Add runtime JSON Schema validation compatible with the schema draft emitted by `schemars`.
3. Check the schema major before full interpretation and return the same stable conceptual errors as the Rust loader:
   - malformed document;
   - missing/invalid schema version;
   - unsupported schema major;
   - semantic validation failure.
4. Ignore compatible unknown fields and future minor versions, matching Drop 1 behavior.
5. Normalize analysis data into read-only album state without mutating the parsed document.
6. Index photos, groups, configurations, alignments, and ungrouped entries by ID for UI access.
7. Validate relative source paths against traversal, absolute-path injection, separator differences, and duplicate normalized keys.
8. Keep absolute native paths inside platform adapters; do not put them in generic UI state or browser logs.
9. Add a schema/type freshness task to the combined check.

### 8.2 Tests

- Golden `camcomp.json` round-trip through the browser reader without semantic mutation.
- Unknown fields and future minor versions load.
- Unsupported majors and malformed JSON fail clearly.
- Missing references, duplicate IDs, invalid group membership, non-finite transforms, and invalid inverses are rejected by the schema/semantic layer.
- Windows separators, POSIX separators, Unicode normalization, and traversal attempts resolve deterministically or fail.
- Generated TypeScript contracts are current with the checked Rust schema.

### 8.3 Exit criteria

- Browser and Rust readers agree on the accepted/rejected fixture corpus.
- No application component casts unvalidated JSON to an analysis type.
- Schema changes fail CI until browser contracts are regenerated and reviewed.

## 9. Phase 3 — Source acquisition and resolution adapters

### 9.1 Shared contracts

Define camcomp-owned TypeScript interfaces equivalent to:

```text
AnalysisRepository
  loadAnalysis(selection)
  analyze(selection, options)          # native capability only
  refreshAnalysis(album, options)      # native capability only
  cancelAnalysis()                     # native capability only

SourceProvider
  chooseFolder()
  chooseFiles()
  acceptDrop(payload)
  resolve(relativePath)
  capabilities()

ResolvedPhotoSource
  photoId
  sourceKind: exact | sidecar | decoded | native
  displayHandle
  originalPath
  displayPath
  warnings
```

The actual TypeScript signatures should model cancellation and resource cleanup explicitly.

### 9.2 Chromium adapter tasks

1. Use `showDirectoryPicker()` when available and initiated by a user gesture.
2. Find root `camcomp.json` without recursively reading image bytes.
3. Traverse entries lazily and index relative paths needed by the document.
4. Store directory handles in IndexedDB where structured cloning is supported.
5. Query/request permission when reopening and show a reauthorization action rather than failing silently.
6. Fall back to recursive file input when handle APIs are denied or unavailable.
7. Use `getAsFileSystemHandle()` for dropped directories when available, with a file/drop fallback.

### 9.3 Safari and Firefox adapter tasks

1. Use `<input type="file" webkitdirectory multiple>` for recursive folder acquisition.
2. Reconstruct relative paths from `webkitRelativePath` without pretending a persistent folder handle exists.
3. Detect and strip the one selected-root prefix that `webkitRelativePath` prepends, so `folder/sub/photo.jpg` resolves the document path `sub/photo.jpg` without weakening traversal checks.
4. Reject selections that do not have one unambiguous selected-root prefix when a folder-relative analysis is required.
5. Require folder reselection after page/application restart.
6. Accept selected/dropped JSON and files through standard `File`/`DataTransfer` APIs.
7. Preserve a clear difference between “folder can be reselected” and “folder permission is retained.”

### 9.4 Tauri adapter tasks

1. Replace direct frontend Tauri calls with an injected adapter over the existing commands.
2. Keep native folder and multi-file dialogs and native drag/drop paths.
3. Add a native preflight that computes the analysis source root and intended `camcomp.json` destination before analysis.
4. For explicit photos from multiple directories, use the nearest meaningful common ancestor selected by Drop 1 discovery and show the destination before writing.
5. Reject a filesystem/volume root or no-common-root destination with an actionable request to open a containing folder; never write `camcomp.json` to an unexpectedly broad root.
6. Scope native photo access to roots/files acquired through the current import flow.
7. Provide local display handles/URLs without copying full image bytes through JSON IPC.
8. Keep analysis off the UI thread and preserve typed progress and cancellation.

### 9.5 Shared resolution rules

1. A folder with a valid `camcomp.json` loads it immediately without invoking analysis or hashing source contents.
2. Resolution enumerates paths and may flag cheap size/mtime differences; content hashing requires an explicit verify/refresh action.
3. A standalone JSON selection prompts for the source folder/files required by its relative paths.
4. A selection containing one JSON plus photos treats the photos as source-resolution candidates for that analysis; it does not start a new analysis.
5. More than one selected JSON is rejected as ambiguous.
6. Photos without JSON:
   - start native analysis in Tauri;
   - show a desktop-analysis explanation/action in the standalone browser.
7. Missing photos remain represented in group state with a visible error; the rest of the album remains usable.
8. No browser adapter writes, renames, moves, or deletes source files.

### 9.6 Tests

- Chromium handle, permission-denied, permission-revoked, and fallback-input flows.
- Safari/Firefox recursive input with nested paths, selected-root prefix stripping, and ambiguous-root rejection.
- File and folder drag/drop for each supported adapter shape.
- JSON-only, photos-only, JSON-plus-photos, multiple-JSON, empty, and cancelled selections.
- Existing cache loads without an analysis or hash invocation.
- Missing and cheap-mismatch source resolution.
- Tauri common-root selection on POSIX paths, Windows drive paths, different volumes, nested folders, and filesystem-root rejection.
- Native display access cannot escape acquired roots.
- To shorten the test run time, run most coverage on Chromium-only, with just sanity checks on the others.

### 9.7 Exit criteria

- Folder, file, and drag/drop acquisition work through one application contract.
- Browser mode can load a previous analysis and resolve its sources locally.
- Tauri can determine and display a safe `camcomp.json` target for new analysis.
- Capability differences are explicit and tested, not hidden behind browser-name branches.

## 10. Phase 4 — Import, cache, progress, and result UX

### 10.1 Tasks

1. Implement an import screen with:
   - Open folder;
   - Select files;
   - Drop photos or `camcomp.json`;
   - recent retained-directory affordance where Chromium permission allows;
   - platform capability explanation.
2. On a folder with valid `camcomp.json`:
   - load and validate immediately;
   - resolve source paths;
   - show cached groups and warnings;
   - do not automatically analyze, fingerprint, or rewrite.
3. Provide an explicit native Refresh analysis action that invokes incremental Drop 1 reuse.
4. On a folder without JSON in Tauri, start analysis after showing the selected source and write destination.
5. On photos without JSON in a browser, keep the selection intact and explain how to open it in Tauri; do not imply that analysis ran.
6. Render typed progress stages, determinate/indeterminate states, warnings, cancellation, completion, and cache-reuse counters.
7. Keep Cancel enabled only for a native in-flight analysis and preserve the previous valid JSON.
8. Present structured errors with remediation:
   - unsupported format;
   - malformed/incompatible JSON;
   - missing source;
   - permission denied/revoked;
   - invalid write target;
   - cancelled analysis;
   - decoder/memory guard.
9. Never overwrite malformed or incompatible existing JSON automatically. Any future replace action must be explicit and separately confirmed.
10. Show the saved filename/destination after native analysis without exposing unrelated absolute paths in generic logs.
11. Render group summaries, flags, confidences, and ungrouped reasons as the bridge into the viewer.
12. Preserve the user's selected analysis when source reauthorization fails so they can retry.

### 10.2 Tests

- State-machine transition tests, including repeated clicks and cancellation races.
- Existing valid cache invokes load but not analysis/refresh.
- Missing cache starts analysis only in Tauri.
- Browser photos-only flow never claims success or emits a JSON save.
- Progress event ordering and stale-run isolation.
- Cancellation leaves prior JSON untouched.
- Malformed/incompatible JSON is not overwritten.
- Keyboard, live-region, focus-restoration, and error-summary behavior.
- No input photo disappears from grouped/ungrouped result presentation.

### 10.3 Exit criteria

- Every Drop 2 acquisition route reaches a deterministic ready, analysis-required, cancelled, or error state.
- Cached analyses load without hidden work.
- New native analyses save `camcomp.json` atomically through the completed core.
- A user can reach the first group or inspect ungrouped reasons without using the CLI.

## 11. Phase 5 — HEIC browser fallback gate

### 11.1 Tasks

1. Implement actual decode capability probing against a selected HEIC rather than relying on user-agent detection.
2. Use exact HEIC/HEIF sources where the browser decodes them natively.
3. For non-native runtimes, resolve same-stem sidecars in deterministic order:
   - `.jpg`;
   - `.jpeg`;
   - `.png`.
4. Match sidecars within the same relative directory, with documented case behavior per source provider.
5. Decode the sidecar and compare its pixel dimensions with the document's oriented dimensions; reject mismatches and ensure EXIF orientation is not applied a second time.
6. Mark sidecar substitution visibly because compression, color, crop, and content may still differ despite matching dimensions.
7. Build narrow worker spikes for:
   - the existing `heif-oxide` through a small WASM/browser boundary;
   - `heic-to` using its worker/CSP-compatible path.
8. Test both candidates on all available Drop 1 HEIC fixtures and representative 50 MP files.
9. Measure:
   - fixture coverage;
   - orientation;
   - dimensions;
   - color-profile behavior;
   - time to bitmap;
   - peak memory and cleanup;
   - worker cancellation;
   - bundle size;
   - CSP requirements;
   - license and source-offer obligations.
10. Prefer a bitmap or lossless full-detail source. Do not introduce JPEG quality loss into the material used for detailed comparison.
11. Put compressed input, dimensions, decoded allocation, and concurrent-decoder guards around the selected path.
12. Decode at most one new HEIC source at a time until measurements justify otherwise.
13. Implement the final Tauri fallback through the existing native `heif-oxide` decoder off the UI thread, producing a lossless display source in application cache storage or a scoped native stream—never in the source folder and never as a full-image JSON IPC payload.
14. Key any native derived display source by the original digest and display-decoder parameters, and clean it through the application-cache policy.

### 11.2 Tests and evidence

- Magic-byte HEIC detection independent of extension.
- Native Safari path selected by successful decode capability.
- Sidecar preference before worker decode on Chromium/Firefox.
- Wrong-dimension sidecar rejected with the exact HEIC still attempted.
- Decoder output matches expected oriented dimensions and reviewed color fixtures.
- Worker does not block the UI and honors cancellation.
- Decoder resources and bitmaps are released after group changes.
- CSP remains free of `unsafe-eval`; if a candidate cannot meet this, record it as a rejection.
- Dependency license inventory records LGPL obligations if `heic-to` is selected.

### 11.3 Decision Gate A — non-native HEIC

Select a worker decoder only if it:

- decodes the representative HEIC corpus correctly;
- preserves orientation and acceptable color behavior;
- stays within the predefined memory guard on 50 MP sources;
- runs off the UI thread and cancels;
- complies with the application's CSP;
- has acceptable bundle and licensing consequences.

Prefer `heif-oxide` if it meets the gate because it is already shipped, permissively licensed, and compiled for WASM. Select `heic-to` only if its broader libheif behavior materially improves correctness enough to justify the larger LGPL dependency and obligations.

If neither passes, accept graceful degradation through sidecars and Tauri, document Chromium/Firefox HEIC as unavailable without those paths, and reopen the decoder decision rather than weakening fidelity or memory guards.

## 12. Phase 6 — OpenSeadragon simple-source gate

### 12.1 Tasks

1. Add OpenSeadragon behind a plain TypeScript `ViewerAdapter`; do not use a framework wrapper.
2. Define `PhotoSourceProvider` output for:
   - exact browser-native source;
   - same-stem sidecar;
   - worker-decoded source;
   - scoped Tauri local source.
3. Open each source as an OpenSeadragon simple image.
4. Normalize oriented dimensions and coordinates at the adapter boundary.
5. Implement lifecycle ownership for:
   - viewer instance;
   - source File/handle;
   - Blob/object URL;
   - decoded `ImageBitmap`/canvas;
   - event subscriptions;
   - resize observers.
6. Keep only the active group resident initially.
7. Load bounded previews/placeholder metadata before full source readiness where this does not alter comparison fidelity.
8. Display per-photo loading, failure, and fallback state without blocking other group members.
9. Run the Phase 0 large-image matrix in Safari, Chromium, Firefox, and Tauri.
10. Compare native `<img>` memory and startup evidence only as a baseline; do not build a second product renderer.

### 12.2 Tests

- Viewer creation, source replacement, destroy, and repeated group navigation.
- Object URL revocation and event-listener cleanup.
- EXIF orientation is not applied twice.
- Browser-native, sidecar, decoded, and Tauri source types reach identical normalized viewer contracts.
- One failed photo does not tear down the group.
- Repeated group changes return memory near the recorded baseline.
- Real 50 MP group tests for two, three, and four sources.

### 12.3 Decision Gate B — simple sources

Accept OpenSeadragon simple sources when:

- all required group sizes render without a tab/webview crash;
- peak memory stays below the Phase 0 ceiling;
- pan/zoom meets the interaction criterion;
- leaving a group releases the required memory;
- all accepted source kinds preserve dimensions, orientation, and fidelity.

If the gate fails because full decodes remain resident, activate a contained Phase 6B:

1. retain OpenSeadragon and `ViewerAdapter`;
2. add a digest-keyed `TiledPhotoSourceProvider`;
3. cache browser pyramids in OPFS and native pyramids in application cache storage;
4. generate one pyramid at a time with cancellation and size guards;
5. rerun Gate B.

A custom Canvas/WebGL renderer is evaluated only if OpenSeadragon also fails with real tiled sources. Phase 6B is not implemented when simple sources pass.

## 13. Phase 7 — Album shell, adaptive layout, and fixed EXIF

### 13.1 Tasks

1. Build album navigation over ordered groups with current/total position and previous/next controls.
2. Preserve stable photo ordering from the analysis document in normal mode.
3. Implement adaptive group layout:
   - two or three photos in columns for a landscape viewport;
   - two or three photos in rows for a portrait viewport;
   - four photos in a `2 × 2` grid;
   - more than four photos in a deterministic near-square scrollable grid so no member is hidden.
4. Recompute layout through container/viewport observation without recreating viewers unnecessarily.
5. Provide per-photo frame, loading state, source warning, alignment status, and fallback state.
6. Display the fixed default metadata:
   - effective 35 mm equivalent focal length when present, clearly labelled when corrected by the Drop 1 zoom policy, then reported 35 mm equivalent or physical focal length as labelled fallbacks;
   - oriented resolution;
   - camera make/model.
7. Represent missing metadata as unavailable, not as an invented value.
8. Expose anchor identity and alignment confidence/provenance without making diagnostics dominate the viewing surface.
9. Preserve a result/ungrouped route so Drop 3 viewing does not make Drop 2 diagnostics inaccessible.
10. Add fit/reset and per-photo focus affordances shared by later navigation phases.

### 13.2 Tests

- Exact layout selection for 2/3/4/>4 photos and portrait/landscape containers.
- Resize without loss of viewer/navigation state.
- Mixed aspect ratio and orientation fixtures.
- Fixed tag value, fallback, and missing-value presentation.
- Stable ordering and group navigation boundaries.
- Missing source and no-alignment members remain visible.
- Keyboard traversal and logical reading order match visual layout.

### 13.3 Exit criteria

- Every member of a selected group is visible or reachable without silent omission.
- Required 2/3/4 layouts adapt correctly.
- The three required EXIF values are shown consistently.
- Layout changes do not reset zoom or leak viewer resources.

## 14. Phase 8 — Canonical normalized synchronized navigation

### 14.1 Tasks

1. Define renderer-independent canonical navigation state:
   - mode;
   - leader photo ID;
   - normalized anchor-space center;
   - normalized visible span;
   - interaction source/sequence.
2. Define `PointMapper` and implement `NormalizedPointMapper`.
3. Convert OpenSeadragon image/viewport coordinates only inside `ViewerAdapter`.
4. Implement one leader at a time to prevent feedback loops between synchronized viewers.
5. Synchronize pan and zoom from mouse, trackpad, touch-capable desktop hardware, keyboard, fit, and reset actions.
6. Preserve normalized center/span across layout changes and leader changes.
7. Define bounds behavior for differing aspect ratios and image edges without oscillation.
8. Zoom toward the pointer in the leader while applying the resulting canonical state to peers.
9. Expose a temporary synchronization pause or independent inspection action if needed for recovery, without changing the default synchronized behavior.
10. Throttle/coalesce updates through animation frames rather than emitting recursive per-viewer event storms.

### 14.2 Tests

- Normalized point and visible-span conversion for mixed dimensions/aspect ratios.
- Leader event does not recurse through target viewer events.
- Pan, wheel zoom, keyboard zoom, fit, reset, and resize remain synchronized.
- Center near each edge remains stable.
- Leader changes preserve the scene center.
- Two, three, and four viewer synchronization.
- Repeated interactions maintain deterministic state within float tolerance.

### 14.3 Exit criteria

- The initial Drop 3 synchronized pan/zoom requirement works independently of alignment evidence.
- Canonical state contains no OpenSeadragon-specific object.
- Missing/unreliable alignments cannot disable normalized navigation.

## 15. Phase 9 — Global-homography-aligned navigation

This phase deliberately includes the requirements document's “further evolution” within the combined delivery, per the accepted alignment decision in `ADR-DROP3.md`. It remains separately gated after the baseline Drop 3.1 normalized viewer.

### 15.1 Tasks

1. Implement `GlobalHomographyPointMapper` over the persisted normalized-oriented matrices and inverses.
2. Map a leader point to anchor coordinates, then into each target photo.
3. Treat the anchor transform as identity and preserve row-major/direction semantics from the schema.
4. Reject non-finite mappings, near-zero projective denominators, invalid matrices, and unavailable target inverses.
5. Define mapping eligibility from actual alignment presence, schema validity, confidence, and provenance; do not invent an uncalibrated numeric threshold.
6. Add a user-visible navigation-mode switch:
   - normalized;
   - aligned.
7. Fall back per target to normalized coordinates when global mapping is unavailable or outside supported coverage, and show that fallback without disabling peers.
8. Keep normalized visible span as the initial zoom semantic. Record local homography-scale adjustment as a possible refinement, not hidden behavior.
9. Map the interaction point/crosshair into peers so the user can inspect the corresponding feature under the global transform.
10. Smooth or coalesce target-center updates to avoid abrupt jumps when the leader changes or a mapping becomes unavailable.
11. Keep local correspondence interpolation outside this implementation while preserving `PointMapper` extensibility.

### 15.2 Tests

- Identity, translation, rotation, scale, and perspective control-point fixtures shared with Rust geometry tests.
- OpenSeadragon viewport ↔ image coordinates ↔ normalized-oriented coordinates at corners, center, mixed aspect ratios, and zoomed states.
- Correct photo-to-anchor and anchor-to-photo direction.
- Mapping between two non-anchor members through anchor space.
- Missing, composed, low-confidence, invalid, outside-bounds, and manually unverified alignment cases.
- Per-target normalized fallback while other targets remain aligned.
- Mode switching preserves an intelligible center and zoom.
- Full-resolution mapping is independent of analysis image dimensions.

### 15.3 Decision Gate C — aligned navigation

Accept global-aligned mode when:

- every synthetic control maps within the Phase 0 float tolerance;
- independent real-scene landmark probes stay below the Phase 0 maximum error and improve over normalized mapping by the predefined margin;
- direct and composed mappings are reported separately so a composed result cannot hide weaker behavior;
- fallback is visible and stable for unsupported regions/members;
- moving subjects, parallax, and composed transforms do not cause crashes or uncontrolled viewport jumps;
- normalized mode remains available at all times.

If global mapping misses the predefined real-landmark gate, retain the tested mapper behind an experimental capability or defer its UI exposure; do not lower the gate after seeing results and do not implement local interpolation merely to hide a failed global gate.

## 16. Phase 10 — Two-photo comparison slider

### 16.1 Tasks

1. Show the slider-mode switch only for groups with exactly two resolved photos.
2. Prototype `img-comparison-slider` with two live OpenSeadragon viewer containers.
3. Verify pointer capture, wheel zoom, drag pan, touch-capable desktop gestures, focus, keyboard divider movement, resize, and teardown.
4. If the Web Component passes, wrap it behind a camcomp-owned `ComparisonSlider` component rather than exposing it throughout the app.
5. If it fails, implement a small local shell with:
   - two overlaid viewer containers;
   - a clipped leading layer;
   - a semantic range control/divider;
   - vertical default and horizontal orientation where the chosen control supports it cleanly.
6. Share the same `NavigationState`, `PointMapper`, `ViewerAdapter`, photo sources, and resource ownership as grid mode.
7. Preserve divider position while switching navigation mode; reset or clamp it predictably when the group changes.
8. Switching between grid and slider must not silently change photo order, zoom center, aligned/normalized mode, or metadata.
9. Use Juxtapose and Improve-ImgSLI only as interaction references; copy no source.

### 16.2 Tests

- Slider unavailable for group sizes other than two.
- Grid/slider round-trip preserves navigation and source state.
- Divider keyboard semantics, focus indication, labels, and screen-reader value.
- Wheel/pan events reach the intended viewer rather than being swallowed by the divider.
- Aligned and normalized navigation work in slider mode.
- Resize, orientation, repeated mount/unmount, and object URL cleanup.
- Dependency wrapper and local fallback satisfy the same component contract.

### 16.3 Decision Gate D — slider component

Adopt `img-comparison-slider` only if nested OpenSeadragon interaction and accessibility pass. Otherwise use the local clipping shell. The gate selects an implementation detail; it does not remove slider mode from Drop 3.1.

## 17. Phase 11 — PWA, offline shell, and capability hardening

### 17.1 Tasks

1. Finalize manifest metadata, icons, theme, display mode, and static routing.
2. Cache only fingerprinted application-shell assets in the service worker.
3. Keep selected photos, EXIF, analysis JSON, object URLs, decoded images, and derived private data out of Cache Storage.
4. Version service-worker caches and clean obsolete app-shell versions.
5. Add an offline startup state that can open locally selected data but never implies previously selected folders remain authorized.
6. Expose capability status for:
   - directory handles;
   - recursive input fallback;
   - retained permission;
   - native HEIC;
   - worker HEIC;
   - Tauri analysis;
   - PWA installation where the browser exposes it.
7. Provide explicit clear actions for retained directory handles/settings and any OPFS data used by contingencies.
8. Ensure no network request contains selected filenames, paths, metadata, JSON, image bytes, or generated blobs.
9. Apply a production CSP compatible with the emitted Svelte CSS, OpenSeadragon, workers, WASM if selected, and local object URLs. Do not add remote CDN scripts or broad script `unsafe-eval`; document any required inline-style directive and permit `wasm-unsafe-eval` only when the selected decoder proves it necessary.
10. Verify static hosting under HTTPS and local development under a secure localhost context.

### 17.2 Tests

- Offline app-shell reload in Chromium, WebKit, and Firefox Playwright projects.
- Service-worker update and stale-cache cleanup.
- Cache inspection confirms private inputs are absent.
- Network interception confirms no private data leaves the origin.
- Permission/reselection UX after reload for each browser adapter.
- Decoder worker and WASM under production CSP.
- Clear retained data removes IndexedDB/OPFS state without touching source files.

### 17.3 Exit criteria

- The frontend is deployable as static files.
- Browser mode remains useful without an application server.
- Offline/PWA behavior does not persist or upload private inputs unexpectedly.
- Capability limitations are stated at the action that depends on them.

## 18. Phase 12 — Cross-platform native hardening without packaging

### 18.1 Tasks

1. Keep Tauri commands and path handling portable across macOS, Windows, and Linux.
2. Remove spike-specific descriptions/configuration that call the frontend a Drop 1 proof once product UI replaces it.
3. Test separators, drive letters, UNC-style paths where testable, symlinks, Unicode, read-only folders, and permission failures.
4. Ensure native file scopes are granted from user acquisition and revoked/replaced when an album closes.
5. Preserve direct in-process Rust analysis with no sidecar.
6. Verify progress, cancellation, refresh, atomic write, and load-without-scan through the product frontend.
7. Add required native compile/check jobs on macOS, Windows, and Linux CI hosts; a platform is not covered by cross-compilation alone.
8. Build/check native application code on each host, but do not produce installers, DMGs, MSIX packages, AppImages, signatures, notarization, or release artifacts.
9. Document native development commands separately from future packaging instructions.

### 18.2 Tests

- Existing `camcomp-desktop` Rust command tests.
- New source-root/write-target and scoped-display-source command tests.
- Frontend adapter integration tests with mocked IPC.
- Host-platform compile/check matrix.
- Manual Tauri development-run smoke test on available host platforms.
- Source photos unchanged before/after load, view, analysis, cancel, and refresh.

### 18.3 Exit criteria

- No code path assumes POSIX-only or macOS-only source paths.
- Native analysis/viewing behavior is represented by automated adapter and command tests.
- Cross-platform source checks pass without claiming packaged-release support.

## 19. Phase 13 — Hardening, documentation, and final verification

### 19.1 Tasks

1. Audit import/viewer state for races, stale async completion, double cancellation, and album replacement during decode.
2. Audit object URLs, workers, viewer instances, observers, event listeners, IndexedDB handles, and decoded buffers for cleanup.
3. Audit malformed dimensions, oversized files, decompression bombs, invalid paths, and hostile JSON.
4. Audit keyboard-only use, focus order, visible focus, live progress, error association, and slider semantics.
5. Run dependency license/advisory checks and record the selected HEIC decoder obligations.
6. Update `docs/GUIDE.md` for browser/Tauri acquisition, cache behavior, viewer modes, format fallbacks, and limitations.
7. Update `README.md` status and development/verification commands.
8. Update `ADR-DROP3.md` only if measured gates force a different accepted architecture.
9. Record implementation and measurement evidence in an implementation handover artifact when code is delivered.
10. Confirm no private fixture path, filename, metadata, or cache content appears in committed outputs.

### 19.2 Full verification

Run the combined tasks, including:

```text
mise run fmt
mise run lint
mise run test
mise run test-fixtures
mise run build
mise run schema:check
mise run web:fmt
mise run web:lint
mise run web:typecheck
mise run web:test
mise run web:e2e
mise run web:build
mise run tauri:check
mise run check
```

Also perform and record:

- browser folder reload in desktop Safari, Chromium, and Firefox;
- Chromium retained-handle reauthorization;
- Safari/Firefox folder reselection;
- JSON-only then source-folder resolution;
- native cold analysis and save;
- native existing-cache load with no scan;
- explicit native incremental refresh;
- native cancellation preserving previous JSON;
- JPEG, PNG, native HEIC, sidecar HEIC, and worker HEIC paths;
- two-, three-, four-, and larger-group layout;
- normalized pan/zoom;
- global-aligned navigation and fallback;
- two-photo grid/slider switching;
- 50 MP Gate B matrix;
- offline app-shell and no-private-network checks;
- no source photo modification.

### 19.3 Exit criteria

- All Drops 2 and 3.1 definition-of-done items pass.
- Every activated decision gate has recorded evidence.
- Any unactivated pyramid contingency remains clearly documented and absent from production complexity.
- Browser, native, format, memory, and alignment limitations are visible in the guide.
- No installer/package claim is made.

## 20. Drops 2 and 3.1 definition of done

### 20.1 Drop 2 functional

- [ ] The user can open a local folder in Tauri.
- [ ] The user can select photos in Tauri.
- [ ] The user can drag/drop photos in Tauri.
- [ ] The user can select or drag/drop a previous `camcomp.json`.
- [ ] Browser users can load an existing analysis and grant its source folder/files locally.
- [ ] Chromium uses a directory handle when available.
- [ ] Safari and Firefox use recursive directory input with clear reselection behavior.
- [ ] A valid folder cache is detected and loaded without automatic analysis, hashing, or rewrite.
- [ ] Explicit native refresh invokes incremental Drop 1 reuse.
- [ ] New native analysis writes `camcomp.json` to the displayed safe common source root.
- [ ] Browser photos without JSON clearly require Tauri analysis.
- [ ] Progress, warnings, cancellation, errors, completion, and cache reuse are visible.
- [ ] Malformed/incompatible JSON is never overwritten automatically.
- [ ] Every input remains represented in grouped or ungrouped result presentation.

### 20.2 Drop 3.1 display

- [ ] A selected group displays every resolved member on one page.
- [ ] Two/three photos use columns in landscape and rows in portrait.
- [ ] Four photos use a `2 × 2` grid.
- [ ] Larger groups remain reachable through a deterministic generic grid.
- [ ] Focal length, oriented resolution, and camera model are shown by default.
- [ ] Missing metadata and missing photo sources are explicit.
- [ ] The user can zoom and pan with desktop mouse/trackpad and keyboard controls.
- [ ] Normalized mode keeps all viewer centers/spans synchronized.
- [ ] Aligned mode maps the interaction point through persisted global homographies.
- [ ] Unsupported alignments fall back visibly and deterministically.
- [ ] Local correspondence refinement is not accidentally implied or implemented.
- [ ] Exactly two photos can switch between grid and comparison-slider modes.
- [ ] Slider mode preserves navigation, order, metadata, and alignment mode.

### 20.3 Formats and performance

- [ ] JPEG and PNG display in desktop Safari, Chromium, Firefox, and Tauri.
- [ ] Safari uses native HEIC when actual decode succeeds.
- [ ] Non-native browsers prefer a dimension-validated same-stem sidecar.
- [ ] The selected worker HEIC decoder passes Gate A, or the documented sidecar/Tauri degradation is used.
- [ ] Full-detail comparison never silently uses a quality-`0.8` JPEG conversion.
- [ ] OpenSeadragon simple sources pass Gate B on representative 50 MP groups, or the pyramid contingency is explicitly activated.
- [ ] Viewer and decoder resources are released between groups.
- [ ] One failed source does not discard the rest of the group.

### 20.4 Architecture and privacy

- [ ] One Svelte/TypeScript/Vite build runs as static PWA assets and inside Tauri.
- [ ] UI components depend on typed platform, source, mapping, and viewer contracts.
- [ ] Browser/Tauri differences are isolated in adapters.
- [ ] `camcomp.json` browser types and validation stay synchronized with the Rust schema.
- [ ] No photo, EXIF, analysis JSON, or derived private data is uploaded.
- [ ] The service worker caches application assets only.
- [ ] Derived browser data uses OPFS only when required and is clearable.
- [ ] Native derived data uses application cache storage, never the source folder.
- [ ] Source photos are never modified, moved, or renamed.
- [ ] Cross-platform native code/checks pass without packaging deliverables.

### 20.5 Quality

- [ ] Frontend formatting, ESLint, typecheck, unit/component tests, E2E tests, and production build pass.
- [ ] Existing Rust formatting, Clippy, tests, reference fixtures, build, and schema checks remain green.
- [ ] Browser contract tests cover Chromium, WebKit, and Firefox.
- [ ] Real picker/permission/drop smoke tests are recorded.
- [ ] Keyboard and screen-reader acceptance checks pass.
- [ ] Large-image memory and responsiveness evidence is recorded on named hardware.
- [ ] Decoder and viewer dependency licenses are reviewed and documented.
- [ ] User/developer documentation reflects actual capabilities and limitations.

## 21. Requirement traceability

| Requirement | Planned coverage |
|---|---|
| Open a local folder | Phases 3–4; native plus browser capability adapters |
| Select/drag a set of files | Phases 3–4; native analysis and browser analysis-required state |
| Drag/drop previous JSON | Phases 2–4 |
| Detect folder `camcomp.json` | Phases 3–4 |
| Save Drop 1 JSON | Phase 4 through native atomic persistence |
| Never modify source photos | Phases 3, 11–13 and definition of done |
| Show selected group on one page | Phase 7 |
| Adaptive 2/3/4 layout | Phase 7 |
| Zoom and synchronized pan | Phase 8 |
| Feature-aligned evolution | Phase 9 global mapping; local refinement deferred |
| Default focal/resolution/model tags | Phase 7 |
| Local/private operation | Phases 1, 3, 11, and 13 |
| Interruptible analysis/progress | Phase 4 through completed Drop 1/Tauri contracts |

## 22. Gate summary and contingencies

| Gate | Required evidence | Accepted path | Contingency |
|---|---|---|---|
| A — non-native HEIC | correctness, fidelity, CSP, memory, worker, license | `heif-oxide` WASM or `heic-to` | sidecar then Tauri; reopen decoder decision |
| B — 50 MP source | group-size memory, responsiveness, cleanup | OpenSeadragon simple sources | OpenSeadragon tiled provider, then custom renderer only if needed |
| C — global alignment | synthetic accuracy, real usefulness, stable fallback | switchable global mapper | retain normalized mode; defer aligned UI if evidence fails |
| D — slider shell | nested viewer input and accessibility | `img-comparison-slider` wrapper | small camcomp-owned clipping shell |

A contingency is activated only by its gate. It must not be implemented pre-emptively “just in case,” because each one adds a separate codec, storage, or rendering system that requires its own security, memory, and maintenance surface.
