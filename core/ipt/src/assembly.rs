// SPDX-License-Identifier: MIT
//! Assemblies (`.iam`): the documents an assembly refers to and the
//! occurrences that place them. See `README.md` (*Assemblies*) for the
//! layouts as far as they are known.
//!
//! An assembly is a compound file with the segment database, as a part,
//! whose root storage has the class id [`ASSEMBLY_CLSID`]. Three places
//! make up its occurrences:
//!
//! - the definitions segment (`AmDcSegment`): one occurrence record per
//!   occurrence, with its flags (grounded, hidden, suppressed) and its
//!   browser label; it names the occurrence's key through a proxy record;
//! - the reference segment (`AmRxSegment`): per key, a document
//!   descriptor (the referenced document's ids, its range box, the model
//!   state) and a placement (the occurrence's 4x4 transform in the
//!   assembly, cm);
//! - the `UFRxDoc` stream: the referenced files (their paths as saved, ids)
//!   and a table that gives each key its file and its instance number.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::cfb::guid_bytes;
use crate::dc::{self, Definitions, Reader};
use crate::{IptError, IptFile};

/// The class id of an assembly's root storage.
pub const ASSEMBLY_CLSID: &str = "e60f81e1-49b3-11d0-93c3-7e0706000000";

/// The assembly's definitions segment.
pub const DEFINITIONS_SEGMENT: &str = "AmDcSegment";
/// The segment with the occurrences' documents and placements.
pub const REFERENCES_SEGMENT: &str = "AmRxSegment";
/// The stream with the referenced files.
pub const REFERENCES_STREAM: &str = "/UFRxDoc";

/// An occurrence (definitions segment).
pub const OCCURRENCE: [u8; 16] = dc::type_id("614d8790d011f8d10008cabc0663dc09");
/// The record an occurrence names its key by (definitions segment).
pub const OCCURRENCE_KEY: [u8; 16] = dc::type_id("604d8790d011f8d10008cabc0663dc09");
/// An occurrence's placement (reference segment).
pub const PLACEMENT: [u8; 16] = dc::type_id("bc922723b04480c82c6d528fa117de79");
/// An occurrence's document descriptor (reference segment).
pub const DESCRIPTOR: [u8; 16] = dc::type_id("fe32cdfdfc4349240b596b8ccc01f1ad");
/// The graphics segment, which some releases keep the displayed
/// occurrences' transforms in.
pub const GRAPHICS_SEGMENT: &str = "AmGraphicsSegment";
/// An occurrence's display transform (graphics segment).
pub const DISPLAY: [u8; 16] = dc::type_id("07d0d0b9d4112d5f6000f8830e73fcb0");

/// Occurrence flags (the header's flags of an occurrence record).
pub const FLAG_HIDDEN: u32 = 0x0100_0000;
pub const FLAG_GROUNDED: u32 = 0x0200_0000;
pub const FLAG_SUPPRESSED: u32 = 0x4000_0000;

/// The flag of a descriptor's body that the part keeps hidden.
pub const BODY_HIDDEN: u32 = 0x08;

/// A document the assembly refers to, as the `UFRxDoc` stream lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    /// The id the occurrence table names it by.
    pub id: u32,
    /// The full path as saved (on the machine that saved the assembly).
    pub path: String,
    /// A library's name for library parts (`Content Center Files`), else
    /// empty.
    pub library: String,
    /// The library member's name, else empty.
    pub member: String,
    /// A variant's name (stored for some references), if any.
    pub variant: Option<String>,
}

impl Reference {
    /// The file name of the saved path (either separator).
    pub fn file_name(&self) -> &str {
        self.path.rsplit(['\\', '/']).next().unwrap_or(&self.path)
    }

    /// The file name without its extension.
    pub fn stem(&self) -> &str {
        let name = self.file_name();
        name.rsplit_once('.').map_or(name, |(stem, _)| stem)
    }

    /// Whether it is an assembly (by its extension).
    pub fn is_assembly(&self) -> bool {
        self.file_name().to_ascii_lowercase().ends_with(".iam")
    }
}

/// An occurrence of a document in the assembly.
#[derive(Clone, Debug, PartialEq)]
pub struct Occurrence {
    /// Its record in the definitions segment.
    pub record: usize,
    /// The key the reference segment and the occurrence table name it by;
    /// None when it has no proxy record (suppressed occurrences).
    pub key: Option<u32>,
    /// The browser's name when one is stored (empty names are left out:
    /// the default name is the document's and the instance number).
    pub label: Option<String>,
    pub flags: u32,
    /// The referenced document ([`Reference::id`]), from the occurrence
    /// table.
    pub reference: Option<u32>,
    /// The number in its default name (`<document>:<instance>`).
    pub instance: Option<u32>,
    /// Its placement in the assembly: maps the document's coordinates to
    /// the assembly's (cm), row-major.
    pub transform: Option<[[f64; 4]; 4]>,
    /// The id of the referenced document's version (the descriptor's first
    /// GUID); a part file lists it in its own `UFRxDoc` stream.
    pub document: Option<[u8; 16]>,
    /// The referenced document's range box in its own coordinates (cm):
    /// min and max.
    pub range: Option<[[f64; 3]; 2]>,
    /// The centre of the range box placed in the assembly (cm), stored
    /// with the placement.
    pub center: Option<[f64; 3]>,
    /// The number of shown body entries the range box is made of (a group
    /// of surfaces counts once).
    pub range_bodies: usize,
    /// The referenced document's model state (`Default`).
    pub model_state: Option<String>,
    /// The transform the graphics segment displays it with, when the file
    /// keeps one (cm, row-major).
    pub displayed: Option<[[f64; 4]; 4]>,
    /// The occurrence table's entry names the referenced document's id
    /// for parts ([`Occurrence::document`] found in the entry).
    pub identity_checked: bool,
}

impl Occurrence {
    pub fn hidden(&self) -> bool {
        self.flags & FLAG_HIDDEN != 0
    }

    pub fn grounded(&self) -> bool {
        self.flags & FLAG_GROUNDED != 0
    }

    /// Suppressed: flagged, or without the key that every loaded
    /// occurrence has.
    pub fn suppressed(&self) -> bool {
        self.flags & FLAG_SUPPRESSED != 0 || self.key.is_none()
    }

    /// The name shown for it: the stored label, else `<document>:<n>`.
    pub fn name(&self, reference: Option<&Reference>) -> String {
        if let Some(label) = &self.label {
            return label.clone();
        }
        match (reference, self.instance) {
            (Some(r), Some(n)) => format!("{}:{n}", r.stem()),
            (Some(r), None) => r.stem().to_owned(),
            (None, _) => format!("Occurrence {}", self.record),
        }
    }
}

/// An assembly's references and occurrences.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembly {
    /// The assembly's own path as saved, from the `UFRxDoc` stream.
    pub saved_path: Option<String>,
    pub references: Vec<Reference>,
    /// In the definitions segment's order.
    pub occurrences: Vec<Occurrence>,
    /// What could not be read, with why.
    pub warnings: Vec<String>,
}

impl Assembly {
    pub fn reference(&self, id: u32) -> Option<&Reference> {
        self.references.iter().find(|r| r.id == id)
    }
}

impl IptFile {
    /// The root storage's class id is an assembly's.
    pub fn is_assembly(&self) -> bool {
        crate::cfb::guid_text(&self.container.root().clsid) == ASSEMBLY_CLSID
    }

    /// A segment's records as definitions (types, bytes, labels); None
    /// when the file has no segment of that name.
    pub fn segment_records(&self, name: &str) -> Result<Option<Definitions>, IptError> {
        let Some(segment) = self.segments.iter().find(|s| s.name == name) else {
            return Ok(None);
        };
        let content = self.segment_data(segment)?;
        let records = content
            .records
            .map_err(|e| IptError::Segment(format!("{name}: {e}")))?;
        Ok(Some(Definitions::new(
            segment.meta.major(),
            content.data,
            &content.tables,
            &records,
        )))
    }

    /// Whether the `UFRxDoc` stream holds this 16-byte id: a part lists
    /// its own version id there, which an assembly's occurrences name
    /// ([`Occurrence::document`]).
    pub fn lists_document(&self, id: &[u8; 16]) -> bool {
        self.container
            .read_path(REFERENCES_STREAM)
            .is_ok_and(|data| data.windows(16).any(|w| w == id))
    }
}

/// Reads an assembly's references and occurrences.
pub fn read(file: &IptFile) -> Result<Assembly, IptError> {
    let mut out = Assembly::default();
    let dc = file
        .segment_records(DEFINITIONS_SEGMENT)?
        .ok_or_else(|| IptError::NotPart(format!("no {DEFINITIONS_SEGMENT}")))?;
    let rx = file.segment_records(REFERENCES_SEGMENT)?;
    let labels = dc.labels();

    // The occurrences and their keys.
    for i in dc.of_type(&OCCURRENCE) {
        let flags = dc.header(i).map_or(0, |h| h.flags);
        let key = occurrence_key(&dc, i);
        let label = labels
            .get(&i)
            .map(|l| l.name.trim().to_owned())
            .filter(|n| !n.is_empty());
        out.occurrences.push(Occurrence {
            record: i,
            key,
            label,
            flags,
            reference: None,
            instance: None,
            transform: None,
            document: None,
            range: None,
            center: None,
            range_bodies: 0,
            model_state: None,
            displayed: None,
            identity_checked: false,
        });
    }

    // The display transforms by key.
    let mut displayed: HashMap<u32, [[f64; 4]; 4]> = HashMap::new();
    if let Some(graphics) = file.segment_records(GRAPHICS_SEGMENT).ok().flatten() {
        for i in graphics.of_type(&DISPLAY) {
            if let Some((key, m)) = display(graphics.bytes(i)) {
                displayed.insert(key, m);
            }
        }
    }

    // Descriptors and placements by key.
    let mut descriptors: HashMap<u32, Descriptor> = HashMap::new();
    let mut by_record: HashMap<usize, u32> = HashMap::new();
    let mut placements: HashMap<u32, (Matrix, Option<[f64; 3]>)> = HashMap::new();
    if let Some(rx) = &rx {
        for i in rx.of_type(&DESCRIPTOR) {
            match descriptor(rx.bytes(i)) {
                Ok(d) => {
                    by_record.insert(i, d.key);
                    descriptors.insert(d.key, d);
                }
                Err(e) => out.warnings.push(format!(
                    "{REFERENCES_SEGMENT}#{i}: document descriptor: {e}"
                )),
            }
        }
        for i in rx.of_type(&PLACEMENT) {
            match placement(rx.bytes(i)) {
                Ok((d, m, center)) => match d.and_then(|d| by_record.get(&d)) {
                    Some(key) => {
                        placements.insert(*key, (m, center));
                    }
                    None => out.warnings.push(format!(
                        "{REFERENCES_SEGMENT}#{i}: a placement without its document descriptor"
                    )),
                },
                Err(e) => out
                    .warnings
                    .push(format!("{REFERENCES_SEGMENT}#{i}: placement: {e}")),
            }
        }
    }

    // The referenced files and the occurrence table.
    let stream = file.container.read_path(REFERENCES_STREAM).ok();
    let mut table: HashMap<u32, Entry> = HashMap::new();
    if let Some(data) = &stream {
        match references(data) {
            Ok((saved, list, end)) => {
                out.saved_path = saved;
                out.references = list;
                let documents: HashMap<u32, [u8; 16]> =
                    descriptors.iter().map(|(k, d)| (*k, d.document)).collect();
                table = occurrence_table(data, end, &out.references, &documents);
            }
            Err(e) => out.warnings.push(format!("UFRxDoc: {e}")),
        }
    } else {
        out.warnings.push("no UFRxDoc stream".to_owned());
    }

    for o in &mut out.occurrences {
        let Some(key) = o.key else {
            continue;
        };
        if let Some(d) = descriptors.get(&key) {
            o.document = Some(d.document);
            o.range = d.range;
            o.range_bodies = d.bodies;
            o.model_state = d.model_state.clone();
        }
        if let Some((m, center)) = placements.get(&key) {
            o.transform = Some(*m);
            o.center = *center;
        }
        o.displayed = displayed.get(&key).copied();
        if let Some(e) = table.get(&key) {
            o.reference = Some(e.reference);
            o.instance = Some(e.instance);
            o.identity_checked = e.identity_checked;
        }
    }
    Ok(out)
}

/// The key an occurrence record names: its last u32 refers to a record
/// that ends with the key, the u32 3 and the text `DCx` (and two bytes).
fn occurrence_key(dc: &Definitions, record: usize) -> Option<u32> {
    let bytes = dc.bytes(record);
    let proxy = dc::reference(u32::from_le_bytes(
        bytes.get(bytes.len().checked_sub(4)?..)?.try_into().ok()?,
    ))?;
    if !dc.is(proxy, &OCCURRENCE_KEY) {
        return None;
    }
    tagged_key(dc.bytes(proxy), "DCx")
}

/// A 4x4 matrix, row-major.
type Matrix = [[f64; 4]; 4];

/// A document descriptor (reference segment).
#[derive(Clone, Debug)]
struct Descriptor {
    key: u32,
    document: [u8; 16],
    range: Option<[[f64; 3]; 2]>,
    bodies: usize,
    model_state: Option<String>,
}

/// u32, u16, u16, the key (u32), the text `AmRx`, u16, a reference, three
/// 16-byte ids (the first: the document's version), then a list of the
/// document's bodies: `06 00 00 30`, the count (u32) and u32 0, and per
/// body a head ending with `00 00 00 10`, an id (u32) and u32 1, its range
/// box (six f64: min and max, cm) and its colour as a text (`R,G,B`, or
/// empty); the model state's name (a text) ends the record. The range is
/// the bodies' boxes together.
fn descriptor(bytes: &[u8]) -> Result<Descriptor, String> {
    let e = |e: dc::DcError| e.to_string();
    let mut r = Reader::new(bytes);
    r.skip(8).map_err(e)?;
    let key = r.u32().map_err(e)?;
    let tag = r.text().map_err(e)?;
    if tag != "AmRx" {
        return Err(format!("no AmRx tag ({tag:?})"));
    }
    r.skip(6).map_err(e)?;
    let mut document = [0u8; 16];
    document.copy_from_slice(r.take(16).map_err(e)?);
    r.skip(32).map_err(e)?;
    let mut range: Option<[[f64; 3]; 2]> = None;
    let mut bodies = 0;
    if r.u32().map_err(e)? == 0x3000_0006 {
        let count = r.u32().map_err(e)? as usize;
        r.skip(4).map_err(e)?;
        for _ in 0..count.min(100_000) {
            // The body's head ends with 00 00 00 10, its id (the node of
            // the part's body) and its flags (as the part's result list,
            // `result`: 0x01 a solid, 0x22 surfaces, 0x10 a mesh, 0x08
            // hidden).
            let rest = &bytes[r.pos..];
            let Some(at) = rest.windows(12).take(64).position(|w| {
                w[..4] == [0, 0, 0, 0x10] && u32::from_le_bytes([w[8], w[9], w[10], w[11]]) < 0x100
            }) else {
                break;
            };
            let flags = u32_at(rest, at + 8).unwrap_or(0);
            r.skip(at + 12).map_err(e)?;
            let mut v = [0.0; 6];
            for x in &mut v {
                *x = r.f64().map_err(e)?;
            }
            r.text().map_err(e)?;
            let ok = v.iter().all(|x| x.is_finite() && x.abs() < 1e9)
                && (0..3).all(|k| v[k] <= v[k + 3]);
            // The range box is the shown bodies'.
            if !ok || flags & BODY_HIDDEN != 0 {
                continue;
            }
            bodies += 1;
            range = Some(match range {
                None => [[v[0], v[1], v[2]], [v[3], v[4], v[5]]],
                Some([min, max]) => [
                    std::array::from_fn(|k| min[k].min(v[k])),
                    std::array::from_fn(|k| max[k].max(v[k + 3])),
                ],
            });
        }
    }
    // The model state's name ends the record.
    let mut model_state = None;
    for n in 0..=256usize {
        let Some(at) = bytes.len().checked_sub(4 + 2 * n) else {
            break;
        };
        if at < r.pos || u32_at(bytes, at) != Some(n as u32) {
            continue;
        }
        let name = Reader::at(bytes, at).text().map_err(e)?;
        if !name.chars().any(|c| c < ' ') {
            model_state = Some(name);
            break;
        }
    }
    Ok(Descriptor {
        key,
        document,
        range,
        bodies,
        model_state,
    })
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A matrix stored with masks: an optional u32 `0x203`, a u32 whose low
/// half marks the elements that are Â±1 and whose high half those that are
/// 0 or âˆ’1 (both: âˆ’1), then the others as f64, row by row.
pub fn masked_matrix(r: &mut Reader) -> dc::Result<[[f64; 4]; 4]> {
    let mut first = r.u32()?;
    if first == 0x203 {
        first = r.u32()?;
    }
    let (ones, zeros) = (first & 0xFFFF, first >> 16);
    let mut m = [[0.0; 4]; 4];
    for bit in 0..16 {
        let (one, zero) = ((ones >> bit) & 1 != 0, (zeros >> bit) & 1 != 0);
        m[bit / 4][bit % 4] = match (zero, one) {
            (false, false) => r.f64()?,
            (false, true) => 1.0,
            (true, false) => 0.0,
            (true, true) => -1.0,
        };
    }
    Ok(m)
}

/// The key named by a record that ends with the key, the u32 3, a
/// three-letter tag (`DCx`, `GRx`) and two bytes.
fn tagged_key(data: &[u8], tag: &str) -> Option<u32> {
    let pattern: Vec<u8> = [3u8, 0, 0, 0]
        .into_iter()
        .chain(tag.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let at = data.windows(pattern.len()).rposition(|w| w == pattern)?;
    u32_at(data, at.checked_sub(4)?)
}

/// A display transform (graphics segment): a 15-byte head, the masked
/// matrix, then the occurrence's key tagged `GRx`. Only rigid transforms
/// are taken.
fn display(bytes: &[u8]) -> Option<(u32, [[f64; 4]; 4])> {
    let key = tagged_key(bytes, "GRx")?;
    let mut r = Reader::at(bytes, 15);
    let m = masked_matrix(&mut r).ok()?;
    let rigid = m[3] == [0.0, 0.0, 0.0, 1.0]
        && (0..3).all(|a| {
            (0..3).all(|b| {
                let d: f64 = (0..3).map(|k| m[a][k] * m[b][k]).sum();
                (d - if a == b { 1.0 } else { 0.0 }).abs() < 1e-6
            })
        });
    rigid.then_some((key, m))
}

/// A placement: u32, u16, the descriptor (a reference), the masked
/// matrix, then nine f64: three not read, the placed x axis and the
/// placed centre of the range box.
type Placed = (Option<usize>, [[f64; 4]; 4], Option<[f64; 3]>);

fn placement(bytes: &[u8]) -> Result<Placed, String> {
    let e = |e: dc::DcError| e.to_string();
    let mut r = Reader::new(bytes);
    r.skip(6).map_err(e)?;
    let descriptor = r.reference().map_err(e)?;
    let m = masked_matrix(&mut r).map_err(e)?;
    if m.iter().flatten().any(|v| !v.is_finite()) || m[3] != [0.0, 0.0, 0.0, 1.0] {
        return Err("not an affine transform".to_owned());
    }
    let center = (|| -> dc::Result<[f64; 3]> {
        r.skip(6 * 8)?;
        Ok([r.f64()?, r.f64()?, r.f64()?])
    })()
    .ok()
    .filter(|c| c.iter().all(|v| v.is_finite()));
    Ok((descriptor, m, center))
}

/// The `UFRxDoc` stream's file list. The stream starts with the document's
/// own path as saved (the first path in it), model states and design
/// views; the list follows with an entry per file: the path (text), u32
/// flags, the library's name (text), u16, the member's name (text), eight
/// bytes, two 16-byte ids, the file's id (u32), two u32 and u32 flags;
/// flag 0x20: a text (a variant's name) and a u32 follow. An empty text
/// ends the list. The first entry is the first path after the document's
/// own. Returns the own path, the files and where the list ends.
fn references(data: &[u8]) -> Result<(Option<String>, Vec<Reference>, usize), String> {
    let e = |e: dc::DcError| e.to_string();
    // The document's own path, then the first file's.
    let Some((_, own, saved)) = next_path(data, 0) else {
        return Err("no path".to_owned());
    };
    let Some((at, _, _)) = next_path(data, own) else {
        // No files.
        return Ok((Some(saved), Vec::new(), data.len()));
    };
    let saved = Some(saved);
    let mut r = Reader::at(data, at);
    let mut list = Vec::new();
    let end = loop {
        let start = r.pos;
        let path = r.text().map_err(e)?;
        if path.is_empty() {
            break start;
        }
        if path.chars().any(|c| c < ' ') {
            return Err(format!("a file list entry at {start} is no path"));
        }
        r.u32().map_err(e)?;
        let library = r.text().map_err(e)?;
        r.u16().map_err(e)?;
        let member = r.text().map_err(e)?;
        r.skip(8 + 32).map_err(e)?;
        let id = r.u32().map_err(e)?;
        r.skip(8).map_err(e)?;
        let flags = r.u32().map_err(e)?;
        let variant = if flags & 0x20 != 0 {
            let v = r.text().map_err(e)?;
            r.u32().map_err(e)?;
            Some(v)
        } else {
            None
        };
        list.push(Reference {
            id,
            path,
            library,
            member,
            variant,
        });
    };
    Ok((saved, list, end))
}

/// A class id (as the container stores it) in the `UFRxDoc` stream's
/// form: its text's 32 hex digits as four little-endian u32.
pub fn stream_guid(clsid: &[u8; 16]) -> [u8; 16] {
    let text = crate::cfb::guid_text(clsid).replace('-', "");
    let mut out = [0u8; 16];
    for k in 0..4 {
        let word = u32::from_str_radix(&text[8 * k..8 * k + 8], 16).unwrap_or(0);
        out[4 * k..4 * k + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

/// The first path-like text from `from` on: where it starts and ends, and
/// the text.
fn next_path(data: &[u8], from: usize) -> Option<(usize, usize, String)> {
    (from..data.len().saturating_sub(4)).find_map(|at| {
        let n = u32_at(data, at)? as usize;
        if !(3..1024).contains(&n) {
            return None;
        }
        let mut r = Reader::at(data, at);
        let text = r.text().ok()?;
        let lower = text.to_ascii_lowercase();
        let path = [".iam", ".ipt", ".ipn", ".ide"]
            .iter()
            .any(|x| lower.ends_with(x))
            && !text.chars().any(|c| c < ' ');
        path.then_some((at, r.pos, text))
    })
}

/// An entry of the occurrence table.
#[derive(Clone, Copy, Debug)]
struct Entry {
    reference: u32,
    instance: u32,
    identity_checked: bool,
}

/// The occurrence table after the file list: six bytes, the count (u32),
/// u32, then an entry per occurrence: the file's id, the key, a sequence
/// number and the instance number (four u32), then properties of
/// variable length (for parts they hold the descriptor's document id).
/// The entries are found by their heads: a file of the list, a key not
/// seen yet and plausible numbers; a part's head is taken only when the
/// properties up to the next head hold its document id.
fn occurrence_table(
    data: &[u8],
    end: usize,
    references: &[Reference],
    documents: &HashMap<u32, [u8; 16]>,
) -> HashMap<u32, Entry> {
    let ids: HashMap<u32, &Reference> = references.iter().map(|r| (r.id, r)).collect();
    let n = documents.len() as u32;
    let bound = 16 * n + 64;
    let mut left: HashSet<u32> = documents.keys().copied().collect();
    let mut heads: Vec<(usize, u32, u32, u32)> = Vec::new();
    let mut at = end + 14;
    while at + 16 <= data.len() && !left.is_empty() {
        let v = |k: usize| u32_at(data, at + 4 * k).unwrap_or(0);
        let (file, key, sequence, instance) = (v(0), v(1), v(2), v(3));
        if ids.contains_key(&file)
            && left.contains(&key)
            && (1..=bound).contains(&sequence)
            && (1..=bound).contains(&instance)
        {
            heads.push((at, file, key, instance));
            left.remove(&key);
            at += 16;
        } else {
            at += 1;
        }
    }
    let mut out = HashMap::new();
    for (k, &(at, file, key, instance)) in heads.iter().enumerate() {
        let stop = heads.get(k + 1).map_or(data.len(), |h| h.0);
        let found = documents
            .get(&key)
            .is_some_and(|d| data[at..stop].windows(16).any(|w| w == d));
        let part = ids.get(&file).is_some_and(|r| !r.is_assembly());
        out.insert(
            key,
            Entry {
                reference: file,
                instance,
                identity_checked: part && found,
            },
        );
    }
    out
}

/// A summary for reports: occurrences by state, references by kind.
pub fn summary(a: &Assembly) -> BTreeMap<&'static str, usize> {
    let mut s = BTreeMap::new();
    s.insert("references", a.references.len());
    s.insert("occurrences", a.occurrences.len());
    s.insert(
        "suppressed",
        a.occurrences.iter().filter(|o| o.suppressed()).count(),
    );
    s.insert(
        "hidden",
        a.occurrences.iter().filter(|o| o.hidden()).count(),
    );
    s.insert(
        "without_file",
        a.occurrences
            .iter()
            .filter(|o| !o.suppressed() && o.reference.is_none())
            .count(),
    );
    s.insert(
        "without_placement",
        a.occurrences
            .iter()
            .filter(|o| !o.suppressed() && o.transform.is_none())
            .count(),
    );
    s.insert(
        "displayed",
        a.occurrences
            .iter()
            .filter(|o| o.displayed.is_some())
            .count(),
    );
    s.insert(
        "displayed_elsewhere",
        a.occurrences
            .iter()
            .filter(|o| match (o.transform, o.displayed) {
                (Some(m), Some(d)) => !same_transform(&m, &d),
                _ => false,
            })
            .count(),
    );
    s
}

/// Whether two placements (cm) are the same within rounding: 1e-6 in the
/// rotation, 1e-5 cm in the translation.
pub fn same_transform(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> bool {
    (0..3).all(|r| {
        (0..3).all(|c| (a[r][c] - b[r][c]).abs() <= 1e-6) && (a[r][3] - b[r][3]).abs() <= 1e-5
    })
}

/// The class id bytes of an assembly's root storage.
pub fn assembly_clsid() -> [u8; 16] {
    guid_bytes(ASSEMBLY_CLSID).expect("class id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testassembly::{self, CUBE_ID, PAIR_ID};

    fn read_test(a: &testassembly::TestAssembly) -> (IptFile, Assembly) {
        let f = IptFile::parse(testassembly::assembly_file(a)).unwrap();
        let read = read(&f).unwrap();
        (f, read)
    }

    #[test]
    fn reads_the_test_assembly() {
        let (f, a) = read_test(&testassembly::top_assembly());
        assert!(f.is_assembly());
        assert!(!f.is_part());
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        assert_eq!(
            a.saved_path.as_deref(),
            Some(r"C:\Elsewhere\Project\top.iam")
        );
        let files: Vec<(u32, &str)> = a.references.iter().map(|r| (r.id, r.file_name())).collect();
        assert_eq!(
            files,
            [
                (2, "cube.ipt"),
                (3, "sub.iam"),
                (5, "missing.ipt"),
                (6, "found.ipt")
            ]
        );
        assert!(a.references[1].is_assembly());
        assert_eq!(a.references[0].stem(), "cube");
        assert_eq!(a.occurrences.len(), 6);
        let names: Vec<String> = a
            .occurrences
            .iter()
            .map(|o| o.name(o.reference.and_then(|id| a.reference(id))))
            .collect();
        assert_eq!(names[..4], ["cube:1", "cube:2", "sub:1", "missing:1"]);
        assert_eq!(names[5], "Found by name");
        let o = &a.occurrences;
        assert_eq!(o[0].key, Some(2));
        assert_eq!(o[0].reference, Some(2));
        assert_eq!(o[0].document, Some(CUBE_ID));
        assert!(o[0].identity_checked);
        // A rotation of 90° about z and 5 cm along x.
        let m = o[0].transform.unwrap();
        assert!((m[0][1] + 1.0).abs() < 1e-12 && (m[1][0] - 1.0).abs() < 1e-12);
        assert_eq!(m[0][3], 5.0);
        assert_eq!(o[0].range, Some([[0.0; 3], [1.0; 3]]));
        // The solid and the group of surfaces; the hidden surface outside
        // the range box is left out of it.
        assert_eq!(o[0].range_bodies, 2);
        assert_eq!(o[0].model_state.as_deref(), Some("Default"));
        assert_eq!(o[0].displayed, o[0].transform);
        assert_eq!(o[4].displayed, None);
        let c = o[0].center.unwrap();
        assert!((c[0] - 4.5).abs() < 1e-12 && (c[1] - 0.5).abs() < 1e-12);
        assert!(o[1].hidden() && !o[1].grounded() && !o[1].suppressed());
        assert!(o[2].grounded());
        assert!(!o[2].identity_checked, "sub-assemblies are not checked");
        assert!(o[4].suppressed());
        assert_eq!(o[4].key, None);
        assert_eq!(o[4].transform, None);
        assert_eq!(o[5].document, Some(PAIR_ID));
        let s = summary(&a);
        assert_eq!(s["suppressed"], 1);
        assert_eq!(s["hidden"], 1);
        assert_eq!(s["without_file"], 0);
        assert_eq!(s["without_placement"], 0);
        assert_eq!(s["displayed"], 5);
        assert_eq!(s["displayed_elsewhere"], 0);
    }

    #[test]
    fn a_part_lists_its_document_id() {
        let part = testassembly::with_document_id(&crate::testdata::test_part(), PAIR_ID);
        let f = IptFile::parse(part).unwrap();
        assert!(f.lists_document(&PAIR_ID));
        assert!(!f.lists_document(&CUBE_ID));
        assert!(f.brep_records().is_ok());
    }

    #[test]
    fn an_assembly_without_references() {
        let mut a = testassembly::sub_assembly();
        a.references.clear();
        for o in &mut a.occurrences {
            o.reference = 9;
        }
        let (_, read) = read_test(&a);
        assert!(read.references.is_empty());
        assert!(read.occurrences.iter().all(|o| o.reference.is_none()));
        assert_eq!(summary(&read)["without_file"], 2);
    }

    #[test]
    fn stream_guids_are_four_words() {
        let clsid = assembly_clsid();
        assert_eq!(
            stream_guid(&clsid),
            [
                0xe1, 0x81, 0x0f, 0xe6, 0xd0, 0x11, 0xb3, 0x49, 0x07, 0x7e, 0xc3, 0x93, 0, 0, 0, 6
            ]
        );
    }
}
