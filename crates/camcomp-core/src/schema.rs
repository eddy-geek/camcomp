//! The `camcomp.json` contract (`SPEC.md` §8).
//!
//! Every type here is camcomp-owned: no `image`, `ndarray`, `nalgebra` or Tauri
//! type appears in the document, so the file stays portable between the native
//! core, a WASM build and the TypeScript frontend.
//!
//! Field naming is `camelCase` in JSON and `snake_case` in Rust. Unknown fields
//! are ignored on read so a newer minor version stays loadable.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::geometry::Mat3;

/// Schema version of documents this build writes.
pub const SCHEMA_VERSION: &str = "1.0";
/// Schema major version this build can read.
pub const SCHEMA_MAJOR: u32 = 1;
/// Algorithm identity persisted with every result.
pub const ALGORITHM_NAME: &str = "sift-homography-gradient-v1";
/// Algorithm version. Bumping it invalidates cached pair results.
pub const ALGORITHM_VERSION: &str = "1.0.0";
/// Application version, from the crate manifest.
pub const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Parameters and provenance
// ---------------------------------------------------------------------------

/// Tunable analysis parameters, persisted so a result can be explained and so
/// changing a threshold invalidates exactly the cached work that depended on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnalysisParameters {
    /// Sliding gap that separates two shooting sessions (`SPEC.md` §4.1).
    ///
    /// Set to [`u32::MAX`] to disable session detection entirely, which puts
    /// every photo in one session. That is a calibration instrument, not a
    /// product default: it removes the cheapest prefilter and makes the visual
    /// half decide every pair.
    pub session_gap_seconds: u32,
    /// Maximum relative difference of *diagonal angle of view*, derived from the
    /// 35 mm equivalent focal lengths (`SPEC.md` §4.2).
    ///
    /// Not a focal ratio: angle of view is `2·atan(d / 2f)`, so a constant
    /// angular tolerance is more permissive at the wide end and stricter at the
    /// long end than a constant focal ratio. `0.15` here is within the valid
    /// window measured over the labelled sets: the widest pair that must be
    /// accepted under this tier now measures 0.147 (`tstgrp3` g1 and `tstgrp5`
    /// g1, after `tstgrp5` was relabelled into a single group) and the closest
    /// that must be rejected measures 0.750 (`IMPLEM-DROP1.md` §6.11, revised).
    pub field_of_view_tolerance: f64,
    /// Same tolerance, applied when *both* photos were taken at a native lens
    /// step — no digital zoom or sensor crop on either side.
    ///
    /// Two phones' ultrawide cameras differ in framing as a matter of hardware
    /// (measured: 15 mm against 12 mm equivalent, 10 % of angle of view), yet
    /// "both ultrawides" is exactly the comparison the product exists for. A
    /// deliberate zoom step by the photographer is a different intent and stays
    /// under the strict tolerance.
    #[serde(default = "default_native_lens_fov_tolerance")]
    pub field_of_view_tolerance_native_lens: f64,
    /// Whether `FocalLengthIn35mmFilm` is taken at face value or combined with
    /// `DigitalZoomRatio` (`IMPLEM-DROP1.md` §6.7).
    #[serde(default)]
    pub focal_zoom_policy: FocalZoomPolicy,
    /// Composite score a pair must reach once every hard gate passes.
    pub compatibility_threshold: f64,
    /// Longest edge of the analysis image, in pixels.
    pub analysis_longest_edge: u32,
    /// Upper bound on SIFT keypoints retained per photo.
    pub max_features_per_photo: usize,
    /// Lowe ratio-test threshold.
    pub lowe_ratio: f64,
    /// Require forward and reverse nearest neighbours to agree.
    pub mutual_match: bool,
    /// RANSAC inlier threshold, in analysis pixels.
    pub ransac_threshold_px: f64,
    /// RANSAC iteration cap.
    pub ransac_max_iterations: usize,
    /// RANSAC seed. Fixed, so a rerun reproduces the same inlier set.
    pub ransac_seed: u64,
    /// Upper bound on persisted correspondences per aligned photo.
    pub max_correspondences: usize,
    /// Below this keypoint count a photo cannot produce a decision, only
    /// `uncertain` (`SPEC.md` §5.2).
    pub min_keypoints_for_decision: usize,
    /// Below this ratio-filtered match count a pair stays `uncertain`.
    pub min_matches_for_decision: usize,
    /// Guard on decoded pixel count, per photo.
    pub max_megapixels: f64,
    /// Keep at most one photo per configuration in a group (`SPEC.md` §6).
    ///
    /// On by default, because the product compares configurations against each
    /// other. Turning it off lets two shots from the same device and mode group
    /// together, which some labelled sets expect.
    #[serde(default = "default_true")]
    pub one_photo_per_configuration: bool,
    /// Per-device capture-time corrections, in seconds, keyed by device key.
    #[serde(default)]
    pub device_time_offset_seconds: BTreeMap<String, i64>,
}

impl Default for AnalysisParameters {
    fn default() -> Self {
        Self {
            // 60 rather than the 30 of `REQUIREMENTS.md` §3: three labelled
            // groups sit 31, 36 and 50 s apart, which is two device clocks
            // disagreeing, not a photographer taking a break
            // (`IMPLEM-DROP1.md` §6.12).
            session_gap_seconds: 60,
            // Calibrated, not assumed: over the labelled sets the widest pair
            // that must be accepted under this tier measures 0.147 and the
            // closest that must be rejected measures 0.750
            // (`IMPLEM-DROP1.md` §6.11, revised).
            field_of_view_tolerance: 0.15,
            field_of_view_tolerance_native_lens: default_native_lens_fov_tolerance(),
            focal_zoom_policy: FocalZoomPolicy::default(),
            compatibility_threshold: 0.80,
            analysis_longest_edge: 1200,
            max_features_per_photo: 4000,
            lowe_ratio: 0.75,
            mutual_match: true,
            ransac_threshold_px: 3.0,
            ransac_max_iterations: 2000,
            // Fixed so a rerun reproduces the same inlier set.
            ransac_seed: 0x00CA_3C09_9911,
            max_correspondences: 200,
            min_keypoints_for_decision: 150,
            min_matches_for_decision: 12,
            max_megapixels: 120.0,
            one_photo_per_configuration: true,
            device_time_offset_seconds: BTreeMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_native_lens_fov_tolerance() -> f64 {
    // Twice the strict tier. Accepts the labelled 12 mm / 15 mm ultrawide pair
    // at 0.104 and still rejects an ultrawide against a main camera, the closest
    // of which measures 0.278.
    0.20
}

/// How to reconcile `FocalLengthIn35mmFilm` with `DigitalZoomRatio`.
///
/// EXIF defines the 35 mm equivalent as a property of the *lens*, so a
/// spec-abiding device reports the un-zoomed value and leaves the zoom in its own
/// tag. Some devices fold the zoom in anyway, and comparing one convention
/// against the other reports a 46 % focal difference between two photos that
/// measurably share a framing (`IMPLEM-DROP1.md` §6.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FocalZoomPolicy {
    /// Use `FocalLengthIn35mmFilm` exactly as reported.
    TrustExif,
    /// Multiply by `DigitalZoomRatio` whenever it exceeds 1, for every device.
    ApplyDigitalZoom,
    /// Apply the zoom except for devices measured to fold it in already. The
    /// per-device decision is recorded on each photo, so a document always says
    /// which convention produced its focal lengths.
    #[default]
    PerDevice,
}

/// Weights of the composite compatibility score (`SPEC.md` §5.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetricWeights {
    /// Weight of [`PairMetrics::geometric_overlap`].
    pub geometric_overlap: f64,
    /// Weight of [`PairMetrics::feature_coverage`].
    pub feature_coverage: f64,
    /// Weight of [`PairMetrics::inlier_confidence`].
    pub inlier_confidence: f64,
    /// Weight of [`PairMetrics::aligned_appearance_similarity`].
    pub aligned_appearance_similarity: f64,
}

impl Default for MetricWeights {
    fn default() -> Self {
        Self {
            geometric_overlap: 0.35,
            feature_coverage: 0.25,
            inlier_confidence: 0.20,
            aligned_appearance_similarity: 0.20,
        }
    }
}

impl MetricWeights {
    /// Sum of the weights. Used to normalize the score so a reweighted set does
    /// not silently move the threshold.
    pub fn sum(&self) -> f64 {
        self.geometric_overlap
            + self.feature_coverage
            + self.inlier_confidence
            + self.aligned_appearance_similarity
    }
}

/// Geometric hard gates (`SPEC.md` §5.2). A pair that fails any of these cannot
/// be compatible no matter how high the composite score is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HardGates {
    /// Minimum RANSAC inlier count.
    pub min_inliers: usize,
    /// Maximum median reprojection error, in analysis pixels.
    pub max_median_reprojection_error_px: f64,
    /// Side of the square occupancy grid.
    pub grid_size: u32,
    /// Minimum occupied cells, required in *both* images.
    pub min_occupied_cells: usize,
    /// Minimum geometric overlap.
    pub min_geometric_overlap: f64,
    /// Minimum affine scale ratio of the fitted transform.
    pub min_scale: f64,
    /// Maximum affine scale ratio of the fitted transform.
    pub max_scale: f64,
    /// Maximum projective row magnitude in normalized coordinates.
    pub max_perspective: f64,
}

impl Default for HardGates {
    fn default() -> Self {
        Self {
            min_inliers: 25,
            max_median_reprojection_error_px: 3.0,
            grid_size: 4,
            min_occupied_cells: 5,
            min_geometric_overlap: 0.60,
            min_scale: 0.2,
            max_scale: 5.0,
            max_perspective: 0.6,
        }
    }
}

/// Algorithm identity and configuration, persisted with every document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AlgorithmInfo {
    /// Stable algorithm name.
    pub name: String,
    /// Algorithm version. A change invalidates cached pair results.
    pub version: String,
    /// Tunables in force for this document.
    pub parameters: AnalysisParameters,
    /// Score weights in force for this document.
    pub weights: MetricWeights,
    /// Hard gates in force for this document.
    pub gates: HardGates,
    /// `true` while the thresholds are still derived from a small labelled set
    /// (`SPEC.md` §9). Honest defaults beat confident ones.
    pub thresholds_provisional: bool,
}

impl Default for AlgorithmInfo {
    fn default() -> Self {
        Self {
            name: ALGORITHM_NAME.to_string(),
            version: ALGORITHM_VERSION.to_string(),
            parameters: AnalysisParameters::default(),
            weights: MetricWeights::default(),
            gates: HardGates::default(),
            thresholds_provisional: true,
        }
    }
}

impl AlgorithmInfo {
    /// Cache key covering everything that can change a pair result.
    ///
    /// Excludes parameters that only affect grouping or output volume, so that
    /// changing, say, `max_correspondences` does not force a full re-analysis.
    pub fn pair_cache_key(&self) -> String {
        let p = &self.parameters;
        let g = &self.gates;
        let w = &self.weights;
        format!(
            "{}/{}|edge={}|feat={}|lowe={}|mutual={}|ransac={}:{}:{}|minkp={}|minm={}|gates={}:{}:{}:{}:{}:{}:{}:{}|w={}:{}:{}:{}|thr={}",
            self.name,
            self.version,
            p.analysis_longest_edge,
            p.max_features_per_photo,
            p.lowe_ratio,
            p.mutual_match,
            p.ransac_threshold_px,
            p.ransac_max_iterations,
            p.ransac_seed,
            p.min_keypoints_for_decision,
            p.min_matches_for_decision,
            g.min_inliers,
            g.max_median_reprojection_error_px,
            g.grid_size,
            g.min_occupied_cells,
            g.min_geometric_overlap,
            g.min_scale,
            g.max_scale,
            g.max_perspective,
            w.geometric_overlap,
            w.feature_coverage,
            w.inlier_confidence,
            w.aligned_appearance_similarity,
            p.compatibility_threshold,
        )
    }
}

// ---------------------------------------------------------------------------
// Photos
// ---------------------------------------------------------------------------

/// Cheap change-detection plus the content identity used for cache reuse
/// (`SPEC.md` §3.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    /// File size in bytes.
    pub size: u64,
    /// Modification time, RFC 3339. `None` when the filesystem does not report
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    /// SHA-256 of the file contents. The only field that is part of cache
    /// *identity*.
    pub sha256: String,
}

impl Fingerprint {
    /// Whether the cheap fields match, permitting the digest to be assumed
    /// unchanged without re-hashing.
    pub fn fast_path_matches(&self, other: &Fingerprint) -> bool {
        self.size == other.size && self.modified_at == other.modified_at
    }
}

/// Original, oriented and analysis dimensions, retained so any persisted
/// normalized coordinate can be converted back to pixels (`SPEC.md` §3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Dimensions {
    /// Dimensions as stored in the file.
    pub original: [u32; 2],
    /// Dimensions after applying orientation.
    pub oriented: [u32; 2],
    /// Dimensions of the reduced analysis image, when one was produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<[u32; 2]>,
}

/// Where a capture timestamp came from. Recorded because the answer changes how
/// much the session assignment can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TimestampSource {
    /// `DateTimeOriginal`, with `OffsetTimeOriginal` when present.
    ExifDateTimeOriginal,
    /// `CreateDate` / `DateTimeDigitized`.
    ExifCreateDate,
    /// `DateTime` (file change time recorded by the camera).
    ExifModifyDate,
    /// Filesystem modification time. A weak fallback.
    FileModified,
    /// No usable timestamp. Forces manual grouping.
    Missing,
}

/// How the 35 mm equivalent focal length was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FocalSource {
    /// Read from `FocalLengthIn35mmFilm`.
    Exif35mm,
    /// `FocalLengthIn35mmFilm` multiplied by `DigitalZoomRatio`, because the
    /// device reports the two independently ([`FocalZoomPolicy`]).
    Exif35mmZoomApplied,
    /// Derived from the physical focal length and a known crop factor.
    DerivedFromCropFactor,
    /// Unknown. Focal comparisons involving this photo are `uncertain`.
    Unknown,
}

/// Normalized metadata for one photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PhotoMetadata {
    /// Capture instant as recorded, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
    /// Where [`Self::captured_at`] came from.
    pub capture_time_source: TimestampSource,
    /// `true` when no timezone was available and one was assumed.
    pub timezone_assumed: bool,
    /// Capture instant after the per-device offset, RFC 3339. This is what
    /// session detection uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corrected_at: Option<String>,
    /// Per-device offset applied, in seconds.
    pub applied_time_offset_seconds: i64,
    /// EXIF orientation as found. `1` when absent or invalid.
    pub exif_orientation: u16,
    /// `true` when the decoder already delivered oriented pixels, so the EXIF
    /// orientation must *not* be applied again (HEIF container transforms).
    pub orientation_applied_by_decoder: bool,
    /// EXIF `Make`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_make: Option<String>,
    /// EXIF `Model`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,
    /// EXIF `LensModel`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lens_model: Option<String>,
    /// Physical focal length, in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focal_length_mm: Option<f64>,
    /// 35 mm equivalent focal length exactly as reported, in millimetres.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focal_length_35mm: Option<f64>,
    /// The equivalent focal length actually compared, after the
    /// [`FocalZoomPolicy`]. Equal to [`Self::focal_length_35mm`] unless a zoom
    /// was applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_focal_length_35mm: Option<f64>,
    /// Where the equivalent focal length came from.
    pub focal_source: FocalSource,
    /// Digital zoom ratio, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digital_zoom_ratio: Option<f64>,
    /// `true` when this photo used a lens at its native field of view — no
    /// digital zoom and no sensor crop. Photos that are both native compare
    /// under [`AnalysisParameters::field_of_view_tolerance_native_lens`].
    #[serde(default)]
    pub native_lens: bool,
    /// Raw value of the tag that identifies the shooting mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_tag_raw: Option<String>,
    /// Which EXIF tag the mode was read from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_tag_source: Option<String>,
    /// Normalized mode label, for example `auto`, `master`, `hi-res`, `portrait`,
    /// `night` or `text` (see `SPEC-OPPO.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_label: Option<String>,
    /// Capture flags that are *not* part of the configuration identity, for
    /// example `portrait`, `burst` or `flash-fired` (see `SPEC-OPPO.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mode_flags: Vec<String>,
    /// Bits of the mode tag that this version does not interpret, as hex.
    ///
    /// Present means "this device said something we have not verified". The bits
    /// fork the configuration rather than being folded into a known mode,
    /// because silently merging two shooting modes corrupts Drop 4 scoring
    /// while an odd configuration name is visible and correctable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_tag_unknown_bits: Option<String>,
}

/// Diagnostics from feature extraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FeatureSummary {
    /// Keypoints retained.
    pub keypoints: usize,
    /// Extraction wall time.
    pub duration_ms: u64,
    /// `true` when the count fell below
    /// [`AnalysisParameters::min_keypoints_for_decision`].
    pub low_texture: bool,
}

/// One input photo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    /// Stable analysis ID, derived from the path relative to the source root.
    pub id: String,
    /// Path relative to the source root, using `/` separators.
    pub path: String,
    /// Configuration (device + mode) this photo belongs to.
    pub configuration_id: String,
    /// Content identity and change-detection fields.
    pub fingerprint: Fingerprint,
    /// Original, oriented and analysis dimensions.
    pub dimensions: Dimensions,
    /// Normalized metadata.
    pub metadata: PhotoMetadata,
    /// Feature-extraction diagnostics, when the photo was analyzed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<FeatureSummary>,
    /// Non-fatal observations, for example an unreadable EXIF block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A device + shooting-mode combination (`SPEC.md` §2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Configuration {
    /// Stable configuration ID.
    pub id: String,
    /// Device identity without the mode, for per-device clock offsets and
    /// per-device scoring.
    pub device_key: String,
    /// EXIF `Make`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    /// EXIF `Model`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Normalized mode label, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_label: Option<String>,
    /// Human-readable label for the UI.
    pub display_name: String,
    /// Photos assigned to this configuration.
    pub photo_count: usize,
}

/// A chronological run of photos with no gap longer than the session gap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Stable session ID.
    pub id: String,
    /// Member photo IDs, in corrected-capture-time order.
    pub photo_ids: Vec<String>,
    /// First corrected capture time in the session, RFC 3339.
    pub start: String,
    /// Last corrected capture time in the session, RFC 3339.
    pub end: String,
}

// ---------------------------------------------------------------------------
// Pairs
// ---------------------------------------------------------------------------

/// Outcome of a candidate pair. `Uncertain` is first class and is never coerced
/// into a decision (`SPEC.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PairState {
    /// Same scene: every hard gate passed and the score met the threshold.
    Compatible,
    /// Enough evidence to conclude the photos do not show the same scene.
    Incompatible,
    /// Evidence too weak to decide.
    Uncertain,
}

/// Why a pair ended up `uncertain`, or why it was not analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PairReason {
    /// One or both photos yielded too few keypoints.
    LowTexture,
    /// Too few surviving descriptor matches to fit anything.
    InsufficientMatches,
    /// RANSAC found no consensus.
    NoConsensus,
    /// A hard gate failed.
    GateFailed,
    /// Gates passed but the composite score fell short.
    ScoreBelowThreshold,
    /// Both 35 mm equivalent focal lengths are known and outside tolerance, so
    /// the pair was rejected without spending a visual analysis on it.
    FocalMismatch,
    /// A photo could not be decoded.
    Unreadable,
}

/// Result of comparing two focal lengths (`SPEC.md` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum FocalCompatibility {
    /// Both equivalent focal lengths known and within tolerance.
    Compatible,
    /// Both known and outside tolerance.
    Incompatible,
    /// At least one equivalent focal length unknown. Does not exclude visual
    /// analysis; the visual evidence is the stronger signal.
    Unknown,
}

/// The framing comparison for one pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FocalComparison {
    /// Verdict.
    pub state: FocalCompatibility,
    /// Relative difference of diagonal angle of view. **This is the value the
    /// verdict is based on.**
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_fov_difference: Option<f64>,
    /// Relative difference of effective 35 mm equivalent focal length. Recorded
    /// for explanation only; it is not what the predicate compares.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_focal_difference: Option<f64>,
    /// The tolerance this comparison was judged against. Two tiers exist, so
    /// the verdict is not explainable from the difference alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_tolerance: Option<f64>,
    /// `true` when both photos were at a native lens step and the relaxed
    /// tolerance applied.
    #[serde(default)]
    pub native_lens_pair: bool,
}

/// Which hard gate failed. Every rejection names its gate (`PLAN-DROP1.md` §12.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GateFailure {
    /// Fewer inliers than [`HardGates::min_inliers`].
    Inliers,
    /// Median reprojection error above the limit.
    ReprojectionError,
    /// Occupancy grid too concentrated in the first image.
    SpatialDistributionA,
    /// Occupancy grid too concentrated in the second image.
    SpatialDistributionB,
    /// Geometric overlap below [`HardGates::min_geometric_overlap`].
    GeometricOverlap,
    /// Transform is singular or non-invertible.
    Degenerate,
    /// Transform mirrors the image.
    Reflection,
    /// Affine scale outside the plausible band.
    Scale,
    /// Projective row too strong to be a plausible viewpoint change.
    Perspective,
}

/// Normalized components and raw diagnostics for one pair (`SPEC.md` §5.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairMetrics {
    /// Keypoints in the first photo.
    pub keypoints_a: usize,
    /// Keypoints in the second photo.
    pub keypoints_b: usize,
    /// Matches surviving the ratio test (and mutual check when enabled).
    pub filtered_matches: usize,
    /// RANSAC inliers.
    pub inliers: usize,
    /// `inliers / filtered_matches`.
    pub inlier_ratio: f64,
    /// Median reprojection error, in analysis pixels.
    pub median_reprojection_error_px: f64,
    /// Occupied grid cells in the first photo.
    pub occupied_cells_a: usize,
    /// Occupied grid cells in the second photo.
    pub occupied_cells_b: usize,
    /// Normalized component: intersection over the smaller area, in the anchor
    /// frame.
    pub geometric_overlap: f64,
    /// Normalized component: spatial extent and grid spread of inliers.
    pub feature_coverage: f64,
    /// Normalized component: count, ratio and reprojection quality combined.
    pub inlier_confidence: f64,
    /// Normalized component: gradient similarity over the registered overlap.
    pub aligned_appearance_similarity: f64,
    /// Post-registration SSIM. Diagnostic only; never decides compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered_ssim: Option<f64>,
    /// Affine scale of the fitted transform (largest singular value).
    pub affine_scale: f64,
    /// Pair analysis wall time.
    pub duration_ms: u64,
}

/// One analyzed or skipped candidate pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PairResult {
    /// Canonical pair ID, `pair-<a>-<b>` with `a < b` by photo ID.
    pub id: String,
    /// First photo ID, the lexicographically smaller of the two.
    pub a: String,
    /// Second photo ID.
    pub b: String,
    /// Session both photos belong to.
    pub session_id: String,
    /// Absolute capture-time distance, in seconds.
    pub time_distance_seconds: i64,
    /// Verdict.
    pub state: PairState,
    /// Why, when the verdict is not `compatible`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<PairReason>,
    /// Composite score, when it could be computed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    /// Whether every hard gate passed.
    pub hard_gates_passed: bool,
    /// Gates that failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_failures: Vec<GateFailure>,
    /// Focal comparison.
    pub focal: FocalComparison,
    /// Metrics, when the pair was visually analyzed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<PairMetrics>,
    /// Row-major `3 × 3` homography mapping normalized oriented coordinates of
    /// `a` into those of `b`. Present whenever a transform was fitted, even if
    /// the pair was rejected, so a rejection stays inspectable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homography_a_to_b: Option<Vec<f64>>,
    /// Bounded, spatially sampled inlier correspondences in normalized
    /// coordinates, `a` to `b`. Retained only for compatible pairs, which are
    /// the only ones that can form a group (`SPEC.md` §7.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correspondences: Vec<Correspondence>,
}

/// A single inlier correspondence, in normalized coordinates.
///
/// The `photo` / `anchor` naming follows the persisted direction: within a
/// [`PairResult`] the fields hold `a` and `b`; within an [`Alignment`] they hold
/// the member photo and the group anchor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Correspondence {
    /// Normalized point in the source photo.
    pub photo: [f64; 2],
    /// Normalized point in the target (anchor) photo.
    pub anchor: [f64; 2],
    /// Descriptor-match quality in `[0, 1]`, higher is better.
    pub match_quality: f64,
    /// Reprojection error in normalized anchor units.
    pub reprojection_error_normalized: f64,
    /// Local feature scale in the source photo, normalized by its longest edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Local feature orientation in the source photo, in degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

/// Coordinate system of a persisted transform. Only one value exists today; it
/// is spelled out so a future addition is a schema change and not a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CoordinateSystem {
    /// `[0, 1] × [0, 1]` over the oriented image, origin top-left, y down.
    NormalizedOriented,
}

/// Direction of a persisted transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TransformDirection {
    /// `anchorPoint ~ H · photoPoint`.
    PhotoToAnchor,
}

/// How an alignment was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AlignmentProvenance {
    /// Directly from the photo↔anchor pair analysis.
    DirectPair,
    /// Composed through another member because no direct pair was analyzed.
    /// Correspondences were remapped through the composition.
    Composed,
    /// A manual membership with no usable pair evidence. No transform.
    ManualUnverified,
}

/// Group-relative alignment for one non-anchor member (`SPEC.md` §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Alignment {
    /// Member this alignment describes.
    pub photo_id: String,
    /// Coordinate system of [`Self::homography`].
    pub coordinate_system: CoordinateSystem,
    /// Direction of [`Self::homography`].
    pub direction: TransformDirection,
    /// Row-major `3 × 3` homography, photo to anchor.
    pub homography: Vec<f64>,
    /// Row-major inverse, anchor to photo. Persisted so clients need no linear
    /// algebra; validated on load.
    pub inverse_homography: Vec<f64>,
    /// Bounded, spatially distributed correspondences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub correspondences: Vec<Correspondence>,
    /// Alignment confidence in `[0, 1]`, surfaced by the viewer so it can fall
    /// back gracefully.
    pub confidence: f64,
    /// How this alignment was derived.
    pub provenance: AlignmentProvenance,
}

/// Something worth surfacing about a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum GroupFlag {
    /// Members disagree pairwise; the automatic rules would not have built this.
    InternalDisagreement,
    /// More than one photo from the same configuration.
    DuplicateConfiguration,
    /// Contains a member added manually without supporting evidence.
    ManualMembership,
    /// The anchor was chosen manually.
    ManualAnchor,
    /// Some member pair was never analyzed.
    UnverifiedPairs,
    /// Score of at least one member is within 0.05 of the threshold, so the
    /// group deserves a human glance (`SPEC.md` §5.4).
    NearThreshold,
}

/// Evidence that produced a group, kept separate from manual decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GroupEvidence {
    /// Mean composite score of the anchor→member pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mean_anchor_score: Option<f64>,
    /// Lowest composite score among anchor→member pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_anchor_score: Option<f64>,
    /// Non-anchor member pairs that were analyzed and found compatible.
    pub compatible_member_pairs: usize,
    /// Non-anchor member pairs that were analyzed and found incompatible.
    pub incompatible_member_pairs: usize,
    /// Non-anchor member pairs with no usable evidence.
    pub unverified_member_pairs: usize,
}

/// A comparison group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Stable group ID.
    pub id: String,
    /// Anchor photo. All alignments map into its coordinate system.
    pub anchor_photo_id: String,
    /// Members, anchor first, then by photo ID.
    pub photo_ids: Vec<String>,
    /// Alignments for every non-anchor member that has usable evidence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alignments: Vec<Alignment>,
    /// Flags worth surfacing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flags: Vec<GroupFlag>,
    /// Automatic evidence.
    pub evidence: GroupEvidence,
}

/// Why a photo joined no group. A normal outcome and a primary input to manual
/// correction (`SPEC.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum UngroupedReason {
    /// Alone in its session.
    NoSessionPeer,
    /// Session peers exist but none is focal-compatible.
    NoFocalCompatiblePeer,
    /// Every candidate pair was analyzed and rejected.
    AllPairsIncompatible,
    /// Every candidate pair ended `uncertain`.
    AllPairsUncertain,
    /// Visually compatible with a peer, but every such peer belongs to a
    /// configuration already represented in the group. See
    /// [`AnalysisParameters::one_photo_per_configuration`].
    NoDistinctConfigurationPeer,
    /// Compatible with an anchor but incompatible with a member that anchor had
    /// already accepted. This is what stops non-transitive evidence from
    /// collapsing into one group.
    InconsistentWithGroupMembers,
    /// No usable capture timestamp, so no session could be assigned.
    UnusableTimestamp,
    /// The file could not be read or decoded.
    UnreadableFile,
    /// Removed from its group by a manual override.
    ManuallyRemoved,
}

/// An input photo that is in no group.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Ungrouped {
    /// The photo.
    pub photo_id: String,
    /// Machine-readable reason.
    pub reason: UngroupedReason,
    /// Optional human-readable detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

// ---------------------------------------------------------------------------
// Manual overrides
// ---------------------------------------------------------------------------

/// A human correction to automatic grouping. Stored separately from automatic
/// evidence so the original analysis stays inspectable (`SPEC.md` §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum OverrideOp {
    /// Add a photo to a group, even without supporting evidence.
    AddPhoto {
        /// Target group.
        group_id: String,
        /// Photo to add.
        photo_id: String,
    },
    /// Remove a photo from a group.
    RemovePhoto {
        /// Target group.
        group_id: String,
        /// Photo to remove.
        photo_id: String,
    },
    /// Split the listed photos out of a group into a new one.
    SplitGroup {
        /// Group to split.
        group_id: String,
        /// Photos to move out.
        photo_ids: Vec<String>,
    },
    /// Merge several groups into the first one.
    MergeGroups {
        /// Groups to merge, target first.
        group_ids: Vec<String>,
    },
    /// Choose a different anchor.
    SetAnchor {
        /// Target group.
        group_id: String,
        /// New anchor, which must be a member.
        photo_id: String,
    },
    /// Permit more than one photo from the same configuration in a group.
    AllowDuplicateConfiguration {
        /// Target group.
        group_id: String,
    },
}

/// A recorded manual override.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ManualOverride {
    /// Stable override ID, assigned in application order.
    pub id: String,
    /// The operation.
    #[serde(flatten)]
    pub op: OverrideOp,
    /// When it was recorded, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Optional user note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

/// Application identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationInfo {
    /// Application name.
    pub name: String,
    /// Application version.
    pub version: String,
}

impl Default for ApplicationInfo {
    fn default() -> Self {
        Self {
            name: "camcomp".to_string(),
            version: APPLICATION_VERSION.to_string(),
        }
    }
}

/// Where the photos came from.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    /// Common source folder, when the inputs share one. Absent for an arbitrary
    /// file selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// When the analysis was produced, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_at: Option<String>,
}

/// Counters describing what the run actually did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunStats {
    /// Photos in the document.
    pub photos: usize,
    /// Sessions detected.
    pub sessions: usize,
    /// Candidate pairs generated.
    pub candidate_pairs: usize,
    /// Pairs visually analyzed in this run.
    pub analyzed_pairs: usize,
    /// Pair results reused from the previous document.
    pub reused_pairs: usize,
    /// Pairs found compatible.
    pub compatible_pairs: usize,
    /// Pairs found incompatible.
    pub incompatible_pairs: usize,
    /// Pairs left uncertain.
    pub uncertain_pairs: usize,
    /// Photos whose features were extracted in this run.
    pub features_extracted: usize,
    /// Groups produced.
    pub groups: usize,
    /// Photos in no group.
    pub ungrouped: usize,
    /// Total wall time.
    pub duration_ms: u64,
    /// Largest analysis image processed, in pixels. A crude peak-memory proxy.
    pub peak_analysis_pixels: u64,
    /// `true` when the run stopped early because it was cancelled.
    pub cancelled: bool,
}

/// The complete `camcomp.json` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDocument {
    /// Schema version, `major.minor`.
    pub schema_version: String,
    /// Application identity.
    pub application: ApplicationInfo,
    /// Algorithm identity and parameters.
    pub algorithm: AlgorithmInfo,
    /// Source description.
    pub source: SourceInfo,
    /// Configurations detected. Drop 4 selects from this list.
    pub configurations: Vec<Configuration>,
    /// Every input photo.
    pub photos: Vec<Photo>,
    /// Sessions.
    pub sessions: Vec<Session>,
    /// Pair results for every candidate pair.
    pub pairs: Vec<PairResult>,
    /// Groups after automatic construction and manual overrides.
    pub groups: Vec<Group>,
    /// Photos in no group, with a reason.
    pub ungrouped: Vec<Ungrouped>,
    /// Manual overrides, in application order.
    #[serde(default)]
    pub manual_overrides: Vec<ManualOverride>,
    /// Run counters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<RunStats>,
}

impl Default for AnalysisDocument {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            application: ApplicationInfo::default(),
            algorithm: AlgorithmInfo::default(),
            source: SourceInfo::default(),
            configurations: Vec::new(),
            photos: Vec::new(),
            sessions: Vec::new(),
            pairs: Vec::new(),
            groups: Vec::new(),
            ungrouped: Vec::new(),
            manual_overrides: Vec::new(),
            stats: None,
        }
    }
}

fn unique_ids<'a>(
    label: &str,
    ids: impl IntoIterator<Item = &'a str>,
) -> crate::Result<BTreeSet<&'a str>> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() {
            return Err(crate::Error::InconsistentDocument(format!(
                "{label} ID is empty"
            )));
        }
        if !seen.insert(id) {
            return Err(crate::Error::InconsistentDocument(format!(
                "duplicate {label} ID `{id}`"
            )));
        }
    }
    Ok(seen)
}

fn normalized_document_path(path: &str) -> crate::Result<String> {
    let with_slashes = path.replace('\\', "/");
    if with_slashes.is_empty()
        || with_slashes.starts_with('/')
        || with_slashes.as_bytes().get(1) == Some(&b':')
    {
        return Err(crate::Error::InconsistentDocument(format!(
            "photo path `{path}` is not source-root-relative"
        )));
    }
    let mut segments = Vec::new();
    for segment in with_slashes.split('/') {
        let normalized: String = segment.nfc().collect();
        if normalized.is_empty()
            || normalized == "."
            || normalized == ".."
            || normalized.chars().any(char::is_control)
        {
            return Err(crate::Error::InconsistentDocument(format!(
                "photo path `{path}` contains an invalid segment"
            )));
        }
        segments.push(normalized);
    }
    Ok(segments.join("/"))
}

impl AnalysisDocument {
    /// Look up a photo by ID.
    pub fn photo(&self, id: &str) -> Option<&Photo> {
        self.photos.iter().find(|p| p.id == id)
    }

    /// Look up a pair by canonical ID.
    pub fn pair(&self, id: &str) -> Option<&PairResult> {
        self.pairs.iter().find(|p| p.id == id)
    }

    /// Look up a pair by the two photo IDs, in any order.
    pub fn pair_of(&self, a: &str, b: &str) -> Option<&PairResult> {
        let id = canonical_pair_id(a, b);
        self.pair(&id)
    }

    /// Look up a group by ID.
    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    /// Every input photo appears exactly once across groups and the ungrouped
    /// list. A document that fails this is not a valid analysis
    /// (`SPEC.md` §6).
    pub fn validate(&self) -> crate::Result<()> {
        let major = schema_major(&self.schema_version).ok_or_else(|| {
            crate::Error::InconsistentDocument(format!(
                "schema version `{}` is not `major.minor`",
                self.schema_version
            ))
        })?;
        if major != SCHEMA_MAJOR {
            return Err(crate::Error::InconsistentDocument(format!(
                "schema major version {major} is not {SCHEMA_MAJOR}"
            )));
        }

        let configuration_ids = unique_ids(
            "configuration",
            self.configurations.iter().map(|item| item.id.as_str()),
        )?;
        let photo_ids = unique_ids("photo", self.photos.iter().map(|item| item.id.as_str()))?;
        let session_ids = unique_ids("session", self.sessions.iter().map(|item| item.id.as_str()))?;
        unique_ids("pair", self.pairs.iter().map(|item| item.id.as_str()))?;
        unique_ids("group", self.groups.iter().map(|item| item.id.as_str()))?;
        unique_ids(
            "manual override",
            self.manual_overrides.iter().map(|item| item.id.as_str()),
        )?;

        let mut normalized_paths = BTreeMap::new();
        for photo in &self.photos {
            if !configuration_ids.contains(photo.configuration_id.as_str()) {
                return Err(crate::Error::InconsistentDocument(format!(
                    "photo `{}` references unknown configuration `{}`",
                    photo.id, photo.configuration_id
                )));
            }
            let normalized = normalized_document_path(&photo.path)?;
            if let Some(previous) = normalized_paths.insert(normalized.clone(), photo.id.as_str()) {
                return Err(crate::Error::InconsistentDocument(format!(
                    "photos `{previous}` and `{}` have duplicate normalized path `{normalized}`",
                    photo.id
                )));
            }
        }

        let mut session_membership = BTreeMap::new();
        for session in &self.sessions {
            let mut local_members = BTreeSet::new();
            for photo_id in &session.photo_ids {
                if !photo_ids.contains(photo_id.as_str()) {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "session `{}` references unknown photo `{photo_id}`",
                        session.id
                    )));
                }
                if !local_members.insert(photo_id.as_str()) {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "session `{}` contains duplicate photo `{photo_id}`",
                        session.id
                    )));
                }
                if let Some(previous) =
                    session_membership.insert(photo_id.as_str(), session.id.as_str())
                {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "photo `{photo_id}` belongs to sessions `{previous}` and `{}`",
                        session.id
                    )));
                }
            }
        }

        for pair in &self.pairs {
            if pair.a >= pair.b {
                return Err(crate::Error::InconsistentDocument(format!(
                    "pair `{}` does not contain two canonically ordered photo IDs",
                    pair.id
                )));
            }
            let expected_id = canonical_pair_id(&pair.a, &pair.b);
            if pair.id != expected_id {
                return Err(crate::Error::InconsistentDocument(format!(
                    "pair `{}` has non-canonical ID; expected `{expected_id}`",
                    pair.id
                )));
            }
            if !photo_ids.contains(pair.a.as_str()) || !photo_ids.contains(pair.b.as_str()) {
                return Err(crate::Error::InconsistentDocument(format!(
                    "pair `{}` references an unknown photo",
                    pair.id
                )));
            }
            if !session_ids.contains(pair.session_id.as_str())
                || session_membership.get(pair.a.as_str()) != Some(&pair.session_id.as_str())
                || session_membership.get(pair.b.as_str()) != Some(&pair.session_id.as_str())
            {
                return Err(crate::Error::InconsistentDocument(format!(
                    "pair `{}` does not reference the session containing both photos",
                    pair.id
                )));
            }
            if let Some(homography) = &pair.homography_a_to_b {
                if homography.len() != 9 || Mat3::from_row_major(homography).is_none() {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "pair `{}` homography is not a finite 3x3 matrix",
                        pair.id
                    )));
                }
            }
        }

        let mut seen: BTreeMap<&str, u32> =
            self.photos.iter().map(|p| (p.id.as_str(), 0)).collect();
        for group in &self.groups {
            if group.photo_ids.len() < 2 {
                return Err(crate::Error::InconsistentDocument(format!(
                    "group `{}` has {} member(s); a group is meaningful at two",
                    group.id,
                    group.photo_ids.len()
                )));
            }
            if !group.photo_ids.contains(&group.anchor_photo_id) {
                return Err(crate::Error::InconsistentDocument(format!(
                    "group `{}` anchor `{}` is not a member",
                    group.id, group.anchor_photo_id
                )));
            }
            if group.photo_ids.first() != Some(&group.anchor_photo_id) {
                return Err(crate::Error::InconsistentDocument(format!(
                    "group `{}` anchor must be its first member",
                    group.id
                )));
            }
            let mut group_members = BTreeSet::new();
            for pid in &group.photo_ids {
                if !group_members.insert(pid.as_str()) {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "group `{}` contains duplicate photo `{pid}`",
                        group.id
                    )));
                }
                match seen.get_mut(pid.as_str()) {
                    Some(count) => *count += 1,
                    None => {
                        return Err(crate::Error::InconsistentDocument(format!(
                            "group `{}` references unknown photo `{pid}`",
                            group.id
                        )))
                    }
                }
            }
            let mut aligned_members = BTreeSet::new();
            for alignment in &group.alignments {
                if !group_members.contains(alignment.photo_id.as_str())
                    || alignment.photo_id == group.anchor_photo_id
                {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "group `{}` alignment references invalid member `{}`",
                        group.id, alignment.photo_id
                    )));
                }
                if !aligned_members.insert(alignment.photo_id.as_str()) {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "group `{}` contains duplicate alignment for `{}`",
                        group.id, alignment.photo_id
                    )));
                }
                if alignment.homography.len() != 9 || alignment.inverse_homography.len() != 9 {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "group `{}` alignment for `{}` is not a 3x3 matrix",
                        group.id, alignment.photo_id
                    )));
                }
                let h = Mat3::from_row_major(&alignment.homography).ok_or_else(|| {
                    crate::Error::InconsistentDocument(format!(
                        "group `{}` alignment for `{}` is not finite",
                        group.id, alignment.photo_id
                    ))
                })?;
                let inv = Mat3::from_row_major(&alignment.inverse_homography).ok_or_else(|| {
                    crate::Error::InconsistentDocument(format!(
                        "group `{}` inverse alignment for `{}` is not finite",
                        group.id, alignment.photo_id
                    ))
                })?;
                // The persisted inverse must actually invert, otherwise a client
                // that trusts it silently mismaps every pointer.
                let probe = [0.37, 0.61];
                let round_trip = h.apply(probe).and_then(|p| inv.apply(p)).ok_or_else(|| {
                    crate::Error::InconsistentDocument(format!(
                        "group `{}` alignment for `{}` maps to infinity",
                        group.id, alignment.photo_id
                    ))
                })?;
                let err = (round_trip[0] - probe[0]).hypot(round_trip[1] - probe[1]);
                if err > 1e-6 {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "group `{}` inverse alignment for `{}` does not invert (residual {err:.3e})",
                        group.id, alignment.photo_id
                    )));
                }
            }
        }
        for ungrouped in &self.ungrouped {
            match seen.get_mut(ungrouped.photo_id.as_str()) {
                Some(count) => *count += 1,
                None => {
                    return Err(crate::Error::InconsistentDocument(format!(
                        "ungrouped list references unknown photo `{}`",
                        ungrouped.photo_id
                    )))
                }
            }
        }
        let mut missing: Vec<&str> = Vec::new();
        let mut duplicated: Vec<&str> = Vec::new();
        for (id, count) in &seen {
            match count {
                0 => missing.push(id),
                1 => {}
                _ => duplicated.push(id),
            }
        }
        if !missing.is_empty() {
            return Err(crate::Error::InconsistentDocument(format!(
                "{} photo(s) appear in neither a group nor the ungrouped list, first `{}`",
                missing.len(),
                missing[0]
            )));
        }
        if !duplicated.is_empty() {
            return Err(crate::Error::InconsistentDocument(format!(
                "{} photo(s) appear more than once, first `{}`",
                duplicated.len(),
                duplicated[0]
            )));
        }
        Ok(())
    }
}

/// Parse the major component of a `major.minor` version string.
pub fn schema_major(version: &str) -> Option<u32> {
    version.split('.').next()?.parse().ok()
}

/// Canonical pair ID: order-independent, so a pair is analyzed once.
pub fn canonical_pair_id(a: &str, b: &str) -> String {
    if a <= b {
        format!("pair-{a}-{b}")
    } else {
        format!("pair-{b}-{a}")
    }
}

/// The JSON Schema for [`AnalysisDocument`], pretty-printed.
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(AnalysisDocument);
    serde_json::to_string_pretty(&schema).expect("schema serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_document() -> AnalysisDocument {
        let mut doc = AnalysisDocument::default();
        doc.configurations.push(Configuration {
            id: "configuration-1".into(),
            device_key: "device-1".into(),
            make: None,
            model: None,
            mode_label: None,
            display_name: "Test camera".into(),
            photo_count: 1,
        });
        doc.photos.push(Photo {
            id: "photo-aaaa".into(),
            path: "a.jpg".into(),
            configuration_id: "configuration-1".into(),
            fingerprint: Fingerprint {
                size: 1,
                modified_at: None,
                sha256: "0".repeat(64),
            },
            dimensions: Dimensions {
                original: [4, 3],
                oriented: [4, 3],
                analysis: None,
            },
            metadata: PhotoMetadata {
                captured_at: None,
                capture_time_source: TimestampSource::Missing,
                timezone_assumed: false,
                corrected_at: None,
                applied_time_offset_seconds: 0,
                exif_orientation: 1,
                orientation_applied_by_decoder: false,
                camera_make: None,
                camera_model: None,
                lens_model: None,
                focal_length_mm: None,
                focal_length_35mm: None,
                effective_focal_length_35mm: None,
                focal_source: FocalSource::Unknown,
                digital_zoom_ratio: None,
                native_lens: true,
                mode_tag_raw: None,
                mode_tag_source: None,
                mode_label: None,
                mode_flags: vec![],
                mode_tag_unknown_bits: None,
            },
            features: None,
            notes: vec![],
        });
        doc.ungrouped.push(Ungrouped {
            photo_id: "photo-aaaa".into(),
            reason: UngroupedReason::UnusableTimestamp,
            detail: None,
        });
        doc
    }

    #[test]
    fn document_round_trips_without_semantic_change() {
        let doc = minimal_document();
        let json = serde_json::to_string_pretty(&doc).unwrap();
        let back: AnalysisDocument = serde_json::from_str(&json).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let doc = minimal_document();
        let mut value: serde_json::Value = serde_json::to_value(&doc).unwrap();
        value["somethingFromTheFuture"] = serde_json::json!({ "nested": [1, 2, 3] });
        value["photos"][0]["futurePhotoField"] = serde_json::json!(true);
        let back: AnalysisDocument = serde_json::from_value(value).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn schema_major_mismatch_is_rejected() {
        let mut doc = minimal_document();
        doc.schema_version = "2.0".into();
        assert!(doc.validate().is_err());
        doc.schema_version = "not-a-version".into();
        assert!(doc.validate().is_err());
    }

    #[test]
    fn every_photo_must_appear_exactly_once() {
        let mut doc = minimal_document();
        doc.ungrouped.clear();
        let err = doc.validate().unwrap_err().to_string();
        assert!(
            err.contains("neither a group nor the ungrouped list"),
            "{err}"
        );

        let mut doc = minimal_document();
        doc.ungrouped.push(doc.ungrouped[0].clone());
        let err = doc.validate().unwrap_err().to_string();
        assert!(err.contains("more than once"), "{err}");
    }

    #[test]
    fn a_group_of_one_is_rejected() {
        let mut doc = minimal_document();
        doc.ungrouped.clear();
        doc.groups.push(Group {
            id: "group-1".into(),
            anchor_photo_id: "photo-aaaa".into(),
            photo_ids: vec!["photo-aaaa".into()],
            alignments: vec![],
            flags: vec![],
            evidence: GroupEvidence {
                mean_anchor_score: None,
                min_anchor_score: None,
                compatible_member_pairs: 0,
                incompatible_member_pairs: 0,
                unverified_member_pairs: 0,
            },
        });
        let err = doc.validate().unwrap_err().to_string();
        assert!(err.contains("meaningful at two"), "{err}");
    }

    #[test]
    fn a_persisted_inverse_that_does_not_invert_is_rejected() {
        let mut doc = minimal_document();
        doc.photos.push({
            let mut p = doc.photos[0].clone();
            p.id = "photo-bbbb".into();
            p.path = "b.jpg".into();
            p
        });
        doc.ungrouped.clear();
        doc.groups.push(Group {
            id: "group-1".into(),
            anchor_photo_id: "photo-aaaa".into(),
            photo_ids: vec!["photo-aaaa".into(), "photo-bbbb".into()],
            alignments: vec![Alignment {
                photo_id: "photo-bbbb".into(),
                coordinate_system: CoordinateSystem::NormalizedOriented,
                direction: TransformDirection::PhotoToAnchor,
                homography: vec![1.0, 0.0, 0.1, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                // Wrong on purpose: the inverse of a +0.1 shift is -0.1.
                inverse_homography: vec![1.0, 0.0, 0.1, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                correspondences: vec![],
                confidence: 0.9,
                provenance: AlignmentProvenance::DirectPair,
            }],
            flags: vec![],
            evidence: GroupEvidence {
                mean_anchor_score: None,
                min_anchor_score: None,
                compatible_member_pairs: 1,
                incompatible_member_pairs: 0,
                unverified_member_pairs: 0,
            },
        });
        let err = doc.validate().unwrap_err().to_string();
        assert!(err.contains("does not invert"), "{err}");

        // The correct inverse passes.
        doc.groups[0].alignments[0].inverse_homography =
            vec![1.0, 0.0, -0.1, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        doc.validate().unwrap();
    }

    #[test]
    fn duplicate_ids_and_normalized_paths_are_rejected() {
        let mut document = minimal_document();
        let mut duplicate = document.photos[0].clone();
        duplicate.path = "other.jpg".into();
        document.photos.push(duplicate);
        assert!(document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate photo ID"));

        let mut document = minimal_document();
        let mut duplicate = document.photos[0].clone();
        duplicate.id = "photo-bbbb".into();
        duplicate.path = "cafe\u{301}.jpg".into();
        document.photos[0].path = "café.jpg".into();
        document.photos.push(duplicate);
        let error = document.validate().unwrap_err().to_string();
        assert!(error.contains("duplicate normalized path"), "{error}");
    }

    #[test]
    fn invalid_cross_references_and_group_order_are_rejected() {
        let mut document = minimal_document();
        document.photos[0].configuration_id = "missing".into();
        assert!(document
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown configuration"));

        let mut document = minimal_document();
        document.photos.push({
            let mut photo = document.photos[0].clone();
            photo.id = "photo-bbbb".into();
            photo.path = "b.jpg".into();
            photo
        });
        document.ungrouped.clear();
        document.groups.push(Group {
            id: "group-1".into(),
            anchor_photo_id: "photo-aaaa".into(),
            photo_ids: vec!["photo-bbbb".into(), "photo-aaaa".into()],
            alignments: vec![],
            flags: vec![],
            evidence: GroupEvidence {
                mean_anchor_score: None,
                min_anchor_score: None,
                compatible_member_pairs: 0,
                incompatible_member_pairs: 0,
                unverified_member_pairs: 1,
            },
        });
        let error = document.validate().unwrap_err().to_string();
        assert!(error.contains("anchor must be its first member"), "{error}");
    }

    #[test]
    fn pair_ids_are_order_independent() {
        assert_eq!(
            canonical_pair_id("photo-b", "photo-a"),
            canonical_pair_id("photo-a", "photo-b")
        );
        assert_eq!(canonical_pair_id("a", "b"), "pair-a-b");
    }

    #[test]
    fn weights_sum_to_one_by_default() {
        assert!((MetricWeights::default().sum() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn pair_cache_key_changes_with_a_threshold_change() {
        let base = AlgorithmInfo::default();
        let mut changed = base.clone();
        changed.parameters.compatibility_threshold = 0.7;
        assert_ne!(base.pair_cache_key(), changed.pair_cache_key());

        // A parameter that only affects output volume must not invalidate pairs.
        let mut cosmetic = base.clone();
        cosmetic.parameters.max_correspondences = 50;
        assert_eq!(base.pair_cache_key(), cosmetic.pair_cache_key());
    }

    #[test]
    fn generated_schema_is_a_json_schema_object() {
        let schema: serde_json::Value = serde_json::from_str(&json_schema()).unwrap();
        assert!(schema.get("$schema").is_some());
        assert!(schema["properties"]["schemaVersion"].is_object());
        assert!(schema["properties"]["groups"].is_object());
    }

    #[test]
    fn override_ops_are_externally_tagged_by_op() {
        let json = serde_json::to_string(&ManualOverride {
            id: "override-1".into(),
            op: OverrideOp::SetAnchor {
                group_id: "group-1".into(),
                photo_id: "photo-aaaa".into(),
            },
            created_at: None,
            note: None,
        })
        .unwrap();
        assert!(json.contains("\"op\":\"set-anchor\""), "{json}");
        let back: ManualOverride = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.op, OverrideOp::SetAnchor { .. }));
    }
}
