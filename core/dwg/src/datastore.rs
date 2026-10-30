// SPDX-License-Identifier: MIT
//! The data storage section of R2013 and later (the specification's
//! chapter 24): file segments found through the segment index; the data
//! index points at the data records of the data segments, each with the
//! handle of its object. The B-rep record of a solid or region is such a
//! record (its binary form); a large one is a blob over several pages.

use std::collections::BTreeMap;

use crate::DwgError;

/// The size of a file segment's header.
pub const SEGMENT_HEADER: usize = 48;
/// The signature of a file segment.
pub const SEGMENT_SIGNATURE: u16 = 0xd5ac;
/// A data record that refers to a blob of pages.
pub const BLOB_REFERENCE: u32 = 0xbb10_6bb1;

fn corrupt(msg: impl Into<String>) -> DwgError {
    DwgError::Corrupt(format!("data storage: {}", msg.into()))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, DwgError> {
    crate::container::span(b, at, 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| corrupt(format!("ends before byte {}", at.saturating_add(4))))
}

fn u64_at(b: &[u8], at: usize) -> Result<u64, DwgError> {
    Ok(u64::from(u32_at(b, at)?) | u64::from(u32_at(b, at + 4)?) << 32)
}

/// A file segment: where it starts and its name.
#[derive(Clone, Debug)]
struct Segment {
    start: usize,
    name: String,
    size: usize,
    /// Where the object data starts, from the segment's start (the
    /// header's object data alignment offset, in 16 bytes).
    object_data: usize,
}

fn segment(data: &[u8], start: usize) -> Result<Segment, DwgError> {
    let head = crate::container::span(data, start, SEGMENT_HEADER)
        .ok_or_else(|| corrupt(format!("a segment at {start} past the end")))?;
    if u16::from_le_bytes([head[0], head[1]]) != SEGMENT_SIGNATURE {
        return Err(corrupt(format!("no segment signature at {start}")));
    }
    Ok(Segment {
        start,
        name: crate::bits::latin1(&head[2..8]),
        size: u32_at(head, 16)? as usize,
        object_data: (u32_at(head, 36)? as usize) << 4,
    })
}

/// The data records of the section, by object handle: every record whose
/// bytes are the binary B-rep form (the other records hold properties).
pub fn brep_records(data: &[u8]) -> Result<BTreeMap<u64, Vec<u8>>, DwgError> {
    let index_offset = u32_at(data, 24)? as usize;
    let index_count = u32_at(data, 32)? as usize;
    let data_index = u32_at(data, 40)? as usize;
    // The segment index: each segment's offset and size.
    let index = segment(data, index_offset)?;
    if index.name != "segidx" {
        return Err(corrupt(format!(
            "the segment index is named {:?}",
            index.name
        )));
    }
    // Each entry takes 12 bytes of the section: no more than it holds.
    let mut segments = Vec::with_capacity(index_count.min(data.len() / 12));
    for i in 0..index_count {
        let at = index.start + SEGMENT_HEADER + 12 * i;
        let offset = u64_at(data, at)? as usize;
        segments.push(offset);
    }
    let seg = |i: usize| -> Result<Segment, DwgError> {
        let start = *segments
            .get(i)
            .ok_or_else(|| corrupt(format!("segment {i} is not in the index")))?;
        segment(data, start)
    };
    let datidx = seg(data_index)?;
    if datidx.name != "datidx" {
        return Err(corrupt(format!(
            "the data index is named {:?}",
            datidx.name
        )));
    }
    let base = datidx.start + SEGMENT_HEADER;
    let count = u32_at(data, base)? as usize;
    // Each data segment's record headers.
    let mut by_segment: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..count {
        let at = base + 8 + 12 * i;
        let segment_index = u32_at(data, at)? as usize;
        let local = u32_at(data, at + 4)? as usize;
        if segment_index != 0 {
            by_segment.entry(segment_index).or_default().push(local);
        }
    }
    let mut out = BTreeMap::new();
    for (index, mut locals) in by_segment {
        let s = seg(index)?;
        if s.name != "_data_" {
            continue;
        }
        locals.sort_unstable();
        // The record headers, at their local offsets after the segment's
        // header; the records from the object data's start (drawings put
        // it at the header's alignment offset, a multiple of 16 bytes).
        let mut headers = Vec::new();
        for &local in &locals {
            let at = s.start + SEGMENT_HEADER + local;
            headers.push((u64_at(data, at + 8)?, u32_at(data, at + 16)? as usize));
        }
        let data_start = s.start + s.object_data.max(SEGMENT_HEADER + 20 * locals.len());
        let segment_end = s.start + s.size;
        let mut starts: Vec<usize> = headers.iter().map(|h| data_start + h.1).collect();
        starts.sort_unstable();
        for &(handle, local) in &headers {
            let at = data_start + local;
            let next = starts
                .iter()
                .copied()
                .find(|&p| p > at)
                .unwrap_or(segment_end);
            let max = next.saturating_sub(at);
            let size = u32_at(data, at)?;
            let bytes = if size == BLOB_REFERENCE {
                blob(data, at + 4, &seg)?
            } else if (size as usize) + 4 <= max {
                data[at + 4..at + 4 + size as usize].to_vec()
            } else {
                return Err(corrupt(format!(
                    "the record of handle {handle:X} is larger than its space"
                )));
            };
            if crate::solid::is_binary(&bytes) {
                out.insert(handle, bytes);
            }
        }
    }
    Ok(out)
}

/// The bytes of a blob: its pages, each a blob segment.
fn blob(
    data: &[u8],
    at: usize,
    seg: &dyn Fn(usize) -> Result<Segment, DwgError>,
) -> Result<Vec<u8>, DwgError> {
    let total = u64_at(data, at)? as usize;
    let pages = u32_at(data, at + 8)? as usize;
    if total > data.len() {
        return Err(corrupt(format!("a blob of {total} bytes")));
    }
    let mut out = Vec::with_capacity(total);
    for i in 0..pages {
        let index = u32_at(data, at + 32 + 8 * i)? as usize;
        let s = seg(index)?;
        let body = s.start + SEGMENT_HEADER;
        let size = u64_at(data, body + 24)? as usize;
        let bytes = crate::container::span(data, body + 32, size)
            .ok_or_else(|| corrupt("a blob page past the end"))?;
        out.extend_from_slice(bytes);
    }
    out.truncate(total);
    Ok(out)
}

/// Writes a data storage section holding `records` (the test files): the
/// header, the segment index, the data index and one data segment.
pub fn write(records: &[(u64, Vec<u8>)]) -> Vec<u8> {
    fn header(name: &str, index: u32, size: usize) -> Vec<u8> {
        let mut h = SEGMENT_SIGNATURE.to_le_bytes().to_vec();
        let mut n = name.as_bytes().to_vec();
        n.resize(6, 0);
        h.extend_from_slice(&n);
        for v in [index, 0, size as u32, 0, 2, 0, 0, 0] {
            h.extend_from_slice(&v.to_le_bytes());
        }
        h.extend_from_slice(&[0x55; 8]);
        h
    }
    fn pad(mut s: Vec<u8>) -> Vec<u8> {
        let size = s.len().div_ceil(0x80) * 0x80;
        s.resize(size, 0x70);
        let n = s.len() as u32;
        s[16..20].copy_from_slice(&n.to_le_bytes());
        s
    }
    // Segment 1: the data index; 2: the data; 0: the segment index.
    let mut data_segment = header("_data_", 2, 0);
    let mut locals = Vec::new();
    let mut offset = 0u32;
    for (handle, bytes) in records {
        locals.push((data_segment.len() - SEGMENT_HEADER) as u32);
        for v in [bytes.len() as u32 + 4, 1] {
            data_segment.extend_from_slice(&v.to_le_bytes());
        }
        data_segment.extend_from_slice(&handle.to_le_bytes());
        data_segment.extend_from_slice(&offset.to_le_bytes());
        offset += bytes.len() as u32 + 4;
    }
    // The object data from a multiple of 16 bytes, as drawings have it.
    data_segment.resize(data_segment.len().div_ceil(16) * 16, 0x62);
    let object_data = (data_segment.len() >> 4) as u32;
    data_segment[36..40].copy_from_slice(&object_data.to_le_bytes());
    for (_, bytes) in records {
        data_segment.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        data_segment.extend_from_slice(bytes);
    }
    let data_segment = pad(data_segment);
    let mut datidx = header("datidx", 1, 0);
    datidx.extend_from_slice(&(records.len() as u32).to_le_bytes());
    datidx.extend_from_slice(&0u32.to_le_bytes());
    for local in &locals {
        for v in [2u32, *local, 0] {
            datidx.extend_from_slice(&v.to_le_bytes());
        }
    }
    let datidx = pad(datidx);
    let file_header = 0x40usize;
    let segidx_size = pad(header("segidx", 0, 0))
        .len()
        .max(SEGMENT_HEADER + 36)
        .div_ceil(0x80)
        * 0x80;
    let segidx_at = file_header;
    let datidx_at = segidx_at + segidx_size;
    let data_at = datidx_at + datidx.len();
    let mut segidx = header("segidx", 0, 0);
    for (at, size) in [
        (segidx_at, segidx_size),
        (datidx_at, datidx.len()),
        (data_at, data_segment.len()),
    ] {
        segidx.extend_from_slice(&(at as u64).to_le_bytes());
        segidx.extend_from_slice(&(size as u32).to_le_bytes());
    }
    let mut segidx = pad(segidx);
    segidx.resize(segidx_size, 0x70);
    let mut out = Vec::new();
    for v in [
        0x6473_6341u32, // signature
        file_header as u32,
        2,
        2,
        0,
        1,
        segidx_at as u32,
        0,
        3,
        0,
        1,
        0,
        0,
        0,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.resize(file_header, 0);
    out.extend(segidx);
    out.extend(datidx);
    out.extend(data_segment);
    let size = out.len() as u32;
    out[52..56].copy_from_slice(&size.to_le_bytes());
    out
}

// The ASM test body needs the feature `solids`.
#[cfg(all(test, feature = "solids"))]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        let asm = mitcad_asm::testdata::cube_blob();
        let mut older = mitcad_asm::asm::file::OLDER_MAGIC.to_vec();
        older.extend_from_slice(&asm[15..]);
        let records = vec![
            (0x2a, asm.clone()),
            (0x2b, older.clone()),
            (0x2c, b"not a body".to_vec()),
        ];
        let data = write(&records);
        let read = brep_records(&data).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[&0x2a], asm);
        assert_eq!(read[&0x2b], older);
    }
}
