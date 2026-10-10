// SPDX-License-Identifier: MIT
//! The definitions segment (`PmDCSegment`): parameters, sketches, features
//! and work features as a network of typed records that refer to each
//! other. See `README.md` (*The definitions segment*) for the layouts as far
//! as they are known.
//!
//! - A reference is a u32 whose bits 0 to 30 are a one-based record index
//!   (0: none) and whose bit 31 is a flag that does not change the index.
//! - Most records start with a header: u32 value, u16 id, u32 next
//!   reference, u32 flags, u32 context reference (the collection the record
//!   belongs to) and u32 node number (22 bytes). From segment major version
//!   28 a u32 more (0 in the files seen) comes before the context, so the
//!   header has 26 bytes.
//! - Lists start with a u16 kind, u16 `0x3000` and a u32 count; a list that
//!   is not empty then has two u32 (kind 2) or two u16 (kinds 3 and 8) the
//!   reader does not use, and the items, u32 each (references, mostly).
//! - Texts are a u32 count of UTF-16 code units and the units.

use std::fmt;
use std::ops::Range;

use crate::rse::{Record, Tables};

/// A 16-byte record type as stored, from its hex form at compile time.
pub const fn type_id(hex: &str) -> [u8; 16] {
    let b = hex.as_bytes();
    assert!(b.len() == 32, "a type id has 32 hex digits");
    let mut out = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        out[i] = (digit(b[2 * i]) << 4) | digit(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("not a hex digit"),
    }
}

/// The hex form of a type id.
pub fn type_text(id: &[u8; 16]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// The record a reference names (its index in the segment), or None for
/// the null reference.
pub fn reference(value: u32) -> Option<usize> {
    ((value & 0x7FFF_FFFF) as usize).checked_sub(1)
}

/// A read error: what was expected where.
#[derive(Clone, Debug, PartialEq)]
pub struct DcError(pub String);

impl fmt::Display for DcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DcError {}

pub type Result<T> = std::result::Result<T, DcError>;

/// The header most records start with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub value: u32,
    pub id: u16,
    pub next: Option<usize>,
    pub flags: u32,
    /// The collection the record belongs to.
    pub context: Option<usize>,
    /// A number that other records (the history state table) name the
    /// record by.
    pub node: u32,
}

/// The definitions segment's records.
#[derive(Clone, Debug, Default)]
pub struct Definitions {
    /// The segment's major version (25 for files saved by a 2021 release,
    /// 28 by a 2024 one).
    pub major: u8,
    data: Vec<u8>,
    ranges: Vec<Range<usize>>,
    kinds: Vec<usize>,
    types: Vec<[u8; 16]>,
}

impl Definitions {
    /// The records of a decompressed segment.
    pub fn new(major: u8, data: Vec<u8>, tables: &Tables, records: &[Record]) -> Definitions {
        Definitions {
            major,
            data,
            ranges: records.iter().map(|r| r.range.clone()).collect(),
            kinds: records.iter().map(Record::type_index).collect(),
            types: tables.types.iter().map(|t| t.id).collect(),
        }
    }

    /// Records made by tests: (type, bytes) in order.
    pub fn from_records(major: u8, records: &[([u8; 16], Vec<u8>)]) -> Definitions {
        let mut d = Definitions {
            major,
            ..Definitions::default()
        };
        for (t, bytes) in records {
            let kind = match d.types.iter().position(|x| x == t) {
                Some(k) => k,
                None => {
                    d.types.push(*t);
                    d.types.len() - 1
                }
            };
            let start = d.data.len();
            d.data.extend_from_slice(bytes);
            d.ranges.push(start..d.data.len());
            d.kinds.push(kind);
        }
        d
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// The bytes of record `i` (empty when there is none).
    pub fn bytes(&self, i: usize) -> &[u8] {
        self.ranges
            .get(i)
            .map_or(&[][..], |r| &self.data[r.clone()])
    }

    pub fn type_of(&self, i: usize) -> Option<&[u8; 16]> {
        self.kinds.get(i).and_then(|&k| self.types.get(k))
    }

    pub fn is(&self, i: usize, t: &[u8; 16]) -> bool {
        self.type_of(i) == Some(t)
    }

    /// The records of a type, in order.
    pub fn of_type<'a>(&'a self, t: &'a [u8; 16]) -> impl Iterator<Item = usize> + 'a {
        (0..self.len()).filter(move |&i| self.is(i, t))
    }

    /// The length of the common header.
    pub fn header_len(&self) -> usize {
        if self.major >= 28 { 26 } else { 22 }
    }

    /// Record `i`'s header, when it is long enough to have one.
    pub fn header(&self, i: usize) -> Option<Header> {
        let mut r = Reader::new(self.bytes(i));
        let value = r.u32().ok()?;
        let id = r.u16().ok()?;
        let next = r.reference().ok()?;
        let flags = r.u32().ok()?;
        if self.major >= 28 {
            r.u32().ok()?;
        }
        let context = r.reference().ok()?;
        let node = r.u32().ok()?;
        Some(Header {
            value,
            id,
            next,
            flags,
            context,
            node,
        })
    }

    /// A reader of record `i` after its header.
    pub fn body(&self, i: usize) -> Reader<'_> {
        let mut r = Reader::new(self.bytes(i));
        r.pos = self.header_len();
        r
    }

    /// The length of the prefix that the records of sketch entities,
    /// transforms, parameters and feature properties have after the
    /// header: from major 25 eight bytes (an i32, −1 or in a fillet edge
    /// set's parameter and booleans their index, and an i32 release stamp
    /// such as 2706), in majors 23 and 24 the first i32 alone, and none up
    /// to major 22 *(verified for majors 20 to 22, 24 and 27; 23 not
    /// seen)*.
    pub fn prefix_len(&self) -> usize {
        match self.major {
            0..=22 => 0,
            23 | 24 => 4,
            _ => 8,
        }
    }

    /// A reader of record `i` after its header and the prefix (see
    /// [`Definitions::prefix_len`]).
    pub fn fields(&self, i: usize) -> Reader<'_> {
        let mut r = Reader::new(self.bytes(i));
        r.pos = self.header_len() + self.prefix_len();
        r
    }

    /// A reader of record `i` from its start.
    pub fn reader(&self, i: usize) -> Reader<'_> {
        Reader::new(self.bytes(i))
    }

    /// The labels: the browser's names of records. A label record
    /// (`2ba4482b…`) starts with u32, u16, two u32 (from major 28 a u32
    /// more), then the labelled record, its parent and the next label;
    /// then an index, a reference list (the records shown under it: a
    /// feature's sketch, a folder's contents), the name and a 16-byte class
    /// id (zero for sketches, work features and folders). Labels in other
    /// forms are left out.
    pub fn labels(&self) -> std::collections::HashMap<usize, Label> {
        let mut out = std::collections::HashMap::new();
        for i in self.of_type(&LABEL) {
            let mut r = self.reader(i);
            let read = |r: &mut Reader| -> Result<(usize, Label)> {
                r.skip(if self.major >= 28 { 18 } else { 14 })?;
                let owner = r
                    .reference()?
                    .ok_or_else(|| DcError("no labelled record".to_owned()))?;
                r.skip(12)?;
                let children = r.references()?;
                let name = r.text()?;
                let mut class = [0u8; 16];
                class.copy_from_slice(r.take(16)?);
                Ok((
                    owner,
                    Label {
                        record: i,
                        name,
                        children,
                        class,
                    },
                ))
            };
            if let Ok((owner, label)) = read(&mut r) {
                out.entry(owner).or_insert(label);
            }
        }
        out
    }
}

/// A browser label of a record.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// The label record itself.
    pub record: usize,
    pub name: String,
    /// The records shown under it.
    pub children: Vec<usize>,
    /// The kind of feature (zero for others).
    pub class: [u8; 16],
}

/// The document record (the first record): the part's name and its
/// collections.
pub const DOCUMENT: [u8; 16] = type_id("164d8790d011f8d10008cabc0663dc09");
/// A collection of records (the part's root collection, parameter tables).
pub const COLLECTION: [u8; 16] = type_id("634d8790d011f8d10008cabc0663dc09");
/// A label: a browser node's name for a record.
pub const LABEL: [u8; 16] = type_id("2ba4482bd2115864600074b79b49ebb0");

/// Bounds-checked little-endian reads of a record.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Reader { data, pos }
    }

    pub fn left(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| {
                DcError(format!(
                    "{n} bytes at {} past the end of {}",
                    self.pos,
                    self.data.len()
                ))
            })?;
        let b = &self.data[self.pos..end];
        self.pos = end;
        Ok(b)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f64(&mut self) -> Result<f64> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(f64::from_le_bytes(a))
    }

    pub fn reference(&mut self) -> Result<Option<usize>> {
        Ok(reference(self.u32()?))
    }

    /// A u32-counted UTF-16 text.
    pub fn text(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        if n > self.left() / 2 {
            return Err(DcError(format!(
                "a text of {n} characters at {} with {} bytes left",
                self.pos - 4,
                self.left()
            )));
        }
        let units: Vec<u16> = self
            .take(2 * n)?
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        Ok(String::from_utf16_lossy(&units))
    }

    /// A list's kind and u32 items (see the module docs).
    pub fn list(&mut self) -> Result<(u16, Vec<u32>)> {
        let at = self.pos;
        let kind = self.u16()?;
        let tag = self.u16()?;
        if tag != 0x3000 {
            return Err(DcError(format!("no list at {at} ({kind:#x} {tag:#x})")));
        }
        let n = self.u32()? as usize;
        if n == 0 {
            return Ok((kind, Vec::new()));
        }
        match kind {
            2 => self.skip(8)?,
            3 | 8 => self.skip(4)?,
            _ => return Err(DcError(format!("a list of kind {kind} at {at}"))),
        }
        if n > self.left() / 4 {
            return Err(DcError(format!(
                "a list of {n} items at {at} with {} bytes left",
                self.left()
            )));
        }
        let items = (0..n).map(|_| self.u32()).collect::<Result<Vec<_>>>()?;
        Ok((kind, items))
    }

    /// A reference list: its records (null references left out).
    pub fn references(&mut self) -> Result<Vec<usize>> {
        Ok(self.list()?.1.into_iter().filter_map(reference).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_one_based() {
        assert_eq!(reference(0), None);
        assert_eq!(reference(0x8000_0000), None);
        assert_eq!(reference(1), Some(0));
        assert_eq!(reference(0x8000_0010), Some(15));
        assert_eq!(
            type_text(&type_id("264d8790d011f8d10008cabc0663dc09")),
            "264d8790d011f8d10008cabc0663dc09"
        );
    }

    #[test]
    fn reads_headers_lists_and_texts() {
        let mut rec = Vec::new();
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&7u16.to_le_bytes());
        rec.extend_from_slice(&0x8000_0002u32.to_le_bytes());
        rec.extend_from_slice(&0x0003_4200u32.to_le_bytes());
        rec.extend_from_slice(&0x8000_0001u32.to_le_bytes());
        rec.extend_from_slice(&99u32.to_le_bytes());
        // A reference list of two, then a text.
        rec.extend_from_slice(&[2, 0, 0, 0x30, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        rec.extend_from_slice(&0x8000_0001u32.to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&2u32.to_le_bytes());
        rec.extend_from_slice(&[b'd', 0, b'1', 0]);
        let d =
            Definitions::from_records(25, &[(COLLECTION, vec![0; 30]), (DOCUMENT, rec.clone())]);
        let h = d.header(1).unwrap();
        assert_eq!(h.id, 7);
        assert_eq!(h.next, Some(1));
        assert_eq!(h.context, Some(0));
        assert_eq!(h.node, 99);
        let mut r = d.body(1);
        assert_eq!(r.references().unwrap(), vec![0]);
        assert_eq!(r.text().unwrap(), "d1");
        assert!(r.text().is_err());
        // From major 28 the header has a u32 more before the context.
        let mut long = rec[..14].to_vec();
        long.extend_from_slice(&0u32.to_le_bytes());
        long.extend_from_slice(&rec[14..]);
        let d = Definitions::from_records(28, &[(COLLECTION, vec![0; 30]), (DOCUMENT, long)]);
        assert_eq!(d.header(1).unwrap().context, Some(0));
        assert_eq!(d.body(1).references().unwrap(), vec![0]);
    }
}
