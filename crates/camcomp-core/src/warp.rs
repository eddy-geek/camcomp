//! Projective warping and the gradient-based appearance metric.
//!
//! Written in-house rather than delegated: the border mask is not an
//! afterthought here, it is the thing that stops non-overlapping pixels from
//! inflating the similarity score (`PLAN-DROP1.md` §13.1).

use crate::domain::{GrayImage, ImageWarper};
use crate::geometry::Mat3;

/// Inverse-mapping bilinear warper.
#[derive(Debug, Clone, Copy, Default)]
pub struct BilinearWarper;

impl ImageWarper for BilinearWarper {
    fn warp(
        &self,
        source: &GrayImage,
        src_to_dst: &Mat3,
        dst_dims: (u32, u32),
    ) -> (GrayImage, Vec<bool>) {
        let (dw, dh) = dst_dims;
        let mut out = GrayImage::zeros(dw, dh);
        let mut mask = vec![false; dw as usize * dh as usize];
        let Some(inverse) = src_to_dst.inverse() else {
            return (out, mask);
        };
        let dst = out.as_mut_slice();
        for y in 0..dh {
            for x in 0..dw {
                // Sample at pixel centres so a 1:1 transform is an exact copy.
                let p = [x as f64, y as f64];
                if let Some(sp) = inverse.apply(p) {
                    if let Some(value) = source.sample_bilinear(sp[0], sp[1]) {
                        let idx = y as usize * dw as usize + x as usize;
                        dst[idx] = value.round().clamp(0.0, 255.0) as u8;
                        mask[idx] = true;
                    }
                }
            }
        }
        (out, mask)
    }
}

/// Sobel gradient magnitude, normalized to `[0, 1]`.
///
/// Border pixels have no full `3 × 3` neighbourhood and are reported as invalid
/// rather than as zero gradient, which would otherwise read as a strong
/// agreement between two images that happen to both have borders.
pub fn gradient_magnitude(image: &GrayImage) -> (Vec<f32>, Vec<bool>) {
    let (w, h) = image.dimensions();
    let n = w as usize * h as usize;
    let mut out = vec![0.0f32; n];
    let mut valid = vec![false; n];
    if w < 3 || h < 3 {
        return (out, valid);
    }
    let src = image.as_slice();
    let wu = w as usize;
    for y in 1..(h as usize - 1) {
        for x in 1..(wu - 1) {
            let p = |dx: isize, dy: isize| -> f32 {
                src[(y as isize + dy) as usize * wu + (x as isize + dx) as usize] as f32
            };
            let gx = -p(-1, -1) - 2.0 * p(-1, 0) - p(-1, 1) + p(1, -1) + 2.0 * p(1, 0) + p(1, 1);
            let gy = -p(-1, -1) - 2.0 * p(0, -1) - p(1, -1) + p(-1, 1) + 2.0 * p(0, 1) + p(1, 1);
            let idx = y * wu + x;
            // Maximum Sobel response for 8-bit input is 4 * 255 per axis.
            out[idx] = ((gx * gx + gy * gy).sqrt() / (4.0 * 255.0 * std::f32::consts::SQRT_2))
                .clamp(0.0, 1.0);
            valid[idx] = true;
        }
    }
    (out, valid)
}

/// Zero-mean normalized cross-correlation over the pixels both masks accept,
/// remapped from `[-1, 1]` to `[0, 1]`.
///
/// `None` when the two images have too little common support to mean anything.
pub fn masked_zncc(
    a: &[f32],
    a_valid: &[bool],
    b: &[f32],
    b_valid: &[bool],
    min_samples: usize,
) -> Option<f64> {
    if a.len() != b.len() || a.len() != a_valid.len() || b.len() != b_valid.len() {
        return None;
    }
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for i in 0..a.len() {
        if a_valid[i] && b_valid[i] {
            xs.push(a[i] as f64);
            ys.push(b[i] as f64);
        }
    }
    if xs.len() < min_samples {
        return None;
    }
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den_x = 0.0;
    let mut den_y = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        num += dx * dy;
        den_x += dx * dx;
        den_y += dy * dy;
    }
    let den = (den_x * den_y).sqrt();
    if den <= 1e-12 {
        // One side is perfectly flat: no structure to compare, so this is
        // absence of evidence rather than a match.
        return None;
    }
    let zncc = num / den;
    if !zncc.is_finite() {
        return None;
    }
    Some(((zncc + 1.0) / 2.0).clamp(0.0, 1.0))
}

/// Global SSIM over the pixels both masks accept. Diagnostic only
/// (`SPEC.md` §5.3).
pub fn masked_ssim(
    a: &GrayImage,
    a_valid: &[bool],
    b: &GrayImage,
    b_valid: &[bool],
    min_samples: usize,
) -> Option<f64> {
    if a.dimensions() != b.dimensions() {
        return None;
    }
    let sa = a.as_slice();
    let sb = b.as_slice();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for i in 0..sa.len() {
        if a_valid[i] && b_valid[i] {
            xs.push(sa[i] as f64);
            ys.push(sb[i] as f64);
        }
    }
    if xs.len() < min_samples {
        return None;
    }
    let n = xs.len() as f64;
    let mx = xs.iter().sum::<f64>() / n;
    let my = ys.iter().sum::<f64>() / n;
    let vx = xs.iter().map(|v| (v - mx) * (v - mx)).sum::<f64>() / n;
    let vy = ys.iter().map(|v| (v - my) * (v - my)).sum::<f64>() / n;
    let cov = xs
        .iter()
        .zip(ys.iter())
        .map(|(x, y)| (x - mx) * (y - my))
        .sum::<f64>()
        / n;
    let c1 = (0.01 * 255.0f64).powi(2);
    let c2 = (0.03 * 255.0f64).powi(2);
    let ssim =
        ((2.0 * mx * my + c1) * (2.0 * cov + c2)) / ((mx * mx + my * my + c1) * (vx + vy + c2));
    if ssim.is_finite() {
        Some(ssim.clamp(-1.0, 1.0))
    } else {
        None
    }
}

/// Intersect two validity masks.
pub fn intersect_masks(a: &[bool], b: &[bool]) -> Vec<bool> {
    a.iter().zip(b.iter()).map(|(x, y)| *x && *y).collect()
}

/// Shrink a validity mask by `radius` pixels.
///
/// Needed because a warp introduces a hard edge at the boundary of the valid
/// region. That edge is an artefact of the warp, not a feature of the scene, and
/// a gradient metric would happily match it against the other image's border.
pub fn erode_mask(mask: &[bool], dims: (u32, u32), radius: u32) -> Vec<bool> {
    let (w, h) = dims;
    if radius == 0 || mask.len() != w as usize * h as usize {
        return mask.to_vec();
    }
    let mut out = vec![false; mask.len()];
    let r = radius as i64;
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let idx = (y * w as i64 + x) as usize;
            if !mask[idx] {
                continue;
            }
            let mut keep = true;
            'window: for dy in -r..=r {
                for dx in -r..=r {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        keep = false;
                        break 'window;
                    }
                    if !mask[(ny * w as i64 + nx) as usize] {
                        keep = false;
                        break 'window;
                    }
                }
            }
            out[idx] = keep;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32, cell: u32) -> GrayImage {
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let on = ((x / cell) + (y / cell)).is_multiple_of(2);
                data.push(if on { 220 } else { 40 });
            }
        }
        GrayImage::new(w, h, data).unwrap()
    }

    #[test]
    fn identity_warp_reproduces_the_source() {
        let src = checker(32, 24, 4);
        let (out, mask) = BilinearWarper.warp(&src, &Mat3::identity(), src.dimensions());
        assert_eq!(out.as_slice(), src.as_slice());
        assert!(mask.iter().all(|m| *m));
    }

    #[test]
    fn a_shift_leaves_an_invalid_border() {
        let src = checker(32, 24, 4);
        // Shift right by 5 px: the leftmost 5 columns have no source pixel.
        let shift = Mat3([1.0, 0.0, 5.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let (_out, mask) = BilinearWarper.warp(&src, &shift, src.dimensions());
        for y in 0..24usize {
            for x in 0..5usize {
                assert!(!mask[y * 32 + x], "({x},{y}) should be outside the source");
            }
            assert!(mask[y * 32 + 10]);
        }
    }

    #[test]
    fn a_singular_transform_produces_an_empty_mask() {
        let src = checker(16, 16, 4);
        let singular = Mat3([1.0, 2.0, 3.0, 2.0, 4.0, 6.0, 0.0, 0.0, 1.0]);
        let (_out, mask) = BilinearWarper.warp(&src, &singular, (16, 16));
        assert!(mask.iter().all(|m| !*m));
    }

    #[test]
    fn a_warp_round_trip_recovers_the_interior() {
        let src = checker(48, 36, 6);
        let m = Mat3([1.0, 0.0, 3.0, 0.0, 1.0, -2.0, 0.0, 0.0, 1.0]);
        let (warped, _) = BilinearWarper.warp(&src, &m, src.dimensions());
        let (back, mask) = BilinearWarper.warp(&warped, &m.inverse().unwrap(), src.dimensions());
        let mut compared = 0;
        for y in 6..30u32 {
            for x in 6..42u32 {
                if mask[(y * 48 + x) as usize] {
                    let a = src.get(x, y).unwrap() as i32;
                    let b = back.get(x, y).unwrap() as i32;
                    assert!((a - b).abs() <= 2, "({x},{y}) {a} vs {b}");
                    compared += 1;
                }
            }
        }
        assert!(compared > 500);
    }

    #[test]
    fn gradient_marks_borders_invalid() {
        let (grad, valid) = gradient_magnitude(&checker(8, 8, 2));
        assert!(!valid[0]);
        assert!(valid[8 + 1]);
        assert_eq!(grad.len(), 64);
        assert!(grad.iter().all(|g| (0.0..=1.0).contains(g)));
    }

    #[test]
    fn gradient_of_a_tiny_image_is_entirely_invalid() {
        let (_g, valid) = gradient_magnitude(&GrayImage::zeros(2, 2));
        assert!(valid.iter().all(|v| !*v));
    }

    #[test]
    fn zncc_is_one_for_identical_structure() {
        let (g, v) = gradient_magnitude(&checker(32, 32, 4));
        let s = masked_zncc(&g, &v, &g, &v, 10).unwrap();
        assert!((s - 1.0).abs() < 1e-9, "{s}");
    }

    #[test]
    fn zncc_survives_an_exposure_and_contrast_change() {
        let src = checker(48, 48, 6);
        // Simulate a different camera's tone curve: scale and offset.
        let mut brighter = src.clone();
        for v in brighter.as_mut_slice() {
            *v = ((*v as f32) * 0.6 + 60.0).round().clamp(0.0, 255.0) as u8;
        }
        let (ga, va) = gradient_magnitude(&src);
        let (gb, vb) = gradient_magnitude(&brighter);
        let s = masked_zncc(&ga, &va, &gb, &vb, 10).unwrap();
        assert!(s > 0.9, "exposure change should not destroy the score: {s}");
    }

    #[test]
    fn zncc_is_low_for_unrelated_structure() {
        let (ga, va) = gradient_magnitude(&checker(64, 64, 4));
        let mut noise = GrayImage::zeros(64, 64);
        let mut state = 0x1234_5678u32;
        for v in noise.as_mut_slice() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            *v = (state >> 24) as u8;
        }
        let (gb, vb) = gradient_magnitude(&noise);
        let s = masked_zncc(&ga, &va, &gb, &vb, 10).unwrap();
        assert!(s < 0.7, "unrelated structure scored {s}");
    }

    #[test]
    fn zncc_refuses_a_flat_image_instead_of_scoring_it() {
        let (ga, va) = gradient_magnitude(&checker(32, 32, 4));
        let (gb, vb) = gradient_magnitude(&GrayImage::zeros(32, 32));
        assert!(masked_zncc(&ga, &va, &gb, &vb, 10).is_none());
    }

    #[test]
    fn zncc_requires_a_minimum_sample_count() {
        let (g, v) = gradient_magnitude(&checker(8, 8, 2));
        assert!(masked_zncc(&g, &v, &g, &v, 10_000).is_none());
    }

    #[test]
    fn non_overlap_cannot_inflate_the_score() {
        // Two structured images whose valid regions do not intersect: there is
        // no evidence at all, and the metric must say so.
        let (ga, mut va) = gradient_magnitude(&checker(32, 32, 4));
        let (gb, mut vb) = gradient_magnitude(&checker(32, 32, 4));
        for i in 0..va.len() {
            if i % 2 == 0 {
                vb[i] = false;
            } else {
                va[i] = false;
            }
        }
        assert!(masked_zncc(&ga, &va, &gb, &vb, 10).is_none());
    }

    #[test]
    fn erosion_removes_the_boundary_ring() {
        let mut mask = vec![false; 10 * 10];
        for y in 3..7 {
            for x in 3..7 {
                mask[y * 10 + x] = true;
            }
        }
        let eroded = erode_mask(&mask, (10, 10), 1);
        // A 4x4 block eroded by 1 leaves a 2x2 core.
        assert_eq!(eroded.iter().filter(|m| **m).count(), 4);
        assert!(eroded[4 * 10 + 4]);
        assert!(!eroded[3 * 10 + 3]);
    }

    #[test]
    fn erosion_treats_the_image_edge_as_outside() {
        let mask = vec![true; 5 * 5];
        let eroded = erode_mask(&mask, (5, 5), 1);
        assert_eq!(eroded.iter().filter(|m| **m).count(), 9);
    }

    #[test]
    fn erosion_with_zero_radius_is_identity() {
        let mask = vec![true, false, true, true];
        assert_eq!(erode_mask(&mask, (2, 2), 0), mask);
    }

    #[test]
    fn a_warp_boundary_edge_does_not_survive_erosion() {
        let src = checker(64, 64, 8);
        let shift = Mat3([1.0, 0.0, 20.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
        let (warped, mask) = BilinearWarper.warp(&src, &shift, src.dimensions());
        let (_g, grad_valid) = gradient_magnitude(&warped);
        let combined = intersect_masks(&erode_mask(&mask, (64, 64), 2), &grad_valid);
        // Column 19 is the last invalid one; column 20 is the boundary and must
        // be excluded, column 25 is well inside and must be kept.
        for y in 10..50usize {
            assert!(!combined[y * 64 + 20]);
            assert!(combined[y * 64 + 25]);
        }
    }

    #[test]
    fn ssim_of_an_image_with_itself_is_one() {
        let img = checker(32, 32, 4);
        let mask = vec![true; 32 * 32];
        let s = masked_ssim(&img, &mask, &img, &mask, 10).unwrap();
        assert!((s - 1.0).abs() < 1e-6, "{s}");
    }
}
