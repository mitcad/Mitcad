// SPDX-License-Identifier: MIT
//! A PNG reader for the previews (mitcad#1539): 8 bits a channel, not
//! interlaced, every colour type (grey, RGB, palette, grey with alpha,
//! RGBA), into RGBA pixels. What the application's Save writes (Qt's PNG
//! writer, RGB or RGBA) is always among them; other PNGs are
//! [`ReadError::Unsupported`], damaged ones [`ReadError::Broken`].

use mitcad_model_base::design_preview::MAX_SIDE;

/// Why a PNG was not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadError {
    /// A valid PNG this reader does not decode: larger than the previews
    /// may be, other than 8 bits a sample, or interlaced.
    Unsupported,
    /// No PNG, or a damaged one.
    Broken(String),
}

fn broken(why: &str) -> ReadError {
    ReadError::Broken(why.to_owned())
}

/// RGBA pixels, row by row from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rgba {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The pixels of the PNG file `png` as RGBA.
pub(crate) fn read(png: &[u8]) -> Result<Rgba, ReadError> {
    if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(broken("no PNG signature"));
    }
    let mut header: Option<&[u8]> = None;
    let mut palette: &[u8] = &[];
    let mut transparency: &[u8] = &[];
    let mut data = Vec::new();
    let mut at = 8usize;
    while at < png.len() {
        let length = u32_at(png, at).ok_or_else(|| broken("a chunk is cut short"))? as usize;
        let kind = png
            .get(at + 4..at + 8)
            .ok_or_else(|| broken("a chunk is cut short"))?;
        let end = (at + 8)
            .checked_add(length)
            .ok_or_else(|| broken("a chunk is too long"))?;
        let body = png
            .get(at + 8..end)
            .ok_or_else(|| broken("a chunk is cut short"))?;
        match kind {
            b"IHDR" => header = Some(body),
            b"PLTE" => palette = body,
            b"tRNS" => transparency = body,
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        // The chunk's CRC follows; it is not checked.
        at = end + 4;
    }
    let header = header.ok_or_else(|| broken("no IHDR chunk"))?;
    if header.len() < 13 {
        return Err(broken("the IHDR chunk is cut short"));
    }
    let width = u32_at(header, 0).unwrap_or(0);
    let height = u32_at(header, 4).unwrap_or(0);
    let (depth, color, interlace) = (header[8], header[9], header[12]);
    if width == 0 || height == 0 {
        return Err(broken("the image has no pixels"));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(ReadError::Unsupported);
    }
    let channels = match color {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(broken("an unknown colour type")),
    };
    if depth != 8 || interlace != 0 {
        return Err(ReadError::Unsupported);
    }
    let (width, height) = (width as usize, height as usize);
    let stride = width * channels;
    let expected = (stride + 1) * height;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, expected)
        .map_err(|_| broken("the pixel data does not inflate"))?;
    if raw.len() != expected {
        return Err(broken("the pixel data has the wrong size"));
    }
    let samples = unfilter(&raw, stride, height, channels)?;
    let mut pixels = Vec::with_capacity(width * height * 4);
    for sample in samples.chunks_exact(channels) {
        let rgba = match color {
            0 => [sample[0], sample[0], sample[0], 255],
            2 => [sample[0], sample[1], sample[2], 255],
            3 => {
                let index = usize::from(sample[0]);
                let rgb = palette
                    .get(index * 3..index * 3 + 3)
                    .ok_or_else(|| broken("a colour is not in the palette"))?;
                let alpha = transparency.get(index).copied().unwrap_or(255);
                [rgb[0], rgb[1], rgb[2], alpha]
            }
            4 => [sample[0], sample[0], sample[0], sample[1]],
            _ => [sample[0], sample[1], sample[2], sample[3]],
        };
        pixels.extend_from_slice(&rgba);
    }
    Ok(Rgba {
        width: width as u32,
        height: height as u32,
        pixels,
    })
}

/// The samples of `raw`'s rows (each a filter type and `stride` bytes)
/// with their filters undone.
fn unfilter(
    raw: &[u8],
    stride: usize,
    height: usize,
    channels: usize,
) -> Result<Vec<u8>, ReadError> {
    let mut out = vec![0u8; stride * height];
    let zero = vec![0u8; stride];
    for (y, line) in raw.chunks_exact(stride + 1).enumerate() {
        let (kind, row) = (line[0], &line[1..]);
        let (done, rest) = out.split_at_mut(y * stride);
        let above = if y == 0 {
            &zero[..]
        } else {
            &done[(y - 1) * stride..]
        };
        let current = &mut rest[..stride];
        for (i, (&byte, &b)) in row.iter().zip(above).enumerate() {
            let a = if i >= channels {
                current[i - channels]
            } else {
                0
            };
            let c = if i >= channels {
                above[i - channels]
            } else {
                0
            };
            let predicted = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return Err(broken("an unknown row filter")),
            };
            current[i] = byte.wrapping_add(predicted);
        }
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}
