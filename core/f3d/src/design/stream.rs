// SPDX-License-Identifier: MIT
//! Object database framing of a design segment: `MetaStream.dat` (class
//! table, named roots, object and sub-chunk indexes, external documents)
//! and the objects of `BulkStream.dat` (header, root part, references,
//! strings). Layouts: timeline format study, sections 2 to 4.
//!
//! The helpers reproduce the reference decoder byte for byte, including
//! its scanning heuristics, so that both decoders make the same choices on
//! every file.

use std::collections::HashMap;
use std::fmt;

use super::unicode;

/// Little-endian reads at a byte position; `None` past the end.
pub(crate) fn u32_at(d: &[u8], p: usize) -> Option<u32> {
    let b = d.get(p..p.checked_add(4)?)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub(crate) fn i32_at(d: &[u8], p: usize) -> Option<i32> {
    u32_at(d, p).map(|v| v as i32)
}

pub(crate) fn u64_at(d: &[u8], p: usize) -> Option<u64> {
    let b = d.get(p..p.checked_add(8)?)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(b);
    Some(u64::from_le_bytes(a))
}

pub(crate) fn f64_at(d: &[u8], p: usize) -> Option<f64> {
    u64_at(d, p).map(f64::from_bits)
}

/// `n` consecutive f64 values.
pub(crate) fn f64s_at(d: &[u8], p: usize, n: usize) -> Option<Vec<f64>> {
    (0..n).map(|i| f64_at(d, p + 8 * i)).collect()
}

/// Python 3.12's `sum()` of floats (start 0): Neumaier-compensated, so a
/// length or a norm rounds exactly as in the reference decoder.
pub(crate) fn py_sum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let mut it = xs.into_iter();
    let Some(first) = it.next() else {
        return 0.0;
    };
    let mut f = 0.0 + first;
    let mut c = 0.0;
    for x in it {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// `d[a..b]` clamped to the data, like a Python slice.
pub(crate) fn slice(d: &[u8], a: usize, b: usize) -> &[u8] {
    let a = a.min(d.len());
    &d[a..b.clamp(a, d.len())]
}

/// Lower-case hex of bytes (Python `bytes.hex()`).
pub(crate) fn hex(d: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(2 * d.len());
    for b in d {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// ISO 8859-1 bytes as text (every byte is one character).
pub(crate) fn latin1(d: &[u8]) -> String {
    d.iter().map(|&b| b as char).collect()
}

/// UTF-16LE code units (a trailing odd byte is dropped).
pub(crate) fn utf16_units(b: &[u8]) -> Vec<u16> {
    b.as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

/// A `str16` at `p`: u32 character count (at most 4096) and strict
/// UTF-16LE. Returns the text and the end offset.
pub(crate) fn str16_at(d: &[u8], p: usize) -> Option<(String, usize)> {
    let n = u32_at(d, p)? as usize;
    if n > 4096 || p + 4 + 2 * n > d.len() {
        return None;
    }
    let units = utf16_units(&d[p + 4..p + 4 + 2 * n]);
    String::from_utf16(&units).ok().map(|s| (s, p + 4 + 2 * n))
}

/// A non-empty, printable `str16` at `p`.
pub(crate) fn text_at(d: &[u8], p: usize) -> Option<(String, usize)> {
    str16_at(d, p).filter(|(s, _)| !s.is_empty() && unicode::is_printable(s))
}

/// Every printable non-empty `str16` found by scanning the data.
pub(crate) fn str16s(d: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 4 < d.len() {
        match text_at(d, p) {
            Some((s, e)) => {
                out.push(s);
                p = e;
            }
            None => p += 1,
        }
    }
    out
}

/// An object header (section 4.1): `str8 tag | u64 id | str8 name`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectHeader {
    /// The class tag as written: decimal 256 + class index.
    pub tag: String,
    pub id: u64,
    /// Root name (`rootInstance`, `ComponentsRoot`, ...); usually empty.
    pub name: String,
    /// Offset after the header.
    pub end: usize,
}

impl ObjectHeader {
    pub fn parse(d: &[u8]) -> Option<ObjectHeader> {
        let n = u32_at(d, 0)? as usize;
        let tag = latin1(d.get(4..4 + n)?);
        let id = u64_at(d, 4 + n)?;
        let ln = u32_at(d, 12 + n)? as usize;
        let name = latin1(d.get(16 + n..16 + n + ln)?);
        Some(ObjectHeader {
            tag,
            id,
            name,
            end: 16 + n + ln,
        })
    }

    /// The class table index the tag names.
    pub fn class_index(&self) -> Option<usize> {
        self.tag.parse::<usize>().ok()?.checked_sub(256)
    }
}

/// Offset after the object header: `str8 tag | u64 id | str8 name`.
pub(crate) fn header_end(d: &[u8]) -> Option<usize> {
    let n = u32_at(d, 0)? as usize;
    let p = 4 + n + 8;
    let ln = u32_at(d, p)? as usize;
    Some(p + 4 + ln)
}

/// Error from reading the MetaStream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MetaStream at {:#x}: {}", self.offset, self.message)
    }
}

impl std::error::Error for MetaError {}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn fail(&self, what: &str) -> MetaError {
        MetaError {
            offset: self.p,
            message: format!("truncated {what}"),
        }
    }

    fn eof(&self) -> bool {
        self.p >= self.d.len()
    }

    fn u32(&mut self) -> Result<u32, MetaError> {
        let v = u32_at(self.d, self.p).ok_or_else(|| self.fail("u32"))?;
        self.p += 4;
        Ok(v)
    }

    fn u64(&mut self) -> Result<u64, MetaError> {
        let v = u64_at(self.d, self.p).ok_or_else(|| self.fail("u64"))?;
        self.p += 8;
        Ok(v)
    }

    fn bytes(&mut self, n: usize, what: &str) -> Result<&[u8], MetaError> {
        if n > 1 << 20 {
            return Err(MetaError {
                offset: self.p,
                message: format!("{what} length {n}"),
            });
        }
        let b = self
            .d
            .get(self.p..self.p + n)
            .ok_or_else(|| self.fail(what))?;
        self.p += n;
        Ok(b)
    }

    /// `str8`: u32 length + bytes (ISO 8859-1).
    fn str8(&mut self) -> Result<String, MetaError> {
        let n = self.u32()? as usize;
        Ok(latin1(self.bytes(n, "str8")?))
    }

    /// `str16`: u32 character count + UTF-16LE (invalid units replaced).
    fn str16(&mut self) -> Result<String, MetaError> {
        let n = self.u32()? as usize;
        if n > 1 << 20 {
            return Err(MetaError {
                offset: self.p - 4,
                message: format!("str16 length {n}"),
            });
        }
        let units = utf16_units(self.bytes(2 * n, "str16")?);
        Ok(String::from_utf16_lossy(&units))
    }

    fn pairs(&mut self) -> Result<Vec<(u64, u64)>, MetaError> {
        let n = self.u32()?;
        let mut out = Vec::new();
        for _ in 0..n {
            out.push((self.u64()?, self.u64()?));
        }
        Ok(out)
    }
}

/// One record of the class table (section 3.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassRecord {
    /// Class GUID as written (a few are not in the usual 8-4-4-4-12 form).
    pub guid: String,
    /// Parent class GUID; empty for the root class.
    pub parent: String,
    /// Class schema version; selects the field layout.
    pub version: u32,
    /// Module name (`CommonData`, `MSketch`, ...), may be empty.
    pub module: String,
    /// Objects of this class.
    pub ids: Vec<u64>,
}

/// An external document of the MetaStream tail (section 3.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalRef {
    /// Key that external references in the BulkStream carry.
    pub key: u64,
    pub guid_a: String,
    pub guid_b: String,
    /// Document URN, e.g. `urn:<service>:fs.file:vf.<id>?version=6`.
    pub urn: String,
    pub guid_c: String,
    pub value: u32,
}

/// The parsed `MetaStream.dat` of a segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaStream {
    /// The segment type (`Design`, ...).
    pub segment: String,
    pub h0: u32,
    /// Not reliable (zeros or uninitialised memory in some files).
    pub guid: String,
    /// Header form A: `0x0800xxxx` (the writer's 2.0 build in the low 16 bits);
    /// form B: 1234.
    pub magic: u32,
    /// Form A: `[k]`; form B: `[v0, build, release]`.
    pub version: Vec<u32>,
    pub segment2: String,
    pub module: String,
    /// Segment version and the zero after it.
    pub h1: [u32; 2],
    pub classes: Vec<ClassRecord>,
    /// Objects with a root name (`AssetSettings`, `rootInstance`, ...).
    pub named_roots: Vec<u64>,
    /// Object id -> BulkStream offset, ascending offsets.
    pub index: Vec<(u64, u64)>,
    /// Object id -> BulkStream offset of the object's sub-chunk header.
    pub subchunks: Vec<(u64, u64)>,
    /// Next free object id; `None` if the tail is missing.
    pub next_id: Option<u64>,
    pub external: Vec<ExternalRef>,
    /// Form B tail: `("Application", n)`, `("Server", n)`.
    pub feature_versions: Vec<(String, u32)>,
    /// The tail parsed to the end of the stream.
    pub tail_ok: bool,
}

impl MetaStream {
    pub fn parse(data: &[u8]) -> Result<MetaStream, MetaError> {
        let mut r = Reader { d: data, p: 0 };
        let segment = r.str8()?;
        let h0 = r.u32()?;
        let guid = r.str16()?;
        let magic = r.u32()?;
        let version = if magic == 1234 {
            vec![r.u32()?, r.u32()?, r.u32()?]
        } else {
            vec![r.u32()?]
        };
        let segment2 = r.str8()?;
        let module = r.str8()?;
        let h1 = [r.u32()?, r.u32()?];
        let n = r.u32()?;
        let mut classes = Vec::new();
        for _ in 0..n {
            let guid = r.str8()?;
            let parent = r.str8()?;
            let version = r.u32()?;
            let module = r.str8()?;
            let k = r.u32()?;
            let mut ids = Vec::new();
            for _ in 0..k {
                ids.push(r.u64()?);
            }
            classes.push(ClassRecord {
                guid,
                parent,
                version,
                module,
                ids,
            });
        }
        let n = r.u32()?;
        let mut named_roots = Vec::new();
        for _ in 0..n {
            named_roots.push(r.u64()?);
        }
        let index = r.pairs()?;
        let mut meta = MetaStream {
            segment,
            h0,
            guid,
            magic,
            version,
            segment2,
            module,
            h1,
            classes,
            named_roots,
            index,
            subchunks: Vec::new(),
            next_id: None,
            external: Vec::new(),
            feature_versions: Vec::new(),
            tail_ok: false,
        };
        // The sub-chunk index and the tail are optional: a failure leaves
        // what was read (as the reference decoder does).
        if let Ok(s) = r.pairs() {
            meta.subchunks = s;
        }
        meta.parse_tail(&mut r);
        Ok(meta)
    }

    /// `u64 next_id | [u32 n, n xrefs] | [u32 m, m (str8 name, u32 version)]`.
    fn parse_tail(&mut self, r: &mut Reader) {
        let Ok(next) = r.u64() else { return };
        self.next_id = Some(next);
        if r.eof() {
            self.tail_ok = true;
            return;
        }
        let Ok(n) = r.u32() else { return };
        for _ in 0..n {
            let x = (|| {
                Ok::<_, MetaError>(ExternalRef {
                    key: r.u64()?,
                    guid_a: r.str16()?,
                    guid_b: r.str16()?,
                    urn: r.str16()?,
                    guid_c: r.str16()?,
                    value: r.u32()?,
                })
            })();
            match x {
                Ok(x) => self.external.push(x),
                Err(_) => return,
            }
        }
        if r.eof() {
            self.tail_ok = true;
            return;
        }
        let Ok(m) = r.u32() else { return };
        for _ in 0..m {
            let (Ok(name), Ok(v)) = (r.str8(), r.u32()) else {
                return;
            };
            self.feature_versions.push((name, v));
        }
        self.tail_ok = r.eof();
    }

    /// Header form B (2025 and later, `magic` 1234).
    pub fn is_form_b(&self) -> bool {
        self.magic == 1234
    }

    /// The writer's 2.0 build number (form A).
    pub fn writer_build(&self) -> Option<u32> {
        (!self.is_form_b()).then_some(self.magic & 0xffff)
    }

    /// Writer version: `2.0.<build>` (form A) or
    /// `<release >> 18>.<(release >> 6) & 0xfff>.<build>` (form B).
    pub fn writer(&self) -> String {
        if self.is_form_b() {
            let release = self.version.get(2).copied().unwrap_or(0);
            let build = self.version.get(1).copied().unwrap_or(0);
            format!("{}.{}.{}", release >> 18, (release >> 6) & 0xfff, build)
        } else {
            format!("2.0.{}", self.magic & 0xffff)
        }
    }
}

/// A value of a root-part attribute (section 4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrValue {
    U64(u64),
    Bool(u8),
    Text(String),
}

/// An attribute of the root part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub key: String,
    pub type_name: String,
    pub value: AttrValue,
}

/// The root-class part after the object header (section 4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootPart {
    /// Root reference list (`None` for null references).
    pub refs: Vec<Option<u64>>,
    pub attrs: Vec<Attribute>,
    /// Offset after the root part (object relative).
    pub end: usize,
}

/// A reference read from object data (section 4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ref {
    /// Object id: in this document, or in the external document of `context`.
    pub id: u64,
    /// Offset after the reference.
    pub end: usize,
    /// External document key (the reference goes through an assembly
    /// context), if any.
    pub context: Option<u64>,
}

/// One object of the BulkStream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Object {
    pub id: u64,
    /// BulkStream offsets of the object (`end` = next object or stream end).
    pub offset: u64,
    pub end: u64,
    /// Index into the class table.
    pub class: Option<usize>,
    /// BulkStream offset of the sub-chunk header, if the object has one.
    pub subchunk: Option<u64>,
}

/// A design segment's streams with lookups by id and class.
pub struct Segment {
    pub meta: MetaStream,
    pub bulk: Vec<u8>,
    /// Objects in BulkStream order.
    pub objects: Vec<Object>,
    by_id: HashMap<u64, usize>,
    /// Upper-case GUID of every class record.
    class_upper: Vec<String>,
    /// Upper-case GUID -> last class record with that GUID.
    by_guid: HashMap<String, usize>,
    /// The references carry the target's class GUID (writer 2.0.5119).
    pub long_refs: bool,
}

impl Segment {
    /// Splits the BulkStream into objects with the MetaStream index.
    pub fn new(meta: MetaStream, bulk: Vec<u8>) -> Segment {
        let class_upper: Vec<String> = meta.classes.iter().map(|c| c.guid.to_uppercase()).collect();
        let mut by_guid = HashMap::new();
        for (i, g) in class_upper.iter().enumerate() {
            by_guid.insert(g.clone(), i);
        }
        let mut class_of_id = HashMap::new();
        for (i, c) in meta.classes.iter().enumerate() {
            for &id in &c.ids {
                class_of_id.insert(id, i);
            }
        }
        let mut sub = HashMap::new();
        for &(id, off) in &meta.subchunks {
            sub.insert(id, off);
        }
        let mut order = meta.index.clone();
        order.sort_by_key(|&(_, off)| off);
        let objects: Vec<Object> = order
            .iter()
            .enumerate()
            .map(|(i, &(id, offset))| Object {
                id,
                offset,
                end: order.get(i + 1).map_or(bulk.len() as u64, |n| n.1),
                class: class_of_id.get(&id).copied(),
                subchunk: sub.get(&id).copied(),
            })
            .collect();
        let mut by_id = HashMap::new();
        for (i, o) in objects.iter().enumerate() {
            by_id.insert(o.id, i);
        }
        let long_refs = detect_long_refs(&bulk);
        Segment {
            meta,
            bulk,
            objects,
            by_id,
            class_upper,
            by_guid,
            long_refs,
        }
    }

    pub fn parse(meta: &[u8], bulk: Vec<u8>) -> Result<Segment, MetaError> {
        Ok(Segment::new(MetaStream::parse(meta)?, bulk))
    }

    /// The object with this id.
    pub fn object(&self, id: u64) -> Option<&Object> {
        self.by_id.get(&id).map(|&i| &self.objects[i])
    }

    pub fn contains(&self, id: u64) -> bool {
        self.by_id.contains_key(&id)
    }

    /// The bytes of an object (empty if the index is inconsistent).
    pub fn data(&self, o: &Object) -> &[u8] {
        slice(&self.bulk, o.offset as usize, o.end as usize)
    }

    /// The bytes of the object with this id.
    pub fn data_of(&self, id: u64) -> &[u8] {
        self.object(id).map_or(&[], |o| self.data(o))
    }

    /// The object's header.
    pub fn header(&self, o: &Object) -> Option<ObjectHeader> {
        ObjectHeader::parse(self.data(o))
    }

    /// Object-relative end of the main part: the sub-chunk offset, or the
    /// object end when there is none (or the sub-chunk is at offset 0).
    pub fn main_end(&self, o: &Object) -> usize {
        match o.subchunk {
            Some(s) if s != 0 => s.saturating_sub(o.offset) as usize,
            _ => self.data(o).len(),
        }
    }

    /// Object-relative start of the sub-chunk (0 when there is none).
    pub fn sub_start(&self, o: &Object) -> usize {
        match o.subchunk {
            Some(s) if s != 0 => s.saturating_sub(o.offset) as usize,
            _ => 0,
        }
    }

    pub fn class(&self, o: &Object) -> Option<&ClassRecord> {
        o.class.map(|i| &self.meta.classes[i])
    }

    /// Upper-case class GUID of an object.
    pub fn guid(&self, o: &Object) -> Option<&str> {
        o.class.map(|i| self.class_upper[i].as_str())
    }

    /// Upper-case class GUID of the object with this id.
    pub fn guid_of(&self, id: u64) -> Option<&str> {
        self.object(id).and_then(|o| self.guid(o))
    }

    /// Class version of an object.
    pub fn version(&self, o: &Object) -> Option<u32> {
        self.class(o).map(|c| c.version)
    }

    /// Objects of exactly this class (upper-case GUID), in stream order.
    pub fn objects_of<'a>(&'a self, guid: &'a str) -> impl Iterator<Item = &'a Object> + 'a {
        self.objects
            .iter()
            .filter(move |o| self.guid(o) == Some(guid))
    }

    /// The class record for an upper-case GUID (the last one with it).
    pub fn class_by_guid(&self, guid: &str) -> Option<&ClassRecord> {
        self.by_guid.get(guid).map(|&i| &self.meta.classes[i])
    }

    /// The class and its ancestors (upper-case GUIDs), following the
    /// class table.
    pub fn chain(&self, guid: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut i = self.by_guid.get(&guid.to_uppercase()).copied();
        while let Some(c) = i {
            out.push(self.class_upper[c].as_str());
            // A cycle cannot occur in a valid table; stop anyway.
            if out.len() > 256 {
                break;
            }
            let parent = self.meta.classes[c].parent.to_uppercase();
            i = if parent.is_empty() {
                None
            } else {
                self.by_guid.get(&parent).copied()
            };
        }
        out
    }

    /// The object's class is `base` or derives from it.
    pub fn is_kind_of(&self, id: u64, base: &str) -> bool {
        self.guid_of(id)
            .is_some_and(|g| self.chain(g).contains(&base))
    }

    /// Objects whose class is `base` or derives from it, in stream order.
    pub fn objects_of_kind(&self, base: &str) -> Vec<&Object> {
        let kind: Vec<bool> = self
            .class_upper
            .iter()
            .map(|g| self.chain(g).contains(&base))
            .collect();
        self.objects
            .iter()
            .filter(|o| o.class.is_some_and(|c| kind[c]))
            .collect()
    }

    fn guid_matches(&self, id: u64, bytes: &[u8]) -> bool {
        let Some(target) = self.guid_of(id) else {
            return false;
        };
        if bytes.is_ascii() {
            bytes.len() == target.len()
                && bytes
                    .iter()
                    .zip(target.bytes())
                    .all(|(a, b)| a.to_ascii_uppercase() == b)
        } else {
            latin1(bytes).to_uppercase() == target
        }
    }

    /// A non-null reference at `p` (section 4.4):
    /// `01 id [str8 guid] 00 00` or, through an assembly context,
    /// `01 id [str8 guid] 01 key 00 str8 guid 00`. The id must be an object
    /// of this segment and the GUIDs must name its class.
    pub fn ref_at(&self, d: &[u8], p: usize) -> Option<Ref> {
        if p + 11 > d.len() || d[p] != 1 {
            return None;
        }
        let id = u64_at(d, p + 1)?;
        if !self.contains(id) {
            return None;
        }
        let mut q = p + 9;
        if self.long_refs {
            if d.get(q..q + 4) != Some(&[0x24, 0, 0, 0][..]) {
                return None;
            }
            if !self.guid_matches(id, slice(d, q + 4, q + 40)) {
                return None;
            }
            q += 40;
        }
        let mut context = None;
        if d.get(q..q + 2) == Some(&[0, 0][..]) {
            q += 2;
        } else if d.get(q) == Some(&1) && d.get(q + 9..q + 14) == Some(&[0, 0x24, 0, 0, 0][..]) {
            context = u64_at(d, q + 1);
            if !self.guid_matches(id, slice(d, q + 14, q + 50)) || d.get(q + 50) != Some(&0) {
                return None;
            }
            q += 51;
        } else {
            return None;
        }
        Some(Ref {
            id,
            end: q,
            context,
        })
    }

    /// All references found by scanning `d[start..end]` (a reference may
    /// extend past `end`).
    pub fn refs_in(&self, d: &[u8], start: usize, end: usize) -> Vec<(usize, Ref)> {
        let mut out = Vec::new();
        let mut p = start;
        let end = end.min(d.len());
        while p < end {
            match self.ref_at(d, p) {
                Some(r) => {
                    out.push((p, r));
                    p = r.end;
                }
                None => p += 1,
            }
        }
        out
    }

    /// Ids of all references in an object's data, in order.
    pub fn ref_ids(&self, d: &[u8]) -> Vec<u64> {
        self.refs_in(d, 0, d.len())
            .into_iter()
            .map(|(_, r)| r.id)
            .collect()
    }

    /// `u32 n` + `n` references at `p`; `None` unless all `n` are valid.
    pub fn ref_list_at(&self, d: &[u8], p: usize) -> Option<(Vec<u64>, usize)> {
        let n = u32_at(d, p)?;
        let mut q = p + 4;
        let mut out = Vec::new();
        for _ in 0..n {
            let r = self.ref_at(d, q)?;
            out.push(r.id);
            q = r.end;
        }
        Some((out, q))
    }

    /// The root-class part after the header (section 4.2).
    pub fn root_part(&self, d: &[u8]) -> Option<RootPart> {
        let mut p = header_end(d)?;
        let a = *d.get(p)?;
        p += 1;
        if a > 1 {
            return None;
        }
        let mut refs = Vec::new();
        if a == 1 {
            let n = u32_at(d, p)?;
            p += 4;
            if n > 100_000 {
                return None;
            }
            for _ in 0..n {
                if *d.get(p)? == 0 {
                    refs.push(None);
                    p += 1;
                    continue;
                }
                let r = self.ref_at(d, p)?;
                refs.push(Some(r.id));
                p = r.end;
            }
        }
        let h = *d.get(p)?;
        p += 1;
        if h > 1 {
            return None;
        }
        let mut attrs = Vec::new();
        if h == 1 {
            let n = u32_at(d, p)?;
            p += 4;
            if n > 1000 {
                return None;
            }
            for _ in 0..n {
                let (attr, end) = attribute_at(d, p)?;
                attrs.push(attr);
                p = end;
            }
        }
        Some(RootPart {
            refs,
            attrs,
            end: p,
        })
    }
}

/// One root-part attribute: `str8 key, str8 type, value`.
fn attribute_at(d: &[u8], p: usize) -> Option<(Attribute, usize)> {
    let ln = u32_at(d, p)? as usize;
    let key = latin1(slice(d, p + 4, p + 4 + ln));
    let p = p + 4 + ln;
    let ln = u32_at(d, p)? as usize;
    let typ = slice(d, p + 4, p + 4 + ln).to_vec();
    let mut p = p + 4 + ln;
    let value = if typ.ends_with(b"uint64") {
        let v = u64_at(d, p)?;
        p += 8;
        AttrValue::U64(v)
    } else if typ.ends_with(b"bool") {
        let v = *d.get(p)?;
        p += 1;
        AttrValue::Bool(v)
    } else if typ.ends_with(b"IString") {
        let n = u32_at(d, p)? as usize;
        let units = utf16_units(slice(d, p + 4, p + 4 + 2 * n));
        p += 4 + 2 * n;
        AttrValue::Text(String::from_utf16_lossy(&units))
    } else {
        return None;
    };
    Some((
        Attribute {
            key,
            type_name: latin1(&typ),
            value,
        },
        p,
    ))
}

/// The references carry class GUIDs: more than five `01 u64 str8(36)`
/// patterns among the GUID-shaped strings of the first 400 000 bytes.
fn detect_long_refs(bulk: &[u8]) -> bool {
    let end = bulk.len().min(400_000);
    let mut n = 0;
    let mut p = 0;
    while p + 36 <= end {
        if is_guid(&bulk[p..p + 36]) {
            if p >= 13 && bulk[p - 4..p] == [0x24, 0, 0, 0] && bulk[p - 13] == 1 {
                n += 1;
                if n > 5 {
                    return true;
                }
            }
            p += 36;
        } else {
            p += 1;
        }
    }
    false
}

/// `8-4-4-4-12` hex digits.
fn is_guid(b: &[u8]) -> bool {
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_strings_and_scalars() {
        let mut d = vec![2, 0, 0, 0, b'X', 0, b'Y', 0];
        d.extend(1.5f64.to_le_bytes());
        assert_eq!(str16_at(&d, 0), Some(("XY".to_string(), 8)));
        assert_eq!(f64_at(&d, 8), Some(1.5));
        assert_eq!(f64_at(&d, 9), None);
        assert_eq!(hex(&[0, 0xab, 1]), "00ab01");
        // Unpaired surrogate: not a str16.
        assert_eq!(str16_at(&[1, 0, 0, 0, 0x00, 0xd8], 0), None);
        // Control characters are not printable text.
        assert_eq!(text_at(&[1, 0, 0, 0, 7, 0], 0), None);
        assert_eq!(str16s(&[0, 1, 0, 0, 0, b'a', 0, 9]), vec!["a".to_string()]);
        assert_eq!(slice(&[1, 2, 3], 2, 9), &[3]);
        assert_eq!(slice(&[1, 2, 3], 5, 9), &[] as &[u8]);
        // Compensated like Python's sum(): 0.1 + 0.2 + 0.3 == 0.6 exactly.
        assert_eq!(py_sum([0.1, 0.2, 0.3]), 0.6);
        assert_ne!(0.1 + 0.2 + 0.3, 0.6);
        assert_eq!(py_sum([f64::INFINITY, 1.0]), f64::INFINITY);
    }

    #[test]
    fn detects_guid_shapes() {
        assert!(is_guid(b"2F4C1849-1A5A-4F6C-A086-8DD445CBF94B"));
        assert!(!is_guid(b"2F4C1849-1A5A-4F6C-A086-8DD445CBF94"));
        assert!(!is_guid(b"2F4C1849+1A5A-4F6C-A086-8DD445CBF94B"));
    }
}
