//! Session detection, focal compatibility and candidate-pair generation.
//!
//! This is the cheap half of the pipeline and the only defence against the
//! quadratic cost of the expensive half. Everything here is deterministic under
//! input permutation: device clocks have one-second resolution and photos from
//! different devices routinely share a timestamp, so ordering uses the documented
//! tie-break (corrected time, then configuration, then analysis ID)
//! (`SPEC.md` §4.1).

use crate::domain::InspectedPhoto;
use crate::schema::{FocalComparison, FocalCompatibility};

/// A detected session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionMembers {
    /// Stable session ID, `session-001` and up in chronological order.
    pub id: String,
    /// Members, in corrected capture order.
    pub photo_ids: Vec<String>,
    /// First corrected instant, as `(seconds, nanoseconds)`.
    pub start: (i64, u32),
    /// Last corrected instant.
    pub end: (i64, u32),
}

/// Result of session detection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionAssignment {
    /// Sessions, chronological.
    pub sessions: Vec<SessionMembers>,
    /// Photos with no usable timestamp. These get no session and must be grouped
    /// manually; they are never silently dropped.
    pub without_timestamp: Vec<String>,
}

/// Sort key implementing the documented tie-break.
fn order_key(photo: &InspectedPhoto) -> ((i64, u32), &str, &str) {
    (
        photo.corrected_instant.unwrap_or((i64::MIN, 0)),
        photo.configuration_id.as_str(),
        photo.id.as_str(),
    )
}

/// Group photos into sessions using the sliding-gap rule.
///
/// A new session begins when the gap from the *previous* photo exceeds the gap
/// threshold. The rule has no upper bound on total session length: a chain of
/// short gaps keeps one session open (`SPEC.md` §4.1).
pub fn detect_sessions(photos: &[InspectedPhoto], gap_seconds: u32) -> SessionAssignment {
    let mut timed: Vec<&InspectedPhoto> = photos
        .iter()
        .filter(|p| p.corrected_instant.is_some())
        .collect();
    timed.sort_by(|a, b| order_key(a).cmp(&order_key(b)));

    let mut without_timestamp: Vec<String> = photos
        .iter()
        .filter(|p| p.corrected_instant.is_none())
        .map(|p| p.id.clone())
        .collect();
    without_timestamp.sort();

    let mut sessions: Vec<SessionMembers> = Vec::new();
    let gap = gap_seconds as i64;
    for photo in timed {
        let instant = photo.corrected_instant.expect("filtered above");
        let start_new = match sessions.last() {
            None => true,
            Some(current) => instant.0 - current.end.0 > gap,
        };
        if start_new {
            sessions.push(SessionMembers {
                id: String::new(),
                photo_ids: vec![photo.id.clone()],
                start: instant,
                end: instant,
            });
        } else {
            let current = sessions.last_mut().expect("non-empty");
            current.photo_ids.push(photo.id.clone());
            current.end = instant;
        }
    }
    for (i, session) in sessions.iter_mut().enumerate() {
        session.id = format!("session-{:03}", i + 1);
    }
    SessionAssignment {
        sessions,
        without_timestamp,
    }
}

/// Symmetric relative focal difference, `abs(a - b) / min(a, b)`.
///
/// Symmetric by construction: using the smaller value as the denominator means
/// the predicate cannot depend on argument order (`SPEC.md` §4.2). Recorded for
/// explanation; the predicate itself compares angles of view.
pub fn relative_focal_difference(a: f64, b: f64) -> Option<f64> {
    if !a.is_finite() || !b.is_finite() || a <= 0.0 || b <= 0.0 {
        return None;
    }
    Some((a - b).abs() / a.min(b))
}

/// Diagonal of the 36 × 24 mm frame the 35 mm equivalent refers to.
const REFERENCE_DIAGONAL_MM: f64 = 43.266_615_305_567_87;

/// Diagonal angle of view, in degrees, for a 35 mm equivalent focal length.
pub fn diagonal_angle_of_view_degrees(focal_35mm: f64) -> Option<f64> {
    if !focal_35mm.is_finite() || focal_35mm <= 0.0 {
        return None;
    }
    Some(
        2.0 * (REFERENCE_DIAGONAL_MM / (2.0 * focal_35mm))
            .atan()
            .to_degrees(),
    )
}

/// Symmetric relative difference of the two diagonal angles of view.
///
/// This, not the focal ratio, is the framing comparison (`SPEC.md` §4.2). The
/// two are not interchangeable: angle of view is `2·atan(d / 2f)`, so a given
/// focal ratio is a small angular difference at the wide end and nearly the whole
/// of it at the long end. 15 mm against 12 mm is a 25 % focal ratio but an 10 %
/// angular difference — two ultrawides framing much the same scene — while 28 mm
/// against 24 mm is a 17 % focal ratio and a 12 % angular difference, a framing
/// the photographer deliberately changed. Only the angular measure orders those
/// two the way the labelled sets do (`IMPLEM-DROP1.md` §6.11).
pub fn relative_fov_difference(a: f64, b: f64) -> Option<f64> {
    let a = diagonal_angle_of_view_degrees(a)?;
    let b = diagonal_angle_of_view_degrees(b)?;
    Some((a - b).abs() / a.min(b))
}

/// What the framing predicate needs to know about one photo.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FocalDescriptor {
    /// 35 mm equivalent focal length after the zoom policy, when known.
    pub effective_35mm: Option<f64>,
    /// `true` when the lens was at its native field of view.
    pub native_lens: bool,
}

impl FocalDescriptor {
    /// Read the descriptor off persisted metadata.
    pub fn from_metadata(metadata: &crate::schema::PhotoMetadata) -> Self {
        Self {
            // Documents written before the effective value existed only have the
            // reported one, and for them the two are the same thing.
            effective_35mm: metadata
                .effective_focal_length_35mm
                .or(metadata.focal_length_35mm),
            native_lens: metadata.native_lens,
        }
    }
}

/// The tolerance that applies to a pair (`SPEC.md` §4.2).
///
/// Two lenses at their native field of view are compared under the relaxed
/// tolerance, because "both phones' ultrawide" is a comparison the product
/// exists to make even though the hardware fields of view differ. As soon as
/// either side zoomed or cropped, the framing was a choice rather than a property
/// of the lens, and the strict tolerance applies.
fn tolerance_for(
    a: &FocalDescriptor,
    b: &FocalDescriptor,
    tolerance: f64,
    native: f64,
) -> (f64, bool) {
    if a.native_lens && b.native_lens {
        (tolerance.max(native), true)
    } else {
        (tolerance, false)
    }
}

/// Compare the framing of two photos from their 35 mm equivalent focal lengths.
///
/// The comparison is on the diagonal angle of view, not on the focal ratio, and
/// under one of two tolerances depending on whether both lenses were at their
/// native field of view.
///
/// An unknown focal length yields [`FocalCompatibility::Unknown`], which does not
/// exclude the pair from visual analysis: the visual evidence is the stronger
/// signal. Physical focal lengths are never compared across devices.
pub fn focal_comparison(
    a: &FocalDescriptor,
    b: &FocalDescriptor,
    tolerance: f64,
    native_lens_tolerance: f64,
) -> FocalComparison {
    let (applied, native_pair) = tolerance_for(a, b, tolerance, native_lens_tolerance);
    let unknown = FocalComparison {
        state: FocalCompatibility::Unknown,
        relative_fov_difference: None,
        relative_focal_difference: None,
        applied_tolerance: None,
        native_lens_pair: native_pair,
    };
    match (a.effective_35mm, b.effective_35mm) {
        (Some(a), Some(b)) => match relative_fov_difference(a, b) {
            Some(diff) => FocalComparison {
                state: if diff <= applied {
                    FocalCompatibility::Compatible
                } else {
                    FocalCompatibility::Incompatible
                },
                relative_fov_difference: Some(diff),
                relative_focal_difference: relative_focal_difference(a, b),
                applied_tolerance: Some(applied),
                native_lens_pair: native_pair,
            },
            None => unknown,
        },
        _ => unknown,
    }
}

/// A pair worth considering.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidatePair {
    /// Lexicographically smaller photo ID.
    pub a: String,
    /// Lexicographically larger photo ID.
    pub b: String,
    /// Session both belong to.
    pub session_id: String,
    /// Absolute capture-time distance, in seconds.
    pub time_distance_seconds: i64,
    /// Focal comparison.
    pub focal: FocalComparison,
    /// The caller asked for this pair explicitly, which overrides the focal
    /// prefilter.
    ///
    /// Needed because a manual group member may have no usable focal metadata
    /// yet still need an alignment for the viewer (`SPEC.md` §6).
    pub forced: bool,
}

impl CandidatePair {
    /// Whether this pair should be visually analyzed.
    ///
    /// A known focal mismatch is decided from metadata alone, so no SIFT work is
    /// spent on it (`PLAN-DROP1.md` §10.3) — unless the caller forced the pair.
    pub fn should_analyze(&self) -> bool {
        self.forced || self.focal.state != FocalCompatibility::Incompatible
    }
}

/// Generate every in-session pair.
///
/// Photos need not be consecutive to be candidates. Every metadata-plausible
/// pair within a session is generated; the focal prefilter, not a work cap,
/// is what keeps the visual half of the pipeline off pairs that are already
/// decided (`SPEC.md` §4.3).
pub fn generate_candidates(
    assignment: &SessionAssignment,
    photos: &[InspectedPhoto],
    tolerance: f64,
    native_lens_tolerance: f64,
) -> Vec<CandidatePair> {
    let index: std::collections::HashMap<&str, &InspectedPhoto> =
        photos.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut out: Vec<CandidatePair> = Vec::new();
    for session in &assignment.sessions {
        let mut pairs: Vec<CandidatePair> = Vec::new();
        for i in 0..session.photo_ids.len() {
            for j in (i + 1)..session.photo_ids.len() {
                let (mut a, mut b) = (session.photo_ids[i].as_str(), session.photo_ids[j].as_str());
                if a > b {
                    std::mem::swap(&mut a, &mut b);
                }
                let Some(pa) = index.get(a) else { continue };
                let Some(pb) = index.get(b) else { continue };
                let distance = match (pa.corrected_instant, pb.corrected_instant) {
                    (Some(x), Some(y)) => (x.0 - y.0).abs(),
                    _ => 0,
                };
                pairs.push(CandidatePair {
                    a: a.to_string(),
                    b: b.to_string(),
                    session_id: session.id.clone(),
                    time_distance_seconds: distance,
                    focal: focal_comparison(
                        &FocalDescriptor::from_metadata(&pa.metadata),
                        &FocalDescriptor::from_metadata(&pb.metadata),
                        tolerance,
                        native_lens_tolerance,
                    ),
                    forced: false,
                });
            }
        }
        // Stable, readable output order.
        pairs.sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));
        out.extend(pairs);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Fingerprint, FocalSource, PhotoMetadata, TimestampSource};

    /// A photo at a native lens step, which is the common case in the sets.
    fn photo(id: &str, seconds: i64, focal: Option<f64>, config: &str) -> InspectedPhoto {
        native(id, seconds, focal, config, true)
    }

    fn native(
        id: &str,
        seconds: i64,
        focal: Option<f64>,
        config: &str,
        native_lens: bool,
    ) -> InspectedPhoto {
        let mut p = build(id, seconds, focal, config);
        p.metadata.native_lens = native_lens;
        if !native_lens {
            p.metadata.digital_zoom_ratio = Some(1.5);
        }
        p
    }

    fn descriptor(focal: Option<f64>, native_lens: bool) -> FocalDescriptor {
        FocalDescriptor {
            effective_35mm: focal,
            native_lens,
        }
    }

    fn build(id: &str, seconds: i64, focal: Option<f64>, config: &str) -> InspectedPhoto {
        InspectedPhoto {
            id: id.to_string(),
            relative_path: format!("{id}.jpg"),
            absolute_path: std::path::PathBuf::from(format!("/tmp/{id}.jpg")),
            fingerprint: Fingerprint {
                size: 1,
                modified_at: None,
                sha256: "0".repeat(64),
            },
            metadata: PhotoMetadata {
                captured_at: None,
                capture_time_source: TimestampSource::ExifDateTimeOriginal,
                timezone_assumed: false,
                corrected_at: None,
                applied_time_offset_seconds: 0,
                exif_orientation: 1,
                orientation_applied_by_decoder: false,
                camera_make: None,
                camera_model: None,
                lens_model: None,
                focal_length_mm: None,
                focal_length_35mm: focal,
                effective_focal_length_35mm: focal,
                focal_source: if focal.is_some() {
                    FocalSource::Exif35mm
                } else {
                    FocalSource::Unknown
                },
                digital_zoom_ratio: None,
                native_lens: true,
                mode_tag_raw: None,
                mode_tag_source: None,
                mode_label: None,
                mode_flags: vec![],
                mode_tag_unknown_bits: None,
            },
            configuration_id: config.to_string(),
            device_key: config.to_string(),
            original_dimensions: (100, 100),
            oriented_dimensions: (100, 100),
            corrected_instant: Some((seconds, 0)),
            notes: vec![],
        }
    }

    fn untimed(id: &str) -> InspectedPhoto {
        let mut p = photo(id, 0, None, "c");
        p.corrected_instant = None;
        p.metadata.capture_time_source = TimestampSource::Missing;
        p
    }

    #[test]
    fn exactly_the_gap_stays_in_one_session() {
        let photos = vec![photo("p1", 0, None, "c"), photo("p2", 30, None, "c")];
        let a = detect_sessions(&photos, 30);
        assert_eq!(a.sessions.len(), 1);
        assert_eq!(a.sessions[0].photo_ids.len(), 2);
    }

    #[test]
    fn one_second_beyond_the_gap_starts_a_new_session() {
        let photos = vec![photo("p1", 0, None, "c"), photo("p2", 31, None, "c")];
        let a = detect_sessions(&photos, 30);
        assert_eq!(a.sessions.len(), 2);
        assert_eq!(a.sessions[0].id, "session-001");
        assert_eq!(a.sessions[1].id, "session-002");
    }

    #[test]
    fn a_chain_of_short_gaps_produces_one_long_session() {
        let photos: Vec<InspectedPhoto> = (0..10)
            .map(|i| photo(&format!("p{i:02}"), i * 25, None, "c"))
            .collect();
        let a = detect_sessions(&photos, 30);
        assert_eq!(a.sessions.len(), 1);
        assert_eq!(a.sessions[0].photo_ids.len(), 10);
        // Total span is 225 s, far beyond the 30 s gap.
        assert_eq!(a.sessions[0].end.0 - a.sessions[0].start.0, 225);
    }

    #[test]
    fn session_assignment_is_stable_under_input_permutation() {
        let mut photos = vec![
            photo("p3", 10, None, "b"),
            photo("p1", 10, None, "a"),
            photo("p2", 10, None, "a"),
            photo("p4", 100, None, "a"),
        ];
        let first = detect_sessions(&photos, 30);
        photos.reverse();
        let second = detect_sessions(&photos, 30);
        assert_eq!(first, second);
        // Equal timestamps break by configuration then ID.
        assert_eq!(
            first.sessions[0].photo_ids,
            vec!["p1".to_string(), "p2".to_string(), "p3".to_string()]
        );
    }

    #[test]
    fn photos_without_a_timestamp_are_reported_not_dropped() {
        let photos = vec![photo("p1", 0, None, "c"), untimed("p2")];
        let a = detect_sessions(&photos, 30);
        assert_eq!(a.sessions.len(), 1);
        assert_eq!(a.without_timestamp, vec!["p2".to_string()]);
    }

    #[test]
    fn focal_difference_is_symmetric() {
        let a = relative_focal_difference(23.0, 24.0).unwrap();
        let b = relative_focal_difference(24.0, 23.0).unwrap();
        assert!((a - b).abs() < 1e-15);
        assert!((a - 1.0 / 23.0).abs() < 1e-12);
    }

    #[test]
    fn focal_difference_rejects_non_positive_and_non_finite_values() {
        assert!(relative_focal_difference(0.0, 24.0).is_none());
        assert!(relative_focal_difference(-1.0, 24.0).is_none());
        assert!(relative_focal_difference(f64::NAN, 24.0).is_none());
    }

    #[test]
    fn angle_of_view_matches_the_textbook_values() {
        let aov = |f: f64| diagonal_angle_of_view_degrees(f).unwrap();
        // 50 mm on 35 mm film is the classic ~47 degrees diagonal.
        assert!((aov(50.0) - 46.79).abs() < 0.02, "{}", aov(50.0));
        assert!((aov(24.0) - 84.06).abs() < 0.02, "{}", aov(24.0));
        assert!((aov(12.0) - 121.96).abs() < 0.02, "{}", aov(12.0));
        // Monotone decreasing, and bounded below 180 degrees.
        assert!(aov(12.0) > aov(24.0) && aov(24.0) > aov(50.0));
        assert!(aov(0.5) < 180.0);
        assert!(diagonal_angle_of_view_degrees(0.0).is_none());
        assert!(diagonal_angle_of_view_degrees(-5.0).is_none());
        assert!(diagonal_angle_of_view_degrees(f64::NAN).is_none());
    }

    #[test]
    fn the_angular_difference_is_symmetric_and_not_the_focal_ratio() {
        let fwd = relative_fov_difference(12.0, 15.0).unwrap();
        let rev = relative_fov_difference(15.0, 12.0).unwrap();
        assert!((fwd - rev).abs() < 1e-15);

        // The whole point of the basis change: two ultrawides 25 % apart in
        // focal length are 10 % apart in framing, while a moderate zoom step
        // 17 % apart in focal length is 12 % apart in framing — the opposite
        // order (`IMPLEM-DROP1.md` §6.11).
        let ultrawide_focal = relative_focal_difference(12.0, 15.0).unwrap();
        let zoomed_focal = relative_focal_difference(24.0, 28.0).unwrap();
        assert!(ultrawide_focal > zoomed_focal);
        let ultrawide_fov = relative_fov_difference(12.0, 15.0).unwrap();
        let zoomed_fov = relative_fov_difference(24.0, 28.0).unwrap();
        assert!(
            ultrawide_fov < zoomed_fov,
            "angle of view must order these the other way: {ultrawide_fov} vs {zoomed_fov}"
        );
        assert!((ultrawide_fov - 0.1035).abs() < 0.001, "{ultrawide_fov}");
        assert!((zoomed_fov - 0.1153).abs() < 0.001, "{zoomed_fov}");
    }

    #[test]
    fn the_framing_predicate_boundary_is_inclusive_and_order_independent() {
        let diff = relative_fov_difference(24.0, 28.0).unwrap();
        let a = descriptor(Some(24.0), false);
        let b = descriptor(Some(28.0), false);
        // Exactly at the tolerance is compatible, in both argument orders.
        assert_eq!(
            focal_comparison(&a, &b, diff, diff).state,
            FocalCompatibility::Compatible
        );
        assert_eq!(
            focal_comparison(&b, &a, diff, diff).state,
            FocalCompatibility::Compatible
        );
        let under = diff - 1e-9;
        assert_eq!(
            focal_comparison(&a, &b, under, under).state,
            FocalCompatibility::Incompatible
        );
    }

    #[test]
    fn two_native_lenses_get_the_relaxed_tolerance_and_a_zoomed_pair_does_not() {
        // The labelled ultrawide pair: 15 mm against 12 mm, both lenses at their
        // native field of view.
        let oppo_uw = descriptor(Some(15.0), true);
        let pixel_uw = descriptor(Some(12.0), true);
        let relaxed = focal_comparison(&oppo_uw, &pixel_uw, 0.10, 0.20);
        assert_eq!(relaxed.state, FocalCompatibility::Compatible);
        assert!(relaxed.native_lens_pair);
        assert_eq!(relaxed.applied_tolerance, Some(0.20));

        // The same framing difference reached by zooming is judged strictly.
        let zoomed = focal_comparison(&oppo_uw, &descriptor(Some(12.0), false), 0.10, 0.20);
        assert_eq!(zoomed.state, FocalCompatibility::Incompatible);
        assert!(!zoomed.native_lens_pair);
        assert_eq!(zoomed.applied_tolerance, Some(0.10));

        // The relaxed tier is not a licence to group a main camera with an
        // ultrawide: that pair measures 0.278 against a 0.20 tolerance.
        let main = descriptor(Some(23.0), true);
        assert_eq!(
            focal_comparison(&main, &oppo_uw, 0.10, 0.20).state,
            FocalCompatibility::Incompatible
        );
    }

    #[test]
    fn the_relaxed_tier_can_never_be_stricter_than_the_base_tier() {
        // A user who sets the native tolerance below the base tolerance must not
        // end up with a *tighter* rule for native pairs than for zoomed ones.
        let a = descriptor(Some(23.0), true);
        let b = descriptor(Some(24.0), true);
        let cmp = focal_comparison(&a, &b, 0.10, 0.0);
        assert_eq!(cmp.state, FocalCompatibility::Compatible);
        assert_eq!(cmp.applied_tolerance, Some(0.10));
    }

    #[test]
    fn both_the_angular_and_the_focal_difference_are_recorded() {
        let cmp = focal_comparison(
            &descriptor(Some(12.0), true),
            &descriptor(Some(15.0), true),
            0.10,
            0.20,
        );
        let fov = cmp.relative_fov_difference.unwrap();
        let focal = cmp.relative_focal_difference.unwrap();
        assert!((focal - 0.25).abs() < 1e-12);
        assert!(fov < focal, "the angular measure must be the smaller here");
    }

    #[test]
    fn a_missing_equivalent_focal_length_is_unknown_not_incompatible() {
        let known = descriptor(Some(24.0), true);
        let missing = descriptor(None, true);
        assert_eq!(
            focal_comparison(&missing, &known, 0.10, 0.20).state,
            FocalCompatibility::Unknown
        );
        assert_eq!(
            focal_comparison(&known, &missing, 0.10, 0.20).state,
            FocalCompatibility::Unknown
        );
        assert_eq!(
            focal_comparison(&missing, &missing, 0.10, 0.20).state,
            FocalCompatibility::Unknown
        );
    }

    #[test]
    fn a_descriptor_read_from_an_older_document_falls_back_to_the_reported_focal() {
        let mut photo = photo("p1", 0, Some(24.0), "a");
        // A document written before the effective value existed.
        photo.metadata.effective_focal_length_35mm = None;
        let d = FocalDescriptor::from_metadata(&photo.metadata);
        assert_eq!(d.effective_35mm, Some(24.0));
    }

    #[test]
    fn an_unknown_focal_pair_is_still_analyzed_but_a_mismatch_is_not() {
        let unknown = CandidatePair {
            a: "a".into(),
            b: "b".into(),
            session_id: "session-001".into(),
            time_distance_seconds: 0,
            focal: focal_comparison(
                &descriptor(None, true),
                &descriptor(Some(24.0), true),
                0.10,
                0.20,
            ),
            forced: false,
        };
        assert!(unknown.should_analyze());

        let mismatch = CandidatePair {
            focal: focal_comparison(
                &descriptor(Some(24.0), true),
                &descriptor(Some(70.0), true),
                0.10,
                0.20,
            ),
            ..unknown.clone()
        };
        assert!(!mismatch.should_analyze());
    }

    #[test]
    fn non_consecutive_photos_remain_candidates() {
        let photos = vec![
            photo("p1", 0, Some(24.0), "a"),
            photo("p2", 10, Some(24.0), "b"),
            photo("p3", 20, Some(24.0), "c"),
        ];
        let assignment = detect_sessions(&photos, 30);
        let pairs = generate_candidates(&assignment, &photos, 0.10, 0.20);
        assert_eq!(pairs.len(), 3);
        assert!(pairs.iter().any(|p| p.a == "p1" && p.b == "p3"));
    }

    #[test]
    fn candidate_ids_are_canonically_ordered() {
        let photos = vec![photo("zz", 0, None, "a"), photo("aa", 1, None, "b")];
        let assignment = detect_sessions(&photos, 30);
        let pairs = generate_candidates(&assignment, &photos, 0.10, 0.20);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].a, "aa");
        assert_eq!(pairs[0].b, "zz");
    }

    #[test]
    fn candidate_generation_is_stable_under_input_permutation() {
        let mut photos = vec![
            photo("p1", 0, Some(24.0), "a"),
            photo("p2", 5, Some(24.0), "b"),
            photo("p3", 9, Some(50.0), "c"),
        ];
        let first = generate_candidates(&detect_sessions(&photos, 30), &photos, 0.10, 0.20);
        photos.rotate_left(2);
        let second = generate_candidates(&detect_sessions(&photos, 30), &photos, 0.10, 0.20);
        assert_eq!(first, second);
    }

    #[test]
    fn a_large_session_generates_every_pair() {
        // 30 photos one second apart: 435 pairs, no cap.
        let photos: Vec<InspectedPhoto> = (0..30)
            .map(|i| photo(&format!("p{i:02}"), i, Some(24.0), "a"))
            .collect();
        let assignment = detect_sessions(&photos, 30);
        assert_eq!(assignment.sessions.len(), 1);
        let pairs = generate_candidates(&assignment, &photos, 0.10, 0.20);
        assert_eq!(pairs.len(), 435, "every pair must be reported");
    }

    #[test]
    fn photos_in_different_sessions_are_never_paired() {
        let photos = vec![
            photo("p1", 0, Some(24.0), "a"),
            photo("p2", 1000, Some(24.0), "b"),
        ];
        let assignment = detect_sessions(&photos, 30);
        assert_eq!(assignment.sessions.len(), 2);
        assert!(generate_candidates(&assignment, &photos, 0.10, 0.20).is_empty());
    }

    #[test]
    fn different_physical_focal_lengths_are_not_compared_across_devices() {
        // 6.75 mm on one phone and 6.9 mm on another are not the same field of
        // view; only the 35 mm equivalents are comparable, and here they differ.
        let oppo = photo("p1", 0, Some(70.0), "oppo");
        let pixel = photo("p2", 1, Some(49.0), "pixel");
        let cmp = focal_comparison(
            &FocalDescriptor::from_metadata(&oppo.metadata),
            &FocalDescriptor::from_metadata(&pixel.metadata),
            0.10,
            0.20,
        );
        assert_eq!(cmp.state, FocalCompatibility::Incompatible);
        assert!((cmp.relative_focal_difference.unwrap() - 21.0 / 49.0).abs() < 1e-12);
    }
}
