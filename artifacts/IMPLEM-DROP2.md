# Drops 2 and 3.1 Implementation Notes

- **Status:** Drops 2 and 3.1 code delivery complete. The combined Rust/frontend
  checks, three-browser E2E matrix, production builds, schema/contract freshness, and
  Drop 1 reference suites pass. Real-browser, cross-host, 50 MP, screen-reader, and
  landmark decision-gate evidence remains explicitly pending (§6).
- **Date:** 2026-08-26
- **Updated:** 2026-08-31 — stable full-viewport comparison, pixel inspection, and
  bounded native HEIC decoding
- **Sources:** `PLAN-DROP2.md`, `ADR-DROP3.md`, the generated schema, and the
  completed Drop 1 Rust contracts
- **Purpose:** Record what was actually implemented, integration feedback discovered
  while implementing it, measured evidence, gate status, and honest limitations.

## 1. Delivered architecture

### 1.1 Shared frontend

`apps/web` is a static Svelte 5, TypeScript, and Vite application shared by browser
and Tauri. It includes:

- strict TypeScript, Svelte diagnostics, ESLint with warnings denied, Prettier,
  Vitest/Testing Library, and Playwright projects for Chromium, WebKit, and Firefox;
- a generated app-shell service worker and install manifest;
- feature-detected browser and Tauri adapters behind camcomp-owned interfaces;
- an explicit import state machine with stale-run isolation and typed failures;
- schema-derived TypeScript plus build-time-generated AJV validation;
- immutable album indexes;
- browser directory-handle, recursive-input, file, and drag/drop acquisition;
- native dialogs, native drops, cache load, analysis, progress, cancellation,
  refresh, and source resolution;
- OpenSeadragon simple-image viewers behind `ViewerAdapter`;
- adaptive 2/3/4/larger layouts, fixed EXIF presentation, normalized navigation,
  experimental global-homography navigation, and a local two-photo clipping slider.

OpenSeadragon is loaded as a separate lazy chunk so the import shell does not load the
viewer before a group needs it.

### 1.2 Native bridge

The Tauri bridge now preflights every native analysis selection through Drop 1
`pipeline::discover`, uses the actual common source root, rejects filesystem and
volume roots, and shows only safe root/destination labels. It no longer writes beside
whichever explicit file happened to be first.

Native display resolution is scoped to the currently acquired root. Relative paths,
parent traversal, absolute injection, non-portable separators, and symlink escapes are
rejected. JPEG/PNG sources are exposed through Tauri's dynamically extended asset
scope. HEIC uses a dimension-matching same-stem sidecar first, then a size-guarded
`heif-oxide` decode to a digest-keyed lossless PNG in application cache. At most two
distinct cache misses run concurrently; requests for the same digest serialize and reuse
the committed cache entry. No full image is sent through JSON IPC and no display
derivative is written beside the photos.

Run completion is ID-aware: completion of a cancelled stale run can no longer clear
the cancellation token of the active replacement run.

## 2. Integration feedback and deviations

### 2.1 The checked schema was stale

The checked `schema/camcomp.schema.json` still described the old `focalTolerance`
field while current Drop 1 Rust types persist field-of-view tolerances and the focal
zoom policy. The Rust schema was regenerated before generating TypeScript. Contract
freshness now checks both TypeScript types and the standalone validator.

Feedback: schema freshness must remain in the combined check; otherwise browser reload
can reject documents written by the same repository revision.

### 2.2 Runtime AJV compilation violates the required CSP

The first production browser run failed before mounting because normal AJV compilation
uses `new Function`, correctly blocked by `script-src 'self'`. Adding `unsafe-eval`
would have violated the plan. Validation is now compiled at build time into a checked
standalone module. Custom Rust numeric formats are converted during generation to
integer/minimum/maximum schema constraints.

Feedback: browser contract generation must cover validator code, not only interfaces.
A type-only freshness check would not catch this production-only CSP failure.

### 2.3 Rust semantic validation needed strengthening

The existing Rust `AnalysisDocument::validate` checked the group partition and inverse
homographies but did not reject every duplicate ID, normalized duplicate path, bad
configuration/session/pair reference, non-canonical pair ID, duplicate group member,
invalid group order, or invalid alignment member that the browser contract must reject.
The Rust validator now enforces those invariants and NFC-normalizes document paths for
duplicate detection.

Feedback: JSON Schema is necessary but not sufficient; browser and Rust semantic
fixtures must continue to evolve together.

### 2.4 Tauri project discovery does not follow an external config first

Launching Tauri from `apps/web` with a sibling `src-tauri` config failed before
`--config` was applied because CLI project discovery runs first. A small Node launcher
now starts the pinned Tauri CLI from the repository root, where the desktop project is
discoverable, while Tauri runs frontend commands in `apps/web`.

Feedback: keep `mise run tauri:dev` as the documented entry point instead of relying on
the current shell directory or `npx` downloading an unpinned CLI.

### 2.5 Native source scope must not be a broad static permission

No `$HOME/**` or equivalent filesystem permission was added. Dialog-selected paths and
each resolved display file enter Tauri's runtime asset scope, while command-side access
is separately constrained by `SourceScope` and canonical path checks.

### 2.6 Refresh must reuse, not invalidate, Drop 1 work

An early frontend implementation sent `reuseCache: false` for Refresh, which contradicted
the plan. Refresh now explicitly sends `reuseCache: true`; cache counters are shown from
the returned run statistics.

### 2.7 Analysis needs an explicit destination confirmation state

Native photos without a cache no longer begin analysis immediately. The state machine
first presents the safe preflight destination, then starts only from **Run analysis**.
Browser photos remain selected but enter `native-analysis-required` without claiming a
save or successful analysis.

### 2.8 Viewer navigation must be one-way and survive remounts

OpenSeadragon animation and resize events previously let programmatic follower updates
become new leader input; aspect-ratio reconciliation then compounded the shared span and
made every image shrink repeatedly. Adapters now publish only explicit user gestures or
toolbar zoom. Programmatic application is silent, removed leaders are cleared, and
remounted grid/slider viewers receive the canonical center and scale.

Navigation distinguishes normalized span from physical pixel scale. **Fit all** restores
the whole-image view; **100%** and **200%** preserve the current center and use
`devicePixelRatio` as the best browser/Tauri estimate of physical screen pixels. Each
viewer derives its own span so mixed image resolutions remain at the requested scale.

### 2.9 WebKit offline navigation is a Playwright limitation in this environment

The pinned WebKit runner reports an internal error for top-level navigation after
`browserContext.setOffline(true)`, even with an active controlling service worker.
The automated WebKit check therefore inspects the actual Cache Storage response for
`index.html`; Chromium and Firefox perform a full offline navigation. A real Safari
manual offline reload remains an acceptance item and is not claimed by the automated
WebKit result.

### 2.10 CSP and progress contract feedback

AJV runtime evaluation was removed, and scripts run under `script-src 'self'` without
`unsafe-eval`. `style-src 'unsafe-inline'` remains deliberately scoped to styles because
OpenSeadragon positions runtime canvas elements with inline geometry and the local slider
updates its clip/divider geometry at runtime. No inline or remote script permission is
present.

Rust progress event fields now serialize in camelCase, including `elapsedMs`; the Tauri
adapter no longer carries a snake_case compatibility branch. Feedback: event payloads
need the same explicit field-renaming policy as command request/response structs.

### 2.11 Generated frontend ordering matters to native checks

The obsolete tracked Drop 1 files under `apps/desktop/dist` were removed after explicit
approval; Tauri now consumes only generated `apps/web/dist`. An initial parallel
`mise run check` exposed a race where Vite's `emptyOutDir` briefly removed `sw.js` while
Tauri's `generate_context!` read the frontend. Rust lint/test/build and `tauri:check` now
depend on `web:build`, making the combined check deterministic.

### 2.12 Comparison chrome must not resize or obstruct the viewer

The ready album now fills the webview with gapless image cells. App, group, photo, EXIF,
and slider chrome are overlays that auto-hide after pointer inactivity; the group sidebar
collapses to a left-edge activation rail and expands on hover or keyboard focus. Hidden
overlays do not participate in layout or intercept image gestures. Focus retained by an
OpenSeadragon canvas does not keep that photo's informational chrome visible; only focus
inside an actual control can override inactivity hiding.

### 2.13 Native HEIC latency was development-profile work, not a decoder regression

A Tauri development launch appeared to process group members sequentially at roughly
40 seconds per photo. The frontend already mounted each `PhotoFrame` independently and
issued its native source-resolution command concurrently, but `SourceScope` held one
global mutex across cache validation, HEIC decode, and PNG encoding. More importantly,
the native development profile left the pure-Rust pixel and compression loops at
`opt-level=0`.

A standalone harness timed the exact decode-to-cache stages for the 8192×6144,
14 MiB `tstgrp1/g1-IMG20260822113555.heic` on an Apple M4 Pro:

| build profile | read | `heif-oxide` decode | lossless PNG | total |
|---|---:|---:|---:|---:|
| unoptimized development | 3.8 ms | 3.40 s | 16.00 s | 19.40 s |
| release | 3.7 ms | 289 ms | 264 ms | 557 ms |
| development, `opt-level=2` | 3.7 ms | 346 ms | 550 ms | 900 ms |

The generated PNG was 85 MiB and the measured peak process working set was about
690 MiB for one 50 MP conversion. This makes unconstrained group-wide parallelism unsafe.
The workspace development profile now uses `opt-level=2`, retaining debug information
and assertions while removing the pathological pixel-loop cost. The global mutex became
a two-permit, digest-aware gate: different cache misses may proceed together, while two
requests for the same source cannot race the shared temporary/cache path. Cached entries
are checked before waiting and again after admission.

Two 50 MP conversions through the optimized development harness took 1.89 seconds
sequentially and 1.10 seconds when run concurrently. The limit remains two because the
wall-clock improvement otherwise trades directly against hundreds of MiB of additional
working memory per admitted image. A unit test holds two distinct permits, verifies a
third waits, and verifies matching digest keys serialize.

No decoder replacement or GPU path was adopted. `heif-oxide` already parallelizes
independent HEIC grid tiles and row-wise YUV-to-RGB conversion, and the Drop 1 comparison
measured it faster than the SIMD/tile-parallel `heic` crate; that alternative also requires
AGPL-3.0 compliance or a commercial licence. `libheif` wrappers add native or downloaded
binary packaging and LGPL/GPL review without hardware acceleration in the usual
`libde265` configuration. Hardware HEVC still-image decode would require separate
ImageIO/VideoToolbox, Windows Imaging Component, and Linux FFmpeg/VA-API adapters. At
about 0.3 seconds for the measured CPU decode, that platform-specific complexity is not
justified.

## 3. Format and viewer decisions

### Gate A — non-native browser HEIC

Native decode is probed against the selected HEIC; when it fails, `.jpg`, `.jpeg`, then
`.png` sidecars in the same relative directory are decoded and accepted only when their
oriented dimensions match. If no sidecar works, a high-memory worker is offered with an
explicit per-photo confirmation; Tauri remains available as the lower-risk fallback.

The browser-libheif stream was revisited with current packages:

| Candidate | Current package | Finding |
|---|---:|---|
| `heic2any` | 0.0.4, MIT, 2.7 MB unpacked | last published March 2023; JPEG/PNG/GIF conversion API and stale browser-support claims; not selected |
| `libheif-js` | 1.19.8, LGPL-3.0, 6.4 MB unpacked | lower-level libheif build; last published June 2025; no advantage over the maintained wrapper for this spike |
| `heic-to` | 1.5.2, LGPL-3.0, 24.4 MB unpacked | maintained wrapper over libheif 1.22.2 with CSP and bitmap paths; selected behind explicit high-memory confirmation |

The `heic-to/csp` spike used a disposable outer worker around its decoder worker,
`ImageBitmap`, lossless `OffscreenCanvas` PNG output, a 256 MiB compressed-input guard,
a 60 MP decoded-pixel guard, and one-at-a-time scheduling. Results on the local macOS
arm64 host with Playwright Chromium 149:

- all 18 Drop 1 HEIC fixtures decoded at the Rust-reported oriented dimensions;
- coverage included 4096×3072, 5888×4416, 8192×6144, 3072×4096, and 6144×8192;
- end-to-end decode/display times were 2.7–7.6 seconds per image;
- the production CSP stayed free of `unsafe-eval`;
- terminating the album during an in-flight 50 MP decode cancelled cleanly;
- one 8192×6144 HEIC plus one 4080×3072 JPEG viewer raised total Chromium RSS from
  292 MiB to a 2,120 MiB peak: **+1,828 MiB** for one decoded HEIC comparison.

Correctness, orientation dimensions, CSP, and cancellation passed, while color-profile
fidelity was not independently reviewed. The initial implementation rejected `heic-to`
because of the 50 MP peak. The product decision was subsequently changed to accept that
cost temporarily, provided decoding never starts silently. Production therefore keeps
native decode and sidecars first, shows the measured 2,120 MiB peak / 1,828 MiB delta,
requires **Decode HEIC anyway**, limits input to 256 MiB, verifies the largest encoded
`ispe` dimensions against the document before allocation, rejects sources above 60 MP,
serializes decodes, and terminates the worker when the album closes. Gate A is recorded as a deliberate
high-memory exception rather than represented as an unqualified pass.

`heic-to` and its embedded libheif build add LGPL-3.0 obligations. The shipped static
third-party notice identifies the exact version, source, license terms, and replacement
path. Tauri continues to use the MIT/Apache-2.0 `heif-oxide` decoder and lossless PNG.

### Gate B — OpenSeadragon simple sources

Simple image sources are implemented with explicit viewer, event, object-URL, and
selection cleanup. The pyramid contingency is absent. Automated source lifecycle and
multi-browser viewer checks do not substitute for the pending representative 50 MP
memory/recovery matrix, so Gate B is not yet recorded as accepted.

### Gate C — global alignment

Synthetic identity, translation, composition through the anchor, invalid denominator,
and per-target fallback tests pass. Real independent landmark probes have not been
recorded, so the UI labels global-aligned mode **Experimental** and normalized mode
remains available at all times.

### Gate D — comparison slider

A camcomp-owned clipping shell was selected instead of another runtime dependency. The
underlying OpenSeadragon leader receives pan/zoom input, the clipped peer follows shared
navigation, and the divider is a semantic range input. Multi-browser tests cover
mounting, keyboard movement, grid/slider round trips, and aligned-mode fallback.

## 4. Dependency and privacy review

Pinned direct frontend runtime licences:

| Dependency | Version | Licence |
|---|---:|---|
| Svelte | 5.56.8 | MIT |
| OpenSeadragon | 6.1.0 | BSD-3-Clause |
| AJV | 8.20.0 | MIT; build-time validator generation only |
| Tauri JavaScript API | 2.11.1 | Apache-2.0 OR MIT |
| Tauri dialog plugin | 2.7.2 | MIT OR Apache-2.0 |
| Vite PWA plugin | 1.3.0 | MIT |
| heic-to | 1.5.2 | LGPL-3.0; separate user-confirmed browser worker |

`npm audit --audit-level=high` currently reports zero vulnerabilities. No remote viewer
or decoder script is loaded; the pinned worker is built and served locally.

The service worker precaches generated application assets only and has no runtime cache
route. Browser tests inspect cache URLs and intercept external requests. Selected
photos, filenames, metadata, JSON, object URLs, and decoded images are not application
shell entries.

## 5. Verification evidence

- `mise run check`: passed, including Rust formatting, Clippy with warnings denied,
  240 core tests, 15 Tauri command tests, doc tests, 9 release reference-set tests,
  release builds, schema freshness, frontend formatting/lint/typecheck, 42
  unit/component tests, 17 passing Playwright tests plus four deliberate cross-browser
  skips for the one-run real-photo cases, PWA build, TypeScript contract/validator
  freshness, and Tauri compile check.
- `mise run test-fixtures`: completed without crashes or false positives; 11 of 13
  labelled groups reproduced, matching the documented Drop 1 baseline. Two of eight
  sets intentionally report incomplete label reproduction under defaults.
- A Playwright Chromium viewer test loads the real full-resolution `tstgrp2/g3` JPEG
  pair, verifies both OpenSeadragon instances, required EXIF presentation, the actual
  direct-pair confidence, aligned mode, and comparison slider. The same test is skipped
  in WebKit/Firefox to avoid tripling real-photo cost; synthetic viewer sanity checks run
  in all three projects.
- A second Chromium real-photo test loads the `tstgrp3` HEIC/JPEG comparison, confirms
  no worker request occurs before the 2,120 MiB warning is accepted, then verifies the
  lossless decode, source warning, dimensions, both viewers, and album cleanup.
- Production build: app shell 294.36 kB / 56.43 kB gzip; lazy OpenSeadragon chunk
  350.65 kB / 88.33 kB gzip; HEIC worker asset 2,996.09 kB; service worker precaches
  12 static entries (3,591.54 KiB).
- `npm audit --audit-level=high`: zero vulnerabilities.
- Tauri development launch reached the running native binary with the shared Vite
  frontend.
- The 2026-08-31 viewer follow-up passes formatting, ESLint, strict Svelte/TypeScript,
  the production PWA build, 54 unit/component tests, and synthetic viewer tests in
  Chromium, WebKit, and Firefox. Browser coverage verifies stable dimensions, gapless
  viewport use, collapsed/revealed chrome, focused-canvas hiding, and device-aware
  100%/200% rendering within one backing pixel per edge.
- The 2026-08-31 native HEIC follow-up passes 242 core tests, 16 Tauri command tests,
  documentation tests, 10 release reference-set tests, workspace Clippy with warnings
  denied, the frontend production build, and the Tauri compile check. Its gate test
  covers the two-permit bound and same-digest serialization.

## 6. Acceptance status requiring real/manual evidence

The following are not silently marked complete by unit tests:

- real Safari, Chromium, and Firefox picker, permission, and drag/drop smoke tests;
- Safari-native HEIC, color-profile fidelity, and representative sidecar/Tauri HEIC visual checks;
- named-hardware 50 MP startup, interaction, peak-memory, and cleanup measurements;
- independent real-scene landmark error measurements for Gate C;
- native cold analysis, cache-only load, refresh, and cancellation through the visible
  product UI on each available host;
- screen-reader acceptance beyond automated roles, labels, focus, and live-region checks;
- Windows and Linux host compile jobs (source-level path tests and the local macOS check
  do not replace those hosts).

No installer, package, signing, notarization, or auto-update claim is made.
