//! `camcomp` — command-line adapter over `camcomp-core`.
//!
//! Exists so fixtures, batch runs, profiling and calibration stay independent
//! from any UI (`ADR-DROP1.md` §5.1).
//!
//! Exit codes: `0` success, `1` analysis or I/O failure, `2` invalid usage,
//! `3` the analysis completed but did not satisfy an `--expect-*` assertion.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use camcomp_core::cache;
use camcomp_core::pipeline::{self, AnalysisOptions};
use camcomp_core::progress::{CancelToken, ProgressEvent, ProgressSink};
use camcomp_core::schema::{AnalysisDocument, ManualOverride, OverrideOp};
use clap::{Args, Parser, Subcommand, ValueEnum};

const EXIT_FAILURE: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_EXPECTATION: i32 = 3;

#[derive(Parser)]
#[command(
    name = "camcomp",
    version,
    about = "Group photos of the same scene taken by different cameras or modes",
    long_about = None
)]
struct Cli {
    /// Emit newline-delimited JSON progress on stderr instead of human text.
    #[arg(long, global = true)]
    json_progress: bool,

    /// Suppress progress output entirely.
    #[arg(long, short, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read metadata and fingerprints without analyzing pixels.
    Inspect {
        /// Folders or files to inspect.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        /// How to reconcile the 35 mm equivalent focal length with the digital
        /// zoom ratio. Changes the effective focal length reported here.
        #[arg(long, value_enum)]
        focal_zoom_policy: Option<ZoomPolicyArg>,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Run the complete Drop 1 analysis.
    Analyze {
        /// Folders or files to analyze.
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        #[command(flatten)]
        tuning: Tuning,

        /// Where to write `camcomp.json`. Defaults to the source folder.
        #[arg(long)]
        out: Option<PathBuf>,

        /// Load a previous analysis from this path instead of auto-discovering it.
        #[arg(long)]
        cache_in: Option<PathBuf>,

        /// Do not read or reuse any previous analysis.
        #[arg(long)]
        no_cache: bool,

        /// Do not write `camcomp.json`.
        #[arg(long)]
        dry_run: bool,

        /// Analyze these canonical pair IDs even if the budget skipped them.
        #[arg(long = "force-pair")]
        force_pairs: Vec<String>,

        /// Print the per-pair component table (calibration evidence).
        #[arg(long)]
        table: bool,

        /// Fail with exit code 3 unless every group derived from the `gN-` filename
        /// labels was reproduced exactly.
        #[arg(long)]
        expect_labels: bool,

        /// Output format for the summary.
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
    },
    /// Validate an existing `camcomp.json`.
    Validate {
        /// Path to the document, or a folder containing one.
        path: PathBuf,
    },
    /// Explain a pair, a group or an ungrouped photo.
    Explain {
        /// Path to the document, or a folder containing one.
        path: PathBuf,
        /// Pair, group or photo ID. Omit to list everything.
        id: Option<String>,
    },
    /// Apply manual overrides to an existing analysis and rewrite it.
    Override(OverrideArgs),
    /// Print the generated JSON Schema for `camcomp.json`.
    Schema,
}

#[derive(Args)]
struct Tuning {
    /// Session gap, in seconds.
    #[arg(long)]
    session_gap: Option<u32>,
    /// Disable session detection: every photo lands in one session.
    ///
    /// A calibration instrument. It removes the cheapest prefilter, so the
    /// visual half of the algorithm has to decide every pair.
    #[arg(long, conflicts_with = "session_gap")]
    no_session_gap: bool,
    /// Relative tolerance on diagonal angle of view. Not a focal ratio: 0.10 is
    /// about the 15 % focal rule at 24 mm, looser at ultrawide, stricter at tele.
    #[arg(long)]
    fov_tolerance: Option<f64>,
    /// Same tolerance for a pair where both photos used a lens at its native
    /// field of view — no digital zoom on either side.
    #[arg(long)]
    fov_tolerance_native_lens: Option<f64>,
    /// How to reconcile the 35 mm equivalent focal length with the digital zoom
    /// ratio.
    #[arg(long, value_enum)]
    focal_zoom_policy: Option<ZoomPolicyArg>,
    /// Composite score a pair must reach after the hard gates.
    #[arg(long)]
    threshold: Option<f64>,
    /// Longest edge of the analysis image, in pixels.
    #[arg(long)]
    analysis_edge: Option<u32>,
    /// Maximum pairs analyzed per session.
    #[arg(long)]
    max_pairs: Option<usize>,
    /// Maximum SIFT keypoints per photo.
    #[arg(long)]
    max_features: Option<usize>,
    /// Lowe ratio-test threshold.
    #[arg(long)]
    lowe_ratio: Option<f64>,
    /// Minimum RANSAC inliers for the hard gate.
    #[arg(long)]
    min_inliers: Option<usize>,
    /// Minimum geometric overlap for the hard gate.
    #[arg(long)]
    min_overlap: Option<f64>,
    /// Allow more than one photo per configuration in an automatic group.
    #[arg(long)]
    allow_duplicate_config: bool,
    /// Per-device capture-time correction, as `device-key=seconds`. Repeatable.
    #[arg(long = "time-offset")]
    time_offsets: Vec<String>,
}

#[derive(Args)]
struct OverrideArgs {
    /// Path to the document, or a folder containing one.
    path: PathBuf,
    /// Add a photo to a group: `GROUP_ID=PHOTO_ID`.
    #[arg(long = "add")]
    add: Vec<String>,
    /// Remove a photo from a group: `GROUP_ID=PHOTO_ID`.
    #[arg(long = "remove")]
    remove: Vec<String>,
    /// Set a group's anchor: `GROUP_ID=PHOTO_ID`.
    #[arg(long = "anchor")]
    anchor: Vec<String>,
    /// Split photos out of a group: `GROUP_ID=PHOTO_ID,PHOTO_ID`.
    #[arg(long = "split")]
    split: Vec<String>,
    /// Merge groups: `TARGET_ID,OTHER_ID,...`.
    #[arg(long = "merge")]
    merge: Vec<String>,
    /// Allow more than one photo per configuration in a group.
    #[arg(long = "allow-duplicate")]
    allow_duplicate: Vec<String>,
    /// Discard the overrides already recorded in the document.
    #[arg(long)]
    replace: bool,
    /// Do not write the result back.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum ZoomPolicyArg {
    /// Use `FocalLengthIn35mmFilm` as reported.
    TrustExif,
    /// Multiply by `DigitalZoomRatio` for every device.
    ApplyDigitalZoom,
    /// Apply the zoom except for devices measured to fold it in already.
    PerDevice,
}

impl From<ZoomPolicyArg> for camcomp_core::schema::FocalZoomPolicy {
    fn from(value: ZoomPolicyArg) -> Self {
        match value {
            ZoomPolicyArg::TrustExif => Self::TrustExif,
            ZoomPolicyArg::ApplyDigitalZoom => Self::ApplyDigitalZoom,
            ZoomPolicyArg::PerDevice => Self::PerDevice,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum Format {
    /// Human-readable summary.
    Text,
    /// Machine-readable JSON.
    Json,
}

fn main() {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("camcomp: {e}");
            std::process::exit(EXIT_FAILURE);
        }
    }
}

/// Progress printer. Human output is rewritten in place so a long analysis does
/// not scroll a thousand lines past the reader.
struct Printer {
    json: bool,
    quiet: bool,
}

impl ProgressSink for Printer {
    fn emit(&self, event: &ProgressEvent) {
        if self.quiet {
            return;
        }
        let mut err = std::io::stderr().lock();
        if self.json {
            if let Ok(line) = serde_json::to_string(event) {
                let _ = writeln!(err, "{line}");
            }
            return;
        }
        match event {
            ProgressEvent::StageStarted { stage, total } => {
                let _ = match total {
                    Some(t) => writeln!(err, "  {} ({t} item(s))", stage.as_str()),
                    None => writeln!(err, "  {}", stage.as_str()),
                };
            }
            ProgressEvent::Advanced {
                stage,
                done,
                total,
                detail,
            } => {
                let suffix = detail.clone().unwrap_or_default();
                let _ = match total {
                    Some(t) => write!(err, "\r    {} {done}/{t} {suffix:<48}", stage.as_str()),
                    None => write!(err, "\r    {} {done} {suffix:<48}", stage.as_str()),
                };
                let _ = err.flush();
            }
            ProgressEvent::StageFinished { stage, elapsed_ms } => {
                let _ = writeln!(
                    err,
                    "\r    {} done in {elapsed_ms} ms{:<40}",
                    stage.as_str(),
                    ""
                );
            }
            ProgressEvent::Warning { stage, message } => {
                let _ = writeln!(err, "\r  ! [{}] {message}", stage.as_str());
            }
        }
    }
}

fn run(cli: &Cli) -> Result<i32, Box<dyn std::error::Error>> {
    let printer = Printer {
        json: cli.json_progress,
        quiet: cli.quiet,
    };
    match &cli.command {
        Command::Schema => {
            println!("{}", camcomp_core::schema::json_schema());
            Ok(0)
        }
        Command::Inspect {
            inputs,
            focal_zoom_policy,
            format,
        } => {
            let cancel = CancelToken::new();
            let reporter = camcomp_core::progress::Reporter::new(&printer, &cancel);
            let mut algorithm = camcomp_core::AlgorithmInfo::default();
            if let Some(policy) = focal_zoom_policy {
                algorithm.parameters.focal_zoom_policy = (*policy).into();
            }
            let inspection = pipeline::inspect(inputs, &algorithm, None, &reporter)?;
            match format {
                Format::Json => {
                    let payload: Vec<_> = inspection
                        .photos
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "id": p.id,
                                "path": p.relative_path,
                                "configurationId": p.configuration_id,
                                "metadata": p.metadata,
                                "original": [p.original_dimensions.0, p.original_dimensions.1],
                                "oriented": [p.oriented_dimensions.0, p.oriented_dimensions.1],
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                }
                Format::Text => {
                    println!("{} photo(s)", inspection.photos.len());
                    for config in &inspection.configurations {
                        println!(
                            "  configuration {} — {} ({} photo(s))",
                            config.id, config.display_name, config.photo_count
                        );
                    }
                    println!();
                    for photo in &inspection.photos {
                        let m = &photo.metadata;
                        // The effective focal length is what the predicate uses,
                        // so print the correction rather than hiding it.
                        let focal = match (m.focal_length_35mm, m.effective_focal_length_35mm) {
                            (Some(r), Some(e)) if (r - e).abs() > 0.5 => {
                                format!("{r:.0}->{e:.0}")
                            }
                            (Some(r), _) => format!("{r:.0}"),
                            (None, _) => "?".to_string(),
                        };
                        let fov = m
                            .effective_focal_length_35mm
                            .and_then(camcomp_core::sessions::diagonal_angle_of_view_degrees)
                            .map(|a| format!("{a:.0}deg"))
                            .unwrap_or_else(|| "?".into());
                        let mut mode = m.mode_label.clone().unwrap_or_else(|| "-".into());
                        if !m.mode_flags.is_empty() {
                            mode = format!("{mode}[{}]", m.mode_flags.join(","));
                        }
                        println!(
                            "  {:<52} {:>5}x{:<5} focal35={:<10} fov={:<7} {} mode={:<16} {}",
                            photo.relative_path,
                            photo.oriented_dimensions.0,
                            photo.oriented_dimensions.1,
                            focal,
                            fov,
                            if m.native_lens { "native" } else { "zoomed" },
                            mode,
                            m.corrected_at
                                .clone()
                                .unwrap_or_else(|| "no timestamp".into()),
                        );
                        for note in &photo.notes {
                            println!("      ! {note}");
                        }
                    }
                    if !inspection.skipped.is_empty() {
                        println!(
                            "\n  {} unsupported file(s) skipped",
                            inspection.skipped.len()
                        );
                    }
                }
            }
            Ok(0)
        }
        Command::Analyze {
            inputs,
            tuning,
            out,
            cache_in,
            no_cache,
            dry_run,
            force_pairs,
            table,
            expect_labels,
            format,
        } => {
            let mut options = AnalysisOptions::with_reuse();
            options.force_pairs = force_pairs.clone();
            apply_tuning(&mut options, tuning)?;

            let target = out.clone().or_else(|| {
                inputs.first().and_then(|p| {
                    if p.is_dir() {
                        Some(p.join(cache::CACHE_FILE_NAME))
                    } else {
                        p.parent().map(|d| d.join(cache::CACHE_FILE_NAME))
                    }
                })
            });

            let previous = if *no_cache {
                None
            } else {
                let candidate = cache_in.clone().or_else(|| target.clone());
                match candidate {
                    Some(path) if path.is_file() => Some(cache::load(&path)?),
                    _ => None,
                }
            };

            let cancel = install_interrupt_handler();
            let document =
                pipeline::analyze(inputs, &options, previous.as_ref(), &printer, &cancel)?;

            if !*dry_run {
                if let Some(path) = &target {
                    cache::save_atomic(path, &document)?;
                    if !cli.quiet {
                        eprintln!("wrote {}", path.display());
                    }
                }
            }

            if *table {
                print_table(&document);
            }

            match format {
                Format::Json => println!("{}", cache::to_json(&document)?),
                Format::Text => print_summary(&document),
            }

            if *expect_labels {
                let report = check_labels(&document);
                print_label_report(&report);
                if !report.satisfied() {
                    return Ok(EXIT_EXPECTATION);
                }
            }
            Ok(0)
        }
        Command::Validate { path } => {
            let path = resolve_document_path(path)?;
            let document = cache::load(&path)?;
            pipeline::validate(&document)?;
            println!(
                "{} is valid: {} photo(s), {} group(s), {} ungrouped",
                path.display(),
                document.photos.len(),
                document.groups.len(),
                document.ungrouped.len()
            );
            Ok(0)
        }
        Command::Explain { path, id } => {
            let path = resolve_document_path(path)?;
            let document = cache::load(&path)?;
            match id {
                None => {
                    print_summary(&document);
                    print_table(&document);
                }
                Some(id) => {
                    // Exact identifiers first. A photo ID is a substring of every
                    // pair ID that mentions it, so a fuzzy pair match must come
                    // last or asking about a photo silently answers about one of
                    // its pairs.
                    if let Some(pair) = document.pair(id) {
                        println!("{}\n  {}", pair.id, pipeline::explain_pair(pair));
                    } else if let Some(group) = document.group(id) {
                        print_group(&document, group);
                    } else if let Some(photo) = document.photo(id) {
                        print_photo(&document, photo);
                    } else if let Some(photo) = document
                        .photos
                        .iter()
                        .find(|p| p.path.contains(id.as_str()))
                    {
                        print_photo(&document, photo);
                    } else if let Some(pair) =
                        document.pairs.iter().find(|p| p.id.contains(id.as_str()))
                    {
                        println!("{}\n  {}", pair.id, pipeline::explain_pair(pair));
                    } else {
                        eprintln!("camcomp: `{id}` is not a known pair, group or photo");
                        return Ok(EXIT_USAGE);
                    }
                }
            }
            Ok(0)
        }
        Command::Override(args) => {
            let path = resolve_document_path(&args.path)?;
            let document = cache::load(&path)?;
            let mut overrides: Vec<ManualOverride> = if args.replace {
                Vec::new()
            } else {
                document.manual_overrides.clone()
            };
            let start = overrides.len();
            for (index, op) in parse_override_ops(args)?.into_iter().enumerate() {
                overrides.push(ManualOverride {
                    id: format!("override-{}", start + index + 1),
                    op,
                    created_at: Some(chrono_now()),
                    note: None,
                });
            }
            let updated = pipeline::regroup(&document, &overrides)?;
            if !args.dry_run {
                cache::save_atomic(&path, &updated)?;
                println!("wrote {}", path.display());
            }
            print_summary(&updated);
            Ok(0)
        }
    }
}

fn chrono_now() -> String {
    // The core already depends on chrono; the CLI only needs a timestamp string.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| format!("@{}", d.as_secs()))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn install_interrupt_handler() -> CancelToken {
    // No signal crate: the token is returned unarmed, so the CLI is cancellable
    // by the caller (and by a Tauri command) without adding a dependency just to
    // catch Ctrl-C, which the shell already handles.
    CancelToken::new()
}

fn apply_tuning(
    options: &mut AnalysisOptions,
    tuning: &Tuning,
) -> Result<(), Box<dyn std::error::Error>> {
    let p = &mut options.algorithm.parameters;
    if let Some(v) = tuning.session_gap {
        p.session_gap_seconds = v;
    }
    if tuning.no_session_gap {
        // One session for everything. The sliding-gap rule can never open a
        // second session at this value.
        p.session_gap_seconds = u32::MAX;
    }
    if let Some(v) = tuning.fov_tolerance {
        p.field_of_view_tolerance = v;
    }
    if let Some(v) = tuning.fov_tolerance_native_lens {
        p.field_of_view_tolerance_native_lens = v;
    }
    if let Some(v) = tuning.focal_zoom_policy {
        p.focal_zoom_policy = v.into();
    }
    if let Some(v) = tuning.threshold {
        p.compatibility_threshold = v;
    }
    if let Some(v) = tuning.analysis_edge {
        p.analysis_longest_edge = v;
    }
    if let Some(v) = tuning.max_pairs {
        p.max_pairs_per_session = v;
    }
    if let Some(v) = tuning.max_features {
        p.max_features_per_photo = v;
    }
    if let Some(v) = tuning.lowe_ratio {
        p.lowe_ratio = v;
    }
    if tuning.allow_duplicate_config {
        p.one_photo_per_configuration = false;
    }
    for entry in &tuning.time_offsets {
        let (device, seconds) = entry
            .split_once('=')
            .ok_or_else(|| format!("--time-offset expects `device-key=seconds`, got `{entry}`"))?;
        let seconds: i64 = seconds
            .parse()
            .map_err(|_| format!("`{seconds}` is not a whole number of seconds"))?;
        p.device_time_offset_seconds
            .insert(device.to_string(), seconds);
    }
    if let Some(v) = tuning.min_inliers {
        options.algorithm.gates.min_inliers = v;
    }
    if let Some(v) = tuning.min_overlap {
        options.algorithm.gates.min_geometric_overlap = v;
    }
    Ok(())
}

fn resolve_document_path(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if path.is_dir() {
        cache::find_in_folder(path)
            .ok_or_else(|| format!("no {} in {}", cache::CACHE_FILE_NAME, path.display()).into())
    } else if path.is_file() {
        Ok(path.to_path_buf())
    } else {
        Err(format!("{} does not exist", path.display()).into())
    }
}

fn parse_pair_arg(kind: &str, raw: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    raw.split_once('=')
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .ok_or_else(|| format!("--{kind} expects `GROUP_ID=PHOTO_ID`, got `{raw}`").into())
}

fn parse_override_ops(args: &OverrideArgs) -> Result<Vec<OverrideOp>, Box<dyn std::error::Error>> {
    let mut ops = Vec::new();
    for raw in &args.add {
        let (group_id, photo_id) = parse_pair_arg("add", raw)?;
        ops.push(OverrideOp::AddPhoto { group_id, photo_id });
    }
    for raw in &args.remove {
        let (group_id, photo_id) = parse_pair_arg("remove", raw)?;
        ops.push(OverrideOp::RemovePhoto { group_id, photo_id });
    }
    for raw in &args.anchor {
        let (group_id, photo_id) = parse_pair_arg("anchor", raw)?;
        ops.push(OverrideOp::SetAnchor { group_id, photo_id });
    }
    for raw in &args.split {
        let (group_id, list) = parse_pair_arg("split", raw)?;
        ops.push(OverrideOp::SplitGroup {
            group_id,
            photo_ids: list.split(',').map(|s| s.trim().to_string()).collect(),
        });
    }
    for raw in &args.merge {
        let group_ids: Vec<String> = raw.split(',').map(|s| s.trim().to_string()).collect();
        if group_ids.len() < 2 {
            return Err(format!("--merge needs at least two group IDs, got `{raw}`").into());
        }
        ops.push(OverrideOp::MergeGroups { group_ids });
    }
    for raw in &args.allow_duplicate {
        ops.push(OverrideOp::AllowDuplicateConfiguration {
            group_id: raw.trim().to_string(),
        });
    }
    Ok(ops)
}

fn print_summary(document: &AnalysisDocument) {
    let path_of: BTreeMap<&str, &str> = document
        .photos
        .iter()
        .map(|p| (p.id.as_str(), p.path.as_str()))
        .collect();

    if let Some(stats) = &document.stats {
        println!(
            "{} photo(s), {} session(s), {} candidate pair(s) — {} analyzed, {} reused, {} budget-skipped",
            stats.photos,
            stats.sessions,
            stats.candidate_pairs,
            stats.analyzed_pairs,
            stats.reused_pairs,
            stats.budget_skipped_pairs
        );
        println!(
            "pairs: {} compatible, {} incompatible, {} uncertain",
            stats.compatible_pairs, stats.incompatible_pairs, stats.uncertain_pairs
        );
        println!(
            "{} group(s), {} ungrouped — {} ms, {} features extracted{}",
            stats.groups,
            stats.ungrouped,
            stats.duration_ms,
            stats.features_extracted,
            if stats.cancelled { " (cancelled)" } else { "" }
        );
    }
    println!();
    for group in &document.groups {
        print_group_line(group, &path_of);
    }
    if !document.ungrouped.is_empty() {
        println!("\nungrouped:");
        for item in &document.ungrouped {
            println!(
                "  {:<52} {}",
                path_of.get(item.photo_id.as_str()).unwrap_or(&"?"),
                serde_plain(&item.reason)
            );
        }
    }
}

fn print_group_line(group: &camcomp_core::Group, path_of: &BTreeMap<&str, &str>) {
    let score = group
        .evidence
        .min_anchor_score
        .map(|s| format!("{s:.3}"))
        .unwrap_or_else(|| "-".into());
    let flags: Vec<String> = group.flags.iter().map(serde_plain).collect();
    println!(
        "{} — {} photo(s), min score {score}{}",
        group.id,
        group.photo_ids.len(),
        if flags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", flags.join(", "))
        }
    );
    for id in &group.photo_ids {
        let is_anchor = *id == group.anchor_photo_id;
        let marker = if is_anchor { "*" } else { " " };
        let alignment = group.alignments.iter().find(|a| a.photo_id == *id);
        let status = match (is_anchor, alignment) {
            (true, _) => "anchor".to_string(),
            (false, Some(a)) => format!(
                "confidence {:.2} ({})",
                a.confidence,
                serde_plain(&a.provenance)
            ),
            // Must not read as "fine": without a transform the viewer cannot
            // align this member, and the user needs to know that.
            (false, None) => "NO ALIGNMENT — pair not analyzed".to_string(),
        };
        println!(
            "  {marker} {:<52} {status}",
            path_of.get(id.as_str()).unwrap_or(&"?")
        );
    }
}

/// One photo, its metadata and every pair it takes part in — including the pair
/// IDs, which is how a user gets an argument for `--force-pair`.
fn print_photo(document: &AnalysisDocument, photo: &camcomp_core::Photo) {
    println!("{} — {}", photo.id, photo.path);
    println!(
        "  configuration {}  focal35 {}  mode {}",
        photo.configuration_id,
        photo
            .metadata
            .focal_length_35mm
            .map(|f| format!("{f:.0} mm"))
            .unwrap_or_else(|| "unknown".into()),
        photo
            .metadata
            .mode_label
            .clone()
            .unwrap_or_else(|| "-".into())
    );
    let in_group = document
        .groups
        .iter()
        .find(|g| g.photo_ids.contains(&photo.id));
    match in_group {
        Some(g) => println!("  in group {}", g.id),
        None => {
            let reason = document
                .ungrouped
                .iter()
                .find(|u| u.photo_id == photo.id)
                .map(|u| serde_plain(&u.reason))
                .unwrap_or_else(|| "?".into());
            println!("  ungrouped: {reason}");
        }
    }
    let path_of: BTreeMap<&str, &str> = document
        .photos
        .iter()
        .map(|p| (p.id.as_str(), p.path.as_str()))
        .collect();
    for pair in document
        .pairs
        .iter()
        .filter(|p| p.a == photo.id || p.b == photo.id)
    {
        let other = if pair.a == photo.id { &pair.b } else { &pair.a };
        println!(
            "  {}\n    with {} — {}",
            pair.id,
            path_of.get(other.as_str()).unwrap_or(&"?"),
            pipeline::explain_pair(pair)
        );
    }
}

fn print_group(document: &AnalysisDocument, group: &camcomp_core::Group) {
    let path_of: BTreeMap<&str, &str> = document
        .photos
        .iter()
        .map(|p| (p.id.as_str(), p.path.as_str()))
        .collect();
    print_group_line(group, &path_of);
    println!("  evidence: {:?}", group.evidence);
    for alignment in &group.alignments {
        println!(
            "  alignment {} ({} correspondence(s)): {:?}",
            alignment.photo_id,
            alignment.correspondences.len(),
            alignment.homography
        );
    }
    for pair in document
        .pairs
        .iter()
        .filter(|p| group.photo_ids.contains(&p.a) && group.photo_ids.contains(&p.b))
    {
        println!("  {} → {}", pair.id, pipeline::explain_pair(pair));
    }
}

fn print_table(document: &AnalysisDocument) {
    let short: BTreeMap<&str, String> = document
        .photos
        .iter()
        .map(|p| {
            let name = p.path.rsplit('/').next().unwrap_or(&p.path);
            (p.id.as_str(), name.to_string())
        })
        .collect();
    println!(
        "\n{:<34} {:<34} {:<13} {:<8} {:<22} {:>7} {:>6} {:>8} {:>8} {:>8} {:>8} {:>6} {:>7}",
        "a",
        "b",
        "state",
        "score",
        "reason",
        "fovdiff",
        "fovtol",
        "overlap",
        "coverg",
        "confid",
        "appear",
        "inl",
        "reproj"
    );
    for pair in &document.pairs {
        let row = pipeline::pair_row(pair);
        let get = |k: &str| row.get(k).cloned().unwrap_or_default();
        println!(
            "{:<34} {:<34} {:<13} {:<8} {:<22} {:>7} {:>6} {:>8} {:>8} {:>8} {:>8} {:>6} {:>7}",
            short.get(pair.a.as_str()).cloned().unwrap_or_default(),
            short.get(pair.b.as_str()).cloned().unwrap_or_default(),
            get("state"),
            get("score"),
            get("reason"),
            get("fovdiff"),
            get("fovtol"),
            get("overlap"),
            get("coverage"),
            get("confidence"),
            get("appearance"),
            get("inliers"),
            get("reproj_px"),
        );
    }
}

fn serde_plain<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "?".to_string())
}

// ---------------------------------------------------------------------------
// Label checking against the `gN-` filename convention
// ---------------------------------------------------------------------------

/// Expected grouping derived from the reference-set filename convention:
/// `g1-`, `g2-` … share a group; `gx-` is deliberately undecided
/// (`PLAN-DROP1.md` §5.1).
#[derive(Debug, Default)]
struct LabelReport {
    expected_groups: usize,
    matched_groups: usize,
    /// Labelled group -> the photos it should contain but no single group did.
    missing: Vec<(String, Vec<String>)>,
    /// Groups the analysis produced that no label asked for.
    unexpected: Vec<String>,
    /// Pairs the analysis grouped together that the labels put apart.
    false_positives: Vec<(String, String)>,
    undecided: Vec<String>,
}

impl LabelReport {
    fn satisfied(&self) -> bool {
        self.false_positives.is_empty() && self.missing.is_empty()
    }
}

fn label_of(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let prefix = name.split('-').next()?;
    let rest = prefix.strip_prefix('g')?;
    if rest.is_empty() {
        return None;
    }
    Some(prefix.to_string())
}

fn check_labels(document: &AnalysisDocument) -> LabelReport {
    let mut report = LabelReport::default();
    let mut expected: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for photo in &document.photos {
        match label_of(&photo.path) {
            Some(label) if label == "gx" => report.undecided.push(photo.path.clone()),
            Some(label) => expected.entry(label).or_default().push(photo.id.as_str()),
            None => {}
        }
    }
    // A label with a single member expects no group, since a group is meaningful
    // at two members.
    expected.retain(|_, members| members.len() >= 2);
    report.expected_groups = expected.len();

    let label_of_id: BTreeMap<&str, String> = document
        .photos
        .iter()
        .filter_map(|p| label_of(&p.path).map(|l| (p.id.as_str(), l)))
        .collect();
    let path_of: BTreeMap<&str, &str> = document
        .photos
        .iter()
        .map(|p| (p.id.as_str(), p.path.as_str()))
        .collect();

    for group in &document.groups {
        let labels: Vec<&String> = group
            .photo_ids
            .iter()
            .filter_map(|id| label_of_id.get(id.as_str()))
            .collect();
        // Two photos in one group with different labels is a false positive: the
        // error that silently corrupts a comparison (`SPEC.md` §9).
        for i in 0..group.photo_ids.len() {
            for j in (i + 1)..group.photo_ids.len() {
                let (a, b) = (&group.photo_ids[i], &group.photo_ids[j]);
                let (la, lb) = (label_of_id.get(a.as_str()), label_of_id.get(b.as_str()));
                if let (Some(la), Some(lb)) = (la, lb) {
                    if la != lb && la != "gx" && lb != "gx" {
                        report.false_positives.push((
                            path_of.get(a.as_str()).unwrap_or(&"?").to_string(),
                            path_of.get(b.as_str()).unwrap_or(&"?").to_string(),
                        ));
                    }
                }
            }
        }
        let consistent = labels
            .first()
            .map(|first| labels.iter().all(|l| l == first || *l == "gx"));
        if consistent != Some(true) {
            report.unexpected.push(group.id.clone());
        }
    }

    for (label, members) in &expected {
        let complete = document
            .groups
            .iter()
            .any(|g| members.iter().all(|m| g.photo_ids.iter().any(|p| p == m)));
        if complete {
            report.matched_groups += 1;
        } else {
            report.missing.push((
                label.clone(),
                members
                    .iter()
                    .map(|m| path_of.get(*m).unwrap_or(&"?").to_string())
                    .collect(),
            ));
        }
    }
    report
}

fn print_label_report(report: &LabelReport) {
    println!(
        "\nlabels: {}/{} expected group(s) reproduced, {} false positive(s)",
        report.matched_groups,
        report.expected_groups,
        report.false_positives.len()
    );
    for (a, b) in &report.false_positives {
        println!("  ! grouped together but labelled apart: {a} + {b}");
    }
    for (label, members) in &report.missing {
        println!("  - {label} not reproduced: {}", members.join(" + "));
    }
    if !report.undecided.is_empty() {
        println!(
            "  ? undecided by label (gx): {}",
            report.undecided.join(", ")
        );
    }
}
