# ADR-DROP1: Implementation Stack for Grouping and Alignment

- **Status:** Proposed
- **Date:** 2026-08-23
- **Scope:** Drop 1 analysis engine and its path to desktop and web applications

## 1. Context

Drop 1 needs image decoding, EXIF extraction, local feature detection, descriptor matching, RANSAC homography estimation, image warping, numerical scoring, and JSON serialization. Later drops need interactive, high-resolution image display and must consume the alignment data produced by Drop 1.

The product may be delivered as:

- a local desktop application with direct folder access and no photo upload;
- a hosted web application with server-side analysis;
- a browser-only application that analyzes local files without uploading them;
- a shared web frontend packaged as a desktop application.

These delivery models place different constraints on file access, image codec support, compute performance, packaging, privacy, and code reuse.

## 2. Decision drivers

The stack should be evaluated against:

1. reliability of SIFT/AKAZE, descriptor matching, RANSAC, homography, and image-warping implementations;
2. EXIF and codec support, including a future decision on HEIC and RAW;
3. deterministic behavior and access to analysis diagnostics;
4. performance on multi-megapixel photos while using reduced analysis images;
5. local folder access and incremental cache updates;
6. ease of packaging on macOS, Windows, and Linux;
7. ability to reuse the Drop 1 engine in automated tests and batch workflows;
8. reuse between desktop and web deliveries;
9. quality of the Drop 3 zoom, pan, pointer, and synchronized-viewport experience;
10. offline use and keeping private photos on the user's device;
11. dependency maturity, binary size, and maintenance risk;
12. speed of the calibration loop, since Drop 1's thresholds and weights are undetermined and must be found empirically;
13. maintainer familiarity and willingness to maintain the stack, this being a single-maintainer project.

The persisted `camcomp.json` contract and normalized alignment coordinates shall remain implementation-neutral. UI and analysis components must not exchange language-specific serialized objects.

## 3. Considered options

### 3.1 Python analysis core with a Python desktop UI

**Representative stack**

- Python;
- OpenCV and NumPy for image analysis;
- an EXIF abstraction backed by a mature metadata library or ExifTool;
- PySide6/Qt for desktop UI;
- pytest for engine tests;
- a schema-validation library for `camcomp.json`.

**Desktop**

This is the shortest route to a fully local application. Qt provides folder selection, drag-and-drop, threads/processes, and image display without requiring a browser runtime. Analysis can run in worker processes to keep the UI responsive.

**Web**

The same Python engine can run behind an HTTP API. A separate JavaScript/TypeScript frontend would upload or reference photos and receive progress and JSON results. The Qt UI itself cannot be reused.

**Advantages**

- mature, well-documented computer-vision implementation;
- fastest path to testing the algorithm and tuning thresholds;
- straightforward numerical experimentation and diagnostics;
- one language for engine and initial desktop UI;
- easy CLI and batch operation;
- server-side web analysis can reuse the engine directly.

**Disadvantages**

- desktop packaging includes Python and native libraries;
- Qt desktop UI code is not reusable in a web frontend;
- Python packaging of OpenCV, metadata tools, and optional codecs needs platform testing;
- high-resolution interactive image rendering may need careful Qt optimization;
- browser-only offline analysis is not supported.

**Best fit**

A desktop-first product or an algorithm-validation phase where delivery speed and CV reliability matter more than frontend reuse.

### 3.2 Python analysis core with a TypeScript web UI and Tauri desktop shell

**Representative stack**

- Python, OpenCV, and NumPy analysis package;
- a versioned CLI or local RPC boundary around the analysis package;
- TypeScript web frontend using Canvas or WebGL for Drop 3;
- Tauri for desktop packaging;
- a packaged Python analysis sidecar for desktop;
- the same Python package in a web backend for hosted deployment.

**Desktop**

Tauri provides native file dialogs and controlled filesystem access. The frontend starts the bundled analyzer as a sidecar, sends an analysis request, receives structured progress, and loads `camcomp.json`. Source images remain local.

The sidecar must use a framed protocol rather than parsing human-readable console output. A simple initial protocol can use newline-delimited JSON over standard input/output, with explicit protocol versions and cancellation messages. Large images should be passed by authorized file path rather than copied through IPC.

**Web**

The frontend can be deployed as a regular web application. In hosted mode, an API service runs the same Python analysis package. Browser-only analysis would remain a separate future engine option.

**Advantages**

- retains the most mature and productive CV stack;
- reuses the interactive frontend between desktop and hosted web;
- Canvas/WebGL is well suited to synchronized zoom, pointer mapping, and overlays;
- Tauri is generally smaller than Electron;
- analysis remains usable independently through its Python API and CLI;
- local desktop mode does not require photo uploads.

**Disadvantages**

- two languages and an IPC boundary;
- desktop distribution must bundle Python, OpenCV, codecs, and the sidecar;
- packaging and signing need validation on every target OS;
- process lifecycle, cancellation, progress, and crash reporting require explicit design;
- hosted web mode requires uploading large and potentially private photos;
- browser-only offline operation is not achieved.

**Best fit**

A product expected to need both a polished desktop application and a hosted web UI, while minimizing risk in the image-analysis algorithm.

### 3.3 TypeScript/JavaScript throughout with OpenCV.js

**Representative stack**

- TypeScript;
- OpenCV.js/WebAssembly for feature detection and geometric registration;
- browser EXIF libraries;
- Web Workers for analysis;
- Canvas/WebGL for display;
- Tauri or Electron for desktop packaging.

**Desktop**

The same web application can run in a desktop shell. Tauri adds native filesystem integration with a smaller shell; Electron offers a uniform Chromium/Node environment but has a larger distribution and resource footprint.

**Web**

Photos can be analyzed entirely in the browser, preserving privacy and avoiding server upload. File System Access API support varies by browser, so file input and drag-and-drop fallbacks are still required.

**Advantages**

- maximum UI and analysis code reuse between desktop and browser;
- browser-only offline processing is possible;
- no Python sidecar or analysis server is required;
- straightforward integration with Canvas/WebGL and workers;
- photos can remain local in both browser and desktop modes.

**Disadvantages**

- OpenCV.js builds and JavaScript bindings expose a less predictable subset of native OpenCV functionality;
- SIFT availability, contrib modules, SIMD, threading, and codec support must be verified with a prototype rather than assumed;
- WASM memory pressure and browser limits matter for multiple large images;
- EXIF, HEIC, color management, and RAW support are less consistent in browsers;
- debugging numerical CV behavior is generally less convenient than in native OpenCV/Python;
- long analysis tasks require careful worker, cancellation, and memory management;
- a custom OpenCV.js build increases build and update complexity.

**Best fit**

A strict browser-only/offline requirement that justifies greater technical risk in the Drop 1 engine.

### 3.4 Pure-Rust core with Tauri and optional WebAssembly

**Representative stack**

- Rust analysis library organized behind internal feature-extractor, matcher, homography-estimator, and image-warper interfaces;
- `image` and `imageproc` for common-format decoding, thumbnails, gradients, and projective warping;
- `kamadak-exif` for standard EXIF fields;
- `sift-features` for SIFT keypoints and 128-byte descriptors;
- a small internal brute-force KNN matcher with Lowe ratio and optional mutual matching;
- `vision-geometry` for normalized DLT and RANSAC homography estimation;
- `geo` for polygon overlap, with default multithreading disabled for WASM;
- `serde`, `serde_json`, `schemars`, and `sha2` for the cache contract;
- `rayon` for native task parallelism;
- a TypeScript frontend and Tauri 2 desktop shell.

`purecv` is an alternative integrated provider for ORB, KNN matching, homography, perspective warping, gradients, and WASM bindings. It is not the proposed baseline because it declares itself unstable/work-in-progress and uses LGPL licensing with a linking exception. It remains useful as a fallback, comparison implementation, or future consolidation candidate after a license and quality review.

**Desktop**

The native Rust engine is called directly from Tauri commands. Analysis should run in Tauri-managed asynchronous tasks backed by a bounded Rayon pool, with progress and cancellation sent as typed events. There is no Python runtime, sidecar process, native OpenCV installation, or application/sidecar protocol.

**Web**

The domain, schema, scoring, and most CV code can compile to WebAssembly. Browser-specific file acquisition and worker orchestration remain adapters. Native Rayon must be disabled or replaced by worker-level parallelism. The selected SIFT and image-processing crates compile for `wasm32-unknown-unknown`; the proposed homography crate requires the consumer to enable `getrandom`'s `wasm_js` feature, which the dependency smoke check verified.

A hosted web service can instead run the same Rust core natively and avoids these WASM constraints, at the cost of uploading photos.

**Advantages**

- complete Drop 1 algorithm coverage is available without OpenCV FFI;
- one native language and process for the analysis engine and Tauri backend;
- strong type and memory safety;
- good native performance, deterministic seeded robust fitting, and bounded concurrency;
- direct access to all diagnostics required by the specification;
- compact desktop deployment without a Python/OpenCV sidecar;
- substantial engine reuse for hosted web and a credible browser-WASM path;
- the normalized alignment contract remains directly consumable by the TypeScript frontend.

**Disadvantages**

- the key CV crates are much younger and less broadly validated than OpenCV;
- `sift-features` is at `0.1.0`, exposes only limited tuning, and has a small maintainer/user base;
- `vision-geometry` is new, currently has an MSRV of Rust 1.93, and exposes some RANSAC configuration as lower-level API;
- descriptor conversion and matching glue must be maintained by camcomp;
- no selected pure-Rust HEIC or RAW decoder is mature enough to include without a separate format spike;
- browser and native file access, parallelism, and optional codecs still require target-specific adapters;
- numerical behavior and failure modes must be validated against known ground truth on representative photos, with an OpenCV comparison held in reserve.

**Best fit**

A Rust- and desktop-first product that values a single Tauri-native engine, accepts a focused validation spike for young CV dependencies, and treats browser-only execution and advanced codecs as explicit follow-up capabilities rather than assumptions.

#### 3.4.1 Variant: Rust core with OpenCV bindings

This variant keeps the whole architecture of Option 3.4 — one Rust engine, Tauri commands, no sidecar, camcomp-owned traits — but implements the `FeatureExtractor`, `DescriptorMatcher`, `HomographyEstimator`, and `ImageWarper` traits with the [`opencv`](https://crates.io/crates/opencv) Rust bindings instead of the young pure-Rust crates.

**Advantages**

- canonical, deployment-proven CV behavior on day one, removing the largest identified risk;
- the same OpenCV that serves as the diagnostic contingency, so there is only one implementation to reason about;
- SIFT tuning knobs, `findHomography` variants, and battle-tested degeneracy handling are available immediately.

**Disadvantages**

- reintroduces a native, non-Rust dependency into packaging and cross-compilation, including `libclang` at build time;
- does not compile to `wasm32-unknown-unknown`, so it closes the browser-only path unless a second extractor implementation is maintained behind the same traits;
- larger binaries and per-OS build/signing validation.

The trait boundary of Section 6 makes it a swap of one adapter rather than a re-architecture. If Gate A fails, this is the cheapest recovery, and the browser path is already gated separately.

Note on licensing: the SIFT patent expired in March 2020 and SIFT moved into the main OpenCV modules in 4.4.0, so neither the OpenCV variant nor `sift-features` carries a patent constraint.

### 3.5 Native C++ OpenCV core

**Representative stack**

- C++ and OpenCV;
- C ABI, command-line boundary, or language bindings for UI integration;
- Qt, Tauri, or a web frontend.

**Advantages**

- direct access to the canonical OpenCV API;
- excellent native performance and control;
- fewer surprises than wrappers for advanced OpenCV functions.

**Disadvantages**

- significantly higher implementation and memory-safety burden;
- cross-platform dependency and packaging complexity;
- requires bindings or IPC for a web-oriented UI;
- browser use requires a separate Emscripten/OpenCV.js build;
- unnecessary optimization for thumbnail-based Drop 1 analysis until profiling says otherwise.

**Best fit**

A performance-critical mature engine, not the initial Drop 1 implementation.

### 3.6 Rust dependency audit

The following evidence was collected on 2026-08-23 from published crate metadata, documentation, current repository source, local native builds, local WASM checks, and upstream test suites.

| Capability | Candidate | Audit result | Proposed use |
|---|---|---|---|
| JPEG/PNG/WebP/TIFF and thumbnail decoding | [`image 0.25.10`](https://crates.io/crates/image/0.25.10) | Mature, active, permissive, Rust 1.88; no HEIC decoder | Baseline |
| Resize, gradients, and homography warp | [`imageproc 0.25.x`](https://docs.rs/imageproc/0.25.1/imageproc/) | Mature, permissive; projective `Projection` and bilinear `warp_into`; aligns with the version used by `sift-features` | Baseline; upgrade both crates together later |
| Standard EXIF | [`kamadak-exif 0.6.1`](https://crates.io/crates/kamadak-exif/0.6.1) | Pure Rust, BSD-2-Clause, active; manufacturer MakerNote interpretation remains limited | Baseline behind a metadata interface |
| SIFT detector and descriptor | [`sift-features 0.1.0`](https://docs.rs/sift-features/latest/sift_features/) | MIT; full detector plus 128-byte descriptors; native and `wasm32-unknown-unknown` library checks passed; young, minimally configurable, and last source change was in 2024 | Conditional baseline and principal spike risk |
| Descriptor matching | Internal brute-force KNN | SIFT output is directly usable with squared/L2 distance; KNN=2, Lowe filtering, and mutual matching are small deterministic operations; avoids another young abstraction | Baseline, with fixture and property tests |
| Normalized DLT and robust homography | [`vision-geometry 0.8.0`](https://crates.io/crates/vision-geometry/0.8.0) | MIT; normalization, degeneracy/rank checks, seeded adaptive RANSAC, inliers, refit, and tests with outliers; 37 native tests passed; young and Rust 1.93 minimum | Conditional baseline behind an internal trait |
| Alternative consensus | [`arrsac 0.11.0`](https://github.com/rust-cv/arrsac) | MIT, maintained, `no_std`; provides consensus only, not a homography estimator | Reserve if the baseline RANSAC proves inadequate |
| Polygon overlap | [`geo 0.33.1`](https://crates.io/crates/geo/0.33.1) | Mature and permissive; disable unnecessary default multithreading for WASM | Baseline, unless a small convex-quad clipper is preferable |
| Cache hashing | [`sha2 0.11.0`](https://crates.io/crates/sha2/0.11.0) | Mature, pure Rust, permissive, WASM-capable | Baseline |
| JSON and schema | `serde`, `serde_json`, `schemars` | Mature, permissive, native/WASM-capable | Baseline |
| Native parallelism | `rayon` | Mature; use natively, but do not assume transparent browser threading | Native-only baseline |
| Integrated CV alternative | [`purecv 0.7.1`](https://github.com/webarkit/purecv) | Covers ORB, KNN/ratio matching, DLT/RANSAC homography, warping, gradients, native parallelism, and WASM; 384 unit tests and 40 doc tests passed locally; project is less than one year old and declares itself unstable/WIP; LGPL-2.1-or-later metadata and LGPL-3.0 source headers include a linking exception | Evaluate as fallback, not initial baseline |
| AKAZE alternative | [`akaze 0.7.0`](https://github.com/rust-cv/akaze) | MIT but repository is archived, last code update was in 2020, and it depends on old image/ndarray generations | Do not select |
| Older homography crate | [`homography 0.2.0`](https://crates.io/crates/homography/0.2.0) | MIT but lacks point normalization, robust fitting, and extensive validation | Do not select |

The `sift-features` library itself checked successfully both natively and for `wasm32-unknown-unknown`. Its complete test target did not run in the research environment because its test-only OpenCV comparison dependency requires `libclang`; this does not add OpenCV to consumers but does reduce confidence until camcomp runs the upstream parity tests in CI.

`purecv` checked for `wasm32-unknown-unknown`, and its repository contains a dedicated WASM package and CI build. `vision-geometry` did not compile for bare `wasm32-unknown-unknown` in isolation because its random-number dependency requires the `getrandom/wasm_js` feature. The complete camcomp dependency smoke check succeeded after adding a direct `getrandom` dependency with that feature, so the workaround is verified; the project must encode it explicitly and keep it in WASM CI.

HEIC remains the largest format gap. [`heif-oxide 0.1.0`](https://crates.io/crates/heif-oxide/0.1.0) is permissive and pure Rust but too young to accept without real iPhone fixtures. `heif-rs` is Apache-2.0 but statically links native libheif and changes packaging and WASM assumptions. The pure-Rust `heic` crate uses AGPL-3.0-or-commercial licensing. Drop 1 should initially declare JPEG/PNG support unless a HEIC spike selects and validates a decoder. RAW should also remain deferred.

This audit establishes **coverage**, not OpenCV-equivalent maturity. Every major Drop 1 operation has a credible Rust implementation; the residual risk is validation, API youth, advanced codecs, and cross-target integration.

### 3.7 Browser-only feasibility

A pure client-side implementation is **technically feasible for an initial JPEG/PNG scope**, but it should remain conditional until runtime memory and performance are measured in real browsers. It does not require a server or OpenCV.js.

A temporary crate containing the proposed `image`, `imageproc`, `kamadak-exif`, `sift-features`, `vision-geometry`, `geo`, hashing, schema, and `wasm-bindgen` dependencies compiled successfully for `wasm32-unknown-unknown` after explicitly enabling `getrandom`'s `wasm_js` feature. A release smoke module that invoked SIFT and linked representative schema, geometry, and hashing code was 628 KiB before JavaScript glue and final `wasm-opt` processing. This proves target compatibility, not acceptable browser runtime behavior or final bundle size.

The browser architecture should be:

1. obtain files through `showDirectoryPicker()` when available, with multi-file input and drag-and-drop fallbacks;
2. read EXIF and compressed bytes in a worker-compatible adapter;
3. decode and downscale one source photo at a time before SIFT, preferably using `createImageBitmap`/`OffscreenCanvas` when their orientation and color behavior pass fixtures;
4. transfer only the oriented grayscale analysis buffer to a dedicated analysis Web Worker;
5. run the single-threaded WASM core in that worker initially;
6. persist metadata, descriptors, pair results, and optional source handles in IndexedDB or the Origin Private File System;
7. write `camcomp.json` back through a granted directory/file handle where supported, otherwise offer it as a download;
8. keep source photos as user-selected `File`/handle references or object URLs rather than copying them into WASM memory for Drop 3.

Browser filesystem behavior is not equivalent to Tauri:

- [`showDirectoryPicker()`](https://developer.mozilla.org/en-US/docs/Web/API/Window/showDirectoryPicker) is secure-context-only and marked limited availability, so automatic discovery and in-place update of `camcomp.json` cannot be universal;
- file-input fallback can analyze selected files but cannot silently scan or overwrite their source folder;
- the [Origin Private File System](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system) is suitable for internal caches but is origin-private, not the user's visible photo folder;
- persisted handles and permissions must be treated as optional and revalidated after reload.

Memory is the principal feasibility risk. `sift-features` currently upsamples the input by two and retains Gaussian scale-space and Difference-of-Gaussian arrays for all octaves. For a 1,200 × 900 grayscale analysis image, those `f32` arrays alone have a rough upper bound around 250 MB before decoded source pixels, keypoints, descriptors, and temporary allocations. Browser and mobile memory ceilings make parallel extraction unsafe. Mitigations are:

- process and release one analysis image at a time;
- cache compact keypoints/descriptors before moving to the next image;
- test an 800-pixel or lower browser analysis edge separately from the native default;
- prefer browser-assisted downscaled decode so a 48-megapixel source is not fully expanded in WASM;
- profile and, if necessary, contribute or maintain a streaming-octave SIFT implementation that discards completed pyramid levels;
- reject files using compressed-size, pixel-count, and allocation-budget guards.

The current `sift-features` dependency also enables default `image`/`imageproc` features transitively, which pulls in Rayon and more codecs than a minimal browser build needs. It still compiles to WASM, but camcomp should seek an upstream feature-gating change or use a reviewed patch before treating the browser bundle and runtime as production-ready.

Transparent multithreading must not be assumed. WASM threads require [`SharedArrayBuffer`](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer) and cross-origin isolation, which complicates deployment. The first browser implementation should use one analysis worker and remain responsive through progress and cancellation. Parallelizing independent photos across workers is a later optimization after peak memory is bounded.

A browser-only release therefore has these initial acceptance boundaries:

- desktop-class browsers first;
- JPEG and PNG first;
- explicit file/folder permission;
- one analysis worker;
- bounded analysis dimensions and sequential extraction;
- downloadable JSON fallback;
- no promise of HEIC, RAW, mobile-browser performance, or automatic source-folder cache discovery on every browser.

Within those boundaries, pure client-side Drop 1 and Drop 3 are feasible. General cross-browser, mobile, HEIC-capable operation is not yet established.

## 4. Comparison

Scores are relative: 5 is most favorable.

| Criterion | Python + Qt | Python + Tauri/web UI | TypeScript + OpenCV.js | Rust native/WASM | Rust + OpenCV FFI | C++ OpenCV |
|---|---:|---:|---:|---:|---:|---:|
| Drop 1 capability coverage and diagnostics | 5 | 5 | 3 | 4 | 5 | 5 |
| CV dependency maturity | 5 | 5 | 3 | 3 | 5 | 5 |
| Drop 1 iteration speed | 5 | 4 | 3 | 3 | 3 | 2 |
| Desktop UI delivery | 4 | 4 | 4 | 5 | 5 | 3 |
| Hosted web reuse | 2 | 5 | 5 | 4 | 4 | 3 |
| Browser-only/offline web | 1 | 1 | 5 | 4 | 1 | 2 |
| Drop 3 frontend reuse | 2 | 5 | 5 | 5 | 5 | 3 |
| Packaging simplicity | 3 | 2 | 4 | 5 | 2 | 2 |
| Codec/EXIF flexibility | 4 | 4 | 2 | 3 | 5 | 5 |
| Initial engineering risk | 5 | 4 | 3 | 3 | 3 | 2 |

The Rust scores improve because source inspection and builds verified complete algorithm coverage and direct Tauri integration. They remain below OpenCV on CV maturity because the proposed SIFT and homography crates are young and lack OpenCV's deployment history. Packaging scores assume JPEG/PNG and a pure-Rust baseline; selecting native libheif would reduce that advantage.

The table does not make a browser-only build equivalent to a hosted web build. That distinction is central to the decision.

### 4.1 How the table is being read

The decision is explicitly **not** a sum. It is driven by three criteria treated as dominant, in this order:

1. **single-runtime desktop delivery** (no Python or OpenCV sidecar to package, sign, and version) — rows "Desktop UI delivery" and "Packaging simplicity";
2. **keeping the browser-only path open** without a second engine — row "Browser-only/offline web", the only row where the proposal beats every OpenCV-based option;
3. **frontend reuse across desktop and web** — row "Drop 3 frontend reuse".

Everything else is a cost being knowingly accepted. 

## 5. Proposed decision

Provisionally adopt a **pure-Rust analysis core integrated directly into a Tauri 2 application**, with a TypeScript frontend and the implementation-neutral, versioned `camcomp.json` contract.

The runtime baseline is:

1. `image`, `imageproc`, and `kamadak-exif` for image and metadata normalization;
2. `sift-features` as the primary feature extractor;
3. an internal deterministic brute-force KNN matcher with Lowe ratio and optional mutual checks;
4. `vision-geometry` for normalized-DLT RANSAC homography behind a camcomp-owned trait;
5. `imageproc` for perspective warping and gradient metrics;
6. `geo`, `sha2`, `serde`, `serde_json`, and `schemars` for overlap, fingerprints, and the cache contract;
7. target-gated `rayon` for bounded native parallelism;
8. direct Tauri commands and typed events for import, analysis, progress, and cancellation.

This proposal chooses Option 3.4, conditional on the vertical validation spike in Section 9. The spike must demonstrate correct grouping and stable alignment on the initial dataset, acceptable performance, OpenCV-comparable geometric results, and a packageable Tauri build. Failure of the SIFT or homography acceptance criteria reopens the decision, in the order: Option 3.4.1 (swap the trait implementations for OpenCV bindings, native-only), then Python/OpenCV as a sidecar or a rewrite.

### 5.1 The algorithm is the unsolved problem, not the runtime

The thresholds, weights, gates, and even the shape of the scoring function are undetermined (`SPEC.md` §5.4.1, §9). Choosing a runtime before the algorithm is characterised is a real risk, and it is aggravated by picking the stack with the slowest experimental loop.

The decision is nevertheless to **calibrate directly in Rust**, on the real photo sets described in `SPEC.md` §9. Those sets have a strong, discriminating shape —  at least two matching photos, and at least one near-miss from the same session — so the expected outcome is visible without statistical machinery, and a second implementation would mostly be ceremony. Build one thing.

The contingency is explicit rather than pre-emptive. A **Python/OpenCV reference implementation** of the pipeline is built only if calibration in Rust stalls, meaning any of:

- the real sets cannot be separated at any threshold, and it is unclear whether the cause is the algorithm or `sift-features`/`vision-geometry`;
- results are unstable or non-reproducible in a way not explained by the code;
- more than a timeboxed effort has been spent on the calibration loop without convergence.

At that point the reference exists to answer one question — algorithm or dependency? — and it is retired once answered. It is development-only, lives in `tools/`, and is never shipped. Section 7's rule that OpenCV shall not be a runtime dependency is unchanged.

Building it up front was considered and rejected: it doubles the surface to keep in sync, and the diagnostic value it buys is only needed in the failure case.

For web delivery:

- a hosted service can reuse the Rust core natively;
- browser-only analysis is a separate target using the same domain logic and WASM-capable CV components, but it is not accepted until the random-source feature issue, worker model, memory ceiling, and file/codec path are proven;
- Drop 3's TypeScript display and normalized alignment mapping are shared between Tauri and web regardless of where analysis runs.

The analysis core shall remain usable without Tauri through a Rust library API and a small CLI. This keeps fixtures, batch operation, profiling, and algorithm calibration independent from the UI.

## 6. Component boundaries

The Rust workspace should separate a UI-independent `camcomp-core` crate from Tauri commands, a CLI adapter, and any later WASM adapter. The core should expose operations equivalent to:

```text
inspect(inputs, options) -> photo metadata
analyze(inputs, options, previousAnalysis?) -> analysis result
regroup(analysis, overrides) -> updated groups
validate(document) -> validation result
```

Internal traits shall isolate the young CV dependencies:

```text
FeatureExtractor
DescriptorMatcher
HomographyEstimator
ImageWarper
MetadataReader
```

This permits fixture comparison, dependency replacement, and target-specific adapters without changing grouping or JSON code. Public operations shall use camcomp-owned schema types rather than `image`, `ndarray`, `nalgebra`, `purecv`, or Tauri types. Binary or high-volume feature caches may use an internal versioned format, but `camcomp.json` remains portable and inspectable.

A Tauri, CLI, hosted-server, or WASM adapter shall provide:

- version and capability negotiation;
- analysis start and cancellation;
- structured progress events;
- structured errors without absolute-path or metadata leakage in hosted logs;
- atomic writing of `camcomp.json`;
- recovery from an incomplete prior run.

The frontend shall own:

- import and folder-selection UX;
- group correction UX;
- side-by-side display;
- viewport synchronization;
- application of persisted homographies and local correspondences;
- blind-mode behavior and scoring.

The engine shall own:

- image and metadata normalization;
- session and focal prefilters;
- feature extraction and caching;
- pair scoring and geometric validation;
- automatic grouping;
- alignment evidence;
- schema-compliant analysis output.

## 7. Dependency considerations

The versions in Section 3.6 are an evidence snapshot, not permission to use floating version ranges. Implementation should pin reviewed releases in `Cargo.lock`, record a workspace MSRV compatible with the selected `vision-geometry` release, and update dependencies deliberately with fixture regressions.

Before accepting the stack, representative files must verify:

- EXIF orientation and timestamps;
- 35 mm equivalent focal length extraction;
- manufacturer-specific mode tags and MakerNote limitations;
- ICC/color handling relevant to analysis thumbnails;
- JPEG and PNG decoding across devices;
- Unicode and non-ASCII paths;
- pixel-count and allocation limits before full decode;
- license and redistribution requirements.

The first release should not imply HEIC or RAW support merely because candidate crates exist. Adding either format requires a separate decision covering decoder correctness, color depth/profile handling, metadata extraction, native/WASM availability, licensing, security limits, and packaging.

The initial Rust workspace should align `imageproc` with the version used by `sift-features` to avoid duplicate processing stacks. Dependency types shall be converted at the internal trait boundary. `purecv` shall not enter the runtime dependency graph without a license review and a benchmark showing why the baseline components are insufficient.

OpenCV may be used as a development-only oracle for fixture generation and metric comparison, and as the contingency reference implementation of Section 5.1. It shall not be a shipped dependency under this decision, and any such tooling lives in a `tools/` directory outside the Rust workspace so it cannot enter the runtime graph by accident. Hosted deployments must impose upload, decode-size, pixel-count, time, and memory limits before decoding untrusted images.

## 8. Consequences

### Positive

- Drop 1 uses classical CV without a learned model or shipped OpenCV dependency.
- The Tauri desktop application has one native Rust process rather than a packaged analysis sidecar.
- Algorithm tests, CLI analysis, and calibration remain independent from UI work.
- Camcomp-owned traits prevent young CV crate APIs from leaking into grouping, cache, or frontend contracts.
- The JSON and normalized alignment contract remains language- and target-neutral for Drop 3.
- Desktop photos remain local.
- A later hosted service can reuse the native Rust core.
- A browser-only path can reuse the Rust domain and most CV implementation rather than requiring OpenCV.js.
- The Drop 3 TypeScript frontend is reusable in Tauri and web delivery.

### Negative

- The application still uses Rust and TypeScript, although there is no second backend runtime.
- SIFT and homography dependencies are young and require camcomp-owned regression coverage.
- Some matching and interop glue becomes camcomp code.
- The selected homography stack raises the initial Rust MSRV.
- Pure client-side processing requires browser-specific filesystem, worker, persistence, and memory management.
- HEIC and RAW are not covered by the initial dependency decision.
- Hosted web delivery still introduces upload cost and privacy obligations absent from desktop and browser-only products.
- The stack with the slowest experiment loop was chosen for a problem whose parameters are not yet known, and calibration happens in that stack. Section 5.1 bounds this with a timebox and a contingency rather than removing it.
- Without a reference implementation available from the start, an early calibration failure is ambiguous between an algorithm problem and a young-dependency problem, and resolving that ambiguity costs a detour.
- HEIC is the most likely real-world input from modern iPhones and recent Android devices. Declaring JPEG/PNG-only means part of the intended dataset requires manual conversion before it can be analysed at all, which is a user-visible limitation and not merely a deferred technical decision.

### Risks and mitigations

- **SIFT correctness or performance:** validate against synthetic ground truth and the labelled real sets, cap features and analysis dimensions, and keep the extractor behind a replaceable trait so an OpenCV comparison or replacement stays one adapter away.
- **SIFT memory use:** extract sequentially, downscale before WASM, measure peak memory, and investigate streaming pyramid octaves.
- **Homography numerical behavior:** check inlier masks, reprojection residuals, corner mapping, and degenerate inputs against synthetic transforms with known control points, escalating to an OpenCV comparison only on unexplained disagreement.
- **Young dependency/API risk:** pin exact reviewed releases, wrap their types, retain fixtures, and avoid broad automatic upgrades.
- **WASM random source:** explicitly enable and test `getrandom/wasm_js`; do not rely on transitive feature accidents.
- **WASM threading:** start with one worker and no shared-memory requirement; add threads only with a deployment decision for cross-origin isolation.
- **Filesystem differences:** support directory handles opportunistically and preserve file-input plus JSON-download fallbacks.
- **Tauri packaging risk:** build a minimal signed/packageable application on each target OS before substantial UI work.
- **UI/engine incompatibility:** generate TypeScript schema types where practical, version `camcomp.json`, and test round trips.
- **Large caches:** bound persisted correspondences and keep descriptors in an internal optional cache.
- **Codec inconsistency:** declare first-release formats explicitly and gate HEIC/RAW on representative fixtures and license review.
- **Hosted image attacks:** enforce compressed-size, pixel-count, decode-time, memory, and upload limits.

## 9. Validation spikes before final acceptance

The ADR should move from Proposed to Accepted only after these experiments pass with thresholds defined before results are reviewed.

### 9.1 Native algorithm spike

1. Build a small `camcomp-core` pipeline using the proposed detector, matcher, homography, warp, and gradient dependencies.
2. Run it on the real photo sets and the synthetic fixtures.
3. Verify correctness primarily against known ground truth — synthetic transforms with known control points, and the labelled real sets — rather than against an OpenCV oracle. Bring in the oracle only to diagnose a disagreement that ground truth cannot explain.
4. Verify the compatibility threshold and hard gates produce the expected labels across all real sets; do not tune to a single set.
5. Include degenerate, repeated-pattern, low-texture, exposure-shifted, rotated, translated, and perspective-shifted synthetic fixtures.
6. Confirm normalized homographies map full-resolution pointer coordinates correctly.
7. Confirm persisted sparse correspondences can support a simple local piecewise-affine pointer mapping.
8. Measure runtime and peak memory with sequential and bounded-parallel extraction on representative desktop hardware.
9. Extract required EXIF fields from representative devices and modes, including orientation and missing equivalent focal length.
10. Round-trip a versioned `camcomp.json` fixture and invalidate only changed source fingerprints.

### 9.2 Tauri spike

1. Call the Rust core directly from a Tauri command without a sidecar.
2. Demonstrate progress, cancellation, structured error propagation, and atomic JSON writing.
3. Keep the UI responsive during analysis.
4. Package the minimal application on the first target OS and inspect the shipped dependency/license inventory.

### 9.3 Browser-only spike

1. Build the complete selected dependency graph for `wasm32-unknown-unknown` with explicit target features; the research smoke check already passed and should become reproducible project CI.
2. Run SIFT, matching, homography, and warping in a Web Worker on actual fixtures rather than relying on compile success.
3. Measure wall time and peak memory at several analysis dimensions, including 1,200 and 800 pixels, on desktop Chrome, Firefox, and Safari where supported.
4. Verify cancellation and cleanup release per-photo pyramid memory before the next extraction.
5. Verify EXIF orientation and browser-assisted downscaled decode against native output.
6. Demonstrate folder import and in-place `camcomp.json` discovery where `showDirectoryPicker()` is supported.
7. Demonstrate multi-file input, drag-and-drop, and JSON-download fallbacks where directory access is unavailable.
8. Reload descriptors and pair results from IndexedDB or OPFS without access to stale source files.
9. Confirm the deployment works without shared-memory threads; evaluate cross-origin isolation only as a separate optimization.
10. Decide whether measured mobile-browser behavior is supportable or explicitly out of scope.

Failure of a browser-only spike does not block the native Tauri decision unless browser-only delivery is made a Drop 1 release requirement. Failure of the native SIFT/homography or packaging spike reopens the core stack decision.

## 10. Deferred decisions

- frontend framework and rendering library;
- exact Rust workspace layout and CLI framework;
- supported image codecs beyond initial JPEG/PNG;
- HEIC decoder and native/WASM parity;
- manufacturer-specific metadata strategy beyond standard EXIF;
- hosted API framework, storage, and retention policy;
- production browser-only support and mobile-browser scope;
- whether to adopt `purecv`, contribute upstream changes, or retain the component stack;
- whether SIFT pyramid memory should be improved upstream or through a maintained patch;
- whether performance evidence eventually justifies OpenCV FFI, Rust SIMD work, or another feature extractor.
