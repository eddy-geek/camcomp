//! Camcomp-owned projective geometry.
//!
//! Nothing here leaks a dependency type. Homographies are row-major `3 × 3`
//! matrices, exactly as persisted in `camcomp.json` (`SPEC.md` §7.1). The
//! polygon helpers replace a general geometry crate: every polygon Drop 1
//! clips is a convex quadrilateral, so Sutherland–Hodgman against four
//! half-planes is sufficient and keeps the dependency graph small
//! (`ADR-DROP1.md` §3.6).

/// A row-major `3 × 3` projective transform.
///
/// Element `(row, col)` lives at index `row * 3 + col`. A point is mapped as
/// `dst ~ M · src` with `src = (x, y, 1)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [f64; 9]);

impl Default for Mat3 {
    fn default() -> Self {
        Self::identity()
    }
}

impl Mat3 {
    /// The identity transform.
    pub const fn identity() -> Self {
        Mat3([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
    }

    /// Axis-aligned scaling.
    pub const fn scaling(sx: f64, sy: f64) -> Self {
        Mat3([sx, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 1.0])
    }

    /// Build from a row-major slice, rejecting the wrong length or any
    /// non-finite element.
    pub fn from_row_major(values: &[f64]) -> Option<Self> {
        if values.len() != 9 || values.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let mut m = [0.0; 9];
        m.copy_from_slice(values);
        Some(Mat3(m))
    }

    /// Row-major elements, in persistence order.
    pub fn as_row_major(&self) -> [f64; 9] {
        self.0
    }

    #[inline]
    fn at(&self, r: usize, c: usize) -> f64 {
        self.0[r * 3 + c]
    }

    /// Matrix product `self · other`.
    pub fn mul(&self, other: &Mat3) -> Mat3 {
        let mut out = [0.0f64; 9];
        for r in 0..3 {
            for c in 0..3 {
                let mut acc = 0.0;
                for k in 0..3 {
                    acc += self.at(r, k) * other.at(k, c);
                }
                out[r * 3 + c] = acc;
            }
        }
        Mat3(out)
    }

    /// Determinant.
    pub fn det(&self) -> f64 {
        let m = &self.0;
        m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6])
    }

    /// Inverse, or `None` when the matrix is singular or numerically unusable.
    pub fn inverse(&self) -> Option<Mat3> {
        let m = &self.0;
        let det = self.det();
        if !det.is_finite() || det.abs() < 1e-14 {
            return None;
        }
        let inv_det = 1.0 / det;
        let out = [
            (m[4] * m[8] - m[5] * m[7]) * inv_det,
            (m[2] * m[7] - m[1] * m[8]) * inv_det,
            (m[1] * m[5] - m[2] * m[4]) * inv_det,
            (m[5] * m[6] - m[3] * m[8]) * inv_det,
            (m[0] * m[8] - m[2] * m[6]) * inv_det,
            (m[2] * m[3] - m[0] * m[5]) * inv_det,
            (m[3] * m[7] - m[4] * m[6]) * inv_det,
            (m[1] * m[6] - m[0] * m[7]) * inv_det,
            (m[0] * m[4] - m[1] * m[3]) * inv_det,
        ];
        if out.iter().any(|v| !v.is_finite()) {
            return None;
        }
        Some(Mat3(out))
    }

    /// Normalize so that `m[8] == 1`, when possible.
    pub fn normalized(&self) -> Mat3 {
        let w = self.0[8];
        if w.abs() > 1e-15 && w.is_finite() {
            let mut out = self.0;
            for v in out.iter_mut() {
                *v /= w;
            }
            Mat3(out)
        } else {
            *self
        }
    }

    /// Map a point, returning `None` when the point maps to or beyond the line
    /// at infinity.
    pub fn apply(&self, p: [f64; 2]) -> Option<[f64; 2]> {
        let m = &self.0;
        let w = m[6] * p[0] + m[7] * p[1] + m[8];
        if !w.is_finite() || w.abs() < 1e-12 {
            return None;
        }
        let x = (m[0] * p[0] + m[1] * p[1] + m[2]) / w;
        let y = (m[3] * p[0] + m[4] * p[1] + m[5]) / w;
        if x.is_finite() && y.is_finite() {
            Some([x, y])
        } else {
            None
        }
    }

    /// Every element is finite.
    pub fn is_finite(&self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }

    /// Affine part `[[a, b], [c, d]]` as a flat array, used for the scale and
    /// reflection plausibility checks.
    fn affine_part(&self) -> [f64; 4] {
        [self.0[0], self.0[1], self.0[3], self.0[4]]
    }

    /// Determinant of the affine part. Negative means the transform mirrors the
    /// image, which no camera pair can legitimately produce.
    pub fn affine_det(&self) -> f64 {
        let a = self.affine_part();
        a[0] * a[3] - a[1] * a[2]
    }

    /// Singular values of the affine part, largest first.
    ///
    /// A closed form is used rather than a general SVD: for a `2 × 2` matrix the
    /// singular values follow from the eigenvalues of `AᵀA`.
    pub fn affine_singular_values(&self) -> [f64; 2] {
        let [a, b, c, d] = self.affine_part();
        let e = (a * a + b * b + c * c + d * d) / 2.0;
        let f = (a * a + b * b - c * c - d * d) / 2.0;
        let g = a * c + b * d;
        let radius = (f * f + g * g).sqrt();
        let s1 = (e + radius).max(0.0).sqrt();
        let s2 = (e - radius).max(0.0).sqrt();
        [s1, s2]
    }

    /// Magnitude of the projective row, in the coordinate system of the matrix.
    ///
    /// For a normalized-coordinate homography this is directly interpretable:
    /// `1.0` means the transform sends a point one unit away to infinity.
    pub fn perspective_magnitude(&self) -> f64 {
        self.0[6].abs().max(self.0[7].abs())
    }
}

/// Rebase a pixel-space homography into normalized coordinates.
///
/// `pixel` maps source-image pixels to destination-image pixels. The result
/// maps `[0, 1]²` of the source to `[0, 1]²` of the destination, so it stays
/// valid when Drop 3 applies it at full resolution even though Drop 1 estimated
/// it from a reduced analysis image (`SPEC.md` §7.1).
pub fn pixel_to_normalized(
    pixel: &Mat3,
    src_dims: (u32, u32),
    dst_dims: (u32, u32),
) -> Option<Mat3> {
    if src_dims.0 == 0 || src_dims.1 == 0 || dst_dims.0 == 0 || dst_dims.1 == 0 {
        return None;
    }
    let denorm_src = Mat3::scaling(src_dims.0 as f64, src_dims.1 as f64);
    let norm_dst = Mat3::scaling(1.0 / dst_dims.0 as f64, 1.0 / dst_dims.1 as f64);
    let m = norm_dst.mul(pixel).mul(&denorm_src).normalized();
    if m.is_finite() {
        Some(m)
    } else {
        None
    }
}

/// The unit square, counter-clockwise in image coordinates (y down).
pub const UNIT_SQUARE: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// Signed area of a simple polygon (shoelace). Positive for the winding used by
/// [`UNIT_SQUARE`].
pub fn polygon_signed_area(poly: &[[f64; 2]]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut acc = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        acc += a[0] * b[1] - b[0] * a[1];
    }
    acc / 2.0
}

/// Absolute area of a simple polygon.
pub fn polygon_area(poly: &[[f64; 2]]) -> f64 {
    polygon_signed_area(poly).abs()
}

/// Clip a convex polygon against the unit square.
///
/// Sutherland–Hodgman with the four axis-aligned half-planes. Correct for any
/// convex subject polygon, which is all Drop 1 produces: the image of a
/// rectangle under a non-degenerate homography that does not cross the line at
/// infinity is convex.
pub fn clip_convex_to_unit_square(poly: &[[f64; 2]]) -> Vec<[f64; 2]> {
    // Signed distance to each of the four half-planes: x >= 0, x <= 1, y >= 0,
    // y <= 1. Positive means inside.
    type HalfPlane = fn(&[f64; 2]) -> f64;
    let edges: [HalfPlane; 4] = [|p| p[0], |p| 1.0 - p[0], |p| p[1], |p| 1.0 - p[1]];
    let mut current: Vec<[f64; 2]> = poly.to_vec();
    for dist in edges {
        if current.is_empty() {
            return current;
        }
        let mut next: Vec<[f64; 2]> = Vec::with_capacity(current.len() + 1);
        for i in 0..current.len() {
            let a = current[i];
            let b = current[(i + 1) % current.len()];
            let da = dist(&a);
            let db = dist(&b);
            if da >= 0.0 {
                next.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                let denom = da - db;
                if denom.abs() > f64::EPSILON {
                    let t = da / denom;
                    next.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
                }
            }
        }
        current = next;
    }
    current
}

/// Map the four corners of the unit square through `m`.
///
/// Returns `None` if any corner crosses the line at infinity, which is itself
/// a plausibility failure.
pub fn map_unit_square(m: &Mat3) -> Option<[[f64; 2]; 4]> {
    let mut out = [[0.0; 2]; 4];
    for (i, corner) in UNIT_SQUARE.iter().enumerate() {
        out[i] = m.apply(*corner)?;
    }
    Some(out)
}

/// Geometric overlap between a photo and its anchor, both expressed in the
/// anchor's normalized frame (`SPEC.md` §5.3).
///
/// `photo_to_anchor` maps the photo's normalized coordinates into the anchor's.
/// The anchor rectangle is the unit square by construction. Dividing by the
/// smaller of the two areas *in that same frame* is what keeps the ratio inside
/// `[0, 1]` when the homography carries a large scale factor.
pub fn geometric_overlap(photo_to_anchor: &Mat3) -> Option<f64> {
    let quad = map_unit_square(photo_to_anchor)?;
    let photo_area = polygon_area(&quad);
    if photo_area <= 0.0 || !photo_area.is_finite() {
        return None;
    }
    let clipped = clip_convex_to_unit_square(&quad);
    if clipped.len() < 3 {
        return Some(0.0);
    }
    let inter = polygon_area(&clipped);
    let denom = photo_area.min(1.0);
    if denom <= 0.0 {
        return None;
    }
    Some((inter / denom).clamp(0.0, 1.0))
}

/// Convex hull of a point set (monotone chain), for the feature-coverage metric.
pub fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut pts = points.to_vec();
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup();
    if pts.len() < 3 {
        return pts;
    }

    fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    }

    let mut lower: Vec<[f64; 2]> = Vec::new();
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }

    #[test]
    fn row_major_order_and_direction_are_explicit() {
        // Translate by (2, 3): the translation lives in the third column.
        let m = Mat3([1.0, 0.0, 2.0, 0.0, 1.0, 3.0, 0.0, 0.0, 1.0]);
        let p = m.apply([1.0, 1.0]).unwrap();
        approx(p[0], 3.0);
        approx(p[1], 4.0);
    }

    #[test]
    fn inverse_round_trips() {
        let m = Mat3([1.2, 0.1, -3.0, -0.05, 0.9, 4.0, 1e-4, -2e-4, 1.0]);
        let inv = m.inverse().unwrap();
        let p = [0.31, 0.72];
        let q = m.apply(p).unwrap();
        let back = inv.apply(q).unwrap();
        assert!((back[0] - p[0]).abs() < 1e-9);
        assert!((back[1] - p[1]).abs() < 1e-9);
    }

    #[test]
    fn singular_matrix_has_no_inverse() {
        let m = Mat3([1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 1.0, 1.0, 1.0]);
        assert!(m.inverse().is_none());
    }

    #[test]
    fn from_row_major_rejects_non_finite_and_wrong_length() {
        assert!(Mat3::from_row_major(&[1.0; 8]).is_none());
        assert!(Mat3::from_row_major(&[f64::NAN; 9]).is_none());
        assert!(Mat3::from_row_major(&[1.0; 9]).is_some());
    }

    #[test]
    fn reflection_is_detectable() {
        let mirror = Mat3([-1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(mirror.affine_det() < 0.0);
        assert!(Mat3::identity().affine_det() > 0.0);
    }

    #[test]
    fn singular_values_recover_known_scale() {
        let m = Mat3::scaling(3.0, 0.5);
        let sv = m.affine_singular_values();
        approx(sv[0], 3.0);
        approx(sv[1], 0.5);
    }

    #[test]
    fn identity_overlap_is_one() {
        approx(geometric_overlap(&Mat3::identity()).unwrap(), 1.0);
    }

    #[test]
    fn half_shifted_overlap_is_half() {
        let shift = Mat3([1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        approx(geometric_overlap(&shift).unwrap(), 0.5);
    }

    #[test]
    fn disjoint_overlap_is_zero() {
        let shift = Mat3([1.0, 0.0, 5.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        approx(geometric_overlap(&shift).unwrap(), 0.0);
    }

    #[test]
    fn large_scale_overlap_cannot_exceed_one() {
        // The photo maps to a 4x-larger quad centred on the anchor: the anchor
        // is fully covered, so overlap is 1.0 and not 4.0.
        let m = Mat3([4.0, 0.0, -1.5, 0.0, 4.0, -1.5, 0.0, 0.0, 1.0]);
        approx(geometric_overlap(&m).unwrap(), 1.0);
    }

    #[test]
    fn small_scale_overlap_is_one_when_contained() {
        // Photo maps entirely inside the anchor: the smaller area is the photo's.
        let m = Mat3([0.5, 0.0, 0.25, 0.0, 0.5, 0.25, 0.0, 0.0, 1.0]);
        approx(geometric_overlap(&m).unwrap(), 1.0);
    }

    #[test]
    fn pixel_homography_rebases_to_normalized() {
        // Pure identity in pixels between images of different sizes becomes a
        // scale in normalized coordinates.
        let m = pixel_to_normalized(&Mat3::identity(), (1200, 900), (600, 450)).unwrap();
        let p = m.apply([1.0, 1.0]).unwrap();
        approx(p[0], 2.0);
        approx(p[1], 2.0);
    }

    #[test]
    fn normalized_homography_is_resolution_independent() {
        // Estimated at analysis resolution, applied at full resolution.
        let pixel = Mat3([1.0, 0.0, 60.0, 0.0, 1.0, -30.0, 0.0, 0.0, 1.0]);
        let norm = pixel_to_normalized(&pixel, (1200, 900), (1200, 900)).unwrap();
        // 60 analysis px of a 1200 px wide image is 5% of the width; at
        // 4800 px that is 240 px.
        let full = 4800.0;
        let mapped = norm.apply([0.5, 0.5]).unwrap();
        approx(mapped[0] * full, 0.5 * full + 0.05 * full);
    }

    #[test]
    fn a_quad_containing_the_unit_square_clips_to_the_whole_square() {
        // Diamond with L1 radius 1 about (0.5, 0.5): every square corner sits on
        // its boundary, so the intersection is the square itself.
        let quad = [[0.5, -0.5], [1.5, 0.5], [0.5, 1.5], [-0.5, 0.5]];
        approx(polygon_area(&clip_convex_to_unit_square(&quad)), 1.0);
    }

    #[test]
    fn an_inscribed_rotated_square_survives_clipping_unchanged() {
        // Corners at the edge midpoints: area is half the unit square and none of
        // it is outside, so clipping must not remove anything.
        let quad = [[0.5, 0.0], [1.0, 0.5], [0.5, 1.0], [0.0, 0.5]];
        approx(polygon_area(&clip_convex_to_unit_square(&quad)), 0.5);
    }

    #[test]
    fn a_diagonally_shifted_square_clips_to_a_quarter() {
        let quad = [[0.5, 0.5], [1.5, 0.5], [1.5, 1.5], [0.5, 1.5]];
        approx(polygon_area(&clip_convex_to_unit_square(&quad)), 0.25);
    }

    #[test]
    fn a_fully_outside_quad_clips_to_nothing() {
        let quad = [[2.0, 2.0], [3.0, 2.0], [3.0, 3.0], [2.0, 3.0]];
        assert!(clip_convex_to_unit_square(&quad).len() < 3);
    }

    #[test]
    fn convex_hull_of_a_square_grid_is_the_square() {
        let mut pts = Vec::new();
        for i in 0..5 {
            for j in 0..5 {
                pts.push([i as f64 / 4.0, j as f64 / 4.0]);
            }
        }
        let hull = convex_hull(&pts);
        approx(polygon_area(&hull), 1.0);
    }

    #[test]
    fn convex_hull_of_collinear_points_has_no_area() {
        let pts: Vec<[f64; 2]> = (0..10).map(|i| [i as f64 / 9.0, 0.4]).collect();
        let hull = convex_hull(&pts);
        approx(polygon_area(&hull), 0.0);
    }

    #[test]
    fn perspective_magnitude_flags_extreme_transforms() {
        let sane = Mat3([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.01, 0.02, 1.0]);
        let wild = Mat3([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.9, 0.0, 1.0]);
        assert!(sane.perspective_magnitude() < 0.1);
        assert!(wild.perspective_magnitude() > 0.5);
    }
}
