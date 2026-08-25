# camcomp

Compare photos taken by several cameras, phones, or several modes of the same
phone. camcomp finds which photos show the same scene within one shooting
session, and keeps the analysis in a cacheable `camcomp.json` beside the photos.

**Status: Drop 1** — the grouping and alignment engine, a CLI, and a Tauri
integration spike. The side-by-side viewer, blind mode and scoring are Drops 3
and 4.

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
mise install                 # pinned Rust and Node
cargo build --release
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

## Desktop application

```sh
cd apps/desktop && npx tauri dev      # or: npx tauri build
```

The Tauri shell calls the Rust engine directly — no sidecar process, no Python,
no OpenCV. It exists to prove the architecture: analysis runs off the UI thread,
progress and cancellation work, errors arrive structured, and `camcomp.json` is
written atomically.

## Repository layout

```text
crates/camcomp-core/     the analysis engine; usable as a library, no UI dependency
crates/camcomp-cli/      the `camcomp` binary
apps/desktop/            Tauri 2 shell
schema/                  generated JSON Schema for camcomp.json
scripts/                 fixture and calibration harnesses
artifacts/               requirements, specification, ADR, plan, implementation notes
test/tstgrp[1-8]/        labelled reference photo sets
```

## Verification

```sh
mise run check            # fmt, clippy (warnings denied), tests, release build, schema freshness
mise run test-fixtures    # the labelled reference sets in ./test
mise run calibrate        # per-pair metric distribution against the labels
```

`test/tstgrp[1-8]/` uses a filename convention: `g1-`, `g2-` … mark photos that
belong together, and `gx-` marks one deliberately left undecided. The fixture
runner compares the analysis against those labels.

## Known limitations

Read `artifacts/IMPLEM-DROP1.md` before trusting a threshold. The short version:

- **The composite compatibility score does not discriminate on the current
  labelled sets.** True pairs score 0.892–0.982, and pairs the labels put apart
  score 0.876–0.984. The ranges fully overlap. What actually separates the sets
  is the metadata prefilters and the geometric hard gates, not the weighted sum.
- **The default parameters produce no false positives on all eight reference
  sets, and reproduce 11 of 13 labelled groups.** Both misses are attributable to
  a named prefilter, and each is one override away.
- **One label cannot be satisfied by any framing tolerance.** `tstgrp3`/g1 sits
  at a 0.144 angular difference against a 0.10 tolerance, and no tolerance can
  admit it without also admitting a pair labelled apart in another set. It may
  be a labelling question rather than an algorithm one.
- Thresholds are marked `thresholdsProvisional: true` in every document.
- One 35 mm equivalent focal length in the reference set is wrong: the Pixel does
  not fold its digital zoom into `FocalLengthIn35mmFormat` while the Oppo does.
  camcomp records both values and corrects it per device rather than silently
  reconciling both conventions the same way.
- RAW is not supported. Browser-only operation is a separate, ungated target.
- HEIC decoding uses `heif-oxide` (MIT/Apache-2.0), benchmarked against the
  AGPL-licensed `heic` crate and found faster in every configuration tried
  (`artifacts/IMPLEM-DROP1.md` §1.2) — so the licence question never arises.

## Documents

Start with the [user guide](docs/GUIDE.md) if you want to use camcomp. The rest are
for working on it.

| document | what it is for |
|---|---|
| [`docs/GUIDE.md`](docs/GUIDE.md) | how to use it, and what to do when a group is wrong |
| [`artifacts/REQUIREMENTS.md`](artifacts/REQUIREMENTS.md) | what the product must do |
| [`artifacts/SPEC.md`](artifacts/SPEC.md) | how Drop 1 does it |
| [`artifacts/ADR-DROP1.md`](artifacts/ADR-DROP1.md) | why this stack |
| [`artifacts/PLAN-DROP1.md`](artifacts/PLAN-DROP1.md) | the order of work and its gates |
| [`artifacts/IMPLEM-DROP1.md`](artifacts/IMPLEM-DROP1.md) | what was built, measured, and found |

## Licence

MIT or Apache-2.0, at your option. See [`LICENSE.md`](LICENSE.md).
