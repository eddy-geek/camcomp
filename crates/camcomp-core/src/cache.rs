//! Fingerprinting and `camcomp.json` persistence.
//!
//! Two invariants drive this module. Source photos are never modified, and the
//! only file written into the source folder is `camcomp.json`
//! (`REQUIREMENTS.md` §3). And a write must not be able to destroy a previous
//! valid cache: the document is written to a temporary sibling and renamed
//! (`PLAN-DROP1.md` §15.1).

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::schema::{schema_major, AnalysisDocument, Fingerprint, SCHEMA_MAJOR};
use crate::{Error, Result};

/// Name of the cache file, looked for automatically when a folder is opened.
pub const CACHE_FILE_NAME: &str = "camcomp.json";

/// Look for an existing analysis in a folder.
pub fn find_in_folder(folder: &Path) -> Option<PathBuf> {
    let candidate = folder.join(CACHE_FILE_NAME);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Hex-encode bytes.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// SHA-256 of a file, streamed.
///
/// Streamed rather than read whole: hashing must not require holding a 50 MP
/// compressed file in memory just to identify it (`PLAN-DROP1.md` §9.1).
pub fn sha256_file(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut reader = std::io::BufReader::with_capacity(1 << 16, file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = reader.read(&mut buffer).map_err(|e| Error::Io {
            path: path.to_path_buf(),
            source: e,
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// SHA-256 of an in-memory buffer.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Size and modification time, without hashing.
pub fn cheap_fingerprint(path: &Path) -> Result<(u64, Option<String>)> {
    let meta = std::fs::metadata(path).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let modified = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    Ok((meta.len(), modified))
}

/// Build a fingerprint, reusing a previous digest when size and modification
/// time are unchanged.
///
/// Size and mtime are only a fast path; the digest is the cache identity. A
/// changed mtime forces a re-hash, but if the digest still matches then every
/// cached result for that photo stays valid — so touching a file, or copying it
/// to another folder, does not invalidate analysis (`SPEC.md` §3.1).
pub fn fingerprint_with_fast_path(
    path: &Path,
    previous: Option<&Fingerprint>,
    digest_of_bytes: impl FnOnce() -> Result<String>,
) -> Result<Fingerprint> {
    let (size, modified_at) = cheap_fingerprint(path)?;
    if let Some(prev) = previous {
        if prev.size == size && prev.modified_at == modified_at {
            return Ok(Fingerprint {
                size,
                modified_at,
                sha256: prev.sha256.clone(),
            });
        }
    }
    Ok(Fingerprint {
        size,
        modified_at,
        sha256: digest_of_bytes()?,
    })
}

/// Read and version-check an analysis document.
///
/// A schema *major* mismatch is a clear error, not a best-effort read: silently
/// interpreting incompatible data is worse than refusing (`SPEC.md` §8.1).
pub fn load(path: &Path) -> Result<AnalysisDocument> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    load_str(path, &text)
}

/// Parse and version-check an analysis document from text.
pub fn load_str(path: &Path, text: &str) -> Result<AnalysisDocument> {
    // Read the version before the body, so an incompatible document reports the
    // version rather than a confusing field-level parse error.
    let probe: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Error::MalformedDocument {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
    let declared = probe
        .get("schemaVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match schema_major(declared) {
        Some(major) if major == SCHEMA_MAJOR => {}
        _ => {
            return Err(Error::SchemaVersionMismatch {
                path: path.to_path_buf(),
                found: declared.to_string(),
                supported_major: SCHEMA_MAJOR,
            })
        }
    }
    serde_json::from_str(text).map_err(|e| Error::MalformedDocument {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

/// Serialize a document, pretty-printed and stable.
pub fn to_json(document: &AnalysisDocument) -> Result<String> {
    // Reject non-finite numbers before they become `null` in the output and
    // silently corrupt a homography on the next read.
    let value = serde_json::to_value(document)
        .map_err(|e| Error::InconsistentDocument(format!("document cannot be serialized: {e}")))?;
    if let Some(pointer) = find_non_finite(&value, String::new()) {
        return Err(Error::InconsistentDocument(format!(
            "non-finite number at `{pointer}`"
        )));
    }
    serde_json::to_string_pretty(&value)
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|e| Error::InconsistentDocument(format!("document cannot be serialized: {e}")))
}

fn find_non_finite(value: &serde_json::Value, path: String) -> Option<String> {
    match value {
        // `serde_json` maps a non-finite float to `null` rather than failing, so
        // this is where a NaN homography element would slip through. Every
        // optional field in the schema is skipped when absent, so a `null`
        // anywhere in the document means exactly one thing.
        serde_json::Value::Null => Some(path),
        serde_json::Value::Number(n) => {
            if n.as_f64().map(|v| v.is_finite()).unwrap_or(false)
                || n.as_i64().is_some()
                || n.as_u64().is_some()
            {
                None
            } else {
                Some(path)
            }
        }
        serde_json::Value::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, v)| find_non_finite(v, format!("{path}/{i}"))),
        serde_json::Value::Object(map) => map
            .iter()
            .find_map(|(k, v)| find_non_finite(v, format!("{path}/{k}"))),
        _ => None,
    }
}

/// Write a document atomically.
///
/// Writes a temporary sibling, flushes it, then renames over the target. An
/// interrupted write therefore leaves the previous valid document intact.
pub fn save_atomic(path: &Path, document: &AnalysisDocument) -> Result<()> {
    let json = to_json(document)?;
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| Error::Io {
        path: parent.to_path_buf(),
        source: e,
    })?;
    let temp = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(CACHE_FILE_NAME)
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp).map_err(|e| Error::Io {
            path: temp.clone(),
            source: e,
        })?;
        file.write_all(json.as_bytes()).map_err(|e| Error::Io {
            path: temp.clone(),
            source: e,
        })?;
        file.sync_all().map_err(|e| Error::Io {
            path: temp.clone(),
            source: e,
        })?;
    }
    std::fs::rename(&temp, path).map_err(|e| Error::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}

/// Stable analysis ID for a photo, derived from its path relative to the source
/// root.
///
/// Per-path rather than per-content: two byte-identical files with different
/// names are one content identity but two distinct photos (`SPEC.md` §3.1).
pub fn photo_id_for_path(relative_path: &str) -> String {
    let digest = sha256_bytes(relative_path.as_bytes());
    format!("photo-{}", &digest[..12])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        Configuration, Dimensions, FocalSource, Photo, PhotoMetadata, TimestampSource, Ungrouped,
        UngroupedReason,
    };

    /// Process-unique so two concurrent `cargo test` invocations cannot delete
    /// each other's fixtures.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("camcomp-cache-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn document_with(id: &str) -> AnalysisDocument {
        let mut doc = AnalysisDocument::default();
        doc.configurations.push(Configuration {
            id: "device".into(),
            device_key: "device".into(),
            make: None,
            model: None,
            mode_label: None,
            display_name: "Test device".into(),
            photo_count: 1,
        });
        doc.photos.push(Photo {
            id: id.to_string(),
            path: "a.jpg".into(),
            configuration_id: "device".into(),
            fingerprint: Fingerprint {
                size: 3,
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
            photo_id: id.to_string(),
            reason: UngroupedReason::NoSessionPeer,
            detail: None,
        });
        doc
    }

    #[test]
    fn digests_are_stable_and_content_dependent() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(sha256_bytes(b"abc"), sha256_bytes(b"abd"));
    }

    #[test]
    fn streamed_and_in_memory_digests_agree() {
        let dir = temp_dir("digest");
        let path = dir.join("blob.bin");
        let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&path, &payload).unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(&payload));
    }

    #[test]
    fn hashing_a_missing_file_names_the_path() {
        let err = sha256_file(Path::new("/definitely/not/here.jpg")).unwrap_err();
        assert_eq!(err.code(), "io");
    }

    #[test]
    fn the_fast_path_skips_rehashing_when_size_and_mtime_match() {
        let dir = temp_dir("fastpath");
        let path = dir.join("a.bin");
        std::fs::write(&path, b"hello").unwrap();
        let (size, modified) = cheap_fingerprint(&path).unwrap();
        let previous = Fingerprint {
            size,
            modified_at: modified,
            sha256: "cached-digest".into(),
        };
        let fp = fingerprint_with_fast_path(&path, Some(&previous), || {
            panic!("must not re-hash when the cheap fields match")
        })
        .unwrap();
        assert_eq!(fp.sha256, "cached-digest");
    }

    #[test]
    fn a_changed_size_forces_a_rehash() {
        let dir = temp_dir("rehash");
        let path = dir.join("a.bin");
        std::fs::write(&path, b"hello world").unwrap();
        let previous = Fingerprint {
            size: 5,
            modified_at: None,
            sha256: "stale".into(),
        };
        let fp =
            fingerprint_with_fast_path(&path, Some(&previous), || Ok(sha256_bytes(b"hello world")))
                .unwrap();
        assert_eq!(fp.sha256, sha256_bytes(b"hello world"));
        assert_eq!(fp.size, 11);
    }

    #[test]
    fn a_document_round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let path = dir.join(CACHE_FILE_NAME);
        let doc = document_with("photo-abc");
        save_atomic(&path, &doc).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(doc, back);
    }

    #[test]
    fn an_existing_analysis_is_discovered_in_a_folder() {
        let dir = temp_dir("discover");
        assert!(find_in_folder(&dir).is_none());
        save_atomic(&dir.join(CACHE_FILE_NAME), &document_with("photo-abc")).unwrap();
        assert_eq!(find_in_folder(&dir), Some(dir.join(CACHE_FILE_NAME)));
    }

    #[test]
    fn writing_leaves_no_temporary_file_behind() {
        let dir = temp_dir("notemp");
        save_atomic(&dir.join(CACHE_FILE_NAME), &document_with("photo-abc")).unwrap();
        let entries: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec![CACHE_FILE_NAME.to_string()]);
    }

    #[test]
    fn an_overwrite_replaces_the_previous_document_completely() {
        let dir = temp_dir("overwrite");
        let path = dir.join(CACHE_FILE_NAME);
        save_atomic(&path, &document_with("photo-aaa")).unwrap();
        save_atomic(&path, &document_with("photo-bbb")).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.photos.len(), 1);
        assert_eq!(back.photos[0].id, "photo-bbb");
    }

    #[test]
    fn an_unsupported_major_version_is_rejected_by_version_not_by_parse_error() {
        let dir = temp_dir("version");
        let path = dir.join(CACHE_FILE_NAME);
        let mut value = serde_json::to_value(document_with("photo-abc")).unwrap();
        value["schemaVersion"] = serde_json::json!("2.0");
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let err = load(&path).unwrap_err();
        assert_eq!(err.code(), "schema-version-mismatch");
        assert!(err.to_string().contains("2.0"), "{err}");
    }

    #[test]
    fn a_missing_version_is_rejected() {
        let dir = temp_dir("noversion");
        let path = dir.join(CACHE_FILE_NAME);
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(load(&path).unwrap_err().code(), "schema-version-mismatch");
    }

    #[test]
    fn corrupt_json_fails_without_touching_anything() {
        let dir = temp_dir("corrupt");
        let path = dir.join(CACHE_FILE_NAME);
        std::fs::write(&path, "{ this is not json").unwrap();
        assert_eq!(load(&path).unwrap_err().code(), "malformed-document");
        // The file is still there, unmodified.
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ this is not json"
        );
    }

    #[test]
    fn a_minor_version_ahead_is_still_readable() {
        let dir = temp_dir("minor");
        let path = dir.join(CACHE_FILE_NAME);
        let mut value = serde_json::to_value(document_with("photo-abc")).unwrap();
        value["schemaVersion"] = serde_json::json!("1.7");
        value["futureField"] = serde_json::json!("ignored");
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.schema_version, "1.7");
    }

    #[test]
    fn non_finite_numbers_are_rejected_before_serialization() {
        let mut doc = document_with("photo-abc");
        doc.algorithm.parameters.compatibility_threshold = f64::NAN;
        let err = to_json(&doc).unwrap_err();
        assert_eq!(err.code(), "inconsistent-document");
    }

    #[test]
    fn photo_ids_are_path_derived_and_stable() {
        let a = photo_id_for_path("camera-a/img-001.jpg");
        assert_eq!(a, photo_id_for_path("camera-a/img-001.jpg"));
        assert_ne!(a, photo_id_for_path("camera-b/img-001.jpg"));
        assert!(a.starts_with("photo-"));
        assert_eq!(a.len(), "photo-".len() + 12);
    }

    #[test]
    fn floats_survive_a_json_round_trip_bit_for_bit() {
        // Regression guard for serde_json's fast-path float parser, which is off
        // by up to one ulp unless the `float_roundtrip` feature is enabled. A
        // persisted homography that drifts on every load is not a cache.
        let awkward: [f64; 6] = [
            115.372_863_769_531_25,
            0.002_035_204_099_302_916_2,
            0.979_476_012_823_874_5,
            0.983_103_372_362_757_5,
            1.797_693_134_862_315_7e308,
            5e-324,
        ];
        for value in awkward {
            let text = serde_json::to_string(&value).unwrap();
            let back: f64 = serde_json::from_str(&text).unwrap();
            assert_eq!(
                back.to_bits(),
                value.to_bits(),
                "{value:?} was written as {text} and read back as {back:?}"
            );
        }
    }

    #[test]
    fn a_homography_survives_a_document_round_trip_bit_for_bit() {
        use crate::schema::{
            Alignment, AlignmentProvenance, CoordinateSystem, Group, GroupEvidence,
            TransformDirection,
        };
        let homography = vec![
            0.975_197_149_396_718_5,
            -0.010_146_782_973_979_815,
            0.017_677_827_458_120_43,
            -0.008_835_473_758_557_873,
            0.983_859_669_891_301_2,
            0.023_430_692_241_709_627,
            -0.016_637_020_799_089_772,
            -0.018_469_070_227_245_532,
            1.0,
        ];
        let inverse = crate::geometry::Mat3::from_row_major(&homography)
            .unwrap()
            .inverse()
            .unwrap()
            .as_row_major()
            .to_vec();

        let mut doc = document_with("photo-aaa");
        doc.photos.push({
            let mut p = doc.photos[0].clone();
            p.id = "photo-bbb".into();
            p.path = "b.jpg".into();
            p
        });
        doc.ungrouped.clear();
        doc.groups.push(Group {
            id: "group-1".into(),
            anchor_photo_id: "photo-aaa".into(),
            photo_ids: vec!["photo-aaa".into(), "photo-bbb".into()],
            alignments: vec![Alignment {
                photo_id: "photo-bbb".into(),
                coordinate_system: CoordinateSystem::NormalizedOriented,
                direction: TransformDirection::PhotoToAnchor,
                homography: homography.clone(),
                inverse_homography: inverse,
                correspondences: vec![],
                confidence: 0.975_234_242_630_102_8,
                provenance: AlignmentProvenance::DirectPair,
            }],
            flags: vec![],
            evidence: GroupEvidence {
                mean_anchor_score: Some(0.975_234_242_630_102_8),
                min_anchor_score: Some(0.975_234_242_630_102_8),
                compatible_member_pairs: 1,
                incompatible_member_pairs: 0,
                unverified_member_pairs: 0,
            },
        });
        doc.validate().unwrap();

        let text = to_json(&doc).unwrap();
        let back = load_str(Path::new("camcomp.json"), &text).unwrap();
        assert_eq!(doc, back);
        // And a second cycle is still identical, so repeated save/load cannot
        // accumulate drift.
        assert_eq!(text, to_json(&back).unwrap());
    }

    #[test]
    fn json_output_ends_with_a_newline() {
        let json = to_json(&document_with("photo-abc")).unwrap();
        assert!(json.ends_with("}\n"));
    }
}
