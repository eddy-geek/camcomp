# Drop 1 Implementation Plan

- **Status:** Proposed
- **Sources:** `REQUIREMENTS.md`, `SPEC.md`, and `ADR-DROP1.md`
- **Primary target:** Pure-Rust analysis core integrated with Tauri 2
- **Initial formats:** JPEG and PNG
- **Compatibility threshold:** `0.80` after geometric hard gates

## 1. Objective

Deliver a tested Drop 1 implementation that:

- imports photo metadata and stable source fingerprints;
- detects shooting sessions and focal-compatible candidate pairs;
- identifies photos showing substantially the same scene despite moderate camera movement;
- creates non-transitive, anchor-based comparison groups;
- persists pair diagnostics, groups, manual overrides, global homographies, and sparse correspondences in a versioned `camcomp.json`;
- reloads and incrementally reuses valid cached analysis;
- exposes a UI-independent Rust API and CLI;
- proves direct Tauri integration without a sidecar;
- preserves a feasible, separately gated browser-only path.

Drop 1 does not deliver the complete import UI, side-by-side viewer, blind mode, or scoring UI. The Tauri and browser work in this plan consists of architecture and packaging spikes, not the complete later drops.

## 2. Delivery strategy

Work is divided into three tracks.

### 2.1 Required native track

The native track produces `camcomp-core`, a CLI, `camcomp.json`, and a minimal Tauri integration. It is required for Drop 1.

The highest-risk dependencies are tested in a narrow vertical spike before production architecture is built. If SIFT, homography, runtime, memory, or packaging fails the predefined acceptance criteria, the ADR is reopened before more code depends on the stack.

### 2.2 Browser-only feasibility track

The browser track proves the same core can execute in a Web Worker with bounded memory and browser filesystem fallbacks. It is not required to accept native Drop 1 unless browser-only delivery is promoted to a Drop 1 requirement.

### 2.3 Calibration track

Calibration runs throughout implementation. Thresholds, weights, and gates remain configuration values with versioned provenance. They are not silently adjusted until the initial dataset passes.

## 3. Proposed repository shape

Confirm this layout during project scaffolding:

```text
camcomp/
├── Cargo.toml
├── Cargo.lock
├── mise.toml
├── crates/
│   ├── camcomp-core/
│   │   ├── src/
│   │   └── tests/
│   ├── camcomp-cli/
│   │   └── src/
│   └── camcomp-wasm/          # browser spike; optional for native Drop 1
├── apps/
│   └── desktop/               # minimal Tauri spike, later product UI
├── tools/                     # dev-only; created if the OpenCV contingency is triggered
├── fixtures/
│   ├── synthetic/
│   ├── manifests/
│   └── expected/
├── schema/
│   └── camcomp.schema.json
├── README.md
├── .gitignore
├── REQUIREMENTS.md
├── SPEC.md
├── ADR-DROP1.md
└── PLAN-DROP1.md
```

`tools/` holds development-only tooling — notably the Python/OpenCV reference implementation if the contingency of `ADR-DROP1.md` §5.1 is triggered. It is deliberately outside the Cargo workspace with its own Python environment, so OpenCV cannot leak into the shipped dependency graph. It does not exist until it is needed.

Private camera photos should not be committed by default. Commit synthetic and redistributable fixtures plus a manifest capable of referring to a local private dataset.

The repository is not yet under version control. Initialize git and make an initial commit of these four documents before any code is written, so that threshold and specification changes are reviewable as diffs — which is the entire point of writing the acceptance criteria down before tuning.

## 4. Quality and verification policy

Every phase follows this order:

1. define expected behavior and failure cases;
2. add a failing unit, integration, fixture, or property test;
3. implement the smallest complete behavior;
4. run focused tests;
5. run the workspace verification suite;
6. record fixture metric changes when algorithm behavior intentionally changes.

Public Rust APIs shall have rustdoc. Errors shall be typed and preserve actionable context without exposing sensitive absolute paths in hosted logs.

The project should expose stable `mise` tasks for:

```text
mise run fmt
mise run lint
mise run test
mise run check
mise run test-fixtures
mise run build
mise run check-wasm
```

The underlying initial checks should include:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release
```

WASM and Tauri commands are added when their workspaces exist. CI shall use the same `mise` tasks rather than a separate, divergent command set.

## 5. Phase 0 — Dataset and acceptance contract

### 5.1 Tasks

1. Inventory the real photo sets. Each set follows the same shape: Photos have a preassigned group in their name (g1-, g2-, etc. or gx if to be left undecided). Record per set:
   - paths and formats;
   - expected group membership and expected pair labels;
   - devices and modes;
   - EXIF timestamps, orientation, focal lengths, and camera models;
   - known camera movement, exposure differences, or subject movement;
   - why the negative is a negative (different framing, different subject, adjacent scene), since that is what the algorithm must actually discriminate.
2. Aim for enough sets to span several devices, scenes, and lighting conditions. Collecting more sets is the cheapest way to make the thresholds defensible and should continue in the background throughout the project.
3. Create synthetic fixtures for:
   - identity and known projective transforms;
   - translation, rotation, scale, and mild perspective;
   - exposure and contrast changes;
   - repeated patterns;
   - low-texture images;
   - degenerate and collinear correspondences;
   - mismatched scenes.
4. Define a fixture manifest containing expected pair state: `compatible`, `incompatible`, or `uncertain`.
5. Define initial pre-tuning acceptance measures:
   - expected pair labels;
   - minimum hard gates from the specification;
   - maximum acceptable pointer-mapping error for synthetic transforms;
   - runtime and peak-memory measurements to collect.
6. State the calibration objective before measuring: no false positives on the labelled sets, then maximize recall, residual ambiguity emitted as `uncertain` (`SPEC.md` §9).
7. Do not build an OpenCV reference now; it is the contingency of `ADR-DROP1.md` §5.1.

### 5.2 Tests and evidence

- Validate fixture manifests and ensure every referenced file exists.
- Verify synthetic transforms reproduce known expected coordinates.
- Ensure private fixtures are excluded by `.gitignore`.

### 5.3 Exit criteria

- Several real sets are labelled, plus synthetic uncertain and degenerate cases.
- Expected results are written before the Rust pipeline is tuned.
- The real sets can be accessed by the test runner without committing private photos.

## 6. Phase 1 — Native dependency vertical spike

This phase validates the provisional ADR before production implementation, and is also where the algorithm is first calibrated. There is deliberately no separate reference implementation: per `ADR-DROP1.md` §5.1, one pipeline is built, in Rust, and a Python/OpenCV reference is only created if this phase stalls.

### 6.1 Tasks

1. Build a narrow Rust executable or test harness using pinned candidate dependencies:
   - `image` and `imageproc`;
   - `kamadak-exif`;
   - `sift-features`;
   - `vision-geometry`;
   - `geo`;
   - `sha2`.
2. Decode and orient one representative image.
3. Generate a configurable analysis image, initially 1,200 pixels on the longest edge.
4. Extract capped SIFT keypoints and descriptors.
5. Implement deterministic brute-force two-nearest-neighbour descriptor matching.
6. Apply Lowe ratio filtering and optional mutual matching.
7. Fit a seeded RANSAC homography and retain inlier indices.
8. Warp the overlap and calculate an initial normalized-gradient similarity.
9. Emit every component value and gate outcome per pair, as a table over the labelled sets.
10. Measure native runtime and peak memory with sequential extraction, and per-pair wall time so the pair budget of `SPEC.md` §4.3 is a measurement rather than a round number.

### 6.1bis Calibration

Run the harness over all the real photo sets and the synthetic fixtures, then:

1. tabulate the achieved range of each component on positives and negatives — the near-miss negatives are the interesting column (`SPEC.md` §5.4.1);
2. set the gates and the threshold against the stated objective: no false positives, then maximum recall, remainder `uncertain`;
3. if the weighted composite does not separate the sets but the components visibly do, replace it with a simple rule over the gates and components (`SPEC.md` §10) rather than hunting for weights;
4. commit the calibrated parameter set with the evidence, and update `SPEC.md` where the placeholder values no longer hold.

Timebox this. If the sets cannot be separated and it is unclear whether the algorithm or the young dependencies are at fault, that triggers the Python/OpenCV contingency of `ADR-DROP1.md` §5.1 — do not absorb it silently into ordinary debugging.

### 6.2 Tests

- Descriptor distance and KNN ordering tests, including equal-distance ties.
- Lowe ratio boundary tests.
- Mutual-match tests.
- Homography tests for exact, noisy, outlier, singular, reflected, and degenerate inputs.
- Warp direction and border-mask tests.
- Determinism tests using fixed seeds and stable input ordering.
- Synthetic control points map within the predefined tolerance.

### 6.3 Decision Gate A — Rust CV stack

Accept the provisional Rust decision only if:

- the positive pair in every real set produces geometrically credible matches;
- the same-session near-miss negatives are rejected or marked uncertain in every real set;
- degenerate synthetic cases are rejected or marked uncertain;
- mapped synthetic control points remain within the predefined tolerance;
- native runtime and memory are acceptable on representative hardware;
- diagnostics required by `SPEC.md` are accessible without leaking dependency-specific types;
- a calibrated parameter set exists and is justified by the measured component ranges;
- all selected dependency licenses are acceptable.

If this gate fails, evaluate in order:

1. parameter or adapter corrections that do not overfit fixtures;
2. a Python/OpenCV reference run on the same sets, to establish whether the algorithm or the dependencies are at fault (`ADR-DROP1.md` §5.1);
3. OpenCV through the `opencv` Rust bindings behind the same traits, for native-only delivery (ADR Option 3.4.1) — this preserves the entire architecture and costs one adapter, so it precedes any option that changes the shape of the system;
4. a replaceable alternative implementation such as `purecv` after license review;
5. Python/OpenCV as the ADR fallback.

Do not continue into a broad production implementation while Gate A is unresolved.

## 7. Phase 2 — Project and workspace foundation

### 7.1 Tasks

1. Initialize the Rust workspace and pin an MSRV compatible with selected dependencies.
2. Add `mise.toml` with pinned Rust, Node/Bun where needed, and verification tasks.
3. Add `README.md`, `.gitignore`, dependency policy, and fixture instructions.
4. Create `camcomp-core` and `camcomp-cli` crates.
5. Establish module boundaries:
   - `domain`;
   - `schema`;
   - `metadata`;
   - `image`;
   - `features`;
   - `matching`;
   - `registration`;
   - `metrics`;
   - `grouping`;
   - `cache`;
   - `pipeline`.
6. Define camcomp-owned traits:
   - `FeatureExtractor`;
   - `DescriptorMatcher`;
   - `HomographyEstimator`;
   - `ImageWarper`;
   - `MetadataReader`.
7. Define typed errors, progress events, cancellation checks, and deterministic ordering rules.
8. Configure formatting, Clippy, tests, release builds, dependency/license auditing, and CI.

### 7.2 Tests

- Workspace smoke test.
- Trait adapter compilation tests.
- Error serialization tests for adapter boundaries.
- Cancellation test using a deterministic fake stage.
- No-dependency-type leakage tests through public API signatures where practical.

### 7.3 Exit criteria

- `mise run check` performs formatting, linting, tests, and build checks.
- Core compiles without Tauri.
- CLI depends on core, not the reverse.
- Dependency versions are locked and no OpenCV runtime is present.

## 8. Phase 3 — Domain model and `camcomp.json` contract

### 8.1 Tasks

1. Define camcomp-owned types for:
   - source identity and fingerprint;
   - metadata and timestamp source;
   - configuration and device identity;
   - session;
   - pair state and diagnostics;
   - metrics and hard-gate outcomes;
   - group, anchor, and membership;
   - normalized homography;
   - sparse correspondence;
   - manual override;
   - algorithm version and parameters;
   - complete analysis document.
2. Distinguish content identity from current source path so rename reuse is possible.
3. Define coordinate conventions:
   - oriented image only;
   - normalized `[0, 1] × [0, 1]` coordinates;
   - row-major `3 × 3` homographies;
   - explicit `photo-to-anchor` direction.
4. Generate `schema/camcomp.schema.json` from Rust types.
5. Implement schema-major-version validation and tolerant unknown-field reading.
6. Define deterministic serialization ordering where arrays represent stable output.
7. Add algorithm and parameter provenance, including the `0.80` threshold and metric weights.

### 8.2 Tests

- JSON round-trip fixture.
- Generated-schema validation.
- Unknown-field compatibility.
- Unsupported-major-version rejection.
- Non-finite number rejection before serialization.
- Homography matrix ordering and direction test.
- Normalized-coordinate conversion and inverse round-trip tests.
- Manual override preservation test.

### 8.3 Exit criteria

- The JSON schema represents all required automatic evidence and manual overrides.
- No OpenCV, `image`, `nalgebra`, `ndarray`, `purecv`, or Tauri type appears in the public schema.
- A checked-in golden document can be loaded and written without semantic changes.

## 9. Phase 4 — Source inspection, fingerprints, and metadata

### 9.1 Tasks

1. Discover supported JPEG/PNG inputs deterministically.
2. Reject unsupported formats, oversized dimensions, and malformed files with typed errors.
3. Stream SHA-256 rather than loading the compressed file solely for hashing.
4. Extract size, modification time, relative path, and content digest.
5. Extract standard EXIF:
   - capture timestamp and source;
   - orientation;
   - make and model;
   - lens model;
   - physical focal length;
   - 35 mm equivalent focal length;
   - dimensions;
   - configured mode tags where available.
6. Apply optional per-device clock offsets without altering original metadata.
7. Represent missing or ambiguous timestamps and focal metadata explicitly.
8. Normalize camera/configuration identity using documented rules while retaining raw values.

### 9.2 Tests

- EXIF orientation cases 1–8.
- Timestamp with, without, and with ambiguous timezone data.
- Device clock offset application.
- Rational focal-length parsing.
- Missing 35 mm equivalent focal length.
- Unicode and non-ASCII paths.
- Rename with identical content digest.
- Same path with changed content invalidation.
- Corrupt, oversized, and unsupported image handling.

### 9.3 Exit criteria

- Representative device files produce reviewed metadata.
- Missing metadata does not cause an implicit false match or panic.
- Fingerprints support safe cache invalidation and rename-aware reuse.

## 10. Phase 5 — Sessions and candidate generation

### 10.1 Tasks

1. Sort photos by corrected capture time with deterministic tie-breaking.
2. Implement the 30-second sliding-gap session rule.
3. Generate non-consecutive candidate pairs within each session.
4. Apply 35 mm equivalent focal compatibility relative to a prospective anchor.
5. Mark focal compatibility uncertain when equivalent field-of-view information is unavailable.
6. Avoid duplicate pair analysis through canonical pair IDs.
7. Report photos excluded by missing timestamps for manual handling.
8. Implement the per-session pair budget of `SPEC.md` §4.3 as a guard against pathological bursts, spending it in increasing capture-time distance, and record skipped pairs as `uncertain` with reason `budget`. No prescreen: with a handful of photos per session, every pair is affordable.
9. Expose a CLI/API way to force analysis of a specific skipped pair.

### 10.2 Tests

- Exactly 30 seconds remains in one session; over 30 seconds starts another.
- A chain of sub-30-second gaps may produce a session longer than 30 seconds.
- Non-consecutive photos remain candidates.
- Focal difference at, below, and above `0.15`, and symmetry of the predicate under argument order.
- Different physical focal lengths are not compared across devices without conversion.
- Stable output under equal timestamps and input reordering, using the documented tie-break.
- A pathologically large session is capped at the pair budget, spends it on the temporally closest pairs, and marks the remainder `uncertain`/`budget`.

### 10.3 Exit criteria

- Candidate generation is deterministic and independently testable.
- Expensive visual analysis is never run for a known-incompatible pair.
- Uncertain metadata remains visible to later grouping and manual correction.
- The number of pairs actually analyzed is bounded and reported, so analysis time is predictable from the input.

## 11. Phase 6 — Production image and feature pipeline

### 11.1 Tasks

1. Decode only validated supported inputs.
2. Apply EXIF orientation before all coordinate generation.
3. Produce a configurable aspect-preserving analysis image.
4. Implement the `sift-features` adapter with a configurable feature cap.
5. Convert crate output immediately into camcomp-owned keypoint and descriptor types.
6. Cache feature results once per photo using the source fingerprint, detector version, and detector parameters.
7. Implement sequential extraction first; add bounded Rayon concurrency only after profiling.
8. Release decoded and pyramid buffers before processing the next image where possible.
9. Record feature count, extraction duration, analysis dimensions, and non-sensitive diagnostics.

### 11.2 Tests

- Orientation is applied exactly once.
- Original, oriented, and analysis dimensions are retained correctly.
- Analysis resizing preserves aspect ratio.
- Feature-cache hit and miss behavior.
- Detector parameter or algorithm-version change invalidates the feature cache.
- Low-texture fixture produces a bounded uncertain result.
- Sequential extraction does not retain prior photo buffers.

### 11.3 Exit criteria

- Each photo is decoded and feature-extracted at most once per valid cache key.
- Feature output is independent from filesystem iteration order.
- Memory and runtime measurements are captured for representative image sizes.

## 12. Phase 7 — Pair registration and geometric gates

### 12.1 Tasks

1. Implement deterministic brute-force KNN matching over SIFT descriptors.
2. Apply configurable Lowe ratio filtering.
3. Apply optional forward/reverse mutual filtering.
4. Shuffle or order correspondence input deterministically for robust fitting.
5. Estimate homography with a fixed persisted RANSAC seed and configurable pixel threshold.
6. Recompute diagnostics from the fitted transform:
   - inlier count and ratio;
   - per-inlier reprojection error;
   - median reprojection error;
   - grid occupancy in both images;
   - convex/spatial feature coverage;
   - determinant, invertibility, reflection, scale, and perspective plausibility;
   - transformed-corner overlap.
7. Classify hard-gate outcomes separately from the composite score.
8. Return `uncertain` for insufficient low-texture evidence.
9. Retain a spatially distributed bounded set of 100–300 inlier correspondences.
10. Convert thumbnail-space homography and points to normalized oriented coordinates.

### 12.2 Tests

- Match tie and stable-order behavior.
- Minimum 25-inlier gate.
- Five-cell `4 × 4` occupancy gate in both images.
- Minimum `0.60` geometric-overlap gate.
- Small localized common object fails spatial coverage.
- Singular, reflected, implausibly scaled, and extreme-perspective transforms fail.
- Correct transform direction from photo to anchor.
- Normalized homography remains valid at full resolution.
- Spatial correspondence sampling favors coverage over descriptor-distance concentration.

### 12.3 Exit criteria

- Every rejection names the failed gate and retains useful diagnostics.
- A high appearance score cannot bypass failed geometry.
- Persisted alignment reproduces synthetic mappings within the agreed tolerance.

## 13. Phase 8 — Metrics and compatibility score

### 13.1 Tasks

1. Calculate geometric overlap over valid transformed polygons.
2. Calculate feature coverage from inlier distribution.
3. Calculate inlier confidence from count, ratio, and reprojection quality.
4. Warp only the valid overlap region and mask invalid border pixels.
5. Calculate aligned appearance similarity from normalized gradients or edges.
6. Normalize each component into `[0, 1]` with documented behavior.
7. Implement the initial weighted score:

```text
0.35 × geometricOverlap
+ 0.25 × featureCoverage
+ 0.20 × inlierConfidence
+ 0.20 × alignedAppearanceSimilarity
```

8. Accept a pair only when all hard gates pass and score is at least `0.80`.
9. Persist raw diagnostics, normalized components, weights, threshold, and algorithm version.
10. Keep post-registration SSIM optional and diagnostic-only.

### 13.2 Tests

- Component normalization boundaries and non-finite inputs.
- Exact weighted-score fixtures.
- Score exactly `0.80` is accepted after gates.
- Score above `0.80` is rejected when a hard gate fails.
- Exposure/contrast changes retain useful gradient similarity.
- Border pixels and non-overlap never inflate similarity.
- Repeated execution produces identical scores within documented float tolerances.

### 13.3 Exit criteria

- Pair results are explainable from persisted metrics and gates.
- Threshold changes require an algorithm-parameter change and trigger affected cache invalidation.
- Initial fixtures match their predeclared expected states without special-case filenames or devices.

## 14. Phase 9 — Group construction and manual overrides

### 14.1 Tasks

1. Build a pair-evidence graph without equating connected components to groups.
2. Select an anchor based on strong, spatially distributed compatibility with the useful candidate set.
3. Add members compatible with the anchor and focal tolerance.
4. Check known non-anchor pair evidence for disagreement.
5. Split or flag groups with substantial internal inconsistency.
6. Prefer at most one photo per configuration unless overridden.
7. Keep anchor selection stable across cache reloads.
8. Implement manual operations in the core API:
   - add/remove photo;
   - split/merge group;
   - select another anchor;
   - allow duplicate configuration membership explicitly.
9. Recompute or compose alignment data after anchor changes and record provenance.
10. Preserve automatic evidence separately from manual decisions.
11. Emit every ungrouped photo with a machine-readable reason; assert that the union of grouped and ungrouped photo IDs equals the input set.
12. Accept manual memberships that contradict or lack automatic evidence, flag them, and compute alignment on demand for them.

### 14.2 Tests

- `A–B` and `B–C` compatible but `A–C` incompatible does not become one unchecked connected component.
- Group membership is stable under input permutation.
- One-configuration preference.
- Anchor stability after reload.
- Split, merge, add, remove, and re-anchor overrides.
- Overrides survive automatic re-analysis of unaffected pairs.
- Invalid overrides return actionable errors without corrupting the document.
- No input photo is missing from both the groups and the ungrouped list.
- A manually added, never-analyzed member is retained and flagged rather than dropped on the next analysis.

### 14.3 Exit criteria

- Automatic groups satisfy anchor and consistency rules.
- All human adjustments required by the specification are representable before a full UI exists.
- Re-analysis never silently discards manual overrides.

## 15. Phase 10 — Cache, incremental analysis, and CLI

### 15.1 Tasks

1. Implement layered caches for metadata, features, pair results, and groups.
2. Key each layer by the minimum relevant fingerprints, algorithm versions, and parameters.
3. Load an existing `camcomp.json` when explicitly supplied or found by a folder adapter.
4. Invalidate only affected photo, pair, and group results after additions, removals, or changes.
5. Write JSON atomically using a temporary sibling and rename where supported.
6. Recover cleanly from interrupted or incomplete analysis.
7. Expose CLI operations for:
   - inspect;
   - analyze;
   - validate JSON;
   - print pair/group diagnostics;
   - apply or import manual overrides;
   - force selected cache layers to refresh.
8. Emit machine-readable progress and a concise human-readable summary.

### 15.2 Tests

- Complete cold analysis and warm reload.
- Rename reuse through content digest.
- Changed file invalidates only its dependent pairs/groups.
- Parameter and algorithm-version invalidation.
- Added and removed photos.
- Interrupted write leaves the previous valid JSON intact.
- Invalid/corrupt JSON fails without modifying source photos.
- CLI exit codes and structured output.

### 15.3 Exit criteria

- Re-running unchanged inputs performs no unnecessary feature or pair analysis.
- `camcomp.json` validates against the generated schema.
- Cache behavior is observable in diagnostics and integration tests.

## 16. Phase 11 — Tauri integration spike

This is an ADR acceptance spike, not the full Drop 2 UI.

### 16.1 Tasks

1. Create a minimal Tauri 2 application that invokes `camcomp-core` directly.
2. Select or provide fixture paths through a narrow command.
3. Run analysis off the UI thread.
4. Send typed progress and completion events.
5. Support cancellation between pipeline stages and within long pair loops.
6. Propagate structured errors to the frontend.
7. Write and reload `camcomp.json` atomically.
8. Package the application on the first target OS.
9. Inspect package size, licenses, and absence of Python/OpenCV sidecars.

### 16.2 Tests

- Rust command tests without launching the complete UI.
- Frontend event/type contract tests.
- Cancellation leaves a valid prior cache.
- UI responsiveness smoke test.
- Packaged application runs analysis on representative fixtures.

### 16.3 Decision Gate B — Native architecture

Accept the ADR when:

- Gate A remains satisfied by the production adapters;
- the package runs without external runtimes;
- progress and cancellation work;
- JSON output round-trips;
- packaging and licenses are acceptable.

## 17. Phase 12 — Browser-only spike

This phase may proceed after Gate A and can run in parallel with lower-risk native work if it does not change shared files concurrently.

### 17.1 Tasks

1. Create `camcomp-wasm` with explicit `getrandom/wasm_js` and no accidental native-only features.
2. Expose only coarse operations across the WASM boundary to avoid copying descriptors repeatedly.
3. Run analysis in one Web Worker without shared-memory threads.
4. Add browser-assisted downscaled decode using `createImageBitmap` and `OffscreenCanvas`, guarded by orientation/color fixtures.
5. Fall back to Rust JPEG/PNG decode where browser decode behavior is unsuitable.
6. Process one image at a time and persist compact features before releasing the pyramid.
7. Implement IndexedDB or OPFS caches.
8. Implement:
   - `showDirectoryPicker()` import and in-place JSON where supported;
   - multi-file input;
   - drag-and-drop;
   - JSON download fallback.
9. Measure 1,200-, 800-, and lower-edge analysis configurations.
10. Test desktop Chrome, Firefox, and Safari where APIs permit.
11. Decide whether mobile browser support is viable or explicitly excluded.
12. Seek an upstream `sift-features` feature-gating change or maintain a reviewed patch to disable unwanted transitive defaults.

### 17.2 Tests

- WASM compile in CI.
- Real browser Playwright tests for supported flows.
- Worker progress, cancellation, and error handling.
- Native/WASM metadata and score comparison on shared fixtures.
- Memory is released before the next photo.
- Directory-handle permission loss and fallback behavior.
- Cache reload without stale source access.
- JSON download/import round trip.

### 17.3 Decision Gate C — Browser support

Browser-only Drop 1 is accepted only if:

- complete fixture analysis runs in a worker without blocking the UI;
- measured peak memory stays within a documented budget;
- native and browser classifications agree within defined tolerances;
- filesystem fallbacks satisfy the reduced browser contract;
- supported browser and format boundaries are documented.

Failure at Gate C does not block native Drop 1.

## 18. Phase 13 — Calibration, hardening, and release verification

### 18.1 Tasks

1. Expand the labelled real sets beyond those available at Phase 1, keeping the same two-plus-one shape.
2. Add difficult positives and negatives:
   - exposure and white-balance shifts;
   - HDR and sharpening differences;
   - repeated architecture;
   - adjacent scenes;
   - low texture;
   - parallax;
   - moving people, foliage, or vehicles;
   - orientation changes;
   - timestamp offsets.
3. Record false positives, false negatives, and uncertain outcomes.
4. Tune weights and gates as one versioned parameter set.
5. Prefer an uncertain result to ungrounded acceptance for sparse evidence.
6. Profile hot paths before optimization.
7. Audit panics, unchecked allocations, image limits, path handling, and malformed JSON/images.
8. Generate license inventory and dependency advisories.
9. Update `SPEC.md` and the ADR if implemented behavior or accepted architecture differs.
10. Freeze golden outputs for the accepted algorithm version.

### 18.2 Full verification

Run:

```text
mise run fmt
mise run lint
mise run test
mise run test-fixtures
mise run build
mise run check-wasm      # when browser spike is enabled
```

Also verify:

- clean-room cold analysis;
- warm cache reload;
- incremental update;
- manual override persistence;
- normalized pointer mapping;
- Tauri packaged smoke test;
- generated JSON Schema validation;
- no source photo modification;
- no secrets or private fixture paths in committed output.

### 18.3 Exit criteria

- All Drop 1 definition-of-done items pass.
- Thresholds and expected fixture changes are reviewed.
- Remaining uncertain cases and unsupported formats are documented.
- The ADR status is updated from Proposed to Accepted, or the fallback decision is documented.

## 19. Drop 1 definition of done

> **Status: see `IMPLEM-DROP1.md` §7.4 for the full met/not-met accounting with
> evidence.** Checkboxes below are marked from that section; a deviation gets a
> pointer to the section that justifies it rather than a bare unchecked box.

### 19.1 Functional

- [x] Inputs are inspected without modification.
- [x] Capture time, orientation, device, focal, dimensions, and mode metadata are represented.
- [x] Per-device clock offsets are supported.
- [x] Sessions use a sliding-gap rule — shipped at 60 s, not 30 s; evidenced deviation, `IMPLEM-DROP1.md` §6.12.
- [x] Focal compatibility is evaluated from 35 mm equivalent values — as an angle-of-view predicate with two tolerance tiers, not a single 0.15 focal-ratio anchor; `IMPLEM-DROP1.md` §6.11 explains why the ratio rule cannot satisfy the labels.
- [x] Non-consecutive photos can be grouped.
- [x] SIFT matching tolerates moderate translation, rotation, scale, perspective, and processing differences.
- [x] Hard geometric gates are evaluated independently from the composite score.
- [x] Pair compatibility requires all gates and score `>= 0.80`.
- [x] Low-evidence cases can be `uncertain`.
- [x] Group construction does not use unchecked connected components.
- [x] Every input photo appears either in a group or in the ungrouped list with a reason.
- [x] Automatic groups and evidence are separate from manual overrides.
- [x] Human add/remove/split/merge/re-anchor operations are representable.
- [ ] Thresholds, weights, and gates are the values calibrated in Phase 1 against the real sets, not the placeholders in the initial specification — kept at spec defaults on purpose; the composite score does not discriminate on the labelled set (`IMPLEM-DROP1.md` §6.3), so `thresholdsProvisional` stays `true`.
- [ ] Every real set is classified correctly — 11 of 13 labelled groups reproduced automatically, 0 false positives (`IMPLEM-DROP1.md` §6.1); the 2 misses are named and each reachable by one override.

### 19.1bis Performance and scale

- [x] Pairs analyzed per session are bounded by an explicit budget; skipped pairs are `uncertain`, visible, and analyzable on demand.
- [ ] A documented reference dataset (order of hundreds of photos) analyzes cold within a stated time — warm reload is near-instant (met), but the largest reference set is six photos, so the hundreds-of-photos cold-time claim is untested; per-pair and per-decode costs are measured for extrapolation (`IMPLEM-DROP1.md` §6.10, §7.4).
- [ ] Peak memory is measured and documented for the native path, and bounded by an explicit guard — measured (`peakAnalysisPixels`, ~150 MB for 50 MP) but no allocation guard beyond the pixel-count limit is enforced (`IMPLEM-DROP1.md` §7.4).
- [x] Analysis is cancellable and resumable without losing completed work.

### 19.2 Alignment

- [x] Every non-anchor member has a normalized oriented photo-to-anchor homography.
- [x] Transform direction and row-major ordering are schema-defined.
- [x] A bounded spatially distributed set of inlier correspondences is persisted.
- [x] Full-resolution pointer mapping passes synthetic control-point tests.
- [x] Alignment confidence and fallback conditions are persisted.

### 19.3 Cache and schema

- [x] `camcomp.json` has schema, application, and algorithm versions.
- [x] Parameters, weights, gates, diagnostics, sessions, pairs, groups, overrides, and alignments are persisted.
- [x] The generated JSON Schema is checked in.
- [x] Unchanged analysis is reusable.
- [x] Changed inputs invalidate only affected results.
- [x] Unsupported schema-major versions fail clearly.
- [x] Atomic writes protect the previous valid cache.

### 19.4 Architecture and quality

- [x] Core is usable as a Rust library and CLI without Tauri.
- [x] Dependency-specific types do not leak through core public contracts.
- [x] No OpenCV or Python runtime is shipped under the accepted Rust decision.
- [ ] Tauri invokes the core directly and remains responsive — the shell compiles and its command surface is unit-tested, but no signed bundle was produced and the window was not opened in this environment (`IMPLEM-DROP1.md` §7.4, Gate B).
- [x] Progress, cancellation, and typed errors are implemented.
- [ ] Unit, integration, property, fixture, and schema tests pass — all do (`IMPLEM-DROP1.md` §7); packaging tests (Gate B smoke test, Gate C WASM) were not attempted.
- [x] Public APIs are documented.
- [x] Rust formatting and Clippy pass with warnings denied.
- [x] Runtime, memory, supported formats, and known limitations are documented.
- [x] IMPLEM-DROP1.md contains minimal useful implementation notes for handover.

## 20. Issues to resolve before or during Phase 0

1. Location and redistribution status of the real photo sets -> ./test/tstgrp[1-8]
2. Expected positive and negative pair labels for each set.
3. First desktop packaging target: macOS, Windows, or Linux.
4. HEIC handling: concerns on `heic` crate maturity for decoding.
5. Exact device/mode identity rules and available manufacturer tags.
6. Whether browser-only delivery is a release requirement or a non-blocking feasibility target.
7. Native and browser memory budgets.
8. Initialize the git repository and commit the current documents before code.
9. Reference dataset size and the wall-clock target for a cold analysis of it.
