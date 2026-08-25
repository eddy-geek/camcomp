//! Normalized metric components, the composite score, and the end-to-end
//! pairwise analysis that produces a [`PairState`].
//!
//! Two rules shape this module. Geometry decides first: a pair that fails a hard
//! gate is rejected regardless of its score (`SPEC.md` §5.4). And weak evidence
//! produces `Uncertain`, never a decision — a false negative costs one manual
//! action, a false positive silently corrupts a comparison (`SPEC.md` §9).

use crate::domain::{
    DescriptorMatcher, GrayImage, HomographyEstimator, ImageWarper, PhotoFeatures, Registration,
};
use crate::geometry::{pixel_to_normalized, Mat3};
use crate::matching::BruteForceMatcher;
use crate::registration::{
    evaluate_gates, hull_area_fraction, median, occupied_cells, overlap_or_zero, GateInputs,
    RansacHomographyEstimator,
};
use crate::schema::{
    AlgorithmInfo, Correspondence, GateFailure, MetricWeights, PairMetrics, PairReason, PairState,
};
use crate::warp::{
    erode_mask, gradient_magnitude, intersect_masks, masked_ssim, masked_zncc, BilinearWarper,
};

/// Occupancy fraction at which the coverage component saturates.
///
/// Real matched pairs rarely fill every cell of a `4 × 4` grid; requiring 16/16
/// for a full score would compress the component's dynamic range to the point
/// where it stops discriminating (`SPEC.md` §5.4.1). Twelve cells is treated as
/// complete coverage. The value is part of the algorithm version.
const COVERAGE_GRID_SATURATION: f64 = 0.75;
/// Convex-hull area fraction at which the coverage component saturates.
const COVERAGE_HULL_SATURATION: f64 = 0.50;
/// Inlier count at which the confidence component saturates.
const CONFIDENCE_INLIER_SATURATION: f64 = 100.0;
/// Inlier ratio at which the confidence component saturates.
const CONFIDENCE_RATIO_SATURATION: f64 = 0.50;
/// Minimum overlapping samples for the appearance metric to mean anything.
const MIN_APPEARANCE_SAMPLES: usize = 400;
/// Radius, in analysis pixels, by which the warp mask is eroded before the
/// appearance metric looks at it.
const APPEARANCE_MASK_EROSION: u32 = 2;
/// Grid used when sampling correspondences for spatial coverage.
const CORRESPONDENCE_SAMPLING_GRID: u32 = 6;

/// Clamp into `[0, 1]`, mapping non-finite input to `0.0`.
#[inline]
pub fn normalize_unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Feature-coverage component: how much of each image the inliers actually span.
///
/// Combines grid occupancy with convex-hull area, and averages the two images:
/// a transform supported by a well-spread set in one image and a blob in the
/// other is not credible.
pub fn feature_coverage(
    inliers_a: &[[f64; 2]],
    dims_a: (u32, u32),
    inliers_b: &[[f64; 2]],
    dims_b: (u32, u32),
    grid: u32,
) -> f64 {
    let per_image = |points: &[[f64; 2]], dims: (u32, u32)| -> f64 {
        if grid == 0 {
            return 0.0;
        }
        let cells = occupied_cells(points, dims, grid) as f64 / (grid * grid) as f64;
        let hull = hull_area_fraction(points, dims);
        let grid_term = normalize_unit(cells / COVERAGE_GRID_SATURATION);
        let hull_term = normalize_unit(hull / COVERAGE_HULL_SATURATION);
        0.6 * grid_term + 0.4 * hull_term
    };
    normalize_unit((per_image(inliers_a, dims_a) + per_image(inliers_b, dims_b)) / 2.0)
}

/// Inlier-confidence component: count, ratio and reprojection quality combined.
pub fn inlier_confidence(
    inliers: usize,
    filtered_matches: usize,
    median_error_px: f64,
    ransac_threshold_px: f64,
) -> f64 {
    let count_term = normalize_unit(inliers as f64 / CONFIDENCE_INLIER_SATURATION);
    let ratio = if filtered_matches == 0 {
        0.0
    } else {
        inliers as f64 / filtered_matches as f64
    };
    let ratio_term = normalize_unit(ratio / CONFIDENCE_RATIO_SATURATION);
    let error_term = if ransac_threshold_px <= 0.0 || !median_error_px.is_finite() {
        0.0
    } else {
        normalize_unit(1.0 - median_error_px / ransac_threshold_px)
    };
    normalize_unit(0.4 * count_term + 0.3 * ratio_term + 0.3 * error_term)
}

/// The weighted composite score (`SPEC.md` §5.4).
///
/// Divided by the weight sum so a reweighting does not silently move the
/// threshold along with it.
pub fn composite_score(
    geometric_overlap: f64,
    feature_coverage: f64,
    inlier_confidence: f64,
    aligned_appearance_similarity: f64,
    weights: &MetricWeights,
) -> f64 {
    let sum = weights.sum();
    if sum <= 0.0 {
        return 0.0;
    }
    let raw = weights.geometric_overlap * normalize_unit(geometric_overlap)
        + weights.feature_coverage * normalize_unit(feature_coverage)
        + weights.inlier_confidence * normalize_unit(inlier_confidence)
        + weights.aligned_appearance_similarity * normalize_unit(aligned_appearance_similarity);
    normalize_unit(raw / sum)
}

/// Everything needed to analyze one candidate pair.
pub struct PairInput<'a> {
    /// Features of the first photo.
    pub features_a: &'a PhotoFeatures,
    /// Features of the second photo.
    pub features_b: &'a PhotoFeatures,
    /// Analysis image of the first photo.
    pub image_a: &'a GrayImage,
    /// Analysis image of the second photo.
    pub image_b: &'a GrayImage,
}

/// The verdict and evidence for one candidate pair.
#[derive(Debug, Clone, PartialEq)]
pub struct PairAnalysis {
    /// Verdict.
    pub state: PairState,
    /// Why, when the verdict is not `compatible`.
    pub reason: Option<PairReason>,
    /// Composite score, when computable.
    pub score: Option<f64>,
    /// Whether every hard gate passed.
    pub hard_gates_passed: bool,
    /// Gates that failed.
    pub gate_failures: Vec<GateFailure>,
    /// Metrics, when a transform was fitted.
    pub metrics: Option<PairMetrics>,
    /// Normalized homography from the first photo to the second.
    pub homography_a_to_b: Option<Mat3>,
    /// Bounded, spatially sampled inlier correspondences, `a` to `b`.
    pub correspondences: Vec<Correspondence>,
}

impl PairAnalysis {
    fn uncertain(reason: PairReason) -> Self {
        Self {
            state: PairState::Uncertain,
            reason: Some(reason),
            score: None,
            hard_gates_passed: false,
            gate_failures: Vec::new(),
            metrics: None,
            homography_a_to_b: None,
            correspondences: Vec::new(),
        }
    }
}

/// Analyze one candidate pair end to end.
pub fn analyze_pair(input: &PairInput<'_>, algorithm: &AlgorithmInfo) -> PairAnalysis {
    let started = std::time::Instant::now();
    let params = &algorithm.parameters;

    // Low texture is a lack of evidence, not evidence of a mismatch.
    if input.features_a.len() < params.min_keypoints_for_decision
        || input.features_b.len() < params.min_keypoints_for_decision
    {
        return PairAnalysis::uncertain(PairReason::LowTexture);
    }

    let matcher = BruteForceMatcher {
        lowe_ratio: params.lowe_ratio,
        mutual: params.mutual_match,
    };
    let matches = matcher.match_descriptors(input.features_a, input.features_b);
    if matches.len() < params.min_matches_for_decision {
        return PairAnalysis::uncertain(PairReason::InsufficientMatches);
    }

    let src: Vec<[f64; 2]> = matches
        .iter()
        .map(|m| {
            let kp = input.features_a.keypoints[m.a];
            [kp.x as f64, kp.y as f64]
        })
        .collect();
    let dst: Vec<[f64; 2]> = matches
        .iter()
        .map(|m| {
            let kp = input.features_b.keypoints[m.b];
            [kp.x as f64, kp.y as f64]
        })
        .collect();

    let estimator = RansacHomographyEstimator {
        threshold_px: params.ransac_threshold_px,
        max_iterations: params.ransac_max_iterations,
        seed: params.ransac_seed,
        min_inliers: 8,
    };
    let Some(registration) = estimator.estimate(&src, &dst) else {
        return PairAnalysis::uncertain(PairReason::NoConsensus);
    };

    let dims_a = input.image_a.dimensions();
    let dims_b = input.image_b.dimensions();
    let Some(normalized) = pixel_to_normalized(&registration.homography, dims_a, dims_b) else {
        return PairAnalysis::uncertain(PairReason::NoConsensus);
    };

    let inlier_points_a: Vec<[f64; 2]> = registration.inliers.iter().map(|&i| src[i]).collect();
    let inlier_points_b: Vec<[f64; 2]> = registration.inliers.iter().map(|&i| dst[i]).collect();
    let median_error = median(&registration.reprojection_errors_px).unwrap_or(f64::INFINITY);

    let overlap = overlap_or_zero(&normalized);
    let coverage = feature_coverage(
        &inlier_points_a,
        dims_a,
        &inlier_points_b,
        dims_b,
        algorithm.gates.grid_size,
    );
    let confidence = inlier_confidence(
        registration.inliers.len(),
        matches.len(),
        median_error,
        params.ransac_threshold_px,
    );

    let gate_inputs = GateInputs {
        inliers: registration.inliers.len(),
        median_reprojection_error_px: median_error,
        occupied_cells_a: occupied_cells(&inlier_points_a, dims_a, algorithm.gates.grid_size),
        occupied_cells_b: occupied_cells(&inlier_points_b, dims_b, algorithm.gates.grid_size),
        geometric_overlap: overlap,
        normalized_homography: normalized,
    };
    let gate_failures = evaluate_gates(&gate_inputs, &algorithm.gates);
    let gates_passed = gate_failures.is_empty();

    let appearance = aligned_appearance_similarity(input, &registration);
    let ssim = registered_ssim(input, &registration);

    let sv = normalized.affine_singular_values();
    let metrics = PairMetrics {
        keypoints_a: input.features_a.len(),
        keypoints_b: input.features_b.len(),
        filtered_matches: matches.len(),
        inliers: registration.inliers.len(),
        inlier_ratio: if matches.is_empty() {
            0.0
        } else {
            registration.inliers.len() as f64 / matches.len() as f64
        },
        median_reprojection_error_px: if median_error.is_finite() {
            median_error
        } else {
            f64::MAX
        },
        occupied_cells_a: gate_inputs.occupied_cells_a,
        occupied_cells_b: gate_inputs.occupied_cells_b,
        geometric_overlap: overlap,
        feature_coverage: coverage,
        inlier_confidence: confidence,
        aligned_appearance_similarity: appearance.unwrap_or(0.0),
        registered_ssim: ssim,
        affine_scale: sv[0],
        duration_ms: started.elapsed().as_millis() as u64,
    };

    // No appearance evidence at all: the geometry may look fine but nothing
    // confirms the two images show the same thing, so this stays undecided.
    let Some(appearance) = appearance else {
        return PairAnalysis {
            state: PairState::Uncertain,
            reason: Some(PairReason::LowTexture),
            score: None,
            hard_gates_passed: gates_passed,
            gate_failures,
            metrics: Some(metrics),
            homography_a_to_b: Some(normalized),
            correspondences: Vec::new(),
        };
    };

    let score = composite_score(
        overlap,
        coverage,
        confidence,
        appearance,
        &algorithm.weights,
    );

    let (state, reason) = if !gates_passed {
        (PairState::Incompatible, Some(PairReason::GateFailed))
    } else if score + 1e-12 >= params.compatibility_threshold {
        (PairState::Compatible, None)
    } else {
        (
            PairState::Incompatible,
            Some(PairReason::ScoreBelowThreshold),
        )
    };

    let correspondences = if state == PairState::Compatible {
        sample_correspondences(
            input,
            &registration,
            &matches,
            &normalized,
            dims_a,
            params.max_correspondences,
        )
    } else {
        Vec::new()
    };

    PairAnalysis {
        state,
        reason,
        score: Some(score),
        hard_gates_passed: gates_passed,
        gate_failures,
        metrics: Some(metrics),
        homography_a_to_b: Some(normalized),
        correspondences,
    }
}

/// Gradient similarity over the registered overlap.
///
/// `None` when there is not enough common, structured support to say anything.
fn aligned_appearance_similarity(
    input: &PairInput<'_>,
    registration: &Registration,
) -> Option<f64> {
    let dims_b = input.image_b.dimensions();
    let (warped, warp_mask) = BilinearWarper.warp(input.image_a, &registration.homography, dims_b);
    let usable = erode_mask(&warp_mask, dims_b, APPEARANCE_MASK_EROSION);
    if usable.iter().filter(|m| **m).count() < MIN_APPEARANCE_SAMPLES {
        return None;
    }
    let (grad_warped, valid_warped) = gradient_magnitude(&warped);
    let (grad_b, valid_b) = gradient_magnitude(input.image_b);
    let mask_a = intersect_masks(&usable, &valid_warped);
    let mask_b = intersect_masks(&usable, &valid_b);
    masked_zncc(
        &grad_warped,
        &mask_a,
        &grad_b,
        &mask_b,
        MIN_APPEARANCE_SAMPLES,
    )
}

/// Post-registration SSIM, retained as a diagnostic only.
fn registered_ssim(input: &PairInput<'_>, registration: &Registration) -> Option<f64> {
    let dims_b = input.image_b.dimensions();
    let (warped, warp_mask) = BilinearWarper.warp(input.image_a, &registration.homography, dims_b);
    let usable = erode_mask(&warp_mask, dims_b, APPEARANCE_MASK_EROSION);
    masked_ssim(
        &warped,
        &usable,
        input.image_b,
        &vec![true; usable.len()],
        MIN_APPEARANCE_SAMPLES,
    )
}

/// Pick a bounded, spatially spread subset of the inlier correspondences.
///
/// Round-robin over grid cells rather than "lowest descriptor distance first":
/// the best-matching features cluster, and a cluster is exactly what the
/// alignment data must not be (`SPEC.md` §7.2).
fn sample_correspondences(
    input: &PairInput<'_>,
    registration: &Registration,
    matches: &[crate::domain::FeatureMatch],
    normalized: &Mat3,
    dims_a: (u32, u32),
    limit: usize,
) -> Vec<Correspondence> {
    if limit == 0 {
        return Vec::new();
    }
    let grid = CORRESPONDENCE_SAMPLING_GRID;
    let cells = (grid * grid) as usize;
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); cells];

    let dims_b = input.image_b.dimensions();
    for (slot, &match_index) in registration.inliers.iter().enumerate() {
        let kp = input.features_a.keypoints[matches[match_index].a];
        let cx = ((kp.x as f64 / dims_a.0 as f64) * grid as f64).floor();
        let cy = ((kp.y as f64 / dims_a.1 as f64) * grid as f64).floor();
        if cx < 0.0 || cy < 0.0 {
            continue;
        }
        let cx = (cx as u32).min(grid - 1);
        let cy = (cy as u32).min(grid - 1);
        buckets[(cy * grid + cx) as usize].push(slot);
    }

    // Within a cell prefer the most distinctive matches; across cells take turns.
    for bucket in buckets.iter_mut() {
        bucket.sort_by(|x, y| {
            let rx = matches[registration.inliers[*x]].ratio;
            let ry = matches[registration.inliers[*y]].ratio;
            rx.partial_cmp(&ry)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(x.cmp(y))
        });
    }

    let mut out = Vec::with_capacity(limit.min(registration.inliers.len()));
    let mut round = 0usize;
    loop {
        let mut took_any = false;
        for bucket in buckets.iter() {
            if out.len() >= limit {
                return out;
            }
            if let Some(&slot) = bucket.get(round) {
                took_any = true;
                let m = matches[registration.inliers[slot]];
                let kp_a = input.features_a.keypoints[m.a];
                let kp_b = input.features_b.keypoints[m.b];
                let photo = [
                    kp_a.x as f64 / dims_a.0 as f64,
                    kp_a.y as f64 / dims_a.1 as f64,
                ];
                let anchor = [
                    kp_b.x as f64 / dims_b.0 as f64,
                    kp_b.y as f64 / dims_b.1 as f64,
                ];
                // Residual in normalized anchor units, so it survives a change of
                // analysis resolution (`SPEC.md` §7.2).
                let error = normalized
                    .apply(photo)
                    .map(|p| (p[0] - anchor[0]).hypot(p[1] - anchor[1]))
                    .unwrap_or(f64::NAN);
                out.push(Correspondence {
                    photo,
                    anchor,
                    match_quality: normalize_unit(1.0 - m.ratio as f64),
                    reprojection_error_normalized: if error.is_finite() { error } else { 1.0 },
                    scale: Some(kp_a.size as f64 / dims_a.0.max(dims_a.1) as f64),
                    angle: Some(kp_a.angle as f64),
                });
            }
        }
        if !took_any {
            break;
        }
        round += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registration::occupied_cells as occ;
    use crate::schema::HardGates;

    #[test]
    fn normalization_handles_out_of_range_and_non_finite_values() {
        assert_eq!(normalize_unit(0.5), 0.5);
        assert_eq!(normalize_unit(-1.0), 0.0);
        assert_eq!(normalize_unit(2.0), 1.0);
        // Non-finite means "no evidence", which is 0, not a perfect score.
        assert_eq!(normalize_unit(f64::NAN), 0.0);
        assert_eq!(normalize_unit(f64::INFINITY), 0.0);
        assert_eq!(normalize_unit(f64::NEG_INFINITY), 0.0);
    }

    #[test]
    fn the_composite_score_matches_a_hand_computed_fixture() {
        let w = MetricWeights::default();
        // 0.35*0.86 + 0.25*0.82 + 0.20*0.91 + 0.20*0.88
        //   = 0.301 + 0.205 + 0.182 + 0.176 = 0.864
        let score = composite_score(0.86, 0.82, 0.91, 0.88, &w);
        assert!((score - 0.864).abs() < 1e-12, "{score}");
    }

    #[test]
    fn the_score_is_normalized_by_the_weight_sum() {
        // Doubling every weight must not change the score.
        let base = MetricWeights::default();
        let doubled = MetricWeights {
            geometric_overlap: base.geometric_overlap * 2.0,
            feature_coverage: base.feature_coverage * 2.0,
            inlier_confidence: base.inlier_confidence * 2.0,
            aligned_appearance_similarity: base.aligned_appearance_similarity * 2.0,
        };
        let a = composite_score(0.9, 0.8, 0.7, 0.6, &base);
        let b = composite_score(0.9, 0.8, 0.7, 0.6, &doubled);
        assert!((a - b).abs() < 1e-12);
    }

    #[test]
    fn perfect_components_score_one_and_empty_components_score_zero() {
        let w = MetricWeights::default();
        assert!((composite_score(1.0, 1.0, 1.0, 1.0, &w) - 1.0).abs() < 1e-12);
        assert_eq!(composite_score(0.0, 0.0, 0.0, 0.0, &w), 0.0);
    }

    #[test]
    fn zero_weights_do_not_divide_by_zero() {
        let w = MetricWeights {
            geometric_overlap: 0.0,
            feature_coverage: 0.0,
            inlier_confidence: 0.0,
            aligned_appearance_similarity: 0.0,
        };
        assert_eq!(composite_score(1.0, 1.0, 1.0, 1.0, &w), 0.0);
    }

    #[test]
    fn coverage_rewards_spread_and_punishes_clustering() {
        let dims = (1200, 900);
        let spread: Vec<[f64; 2]> = (0..4)
            .flat_map(|i| {
                (0..4).map(move |j| {
                    [
                        1200.0 * (i as f64 + 0.5) / 4.0,
                        900.0 * (j as f64 + 0.5) / 4.0,
                    ]
                })
            })
            .collect();
        let clustered: Vec<[f64; 2]> = (0..40)
            .map(|i| [600.0 + (i % 7) as f64, 450.0 + (i / 7) as f64])
            .collect();
        let good = feature_coverage(&spread, dims, &spread, dims, 4);
        let bad = feature_coverage(&clustered, dims, &clustered, dims, 4);
        assert!(good > 0.8, "spread coverage {good}");
        assert!(bad < 0.15, "clustered coverage {bad}");
        assert_eq!(occ(&clustered, dims, 4), 1);
    }

    #[test]
    fn coverage_averages_both_images() {
        let dims = (1200, 900);
        let spread: Vec<[f64; 2]> = (0..4)
            .flat_map(|i| {
                (0..4).map(move |j| {
                    [
                        1200.0 * (i as f64 + 0.5) / 4.0,
                        900.0 * (j as f64 + 0.5) / 4.0,
                    ]
                })
            })
            .collect();
        let clustered: Vec<[f64; 2]> = (0..40).map(|i| [600.0, 450.0 + i as f64]).collect();
        let mixed = feature_coverage(&spread, dims, &clustered, dims, 4);
        let both_good = feature_coverage(&spread, dims, &spread, dims, 4);
        assert!(mixed < both_good);
    }

    #[test]
    fn coverage_of_nothing_is_zero() {
        assert_eq!(feature_coverage(&[], (100, 100), &[], (100, 100), 4), 0.0);
        assert_eq!(feature_coverage(&[], (100, 100), &[], (100, 100), 0), 0.0);
    }

    #[test]
    fn confidence_saturates_and_bottoms_out() {
        assert!(inlier_confidence(500, 600, 0.0, 3.0) > 0.99);
        assert_eq!(inlier_confidence(0, 0, 0.0, 3.0), 0.3);
        // A median error at the RANSAC threshold contributes nothing.
        let at_threshold = inlier_confidence(100, 200, 3.0, 3.0);
        let at_zero = inlier_confidence(100, 200, 0.0, 3.0);
        assert!(at_threshold < at_zero);
    }

    #[test]
    fn confidence_ignores_a_non_finite_error() {
        let c = inlier_confidence(100, 100, f64::NAN, 3.0);
        assert!(c.is_finite() && (0.0..=1.0).contains(&c));
    }

    // ---- End-to-end pair analysis on synthetic ground truth -----------------

    /// Three octaves of value noise on a seed-dependent lattice.
    ///
    /// Aperiodic on purpose: a repeating pattern would let RANSAC find a
    /// consistent transform between unrelated images. And the *structure*
    /// depends on the seed, not only an additive grain — otherwise the
    /// "unrelated scenes" negative is really the same scene twice, which is a
    /// fixture that proves nothing.
    fn textured(w: u32, h: u32, seed: u32) -> GrayImage {
        fn lattice(x: i32, y: i32, seed: u32) -> f32 {
            let mut h = (x as u32).wrapping_mul(374_761_393)
                ^ (y as u32).wrapping_mul(668_265_263)
                ^ seed.wrapping_mul(2_246_822_519);
            h ^= h >> 13;
            h = h.wrapping_mul(1_274_126_177);
            h ^= h >> 16;
            (h % 10_000) as f32 / 10_000.0
        }
        fn octave(x: f32, y: f32, cell: f32, seed: u32) -> f32 {
            let gx = (x / cell).floor();
            let gy = (y / cell).floor();
            let fx = x / cell - gx;
            let fy = y / cell - gy;
            let sx = fx * fx * (3.0 - 2.0 * fx);
            let sy = fy * fy * (3.0 - 2.0 * fy);
            let (gx, gy) = (gx as i32, gy as i32);
            let a = lattice(gx, gy, seed);
            let b = lattice(gx + 1, gy, seed);
            let c = lattice(gx, gy + 1, seed);
            let d = lattice(gx + 1, gy + 1, seed);
            let top = a + (b - a) * sx;
            let bottom = c + (d - c) * sx;
            top + (bottom - top) * sy
        }
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let (xf, yf) = (x as f32, y as f32);
                let v = 0.50 * octave(xf, yf, 17.0, seed)
                    + 0.30 * octave(xf, yf, 7.0, seed ^ 0x9E37_79B9)
                    + 0.20 * octave(xf, yf, 3.0, seed ^ 0x85EB_CA6B);
                data.push((v * 255.0).clamp(0.0, 255.0) as u8);
            }
        }
        GrayImage::new(w, h, data).unwrap()
    }

    /// Resample `src` through `m` (source-to-destination) using the production
    /// warper, so the synthetic pair has a known ground-truth transform.
    fn transformed(src: &GrayImage, m: &Mat3) -> GrayImage {
        BilinearWarper.warp(src, m, src.dimensions()).0
    }

    fn algorithm() -> AlgorithmInfo {
        AlgorithmInfo {
            gates: HardGates::default(),
            ..Default::default()
        }
    }

    fn features_of(img: &GrayImage, limit: usize) -> PhotoFeatures {
        use crate::domain::FeatureExtractor;
        crate::features::SiftExtractor::new(limit)
            .extract(img)
            .unwrap()
    }

    #[test]
    fn a_known_translation_is_compatible_and_maps_control_points() {
        let a = textured(480, 360, 7);
        let truth = Mat3([1.0, 0.0, 18.0, 0.0, 1.0, -11.0, 0.0, 0.0, 1.0]);
        let b = transformed(&a, &truth);

        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let algo = algorithm();
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algo,
        );

        assert_eq!(result.state, PairState::Compatible, "{result:?}");
        assert!(result.hard_gates_passed);
        assert!(result.score.unwrap() >= algo.parameters.compatibility_threshold);
        assert!(!result.correspondences.is_empty());

        // The persisted normalized transform must map a full-resolution pointer
        // correctly: 18 analysis px of a 480 px image is 3.75% of the width.
        let h = result.homography_a_to_b.unwrap();
        let mapped = h.apply([0.5, 0.5]).unwrap();
        assert!(
            (mapped[0] - (0.5 + 18.0 / 480.0)).abs() < 0.01,
            "{mapped:?}"
        );
        assert!(
            (mapped[1] - (0.5 - 11.0 / 360.0)).abs() < 0.01,
            "{mapped:?}"
        );
    }

    #[test]
    fn a_known_rotation_and_scale_is_still_compatible() {
        let a = textured(480, 360, 11);
        let angle = 6.0f64.to_radians();
        let (s, c) = (angle.sin(), angle.cos());
        let scale = 1.08;
        // Rotate and scale about the image centre.
        let cx = 240.0;
        let cy = 180.0;
        let truth = Mat3([
            scale * c,
            -scale * s,
            cx - scale * (c * cx - s * cy),
            scale * s,
            scale * c,
            cy - scale * (s * cx + c * cy),
            0.0,
            0.0,
            1.0,
        ]);
        let b = transformed(&a, &truth);
        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algorithm(),
        );
        assert_eq!(result.state, PairState::Compatible, "{result:?}");
    }

    #[test]
    fn an_exposure_change_does_not_break_a_true_match() {
        let a = textured(480, 360, 13);
        let truth = Mat3([1.0, 0.0, 9.0, 0.0, 1.0, 6.0, 0.0, 0.0, 1.0]);
        let mut b = transformed(&a, &truth);
        for v in b.as_mut_slice() {
            *v = ((*v as f32) * 0.65 + 55.0).round().clamp(0.0, 255.0) as u8;
        }
        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algorithm(),
        );
        assert_eq!(result.state, PairState::Compatible, "{result:?}");
    }

    #[test]
    fn two_unrelated_scenes_are_not_compatible() {
        let a = textured(480, 360, 3);
        let b = textured(480, 360, 999);
        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algorithm(),
        );
        assert_ne!(result.state, PairState::Compatible, "{result:?}");
    }

    #[test]
    fn a_barely_overlapping_pair_is_rejected_by_the_overlap_gate() {
        let a = textured(640, 480, 21);
        // Shift by 85% of the width: the true overlap is about 0.15.
        let truth = Mat3([1.0, 0.0, 544.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let b = transformed(&a, &truth);
        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algorithm(),
        );
        assert_ne!(result.state, PairState::Compatible, "{result:?}");
    }

    #[test]
    fn a_flat_image_pair_is_uncertain_not_incompatible() {
        let flat = GrayImage::zeros(320, 240);
        let fa = features_of(&flat, 3000);
        let fb = fa.clone();
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &flat,
                image_b: &flat,
            },
            &algorithm(),
        );
        assert_eq!(result.state, PairState::Uncertain);
        assert_eq!(result.reason, Some(PairReason::LowTexture));
    }

    #[test]
    fn analysis_is_reproducible() {
        let a = textured(360, 270, 5);
        let truth = Mat3([1.0, 0.0, 7.0, 0.0, 1.0, -5.0, 0.0, 0.0, 1.0]);
        let b = transformed(&a, &truth);
        let fa = features_of(&a, 2000);
        let fb = features_of(&b, 2000);
        let input = PairInput {
            features_a: &fa,
            features_b: &fb,
            image_a: &a,
            image_b: &b,
        };
        let algo = algorithm();
        let first = analyze_pair(&input, &algo);
        let second = analyze_pair(&input, &algo);
        assert_eq!(first.state, second.state);
        assert_eq!(first.score, second.score);
        assert_eq!(first.homography_a_to_b, second.homography_a_to_b);
        assert_eq!(first.correspondences, second.correspondences);
    }

    #[test]
    fn correspondences_are_spread_rather_than_clustered() {
        let a = textured(480, 360, 17);
        let truth = Mat3([1.0, 0.0, 12.0, 0.0, 1.0, 8.0, 0.0, 0.0, 1.0]);
        let b = transformed(&a, &truth);
        let fa = features_of(&a, 3000);
        let fb = features_of(&b, 3000);
        let mut algo = algorithm();
        algo.parameters.max_correspondences = 36;
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algo,
        );
        assert_eq!(result.state, PairState::Compatible);
        assert!(result.correspondences.len() <= 36);
        let points: Vec<[f64; 2]> = result
            .correspondences
            .iter()
            .map(|c| [c.photo[0] * 480.0, c.photo[1] * 360.0])
            .collect();
        let cells = occ(&points, (480, 360), 4);
        assert!(cells >= 8, "correspondences only spanned {cells} cells");
        for c in &result.correspondences {
            assert!((0.0..=1.0).contains(&c.photo[0]));
            assert!((0.0..=1.0).contains(&c.photo[1]));
            assert!(c.reprojection_error_normalized.is_finite());
        }
    }

    #[test]
    fn a_high_appearance_score_cannot_bypass_a_failed_gate() {
        // Identical images, but with the overlap gate set to an impossible value.
        let a = textured(400, 300, 23);
        let fa = features_of(&a, 2000);
        let mut algo = algorithm();
        algo.gates.min_geometric_overlap = 1.5;
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fa,
                image_a: &a,
                image_b: &a,
            },
            &algo,
        );
        assert_eq!(result.state, PairState::Incompatible);
        assert_eq!(result.reason, Some(PairReason::GateFailed));
        assert!(result.score.unwrap() > 0.8, "score was still computed");
        assert!(result.correspondences.is_empty());
    }

    #[test]
    fn a_threshold_of_one_rejects_on_score_and_names_that_reason() {
        let a = textured(400, 300, 29);
        let truth = Mat3([1.0, 0.0, 6.0, 0.0, 1.0, 4.0, 0.0, 0.0, 1.0]);
        let b = transformed(&a, &truth);
        let fa = features_of(&a, 2000);
        let fb = features_of(&b, 2000);
        let mut algo = algorithm();
        algo.parameters.compatibility_threshold = 1.01;
        let result = analyze_pair(
            &PairInput {
                features_a: &fa,
                features_b: &fb,
                image_a: &a,
                image_b: &b,
            },
            &algo,
        );
        assert_eq!(result.state, PairState::Incompatible);
        assert_eq!(result.reason, Some(PairReason::ScoreBelowThreshold));
        assert!(result.hard_gates_passed);
    }
}
