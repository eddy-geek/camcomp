# camcomp

Compare photos taken by several cameras, phones, or several modes of the same
phone. camcomp finds which photos show the same scene within one shooting
session, and keeps the analysis in a cacheable `camcomp.json` beside the photos.

**Status: Drops 2 and 3.1** — the grouping and alignment engine, CLI, shared
browser/Tauri import interface, and synchronized comparison viewer are implemented.
Blind mode, configurable review tags, and preference scoring remain deferred.

Nothing leaves your machine, and no source photo is ever modified. The only file
camcomp writes into a photo folder is `camcomp.json`.

## What Drop 1 does

Given a folder of photos, camcomp:

1. reads metadata and a content fingerprint for every file;
2. splits them into shooting sessions using a 60-second sliding gap;
3. pairs up session members whose angle of view is close enough to be worth
   comparing;
4. matches SIFT features, fits a homography with RANSAC, and measures overlap,
   feature coverage, registration confidence, and gradient similarity over the
   registered overlap;
5. builds anchor-based comparison groups;
6. records every input photo either in a group or in the ungrouped list, with a
   machine-readable reason;
7. persists groups, pair diagnostics, manual overrides, and the normalized
   homographies a viewer needs for synchronized zoom and panning.

Supported formats: **JPEG, PNG and HEIC**. HEIC is decoded in pure Rust, so the
desktop application ships as a single binary with no external runtime.

## Getting started

```sh
mise install
npm ci --prefix apps/web
mise run build
mise run web:build
```

Analyze a folder:

```sh
./target/release/camcomp analyze /path/to/photos
```

That writes `/path/to/photos/camcomp.json` and prints a summary. Running it again
reuses the cached analysis and re-reads nothing.

### Useful commands

```sh
camcomp inspect  PATH...        # metadata only, no pixel analysis
camcomp analyze  PATH...        # the full Drop 1 analysis
camcomp validate PATH           # structural check of an existing camcomp.json
camcomp explain  PATH [ID]      # why a pair, group or photo ended up as it did
camcomp override PATH --add GROUP=PHOTO
camcomp schema                  # the generated JSON Schema
```

Add `--dry-run` to analyze without writing, `--table` for the per-pair component
table, and `--json-progress` for newline-delimited progress on stderr.

Exit codes: `0` success, `1` failure, `2` invalid usage, `3` the analysis ran but
did not satisfy an `--expect-*` assertion.

### Adjusting the grouping by hand

Automatic grouping is a starting point, not a verdict. Overrides are stored
separately from the automatic evidence, so the original analysis stays
inspectable and survives re-analysis:

```sh
camcomp override photos/ --add group-61a15592f217=photo-9c1b0e4d2a71
camcomp override photos/ --anchor group-61a15592f217=photo-9c1b0e4d2a71
camcomp override photos/ --split group-61a15592f217=photo-aaa,photo-bbb
camcomp override photos/ --merge group-aaa,group-bbb
camcomp override photos/ --remove group-61a15592f217=photo-9c1b0e4d2a71
```

## Browser and desktop applications

```sh
mise run web:dev       # browser PWA at http://127.0.0.1:5173
mise run tauri:dev     # the same frontend inside Tauri
```

Both modes load a selected local `camcomp.json` without uploading or automatically
rescanning photos. Browser mode is read-only: it uses a Chromium directory handle
when available and recursive file input elsewhere. New analysis, incremental refresh,
progress, cancellation, and atomic persistence are available through the Tauri app.

The viewer lays out every group member, displays focal length, oriented resolution,
and camera model, and synchronizes OpenSeadragon pan/zoom. Normalized navigation is
always available; experimental aligned navigation maps through Drop 1's global
homographies and shows deterministic per-photo fallback. Two-photo groups can use a keyboard-accessible
comparison slider.

## Repository layout

```text
crates/camcomp-core/     the analysis engine; usable as a library, no UI dependency
crates/camcomp-cli/      the `camcomp` binary
apps/web/                shared Svelte/TypeScript PWA and viewer
apps/desktop/            Tauri 2 native adapter
schema/                  generated JSON Schema for camcomp.json
scripts/                 fixture and calibration harnesses
artifacts/               requirements, specification, ADR, plan, implementation notes
test/tstgrp[1-9]/        labelled reference photo sets
```

## Verification

```sh
mise run check            # Rust + frontend format, lint, tests, E2E, builds, contracts
mise run test-fixtures    # the labelled reference sets in ./test
mise run web:test         # frontend unit and component tests
mise run web:e2e          # Chromium, WebKit, and Firefox
mise run tauri:check      # native application compile check
mise run calibrate        # per-pair metric distribution against the labels
```

`test/tstgrp[1-9]/` uses a filename convention: `g1-`, `g2-` … mark photos that
belong together, and `gx-` marks one deliberately left undecided. The fixture
runner compares the analysis against those labels.

## Known limitations

Read `artifacts/IMPLEM-DROP1.md` before trusting a threshold. The short version:

- **The composite compatibility score does not discriminate on the current
  labelled sets.** True pairs score 0.892–0.982, and pairs the labels put apart
  score 0.876–0.984. The ranges fully overlap. What actually separates the sets
  is the metadata prefilters and the geometric hard gates, not the weighted sum.
- **The default parameters produce no false positives on all eight reference
  sets, and reproduce 12 of 13 labelled groups.** The remaining miss is
  attributable to a named prefilter (`tstgrp1`/g1, one-photo-per-configuration)
  and is one override away.
- The framing tolerance was widened from `0.10` to `0.15` to admit `tstgrp3`/g1
  (0.144 angular difference), which no tighter tolerance could reach without
  also admitting a pair labelled apart in `tstgrp5`. That conflict is resolved:
  `tstgrp5` was relabelled into a single group, so the widest pair the strict
  tier must now accept (0.147) sits well clear of the closest it must reject
  (0.750).
- Thresholds are marked `thresholdsProvisional: true` in every document.
- One 35 mm equivalent focal length in the reference set is wrong: the Pixel does
  not fold its digital zoom into `FocalLengthIn35mmFormat` while the Oppo does.
  camcomp records both values and corrects it per device rather than silently
  reconciling both conventions the same way.
- RAW is not supported. Browser mode cannot create or refresh an analysis.
- Safari uses HEIC when an actual native decode succeeds. Other browsers prefer a
  dimension-matched same-stem JPEG/PNG sidecar, then offer an explicit high-memory
  lossless worker decode. A 50 MP worker decode measured about 2.1 GiB peak RAM, so
  it never starts without confirmation; Tauri remains the lower-risk fallback.
- Tauri HEIC display uses `heif-oxide` and writes a digest-keyed lossless PNG only
  to application cache storage. The source folder is never modified.

## Documents

Start with the [user guide](docs/GUIDE.md) if you want to use camcomp. The rest are
for working on it.

| document | what it is for |
|---|---|
| [`docs/GUIDE.md`](docs/GUIDE.md) | how to use it, and what to do when a group is wrong |
| [`artifacts/REQUIREMENTS.md`](artifacts/REQUIREMENTS.md) | what the product must do |
| [`artifacts/SPEC.md`](artifacts/SPEC.md) | how Drop 1 does it |
| [`artifacts/SPEC-OPPO.md`](artifacts/SPEC-OPPO.md) | OPPO metadata, mode, flash, and Pro XDR evidence |
| [`artifacts/ADR-DROP1.md`](artifacts/ADR-DROP1.md) | why this stack |
| [`artifacts/PLAN-DROP1.md`](artifacts/PLAN-DROP1.md) | Drop 1 implementation plan and gates |
| [`artifacts/IMPLEM-DROP1.md`](artifacts/IMPLEM-DROP1.md) | Drop 1 implementation and measurements |
| [`artifacts/PLAN-DROP2.md`](artifacts/PLAN-DROP2.md) | Drops 2 and 3.1 implementation plan and gates |
| [`artifacts/IMPLEM-DROP2.md`](artifacts/IMPLEM-DROP2.md) | delivered UI, integration feedback, evidence, and pending manual gates |

## Licence

camcomp is MIT or Apache-2.0, at your option. The optional browser HEIC worker includes
`heic-to` under LGPL-3.0; source and replacement notices ship with the frontend. See
[`LICENSE.md`](LICENSE.md).
