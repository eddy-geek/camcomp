//! EXIF extraction and metadata normalization.
//!
//! Raw values are kept alongside the normalized ones. Nothing is inferred from
//! incompatible metadata: a missing 35 mm equivalent focal length stays missing
//! rather than being back-computed from a physical focal length whose sensor is
//! unknown (`SPEC.md` §4.2).

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};

use crate::domain::{MetadataReader, RawMetadata};
use crate::schema::{FocalSource, FocalZoomPolicy};

/// Reads standard EXIF with `kamadak-exif`, which also handles the EXIF block
/// inside a HEIF container.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExifMetadataReader;

impl MetadataReader for ExifMetadataReader {
    fn read(&self, bytes: &[u8]) -> RawMetadata {
        let mut out = RawMetadata::default();
        let exif = match exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
            Ok(exif) => exif,
            Err(container_error) => {
                // `kamadak-exif` rejects a container EXIF block over 64 KiB, and
                // every reference Oppo HEIC exceeds that because of its embedded
                // thumbnail. Recover the payload from the ISO-BMFF structure and
                // parse it directly rather than losing the photo's metadata.
                match crate::heif::extract_exif_tiff(bytes)
                    .and_then(|tiff| exif::Reader::new().read_raw(tiff).ok())
                {
                    Some(exif) => exif,
                    None => {
                        out.notes
                            .push(format!("no readable EXIF block: {container_error}"));
                        return out;
                    }
                }
            }
        };

        for (tag, name) in [
            (exif::Tag::DateTimeOriginal, "DateTimeOriginal"),
            (exif::Tag::DateTimeDigitized, "DateTimeDigitized"),
            (exif::Tag::DateTime, "DateTime"),
        ] {
            if let Some(value) = ascii_field(&exif, tag) {
                out.date_time = Some(value);
                out.date_time_tag = Some(name);
                break;
            }
        }

        out.utc_offset_minutes = [
            exif::Tag::OffsetTimeOriginal,
            exif::Tag::OffsetTimeDigitized,
            exif::Tag::OffsetTime,
        ]
        .into_iter()
        .find_map(|tag| ascii_field(&exif, tag))
        .and_then(|s| parse_utc_offset_minutes(&s));

        out.sub_second = [
            exif::Tag::SubSecTimeOriginal,
            exif::Tag::SubSecTimeDigitized,
            exif::Tag::SubSecTime,
        ]
        .into_iter()
        .find_map(|tag| ascii_field(&exif, tag))
        .and_then(|s| parse_sub_second_nanos(&s));

        out.orientation = exif
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0))
            .map(|v| v as u16);

        out.make = ascii_field(&exif, exif::Tag::Make);
        out.model = ascii_field(&exif, exif::Tag::Model);
        out.lens_model = ascii_field(&exif, exif::Tag::LensModel);
        out.software = ascii_field(&exif, exif::Tag::Software);
        out.image_description = ascii_field(&exif, exif::Tag::ImageDescription);
        out.user_comment = user_comment(&exif);

        out.focal_length_mm = rational_field(&exif, exif::Tag::FocalLength).filter(|v| *v > 0.0);
        out.focal_length_35mm = exif
            .get_field(exif::Tag::FocalLengthIn35mmFilm, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0))
            .map(|v| v as f64)
            // Several devices write 0 to mean "not known"; a 0 mm lens does not
            // exist, so treating it as a value would poison every comparison.
            .filter(|v| *v > 0.0)
            .or_else(|| {
                rational_field(&exif, exif::Tag::FocalLengthIn35mmFilm).filter(|v| *v > 0.0)
            });
        out.digital_zoom_ratio = rational_field(&exif, exif::Tag::DigitalZoomRatio);

        let w = exif
            .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));
        let h = exif
            .get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));
        if let (Some(w), Some(h)) = (w, h) {
            out.exif_dimensions = Some((w, h));
        }

        out
    }
}

fn ascii_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    match &field.value {
        exif::Value::Ascii(entries) => {
            let raw = entries.first()?;
            let text = String::from_utf8_lossy(raw);
            let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => {
            let text = field.display_value().to_string();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
    }
}

fn rational_field(exif: &exif::Exif, tag: exif::Tag) -> Option<f64> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    match &field.value {
        exif::Value::Rational(v) => v.first().map(|r| r.to_f64()),
        exif::Value::SRational(v) => v.first().map(|r| r.to_f64()),
        exif::Value::Float(v) => v.first().map(|f| *f as f64),
        exif::Value::Double(v) => v.first().copied(),
        _ => field.value.get_uint(0).map(|v| v as f64),
    }
    .filter(|v| v.is_finite())
}

/// `UserComment` is an `Undefined` blob with an 8-byte character-code prefix.
///
/// The reference OPPO files write an invalid prefix, which is why `exiftool`
/// warns about them, so the printable ASCII payload is recovered directly rather
/// than trusting the declared encoding.
fn user_comment(exif: &exif::Exif) -> Option<String> {
    let field = exif.get_field(exif::Tag::UserComment, exif::In::PRIMARY)?;
    let bytes = match &field.value {
        exif::Value::Undefined(b, _) => b.as_slice(),
        exif::Value::Ascii(entries) => entries.first().map(|v| v.as_slice())?,
        _ => return None,
    };
    let payload = if bytes.len() > 8 {
        match &bytes[..8] {
            b"ASCII\0\0\0" => &bytes[8..],
            b"UNICODE\0" => return None,
            b"JIS\0\0\0\0\0" => return None,
            _ => bytes,
        }
    } else {
        bytes
    };
    let text: String = payload
        .iter()
        .copied()
        .take_while(|b| *b != 0)
        .filter(|b| b.is_ascii_graphic() || *b == b' ')
        .map(char::from)
        .collect();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse `+HH:MM` / `-HH:MM` into minutes east of UTC.
pub fn parse_utc_offset_minutes(text: &str) -> Option<i32> {
    let text = text.trim();
    let (sign, rest) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    let (h, m) = rest.split_once(':')?;
    let hours: i32 = h.parse().ok()?;
    let minutes: i32 = m.parse().ok()?;
    if !(0..=14).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 60 + minutes))
}

/// Parse EXIF sub-second digits into nanoseconds. `"07"` is 70 ms, not 7 ns.
pub fn parse_sub_second_nanos(text: &str) -> Option<u32> {
    let digits: String = text
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    let truncated = &digits[..digits.len().min(9)];
    let value: u64 = truncated.parse().ok()?;
    let scale = 10u64.pow(9 - truncated.len() as u32);
    u32::try_from(value * scale).ok()
}

/// Parse the EXIF `YYYY:MM:DD HH:MM:SS` form.
pub fn parse_exif_datetime(text: &str) -> Option<NaiveDateTime> {
    let text = text.trim();
    let (date, time) = text.split_once(' ')?;
    let mut date_parts = date.split(':');
    let year: i32 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts
        .next()
        .unwrap_or("0")
        .split('.')
        .next()?
        .parse()
        .ok()?;
    NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second.min(59))
}

/// Attach an offset to a naive capture time.
pub fn with_offset(naive: NaiveDateTime, offset_minutes: i32) -> Option<DateTime<FixedOffset>> {
    let offset = FixedOffset::east_opt(offset_minutes * 60)?;
    offset.from_local_datetime(&naive).earliest()
}

/// A normalized slug usable as a stable identifier component.
pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_dash = true;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

/// Identified shooting mode plus the tag it came from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModeIdentification {
    /// Normalized label, for example `auto`, `master`, `hi-res` or `hdr-plus`.
    ///
    /// Part of the configuration identity, so it carries only what changes the
    /// *image pipeline*.
    pub label: Option<String>,
    /// Raw tag value the label was derived from.
    pub raw: Option<String>,
    /// Which EXIF tag was used.
    pub source: Option<String>,
    /// Capture flags that do not change the pipeline and are therefore kept out
    /// of the configuration identity: `portrait`, `burst`.
    pub flags: Vec<String>,
    /// Bits of the tag this version does not interpret.
    pub unknown_bits: Option<u64>,
    /// Why the tag was not fully understood, when it was not.
    pub note: Option<String>,
}

/// Master mode: measurably the shortest exposures of any mode.
const OPLUS_MASTER: u64 = 0x0000_0100;
/// High-resolution mode: every observed photo is 8192 × 6144 (50 MP).
const OPLUS_HIGH_RES: u64 = 0x0000_2000;
/// Portrait-orientation capture: every observed photo has portrait pixel
/// dimensions.
const OPLUS_PORTRAIT: u64 = 0x0000_0020;
/// Burst frame: 1:1 with the `_BURST` filename convention over 11 photos.
const OPLUS_BURST: u64 = 0x0400_0000;
/// Every bit this version claims to understand.
const OPLUS_KNOWN_FLAGS: u64 = OPLUS_MASTER | OPLUS_HIGH_RES | OPLUS_PORTRAIT | OPLUS_BURST;

/// Writer profiles observed with the flag bits masked off.
///
/// The remaining bits are constant per writer, not per mode: the first value
/// covers 269 HEICs across three folders and three days with every mode
/// combination appearing underneath it, and the other two appear only on files
/// that a Google Photos export re-encoded to JPEG.
///
/// A profile that is not in this list is reported rather than assumed, which is
/// why the list can stay short: an unknown profile is constant across a device,
/// so it marks that device's photos without splitting its modes from each other.
const OPLUS_KNOWN_PROFILES: [u64; 3] = [0x2010_0000, 0x2_1030_0000, 0x3_1030_0000];

/// Decode an `oplus_<u64>` mode code.
fn identify_oplus_mode(comment: &str, digits: &str) -> ModeIdentification {
    let mut out = ModeIdentification {
        raw: Some(comment.to_string()),
        source: Some("UserComment".to_string()),
        ..Default::default()
    };
    let Ok(code) = digits.parse::<u64>() else {
        // Keep the raw value: an unreadable code is a bug report, not a photo to
        // silently treat as mode-less.
        out.note = Some(format!(
            "unrecognized OPPO mode tag `{comment}`; no mode identified"
        ));
        return out;
    };

    let profile = code & !OPLUS_KNOWN_FLAGS;
    let mut parts: Vec<String> = Vec::new();
    if code & OPLUS_HIGH_RES != 0 {
        parts.push("hi-res".to_string());
    }
    if code & OPLUS_MASTER != 0 {
        parts.push("master".to_string());
    }
    if parts.is_empty() {
        // `oplus_537919488` (0x20100000), the most common value by a wide
        // margin, is the phone's plain "auto".
        parts.push("auto".to_string());
    }
    if code & OPLUS_PORTRAIT != 0 {
        out.flags.push("portrait".to_string());
    }
    if code & OPLUS_BURST != 0 {
        out.flags.push("burst".to_string());
    }

    if !OPLUS_KNOWN_PROFILES.contains(&profile) {
        out.unknown_bits = Some(profile);
        parts.push(format!("x{profile:x}"));
        out.note = Some(format!(
            "OPPO mode tag `{comment}` carries unverified bits 0x{profile:x}; \
             treated as a distinct configuration rather than folded into a known mode"
        ));
    }
    out.label = Some(parts.join("-"));
    out
}

/// Identify the shooting mode ([Q10] in `REQUIREMENTS.md`).
///
/// The reference Oppo Find X9 Pro encodes the mode in `UserComment` as
/// `oplus_<integer>`. Verified bits, with the evidence behind each
/// (`IMPLEM-DROP1.md` §2.1):
///
/// | mask         | meaning                      | in the configuration ID |
/// |--------------|------------------------------|-------------------------|
/// | `0x0100`     | Master mode                  | yes                     |
/// | `0x2000`     | high-resolution 50 MP mode   | yes                     |
/// | `0x0020`     | portrait-orientation capture | no                      |
/// | `0x04000000` | burst frame                  | no                      |
///
/// Anything else is reported, never guessed: unverified bits become part of the
/// label so they cannot silently merge two shooting modes into one
/// configuration, which would corrupt Drop 4 scoring.
///
/// Google devices expose no mode tag; their `Software` field identifies the HDR+
/// pipeline, and the build number is deliberately dropped so a firmware update
/// does not fork the configuration list.
pub fn identify_mode(raw: &RawMetadata) -> ModeIdentification {
    if let Some(comment) = raw.user_comment.as_deref() {
        let trimmed = comment.trim();
        // Case-insensitive prefix: the payload is recovered from a malformed
        // character-code prefix, so its case is not something to bet on.
        if trimmed.len() > 6 && trimmed[..6].eq_ignore_ascii_case("oplus_") {
            return identify_oplus_mode(trimmed, trimmed[6..].trim());
        }
    }
    if let Some(software) = raw.software.as_deref() {
        if software.to_ascii_lowercase().starts_with("hdr+") {
            return ModeIdentification {
                label: Some("hdr-plus".to_string()),
                raw: Some(software.to_string()),
                source: Some("Software".to_string()),
                ..Default::default()
            };
        }
    }
    ModeIdentification::default()
}

/// Portrait-orientation flag from the Oppo mode code, when present.
///
/// Not used to rotate anything — the HEIF container already carries the
/// transform — but recorded because it is the only orientation signal in files
/// whose EXIF `Orientation` is the invalid value `0`.
pub fn oplus_portrait_flag(raw: &RawMetadata) -> Option<bool> {
    let comment = raw.user_comment.as_deref()?.trim();
    if comment.len() <= 6 || !comment[..6].eq_ignore_ascii_case("oplus_") {
        return None;
    }
    let code = comment[6..].trim().parse::<u64>().ok()?;
    Some(code & OPLUS_PORTRAIT != 0)
}

// ---------------------------------------------------------------------------
// Focal length
// ---------------------------------------------------------------------------

/// Digital zoom actually in force. `0` and absent both mean "not used"; several
/// devices write `0` rather than omitting the tag.
fn effective_zoom(raw: &RawMetadata) -> f64 {
    raw.digital_zoom_ratio
        .filter(|z| z.is_finite() && *z > 0.0)
        .unwrap_or(1.0)
}

/// Devices measured to fold `DigitalZoomRatio` into `FocalLengthIn35mmFilm`.
///
/// Derived, not assumed: the implied crop factor `focalLengthIn35mm /
/// focalLength` moves in proportion to the zoom on these devices and stays
/// constant on the others (`IMPLEM-DROP1.md` §6.7). Keyed on the make because it
/// is a camera-application convention rather than a sensor property; a model
/// that does not share it under-corrects, which shows up as a rejected pair
/// rather than as a wrong grouping.
pub fn zoom_baked_into_focal_35mm(make: Option<&str>, model: Option<&str>) -> bool {
    let make = make.unwrap_or("").to_ascii_lowercase();
    let model = model.unwrap_or("").to_ascii_lowercase();
    make.starts_with("oppo") || model.starts_with("oppo")
}

/// The focal length as read, as compared, and how the two relate.
#[derive(Debug, Clone, PartialEq)]
pub struct FocalReading {
    /// `FocalLengthIn35mmFilm` exactly as reported.
    pub reported_35mm: Option<f64>,
    /// The value the focal predicate compares.
    pub effective_35mm: Option<f64>,
    /// Provenance of [`Self::effective_35mm`].
    pub source: FocalSource,
    /// `true` when the lens was at its native field of view.
    pub native_lens: bool,
    /// Explanation, when the reported and effective values differ.
    pub note: Option<String>,
}

/// Resolve the focal length to compare, under a [`FocalZoomPolicy`].
///
/// EXIF defines the 35 mm equivalent as a property of the lens, so the zoom
/// belongs in `DigitalZoomRatio`; a device that folds it in cannot be compared
/// against one that does not (`IMPLEM-DROP1.md` §6.7). Both the reported and the
/// effective value are kept, and the decision is recorded per photo, so a
/// document always says which convention produced its focal lengths.
pub fn read_focal(raw: &RawMetadata, policy: FocalZoomPolicy) -> FocalReading {
    let zoom = effective_zoom(raw);
    let native_lens = zoom <= 1.0;
    let reported = raw.focal_length_35mm.filter(|f| f.is_finite() && *f > 0.0);

    let apply = !native_lens
        && match policy {
            FocalZoomPolicy::TrustExif => false,
            FocalZoomPolicy::ApplyDigitalZoom => true,
            FocalZoomPolicy::PerDevice => {
                !zoom_baked_into_focal_35mm(raw.make.as_deref(), raw.model.as_deref())
            }
        };

    match (reported, apply) {
        (Some(f), true) => FocalReading {
            reported_35mm: Some(f),
            effective_35mm: Some(f * zoom),
            source: FocalSource::Exif35mmZoomApplied,
            native_lens,
            note: Some(format!(
                "35 mm equivalent {f:.0} mm combined with digital zoom {zoom:.2} \
                 to {:.0} mm for comparison",
                f * zoom
            )),
        },
        (Some(f), false) => FocalReading {
            reported_35mm: Some(f),
            effective_35mm: Some(f),
            source: FocalSource::Exif35mm,
            native_lens,
            note: None,
        },
        (None, _) => FocalReading {
            reported_35mm: None,
            effective_35mm: None,
            source: FocalSource::Unknown,
            native_lens,
            note: None,
        },
    }
}

/// Device identity without the mode, used for per-device clock offsets and
/// per-device scoring in Drop 4.
pub fn device_key(raw: &RawMetadata) -> String {
    match (raw.make.as_deref(), raw.model.as_deref()) {
        (_, Some(model)) if !model.is_empty() => slug(model),
        (Some(make), None) => slug(make),
        _ => "unknown-device".to_string(),
    }
}

/// Configuration identity: device plus mode (`SPEC.md` §2).
pub fn configuration_id(device_key: &str, mode_label: Option<&str>) -> String {
    match mode_label {
        Some(mode) => format!("{device_key}--{}", slug(mode)),
        None => device_key.to_string(),
    }
}

/// Human-readable configuration label.
pub fn configuration_display_name(raw: &RawMetadata, mode_label: Option<&str>) -> String {
    let device = raw
        .model
        .clone()
        .or_else(|| raw.make.clone())
        .unwrap_or_else(|| "Unknown device".to_string());
    match mode_label {
        Some(mode) => format!("{device} ({mode})"),
        None => device,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_offsets_parse_in_both_directions() {
        assert_eq!(parse_utc_offset_minutes("+02:00"), Some(120));
        assert_eq!(parse_utc_offset_minutes("-05:30"), Some(-330));
        assert_eq!(parse_utc_offset_minutes("+00:00"), Some(0));
        assert_eq!(parse_utc_offset_minutes("02:00"), None);
        assert_eq!(parse_utc_offset_minutes("+99:00"), None);
        assert_eq!(parse_utc_offset_minutes(""), None);
    }

    #[test]
    fn sub_second_digits_scale_by_position() {
        assert_eq!(parse_sub_second_nanos("0740"), Some(74_000_000));
        assert_eq!(parse_sub_second_nanos("5"), Some(500_000_000));
        assert_eq!(parse_sub_second_nanos("153"), Some(153_000_000));
        assert_eq!(parse_sub_second_nanos(""), None);
        assert_eq!(parse_sub_second_nanos("abc"), None);
    }

    #[test]
    fn exif_datetime_parses_the_colon_form() {
        let dt = parse_exif_datetime("2026:08:22 11:53:55").unwrap();
        assert_eq!(dt.to_string(), "2026-08-22 11:53:55");
        assert!(parse_exif_datetime("2026-08-22 11:53:55").is_none());
        assert!(parse_exif_datetime("garbage").is_none());
        assert!(parse_exif_datetime("2026:02:30 11:53:55").is_none());
    }

    #[test]
    fn a_leap_second_does_not_break_parsing() {
        // Some cameras write :60. Clamping keeps the photo usable.
        assert!(parse_exif_datetime("2026:08:22 11:53:60").is_some());
    }

    #[test]
    fn offsets_produce_the_expected_instant() {
        let naive = parse_exif_datetime("2026:08:22 11:53:55").unwrap();
        let a = with_offset(naive, 120).unwrap();
        let b = with_offset(naive, 0).unwrap();
        assert_eq!(b.timestamp() - a.timestamp(), 7200);
        assert!(a.to_rfc3339().ends_with("+02:00"));
    }

    #[test]
    fn slugs_are_stable_and_filesystem_safe() {
        assert_eq!(slug("OPPO Find X9 Pro"), "oppo-find-x9-pro");
        assert_eq!(slug("Pixel 9 Pro XL"), "pixel-9-pro-xl");
        assert_eq!(slug("  weird__name!! "), "weird-name");
        assert_eq!(slug(""), "unknown");
        assert_eq!(slug("---"), "unknown");
    }

    fn oppo(comment: &str) -> RawMetadata {
        RawMetadata {
            make: Some("OPPO".into()),
            model: Some("OPPO Find X9 Pro".into()),
            user_comment: Some(comment.into()),
            software: Some("MediaTek Camera Application".into()),
            ..Default::default()
        }
    }

    #[test]
    fn every_observed_oppo_mode_code_decodes_as_measured() {
        // Every distinct `oplus_` value in a 357-photo survey, with the counts
        // that back each decoding (`IMPLEM-DROP1.md` §2.1). Kept as one table so
        // a change to the bit meanings has to face all of the evidence at once.
        let cases: [(&str, usize, &str, &[&str], bool); 14] = [
            // value             n     label                 flags                unknown
            ("oplus_537919488", 106, "auto", &[], false),
            ("oplus_537927680", 54, "hi-res", &[], false),
            ("oplus_537919520", 53, "auto", &["portrait"], false),
            ("oplus_537919744", 20, "master", &[], false),
            ("oplus_537919776", 20, "master", &["portrait"], false),
            ("oplus_537927712", 16, "hi-res", &["portrait"], false),
            ("oplus_605028352", 11, "auto", &["burst"], false),
            ("oplus_8861523968", 2, "hi-res", &[], false),
            ("oplus_8861515808", 2, "auto", &["portrait"], false),
            ("oplus_8861524000", 2, "hi-res", &["portrait"], false),
            ("oplus_13156483328", 2, "master", &[], false),
            ("oplus_13156483360", 2, "master", &["portrait"], false),
            // Two photos whose remaining bits are not accounted for. They are
            // reported, not folded into `auto`.
            ("oplus_672137216", 1, "auto-x28100000", &[], true),
            ("oplus_672137248", 1, "auto-x28100000", &["portrait"], true),
        ];
        for (value, count, label, flags, unknown) in cases {
            let id = identify_mode(&oppo(value));
            assert_eq!(id.label.as_deref(), Some(label), "{value} (n = {count})");
            assert_eq!(id.flags, flags, "{value} flags");
            assert_eq!(id.unknown_bits.is_some(), unknown, "{value} unknown bits");
            assert_eq!(id.raw.as_deref(), Some(value));
            assert_eq!(id.source.as_deref(), Some("UserComment"));
            assert_eq!(id.note.is_some(), unknown, "{value} note");
        }
    }

    #[test]
    fn an_unverified_mode_bit_forks_the_configuration_instead_of_being_ignored() {
        // Silently merging an unknown mode into `auto` would make Drop 4 compare
        // two different pipelines as one configuration, which nothing downstream
        // could detect. A strange configuration name is visible and correctable.
        let unknown = identify_mode(&oppo("oplus_672137216"));
        let auto = identify_mode(&oppo("oplus_537919488"));
        assert_ne!(unknown.label, auto.label);
        assert_eq!(unknown.unknown_bits, Some(0x2810_0000));
        assert!(unknown.note.unwrap().contains("0x28100000"));

        // An unknown profile is constant for a writer, so it marks a device
        // without splitting that device's own modes from each other.
        let unknown_master = identify_mode(&oppo("oplus_672137472")); // + 0x0100
        assert_eq!(unknown_master.label.as_deref(), Some("master-x28100000"));
    }

    #[test]
    fn the_burst_flag_stays_out_of_the_configuration_identity() {
        // A burst frame is still an `auto` photo: the pipeline is the same, so
        // splitting the configuration would only fragment the comparison.
        let burst = identify_mode(&oppo("oplus_605028352"));
        assert_eq!(burst.label.as_deref(), Some("auto"));
        assert_eq!(burst.flags, vec!["burst".to_string()]);
        assert!(burst.unknown_bits.is_none());
    }

    #[test]
    fn a_high_resolution_master_shot_reports_both_modes() {
        // Not observed in the survey, but the bits are independent and the label
        // must not silently drop one of them.
        let both = identify_mode(&oppo("oplus_537927936")); // 0x20102100
        assert_eq!(both.label.as_deref(), Some("hi-res-master"));
        assert!(both.unknown_bits.is_none());
    }

    #[test]
    fn a_malformed_mode_tag_keeps_its_raw_value_and_identifies_nothing() {
        let broken = identify_mode(&oppo("oplus_not-a-number"));
        assert!(broken.label.is_none());
        assert_eq!(broken.raw.as_deref(), Some("oplus_not-a-number"));
        assert!(broken.note.unwrap().contains("unrecognized"));

        // A value past u64 is malformed rather than a mode.
        let huge = identify_mode(&oppo("oplus_99999999999999999999999"));
        assert!(huge.label.is_none());
        assert!(huge.note.is_some());

        // A bare prefix carries no code.
        assert_eq!(
            identify_mode(&oppo("oplus_")),
            ModeIdentification::default()
        );
    }

    #[test]
    fn the_mode_prefix_is_matched_case_insensitively_and_trimmed() {
        // The payload is recovered from a malformed character-code prefix, so
        // its exact spelling is not something to bet the mode on.
        for value in ["OPLUS_537919744", " oplus_537919744 ", "Oplus_ 537919744"] {
            assert_eq!(
                identify_mode(&oppo(value)).label.as_deref(),
                Some("master"),
                "{value}"
            );
        }
    }

    #[test]
    fn the_portrait_bit_is_read_separately_from_the_mode() {
        assert_eq!(oplus_portrait_flag(&oppo("oplus_537919520")), Some(true));
        assert_eq!(oplus_portrait_flag(&oppo("oplus_537927712")), Some(true));
        assert_eq!(oplus_portrait_flag(&oppo("oplus_537919488")), Some(false));
        assert_eq!(oplus_portrait_flag(&oppo("oplus_537927680")), Some(false));
        assert_eq!(oplus_portrait_flag(&RawMetadata::default()), None);
    }

    #[test]
    fn mode_identification_records_its_source() {
        let id = identify_mode(&oppo("oplus_537919744"));
        assert_eq!(id.source.as_deref(), Some("UserComment"));
        assert_eq!(id.raw.as_deref(), Some("oplus_537919744"));
    }

    #[test]
    fn pixel_software_identifies_hdr_plus_without_the_build_number() {
        let raw = RawMetadata {
            make: Some("Google".into()),
            model: Some("Pixel 9 Pro XL".into()),
            software: Some("HDR+ 1.0.936816820zd".into()),
            ..Default::default()
        };
        let id = identify_mode(&raw);
        assert_eq!(id.label.as_deref(), Some("hdr-plus"));
        assert_eq!(id.source.as_deref(), Some("Software"));
    }

    #[test]
    fn an_unrecognized_comment_yields_no_mode_rather_than_a_guess() {
        let raw = RawMetadata {
            make: Some("OPPO".into()),
            model: Some("OPPO Find X9 Pro".into()),
            user_comment: Some("Labeled by PeakFinder".into()),
            software: Some("MediaTek Camera Application".into()),
            ..Default::default()
        };
        assert_eq!(identify_mode(&raw), ModeIdentification::default());
    }

    fn with_focal(
        make: &str,
        model: &str,
        focal: f64,
        focal_35mm: f64,
        zoom: Option<f64>,
    ) -> RawMetadata {
        RawMetadata {
            make: Some(make.into()),
            model: Some(model.into()),
            focal_length_mm: Some(focal),
            focal_length_35mm: Some(focal_35mm),
            digital_zoom_ratio: zoom,
            ..Default::default()
        }
    }

    #[test]
    fn a_device_that_folds_zoom_into_the_equivalent_focal_length_is_left_alone() {
        // Measured: the Oppo's implied crop factor moves with the zoom, so 35 mm
        // already includes it. Multiplying again would report 53 mm for a 35 mm
        // framing (`IMPLEM-DROP1.md` §6.7).
        let oppo = with_focal("OPPO", "OPPO Find X9 Pro", 6.75, 35.0, Some(1.52));
        let reading = read_focal(&oppo, FocalZoomPolicy::PerDevice);
        assert_eq!(reading.effective_35mm, Some(35.0));
        assert_eq!(reading.source, FocalSource::Exif35mm);
        assert!(!reading.native_lens);
        assert!(reading.note.is_none());
    }

    #[test]
    fn a_device_that_reports_zoom_separately_gets_it_applied() {
        // The Pixel's crop factor is constant across zoom values, so the zoom is
        // missing from the equivalent focal length and has to be combined.
        let pixel = with_focal("Google", "Pixel 9 Pro XL", 6.9, 24.0, Some(1.47));
        let reading = read_focal(&pixel, FocalZoomPolicy::PerDevice);
        assert_eq!(reading.reported_35mm, Some(24.0));
        assert_eq!(reading.effective_35mm, Some(24.0 * 1.47));
        assert_eq!(reading.source, FocalSource::Exif35mmZoomApplied);
        assert!(!reading.native_lens);
        // The correction is never silent.
        assert!(reading.note.unwrap().contains("digital zoom"));
    }

    #[test]
    fn the_zoom_policy_is_honoured_in_all_three_settings() {
        let pixel = with_focal("Google", "Pixel 9 Pro XL", 6.9, 24.0, Some(1.47));
        let oppo = with_focal("OPPO", "OPPO Find X9 Pro", 6.75, 35.0, Some(1.52));

        assert_eq!(
            read_focal(&pixel, FocalZoomPolicy::TrustExif).effective_35mm,
            Some(24.0)
        );
        assert_eq!(
            read_focal(&oppo, FocalZoomPolicy::ApplyDigitalZoom).effective_35mm,
            Some(35.0 * 1.52)
        );
        assert_eq!(
            read_focal(&oppo, FocalZoomPolicy::PerDevice).effective_35mm,
            Some(35.0)
        );
    }

    #[test]
    fn a_zoom_of_zero_or_one_means_the_lens_is_at_its_native_field_of_view() {
        // Several devices write 0 rather than omitting the tag.
        for zoom in [None, Some(0.0), Some(1.0)] {
            let raw = with_focal("Google", "Pixel 9 Pro XL", 2.02, 12.0, zoom);
            let reading = read_focal(&raw, FocalZoomPolicy::PerDevice);
            assert!(reading.native_lens, "{zoom:?}");
            assert_eq!(reading.effective_35mm, Some(12.0), "{zoom:?}");
            assert_eq!(reading.source, FocalSource::Exif35mm);
        }
        // A non-finite ratio is not a reason to rewrite a focal length.
        let odd = with_focal("Google", "Pixel 9 Pro XL", 2.02, 12.0, Some(f64::NAN));
        assert!(read_focal(&odd, FocalZoomPolicy::PerDevice).native_lens);
    }

    #[test]
    fn a_missing_equivalent_focal_length_is_not_invented_from_the_zoom() {
        let raw = RawMetadata {
            make: Some("Google".into()),
            model: Some("Pixel 9 Pro XL".into()),
            focal_length_mm: Some(6.9),
            digital_zoom_ratio: Some(2.0),
            ..Default::default()
        };
        let reading = read_focal(&raw, FocalZoomPolicy::PerDevice);
        assert!(reading.effective_35mm.is_none());
        assert_eq!(reading.source, FocalSource::Unknown);
    }

    #[test]
    fn the_zoom_device_table_is_matched_on_make_or_model() {
        assert!(zoom_baked_into_focal_35mm(
            Some("OPPO"),
            Some("OPPO Find X9 Pro")
        ));
        assert!(zoom_baked_into_focal_35mm(None, Some("OPPO Find X9 Pro")));
        assert!(!zoom_baked_into_focal_35mm(
            Some("Google"),
            Some("Pixel 9 Pro XL")
        ));
        assert!(!zoom_baked_into_focal_35mm(None, None));
    }

    #[test]
    fn configuration_identity_separates_modes_of_one_device() {
        let raw = oppo("oplus_537919744");
        let device = device_key(&raw);
        assert_eq!(device, "oppo-find-x9-pro");
        let master = configuration_id(&device, Some("master"));
        let photo = configuration_id(&device, Some("photo"));
        let unknown = configuration_id(&device, None);
        assert_ne!(master, photo);
        assert_ne!(master, unknown);
        assert_eq!(master, "oppo-find-x9-pro--master");
        assert_eq!(
            configuration_display_name(&raw, Some("master")),
            "OPPO Find X9 Pro (master)"
        );
    }

    #[test]
    fn a_file_without_exif_is_reported_not_guessed() {
        let raw = ExifMetadataReader.read(b"definitely not an image");
        assert!(raw.date_time.is_none());
        assert!(raw.make.is_none());
        assert_eq!(raw.notes.len(), 1);
        assert!(raw.notes[0].contains("no readable EXIF"));
    }
}
