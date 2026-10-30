// SPDX-License-Identifier: MIT
//! The R2007 container (the specification's chapter 5): the file header at
//! 0x80 and the system pages (the page map and the section map) are
//! Reed-Solomon coded (255, 239) and interleaved, the data pages coded
//! (255, 251), interleaved or not; pages are compressed with the R2007
//! variant of LZ77 ([`decompress`]).
//!
//! The coding is systematic: the data bytes stand as they are with parity
//! bytes after them, so a file read without errors needs no decoding, only
//! the blocks put back in order. The reader does not correct errors.

use std::collections::BTreeMap;

use crate::DwgError;
use crate::classes::Class;
use crate::container::{self, Sections};
use crate::header::HeaderVars;

fn corrupt(msg: impl Into<String>) -> DwgError {
    DwgError::Corrupt(format!("R2007 container: {}", msg.into()))
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, DwgError> {
    container::span(b, at, 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| corrupt(format!("data ends before byte {}", at.saturating_add(8))))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, DwgError> {
    container::span(b, at, 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| {
            corrupt(format!(
                "the file ends before byte {}",
                at.saturating_add(4)
            ))
        })
}

/// The bytes `len` from `at` in the file, after its 0x480 bytes of meta
/// data and file header (`at` and `len` are fields of the file).
fn page_bytes(file: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    container::span(file, at.checked_add(0x480)?, len)
}

/// The data bytes of `blocks` Reed-Solomon code words of (255, `k`):
/// interleaved (byte j of block b at j * blocks + b) or one after another.
fn rs_data(coded: &[u8], blocks: usize, k: usize, interleaved: bool) -> Result<Vec<u8>, DwgError> {
    // `blocks` comes from a size the file gives: the products are checked
    // (wrapped, a huge count could pass the length test).
    let fits = blocks
        .checked_mul(255)
        .is_some_and(|coded_len| coded.len() >= coded_len);
    if !fits || k > 255 {
        return Err(corrupt("a coded page ends early"));
    }
    let mut out = Vec::with_capacity(blocks * k);
    if interleaved {
        for b in 0..blocks {
            for j in 0..k {
                out.push(coded[j * blocks + b]);
            }
        }
    } else {
        out.extend_from_slice(&coded[..blocks * k]);
    }
    Ok(out)
}

/// Codes data for the test files: blocks of `k` bytes (the last padded
/// with zeros), parity bytes left zero (the reader does not check them).
fn rs_code(data: &[u8], k: usize, interleaved: bool) -> Vec<u8> {
    let blocks = data.len().div_ceil(k).max(1);
    let mut padded = data.to_vec();
    padded.resize(blocks * k, 0);
    let mut out = vec![0u8; blocks * 255];
    for b in 0..blocks {
        for j in 0..k {
            let at = if interleaved {
                j * blocks + b
            } else {
                b * k + j
            };
            out[at] = padded[b * k + j];
        }
    }
    out
}

/// The order a literal run's bytes are written in: for a run of 1 to 32
/// bytes, (count, source index) pieces copied one after another. Within a
/// run, pieces of 2 and 3 bytes are reversed and the halves of a 16-byte
/// piece swapped (mitcad#563: they were copied as stored, which garbled
/// runs of 7, 11, 15 and 18 to 32 bytes).
fn literal_pieces(n: usize) -> &'static [(usize, usize)] {
    match n {
        1 => &[(1, 0)],
        2 => &[(1, 1), (1, 0)],
        3 => &[(1, 2), (1, 1), (1, 0)],
        4 => &[(4, 0)],
        5 => &[(1, 4), (4, 0)],
        6 => &[(1, 5), (4, 1), (1, 0)],
        7 => &[(1, 6), (1, 5), (4, 1), (1, 0)],
        8 => &[(8, 0)],
        9 => &[(1, 8), (8, 0)],
        10 => &[(1, 9), (8, 1), (1, 0)],
        11 => &[(1, 10), (1, 9), (8, 1), (1, 0)],
        12 => &[(4, 8), (8, 0)],
        13 => &[(1, 12), (4, 8), (8, 0)],
        14 => &[(1, 13), (4, 9), (8, 1), (1, 0)],
        15 => &[(1, 14), (1, 13), (4, 9), (8, 1), (1, 0)],
        16 => &[(8, 8), (8, 0)],
        17 => &[(8, 9), (1, 8), (8, 0)],
        18 => &[(1, 17), (8, 9), (8, 1), (1, 0)],
        19 => &[(1, 18), (1, 17), (1, 16), (8, 8), (8, 0)],
        20 => &[(4, 16), (8, 8), (8, 0)],
        21 => &[(1, 20), (4, 16), (8, 8), (8, 0)],
        22 => &[(1, 21), (1, 20), (4, 16), (8, 8), (8, 0)],
        23 => &[(1, 22), (1, 21), (1, 20), (4, 16), (8, 8), (8, 0)],
        24 => &[(8, 16), (8, 8), (8, 0)],
        25 => &[(8, 17), (1, 16), (8, 8), (8, 0)],
        26 => &[(1, 25), (8, 17), (1, 16), (8, 8), (8, 0)],
        27 => &[(1, 26), (1, 25), (8, 17), (1, 16), (8, 8), (8, 0)],
        28 => &[(4, 24), (8, 16), (8, 8), (8, 0)],
        29 => &[(1, 28), (4, 24), (8, 16), (8, 8), (8, 0)],
        30 => &[(1, 29), (1, 28), (4, 24), (8, 16), (8, 8), (8, 0)],
        31 => &[(1, 30), (4, 26), (8, 18), (8, 10), (8, 2), (1, 1), (1, 0)],
        _ => &[(8, 24), (8, 16), (8, 8), (8, 0)],
    }
}

/// Copies a literal run of `n` bytes from `src` in the run's byte order.
fn copy_literals(src: &[u8], out: &mut Vec<u8>, n: usize) {
    let mut at = 0;
    while at < n {
        let chunk = (n - at).min(32);
        for &(count, index) in literal_pieces(chunk) {
            out.extend_from_slice(&src[at + index..at + index + count]);
        }
        at += chunk;
    }
}

/// The run's bytes as stored, from the bytes as they read.
fn store_literals(data: &[u8], out: &mut Vec<u8>) {
    let mut at = 0;
    while at < data.len() {
        let chunk = (data.len() - at).min(32);
        let mut stored = vec![0u8; chunk];
        let mut k = 0;
        for &(count, index) in literal_pieces(chunk) {
            stored[index..index + count].copy_from_slice(&data[at + k..at + k + count]);
            k += count;
        }
        out.extend(stored);
        at += chunk;
    }
}

struct Input<'a> {
    src: &'a [u8],
    i: usize,
}

impl Input<'_> {
    fn byte(&mut self) -> Result<u8, DwgError> {
        let b = *self
            .src
            .get(self.i)
            .ok_or_else(|| corrupt("compressed data ends early"))?;
        self.i += 1;
        Ok(b)
    }

    /// A literal run's length from its opcode (0 to 15).
    fn literal_length(&mut self, op: u8) -> Result<usize, DwgError> {
        let mut length = usize::from(op) + 8;
        if length == 0x17 {
            let n = usize::from(self.byte()?);
            length += n;
            if n == 0xff {
                loop {
                    let n = usize::from(self.byte()?) | usize::from(self.byte()?) << 8;
                    length += n;
                    if n != 0xffff {
                        break;
                    }
                }
            }
        }
        Ok(length)
    }

    /// A copy's source offset and length, and the opcode whose low three
    /// bits count the literals after it.
    fn instruction(&mut self, mut op: u8) -> Result<(usize, usize, u8), DwgError> {
        let (offset, length);
        match op >> 4 {
            0 => {
                let l = usize::from(op & 0xf) + 0x13;
                let o = usize::from(self.byte()?);
                op = self.byte()?;
                length = (usize::from(op >> 3) & 0x10) + l;
                offset = (usize::from(op & 0x78) << 5) + 1 + o;
            }
            1 => {
                length = usize::from(op & 0xf) + 3;
                let o = usize::from(self.byte()?);
                op = self.byte()?;
                offset = (usize::from(op & 0xf8) << 5) + 1 + o;
            }
            2 => {
                let mut o = usize::from(self.byte()?);
                o |= usize::from(self.byte()?) << 8;
                let mut l = usize::from(op & 7);
                if op & 8 == 0 {
                    op = self.byte()?;
                    l += usize::from(op & 0xf8);
                } else {
                    o += 1;
                    l += usize::from(self.byte()?) << 3;
                    op = self.byte()?;
                    l += (usize::from(op & 0xf8) << 8) + 0x100;
                }
                offset = o;
                length = l;
            }
            _ => {
                length = usize::from(op >> 4);
                let o = usize::from(op & 15);
                op = self.byte()?;
                offset = (usize::from(op & 0xf8) << 1) + o + 1;
            }
        }
        Ok((offset, length, op))
    }
}

/// Decompresses R2007 data into `size` bytes (zeros after the data).
pub fn decompress(src: &[u8], size: usize) -> Result<Vec<u8>, DwgError> {
    let mut out = decompress_bounded(src, size)?;
    out.resize(size, 0);
    Ok(out)
}

/// Decompresses R2007 data into at most `size` bytes, without zeros after
/// the data: memory as the data needs, whatever size the file claims.
fn decompress_bounded(src: &[u8], size: usize) -> Result<Vec<u8>, DwgError> {
    // Sized from the data, not from `size` (a field of the file).
    let mut out: Vec<u8> = Vec::with_capacity(size.min(src.len().saturating_mul(4)));
    let mut input = Input { src, i: 0 };
    let mut length = 0;
    // An opcode 0x2X first: two bytes after it, then the byte whose low
    // three bits count the first literals (mitcad#563: read from the byte
    // before it, a stream that starts so went wrong at its first copy).
    if src.first().is_some_and(|op| op & 0xf0 == 0x20) {
        input.i = 4;
        length = usize::from(
            *src.get(3)
                .ok_or_else(|| corrupt("compressed data ends early"))?
                & 7,
        );
    }
    let mut pending: Option<u8> = None;
    while input.i < src.len() || pending.is_some() {
        if length == 0 {
            let op = match pending.take() {
                Some(op) => op,
                None => input.byte()?,
            };
            length = input.literal_length(op)?;
        }
        let end = input.i + length;
        if end > src.len() || out.len() + length > size {
            return Err(corrupt("a literal run past the end"));
        }
        copy_literals(&src[input.i..end], &mut out, length);
        input.i = end;
        if input.i >= src.len() {
            break;
        }
        // Copies from the output, until literals follow.
        let mut op = input.byte()?;
        loop {
            let (offset, count, next) = input.instruction(op)?;
            if offset > out.len() || offset == 0 || out.len() + count > size {
                return Err(corrupt("a copy out of range"));
            }
            let start = out.len() - offset;
            for k in 0..count {
                let b = out[start + k];
                out.push(b);
            }
            length = usize::from(next & 7);
            if length != 0 || input.i >= src.len() {
                break;
            }
            op = input.byte()?;
            if op >> 4 == 0 {
                pending = Some(op);
                break;
            }
            if op >> 4 == 15 {
                op &= 15;
            }
        }
    }
    Ok(out)
}

/// Compresses data for the test files: literal runs and short copies from
/// up to 496 bytes back (the instructions of opcodes 0x30 to 0xEF).
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut literal_start = 0;
    // The pending copy's second byte, whose low bits count the literals
    // after it (up to 7).
    let mut last_copy_byte: Option<usize> = None;
    let flush = |out: &mut Vec<u8>, last: Option<usize>, lits: &[u8]| {
        if lits.is_empty() {
            return;
        }
        match last {
            Some(at) if lits.len() <= 7 => out[at] |= lits.len() as u8,
            _ => {
                let n = lits.len().max(8);
                if n - 8 < 15 {
                    out.push((n - 8) as u8);
                } else {
                    out.push(15);
                    let mut rest = n - 0x17;
                    if rest < 0xff {
                        out.push(rest as u8);
                    } else {
                        out.push(0xff);
                        rest -= 0xff;
                        while rest >= 0xffff {
                            out.extend_from_slice(&0xffffu16.to_le_bytes());
                            rest -= 0xffff;
                        }
                        out.extend_from_slice(&(rest as u16).to_le_bytes());
                    }
                }
            }
        }
        store_literals(lits, out);
    };
    while i < data.len() {
        // The stream starts with a run of at least 8 literals.
        if i < 8 {
            i += 1;
            continue;
        }
        // The longest match of 3 to 14 bytes (an opcode of 0xF0 and up
        // means another instruction after a copy), 1 to 496 back.
        let mut best = (0, 0);
        for offset in 1..=i.min(496) {
            let mut n = 0;
            while n < 14 && i + n < data.len() && data[i + n - offset] == data[i + n] {
                n += 1;
            }
            if n > best.0 {
                best = (n, offset);
            }
        }
        if best.0 >= 3 {
            let lits = &data[literal_start..i];
            flush(&mut out, last_copy_byte, lits);
            let (n, offset) = best;
            let o = offset - 1;
            out.push((n as u8) << 4 | (o & 15) as u8);
            out.push((((o >> 4) & 0x1f) << 3) as u8);
            last_copy_byte = Some(out.len() - 1);
            i += n;
            literal_start = i;
        } else {
            i += 1;
        }
    }
    let lits = &data[literal_start..];
    if last_copy_byte.is_none() {
        // Only literals: one run of at least 8 bytes.
        let mut padded = lits.to_vec();
        padded.resize(lits.len().max(8), 0);
        flush(&mut out, None, &padded);
    } else {
        flush(&mut out, last_copy_byte, lits);
    }
    out
}

/// A system page at `at` from the pages' start (0x480): its data repeated
/// `factor` times, coded (255, 239) and interleaved, compressed when
/// `compressed < uncompressed`.
fn system_page(
    file: &[u8],
    at: usize,
    compressed: usize,
    uncompressed: usize,
    factor: usize,
) -> Result<Vec<u8>, DwgError> {
    if uncompressed > container::size_limit(file) {
        return Err(corrupt(format!(
            "a system page claims {uncompressed} bytes from a file of {}",
            file.len()
        )));
    }
    let blocks = compressed
        .div_ceil(8)
        .checked_mul(8)
        .and_then(|aligned| aligned.checked_mul(factor.max(1)))
        .map(|n| n.div_ceil(239));
    let coded = blocks
        .and_then(|blocks| page_bytes(file, at, blocks.checked_mul(255)?))
        .ok_or_else(|| corrupt("a system page past the end"))?;
    let blocks = coded.len() / 255;
    let data = rs_data(coded, blocks, 239, true)?;
    let data = &data[..compressed.min(data.len())];
    if compressed < uncompressed {
        decompress(data, uncompressed)
    } else {
        Ok(data[..uncompressed.min(data.len())].to_vec())
    }
}

/// Reads the sections of an R2007 file.
pub fn read(file: &[u8]) -> Result<Sections, DwgError> {
    let security = u32_at(file, 0x18)?;
    if security & 1 != 0 {
        return Err(DwgError::Unsupported(
            "a drawing saved with a password (its data is encrypted)".into(),
        ));
    }
    // The file header.
    let coded = file
        .get(0x80..0x80 + 3 * 255)
        .ok_or_else(|| corrupt("no file header"))?;
    let h = rs_data(coded, 3, 239, true)?;
    let compressed = i32::from_le_bytes([h[0x18], h[0x19], h[0x1a], h[0x1b]]);
    let header = if compressed < 0 {
        h.get(0x20..0x20 + compressed.unsigned_abs() as usize)
            .ok_or_else(|| corrupt("the file header data runs past its page"))?
            .to_vec()
    } else {
        let end = (0x20 + compressed as usize).min(h.len());
        decompress(&h[0x20..end], 0x110)?
    };
    let g = |at: usize| u64_at(&header, at).map(|v| v as usize);
    let page_map = system_page(file, g(0x38)?, g(0x50)?, g(0x58)?, g(0x18)?)?;
    // The page map: sizes and ids; pages follow each other from 0x480.
    let mut pages: BTreeMap<i64, (usize, usize)> = BTreeMap::new();
    let mut offset = 0usize;
    for pair in page_map.as_chunks::<16>().0 {
        let size = u64_at(pair, 0)? as usize;
        let id = u64_at(pair, 8)? as i64;
        pages.insert(id.abs(), (offset, size));
        offset = offset.saturating_add(size);
    }
    let section_map_id = g(0xc0)? as i64;
    let (map_at, _) = *pages
        .get(&section_map_id)
        .ok_or_else(|| corrupt("the page map has no section map page"))?;
    let map = system_page(file, map_at, g(0xb0)?, g(0xc8)?, g(0xd8)?)?;

    let mut s = Sections {
        maintenance: *file.get(11).unwrap_or(&0),
        codepage: u16::from_le_bytes([
            *file.get(0x13).unwrap_or(&0),
            *file.get(0x14).unwrap_or(&0),
        ]),
        ..Default::default()
    };
    // No section or page holds more than this (a size claimed beyond it
    // would only allocate).
    let limit = container::size_limit(file);
    let mut at = 0;
    while at + 0x40 <= map.len() {
        let size = u64_at(&map, at)? as usize;
        let max = u64_at(&map, at + 8)? as usize;
        let encrypted = u64_at(&map, at + 0x10)?;
        let name_length = u64_at(&map, at + 0x20)? as usize;
        let encoding = u64_at(&map, at + 0x30)?;
        let count = u64_at(&map, at + 0x38)? as usize;
        if name_length > 512 || count > 1 << 20 {
            return Err(corrupt("a section map entry out of range"));
        }
        // The name in UTF-16, its length in bytes with the terminating zero
        // (mitcad#563: it was read as a count of characters); the pages
        // follow it.
        let name_at = at + 0x40;
        let units: Vec<u16> = map
            .get(name_at..name_at + name_length)
            .ok_or_else(|| corrupt("a section name past the map's end"))?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .take_while(|&u| u != 0)
            .collect();
        let name = String::from_utf16_lossy(&units);
        let entries = name_at + name_length;
        if entries + 56 * count > map.len() {
            return Err(corrupt(format!("{name}: the page list past the map's end")));
        }
        let mut list = Vec::with_capacity(count);
        for k in 0..count {
            let e = entries + 56 * k;
            list.push((
                u64_at(&map, e)? as usize,
                u64_at(&map, e + 16)? as i64,
                u64_at(&map, e + 24)? as usize,
                u64_at(&map, e + 32)? as usize,
            ));
        }
        at = entries + 56 * count;
        s.listed.insert(name.clone(), size as u64);
        let wanted = [
            container::HEADER,
            container::CLASSES,
            container::HANDLES,
            container::OBJECTS,
        ];
        if !wanted.contains(&name.as_str()) {
            continue;
        }
        if encrypted == 1 {
            return Err(DwgError::Unsupported(format!(
                "the encrypted section {name}"
            )));
        }
        if size > limit {
            return Err(corrupt(format!("the section {name} claims {size} bytes")));
        }
        let mut data = vec![0u8; size];
        for (offset, id, uncompressed, compressed) in list {
            if uncompressed > limit {
                return Err(corrupt(format!(
                    "{name}: page {id} claims {uncompressed} bytes from a file of {}",
                    file.len()
                )));
            }
            let (page_at, _) = *pages
                .get(&id)
                .ok_or_else(|| corrupt(format!("{name}: page {id} is not in the page map")))?;
            let blocks = compressed
                .div_ceil(8)
                .checked_mul(8)
                .map(|aligned| aligned.div_ceil(251));
            let coded = blocks
                .and_then(|blocks| page_bytes(file, page_at, blocks.checked_mul(255)?))
                .ok_or_else(|| corrupt(format!("{name}: page {id} past the end")))?;
            let raw = rs_data(coded, coded.len() / 255, 251, encoding == 4)?;
            let raw = &raw[..compressed.min(raw.len())];
            // The page's data only: the section's bytes are zeros where
            // the data ends.
            let page = if compressed < uncompressed {
                decompress_bounded(raw, uncompressed)?
            } else {
                raw.to_vec()
            };
            if offset < size {
                let n = page.len().min(size - offset).min(max.max(uncompressed));
                data[offset..offset + n].copy_from_slice(&page[..n]);
            }
        }
        match name.as_str() {
            container::HEADER => s.header = data,
            container::CLASSES => s.classes = data,
            container::HANDLES => s.handles = data,
            _ => s.objects = data,
        }
    }
    if s.header.is_empty() || s.handles.is_empty() || s.objects.is_empty() {
        return Err(corrupt("no header, object map or objects section"));
    }
    Ok(s)
}

/// Writes an R2007 file (the test files): the meta data, the file header
/// (uncompressed), the page map, the data pages (compressed, coded and
/// interleaved) and the section map.
pub fn write(header: &HeaderVars, classes: &[Class], objects: &BTreeMap<u64, Vec<u8>>) -> Vec<u8> {
    write_edited(header, classes, objects, &|_, _| {})
}

/// [`write`], each page entry of the section map (offset, page size, id,
/// uncompressed and compressed size, two zeros) passed to `edit` with its
/// section's name before it is written: damaged files for the tests.
fn write_edited(
    header: &HeaderVars,
    classes: &[Class],
    objects: &BTreeMap<u64, Vec<u8>>,
    edit: &dyn Fn(&str, &mut [u64; 7]),
) -> Vec<u8> {
    use crate::objmap::object_stream;
    let v = crate::Version::R2007;
    let (object_data, map) = object_stream(objects, Vec::new(), 0);
    let sections: Vec<(&str, Vec<u8>, usize)> = vec![
        (container::HEADER, crate::header::write(v, header), 0x800),
        (
            container::CLASSES,
            crate::classes::write(classes, v),
            0xf800,
        ),
        (container::HANDLES, crate::objmap::write(&map), 0xf800),
        // Small pages, so that the objects take several.
        (container::OBJECTS, object_data, 0x400),
    ];
    // Data pages, ids from 2 (1 is the page map).
    let mut pages: Vec<(i64, Vec<u8>)> = Vec::new();
    let mut entries = Vec::new();
    for (name, data, page_size) in &sections {
        let mut list = Vec::new();
        for (k, chunk) in data.chunks(*page_size).enumerate() {
            let packed = compress(chunk);
            let (stored, compressed) = if packed.len() < chunk.len() {
                (packed, true)
            } else {
                (chunk.to_vec(), false)
            };
            let mut padded = stored.clone();
            padded.resize(stored.len().div_ceil(8) * 8, 0);
            let mut coded = rs_code(&padded, 251, true);
            coded.resize(coded.len().div_ceil(0x20) * 0x20, 0);
            let id = pages.len() as i64 + 2;
            list.push((
                k * page_size,
                coded.len(),
                id,
                chunk.len(),
                if compressed {
                    stored.len()
                } else {
                    chunk.len()
                },
            ));
            pages.push((id, coded));
        }
        entries.push((name.to_string(), data.len(), *page_size, list));
    }
    // The section map.
    let mut smap = Vec::new();
    for (name, size, max, list) in &entries {
        let units: Vec<u16> = name.encode_utf16().collect();
        for v in [
            *size as u64,
            *max as u64,
            0,
            0,
            // In bytes, with the terminating zero.
            2 * (units.len() as u64 + 1),
            0,
            4,
            list.len() as u64,
        ] {
            smap.extend_from_slice(&v.to_le_bytes());
        }
        for u in &units {
            smap.extend_from_slice(&u.to_le_bytes());
        }
        smap.extend_from_slice(&[0, 0]);
        for (offset, page_size, id, uncompressed, compressed) in list {
            let mut entry = [
                *offset as u64,
                *page_size as u64,
                *id as u64,
                *uncompressed as u64,
                *compressed as u64,
                0,
                0,
            ];
            edit(name.as_str(), &mut entry);
            for v in entry {
                smap.extend_from_slice(&v.to_le_bytes());
            }
        }
    }
    let system = |data: &[u8]| {
        let mut padded = data.to_vec();
        padded.resize(data.len().div_ceil(8) * 8, 0);
        let mut coded = rs_code(&padded, 239, true);
        coded.resize(coded.len().div_ceil(0x20) * 0x20, 0);
        coded
    };
    let section_map = system(&smap);
    let section_map_id = pages.len() as i64 + 2;
    // The page map: itself first, the data pages, the section map; its
    // own size depends on its length, which does not change with it.
    let page_map_data = |own: usize| {
        let mut m = Vec::new();
        m.extend_from_slice(&(own as u64).to_le_bytes());
        m.extend_from_slice(&1u64.to_le_bytes());
        for (id, coded) in &pages {
            m.extend_from_slice(&(coded.len() as u64).to_le_bytes());
            m.extend_from_slice(&(*id as u64).to_le_bytes());
        }
        m.extend_from_slice(&(section_map.len() as u64).to_le_bytes());
        m.extend_from_slice(&(section_map_id as u64).to_le_bytes());
        m
    };
    let first = system(&page_map_data(0));
    let page_map_raw = page_map_data(first.len());
    let page_map = system(&page_map_raw);
    // The file header data.
    let mut fh = vec![0u8; 0x110];
    let mut put = |at: usize, v: u64| fh[at..at + 8].copy_from_slice(&v.to_le_bytes());
    put(0, 0x70);
    put(0x18, 1);
    put(0x38, 0);
    put(0x40, 1);
    put(0x50, page_map_raw.len() as u64);
    put(0x58, page_map_raw.len() as u64);
    put(0xa0, entries.len() as u64 + 1);
    put(0xb0, smap.len() as u64);
    put(0xc0, section_map_id as u64);
    put(0xc8, smap.len() as u64);
    put(0xd8, 1);
    let mut header_page = vec![0u8; 0x20];
    header_page[0x18..0x1c].copy_from_slice(&(-(0x110i32)).to_le_bytes());
    header_page.extend(fh);
    let mut coded_header = rs_code(&header_page, 239, true);
    // Three code words of the 0x3d8 bytes, the rest zero (no check data).
    let blocks = header_page.len().div_ceil(239);
    if blocks < 3 {
        let mut padded = header_page.clone();
        padded.resize(3 * 239, 0);
        coded_header = rs_code(&padded, 239, true);
    }
    coded_header.resize(0x400, 0);

    let mut out = Vec::new();
    out.extend_from_slice(b"AC1021");
    out.extend_from_slice(&[0, 0, 0, 0, 0, crate::header::MAINTENANCE, 0]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&[0x1f, 0x00]);
    out.extend_from_slice(&30u16.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0]);
    out.resize(0x80, 0);
    out.extend(coded_header);
    out.extend(page_map);
    for (_, coded) in pages {
        out.extend(coded);
    }
    out.extend(section_map);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_order_round_trips() {
        for n in 1..=70 {
            let data: Vec<u8> = (0..n as u8).collect();
            let mut stored = Vec::new();
            store_literals(&data, &mut stored);
            let mut back = Vec::new();
            copy_literals(&stored, &mut back, n);
            assert_eq!(back, data, "{n}");
        }
    }

    #[test]
    fn compression_round_trips() {
        let mut text = Vec::new();
        for i in 0..3000u32 {
            text.extend_from_slice(format!("line {} of {}\n", i % 37, i % 11).as_bytes());
        }
        for data in [
            b"abc".to_vec(),
            b"abcdefghij".to_vec(),
            vec![7u8; 600],
            text,
        ] {
            let packed = compress(&data);
            let size = data.len().max(8);
            let mut back = decompress(&packed, size).unwrap();
            back.truncate(data.len());
            assert_eq!(back, data);
        }
    }

    #[test]
    fn instructions_of_the_specification() {
        // Opcode 0x31, then 0x10: 3 bytes from (0x10 & 0xf8) << 1 + 1 + 1
        // = 34 back, no literals after.
        let mut input = Input { src: &[0x10], i: 0 };
        assert_eq!(input.instruction(0x31).unwrap(), (34, 3, 0x10));
    }

    #[test]
    fn literal_runs_in_the_order_of_the_specification() {
        // Pieces of 2 and 3 bytes reversed, a 16-byte piece's halves
        // swapped (mitcad#563).
        let run = |n: usize| {
            let src: Vec<u8> = (0..n as u8).collect();
            let mut out = Vec::new();
            copy_literals(&src, &mut out, n);
            out
        };
        assert_eq!(run(7), [6, 5, 1, 2, 3, 4, 0]);
        assert_eq!(run(19)[..3], [18, 17, 16]);
        assert_eq!(run(19)[3..], (8..16).chain(0..8).collect::<Vec<u8>>()[..]);
        let mut whole: Vec<u8> = (24..32).collect();
        whole.extend(16..24);
        whole.extend(8..16);
        whole.extend(0..8);
        assert_eq!(run(32), whole);
        // Every run takes each of its bytes once.
        for n in 1..=32 {
            let mut sorted = run(n);
            sorted.sort_unstable();
            assert_eq!(sorted, (0..n as u8).collect::<Vec<u8>>(), "{n}");
        }
    }

    #[test]
    fn a_file_of_its_version_id_only_is_corrupt() {
        // The security flags were indexed directly (mitcad#931): a panic.
        assert!(matches!(
            crate::read(b"AC1021\0"),
            Err(DwgError::Corrupt(_))
        ));
        for v in crate::Version::ALL {
            let mut file = v.id().as_bytes().to_vec();
            file.push(0);
            assert!(crate::read(&file).is_err(), "{v}");
        }
    }

    /// A small drawing's sections: the header variables, no classes, one
    /// object.
    fn small_file(edit: &dyn Fn(&str, &mut [u64; 7])) -> Vec<u8> {
        let mut objects = BTreeMap::new();
        objects.insert(1u64, vec![0u8; 40]);
        write_edited(&HeaderVars::default(), &[], &objects, edit)
    }

    #[test]
    fn a_page_claiming_2_pow_63_bytes_is_corrupt() {
        assert!(read(&small_file(&|_: &str, _: &mut [u64; 7]| {})).is_ok());
        // The header's page decompressed into 2^63 bytes: the capacity
        // overflowed (mitcad#931).
        let file = small_file(&|name: &str, entry: &mut [u64; 7]| {
            if name == container::HEADER {
                entry[3] = 1 << 63;
            }
        });
        match read(&file) {
            Err(DwgError::Corrupt(e)) => assert!(e.contains("claims"), "{e}"),
            other => panic!("{other:?}"),
        }
        // Terabytes, below the overflow: refused as well.
        let file = small_file(&|name: &str, entry: &mut [u64; 7]| {
            if name == container::HEADER {
                entry[3] = 1 << 40;
            }
        });
        assert!(matches!(read(&file), Err(DwgError::Corrupt(_))));
    }

    #[test]
    fn sizes_the_file_gives_allocate_nothing_up_front() {
        // A page that claims more than it holds: the output as the data
        // gives it.
        let packed = compress(b"abcdefghij");
        assert_eq!(
            decompress_bounded(&packed, usize::MAX).unwrap(),
            b"abcdefghij"
        );
        // A block count whose coded size overflows.
        assert!(rs_data(&[0; 255], usize::MAX / 100, 251, true).is_err());
        assert!(rs_data(&[0; 255], usize::MAX / 100, 251, false).is_err());
    }

    #[test]
    fn a_stream_that_starts_with_an_opcode_2x() {
        // The start of a file header as saved (mitcad#563): 0x20, two
        // bytes, 0x02 (two literals), 00 70 (stored in reverse), then 0x70
        // 0x00: 7 bytes from 1 back. Its first field, 0x70, as 8 bytes.
        let src = [0x20, 0x00, 0x00, 0x02, 0x00, 0x70, 0x70, 0x00];
        let out = decompress(&src, 9).unwrap();
        assert_eq!(out, [0x70, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
}
