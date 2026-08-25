//! Decoding, orientation and analysis-image production.
//!
//! Drop 1 supports JPEG, PNG and HEIC. HEIC matters because it is what the
//! reference devices actually produce; it is decoded in pure Rust so the
//! desktop package keeps its single-runtime property (`ADR-DROP1.md` §3.6).

use std::path::Path;

use crate::domain::{AnalysisImage, GrayImage};
use crate::{Error, Result};

/// Image formats Drop 1 accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    /// JPEG.
    Jpeg,
    /// PNG.
    Png,
    /// HEIF/HEIC still image.
    Heif,
}

impl SourceFormat {
    /// Lowercase extension list this format is recognized by.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            SourceFormat::Jpeg => &["jpg", "jpeg", "jpe"],
            SourceFormat::Png => &["png"],
            SourceFormat::Heif => &["heic", "heif", "hif"],
        }
    }

    /// Guess from a file extension.
    pub fn from_extension(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        [SourceFormat::Jpeg, SourceFormat::Png, SourceFormat::Heif]
            .into_iter()
            .find(|format| format.extensions().contains(&ext.as_str()))
    }

    /// Identify from magic bytes. Preferred over the extension, which lies.
    pub fn from_magic(bytes: &[u8]) -> Option<Self> {
        if bytes.len() >= 3 && bytes[..3] == [0xFF, 0xD8, 0xFF] {
            return Some(SourceFormat::Jpeg);
        }
        if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
            return Some(SourceFormat::Png);
        }
        if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
            let brand = &bytes[8..12];
            const HEIF_BRANDS: [&[u8; 4]; 7] = [
                b"heic", b"heix", b"heim", b"heis", b"hevc", b"mif1", b"msf1",
            ];
            if HEIF_BRANDS.iter().any(|b| &brand == b) {
                return Some(SourceFormat::Heif);
            }
        }
        None
    }
}

/// Apply an EXIF orientation to a grayscale image.
///
/// Values outside `1..=8` are treated as `1`: several devices in the reference
/// set write `0`, which is not a valid orientation, and rotating on that would
/// corrupt every coordinate downstream.
pub fn apply_orientation(src: &GrayImage, orientation: u16) -> GrayImage {
    let (w, h) = src.dimensions();
    let transposed = matches!(orientation, 5..=8);
    let (dw, dh) = if transposed { (h, w) } else { (w, h) };
    if orientation == 1 || !(1..=8).contains(&orientation) {
        return src.clone();
    }
    let mut out = GrayImage::zeros(dw, dh);
    let dst = out.as_mut_slice();
    let source = src.as_slice();
    for y in 0..h {
        for x in 0..w {
            let value = source[y as usize * w as usize + x as usize];
            let (nx, ny) = match orientation {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (h - 1 - y, x),
                7 => (h - 1 - y, w - 1 - x),
                8 => (y, w - 1 - x),
                _ => (x, y),
            };
            dst[ny as usize * dw as usize + nx as usize] = value;
        }
    }
    out
}

/// Oriented dimensions for a stored size and orientation.
pub fn oriented_dimensions(dims: (u32, u32), orientation: u16) -> (u32, u32) {
    if matches!(orientation, 5..=8) {
        (dims.1, dims.0)
    } else {
        dims
    }
}

/// Target dimensions preserving aspect ratio with `longest_edge` on the longer
/// side. Never upscales.
pub fn analysis_dimensions(dims: (u32, u32), longest_edge: u32) -> (u32, u32) {
    let (w, h) = dims;
    let longest = w.max(h);
    if longest == 0 || longest <= longest_edge {
        return (w.max(1), h.max(1));
    }
    let scale = longest_edge as f64 / longest as f64;
    let nw = ((w as f64 * scale).round() as u32).max(1);
    let nh = ((h as f64 * scale).round() as u32).max(1);
    (nw, nh)
}

/// Downscale with a Catmull-Rom filter, preserving aspect ratio.
pub fn resize_gray(src: &GrayImage, target: (u32, u32)) -> GrayImage {
    if src.dimensions() == target {
        return src.clone();
    }
    let buffer: image::GrayImage =
        image::ImageBuffer::from_raw(src.width(), src.height(), src.as_slice().to_vec())
            .expect("dimensions already validated");
    let resized = image::imageops::resize(
        &buffer,
        target.0,
        target.1,
        image::imageops::FilterType::CatmullRom,
    );
    GrayImage::new(target.0, target.1, resized.into_raw()).expect("resize preserves length")
}

/// Read the stored dimensions without decoding the pixels.
///
/// Used as a pixel guard before committing to a full decode.
pub fn probe_dimensions(path: &Path, bytes: &[u8]) -> Result<(u32, u32)> {
    let format = SourceFormat::from_magic(bytes)
        .or_else(|| SourceFormat::from_extension(path))
        .ok_or_else(|| Error::UnsupportedFormat {
            path: path.to_path_buf(),
        })?;
    match format {
        SourceFormat::Jpeg | SourceFormat::Png => {
            let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| Error::Decode {
                    path: path.to_path_buf(),
                    detail: e.to_string(),
                })?;
            reader.into_dimensions().map_err(|e| Error::Decode {
                path: path.to_path_buf(),
                detail: e.to_string(),
            })
        }
        SourceFormat::Heif => {
            // No header-only path is exposed, so fall back to the EXIF pixel
            // dimensions when available and otherwise report unknown by
            // decoding. Callers use the EXIF values first.
            let decoded = heif_oxide::decode_bytes(bytes).map_err(|e| Error::Decode {
                path: path.to_path_buf(),
                detail: e.to_string(),
            })?;
            Ok((decoded.width, decoded.height))
        }
    }
}

/// Decode a source file into a grayscale analysis image.
///
/// The returned image is oriented, so every coordinate produced downstream is
/// in the oriented frame (`SPEC.md` §3.3).
pub fn load_analysis_image(
    path: &Path,
    bytes: &[u8],
    exif_orientation: u16,
    longest_edge: u32,
    max_megapixels: f64,
) -> Result<AnalysisImage> {
    let format = SourceFormat::from_magic(bytes)
        .or_else(|| SourceFormat::from_extension(path))
        .ok_or_else(|| Error::UnsupportedFormat {
            path: path.to_path_buf(),
        })?;

    let pixel_limit = (max_megapixels * 1_000_000.0) as u64;

    let (gray, original, decoder_oriented) = match format {
        SourceFormat::Jpeg | SourceFormat::Png => {
            // Read the header first so an oversized image is rejected before a
            // full decode allocates for it.
            let dims = probe_dimensions(path, bytes)?;
            guard_pixels(path, dims, pixel_limit)?;
            let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(|e| Error::Decode {
                    path: path.to_path_buf(),
                    detail: e.to_string(),
                })?;
            let decoded = reader.decode().map_err(|e| Error::Decode {
                path: path.to_path_buf(),
                detail: e.to_string(),
            })?;
            let luma = decoded.to_luma8();
            let dims = (luma.width(), luma.height());
            let gray =
                GrayImage::new(dims.0, dims.1, luma.into_raw()).ok_or_else(|| Error::Decode {
                    path: path.to_path_buf(),
                    detail: "decoded buffer length does not match dimensions".into(),
                })?;
            (gray, dims, false)
        }
        SourceFormat::Heif => {
            let decoded = heif_oxide::decode_bytes(bytes).map_err(|e| Error::Decode {
                path: path.to_path_buf(),
                detail: e.to_string(),
            })?;
            let dims = (decoded.width, decoded.height);
            guard_pixels(path, dims, pixel_limit)?;
            let gray = heif_to_gray(&decoded).ok_or_else(|| Error::Decode {
                path: path.to_path_buf(),
                detail: "HEIF pixel buffer length does not match dimensions".into(),
            })?;
            // heif-oxide applies the container's `irot`/`imir` transforms, so the
            // pixels are already oriented and the EXIF value must not be applied
            // a second time.
            (gray, dims, true)
        }
    };

    let oriented = if decoder_oriented {
        gray
    } else {
        apply_orientation(&gray, exif_orientation)
    };
    let oriented_dims = oriented.dimensions();
    let target = analysis_dimensions(oriented_dims, longest_edge);
    let analysis = resize_gray(&oriented, target);
    drop(oriented);

    Ok(AnalysisImage {
        gray: analysis,
        original_dimensions: original,
        oriented_dimensions: oriented_dims,
        exif_orientation,
        orientation_applied_by_decoder: decoder_oriented,
    })
}

fn guard_pixels(path: &Path, dims: (u32, u32), limit: u64) -> Result<()> {
    let pixels = dims.0 as u64 * dims.1 as u64;
    if pixels > limit {
        return Err(Error::ImageTooLarge {
            path: path.to_path_buf(),
            pixels,
            limit,
        });
    }
    Ok(())
}

fn heif_to_gray(decoded: &heif_oxide::DecodedImage) -> Option<GrayImage> {
    let n = decoded.width as usize * decoded.height as usize;
    let mut out = Vec::with_capacity(n);
    // Rec. 601 luma. Matches `image`'s `to_luma8`, so JPEG and HEIC sources are
    // measured on the same scale.
    let push = |out: &mut Vec<u8>, r: f32, g: f32, b: f32| {
        out.push(
            (0.299 * r + 0.587 * g + 0.114 * b)
                .round()
                .clamp(0.0, 255.0) as u8,
        );
    };
    match &decoded.pixels {
        heif_oxide::Pixels::Rgb8(v) => {
            if v.len() < n * 3 {
                return None;
            }
            for px in v.chunks_exact(3).take(n) {
                push(&mut out, px[0] as f32, px[1] as f32, px[2] as f32);
            }
        }
        heif_oxide::Pixels::Rgba8(v) => {
            if v.len() < n * 4 {
                return None;
            }
            for px in v.chunks_exact(4).take(n) {
                push(&mut out, px[0] as f32, px[1] as f32, px[2] as f32);
            }
        }
        heif_oxide::Pixels::Rgb16(v) => {
            if v.len() < n * 3 {
                return None;
            }
            for px in v.chunks_exact(3).take(n) {
                let s = 255.0 / 65535.0;
                push(
                    &mut out,
                    px[0] as f32 * s,
                    px[1] as f32 * s,
                    px[2] as f32 * s,
                );
            }
        }
        heif_oxide::Pixels::Rgba16(v) => {
            if v.len() < n * 4 {
                return None;
            }
            for px in v.chunks_exact(4).take(n) {
                let s = 255.0 / 65535.0;
                push(
                    &mut out,
                    px[0] as f32 * s,
                    px[1] as f32 * s,
                    px[2] as f32 * s,
                );
            }
        }
    }
    GrayImage::new(decoded.width, decoded.height, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32, h: u32) -> GrayImage {
        let mut data = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push((x * 10 + y) as u8);
            }
        }
        GrayImage::new(w, h, data).unwrap()
    }

    #[test]
    fn orientation_one_is_identity() {
        let img = ramp(3, 2);
        assert_eq!(apply_orientation(&img, 1), img);
    }

    #[test]
    fn invalid_orientation_is_treated_as_normal() {
        // The reference OPPO HEIC files write `0`, which is not a valid value.
        let img = ramp(3, 2);
        assert_eq!(apply_orientation(&img, 0), img);
        assert_eq!(apply_orientation(&img, 9), img);
        assert_eq!(apply_orientation(&img, 42), img);
    }

    #[test]
    fn all_eight_orientations_are_dimensionally_correct() {
        let img = ramp(4, 3);
        for o in 1..=8u16 {
            let out = apply_orientation(&img, o);
            let expected = oriented_dimensions((4, 3), o);
            assert_eq!(out.dimensions(), expected, "orientation {o}");
        }
    }

    #[test]
    fn orientation_six_rotates_ninety_degrees_clockwise() {
        // Top-left of the source must land in the top-right of the result.
        let img = ramp(4, 3);
        let out = apply_orientation(&img, 6);
        assert_eq!(out.dimensions(), (3, 4));
        let tl = img.get(0, 0).unwrap();
        assert_eq!(out.get(2, 0).unwrap(), tl);
    }

    #[test]
    fn orientation_three_is_a_half_turn() {
        let img = ramp(4, 3);
        let out = apply_orientation(&img, 3);
        assert_eq!(out.get(3, 2).unwrap(), img.get(0, 0).unwrap());
    }

    #[test]
    fn every_orientation_is_a_permutation_of_the_samples() {
        let img = ramp(5, 3);
        let mut expected: Vec<u8> = img.as_slice().to_vec();
        expected.sort_unstable();
        for o in 1..=8u16 {
            let mut got: Vec<u8> = apply_orientation(&img, o).as_slice().to_vec();
            got.sort_unstable();
            assert_eq!(got, expected, "orientation {o} lost or duplicated samples");
        }
    }

    #[test]
    fn analysis_dimensions_preserve_aspect_ratio_and_never_upscale() {
        assert_eq!(analysis_dimensions((8192, 6144), 1200), (1200, 900));
        assert_eq!(analysis_dimensions((3072, 4096), 1200), (900, 1200));
        assert_eq!(analysis_dimensions((640, 480), 1200), (640, 480));
        assert_eq!(analysis_dimensions((4080, 3072), 1200), (1200, 904));
    }

    #[test]
    fn resize_reaches_the_requested_dimensions() {
        let img = ramp(64, 48);
        let out = resize_gray(&img, analysis_dimensions(img.dimensions(), 16));
        assert_eq!(out.dimensions(), (16, 12));
    }

    #[test]
    fn magic_bytes_beat_the_extension() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(SourceFormat::from_magic(&png), Some(SourceFormat::Png));
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(SourceFormat::from_magic(&jpeg), Some(SourceFormat::Jpeg));
        let heic = *b"\0\0\0\x18ftypheic";
        assert_eq!(SourceFormat::from_magic(&heic), Some(SourceFormat::Heif));
        assert_eq!(SourceFormat::from_magic(b"not an image"), None);
    }

    #[test]
    fn extensions_are_case_insensitive() {
        assert_eq!(
            SourceFormat::from_extension(Path::new("a/B.HEIC")),
            Some(SourceFormat::Heif)
        );
        assert_eq!(
            SourceFormat::from_extension(Path::new("a/B.JpEg")),
            Some(SourceFormat::Jpeg)
        );
        assert_eq!(SourceFormat::from_extension(Path::new("a/B.dng")), None);
    }

    #[test]
    fn an_unsupported_buffer_is_rejected_without_panicking() {
        let err = load_analysis_image(Path::new("x.txt"), b"hello", 1, 1200, 120.0).unwrap_err();
        assert_eq!(err.code(), "unsupported-format");
    }

    #[test]
    fn a_truncated_jpeg_is_a_decode_error_not_a_panic() {
        let err = load_analysis_image(
            Path::new("x.jpg"),
            &[0xFF, 0xD8, 0xFF, 0xE0, 0x00],
            1,
            1200,
            120.0,
        )
        .unwrap_err();
        assert_eq!(err.code(), "decode");
    }

    #[test]
    fn the_pixel_guard_rejects_an_oversized_image() {
        // A 64x64 PNG against a 0.001 megapixel guard.
        let mut png = Vec::new();
        let buffer = image::GrayImage::from_raw(64, 64, vec![7; 64 * 64]).unwrap();
        image::DynamicImage::ImageLuma8(buffer)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let err = load_analysis_image(Path::new("x.png"), &png, 1, 1200, 0.001).unwrap_err();
        assert_eq!(err.code(), "image-too-large");
    }

    #[test]
    fn a_png_round_trips_through_the_loader() {
        let mut png = Vec::new();
        let buffer = ramp(40, 20);
        let img = image::GrayImage::from_raw(40, 20, buffer.as_slice().to_vec()).unwrap();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let loaded = load_analysis_image(Path::new("x.png"), &png, 1, 1200, 120.0).unwrap();
        assert_eq!(loaded.original_dimensions, (40, 20));
        assert_eq!(loaded.oriented_dimensions, (40, 20));
        assert_eq!(loaded.gray.dimensions(), (40, 20));
        assert!(!loaded.orientation_applied_by_decoder);
    }

    #[test]
    fn orientation_is_applied_once_for_non_heif_sources() {
        let mut png = Vec::new();
        let img = image::GrayImage::from_raw(40, 20, ramp(40, 20).as_slice().to_vec()).unwrap();
        image::DynamicImage::ImageLuma8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let loaded = load_analysis_image(Path::new("x.png"), &png, 6, 1200, 120.0).unwrap();
        assert_eq!(loaded.original_dimensions, (40, 20));
        assert_eq!(loaded.oriented_dimensions, (20, 40));
        assert_eq!(loaded.gray.dimensions(), (20, 40));
    }
}
