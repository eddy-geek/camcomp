# Plan — reference-set test suite: cost and structure

- **Status:** Proposed. Nothing in this document is implemented beyond §3.
- **Trigger:** `cargo test --workspace` became unusable. A single test
  (`heic_metadata_is_actually_read`) ran for more than five minutes at 100 % of
  every core, ~700 MB resident, and was killed rather than waited out.
- **Scope:** `crates/camcomp-core/tests/reference_sets.rs` and the `mise` test
  tasks. No engine behaviour changes.
- **Sources:** `IMPLEM-DROP1.md` §6.10 (runtime), §7.2 (test inventory),
  `PLAN-DROP1.md` §11.1.

## 1. Diagnosis: not a bug

Verified, in this order:

1. **The engine is not looping or leaking.** The same work in a release build
   finishes in seconds, and the memory figure is explained: the peak decoded
   buffer for an 8192 × 6144 photo is ~150 MB (`IMPLEM-DROP1.md` §6.10), and
   `cargo test` runs one test per core, so several of those coexist.
2. **An unoptimized build is ~22× slower on this workload.** Measured on
   `tstgrp6` (3 photos, 3 analyzed pairs, analysis edge 480):

   | build | wall | CPU |
   |---|---:|---:|
   | release | 3.1 s | — |
   | debug | **69 s** | 98 s |

   The CPU figure exceeding wall time means `sift-features` parallelizes
   internally; the suite therefore cannot be made faster by using more cores,
   they are already busy.
3. **The suite multiplied that cost by the number of tests.** Before the change
   in §3, five tests each analyzed all eight sets from scratch and four more
   analyzed a set each: **nine full-fidelity passes over real photos where one
   would have done.** Every test called `analyze()` in its own body; nothing was
   shared.
4. **The recent algorithm change made it worse by ~1.7×.** Analyzed pairs across
   the eight sets went from 9 to 15, because a 60 s session gap and a relaxed
   framing predicate admit more pairs to the visual half (`IMPLEM-DROP1.md`
   §6.11–§6.13). That is the intended behaviour of the *product* showing up as an
   unintended cost in the *suite*.

So the suite was always expensive and nobody had measured it; the algorithm change
pushed it past the point where a developer would wait.

## 2. Cost model

What one full-fidelity pass over the eight sets costs, in release, from
`IMPLEM-DROP1.md` §6.10 and measurements at the suite's reduced resolution:

| component | per unit | units per pass | note |
|---|---|---|---|
| HEIC decode | 245–407 ms (50 MP) | ~20 photos | **independent of the analysis edge** — the file is decoded in full, then downscaled |
| SIFT + match + register | ~0.9 s at 1200 px, ~0.1 s at 320 px | 15 pairs | scales with analysis pixels, and matching scales with the *square* of the keypoint count |
| everything else | negligible | | |

Two consequences that decide the plan:

- **At the suite's low resolution, decode dominates.** Reducing the analysis edge
  makes matching cheap but leaves the decode floor untouched. The only way to
  reduce decode cost is to decode fewer photos, which means analyzing fewer sets
  or fewer pairs.
- **In a debug build the decode share is unknown and needs measuring first**
  (§4 step 0). `heif-oxide` and `rust_h265` are pure Rust with bounds checks on
  every access, so their unoptimized penalty may be far worse than the 22× of the
  workload as a whole. If decode turns out to be, say, 70 % of the debug cost, then
  lowering the analysis edge buys almost nothing and the plan must lean on
  analyzing fewer photos instead.

## 3. What has already landed

In the working tree, verified by `cargo test -p camcomp-core --test reference_sets
--release`:

- **One shared analysis pass.** `shared_documents()` analyzes each set once per
  test process behind a `OnceLock`; the documents are immutable and read-only, so
  sharing cannot leak state between tests. Tests needing different parameters, a
  second independent run, or a controlled moment still analyze for themselves.
- **Three new tests** pinning the recent algorithm changes on real photos: the
  framing predicate on the ultrawide pair against the zoom step, the digital-zoom
  policy rewriting only the device that needs it, and session detection being
  switched off without changing a verdict.
- **Label renames** followed through (`auto`, `hi-res`), plus an assertion that
  unknown mode bits remain classified and do not stop the rest of the analysis;
  the complete OPPO evidence is maintained in [`SPEC-OPPO.md`](SPEC-OPPO.md).

Result: **17 tests, 13 s, release.** The debug figure is still unmeasured and is
the open problem.

## 4. Plan

Ordered by evidence needed, then by effect.

### Step 0 — measure the decode share in a debug build

Nothing else in this plan should be decided without it. Time, in debug, on one set
whose photos are 50 MP:

```sh
./target/debug/camcomp --quiet inspect test/tstgrp1              # header only, no full decode
./target/debug/camcomp --quiet analyze test/tstgrp1 --dry-run --no-cache --analysis-edge 480
./target/debug/camcomp --quiet analyze test/tstgrp1 --dry-run --no-cache --analysis-edge 320
```

`inspect` was measured at 2.0 s for three photos in debug, which already shows it
does not decode in full. The difference between the two `analyze` runs isolates the
resolution-dependent half; what remains is decode. Record both in
`IMPLEM-DROP1.md` §6.10 — the engine's debug cost is worth knowing anyway.

**Decision rule:** if resolution-dependent work is the majority, do steps 1–3. If
decode is the majority, do steps 1, 2 and 4, and treat step 3 as a rounding error.

### Step 1 — run the reference sets in release, from `mise`

The integration suite is a release-shaped workload: real photos, real SIFT. Split
the `test` task so the unit tests keep debug assertions and the reference sets do
not pay for them.

```toml
[tasks.test]
run = [
    "cargo test --workspace --all-features --lib --bins",
    "cargo test --workspace --all-features --doc",
    "cargo test -p camcomp-core --all-features --release --test reference_sets",
]
```

Cost: one extra compilation profile for the test target, cached after the first
run. `mise run build` already produces the release artifacts.

**This is the highest-value step and it is nearly free.** It does not excuse the
suite from being cheap in debug — a developer running plain `cargo test` must not
be punished — but it removes the pain from the sanctioned path.

### Step 2 — restructure around the single pass, and merge by concern

The current shape is 17 tests, of which nine only assert properties of documents
another test already paid for. Collapse to **nine tests** whose names still say
what broke, with the breadth test accumulating failures per concern rather than
panicking on the first one — with real photos, seeing all violations at once is
better diagnostics than seeing the first.

| new test | absorbs | analysis cost |
|---|---|---|
| `the_reference_sets_satisfy_every_document_invariant` | `every_reference_set_analyzes_and_validates`, `heic_metadata_is_actually_read`, `oppo_shooting_modes_split_into_distinct_configurations`, `every_group_member_has_an_invertible_normalized_alignment` | shared pass |
| `no_false_positive_on_any_reference_set` | unchanged — the primary objective (`SPEC.md` §9) keeps its own name in the output | shared pass |
| `metadata_prefilters_decide_before_any_pixel_is_read` | `the_focal_prefilter_spends_no_visual_analysis_on_a_known_mismatch`, `the_framing_predicate_separates_two_ultrawides_from_a_zoom_step`, `the_digital_zoom_policy_rewrites_only_the_device_that_needs_it` | shared pass |
| `the_pair_cache_reuses_what_is_valid_and_drops_what_is_not` | `a_warm_reload_reuses_pair_results_and_decodes_nothing`, `changing_a_threshold_invalidates_the_cached_pairs` | shared cold + 2 warm reruns (1 pair) |
| `analysis_is_deterministic_and_leaves_the_source_folder_untouched` | `analysis_is_reproducible_across_runs`, `source_photos_are_never_modified` | 1 fresh pass over the cheapest fixture |
| `the_document_is_usable_by_a_viewer` | `a_document_written_beside_the_photos_reloads_unchanged`, `pointer_mapping_between_two_members_round_trips_at_full_resolution` | shared pass |
| `manual_overrides_reach_the_labelled_answer_and_survive_reanalysis` | unchanged | shared cold + 1 forced pair |
| `the_pair_budget_bounds_the_work_and_reports_what_it_skipped` | unchanged | 2 cold passes with custom parameters |
| `disabling_session_detection_changes_no_verdict` | unchanged | 1 cold pass, `tstgrp7` |

Merging determinism with source-immutability is the one merge that is about cost
rather than coherence: both need a *fresh* run, and one fresh run serves both if it
happens between the two digests. That is worth a comment in the test, not a
comment in a plan.

**No coverage is dropped.** Every assertion in the table's right-hand column
survives; only the `#[test]` boundaries move.

### Step 3 — analysis edge 480 → 320 for the suite

Verified across all eight sets, in release: **the label outcome is identical at
240, 320, 360, 480 and 1200.** Margins at 320, against the shipped thresholds:

| quantity | at 480 | at 320 | floor |
|---|---:|---:|---:|
| keypoints, weakest photo | 623 | 277 | 150 (`minKeypointsForDecision`) |
| inliers, weakest pair | 153 | 82 | 25 (`minInliers`) |
| composite score, weakest pair | 0.917 | 0.892 | 0.80 |

320 halves the analysis pixels and cuts matching cost by roughly 5× (keypoints
scale with area, matching with their square). **Do not go to 240**: the labels
still reproduce, but the weakest photo lands within a factor of ~1.5 of the
keypoint floor, and a suite that runs one bad photo away from a cliff will fail for
reasons unrelated to the change under test.

Conditional on step 0: if decode dominates in debug, this step is cosmetic.

### Step 4 — a small fixture for the tests that need a fresh analysis

Only if step 0 shows decode dominating, or if the debug suite is still over ~2
minutes after steps 1–3.

Three tests need a *fresh* analysis rather than the shared one. Today each pays for
a whole set. Instead, build the cheapest possible real input: a temporary directory
of symlinks to two photos chosen for size, not for content — `tstgrp2`'s 12 MP HEIC
and its Pixel JPEG rather than any 50 MP hi-res shot. One pair, two decodes, real
device metadata, and it never writes into a source folder.

```text
fixture_pair() -> TempDir   // symlinks; skips itself if the photos are absent
```

What this must **not** become: a synthetic or downscaled photo set. The entire
value of this suite is that it runs on unmodified device output — that is what
caught the HEIC container EXIF limit and the `serde_json` float round-trip
(`IMPLEM-DROP1.md` §6.8). Cheap real photos, yes; fake photos, no.

`the_pair_budget_bounds_the_work_and_reports_what_it_skipped` is the exception: it
needs a session with many candidate pairs, so it stays on `tstgrp4` and stays
expensive. Its budget is already 2 pairs; the cost is the decodes.

### Step 5 — make the cost visible so this cannot recur silently

The failure here was not that the suite was slow, it was that nobody knew.

- Have the shared pass print, once, how long it took and at what analysis edge, so
  a debug run announces its own price instead of appearing to hang.
- Record the measured suite runtime, debug and release, in `IMPLEM-DROP1.md`
  §6.10 alongside the engine numbers, and treat a regression in it as a finding.
- Consider a `mise run test-fixtures:quick` for the inner loop, and leave the full
  labelled run to `mise run check`.

## 5. Rejected alternatives

- **Skip or subset the sets in debug builds.** Fastest fix, and dishonest: the
  default `cargo test` would silently verify less than it appears to. If a set is
  not worth running, delete it from the suite and say so.
- **Cache analyses across processes** (a `camcomp.json` in a temp directory reused
  between runs). Turns a slow suite into a stateful one, and a stale cache would
  hide exactly the regressions this suite exists to catch.
- **Lower the gates so a smaller analysis image passes comfortably.** That tests an
  algorithm nobody ships.
- **Move the reference sets out of `cargo test` entirely** (into `mise run
  test-fixtures` only). Loses the assertions that are not about labels — document
  validity, cache round-tripping, source immutability — which are the ones that
  have actually caught bugs.

## 6. Acceptance criteria

1. `cargo test -p camcomp-core --test reference_sets --release` stays under 1 minute
   and reports the same verdicts as today.
2. A plain debug `cargo test --workspace` completes in a few minutes at worst, and
   says what it is spending the time on.
3. Every assertion listed in §4 step 2 still exists, and the reference sets still
   reproduce 11 of 13 labels with zero false positives at the shipped defaults.
4. The measured runtimes, debug and release, are written down in
   `IMPLEM-DROP1.md` §6.10.
