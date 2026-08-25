//! Minimal ISO-BMFF walker that recovers the EXIF payload from a HEIF file.
//!
//! Needed because `kamadak-exif` refuses a container EXIF block larger than
//! 64 KiB, and every reference Oppo HEIC exceeds that: the block carries a
//! ~62 KiB embedded JPEG thumbnail. Without this, HEIC photos lose their
//! timestamp, focal length and mode tag, which is most of Drop 1's input.
//!
//! Only the boxes needed to locate one item are parsed — `meta`, `iinf`, `infe`,
//! `iloc`, `idat`. The pixel data is `heif-oxide`'s job.

/// Largest EXIF payload accepted, as a guard against a malformed `iloc`.
const MAX_PAYLOAD: usize = 8 * 1024 * 1024;

/// Read-only big-endian cursor over a byte slice.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let b = self.take(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Option<u64> {
        let b = self.take(8)?;
        Some(u64::from_be_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// Read an unsigned integer of `size` bytes (0, 4 or 8 per the spec).
    fn sized(&mut self, size: u8) -> Option<u64> {
        match size {
            0 => Some(0),
            4 => self.u32().map(u64::from),
            8 => self.u64(),
            _ => None,
        }
    }

    /// Skip a null-terminated string.
    fn skip_cstr(&mut self) {
        while let Some(b) = self.u8() {
            if b == 0 {
                return;
            }
        }
    }

    /// Version and flags of a FullBox.
    fn fullbox_header(&mut self) -> Option<(u8, u32)> {
        let v = self.u32()?;
        Some(((v >> 24) as u8, v & 0x00FF_FFFF))
    }
}

/// One box: its four-character type and its payload.
struct BoxRef<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
}

/// Iterate the boxes in a buffer.
fn boxes<'a>(data: &'a [u8]) -> Vec<BoxRef<'a>> {
    let mut out = Vec::new();
    let mut cursor = Cursor::new(data);
    while cursor.remaining() >= 8 {
        let start = cursor.pos;
        let Some(size32) = cursor.u32() else { break };
        let Some(kind_bytes) = cursor.take(4) else {
            break;
        };
        let mut kind = [0u8; 4];
        kind.copy_from_slice(kind_bytes);

        let total: usize = match size32 {
            // `0` means "extends to the end of the enclosing buffer".
            0 => data.len() - start,
            // `1` means a 64-bit size follows the type.
            1 => match cursor.u64() {
                Some(large) if large >= 16 => match usize::try_from(large) {
                    Ok(v) => v,
                    Err(_) => break,
                },
                _ => break,
            },
            n if n >= 8 => n as usize,
            _ => break,
        };
        let end = match start.checked_add(total) {
            Some(end) if end <= data.len() => end,
            // Truncated box: take whatever is left rather than reading past it.
            _ => data.len(),
        };
        if cursor.pos > end {
            break;
        }
        out.push(BoxRef {
            kind,
            payload: &data[cursor.pos..end],
        });
        if end <= start {
            break;
        }
        cursor.pos = end;
    }
    out
}

fn find<'a>(list: &'a [BoxRef<'a>], kind: &[u8; 4]) -> Option<&'a BoxRef<'a>> {
    list.iter().find(|b| &b.kind == kind)
}

/// The item ID whose `infe` declares `item_type == "Exif"`.
fn exif_item_id(iinf: &[u8]) -> Option<u32> {
    let mut cursor = Cursor::new(iinf);
    let (version, _flags) = cursor.fullbox_header()?;
    let count = if version == 0 {
        cursor.u16()? as u32
    } else {
        cursor.u32()?
    };
    let rest = &iinf[cursor.pos..];
    for entry in boxes(rest).iter().take(count.max(1) as usize + 8) {
        if &entry.kind != b"infe" {
            continue;
        }
        let mut c = Cursor::new(entry.payload);
        let (v, _f) = match c.fullbox_header() {
            Some(v) => v,
            None => continue,
        };
        let item_id: u32 = match v {
            0..=2 => match c.u16() {
                Some(v) => v as u32,
                None => continue,
            },
            _ => match c.u32() {
                Some(v) => v,
                None => continue,
            },
        };
        if c.u16().is_none() {
            continue;
        } // protection_index
        if v >= 2 {
            match c.take(4) {
                Some(t) if t == b"Exif" => return Some(item_id),
                _ => continue,
            }
        } else {
            // Versions 0 and 1 carry item_name / content_type strings instead of
            // a four-character type; the EXIF item declares the MIME type.
            c.skip_cstr();
            let content_type = entry.payload.get(c.pos..).unwrap_or(&[]);
            if content_type.starts_with(b"image/exif") || content_type.starts_with(b"Exif") {
                return Some(item_id);
            }
        }
    }
    None
}

/// Extents `(offset, length)` for an item, plus the construction method.
fn item_extents(iloc: &[u8], item_id: u32) -> Option<(u8, Vec<(u64, u64)>)> {
    let mut c = Cursor::new(iloc);
    let (version, _flags) = c.fullbox_header()?;
    let sizes = c.u8()?;
    let offset_size = sizes >> 4;
    let length_size = sizes & 0x0F;
    let base = c.u8()?;
    let base_offset_size = base >> 4;
    let index_size = if version >= 1 { base & 0x0F } else { 0 };
    let item_count = if version < 2 {
        c.u16()? as u32
    } else {
        c.u32()?
    };

    for _ in 0..item_count {
        let id = if version < 2 {
            c.u16()? as u32
        } else {
            c.u32()?
        };
        let construction_method = if version >= 1 {
            (c.u16()? & 0x0F) as u8
        } else {
            0
        };
        let _data_reference_index = c.u16()?;
        let base_offset = c.sized(base_offset_size)?;
        let extent_count = c.u16()?;
        let mut extents = Vec::with_capacity(extent_count as usize);
        for _ in 0..extent_count {
            if index_size > 0 {
                c.sized(index_size)?;
            }
            let offset = c.sized(offset_size)?;
            let length = c.sized(length_size)?;
            extents.push((base_offset.checked_add(offset)?, length));
        }
        if id == item_id {
            return Some((construction_method, extents));
        }
    }
    None
}

/// Recover the TIFF-structured EXIF payload from a HEIF file.
///
/// Returns the bytes starting at the TIFF byte-order marker, ready for
/// `exif::Reader::read_raw`. `None` when the file carries no EXIF item or the
/// structure is not something this walker understands — never a panic, because
/// the input is untrusted.
pub fn extract_exif_tiff(bytes: &[u8]) -> Option<Vec<u8>> {
    let top = boxes(bytes);
    let meta = find(&top, b"meta")?;
    // `meta` is a FullBox: skip its version and flags before reading children.
    let meta_children = boxes(meta.payload.get(4..)?);
    let iinf = find(&meta_children, b"iinf")?;
    let iloc = find(&meta_children, b"iloc")?;
    let item_id = exif_item_id(iinf.payload)?;
    let (construction_method, extents) = item_extents(iloc.payload, item_id)?;

    let mut payload: Vec<u8> = Vec::new();
    match construction_method {
        // 0: offsets are absolute within the file.
        0 => {
            for (offset, length) in extents {
                let start = usize::try_from(offset).ok()?;
                let slice = if length == 0 {
                    bytes.get(start..)?
                } else {
                    let end = start.checked_add(usize::try_from(length).ok()?)?;
                    bytes.get(start..end)?
                };
                if payload.len() + slice.len() > MAX_PAYLOAD {
                    return None;
                }
                payload.extend_from_slice(slice);
            }
        }
        // 1: offsets are relative to the `idat` box.
        1 => {
            let idat = find(&meta_children, b"idat")?;
            for (offset, length) in extents {
                let start = usize::try_from(offset).ok()?;
                let slice = if length == 0 {
                    idat.payload.get(start..)?
                } else {
                    let end = start.checked_add(usize::try_from(length).ok()?)?;
                    idat.payload.get(start..end)?
                };
                if payload.len() + slice.len() > MAX_PAYLOAD {
                    return None;
                }
                payload.extend_from_slice(slice);
            }
        }
        // 2 is construction by item reference, which no still image needs.
        _ => return None,
    }

    // The item payload begins with a 32-bit offset to the TIFF header
    // (ISO/IEC 23008-12 §A.2.1), almost always zero.
    if payload.len() < 8 {
        return None;
    }
    let tiff_offset = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
    let start = 4usize.checked_add(tiff_offset)?;
    let tiff = payload.get(start..)?;
    // Sanity-check the byte-order marker so a misparse fails here rather than
    // producing plausible-looking nonsense.
    if !(tiff.starts_with(b"II\x2a\x00") || tiff.starts_with(b"MM\x00\x2a")) {
        return None;
    }
    Some(tiff.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 8);
        out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// Minimal but structurally valid HEIF carrying one EXIF item.
    fn synthetic_heif(tiff: &[u8], oversized_thumbnail: usize) -> Vec<u8> {
        let mut item_payload = vec![0u8, 0, 0, 0]; // tiff_header_offset = 0
        item_payload.extend_from_slice(tiff);
        item_payload.extend_from_slice(&vec![0xAA; oversized_thumbnail]);

        // infe, version 2: item_ID = 7, item_type = "Exif".
        let mut infe = vec![2u8, 0, 0, 0];
        infe.extend_from_slice(&7u16.to_be_bytes());
        infe.extend_from_slice(&0u16.to_be_bytes());
        infe.extend_from_slice(b"Exif");
        infe.push(0); // empty item_name
        let infe = make_box(b"infe", &infe);

        let mut iinf = vec![0u8, 0, 0, 0];
        iinf.extend_from_slice(&1u16.to_be_bytes());
        iinf.extend_from_slice(&infe);
        let iinf = make_box(b"iinf", &iinf);

        // iloc version 0: offset_size = 4, length_size = 4, base_offset_size = 0.
        let build_iloc = |absolute_offset: u32| {
            let mut iloc = vec![0u8, 0, 0, 0];
            iloc.push(0x44);
            iloc.push(0x00);
            iloc.extend_from_slice(&1u16.to_be_bytes());
            iloc.extend_from_slice(&7u16.to_be_bytes());
            iloc.extend_from_slice(&0u16.to_be_bytes());
            iloc.extend_from_slice(&1u16.to_be_bytes());
            iloc.extend_from_slice(&absolute_offset.to_be_bytes());
            iloc.extend_from_slice(&(item_payload.len() as u32).to_be_bytes());
            make_box(b"iloc", &iloc)
        };

        let mut ftyp = Vec::new();
        ftyp.extend_from_slice(b"heic");
        ftyp.extend_from_slice(&0u32.to_be_bytes());
        ftyp.extend_from_slice(b"mif1heic");
        let ftyp = make_box(b"ftyp", &ftyp);

        // Two passes: the iloc offset depends on the size of the boxes preceding
        // the payload, and the iloc size is fixed, so one probe is enough.
        let probe_iloc = build_iloc(0);
        let mut meta_payload = vec![0u8, 0, 0, 0];
        meta_payload.extend_from_slice(&iinf);
        meta_payload.extend_from_slice(&probe_iloc);
        let meta_len = make_box(b"meta", &meta_payload).len();
        let payload_offset = (ftyp.len() + meta_len + 8) as u32; // + mdat header

        let iloc = build_iloc(payload_offset);
        let mut meta_payload = vec![0u8, 0, 0, 0];
        meta_payload.extend_from_slice(&iinf);
        meta_payload.extend_from_slice(&iloc);
        let meta = make_box(b"meta", &meta_payload);

        let mut file = ftyp;
        file.extend_from_slice(&meta);
        file.extend_from_slice(&make_box(b"mdat", &item_payload));
        file
    }

    /// A tiny valid little-endian TIFF with one IFD entry.
    fn tiny_tiff() -> Vec<u8> {
        let mut t = Vec::new();
        t.extend_from_slice(b"II\x2a\x00");
        t.extend_from_slice(&8u32.to_le_bytes()); // IFD0 offset
        t.extend_from_slice(&1u16.to_le_bytes()); // one entry
        t.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        t.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        t.extend_from_slice(&1u32.to_le_bytes()); // count
        t.extend_from_slice(&1u16.to_le_bytes()); // value = 1
        t.extend_from_slice(&0u16.to_le_bytes()); // padding
        t.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        t
    }

    #[test]
    fn the_exif_payload_is_recovered_from_a_synthetic_container() {
        let tiff = tiny_tiff();
        let file = synthetic_heif(&tiff, 0);
        let recovered = extract_exif_tiff(&file).expect("exif item located");
        assert_eq!(&recovered[..tiff.len()], &tiff[..]);
    }

    #[test]
    fn a_payload_beyond_the_container_size_limit_is_still_recovered() {
        // This is the whole point: 100 KiB of thumbnail is what makes
        // `kamadak-exif`'s container reader give up.
        let tiff = tiny_tiff();
        let file = synthetic_heif(&tiff, 100 * 1024);
        let recovered = extract_exif_tiff(&file).expect("exif item located");
        assert!(recovered.len() > 100 * 1024);
        assert!(recovered.starts_with(b"II\x2a\x00"));
    }

    #[test]
    fn the_recovered_payload_parses_as_exif() {
        let file = synthetic_heif(&tiny_tiff(), 0);
        let tiff = extract_exif_tiff(&file).unwrap();
        let exif = exif::Reader::new().read_raw(tiff).unwrap();
        let orientation = exif
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));
        assert_eq!(orientation, Some(1));
    }

    #[test]
    fn a_file_without_an_exif_item_yields_none() {
        let mut file = Vec::new();
        file.extend_from_slice(&make_box(b"ftyp", b"heicmif1"));
        file.extend_from_slice(&make_box(b"mdat", &[1, 2, 3, 4]));
        assert!(extract_exif_tiff(&file).is_none());
    }

    #[test]
    fn malformed_input_never_panics() {
        assert!(extract_exif_tiff(&[]).is_none());
        assert!(extract_exif_tiff(b"garbage").is_none());
        // Truncated boxes, absurd sizes and a truncated meta payload.
        assert!(extract_exif_tiff(&[0, 0, 0, 0]).is_none());
        assert!(extract_exif_tiff(&[0xFF, 0xFF, 0xFF, 0xFF, b'm', b'e', b't', b'a']).is_none());
        assert!(extract_exif_tiff(&[0, 0, 0, 9, b'm', b'e', b't', b'a', 0]).is_none());
        assert!(extract_exif_tiff(&[0, 0, 0, 1, b'm', b'e', b't', b'a']).is_none());

        let mut truncated = synthetic_heif(&tiny_tiff(), 0);
        for cut in [4, 12, 20, 40, 60] {
            if truncated.len() > cut {
                let slice = &truncated[..truncated.len() - cut];
                let _ = extract_exif_tiff(slice);
            }
        }
        truncated.truncate(30);
        let _ = extract_exif_tiff(&truncated);
    }

    #[test]
    fn a_payload_whose_byte_order_marker_is_wrong_is_rejected() {
        let file = synthetic_heif(b"NOTATIFFHEADER", 0);
        assert!(extract_exif_tiff(&file).is_none());
    }

    #[test]
    fn boxes_with_a_sixty_four_bit_size_are_understood() {
        let payload = vec![7u8; 24];
        let mut large = Vec::new();
        large.extend_from_slice(&1u32.to_be_bytes());
        large.extend_from_slice(b"free");
        large.extend_from_slice(&((payload.len() + 16) as u64).to_be_bytes());
        large.extend_from_slice(&payload);
        let parsed = boxes(&large);
        assert_eq!(parsed.len(), 1);
        assert_eq!(&parsed[0].kind, b"free");
        assert_eq!(parsed[0].payload, &payload[..]);
    }
}
