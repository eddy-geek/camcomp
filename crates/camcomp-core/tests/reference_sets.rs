//! End-to-end tests over the labelled reference sets in `./test`.
//!
//! These are the tests that would have caught the HEIC EXIF problem: every set
//! is real device output, and a metadata regression turns straight into a
//! grouping regression.
//!
//! They skip themselves when `./test/tstgrp*` is absent, so a checkout without
//! the photo sets still passes. The analysis edge is reduced to keep the suite
//! usable; the label outcome is identical at 320, 480, 600 and the 1200 default
//! (see `artifacts/IMPLEM-DROP1.md` §6.10). In a debug build the resolution
//! barely matters: HEIC decode, which does not shrink with the analysis edge,
//! is the majority of the cost (`artifacts/IMPLEM-DROP1.md` §6.10, §0 measurement),
//! which is why this suite runs in release from `mise run test` (`mise.toml`).
//!
//! `shared_documents()` analyzes each set once per test process and reports how
//! long that took, so a debug run announces its own price instead of appearing
//! to hang (`PLAN-TEST-REFACTOR.md` §5).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use camcomp_core::cache;
use camcomp_core::domain::MetadataReader;
use camcomp_core::imageio::probe_dimensions;
use camcomp_core::metadata::{identify_mode_with_dimensions, ExifMetadataReader};
use camcomp_core::pipeline::{self, AnalysisOptions};
use camcomp_core::schema::{
    AnalysisDocument, FocalCompatibility, FocalSource, PairReason, PairState, UngroupedReason,
};

/// Reduced analysis resolution, so the suite stays cheap. Verified across all
/// eight reference sets in release: the label outcome, and the margins against
/// every gate, are unchanged from 480 down to 320 (`PLAN-TEST-REFACTOR.md` §3).
/// Do not go lower: at 240 the weakest photo's keypoint count sits within a
/// factor of ~1.5 of `minKeypointsForDecision`, and a suite that runs one bad
/// photo away from a cliff fails for reasons unrelated to the change under
/// test.
const TEST_ANALYSIS_EDGE: u32 = 320;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace layout")
}

/// The labelled sets, or an empty list when the photos are not present.
fn reference_sets() -> Vec<PathBuf> {
    let test_dir = repository_root().join("test");
    let Ok(entries) = std::fs::read_dir(&test_dir) else {
        return Vec::new();
    };
    let mut sets: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("tstgrp"))
                    .unwrap_or(false)
        })
        .collect();
    sets.sort();
    sets
}

fn options() -> AnalysisOptions {
    let mut options = AnalysisOptions::with_reuse();
    options.algorithm.parameters.analysis_longest_edge = TEST_ANALYSIS_EDGE;
    options
}

fn analyze(set: &Path, options: &AnalysisOptions) -> AnalysisDocument {
    let inputs = [set.to_path_buf()];
    pipeline::analyze_quiet(&inputs, options, None)
        .unwrap_or_else(|e| panic!("{} failed to analyze: {e}", set.display()))
}

/// Every set analyzed **once per test process**, at [`options`].
///
/// Analyzing real photos is the entire cost of this suite: SIFT plus brute-force
/// matching over real descriptors, and HEIC decode on top of that. Most of the
/// tests here assert different properties of the *same* documents, so
/// recomputing them per test multiplied that cost by the number of tests — nine
/// full passes over eight sets where one would do.
///
/// The documents are immutable and only read, so sharing them cannot leak state
/// between tests. A test that needs different parameters, a second independent
/// run, or the analysis to happen at a controlled moment analyzes for itself and
/// says why.
fn shared_documents() -> &'static BTreeMap<PathBuf, AnalysisDocument> {
    static DOCUMENTS: OnceLock<BTreeMap<PathBuf, AnalysisDocument>> = OnceLock::new();
    DOCUMENTS.get_or_init(|| {
        let sets = reference_sets();
        let started = Instant::now();
        let documents: BTreeMap<PathBuf, AnalysisDocument> = sets
            .into_iter()
            .map(|set| {
                let document = analyze(&set, &options());
                (set, document)
            })
            .collect();
        eprintln!(
            "shared reference-set pass: {} set(s) analyzed in {:.1}s at analysis edge {}",
            documents.len(),
            started.elapsed().as_secs_f64(),
            TEST_ANALYSIS_EDGE
        );
        documents
    })
}

fn shared(set: &Path) -> &'static AnalysisDocument {
    shared_documents()
        .get(set)
        .unwrap_or_else(|| panic!("{} was not analyzed", set.display()))
}

/// Group label from the filename convention: `g1-`, `g2-`, `gx-`.
fn label_of(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let prefix = name.split('-').next()?;
    if prefix.len() > 1 && prefix.starts_with('g') {
        Some(prefix)
    } else {
        None
    }
}

fn labels_by_id(document: &AnalysisDocument) -> BTreeMap<&str, &str> {
    document
        .photos
        .iter()
        .filter_map(|p| label_of(&p.path).map(|l| (p.id.as_str(), l)))
        .collect()
}

fn set_name(set: &Path) -> String {
    set.file_name().unwrap().to_string_lossy().to_string()
}

fn calibration_mode_from_filename(name: &str) -> &'static str {
    if name.contains("Mode_Hi-Res") {
        "hi-res"
    } else if name.contains("Mode_Portrait") {
        "portrait"
    } else if name.contains("Mode_Night") {
        "night"
    } else if name.contains("Mode_Text") {
        "text"
    } else if name.contains("Mode_Master") {
        "master"
    } else {
        "auto"
    }
}

#[test]
fn oppo_metadata_calibration_matches_the_renamed_mode_labels() {
    let directory = repository_root().join("data").join("metadata-calibration");
    if !directory.is_dir() {
        eprintln!("no metadata calibration directory; skipping");
        return;
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("read metadata calibration directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("jpg") | Some("heic")
            )
        })
        .collect();
    files.sort();

    let reader = ExifMetadataReader;
    assert_eq!(
        files.len(),
        32,
        "calibration corpus changed; update this test"
    );
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        let bytes = std::fs::read(&path).expect("read calibration photo");
        let raw = reader.read(&bytes);
        let dimensions = probe_dimensions(&path, &bytes).expect("probe calibration dimensions");
        let mode = identify_mode_with_dimensions(&raw, dimensions);
        let base_label = mode
            .label
            .as_deref()
            .unwrap_or("-")
            .split("-x")
            .next()
            .unwrap();
        assert_eq!(base_label, calibration_mode_from_filename(&name), "{name}");

        let flash_fired = raw.flash.map(|value| value & 0x0001 != 0).unwrap_or(false);
        assert_eq!(
            mode.flags.iter().any(|flag| flag == "flash-fired"),
            flash_fired,
            "{name}"
        );
    }
}

/// A temporary directory removed on drop, even if the test panics.
struct TempFixtureDir {
    path: PathBuf,
}

impl Drop for TempFixtureDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A minimal real-photo pair for the tests that need a *fresh* analysis rather
/// than the shared one: `tstgrp2`'s 12 MP Oppo HEIC and its Pixel JPEG
/// counterpart, chosen for size rather than content. Linked into a temporary
/// directory so the test never writes into a source folder, and skips itself
/// when the reference photos are absent.
///
/// This must never become a synthetic or downscaled photo: the value of this
/// suite is that it runs on unmodified device output, which is what caught the
/// HEIC container EXIF limit and the `serde_json` float round-trip
/// (`IMPLEM-DROP1.md` §6.8). Cheap real photos, not fake ones
/// (`PLAN-TEST-REFACTOR.md` §4 step 4).
fn fixture_pair() -> Option<TempFixtureDir> {
    let source = repository_root().join("test").join("tstgrp2");
    let names = [
        "g2-oppo-IMG20260822115305.heic",
        "g2-PXL_20260822_095305153.jpg",
    ];
    let originals: Vec<PathBuf> = names.iter().map(|n| source.join(n)).collect();
    if originals.iter().any(|p| !p.is_file()) {
        return None;
    }

    let dir = std::env::temp_dir().join(format!(
        "camcomp-fixture-pair-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create fixture dir");
    for original in &originals {
        let name = original.file_name().unwrap();
        let link = dir.join(name);
        #[cfg(unix)]
        std::os::unix::fs::symlink(original, &link).expect("symlink fixture photo");
        #[cfg(not(unix))]
        std::fs::copy(original, &link).expect("copy fixture photo");
    }
    Some(TempFixtureDir { path: dir })
}

/// Digest of every regular file directly inside a folder, by filename.
fn digest_folder(dir: &Path) -> BTreeMap<String, String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().to_string(),
                cache::sha256_file(&p).unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_reference_sets_satisfy_every_document_invariant() {
    // Real-photo diagnostics are worth seeing all at once rather than stopping
    // at the first violation, so every concern below accumulates into one list.
    let sets = reference_sets();
    if sets.is_empty() {
        eprintln!("no ./test/tstgrp* sets present; skipping");
        return;
    }
    let mut failures: Vec<String> = Vec::new();

    // Every set analyzes and validates; grouping must not silently discard
    // input (`REQUIREMENTS.md` §3).
    for set in &sets {
        let document = shared(set);
        if let Err(e) = document.validate() {
            failures.push(format!("{}: invalid document: {e}", set_name(set)));
        }
        if document.photos.is_empty() {
            failures.push(format!("{}: no photos discovered", set_name(set)));
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for group in &document.groups {
            for id in &group.photo_ids {
                if !seen.insert(id.as_str()) {
                    failures.push(format!("{}: {id} appears twice", set_name(set)));
                }
            }
        }
        for item in &document.ungrouped {
            if !seen.insert(item.photo_id.as_str()) {
                failures.push(format!(
                    "{}: {} appears twice",
                    set_name(set),
                    item.photo_id
                ));
            }
        }
        if seen.len() != document.photos.len() {
            failures.push(format!(
                "{}: some photo is in neither a group nor the ungrouped list",
                set_name(set)
            ));
        }
    }

    // HEIC metadata is actually read. Regression guard for the container EXIF
    // size limit: without the ISO-BMFF fallback in `heif.rs`, every Oppo HEIC
    // loses its timestamp, focal length and mode tag, and the whole pipeline
    // degrades to "unusable timestamp".
    let mut heic_seen = 0usize;
    for set in &sets {
        let document = shared(set);
        for photo in &document.photos {
            if !photo.path.to_ascii_lowercase().ends_with(".heic") {
                continue;
            }
            heic_seen += 1;
            let meta = &photo.metadata;
            if meta.captured_at.is_none() {
                failures.push(format!(
                    "{}: {} has no capture timestamp",
                    set_name(set),
                    photo.path
                ));
            }
            if meta.focal_length_35mm.is_none() {
                failures.push(format!(
                    "{}: {} has no 35 mm equivalent focal length",
                    set_name(set),
                    photo.path
                ));
            }
            if meta.camera_make.as_deref() != Some("OPPO") {
                failures.push(format!(
                    "{}: {} lost its camera make",
                    set_name(set),
                    photo.path
                ));
            }
            if meta.mode_label.is_none() {
                failures.push(format!(
                    "{}: {} has no shooting mode ([Q10])",
                    set_name(set),
                    photo.path
                ));
            }
            // The HEIF decoder applies the container transform itself, so the
            // EXIF value must not be applied a second time.
            if !meta.orientation_applied_by_decoder {
                failures.push(format!(
                    "{}: {} would be rotated twice",
                    set_name(set),
                    photo.path
                ));
            }
        }
    }
    if heic_seen < 15 {
        failures.push(format!("only {heic_seen} HEIC photos inspected"));
    }

    // Oppo shooting modes split into distinct configurations. Observed across
    // the reference sets: the plain mode, Master mode, the 50 MP high-resolution
    // mode, and the Pixel's HDR+ pipeline.
    let mut modes: BTreeSet<String> = BTreeSet::new();
    for set in &sets {
        for config in &shared(set).configurations {
            if let Some(mode) = &config.mode_label {
                modes.insert(mode.clone());
            }
        }
    }
    for expected in ["auto", "master", "hi-res", "hdr-plus"] {
        if !modes.contains(expected) {
            failures.push(format!(
                "mode `{expected}` was never identified; found {modes:?}"
            ));
        }
    }
    // Unknown mode bits are expected to remain visible and must not block the
    // rest of the analysis. The metadata decoder decorates their mode label and
    // preserves the raw bits, so a future device is classified rather than
    // silently merged into a known pipeline (`IMPLEM-DROP1.md` §2.1).
    for set in &sets {
        for photo in &shared(set).photos {
            let Some(bits) = photo.metadata.mode_tag_unknown_bits.as_deref() else {
                continue;
            };
            let classified = photo
                .metadata
                .mode_label
                .as_deref()
                .map(|label| label.contains("-x"))
                .unwrap_or(false);
            if !classified {
                failures.push(format!(
                    "{}: {} has unclassified mode bits {bits:?}",
                    set_name(set),
                    photo.path
                ));
            }
        }
    }

    // Every group member has an invertible normalized alignment. Full-resolution
    // pointer mapping: the persisted transform is in normalized coordinates, so
    // it must round-trip at any scale (`SPEC.md` §7.3).
    let mut alignments_checked = 0usize;
    for set in &sets {
        let document = shared(set);
        for group in &document.groups {
            for member in group
                .photo_ids
                .iter()
                .filter(|id| **id != group.anchor_photo_id)
            {
                let Some(alignment) = group.alignments.iter().find(|a| a.photo_id == *member)
                else {
                    failures.push(format!("{}: {member} has no alignment", set_name(set)));
                    continue;
                };
                if alignment.homography.len() != 9 || alignment.inverse_homography.len() != 9 {
                    failures.push(format!(
                        "{}: {member} has a malformed homography",
                        set_name(set)
                    ));
                    continue;
                }
                let (Some(h), Some(inv)) = (
                    camcomp_core::geometry::Mat3::from_row_major(&alignment.homography),
                    camcomp_core::geometry::Mat3::from_row_major(&alignment.inverse_homography),
                ) else {
                    failures.push(format!(
                        "{}: {member} has a non-finite homography",
                        set_name(set)
                    ));
                    continue;
                };

                for probe in [[0.5, 0.5], [0.2, 0.8], [0.9, 0.1]] {
                    let Some(mapped) = h.apply(probe) else {
                        failures.push(format!("{}: {member} maps to infinity", set_name(set)));
                        continue;
                    };
                    let Some(back) = inv.apply(mapped) else {
                        failures.push(format!("{}: {member} does not invert", set_name(set)));
                        continue;
                    };
                    let error = (back[0] - probe[0]).hypot(back[1] - probe[1]);
                    if error >= 1e-9 {
                        failures.push(format!(
                            "{}: {member} round-trip error {error:.3e}",
                            set_name(set)
                        ));
                    }
                }
                if alignment.correspondences.is_empty()
                    && alignment.provenance == camcomp_core::schema::AlignmentProvenance::DirectPair
                {
                    failures.push(format!(
                        "{}: a direct-pair alignment carried no correspondences",
                        set_name(set)
                    ));
                }
                for c in &alignment.correspondences {
                    if !(0.0..=1.0).contains(&c.photo[0])
                        || !(0.0..=1.0).contains(&c.photo[1])
                        || !(0.0..=1.0).contains(&c.anchor[0])
                        || !(0.0..=1.0).contains(&c.anchor[1])
                        || !c.reprojection_error_normalized.is_finite()
                    {
                        failures.push(format!(
                            "{}: {member} has an out-of-range correspondence",
                            set_name(set)
                        ));
                    }
                }
                alignments_checked += 1;
            }
        }
    }
    if alignments_checked == 0 {
        failures.push("no group member was checked for alignment invertibility".into());
    }

    assert!(
        failures.is_empty(),
        "{} violation(s):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[test]
fn no_false_positive_on_any_reference_set() {
    // The primary calibration objective (`SPEC.md` §9): a false negative costs
    // one manual action, a false positive silently corrupts a comparison.
    let sets = reference_sets();
    if sets.is_empty() {
        eprintln!("no ./test/tstgrp* sets present; skipping");
        return;
    }
    let mut offenders: Vec<String> = Vec::new();
    for set in &sets {
        let document = shared(set);
        let labels = labels_by_id(document);
        let path_of: BTreeMap<&str, &str> = document
            .photos
            .iter()
            .map(|p| (p.id.as_str(), p.path.as_str()))
            .collect();
        for group in &document.groups {
            for i in 0..group.photo_ids.len() {
                for j in (i + 1)..group.photo_ids.len() {
                    let (a, b) = (&group.photo_ids[i], &group.photo_ids[j]);
                    let (Some(la), Some(lb)) = (labels.get(a.as_str()), labels.get(b.as_str()))
                    else {
                        continue;
                    };
                    // `gx` is deliberately undecided by the label.
                    if *la == "gx" || *lb == "gx" || la == lb {
                        continue;
                    }
                    offenders.push(format!(
                        "{}: {} + {}",
                        set_name(set),
                        path_of.get(a.as_str()).unwrap_or(&"?"),
                        path_of.get(b.as_str()).unwrap_or(&"?")
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "photos labelled apart were grouped together:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn metadata_prefilters_decide_before_any_pixel_is_read() {
    let sets = reference_sets();

    // tstgrp4 is the set with six photos across three focal lengths, so most of
    // its 15 candidate pairs are decided from metadata alone.
    if let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp4") {
        let document = shared(set);
        let stats = document.stats.clone().expect("stats");
        let focal_rejected = document
            .pairs
            .iter()
            .filter(|p| p.reason == Some(PairReason::FocalMismatch))
            .count();
        assert!(focal_rejected > 0, "expected some focal-only rejections");
        assert_eq!(
            stats.analyzed_pairs + focal_rejected,
            stats.candidate_pairs,
            "every candidate pair must be either analyzed or decided from metadata"
        );
        for pair in document
            .pairs
            .iter()
            .filter(|p| p.reason == Some(PairReason::FocalMismatch))
        {
            assert!(
                pair.metrics.is_none(),
                "{} was visually analyzed despite a focal mismatch",
                pair.id
            );
            assert_eq!(pair.state, PairState::Incompatible);
        }
    } else {
        eprintln!("tstgrp4 not present; skipping focal prefilter check");
    }

    // The pair of measurements the angle-of-view basis exists for
    // (`IMPLEM-DROP1.md` §6.11). By focal ratio these two are ordered the wrong
    // way round — 0.250 for the pair the user labelled together against 0.167
    // for a pair once labelled apart in the same set — so a focal-ratio
    // threshold cannot exist. Both measurements live inside `tstgrp4` itself:
    // `g2` is the accepted native-ultrawide pair, and every pair crossing `g3`
    // (one side digitally zoomed) is the rejected one.
    if let Some(wide) = sets.iter().find(|s| set_name(s) == "tstgrp4") {
        // Two native ultrawides, 15 mm against 12 mm: accepted under the
        // relaxed tier, and the document says which tier decided it.
        let document = shared(wide);
        let ultrawide = document
            .pairs
            .iter()
            .find(|p| {
                [&p.a, &p.b].iter().all(|id| {
                    document
                        .photo(id)
                        .map(|photo| photo.path.contains("g2-"))
                        .unwrap_or(false)
                })
            })
            .expect("the g2 ultrawide pair");
        assert!(
            ultrawide.focal.native_lens_pair,
            "both ultrawides are at a native lens step"
        );
        assert_eq!(ultrawide.focal.state, FocalCompatibility::Compatible);
        let fov = ultrawide.focal.relative_fov_difference.expect("fov diff");
        let focal = ultrawide
            .focal
            .relative_focal_difference
            .expect("focal diff");
        assert!((fov - 0.104).abs() < 0.005, "angular difference {fov:.3}");
        assert!((focal - 0.250).abs() < 0.005, "focal ratio {focal:.3}");
        assert!(
            document
                .groups
                .iter()
                .any(|g| g.photo_ids.contains(&ultrawide.a) && g.photo_ids.contains(&ultrawide.b)),
            "the labelled ultrawide pair must end up in one group"
        );

        // `g3`'s Pixel photo used digital zoom, so every pair crossing *it*
        // (not the Oppo `g3` photo, which is native) is rejected from
        // metadata, with no pixels spent, even though several of those pairs
        // have a *smaller* focal ratio than the accepted g2 pair.
        let across: Vec<_> = document
            .pairs
            .iter()
            .filter(|p| {
                let is_zoomed_g3 = |id: &String| {
                    document
                        .photo(id)
                        .map(|photo| photo.path.contains("g3-PXL"))
                        .unwrap_or(false)
                };
                let is_other_group = |id: &String| {
                    document
                        .photo(id)
                        .map(|photo| !photo.path.contains("g3-"))
                        .unwrap_or(false)
                };
                (is_zoomed_g3(&p.a) && is_other_group(&p.b))
                    || (is_zoomed_g3(&p.b) && is_other_group(&p.a))
            })
            .collect();
        assert!(!across.is_empty(), "expected cross-label pairs against g3");
        for pair in across {
            assert!(
                !pair.focal.native_lens_pair,
                "{}: one side used digital zoom, so the strict tier must apply",
                pair.id
            );
            assert_eq!(
                pair.focal.state,
                FocalCompatibility::Incompatible,
                "{}: framing difference {:?} was accepted",
                pair.id,
                pair.focal.relative_fov_difference
            );
            assert!(
                pair.metrics.is_none(),
                "{} was visually analyzed despite a framing mismatch",
                pair.id
            );
        }
    } else {
        eprintln!("tstgrp4 not present; skipping framing predicate check");
    }

    // tstgrp7 g2 is the measurement behind the digital-zoom policy: 35 mm on
    // the Oppo against 24 mm on the Pixel, a 46 % difference that the pixels
    // contradict (`IMPLEM-DROP1.md` §6.7). The Pixel reports its zoom
    // separately, the Oppo folds it in, and only the Pixel value may be
    // rewritten.
    if let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp7") {
        let mut rewritten = 0usize;
        for photo in &shared(set).photos {
            let m = &photo.metadata;
            let (Some(reported), Some(effective)) =
                (m.focal_length_35mm, m.effective_focal_length_35mm)
            else {
                panic!("{}: no focal length", photo.path);
            };
            let zoom = m.digital_zoom_ratio.unwrap_or(1.0);
            assert!(zoom > 1.0, "{}: expected a zoomed shot", photo.path);
            assert!(
                !m.native_lens,
                "{}: a zoomed shot is not native",
                photo.path
            );
            match m.camera_make.as_deref() {
                Some("OPPO") => {
                    assert_eq!(reported, effective, "{}: must be left alone", photo.path);
                    assert_eq!(m.focal_source, FocalSource::Exif35mm);
                }
                _ => {
                    assert!(
                        (effective - reported * zoom).abs() < 1e-9,
                        "{}: zoom not applied",
                        photo.path
                    );
                    assert_eq!(m.focal_source, FocalSource::Exif35mmZoomApplied);
                    rewritten += 1;
                }
            }
        }
        assert_eq!(rewritten, 1, "exactly one photo needed the correction");

        // And the correction is what lets the labelled pair form at all.
        let group = shared(set)
            .groups
            .first()
            .expect("the g2 pair must be grouped");
        assert_eq!(group.photo_ids.len(), 2);
    } else {
        eprintln!("tstgrp7 not present; skipping digital zoom policy check");
    }
}

#[test]
fn the_pair_cache_reuses_what_is_valid_and_drops_what_is_not() {
    let sets = reference_sets();
    // tstgrp6 has three mutually compatible photos, so there is real work to
    // reuse, and one shared cold document serves both checks below.
    let Some(set) = sets
        .iter()
        .find(|s| set_name(s) == "tstgrp6")
        .or_else(|| sets.first())
    else {
        eprintln!("no ./test/tstgrp* sets present; skipping");
        return;
    };

    let cold = shared(set);
    let cold_stats = cold.stats.clone().expect("stats");
    assert!(cold_stats.analyzed_pairs > 0, "nothing was analyzed cold");
    assert!(cold_stats.features_extracted > 0);

    // A warm reload reuses every pair and decodes nothing.
    let warm = pipeline::analyze_quiet(std::slice::from_ref(set), &options(), Some(cold)).unwrap();
    let warm_stats = warm.stats.clone().expect("stats");
    assert_eq!(
        warm_stats.analyzed_pairs, 0,
        "warm reload re-analyzed {} pair(s)",
        warm_stats.analyzed_pairs
    );
    assert_eq!(
        warm_stats.features_extracted, 0,
        "warm reload decoded {} photo(s)",
        warm_stats.features_extracted
    );
    assert_eq!(warm_stats.reused_pairs, cold_stats.analyzed_pairs);
    assert_eq!(warm.groups, cold.groups);
    assert_eq!(warm.ungrouped, cold.ungrouped);

    // Changing a matcher parameter invalidates every cached pair.
    let mut changed = options();
    changed.algorithm.parameters.lowe_ratio = 0.7;
    let rerun = pipeline::analyze_quiet(std::slice::from_ref(set), &changed, Some(cold)).unwrap();
    let stats = rerun.stats.clone().expect("stats");
    assert_eq!(
        stats.reused_pairs, 0,
        "a matcher parameter change must invalidate cached pairs"
    );
}

#[test]
fn analysis_is_deterministic_and_leaves_the_source_folder_untouched() {
    // Both checks need a *fresh* analysis rather than the shared one; running
    // it on the cheapest real fixture instead of a whole reference set is what
    // keeps this cheap. One fresh run's before/after digest proves nothing was
    // modified, and a second, deliberately independent run proves the answer
    // reproduces.
    let Some(fixture) = fixture_pair() else {
        eprintln!("tstgrp2 fixture photos not present; skipping");
        return;
    };

    let before = digest_folder(&fixture.path);
    let first = analyze(&fixture.path, &options());
    let second = analyze(&fixture.path, &options());

    assert_eq!(
        before,
        digest_folder(&fixture.path),
        "a source file changed during analysis"
    );
    assert!(
        cache::find_in_folder(&fixture.path).is_none(),
        "analysis must not write camcomp.json unless asked to"
    );

    assert_eq!(first.groups, second.groups);
    assert_eq!(first.ungrouped, second.ungrouped);
    let scores = |doc: &AnalysisDocument| -> Vec<(String, Option<f64>, PairState)> {
        doc.pairs
            .iter()
            .map(|p| (p.id.clone(), p.score, p.state))
            .collect()
    };
    assert_eq!(scores(&first), scores(&second));
}

#[test]
fn the_document_is_usable_by_a_viewer() {
    let sets = reference_sets();

    // A document written beside the photos reloads unchanged. Never write
    // into the source folder from a test: copy the analysis out.
    if let Some(set) = sets.first() {
        let document = shared(set);
        let dir = std::env::temp_dir().join(format!("camcomp-ref-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(cache::CACHE_FILE_NAME);
        cache::save_atomic(&path, document).unwrap();
        let back = cache::load(&path).unwrap();
        assert_eq!(*document, back);
        back.validate().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    } else {
        eprintln!("no ./test/tstgrp* sets present; skipping document round-trip check");
    }

    // Pointer mapping between two members round-trips at full resolution.
    if let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp6") {
        let document = shared(set);
        let group = document.groups.first().expect("one group");
        assert!(group.photo_ids.len() >= 3, "expected a three-member group");
        let a = &group.photo_ids[1];
        let b = &group.photo_ids[2];

        let probe = [0.42, 0.61];
        let mapped = pipeline::map_point_through_group(document, &group.id, a, b, probe)
            .expect("a maps into b");
        let back = pipeline::map_point_through_group(document, &group.id, b, a, mapped)
            .expect("b maps back into a");
        let error = (back[0] - probe[0]).hypot(back[1] - probe[1]);
        assert!(error < 1e-9, "pointer round-trip error {error:.3e}");

        // And the mapping is meaningful: two photos of the same scene must not
        // map a centre-ish point to somewhere off the far edge.
        assert!(
            (-0.5..=1.5).contains(&mapped[0]) && (-0.5..=1.5).contains(&mapped[1]),
            "mapped point {mapped:?} is implausible for a compatible pair"
        );
    } else {
        eprintln!("tstgrp6 not present; skipping pointer-mapping check");
    }
}

#[test]
fn manual_overrides_reach_the_labelled_answer_and_survive_reanalysis() {
    // The requirement is not that the automatic rules agree with the labels, but
    // that a human can reach the labelled answer and that re-analysis keeps it
    // (`SPEC.md` §6).
    let sets = reference_sets();
    let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp3") else {
        eprintln!("tstgrp3 not present; skipping");
        return;
    };
    // At the shipped default `tstgrp3` groups automatically (§6.11). To exercise
    // the human-correction path against a real fixture, tighten the strict tier
    // back to its pre-§6.11-revision value so the Pixel is excluded the way it
    // used to be — the point of this test is the override mechanism, not the
    // calibrated default.
    let mut tight = options();
    tight.algorithm.parameters.field_of_view_tolerance = 0.10;
    let document = pipeline::analyze_quiet(std::slice::from_ref(set), &tight, None).unwrap();
    assert_eq!(document.groups.len(), 1);
    assert_eq!(document.ungrouped.len(), 1);
    let group_id = document.groups[0].id.clone();
    let outsider = document.ungrouped[0].photo_id.clone();
    assert_eq!(
        document.ungrouped[0].reason,
        UngroupedReason::NoFocalCompatiblePeer
    );

    let overrides = vec![camcomp_core::schema::ManualOverride {
        id: "override-1".into(),
        op: camcomp_core::schema::OverrideOp::AddPhoto {
            group_id: group_id.clone(),
            photo_id: outsider.clone(),
        },
        created_at: None,
        note: Some("labelled as one group by the reference set".into()),
    }];
    let corrected = pipeline::regroup(&document, &overrides).unwrap();
    corrected.validate().unwrap();
    assert_eq!(corrected.groups.len(), 1);
    assert_eq!(corrected.groups[0].photo_ids.len(), 3);
    assert!(corrected.ungrouped.is_empty());
    assert!(corrected.groups[0]
        .flags
        .contains(&camcomp_core::schema::GroupFlag::ManualMembership));

    // The manually added member has no alignment yet, because its pair was never
    // visually analyzed. That must be visible rather than papered over.
    let group = &corrected.groups[0];
    assert!(
        !group.alignments.iter().any(|a| a.photo_id == outsider),
        "an alignment was invented for a pair that was never analyzed"
    );
    assert!(group
        .flags
        .contains(&camcomp_core::schema::GroupFlag::UnverifiedPairs));

    // Re-analysis carries the override forward rather than discarding it.
    let reanalyzed =
        pipeline::analyze_quiet(std::slice::from_ref(set), &tight, Some(&corrected)).unwrap();
    assert_eq!(reanalyzed.manual_overrides.len(), 1);
    let group = reanalyzed
        .groups
        .iter()
        .find(|g| g.photo_ids.contains(&outsider))
        .expect("the manually added photo is still grouped");
    assert_eq!(group.photo_ids.len(), 3);
    reanalyzed.validate().unwrap();

    // Forcing computes the missing alignment on demand (`SPEC.md` §6), overriding
    // the focal prefilter that rejected it from metadata. The selector is the
    // *photo*, because that is what a caller has after adding it by hand — having
    // to find a pair ID first would make the feature unusable in practice.
    let pair_id = reanalyzed
        .pairs
        .iter()
        .find(|p| p.reason == Some(PairReason::FocalMismatch))
        .expect("the focal-rejected pair is recorded")
        .id
        .clone();
    let mut forced = tight.clone();
    forced.force_pairs = vec![outsider.clone()];
    let after =
        pipeline::analyze_quiet(std::slice::from_ref(set), &forced, Some(&reanalyzed)).unwrap();
    let pair = after.pair(&pair_id).expect("the pair is still present");
    assert!(
        pair.metrics.is_some(),
        "--force-pair did not actually analyze {pair_id}"
    );
    assert_ne!(pair.reason, Some(PairReason::FocalMismatch));
    let group = after
        .groups
        .iter()
        .find(|g| g.photo_ids.contains(&outsider))
        .expect("still grouped");
    assert!(
        group.alignments.iter().any(|a| a.photo_id == outsider),
        "the forced analysis produced no alignment for the manual member"
    );
    after.validate().unwrap();
}

#[test]
fn every_candidate_pair_is_analyzed_or_metadata_rejected() {
    // No pair budget: every in-session candidate is either visually analyzed
    // or decided from metadata alone (`SPEC.md` §4.3, revised).
    let sets = reference_sets();
    let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp4") else {
        eprintln!("tstgrp4 not present; skipping");
        return;
    };
    let document = shared(set);
    let stats = document.stats.clone().expect("stats");
    let focal_rejected = document
        .pairs
        .iter()
        .filter(|p| p.reason == Some(PairReason::FocalMismatch))
        .count();
    assert_eq!(
        stats.analyzed_pairs + focal_rejected,
        stats.candidate_pairs,
        "every candidate pair must be accounted for"
    );
}

#[test]
fn disabling_session_detection_changes_no_verdict() {
    // §6.13: a prefilter hides the thing it protects, so the suite has to check
    // at least once that the rest of the algorithm — not the clock — is what
    // produced the answer. tstgrp7 is the set where sessions hide a pair: g1 and
    // g2 are 81 s apart at an identical 35 mm equivalent, so no metadata rule can
    // separate them and the geometry has to.
    let sets = reference_sets();
    let Some(set) = sets.iter().find(|s| set_name(s) == "tstgrp7") else {
        eprintln!("tstgrp7 not present; skipping");
        return;
    };
    let mut every_photo_in_one_session = options();
    every_photo_in_one_session
        .algorithm
        .parameters
        .session_gap_seconds = u32::MAX;
    let document = analyze(set, &every_photo_in_one_session);

    let stats = document.stats.clone().expect("stats");
    assert_eq!(stats.sessions, 1, "session detection is off");
    assert!(
        stats.analyzed_pairs > shared(set).stats.as_ref().unwrap().analyzed_pairs,
        "more pairs must reach the visual half with sessions off"
    );
    // Same groups as with sessions on, and the pair the clock used to hide is
    // judged rather than assumed: no consensus, so `uncertain`, not `compatible`.
    assert_eq!(document.groups, shared(set).groups);
    let hidden = document
        .pairs
        .iter()
        .find(|p| {
            let label = |id: &String| {
                document
                    .photo(id)
                    .map(|photo| photo.path.contains("g1-"))
                    .unwrap_or(false)
            };
            label(&p.a) != label(&p.b)
        })
        .expect("the cross-label pair is now a candidate");
    assert_ne!(
        hidden.state,
        PairState::Compatible,
        "{}: two different scenes were called compatible",
        hidden.id
    );
}
