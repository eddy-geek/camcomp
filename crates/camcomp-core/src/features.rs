//! SIFT feature extraction behind [`FeatureExtractor`], plus the per-photo
//! feature cache.
//!
//! The crate's `ndarray` output is converted to camcomp types immediately, so
//! swapping the detector is a change confined to this file
//! (`ADR-DROP1.md` §6).

use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::{
    FeatureCacheKey, FeatureExtractor, GrayImage, Keypoint, PhotoFeatures, DESCRIPTOR_LEN,
};
use crate::Result;

/// Smallest analysis image SIFT is asked to process. Below this the scale space
/// has no useful octaves and the result would be noise, so it is reported as no
/// features rather than as a detection.
const MIN_ANALYSIS_EDGE: u32 = 32;

/// SIFT extractor backed by the `sift-features` crate.
#[derive(Debug, Clone, Copy)]
pub struct SiftExtractor {
    /// Upper bound on retained keypoints.
    pub features_limit: usize,
}

impl Default for SiftExtractor {
    fn default() -> Self {
        Self {
            features_limit: 4000,
        }
    }
}

impl SiftExtractor {
    /// Build with an explicit feature cap.
    pub fn new(features_limit: usize) -> Self {
        Self { features_limit }
    }
}

impl FeatureExtractor for SiftExtractor {
    fn identity(&self) -> String {
        "sift-features/0.1.0".to_string()
    }

    fn parameter_signature(&self) -> String {
        format!("limit={}", self.features_limit)
    }

    fn extract(&self, image: &GrayImage) -> Result<PhotoFeatures> {
        let started = std::time::Instant::now();
        let (w, h) = image.dimensions();
        if w < MIN_ANALYSIS_EDGE || h < MIN_ANALYSIS_EDGE {
            return Ok(PhotoFeatures {
                keypoints: Vec::new(),
                descriptors: Vec::new(),
                analysis_dimensions: (w, h),
                duration_ms: started.elapsed().as_millis() as u64,
            });
        }

        let buffer: image::GrayImage =
            image::ImageBuffer::from_raw(w, h, image.as_slice().to_vec())
                .expect("dimensions already validated");
        let result = sift_features::sift(&buffer, Some(self.features_limit));
        drop(buffer);

        let keypoints: Vec<Keypoint> = result
            .keypoints
            .iter()
            .map(|kp| Keypoint {
                x: kp.x,
                y: kp.y,
                size: kp.size,
                angle: kp.angle,
                response: kp.response,
            })
            .collect();

        let rows = result.descriptors.nrows();
        let mut descriptors = Vec::with_capacity(rows);
        for row in result.descriptors.rows() {
            let mut d = [0u8; DESCRIPTOR_LEN];
            for (slot, value) in d.iter_mut().zip(row.iter()) {
                *slot = *value;
            }
            descriptors.push(d);
        }

        // The crate documents one descriptor per keypoint; truncate to the
        // shorter of the two rather than trusting the invariant blindly, because
        // a mismatch would silently pair a keypoint with someone else's
        // descriptor.
        let n = keypoints.len().min(descriptors.len());
        Ok(PhotoFeatures {
            keypoints: keypoints[..n].to_vec(),
            descriptors: descriptors[..n].to_vec(),
            analysis_dimensions: (w, h),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// In-memory feature cache, keyed by content digest plus detector identity.
///
/// Deliberately not written to disk: the only file camcomp may create in the
/// source folder is `camcomp.json` (`REQUIREMENTS.md` §3), and a warm reload
/// needs no features at all because pair results are themselves cached.
#[derive(Default)]
pub struct FeatureCache {
    entries: HashMap<FeatureCacheKey, Arc<PhotoFeatures>>,
    hits: usize,
    misses: usize,
}

impl FeatureCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetch or compute the features for one photo.
    pub fn get_or_extract<E, F>(
        &mut self,
        key: FeatureCacheKey,
        extractor: &E,
        load_image: F,
    ) -> Result<Arc<PhotoFeatures>>
    where
        E: FeatureExtractor,
        F: FnOnce() -> Result<GrayImage>,
    {
        if let Some(found) = self.entries.get(&key) {
            self.hits += 1;
            return Ok(Arc::clone(found));
        }
        self.misses += 1;
        let image = load_image()?;
        let features = Arc::new(extractor.extract(&image)?);
        drop(image);
        self.entries.insert(key, Arc::clone(&features));
        Ok(features)
    }

    /// Insert a precomputed entry.
    pub fn insert(&mut self, key: FeatureCacheKey, features: Arc<PhotoFeatures>) {
        self.entries.insert(key, features);
    }

    /// Look up without computing.
    pub fn get(&self, key: &FeatureCacheKey) -> Option<Arc<PhotoFeatures>> {
        self.entries.get(key).map(Arc::clone)
    }

    /// Number of cache hits so far.
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// Number of cache misses so far.
    pub fn misses(&self) -> usize {
        self.misses
    }

    /// Drop every entry, releasing the descriptors.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Entries currently held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Build a cache key from a content digest and an extractor.
pub fn cache_key<E: FeatureExtractor>(sha256: &str, extractor: &E) -> FeatureCacheKey {
    FeatureCacheKey {
        sha256: sha256.to_string(),
        detector: extractor.identity(),
        parameters: extractor.parameter_signature(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// Textured test pattern: enough corners for SIFT to find something.
    fn textured(w: u32, h: u32) -> GrayImage {
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let a = ((x as f32 * 0.21).sin() * (y as f32 * 0.17).cos() * 110.0) as i32;
                let b = if (x / 11 + y / 13) % 2 == 0 { 150 } else { 60 };
                data.push((b + a).clamp(0, 255) as u8);
            }
        }
        GrayImage::new(w, h, data).unwrap()
    }

    #[test]
    fn extraction_finds_features_in_a_textured_image() {
        let features = SiftExtractor::new(500)
            .extract(&textured(192, 144))
            .unwrap();
        assert!(features.len() > 20, "only {} features", features.len());
        assert_eq!(features.keypoints.len(), features.descriptors.len());
        assert_eq!(features.analysis_dimensions, (192, 144));
    }

    #[test]
    fn the_feature_cap_is_honoured() {
        let features = SiftExtractor::new(25).extract(&textured(256, 192)).unwrap();
        assert!(features.len() <= 25, "{} features", features.len());
    }

    #[test]
    fn a_flat_image_yields_almost_nothing_rather_than_failing() {
        let features = SiftExtractor::new(500)
            .extract(&GrayImage::zeros(128, 128))
            .unwrap();
        assert!(
            features.len() < 10,
            "{} features on a flat image",
            features.len()
        );
    }

    #[test]
    fn a_tiny_image_is_reported_as_no_features() {
        let features = SiftExtractor::default()
            .extract(&GrayImage::zeros(8, 8))
            .unwrap();
        assert!(features.is_empty());
    }

    #[test]
    fn extraction_is_deterministic() {
        let img = textured(160, 120);
        let a = SiftExtractor::new(300).extract(&img).unwrap();
        let b = SiftExtractor::new(300).extract(&img).unwrap();
        assert_eq!(a.keypoints, b.keypoints);
        assert_eq!(a.descriptors, b.descriptors);
    }

    #[test]
    fn the_cache_extracts_once_per_key() {
        let mut cache = FeatureCache::new();
        let extractor = SiftExtractor::new(100);
        let calls = Cell::new(0);
        let key = cache_key("abc", &extractor);
        for _ in 0..3 {
            cache
                .get_or_extract(key.clone(), &extractor, || {
                    calls.set(calls.get() + 1);
                    Ok(textured(96, 96))
                })
                .unwrap();
        }
        assert_eq!(calls.get(), 1);
        assert_eq!(cache.hits(), 2);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn changing_the_detector_parameters_invalidates_the_cache() {
        let a = cache_key("abc", &SiftExtractor::new(100));
        let b = cache_key("abc", &SiftExtractor::new(200));
        assert_ne!(a, b);
        let c = cache_key("def", &SiftExtractor::new(100));
        assert_ne!(a, c);
    }

    #[test]
    fn a_failing_loader_propagates_and_does_not_poison_the_cache() {
        let mut cache = FeatureCache::new();
        let extractor = SiftExtractor::new(100);
        let key = cache_key("abc", &extractor);
        let err = cache
            .get_or_extract(key.clone(), &extractor, || {
                Err(crate::Error::InvalidParameters("boom".into()))
            })
            .unwrap_err();
        assert_eq!(err.code(), "invalid-parameters");
        assert!(cache.get(&key).is_none());
    }
}
