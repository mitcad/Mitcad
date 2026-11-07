// SPDX-License-Identifier: MIT
//! Decoders for the parts of the design segment that the timeline format
//! study marks certain or probable: parameters (section 7), the timeline
//! and the common item tail (8, 9.1), extrude fields (9.2), components and
//! body blob names (12), and the byte coverage table (14). Same choices as
//! the reference decoder.

use std::collections::HashMap;

use super::classes::{self, *};
use super::stream::{
    Object, Ref, Segment, f64_at, f64s_at, i32_at, slice, str16_at, text_at, u32_at, u64_at,
    utf16_units,
};
use super::unicode;

// The item state types moved to the dump IR's crate (mitcad#776).
pub use mitcad_dump::state::{ItemState, Suppression, item_state, item_suppression};

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
    /// Two more state bytes, in front of the item's `6CCF41D5` reference:
    /// `00 00` on a healthy item, the first 1 on a suppressed
    /// one (also seen as the middle one of `01 00 00` in the short
    /// reference layout) and the second on one after the timeline marker.
    pub after: Option<[u8; 2]>,
    /// Object-relative byte range `[start, end)` of the decoded fields.
    pub start: usize,
    pub end: usize,
}

impl ItemTail {
    /// The display name: the custom name, else the shown base name + index.
    pub fn display_name(&self) -> String {
        if !self.custom_name.is_empty() {
            self.custom_name.clone()
        } else if !self.base_name.is_empty() {
            format!("{}{}", shown_base_name(&self.base_name), self.index)
        } else {
            String::new()
        }
    }

    /// What the result number and the state bytes say about the item
    /// ([`item_state`]).
    pub fn state(&self) -> ItemState {
        item_state(self.result_no, self.flags)
    }

    /// The item is after the timeline marker (rolled back): the
    /// second byte of [`Self::after`]. In the controlled variant the
    /// rolled back items keep their result numbers, which the history no
    /// longer holds; in the corpora they are the end of the timeline.
    pub fn rolled_back(&self) -> bool {
        self.after.is_some_and(|a| a[1] == 1)
    }

    /// How the item is suppressed, if it is ([`item_suppression`]).
    pub fn suppression(&self) -> Option<Suppression> {
        item_suppression(self.result_no, self.flags)
    }
}

/// The name of a sketch item: the name the sketch object holds, or the
/// default name (the tail's custom name is one the sketch no longer has)
/// when that is empty ([`super::sketch::sketch_name`]).
pub fn sketch_item_name(own: &str, tail: &ItemTail) -> String {
    if own.is_empty() {
        ItemTail {
            custom_name: String::new(),
            ..tail.clone()
        }
        .display_name()
    } else {
        own.to_owned()
    }
}

/// The default name base the application shows for a base name the file
/// keeps: construction planes and axes and joint origins are stored under
/// internal names (`WorkPlane` ...) and shown as `Plane1`, `Axis1` and
/// `Joint Origin1` *(every one of 165 such items in the reference dumps of
/// the corpora: 123 planes, 8 axes, 34 joint origins)*.
fn shown_base_name(base: &str) -> &str {
    match base {
        "WorkPlane" => "Plane",
        "WorkAxis" => "Axis",
        "JointOrigin" => "Joint Origin",
        _ => base,
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
    /// The extrusion's direction in the component's coordinates (a unit
    /// vector), stored after the extent fields: `u8 flag | u8 solid | u32
    /// start | f64 x | f64 y | f64 z` (mitcad#96: valid on every extrusion
    /// of the corpus, whatever that `u32`; mitcad#154: [`Self::solid`] and
    /// [`Self::start`]).
    pub vector: Option<[f64; 3]>,
    /// The byte after the extent codes (mitcad#96): 1 only on one-sided
    /// extrusions through all or up to an object, which then go against
    /// their sketch's normal ([`ExtentSlot`]); 0 on every other extrusion.
    pub flag: Option<u8>,
    /// A symmetric extent's length (code a 3, mitcad#96): `true` the whole
    /// length, `false` half of it each way. The byte 19 bytes before the
    /// item's first input slot ([`BODY_INPUT`], after `u8 0 | byte | 14 × 0
    /// | u32 1`) is 1 or 2 *(the reference models' symmetric extrusions: 1
    /// on every whole length, 2 on the half one; the corpus' settled ones
    /// agree but for three cuts, where either length cuts the same)*.
    pub full_length: Option<bool>,
    /// The byte after [`Self::flag`]: 1 a solid, 0 a surface extrusion
    /// (mitcad#154; the cadmpeg format description's geometry kind).
    pub solid: Option<bool>,
    /// The byte after that: where the profile starts, 0 on the profile's
    /// plane, 1 on a plane offset from it, 2 on a selected face (mitcad#154;
    /// the description's start support).
    pub start: Option<u8>,
    /// Each side's extent kind (mitcad#154; the description's per-side
    /// extent values): 0 none, 1 a distance, 2 up to an object, 3 up to the
    /// next face, 4 through all. Read after the direction vector and seven
    /// optional references, `u32 side one`, and, unless side one is up to
    /// an object, nine zero bytes and `u32 side two`; only the
    /// combinations the description lists for the extent codes are taken.
    pub sides: Option<[u32; 2]>,
    /// For each side up to an object, its "nearest solution" byte (the API
    /// reference dumps' `isMinimumSolution`): the byte after the side's
    /// object reference (one of 55 up-to-an-object extents has it 1, the
    /// other 54 have 0). Side two only in the two-sided layout
    /// `[m1] [3 × 0] [u32 1] [u32 two] [u8 0] [ref] [m2]`; `None` where the
    /// side is not up to an object or the layout is not recognised.
    pub minimum: [Option<bool>; 2],
    /// The thin-wall block (mitcad#186): after the extent kinds the record
    /// keeps `[6 zero bytes] [slot] [5 bytes] [u8 thin] [u32 location]
    /// [slot] [5 zero bytes] [u32 1] [ref BODY_INPUT]`; the byte `thin` is
    /// 1 for a thin extrusion and 0 for a solid one. Read from the record's
    /// end, back from the first input slot (class version 8; the block is
    /// consistent on every extrusion of the corpus, where it is always the
    /// solid form, and on the reference models' thin extrusions).
    pub thin: Option<bool>,
    /// The wall's location when [`Self::thin`] is read: 0 side one, 1 the
    /// centre, 2 side two (the reference models' three thin variants; the
    /// dump names them `Side1`, `Center` and `Side2`).
    pub thin_location: Option<u8>,
    /// The parameter holder of the wall thickness (the slot after the
    /// location); `None` for a solid extrusion.
    pub thin_holder: Option<u64>,
    /// The second side's wall of a two-sided thin extrusion (mitcad#25): its
    /// location (as [`Self::thin_location`]) and the thickness' holder; the
    /// block is `u8 1 | u32 location | holder | u32 location two | holder
    /// two | u32 1 | ref BODY_INPUT`.
    pub thin_two: Option<(u8, u64)>,
    /// The selection of the face an extrusion starts on ([`Self::start`]
    /// 2): in class versions 7 and later, a reference right after the root
    /// part, where an extrusion that starts elsewhere has one zero byte
    /// (mitcad#641: the fields after it were read 10 bytes early).
    pub start_entity: Option<u64>,
}

impl ExtrudeFields {
    /// `Join`, `Cut`, `Intersect`, `NewBody` for the known codes.
    pub fn operation(&self) -> Option<&'static str> {
        match self.operation_code {
            1 => Some("Join"),
            2 => Some("Cut"),
            3 => Some("Intersect"),
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

    /// The parameter is a favorite (the parameters dialog's star): the third
    /// byte from the end of the flags after the expression is 1.
    pub fn is_favorite(&self) -> bool {
        self.gap1.len() >= 3 && self.gap1[self.gap1.len() - 3] == 1
    }

    /// The parameter of a driven (reference) sketch dimension: the fourth
    /// byte from the end of the flags after the expression is 1 (mitcad#33,
    /// probable: in every class version it marks the sketch dimensions
    /// whose values look measured, such as `1.500001 mm`; the reference
    /// models have no driven dimensions).
    pub fn is_driven(&self) -> bool {
        self.gap1.len() >= 4 && self.gap1[self.gap1.len() - 4] == 1
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
/// SCHEMA.md shaping (`mitcad_f3d_design::build`).
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
    /// The mesh bodies (mitcad#1366).
    pub mesh_bodies: Vec<super::mesh::MeshBodyEntry>,
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
    // The names users gave their sketches: the sketch object holds them, the
    // sketch item's tail does not.
    let mut sketch_names: HashMap<u64, String> = HashMap::new();
    for s in seg.objects_of(SKETCH) {
        if let Some(name) = super::sketch::sketch_name(seg, s) {
            for r in seg.ref_ids(seg.data(s)) {
                if seg.guid_of(r) == Some(SKETCH_FEATURE) {
                    sketch_names.insert(r, name.clone());
                }
            }
        }
    }
    // (Some sketch items refer to their sketch where the sketch has no
    // reference back.)
    for f in seg.objects_of(SKETCH_FEATURE) {
        if sketch_names.contains_key(&f.id) {
            continue;
        }
        for r in seg.ref_ids(seg.data(f)) {
            if let Some(s) = seg.object(r).filter(|_| seg.guid_of(r) == Some(SKETCH))
                && let Some(name) = super::sketch::sketch_name(seg, s)
            {
                sketch_names.insert(f.id, name);
            }
        }
    }
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
        let name = match (sketch_names.get(&fid), &tail) {
            (Some(own), Some(t)) if class == SKETCH_FEATURE => Some(sketch_item_name(own, t)),
            _ => tail.as_ref().map(ItemTail::display_name),
        };
        timeline.push(TimelineEntry {
            pos,
            id: fid,
            type_name: classes::known_name(&class),
            class_version: seg.version(o).unwrap_or(0),
            class,
            name,
            tail,
            extrude,
        });
    }

    // Parameters; the holder's root reference list names the owning feature.
    // (A count, the integer parameter of a pattern, and a text have holders
    // of their own classes with the same layout.)
    let mut holder_owner = HashMap::new();
    for h in seg
        .objects_of(PARAMETER_HOLDER)
        .chain(seg.objects_of(INT_HOLDER))
        .chain(seg.objects_of(TEXT_HOLDER))
    {
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
        mesh_bodies: super::mesh::mesh_bodies(seg),
    }
}

/// The common item tail (section 9.1), anchored at a reference to a
/// health object (`B7F34D7B`) in the main part and found by scanning back:
/// `i32 result_no | str16 base_name | u32 index | u32 0 |
/// str16 custom_name | u32 n | n refs | 3 flag bytes | ref health`.
///
/// The last such reference anchors it, the earlier ones when no tail ends
/// there (mitcad#96: fillets of class version 3 can seem to refer to a
/// health object near their start too, `ref 4A557CE2 | u32 0 | ref health |
/// 00 01 00 00 00 | ref 2CA5A1CD ...`; that is the corner flag `01`, a
/// zero and a `01` read as a reference to object 256 (`01 00 01 00 00 ...`
/// is a reference marker and the id 0x100), which is a health object in a
/// design that has one with that id; a tail found back from that one has
/// no base name, a wrong result number and no inputs. In the learning
/// dump the last reference gave the tail of all 53 such items, and every
/// other fillet and chamfer has one reference).
pub fn feature_tail(seg: &Segment, o: &Object) -> Option<ItemTail> {
    let d = seg.data(o);
    let end = seg.main_end(o);
    let health: Vec<usize> = seg
        .refs_in(d, 0, end)
        .into_iter()
        .filter(|(_, r)| seg.guid_of(r.id) == Some(HEALTH))
        .map(|(p, _)| p)
        .collect();
    health
        .into_iter()
        .rev()
        .find_map(|p| tail_before(seg, d, p))
}

/// The tail that ends at the health reference at `p`.
fn tail_before(seg: &Segment, d: &[u8], p: usize) -> Option<ItemTail> {
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
    // (Zero on every item. A hole whose sub-items list the hole itself,
    // `u32 1 | ref self`, ends its tail with `ref self | 00 00 00`; read
    // from 20 bytes later, the zeros before that reference look like an
    // empty base name, the index 256 and no sub-items, with the
    // reference's first bytes as this `u32`.)
    if after_index != 0 {
        return None;
    }
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
        after: after_health(seg, d, p),
        start: b - 4,
        end: p,
    })
}

/// The two state bytes in front of the first reference of class
/// `6CCF41D5` that follows the health reference at `p` (and the reference
/// of class `5FBE538E` before it, [`ItemTail::after`]).
fn after_health(seg: &Segment, d: &[u8], p: usize) -> Option<[u8; 2]> {
    let end = seg.ref_at(d, p)?.end;
    let refs = seg.refs_in(d, end, end + 80);
    let class = |r: &Ref, g: &str| seg.guid_of(r.id).is_some_and(|c| c.starts_with(g));
    let first = refs.iter().position(|(_, r)| class(r, "5FBE538E"))?;
    let (at, _) = refs[first + 1..]
        .iter()
        .find(|(_, r)| class(r, "6CCF41D5"))?;
    let a = d.get(at.checked_sub(2)?..*at)?;
    Some([a[0], a[1]])
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
    // The start face's selection takes the place of the first byte
    // (mitcad#641).
    let start_ref = (skip == 3)
        .then(|| seg.ref_at(d, root.end))
        .flatten()
        .filter(|r| d.get(r.end..r.end + 2) == Some(&[0, 1]));
    let p = start_ref.as_ref().map_or(root.end + skip, |r| r.end + 2);
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
    // The byte before the `u32` is 1 for a solid, 0 for a surface
    // extrusion (mitcad#154).
    let vector = matches!(d.get(p + 13), Some(0 | 1))
        .then(|| f64s_at(d, p + 18, 3))
        .flatten()
        .map(|v| [v[0], v[1], v[2]])
        .filter(|v| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() < 1e-6);
    let (thin, thin_location, thin_holder, thin_two) = match vector
        .filter(|_| seg.version(o).is_some_and(|v| v >= 8))
        .and_then(|_| thin_block(seg, d, p + 42))
    {
        Some((thin, location, holder, two)) => (Some(thin), Some(location), holder, two),
        None => (None, None, None, None),
    };
    Some(ExtrudeFields {
        operation_code,
        extent_a,
        extent_b,
        direction,
        vector,
        flag: d.get(p + 12).copied(),
        full_length: (extent_a == 3)
            .then(|| symmetric_full_length(seg, d, p + 42))
            .flatten(),
        solid: vector.and(d.get(p + 13)).map(|&b| b == 1),
        start: vector.and(d.get(p + 14)).copied().filter(|&b| b <= 2),
        sides: vector.and_then(|_| extent_sides(seg, d, p + 42, extent_a)),
        minimum: vector.map_or([None; 2], |_| to_object_minimum(seg, d, p + 42, extent_a)),
        thin,
        thin_location,
        thin_holder,
        thin_two,
        start_entity: start_ref.map(|r| r.id),
    })
}

/// The raw fields of a revolve after its root part *(the API reference
/// dumps' 103 revolutions of class versions 1 and 2, and the controlled
/// models: a revolution, a symmetric one and a two-sided one)*:
///
/// ```text
/// u32 operation | u32 2 | u8 0 or 1 (not known)
///   | version 2 only: u8 project axis
///   | u8 solid | u8 symmetric | u8 two sides
///   | [ref second angle holder or 00] | ref angle holder
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RevolveFields {
    /// 1 Join, 2 Cut, 3 Intersect, 4 NewBody (as an extrusion's).
    pub operation_code: u32,
    /// The axis is projected into the sketch's plane (`isProjectAxis`: 1 on
    /// every revolution of the corpus, 0 on the controlled models that a
    /// script made); class version 1 has no such byte.
    pub project_axis: Option<bool>,
    /// A solid (not a surface) revolution.
    pub solid: bool,
    /// The angle goes half each way.
    pub symmetric: bool,
    /// Two angles, one each way.
    pub two_sides: bool,
}

/// [`RevolveFields`] of a revolve item; `None` for other layouts.
pub fn revolve_fields(seg: &Segment, o: &Object) -> Option<RevolveFields> {
    let version = seg.version(o).filter(|v| matches!(v, 1 | 2))?;
    let d = seg.data(o);
    let p = seg.root_part(d)?.end;
    let operation_code = u32_at(d, p)?;
    if !(1..=4).contains(&operation_code) || u32_at(d, p + 4)? != 2 || *d.get(p + 8)? > 1 {
        return None;
    }
    let flag = |at: usize| match d.get(at) {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    };
    let (project_axis, at) = if version == 2 {
        (Some(flag(p + 9)?), p + 10)
    } else {
        (None, p + 9)
    };
    Some(RevolveFields {
        operation_code,
        project_axis,
        solid: flag(at)?,
        symmetric: flag(at + 1)?,
        two_sides: flag(at + 2)?,
    })
}

/// The end of the seven optional reference slots that follow the direction
/// vector (`from` is its end): an absent slot is one zero byte.
fn slots_end(seg: &Segment, d: &[u8], from: usize) -> Option<usize> {
    let mut q = from;
    for _ in 0..7 {
        if *d.get(q)? == 0 {
            q += 1;
        } else {
            q = seg.ref_at(d, q)?.end;
        }
    }
    Some(q)
}

/// A thin block: the flag, the first wall's location and holder, the
/// second's (two-sided extrusions).
type ThinBlock = (bool, u8, Option<u64>, Option<(u8, u64)>);

/// [`ExtrudeFields::thin`], [`ExtrudeFields::thin_location`] and
/// [`ExtrudeFields::thin_holder`]: `from` is the end of the direction
/// vector. The block is found back from the first input slot after the
/// reference slots (`u32 1 | ref BODY_INPUT`): five zero bytes, the
/// thickness slot (a parameter holder, else one absent byte), the `u32`
/// location and the `u8` flag before them. Only the two consistent forms
/// are taken: a solid extrusion (flag 0, location 0, no holder) and a thin
/// one (flag 1, location 0 to 2, a holder).
fn thin_block(seg: &Segment, d: &[u8], from: usize) -> Option<ThinBlock> {
    let after = slots_end(seg, d, from)?;
    let refs = seg.refs_in(d, after, d.len());
    let (slot, _) = refs.iter().find(|(s, r)| {
        seg.guid_of(r.id) == Some(BODY_INPUT) && *s >= after + 4 && u32_at(d, *s - 4) == Some(1)
    })?;
    let anchor = slot - 4;
    // Two sides: the second wall's holder ends at the anchor, the location
    // before it, the first wall's holder, its location and the flag.
    let holder = |end: usize| {
        refs.iter()
            .find(|(_, r)| r.end == end && seg.guid_of(r.id) == Some(PARAMETER_HOLDER))
    };
    if let Some((s2, r2)) = holder(anchor)
        && let Some(loc2) = s2
            .checked_sub(4)
            .and_then(|a| u32_at(d, a))
            .filter(|&l| l <= 2)
        && let Some((s1, r1)) = holder(s2 - 4)
        && let Some(loc1) = s1
            .checked_sub(4)
            .and_then(|a| u32_at(d, a))
            .filter(|&l| l <= 2)
        && d.get(s1.checked_sub(5)?) == Some(&1)
    {
        return Some((true, loc1 as u8, Some(r1.id), Some((loc2 as u8, r2.id))));
    }
    let end = anchor.checked_sub(5)?;
    if slice(d, end, anchor) != [0u8; 5] {
        return None;
    }
    let holder = refs
        .iter()
        .find(|(_, r)| r.end == end && seg.guid_of(r.id) == Some(PARAMETER_HOLDER));
    let (start, holder) = match holder {
        Some((s, r)) => (*s, Some(r.id)),
        None if *d.get(end.checked_sub(1)?)? == 0 => (end - 1, None),
        None => return None,
    };
    let block = start.checked_sub(5)?;
    let flag = *d.get(block)?;
    let location = u32_at(d, block + 1)?;
    match (flag, holder, location) {
        (0, None, 0) => Some((false, 0, None, None)),
        (1, Some(h), 0..=2) => Some((true, location as u8, Some(h), None)),
        _ => None,
    }
}

/// [`ExtrudeFields::sides`]: `from` is the end of the direction vector.
fn extent_sides(seg: &Segment, d: &[u8], from: usize, direction: u32) -> Option<[u32; 2]> {
    let q = slots_end(seg, d, from)?;
    let one = u32_at(d, q)?;
    let two = if one == 2 {
        0
    } else {
        // The older writer leaves nine zero bytes between the sides; the
        // newer one (the reference models, mitcad#186) keeps `u8 0 | u32 a |
        // u32 b` there: `a` is 2 on a side through all (else 0), `b` 1 on
        // two sides and symmetric extents (else 0).
        if *d.get(q + 4)? != 0 || u32_at(d, q + 5)? > 2 || u32_at(d, q + 9)? > 1 {
            return None;
        }
        u32_at(d, q + 13)?
    };
    let listed = matches!(
        (direction, one, two),
        (1, 1..=4, 0) | (2, 1..=4, 0..=4) | (3, 1, 0) | (3, 1, 1) | (3, 4, 4)
    );
    listed.then_some([one, two])
}

/// [`ExtrudeFields::minimum`]: `from` is the end of the direction vector.
fn to_object_minimum(seg: &Segment, d: &[u8], from: usize, direction: u32) -> [Option<bool>; 2] {
    let flag = |at: usize| d.get(at).copied().filter(|b| *b <= 1).map(|b| b == 1);
    let Some(q) = slots_end(seg, d, from).filter(|&q| u32_at(d, q) == Some(2)) else {
        return [None; 2];
    };
    let Some(first) = seg.ref_at(d, q + 4) else {
        return [None; 2];
    };
    let e = first.end;
    let second = (direction == 2
        && slice(d, e + 1, e + 4) == [0u8; 3]
        && u32_at(d, e + 4) == Some(1)
        && u32_at(d, e + 8) == Some(2)
        && d.get(e + 12) == Some(&0))
    .then(|| seg.ref_at(d, e + 13))
    .flatten()
    .and_then(|r| flag(r.end));
    [flag(e), second]
}

/// See [`ExtrudeFields::full_length`]; `from` is the end of the direction
/// vector.
fn symmetric_full_length(seg: &Segment, d: &[u8], from: usize) -> Option<bool> {
    let (s, _) = seg
        .refs_in(d, from, d.len())
        .into_iter()
        .find(|(_, r)| seg.guid_of(r.id) == Some(BODY_INPUT))?;
    if s < from + 20 || u32_at(d, s - 4) != Some(1) || d[s - 20] != 0 {
        return None;
    }
    match d[s - 19] {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// The object an input slot's role ([`ExtentSlot::role`]) is read after.
const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";

/// The role of an extrusion's profile slot ([`ExtentSlot`]).
pub const PROFILE_ROLE: u32 = 65;

/// The input slot listing the bodies changed by an extrusion.
pub const PARTICIPANT_ROLE: u32 = 8;

/// The same slot in a revolution's and a hole's record (the API reference
/// dumps: the revolutions that have it list the bodies they changed).
pub const PARTICIPANT_ROLE_2: u32 = 4;

/// An extrusion's input slot (mitcad#96): a [`BODY_INPUT`] object the item
/// refers to, `u32 n | n refs | ... | ref 2DF7DA30 | u32 0 | u32 role |
/// ...` after its root part *(the corpus' extrusions of every class
/// version; the learning dump's items, where the history settled their
/// extents, agree with these roles on all but two that went to an
/// equivalent extent)*:
///
/// - 65: the profile source (a profile source, a face for a face profile);
/// - 8: the participating bodies ([`BODY_REF`]);
/// - 5: one side extends through all (one slot per such side, its input
///   the bodies); in the reference models' newer writer also a side up to a
///   plane (its input the plane's [`ENTITY_REF`]);
/// - 17, 18: one side extends up to an object, the slot's one input
///   ([`FACE_REF`]); 18 where the extent codes still say a distance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtentSlot {
    pub role: u32,
    /// The slot's inputs (the `n` references), in order.
    pub inputs: Vec<u64>,
}

impl ExtentSlot {
    /// A side up to an object.
    pub fn is_to_object(&self) -> bool {
        matches!(self.role, 17 | 18)
    }

    /// A side through all.
    pub fn is_through_all(&self) -> bool {
        self.role == 5
    }
}

/// An item's input slots in the order it refers to them (slots without
/// the role's object are left out).
pub fn extent_slots(seg: &Segment, item: u64) -> Vec<ExtentSlot> {
    let mut out = Vec::new();
    let mut seen = Vec::new();
    for id in seg.ref_ids(seg.data_of(item)) {
        if seg.guid_of(id) != Some(BODY_INPUT) || seen.contains(&id) {
            continue;
        }
        seen.push(id);
        let d = seg.data_of(id);
        let Some(root) = seg.root_part(d) else {
            continue;
        };
        let Some(n) = u32_at(d, root.end).filter(|&n| n <= 10_000) else {
            continue;
        };
        let mut q = root.end + 4;
        let mut inputs = Vec::new();
        for _ in 0..n {
            let Some(r) = seg.ref_at(d, q) else { break };
            inputs.push(r.id);
            q = r.end;
        }
        if inputs.len() != n as usize {
            continue;
        }
        let key = seg
            .refs_in(d, q, d.len())
            .into_iter()
            .find(|(_, r)| seg.guid_of(r.id) == Some(SLOT_KEY));
        if let Some(role) = key.and_then(|(_, r)| u32_at(d, r.end + 4)) {
            out.push(ExtentSlot { role, inputs });
        }
    }
    out
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
    let (g1, nstr) = match version {
        ..=6 => (5, 3),
        7 => (9, 4),
        8 | 9 => (10, 4),
        _ => (11, 4),
    };
    // Version 8 (2026 writers; verified on the reference models): the gap
    // is `u8 | u32 | u8 | str16 comment`, so the comment comes before the
    // number (the string after the role stays empty), and the flags after
    // the expression take 10 bytes.
    // Version 9 (writer 2705): one more `u32 0` in front of the `u32 6`.
    // Version 10 (writer 2706): `u32 5` in place of that `u32 6`, and the
    // flags after the expression take 11 bytes, the user flag still last
    // (every parameter of a writer-2706 design: its 263 parameters decode
    // only so, and the user parameters' flag is the 11th byte).
    let v8_comment = if version >= 8 {
        let (c, q) = str16_at(d, p + if version >= 9 { 10 } else { 6 })?;
        Some((c, q - p))
    } else {
        None
    };
    let gaps = match &v8_comment {
        Some((_, gap)) => *gap..*gap + 1,
        None => 0..24,
    };
    for gap in gaps {
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
        let mut comment = if nstr == 4 { s.next() } else { None };
        if let Some((c, _)) = &v8_comment
            && comment.as_deref().is_none_or(str::is_empty)
        {
            comment = Some(c.clone());
        }
        let unit = s.next()?;
        let name = s.next()?;
        // A text parameter keeps a default expression with a unit (`1 mm`)
        // where the other parameters keep nothing: not a comment.
        if unit == "Text" {
            comment = v8_comment.as_ref().map(|(c, _)| c.clone());
        }
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

/// A component's name: the `str16` that follows the last GUID text of the
/// object (the component's own identifiers come before it). It is empty
/// while the component has its default name (`Component<n>`), and then the
/// component has no name here: the last printable text before it is the
/// component's material (`PrismMaterial-018`), not a name. Without a text
/// after the last GUID: the last printable `str16` that is not a GUID
/// (probable, section 12).
pub fn component_name(d: &[u8]) -> Option<String> {
    let mut name = None;
    let mut after_guid = None;
    let mut p = 0;
    while p + 4 < d.len() {
        match text_at(d, p) {
            Some((s, e)) => {
                if guidish(&s) {
                    after_guid = Some(e);
                } else {
                    name = Some(s);
                }
                p = e;
            }
            None => p += 1,
        }
    }
    if let Some(e) = after_guid
        && let Some((s, _)) = str16_at(d, e)
        && (s.is_empty() || unicode::is_printable(&s))
    {
        return Some(s).filter(|s| !s.is_empty() && !guidish(s));
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

/// The matrix an input slot's role key object holds (the key object
/// after the slot's inputs, [`SLOT_KEY`]): 16 doubles, a 4 x 4 row-major
/// transform in cm with the translation in the last column and the row
/// `0 0 0 1` at the bottom *(read from the entities of other components
/// that a design's planes and extrusions name: the key objects of the
/// slots that hold them carry it; the transform that places the entity's
/// component in the design where the selection was made)*. A byte or two
/// of flags come before the numbers.
fn slot_matrix(seg: &Segment, key: u64) -> Option<[[f64; 4]; 4]> {
    let d = seg.data_of(key);
    let root = seg.root_part(d)?;
    (0..4).find_map(|skip| {
        let v = f64s_at(d, root.end + skip, 16)?;
        let bottom = v[12..16] == [0.0, 0.0, 0.0, 1.0];
        let finite = v.iter().all(|x| x.is_finite() && x.abs() < 1e9);
        let rows = |r: usize| [v[4 * r], v[4 * r + 1], v[4 * r + 2]];
        let unit = (0..3).all(|r| {
            let n = rows(r).iter().map(|x| x * x).sum::<f64>();
            (n - 1.0).abs() < 1e-6
        });
        (bottom && finite && unit).then(|| {
            [
                [v[0], v[1], v[2], v[3]],
                [v[4], v[5], v[6], v[7]],
                [v[8], v[9], v[10], v[11]],
                [0.0, 0.0, 0.0, 1.0],
            ]
        })
    })
}

/// The transform each entity input carries in the slot that holds it
/// ([`slot_matrix`]), by the id of the entity's reference object: where the
/// entity's component is placed in the design at the selection. Computed
/// once per segment.
pub fn slot_contexts(seg: &Segment) -> &HashMap<u64, [[f64; 4]; 4]> {
    seg.contexts.get_or_init(|| {
        let mut out = HashMap::new();
        for o in seg.objects_of(BODY_INPUT) {
            let d = seg.data(o);
            let Some(root) = seg.root_part(d) else {
                continue;
            };
            let Some(n) = u32_at(d, root.end).filter(|n| (1..=10_000).contains(n)) else {
                continue;
            };
            let mut q = root.end + 4;
            let mut inputs = Vec::new();
            for _ in 0..n {
                let Some(r) = seg.ref_at(d, q) else { break };
                inputs.push(r.id);
                q = r.end;
            }
            if inputs.len() != n as usize {
                continue;
            }
            let key = seg
                .refs_in(d, q, d.len())
                .into_iter()
                .find(|(_, r)| seg.guid_of(r.id) == Some(SLOT_KEY));
            let Some(matrix) = key.and_then(|(_, r)| slot_matrix(seg, r.id)) else {
                continue;
            };
            for input in inputs {
                out.entry(input).or_insert(matrix);
            }
        }
        out
    })
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
            after: None,
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

    #[test]
    fn display_names_of_construction_items() {
        let mut t = ItemTail {
            base_name: "WorkPlane".into(),
            index: 2,
            after_index: 0,
            custom_name: String::new(),
            result_no: -1,
            sub_features: vec![],
            flags: [0; 3],
            after: None,
            start: 0,
            end: 0,
        };
        assert_eq!(t.display_name(), "Plane2");
        t.base_name = "WorkAxis".into();
        assert_eq!(t.display_name(), "Axis2");
        t.base_name = "JointOrigin".into();
        assert_eq!(t.display_name(), "Joint Origin2");
        t.base_name = "Sketch".into();
        assert_eq!(t.display_name(), "Sketch2");
        // a name the user gave is shown as it is
        t.base_name = "WorkPlane".into();
        t.custom_name = "Mid".into();
        assert_eq!(t.display_name(), "Mid");
    }
}
