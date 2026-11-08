// SPDX-License-Identifier: MIT
//! A PNG writer (8-bit RGB or RGBA, one IDAT): each row with the filter
//! that leaves the smallest sum of bytes, then deflate. Deterministic: the
//! same pixels give the same bytes.

/// The PNG of `pixels`: `width * height` pixels of `channels` bytes each
/// (3: RGB, 4: RGBA), row by row from the top.
pub fn encode(width: u32, height: u32, channels: usize, pixels: &[u8]) -> Vec<u8> {
    assert!(channels == 3 || channels == 4, "RGB or RGBA");
    let stride = width as usize * channels;
    assert_eq!(pixels.len(), stride * height as usize, "the pixels' size");
    let mut filtered = Vec::with_capacity((stride + 1) * height as usize);
    let zero = vec![0u8; stride];
    let mut candidate = vec![0u8; stride];
    let mut best = vec![0u8; stride];
    for y in 0..height as usize {
        let row = &pixels[y * stride..(y + 1) * stride];
        let above = if y == 0 {
            &zero[..]
        } else {
            &pixels[(y - 1) * stride..y * stride]
        };
        let mut best_type = 0u8;
        let mut best_cost = u64::MAX;
        for kind in 0..5u8 {
            filter(kind, row, above, channels, &mut candidate);
            // The usual heuristic: the bytes as signed, absolute, summed.
            let cost: u64 = candidate
                .iter()
                .map(|&b| u64::from((b as i8).unsigned_abs()))
                .sum();
            if cost < best_cost {
                best_cost = cost;
                best_type = kind;
                best.copy_from_slice(&candidate);
            }
        }
        filtered.push(best_type);
        filtered.extend_from_slice(&best);
    }
    let data = miniz_oxide::deflate::compress_to_vec_zlib(&filtered, 6);
    let mut out = Vec::with_capacity(data.len() + 64);
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    // 8 bits, colour type 2 (RGB) or 6 (RGBA), deflate, adaptive
    // filters, no interlace.
    header.extend_from_slice(&[8, if channels == 4 { 6 } else { 2 }, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &data);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn filter(kind: u8, row: &[u8], above: &[u8], bpp: usize, out: &mut [u8]) {
    for i in 0..row.len() {
        let a = if i >= bpp { row[i - bpp] } else { 0 };
        let b = above[i];
        let c = if i >= bpp { above[i - bpp] } else { 0 };
        let predicted = match kind {
            0 => 0,
            1 => a,
            2 => b,
            3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
            _ => paeth(a, b, c),
        };
        out[i] = row[i].wrapping_sub(predicted);
    }
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

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// The CRC-32 of PNG chunks (ISO 3309, as zip's).
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Width, height and colour type of a PNG this module wrote (tests).
pub fn header(png: &[u8]) -> Option<(u32, u32, u8)> {
    if png.len() < 33 || &png[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(png[20..24].try_into().ok()?);
    Some((width, height, png[25]))
}

/// The pixels of a PNG this module wrote (one IDAT), for tests: RGB or
/// RGBA rows.
pub fn decode(png: &[u8]) -> Option<(u32, u32, usize, Vec<u8>)> {
    let (width, height, color) = header(png)?;
    let channels = if color == 6 { 4 } else { 3 };
    let mut at = 8;
    let mut data = Vec::new();
    while at + 8 <= png.len() {
        let length = u32::from_be_bytes(png[at..at + 4].try_into().ok()?) as usize;
        let kind = &png[at + 4..at + 8];
        let body = png.get(at + 8..at + 8 + length)?;
        if kind == b"IDAT" {
            data.extend_from_slice(body);
        }
        at += 12 + length;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).ok()?;
    let stride = width as usize * channels;
    let mut pixels = vec![0u8; stride * height as usize];
    for y in 0..height as usize {
        let kind = raw[y * (stride + 1)];
        let row = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for i in 0..stride {
            let a = if i >= channels {
                pixels[y * stride + i - channels]
            } else {
                0
            };
            let b = if y > 0 {
                pixels[(y - 1) * stride + i]
            } else {
                0
            };
            let c = if y > 0 && i >= channels {
                pixels[(y - 1) * stride + i - channels]
            } else {
                0
            };
            let predicted = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                _ => paeth(a, b, c),
            };
            pixels[y * stride + i] = row[i].wrapping_add(predicted);
        }
    }
    Some((width, height, channels, pixels))
}
