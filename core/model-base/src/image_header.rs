// SPDX-License-Identifier: MIT
//! The size of an image file from its header (mitcad#782, image
//! references): PNG, JPEG, BMP and WebP, the formats the application reads
//! in every build, and HEIF (mitcad#850, `image_header/heif.rs`:
//! HEVC-coded still images, which builds with the HEIF decoder read). Only
//! the header is read: the
//! format, the width and height in pixels as the image is shown, and the
//! size of a pixel where the file states one (PNG's `pHYs`, JPEG's JFIF
//! density, BMP's pixels per metre). The pixels themselves are decoded by
//! the application when it draws the image.

mod heif;

/// The largest width or height taken, in pixels.
pub const MAX_SIDE: u32 = 32_768;

/// The size of a pixel (mm) of a file that states none: 96 pixels to the
/// inch.
pub const DEFAULT_PIXEL_SIZE: f64 = 25.4 / 96.0;

/// What an image file's header says.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageHeader {
    /// `png`, `jpeg`, `bmp`, `webp` or `heif`.
    pub format: &'static str,
    pub width: u32,
    pub height: u32,
    /// The size of a pixel across (mm) the file states, if it states one.
    pub pixel_size: Option<f64>,
}

impl ImageHeader {
    /// The size of a pixel (mm): the file's own, else 96 to the inch.
    pub fn pixel_size_or_default(&self) -> f64 {
        self.pixel_size.unwrap_or(DEFAULT_PIXEL_SIZE)
    }
}

/// The header of a PNG, JPEG, BMP, WebP or HEIF file, or why it is none of
/// them (or one without a size, larger than [`MAX_SIDE`], or a HEIF file
/// MitCad does not read).
pub fn read(bytes: &[u8]) -> Result<ImageHeader, String> {
    let header = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        jpeg(bytes)
    } else if bytes.starts_with(b"BM") {
        bmp(bytes)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        webp(bytes)
    } else if heif::is_heif(bytes) {
        heif::read(bytes)
    } else {
        return Err("the file is no PNG, JPEG, BMP, WebP or HEIF image".to_owned());
    }?;
    if header.width == 0 || header.height == 0 {
        return Err(format!("the {} image has no pixels", header.format));
    }
    if header.width > MAX_SIDE || header.height > MAX_SIDE {
        return Err(format!(
            "the {} image is {} × {} pixels, more than {MAX_SIDE} on a side",
            header.format, header.width, header.height
        ));
    }
    Ok(header)
}

fn bytes_at<const N: usize>(bytes: &[u8], at: usize) -> Option<[u8; N]> {
    bytes.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn u16_be(bytes: &[u8], at: usize) -> Option<u32> {
    bytes_at(bytes, at).map(|b| u32::from(u16::from_be_bytes(b)))
}

fn u32_be(bytes: &[u8], at: usize) -> Option<u32> {
    bytes_at(bytes, at).map(u32::from_be_bytes)
}

fn u16_le(bytes: &[u8], at: usize) -> Option<u32> {
    bytes_at(bytes, at).map(|b| u32::from(u16::from_le_bytes(b)))
}

fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    bytes_at(bytes, at).map(u32::from_le_bytes)
}

fn i32_le(bytes: &[u8], at: usize) -> Option<i32> {
    bytes_at(bytes, at).map(i32::from_le_bytes)
}

fn u24_le(bytes: &[u8], at: usize) -> Option<u32> {
    bytes_at::<3>(bytes, at)
        .map(|b| u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16))
}

fn cut(format: &str) -> String {
    format!("the {format} file ends inside its header")
}

/// Pixels per metre as a pixel's size, when it is one.
fn per_metre(count: i64) -> Option<f64> {
    if count > 0 {
        Some(1000.0 / count as f64)
    } else {
        None
    }
}

/// IHDR's size; pHYs (in metres) before the first IDAT.
fn png(bytes: &[u8]) -> Result<ImageHeader, String> {
    if bytes.get(12..16) != Some(b"IHDR".as_slice()) {
        return Err("the PNG file does not start with its IHDR chunk".to_owned());
    }
    let width = u32_be(bytes, 16).ok_or_else(|| cut("PNG"))?;
    let height = u32_be(bytes, 20).ok_or_else(|| cut("PNG"))?;
    let mut pixel_size = None;
    let mut at = 8usize;
    while let (Some(length), Some(kind)) = (u32_be(bytes, at), bytes.get(at + 4..at + 8)) {
        if kind == b"IDAT" || kind == b"IEND" {
            break;
        }
        if kind == b"pHYs" && length >= 9 && bytes.get(at + 16) == Some(&1) {
            let count = u32_be(bytes, at + 8);
            pixel_size = count.and_then(|x| per_metre(i64::from(x)));
        }
        match at.checked_add(12 + length as usize) {
            Some(next) => at = next,
            None => break,
        }
    }
    Ok(ImageHeader {
        format: "png",
        width,
        height,
        pixel_size,
    })
}

/// The first start of frame's size; the JFIF segment's density.
fn jpeg(bytes: &[u8]) -> Result<ImageHeader, String> {
    let no_frame = || "the JPEG file has no frame size before its image data".to_owned();
    let mut pixel_size = None;
    let mut at = 2usize;
    loop {
        // Fill bytes before a marker.
        while bytes.get(at) == Some(&0xff) && bytes.get(at + 1) == Some(&0xff) {
            at += 1;
        }
        let (Some(&0xff), Some(&marker)) = (bytes.get(at), bytes.get(at + 1)) else {
            return Err(no_frame());
        };
        at += 2;
        // Markers without a length.
        if marker == 0x01 || (0xd0..=0xd8).contains(&marker) {
            continue;
        }
        if marker == 0xd9 || marker == 0xda {
            return Err(no_frame());
        }
        let length = u16_be(bytes, at).ok_or_else(|| cut("JPEG"))? as usize;
        if length < 2 {
            return Err("the JPEG file has a segment shorter than its length".to_owned());
        }
        let segment = bytes.get(at + 2..at + length).ok_or_else(|| cut("JPEG"))?;
        // The starts of frames: C0 to CF but the tables C4, C8 and CC.
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let height = u16_be(segment, 1).ok_or_else(|| cut("JPEG"))?;
            let width = u16_be(segment, 3).ok_or_else(|| cut("JPEG"))?;
            return Ok(ImageHeader {
                format: "jpeg",
                width,
                height,
                pixel_size,
            });
        }
        if marker == 0xe0 && segment.starts_with(b"JFIF\0") {
            let density = u16_be(segment, 8).filter(|d| *d > 0).map(f64::from);
            pixel_size = match (segment.get(7), density) {
                (Some(1), Some(per_inch)) => Some(25.4 / per_inch),
                (Some(2), Some(per_cm)) => Some(10.0 / per_cm),
                _ => pixel_size,
            };
        }
        at += length;
    }
}

/// The bitmap info header's size and pixels per metre.
fn bmp(bytes: &[u8]) -> Result<ImageHeader, String> {
    let info = u32_le(bytes, 14).ok_or_else(|| cut("BMP"))?;
    let (width, height, pixel_size) = if info == 12 {
        // The old core header: 16-bit sizes, no density.
        let width = u16_le(bytes, 18).ok_or_else(|| cut("BMP"))?;
        let height = u16_le(bytes, 20).ok_or_else(|| cut("BMP"))?;
        (width, height, None)
    } else if info >= 40 {
        let width = i32_le(bytes, 18).ok_or_else(|| cut("BMP"))?;
        // Negative: the rows top down.
        let height = i32_le(bytes, 22).ok_or_else(|| cut("BMP"))?;
        let density = i32_le(bytes, 38).ok_or_else(|| cut("BMP"))?;
        let Ok(width) = u32::try_from(width) else {
            return Err("the BMP image has a negative width".to_owned());
        };
        (width, height.unsigned_abs(), per_metre(i64::from(density)))
    } else {
        return Err(format!(
            "the BMP file's header of {info} bytes is not known"
        ));
    };
    Ok(ImageHeader {
        format: "bmp",
        width,
        height,
        pixel_size,
    })
}

/// The first chunk's size: lossy (`VP8 `), lossless (`VP8L`) or extended
/// (`VP8X`, the canvas).
fn webp(bytes: &[u8]) -> Result<ImageHeader, String> {
    let kind = bytes.get(12..16).ok_or_else(|| cut("WebP"))?;
    let (width, height) = match kind {
        b"VP8 " => {
            if bytes.get(23..26) != Some([0x9d, 0x01, 0x2a].as_slice()) {
                return Err("the WebP file's lossy frame has no start code".to_owned());
            }
            let width = u16_le(bytes, 26).ok_or_else(|| cut("WebP"))?;
            let height = u16_le(bytes, 28).ok_or_else(|| cut("WebP"))?;
            (width & 0x3fff, height & 0x3fff)
        }
        b"VP8L" => {
            if bytes.get(20) != Some(&0x2f) {
                return Err("the WebP file's lossless frame has no signature".to_owned());
            }
            let bits = u32_le(bytes, 21).ok_or_else(|| cut("WebP"))?;
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1)
        }
        b"VP8X" => {
            let width = u24_le(bytes, 24).ok_or_else(|| cut("WebP"))?;
            let height = u24_le(bytes, 27).ok_or_else(|| cut("WebP"))?;
            (width + 1, height + 1)
        }
        _ => return Err("the WebP file's first chunk is not an image".to_owned()),
    };
    Ok(ImageHeader {
        format: "webp",
        width,
        height,
        pixel_size: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(bytes: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        // The CRC is not read.
        bytes.extend_from_slice(&[0, 0, 0, 0]);
    }

    fn png_file(width: u32, height: u32, per_metre: Option<u32>) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = width.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
        chunk(&mut bytes, b"IHDR", &ihdr);
        if let Some(count) = per_metre {
            let mut phys = count.to_be_bytes().to_vec();
            phys.extend_from_slice(&count.to_be_bytes());
            phys.push(1);
            chunk(&mut bytes, b"pHYs", &phys);
        }
        chunk(&mut bytes, b"IDAT", &[0; 4]);
        chunk(&mut bytes, b"IEND", &[]);
        bytes
    }

    fn bmp_file(width: i32, height: i32, per_metre: i32) -> Vec<u8> {
        let mut bytes = b"BM".to_vec();
        bytes.extend_from_slice(&[0; 12]);
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&[1, 0, 24, 0]);
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&per_metre.to_le_bytes());
        bytes.extend_from_slice(&per_metre.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        bytes
    }

    fn jpeg_file(width: u16, height: u16, jfif: Option<(u8, u16)>) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        if let Some((units, density)) = jfif {
            bytes.extend_from_slice(&[0xff, 0xe0, 0, 16]);
            bytes.extend_from_slice(b"JFIF\0\x01\x02");
            bytes.push(units);
            bytes.extend_from_slice(&density.to_be_bytes());
            bytes.extend_from_slice(&density.to_be_bytes());
            bytes.extend_from_slice(&[0, 0]);
        }
        // A quantisation table first: a segment that is not a frame.
        bytes.extend_from_slice(&[0xff, 0xdb, 0, 4, 0, 0]);
        bytes.extend_from_slice(&[0xff, 0xc0, 0, 11, 8]);
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&[1, 1, 0x11, 0]);
        bytes.extend_from_slice(&[0xff, 0xd9]);
        bytes
    }

    fn webp_file(kind: &[u8], data: &[u8]) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(data);
        bytes
    }

    fn pixel_size(bytes: &[u8]) -> Option<f64> {
        read(bytes).unwrap().pixel_size
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn png_sizes_and_densities_are_read() {
        let header = read(&png_file(200, 100, None)).unwrap();
        assert_eq!(header.format, "png");
        assert_eq!((header.width, header.height), (200, 100));
        assert_eq!(header.pixel_size, None);
        assert!(near(header.pixel_size_or_default(), 25.4 / 96.0));
        // 4000 pixels per metre: 0.25 mm each.
        assert!(near(
            pixel_size(&png_file(200, 100, Some(4000))).unwrap(),
            0.25
        ));
    }

    #[test]
    fn jpeg_sizes_and_densities_are_read() {
        let header = read(&jpeg_file(640, 480, None)).unwrap();
        assert_eq!(header.format, "jpeg");
        assert_eq!((header.width, header.height), (640, 480));
        assert_eq!(header.pixel_size, None);
        // 254 dots per inch: 0.1 mm; 40 per cm: 0.25 mm; an aspect only: none.
        assert!(near(
            pixel_size(&jpeg_file(4, 3, Some((1, 254)))).unwrap(),
            0.1
        ));
        assert!(near(
            pixel_size(&jpeg_file(4, 3, Some((2, 40)))).unwrap(),
            0.25
        ));
        assert_eq!(pixel_size(&jpeg_file(4, 3, Some((0, 1)))), None);
        let mut cut = jpeg_file(4, 3, None);
        cut.truncate(8);
        assert!(read(&cut).is_err());
    }

    #[test]
    fn bmp_sizes_and_densities_are_read() {
        let header = read(&bmp_file(200, -100, 2000)).unwrap();
        assert_eq!(header.format, "bmp");
        assert_eq!((header.width, header.height), (200, 100));
        assert!(near(header.pixel_size.unwrap(), 0.5));
        assert_eq!(pixel_size(&bmp_file(20, 10, 0)), None);
        assert!(read(&bmp_file(-20, 10, 0)).is_err());
    }

    #[test]
    fn webp_sizes_are_read() {
        let mut lossy = vec![0, 0, 0, 0x9d, 0x01, 0x2a];
        lossy.extend_from_slice(&300u16.to_le_bytes());
        lossy.extend_from_slice(&150u16.to_le_bytes());
        let header = read(&webp_file(b"VP8 ", &lossy)).unwrap();
        assert_eq!(header.format, "webp");
        assert_eq!((header.width, header.height), (300, 150));
        let bits: u32 = 299 | (149 << 14);
        let mut lossless = vec![0x2f];
        lossless.extend_from_slice(&bits.to_le_bytes());
        let header = read(&webp_file(b"VP8L", &lossless)).unwrap();
        assert_eq!((header.width, header.height), (300, 150));
        let extended: [u8; 10] = [0, 0, 0, 0, 43, 1, 0, 149, 0, 0];
        let header = read(&webp_file(b"VP8X", &extended)).unwrap();
        assert_eq!((header.width, header.height), (300, 150));
    }

    #[test]
    fn other_files_are_refused() {
        let error = read(b"GIF89a").unwrap_err();
        assert!(error.contains("no PNG, JPEG, BMP, WebP or HEIF"), "{error}");
        let error = read(&png_file(0, 10, None)).unwrap_err();
        assert!(error.contains("no pixels"), "{error}");
        let error = read(&png_file(40_000, 10, None)).unwrap_err();
        assert!(error.contains("more than 32768"), "{error}");
        assert!(read(&png_file(10, 10, None)[..20]).is_err());
    }
}
