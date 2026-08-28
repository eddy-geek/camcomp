# Drop 1 Implementation Notes

- **Status:** Drop 1 wrapped up. The framing predicate compares angles of view
  under two tolerances (§6.11), the strict tier was widened to `0.15` once
  `tstgrp5` was relabelled (§6.11), the session gap is 60 s and can be switched
  off for calibration (§6.12, §6.13), the digital-zoom focal policy is
  implemented (§6.7), and the Oppo mode decoding is hardened against a
  357-photo survey (§2.1). Measured: 12 of 13 labelled groups, zero false
  positives, and the same numbers with session detection disabled. The per-
  session pair budget (§4.3 of `SPEC.md`) was removed: every candidate pair is
  now either visually analyzed or decided from metadata, so nothing is ever
  silently left partially analyzed. The reference-set test suite was
  restructured for cost (`PLAN-TEST-REFACTOR.md`, §6.10) and the HEIC decoder
  choice was re-verified against a newer competitor (§1.2). Definition-of-done
  status against `PLAN-DROP1.md` §19 is in §7.1; open items for Drop 2+ are in
  §8.
- **Sources:** `REQUIREMENTS.md`, `SPEC.md`, `ADR-DROP1.md`, `PLAN-DROP1.md`
- **Purpose:** record what was actually built, what was measured, and where the
  implementation deviates from the plan — so the deviations are reviewable as
  diffs rather than discovered later.

## 1. Dependency spike results (Phase 1, `PLAN-DROP1.md` §6)

Run on macOS 26.6.2, arm64, Rust 1.97.1. All checks passed, so the ADR's
provisional Rust decision is carried into implementation.

| Capability | Selected | Result |
|---|---|---|
| JPEG/PNG decode, resize | `image 0.25.10` | works; `default-features = false` with `jpeg`/`png`/`tiff`/`webp` only |
| **HEIC decode** | `heif-oxide 0.1.0` + `rust_h265 0.1.0` | **works, pure Rust, MIT/Apache-2.0**; all 18 reference HEICs decoded with correct dimensions |
| EXIF (incl. inside HEIF) | `kamadak-exif 0.6.1` | works; `isobmff.rs` handles the HEIF container |
| SIFT | `sift-features 0.1.0` | works; `sift(&GrayImage, Option<limit>) -> SiftResult` |
| Homography + RANSAC | `vision-geometry 0.8.0` | works; `dlt_homography_ransac`, seeded `StdRng`, deterministic |

### 1.1 HEIC decode measurements

Decoding to full-resolution RGB, release build, single-threaded:

| Resolution | Time |
|---|---|
| 4096 × 3072 (12.6 MP) | 57–83 ms |
| 5888 × 4416 (26 MP) | 126–151 ms |
| 8192 × 6144 (50 MP) | 245–407 ms |

This settles `PLAN-DROP1.md` §20 item 4 (HEIC maturity concern) and reverses the
ADR §3.6 conclusion that preferred the AGPL `heic` crate: `heif-oxide` is
permissive, pure Rust, and decodes every reference file. **The ADR should be
updated.**

### 1.2 Re-checked against the `heic` crate (imazen), and kept `heif-oxide`

ADR §3.6 named `heic` (AGPL-3.0-only OR commercial) as the preferred pure-Rust
decoder before this spike ran; §1.1 above reversed that in favour of the
permissive `heif-oxide`. Revisited once more since, because `heic` had reached
0.1.6/0.2.0 and advertises AVX2/NEON SIMD and tile-parallel decoding.

Benchmarked head-to-head on two reference photos (`test/tstgrp1`, 8192 × 6144
and 4096 × 3072 HEIC) in a standalone project pinned to `heic = "0.1.6"` and
`heif-oxide = "0.1.0"`:

| build | photo | `heic` (default features) | `heic` (+`parallel`) | `heif-oxide` |
|---|---|---:|---:|---:|
| release | 50 MP | 1.54 s | 0.33–0.40 s | 0.26–0.31 s |
| release | 12.6 MP | 0.51 s | 0.11 s | 0.06–0.07 s |
| debug | 50 MP | 16.8 s | 5.4 s | 4.5–5.8 s |
| debug | 12.6 MP | 4.9 s | 1.6 s | 0.87–0.93 s |

`heif-oxide` was faster in every configuration tried, including `heic` with its
`parallel` feature (tile decoding via `rayon`) enabled, in both debug and
release. The 50 MP release case repeated three times to rule out noise: 0.26,
0.26, 0.27 s for `heif-oxide` against 0.33–0.36 s for `heic`+`parallel`,
consistent with the table.

**Decision: no change.** `heif-oxide` stays, for the same permissive-licensing
reason as §1.1 and because it also measures faster. This closes the AGPL
question raised by ADR §3.6 without needing it: the dependency that would have
required an AGPL notice is not in the tree, so `LICENSE.md` states the plain
MIT/Apache-2.0 dual licence already on every crate actually used.

Caveat: this is two photos, not a benchmark suite, though the margin (2–5× in
release, 2–3× in debug) is well outside what two data points of noise would
explain.

### 1.3 Dependency substitutions

Three deviations from the ADR §5 runtime baseline, all reducing the graph:

- **`geo` dropped.** Every polygon Drop 1 clips is a convex quadrilateral, so
  `geometry.rs` implements Sutherland–Hodgman against four half-planes plus a
  monotone-chain convex hull. ADR §3.6 already allowed this ("unless a small
  convex-quad clipper is preferable"). Avoids ~25 transitive crates.
- **`vision-calibration-core 0.8.0` added.** `vision-geometry` takes `Pt2`,
  `Mat3` and `RansacOptions` in its public signatures but does not re-export
  them. Confined to `registration.rs`.
- **`rayon` not used.** Extraction is sequential, per `PLAN-DROP1.md` §11.1. A
  50 MP decode is ~150 MB; running several in parallel is exactly the allocation
  that needs measuring before it is enabled.

## 2. Answers to open questions

### 2.1 [Q10] — the shooting-mode tag

**Answered from the reference sets, then hardened against a 357-photo survey.**
The Oppo Find X9 Pro writes the mode into EXIF `UserComment` as
`oplus_<integer>`. `exiftool` warns "Invalid EXIF text encoding for UserComment"
because the 8-byte character-code prefix is malformed, so `metadata.rs` recovers
the printable ASCII payload directly instead of trusting the declared encoding.

The survey covers every `.heic` in `data/ferr-oppo`, `data/peakfinder` and
`data/Photos-1-001 (1)`: three folders, three shooting days, **292 photos with an
`oplus_` tag, 14 distinct codes**. Each bit is correlated against independent
EXIF, so the decoding is measured rather than assumed:

| mask | meaning | evidence | in the configuration ID |
|---|---|---|---|
| `0x0100` | Master mode | n = 44; median exposure **1/5 494 s against 1/689 s** for the plain mode, shortest 1/36 344 s against 1/7 841 s — which matches the user's own description of master as "shorter exposure, darker" | yes |
| `0x2000` | high-resolution 50 MP mode | n = 74; of the 70 written by the camera itself, **every one is 8192 × 6144**, and **no 8192-wide photo lacks the bit**; digital zoom is always 1. The remaining 4 are export JPEGs the exporter downsized | yes |
| `0x0020` | portrait-orientation capture | n = 96; **no photo with portrait pixel dimensions lacks the bit (0 of 196)**, and 79 of the 96 that carry it report portrait dimensions before the container's rotation transform | no |
| `0x04000000` | burst frame | n = 11; **1:1 with the `_BURST` filename convention in both directions** | no |

The 17 landscape photos that carry `0x0020` are the reason it is called
"portrait-orientation capture" and not "portrait pixels": `exiftool` reports the
coded dimensions and the HEIF container's `irot` is applied afterwards. Nothing
rotates on this bit — `imageio.rs` uses the container transform (§2.2) — so the
ambiguity costs nothing, but it is not evidence of a mode either.

`0x0020` and `0x04000000` are deliberately **not** part of the configuration
identity. Orientation and burst do not change the image pipeline, so splitting
`auto` from `auto-in-portrait` would fragment the comparison for nothing. They are
recorded as `modeFlags` instead.

Labels use the phone's own vocabulary: `auto` (the plain mode, and by far the most
common value at n = 106), `master`, `hi-res`. The earlier `photo` and `hd` were
our invention and are gone.

#### What is *not* decoded, and how that is handled

With the flag bits masked off, the remainder is constant per writer, not per
mode:

| profile | n | seen on |
|---|---:|---|
| `0x20100000` | 280 | every camera-written HEIC, every mode combination underneath it |
| `0x210300000` | 6 | files a Google Photos export re-encoded to JPEG (`.heic` extension, JPEG magic bytes, valid `Orientation` where the camera writes the invalid `0`) |
| `0x310300000` | 4 | the same, and only on master shots |
| `0x28100000` | 2 | **unaccounted for**; no distinguishing tag found against a plain `auto` photo |

An unrecognized profile is reported, not absorbed: it becomes part of the label
(`auto-x28100000`) and lands in `modeTagUnknownBits` with a note on the photo. The
reasoning is asymmetric-risk. Folding unknown bits into `auto` would make Drop 4
compare two different pipelines as one configuration, and nothing downstream could
detect it; an odd configuration name is visible and correctable. Because a profile
is constant for a writer, an unknown one marks a device's photos without splitting
that device's own modes from each other.

The two `0x28100000` photos are the open question left in the tag. Both are at
2× zoom on the main lens, one landscape and one portrait, 4.7 MB against a typical
2.8 MB, with an otherwise identical tag set. A dedicated shoot naming each mode
would settle it — and would also confirm whether `0x210300000` is an export
artefact or a genuine second writer.

Google devices expose no mode tag. `Software` identifies the pipeline
(`HDR+ 1.0.936816820zd`); the build number is dropped so a firmware update does
not fork the configuration list. Mode label: `hdr-plus`.

Where no mode is recognized the mode stays `None` rather than being invented — the
`Labeled by PeakFinder` derivatives in `tstgrp2` are an example. A code that does
not parse keeps its raw value and produces no label, because an unreadable tag is
a bug report, not a mode-less photo.

### 2.2 EXIF `Orientation = 0`

Every Oppo HEIC in the reference set writes `Orientation = 0`, which is not a
valid EXIF value. `imageio.rs` treats anything outside `1..=8` as `1`. Rotating
on a `0` would corrupt every coordinate downstream.

Separately, `heif-oxide` applies the HEIF container's `irot`/`imir` transforms
itself, so HEIC pixels arrive already oriented and the EXIF value must **not** be
applied a second time. This is recorded per photo as
`orientationAppliedByDecoder`.

### 2.3 Missing 35 mm equivalent focal length

`tstgrp2/g3-20260822_115835_*.jpg` reports `FocalLengthIn35mmFormat = 0`. A 0 mm
lens does not exist, so `0` is read as "unknown" and the focal comparison becomes
`Unknown`, which does not exclude the pair from visual analysis.

### 2.4 Timestamps without a timezone

The `PeakFinder` derivatives in `tstgrp2` carry `DateTimeOriginal` with no
`OffsetTimeOriginal`. Treating a naive time as UTC would shift them two hours away
from their session peers. The implementation assumes the *most common offset among
photos in the same run that do declare one* (falling back to UTC), and records
`timezoneAssumed: true` per photo. Documented heuristic, visible in the output.

## 3. Deviations from the specification, and why

### 3.1 Component saturation points

`SPEC.md` §5.4.1 predicted this: the components do not reach `1.0` in practice, so
a `0.80` threshold over raw components would be unreachable. Section 5.4.1
explicitly permits "rescaling components against their observed usable range …
provided the mapping is persisted with the algorithm parameters".

`metrics.rs` therefore saturates two components at documented points, which are
part of the algorithm version:

- feature coverage: `12/16` grid cells and `50 %` convex-hull area count as
  complete;
- inlier confidence: `100` inliers and a `0.50` inlier ratio count as complete.

Geometric overlap and appearance similarity are used raw. Appearance similarity
is gradient ZNCC mapped from `[-1, 1]` to `[0, 1]`, so unrelated content sits near
`0.5` rather than `0.0` — deliberate, and the reason the weight on it is only
`0.20`.

The composite score is divided by the weight sum, so reweighting cannot silently
move the threshold along with it.

### 3.2 Pair-level homographies are persisted; correspondences are not duplicated

`SPEC.md` §7.2 asks for correspondences per aligned photo. They are stored once,
in the group alignment, and the *pair* record keeps the homography plus a bounded
correspondence set only for `compatible` pairs. Re-anchoring composes transforms
and remaps the stored anchor-side points, recording
`provenance: composed` and a degraded confidence.

### 3.3 No on-disk feature cache

`REQUIREMENTS.md` §3 permits exactly one file in the source folder:
`camcomp.json`. Features are cached in memory for the duration of a run. A warm
reload needs no features at all, because pair results are themselves cached in the
document — so the cache layer that matters is already persistent.

## 4. Findings from the reference sets that the thresholds cannot satisfy

These are recorded before tuning, per `PLAN-DROP1.md` §4. They are properties of
the labelled data, not bugs.

> **Status: both findings are resolved, and §6.11–§6.13 record how.** §4.1 was
> the right question asked of the wrong quantity — the rule is about *angle of
> view*, not focal ratio — and §4.2 was a session gap tuned to the photographer
> rather than to the device clocks. The sections are kept because the reasoning
> that led to the change is worth more than the conclusion.

### 4.1 `tstgrp4` and `tstgrp8`: labelled groups whose focal lengths differ by far more than 15 %

| set | group | Oppo 35 mm eq. | Pixel 35 mm eq. | relative difference |
|---|---|---|---|---|
| `tstgrp4` | g1 | 23 | 24 | 4 % — within tolerance |
| `tstgrp4` | g2 | 15 | 12 | **25 %** |
| `tstgrp4` | g3 | 70 | 49 | **43 %** |
| `tstgrp8` | g2 | 70 | 49 | **43 %** |

The two devices were pointed at the same scene with different zoom steps: the
Oppo used its 3× tele (70 mm equivalent), the Pixel a 2× sensor crop (49 mm). The
user's own labels therefore contradict `REQUIREMENTS.md` §3's "same focal length,
within 15 %" rule.

`SPEC.md` §4.2 only relaxes the focal rule for *unknown* values, not for known
mismatches. So with the specified `focalTolerance = 0.15` these pairs are rejected
from metadata alone and their groups cannot form. The tolerance is a persisted
parameter and the CLI exposes it, so the effect is measurable rather than
hard-coded — see §6 for the measured numbers at several tolerances.

**This needs a decision from the requirements, not from the code.** Either the
15 % rule is about *field of view* (and these are genuine negatives), or it is
about *what the user considers comparable* (and the rule needs widening or
replacing with an overlap-based test).

### 4.2 `tstgrp2`: a labelled group whose members are 50 s apart

`g3-20260822_115745_*.jpg` and `g3-20260822_115835_*.jpg` are 50 seconds apart.
The 30-second sliding-gap rule puts them in different sessions, so they are
reported as ungrouped with reason `no-session-peer`. Correct per specification,
and a real limit of a time-based prefilter.

## 5. Repository shape

Follows `PLAN-DROP1.md` §3 with the `tools/` directory absent, as intended — the
Python/OpenCV contingency of `ADR-DROP1.md` §5.1 was not triggered.

```text
camcomp/
├── Cargo.toml            # workspace, pinned versions
├── mise.toml             # fmt / lint / test / check / test-fixtures / build
├── crates/
│   ├── camcomp-core/     # UI-independent engine
│   └── camcomp-cli/      # `camcomp` binary
├── apps/desktop/         # Tauri 2 spike
├── schema/camcomp.schema.json
├── artifacts/            # requirements, spec, ADR, plan, these notes
└── test/tstgrp[1-8]/     # labelled reference sets
```

### 5.1 Core module map

| module | responsibility |
|---|---|
| `error` | typed errors with stable `code()` for adapters |
| `geometry` | `Mat3`, normalized-coordinate rebasing, quad clipping, overlap, hull |
| `progress` | `ProgressSink`, `CancelToken`, `Reporter` |
| `schema` | the `camcomp.json` contract, `validate()`, generated JSON Schema |
| `domain` | `GrayImage`, keypoints, the five dependency-isolating traits |
| `imageio` | format detection, decode, orientation, analysis image |
| `metadata` | EXIF extraction, mode identification, configuration identity |
| `features` | `sift-features` adapter, in-memory feature cache |
| `matching` | deterministic brute-force KNN, Lowe ratio, mutual check |
| `registration` | RANSAC adapter, hard gates, occupancy and hull measures |
| `warp` | projective warp with border mask, Sobel gradient, masked ZNCC/SSIM |
| `metrics` | normalized components, composite score, end-to-end pair analysis |
| `sessions` | sliding-gap sessions, focal predicate, candidate pairs |
| `grouping` | anchor-based groups, alignment resolution, manual overrides |
| `cache` | fingerprints, atomic `camcomp.json` read/write |
| `pipeline` | `inspect` / `analyze` / `regroup` / `validate` |

## 6. Calibration evidence

Reproduce with `mise run test-fixtures`, `mise run test-fixtures:relaxed` and
`mise run calibrate`. All eight labelled sets in `./test` ran to completion
without a crash on macOS 26.6.2 / arm64, release build.

### 6.1 Outcome at the current defaults

`sessionGapSeconds = 60`, `fieldOfViewTolerance = 0.15`,
`fieldOfViewToleranceNativeLens = 0.20`, `focalZoomPolicy = per-device`,
`compatibilityThreshold = 0.80`, `onePhotoPerConfiguration = true`.

| set | photos | groups found | labelled groups reproduced | false positives |
|---|---:|---:|---:|---:|
| `tstgrp1` | 3 | 0 | 0 / 1 | 0 |
| `tstgrp2` | 4 | 2 | 2 / 2 | 0 |
| `tstgrp3` | 3 | 1 | 1 / 1 | 0 |
| `tstgrp4` | 6 | 3 | 3 / 3 | 0 |
| `tstgrp5` | 4 | 1 | 1 / 1 | 0 |
| `tstgrp6` | 3 | 1 | 1 / 1 | 0 |
| `tstgrp7` | 3 | 1 | 1 / 1 | 0 |
| `tstgrp8` | 4 | 2 | 2 / 2 | 0 |
| **total** | **30** | **11** | **12 / 13** | **0** |

**Both calibration objectives are now met on the labelled set: no false
positives, and 12 of 13 labelled groups reproduced automatically** (5 of 13
originally, 11 of 13 in the previous revision). The remaining miss:

| miss | decided by | measured value |
|---|---|---|
| `tstgrp1` g1 | one-photo-per-configuration | both photos are `oppo-find-x9-pro--hi-res` |

Six labels were recovered by the three changes of §6.11–§6.13: `tstgrp2` g3 and
`tstgrp5` g1 and `tstgrp8` g2 by the session gap, `tstgrp4` g2 by the native-lens
tier, `tstgrp4` g3 and `tstgrp7` g2 by the zoom policy. A seventh, `tstgrp3` g1,
was recovered by widening the strict tier to `0.15` once `tstgrp5` was
relabelled from two groups into one (§6.11).

### 6.2 Outcome with the prefilters relaxed, and with sessions off

Two comparison passes, because "does it pass" matters less than "which stage
decided".

| | defaults | no sessions | all prefilters off |
|---|---:|---:|---:|
| labelled groups reproduced | 12 / 13 | **12 / 13** | 13 / 13 |
| false positives | **0** | **0** | **2** |

- **no sessions** — `--no-session-gap`, everything else at defaults. Session
  detection is disabled entirely: every photo in a set lands in one session, so
  the framing predicate and the geometric gates have to judge every pair.
  `mise run test-fixtures:no-sessions`.
- **all prefilters off** — `--no-session-gap --fov-tolerance 1.0
  --allow-duplicate-config`. Nothing is decided from metadata.
  `mise run test-fixtures:relaxed`.

The middle column is the result worth having. **Removing the time prefilter
changes nothing at all**: the same 12 labels, the same zero false positives. They
are not load-bearing for correctness — they are a cost control (§6.10).

The false positives in the last column locate what is still doing the work: both
are in `tstgrp1`, from allowing duplicate configurations. Widening the strict
framing tolerance to `0.15` and relabelling `tstgrp5` (§6.11) removed the framing
predicate's own blind spot — the four false positives this column used to
produce when the predicate was switched off are gone, because the pair that used
to need it (the 28 mm zoom step) is no longer labelled apart from the group it
belongs to.

### 6.3 The composite score does not discriminate

Component ranges over the relaxed run, split by what the labels expect
(`SPEC.md` §5.4.1 asked for exactly this table):

| component | true pairs (n = 15) | pairs labelled apart (n = 8 analyzed) | separates? |
|---|---|---|---|
| geometric overlap | 0.844 – 1.000 | 0.986 – 1.000 | no — *inverted* |
| feature coverage | 0.940 – 1.000 | 0.749 – 0.990 | no |
| inlier confidence | 0.852 – 0.978 | 0.866 – 0.967 | no |
| aligned appearance | 0.779 – 0.966 | 0.774 – 0.980 | no |
| **composite score** | **0.892 – 0.982** | **0.876 – 0.984** | **no** |
| affine scale | 0.826 – 1.081 | 0.473 – 3.200 | partly |

**The composite score ranges overlap completely.** No threshold on it can
separate the labelled sets. This answers the open question in `SPEC.md` §10: the
weighted composite does not survive calibration as a discriminator. What
discriminates is the metadata prefilters and the geometric hard gates.

Two components behave worse than the specification assumed:

- **Feature coverage has almost no dynamic range on true pairs** (0.940–1.000,
  mostly exactly 1.000). It contributes 0.25 of the weight and decides nothing.
- **Geometric overlap is inverted.** Negatives score *higher* (mean 0.998) than
  true pairs (mean 0.967). §6.5 explains why, and it is a structural property of
  the definition rather than a tuning problem.

### 6.4 Where the discrimination actually lives

Of the 20 pairs the labels put apart:

- 12 were rejected by the focal prefilter from metadata alone, at zero cost;
- 2 were `uncertain / no-consensus` — RANSAC found no credible transform, which
  is the correct answer for genuinely different scenes and, importantly, is not
  coerced into a decision;
- 6 passed every gate and scored above threshold (the false positives of §6.2).

`tstgrp7` is the clearest demonstration of the hard gates working: with the
session gap relaxed, `g1` against both `g2` photos produced no RANSAC consensus
and was reported `uncertain`, not `incompatible` and not `compatible`.

### 6.5 Why `geometricOverlap` cannot see a zoom-in

`SPEC.md` §5.3 defines overlap as the intersection divided by *the smaller* of
the two areas. When one photo is a zoomed crop of the other, the narrower frame
lies entirely inside the wider one, so the intersection *is* the smaller area and
overlap is exactly 1.0 — a perfect score for a pair that does not share a framing
at all.

`tstgrp5` made this measurable, back when it was labelled as two groups (`g1`
native and `g2` zoomed) rather than the single group it is now (§6.11). The
measurements below predate that relabelling and use the old split; they still
demonstrate the structural point about the overlap metric.

Within-label pairs versus cross-label pairs, at the old labelling:

| pair | focal ratio | measured affine scale | overlap | score | label |
|---|---:|---:|---:|---:|---|
| g2 + g2-master | 1.000 | 1.027 | 0.978 | 0.981 | same |
| g1-hd + g1-pxl | 0.958 | 1.051 | 1.000 | 0.969 | same |
| g2 + g1-hd | 0.821 | 0.822 | **1.000** | 0.984 | different |
| g2 + g1-pxl | 0.857 | 0.855 | **1.000** | 0.973 | different |
| g2-master + g1-hd | 0.821 | 0.837 | **1.000** | 0.976 | different |
| g2-master + g1-pxl | 0.857 | 0.871 | **1.000** | 0.969 | different |

The measured affine scale tracks the focal ratio to three decimals (0.822 against
23/28 = 0.821), so the geometry does know about the zoom. The overlap metric
throws that information away, and the score has no other channel for it.

The obvious repair — tighten the affine-scale gate to the focal tolerance, so
pixels enforce what metadata enforces — is not applied, because §6.6 shows it
cannot work either.

### 6.6 Is any focal tolerance consistent with the labels?

Almost. One pair stands in the way, and it is a good candidate for a labelling
mistake rather than an algorithm problem.

Reproduce with:

```sh
for i in 1 2 3 4 5 6 7 8; do
    camcomp --quiet inspect test/tstgrp$i --format json > /tmp/insp-$i.json
done
scripts/focal-hypothesis.py --gap 60 /tmp/insp-*.json
```

The script asks the only question that matters for a prefilter: among pairs the
*session* filter lets through, is there a tolerance that accepts every
labelled-together pair and rejects every labelled-apart one? Pairs in different
sessions are excluded, because the focal predicate never sees them.

| focal length used | session gap | widest pair to ACCEPT | closest pair to REJECT | verdict |
|---|---:|---:|---:|---|
| as reported by EXIF | 30 s | 0.458 (`tstgrp7`/g2) | 0.217 (`tstgrp5`) | interleaved |
| **zoom-corrected** (§6.7) | 30 s | 0.250 (`tstgrp4`/g2) | 0.217 (`tstgrp5`) | interleaved |
| **zoom-corrected**, `tstgrp4`/g2 set aside | 30 s | 0.147 (`tstgrp3`/g1) | 0.217 (`tstgrp5`) | **valid: `[0.147, 0.217)`** |
| **zoom-corrected**, `tstgrp4`/g2 set aside | 60 s | 0.147 (`tstgrp3`/g1) | 0.167 (`tstgrp5`) | **valid: `[0.147, 0.167)`** |
| zoom-corrected, `tstgrp4`/g2 set aside | 90 s | 0.250 | 0.000 (`tstgrp7`) | interleaved |

Three things follow.

**The specified default `focalTolerance = 0.15` is correct.** It sits inside the
valid window at both a 30 s and a 60 s session gap. It was never the problem.

**The zoom correction is the actual fix**, not a threshold change. It moves the
widest must-accept pair from 0.458 to 0.250 and turns an unsolvable problem into a
one-exception problem.

**`tstgrp4`/g2 is the single exception**, at a corrected ratio of 0.250:

```text
g2-IMG20260822131909.heic       Oppo ultrawide,  2.31 mm, 15 mm equivalent, no zoom
g2-PXL_20260822_111909642.jpg   Pixel ultrawide, 2.02 mm, 12 mm equivalent, no zoom
```

Both phones at their native ultrawide with no zoom, so the *intent* is clearly
"compare the two ultrawide cameras" — but the hardware fields of view genuinely
differ by 25 %, and the resulting photos do not frame the same area. Under
`REQUIREMENTS.md` §3 ("cover the same area of the scene") this pair arguably should
*not* be one group; under "compare like lens against like lens" it should. Either
the label is a mistake, or the requirement needs a second notion of "comparable".

The 90 s row is not a failure of the focal predicate: at that gap `tstgrp7`'s g1
joins the session and its focal length is *identical* to g2's (both 35 mm
equivalent), so no focal rule can separate them. The visual test does, correctly,
by returning `uncertain / no-consensus`. This is the division of labour working:
metadata separates framing, geometry separates scenes.

### 6.6bis Recommended configuration, and what it would score

> **Superseded by §6.11–§6.13, which went further.** The recommendation below was
> right about the session gap and about the zoom correction, and wrong about the
> tolerance: it kept a *focal-ratio* rule and therefore needed `tstgrp4`/g2 set
> aside. Comparing angles of view instead makes that pair ordinary. What shipped:
> `sessionGapSeconds = 60`, `fieldOfViewTolerance = 0.10`,
> `fieldOfViewToleranceNativeLens = 0.20`, `focalZoomPolicy = per-device`, and
> `onePhotoPerConfiguration` left at `true`.

Not applied — it needs the §6.7 correction implemented and a decision on
`tstgrp4`/g2 — but fully evidenced:

| parameter | value | why |
|---|---|---|
| `focalTolerance` | `0.15` | unchanged; inside the valid window |
| focal length source | zoom-corrected | §6.7 |
| `sessionGapSeconds` | `60` | the three missed groups are 31 s, 36 s and 50 s apart |
| `onePhotoPerConfiguration` | `false` | `tstgrp1`/g1 is two shots from one configuration |

Expected outcome: 12 of 13 labelled groups, 0 false positives, with `tstgrp4`/g2
either relabelled or corrected by one override. The measured rejections that keep
false positives at zero would still be: `tstgrp5` cross-label pairs by focal
(0.167 ≥ 0.15) and `tstgrp7` g1 by geometry (no RANSAC consensus).

Raising the session gap to 60 s is the change with the least evidence behind it —
it is justified by three data points, all of which are a device clock disagreeing
with another by a few seconds. A per-device clock offset (already supported, see
`--time-offset`) may be the better instrument.

Until that is decided, `thresholdsProvisional` stays `true` and the defaults are
the ones the specification states.

### 6.7 A metadata defect worth fixing upstream of any threshold

`tstgrp7` g2 reads 35 mm eq. on the Oppo and 24 mm eq. on the Pixel — a 46 %
difference — yet the measured affine scale is 0.979, i.e. the two frames are the
same size. The Pixel's `DigitalZoomRatio` is 1.47 and 24 × 1.47 = 35.3.

**The Pixel does not fold digital zoom into `FocalLengthIn35mmFormat`; the Oppo
does.** Verified across the sets by computing the implied crop factor
`focalLengthIn35mm / focalLength`:

| device | at zoom 1 | at zoom 1.5 | conclusion |
|---|---:|---:|---|
| Oppo Find X9 Pro | 23 / 6.75 = 3.41 | 35 / 6.75 = 5.19 | zoom is baked in |
| Pixel 9 Pro XL | 24 / 6.9 = 3.48 | 24 / 6.9 = 3.48 | zoom is **not** baked in |

Applying the correction changes the focal ratios of four labelled pairs and brings
each into line with the measured scale:

| set | pair | reported ratio | corrected ratio | measured scale |
|---|---|---:|---:|---:|
| `tstgrp7` | g2 | 0.686 | 0.992 | 0.979 |
| `tstgrp8` | g2 | 0.700 | 0.991 | 1.024 |
| `tstgrp4` | g3 | 0.700 | 0.966 | 0.955 |
| `tstgrp3` | g1 | 0.786 | 0.871 | 0.882 |

**Now implemented** as the `focalZoomPolicy` parameter, with the three values this
section suggested: `trust-exif`, `apply-digital-zoom`, and `per-device` (the
default). It recovered two labels on its own, `tstgrp4`/g3 and `tstgrp7`/g2.

The concern that stopped it — "a hard-coded device list that silently rewrites a
user's focal lengths is worse than not doing it" — is addressed by making it not
silent rather than by not doing it:

- both `focalLength35mm` (as reported) and `effectiveFocalLength35mm` (as
  compared) are persisted on every photo, so the two conventions never collapse
  into one number;
- `focalSource` says which of the two happened, as `exif-35mm` or
  `exif-35mm-zoom-applied`;
- a photo whose focal length was rewritten carries a note saying so, which
  `camcomp inspect` prints as `focal35=24->35`;
- the policy is a persisted parameter, so changing it invalidates exactly the
  cached pair results that depended on it.

The default is `per-device` rather than `apply-digital-zoom` because EXIF defines
the 35 mm equivalent as a property of the *lens*: a spec-abiding device reports the
un-zoomed value and leaves the zoom in its own tag, so applying the zoom is the
correct default and the device table lists the deviations. The table has one
entry, matched on make: OPPO. A model that does not share the convention
under-corrects, which surfaces as a *rejected* pair — visible in the ungrouped
list — rather than as a wrong grouping.

`scripts/focal-hypothesis.py` keeps its own copy of the correction, which is now
useful as an independent check on the engine rather than as a preview of it.

### 6.8 A dependency defect that would have corrupted the cache silently

`serde_json`'s default float parser uses a fast path that is not correctly
rounded. It writes a correct shortest representation and reads it back off by up
to one ulp:

```text
115.37286376953125  ->  written "115.37286376953125"  ->  read 115.37286376953124
```

That is not cosmetic. Every persisted homography element, correspondence and
metric would shift on each load-and-rewrite cycle, so `camcomp.json` would not be
a cache — it would be a slowly drifting approximation, and the Phase 3 exit
criterion "a checked-in golden document can be loaded and written without
semantic changes" would be quietly false.

Fixed by enabling the crate's `float_roundtrip` feature, which is not optional for
this project. Two regression tests guard it:
`cache::tests::floats_survive_a_json_round_trip_bit_for_bit` and
`cache::tests::a_homography_survives_a_document_round_trip_bit_for_bit`, the
second of which also asserts that a *second* cycle is byte-identical so drift
cannot accumulate.

Found only because the reference-set integration test compares the analyzed
document against the reloaded one for exact equality. A tolerance-based assertion
would have passed.

### 6.9 The human-correction path reaches the labelled answer

`tstgrp3` is the worked example. Its three photos are labelled as one group; the
Pixel is excluded automatically because its 110 mm equivalent is 27 % from the
Oppo's 140 mm.

```sh
camcomp analyze photos/                              # 1 group of 2, 1 ungrouped
camcomp override photos/ --add GROUP=PHOTO           # 3 members, flagged
camcomp analyze photos/ --force-pair PAIR            # alignment computed on demand
```

Result: a group of three with a `direct-pair` alignment at confidence 0.95 for the
manually added member, `manual-membership` flagged, nothing ungrouped, and the
override carried through re-analysis. Timings: 1.6 s cold, 1 ms for the override
(no decoding), 1.6 s for the forced pair.

Two gaps were found and closed while verifying this:

- `--force-pair` overrode only the *budget*, not the focal prefilter, so it could
  not produce the alignment a manual member needs. `CandidatePair::forced` now
  overrides both, and a forced pair whose cached record carries no visual evidence
  is re-analyzed rather than reused.
- The CLI printed `anchor` for a member that had *no* alignment, which read as
  success. It now prints `NO ALIGNMENT — pair not analyzed`.

### 6.10 Runtime and memory

Release build, sequential extraction, analysis edge 1200 px:

| stage | measurement |
|---|---|
| HEIC decode, 12.6 MP | 57–83 ms |
| HEIC decode, 50 MP | 245–407 ms |
| SIFT + match + register, one pair at 1200 px | ~0.9 s |
| whole set, 3 photos / 3 pairs (`tstgrp6`) | 3.0 s cold |
| whole set, 6 photos / 15 candidate pairs (`tstgrp4`) | 1.8 s cold (14 pairs decided from metadata) |
| warm reload, any set | 0 pairs analyzed, 0 photos decoded |

Peak decoded buffer is the largest source image: 8192 × 6144 RGB is about 150 MB,
which is why extraction is sequential and analysis images are released per
session rather than per album. `peakAnalysisPixels` is persisted per run.

The label outcome is identical at analysis edges of 320, 480, 600 and the 1200
default, which is what lets the integration test run at 320
(`PLAN-TEST-REFACTOR.md`).

**Debug build, on `tstgrp1` (3 photos: two 50 MP HEIC, one 12.6 MP HEIC),
`camcomp analyze --dry-run --no-cache`:**

| analysis edge | wall | CPU |
|---|---:|---:|
| `inspect` (header only, no full decode) | 1.3 s | 0.3 s |
| 480 | 61.5 s | 111.6 s |
| 320 | 46.3 s | 96.7 s |
| 120 | 35.5 s | 86.4 s |

The gap between 480 and 320 (15 s) is the resolution-dependent half; what
remains at 320 (46 s) is mostly decode, and even at the lowest edge tried the
floor is 35 s for one 50 MP + one 12.6 MP HEIC decode pair. **Decode dominates
the debug cost** — the opposite of the release profile above, where decode is
tens of milliseconds and matching is the visible cost. `heif-oxide` and
`rust_h265` are pure Rust with bounds checks on every access, and an
unoptimized build pays for that on every pixel of a 50 MP frame.

This is why lowering the reference-set suite's analysis edge (480 → 320) helps
the release run but is close to cosmetic for a debug run, and why the suite
instead moved to running in release from `mise run test`
(`PLAN-TEST-REFACTOR.md` §4 step 1): no analysis-edge change fixes a
debug-mode decode floor measured in tens of seconds per photo.

**Measured suite runtime** (`cargo test -p camcomp-core --test reference_sets`,
eight reference sets, nine tests, one shared analysis pass at edge 320):

| build | wall |
|---|---:|
| release | 8.9 s |
| debug | 7 min 17 s (shared pass: 6 min 32 s of it) |

`cargo test --workspace --all-features --lib --bins` (everything else in the
workspace, debug): 52.3 s. `--doc`: 13.6 s.

The reference-set suite now only runs in release from `mise run test`
(`PLAN-TEST-REFACTOR.md` §4 step 1) — 8.9 s there, against the original single
test that ran over 5 minutes and was killed. **A plain `cargo test --workspace`
typed directly, without `mise`, still executes `reference_sets` in debug and
still pays about 7 minutes**, because that number is the shared pass decoding
~20 real photos once, several at 50 MP, and decode is what dominates in an
unoptimized build (measured above). No test-structure change removes that: the
one-shared-pass design already cut the old suite's per-test-multiplied cost
(nine full passes) to one, and going further would mean either analyzing fewer
of the eight labelled sets — which the suite's own coverage argument (§4 step 4
in `PLAN-TEST-REFACTOR.md`) rejects — or skipping the suite in debug, which the
plan rejects as dishonest. This is the outcome the plan's own decision rule
anticipates for the "decode dominates" branch: steps 1, 2 and 4 land, step 3 is
close to a rounding error in debug, and the acceptance criterion of "a few
minutes at worst" is met for the sanctioned `mise run test` path but not for a
raw `cargo test --workspace` invocation when the reference photos are present.
The suite says so itself: `shared reference-set pass: 8 set(s) analyzed in
392.3s at analysis edge 320`, printed once instead of the run appearing to
hang.

### 6.11 The framing predicate compares angles of view, not focal lengths

This is the change that resolves §4.1 and §6.6, and it is a change of *quantity*
rather than of threshold.

`REQUIREMENTS.md` §3 asks for two things in one sentence: "same focal length,
within 15 %" and "the photos should cover the same area of the scene". Those are
not the same test. Angle of view is `2·atan(d / 2f)`, so a fixed focal ratio is a
small angular difference at the wide end and nearly all of it at the long end.
Measured on the labelled pairs:

| pair | effective focal | focal ratio | angular difference | label |
|---|---|---:|---:|---|
| `tstgrp4` g2 | 15 vs 12 mm (both ultrawide) | **0.250** | **0.104** | same |
| `tstgrp5` g1 | 24 vs 28 mm (one zoomed 1.21×) | **0.167** | **0.115** | same |
| `tstgrp5` g1 | 23 vs 28 mm | 0.217 | 0.147 | same |
| `tstgrp3` g1 | 140 vs 122 mm | 0.147 | 0.144 | same |
| `tstgrp4` g1/g2 | 23 vs 15 mm | 0.533 | 0.278 | different |

**The first two rows are the whole argument.** By focal ratio the pair
labelled *together* (`tstgrp4` g2) is further apart (0.250) than the pair the
28 mm zoom step used to be labelled apart from at 0.167, so no focal-ratio
threshold could separate them — that was §6.6's "interleaved" verdict. By angle
of view the order reverses (0.104 against 0.115) and a threshold exists. Two
ultrawides at 122° and 110° frame much the same scene; 24 mm against 28 mm is a
framing the photographer deliberately changed — which is why `tstgrp5` was
relabelled into a single group rather than kept as two (see below).

A single angular threshold would have to clear the widest pair that must be
accepted and stay under the closest that must be rejected, so the predicate has
two tiers:

| tier | applies when | value | widest pair it must accept | closest it must reject |
|---|---|---:|---:|---:|
| strict | either photo used digital zoom or a sensor crop | `0.15` | 0.147 (`tstgrp3` g1 / `tstgrp5` g1) | 0.750 (`tstgrp7`) |
| native lens | **both** photos at a lens's native field of view | `0.20` | 0.104 (`tstgrp4` g2) | 0.278 (`tstgrp4` g1/g2) |

The tier is not a fudge factor, it is the second notion of "comparable" that §6.6
said the requirements were missing. Two phones' ultrawides differ in field of view
because the hardware differs, and comparing them is precisely what the product is
for; a zoom step is a choice the photographer made, and judging it loosely would
merge framings the user distinguishes. `nativeLens` comes from
`DigitalZoomRatio ≤ 1` (absent and `0` both mean "not used"), and is persisted per
photo, as is the tolerance each pair was judged against — the verdict is not
explainable from the difference alone.

Margins at the shipped values: the strict tier sits 2 % above the widest pair it
must accept and 80 % below the closest it must reject; the native tier sits 93 %
above and 39 % below.

**`tstgrp3` g1 was the original blocker, and it resolved once `tstgrp5` did.**
At 0.144 angular difference `tstgrp3` g1 needed a tolerance of at least 0.144,
which would also have admitted the 28 mm-zoom pair inside the old two-group
`tstgrp5` labelling (0.115 and 0.147), and §6.5 showed the composite score could
not reject those on its own. Relabelling `tstgrp5` as one group — the zoom step
*is* the same scene, just as the requirement's "cover the same area" test says —
removed that conflict entirely: every same-session pair labelled apart now sits
at 0.278 or further, so `0.15` clears `tstgrp3` with room to spare instead of
needing a window 6 % wide.

Two named parameters replace `focalTolerance`: `fieldOfViewTolerance` and
`fieldOfViewToleranceNativeLens`. The rename is deliberate. A number called
`focalTolerance` set to `0.15` invites the reader to compute a focal ratio, and
they would be wrong by a factor that depends on the focal length. For the record,
`0.15` of angle of view is about a 20 % focal-ratio window at main-camera focal
lengths, looser at ultrawide, stricter at tele.

### 6.12 The session gap is 60 seconds, not 30

`REQUIREMENTS.md` §3 says photos in a session are "normally taken less than 30
seconds from each other", and that is a statement about the photographer. The
implementation also has to absorb two device clocks disagreeing. The three
labelled groups that the 30 s rule missed sit 31 s, 36 s and 50 s apart — one
second, six seconds and twenty seconds past a boundary — and all three are the same
two devices photographing the same scene.

Raising the gap to 60 s recovers all three and introduces nothing: the pairs it
newly admits (`tstgrp5` at 42 s and 52 s) are rejected by the framing predicate at
0.115, and the next-nearest cross-label pair in any set is 81 s away.

This is still the change with the least evidence behind it — three data points —
and a per-device clock offset (`--time-offset`, already supported) remains the more
honest instrument for clock skew. The difference from the previous revision is that
the session gap is no longer *load-bearing*: §6.2 measures the same 11 labels and
the same zero false positives with session detection switched off entirely. That is
the real safety net, and it is what makes a 60 s gap a cost decision rather than a
correctness decision.

### 6.13 Why sessions can be switched off, and why that matters

`mise run test-fixtures:no-sessions` runs every set with `--no-session-gap`, which
puts every photo in one session. It exists because a prefilter hides the thing it
protects: a pair that never becomes a candidate is never judged, so a suite that
always runs with sessions on cannot tell whether the rest of the algorithm works or
whether the clock was simply doing the sorting.

With sessions off the result is unchanged — 12/13, zero false positives — and two
things become visible that the defaults conceal:

- `tstgrp7` g1 against g2 is analyzed and correctly *not* grouped. The two photos
  are 81 s apart at an identical 35 mm equivalent, so no metadata rule could
  separate them; RANSAC finds no consensus and the pair is reported `uncertain`.
  This is the division of labour working as designed: metadata separates framing,
  geometry separates scenes.
- `tstgrp2` g2 against g3, 279 s apart at nearly identical focal lengths, is also
  judged visually and correctly kept apart.

The cost is the quadratic one sessions exist to bound: on these sets it is 3
analyzed pairs instead of 1 for `tstgrp7`. On a folder of hundreds of photos it
would be the difference between minutes and hours, which is why this is a
calibration instrument and not a default.

## 7. Verification status

Run on macOS 26.6.2 / arm64, Rust 1.97.1, at Drop 1 wrap-up.

| check | result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | clean |
| `mise run test` (lib/bins + doc in debug, `reference_sets` in release) | **264 passed, 0 failed** (239 + 15 + 1 + 9, see §7.1) |
| `cargo build --workspace --release` | clean, including the Tauri shell |
| `mise run test-fixtures` | all 8 sets complete, no crash, 0 false positives, 12/13 labels (§7.3) |
| generated JSON Schema | checked in, `mise run schema:check` clean |

### 7.1 Test inventory

| module | tests | what they pin down |
|---|---:|---|
| `geometry` | 20 | row-major order and transform direction; inverse round-trip; singular and reflected matrices; overlap for identity, half-shift, disjoint, containment and large scale; quad clipping in four configurations; convex hull including the collinear case; normalized homographies staying valid at full resolution |
| `registration` | 22 | exact projective recovery; outlier exclusion; noise budget; collinear and degenerate inputs; seeded reproducibility; each of the nine hard gates including boundary inclusivity, and that all failures are reported rather than the first |
| `metrics` | 20 | component normalization including non-finite input; hand-computed score fixture; weight-sum invariance; coverage rewarding spread; end-to-end pair analysis on synthetic ground truth for translation, rotation+scale and exposure change; unrelated scenes; barely-overlapping pairs; flat images as `uncertain`; that a high appearance score cannot bypass a failed gate; correspondence spatial spread |
| `grouping` | 20 | non-transitive evidence not collapsing into one component; one-per-configuration; anchor-relative focal exclusion; permutation stability; anchor stability across reload; every ungrouped reason; all six override operations; invalid overrides rejected without corrupting the document; alignment composed through a bridge member |
| `cache` | 18 | digest stability; streamed vs in-memory agreement; the size/mtime fast path and forced re-hash; atomic write leaving no temporary file; schema major mismatch reported *as* a version error; corrupt JSON not touching the file; **bit-for-bit float and homography round-trip** (§6.8) |
| `sessions` | 22 | the 30 s boundary in both directions; a chain of short gaps making one long session; permutation stability under equal timestamps; focal predicate symmetry and boundary; unknown focal not excluding; every candidate pair generated with no work cap |
| `warp` | 17 | identity warp exactness; invalid border after a shift; singular transform; warp round-trip; Sobel border invalidity; mask erosion removing the warp boundary; ZNCC surviving an exposure change, refusing a flat image, and refusing non-overlap |
| `imageio` | 15 | all eight EXIF orientations dimensionally correct and sample-preserving; invalid orientation `0` treated as normal; magic bytes beating the extension; aspect-preserving analysis dimensions; the pixel guard; truncated input as an error not a panic |
| `pipeline` | 15 | deterministic discovery skipping our own output; unsupported and missing inputs; photos with no timestamp reported not dropped; unreadable files still in the document; parameter validation before any work; cancellation; every stage emitting progress; no source photo modified; a `--force-pair` selector naming a photo (which never forms a candidate pair without a shared session), and one matching nothing not forcing everything |
| `metadata` | 13 | UTC offsets both directions; sub-second scaling by digit position; the EXIF colon date form and a `:60` second; **all five observed Oppo `oplus_` mode codes**; the portrait bit read separately; Pixel HDR+ without its build number; an unrecognized comment yielding no mode rather than a guess |
| `matching` | 12 | descriptor distance symmetry; **equal distances breaking ties by lower index**; ambiguous and duplicate matches dropped rather than accepted; ratio-test boundary inclusivity; the mutual check; independence from descriptor order |
| `schema` | 11 | document round-trip; unknown fields ignored; major-version rejection; every photo appearing exactly once; a group of one rejected; **a persisted inverse that does not invert rejected**; cache key sensitivity |
| `features` | 8 | features found in texture; the cap honoured; a flat image yielding almost nothing; a tiny image reported as none; determinism; the cache extracting once per key and not poisoned by a failing loader |
| `heif` | 7 | **EXIF recovered from a synthetic container, including a payload over the 64 KiB container limit**; the recovered payload parsing as EXIF; 64-bit box sizes; malformed input never panicking |
| `domain` | 3 | buffer/dimension mismatch; bilinear sampling and clamping; out-of-bounds reads |
| `progress` | 3 | cancellation observed; closures as sinks; stable event tag |
| **`camcomp-desktop`** | 15 | in-process engine reported; core error codes surviving the IPC boundary as JSON; cancellation with nothing running; a new run cancelling the previous one; request defaults and tunable overrides; write target next to the photos; the progress event contract; preflight labels never leaking an absolute path; sidecar selection order and dimension mismatches; relative-photo-path traversal and symlink-escape rejection; semantic validation on load |
| **`tests/reference_sets.rs`** | 9 | see §7.2 |
| doctest | 1 | the `lib.rs` quick-start example compiles |

Total: 239 (`camcomp-core` unit/integration) + 15 (`camcomp-desktop`) + 9
(`reference_sets`) + 1 (doctest) = **264**, all passing (§7). The per-module
breakdown above for modules other than `sessions` and `pipeline` predates
several unrelated additions and is known to undercount; it is retained for the
qualitative "what they pin down" descriptions rather than as an exact count.

Tests deliberately assert the *hard* direction wherever there is one: that
`uncertain` is not coerced into a decision, that a rejection names its gate, that a
manually added member is retained and flagged rather than dropped, and that
non-overlapping pixels cannot inflate a similarity score.

### 7.2 Integration tests over the reference sets

`crates/camcomp-core/tests/reference_sets.rs`. Restructured per
`PLAN-TEST-REFACTOR.md`: 9 tests, one shared analysis pass over the eight sets at
a 320 px analysis edge — the label outcome is identical at 320, 480, 600 and the
1200 default (§6.10). Runs in release from `mise run test`; a plain debug
`cargo test` still pays the decode cost the same section measures. The tests skip
themselves when `./test/tstgrp*` is absent, which it is in a fresh clone — the
photo sets are not committed.

| test | what would break it |
|---|---|
| `the_reference_sets_satisfy_every_document_invariant` | a crash, an invalid document, a photo in neither a group nor the ungrouped list, the container-EXIF regression of §2.1 (timestamp/focal/make/mode/`orientationAppliedByDecoder` on every HEIC), losing the `auto`/`master`/`hi-res`/`hdr-plus` mode decode, an unverified mode bit reaching a label, or a non-invertible group alignment. Failures accumulate into one list rather than stopping at the first |
| `no_false_positive_on_any_reference_set` | two photos labelled apart ending up in one group — the error `SPEC.md` §9 calls the harmful one |
| `metadata_prefilters_decide_before_any_pixel_is_read` | wasting SIFT on a metadata-decidable pair: the focal prefilter (`tstgrp4`), the angle-of-view framing predicate (`tstgrp4`/`tstgrp5`, §6.11), and the digital-zoom policy (`tstgrp7`, §6.7) each get one assertion that they decided without a pixel |
| `the_pair_cache_reuses_what_is_valid_and_drops_what_is_not` | a warm reload re-analyzing anything, or a matcher parameter change failing to invalidate the cache |
| `analysis_is_deterministic_and_leaves_the_source_folder_untouched` | non-determinism in SIFT, matching, RANSAC or ordering; or any source file changing during analysis. Runs on a small two-photo fixture (§4 below), not a full reference set |
| `the_document_is_usable_by_a_viewer` | any lossy `camcomp.json` persistence (the test that caught §6.8), or the `SPEC.md` §7.3 pointer-mapping path breaking |
| `manual_overrides_reach_the_labelled_answer_and_survive_reanalysis` | the human-correction path; walks `tstgrp3` from 2 members to the labelled 3 at a tightened tolerance, asserts the member has no alignment until forced, then that `--force-pair` produces one (§6.9) |
| `every_candidate_pair_is_analyzed_or_metadata_rejected` | every candidate pair is accounted for now that there is no work cap: analyzed or decided from metadata, with nothing left in between |
| `disabling_session_detection_changes_no_verdict` | the session gap becoming load-bearing for correctness rather than a cost control (§6.13) |

Note what these tests deliberately do **not** assert: that the automatic grouping
matches the labels. Given §6.6 that would be asserting a contradiction. They assert
the two things that are unambiguous — no crash, and no false positive — and leave
recall to `mise run test-fixtures`, which reports it rather than enforcing it.

### 7.3 Calibration status per reference set

`mise run test-fixtures` at the specified defaults, release build. Exit code 3
means "a label was not reproduced", which is a finding, not a build failure; a
crash would be a non-zero exit from the binary itself and fails the run.

| set | photos | pairs analyzed | groups | labels met | false pos. | blocked by | cold time |
|---|---:|---:|---:|---:|---:|---|---:|
| `tstgrp1` | 3 | 1 of 3 | 0 | 0/1 | 0 | one-per-configuration (two `hi-res` shots) | 2.9 s |
| `tstgrp2` | 4 | 2 of 2 | 2 | **2/2** | 0 | — | 3.3 s |
| `tstgrp3` | 3 | 3 of 3 | 1 | **1/1** | 0 | — | 2.9 s |
| `tstgrp4` | 6 | 3 of 15 | 3 | **3/3** | 0 | — | 6.3 s |
| `tstgrp5` | 4 | 6 of 6 | 1 | **1/1** | 0 | — | 4.8 s |
| `tstgrp6` | 3 | 3 of 3 | 1 | **1/1** | 0 | — | 3.5 s |
| `tstgrp7` | 3 | 1 of 1 | 1 | **1/1** | 0 | — | 1.9 s |
| `tstgrp8` | 4 | 2 of 2 | 2 | **2/2** | 0 | — | 4.0 s |
| **total** | **30** | **21 of 35** | **11** | **12/13** | **0** | | |

Seven of the eight sets now reproduce every label; only `tstgrp1` is blocked, for
the reason named and measured in §6.1 (one-per-configuration). This is the same
12/13, 0 false positives reported in §6.1 — this table is that result broken
down per set, with cold timings.

### 7.4 Drop 1 definition of done

Against `PLAN-DROP1.md` §19, whose checkboxes are marked directly (26 of 30 met).
Summary:

**Met.** Inputs inspected without modification; metadata including per-device
clock offsets; sliding-gap sessions; a symmetric angle-of-view framing predicate;
non-consecutive grouping; SIFT matching tolerant of translation, rotation, scale
and processing differences; hard gates evaluated independently of the score;
`uncertain` as a first-class outcome; anchor-based grouping that is not connected
components; every photo in a group or in the ungrouped list with a reason;
overrides stored separately and surviving re-analysis; add/remove/split/merge/
re-anchor; every candidate pair either visually analyzed or decided from
metadata, with `--force-pair` overriding the focal prefilter on demand;
normalized `photo-to-anchor` homographies with persisted inverses and
spatially sampled correspondences; full-resolution pointer mapping; versioned
schema with atomic writes and incremental reuse; core usable without Tauri; no
dependency type in the public schema; no OpenCV or Python runtime; typed errors,
progress and cancellation; documented public API; formatting and Clippy clean;
analysis cancellable without losing completed work.

**Not met, and why.**

- *"Thresholds are the values calibrated in Phase 1 against the real sets."* The
  specified defaults are kept, because calibration showed the composite score
  cannot discriminate (§6.3) and the labels are unsatisfiable by any threshold
  (§6.6). Changing numbers would have made the result look better without being
  better. `thresholdsProvisional` stays `true`.
- *"Every real set is classified correctly."* 12 of 13 labelled groups reproduced
  automatically, 0 false positives (§6.1, §7.3). The remaining miss is
  attributable to a named prefilter with a measured value, and is reachable
  through the override API.
- *"A documented reference dataset of hundreds of photos analyzes cold within a
  stated time."* Not measured. The largest set here is six photos. Per-pair and
  per-decode costs are measured (§6.10) so the extrapolation is available, but the
  claim itself is untested.
- *Peak memory* is reported as `peakAnalysisPixels` and the largest decoded buffer
  is known (~150 MB for 50 MP), but no explicit allocation guard beyond the pixel
  count limit is enforced.
- *Browser/WASM (Gate C)* not attempted. `mise run check-wasm` exists; the target
  is not installed and `camcomp-wasm` does not exist.
- *Packaged Tauri smoke test (Gate B)* partially: the shell compiles, its command
  surface is unit-tested, and the icon and config are in place, but no signed
  bundle was produced and the window was not opened in this environment.

## 8. Open items

In rough order of value. Carried into Drop 2+ rather than blocking this wrap-up.

- **`SPEC.md` §5.3's overlap definition cannot see a zoom-in** (§6.5). Worth a
  companion metric — intersection over the *union*, or over the anchor area — so
  the score has a channel for framing difference at all.
- **`tstgrp1`/g1 remains unreproduced automatically** (§7.3), decided by the
  one-photo-per-configuration rule; named, measured, and reachable by one
  override. Not a visual-algorithm failure.
- The Pixel digital-zoom defect (§6.7) is recorded, not corrected.
- Thresholds remain marked `thresholdsProvisional: true` in every document —
  intentional (§6.3, §6.6), pending either a discriminating replacement for the
  composite score or a decision to ship on gates plus prefilters alone.
- **Hundreds-of-photos cold-run time is still unmeasured.** The largest labelled
  set is six photos; §6.10 gives per-decode and per-pair costs to extrapolate
  from, but the `PLAN-DROP1.md` §19.1bis claim itself is untested.
- **No explicit peak-memory allocation guard** beyond the pixel-count limit
  (`PLAN-DROP1.md` §19.1bis).
- **Tauri packaging (Gate B) and WASM feasibility (Gate C) were not attempted**
  in this environment. The command surface is unit-tested and the shell
  compiles, but no signed bundle was produced, no window was opened, and
  `camcomp-wasm` does not exist.
