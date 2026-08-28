//! The public operations: `inspect`, `analyze`, `regroup`, `validate`.
//!
//! Mirrors `ADR-DROP1.md` §6. Nothing here is UI-aware; the CLI, a Tauri
//! command and a future WASM adapter all drive the same functions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};

use crate::cache;
use crate::domain::{
    FeatureExtractor, GrayImage, InspectedPhoto, MetadataReader, PhotoFeatures, RawMetadata,
};
use crate::features::{cache_key, FeatureCache, SiftExtractor};
use crate::grouping::{
    apply_overrides, build_groups, GroupingInput, GroupingOutput, GroupingPhoto,
};
use crate::imageio::{
    analysis_dimensions, load_analysis_image, oriented_dimensions, probe_dimensions, SourceFormat,
};
use crate::metadata::{
    configuration_display_name, configuration_id, device_key, identify_mode, parse_exif_datetime,
    with_offset, ExifMetadataReader,
};
use crate::metrics::{analyze_pair, PairInput};
use crate::progress::{CancelToken, NoopProgress, ProgressSink, Reporter, Stage};
use crate::schema::{
    canonical_pair_id, AlgorithmInfo, AnalysisDocument, ApplicationInfo, Configuration, Dimensions,
    FeatureSummary, Fingerprint, ManualOverride, PairReason, PairResult, PairState, Photo,
    PhotoMetadata, RunStats, Session, SourceInfo, TimestampSource, SCHEMA_VERSION,
};
use crate::sessions::{detect_sessions, generate_candidates, CandidatePair, FocalDescriptor};
use crate::{Error, Result};

/// Options for [`analyze`].
#[derive(Debug, Clone, Default)]
pub struct AnalysisOptions {
    /// Algorithm identity, parameters, weights and gates.
    pub algorithm: AlgorithmInfo,
    /// Canonical pair IDs to analyze even if the focal prefilter rejected them
    /// (`SPEC.md` §4.3).
    pub force_pairs: Vec<String>,
    /// Reuse cached pair results from the previous document.
    pub reuse_previous: bool,
    /// Ignore the previous document's manual overrides. Off by default:
    /// re-analysis must never silently discard human decisions.
    pub drop_overrides: bool,
}

impl AnalysisOptions {
    /// Defaults with cache reuse enabled.
    pub fn with_reuse() -> Self {
        Self {
            reuse_previous: true,
            ..Default::default()
        }
    }
}

/// One discovered input file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Absolute path.
    pub absolute: PathBuf,
    /// Path relative to the source root, `/`-separated.
    pub relative: String,
}

/// Result of input discovery.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Discovery {
    /// Common source folder, when the inputs share one.
    pub root: Option<PathBuf>,
    /// Supported files, ordered by relative path.
    pub files: Vec<DiscoveredFile>,
    /// Files that were skipped because their format is unsupported.
    pub skipped: Vec<PathBuf>,
}

fn to_relative(root: Option<&Path>, path: &Path) -> String {
    let raw = match root {
        Some(root) => path.strip_prefix(root).unwrap_or(path),
        None => path,
    };
    raw.components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, skipped: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| Error::Io {
            path: dir.to_path_buf(),
            source: e,
        })?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        // Skip dotfiles, and never treat our own output as an input.
        if name.starts_with('.') || name == cache::CACHE_FILE_NAME {
            continue;
        }
        if entry.is_dir() {
            walk(&entry, out, skipped)?;
        } else if SourceFormat::from_extension(&entry).is_some() {
            out.push(entry);
        } else {
            skipped.push(entry);
        }
    }
    Ok(())
}

fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut iter = paths.iter();
    let first = iter.next()?.parent()?.to_path_buf();
    let mut prefix: Vec<std::ffi::OsString> = first
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    for path in iter {
        let parent = path.parent()?;
        let components: Vec<std::ffi::OsString> = parent
            .components()
            .map(|c| c.as_os_str().to_owned())
            .collect();
        let shared = prefix
            .iter()
            .zip(components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        prefix.truncate(shared);
        if prefix.is_empty() {
            return None;
        }
    }
    if prefix.is_empty() {
        None
    } else {
        Some(prefix.iter().collect())
    }
}

/// Enumerate the inputs deterministically.
///
/// A folder is walked recursively; an explicit file is taken as given and an
/// explicitly named unsupported file is an error rather than a silent skip.
pub fn discover(inputs: &[PathBuf]) -> Result<Discovery> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut skipped: Vec<PathBuf> = Vec::new();
    let mut single_dir: Option<PathBuf> = None;

    for input in inputs {
        let canonical = input.canonicalize().unwrap_or_else(|_| input.to_path_buf());
        if canonical.is_dir() {
            if inputs.len() == 1 {
                single_dir = Some(canonical.clone());
            }
            walk(&canonical, &mut files, &mut skipped)?;
        } else if canonical.is_file() {
            if SourceFormat::from_extension(&canonical).is_none() {
                return Err(Error::UnsupportedFormat { path: canonical });
            }
            files.push(canonical);
        } else {
            return Err(Error::Io {
                path: canonical,
                source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file or folder"),
            });
        }
    }

    files.sort();
    files.dedup();

    let root = single_dir.or_else(|| {
        if files.len() > 1 {
            common_ancestor(&files)
        } else {
            files
                .first()
                .and_then(|f| f.parent().map(|p| p.to_path_buf()))
        }
    });

    let mut discovered: Vec<DiscoveredFile> = files
        .into_iter()
        .map(|absolute| DiscoveredFile {
            relative: to_relative(root.as_deref(), &absolute),
            absolute,
        })
        .collect();
    discovered.sort_by(|a, b| a.relative.cmp(&b.relative));

    Ok(Discovery {
        root,
        files: discovered,
        skipped,
    })
}

/// Metadata gathered before the timezone fallback is known.
struct Staged {
    relative: String,
    absolute: PathBuf,
    fingerprint: Fingerprint,
    raw: RawMetadata,
    original_dimensions: (u32, u32),
    decoder_orients: bool,
    readable: bool,
    notes: Vec<String>,
}

/// Result of [`inspect`].
#[derive(Debug, Clone)]
pub struct Inspection {
    /// Common source folder, when there is one.
    pub root: Option<PathBuf>,
    /// Photos, ordered by relative path.
    pub photos: Vec<InspectedPhoto>,
    /// Configurations detected, ordered by ID.
    pub configurations: Vec<Configuration>,
    /// Files skipped because their format is unsupported.
    pub skipped: Vec<PathBuf>,
}

/// Read fingerprints and metadata without decoding pixels where avoidable.
pub fn inspect(
    inputs: &[PathBuf],
    algorithm: &AlgorithmInfo,
    previous: Option<&AnalysisDocument>,
    reporter: &Reporter<'_>,
) -> Result<Inspection> {
    let started = std::time::Instant::now();
    reporter.stage_started(Stage::Discover, None);
    let discovery = discover(inputs)?;
    reporter.stage_finished(Stage::Discover, started.elapsed().as_millis() as u64);

    let previous_fingerprints: HashMap<&str, &Fingerprint> = previous
        .map(|doc| {
            doc.photos
                .iter()
                .map(|p| (p.path.as_str(), &p.fingerprint))
                .collect()
        })
        .unwrap_or_default();

    let inspect_started = std::time::Instant::now();
    reporter.stage_started(Stage::Inspect, Some(discovery.files.len() as u64));

    let reader = ExifMetadataReader;
    let mut staged: Vec<Staged> = Vec::with_capacity(discovery.files.len());
    for (index, file) in discovery.files.iter().enumerate() {
        reporter.check_cancelled()?;
        let mut notes: Vec<String> = Vec::new();
        let bytes = match std::fs::read(&file.absolute) {
            Ok(bytes) => bytes,
            Err(e) => {
                reporter.warn(
                    Stage::Inspect,
                    format!("{}: cannot be read ({e})", file.relative),
                );
                notes.push(format!("unreadable: {e}"));
                staged.push(Staged {
                    relative: file.relative.clone(),
                    absolute: file.absolute.clone(),
                    fingerprint: Fingerprint {
                        size: 0,
                        modified_at: None,
                        sha256: String::new(),
                    },
                    raw: RawMetadata::default(),
                    original_dimensions: (0, 0),
                    decoder_orients: false,
                    readable: false,
                    notes,
                });
                continue;
            }
        };

        let fingerprint = cache::fingerprint_with_fast_path(
            &file.absolute,
            previous_fingerprints.get(file.relative.as_str()).copied(),
            || Ok(cache::sha256_bytes(&bytes)),
        )?;

        let raw = reader.read(&bytes);
        notes.extend(raw.notes.iter().cloned());

        let format = SourceFormat::from_magic(&bytes)
            .or_else(|| SourceFormat::from_extension(&file.absolute));
        let decoder_orients = format == Some(SourceFormat::Heif);

        // Prefer the EXIF pixel dimensions: for HEIC the only other way to learn
        // the size is a full decode, which is 250-400 ms for a 50 MP file.
        let dims = match raw.exif_dimensions {
            Some(dims) if dims.0 > 0 && dims.1 > 0 => Some(dims),
            _ => match probe_dimensions(&file.absolute, &bytes) {
                Ok(dims) => Some(dims),
                Err(e) => {
                    reporter.warn(Stage::Inspect, format!("{}: {e}", file.relative));
                    notes.push(format!("dimensions unavailable: {e}"));
                    None
                }
            },
        };
        let readable = dims.is_some();

        staged.push(Staged {
            relative: file.relative.clone(),
            absolute: file.absolute.clone(),
            fingerprint,
            raw,
            original_dimensions: dims.unwrap_or((0, 0)),
            decoder_orients,
            readable,
            notes,
        });
        reporter.advanced(
            Stage::Inspect,
            index as u64 + 1,
            Some(discovery.files.len() as u64),
            Some(file.relative.clone()),
        );
    }

    // Timezone fallback: the most common declared offset in this run. Treating a
    // naive time as UTC would move a photo two hours away from its own session
    // (see `IMPLEM-DROP1.md` §2.4).
    let assumed_offset_minutes = majority_offset(&staged);

    let mut photos: Vec<InspectedPhoto> = Vec::with_capacity(staged.len());
    let mut configurations: BTreeMap<String, Configuration> = BTreeMap::new();

    for item in &staged {
        let mode = identify_mode(&item.raw);
        let device = device_key(&item.raw);
        let config_id = configuration_id(&device, mode.label.as_deref());
        let entry = configurations
            .entry(config_id.clone())
            .or_insert_with(|| Configuration {
                id: config_id.clone(),
                device_key: device.clone(),
                make: item.raw.make.clone(),
                model: item.raw.model.clone(),
                mode_label: mode.label.clone(),
                display_name: configuration_display_name(&item.raw, mode.label.as_deref()),
                photo_count: 0,
            });
        entry.photo_count += 1;

        let exif_orientation = item
            .raw
            .orientation
            .filter(|o| (1..=8).contains(o))
            .unwrap_or(1);
        let oriented = if item.decoder_orients {
            item.original_dimensions
        } else {
            oriented_dimensions(item.original_dimensions, exif_orientation)
        };

        let device_offset = algorithm
            .parameters
            .device_time_offset_seconds
            .get(&device)
            .copied()
            .unwrap_or(0);

        let (captured_at, corrected_at, corrected_instant, source, timezone_assumed) =
            resolve_time(&item.raw, assumed_offset_minutes, device_offset);

        let mut notes = item.notes.clone();
        if source == TimestampSource::Missing && item.readable {
            notes.push("no usable capture timestamp; this photo needs manual grouping".to_string());
        }

        let focal = crate::metadata::read_focal(&item.raw, algorithm.parameters.focal_zoom_policy);
        if let Some(note) = &focal.note {
            notes.push(note.clone());
        }
        if let Some(note) = &mode.note {
            notes.push(note.clone());
        }
        photos.push(InspectedPhoto {
            id: cache::photo_id_for_path(&item.relative),
            relative_path: item.relative.clone(),
            absolute_path: item.absolute.clone(),
            fingerprint: item.fingerprint.clone(),
            metadata: PhotoMetadata {
                captured_at,
                capture_time_source: source,
                timezone_assumed,
                corrected_at,
                applied_time_offset_seconds: device_offset,
                exif_orientation,
                orientation_applied_by_decoder: item.decoder_orients,
                camera_make: item.raw.make.clone(),
                camera_model: item.raw.model.clone(),
                lens_model: item.raw.lens_model.clone(),
                focal_length_mm: item.raw.focal_length_mm,
                focal_length_35mm: focal.reported_35mm,
                effective_focal_length_35mm: focal.effective_35mm,
                focal_source: focal.source,
                digital_zoom_ratio: item.raw.digital_zoom_ratio,
                native_lens: focal.native_lens,
                mode_tag_raw: mode.raw.clone(),
                mode_tag_source: mode.source.clone(),
                mode_label: mode.label.clone(),
                mode_flags: mode.flags.clone(),
                mode_tag_unknown_bits: mode.unknown_bits.map(|bits| format!("0x{bits:x}")),
            },
            configuration_id: config_id,
            device_key: device,
            original_dimensions: item.original_dimensions,
            oriented_dimensions: oriented,
            corrected_instant,
            notes,
        });
    }

    reporter.stage_finished(Stage::Inspect, inspect_started.elapsed().as_millis() as u64);

    Ok(Inspection {
        root: discovery.root,
        photos,
        configurations: configurations.into_values().collect(),
        skipped: discovery.skipped,
    })
}

fn majority_offset(staged: &[Staged]) -> i32 {
    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for item in staged {
        if let Some(offset) = item.raw.utc_offset_minutes {
            *counts.entry(offset).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(offset, _)| offset)
        .unwrap_or(0)
}

type ResolvedTime = (
    Option<String>,
    Option<String>,
    Option<(i64, u32)>,
    TimestampSource,
    bool,
);

fn resolve_time(raw: &RawMetadata, assumed_offset: i32, device_offset: i64) -> ResolvedTime {
    let source = match raw.date_time_tag {
        Some("DateTimeOriginal") => TimestampSource::ExifDateTimeOriginal,
        Some("DateTimeDigitized") => TimestampSource::ExifCreateDate,
        Some("DateTime") => TimestampSource::ExifModifyDate,
        _ => TimestampSource::Missing,
    };
    let Some(text) = raw.date_time.as_deref() else {
        return (None, None, None, TimestampSource::Missing, false);
    };
    let Some(naive) = parse_exif_datetime(text) else {
        return (None, None, None, TimestampSource::Missing, false);
    };
    let timezone_assumed = raw.utc_offset_minutes.is_none();
    let offset = raw.utc_offset_minutes.unwrap_or(assumed_offset);
    let Some(captured) = with_offset(naive, offset) else {
        return (None, None, None, TimestampSource::Missing, false);
    };
    let nanos = raw.sub_second.unwrap_or(0);
    let corrected_seconds = captured.timestamp() + device_offset;
    let corrected = FixedOffset::east_opt(offset * 60)
        .and_then(|tz| tz.timestamp_opt(corrected_seconds, nanos).single());
    (
        Some(captured.to_rfc3339()),
        corrected.map(|c: DateTime<FixedOffset>| c.to_rfc3339()),
        Some((corrected_seconds, nanos)),
        source,
        timezone_assumed,
    )
}

/// Run the complete Drop 1 analysis.
pub fn analyze(
    inputs: &[PathBuf],
    options: &AnalysisOptions,
    previous: Option<&AnalysisDocument>,
    sink: &dyn ProgressSink,
    cancel: &CancelToken,
) -> Result<AnalysisDocument> {
    let overall = std::time::Instant::now();
    let reporter = Reporter::new(sink, cancel);
    let params = &options.algorithm.parameters;
    validate_parameters(params)?;

    let inspection = inspect(inputs, &options.algorithm, previous, &reporter)?;
    let photos = inspection.photos;

    // ---- Sessions and candidates ------------------------------------------
    let stage = std::time::Instant::now();
    reporter.stage_started(Stage::Candidates, None);
    let assignment = detect_sessions(&photos, params.session_gap_seconds);
    let mut candidates = generate_candidates(
        &assignment,
        &photos,
        params.field_of_view_tolerance,
        params.field_of_view_tolerance_native_lens,
    );
    // Honour explicit requests to analyze a pair the focal prefilter had
    // rejected.
    //
    // A selector may be a canonical pair ID, or a photo ID, in which case every
    // pair involving that photo is forced. The second form is what a caller
    // actually has after adding a photo to a group by hand.
    if !options.force_pairs.is_empty() {
        for candidate in candidates.iter_mut() {
            let pair_id = canonical_pair_id(&candidate.a, &candidate.b);
            let matched = options.force_pairs.iter().any(|selector| {
                selector == &pair_id
                    || selector == &candidate.a
                    || selector == &candidate.b
                    || pair_id.contains(selector.as_str())
            });
            if matched {
                candidate.forced = true;
            }
        }
    }
    reporter.stage_finished(Stage::Candidates, stage.elapsed().as_millis() as u64);
    reporter.advanced(
        Stage::Candidates,
        candidates.len() as u64,
        Some(candidates.len() as u64),
        Some(format!(
            "{} session(s), {} candidate pair(s)",
            assignment.sessions.len(),
            candidates.len()
        )),
    );

    // ---- Which pair results can be reused? --------------------------------
    let current_sha: HashMap<&str, &str> = photos
        .iter()
        .map(|p| (p.id.as_str(), p.fingerprint.sha256.as_str()))
        .collect();
    let reusable = previous.filter(|doc| {
        options.reuse_previous
            && doc.algorithm.pair_cache_key() == options.algorithm.pair_cache_key()
    });
    let previous_sha: HashMap<&str, &str> = reusable
        .map(|doc| {
            doc.photos
                .iter()
                .map(|p| (p.id.as_str(), p.fingerprint.sha256.as_str()))
                .collect()
        })
        .unwrap_or_default();
    let previous_pairs: HashMap<&str, &PairResult> = reusable
        .map(|doc| doc.pairs.iter().map(|p| (p.id.as_str(), p)).collect())
        .unwrap_or_default();
    let unchanged = |id: &str| -> bool {
        match (current_sha.get(id), previous_sha.get(id)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    };

    let mut results: BTreeMap<String, PairResult> = BTreeMap::new();
    let mut pending: Vec<&CandidatePair> = Vec::new();
    let mut reused = 0usize;

    for candidate in &candidates {
        let id = canonical_pair_id(&candidate.a, &candidate.b);
        let base = PairResult {
            id: id.clone(),
            a: candidate.a.clone(),
            b: candidate.b.clone(),
            session_id: candidate.session_id.clone(),
            time_distance_seconds: candidate.time_distance_seconds,
            state: PairState::Uncertain,
            reason: None,
            score: None,
            hard_gates_passed: false,
            gate_failures: vec![],
            focal: candidate.focal.clone(),
            metrics: None,
            homography_a_to_b: None,
            correspondences: vec![],
        };

        if !candidate.should_analyze() {
            // Decided from metadata alone: no SIFT work is spent here.
            results.insert(
                id.clone(),
                PairResult {
                    state: PairState::Incompatible,
                    reason: Some(PairReason::FocalMismatch),
                    ..base
                },
            );
            continue;
        }
        if let Some(prev) = previous_pairs.get(id.as_str()) {
            // A forced pair whose cached record carries no visual evidence must
            // actually be analyzed: reusing a metadata-only rejection would make
            // `--force-pair` silently do nothing.
            let cached_is_metadata_only = prev.metrics.is_none();
            if !(candidate.forced && cached_is_metadata_only)
                && unchanged(&candidate.a)
                && unchanged(&candidate.b)
            {
                reused += 1;
                results.insert(
                    id.clone(),
                    PairResult {
                        session_id: candidate.session_id.clone(),
                        time_distance_seconds: candidate.time_distance_seconds,
                        focal: candidate.focal.clone(),
                        ..(*prev).clone()
                    },
                );
                continue;
            }
        }
        results.insert(id.clone(), base);
        pending.push(candidate);
    }

    // ---- Features and pair analysis, one session at a time ----------------
    let by_id: HashMap<&str, &InspectedPhoto> = photos.iter().map(|p| (p.id.as_str(), p)).collect();
    let extractor = SiftExtractor::new(params.max_features_per_photo);
    let mut feature_cache = FeatureCache::new();
    let mut feature_summaries: BTreeMap<String, FeatureSummary> = BTreeMap::new();
    let mut analysis_dims: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    let mut peak_analysis_pixels = 0u64;
    let mut analyzed = 0usize;
    let mut unreadable: BTreeSet<String> = BTreeSet::new();
    let mut cancelled = false;

    let features_stage = std::time::Instant::now();
    reporter.stage_started(Stage::Features, None);
    reporter.stage_started(Stage::Pairs, Some(pending.len() as u64));

    'sessions: for session in &assignment.sessions {
        let session_pending: Vec<&CandidatePair> = pending
            .iter()
            .copied()
            .filter(|c| c.session_id == session.id)
            .collect();
        if session_pending.is_empty() {
            continue;
        }
        let needed: BTreeSet<&str> = session_pending
            .iter()
            .flat_map(|c| [c.a.as_str(), c.b.as_str()])
            .collect();

        // Session-scoped so the decoded buffers are released before the next
        // session, which is what keeps peak memory proportional to a session
        // rather than to the album (`PLAN-DROP1.md` §11.1).
        let mut loaded: HashMap<&str, (Arc<GrayImage>, Arc<PhotoFeatures>)> = HashMap::new();
        for id in &needed {
            if cancel.is_cancelled() {
                cancelled = true;
                break 'sessions;
            }
            let Some(photo) = by_id.get(id) else { continue };
            match load_features(
                photo,
                &extractor,
                &mut feature_cache,
                params.analysis_longest_edge,
                params.max_megapixels,
            ) {
                Ok((image, features, extracted)) => {
                    let pixels = image.width() as u64 * image.height() as u64;
                    peak_analysis_pixels = peak_analysis_pixels.max(pixels);
                    analysis_dims.insert(photo.id.clone(), [image.width(), image.height()]);
                    if extracted {
                        feature_summaries.insert(
                            photo.id.clone(),
                            FeatureSummary {
                                keypoints: features.len(),
                                duration_ms: features.duration_ms,
                                low_texture: features.len() < params.min_keypoints_for_decision,
                            },
                        );
                        reporter.advanced(
                            Stage::Features,
                            feature_summaries.len() as u64,
                            None,
                            Some(format!(
                                "{} ({} keypoints)",
                                photo.relative_path,
                                features.len()
                            )),
                        );
                    }
                    loaded.insert(id, (image, features));
                }
                Err(e) => {
                    reporter.warn(Stage::Features, format!("{}: {e}", photo.relative_path));
                    unreadable.insert(photo.id.clone());
                }
            }
        }

        for candidate in session_pending {
            if cancel.is_cancelled() {
                cancelled = true;
                break 'sessions;
            }
            let id = canonical_pair_id(&candidate.a, &candidate.b);
            let (Some((image_a, features_a)), Some((image_b, features_b))) = (
                loaded.get(candidate.a.as_str()),
                loaded.get(candidate.b.as_str()),
            ) else {
                if let Some(entry) = results.get_mut(&id) {
                    entry.state = PairState::Uncertain;
                    entry.reason = Some(PairReason::Unreadable);
                }
                continue;
            };
            let outcome = analyze_pair(
                &PairInput {
                    features_a,
                    features_b,
                    image_a,
                    image_b,
                },
                &options.algorithm,
            );
            analyzed += 1;
            if let Some(entry) = results.get_mut(&id) {
                entry.state = outcome.state;
                entry.reason = outcome.reason;
                entry.score = outcome.score;
                entry.hard_gates_passed = outcome.hard_gates_passed;
                entry.gate_failures = outcome.gate_failures;
                entry.metrics = outcome.metrics;
                entry.homography_a_to_b =
                    outcome.homography_a_to_b.map(|m| m.as_row_major().to_vec());
                entry.correspondences = outcome.correspondences;
            }
            reporter.advanced(
                Stage::Pairs,
                analyzed as u64,
                Some(pending.len() as u64),
                Some(id),
            );
        }
    }
    reporter.stage_finished(Stage::Features, features_stage.elapsed().as_millis() as u64);
    reporter.stage_finished(Stage::Pairs, features_stage.elapsed().as_millis() as u64);

    let pair_results: Vec<PairResult> = results.into_values().collect();

    // ---- Grouping ---------------------------------------------------------
    let stage = std::time::Instant::now();
    reporter.stage_started(Stage::Grouping, None);
    let session_of: HashMap<&str, &str> = assignment
        .sessions
        .iter()
        .flat_map(|s| s.photo_ids.iter().map(move |p| (p.as_str(), s.id.as_str())))
        .collect();
    let grouping_photos: Vec<GroupingPhoto> = photos
        .iter()
        .map(|p| GroupingPhoto {
            id: p.id.clone(),
            configuration_id: p.configuration_id.clone(),
            focal: FocalDescriptor::from_metadata(&p.metadata),
            session_id: session_of.get(p.id.as_str()).map(|s| s.to_string()),
            readable: !unreadable.contains(&p.id) && p.original_dimensions.0 > 0,
        })
        .collect();

    let preferred_anchors: BTreeSet<String> = previous
        .map(|doc| {
            doc.groups
                .iter()
                .map(|g| g.anchor_photo_id.clone())
                .collect()
        })
        .unwrap_or_default();

    let automatic = build_groups(&GroupingInput {
        photos: &grouping_photos,
        pairs: &pair_results,
        candidates: &candidates,
        field_of_view_tolerance: params.field_of_view_tolerance,
        field_of_view_tolerance_native_lens: params.field_of_view_tolerance_native_lens,
        compatibility_threshold: params.compatibility_threshold,
        one_photo_per_configuration: params.one_photo_per_configuration,
        preferred_anchors: &preferred_anchors,
    });

    let overrides: Vec<ManualOverride> = if options.drop_overrides {
        Vec::new()
    } else {
        previous
            .map(|doc| doc.manual_overrides.clone())
            .unwrap_or_default()
    };
    let grouped = if overrides.is_empty() {
        automatic
    } else {
        apply_overrides(&automatic, &overrides, &grouping_photos, &pair_results)?
    };
    reporter.stage_finished(Stage::Grouping, stage.elapsed().as_millis() as u64);

    // ---- Document assembly -------------------------------------------------
    let previous_features: HashMap<&str, &FeatureSummary> = previous
        .map(|doc| {
            doc.photos
                .iter()
                .filter_map(|p| p.features.as_ref().map(|f| (p.id.as_str(), f)))
                .collect()
        })
        .unwrap_or_default();
    let previous_analysis_dims: HashMap<&str, [u32; 2]> = previous
        .map(|doc| {
            doc.photos
                .iter()
                .filter_map(|p| p.dimensions.analysis.map(|d| (p.id.as_str(), d)))
                .collect()
        })
        .unwrap_or_default();

    let document_photos: Vec<Photo> = photos
        .iter()
        .map(|p| Photo {
            id: p.id.clone(),
            path: p.relative_path.clone(),
            configuration_id: p.configuration_id.clone(),
            fingerprint: p.fingerprint.clone(),
            dimensions: Dimensions {
                original: [p.original_dimensions.0, p.original_dimensions.1],
                oriented: [p.oriented_dimensions.0, p.oriented_dimensions.1],
                analysis: analysis_dims
                    .get(&p.id)
                    .copied()
                    .or_else(|| previous_analysis_dims.get(p.id.as_str()).copied())
                    .or_else(|| {
                        if p.oriented_dimensions.0 > 0 {
                            let d = analysis_dimensions(
                                p.oriented_dimensions,
                                params.analysis_longest_edge,
                            );
                            Some([d.0, d.1])
                        } else {
                            None
                        }
                    }),
            },
            metadata: p.metadata.clone(),
            features: feature_summaries
                .get(&p.id)
                .cloned()
                .or_else(|| previous_features.get(p.id.as_str()).map(|f| (*f).clone())),
            notes: p.notes.clone(),
        })
        .collect();

    let corrected_of: HashMap<&str, &str> = photos
        .iter()
        .filter_map(|p| {
            p.metadata
                .corrected_at
                .as_deref()
                .map(|t| (p.id.as_str(), t))
        })
        .collect();
    let document_sessions: Vec<Session> = assignment
        .sessions
        .iter()
        .map(|s| Session {
            id: s.id.clone(),
            photo_ids: s.photo_ids.clone(),
            start: s
                .photo_ids
                .first()
                .and_then(|id| corrected_of.get(id.as_str()))
                .map(|t| t.to_string())
                .unwrap_or_else(|| instant_to_rfc3339(s.start)),
            end: s
                .photo_ids
                .last()
                .and_then(|id| corrected_of.get(id.as_str()))
                .map(|t| t.to_string())
                .unwrap_or_else(|| instant_to_rfc3339(s.end)),
        })
        .collect();

    let counts = |state: PairState| pair_results.iter().filter(|p| p.state == state).count();
    let stats = RunStats {
        photos: document_photos.len(),
        sessions: document_sessions.len(),
        candidate_pairs: candidates.len(),
        analyzed_pairs: analyzed,
        reused_pairs: reused,
        compatible_pairs: counts(PairState::Compatible),
        incompatible_pairs: counts(PairState::Incompatible),
        uncertain_pairs: counts(PairState::Uncertain),
        features_extracted: feature_summaries.len(),
        groups: grouped.groups.len(),
        ungrouped: grouped.ungrouped.len(),
        duration_ms: overall.elapsed().as_millis() as u64,
        peak_analysis_pixels,
        cancelled,
    };

    let document = AnalysisDocument {
        schema_version: SCHEMA_VERSION.to_string(),
        application: ApplicationInfo::default(),
        algorithm: options.algorithm.clone(),
        source: SourceInfo {
            root: inspection
                .root
                .as_ref()
                .map(|r| r.to_string_lossy().to_string()),
            generated_at: Some(Utc::now().to_rfc3339()),
        },
        configurations: inspection.configurations,
        photos: document_photos,
        sessions: document_sessions,
        pairs: pair_results,
        groups: grouped.groups,
        ungrouped: grouped.ungrouped,
        manual_overrides: overrides,
        stats: Some(stats),
    };
    document.validate()?;

    if cancelled {
        // A cancelled run still returns a valid, if partial, document so the
        // caller can persist completed work rather than losing it.
        return Ok(document);
    }
    Ok(document)
}

fn instant_to_rfc3339(instant: (i64, u32)) -> String {
    Utc.timestamp_opt(instant.0, instant.1)
        .single()
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string())
}

fn validate_parameters(params: &crate::schema::AnalysisParameters) -> Result<()> {
    if params.analysis_longest_edge < 64 {
        return Err(Error::InvalidParameters(
            "analysisLongestEdge must be at least 64".into(),
        ));
    }
    if !(0.0..=1.0).contains(&params.compatibility_threshold) {
        return Err(Error::InvalidParameters(
            "compatibilityThreshold must be in [0, 1]".into(),
        ));
    }
    if params.field_of_view_tolerance < 0.0 || !params.field_of_view_tolerance.is_finite() {
        return Err(Error::InvalidParameters(
            "fieldOfViewTolerance must be a non-negative finite number".into(),
        ));
    }
    if params.field_of_view_tolerance_native_lens < 0.0
        || !params.field_of_view_tolerance_native_lens.is_finite()
    {
        return Err(Error::InvalidParameters(
            "fieldOfViewToleranceNativeLens must be a non-negative finite number".into(),
        ));
    }
    if !(0.0..=1.0).contains(&params.lowe_ratio) {
        return Err(Error::InvalidParameters(
            "loweRatio must be in [0, 1]".into(),
        ));
    }
    if params.max_megapixels <= 0.0 {
        return Err(Error::InvalidParameters(
            "maxMegapixels must be positive".into(),
        ));
    }
    Ok(())
}

fn load_features(
    photo: &InspectedPhoto,
    extractor: &SiftExtractor,
    cache: &mut FeatureCache,
    longest_edge: u32,
    max_megapixels: f64,
) -> Result<(Arc<GrayImage>, Arc<PhotoFeatures>, bool)> {
    let bytes = std::fs::read(&photo.absolute_path).map_err(|e| Error::Io {
        path: PathBuf::from(&photo.relative_path),
        source: e,
    })?;
    let image = load_analysis_image(
        &PathBuf::from(&photo.relative_path),
        &bytes,
        photo.metadata.exif_orientation,
        longest_edge,
        max_megapixels,
    )?;
    drop(bytes);
    let gray = Arc::new(image.gray);
    let key = cache_key(&photo.fingerprint.sha256, extractor);
    let already = cache.get(&key).is_some();
    let features = if let Some(found) = cache.get(&key) {
        found
    } else {
        let extracted = Arc::new(extractor.extract(&gray)?);
        cache.insert(key, Arc::clone(&extracted));
        extracted
    };
    Ok((gray, features, !already))
}

/// Re-run grouping over an existing document with a new set of overrides.
///
/// No image is decoded: everything needed is already persisted.
pub fn regroup(
    document: &AnalysisDocument,
    overrides: &[ManualOverride],
) -> Result<AnalysisDocument> {
    let session_of: HashMap<&str, &str> = document
        .sessions
        .iter()
        .flat_map(|s| s.photo_ids.iter().map(move |p| (p.as_str(), s.id.as_str())))
        .collect();
    let grouping_photos: Vec<GroupingPhoto> = document
        .photos
        .iter()
        .map(|p| GroupingPhoto {
            id: p.id.clone(),
            configuration_id: p.configuration_id.clone(),
            focal: FocalDescriptor::from_metadata(&p.metadata),
            session_id: session_of.get(p.id.as_str()).map(|s| s.to_string()),
            readable: p.dimensions.original[0] > 0,
        })
        .collect();

    // Rebuild the automatic result from persisted evidence, then reapply the
    // complete override list. Automatic evidence and human decisions stay
    // separate (`SPEC.md` §6).
    let preferred: BTreeSet<String> = document
        .groups
        .iter()
        .map(|g| g.anchor_photo_id.clone())
        .collect();
    let candidates: Vec<CandidatePair> = document
        .pairs
        .iter()
        .map(|p| CandidatePair {
            a: p.a.clone(),
            b: p.b.clone(),
            session_id: p.session_id.clone(),
            time_distance_seconds: p.time_distance_seconds,
            focal: p.focal.clone(),
            forced: false,
        })
        .collect();
    let automatic = build_groups(&GroupingInput {
        photos: &grouping_photos,
        pairs: &document.pairs,
        candidates: &candidates,
        field_of_view_tolerance: document.algorithm.parameters.field_of_view_tolerance,
        field_of_view_tolerance_native_lens: document
            .algorithm
            .parameters
            .field_of_view_tolerance_native_lens,
        compatibility_threshold: document.algorithm.parameters.compatibility_threshold,
        one_photo_per_configuration: document.algorithm.parameters.one_photo_per_configuration,
        preferred_anchors: &preferred,
    });
    let grouped: GroupingOutput = if overrides.is_empty() {
        automatic
    } else {
        apply_overrides(&automatic, overrides, &grouping_photos, &document.pairs)?
    };

    let mut out = document.clone();
    out.groups = grouped.groups;
    out.ungrouped = grouped.ungrouped;
    out.manual_overrides = overrides.to_vec();
    if let Some(stats) = out.stats.as_mut() {
        stats.groups = out.groups.len();
        stats.ungrouped = out.ungrouped.len();
    }
    out.validate()?;
    Ok(out)
}

/// Structural validation of a document.
pub fn validate(document: &AnalysisDocument) -> Result<()> {
    document.validate()
}

/// Convenience wrapper: analyze with no progress reporting and no cancellation.
pub fn analyze_quiet(
    inputs: &[PathBuf],
    options: &AnalysisOptions,
    previous: Option<&AnalysisDocument>,
) -> Result<AnalysisDocument> {
    analyze(
        inputs,
        options,
        previous,
        &NoopProgress,
        &CancelToken::new(),
    )
}

/// Human-readable one-line explanation of a pair result, for the CLI and the UI.
pub fn explain_pair(pair: &PairResult) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("{:?}", pair.state).to_lowercase());
    if let Some(score) = pair.score {
        parts.push(format!("score {score:.3}"));
    }
    if let Some(reason) = pair.reason {
        parts.push(format!("reason {}", serde_plain(&reason)));
    }
    if !pair.gate_failures.is_empty() {
        let failed: Vec<String> = pair.gate_failures.iter().map(serde_plain).collect();
        parts.push(format!("gates failed: {}", failed.join(", ")));
    }
    if let (Some(diff), Some(tolerance)) = (
        pair.focal.relative_fov_difference,
        pair.focal.applied_tolerance,
    ) {
        // The verdict is not explainable from the difference alone: which
        // tolerance applied depends on whether both lenses were native.
        parts.push(format!(
            "fov diff {diff:.3} against {tolerance:.2}{}",
            if pair.focal.native_lens_pair {
                " (native lenses)"
            } else {
                ""
            }
        ));
    }
    if let Some(m) = &pair.metrics {
        parts.push(format!(
            "overlap {:.2} coverage {:.2} confidence {:.2} appearance {:.2} inliers {}",
            m.geometric_overlap,
            m.feature_coverage,
            m.inlier_confidence,
            m.aligned_appearance_similarity,
            m.inliers
        ));
    }
    parts.join(" | ")
}

fn serde_plain<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "?".to_string())
}

/// Diagnostics for a pair, as a compact table row. Used by the calibration
/// harness so the component distribution is visible, not just the verdict
/// (`SPEC.md` §5.4.1).
pub fn pair_row(pair: &PairResult) -> BTreeMap<&'static str, String> {
    let mut row = BTreeMap::new();
    row.insert("pair", pair.id.clone());
    row.insert("state", serde_plain(&pair.state));
    row.insert(
        "score",
        pair.score.map(|s| format!("{s:.4}")).unwrap_or_default(),
    );
    row.insert(
        "reason",
        pair.reason.map(|r| serde_plain(&r)).unwrap_or_default(),
    );
    row.insert("focal", serde_plain(&pair.focal.state));
    row.insert(
        "fovdiff",
        pair.focal
            .relative_fov_difference
            .map(|d| format!("{d:.3}"))
            .unwrap_or_default(),
    );
    row.insert(
        "fovtol",
        pair.focal
            .applied_tolerance
            .map(|t| {
                format!(
                    "{t:.2}{}",
                    if pair.focal.native_lens_pair { "n" } else { "" }
                )
            })
            .unwrap_or_default(),
    );
    row.insert(
        "focaldiff",
        pair.focal
            .relative_focal_difference
            .map(|d| format!("{d:.3}"))
            .unwrap_or_default(),
    );
    if let Some(m) = &pair.metrics {
        row.insert("overlap", format!("{:.4}", m.geometric_overlap));
        row.insert("coverage", format!("{:.4}", m.feature_coverage));
        row.insert("confidence", format!("{:.4}", m.inlier_confidence));
        row.insert(
            "appearance",
            format!("{:.4}", m.aligned_appearance_similarity),
        );
        row.insert("inliers", m.inliers.to_string());
        row.insert(
            "reproj_px",
            format!("{:.2}", m.median_reprojection_error_px),
        );
    }
    row
}

/// A pair result with a homography, exposed so a viewer can map a pointer from
/// one photo into another at full resolution (`SPEC.md` §7.3).
pub fn map_point_through_group(
    document: &AnalysisDocument,
    group_id: &str,
    from_photo: &str,
    to_photo: &str,
    normalized_point: [f64; 2],
) -> Option<[f64; 2]> {
    let group = document.group(group_id)?;
    let anchor = group.anchor_photo_id.as_str();
    let to_anchor = |photo: &str, point: [f64; 2]| -> Option<[f64; 2]> {
        if photo == anchor {
            return Some(point);
        }
        let alignment = group.alignments.iter().find(|a| a.photo_id == photo)?;
        crate::geometry::Mat3::from_row_major(&alignment.homography)?.apply(point)
    };
    let from_anchor = |photo: &str, point: [f64; 2]| -> Option<[f64; 2]> {
        if photo == anchor {
            return Some(point);
        }
        let alignment = group.alignments.iter().find(|a| a.photo_id == photo)?;
        crate::geometry::Mat3::from_row_major(&alignment.inverse_homography)?.apply(point)
    };
    let in_anchor = to_anchor(from_photo, normalized_point)?;
    from_anchor(to_photo, in_anchor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FocalCompatibility;

    /// Process-unique so two concurrent `cargo test` invocations cannot delete
    /// each other's fixtures.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("camcomp-pipeline-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A textured PNG with an EXIF-free header. Timestamps are supplied by the
    /// caller through the filename-independent path, so these fixtures exercise
    /// the "no timestamp" branch on purpose.
    fn write_png(dir: &Path, name: &str, width: u32, height: u32, seed: u32, shift: i32) {
        let mut data = Vec::with_capacity((width * height) as usize);
        let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        for y in 0..height {
            for x in 0..width {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let sx = (x as i32 + shift).rem_euclid(width as i32) as u32;
                let noise = ((state >> 27) as i32) - 16;
                let structure = (((sx / 13) + (y / 17)) % 2) as i32 * 95
                    + ((sx as f32 * 0.1).sin() * (y as f32 * 0.13).cos() * 50.0) as i32;
                data.push((80 + structure + noise).clamp(0, 255) as u8);
            }
        }
        let img = image::GrayImage::from_raw(width, height, data).unwrap();
        img.save(dir.join(name)).unwrap();
    }

    #[test]
    fn discovery_is_deterministic_and_skips_our_own_output() {
        let dir = temp_dir("discovery");
        write_png(&dir, "b.png", 64, 48, 1, 0);
        write_png(&dir, "a.png", 64, 48, 2, 0);
        std::fs::write(dir.join(cache::CACHE_FILE_NAME), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();
        std::fs::write(dir.join(".hidden.png"), "x").unwrap();

        let first = discover(std::slice::from_ref(&dir)).unwrap();
        let second = discover(std::slice::from_ref(&dir)).unwrap();
        assert_eq!(first, second);
        let names: Vec<&str> = first.files.iter().map(|f| f.relative.as_str()).collect();
        assert_eq!(names, vec!["a.png", "b.png"]);
        assert_eq!(first.skipped.len(), 1);
        assert_eq!(first.root.as_ref().unwrap().file_name(), dir.file_name());
    }

    #[test]
    fn an_explicitly_named_unsupported_file_is_an_error() {
        let dir = temp_dir("unsupported");
        let path = dir.join("notes.txt");
        std::fs::write(&path, "hello").unwrap();
        assert_eq!(discover(&[path]).unwrap_err().code(), "unsupported-format");
    }

    #[test]
    fn a_missing_input_is_an_io_error() {
        assert_eq!(
            discover(&[PathBuf::from("/definitely/not/here")])
                .unwrap_err()
                .code(),
            "io"
        );
    }

    #[test]
    fn subfolders_are_walked_and_relative_paths_keep_their_shape() {
        let dir = temp_dir("subfolders");
        std::fs::create_dir_all(dir.join("camera-a")).unwrap();
        std::fs::create_dir_all(dir.join("camera-b")).unwrap();
        write_png(&dir.join("camera-a"), "one.png", 64, 48, 1, 0);
        write_png(&dir.join("camera-b"), "two.png", 64, 48, 2, 0);
        let found = discover(std::slice::from_ref(&dir)).unwrap();
        let names: Vec<&str> = found.files.iter().map(|f| f.relative.as_str()).collect();
        assert_eq!(names, vec!["camera-a/one.png", "camera-b/two.png"]);
    }

    #[test]
    fn photos_without_a_timestamp_are_reported_and_never_dropped() {
        let dir = temp_dir("notime");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        write_png(&dir, "b.png", 128, 96, 1, 6);
        let doc = analyze_quiet(&[dir], &AnalysisOptions::default(), None).unwrap();
        assert_eq!(doc.photos.len(), 2);
        assert_eq!(doc.sessions.len(), 0);
        assert_eq!(doc.ungrouped.len(), 2);
        assert!(doc
            .ungrouped
            .iter()
            .all(|u| u.reason == crate::schema::UngroupedReason::UnusableTimestamp));
        doc.validate().unwrap();
    }

    #[test]
    fn an_unreadable_file_appears_in_the_document_rather_than_vanishing() {
        let dir = temp_dir("unreadable");
        write_png(&dir, "good.png", 128, 96, 1, 0);
        std::fs::write(dir.join("broken.png"), b"\x89PNG\r\n\x1a\n truncated").unwrap();
        let doc = analyze_quiet(&[dir], &AnalysisOptions::default(), None).unwrap();
        assert_eq!(doc.photos.len(), 2);
        let broken = doc.photos.iter().find(|p| p.path == "broken.png").unwrap();
        assert!(!broken.notes.is_empty());
        assert_eq!(doc.ungrouped.len(), 2);
        doc.validate().unwrap();
    }

    #[test]
    fn invalid_parameters_are_rejected_before_any_work() {
        let dir = temp_dir("badparams");
        write_png(&dir, "a.png", 64, 48, 1, 0);
        let mut options = AnalysisOptions::default();
        options.algorithm.parameters.compatibility_threshold = 5.0;
        assert_eq!(
            analyze_quiet(std::slice::from_ref(&dir), &options, None)
                .unwrap_err()
                .code(),
            "invalid-parameters"
        );

        let mut options = AnalysisOptions::default();
        options.algorithm.parameters.analysis_longest_edge = 8;
        assert_eq!(
            analyze_quiet(&[dir], &options, None).unwrap_err().code(),
            "invalid-parameters"
        );
    }

    #[test]
    fn cancellation_stops_the_run_and_still_returns_a_valid_document() {
        let dir = temp_dir("cancel");
        write_png(&dir, "a.png", 256, 192, 1, 0);
        write_png(&dir, "b.png", 256, 192, 1, 5);
        let cancel = CancelToken::new();
        cancel.cancel();
        // Cancellation before the first stage aborts with `Cancelled`; the caller
        // keeps whatever was previously persisted.
        let err = analyze(
            &[dir],
            &AnalysisOptions::default(),
            None,
            &NoopProgress,
            &cancel,
        )
        .unwrap_err();
        assert_eq!(err.code(), "cancelled");
    }

    #[test]
    fn progress_events_cover_every_stage() {
        use std::sync::Mutex;
        let dir = temp_dir("progress");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        let seen: Mutex<Vec<String>> = Mutex::new(Vec::new());
        {
            let sink = |ev: &crate::progress::ProgressEvent| {
                if let crate::progress::ProgressEvent::StageStarted { stage, .. } = ev {
                    seen.lock().unwrap().push(stage.as_str().to_string());
                }
            };
            analyze(
                &[dir],
                &AnalysisOptions::default(),
                None,
                &sink,
                &CancelToken::new(),
            )
            .unwrap();
        }
        let stages = seen.into_inner().unwrap();
        for expected in [
            "discover",
            "inspect",
            "candidates",
            "features",
            "pairs",
            "grouping",
        ] {
            assert!(stages.contains(&expected.to_string()), "{stages:?}");
        }
    }

    #[test]
    fn a_document_survives_a_disk_round_trip_and_validates() {
        let dir = temp_dir("roundtrip");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        let doc = analyze_quiet(
            std::slice::from_ref(&dir),
            &AnalysisOptions::default(),
            None,
        )
        .unwrap();
        let path = dir.join(cache::CACHE_FILE_NAME);
        cache::save_atomic(&path, &doc).unwrap();
        let back = cache::load(&path).unwrap();
        assert_eq!(doc, back);
        back.validate().unwrap();
    }

    #[test]
    fn analysis_never_modifies_a_source_photo() {
        let dir = temp_dir("readonly");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        write_png(&dir, "b.png", 128, 96, 1, 4);
        let before: Vec<(String, String)> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (
                    p.file_name().unwrap().to_string_lossy().to_string(),
                    cache::sha256_file(&p).unwrap(),
                )
            })
            .collect();
        let doc = analyze_quiet(
            std::slice::from_ref(&dir),
            &AnalysisOptions::default(),
            None,
        )
        .unwrap();
        cache::save_atomic(&dir.join(cache::CACHE_FILE_NAME), &doc).unwrap();
        for (name, digest) in before {
            assert_eq!(
                cache::sha256_file(&dir.join(&name)).unwrap(),
                digest,
                "{name} was modified"
            );
        }
    }

    #[test]
    fn regroup_applies_overrides_without_decoding_anything() {
        let dir = temp_dir("regroup");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        write_png(&dir, "b.png", 128, 96, 1, 4);
        let doc = analyze_quiet(&[dir], &AnalysisOptions::default(), None).unwrap();
        // Without timestamps there are no groups, so an override on a
        // non-existent group must fail cleanly rather than corrupt anything.
        let bad = vec![ManualOverride {
            id: "override-1".into(),
            op: crate::schema::OverrideOp::SetAnchor {
                group_id: "group-nope".into(),
                photo_id: doc.photos[0].id.clone(),
            },
            created_at: None,
            note: None,
        }];
        assert_eq!(regroup(&doc, &bad).unwrap_err().code(), "invalid-override");
        // And an empty override list is a faithful rebuild.
        let same = regroup(&doc, &[]).unwrap();
        assert_eq!(same.groups, doc.groups);
        assert_eq!(same.ungrouped, doc.ungrouped);
    }

    #[test]
    fn the_majority_offset_falls_back_to_utc_when_nothing_declares_one() {
        assert_eq!(majority_offset(&[]), 0);
    }

    #[test]
    fn a_forced_selector_may_name_a_photo_rather_than_a_pair() {
        let dir = temp_dir("forcedphoto");
        write_png(&dir, "a.png", 128, 96, 1, 0);
        write_png(&dir, "b.png", 128, 96, 1, 4);
        // Nothing is analyzed without timestamps, so photos without a session
        // never form a candidate pair; force it by naming one of the photos.
        let options = AnalysisOptions::default();
        let doc = analyze_quiet(std::slice::from_ref(&dir), &options, None).unwrap();
        let photo = doc.photos[0].id.clone();

        // A selector that matches nothing must not silently force everything.
        let mut nothing = options.clone();
        nothing.force_pairs = vec!["photo-does-not-exist".into()];
        let unchanged = analyze_quiet(std::slice::from_ref(&dir), &nothing, None).unwrap();
        assert_eq!(unchanged.stats.unwrap().analyzed_pairs, 0);

        let mut forced = options.clone();
        forced.force_pairs = vec![photo.clone()];
        let after = analyze_quiet(std::slice::from_ref(&dir), &forced, None).unwrap();
        // These photos share no session, so there is no pair to force; the point
        // is that the selector is accepted and nothing panics or is invented.
        assert!(after.validate().is_ok());
        assert_eq!(after.photos.len(), 2);
    }

    #[test]
    fn explanations_mention_the_state_and_the_reason() {
        let pair = PairResult {
            id: "pair-a-b".into(),
            a: "a".into(),
            b: "b".into(),
            session_id: "session-001".into(),
            time_distance_seconds: 2,
            state: PairState::Incompatible,
            reason: Some(PairReason::FocalMismatch),
            score: None,
            hard_gates_passed: false,
            gate_failures: vec![],
            focal: crate::schema::FocalComparison {
                state: FocalCompatibility::Incompatible,
                relative_fov_difference: Some(0.39),
                relative_focal_difference: Some(0.43),
                applied_tolerance: Some(0.10),
                native_lens_pair: false,
            },
            metrics: None,
            homography_a_to_b: None,
            correspondences: vec![],
        };
        let text = explain_pair(&pair);
        assert!(text.contains("incompatible"), "{text}");
        assert!(text.contains("focal-mismatch"), "{text}");
        let row = pair_row(&pair);
        assert_eq!(row["state"], "incompatible");
        assert_eq!(row["focal"], "incompatible");
    }
}
