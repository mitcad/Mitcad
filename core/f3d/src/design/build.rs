// SPDX-License-Identifier: MIT
//! Shapes the decoded streams into the dump IR (SCHEMA.md), as the
//! reference decoder does: only what the streams support is written;
//! decoder-only data goes under `_f3d`.

use std::collections::{BTreeMap, HashMap};

use super::classes::{self, *};
use super::decode::{self, Decoded, ParameterEntry, TimelineEntry};
use super::ir::*;
use super::recipe;
use super::sketch::{self, LightBulb, SketchInfo};
use super::stream::{Segment, f64_at, f64s_at, header_end, hex, py_sum, str16_at, str16s, u32_at};

// Sweeps, pipes and lofts (mitcad#34).
mod sweeps;
// Joints, as-built joints, joint origins and ground items (mitcad#66).
mod joints;
// Selections of combines, splits, patterns, mirrors and holes (mitcad#67).
mod selections;
// Where the timeline's items put the occurrences (mitcad#81).
mod placements;
// The loops of the profiles extrusions and revolutions select (mitcad#96).
mod profiles;
// The items that made the bodies feature inputs name (mitcad#96).
mod producers;

/// Helpers for the decoder's tests.
#[cfg(test)]
pub(crate) mod tests_support {
    pub(crate) use super::joints::kind_fits;
}

/// Name of this decoder in `generator.decoder`.
pub const DECODER_NAME: &str = concat!("mitcad-f3d ", env!("CARGO_PKG_VERSION"));

fn paramref(p: &ParameterEntry) -> Reference {
    Reference::Parameter(ParameterRef {
        name: Some(p.name.clone()),
        expression: Some(p.expression.clone()),
        value: Some(p.value),
        unit: Some(p.unit.clone()),
        ..ParameterRef::default()
    })
}

/// A count that is no parameter (pattern quantities), as a unitless value.
fn count_ref(value: u32) -> Reference {
    Reference::Parameter(ParameterRef {
        expression: Some(value.to_string()),
        value: Some(f64::from(value)),
        unit: Some(String::new()),
        ..ParameterRef::default()
    })
}

/// `(slot, value)` of a feature's integer inputs ([`INT_HOLDER`]), by slot.
/// Like the parameter holders, their root part is only the list
/// `[feature]` (no attribute flag): `u8 1, u32 1, ref`, then `u32 slot,
/// u8 0, u8 1, u32 value`.
fn int_inputs(seg: &Segment, fid: u64) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for h in inputs_of(seg, fid, INT_HOLDER) {
        let d = seg.data_of(h);
        let Some(p) = header_end(d) else {
            continue;
        };
        if d.get(p..p + 5) != Some(&[1, 1, 0, 0, 0][..]) {
            continue;
        }
        let Some(r) = seg.ref_at(d, p + 5) else {
            continue;
        };
        let q = r.end;
        if d.get(q + 4) != Some(&0) || d.get(q + 5) != Some(&1) {
            continue;
        }
        if let (Some(slot), Some(value)) = (u32_at(d, q), u32_at(d, q + 6)) {
            out.push((slot, value));
        }
    }
    out.sort_unstable();
    out
}

/// Ids of the references in an object to objects of class `guid`, once
/// each, in order.
fn inputs_of(seg: &Segment, id: u64, guid: &str) -> Vec<u64> {
    let mut out = Vec::new();
    for r in seg.ref_ids(seg.data_of(id)) {
        if seg.guid_of(r) == Some(guid) && !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

fn json_value<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).expect("serializes")
}

/// An item's input list: the common tail's `u32 n | n refs` that ends
/// where the tail's decoded fields start (`ItemTail::start`, the result
/// number). `None` for a null reference. The list with the most references
/// that ends there (the end of the list can read as another, of nulls).
fn tail_inputs(seg: &Segment, d: &[u8], end: usize) -> Option<Vec<Option<u64>>> {
    let lo = end.saturating_sub(16 * 1024);
    let mut p = end.checked_sub(4)?;
    let mut best: Option<Vec<Option<u64>>> = None;
    while p >= lo {
        if let Some(n) = u32_at(d, p).filter(|&n| n <= 10_000) {
            let mut q = p + 4;
            let mut out = Vec::new();
            for _ in 0..n {
                if q >= end {
                    break;
                }
                if d.get(q) == Some(&0) {
                    out.push(None);
                    q += 1;
                    continue;
                }
                let Some(r) = seg.ref_at(d, q) else { break };
                out.push(Some(r.id));
                q = r.end;
            }
            let refs = |v: &[Option<u64>]| v.iter().flatten().count();
            if q == end
                && out.len() == n as usize
                && best.as_ref().is_none_or(|b| refs(b) < refs(&out))
            {
                best = Some(out);
            }
        }
        if p == 0 {
            break;
        }
        p -= 1;
    }
    best
}

/// An edge input as a reference: an edge fingerprint with the edge's names
/// (`_f3d`), its geometry left to [`super::inputs`]. `None` when the input
/// has no edge recipe that decodes.
fn edge_input(seg: &Segment, input: u64) -> Option<Reference> {
    let recipe = recipe::of_input(seg, input).filter(|r| r.kind == "edge")?;
    Some(Reference::Edge(Box::new(Fingerprint {
        object_type: Some("BRepEdge".to_owned()),
        f3d: Some(FingerprintF3d {
            object_id: Some(input),
            recipe: Some(recipe.kind),
            entities: Some(recipe.entities),
            ..FingerprintF3d::default()
        }),
        ..Fingerprint::default()
    })))
}

/// A face input as a reference: a face fingerprint with the face's names
/// (`_f3d`), its geometry left to [`super::inputs`]. `None` when the input
/// has no face recipe that decodes.
fn face_input(seg: &Segment, input: u64) -> Option<Reference> {
    let recipe = recipe::of_input(seg, input)
        .filter(|r| matches!(r.kind.as_str(), "face" | "bounded_face"))?;
    Some(Reference::Face(Box::new(Fingerprint {
        object_type: Some("BRepFace".to_owned()),
        f3d: Some(FingerprintF3d {
            object_id: Some(input),
            recipe: Some(recipe.kind),
            entities: Some(recipe.entities),
            ..FingerprintF3d::default()
        }),
        ..Fingerprint::default()
    })))
}

/// A thread's fields (mitcad#35): a thread item's, or a tapped hole's
/// thread sub-item's (class `584D9526`, version 5; older versions in
/// [`thread_fields`]), after the root part
/// *(read from the reference models' threads, whose dumps give the same
/// values, and the corpus')*:
///
/// ```text
/// f64 angle (deg) | u8 external | u32 n, n × 16 bytes (modelled threads)
/// str16 class | str16 designation | str16 designation | str16 size | str16 type
/// u8 full length | u32 location | f64 major diameter | f64 minor diameter (cm)
/// u8 modelled | f64 pitch | f64 pitch diameter (cm) | ref hole (null for an
/// item) | ...
/// ```
///
/// `external` is 1 on every external face of the corpus' threads and 0 on
/// every internal one. The first designation is empty in some files; the
/// second is then the designation.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadFields {
    /// The flank angle, degrees (60, 55 for Whitworth pipe threads).
    pub angle: f64,
    pub internal: bool,
    pub full_length: bool,
    pub modeled: bool,
    pub class: String,
    pub designation: String,
    pub size: String,
    pub thread_type: String,
    /// Diameters and pitch, cm.
    pub major: f64,
    pub minor: f64,
    pub pitch: f64,
    pub pitch_diameter: f64,
    /// The end a partial thread is measured from ([`Self::location`]).
    pub location: u32,
}

impl ThreadFields {
    /// The `ThreadInfo` of the dump (SCHEMA.md §5.3).
    fn info(&self) -> serde_json::Value {
        serde_json::json!({
            "_type": "ThreadInfo",
            "threadType": self.thread_type,
            "threadSize": self.size,
            "threadDesignation": self.designation,
            "threadClass": self.class,
            "isInternal": self.internal,
            "threadAngle": self.angle,
            "majorDiameter": self.major,
            "minorDiameter": self.minor,
            "pitchDiameter": self.pitch_diameter,
            "threadPitch": self.pitch,
        })
    }

    /// The end of the face a partial thread is measured from: 1 where the
    /// axis of the face's cylinder points *(a modelled partial thread of the
    /// corpus lies there; the reference models' threads have 1 and the
    /// default `HighEndThreadLocation`)*, 2 the other end.
    fn location(&self) -> Option<&'static str> {
        match self.location {
            1 => Some("HighEndThreadLocation"),
            2 => Some("LowEndThreadLocation"),
            _ => None,
        }
    }
}

/// See [`ThreadFields`]; `None` for other layouts. Class versions before 5
/// have one designation, and before 4 no list after the side flag *(the
/// corpus' threads of versions 1, 2 and 4)*.
pub fn thread_fields(seg: &Segment, id: u64) -> Option<ThreadFields> {
    let o = seg.object(id)?;
    let version = seg.version(o)?;
    if seg.guid(o) != Some(THREAD) || !(1..=5).contains(&version) {
        return None;
    }
    let d = seg.data(o);
    let mut p = seg.root_part(d)?.end;
    let angle = f64_at(d, p)?;
    let external = *d.get(p + 8)?;
    if external > 1 {
        return None;
    }
    p += 9;
    if version >= 4 {
        let n = u32_at(d, p)? as usize;
        if n > 64 {
            return None;
        }
        p += 4 + 16 * n;
    }
    let mut text = Vec::new();
    for _ in 0..if version >= 5 { 5 } else { 4 } {
        let (s, e) = str16_at(d, p)?;
        text.push(s);
        p = e;
    }
    let full_length = *d.get(p)?;
    let location = u32_at(d, p + 1)?;
    let major = f64_at(d, p + 5)?;
    let minor = f64_at(d, p + 13)?;
    let modeled = *d.get(p + 21)?;
    let pitch = f64_at(d, p + 22)?;
    let pitch_diameter = f64_at(d, p + 30)?;
    let sane = |x: f64| x.is_finite() && x > 0.0;
    if full_length > 1 || modeled > 1 || !(sane(angle) && sane(major) && sane(minor) && sane(pitch))
    {
        return None;
    }
    if text.len() == 4 {
        text.insert(1, String::new());
    }
    let [class, first, second, size, thread_type]: [String; 5] = text.try_into().ok()?;
    Some(ThreadFields {
        angle,
        internal: external == 0,
        full_length: full_length == 1,
        modeled: modeled == 1,
        class,
        designation: if first.is_empty() { second } else { first },
        size,
        thread_type,
        major,
        minor,
        pitch,
        pitch_diameter,
        location,
    })
}

/// The first rigid 4x4 matrix (row-major, last row `0 0 0 1`, orthonormal
/// rotation) in an object's data, at any offset.
fn rigid_matrix(d: &[u8]) -> Option<Mat4> {
    let mut p = 0;
    while p + 128 <= d.len() {
        if let Some(m) = f64s_at(d, p, 16) {
            let last = m[12] == 0.0 && m[13] == 0.0 && m[14] == 0.0 && m[15] == 1.0;
            let row = |r: usize| [m[4 * r], m[4 * r + 1], m[4 * r + 2]];
            let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let orthonormal = (0..3).all(|i| {
                (0..3).all(|j| {
                    let want = if i == j { 1.0 } else { 0.0 };
                    (dot(row(i), row(j)) - want).abs() < 1e-9
                })
            });
            if last && orthonormal && m.iter().all(|x| x.is_finite()) {
                let mut out = [[0.0; 4]; 4];
                for (r, row) in out.iter_mut().enumerate() {
                    row.copy_from_slice(&m[4 * r..4 * r + 4]);
                }
                return Some(out);
            }
        }
        p += 1;
    }
    None
}

/// A fillet's or chamfer's edges by set: the input list holds, for each
/// set, its set input ([`BODY_INPUT`]), then its edges and parameter
/// holders. Each group: (edge references, holders); a group's edges are
/// `None` when one of them does not decode.
///
/// A set input says what its group selects (mitcad#96: the `u32` after
/// `ref 2DF7DA30 | u32 0` in the set input object): 8 edges, 16 faces
/// (`bounded_face` recipes: every edge of the face, as a face reference
/// that [`super::inputs`] turns into the face's edges), 9 a face that the
/// next group's edges repeat (left out). A group without holders goes with
/// the next group that has some (`B e.. B f.. H H`: edges and a face of one
/// set; `H B f.. B f.. H`).
fn edge_groups(seg: &Segment, inputs: &[Option<u64>]) -> Vec<(Option<Vec<Reference>>, Vec<u64>)> {
    // (Edges, holders, what the set input selects.)
    type Group = (Option<Vec<Reference>>, Vec<u64>, Option<u32>);
    let mut groups: Vec<Group> = Vec::new();
    for &i in inputs.iter().flatten() {
        match seg.guid_of(i) {
            Some(BODY_INPUT) => groups.push((Some(Vec::new()), Vec::new(), set_selects(seg, i))),
            Some(FACE_REF) => {
                if let Some(g) = groups.last_mut() {
                    if g.2 == Some(9) {
                        continue;
                    }
                    let entity = edge_input(seg, i).or_else(|| face_input(seg, i));
                    g.0 = match (g.0.take(), entity) {
                        (Some(mut v), Some(e)) => {
                            v.push(e);
                            Some(v)
                        }
                        _ => None,
                    };
                }
            }
            Some(PARAMETER_HOLDER) => {
                if let Some(g) = groups.last_mut() {
                    g.1.push(i);
                }
            }
            _ => {}
        }
    }
    // Groups without holders joined to the next one with holders.
    let mut out: Vec<(Option<Vec<Reference>>, Vec<u64>)> = Vec::new();
    let mut waiting: Option<Option<Vec<Reference>>> = None;
    let later_holders: Vec<bool> = (0..groups.len())
        .map(|k| groups[k + 1..].iter().any(|g| !g.1.is_empty()))
        .collect();
    for (k, (edges, holders, _)) in groups.into_iter().enumerate() {
        let edges = match waiting.take() {
            Some(before) => before.zip(edges).map(|(mut a, b)| {
                a.extend(b);
                a
            }),
            None => edges,
        };
        if holders.is_empty() && later_holders[k] {
            waiting = Some(edges);
        } else {
            out.push((edges, holders));
        }
    }
    out
}

/// What a fillet's or chamfer's set input selects: the `u32` after
/// `ref 2DF7DA30 | u32 0` (8 edges, 9 a face, 16 faces with their
/// edges; [`edge_groups`]).
fn set_selects(seg: &Segment, input: u64) -> Option<u32> {
    let d = seg.data_of(input);
    let (_, r) = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .find(|(_, r)| seg.guid_of(r.id).is_some_and(|g| g.starts_with("2DF7DA30")))?;
    (u32_at(d, r.end)? == 0)
        .then(|| u32_at(d, r.end + 4))
        .flatten()
}

/// A construction plane's light bulb (mitcad#6): in its plane object
/// (`8D424E29`), the reference back to the feature is followed by `u8 0 |
/// u8 1 | str16 name (empty when not renamed) | u32 0 | u8 light bulb` (1
/// on, 0 off). It equals the reference dumps' `isLightBulbOn` for every
/// construction plane of the reference models; the corpus' planes have the
/// same layout. None when it is not found exactly once.
fn plane_light_bulb(seg: &Segment, feature: u64) -> Option<LightBulb> {
    let mut found = None;
    for plane in inputs_of(seg, feature, ORIGIN_PLANE) {
        let d = seg.data_of(plane);
        let Some(start) = header_end(d) else {
            continue;
        };
        for (_, r) in seg.refs_in(d, start, d.len()) {
            if r.id != feature || d.get(r.end..r.end + 2) != Some(&[0, 1][..]) {
                continue;
            }
            let Some((_, e)) = str16_at(d, r.end + 2) else {
                continue;
            };
            if u32_at(d, e) != Some(0) {
                continue;
            }
            let Some(&b) = d.get(e + 4) else {
                continue;
            };
            if b > 1 || found.is_some() {
                return None;
            }
            found = Some(LightBulb {
                on: b == 1,
                raw: d[e..e + 5].to_vec(),
            });
        }
    }
    found
}

/// `XY`/`XZ`/`YZ`, `X`/`Y`/`Z` or `Origin` for origin geometry.
fn origin_name(seg: &Segment, id: u64) -> Option<String> {
    let o = seg.object(id)?;
    let d = seg.data(o);
    match seg.guid(o)? {
        ORIGIN_PLANE => str16s(d)
            .into_iter()
            .find(|s| matches!(s.as_str(), "XY" | "XZ" | "YZ")),
        ORIGIN_AXIS => ["X", "Y", "Z"]
            .into_iter()
            .find(|a| {
                let needle = [1, 0, 0, 0, a.as_bytes()[0]];
                d.windows(5).any(|w| w == needle)
            })
            .map(str::to_string),
        ORIGIN_POINT => Some("Origin".to_string()),
        _ => None,
    }
}

/// Plane object `8D424E29` (section 10.4): `... f64 umin, vmin | u8 0 |
/// f64 origin[3] | u[3] | v[3] | f64 umax, vmax`, found by its shape
/// (unit, orthogonal u and v; increasing bounds).
fn plane_geometry(seg: &Segment, id: u64) -> Option<Geometry> {
    let d = seg.data_of(id);
    let start = header_end(d)?;
    let mut q = start;
    while (q as isize) < d.len() as isize - 8 * 13 {
        let (Some(b), Some(v)) = (f64s_at(d, q, 2), f64s_at(d, q + 17, 11)) else {
            break;
        };
        let (umin, vmin) = (b[0], b[1]);
        let (o, u, w) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
        let (umax, vmax) = (v[9], v[10]);
        let nu = py_sum(u.iter().map(|x| x * x));
        let nv = py_sum(w.iter().map(|x| x * x));
        let dot = py_sum((0..3).map(|i| u[i] * w[i]));
        if (nu - 1.0).abs() < 1e-9
            && (nv - 1.0).abs() < 1e-9
            && dot.abs() < 1e-9
            && d[q + 16] == 0
            && umax > umin
            && vmax > vmin
        {
            let n = [
                u[1] * w[2] - u[2] * w[1],
                u[2] * w[0] - u[0] * w[2],
                u[0] * w[1] - u[1] * w[0],
            ];
            return Some(Geometry {
                geometry_type: Some("Plane".into()),
                origin: Some(o),
                normal: Some(n),
                u_direction: Some(u),
                v_direction: Some(w),
                ..Geometry::default()
            });
        }
        q += 1;
    }
    None
}

/// An occurrence `CE2913AA` (section 12.1): its last component reference
/// (possibly through an assembly context), the transform flag and matrix
/// after it, and its last container (`904F885E`) reference.
#[derive(Clone, Debug, PartialEq)]
pub struct OccurrenceInfo {
    pub object_id: u64,
    pub component: u64,
    /// External document key of a linked component.
    pub context: Option<u64>,
    /// The parent component's child-occurrence container.
    pub container: Option<u64>,
    /// `None`: no transform decoded; `Some(None)`: identity.
    pub matrix: Option<Option<[f64; 16]>>,
}

pub fn occurrence_info(seg: &Segment, id: u64) -> Option<OccurrenceInfo> {
    let d = seg.data_of(id);
    let start = header_end(d)?;
    let refs = seg.refs_in(d, start, d.len());
    let comp = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(COMPONENT))?
        .1;
    let container = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(OCCURRENCE_CONTAINER))
        .map(|(_, r)| r.id);
    let q = comp.end;
    let matrix = match d.get(q) {
        Some(1) => Some(None),
        Some(0) if q + 129 <= d.len() => f64s_at(d, q + 1, 16).and_then(|m| {
            let ok = (m[15] - 1.0).abs() < 1e-12 && m[12].abs() + m[13].abs() + m[14].abs() < 1e-12;
            ok.then(|| {
                let mut a = [0.0; 16];
                a.copy_from_slice(&m);
                Some(a)
            })
        }),
        _ => None,
    };
    Some(OccurrenceInfo {
        object_id: id,
        component: comp.id,
        context: comp.context,
        container,
        matrix,
    })
}

/// The occurrence an occurrence item (mitcad#43) made: through its first
/// [`OCCURRENCE_REF`].
fn item_occurrence(seg: &Segment, item: u64) -> Option<u64> {
    let r = seg
        .ref_ids(seg.data_of(item))
        .into_iter()
        .find(|&r| seg.guid_of(r) == Some(OCCURRENCE_REF))?;
    seg.ref_ids(seg.data_of(r))
        .into_iter()
        .find(|&o| seg.guid_of(o) == Some(OCCURRENCE))
}

/// The component of the occurrence an occurrence item made.
fn occurrence_item_component(seg: &Segment, item: u64) -> Option<u64> {
    occurrence_info(seg, item_occurrence(seg, item)?).map(|i| i.component)
}

/// The name of a timeline group ([`GROUP`]): the `str16` after its list of
/// items.
fn group_name(seg: &Segment, group: u64) -> Option<String> {
    let d = seg.data_of(group);
    let (first, _) = *seg
        .refs_in(d, 0, seg.main_end(seg.object(group)?))
        .first()?;
    let (_, end) = seg.ref_list_at(d, first.checked_sub(4)?)?;
    let (name, _) = str16_at(d, end)?;
    super::unicode::is_printable(&name).then_some(name)
}

fn mat4(m: &Option<[f64; 16]>) -> Mat4 {
    match m {
        None => IDENTITY,
        Some(m) => {
            let mut out = [[0.0; 4]; 4];
            for (r, row) in out.iter_mut().enumerate() {
                for (c, x) in row.iter_mut().enumerate() {
                    *x = m[4 * r + c];
                }
            }
            out
        }
    }
}

/// The component that owns the non-empty timeline: timeline -> feature
/// manager -> component (in the manager's sub-chunk).
fn root_component(seg: &Segment) -> Option<u64> {
    for t in seg.objects_of(TIMELINE) {
        let refs = seg.ref_ids(seg.data(t));
        if refs.len() < 2 || seg.guid_of(refs[0]) != Some(FEATURE_MANAGER) {
            continue;
        }
        if let Some(c) = seg
            .ref_ids(seg.data_of(refs[0]))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(COMPONENT))
        {
            return Some(c);
        }
    }
    None
}

/// The component that owns each timeline item (mitcad#37), by object id:
/// every component has one [`FEATURE_LIST`], which refers to the
/// component's feature manager (which refers to the component) and to the
/// items the component owns. The file has one timeline for the whole
/// design; this tells whose features, sketches and construction geometry
/// each item is (the history's state numbers count per component).
fn item_owners(seg: &Segment) -> HashMap<u64, u64> {
    let mut owners = HashMap::new();
    for list in seg.objects_of(FEATURE_LIST) {
        let refs = seg.ref_ids(seg.data(list));
        let component = refs
            .iter()
            .find(|&&r| seg.guid_of(r) == Some(FEATURE_MANAGER))
            .and_then(|&fm| {
                seg.ref_ids(seg.data_of(fm))
                    .into_iter()
                    .find(|&r| seg.guid_of(r) == Some(COMPONENT))
            });
        let Some(component) = component else {
            continue;
        };
        for r in refs {
            // An item listed by two components has no one owner.
            match owners.get(&r) {
                Some(&c) if c != component => {
                    owners.insert(r, u64::MAX);
                }
                _ => {
                    owners.insert(r, component);
                }
            }
        }
    }
    owners.retain(|_, c| *c != u64::MAX);
    owners
}

/// Names of parameters whose expression mentions `p` (as a name token).
fn dependents(p: &ParameterEntry, params: &[ParameterEntry]) -> Vec<String> {
    params
        .iter()
        .filter(|q| !std::ptr::eq(*q, p) && name_tokens(&q.expression).any(|t| t == p.name))
        .map(|q| q.name.clone())
        .collect()
}

/// `[A-Za-z_][A-Za-z_0-9]*` tokens of an expression, left to right.
fn name_tokens(s: &str) -> impl Iterator<Item = &str> {
    let b = s.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < b.len() {
            if b[i].is_ascii_alphabetic() || b[i] == b'_' {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                return Some(&s[start..i]);
            }
            i += 1;
        }
        None
    })
}

/// The sketch whose object id a profile source (`4BD53E5A`) holds as text.
fn source_sketch(seg: &Segment, src: u64) -> Option<u64> {
    str16s(seg.data_of(src)).into_iter().find_map(|s| {
        if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let id: u64 = s.parse().ok()?;
        (seg.guid_of(id) == Some(SKETCH)).then_some(id)
    })
}

struct Builder<'a> {
    seg: &'a Segment,
    dec: &'a Decoded,
    pos_of: HashMap<u64, usize>,
    name_of: HashMap<u64, Option<String>>,
    sk_of_feat: HashMap<u64, u64>,
    feat_of_sk: HashMap<u64, u64>,
    local_ids: HashMap<u64, (u64, String)>,
    curve_types: HashMap<(u64, String), String>,
    plane_feature: HashMap<u64, u64>,
    /// Axis objects of construction axes, to their feature (mitcad#67).
    axis_feature: HashMap<u64, u64>,
    /// Sketch entities by the ids inputs name them with (mitcad#34).
    tags: sweeps::EntityTags,
    /// Occurrences and components by their GUIDs (mitcad#66).
    guids: joints::Guids,
    /// Component names by object id.
    component_names: HashMap<u64, String>,
}

impl Builder<'_> {
    fn name(&self, f: Option<u64>) -> Option<String> {
        f.and_then(|f| self.name_of.get(&f).cloned().flatten())
    }

    fn pos(&self, f: Option<u64>) -> Option<i64> {
        f.and_then(|f| self.pos_of.get(&f)).map(|&p| p as i64)
    }

    fn feature_ref(&self, fid: u64) -> Reference {
        Reference::Feature(FeatureRef {
            object_type: self
                .seg
                .guid_of(fid)
                .and_then(classes::object_type)
                .map(str::to_string),
            name: Some(self.name(Some(fid))),
            timeline_index: Some(self.pos(Some(fid))),
            ..FeatureRef::default()
        })
    }

    fn plane_ref(&self, entity_ref: u64) -> Option<Reference> {
        let tid = decode::target_id(self.seg, entity_ref)?;
        let on = origin_name(self.seg, tid);
        let geo = if self.seg.guid_of(tid) == Some(ORIGIN_PLANE) {
            plane_geometry(self.seg, tid)
        } else {
            None
        };
        let mut out = if let Some(on) = on {
            ConstructionRef {
                name: Some(Some(on.clone())),
                origin: Some(Some(on)),
                timeline_index: Some(None),
                ..ConstructionRef::default()
            }
        } else if let Some(&f) = self.plane_feature.get(&tid) {
            ConstructionRef {
                name: Some(self.name(Some(f))),
                origin: Some(None),
                timeline_index: Some(self.pos(Some(f))),
                ..ConstructionRef::default()
            }
        } else {
            return None;
        };
        out.geometry = geo;
        Some(Reference::ConstructionPlane(Box::new(out)))
    }

    /// What an entity input names, by its target's kind: a plane, an axis,
    /// the origin point, or a sketch entity (mitcad#66, mitcad#67).
    fn entity_target(&self, entity_ref: u64) -> Option<Reference> {
        let tid = decode::target_id(self.seg, entity_ref)?;
        match self.seg.guid_of(tid) {
            Some(ORIGIN_AXIS) => self.axis_ref(entity_ref),
            Some(ORIGIN_POINT) => Some(Reference::ConstructionPoint(Box::new(ConstructionRef {
                name: Some(Some("Origin".to_owned())),
                origin: Some(Some("Origin".to_owned())),
                timeline_index: Some(None),
                ..ConstructionRef::default()
            }))),
            Some(ORIGIN_PLANE) => self.plane_ref(entity_ref),
            _ => self
                .input_entity(entity_ref)
                .or_else(|| self.axis_ref(entity_ref)),
        }
    }

    fn axis_ref(&self, entity_ref: u64) -> Option<Reference> {
        let tid = decode::target_id(self.seg, entity_ref)?;
        if let Some(on) =
            origin_name(self.seg, tid).filter(|o| matches!(o.as_str(), "X" | "Y" | "Z"))
        {
            return Some(Reference::ConstructionAxis(Box::new(ConstructionRef {
                name: Some(Some(on.clone())),
                origin: Some(Some(on)),
                timeline_index: Some(None),
                ..ConstructionRef::default()
            })));
        }
        // A construction axis (mitcad#67).
        if let Some(&f) = self.axis_feature.get(&tid) {
            return Some(Reference::ConstructionAxis(Box::new(ConstructionRef {
                name: Some(self.name(Some(f))),
                origin: Some(None),
                timeline_index: Some(self.pos(Some(f))),
                ..ConstructionRef::default()
            })));
        }
        let (sid, lid) = self.local_ids.get(&tid)?;
        let f = self.feat_of_sk.get(sid).copied();
        Some(Reference::SketchEntity(SketchEntityRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            id: Some(Some(lid.clone())),
            object_type: self.curve_types.get(&(*sid, lid.clone())).cloned(),
            ..SketchEntityRef::default()
        }))
    }

    /// One profile reference per selected profile, with the sketch from
    /// the profile source (the profile itself is not decoded).
    fn profiles(&self, fid: u64) -> Option<Vec<Reference>> {
        let seg = self.seg;
        let mut sketch_id = None;
        let sources = inputs_of(seg, fid, PROFILE_SOURCE)
            .into_iter()
            .chain(inputs_of(seg, fid, PROFILE_SOURCE_2));
        for src in sources {
            if let Some(sid) = source_sketch(seg, src) {
                sketch_id = Some(sid);
            }
        }
        let count: usize = inputs_of(seg, fid, PROFILE)
            .into_iter()
            .chain(inputs_of(seg, fid, REVOLVE_PROFILE))
            .map(|pr| {
                seg.ref_ids(seg.data_of(pr))
                    .into_iter()
                    .filter(|&r| seg.guid_of(r) == Some(PROFILE_ID))
                    .count()
            })
            .sum();
        let sketch_id = sketch_id?;
        if count == 0 {
            return None;
        }
        let f = self.feat_of_sk.get(&sketch_id).copied();
        let r = Reference::Profile(ProfileRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            ..ProfileRef::default()
        });
        Some(vec![r; count])
    }

    /// The edges of a fillet's or chamfer's sets, the sets given by one
    /// parameter of each (its holder is in the set's input group); `None`
    /// unless every set's edges decode.
    fn set_edges(
        &self,
        it: &TimelineEntry,
        params: &[&ParameterEntry],
    ) -> Option<Vec<Vec<Reference>>> {
        let tail = it.tail.as_ref()?;
        let inputs = tail_inputs(self.seg, self.seg.data_of(it.id), tail.start)?;
        let groups = edge_groups(self.seg, &inputs);
        if params.len() == 1 && groups.len() == 1 {
            return groups[0]
                .0
                .clone()
                .filter(|e| !e.is_empty())
                .map(|e| vec![e]);
        }
        params
            .iter()
            .map(|p| {
                let h = p.holder?;
                let g = groups.iter().find(|g| g.1.contains(&h))?;
                g.0.clone().filter(|e| !e.is_empty())
            })
            .collect()
    }

    /// An item's input list (the common tail's).
    fn item_inputs(&self, it: &TimelineEntry) -> Vec<u64> {
        it.tail
            .as_ref()
            .and_then(|t| tail_inputs(self.seg, self.seg.data_of(it.id), t.start))
            .into_iter()
            .flatten()
            .flatten()
            .collect()
    }

    /// The bodies an item's body inputs name, in input order; `None`
    /// unless there are some and every one decodes.
    fn body_inputs(&self, it: &TimelineEntry) -> Option<Vec<Reference>> {
        let ids: Vec<u64> = self
            .item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(BODY_REF))
            .collect();
        if ids.is_empty() {
            return None;
        }
        ids.into_iter().map(|i| self.body_input(i)).collect()
    }

    /// The plane of an item's first plane input (origin or construction
    /// plane).
    fn input_plane(&self, it: &TimelineEntry) -> Option<Reference> {
        self.item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(ENTITY_REF))
            .find_map(|e| self.plane_ref(e))
    }

    /// The object an extrusion's side up to an object goes to, from the
    /// item's input slot for it ([`decode::extent_slots`], mitcad#96): a
    /// face (by its names) or a plane (role 5 with an entity input: the
    /// reference models' writer). A side up to an object is the first
    /// side, which the extent parameters (`Side1Offset`) make an extent up
    /// to an entity; without them it is one now, without an offset.
    ///
    /// A two-sided extrusion has a slot for each side up to an object
    /// (mitcad#104): they go, in order, to the sides the extent parameters
    /// make extents up to an entity (`Side1Offset`, `Side2Offset`).
    fn extent_objects(&self, fid: u64, x: &mut ExtrudeDetail) {
        let seg = self.seg;
        let inputs: Vec<u64> = decode::extent_slots(seg, fid)
            .into_iter()
            .filter_map(|s| {
                let input = s.inputs.first().copied()?;
                let object = s.is_to_object()
                    || (s.role == 5 && seg.guid_of(input).is_some_and(|g| g != BODY_REF));
                object.then_some(input)
            })
            .collect();
        let to_entity = |d: &Option<Definition>| {
            d.as_ref().and_then(|d| d.definition_type.as_deref())
                == Some("ToEntityExtentDefinition")
        };
        let mut sides = Vec::new();
        if to_entity(&x.extent_one) {
            sides.push(1);
        }
        if to_entity(&x.extent_two) {
            sides.push(2);
        }
        if sides.is_empty() {
            sides.push(1);
        }
        for (input, side) in inputs.into_iter().zip(sides) {
            let entity = match seg.guid_of(input) {
                Some(FACE_REF) => face_input(seg, input),
                Some(ENTITY_REF) => self.entity_target(input),
                Some(BODY_REF) => self.body_input(input),
                _ => None,
            };
            let Some(entity) = entity else { continue };
            let def = if side == 1 {
                &mut x.extent_one
            } else {
                &mut x.extent_two
            };
            let def = def.get_or_insert_with(|| Definition {
                definition_type: Some("ToEntityExtentDefinition".into()),
                ..Definition::default()
            });
            if def.definition_type.as_deref() == Some("ToEntityExtentDefinition") {
                def.entity = Some(entity);
            }
        }
    }

    fn roles(&self, fid: u64) -> BTreeMap<&str, Vec<&ParameterEntry>> {
        let mut out: BTreeMap<&str, Vec<&ParameterEntry>> = BTreeMap::new();
        for p in &self.dec.parameters {
            if p.owner_feature == Some(fid) {
                out.entry(p.role.as_str()).or_default().push(p);
            }
        }
        out
    }

    fn detail(&self, it: &TimelineEntry, sketches: &HashMap<u64, &SketchInfo>) -> Detail {
        let seg = self.seg;
        let fid = it.id;
        let rl = self.roles(fid);
        let pr = |role: &str| rl.get(role).and_then(|v| v.first()).map(|p| paramref(p));
        let def = |t: &str, f: fn(&mut Definition, Reference), p: Reference| {
            let mut d = Definition {
                definition_type: Some(t.into()),
                ..Definition::default()
            };
            f(&mut d, p);
            d
        };
        let distance = |d: &mut Definition, p| d.distance = Some(p);
        let offset = |d: &mut Definition, p| d.offset = Some(p);
        let angle = |d: &mut Definition, p| d.angle = Some(p);
        match it.type_name {
            Some("SketchFeature") if self.sk_of_feat.contains_key(&fid) => {
                let sid = self.sk_of_feat[&fid];
                let mut sd = sketches
                    .get(&sid)
                    .map(|s| s.detail.clone())
                    .unwrap_or_default();
                sd.name = Some(it.name.clone());
                if let Some(e) = inputs_of(seg, fid, ENTITY_REF).first() {
                    sd.reference_plane = self.plane_ref(*e);
                }
                if sd.reference_plane.is_none() {
                    // A sketch on a profile of another sketch.
                    for src in inputs_of(seg, fid, PROFILE_SOURCE) {
                        if let Some(s2) = source_sketch(seg, src)
                            && s2 != sid
                        {
                            let f = self.feat_of_sk.get(&s2).copied();
                            sd.reference_plane = Some(Reference::Profile(ProfileRef {
                                sketch: Some(self.name(f)),
                                sketch_timeline_index: Some(self.pos(f)),
                                ..ProfileRef::default()
                            }));
                            break;
                        }
                    }
                }
                if sd.reference_plane.is_none() && !inputs_of(seg, fid, FACE_REF).is_empty() {
                    sd.reference_plane = Some(Reference::Face(Box::default()));
                }
                Detail::Sketch(Box::new(sd))
            }
            Some("ExtrudeFeature") => {
                let mut x = ExtrudeDetail::default();
                if let Some(ex) = &it.extrude {
                    x.operation = match ex.operation_code {
                        1 => Some("JoinFeatureOperation".into()),
                        2 => Some("CutFeatureOperation".into()),
                        4 => Some("NewBodyFeatureOperation".into()),
                        _ => None,
                    };
                    x.extent_type = match ex.extent_a {
                        1 => Some("OneSideFeatureExtentType".into()),
                        2 => Some("TwoSidesFeatureExtentType".into()),
                        _ => None,
                    };
                }
                if let Some(p) = pr("AlongDistance") {
                    x.extent_one = Some(def("DistanceExtentDefinition", distance, p));
                } else if let Some(p) = pr("Side1Offset") {
                    x.extent_one = Some(def("ToEntityExtentDefinition", offset, p));
                }
                if let Some(p) = pr("AgainstDistance") {
                    x.extent_two = Some(def("DistanceExtentDefinition", distance, p));
                } else if let Some(p) = pr("Side2Offset") {
                    // The second side up to an object (mitcad#104).
                    x.extent_two = Some(def("ToEntityExtentDefinition", offset, p));
                }
                x.taper_angle_one = pr("TaperAngle");
                x.taper_angle_two = pr("Side2TaperAngle");
                if let Some(p) = pr("ProfileOffset") {
                    x.start_extent = Some(def("OffsetStartDefinition", offset, p));
                }
                x.profile = self.profiles(fid);
                // The selected profiles' loops (mitcad#96).
                if let Some(loops) = self.profile_loops(fid) {
                    x.other.insert("_f3d_profile_loops".to_owned(), loops);
                }
                // The bodies a join or cut works on: its body inputs
                // (mitcad#96).
                if matches!(
                    x.operation.as_deref(),
                    Some("JoinFeatureOperation" | "CutFeatureOperation")
                ) {
                    x.participant_bodies = self.body_inputs(it);
                }
                self.extent_objects(fid, &mut x);
                Detail::Extrude(Box::new(x))
            }
            Some("RevolveFeature") => {
                let mut x = RevolveDetail::default();
                if let Some(p) = pr("AlongAngle") {
                    x.extent_definition = Some(def("AngleExtentDefinition", angle, p));
                }
                // The operation is the first `u32` after the root part,
                // with an extrusion's codes; the axis a sketch line (by its
                // curve's ids, as sweeps name them), else an origin or
                // construction axis (mitcad#96).
                let d = seg.data_of(fid);
                x.operation = seg
                    .root_part(d)
                    .and_then(|r| u32_at(d, r.end))
                    .and_then(|code| match code {
                        1 => Some("JoinFeatureOperation".into()),
                        2 => Some("CutFeatureOperation".into()),
                        4 => Some("NewBodyFeatureOperation".into()),
                        _ => None,
                    });
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(a) = self.input_entity(e).or_else(|| self.axis_ref(e)) {
                        x.axis = Some(a);
                    }
                }
                x.profile = self.profiles(fid);
                if let Some(loops) = self.profile_loops(fid) {
                    x.other.insert("_f3d_profile_loops".to_owned(), loops);
                }
                Detail::Revolve(Box::new(x))
            }
            Some("FilletFeature") => {
                let mut x = FilletDetail::default();
                if let Some(radii) = rl.get("Radius").filter(|r| !r.is_empty()) {
                    let mut edges = self.set_edges(it, radii).map(Vec::into_iter);
                    x.edge_sets = Some(
                        radii
                            .iter()
                            .map(|p| EdgeSet {
                                radius: Some(paramref(p)),
                                edges: edges.as_mut().and_then(Iterator::next),
                                ..EdgeSet::default()
                            })
                            .collect(),
                    );
                }
                Detail::Fillet(Box::new(x))
            }
            Some("ChamferFeature") => {
                let mut x = ChamferDetail::default();
                if rl.contains_key("Distance 1") && rl.contains_key("Distance 2") {
                    x.chamfer_type = Some("TwoDistancesChamferType".into());
                    x.edge_sets = Some(vec![EdgeSet {
                        distance_one: pr("Distance 1"),
                        distance_two: pr("Distance 2"),
                        ..EdgeSet::default()
                    }]);
                } else if rl.contains_key("Rotate Angle") && rl.contains_key("Distance") {
                    x.chamfer_type = Some("DistanceAndAngleChamferType".into());
                    x.edge_sets = Some(vec![EdgeSet {
                        distance: pr("Distance"),
                        angle: pr("Rotate Angle"),
                        ..EdgeSet::default()
                    }]);
                } else if let Some(ds) = rl.get("Distance").filter(|d| !d.is_empty()) {
                    x.chamfer_type = Some("EqualDistanceChamferType".into());
                    x.edge_sets = Some(
                        ds.iter()
                            .map(|p| EdgeSet {
                                distance: Some(paramref(p)),
                                ..EdgeSet::default()
                            })
                            .collect(),
                    );
                }
                // The edges of each set, by a parameter of the set.
                let set_params: Option<Vec<&ParameterEntry>> = match x.chamfer_type.as_deref() {
                    Some("TwoDistancesChamferType") => rl
                        .get("Distance 1")
                        .map(|v| v.iter().take(1).copied().collect()),
                    Some(_) => rl.get("Distance").map(|v| match x.edge_sets.as_ref() {
                        Some(s) if s.len() == 1 => v.iter().take(1).copied().collect(),
                        _ => v.clone(),
                    }),
                    None => None,
                };
                if let (Some(params), Some(sets)) = (set_params, x.edge_sets.as_mut())
                    && params.len() == sets.len()
                    && let Some(edges) = self.set_edges(it, &params)
                {
                    for (s, e) in sets.iter_mut().zip(edges) {
                        s.edges = Some(e);
                    }
                }
                Detail::Chamfer(Box::new(x))
            }
            Some("HoleFeature") => {
                let mut x = HoleDetail {
                    hole_diameter: pr("HoleDiameter"),
                    tip_angle: pr("TipAngle"),
                    countersink_diameter: pr("CSDiameter"),
                    countersink_angle: pr("CSAngle"),
                    counterbore_diameter: pr("CBDiameter"),
                    counterbore_depth: pr("CBDepth"),
                    extent_definition: pr("HoleDepth")
                        .map(|p| def("DistanceExtentDefinition", distance, p)),
                    ..HoleDetail::default()
                };
                // Type, points and extent (mitcad#67).
                x.hole_type = Some(
                    if x.counterbore_diameter.is_some() {
                        "CounterboreHoleType"
                    } else if x.countersink_diameter.is_some() {
                        "CountersinkHoleType"
                    } else {
                        "SimpleHoleType"
                    }
                    .to_owned(),
                );
                let points = self.hole_points(it);
                if let Some(&first) = points.first() {
                    x.position = Some(first);
                    x.other
                        .insert("_f3d_positions".to_owned(), json_value(&points));
                }
                let through_all = self.hole_through_all(it);
                if x.extent_definition.is_none() && through_all == Some(true) {
                    x.extent_definition = Some(Definition {
                        definition_type: Some("AllExtentDefinition".into()),
                        ..Definition::default()
                    });
                } else if through_all == Some(true) {
                    // The depth is then the one kept for a distance extent:
                    // a lead the import tries first (mitcad#96).
                    x.other
                        .insert("_f3d_through_all".to_owned(), serde_json::json!(true));
                }
                // A tapped hole's thread is its sub-item (mitcad#35); its
                // depth is the thread's length.
                let thread = it
                    .tail
                    .iter()
                    .flat_map(|t| t.sub_features.iter().flatten())
                    .find_map(|&s| Some((s, thread_fields(seg, s)?)));
                if let Some((sub, t)) = thread {
                    let roles = self.roles(sub);
                    let sub_param =
                        |role: &str| roles.get(role).and_then(|v| v.first()).map(|p| paramref(p));
                    let mut feature = serde_json::json!({
                        "_type": "ThreadFeature",
                        "isModeled": t.modeled,
                        "isFullLength": t.full_length,
                    });
                    if let Some(l) = t.location() {
                        feature["threadLocation"] = l.into();
                    }
                    if let Some(p) = sub_param("ThreadDepth") {
                        feature["threadLength"] = json_value(&p);
                    }
                    if let Some(p) = sub_param("ThreadOffset") {
                        feature["threadOffset"] = json_value(&p);
                    }
                    x.hole_tap_type = Some("TappedHoleTapType".to_owned());
                    x.tapped_hole_info = Some(t.info());
                    x.thread = Some(feature);
                }
                Detail::Hole(Box::new(x))
            }
            Some("ThreadFeature") => {
                let mut x = ThreadDetail {
                    thread_length: pr("ThreadLength"),
                    thread_offset: pr("ThreadOffset"),
                    ..ThreadDetail::default()
                };
                // The thread and its faces (mitcad#35).
                if let Some(t) = thread_fields(seg, fid) {
                    x.thread_info = Some(t.info());
                    x.is_modeled = Some(t.modeled);
                    x.is_full_length = Some(t.full_length);
                    x.thread_location = t.location().map(str::to_owned);
                }
                let faces: Option<Vec<Reference>> = self
                    .item_inputs(it)
                    .into_iter()
                    .filter(|&i| seg.guid_of(i) == Some(FACE_REF))
                    .map(|i| face_input(seg, i))
                    .collect();
                x.input_cylindrical_faces = faces.filter(|f| !f.is_empty());
                Detail::Thread(Box::new(x))
            }
            Some("CircularPatternFeature") => {
                let mut x = CircularPatternDetail {
                    total_angle: pr("TotalAngle"),
                    ..CircularPatternDetail::default()
                };
                for (slot, value) in int_inputs(seg, fid) {
                    if slot == 0 {
                        x.quantity = Some(count_ref(value));
                    }
                }
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(a) = self.axis_ref(e) {
                        x.axis = Some(a);
                    }
                }
                // An edge or face as the axis (mitcad#67).
                let first = self.directions(it).into_iter().next();
                if x.axis.is_none() {
                    x.axis = first.as_ref().and_then(|d| d.0.clone());
                }
                // The axis' line, stored with it (mitcad#96): the geometry
                // of a construction axis, and a fixed line for the import
                // when the axis' entity is not found.
                if let Some((_, Some(direction), Some(point))) = first {
                    if let Some(Reference::ConstructionAxis(c)) = x.axis.as_mut()
                        && c.origin.clone().flatten().is_none()
                        && c.geometry.is_none()
                    {
                        c.geometry = Some(Geometry {
                            geometry_type: Some("InfiniteLine3D".into()),
                            origin: Some(point),
                            direction: Some(direction),
                            ..Geometry::default()
                        });
                    }
                    x.other.insert(
                        "_f3d_axis".to_owned(),
                        serde_json::json!({"origin": point, "direction": direction}),
                    );
                }
                // Patterned bodies (mitcad#33), features and faces (mitcad#67).
                match self.pattern_objects(it) {
                    Some((objects, kind)) => {
                        x.input_entities = Some(objects);
                        x.pattern_entity_type = Some(kind);
                    }
                    None => x.input_entities = self.body_inputs(it),
                }
                Detail::CircularPattern(Box::new(x))
            }
            Some("RectangularPatternFeature") => {
                let mut x = RectangularPatternDetail {
                    distance_one: pr("uSpaceDistance"),
                    distance_two: pr("vSpaceDistance"),
                    ..RectangularPatternDetail::default()
                };
                for (slot, value) in int_inputs(seg, fid) {
                    match slot {
                        0 => x.quantity_one = Some(count_ref(value)),
                        1 => x.quantity_two = Some(count_ref(value)),
                        _ => {}
                    }
                }
                match self.pattern_objects(it) {
                    Some((objects, kind)) => {
                        x.input_entities = Some(objects);
                        x.pattern_entity_type = Some(kind);
                    }
                    None => x.input_entities = self.body_inputs(it),
                }
                // The directions (mitcad#67): the axis, edge or face each
                // names, and its vector where one is stored.
                let mut dirs = self.directions(it).into_iter();
                if let Some((entity, vector, _)) = dirs.next() {
                    x.direction_one_entity = entity;
                    x.direction_one = vector;
                }
                if let Some((entity, vector, _)) = dirs.next() {
                    x.direction_two_entity = entity;
                    x.direction_two = vector;
                }
                Detail::RectangularPattern(Box::new(x))
            }
            Some("MirrorFeature") => {
                // Objects and plane from its selections (mitcad#67).
                let mut map = selections::mirror_map(self, it);
                // Whether bodies are joined with their images: none of the
                // bodies it makes is new (mitcad#96).
                let combine = self.new_bodies(it).map(|n| n == 0);
                if map.contains_key("mirrorPlane") && map.contains_key("inputEntities") {
                    if let Some(c) = combine {
                        map.insert("isCombine".to_owned(), serde_json::json!(c));
                    }
                    return Detail::Other(map);
                }
                // The mirror plane when it is an origin or construction
                // plane (the entity reference, as for sketches).
                let mut map = serde_json::Map::new();
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(p) = self.plane_ref(e) {
                        map.insert(
                            "mirrorPlane".to_owned(),
                            serde_json::to_value(p).expect("serializes"),
                        );
                    }
                }
                // Mirrored bodies (mitcad#33).
                if let Some(b) = self.body_inputs(it) {
                    map.insert("inputEntities".to_owned(), json_value(&b));
                }
                if let Some(c) = combine {
                    map.insert("isCombine".to_owned(), serde_json::json!(c));
                }
                Detail::Other(map)
            }
            // Moved bodies and the move's transform (mitcad#33): the
            // transform is the rigid 4x4 matrix (row-major, cm) of one of
            // its inputs *(the reference models' moves: translations,
            // rotations, free moves)*.
            Some("MoveFeature") => {
                let mut map = serde_json::Map::new();
                if let Some(b) = self.body_inputs(it) {
                    map.insert("inputEntities".to_owned(), json_value(&b));
                }
                let matrix = self
                    .item_inputs(it)
                    .into_iter()
                    .filter(|&i| {
                        !matches!(
                            seg.guid_of(i),
                            Some(BODY_INPUT | BODY_REF | PARAMETER_HOLDER | ENTITY_REF | FACE_REF)
                        )
                    })
                    .find_map(|i| rigid_matrix(seg.data_of(i)));
                if let Some(m) = matrix {
                    map.insert("transform".to_owned(), json_value(&m));
                }
                Detail::Other(map)
            }
            // Split bodies and an origin or construction plane as the tool
            // (mitcad#33).
            Some("SplitBodyFeature") => {
                // Tool and split bodies from its selections (mitcad#67).
                if let Detail::Other(map) = self.split(it)
                    && map.contains_key("splitBodies")
                {
                    return Detail::Other(map);
                }
                let mut map = serde_json::Map::new();
                if let Some(b) = self.body_inputs(it) {
                    map.insert("splitBodies".to_owned(), json_value(&b));
                }
                if let Some(p) = self.input_plane(it) {
                    map.insert("splittingTool".to_owned(), json_value(&p));
                }
                Detail::Other(map)
            }
            // Operation, target and tools (mitcad#67).
            Some("CombineFeature") => self.combine(it),
            Some("ShellFeature") => Detail::Shell(Box::new(ShellDetail {
                inside_thickness: pr("innerThickness"),
                ..ShellDetail::default()
            })),
            Some("OffsetFacesFeature") => Detail::OffsetFaces(Box::new(OffsetFacesDetail {
                distance: pr("distance"),
                ..OffsetFacesDetail::default()
            })),
            Some("CoilFeature") => Detail::Coil(Box::new(CoilDetail {
                diameter: pr("Diameter"),
                pitch: pr("Pitch"),
                revolutions: pr("Revolutions"),
                section_size: pr("SectionSize"),
                angle: pr("TaperAngle"),
                ..CoilDetail::default()
            })),
            Some("PipeFeature") => {
                let mut x = PipeDetail {
                    section_size: pr("SectionSize"),
                    section_thickness: pr("SectionThickness"),
                    distance_one: pr("AlongDistance"),
                    distance_two: pr("AgainstDistance"),
                    ..PipeDetail::default()
                };
                // Operation and path (mitcad#34).
                self.pipe_inputs(it, &mut x);
                Detail::Pipe(Box::new(x))
            }
            Some("SweepFeature") => {
                let mut x = SweepDetail {
                    distance_one: pr("AlongDistance"),
                    distance_two: pr("AgainstDistance"),
                    taper_angle: pr("TaperAngle"),
                    twist_angle: pr("TwistAngle"),
                    ..SweepDetail::default()
                };
                // Operation, profiles, path and rail (mitcad#34).
                self.sweep_inputs(it, &mut x);
                Detail::Sweep(Box::new(x))
            }
            // Sections, conditions, centre line or rails (mitcad#34).
            None if it.class == LOFT => self.loft(it),
            Some("ConstructionPlane") => {
                let mut x = ConstructionPlaneDetail::default();
                if let Some(p) = pr("AlongDistance") {
                    x.definition = Some(def("ConstructionPlaneOffsetDefinition", offset, p));
                } else if let Some(p) = pr("RotateAngle") {
                    x.definition = Some(def("ConstructionPlaneAtAngleDefinition", angle, p));
                } else if let Some(p) = pr("PathDistance") {
                    x.definition = Some(def(
                        "ConstructionPlaneDistanceOnPathDefinition",
                        distance,
                        p,
                    ));
                }
                if let Some(r) = seg
                    .ref_ids(seg.data_of(fid))
                    .into_iter()
                    .find(|&r| seg.guid_of(r) == Some(ORIGIN_PLANE))
                {
                    x.geometry = plane_geometry(seg, r);
                }
                Detail::ConstructionPlane(Box::new(x))
            }
            Some("JointOriginFeature") => Detail::JointOrigin(Box::new(JointOriginDetail {
                offset_x: pr("OffsetX"),
                offset_y: pr("OffsetY"),
                offset_z: pr("OffsetZ"),
                angle: pr("AngleZ"),
                // Its frame and the entities it was built on (mitcad#66).
                geometry: self.joint_origin_geometry(fid),
                ..JointOriginDetail::default()
            })),
            // What joints, as-built joints and ground items say (mitcad#66).
            Some("JointFeature") => self.joint(it),
            Some("AsBuiltJointFeature") => self.as_built_joint(it),
            None if it.class == GROUND_OCCURRENCE => self.ground(it),
            // The placements a captured position puts occurrences at
            // (mitcad#75).
            Some("SnapshotFeature") => self.snapshot(it),
            // The occurrence an insert, a fastener or a paste made
            // (mitcad#75).
            _ if seg.is_kind_of(fid, OCCURRENCE_ITEM) => self.occurrence_item(it),
            _ => Detail::empty(),
        }
    }
}

/// The sketch dimension that a parameter holder belongs to.
struct DimensionOwner {
    /// The sketch feature.
    feature: Option<u64>,
    /// Sketch-local id (`d<i>`).
    id: Option<String>,
    dimension_type: Option<String>,
}

/// Builds the SCHEMA.md dump of a decoded design segment. `file` and
/// `segment_dir` go to `source`.
pub fn build(seg: &Segment, dec: &Decoded, file: &str, segment_dir: &str) -> Dump {
    let items = &dec.timeline;
    let mut b = Builder {
        seg,
        dec,
        pos_of: items.iter().map(|it| (it.id, it.pos)).collect(),
        name_of: items.iter().map(|it| (it.id, it.name.clone())).collect(),
        sk_of_feat: HashMap::new(),
        feat_of_sk: HashMap::new(),
        local_ids: HashMap::new(),
        curve_types: HashMap::new(),
        plane_feature: HashMap::new(),
        axis_feature: HashMap::new(),
        tags: sweeps::EntityTags::default(),
        guids: joints::Guids::new(seg),
        component_names: dec
            .components
            .iter()
            .filter_map(|c| Some((c.id, c.name.clone().filter(|n| !n.is_empty())?)))
            .collect(),
    };
    for s in seg.objects_of(SKETCH) {
        for r in seg.ref_ids(seg.data(s)) {
            if seg.guid_of(r) == Some(SKETCH_FEATURE) {
                b.sk_of_feat.insert(r, s.id);
                b.feat_of_sk.insert(s.id, r);
            }
        }
    }
    let (mut sketch_infos, local_ids) = sketch::sketch_details(seg);
    b.tags = sweeps::EntityTags::new(seg, &local_ids);
    b.local_ids = local_ids;
    for s in &sketch_infos {
        for c in s.detail.curves.iter().flatten() {
            if let (Some(id), Some(t)) = (&c.id, &c.curve_type) {
                b.curve_types.insert((s.id, id.clone()), t.clone());
            }
        }
    }
    for f in seg.objects_of(CONSTRUCTION_PLANE) {
        for r in seg.ref_ids(seg.data(f)) {
            if seg.guid_of(r) == Some(ORIGIN_PLANE) {
                b.plane_feature.insert(r, f.id);
            }
        }
    }
    for f in seg.objects_of(CONSTRUCTION_AXIS) {
        for r in seg.ref_ids(seg.data(f)) {
            if seg.guid_of(r) == Some(ORIGIN_AXIS) {
                b.axis_feature.entry(r).or_insert(f.id);
            }
        }
    }

    // Dimension parameters: holder -> (sketch feature, dimension id, type).
    let mut dim_of_holder: HashMap<u64, DimensionOwner> = HashMap::new();
    for s in &sketch_infos {
        let f = b.feat_of_sk.get(&s.id).copied();
        for (dm, h) in s.detail.dimensions.iter().flatten().zip(&s.holders) {
            if let Some(h) = h {
                let owner = DimensionOwner {
                    feature: f,
                    id: dm.id.clone(),
                    dimension_type: dm.dimension_type.clone(),
                };
                dim_of_holder.insert(*h, owner);
            }
        }
    }
    let mut param_of_holder = HashMap::new();
    for p in &dec.parameters {
        if let Some(h) = p.holder {
            param_of_holder.insert(h, p);
        }
    }
    for s in &mut sketch_infos {
        let holders = s.holders.clone();
        for (dm, h) in s.detail.dimensions.iter_mut().flatten().zip(holders) {
            if let Some(p) = h.and_then(|h| param_of_holder.get(&h)) {
                dm.parameter = Some(Some(paramref(p)));
                if p.is_driven() {
                    dm.is_driving = Some(false);
                }
            }
        }
        // Pattern constraints' quantities, angles and distances.
        let holders = s.constraint_holders.clone();
        let Some(constraints) = s.detail.constraints.as_mut() else {
            continue;
        };
        for (i, name, h) in holders {
            if let (Some(p), Some(props)) = (
                param_of_holder.get(&h),
                constraints.get_mut(i).and_then(|c| c.props.as_mut()),
            ) {
                props.insert(name.to_owned(), json_value(&paramref(p)));
            }
        }
    }
    let sketches: HashMap<u64, &SketchInfo> = sketch_infos.iter().map(|s| (s.id, s)).collect();

    let component_names: HashMap<u64, String> = dec
        .components
        .iter()
        .filter_map(|c| Some((c.id, c.name.clone().filter(|n| !n.is_empty())?)))
        .collect();
    let owners = item_owners(seg);
    let mut tl_items = Vec::new();
    for it in items {
        let owner = owners.get(&it.id).copied();
        let mut f3d = ItemF3d {
            class: Some(it.class.clone()),
            class_version: Some(it.class_version),
            object_id: Some(it.id),
            component: owner,
            ..ItemF3d::default()
        };
        if let Some(t) = &it.tail {
            f3d.base_name = Some(t.base_name.clone());
            f3d.index = Some(t.index);
            f3d.custom_name = Some(t.custom_name.clone());
            f3d.result_no = Some(t.result_no);
            f3d.flags = Some(super::stream::hex(&t.flags));
            f3d.sub_features = Some(t.sub_features.clone());
        }
        if let Some(ex) = &it.extrude {
            f3d.extrude = Some(ExtrudeF3d {
                operation_code: Some(ex.operation_code),
                operation: Some(ex.operation().map(str::to_string)),
                extent_a: Some(ex.extent_a),
                extent_b: Some(ex.extent_b),
                direction: Some(ex.direction),
                direction_vector: ex.vector,
                flag: ex.flag.map(u32::from),
                full_length: ex.full_length,
                slot_roles: Some(
                    decode::extent_slots(seg, it.id)
                        .iter()
                        .map(|s| s.role)
                        .collect(),
                ),
                ..ExtrudeF3d::default()
            });
        }
        // The light bulb (mitcad#6), where it is decoded; beyond the
        // reference decoder, so checked against the reference dumps of the
        // reference models instead (tests/design_models.rs).
        let bulb = match it.type_name {
            Some("SketchFeature") => b
                .sk_of_feat
                .get(&it.id)
                .and_then(|s| seg.object(*s))
                .and_then(|s| sketch::light_bulb(seg, s)),
            Some("ConstructionPlane") => plane_light_bulb(seg, it.id),
            _ => None,
        };
        let props = bulb.map(|bulb| {
            f3d.light_bulb = Some(hex(&bulb.raw));
            serde_json::json!({"isLightBulbOn": bulb.on})
        });
        // Items without a name of their own: occurrences are named after
        // their component, groups by their own name (mitcad#43).
        let object_type = match classes::object_type(&it.class) {
            // An as-built joint of the rigid group type (mitcad#81).
            Some("AsBuiltJoint") if b.is_rigid_group(it.id) => Some("RigidGroup"),
            t => t,
        };
        let name = match &it.name {
            Some(n) if n.is_empty() && object_type == Some("Occurrence") => {
                occurrence_item_component(seg, it.id)
                    .and_then(|c| component_names.get(&c).cloned())
                    .or(Some(String::new()))
            }
            None if it.class == GROUP => group_name(seg, it.id).filter(|n| !n.is_empty()),
            n => n.clone(),
        };
        tl_items.push(TimelineItem {
            index: Some(it.pos as i64),
            name: Some(name),
            object_type: object_type.map(|t| Some(t.to_string())),
            component: owner
                .and_then(|c| component_names.get(&c))
                .map(|n| Some(n.clone())),
            detail: Some(b.detail(it, &sketches)),
            props,
            f3d: Some(f3d),
            ..TimelineItem::default()
        });
    }

    // Parameters: user and model, createdBy for model parameters.
    let (mut user, mut model) = (Vec::new(), Vec::new());
    for p in &dec.parameters {
        let mut rec = Parameter {
            name: Some(p.name.clone()),
            expression: Some(p.expression.clone()),
            value: Some(p.value),
            unit: Some(p.unit.clone()),
            comment: p.comment.clone(),
            dependents: Some(dependents(p, &dec.parameters)),
            ..Parameter::default()
        };
        let Some(h) = p.holder else {
            user.push(rec);
            continue;
        };
        rec.role = Some(p.role.clone());
        if let Some(DimensionOwner {
            feature: f,
            id,
            dimension_type: typ,
        }) = dim_of_holder.get(&h)
        {
            rec.created_by = Some(Reference::SketchDimension(SketchEntityRef {
                sketch: Some(b.name(*f)),
                sketch_timeline_index: Some(b.pos(*f)),
                id: Some(id.clone()),
                object_type: typ.clone(),
                ..SketchEntityRef::default()
            }));
        } else if let Some(f) = p.owner_feature {
            rec.created_by = Some(b.feature_ref(f));
        }
        model.push(rec);
    }

    // Components and occurrences.
    let comps = &dec.components;
    let mut root_id = root_component(seg);
    let infos: Vec<OccurrenceInfo> = seg
        .objects_of(OCCURRENCE)
        .filter_map(|o| occurrence_info(seg, o.id))
        .collect();
    if root_id.is_none() {
        // No parametric timeline: the root occurrence names the root component.
        root_id = infos
            .iter()
            .find(|i| i.container.is_none() && i.context.is_none())
            .map(|i| i.component);
    }
    let root = comps
        .iter()
        .find(|c| Some(c.id) == root_id)
        .and_then(|c| c.name.clone())
        .filter(|n| !n.is_empty());
    let xrefs: HashMap<String, String> = seg
        .meta
        .external
        .iter()
        .map(|x| (format!("{:016x}", x.key), x.urn.clone()))
        .collect();
    let comp_names: HashMap<u64, Option<String>> =
        comps.iter().map(|c| (c.id, c.name.clone())).collect();
    let mut occurrences = occurrence_tree(seg, &infos, &comp_names, &xrefs, root_id);
    // The occurrences ground items ground (mitcad#66).
    let grounded = joints::grounded(&tl_items);
    if let Some(nodes) = occurrences.as_mut() {
        ground(nodes, &grounded);
        placements::Histories::new(seg, root_id).attach(nodes, &b.pos_of);
    }

    let format = Format {
        meta_magic: Some(seg.meta.magic),
        meta_version: Some(seg.meta.version.clone()),
        writer_build: Some(seg.meta.writer_build()),
        writer: Some(seg.meta.writer()),
        long_refs: Some(seg.long_refs),
        classes: Some(seg.meta.classes.len() as u64),
        objects: Some(seg.objects.len() as u64),
        bulk_bytes: Some(seg.bulk.len() as u64),
        ..Format::default()
    };
    let cv = &dec.coverage;
    let f3d = DumpF3d {
        format: Some(format),
        next_object_id: Some(seg.meta.next_id),
        external_documents: Some(
            seg.meta
                .external
                .iter()
                .map(|x| ExternalDocument {
                    key: Some(format!("{:016x}", x.key)),
                    urn: Some(x.urn.clone()),
                    ..ExternalDocument::default()
                })
                .collect(),
        ),
        feature_set_versions: Some(seg.meta.feature_versions.clone()),
        brep_blobs: Some(
            dec.brep_blobs
                .iter()
                .map(|b| BrepBlob {
                    id: Some(b.id),
                    file: Some(b.file.clone()),
                    holder: b.holder,
                    component: b.holder.map(|_| b.component),
                    ..BrepBlob::default()
                })
                .collect(),
        ),
        coverage: Some(Coverage {
            bulk_bytes: Some(cv.bulk_bytes),
            framing_bytes: Some(cv.framing_bytes),
            root_part_ok: Some(cv.root_part_ok),
            objects: Some(cv.objects),
            known_class_bytes: Some(cv.known_class_bytes),
            decoded_bytes: Some(cv.decoded_bytes),
            token_bytes: Some(cv.token_bytes),
            ..Coverage::default()
        }),
        timelines_total: Some(dec.timelines_total as u64),
        parameter_failures: Some(dec.parameter_failures as u64),
        ..DumpF3d::default()
    };

    let units = [
        ("length", "cm"),
        ("angle", "rad"),
        ("area", "cm^2"),
        ("volume", "cm^3"),
        ("mass", "kg"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mut dump = Dump {
        schema: Some(SCHEMA_NAME.into()),
        schema_version: Some(SCHEMA_VERSION),
        generator: Some(Generator {
            decoder: Some(DECODER_NAME.into()),
            writer: Some(seg.meta.writer()),
            ..Generator::default()
        }),
        units: Some(units),
        source: Some(Source {
            mode: Some("f3d_stream".into()),
            file: Some(file.into()),
            segment: Some(segment_dir.into()),
            ..Source::default()
        }),
        document: Some(Document {
            design_type: Some(if items.is_empty() {
                "DirectDesignType".into()
            } else {
                "ParametricDesignType".into()
            }),
            component_count: Some(comps.len() as i64),
            root_component: root,
            ..Document::default()
        }),
        parameters: Some(Parameters {
            user: Some(user),
            model: Some(model),
            ..Parameters::default()
        }),
        components: Some(
            comps
                .iter()
                .map(|c| Component {
                    name: Some(c.name.clone()),
                    is_root: root_id.filter(|&r| r != 0).map(|r| r == c.id),
                    f3d: Some(ComponentF3d {
                        object_id: Some(c.id),
                        ..ComponentF3d::default()
                    }),
                    ..Component::default()
                })
                .collect(),
        ),
        occurrences,
        timeline: Some(Timeline {
            available: Some(!items.is_empty()),
            count: Some(Some(items.len() as i64)),
            items: Some(tl_items),
            ..Timeline::default()
        }),
        errors: Some(Vec::new()),
        f3d: Some(f3d),
        ..Dump::default()
    };
    let bad = super::nonfinite::paths(&dump);
    if !bad.is_empty()
        && let Some(f) = dump.f3d.as_mut()
    {
        f.non_finite = Some(bad);
    }
    dump
}

/// The occurrence tree under the root component (SCHEMA.md §5.2). The
/// stored matrix is relative to the parent component: it is the
/// `transform` of top-level occurrences; nested ones keep it as
/// `_f3d.local_transform` (full-path composition not verified).
fn occurrence_tree(
    seg: &Segment,
    infos: &[OccurrenceInfo],
    comp_names: &HashMap<u64, Option<String>>,
    xrefs: &HashMap<String, String>,
    mut root_id: Option<u64>,
) -> Option<Vec<OccurrenceNode>> {
    let mut owner = HashMap::new();
    for c in seg.objects_of(OCCURRENCE_CONTAINER) {
        for r in seg.ref_ids(seg.data(c)) {
            if seg.guid_of(r) == Some(COMPONENT) {
                owner.insert(c.id, r);
            }
        }
    }
    let mut by_parent: HashMap<Option<u64>, Vec<&OccurrenceInfo>> = HashMap::new();
    for i in infos {
        match i.container {
            None => {
                if root_id.is_none() {
                    root_id = Some(i.component);
                }
            }
            Some(c) => by_parent.entry(owner.get(&c).copied()).or_default().push(i),
        }
    }

    let root_id = root_id?;
    let tree = Tree {
        by_parent,
        comp_names,
        xrefs,
        nodes: std::cell::Cell::new(0),
    };
    Some(
        tree.by_parent
            .get(&Some(root_id))
            .map_or(&[][..], |v| v)
            .iter()
            .map(|i| tree.node(i, 0, true))
            .collect(),
    )
}

/// Marks the occurrences of these objects grounded.
fn ground(nodes: &mut [OccurrenceNode], grounded: &[u64]) {
    for n in nodes {
        if n.f3d
            .as_ref()
            .and_then(|f| f.object_id)
            .is_some_and(|id| grounded.contains(&id))
        {
            n.is_grounded = Some(true);
        }
        if let Some(children) = n.children.as_mut() {
            ground(children, grounded);
        }
    }
}

/// Occurrences by parent component, for [`occurrence_tree`].
struct Tree<'a> {
    by_parent: HashMap<Option<u64>, Vec<&'a OccurrenceInfo>>,
    comp_names: &'a HashMap<u64, Option<String>>,
    xrefs: &'a HashMap<String, String>,
    /// Nodes made so far: a cyclic file cannot blow the tree up.
    nodes: std::cell::Cell<usize>,
}

/// At most this many occurrence nodes.
const MAX_OCCURRENCE_NODES: usize = 100_000;

impl Tree<'_> {
    fn node(&self, i: &OccurrenceInfo, depth: usize, top: bool) -> OccurrenceNode {
        let ext = i.context.is_some();
        let mut f3d = OccurrenceF3d {
            object_id: Some(i.object_id),
            component_object: Some(i.component),
            ..OccurrenceF3d::default()
        };
        let mut rec = OccurrenceNode {
            is_referenced_component: Some(ext),
            ..OccurrenceNode::default()
        };
        if !ext && let Some(n) = self.comp_names.get(&i.component) {
            rec.component = Some(n.clone());
        }
        if let Some(key) = i.context.map(|c| format!("{c:016x}")) {
            f3d.external_document = self.xrefs.get(&key).cloned();
            f3d.external_key = Some(key);
        }
        if let Some(m) = &i.matrix {
            if top {
                rec.transform = Some(mat4(m));
            } else {
                f3d.local_transform = Some(mat4(m));
            }
        }
        self.nodes.set(self.nodes.get() + 1);
        let full = self.nodes.get() >= MAX_OCCURRENCE_NODES;
        let kids: &[&OccurrenceInfo] = if ext || depth > 32 || full {
            &[]
        } else {
            self.by_parent.get(&Some(i.component)).map_or(&[], |v| v)
        };
        rec.children = Some(
            kids.iter()
                .map(|k| self.node(k, depth + 1, false))
                .collect(),
        );
        rec.f3d = Some(f3d);
        rec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_tokens_follow_the_regex() {
        let t: Vec<&str> = name_tokens("d1 * 2 + 1e5 mm/_x9").collect();
        assert_eq!(t, vec!["d1", "e5", "mm", "_x9"]);
        let t: Vec<&str> = name_tokens("12ab3 (Länge)").collect();
        assert_eq!(t, vec!["ab3", "L", "nge"]);
    }
}
