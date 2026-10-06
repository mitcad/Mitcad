// SPDX-License-Identifier: MIT
//! Decoders for the parts of the design segment that the timeline format
//! study marks certain or probable: parameters (section 7), the timeline
//! and the common item tail (8, 9.1), extrude fields (9.2), components and
//! body blob names (12), and the byte coverage table (14). Same choices as
//! the reference decoder.

use std::collections::HashMap;

use super::classes::{self, *};
use super::stream::{
    Object, Segment, f64_at, i32_at, slice, str16_at, text_at, u32_at, u64_at, utf16_units,
};
use super::unicode;

/// The common tail of a timeline item (section 9.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemTail {
    /// Default name base (`Extrude`, `Sketch`); empty for instance items.
    pub base_name: String,
    /// Default name number: display name = `base_name` + `index`.
    pub index: u32,
    /// The `u32` after the index (zero so far).
    pub after_index: u32,
    /// Name given by the user, empty if none.
    pub custom_name: String,
    /// `-1` for items without a result, else a growing number.
    pub result_no: i32,
    /// Sub-items (`None` for a null reference).
    pub sub_features: Vec<Option<u64>>,
    /// The three state bytes before the health reference.
    pub flags: [u8; 3],
    /// Object-relative byte range `[start, end)` of the decoded fields.
    pub start: usize,
    pub end: usize,
}

impl ItemTail {
    /// The display name: the custom name, else base name + index.
    pub fn display_name(&self) -> String {
        if !self.custom_name.is_empty() {
            self.custom_name.clone()
        } else if !self.base_name.is_empty() {
            format!("{}{}", self.base_name, self.index)
        } else {
            String::new()
        }
    }
}

/// Raw extrude fields (section 9.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtrudeFields {
    /// 1 Join, 2 Cut, 4 NewBody (probable).
    pub operation_code: u32,
    /// 1 OneSide, 2 TwoSides (probable), 3 open.
    pub extent_a: u32,
    pub extent_b: u32,
    /// The first ±1.0 after the extent fields.
    pub direction: Option<f64>,
}

impl ExtrudeFields {
    /// `Join`, `Cut`, `NewBody` for the known codes.
    pub fn operation(&self) -> Option<&'static str> {
        match self.operation_code {
            1 => Some("Join"),
            2 => Some("Cut"),
            4 => Some("NewBody"),
            _ => None,
        }
    }
}

/// A timeline item.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineEntry {
    /// Position in the timeline.
    pub pos: usize,
    pub id: u64,
    /// Upper-case class GUID.
    pub class: String,
    pub class_version: u32,
    /// The decoder's type name (`ExtrudeFeature`, ...), if known.
    pub type_name: Option<&'static str>,
    /// Display name; `None` if the tail was not found.
    pub name: Option<String>,
    pub tail: Option<ItemTail>,
    pub extrude: Option<ExtrudeFields>,
}

/// A parameter (section 7).
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterEntry {
    pub id: u64,
    /// Creation number (`dN` names use it).
    pub number: u32,
    /// Parameter holder (`D91D429C`); `None` for user parameters.
    pub holder: Option<u64>,
    pub expression: String,
    pub role: String,
    /// Class versions 7 and later.
    pub comment: Option<String>,
    pub unit: String,
    pub name: String,
    /// Internal units (cm, rad).
    pub value: f64,
    /// Component parameter list (`B0556F58`).
    pub list: Option<u64>,
    /// The feature that owns the parameter (holder's root list).
    pub owner_feature: Option<u64>,
    pub class_version: u32,
    /// Bytes before `number` (version-dependent gap, section 7).
    pub gap0: Vec<u8>,
    /// Flag bytes after the expression.
    pub gap1: Vec<u8>,
    /// Object-relative end of the decoded fields.
    pub end: usize,
}

impl ParameterEntry {
    pub fn is_user(&self) -> bool {
        self.holder.is_none()
    }
}

/// A component and its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentEntry {
    pub id: u64,
    pub name: Option<String>,
}

/// A body blob named by a `40DCC3F4` object, with the component whose
/// bodies it holds (through the blob holder `CD57BC48`, section 12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrepBlobEntry {
    /// The `40DCC3F4` object.
    pub id: u64,
    /// `BREP.<guid>.smb`.
    pub file: String,
    /// The holder object (`CD57BC48`) referencing it.
    pub holder: Option<u64>,
    /// The component the holder references (last one).
    pub component: Option<u64>,
}

/// Byte coverage of the BulkStream (section 14).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoverageCounts {
    pub bulk_bytes: u64,
    pub framing_bytes: u64,
    pub root_part_ok: u64,
    pub objects: u64,
    pub known_class_bytes: u64,
    pub decoded_bytes: u64,
    pub token_bytes: u64,
}

/// Everything decoded from a design segment's object streams, before the
/// SCHEMA.md shaping ([`super::build`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    /// The design timeline in order (the only non-empty `2F4C1849` list).
    pub timeline: Vec<TimelineEntry>,
    /// `2F4C1849` objects with a feature manager (one per component).
    pub timelines_total: usize,
    /// Parameters sorted by creation number.
    pub parameters: Vec<ParameterEntry>,
    pub parameter_failures: usize,
    pub components: Vec<ComponentEntry>,
    /// Objects naming a body blob (`BREP.<guid>.smb`).
    pub brep_blobs: Vec<BrepBlobEntry>,
    pub coverage: CoverageCounts,
}

/// Decodes the parameters, timeline, components and blob names.
pub fn decode(seg: &Segment) -> Decoded {
    let mut decoded_bytes = 0u64;

    // Timelines: root part (2 bytes), feature manager, u32 n, n items.
    let mut timelines: Vec<Vec<u64>> = Vec::new();
    for t in seg.objects_of(TIMELINE) {
        let d = seg.data(t);
        let Some(p) = super::stream::header_end(d) else {
            continue;
        };
        let Some(r) = seg.ref_at(d, p + 2) else {
            continue;
        };
        let Some(n) = u32_at(d, r.end) else { continue };
        let mut q = r.end + 4;
        let mut items = Vec::new();
        for _ in 0..n {
            let Some(rr) = seg.ref_at(d, q) else { break };
            items.push(rr.id);
            q = rr.end;
        }
        if q == d.len() {
            decoded_bytes += d.len() as u64;
        }
        timelines.push(items);
    }
    let timelines_total = timelines.len();
    let main = timelines
        .into_iter()
        .find(|t| !t.is_empty())
        .unwrap_or_default();
    let mut timeline = Vec::new();
    for (pos, &fid) in main.iter().enumerate() {
        let Some(o) = seg.object(fid) else { continue };
        let class = seg.guid(o).unwrap_or_default().to_string();
        let tail = feature_tail(seg, o);
        if let Some(t) = &tail {
            decoded_bytes += (t.end - t.start) as u64;
        }
        let extrude = if class == EXTRUDE {
            extrude_fields(seg, o)
        } else {
            None
        };
        timeline.push(TimelineEntry {
            pos,
            id: fid,
            type_name: classes::known_name(&class),
            class_version: seg.version(o).unwrap_or(0),
            class,
            name: tail.as_ref().map(ItemTail::display_name),
            tail,
            extrude,
        });
    }

    // Parameters; the holder's root reference list names the owning feature.
    let mut holder_owner = HashMap::new();
    for h in seg.objects_of(PARAMETER_HOLDER) {
        let d = seg.data(h);
        if let Some(p) = super::stream::header_end(d)
            && d.get(p) == Some(&1)
            && let Some(r) = seg.ref_at(d, p + 5)
        {
            holder_owner.insert(h.id, r.id);
        }
    }
    let mut parameters = Vec::new();
    let mut parameter_failures = 0;
    for o in seg.objects_of(PARAMETER) {
        match parse_parameter(seg, o) {
            Some(mut prm) => {
                decoded_bytes += prm.end as u64;
                prm.owner_feature = prm.holder.and_then(|h| holder_owner.get(&h).copied());
                parameters.push(prm);
            }
            None => parameter_failures += 1,
        }
    }
    parameters.sort_by_key(|p| p.number);

    let components = seg
        .objects_of(COMPONENT)
        .map(|c| ComponentEntry {
            id: c.id,
            name: component_name(seg.data(c)),
        })
        .collect();
    let mut holder_of: HashMap<u64, (u64, Option<u64>)> = HashMap::new();
    for h in seg.objects_of(BLOB_HOLDER) {
        let refs = seg.ref_ids(seg.data(h));
        let comp = refs
            .iter()
            .rev()
            .find(|&&r| seg.guid_of(r) == Some(COMPONENT))
            .copied();
        for &r in &refs {
            if seg.guid_of(r) == Some(BREP_REF) {
                holder_of.entry(r).or_insert((h.id, comp));
            }
        }
    }
    let brep_blobs = seg
        .objects_of(BREP_REF)
        .filter_map(|b| {
            let file = brep_name(seg.data(b))?;
            let holder = holder_of.get(&b.id);
            Some(BrepBlobEntry {
                id: b.id,
                file,
                holder: holder.map(|h| h.0),
                component: holder.and_then(|h| h.1),
            })
        })
        .collect();

    let mut coverage = coverage(seg);
    coverage.decoded_bytes = decoded_bytes;
    Decoded {
        timeline,
        timelines_total,
        parameters,
        parameter_failures,
        components,
        brep_blobs,
        coverage,
    }
}

/// The common item tail (section 9.1), anchored at the first reference to
/// a health object (`B7F34D7B`) in the main part and found by scanning
/// back: `i32 result_no | str16 base_name | u32 index | u32 0 |
/// str16 custom_name | u32 n | n refs | 3 flag bytes | ref health`.
pub fn feature_tail(seg: &Segment, o: &Object) -> Option<ItemTail> {
    let d = seg.data(o);
    let end = seg.main_end(o);
    let (p, _) = seg
        .refs_in(d, 0, end)
        .into_iter()
        .find(|(_, r)| seg.guid_of(r.id) == Some(HEALTH))?;
    let lo = (p as isize - 3000).max(4);
    let mut b = p as isize - 22;
    while b > lo {
        if let Some(t) = tail_at(seg, d, b as usize, p) {
            return Some(t);
        }
        b -= 1;
    }
    None
}

fn tail_at(seg: &Segment, d: &[u8], b: usize, p: usize) -> Option<ItemTail> {
    let (base, q) = str16_at(d, b)?;
    if !unicode::is_printable(&base) || q + 8 > p {
        return None;
    }
    let index = u32_at(d, q)?;
    let after_index = u32_at(d, q + 4)?;
    let (custom, q) = str16_at(d, q + 8)?;
    if !unicode::is_printable(&custom) || q + 7 > p {
        return None;
    }
    let n = u32_at(d, q)?;
    if n > 1000 {
        return None;
    }
    let mut q = q + 4;
    let mut subs = Vec::new();
    for _ in 0..n {
        if d.get(q) == Some(&0) {
            subs.push(None);
            q += 1;
            continue;
        }
        let r = seg.ref_at(d, q)?;
        subs.push(Some(r.id));
        q = r.end;
    }
    if q + 3 != p {
        return None;
    }
    Some(ItemTail {
        base_name: base,
        index,
        after_index,
        custom_name: custom,
        result_no: i32_at(d, b - 4)?,
        sub_features: subs,
        flags: [d[q], d[q + 1], d[q + 2]],
        start: b - 4,
        end: p,
    })
}

/// Operation, extent fields and direction of an extrude (section 9.2).
pub fn extrude_fields(seg: &Segment, o: &Object) -> Option<ExtrudeFields> {
    let d = seg.data(o);
    let root = seg.root_part(d)?;
    let skip = match seg.version(o) {
        Some(5) => 1,
        Some(6) => 2,
        _ => 3,
    };
    let p = root.end + skip;
    let operation_code = u32_at(d, p)?;
    let extent_a = u32_at(d, p + 4)?;
    let extent_b = u32_at(d, p + 8)?;
    const PLUS: [u8; 8] = 1.0f64.to_le_bytes();
    const MINUS: [u8; 8] = (-1.0f64).to_le_bytes();
    let stop = (d.len() as isize - 8).min(p as isize + 160);
    let mut direction = None;
    let mut q = p as isize + 12;
    while q < stop {
        let w = &d[q as usize..q as usize + 8];
        if w == PLUS || w == MINUS {
            direction = f64_at(d, q as usize);
            break;
        }
        q += 1;
    }
    Some(ExtrudeFields {
        operation_code,
        extent_a,
        extent_b,
        direction,
    })
}

/// Skips the parameter's root part: `u8 | u8 has_attrs | [attributes]`.
fn skip_attrs(d: &[u8], p: usize) -> Option<usize> {
    let has = *d.get(p + 1)?;
    let mut p = p + 2;
    if has != 0 {
        let n = u32_at(d, p)?;
        p += 4;
        for _ in 0..n {
            p += 4 + u32_at(d, p)? as usize;
            let ln = u32_at(d, p)? as usize;
            let typ = slice(d, p + 4, p + 4 + ln);
            p += 4 + ln;
            if typ.ends_with(b"uint64") {
                p += 8;
            } else if typ.ends_with(b"IString") {
                p += 4 + 2 * u32_at(d, p)? as usize;
            } else if typ.ends_with(b"bool") {
                p += 1;
            } else {
                return None;
            }
        }
    }
    Some(p)
}

/// A parameter object `7A6A3D31` (section 7): after a version-dependent
/// gap, `u32 number | ref holder (or 00) | str16 expression | flags |
/// str16 role | [str16 comment] | str16 unit | str16 name | f64 value |
/// 00 | ref list`. The gap is found by trying offsets 0..24.
pub fn parse_parameter(seg: &Segment, o: &Object) -> Option<ParameterEntry> {
    let d = seg.data(o);
    let version = seg.version(o)?;
    let p = skip_attrs(d, super::stream::header_end(d)?)?;
    let (g1, nstr) = if version <= 6 { (5, 3) } else { (9, 4) };
    for gap in 0..24 {
        let q = p + gap;
        if q + 9 > d.len() {
            break;
        }
        let number = u32_at(d, q)?;
        let mut r = q + 4;
        let mut holder = None;
        if let Some(rr) = seg.ref_at(d, r) {
            holder = Some(rr.id);
            r = rr.end;
        } else if d[r] == 0 {
            r += 1;
        } else {
            continue;
        }
        let Some((expression, e)) = text_at(d, r) else {
            continue;
        };
        let mut strs = Vec::new();
        let mut q2 = e + g1;
        for _ in 0..nstr {
            match str16_at(d, q2) {
                Some((s, q3)) if unicode::is_printable(&s) => {
                    strs.push(s);
                    q2 = q3;
                }
                _ => break,
            }
        }
        if strs.len() != nstr || strs[0].is_empty() || strs[nstr - 1].is_empty() {
            continue;
        }
        if q2 + 8 > d.len() {
            continue;
        }
        let value = f64_at(d, q2)?;
        let mut end = q2 + 8;
        let mut list = None;
        if d.get(end) == Some(&0)
            && let Some(rr) = seg.ref_at(d, end + 1)
        {
            list = Some(rr.id);
            end = rr.end;
        }
        let mut s = strs.into_iter();
        let role = s.next()?;
        let comment = if nstr == 4 { s.next() } else { None };
        let unit = s.next()?;
        let name = s.next()?;
        return Some(ParameterEntry {
            id: o.id,
            number,
            holder,
            expression,
            role,
            comment,
            unit,
            name,
            value,
            list,
            owner_feature: None,
            class_version: version,
            gap0: d[p..q].to_vec(),
            gap1: slice(d, e, e + g1).to_vec(),
            end,
        });
    }
    None
}

/// A GUID (`8-...` with 27 more hex digits or dashes) or a persistent name
/// `<n>_<id>`; a trailing newline is allowed (Python's `$`).
fn guidish(s: &str) -> bool {
    let t = s.strip_suffix('\n').unwrap_or(s).as_bytes();
    let guid = t.len() == 36
        && t[..8].iter().all(u8::is_ascii_hexdigit)
        && t[8] == b'-'
        && t[9..].iter().all(|c| c.is_ascii_hexdigit() || *c == b'-');
    let name = match t.iter().position(|&c| c == b'_') {
        Some(i) => {
            i > 0
                && i + 1 < t.len()
                && t[..i].iter().all(u8::is_ascii_digit)
                && t[i + 1..].iter().all(u8::is_ascii_digit)
        }
        None => false,
    };
    guid || name
}

/// A component's name: its last printable `str16` that is not a GUID
/// (probable, section 12).
pub fn component_name(d: &[u8]) -> Option<String> {
    let mut name = None;
    let mut p = 0;
    while p + 4 < d.len() {
        match text_at(d, p) {
            Some((s, e)) => {
                if !guidish(&s) {
                    name = Some(s);
                }
                p = e;
            }
            None => p += 1,
        }
    }
    name
}

/// The `BREP.<guid>.smb` name in a `40DCC3F4` object.
pub fn brep_name(d: &[u8]) -> Option<String> {
    let mut p = 0;
    while p + 4 < d.len() {
        if let Some((s, _)) = str16_at(d, p)
            && s.starts_with("BREP.")
            && s.ends_with(".smb")
        {
            return Some(s);
        }
        p += 1;
    }
    None
}

/// Framing, known-class and token bytes (section 14).
pub fn coverage(seg: &Segment) -> CoverageCounts {
    let mut c = CoverageCounts {
        bulk_bytes: seg.bulk.len() as u64,
        objects: seg.objects.len() as u64,
        ..CoverageCounts::default()
    };
    for o in &seg.objects {
        let d = seg.data(o);
        if seg.guid(o).and_then(classes::known_name).is_some() {
            c.known_class_bytes += d.len() as u64;
        }
        let start = match seg.root_part(d) {
            Some(r) => {
                c.root_part_ok += 1;
                r.end
            }
            None => super::stream::header_end(d).unwrap_or(0),
        };
        c.framing_bytes += start as u64;
        c.token_bytes += (start + token_bytes(seg, d, start)) as u64;
    }
    c
}

/// Bytes after `start` covered by validated references and plausible
/// strings (`str8` of printable ASCII, `str16` of characters below
/// U+2000; a one-character `str16` must be alphanumeric).
fn token_bytes(seg: &Segment, d: &[u8], start: usize) -> usize {
    let mut n = 0;
    let mut p = start;
    let end = d.len();
    while p < end {
        if d[p] == 1
            && let Some(r) = seg.ref_at(d, p)
        {
            n += r.end - p;
            p = r.end;
            continue;
        }
        if p + 4 <= end {
            let ln = u32_at(d, p).unwrap_or(0) as usize;
            if (1..=2048).contains(&ln) {
                let b = slice(d, p + 4, p + 4 + ln);
                if b.len() == ln && b.iter().all(|c| (32..127).contains(c)) {
                    n += 4 + ln;
                    p += 4 + ln;
                    continue;
                }
                let b = slice(d, p + 4, p + 4 + 2 * ln);
                if b.len() == 2 * ln {
                    let units = utf16_units(b);
                    let plausible = units.iter().all(|&u| (32..0x2000).contains(&u))
                        && (ln > 1
                            || char::from_u32(units[0] as u32).is_some_and(unicode::is_alnum_char));
                    if plausible {
                        n += 4 + 2 * ln;
                        p += 4 + 2 * ln;
                        continue;
                    }
                }
            }
        }
        p += 1;
    }
    n
}

/// The u64 object id held by the target object (`90055C05`) of an entity
/// reference (`5A1BF548`): the plane, axis or point it names.
pub fn target_id(seg: &Segment, entity_ref: u64) -> Option<u64> {
    let d = seg.data_of(entity_ref);
    let r = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .find(|(_, r)| seg.guid_of(r.id) == Some(REF_TARGET))?
        .1;
    let t = seg.data_of(r.id);
    u64_at(t, super::stream::header_end(t)? + 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_and_persistent_names() {
        assert!(guidish("2F4C1849-1A5A-4F6C-A086-8DD445CBF94B"));
        assert!(guidish("0_201"));
        assert!(guidish("12_3\n"));
        assert!(!guidish("_3"));
        assert!(!guidish("Bracket"));
        assert!(!guidish("2F4C1849-1A5A-4F6C-A086-8DD445CBF94"));
    }

    #[test]
    fn component_name_is_last_non_guid_text() {
        let mut d = vec![0u8, 0, 0];
        for s in ["Bracket", "0_201", "2F4C1849-1A5A-4F6C-A086-8DD445CBF94B"] {
            d.extend((s.len() as u32).to_le_bytes());
            d.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
        }
        d.extend([9, 9, 9, 9, 9]);
        assert_eq!(component_name(&d).as_deref(), Some("Bracket"));
        assert_eq!(brep_name(&d), None);
    }

    #[test]
    fn display_names() {
        let mut t = ItemTail {
            base_name: "Extrude".into(),
            index: 3,
            after_index: 0,
            custom_name: String::new(),
            result_no: 1,
            sub_features: vec![],
            flags: [0; 3],
            start: 0,
            end: 0,
        };
        assert_eq!(t.display_name(), "Extrude3");
        t.custom_name = "Boss".into();
        assert_eq!(t.display_name(), "Boss");
        t.custom_name.clear();
        t.base_name.clear();
        assert_eq!(t.display_name(), "");
    }
}
