//! Robust homography fitting and the geometric hard gates.
//!
//! The gates are evaluated separately from, and before, the composite score: a
//! high appearance similarity must never be able to rescue an implausible
//! transform (`SPEC.md` §5.2).

use vision_calibration_core::{Pt2, RansacOptions};
use vision_geometry::homography::dlt_homography_ransac;

use crate::domain::{HomographyEstimator, Registration};
use crate::geometry::{convex_hull, geometric_overlap, polygon_area, Mat3};
use crate::schema::{GateFailure, HardGates};

/// Adapter over `vision-geometry`'s normalized-DLT RANSAC.
#[derive(Debug, Clone, Copy)]
pub struct RansacHomographyEstimator {
    /// Inlier threshold, in the units of the input points (analysis pixels).
    pub threshold_px: f64,
    /// Iteration cap.
    pub max_iterations: usize,
    /// Fixed seed, so a rerun reproduces the same consensus set.
    pub seed: u64,
    /// Minimum consensus size for the estimator to report success. Kept below
    /// the hard gate so the gate, not the estimator, is what reports the
    /// rejection.
    pub min_inliers: usize,
}

impl Default for RansacHomographyEstimator {
    fn default() -> Self {
        Self {
            threshold_px: 3.0,
            max_iterations: 2000,
            seed: 0x00CA_3C09_9911,
            min_inliers: 8,
        }
    }
}

impl HomographyEstimator for RansacHomographyEstimator {
    fn estimate(&self, src: &[[f64; 2]], dst: &[[f64; 2]]) -> Option<Registration> {
        if src.len() < 4 || src.len() != dst.len() {
            return None;
        }
        let to_pt = |p: &[f64; 2]| Pt2::new(p[0], p[1]);
        let src_pts: Vec<Pt2> = src.iter().map(to_pt).collect();
        let dst_pts: Vec<Pt2> = dst.iter().map(to_pt).collect();
        let opts = RansacOptions {
            max_iters: self.max_iterations,
            thresh: self.threshold_px,
            min_inliers: self.min_inliers.max(4),
            confidence: 0.999,
            seed: self.seed,
            refit_on_inliers: true,
        };
        let (matrix, inliers) = dlt_homography_ransac(&src_pts, &dst_pts, &opts).ok()?;

        let mut row_major = [0.0f64; 9];
        for r in 0..3 {
            for c in 0..3 {
                row_major[r * 3 + c] = matrix[(r, c)];
            }
        }
        let homography = Mat3::from_row_major(&row_major)?.normalized();
        if !homography.is_finite() {
            return None;
        }

        // Recompute the residuals from the returned transform rather than
        // trusting the estimator's internal scoring: the refit step can move the
        // model after the consensus set was chosen.
        let mut kept = Vec::with_capacity(inliers.len());
        let mut errors = Vec::with_capacity(inliers.len());
        for &i in &inliers {
            if i >= src.len() {
                continue;
            }
            let Some(mapped) = homography.apply(src[i]) else {
                continue;
            };
            let err = (mapped[0] - dst[i][0]).hypot(mapped[1] - dst[i][1]);
            if !err.is_finite() {
                continue;
            }
            kept.push(i);
            errors.push(err);
        }
        if kept.is_empty() {
            return None;
        }
        Some(Registration {
            homography,
            inliers: kept,
            reprojection_errors_px: errors,
        })
    }
}

/// Median of a slice, without mutating the caller's data.
pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((sorted[mid - 1] + sorted[mid]) / 2.0)
    } else {
        Some(sorted[mid])
    }
}

/// Count occupied cells of a `grid × grid` partition of an image.
///
/// A fitted transform supported only by a small common object concentrates its
/// inliers in one or two cells; this is what detects that.
pub fn occupied_cells(points: &[[f64; 2]], dims: (u32, u32), grid: u32) -> usize {
    if grid == 0 || dims.0 == 0 || dims.1 == 0 {
        return 0;
    }
    let mut seen = vec![false; (grid * grid) as usize];
    for p in points {
        if !p[0].is_finite() || !p[1].is_finite() {
            continue;
        }
        let cx = ((p[0] / dims.0 as f64) * grid as f64).floor();
        let cy = ((p[1] / dims.1 as f64) * grid as f64).floor();
        if cx < 0.0 || cy < 0.0 {
            continue;
        }
        let cx = (cx as u32).min(grid - 1);
        let cy = (cy as u32).min(grid - 1);
        seen[(cy * grid + cx) as usize] = true;
    }
    seen.iter().filter(|s| **s).count()
}

/// Fraction of the image area spanned by the convex hull of the given points.
pub fn hull_area_fraction(points: &[[f64; 2]], dims: (u32, u32)) -> f64 {
    if points.len() < 3 || dims.0 == 0 || dims.1 == 0 {
        return 0.0;
    }
    let hull = convex_hull(points);
    let area = polygon_area(&hull);
    let total = dims.0 as f64 * dims.1 as f64;
    if total <= 0.0 {
        return 0.0;
    }
    (area / total).clamp(0.0, 1.0)
}

/// Everything the gates need, gathered once.
#[derive(Debug, Clone)]
pub struct GateInputs {
    /// RANSAC inlier count.
    pub inliers: usize,
    /// Median reprojection error, in analysis pixels.
    pub median_reprojection_error_px: f64,
    /// Occupied grid cells in the first image.
    pub occupied_cells_a: usize,
    /// Occupied grid cells in the second image.
    pub occupied_cells_b: usize,
    /// Geometric overlap in the anchor frame.
    pub geometric_overlap: f64,
    /// Normalized-coordinate homography, used for the plausibility checks.
    pub normalized_homography: Mat3,
}

/// Evaluate every hard gate, returning the failures.
///
/// Every gate is checked, not short-circuited, so a rejection reports all its
/// reasons instead of only the first.
pub fn evaluate_gates(inputs: &GateInputs, gates: &HardGates) -> Vec<GateFailure> {
    let mut failures = Vec::new();
    if inputs.inliers < gates.min_inliers {
        failures.push(GateFailure::Inliers);
    }
    // Deliberately ordered so a non-finite error fails the gate: comparing the
    // other way round would let a NaN median pass.
    if !matches!(
        inputs
            .median_reprojection_error_px
            .partial_cmp(&gates.max_median_reprojection_error_px),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    ) {
        failures.push(GateFailure::ReprojectionError);
    }
    if inputs.occupied_cells_a < gates.min_occupied_cells {
        failures.push(GateFailure::SpatialDistributionA);
    }
    if inputs.occupied_cells_b < gates.min_occupied_cells {
        failures.push(GateFailure::SpatialDistributionB);
    }
    if inputs.geometric_overlap < gates.min_geometric_overlap {
        failures.push(GateFailure::GeometricOverlap);
    }

    let h = &inputs.normalized_homography;
    if !h.is_finite() || h.inverse().is_none() {
        failures.push(GateFailure::Degenerate);
    } else {
        if h.affine_det() <= 0.0 {
            failures.push(GateFailure::Reflection);
        }
        let sv = h.affine_singular_values();
        if sv[1] <= 0.0
            || !(gates.min_scale..=gates.max_scale).contains(&sv[0])
            || !(gates.min_scale..=gates.max_scale).contains(&sv[1])
        {
            failures.push(GateFailure::Scale);
        }
        if h.perspective_magnitude() > gates.max_perspective {
            failures.push(GateFailure::Perspective);
        }
    }
    failures
}

/// Geometric overlap for a normalized photo-to-anchor transform, `0.0` when the
/// transform is unusable.
pub fn overlap_or_zero(photo_to_anchor: &Mat3) -> f64 {
    geometric_overlap(photo_to_anchor).unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A well-conditioned grid of source points.
    fn grid_points(n: u32, w: f64, h: f64) -> Vec<[f64; 2]> {
        let mut pts = Vec::new();
        for i in 0..n {
            for j in 0..n {
                pts.push([
                    w * (i as f64 + 0.5) / n as f64,
                    h * (j as f64 + 0.5) / n as f64,
                ]);
            }
        }
        pts
    }

    fn map_all(m: &Mat3, pts: &[[f64; 2]]) -> Vec<[f64; 2]> {
        pts.iter().map(|p| m.apply(*p).unwrap()).collect()
    }

    #[test]
    fn an_exact_projective_transform_is_recovered() {
        let truth = Mat3([1.05, 0.02, 12.0, -0.03, 0.98, -7.0, 2e-5, 1e-5, 1.0]);
        let src = grid_points(6, 1200.0, 900.0);
        let dst = map_all(&truth, &src);
        let reg = RansacHomographyEstimator::default()
            .estimate(&src, &dst)
            .unwrap();
        assert_eq!(reg.inliers.len(), src.len());
        for p in &src {
            let expected = truth.apply(*p).unwrap();
            let got = reg.homography.apply(*p).unwrap();
            assert!(
                (expected[0] - got[0]).hypot(expected[1] - got[1]) < 0.5,
                "control point off by too much"
            );
        }
    }

    #[test]
    fn outliers_are_excluded_from_the_consensus_set() {
        let truth = Mat3([1.0, 0.0, 20.0, 0.0, 1.0, -10.0, 0.0, 0.0, 1.0]);
        let src = grid_points(6, 1200.0, 900.0);
        let mut dst = map_all(&truth, &src);
        // Corrupt a third of the correspondences.
        for (i, d) in dst.iter_mut().enumerate() {
            if i % 3 == 0 {
                d[0] += 250.0;
                d[1] -= 300.0;
            }
        }
        let reg = RansacHomographyEstimator::default()
            .estimate(&src, &dst)
            .unwrap();
        assert!(reg.inliers.len() >= 20, "{} inliers", reg.inliers.len());
        assert!(
            reg.inliers.iter().all(|i| i % 3 != 0),
            "an outlier was kept"
        );
    }

    #[test]
    fn noise_stays_within_the_reprojection_budget() {
        let truth = Mat3([1.0, 0.0, 5.0, 0.0, 1.0, 5.0, 0.0, 0.0, 1.0]);
        let src = grid_points(7, 1200.0, 900.0);
        let mut dst = map_all(&truth, &src);
        for (i, d) in dst.iter_mut().enumerate() {
            let jitter = ((i * 37) % 7) as f64 * 0.1 - 0.3;
            d[0] += jitter;
            d[1] -= jitter;
        }
        let reg = RansacHomographyEstimator::default()
            .estimate(&src, &dst)
            .unwrap();
        let m = median(&reg.reprojection_errors_px).unwrap();
        assert!(m < 1.0, "median error {m}");
    }

    #[test]
    fn fewer_than_four_points_cannot_be_fitted() {
        let src = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let dst = src.clone();
        assert!(RansacHomographyEstimator::default()
            .estimate(&src, &dst)
            .is_none());
    }

    #[test]
    fn mismatched_input_lengths_are_rejected() {
        let src = grid_points(3, 100.0, 100.0);
        let dst = grid_points(2, 100.0, 100.0);
        assert!(RansacHomographyEstimator::default()
            .estimate(&src, &dst)
            .is_none());
    }

    #[test]
    fn collinear_correspondences_do_not_produce_a_confident_fit() {
        let src: Vec<[f64; 2]> = (0..20).map(|i| [i as f64 * 10.0, 400.0]).collect();
        let dst: Vec<[f64; 2]> = (0..20).map(|i| [i as f64 * 10.0 + 3.0, 400.0]).collect();
        // Either no consensus, or a transform that the degeneracy gate rejects.
        if let Some(reg) = RansacHomographyEstimator::default().estimate(&src, &dst) {
            let cells = occupied_cells(&src, (1200, 900), 4);
            assert!(cells < 5, "collinear points occupied {cells} cells");
            assert!(reg.homography.is_finite());
        }
    }

    #[test]
    fn estimation_is_reproducible_with_a_fixed_seed() {
        let truth = Mat3([1.02, 0.01, 8.0, -0.01, 1.01, -4.0, 0.0, 0.0, 1.0]);
        let src = grid_points(6, 1200.0, 900.0);
        let mut dst = map_all(&truth, &src);
        for (i, d) in dst.iter_mut().enumerate() {
            if i % 5 == 0 {
                d[0] += 400.0;
            }
        }
        let est = RansacHomographyEstimator::default();
        let a = est.estimate(&src, &dst).unwrap();
        let b = est.estimate(&src, &dst).unwrap();
        assert_eq!(a.inliers, b.inliers);
        assert_eq!(a.homography, b.homography);
    }

    #[test]
    fn medians_handle_both_parities_and_empty_input() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0]), Some(3.0));
        assert_eq!(median(&[1.0, 3.0]), Some(2.0));
        assert_eq!(median(&[5.0, 1.0, 3.0]), Some(3.0));
        assert_eq!(median(&[f64::NAN]), None);
    }

    #[test]
    fn occupancy_counts_distinct_cells() {
        let dims = (1200, 900);
        let spread = grid_points(4, 1200.0, 900.0);
        assert_eq!(occupied_cells(&spread, dims, 4), 16);

        let corner: Vec<[f64; 2]> = (0..50).map(|i| [i as f64 * 0.5, i as f64 * 0.5]).collect();
        assert_eq!(occupied_cells(&corner, dims, 4), 1);
    }

    #[test]
    fn occupancy_clamps_points_on_the_far_edge() {
        // A point exactly at width must land in the last cell, not out of bounds.
        assert_eq!(occupied_cells(&[[1200.0, 900.0]], (1200, 900), 4), 1);
    }

    #[test]
    fn hull_fraction_is_area_based() {
        let full = grid_points(4, 1200.0, 900.0);
        let f = hull_area_fraction(&full, (1200, 900));
        assert!(f > 0.5, "{f}");
        let tiny: Vec<[f64; 2]> = vec![[10.0, 10.0], [12.0, 10.0], [11.0, 12.0]];
        assert!(hull_area_fraction(&tiny, (1200, 900)) < 0.001);
        assert_eq!(hull_area_fraction(&[[1.0, 1.0]], (1200, 900)), 0.0);
    }

    fn passing_inputs() -> GateInputs {
        GateInputs {
            inliers: 84,
            median_reprojection_error_px: 1.4,
            occupied_cells_a: 11,
            occupied_cells_b: 10,
            geometric_overlap: 0.86,
            normalized_homography: Mat3([1.01, 0.01, 0.02, -0.01, 1.0, -0.01, 0.01, 0.0, 1.0]),
        }
    }

    #[test]
    fn a_credible_registration_passes_every_gate() {
        assert!(evaluate_gates(&passing_inputs(), &HardGates::default()).is_empty());
    }

    #[test]
    fn the_inlier_gate_is_inclusive_at_its_boundary() {
        let gates = HardGates::default();
        let mut inputs = passing_inputs();
        inputs.inliers = gates.min_inliers;
        assert!(evaluate_gates(&inputs, &gates).is_empty());
        inputs.inliers = gates.min_inliers - 1;
        assert_eq!(evaluate_gates(&inputs, &gates), vec![GateFailure::Inliers]);
    }

    #[test]
    fn the_overlap_gate_is_inclusive_at_its_boundary() {
        let gates = HardGates::default();
        let mut inputs = passing_inputs();
        inputs.geometric_overlap = gates.min_geometric_overlap;
        assert!(evaluate_gates(&inputs, &gates).is_empty());
        inputs.geometric_overlap = gates.min_geometric_overlap - 0.001;
        assert_eq!(
            evaluate_gates(&inputs, &gates),
            vec![GateFailure::GeometricOverlap]
        );
    }

    #[test]
    fn a_localized_common_object_fails_spatial_distribution_in_both_images() {
        let gates = HardGates::default();
        let mut inputs = passing_inputs();
        inputs.occupied_cells_a = 2;
        inputs.occupied_cells_b = 3;
        let failures = evaluate_gates(&inputs, &gates);
        assert!(failures.contains(&GateFailure::SpatialDistributionA));
        assert!(failures.contains(&GateFailure::SpatialDistributionB));
    }

    #[test]
    fn each_image_is_gated_independently() {
        let gates = HardGates::default();
        let mut inputs = passing_inputs();
        inputs.occupied_cells_b = 1;
        let failures = evaluate_gates(&inputs, &gates);
        assert!(!failures.contains(&GateFailure::SpatialDistributionA));
        assert!(failures.contains(&GateFailure::SpatialDistributionB));
    }

    #[test]
    fn a_singular_transform_is_degenerate() {
        let mut inputs = passing_inputs();
        inputs.normalized_homography = Mat3([1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 1.0, 1.0]);
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::Degenerate));
    }

    #[test]
    fn a_reflection_is_rejected() {
        let mut inputs = passing_inputs();
        inputs.normalized_homography = Mat3([-1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::Reflection));
    }

    #[test]
    fn an_implausible_scale_is_rejected() {
        let mut inputs = passing_inputs();
        inputs.normalized_homography = Mat3::scaling(40.0, 40.0);
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::Scale));

        inputs.normalized_homography = Mat3::scaling(0.01, 0.01);
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::Scale));
    }

    #[test]
    fn extreme_perspective_is_rejected() {
        let mut inputs = passing_inputs();
        inputs.normalized_homography = Mat3([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.95, 0.0, 1.0]);
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::Perspective));
    }

    #[test]
    fn a_non_finite_error_fails_the_reprojection_gate() {
        let mut inputs = passing_inputs();
        inputs.median_reprojection_error_px = f64::NAN;
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.contains(&GateFailure::ReprojectionError));
    }

    #[test]
    fn all_failures_are_reported_not_just_the_first() {
        let inputs = GateInputs {
            inliers: 2,
            median_reprojection_error_px: 40.0,
            occupied_cells_a: 1,
            occupied_cells_b: 1,
            geometric_overlap: 0.1,
            normalized_homography: Mat3::scaling(-30.0, 30.0),
        };
        let failures = evaluate_gates(&inputs, &HardGates::default());
        assert!(failures.len() >= 6, "{failures:?}");
    }
}
