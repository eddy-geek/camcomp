/**
 * Something worth surfacing about a group.
 */
export type GroupFlag =
  | "internal-disagreement"
  | "duplicate-configuration"
  | "manual-membership"
  | "manual-anchor"
  | "unverified-pairs"
  | "near-threshold";
/**
 * A recorded manual override.
 */
export type ManualOverride = {
  /**
   * When it was recorded, RFC 3339.
   */
  createdAt?: string | null;
  /**
   * Stable override ID, assigned in application order.
   */
  id: string;
  /**
   * Optional user note.
   */
  note?: string | null;
  [k: string]: unknown;
} & ManualOverride1;
export type ManualOverride1 =
  | {
      /**
       * Target group.
       */
      group_id: string;
      op: "add-photo";
      /**
       * Photo to add.
       */
      photo_id: string;
      [k: string]: unknown;
    }
  | {
      /**
       * Target group.
       */
      group_id: string;
      op: "remove-photo";
      /**
       * Photo to remove.
       */
      photo_id: string;
      [k: string]: unknown;
    }
  | {
      /**
       * Group to split.
       */
      group_id: string;
      op: "split-group";
      /**
       * Photos to move out.
       */
      photo_ids: string[];
      [k: string]: unknown;
    }
  | {
      /**
       * Groups to merge, target first.
       */
      group_ids: string[];
      op: "merge-groups";
      [k: string]: unknown;
    }
  | {
      /**
       * Target group.
       */
      group_id: string;
      op: "set-anchor";
      /**
       * New anchor, which must be a member.
       */
      photo_id: string;
      [k: string]: unknown;
    }
  | {
      /**
       * Target group.
       */
      group_id: string;
      op: "allow-duplicate-configuration";
      [k: string]: unknown;
    };
/**
 * Which hard gate failed. Every rejection names its gate (`PLAN-DROP1.md` §12.3).
 */
export type GateFailure =
  | "inliers"
  | "reprojection-error"
  | "spatial-distribution-a"
  | "spatial-distribution-b"
  | "geometric-overlap"
  | "degenerate"
  | "reflection"
  | "scale"
  | "perspective";
/**
 * Why a pair ended up `uncertain`, or why it was not analyzed.
 */
export type PairReason =
  | "low-texture"
  | "insufficient-matches"
  | "no-consensus"
  | "gate-failed"
  | "score-below-threshold"
  | "focal-mismatch"
  | "unreadable";

/**
 * The complete `camcomp.json` document.
 */
export interface AnalysisDocument {
  algorithm: AlgorithmInfo;
  application: ApplicationInfo;
  /**
   * Configurations detected. Drop 4 selects from this list.
   */
  configurations: Configuration[];
  /**
   * Groups after automatic construction and manual overrides.
   */
  groups: Group[];
  /**
   * Manual overrides, in application order.
   */
  manualOverrides?: ManualOverride[];
  /**
   * Pair results for every candidate pair.
   */
  pairs: PairResult[];
  /**
   * Every input photo.
   */
  photos: Photo[];
  /**
   * Schema version, `major.minor`.
   */
  schemaVersion: string;
  /**
   * Sessions.
   */
  sessions: Session[];
  source: SourceInfo;
  /**
   * Run counters.
   */
  stats?: RunStats | null;
  /**
   * Photos in no group, with a reason.
   */
  ungrouped: Ungrouped[];
  [k: string]: unknown;
}
/**
 * Algorithm identity and parameters.
 */
export interface AlgorithmInfo {
  gates: HardGates;
  /**
   * Stable algorithm name.
   */
  name: string;
  parameters: AnalysisParameters;
  /**
   * `true` while the thresholds are still derived from a small labelled set
   * (`SPEC.md` §9). Honest defaults beat confident ones.
   */
  thresholdsProvisional: boolean;
  /**
   * Algorithm version. A change invalidates cached pair results.
   */
  version: string;
  weights: MetricWeights;
  [k: string]: unknown;
}
/**
 * Hard gates in force for this document.
 */
export interface HardGates {
  /**
   * Side of the square occupancy grid.
   */
  gridSize: number;
  /**
   * Maximum median reprojection error, in analysis pixels.
   */
  maxMedianReprojectionErrorPx: number;
  /**
   * Maximum projective row magnitude in normalized coordinates.
   */
  maxPerspective: number;
  /**
   * Maximum affine scale ratio of the fitted transform.
   */
  maxScale: number;
  /**
   * Minimum geometric overlap.
   */
  minGeometricOverlap: number;
  /**
   * Minimum RANSAC inlier count.
   */
  minInliers: number;
  /**
   * Minimum occupied cells, required in *both* images.
   */
  minOccupiedCells: number;
  /**
   * Minimum affine scale ratio of the fitted transform.
   */
  minScale: number;
}
/**
 * Tunables in force for this document.
 */
export interface AnalysisParameters {
  /**
   * Longest edge of the analysis image, in pixels.
   */
  analysisLongestEdge: number;
  /**
   * Composite score a pair must reach once every hard gate passes.
   */
  compatibilityThreshold: number;
  /**
   * Per-device capture-time corrections, in seconds, keyed by device key.
   */
  deviceTimeOffsetSeconds?: {
    [k: string]: number;
  };
  /**
   * Maximum relative difference of *diagonal angle of view*, derived from the
   * 35 mm equivalent focal lengths (`SPEC.md` §4.2).
   *
   * Not a focal ratio: angle of view is `2·atan(d / 2f)`, so a constant
   * angular tolerance is more permissive at the wide end and stricter at the
   * long end than a constant focal ratio. `0.15` here is within the valid
   * window measured over the labelled sets: the widest pair that must be
   * accepted under this tier now measures 0.147 (`tstgrp3` g1 and `tstgrp5`
   * g1, after `tstgrp5` was relabelled into a single group) and the closest
   * that must be rejected measures 0.750 (`IMPLEM-DROP1.md` §6.11, revised).
   */
  fieldOfViewTolerance: number;
  /**
   * Same tolerance, applied when *both* photos were taken at a native lens
   * step — no digital zoom or sensor crop on either side.
   *
   * Two phones' ultrawide cameras differ in framing as a matter of hardware
   * (measured: 15 mm against 12 mm equivalent, 10 % of angle of view), yet
   * "both ultrawides" is exactly the comparison the product exists for. A
   * deliberate zoom step by the photographer is a different intent and stays
   * under the strict tolerance.
   */
  fieldOfViewToleranceNativeLens?: number;
  /**
   * Whether `FocalLengthIn35mmFilm` is taken at face value or combined with
   * `DigitalZoomRatio` (`IMPLEM-DROP1.md` §6.7).
   */
  focalZoomPolicy?: "trust-exif" | "apply-digital-zoom" | "per-device";
  /**
   * Lowe ratio-test threshold.
   */
  loweRatio: number;
  /**
   * Upper bound on persisted correspondences per aligned photo.
   */
  maxCorrespondences: number;
  /**
   * Upper bound on SIFT keypoints retained per photo.
   */
  maxFeaturesPerPhoto: number;
  /**
   * Guard on decoded pixel count, per photo.
   */
  maxMegapixels: number;
  /**
   * Below this keypoint count a photo cannot produce a decision, only
   * `uncertain` (`SPEC.md` §5.2).
   */
  minKeypointsForDecision: number;
  /**
   * Below this ratio-filtered match count a pair stays `uncertain`.
   */
  minMatchesForDecision: number;
  /**
   * Require forward and reverse nearest neighbours to agree.
   */
  mutualMatch: boolean;
  /**
   * Keep at most one photo per configuration in a group (`SPEC.md` §6).
   *
   * On by default, because the product compares configurations against each
   * other. Turning it off lets two shots from the same device and mode group
   * together, which some labelled sets expect.
   */
  onePhotoPerConfiguration?: boolean;
  /**
   * RANSAC iteration cap.
   */
  ransacMaxIterations: number;
  /**
   * RANSAC seed. Fixed, so a rerun reproduces the same inlier set.
   */
  ransacSeed: number;
  /**
   * RANSAC inlier threshold, in analysis pixels.
   */
  ransacThresholdPx: number;
  /**
   * Sliding gap that separates two shooting sessions (`SPEC.md` §4.1).
   *
   * Set to [`u32::MAX`] to disable session detection entirely, which puts
   * every photo in one session. That is a calibration instrument, not a
   * product default: it removes the cheapest prefilter and makes the visual
   * half decide every pair.
   */
  sessionGapSeconds: number;
}
/**
 * Score weights in force for this document.
 */
export interface MetricWeights {
  /**
   * Weight of [`PairMetrics::aligned_appearance_similarity`].
   */
  alignedAppearanceSimilarity: number;
  /**
   * Weight of [`PairMetrics::feature_coverage`].
   */
  featureCoverage: number;
  /**
   * Weight of [`PairMetrics::geometric_overlap`].
   */
  geometricOverlap: number;
  /**
   * Weight of [`PairMetrics::inlier_confidence`].
   */
  inlierConfidence: number;
}
/**
 * Application identity.
 */
export interface ApplicationInfo {
  /**
   * Application name.
   */
  name: string;
  /**
   * Application version.
   */
  version: string;
  [k: string]: unknown;
}
/**
 * A device + shooting-mode combination (`SPEC.md` §2).
 */
export interface Configuration {
  /**
   * Device identity without the mode, for per-device clock offsets and
   * per-device scoring.
   */
  deviceKey: string;
  /**
   * Human-readable label for the UI.
   */
  displayName: string;
  /**
   * Stable configuration ID.
   */
  id: string;
  /**
   * EXIF `Make`, when known.
   */
  make?: string | null;
  /**
   * Normalized mode label, when known.
   */
  modeLabel?: string | null;
  /**
   * EXIF `Model`, when known.
   */
  model?: string | null;
  /**
   * Photos assigned to this configuration.
   */
  photoCount: number;
  [k: string]: unknown;
}
/**
 * A comparison group.
 */
export interface Group {
  /**
   * Alignments for every non-anchor member that has usable evidence.
   */
  alignments?: Alignment[];
  /**
   * Anchor photo. All alignments map into its coordinate system.
   */
  anchorPhotoId: string;
  evidence: GroupEvidence;
  /**
   * Flags worth surfacing.
   */
  flags?: GroupFlag[];
  /**
   * Stable group ID.
   */
  id: string;
  /**
   * Members, anchor first, then by photo ID.
   */
  photoIds: string[];
  [k: string]: unknown;
}
/**
 * Group-relative alignment for one non-anchor member (`SPEC.md` §7).
 */
export interface Alignment {
  /**
   * Alignment confidence in `[0, 1]`, surfaced by the viewer so it can fall
   * back gracefully.
   */
  confidence: number;
  /**
   * Coordinate system of [`Self::homography`].
   */
  coordinateSystem: "normalized-oriented";
  /**
   * Bounded, spatially distributed correspondences.
   */
  correspondences?: Correspondence[];
  /**
   * Direction of [`Self::homography`].
   */
  direction: "photo-to-anchor";
  /**
   * Row-major `3 × 3` homography, photo to anchor.
   */
  homography: number[];
  /**
   * Row-major inverse, anchor to photo. Persisted so clients need no linear
   * algebra; validated on load.
   */
  inverseHomography: number[];
  /**
   * Member this alignment describes.
   */
  photoId: string;
  /**
   * How this alignment was derived.
   */
  provenance: "direct-pair" | "composed" | "manual-unverified";
  [k: string]: unknown;
}
/**
 * A single inlier correspondence, in normalized coordinates.
 *
 * The `photo` / `anchor` naming follows the persisted direction: within a
 * [`PairResult`] the fields hold `a` and `b`; within an [`Alignment`] they hold
 * the member photo and the group anchor.
 */
export interface Correspondence {
  /**
   * Normalized point in the target (anchor) photo.
   *
   * @minItems 2
   * @maxItems 2
   */
  anchor: [number, number];
  /**
   * Local feature orientation in the source photo, in degrees.
   */
  angle?: number | null;
  /**
   * Descriptor-match quality in `[0, 1]`, higher is better.
   */
  matchQuality: number;
  /**
   * Normalized point in the source photo.
   *
   * @minItems 2
   * @maxItems 2
   */
  photo: [number, number];
  /**
   * Reprojection error in normalized anchor units.
   */
  reprojectionErrorNormalized: number;
  /**
   * Local feature scale in the source photo, normalized by its longest edge.
   */
  scale?: number | null;
  [k: string]: unknown;
}
/**
 * Automatic evidence.
 */
export interface GroupEvidence {
  /**
   * Non-anchor member pairs that were analyzed and found compatible.
   */
  compatibleMemberPairs: number;
  /**
   * Non-anchor member pairs that were analyzed and found incompatible.
   */
  incompatibleMemberPairs: number;
  /**
   * Mean composite score of the anchor→member pairs.
   */
  meanAnchorScore?: number | null;
  /**
   * Lowest composite score among anchor→member pairs.
   */
  minAnchorScore?: number | null;
  /**
   * Non-anchor member pairs with no usable evidence.
   */
  unverifiedMemberPairs: number;
  [k: string]: unknown;
}
/**
 * One analyzed or skipped candidate pair.
 */
export interface PairResult {
  /**
   * First photo ID, the lexicographically smaller of the two.
   */
  a: string;
  /**
   * Second photo ID.
   */
  b: string;
  /**
   * Bounded, spatially sampled inlier correspondences in normalized
   * coordinates, `a` to `b`. Retained only for compatible pairs, which are
   * the only ones that can form a group (`SPEC.md` §7.2).
   */
  correspondences?: Correspondence[];
  focal: FocalComparison;
  /**
   * Gates that failed.
   */
  gateFailures?: GateFailure[];
  /**
   * Whether every hard gate passed.
   */
  hardGatesPassed: boolean;
  /**
   * Row-major `3 × 3` homography mapping normalized oriented coordinates of
   * `a` into those of `b`. Present whenever a transform was fitted, even if
   * the pair was rejected, so a rejection stays inspectable.
   */
  homographyAToB?: number[] | null;
  /**
   * Canonical pair ID, `pair-<a>-<b>` with `a < b` by photo ID.
   */
  id: string;
  /**
   * Metrics, when the pair was visually analyzed.
   */
  metrics?: PairMetrics | null;
  /**
   * Why, when the verdict is not `compatible`.
   */
  reason?: PairReason | null;
  /**
   * Composite score, when it could be computed.
   */
  score?: number | null;
  /**
   * Session both photos belong to.
   */
  sessionId: string;
  /**
   * Verdict.
   */
  state: "compatible" | "incompatible" | "uncertain";
  /**
   * Absolute capture-time distance, in seconds.
   */
  timeDistanceSeconds: number;
  [k: string]: unknown;
}
/**
 * Focal comparison.
 */
export interface FocalComparison {
  /**
   * The tolerance this comparison was judged against. Two tiers exist, so
   * the verdict is not explainable from the difference alone.
   */
  appliedTolerance?: number | null;
  /**
   * `true` when both photos were at a native lens step and the relaxed
   * tolerance applied.
   */
  nativeLensPair?: boolean;
  /**
   * Relative difference of effective 35 mm equivalent focal length. Recorded
   * for explanation only; it is not what the predicate compares.
   */
  relativeFocalDifference?: number | null;
  /**
   * Relative difference of diagonal angle of view. **This is the value the
   * verdict is based on.**
   */
  relativeFovDifference?: number | null;
  /**
   * Verdict.
   */
  state: "compatible" | "incompatible" | "unknown";
  [k: string]: unknown;
}
/**
 * Normalized components and raw diagnostics for one pair (`SPEC.md` §5.3).
 */
export interface PairMetrics {
  /**
   * Affine scale of the fitted transform (largest singular value).
   */
  affineScale: number;
  /**
   * Normalized component: gradient similarity over the registered overlap.
   */
  alignedAppearanceSimilarity: number;
  /**
   * Pair analysis wall time.
   */
  durationMs: number;
  /**
   * Normalized component: spatial extent and grid spread of inliers.
   */
  featureCoverage: number;
  /**
   * Matches surviving the ratio test (and mutual check when enabled).
   */
  filteredMatches: number;
  /**
   * Normalized component: intersection over the smaller area, in the anchor
   * frame.
   */
  geometricOverlap: number;
  /**
   * Normalized component: count, ratio and reprojection quality combined.
   */
  inlierConfidence: number;
  /**
   * `inliers / filtered_matches`.
   */
  inlierRatio: number;
  /**
   * RANSAC inliers.
   */
  inliers: number;
  /**
   * Keypoints in the first photo.
   */
  keypointsA: number;
  /**
   * Keypoints in the second photo.
   */
  keypointsB: number;
  /**
   * Median reprojection error, in analysis pixels.
   */
  medianReprojectionErrorPx: number;
  /**
   * Occupied grid cells in the first photo.
   */
  occupiedCellsA: number;
  /**
   * Occupied grid cells in the second photo.
   */
  occupiedCellsB: number;
  /**
   * Post-registration SSIM. Diagnostic only; never decides compatibility.
   */
  registeredSsim?: number | null;
  [k: string]: unknown;
}
/**
 * One input photo.
 */
export interface Photo {
  /**
   * Configuration (device + mode) this photo belongs to.
   */
  configurationId: string;
  dimensions: Dimensions;
  /**
   * Feature-extraction diagnostics, when the photo was analyzed.
   */
  features?: FeatureSummary | null;
  fingerprint: Fingerprint;
  /**
   * Stable analysis ID, derived from the path relative to the source root.
   */
  id: string;
  metadata: PhotoMetadata;
  /**
   * Non-fatal observations, for example an unreadable EXIF block.
   */
  notes?: string[];
  /**
   * Path relative to the source root, using `/` separators.
   */
  path: string;
  [k: string]: unknown;
}
/**
 * Original, oriented and analysis dimensions.
 */
export interface Dimensions {
  /**
   * Dimensions of the reduced analysis image, when one was produced.
   *
   * @minItems 2
   * @maxItems 2
   */
  analysis?: [number, number] | null;
  /**
   * Dimensions after applying orientation.
   *
   * @minItems 2
   * @maxItems 2
   */
  oriented: [number, number];
  /**
   * Dimensions as stored in the file.
   *
   * @minItems 2
   * @maxItems 2
   */
  original: [number, number];
  [k: string]: unknown;
}
/**
 * Diagnostics from feature extraction.
 */
export interface FeatureSummary {
  /**
   * Extraction wall time.
   */
  durationMs: number;
  /**
   * Keypoints retained.
   */
  keypoints: number;
  /**
   * `true` when the count fell below
   * [`AnalysisParameters::min_keypoints_for_decision`].
   */
  lowTexture: boolean;
  [k: string]: unknown;
}
/**
 * Content identity and change-detection fields.
 */
export interface Fingerprint {
  /**
   * Modification time, RFC 3339. `None` when the filesystem does not report
   * one.
   */
  modifiedAt?: string | null;
  /**
   * SHA-256 of the file contents. The only field that is part of cache
   * *identity*.
   */
  sha256: string;
  /**
   * File size in bytes.
   */
  size: number;
  [k: string]: unknown;
}
/**
 * Normalized metadata.
 */
export interface PhotoMetadata {
  /**
   * Per-device offset applied, in seconds.
   */
  appliedTimeOffsetSeconds: number;
  /**
   * EXIF `Make`.
   */
  cameraMake?: string | null;
  /**
   * EXIF `Model`.
   */
  cameraModel?: string | null;
  /**
   * Where [`Self::captured_at`] came from.
   */
  captureTimeSource:
    | "exif-date-time-original"
    | "exif-create-date"
    | "exif-modify-date"
    | "file-modified"
    | "missing";
  /**
   * Capture instant as recorded, RFC 3339.
   */
  capturedAt?: string | null;
  /**
   * Capture instant after the per-device offset, RFC 3339. This is what
   * session detection uses.
   */
  correctedAt?: string | null;
  /**
   * Digital zoom ratio, when reported.
   */
  digitalZoomRatio?: number | null;
  /**
   * The equivalent focal length actually compared, after the
   * [`FocalZoomPolicy`]. Equal to [`Self::focal_length_35mm`] unless a zoom
   * was applied.
   */
  effectiveFocalLength35mm?: number | null;
  /**
   * EXIF orientation as found. `1` when absent or invalid.
   */
  exifOrientation: number;
  /**
   * 35 mm equivalent focal length exactly as reported, in millimetres.
   */
  focalLength35mm?: number | null;
  /**
   * Physical focal length, in millimetres.
   */
  focalLengthMm?: number | null;
  /**
   * Where the equivalent focal length came from.
   */
  focalSource: "exif35mm" | "exif35mm-zoom-applied" | "derived-from-crop-factor" | "unknown";
  /**
   * EXIF `LensModel`.
   */
  lensModel?: string | null;
  /**
   * Capture flags that are *not* part of the configuration identity, for
   * example `portrait` or `burst`.
   */
  modeFlags?: string[];
  /**
   * Normalized mode label, for example `auto`, `master` or `hi-res`.
   */
  modeLabel?: string | null;
  /**
   * Raw value of the tag that identifies the shooting mode.
   */
  modeTagRaw?: string | null;
  /**
   * Which EXIF tag the mode was read from.
   */
  modeTagSource?: string | null;
  /**
   * Bits of the mode tag that this version does not interpret, as hex.
   *
   * Present means "this device said something we have not verified". The bits
   * fork the configuration rather than being folded into a known mode,
   * because silently merging two shooting modes corrupts Drop 4 scoring
   * while an odd configuration name is visible and correctable.
   */
  modeTagUnknownBits?: string | null;
  /**
   * `true` when this photo used a lens at its native field of view — no
   * digital zoom and no sensor crop. Photos that are both native compare
   * under [`AnalysisParameters::field_of_view_tolerance_native_lens`].
   */
  nativeLens?: boolean;
  /**
   * `true` when the decoder already delivered oriented pixels, so the EXIF
   * orientation must *not* be applied again (HEIF container transforms).
   */
  orientationAppliedByDecoder: boolean;
  /**
   * `true` when no timezone was available and one was assumed.
   */
  timezoneAssumed: boolean;
  [k: string]: unknown;
}
/**
 * A chronological run of photos with no gap longer than the session gap.
 */
export interface Session {
  /**
   * Last corrected capture time in the session, RFC 3339.
   */
  end: string;
  /**
   * Stable session ID.
   */
  id: string;
  /**
   * Member photo IDs, in corrected-capture-time order.
   */
  photoIds: string[];
  /**
   * First corrected capture time in the session, RFC 3339.
   */
  start: string;
  [k: string]: unknown;
}
/**
 * Source description.
 */
export interface SourceInfo {
  /**
   * When the analysis was produced, RFC 3339.
   */
  generatedAt?: string | null;
  /**
   * Common source folder, when the inputs share one. Absent for an arbitrary
   * file selection.
   */
  root?: string | null;
  [k: string]: unknown;
}
/**
 * Counters describing what the run actually did.
 */
export interface RunStats {
  /**
   * Pairs visually analyzed in this run.
   */
  analyzedPairs: number;
  /**
   * `true` when the run stopped early because it was cancelled.
   */
  cancelled: boolean;
  /**
   * Candidate pairs generated.
   */
  candidatePairs: number;
  /**
   * Pairs found compatible.
   */
  compatiblePairs: number;
  /**
   * Total wall time.
   */
  durationMs: number;
  /**
   * Photos whose features were extracted in this run.
   */
  featuresExtracted: number;
  /**
   * Groups produced.
   */
  groups: number;
  /**
   * Pairs found incompatible.
   */
  incompatiblePairs: number;
  /**
   * Largest analysis image processed, in pixels. A crude peak-memory proxy.
   */
  peakAnalysisPixels: number;
  /**
   * Photos in the document.
   */
  photos: number;
  /**
   * Pair results reused from the previous document.
   */
  reusedPairs: number;
  /**
   * Sessions detected.
   */
  sessions: number;
  /**
   * Pairs left uncertain.
   */
  uncertainPairs: number;
  /**
   * Photos in no group.
   */
  ungrouped: number;
  [k: string]: unknown;
}
/**
 * An input photo that is in no group.
 */
export interface Ungrouped {
  /**
   * Optional human-readable detail.
   */
  detail?: string | null;
  /**
   * The photo.
   */
  photoId: string;
  /**
   * Machine-readable reason.
   */
  reason:
    | "no-session-peer"
    | "no-focal-compatible-peer"
    | "all-pairs-incompatible"
    | "all-pairs-uncertain"
    | "no-distinct-configuration-peer"
    | "inconsistent-with-group-members"
    | "unusable-timestamp"
    | "unreadable-file"
    | "manually-removed";
  [k: string]: unknown;
}
