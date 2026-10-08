// SPDX-License-Identifier: MIT
//! The segment database of an `.ipt` file (storage `RSeStorage`): a part
//! is kept in segments (the B-rep, the definitions, the graphics, the
//! browser tree, ...), each a pair of streams named after the segment's
//! id: `M<id>`, the segment's meta stream, and `B<id>`, its data stream.
//! See `README.md` for the layouts as far as they are known.
//!
//! - The meta stream starts with a header (a tag `RSe Meta Stream
//!   Version <n>`, the segment's name in UTF-16 and its 16-byte id, two
//!   dates), then compressed tables: the sizes of the segment's records
//!   ("blocks") and the record types (16-byte ids).
//! - The data stream is a 16-byte prefix, two bytes, then the compressed
//!   records, one after the other in the order of the block table: a
//!   selector (u32; its low byte is the record's type index), the record's
//!   bytes, its size again (u32) and a byte: 0, or 1 when an extended
//!   trailer (typed property lists, not read) follows; the stream ends
//!   with `FF FF FF FF` and a few bytes not read.
//!
//! The data is zlib-compressed in the files seen; a zstd frame is
//! recognised by its magic number too.

use std::fmt;
use std::io::Read;

#[derive(Clone, Debug, PartialEq)]
pub struct RseError(pub String);

impl fmt::Display for RseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for RseError {}

fn err<T>(message: impl Into<String>) -> Result<T, RseError> {
    Err(RseError(message.into()))
}

/// Size limit of a decompressed stream, against damaged or hostile files.
const MAX_STREAM: usize = 1 << 31;

/// Bounds-checked little-endian reads.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8], pos: usize) -> Self {
        Cursor { data, pos }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], RseError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| {
                RseError(format!(
                    "{n} bytes at {} past the end of {}",
                    self.pos,
                    self.data.len()
                ))
            })?;
        let b = &self.data[self.pos..end];
        self.pos = end;
        Ok(b)
    }

    fn u8(&mut self) -> Result<u8, RseError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, RseError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, RseError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn id(&mut self) -> Result<[u8; 16], RseError> {
        let mut id = [0u8; 16];
        id.copy_from_slice(self.take(16)?);
        Ok(id)
    }

    /// A count followed by that many items of `size` bytes: the count is
    /// checked against the bytes left.
    fn count(&mut self, size: usize) -> Result<usize, RseError> {
        let n = self.u32()? as usize;
        let left = self.data.len() - self.pos;
        if n.saturating_mul(size) > left {
            return err(format!("{n} items of {size} bytes with {left} bytes left"));
        }
        Ok(n)
    }
}

/// Whether compressed data starts here: a zlib header or a zstd frame.
fn compression_at(data: &[u8], at: usize) -> Option<Compression> {
    let b = data.get(at..at + 4)?;
    if b == [0x28, 0xB5, 0x2F, 0xFD] {
        return Some(Compression::Zstd);
    }
    // zlib: deflate with a window up to 32 KiB, and a check value.
    if b[0] & 0x0F == 8 && b[0] >> 4 <= 7 && (u16::from(b[0]) << 8 | u16::from(b[1])) % 31 == 0 {
        return Some(Compression::Zlib);
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    Zlib,
    Zstd,
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Compression::Zlib => "zlib",
            Compression::Zstd => "zstd",
        })
    }
}

/// Decompresses a zlib stream or a zstd frame.
pub fn decompress(data: &[u8], compression: Compression) -> Result<Vec<u8>, RseError> {
    match compression {
        Compression::Zlib => {
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, MAX_STREAM)
                .map_err(|e| RseError(format!("zlib: {e:?}")))
        }
        Compression::Zstd => {
            let mut rest = data;
            let mut decoder = ruzstd::decoding::StreamingDecoder::new(&mut rest)
                .map_err(|e| RseError(format!("zstd: {e}")))?;
            let mut out = Vec::new();
            decoder
                .by_ref()
                .take(MAX_STREAM as u64)
                .read_to_end(&mut out)
                .map_err(|e| RseError(format!("zstd: {e}")))?;
            Ok(out)
        }
    }
}

/// The header of a meta stream.
#[derive(Clone, Debug)]
pub struct MetaHeader {
    /// `RSe Meta Stream Version 8` in the files seen.
    pub tag: String,
    /// The segment's name (`PmBRepSegment`).
    pub name: String,
    /// Eight release stamps after the stream version; the low byte of the
    /// sixth is the segment's major version (25 for files saved by a 2021
    /// release, 28 by a 2024 one).
    pub stamps: [u16; 8],
    pub id: [u8; 16],
    /// Where the compressed tables start, and how they are compressed.
    pub data_offset: usize,
    pub compression: Compression,
}

impl MetaHeader {
    /// The segment's major version (see [`MetaHeader::stamps`]).
    pub fn major(&self) -> u8 {
        (self.stamps[5] & 0xFF) as u8
    }
}

/// Reads a meta stream's header.
pub fn meta_header(m: &[u8]) -> Result<MetaHeader, RseError> {
    let mut c = Cursor::new(m, 0);
    let n = c.count(1)?;
    let tag = String::from_utf8_lossy(c.take(n)?).into_owned();
    if !tag.starts_with("RSe Meta Stream") {
        return err(format!("meta stream starts with {tag:?}"));
    }
    // A version (u16) and eight u16 of release stamps.
    c.u16()?;
    let mut stamps = [0u16; 8];
    for s in &mut stamps {
        *s = c.u16()?;
    }
    let chars = c.count(2)?;
    let units: Vec<u16> = c
        .take(2 * chars)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    let name = String::from_utf16_lossy(&units);
    let id = c.id()?;
    // Three u32, then the creation and modification times as text.
    c.take(12)?;
    for _ in 0..2 {
        let n = c.count(1)?;
        c.take(n)?;
    }
    let flag = c.u8()?;
    let data_offset = c.pos;
    let compression = compression_at(m, data_offset).ok_or_else(|| {
        RseError(format!(
            "meta stream of {name}: flag {flag}, no compressed tables at {data_offset}"
        ))
    })?;
    Ok(MetaHeader {
        tag,
        name,
        stamps,
        id,
        data_offset,
        compression,
    })
}

/// A record type of a segment.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordType {
    pub id: [u8; 16],
    /// Two small numbers and two counts of unknown meaning, kept for
    /// inspection.
    pub fields: [u32; 4],
}

/// The tables of a segment's meta stream.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// The first number of the tables (a record slot count; one more than
    /// the blocks in the files seen).
    pub slots: u32,
    /// Distinguishes kinds of segment; 2 for B-rep segments.
    pub kind: u16,
    /// Record sizes, in data order; the top bit is a flag (set in every
    /// record seen).
    pub blocks: Vec<u32>,
    pub types: Vec<RecordType>,
}

/// Reads the decompressed tables of a meta stream.
pub fn tables(raw: &[u8]) -> Result<Tables, RseError> {
    let mut c = Cursor::new(raw, 0);
    let slots = c.u32()?;
    let kind = c.u16()?;
    c.take(8)?;
    let n = c.count(4)?;
    let blocks = (0..n).map(|_| c.u32()).collect::<Result<Vec<_>, _>>()?;
    // A table of 10-byte entries (node ids), then the type table.
    c.u32()?;
    let nodes = c.count(10)?;
    c.take(10 * nodes)?;
    c.take(12)?;
    let n = c.count(28)?;
    let mut types = Vec::with_capacity(n);
    for _ in 0..n {
        let id = c.id()?;
        let a = u32::from(c.u16()?);
        let b = u32::from(c.u16()?);
        let x = c.u32()?;
        let y = c.u32()?;
        types.push(RecordType {
            id,
            fields: [a, b, x, y],
        });
    }
    Ok(Tables {
        slots,
        kind,
        blocks,
        types,
    })
}

/// One record of a data stream.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Position in the block table.
    pub index: usize,
    pub selector: u32,
    /// Where the record's bytes are in the decompressed data.
    pub range: std::ops::Range<usize>,
    /// Its trailer after the echoed size and the byte after it: empty, or
    /// an extended trailer (not read).
    pub trailer: std::ops::Range<usize>,
}

impl Record {
    /// The record's type: an index into [`Tables::types`].
    pub fn type_index(&self) -> usize {
        (self.selector & 0xFF) as usize
    }
}

/// Where a data stream's compressed records start: after a 16-byte
/// prefix and two bytes in the files seen; else the first compressed
/// header in its first 64 bytes.
pub fn data_start(b: &[u8]) -> Result<(usize, Compression), RseError> {
    if let Some(c) = compression_at(b, 18) {
        return Ok((18, c));
    }
    (0..64.min(b.len()))
        .find_map(|at| compression_at(b, at).map(|c| (at, c)))
        .ok_or_else(|| RseError("data stream without compressed records".into()))
}

/// Splits decompressed data into records by the block table. A record
/// whose byte after its echoed size is not 0 has an extended trailer (typed
/// property and reference lists, not read here): its end is where the next
/// record starts, found by that record's echoed size, or the end marker
/// after the last record.
pub fn records(data: &[u8], tables: &Tables) -> Result<Vec<Record>, RseError> {
    let sizes: Vec<usize> = tables
        .blocks
        .iter()
        .map(|b| (b & 0x7FFF_FFFF) as usize)
        .collect();
    let mut c = Cursor::new(data, 0);
    let mut out = Vec::with_capacity(sizes.len());
    for (index, &size) in sizes.iter().enumerate() {
        let selector = c.u32()?;
        let start = c.pos;
        c.take(size)?;
        let echo = c.u32()?;
        if echo as usize != size {
            return err(format!("record {index}: size {size}, then {echo}"));
        }
        let trailer = c.u8()?;
        if tables.types.get((selector & 0xFF) as usize).is_none() {
            return err(format!(
                "record {index}: type {} of {}",
                selector & 0xFF,
                tables.types.len()
            ));
        }
        let trailer_start = c.pos;
        if trailer != 0 {
            c.pos = trailer_end(data, c.pos, sizes.get(index + 1).copied(), selector, tables)
                .ok_or_else(|| {
                    RseError(format!(
                        "record {index}: an extended trailer ({trailer:#x}) whose end is not found"
                    ))
                })?;
        }
        out.push(Record {
            index,
            selector,
            range: start..start + size,
            trailer: trailer_start..c.pos,
        });
    }
    Ok(out)
}

/// Where an extended trailer that starts at `from` ends: where the next
/// record (of `next` bytes) starts, a selector of the same form as
/// `selector` whose bytes are followed by their size, or the end marker
/// after the last record.
fn trailer_end(
    data: &[u8],
    from: usize,
    next: Option<usize>,
    selector: u32,
    tables: &Tables,
) -> Option<usize> {
    let u32_at = |at: usize| {
        data.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    match next {
        Some(n) => (from..data.len().saturating_sub(n + 8)).find(|&q| {
            u32_at(q).is_some_and(|s| {
                s >> 8 == selector >> 8 && ((s & 0xFF) as usize) < tables.types.len()
            }) && u32_at(q + 4 + n) == Some(n as u32)
        }),
        None => (from..data.len().saturating_sub(3)).find(|&q| u32_at(q) == Some(u32::MAX)),
    }
}

/// Writes a meta stream and a data stream for tests: one segment whose
/// records are (type index, bytes) with the given types, zlib-compressed.
pub fn write_segment(
    name: &str,
    id: [u8; 16],
    types: &[[u8; 16]],
    records: &[(u8, Vec<u8>)],
) -> (Vec<u8>, Vec<u8>) {
    let records: Vec<(u8, Vec<u8>, Vec<u8>)> = records
        .iter()
        .map(|(t, b)| (*t, b.clone(), Vec::new()))
        .collect();
    write_segment_with(name, id, 25, types, &records)
}

/// [`write_segment`] with the segment's major version and records with
/// extended trailers (type index, bytes, trailer; an empty trailer is the
/// byte 0, another is written after a byte 1).
pub fn write_segment_with(
    name: &str,
    id: [u8; 16],
    major: u8,
    types: &[[u8; 16]],
    records: &[(u8, Vec<u8>, Vec<u8>)],
) -> (Vec<u8>, Vec<u8>) {
    let compress = |raw: &[u8]| miniz_oxide::deflate::compress_to_vec_zlib(raw, 6);
    // The tables.
    let mut t = Vec::new();
    t.extend_from_slice(&(records.len() as u32 + 1).to_le_bytes());
    t.extend_from_slice(&2u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&(records.len() as u32).to_le_bytes());
    for (_, bytes, _) in records {
        t.extend_from_slice(&(bytes.len() as u32 | 0x8000_0000).to_le_bytes());
    }
    t.extend_from_slice(&(4 * (records.len() as u32 + 1)).to_le_bytes());
    t.extend_from_slice(&0u32.to_le_bytes());
    t.extend_from_slice(&[0u8; 12]);
    t.extend_from_slice(&(types.len() as u32).to_le_bytes());
    for id in types {
        t.extend_from_slice(id);
        t.extend_from_slice(&[0u8; 12]);
    }
    let mut m = Vec::new();
    let tag = b"RSe Meta Stream Version 8";
    m.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    m.extend_from_slice(tag);
    m.extend_from_slice(&8u16.to_le_bytes());
    for stamp in [2000, 2, 2424, 2037, 0, 0x4000 | u16::from(major), 0, 0] {
        m.extend_from_slice(&u16::to_le_bytes(stamp));
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    m.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for u in units {
        m.extend_from_slice(&u.to_le_bytes());
    }
    m.extend_from_slice(&id);
    m.extend_from_slice(&[0u8; 12]);
    for date in ["01/01/2026 00:00:00", "01/02/2026 00:00:00"] {
        m.extend_from_slice(&(date.len() as u32).to_le_bytes());
        m.extend_from_slice(date.as_bytes());
    }
    m.push(1);
    m.extend_from_slice(&compress(&t));
    // The records.
    let mut d = Vec::new();
    for (ty, bytes, trailer) in records {
        d.extend_from_slice(&(u32::from(*ty) | 0x100).to_le_bytes());
        d.extend_from_slice(bytes);
        d.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        if trailer.is_empty() {
            d.push(0);
        } else {
            d.push(1);
            d.extend_from_slice(trailer);
        }
    }
    d.extend_from_slice(&[0xFF; 4]);
    d.extend_from_slice(&[0; 14]);
    let mut b = vec![0u8; 16];
    b.extend_from_slice(&[4, 1]);
    b.extend_from_slice(&compress(&d));
    (m, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_written_segment() {
        let types = [[1u8; 16], [2u8; 16]];
        let written = vec![(0u8, b"first".to_vec()), (1, vec![9; 300]), (0, Vec::new())];
        let (m, b) = write_segment("TestSegment", [7; 16], &types, &written);
        let header = meta_header(&m).unwrap();
        assert_eq!(header.name, "TestSegment");
        assert_eq!(header.id, [7; 16]);
        assert_eq!(header.compression, Compression::Zlib);
        let t = tables(&decompress(&m[header.data_offset..], header.compression).unwrap()).unwrap();
        assert_eq!(t.blocks.len(), 3);
        assert_eq!(t.types.iter().map(|t| t.id).collect::<Vec<_>>(), types);
        let (start, compression) = data_start(&b).unwrap();
        let data = decompress(&b[start..], compression).unwrap();
        let r = records(&data, &t).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(&data[r[0].range.clone()], b"first");
        assert_eq!(r[1].type_index(), 1);
        assert_eq!(data[r[1].range.clone()], vec![9; 300]);
        assert!(r[2].range.is_empty());
        assert_eq!(&data[r[2].range.end + 5..r[2].range.end + 9], &[0xFF; 4]);
    }

    #[test]
    fn refuses_inconsistent_records() {
        let (m, b) = write_segment("S", [0; 16], &[[1; 16]], &[(0, vec![1, 2, 3])]);
        let header = meta_header(&m).unwrap();
        let mut t =
            tables(&decompress(&m[header.data_offset..], header.compression).unwrap()).unwrap();
        let (start, c) = data_start(&b).unwrap();
        let data = decompress(&b[start..], c).unwrap();
        t.blocks[0] = 0x8000_0002;
        assert!(records(&data, &t).is_err());
        t.blocks[0] = 0x8000_0003;
        t.types.clear();
        assert!(records(&data, &t).is_err());
        assert!(meta_header(b"\x04\x00\x00\x00nope").is_err());
    }

    #[test]
    fn skips_extended_trailers() {
        let types = [[1u8; 16], [2u8; 16]];
        let written = vec![
            (
                0u8,
                b"first".to_vec(),
                b"\x01\x00\x00\x00 a typed list".to_vec(),
            ),
            (1, vec![9; 30], Vec::new()),
            (0, b"last".to_vec(), vec![7; 12]),
        ];
        let (m, b) = write_segment_with("S", [3; 16], 28, &types, &written);
        let header = meta_header(&m).unwrap();
        assert_eq!(header.major(), 28);
        let t = tables(&decompress(&m[header.data_offset..], header.compression).unwrap()).unwrap();
        let (start, c) = data_start(&b).unwrap();
        let data = decompress(&b[start..], c).unwrap();
        let r = records(&data, &t).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(&data[r[0].range.clone()], b"first");
        assert_eq!(
            &data[r[0].trailer.clone()],
            b"\x01\x00\x00\x00 a typed list"
        );
        assert_eq!(data[r[1].range.clone()], vec![9; 30]);
        assert!(r[1].trailer.is_empty());
        assert_eq!(&data[r[2].range.clone()], b"last");
        assert_eq!(&data[r[2].trailer.clone()], &[7; 12]);
    }

    #[test]
    fn decompresses_zstd_frames() {
        // A raw zstd frame of one stored block: "abc".
        let frame = [
            0x28, 0xB5, 0x2F, 0xFD, 0x20, 0x03, 0x19, 0x00, 0x00, b'a', b'b', b'c',
        ];
        assert_eq!(compression_at(&frame, 0), Some(Compression::Zstd));
        assert_eq!(decompress(&frame, Compression::Zstd).unwrap(), b"abc");
    }
}
