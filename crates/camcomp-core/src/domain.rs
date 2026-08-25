//! Camcomp-owned runtime types and the traits that isolate the CV dependencies
//! (`ADR-DROP1.md` §6).
//!
//! Nothing in a public signature here mentions `image`, `ndarray` or
//! `nalgebra`. Swapping `sift-features` for OpenCV bindings is meant to be a
//! change of one adapter, not a re-architecture.

use crate::geometry::Mat3;
use crate::schema::Fingerprint;

/// Length of a SIFT descriptor.
pub const DESCRIPTOR_LEN: usize = 128;

/// A single-channel 8-bit image in camcomp's own representation.
///
/// Row-major, `width * height` samples, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayImage {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl GrayImage {
    /// Wrap raw samples. Returns `None` if the length does not match.
    pub fn new(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        if (width as usize).checked_mul(height as usize)? != data.len() {
            return None;
        }
        Some(Self {
            width,
            height,
            data,
        })
    }

    /// A black image.
    pub fn zeros(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; width as usize * height as usize],
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Dimensions as `(width, height)`.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Raw samples, row-major.
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    /// Mutable raw samples, row-major.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Sample at `(x, y)`, or `None` when out of bounds.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Option<u8> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.data[y as usize * self.width as usize + x as usize])
    }

    /// Bilinear sample at continuous coordinates, or `None` when the sample
    /// window leaves the image.
    pub fn sample_bilinear(&self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let x0 = x.floor();
        let y0 = y.floor();
        let x1 = x0 + 1.0;
        let y1 = y0 + 1.0;
        if x1 > (self.width - 1) as f64 || y1 > (self.height - 1) as f64 {
            // Fall back to nearest inside the last row/column.
            let xi = x.round() as i64;
            let yi = y.round() as i64;
            if xi < 0 || yi < 0 || xi >= self.width as i64 || yi >= self.height as i64 {
                return None;
            }
            return Some(self.data[yi as usize * self.width as usize + xi as usize] as f64);
        }
        let fx = x - x0;
        let fy = y - y0;
        let (x0, y0, x1, y1) = (x0 as usize, y0 as usize, x1 as usize, y1 as usize);
        let w = self.width as usize;
        let p00 = self.data[y0 * w + x0] as f64;
        let p10 = self.data[y0 * w + x1] as f64;
        let p01 = self.data[y1 * w + x0] as f64;
        let p11 = self.data[y1 * w + x1] as f64;
        let top = p00 + (p10 - p00) * fx;
        let bottom = p01 + (p11 - p01) * fx;
        Some(top + (bottom - top) * fy)
    }
}

/// A detected local feature, in analysis-image pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keypoint {
    /// X in analysis pixels.
    pub x: f32,
    /// Y in analysis pixels.
    pub y: f32,
    /// Feature diameter in analysis pixels.
    pub size: f32,
    /// Dominant orientation, in degrees.
    pub angle: f32,
    /// Detector response.
    pub response: f32,
}

/// Feature-extraction output for one photo, in camcomp's own types.
#[derive(Debug, Clone, PartialEq)]
pub struct PhotoFeatures {
    /// Detected keypoints.
    pub keypoints: Vec<Keypoint>,
    /// One 128-byte descriptor per keypoint, in the same order.
    pub descriptors: Vec<[u8; DESCRIPTOR_LEN]>,
    /// Dimensions of the analysis image the features were detected in.
    pub analysis_dimensions: (u32, u32),
    /// Extraction wall time.
    pub duration_ms: u64,
}

impl PhotoFeatures {
    /// Number of features.
    pub fn len(&self) -> usize {
        self.keypoints.len()
    }

    /// Whether no feature was detected.
    pub fn is_empty(&self) -> bool {
        self.keypoints.is_empty()
    }
}

/// Cache key for feature extraction: content identity plus detector identity
/// (`PLAN-DROP1.md` §11.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeatureCacheKey {
    /// SHA-256 of the source file.
    pub sha256: String,
    /// Detector name and version.
    pub detector: String,
    /// Detector parameters that change the output.
    pub parameters: String,
}

/// A surviving descriptor match.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureMatch {
    /// Index into the first photo's features.
    pub a: usize,
    /// Index into the second photo's features.
    pub b: usize,
    /// Squared L2 distance to the nearest neighbour.
    pub distance: f32,
    /// Ratio of nearest to second-nearest distance. Lower is more distinctive.
    pub ratio: f32,
}

/// A fitted transform plus its consensus set.
#[derive(Debug, Clone, PartialEq)]
pub struct Registration {
    /// Transform mapping first-image analysis pixels to second-image analysis
    /// pixels.
    pub homography: Mat3,
    /// Indices into the match list that agreed with the transform.
    pub inliers: Vec<usize>,
    /// Per-inlier reprojection error, in analysis pixels, same order as
    /// [`Self::inliers`].
    pub reprojection_errors_px: Vec<f64>,
}

/// Decoded, oriented, reduced image plus the dimensions needed to convert any
/// analysis coordinate back to source pixels.
#[derive(Debug, Clone)]
pub struct AnalysisImage {
    /// Grayscale analysis image.
    pub gray: GrayImage,
    /// Dimensions as stored in the file.
    pub original_dimensions: (u32, u32),
    /// Dimensions after orientation.
    pub oriented_dimensions: (u32, u32),
    /// EXIF orientation found in the file (`1` when absent or invalid).
    pub exif_orientation: u16,
    /// `true` when the decoder already applied the container's orientation.
    pub orientation_applied_by_decoder: bool,
}

/// Extracts local features from an analysis image.
pub trait FeatureExtractor {
    /// Detector name and version, part of the feature cache key.
    fn identity(&self) -> String;
    /// Parameters that change the output, part of the feature cache key.
    fn parameter_signature(&self) -> String;
    /// Detect and describe.
    fn extract(&self, image: &GrayImage) -> crate::Result<PhotoFeatures>;
}

/// Matches two descriptor sets.
pub trait DescriptorMatcher {
    /// Return the surviving matches, deterministically ordered.
    fn match_descriptors(&self, a: &PhotoFeatures, b: &PhotoFeatures) -> Vec<FeatureMatch>;
}

/// Fits a homography robustly.
pub trait HomographyEstimator {
    /// Fit `src -> dst`. `None` when no consensus was found.
    fn estimate(&self, src: &[[f64; 2]], dst: &[[f64; 2]]) -> Option<Registration>;
}

/// Warps an image through a projective transform.
pub trait ImageWarper {
    /// Warp `source` into the frame of an image of size `dst_dims`.
    ///
    /// `src_to_dst` maps source pixels to destination pixels. Returns the warped
    /// samples and a validity mask that is `true` only where a real source pixel
    /// was available, so border pixels cannot inflate a similarity score.
    fn warp(
        &self,
        source: &GrayImage,
        src_to_dst: &Mat3,
        dst_dims: (u32, u32),
    ) -> (GrayImage, Vec<bool>);
}

/// Reads metadata from a source file.
pub trait MetadataReader {
    /// Extract what is available. Missing fields are represented explicitly, not
    /// guessed.
    fn read(&self, bytes: &[u8]) -> RawMetadata;
}

/// Metadata exactly as found, before normalization and clock correction.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawMetadata {
    /// `DateTimeOriginal`, `CreateDate` or `DateTime`, whichever was found
    /// first, as a naive `YYYY-MM-DDTHH:MM:SS` string.
    pub date_time: Option<String>,
    /// Which tag [`Self::date_time`] came from.
    pub date_time_tag: Option<&'static str>,
    /// `OffsetTimeOriginal` in minutes east of UTC.
    pub utc_offset_minutes: Option<i32>,
    /// Sub-second digits of the capture time, for deterministic ordering.
    pub sub_second: Option<u32>,
    /// EXIF `Orientation`. `0` and values above `8` are invalid.
    pub orientation: Option<u16>,
    /// EXIF `Make`.
    pub make: Option<String>,
    /// EXIF `Model`.
    pub model: Option<String>,
    /// EXIF `LensModel`.
    pub lens_model: Option<String>,
    /// EXIF `Software`.
    pub software: Option<String>,
    /// EXIF `ImageDescription`.
    pub image_description: Option<String>,
    /// EXIF `UserComment`, ASCII payload only.
    pub user_comment: Option<String>,
    /// EXIF `FocalLength`, in millimetres.
    pub focal_length_mm: Option<f64>,
    /// EXIF `FocalLengthIn35mmFilm`, in millimetres.
    pub focal_length_35mm: Option<f64>,
    /// EXIF `DigitalZoomRatio`.
    pub digital_zoom_ratio: Option<f64>,
    /// Pixel dimensions from EXIF, when present.
    pub exif_dimensions: Option<(u32, u32)>,
    /// Non-fatal observations while parsing.
    pub notes: Vec<String>,
}

/// Everything the pipeline knows about one input file before pairing.
#[derive(Debug, Clone)]
pub struct InspectedPhoto {
    /// Stable analysis ID.
    pub id: String,
    /// Path relative to the source root, `/`-separated.
    pub relative_path: String,
    /// Absolute path on disk.
    pub absolute_path: std::path::PathBuf,
    /// Content identity and change-detection fields.
    pub fingerprint: Fingerprint,
    /// Normalized metadata.
    pub metadata: crate::schema::PhotoMetadata,
    /// Configuration this photo belongs to.
    pub configuration_id: String,
    /// Device identity without the mode.
    pub device_key: String,
    /// Dimensions as stored.
    pub original_dimensions: (u32, u32),
    /// Dimensions after orientation.
    pub oriented_dimensions: (u32, u32),
    /// Corrected capture instant, as a Unix timestamp with nanosecond part.
    pub corrected_instant: Option<(i64, u32)>,
    /// Non-fatal observations.
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gray_image_rejects_a_mismatched_buffer() {
        assert!(GrayImage::new(2, 2, vec![0; 3]).is_none());
        assert!(GrayImage::new(2, 2, vec![0; 4]).is_some());
    }

    #[test]
    fn bilinear_sampling_interpolates_and_clamps() {
        let img = GrayImage::new(2, 2, vec![0, 100, 200, 255]).unwrap();
        assert_eq!(img.sample_bilinear(0.0, 0.0), Some(0.0));
        assert_eq!(img.sample_bilinear(1.0, 0.0), Some(100.0));
        let mid = img.sample_bilinear(0.5, 0.0).unwrap();
        assert!((mid - 50.0).abs() < 1e-9);
        assert!(img.sample_bilinear(-0.1, 0.0).is_none());
        assert!(img.sample_bilinear(5.0, 0.0).is_none());
    }

    #[test]
    fn out_of_bounds_reads_are_none() {
        let img = GrayImage::zeros(3, 2);
        assert_eq!(img.get(2, 1), Some(0));
        assert_eq!(img.get(3, 1), None);
        assert_eq!(img.get(2, 2), None);
    }
}
