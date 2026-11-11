// SPDX-License-Identifier: MIT
//! A title block's images (mitcad#1625) as PDF image objects: a JPEG file
//! as it is (`DCTDecode`), a PNG file decoded (any colour type, bit depth
//! and interlacing) into 8-bit grey or RGB samples with its transparency
//! as a soft mask, both compressed again (`FlateDecode`). An SVG image is
//! no image object: the writer draws its paths ([`super::svg_image`],
//! mitcad#1627). What an export leaves out of the images, it says
//! ([`export_warnings`]).

use super::sheet::{Item, Sheet};
use super::svg_image;
use super::template::ImageKind;
use mitcad_model::appearance::ImageData;

/// An image as PDF objects: the image's dictionary entries (without its
/// `/Length` and `/SMask`) and its stream, and its soft mask's.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfImage {
    pub entries: String,
    pub stream: Vec<u8>,
    pub mask: Option<(String, Vec<u8>)>,
}

/// Whether a PDF holds an image of this kind as an image.
pub fn embeds(kind: ImageKind) -> bool {
    matches!(kind, ImageKind::Png | ImageKind::Jpeg)
}

/// What an export of these sheets in this format (`pdf`, `svg`, `dxf`,
/// `dwg`) leaves out of their images, in words for the user: one warning
/// per kind of loss. A DXF or DWG file holds no raster images (a PNG or
/// JPEG image is its outline there); an SVG image is written as its paths
/// to PDF, DXF and DWG (mitcad#1627) but for one usvg cannot read (its
/// outline) and its pictures, its text as its glyphs' outlines (mitcad#1637)
/// but for text in fonts this computer lacks (drawn in the bundled font)
/// and characters no font has ([`svg_image`]). The SVG export keeps every
/// image as it is.
pub fn export_warnings<'a>(
    sheets: impl IntoIterator<Item = &'a Sheet>,
    format: &str,
) -> Vec<String> {
    if !matches!(format, "pdf" | "dxf" | "dwg") {
        return Vec::new();
    }
    let mut raster = false;
    let mut unread = false;
    let mut fonts: Vec<String> = Vec::new();
    let mut characters = false;
    let mut pictures = false;
    let mut seen: Vec<&ImageData> = Vec::new();
    for item in sheets
        .into_iter()
        .flat_map(|s| &s.groups)
        .flat_map(|g| &g.items)
    {
        let Item::Image { kind, data, .. } = item else {
            continue;
        };
        if *kind != ImageKind::Svg {
            raster = true;
            continue;
        }
        // Each SVG file read once.
        if seen.contains(&data) {
            continue;
        }
        seen.push(data);
        match svg_image::of_base64(data.base64()) {
            Ok(svg) => {
                for font in svg.left_out.fonts {
                    if !fonts.contains(&font) {
                        fonts.push(font);
                    }
                }
                characters |= svg.left_out.characters;
                pictures |= svg.left_out.pictures;
            }
            Err(_) => unread = true,
        }
    }
    let name = format.to_uppercase();
    let mut out = Vec::new();
    if raster && format != "pdf" {
        out.push(format!(
            "a {name} file holds no raster images: the title block's PNG and JPEG images are \
             written as their outlines"
        ));
    }
    if unread {
        out.push(format!(
            "an SVG image of the title block cannot be read: the {name} file has its outline"
        ));
    }
    if !fonts.is_empty() {
        let fonts = fonts.join("; ");
        let bundled = svg_image::BUNDLED_FONT;
        out.push(format!(
            "the title block's SVG images name fonts this computer does not have ({fonts}): \
             the {name} file draws their text in {bundled}"
        ));
    }
    if characters {
        out.push(format!(
            "the text of the title block's SVG images has characters no font on this computer \
             has: the {name} file cannot draw them"
        ));
    }
    if pictures {
        out.push(format!(
            "the pictures inside the title block's SVG images are left out of the {name} file"
        ));
    }
    out
}

/// An image file as PDF objects, or why it cannot be one.
pub fn of(bytes: &[u8], kind: ImageKind) -> Result<PdfImage, String> {
    match kind {
        ImageKind::Jpeg => jpeg(bytes),
        ImageKind::Png => png(bytes),
        ImageKind::Svg => Err("an SVG image has no PDF form here".to_owned()),
    }
}

/// A JPEG file as it is: its size and colours from its frame header.
fn jpeg(bytes: &[u8]) -> Result<PdfImage, String> {
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 0xff {
            return Err("the JPEG file's markers are broken".to_owned());
        }
        let marker = bytes[at + 1];
        // Fill bytes and markers without a length.
        if marker == 0xff {
            at += 1;
            continue;
        }
        if marker == 0xd8 || marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            at += 2;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        let frame = (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
        if frame {
            let segment = bytes
                .get(at + 4..at + 2 + length)
                .filter(|s| s.len() >= 6)
                .ok_or("the JPEG file's frame header is cut short")?;
            let height = u16::from_be_bytes([segment[1], segment[2]]);
            let width = u16::from_be_bytes([segment[3], segment[4]]);
            let (space, decode) = match segment[5] {
                1 => ("/DeviceGray", ""),
                3 => ("/DeviceRGB", ""),
                // Adobe's CMYK JPEG files store their samples inverted.
                4 => ("/DeviceCMYK", " /Decode [1 0 1 0 1 0 1 0]"),
                n => return Err(format!("a JPEG file of {n} colour components")),
            };
            if width == 0 || height == 0 {
                return Err("the JPEG file has no pixels".to_owned());
            }
            return Ok(PdfImage {
                entries: format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} \
                     /ColorSpace {space} /BitsPerComponent 8 /Filter /DCTDecode{decode}"
                ),
                stream: bytes.to_vec(),
                mask: None,
            });
        }
        if marker == 0xd9 || marker == 0xda {
            break;
        }
        at += 2 + length;
    }
    Err("the JPEG file has no frame header".to_owned())
}

/// A PNG file's pixels as 8-bit samples.
struct Raster {
    width: u32,
    height: u32,
    /// 1 (grey) or 3 (RGB).
    colours: usize,
    samples: Vec<u8>,
    /// One byte a pixel, when any pixel is not opaque.
    alpha: Option<Vec<u8>>,
}

fn png(bytes: &[u8]) -> Result<PdfImage, String> {
    let raster = decode_png(bytes)?;
    let space = if raster.colours == 1 {
        "/DeviceGray"
    } else {
        "/DeviceRGB"
    };
    let entries = |space: &str| {
        format!(
            "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {space} \
             /BitsPerComponent 8 /Filter /FlateDecode",
            raster.width, raster.height
        )
    };
    Ok(PdfImage {
        entries: entries(space),
        stream: miniz_oxide::deflate::compress_to_vec_zlib(&raster.samples, 6),
        mask: raster.alpha.as_ref().map(|alpha| {
            (
                entries("/DeviceGray"),
                miniz_oxide::deflate::compress_to_vec_zlib(alpha, 6),
            )
        }),
    })
}

/// The largest PNG decoded, in pixels (8192 x 8192).
const MAX_PIXELS: u64 = 1 << 26;

/// A PNG file decoded: its chunks read, its data inflated and unfiltered
/// (Adam7's seven passes when it is interlaced), its samples made 8-bit
/// grey or RGB with its transparency (an alpha channel, a palette's
/// `tRNS`, a colour key) as alpha.
fn decode_png(bytes: &[u8]) -> Result<Raster, String> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) {
        return Err("no PNG file".to_owned());
    }
    let mut at = SIGNATURE.len();
    let mut header: Option<[u32; 2]> = None;
    let (mut depth, mut colour, mut interlace) = (0u8, 0u8, 0u8);
    let mut palette: Vec<u8> = Vec::new();
    let mut transparency: Vec<u8> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    while at + 8 <= bytes.len() {
        let length =
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]) as usize;
        let kind = &bytes[at + 4..at + 8];
        let body = bytes
            .get(at + 8..at + 8 + length)
            .ok_or("the PNG file is cut short")?;
        match kind {
            b"IHDR" if body.len() >= 13 => {
                let width = u32::from_be_bytes([body[0], body[1], body[2], body[3]]);
                let height = u32::from_be_bytes([body[4], body[5], body[6], body[7]]);
                header = Some([width, height]);
                depth = body[8];
                colour = body[9];
                interlace = body[12];
            }
            b"PLTE" => palette = body.to_vec(),
            b"tRNS" => transparency = body.to_vec(),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        // The chunk, its type, its data and its CRC.
        at += 12 + length;
    }
    let [width, height] = header.ok_or("the PNG file has no header")?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("a PNG image of {width} x {height} pixels"));
    }
    let channels: usize = match colour {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        other => return Err(format!("a PNG file of colour type {other}")),
    };
    let valid = match colour {
        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
        3 => matches!(depth, 1 | 2 | 4 | 8),
        _ => matches!(depth, 8 | 16),
    };
    if !valid || interlace > 1 {
        return Err("the PNG file's header is not one of the standard's".to_owned());
    }
    if colour == 3 && palette.is_empty() {
        return Err("the PNG file has no palette".to_owned());
    }
    let bits = channels * usize::from(depth);
    // The bytes of a pixel the filters step back by.
    let step = bits.div_ceil(8);
    let (w, h) = (width as usize, height as usize);
    // Each row's filter byte and its samples, of every pass.
    let expected: usize = passes(w, h, interlace == 1)
        .iter()
        .map(|p| p.rows * (1 + (p.columns * bits).div_ceil(8)))
        .sum();
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, expected)
        .map_err(|_| "the PNG file's image data is broken".to_owned())?;
    if raw.len() < expected {
        return Err("the PNG file's image data is cut short".to_owned());
    }
    let colours = if channels <= 2 && colour != 3 { 1 } else { 3 };
    let mut samples = vec![0u8; w * h * colours];
    let mut alpha = vec![255u8; w * h];
    // The palette's alphas, a grey or RGB colour key (16-bit samples).
    let key: Option<Vec<u16>> = match colour {
        0 if transparency.len() >= 2 => {
            Some(vec![u16::from_be_bytes([transparency[0], transparency[1]])])
        }
        2 if transparency.len() >= 6 => Some(
            transparency[..6]
                .chunks(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect(),
        ),
        _ => None,
    };
    let max = (1u32 << depth) - 1;
    let mut offset = 0;
    for pass in passes(w, h, interlace == 1) {
        let row_bytes = (pass.columns * bits).div_ceil(8);
        let mut previous = vec![0u8; row_bytes];
        for row in 0..pass.rows {
            let filter = raw[offset];
            let mut line = raw[offset + 1..offset + 1 + row_bytes].to_vec();
            offset += 1 + row_bytes;
            unfilter(filter, &mut line, &previous, step)?;
            for column in 0..pass.columns {
                let x = pass.x0 + column * pass.dx;
                let y = pass.y0 + row * pass.dy;
                let pixel = y * w + x;
                // The pixel's raw samples (16-bit ones whole).
                let mut raw_samples = [0u16; 4];
                for (c, sample) in raw_samples.iter_mut().enumerate().take(channels) {
                    *sample = match depth {
                        16 => {
                            let i = (column * channels + c) * 2;
                            u16::from_be_bytes([line[i], line[i + 1]])
                        }
                        8 => u16::from(line[column * channels + c]),
                        _ => {
                            let bit = (column * channels + c) * usize::from(depth);
                            let byte = line[bit / 8];
                            let shift = 8 - usize::from(depth) - bit % 8;
                            u16::from((byte >> shift) & max as u8)
                        }
                    };
                }
                let eight = |v: u16| -> u8 {
                    match depth {
                        16 => (v >> 8) as u8,
                        8 => v as u8,
                        _ => (u32::from(v) * 255 / max) as u8,
                    }
                };
                match colour {
                    3 => {
                        let index = usize::from(raw_samples[0]);
                        let rgb = palette.get(index * 3..index * 3 + 3).unwrap_or(&[0, 0, 0]);
                        samples[pixel * 3..pixel * 3 + 3].copy_from_slice(rgb);
                        alpha[pixel] = transparency.get(index).copied().unwrap_or(255);
                    }
                    0 | 4 => {
                        samples[pixel] = eight(raw_samples[0]);
                        if colour == 4 {
                            alpha[pixel] = eight(raw_samples[1]);
                        } else if key.as_deref() == Some(&raw_samples[..1]) {
                            alpha[pixel] = 0;
                        }
                    }
                    _ => {
                        for (out, v) in samples[pixel * 3..pixel * 3 + 3]
                            .iter_mut()
                            .zip(raw_samples)
                        {
                            *out = eight(v);
                        }
                        if colour == 6 {
                            alpha[pixel] = eight(raw_samples[3]);
                        } else if key.as_deref() == Some(&raw_samples[..3]) {
                            alpha[pixel] = 0;
                        }
                    }
                }
            }
            previous = line;
        }
    }
    let opaque = alpha.iter().all(|a| *a == 255);
    Ok(Raster {
        width,
        height,
        colours,
        samples,
        alpha: (!opaque).then_some(alpha),
    })
}

/// One of the passes of a PNG's image: where its pixels are in the image,
/// and how many columns and rows it has.
struct Pass {
    x0: usize,
    y0: usize,
    dx: usize,
    dy: usize,
    columns: usize,
    rows: usize,
}

/// The passes of an image: the whole image, or Adam7's seven (the empty
/// ones left out).
fn passes(w: usize, h: usize, interlaced: bool) -> Vec<Pass> {
    if !interlaced {
        return vec![Pass {
            x0: 0,
            y0: 0,
            dx: 1,
            dy: 1,
            columns: w,
            rows: h,
        }];
    }
    const ADAM7: [(usize, usize, usize, usize); 7] = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    ADAM7
        .iter()
        .map(|&(x0, y0, dx, dy)| Pass {
            x0,
            y0,
            dx,
            dy,
            columns: if w > x0 { (w - x0).div_ceil(dx) } else { 0 },
            rows: if h > y0 { (h - y0).div_ceil(dy) } else { 0 },
        })
        .filter(|p| p.columns > 0 && p.rows > 0)
        .collect()
}

/// A row unfiltered in place (PNG's filters 0 to 4) against the row above.
fn unfilter(filter: u8, line: &mut [u8], previous: &[u8], step: usize) -> Result<(), String> {
    match filter {
        0 => {}
        1 => {
            for i in step..line.len() {
                line[i] = line[i].wrapping_add(line[i - step]);
            }
        }
        2 => {
            for (byte, up) in line.iter_mut().zip(previous) {
                *byte = byte.wrapping_add(*up);
            }
        }
        3 => {
            for i in 0..line.len() {
                let left = if i >= step {
                    u16::from(line[i - step])
                } else {
                    0
                };
                let average = ((left + u16::from(previous[i])) / 2) as u8;
                line[i] = line[i].wrapping_add(average);
            }
        }
        4 => {
            for i in 0..line.len() {
                let a = if i >= step {
                    i16::from(line[i - step])
                } else {
                    0
                };
                let b = i16::from(previous[i]);
                let c = if i >= step {
                    i16::from(previous[i - step])
                } else {
                    0
                };
                let p = a + b - c;
                let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                let predictor = if pa <= pb && pa <= pc {
                    a
                } else if pb <= pc {
                    b
                } else {
                    c
                };
                line[i] = line[i].wrapping_add(predictor as u8);
            }
        }
        other => return Err(format!("a PNG row of filter {other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG file of these rows of RGBA pixels, unfiltered, with a CRC of
    /// zero (the decoder does not check it).
    fn rgba_png(width: u32, rows: &[Vec<[u8; 4]>]) -> Vec<u8> {
        let mut raw = Vec::new();
        for row in rows {
            raw.push(0);
            for p in row {
                raw.extend_from_slice(p);
            }
        }
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut chunk = |kind: &[u8], body: &[u8]| {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out.extend_from_slice(&[0, 0, 0, 0]);
        };
        let mut header = Vec::new();
        header.extend_from_slice(&width.to_be_bytes());
        header.extend_from_slice(&(rows.len() as u32).to_be_bytes());
        header.extend_from_slice(&[8, 6, 0, 0, 0]);
        chunk(b"IHDR", &header);
        chunk(
            b"IDAT",
            &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6),
        );
        chunk(b"IEND", &[]);
        out
    }

    #[test]
    fn a_png_with_alpha_becomes_rgb_and_a_soft_mask() {
        let red = [255, 0, 0, 255];
        let clear = [0, 0, 255, 0];
        let png = rgba_png(2, &[vec![red, clear], vec![clear, red]]);
        let raster = decode_png(&png).unwrap();
        assert_eq!((raster.width, raster.height, raster.colours), (2, 2, 3));
        assert_eq!(&raster.samples[..6], &[255, 0, 0, 0, 0, 255]);
        assert_eq!(raster.alpha, Some(vec![255, 0, 0, 255]));
        let image = of(&png, ImageKind::Png).unwrap();
        assert!(
            image.entries.contains("/Width 2 /Height 2"),
            "{}",
            image.entries
        );
        assert!(image.entries.contains("/DeviceRGB"));
        assert!(image.mask.is_some());
    }

    #[test]
    fn filters_are_undone() {
        // Sub on a row of grey-like RGB: each byte after the first pixel
        // the difference to the one before.
        let mut line = vec![10, 20, 30, 1, 1, 1];
        unfilter(1, &mut line, &[0; 6], 3).unwrap();
        assert_eq!(line, [10, 20, 30, 11, 21, 31]);
        let mut line = vec![1, 2, 3];
        unfilter(2, &mut line, &[10, 10, 10], 1).unwrap();
        assert_eq!(line, [11, 12, 13]);
        let mut line = vec![0, 0];
        unfilter(4, &mut line, &[5, 7], 1).unwrap();
        assert_eq!(line, [5, 7]);
        assert!(unfilter(9, &mut [0], &[0], 1).is_err());
    }

    #[test]
    fn adam7_passes_cover_the_image_once() {
        let (w, h) = (13, 7);
        let mut seen = vec![0; w * h];
        for p in passes(w, h, true) {
            for r in 0..p.rows {
                for c in 0..p.columns {
                    seen[(p.y0 + r * p.dy) * w + p.x0 + c * p.dx] += 1;
                }
            }
        }
        assert!(seen.iter().all(|n| *n == 1));
    }

    #[test]
    fn a_jpeg_is_kept_as_it_is() {
        // SOI, a frame header (8-bit, 3 x 2 pixels, 3 components), EOI.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 8, 0, 2, 0, 3, 3, 1, 0x11, 0, 2, 0x11, 0, 3, 0x11,
            0, 0xff, 0xd9,
        ];
        let image = of(&jpeg, ImageKind::Jpeg).unwrap();
        assert!(
            image.entries.contains("/Width 3 /Height 2"),
            "{}",
            image.entries
        );
        assert!(image.entries.contains("/DCTDecode"));
        assert_eq!(image.stream, jpeg);
        assert!(of(b"<svg/>", ImageKind::Svg).is_err());
    }
}
