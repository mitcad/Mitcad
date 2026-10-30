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
// Assembly relationships: constraints and tangent contacts (mitcad#155).
mod relationships;
// The parts of a timeline item in other components (mitcad#169).
mod parts;

/// The 36-character `str16` values of object data, in order, lower case.
pub(crate) fn guid_strings(d: &[u8]) -> Vec<String> {
    joints::guid_strings(d)
}

pub use parts::item_parts;
// The compute option, symmetry, distance type and suppressed elements of
// patterns and mirrors (mitcad#186).
mod patterns;
// Box, cylinder, sphere, torus and coil primitives (mitcad#25).
mod primitives;
// Sheet-metal flanges and the sheet-metal rule (mitcad#211).
mod sheet;
// Derived designs (mitcad#324).
mod derive;

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
    let details = recipe::details_of_input(seg, input).filter(|r| r.recipe.kind == "edge")?;
    // The faces round the edge's ends, in cyclic order, from the tail.
    let end_faces = recipe::edge_tail(&details.tail).map(|t| {
        t.fans
            .into_iter()
            .map(|f| f.into_iter().map(|i| i as i64).collect())
            .collect()
    });
    let recipe = details.recipe;
    Some(Reference::Edge(Box::new(Fingerprint {
        object_type: Some("BRepEdge".to_owned()),
        f3d: Some(FingerprintF3d {
            object_id: Some(input),
            recipe: Some(recipe.kind),
            entities: Some(recipe.entities),
            end_faces,
            // The edge's curve as the input saw it (mitcad#306).
            curves: super::curve::of_input(seg, input).map(|c| {
                c.iter()
                    .map(super::curve::StoredCurve::to_geometry)
                    .collect()
            }),
            document: recipe::owner_guid(seg, input),
            context: decode::slot_contexts(seg).get(&input).copied(),
            ..FingerprintF3d::default()
        }),
        ..Fingerprint::default()
    })))
}

/// A construction plane's or point's definition code: after the root part a
/// flag byte 1 and a `u32` *(read from the variant models: 0 offset, 2 three
/// points, 7 two edges, 9 tangent at a point, 10 tangent; mitcad#186)*.
fn datum_code(seg: &Segment, id: u64) -> Option<u32> {
    let d = seg.data_of(id);
    let at = seg.root_part(d)?.end;
    (d.get(at) == Some(&1)).then(|| u32_at(d, at + 1))?
}

/// A vertex input as a reference: a vertex fingerprint with the names of
/// the faces that meet there (`_f3d`), its position left to
/// [`super::inputs`]. `None` when the input has no vertex recipe that
/// decodes.
fn vertex_input(seg: &Segment, input: u64) -> Option<Reference> {
    let recipe = recipe::of_input(seg, input).filter(|r| r.kind == "vertex")?;
    Some(Reference::Vertex(Box::new(Fingerprint {
        object_type: Some("BRepVertex".to_owned()),
        f3d: Some(FingerprintF3d {
            object_id: Some(input),
            recipe: Some(recipe.kind),
            entities: Some(recipe.entities),
            ..FingerprintF3d::default()
        }),
        ..Fingerprint::default()
    })))
}

/// What the extrude record's codes say about the extents beyond the
/// parameters (the API reference dumps, 78 of 78 through-all extents and 11
/// of 11 symmetric ones): the extent type (code `a` 3 symmetric), whether
/// there are two extents, a symmetric extent's kind and length byte, and a
/// side through all with the direction the flag byte after the extent codes
/// gives it (0 along the sketch's normal, 1 against it; the byte belongs to
/// the through-all side, whichever side that is).
fn extrude_extent_kinds(ex: &decode::ExtrudeFields, x: &mut ExtrudeDetail) {
    if (1..=3).contains(&ex.extent_a) {
        x.has_two_extents = Some(ex.extent_a == 2);
    }
    if ex.extent_a == 3 {
        x.extent_type = Some("SymmetricFeatureExtentType".into());
    }
    let positive = ex.flag.filter(|f| *f <= 1).map(|f| f == 0);
    let through_all = |positive: Option<bool>| Definition {
        definition_type: Some("ThroughAllExtentDefinition".into()),
        is_positive_direction: positive,
        ..Definition::default()
    };
    match (ex.extent_a, ex.sides) {
        (3, Some([1, 1])) if x.extent_one.is_some() => {
            let one = x.extent_one.take().unwrap_or_default();
            let sym = Definition {
                definition_type: Some("SymmetricExtentDefinition".into()),
                distance: one.distance,
                is_full_length: ex.full_length,
                taper_angle: x.taper_angle_one.clone(),
                ..Definition::default()
            };
            x.extent_one = Some(sym.clone());
            x.symmetric_extent = Some(sym);
        }
        (3, Some([4, 4])) => {
            x.extent_one = Some(through_all(positive));
            // Through all on both sides is the whole length by definition
            // (4 of 4 in the API reference dumps).
            x.symmetric_extent = Some(Definition {
                definition_type: Some("SymmetricExtentDefinition".into()),
                is_full_length: Some(true),
                taper_angle: x.taper_angle_one.clone(),
                ..Definition::default()
            });
        }
        (1 | 2, Some([4, _])) if x.extent_one.is_none() => {
            x.extent_one = Some(through_all(positive));
        }
        (2, Some([a, 4])) if a != 4 && x.extent_two.is_none() => {
            x.extent_two = Some(through_all(positive));
        }
        _ => {}
    }
    // A side along a distance is not a symmetric one.
    for side in [x.extent_one.as_mut(), x.extent_two.as_mut()]
        .into_iter()
        .flatten()
    {
        if side.definition_type.as_deref() == Some("DistanceExtentDefinition") {
            side.is_symmetric = Some(false);
        }
    }
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
            document: recipe::owner_guid(seg, input),
            context: decode::slot_contexts(seg).get(&input).copied(),
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
/// item) | ref | ref | u8 right-handed | f64 taper angle | f64 height
/// | f64 tap drill | f64 taper length | f64 small pitch diameter (cm) | ...
/// ```
///
/// `external` is 1 on every external face of the corpus' threads and 0 on
/// every internal one. The first designation is empty in some files; the
/// second is then the designation. The byte after the two references
/// (versions 4 and 5) is 1 for a right-handed thread and 0 for a left-handed
/// one (the controlled variants of a cosmetic and a modelled thread each
/// differ in this byte only, apart from the list of the modelled one; every
/// thread of the corpora has 1).
///
/// A taper pipe thread (mitcad#733: `1/2 - 14 NPT` of type `NPT`, `Rc 1/2`
/// of type `ISO Taper Internal`) stores the angle 0, its tap drill as both
/// diameters and the pitch diameter 0; the five numbers after the
/// handedness byte (all 0 on straight threads) give its taper
/// ([`ThreadTaper`]).
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
    /// Handedness; `None` where the layout has no such byte (versions
    /// before 4) or the bytes do not fit it.
    pub right_handed: Option<bool>,
    /// A taper pipe thread's taper (mitcad#733); `None` for a straight one.
    pub taper: Option<ThreadTaper>,
    /// The hole whose thread it is (the reference after the pitch
    /// diameter); `None` for a thread item.
    pub hole: Option<u64>,
}

/// A taper pipe thread's numbers after the handedness byte (mitcad#733), as
/// the drum design's `1/2 - 14 NPT` and `Rc 1/2` holes have them: the
/// taper's full angle (radians: 2 atan(1/32) for 1:16 on the diameter), the
/// thread's height h (0.8 P of NPT, 0.640327 P of the 55-degree profile),
/// the tap drill, the thread's length and the pitch diameter at its small
/// end, `length` from the large one (cm). The large end's minor diameter is
/// `small_pitch + length × 2 tan(angle / 2) − h`: the bore's taper of both
/// holes starts with it at the hole's start (the history states' cones).
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadTaper {
    pub angle: f64,
    pub height: f64,
    pub tap_drill: f64,
    pub length: f64,
    pub small_pitch: f64,
}

/// The handedness byte of a thread object: after the pitch diameter come
/// the hole reference (one `0` byte for a thread that is no hole's), two
/// more references, then the byte (1 right-handed, 0 left-handed). `ref_end`
/// gives the end of the reference at a position. With the byte, where it
/// is.
fn thread_handedness(
    d: &[u8],
    q: usize,
    ref_end: impl Fn(&[u8], usize) -> Option<usize>,
) -> Option<(bool, usize)> {
    let mut q = q;
    q = if *d.get(q)? == 0 {
        q + 1
    } else {
        ref_end(d, q)?
    };
    for _ in 0..2 {
        q = ref_end(d, q)?;
    }
    match *d.get(q)? {
        0 => Some((false, q)),
        1 => Some((true, q)),
        _ => None,
    }
}

/// The taper numbers after the handedness byte at `q` ([`ThreadTaper`]):
/// `None` when they are all 0 (a straight thread) or do not fit a taper.
fn thread_taper(d: &[u8], q: usize) -> Option<ThreadTaper> {
    let v = f64s_at(d, q + 1, 5)?;
    let [angle, height, tap_drill, length, small_pitch] = [v[0], v[1], v[2], v[3], v[4]];
    let positive = |x: f64| x.is_finite() && x > 0.0;
    (angle > 0.0
        && angle < 0.5
        && [height, tap_drill, length, small_pitch]
            .into_iter()
            .all(positive))
    .then_some(ThreadTaper {
        angle,
        height,
        tap_drill,
        length,
        small_pitch,
    })
}

impl ThreadFields {
    /// The `ThreadInfo` of the dump (SCHEMA.md §5.3).
    fn info(&self) -> serde_json::Value {
        let mut info = serde_json::json!({
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
        });
        if let Some(right) = self.right_handed {
            info["isRightHanded"] = right.into();
        }
        // A taper pipe thread (mitcad#733): its taper's numbers (cm, the
        // angle in radians).
        if let Some(t) = &self.taper {
            info["isTapered"] = true.into();
            info["_f3d_taper"] = serde_json::json!({
                "angle": t.angle,
                "height": t.height,
                "tapDrill": t.tap_drill,
                "length": t.length,
                "smallPitchDiameter": t.small_pitch,
            });
        }
        info
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
    // (A taper pipe thread stores the angle 0, mitcad#733.)
    if full_length > 1
        || modeled > 1
        || !(angle.is_finite() && angle >= 0.0 && sane(major) && sane(minor) && sane(pitch))
    {
        return None;
    }
    if text.len() == 4 {
        text.insert(1, String::new());
    }
    let [class, first, second, size, thread_type]: [String; 5] = text.try_into().ok()?;
    let handedness = if version >= 4 {
        thread_handedness(d, p + 38, |d, q| seg.ref_at(d, q).map(|r| r.end))
    } else {
        None
    };
    let taper = handedness.and_then(|(_, q)| thread_taper(d, q));
    // A straight thread has its angle.
    if taper.is_none() && !sane(angle) {
        return None;
    }
    let hole = match d.get(p + 38) {
        Some(0) | None => None,
        Some(_) => seg.ref_at(d, p + 38).map(|r| r.id),
    };
    Some(ThreadFields {
        right_handed: handedness.map(|(right, _)| right),
        taper,
        hole,
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

const IDENTITY_MAT4: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Whether a move's transform object ([`classes::MOVE_DATA`]) stores no
/// matrix: after its zeros the `u32 5` is followed by `u8 1 | u16 0 | u8 1
/// | u32 0 | ref` where a stored matrix (128 bytes) would be. The two
/// moves of that layout among the API reference dumps are the identity
/// (every other move stores its matrix: 12 of 12 agree with the dumps).
pub(super) fn identity_move_data(seg: &Segment, id: u64) -> bool {
    if seg.guid_of(id) != Some(MOVE_DATA) {
        return false;
    }
    let d = seg.data_of(id);
    let Some(root) = seg.root_part(d) else {
        return false;
    };
    let Some(rest) = d.get(root.end..) else {
        return false;
    };
    let zeros = rest.iter().take_while(|&&v| v == 0).count();
    let p = root.end + zeros;
    d.get(p..p + 12) == Some(&[5, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0][..])
        && seg.ref_at(d, p + 12).is_some()
}

/// Whether a hole goes against its default direction (mitcad#25): after
/// the first null reference behind the header, `u32 type` (0 simple, 1
/// counterbore, 2 countersink) at +9 and `u32 flipped` at +13; read against
/// the API's `isDefaultDirection` on 39 holes.
fn hole_flipped(seg: &Segment, id: u64) -> Option<bool> {
    let d = seg.data_of(id);
    let (_, null) = seg
        .refs_in(d, header_end(d)?, d.len())
        .into_iter()
        .find(|(_, r)| r.id == 0)?;
    let (kind, flipped) = (u32_at(d, null.end + 9)?, u32_at(d, null.end + 13)?);
    (kind <= 2 && flipped <= 1).then_some(flipped == 1)
}

/// The first rigid 4x4 matrix (row-major, last row `0 0 0 1`, orthonormal
/// rotation) in an object's data, at any offset.
fn rigid_matrix(d: &[u8]) -> Option<Mat4> {
    let mut p = 0;
    while p + 128 <= d.len() {
        if let Some(m) = f64s_at(d, p, 16) {
            let last = m[12] == 0.0 && m[13] == 0.0 && m[14] == 0.0 && m[15] == 1.0;
            let row = |r: usize| [m[4 * r], m[4 * r + 1], m[4 * r + 2]];
            use mitcad_vec3::dot;
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

/// One edge set's options of a fillet (mitcad#154): the set's
/// [`FILLET_SET`] object holds, after its root part, `u8
/// asymmetric | u8 0 | u8 constant radius | u8 flipped | u8 curvature
/// continuity (G2) | u8 tangent chain | u8 0 | u8 0 | ref set input`
/// (neither size byte: a variable radius). Read on the reference models
/// against their dumps' `edgeSets` (continuity, `isFlipped`,
/// `isTangentChain`); the format descriptions we have do not give this
/// record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FilletSetOptions {
    pub asymmetric: bool,
    pub constant: bool,
    pub flipped: bool,
    pub curvature: bool,
    pub tangent_chain: bool,
}

fn fillet_set_options(seg: &Segment, id: u64) -> Option<FilletSetOptions> {
    let d = seg.data_of(id);
    let r = seg.root_part(d)?.end;
    let b = |k: usize| d.get(r + k).copied().filter(|&v| v <= 1);
    let (options, input) = match seg.object(id).and_then(|o| seg.version(o))? {
        10 => {
            if b(1)? != 0 || b(6)? != 0 || b(7)? != 0 {
                return None;
            }
            (
                FilletSetOptions {
                    asymmetric: b(0)? == 1,
                    constant: b(2)? == 1,
                    flipped: b(3)? == 1,
                    curvature: b(4)? == 1,
                    tangent_chain: b(5)? == 1,
                },
                r + 8,
            )
        }
        // Older writers (versions 9 and 4 of the set class, items of class
        // version 2): `u8 asymmetric | u8 constant radius | u8 flipped | u8
        // tangent chain | u8 0`, version 9 with one more `u8 0` before the
        // set input. Read against the API reference dumps (68 sets: the
        // tangent chain agrees on all, 12 of them off). The fifth byte sits
        // where version 10 keeps its curvature flag, but no dump has it
        // set, so a set that has it is not read.
        9 | 4 => {
            let end = if seg.version(seg.object(id)?)? == 9 {
                r + 6
            } else {
                r + 5
            };
            if b(4)? != 0 || (end == r + 6 && b(5)? != 0) {
                return None;
            }
            (
                FilletSetOptions {
                    asymmetric: b(0)? == 1,
                    constant: b(1)? == 1,
                    flipped: b(2)? == 1,
                    curvature: false,
                    tangent_chain: b(3)? == 1,
                },
                end,
            )
        }
        _ => return None,
    };
    // Some sets have a block of zero bytes (20 of them in the dumps' five
    // such sets) between the options and the set input.
    let gap = d.get(input..)?.iter().take_while(|&&v| v == 0).count();
    let set = seg.ref_at(d, input + gap.min(24))?;
    if seg.guid_of(set.id) != Some(BODY_INPUT) {
        return None;
    }
    Some(options)
}

/// A fillet's set options in set order and whether its corners are
/// rolling-ball corners (mitcad#154): the fillet object starts after its
/// root part with `u32 n | n refs` [`FILLET_SET`] `| u32 0 | u8
/// rolling ball corners` (0: setback corners). Read on the reference
/// models against their dumps' `isRollingBallCorner`.
struct FilletOptions {
    sets: Vec<(u64, FilletSetOptions)>,
    rolling: Option<bool>,
}

fn fillet_options(seg: &Segment, fid: u64) -> Option<FilletOptions> {
    let d = seg.data_of(fid);
    let r = seg.root_part(d)?.end;
    let (sets, end) = seg.ref_list_at(d, r)?;
    if sets.is_empty() || sets.iter().any(|&s| seg.guid_of(s) != Some(FILLET_SET)) {
        return None;
    }
    let options: Vec<(u64, FilletSetOptions)> = sets
        .iter()
        .map(|&s| fillet_set_options(seg, s).map(|options| (s, options)))
        .collect::<Option<_>>()?;
    // (Not guarded against a health reference there: the flag, a zero and a
    // one read as a reference to object 256 in every design that has such
    // an object, and are then none, 27 of the API reference dumps' sets.)
    let rolling = (u32_at(d, end) == Some(0))
        .then(|| d.get(end + 4).copied())
        .flatten()
        .filter(|&v| v <= 1)
        .map(|v| v == 1);
    Some(FilletOptions {
        sets: options,
        rolling,
    })
}

/// A chamfer's corner type (mitcad#154): the `u32` right after its root
/// part: 1 chamfer, 2 miter, 3 blend corners. Read on the reference
/// models against their dumps' `cornerType`.
fn chamfer_corner(seg: &Segment, fid: u64) -> Option<&'static str> {
    let d = seg.data_of(fid);
    let r = seg.root_part(d)?.end;
    match u32_at(d, r)? {
        1 => Some("ChamferCornerType"),
        2 => Some("MiterCornerType"),
        3 => Some("BlendCornertype"),
        _ => None,
    }
}

/// A chamfer edge set's tangent-chain option: the byte right after the
/// set record's root part (class version 2: `u8 tangent chain | ...`),
/// 1 on the sets the application made through its user interface and 0
/// on the controlled models made by script, whose dumps list every one of
/// them with `isTangentChain` false and the 193 sets of the API reference
/// dumps with it true. No byte of the record was found for the
/// two-distance chamfer's `isFlipped` (every set of the dumps and of the
/// controlled models has it off, whichever bytes the record holds): the
/// import takes the sides from the geometry (*Named inputs*).
fn chamfer_set_tangent_chain(seg: &Segment, id: u64) -> Option<bool> {
    let o = seg.object(id)?;
    if seg.version(o)? != 2 {
        return None;
    }
    let d = seg.data_of(id);
    let r = seg.root_part(d)?.end;
    match *d.get(r)? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// A chamfer edge set's flip: the byte after the tangent-chain byte (class
/// version 2), set only for the sets of two distances and of a distance
/// and an angle (equal distances have no side). Of the API reference
/// dumps' sets it agrees on all: the one flipped set (a distance and an
/// angle) has it set, no other does. Two distances' sides are taken from
/// the geometry all the same; this is the flag for the other reading.
fn chamfer_set_flipped(seg: &Segment, id: u64) -> Option<bool> {
    let o = seg.object(id)?;
    if seg.version(o)? != 2 {
        return None;
    }
    let d = seg.data_of(id);
    let r = seg.root_part(d)?.end;
    match *d.get(r + 1)? {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
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

/// The plane an entity reference kept after its target went away (the
/// plane object [`PLANE_FRAME`] it refers to): `f64 origin[3] | u[3] |
/// v[3]`, found by its shape (unit, orthogonal u and v). The API names such
/// a plane "reconstructed".
fn frame_plane(seg: &Segment, entity_ref: u64) -> Option<Geometry> {
    let d = seg.data_of(entity_ref);
    let frame = seg
        .refs_in(d, 0, d.len())
        .into_iter()
        .find(|(_, r)| seg.guid_of(r.id) == Some(PLANE_FRAME))?
        .1;
    plane_of_frame(seg.data_of(frame.id))
}

/// The plane in the data of a [`PLANE_FRAME`] object: `f64 origin[3] |
/// u[3] | v[3]` somewhere after its header.
fn plane_of_frame(t: &[u8]) -> Option<Geometry> {
    let mut q = header_end(t)?;
    while q + 8 * 9 <= t.len() {
        if let Some(v) = f64s_at(t, q, 9)
            && v.iter().all(|x| x.is_finite())
        {
            let (o, u, w) = ([v[0], v[1], v[2]], [v[3], v[4], v[5]], [v[6], v[7], v[8]]);
            let nu = py_sum(u.iter().map(|x| x * x));
            let nv = py_sum(w.iter().map(|x| x * x));
            let dot = py_sum((0..3).map(|i| u[i] * w[i]));
            if (nu - 1.0).abs() < 1e-9 && (nv - 1.0).abs() < 1e-9 && dot.abs() < 1e-9 {
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
    /// The GUIDs (`str16`) in its data, lower case: occurrence paths name
    /// it by one of them (captured positions, relationships), also from a
    /// design that inserted this one (mitcad#196).
    pub guids: Vec<String>,
}

pub fn occurrence_info(seg: &Segment, id: u64) -> Option<OccurrenceInfo> {
    let d = seg.data_of(id);
    let start = header_end(d)?;
    let refs = seg.refs_in(d, start, d.len());
    let local = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(COMPONENT))
        .map(|(p, r)| (*p, r.id, r.context, r.end));
    // A component of another document (mitcad#196): its id is that
    // document's, so it is no object here (an id that happens to be one
    // here was read as a reference above, at the same place).
    let external = seg
        .context_refs_in(d, start, d.len())
        .into_iter()
        .rev()
        .find(|(_, r)| r.guid == COMPONENT)
        .map(|(p, r)| (p, r.id, Some(r.key), r.end));
    let (_, component, context, q) = match (local, external) {
        (Some(l), Some(e)) => {
            if l.0 > e.0 {
                l
            } else {
                e
            }
        }
        (l, e) => l.or(e)?,
    };
    let container = refs
        .iter()
        .rev()
        .find(|(_, r)| seg.guid_of(r.id) == Some(OCCURRENCE_CONTAINER))
        .map(|(_, r)| r.id);
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
        component,
        context,
        container,
        matrix,
        guids: joints::guid_strings(d),
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

/// The result number of the base feature with which a component made from
/// bodies put its bodies into the new component (mitcad#156): the base
/// feature its main part refers to, whose item tail numbers that
/// component's history operation.
fn made_result(seg: &Segment, id: u64) -> Option<i32> {
    let o = seg.object(id)?;
    seg.refs_in(seg.data(o), 0, seg.main_end(o))
        .into_iter()
        .filter(|(_, r)| seg.guid_of(r.id) == Some(BASE_FEATURE))
        .find_map(|(_, r)| {
            let tail = decode::feature_tail(seg, seg.object(r.id)?)?;
            (tail.result_no > 0).then_some(tail.result_no)
        })
}

/// The operation of an item without one of its own whose part is a base
/// feature that holds the bodies the item brings (mitcad#641): a paste of
/// bodies and a derive from another design keep them in such a base
/// feature, listed among the item's sub-features, whose result number is
/// the item's state.
fn sub_base_result(seg: &Segment, tail: &decode::ItemTail) -> Option<i32> {
    tail.sub_features
        .iter()
        .flatten()
        .filter(|&&s| seg.is_kind_of(s, BASE_FEATURE))
        .find_map(|&s| {
            let t = decode::feature_tail(seg, seg.object(s)?)?;
            (t.result_no > 0).then_some(t.result_no)
        })
}

/// The operation of a tapped hole's modelled thread (mitcad#719): the
/// thread sub-item has a result number of its own, the hole's plus one,
/// and its history state cuts the thread into the hole's bore (an M10
/// thread's 195 mm³ in a 20 mm deep hole); the hole's own state has only
/// the bore. A cosmetic taper pipe thread's operations taper the bore
/// (mitcad#733: 150 mm³ of a `1/2 - 14 NPT` hole) and count alike; the
/// thread of a hole whose tail lists no sub-items is the one that names
/// the hole (`id`). `None` for other items and straight cosmetic threads.
fn modelled_thread_result(seg: &Segment, id: u64, tail: &decode::ItemTail) -> Option<i32> {
    let shapes = |t: &ThreadFields| t.modeled || t.taper.is_some();
    let listed: Vec<u64> = tail
        .sub_features
        .iter()
        .flatten()
        .copied()
        .filter(|&s| thread_fields(seg, s).is_some_and(|t| shapes(&t)))
        .collect();
    let threads = if listed.is_empty() && tail.sub_features.iter().flatten().next().is_none() {
        seg.objects_of(THREAD)
            .filter(|o| thread_fields(seg, o.id).is_some_and(|t| t.hole == Some(id) && shapes(&t)))
            .map(|o| o.id)
            .collect()
    } else {
        listed
    };
    threads.into_iter().find_map(|s| {
        let t = decode::feature_tail(seg, seg.object(s)?)?;
        (t.result_no > tail.result_no && tail.result_no > 0).then_some(t.result_no)
    })
}

/// Names of the parameters that depend on `p`: those whose expression
/// mentions it (as a name token) and, through them, those that mention
/// those, in the parameters' order (the API reference dumps list all of
/// them, not only the direct ones).
fn dependents(p: &ParameterEntry, params: &[ParameterEntry]) -> Vec<String> {
    let mut found = vec![false; params.len()];
    let mut queue = vec![p.name.as_str()];
    while let Some(name) = queue.pop() {
        for (i, q) in params.iter().enumerate() {
            if !found[i] && !std::ptr::eq(q, p) && name_tokens(&q.expression).any(|t| t == name) {
                found[i] = true;
                queue.push(q.name.as_str());
            }
        }
    }
    params
        .iter()
        .zip(found)
        .filter(|(_, f)| *f)
        .map(|(q, _)| q.name.clone())
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
    /// Point objects of construction points, to their feature (mitcad#186).
    point_feature: HashMap<u64, u64>,
    /// Sketch entities by the ids inputs name them with (mitcad#34).
    tags: sweeps::EntityTags,
    /// Occurrences and components by their GUIDs (mitcad#66).
    guids: joints::Guids,
    /// Component names by object id.
    component_names: HashMap<u64, String>,
    /// The timeline item of each sub-feature that is not itself on the
    /// timeline (mitcad#641): a paste's or derive's base feature.
    sub_item: HashMap<u64, u64>,
}

impl Builder<'_> {
    fn name(&self, f: Option<u64>) -> Option<String> {
        f.and_then(|f| {
            self.name_of
                .get(&f)
                .cloned()
                .flatten()
                .or_else(|| self.name_outside_timeline(f))
        })
    }

    /// The name of a feature or sketch outside the timeline (one of a
    /// component's own list): from its tail, a sketch's from its object.
    fn name_outside_timeline(&self, f: u64) -> Option<String> {
        let tail = decode::feature_tail(self.seg, self.seg.object(f)?)?;
        let own = self
            .sk_of_feat
            .get(&f)
            .and_then(|s| self.seg.object(*s))
            .and_then(|s| sketch::sketch_name(self.seg, s));
        let name = match own {
            Some(own) => decode::sketch_item_name(&own, &tail),
            None => tail.display_name(),
        };
        Some(name).filter(|n| !n.is_empty())
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
            return self.reconstructed_plane(entity_ref);
        };
        if geo.is_some() {
            out.geometry = geo;
        }
        Some(Reference::ConstructionPlane(Box::new(out)))
    }

    /// The plane an entity input kept when its target went away (a
    /// construction plane deleted after the item used it): the API's
    /// "reconstructed" plane, with the geometry the reference stored.
    fn reconstructed_plane(&self, entity_ref: u64) -> Option<Reference> {
        let geometry = frame_plane(self.seg, entity_ref)?;
        Some(Reference::ConstructionPlane(Box::new(ConstructionRef {
            name: Some(Some("Reconstructed-Plane".to_owned())),
            origin: Some(None),
            timeline_index: Some(None),
            geometry: Some(geometry),
            ..ConstructionRef::default()
        })))
    }

    /// What an entity input names, by its target's kind: a plane, an axis,
    /// the origin point, or a sketch entity (mitcad#66, mitcad#67).
    fn entity_target(&self, entity_ref: u64) -> Option<Reference> {
        // A sketch curve or point by its tags, without a target object
        // (mitcad#178: a plane along a sketch path).
        let Some(tid) = decode::target_id(self.seg, entity_ref) else {
            return self.input_entity(entity_ref);
        };
        match self.seg.guid_of(tid) {
            Some(ORIGIN_AXIS) => self.axis_ref(entity_ref),
            // A construction point's own point object (mitcad#186), else the
            // origin.
            Some(ORIGIN_POINT) => Some(Reference::ConstructionPoint(Box::new(
                if let Some(&f) = self.point_feature.get(&tid) {
                    ConstructionRef {
                        name: Some(self.name(Some(f))),
                        origin: Some(None),
                        timeline_index: Some(self.pos(Some(f))),
                        ..ConstructionRef::default()
                    }
                } else {
                    ConstructionRef {
                        name: Some(Some("Origin".to_owned())),
                        origin: Some(Some("Origin".to_owned())),
                        timeline_index: Some(None),
                        ..ConstructionRef::default()
                    }
                },
            ))),
            Some(ORIGIN_PLANE) => self.plane_ref(entity_ref),
            _ => self
                .input_entity(entity_ref)
                .or_else(|| self.axis_ref(entity_ref))
                .or_else(|| self.reconstructed_plane(entity_ref)),
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
        self.sketch_profiles(fid, 0)
    }

    /// An extrusion's profiles: those of [`Self::profiles`], then the faces
    /// its profile slot selects (planar faces of bodies, extruded as they
    /// are, mitcad#169), each of which takes one of the selected profiles'
    /// entries too.
    fn extrude_profiles(&self, fid: u64) -> Option<Vec<Reference>> {
        let faces: Vec<Reference> = decode::extent_slots(self.seg, fid)
            .into_iter()
            .filter(|s| s.role == decode::PROFILE_ROLE)
            .flat_map(|s| s.inputs)
            .filter(|&i| self.seg.guid_of(i) == Some(FACE_REF))
            .filter_map(|i| face_input(self.seg, i))
            .collect();
        let mut out = self.sketch_profiles(fid, faces.len()).unwrap_or_default();
        out.extend(faces);
        (!out.is_empty()).then_some(out)
    }

    /// The sketch a feature's profile source names (the last one of its
    /// inputs).
    fn profile_sketch(&self, fid: u64) -> Option<u64> {
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
        sketch_id
    }

    /// The profile reference a profile source ([`PROFILE_SOURCE`]) names: its
    /// sketch (the API reference dumps' construction planes and axes
    /// measured from a profile).
    fn profile_of_source(&self, src: u64) -> Option<Reference> {
        let sid = source_sketch(self.seg, src)?;
        let f = self.feat_of_sk.get(&sid).copied();
        Some(Reference::Profile(ProfileRef {
            sketch: Some(self.name(f)),
            sketch_timeline_index: Some(self.pos(f)),
            ..ProfileRef::default()
        }))
    }

    /// The selected profiles of a sketch (see [`Self::profiles`]), less
    /// `faces` entries.
    fn sketch_profiles(&self, fid: u64, faces: usize) -> Option<Vec<Reference>> {
        let seg = self.seg;
        let sketch_id = self.profile_sketch(fid);
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
        let count = count.saturating_sub(faces);
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

    /// A set's explicit links to its parameter holder and selection input
    /// (mitcad#186). Holder order in the feature's flat input list need not
    /// follow set order: an edited set can leave its holders before the
    /// first selection group. The set record retains both links.
    fn linked_set_edges(&self, it: &TimelineEntry, holder: u64) -> Option<Vec<Reference>> {
        let seg = self.seg;
        let class = match it.type_name {
            Some("FilletFeature") => FILLET_SET,
            Some("ChamferFeature") => CHAMFER_SET,
            _ => return None,
        };
        let records: Vec<u64> = inputs_of(seg, it.id, class)
            .into_iter()
            .filter(|&id| inputs_of(seg, id, PARAMETER_HOLDER).contains(&holder))
            .collect();
        let [record] = records.as_slice() else {
            return None;
        };
        let groups = inputs_of(seg, *record, BODY_INPUT);
        let [group] = groups.as_slice() else {
            return None;
        };
        if !matches!(set_selects(seg, *group), Some(8 | 16)) {
            return None;
        }
        let data = seg.data_of(*group);
        let (inputs, _) = seg.ref_list_at(data, seg.root_part(data)?.end)?;
        if inputs.is_empty() {
            return None;
        }
        inputs
            .into_iter()
            .map(|input| {
                (seg.guid_of(input) == Some(FACE_REF))
                    .then(|| edge_input(seg, input).or_else(|| face_input(seg, input)))
                    .flatten()
            })
            .collect()
    }

    /// The edges of a fillet's or chamfer's sets, by each set's parameter
    /// holder. Prefer explicit set links; older or compound selections use
    /// the flat input groups. `None` unless every set's edges decode.
    fn set_edges(
        &self,
        it: &TimelineEntry,
        params: &[&ParameterEntry],
    ) -> Option<Vec<Vec<Reference>>> {
        let linked: Option<Vec<Vec<Reference>>> = params
            .iter()
            .map(|p| self.linked_set_edges(it, p.holder?))
            .collect();
        if linked.is_some() {
            return linked;
        }
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

    /// A fillet's edge sets from its input groups, each set's size by the
    /// roles of the parameters its holders hold (mitcad#154; the roles as
    /// the cadmpeg format description names them): `Radius` a constant
    /// radius, `StartRadius`, `EndRadius` and the pairs `MidRadius`,
    /// `MidParams` a variable one, `ChordLen` a chord length,
    /// `EdgeOffset1`, `EdgeOffset2` an asymmetric one; `TangencyWeight`
    /// goes with any. `None` unless every group has holders and a size, so
    /// that the sets by `Radius` parameters stay as they were.
    fn fillet_sets(
        &self,
        it: &TimelineEntry,
        rl: &BTreeMap<&str, Vec<&ParameterEntry>>,
    ) -> Option<Vec<EdgeSet>> {
        let tail = it.tail.as_ref()?;
        let inputs = tail_inputs(self.seg, self.seg.data_of(it.id), tail.start)?;
        let groups = edge_groups(self.seg, &inputs);
        if groups.is_empty() {
            return None;
        }
        // Only where a set is not a constant radius: those keep their
        // older path.
        if !["StartRadius", "ChordLen", "EdgeOffset1"]
            .iter()
            .any(|r| rl.contains_key(r))
        {
            return None;
        }
        let mut sets = Vec::new();
        for (edges, holders) in groups {
            let of = |role: &str| -> Vec<&ParameterEntry> {
                rl.get(role)
                    .map(|v| {
                        v.iter()
                            .copied()
                            .filter(|p| p.holder.is_some_and(|h| holders.contains(&h)))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let one = |role: &str| -> Option<Reference> {
                let v = of(role);
                (v.len() == 1).then(|| paramref(v[0]))
            };
            let mut s = EdgeSet {
                edges: edges.filter(|e| !e.is_empty()),
                tangency_weight: one("TangencyWeight"),
                ..EdgeSet::default()
            };
            if let Some(r) = one("Radius") {
                s.edge_set_type = Some("ConstantRadiusFilletEdgeSet".into());
                s.radius = Some(r);
            } else if let (Some(a), Some(b)) = (one("StartRadius"), one("EndRadius")) {
                s.edge_set_type = Some("VariableRadiusFilletEdgeSet".into());
                s.start_radius = Some(a);
                s.end_radius = Some(b);
                let (radii, positions) = (of("MidRadius"), of("MidParams"));
                if radii.len() != positions.len() {
                    return None;
                }
                if !radii.is_empty() {
                    s.mid_radii = Some(radii.into_iter().map(paramref).collect());
                    s.mid_positions = Some(positions.into_iter().map(paramref).collect());
                }
            } else if let Some(c) = one("ChordLen") {
                s.edge_set_type = Some("ChordLengthFilletEdgeSet".into());
                s.other.insert(
                    "chordLength".into(),
                    serde_json::to_value(&c).expect("serializes"),
                );
            } else if let (Some(a), Some(b)) = (one("EdgeOffset1"), one("EdgeOffset2")) {
                s.edge_set_type = Some("AsymmetricFilletEdgeSet".into());
                s.offset_one = Some(a);
                s.offset_two = Some(b);
            } else {
                return None;
            }
            sets.push(s);
        }
        Some(sets)
    }

    /// The faces an item's face inputs name (mitcad#154), in input order;
    /// `None` unless there are some and every one decodes.
    fn face_inputs(&self, it: &TimelineEntry) -> Option<Vec<Reference>> {
        let ids: Vec<u64> = self
            .item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(FACE_REF))
            .collect();
        if ids.is_empty() {
            return None;
        }
        ids.into_iter().map(|i| face_input(self.seg, i)).collect()
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

    /// An extrusion's participant slot excludes body references used by
    /// its termination slots. Older or unresolved slots keep the common
    /// input-list reading; a decoded empty slot remains an empty list.
    fn extrude_participants(&self, it: &TimelineEntry) -> Option<Vec<Reference>> {
        let slots: Vec<_> = decode::extent_slots(self.seg, it.id)
            .into_iter()
            .filter(|s| s.role == decode::PARTICIPANT_ROLE || s.role == decode::PARTICIPANT_ROLE_2)
            .collect();
        if slots.is_empty() {
            return self.body_inputs(it);
        }
        let mut ids = Vec::new();
        for id in slots.into_iter().flat_map(|s| s.inputs) {
            if self.seg.guid_of(id) != Some(BODY_REF) {
                return self.body_inputs(it);
            }
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids.into_iter()
            .map(|i| self.body_input(i))
            .collect::<Option<Vec<_>>>()
            .or_else(|| self.body_inputs(it))
    }

    /// A construction plane's inputs (mitcad#178): the entities it is
    /// measured from, in input order (faces and edges by their names,
    /// origin and construction planes and axes, sketch entities).
    ///
    /// The item lists its parameter holder, a frame input and then the
    /// entities; which definition it is follows from its parameter (an
    /// offset, an angle, a distance along a path) and, without one, from
    /// the entities: two planes a mid plane, two lines a plane through
    /// them, a line and a point a plane through both, three points a plane
    /// through them. Each reading is checked by the import against the
    /// plane the file stores.
    ///
    /// The definition code the item stores (`datum_code`, mitcad#186: 2
    /// three points, 7 two edges, 9 tangent at a point, 10 tangent) names
    /// those that have no parameter or the same one as another; a vertex
    /// input (a recipe of the faces that meet there) is a point.
    fn plane_inputs(&self, it: &TimelineEntry, x: &mut ConstructionPlaneDetail) {
        let code = datum_code(self.seg, it.id);
        let mut refs = Vec::new();
        let mut undecoded = 0;
        for i in self.item_inputs(it) {
            let r = match self.seg.guid_of(i) {
                Some(PARAMETER_HOLDER | FRAME_INPUT | INPUT_SLOT) => continue,
                Some(FACE_REF) => face_input(self.seg, i)
                    .or_else(|| edge_input(self.seg, i))
                    .or_else(|| vertex_input(self.seg, i)),
                Some(ENTITY_REF) => self.entity_target(i),
                // A profile as the plane it lies in (API reference dumps:
                // two planes offset from a profile).
                Some(PROFILE_SOURCE) => self.profile_of_source(i),
                _ => None,
            };
            match r {
                Some(r) => refs.push(r),
                None => undecoded += 1,
            }
        }
        if refs.is_empty() || undecoded > 0 {
            return;
        }
        let planar = |r: &Reference| {
            matches!(
                r,
                Reference::Face(_) | Reference::ConstructionPlane(_) | Reference::Profile(_)
            )
        };
        let point = |r: &Reference| match r {
            Reference::ConstructionPoint(_) | Reference::Vertex(_) => true,
            Reference::SketchEntity(s) => s.object_type.as_deref() == Some("SketchPoint"),
            _ => false,
        };
        let linear = |r: &Reference| match r {
            Reference::Edge(_) | Reference::ConstructionAxis(_) => true,
            Reference::SketchEntity(s) => s.object_type.as_deref() == Some("SketchLine"),
            _ => false,
        };
        let value = |r: &Reference| json_value(r);
        let kind = x
            .definition
            .as_ref()
            .and_then(|d| d.definition_type.clone());
        let d = x.definition.get_or_insert_with(Definition::default);
        let put = |d: &mut Definition, k: &str, r: &Reference| {
            d.other.insert(k.to_owned(), value(r));
        };
        match kind.as_deref() {
            Some("ConstructionPlaneOffsetDefinition") => match &refs[..] {
                [r] if planar(r) => d.planar_entity = Some(r.clone()),
                // A plane through a point, parallel to a plane.
                [r, p] | [p, r] if planar(r) && point(p) => {
                    d.planar_entity = Some(r.clone());
                    put(d, "point", p);
                    d.definition_type =
                        Some("ConstructionPlaneOffsetThroughPointDefinition".to_owned());
                }
                _ => {}
            },
            // A tangent plane: the face (a cylinder or cone) and the plane
            // its angle is measured from.
            Some("ConstructionPlaneAtAngleDefinition") if code == Some(10) => {
                let faces = refs.iter().filter(|r| matches!(r, Reference::Face(_)));
                let others = refs.iter().filter(|r| !matches!(r, Reference::Face(_)));
                let (Some(face), plane) =
                    (faces.clone().next(), others.chain(faces.skip(1)).next())
                else {
                    return;
                };
                put(d, "tangentFace", face);
                d.planar_entity = plane.filter(|p| planar(p)).cloned();
                d.definition_type = Some("ConstructionPlaneTangentDefinition".to_owned());
            }
            Some("ConstructionPlaneAtAngleDefinition") => match &refs[..] {
                // The line it turns about, and the plane the angle is
                // measured from where it names one.
                [l] => put(d, "linearEntity", l),
                [l, p] | [p, l] if linear(l) && planar(p) => {
                    put(d, "linearEntity", l);
                    d.planar_entity = Some(p.clone());
                }
                _ => {}
            },
            Some("ConstructionPlaneDistanceOnPathDefinition") => {
                let path: Vec<serde_json::Value> = refs.iter().map(value).collect();
                d.other
                    .insert("pathEntities".to_owned(), serde_json::Value::Array(path));
            }
            Some(_) => {}
            None => {
                let (t, keys): (&str, &[&str]) = match &refs[..] {
                    [a, b] if planar(a) && planar(b) => (
                        "ConstructionPlaneMidplaneDefinition",
                        &["planarEntityOne", "planarEntityTwo"],
                    ),
                    [a, b] if linear(a) && linear(b) => (
                        "ConstructionPlaneTwoEdgesDefinition",
                        &["linearEntityOne", "linearEntityTwo"],
                    ),
                    [a, b] if linear(a) && point(b) => (
                        "ConstructionPlaneLineAndPointDefinition",
                        &["linearEntity", "pointEntity"],
                    ),
                    [a, b] if point(a) && linear(b) => (
                        "ConstructionPlaneLineAndPointDefinition",
                        &["pointEntity", "linearEntity"],
                    ),
                    [a, b, c] if point(a) && point(b) && point(c) => (
                        "ConstructionPlaneThreePointsDefinition",
                        &["pointEntityOne", "pointEntityTwo", "pointEntityThree"],
                    ),
                    // Tangent to a face at a point of it.
                    [a, b] if matches!(a, Reference::Face(_)) && point(b) => (
                        "ConstructionPlaneTangentAtPointDefinition",
                        &["tangentFace", "pointEntity"],
                    ),
                    _ => return,
                };
                for (k, r) in keys.iter().zip(&refs) {
                    put(d, k, r);
                }
                d.definition_type = Some(t.to_owned());
            }
        }
    }

    /// The entities a construction point's or axis' inputs name, in input
    /// order (faces, edges and vertices by their names, planes, points,
    /// sketch entities); the parameter holder, the frame, the slot wrappers,
    /// the key point and direction inputs and a sketch's profile source are
    /// not entities. `None` when an input does not decode.
    fn datum_inputs(&self, it: &TimelineEntry) -> Option<Vec<Reference>> {
        let mut out = Vec::new();
        for i in self.item_inputs(it) {
            let r = match self.seg.guid_of(i) {
                Some(
                    PARAMETER_HOLDER | FRAME_INPUT | INPUT_SLOT | KEY_POINT | DIRECTION_INPUT
                    | PROFILE_SOURCE,
                ) => continue,
                Some(FACE_REF) => face_input(self.seg, i)
                    .or_else(|| edge_input(self.seg, i))
                    .or_else(|| vertex_input(self.seg, i)),
                Some(ENTITY_REF) => self.entity_target(i),
                _ => None,
            };
            out.push(r?);
        }
        Some(out)
    }

    /// A construction point's detail (mitcad#186): its position from the
    /// key point input (cm) and, by the definition code and the entities
    /// it is made from, the point at a vertex or point (code 0), at the
    /// centre of a circle (3) or where three planes meet (2) *(the
    /// reference models' points; the corpus' one point is a centre)*.
    /// Other definitions keep the position only.
    fn construction_point(&self, it: &TimelineEntry) -> Detail {
        let mut x = ConstructionPointDetail {
            geometry: self
                .item_inputs(it)
                .iter()
                .find_map(|&i| joints::key_point(self.seg, i))
                .map(|(p, _)| p),
            ..ConstructionPointDetail::default()
        };
        let Some(refs) = self.datum_inputs(it) else {
            return Detail::ConstructionPoint(Box::new(x));
        };
        let plane =
            |r: &Reference| matches!(r, Reference::ConstructionPlane(_) | Reference::Face(_));
        let (kind, keys): (&str, &[&str]) = match (datum_code(self.seg, it.id), &refs[..]) {
            (
                Some(0),
                [
                    Reference::Vertex(_)
                    | Reference::ConstructionPoint(_)
                    | Reference::SketchEntity(_),
                ],
            ) => ("ConstructionPointPointDefinition", &["pointEntity"]),
            (Some(3), [Reference::Edge(_) | Reference::Face(_)]) => {
                ("ConstructionPointCenterDefinition", &["circularEntity"])
            }
            (Some(2), [a, b, c]) if plane(a) && plane(b) && plane(c) => (
                "ConstructionPointThreePlanesDefinition",
                &["planeOne", "planeTwo", "planeThree"],
            ),
            _ => return Detail::ConstructionPoint(Box::new(x)),
        };
        let mut d = Definition {
            definition_type: Some(kind.to_owned()),
            ..Definition::default()
        };
        for (k, r) in keys.iter().zip(&refs) {
            d.other.insert((*k).to_owned(), json_value(r));
        }
        x.definition = Some(d);
        Detail::ConstructionPoint(Box::new(x))
    }

    /// A construction axis' detail (mitcad#186): its line from the
    /// direction input, and, by the entities it is made from, the axis
    /// through two points, along an edge or sketch line, through a
    /// cylindrical, conical or toroidal face, or where two planes meet
    /// *(the reference models' axes and the corpus' ones: an edge or face,
    /// a sketch line)*.
    fn construction_axis(&self, it: &TimelineEntry) -> Detail {
        let mut x = ConstructionAxisDetail::default();
        if let Some((_, Some(direction), Some(origin))) = self.directions(it).into_iter().next() {
            x.geometry = Some(Geometry {
                geometry_type: Some("InfiniteLine3D".to_owned()),
                origin: Some(origin),
                direction: Some(direction),
                ..Geometry::default()
            });
        }
        let Some(mut refs) = self.datum_inputs(it) else {
            return Detail::ConstructionAxis(Box::new(x));
        };
        // A profile (the face of an axis normal to a profile at a point:
        // four in the API reference dumps) comes first.
        let profile = self
            .item_inputs(it)
            .into_iter()
            .filter(|&i| self.seg.guid_of(i) == Some(PROFILE_SOURCE))
            .find_map(|i| self.profile_of_source(i));
        if let Some(p) = profile {
            refs.insert(0, p);
        }
        let point = |r: &Reference| {
            matches!(r, Reference::Vertex(_) | Reference::ConstructionPoint(_))
                || matches!(r, Reference::SketchEntity(s)
                    if s.object_type.as_deref() == Some("SketchPoint"))
        };
        let (kind, keys): (&str, &[&str]) = match &refs[..] {
            [Reference::Edge(_) | Reference::SketchEntity(_) | Reference::ConstructionAxis(_)]
                if !point(&refs[0]) =>
            {
                ("ConstructionAxisEdgeDefinition", &["edgeEntity"])
            }
            // (A planar face alone is a perpendicular at a point: the
            // import of the inputs says so once it knows the face.)
            [Reference::Face(_)] => ("ConstructionAxisCircularFaceDefinition", &["circularFace"]),
            [Reference::Face(_) | Reference::Profile(_), p] if point(p) => (
                "ConstructionAxisNormalToFaceAtPointDefinition",
                &["face", "pointEntity"],
            ),
            [a, b] if point(a) && point(b) => (
                "ConstructionAxisTwoPointDefinition",
                &["pointEntityOne", "pointEntityTwo"],
            ),
            [
                Reference::ConstructionPlane(_) | Reference::Face(_),
                Reference::ConstructionPlane(_) | Reference::Face(_),
            ] => (
                "ConstructionAxisTwoPlaneDefinition",
                &["planarEntityOne", "planarEntityTwo"],
            ),
            _ => return Detail::ConstructionAxis(Box::new(x)),
        };
        let mut d = Definition {
            definition_type: Some(kind.to_owned()),
            ..Definition::default()
        };
        for (k, r) in keys.iter().zip(&refs) {
            d.other.insert((*k).to_owned(), json_value(r));
        }
        x.definition = Some(d);
        Detail::ConstructionAxis(Box::new(x))
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
    fn extent_objects(&self, fid: u64, x: &mut ExtrudeDetail, ex: Option<&decode::ExtrudeFields>) {
        let seg = self.seg;
        let slots = decode::extent_slots(seg, fid);
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
        let declared = sides.len();
        if sides.is_empty() {
            sides.push(1);
        }
        // The slots of the sides up to an object: the roles 17 and 18, and
        // a plane or a face of role 5; with fewer than the sides the
        // parameters declare, the bodies of the role 5 slots too (an extent
        // up to a body: the API reference dumps' two such extrusions).
        let object_slots = |bodies: bool| -> Vec<(u32, u64)> {
            slots
                .iter()
                .filter_map(|s| {
                    let input = s.inputs.first().copied()?;
                    let body = seg.guid_of(input) == Some(BODY_REF);
                    let object = s.is_to_object() || (s.role == 5 && (bodies || !body));
                    object.then_some((s.role, input))
                })
                .collect()
        };
        let mut inputs = object_slots(false);
        if inputs.len() < declared {
            inputs = object_slots(true);
        }
        // Two sides both up to an object keep their slots second side
        // first (the API reference dumps: both such extrusions, whose first
        // slot is the object on the side against the stored direction
        // vector, side one's goes along it).
        if sides.len() == 2 {
            inputs.reverse();
        }
        for ((role, input), side) in inputs.into_iter().zip(sides) {
            let entity = match seg.guid_of(input) {
                Some(FACE_REF) => face_input(seg, input),
                Some(ENTITY_REF) => self.entity_target(input),
                Some(BODY_REF) => self.body_input(input),
                _ => None,
            };
            let def = if side == 1 {
                &mut x.extent_one
            } else {
                &mut x.extent_two
            };
            if entity.is_none() && def.is_none() {
                continue;
            }
            let def = def.get_or_insert_with(|| Definition {
                definition_type: Some("ToEntityExtentDefinition".into()),
                ..Definition::default()
            });
            if def.definition_type.as_deref() == Some("ToEntityExtentDefinition") {
                def.entity = entity.or(def.entity.take());
                // The role 18 slot selects the chain of faces joined to the
                // object (4 of 4 in the API reference dumps; the role 17
                // slot of the other 47).
                def.is_chained = Some(role == 18);
            }
        }
        // The extent's direction hint is the stored direction vector, turned
        // round where the flag byte says the extrusion goes against the
        // sketch's normal (24 of 24 one-sided ones and the two-sided ones
        // of the API reference dumps, 51 in all), and the nearest-solution
        // byte of each side.
        let Some(ex) = ex else { return };
        let turn = if ex.flag == Some(1) { -1.0 } else { 1.0 };
        let hint = ex.vector.map(|v| [v[0] * turn, v[1] * turn, v[2] * turn]);
        for (k, def) in [&mut x.extent_one, &mut x.extent_two]
            .into_iter()
            .enumerate()
        {
            let Some(def) = def
                .as_mut()
                .filter(|d| d.definition_type.as_deref() == Some("ToEntityExtentDefinition"))
            else {
                continue;
            };
            def.is_minimum_solution = ex.minimum[k];
            if ex.minimum[k].is_some() {
                def.direction_hint = hint;
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
                        3 => Some("IntersectFeatureOperation".into()),
                        4 => Some("NewBodyFeatureOperation".into()),
                        _ => None,
                    };
                    // A thin extrusion (mitcad#186): the thickness is the
                    // parameter whose holder the block names, else the
                    // role's first.
                    if let Some(thin) = ex.thin {
                        x.is_thin_extrude = Some(thin);
                        if thin {
                            x.thin_extrude_wall_location_one = Some(
                                match ex.thin_location {
                                    Some(1) => "Center",
                                    Some(2) => "Side2",
                                    _ => "Side1",
                                }
                                .to_owned(),
                            );
                            x.thin_extrude_wall_thickness_one = rl
                                .get("Side1WallThickness")
                                .and_then(|v| {
                                    v.iter()
                                        .find(|p| p.holder.is_some() && p.holder == ex.thin_holder)
                                        .or(v.first())
                                })
                                .map(|p| paramref(p));
                            // The second side's wall (mitcad#25).
                            if let Some((location, holder)) = ex.thin_two {
                                x.thin_extrude_wall_location_two = Some(
                                    match location {
                                        1 => "Center",
                                        2 => "Side2",
                                        _ => "Side1",
                                    }
                                    .to_owned(),
                                );
                                x.thin_extrude_wall_thickness_two = rl
                                    .get("Side2WallThickness")
                                    .and_then(|v| {
                                        v.iter().find(|p| p.holder == Some(holder)).or(v.first())
                                    })
                                    .map(|p| paramref(p));
                            }
                        }
                    }
                    x.extent_type = match ex.extent_a {
                        1 => Some("OneSideFeatureExtentType".into()),
                        2 => Some("TwoSidesFeatureExtentType".into()),
                        _ => None,
                    };
                    // A surface extrusion (mitcad#154).
                    x.is_solid = ex.solid;
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
                x.profile = self.extrude_profiles(fid);
                if let Some(p) = pr("ProfileOffset") {
                    x.start_extent = Some(def("OffsetStartDefinition", offset, p));
                }
                if let Some(ex) = &it.extrude {
                    extrude_extent_kinds(ex, &mut x);
                    // The start byte 2: the extrusion starts on a selected
                    // face, offset by its `ProfileOffset` (mitcad#641).
                    if ex.start == Some(2)
                        && let Some(entity) = ex
                            .start_entity
                            .and_then(|s| self.members(s))
                            .filter(|v| v.len() == 1)
                            .map(|mut v| v.remove(0))
                            .filter(|r| {
                                matches!(r, Reference::Face(_) | Reference::ConstructionPlane(_))
                            })
                    {
                        x.start_extent = Some(Definition {
                            definition_type: Some("FromEntityStartDefinition".into()),
                            entity: Some(entity),
                            offset: pr("ProfileOffset"),
                            ..Definition::default()
                        });
                    }
                    // The start byte 0: the extrusion starts on the profile's
                    // plane. A sketch profile's plane, in the sketch's own
                    // coordinates, is its x-y plane (the API reference
                    // dumps: the same plane on 1188 of 1192; a face's
                    // profile has none).
                    if ex.start == Some(0) && x.start_extent.is_none() {
                        let of_sketch = x
                            .profile
                            .as_ref()
                            .is_some_and(|p| p.iter().all(|r| matches!(r, Reference::Profile(_))));
                        x.start_extent = Some(Definition {
                            definition_type: Some("ProfilePlaneStartDefinition".into()),
                            profile_plane: of_sketch.then(|| Geometry {
                                geometry_type: Some("Plane".into()),
                                origin: Some([0.0, 0.0, 0.0]),
                                normal: Some([0.0, 0.0, 1.0]),
                                u_direction: Some([1.0, 0.0, 0.0]),
                                v_direction: Some([0.0, 1.0, 0.0]),
                                ..Geometry::default()
                            }),
                            ..Definition::default()
                        });
                    }
                }
                // The selected profiles' loops (mitcad#96).
                if let Some(loops) = self.profile_loops(fid) {
                    x.other.insert("_f3d_profile_loops".to_owned(), loops);
                }
                // Text profiles whose text is gone (mitcad#332).
                let missing = self.missing_text_profiles(fid);
                if !missing.is_empty() {
                    x.other
                        .insert("_f3d_missing_texts".to_owned(), serde_json::json!(missing));
                }
                // The bodies a join or cut works on: its body inputs
                // (mitcad#96).
                if matches!(
                    x.operation.as_deref(),
                    Some(
                        "JoinFeatureOperation"
                            | "CutFeatureOperation"
                            | "IntersectFeatureOperation"
                    )
                ) {
                    x.participant_bodies = self.extrude_participants(it);
                }
                self.extent_objects(fid, &mut x, it.extrude.as_ref());
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
                // The record's flags (API reference dumps: the 96 version 2
                // revolutions agree on the project-axis, solid and symmetric
                // bytes), and the bodies a join or cut works on, as an
                // extrusion's.
                if let Some(f) = seg.object(fid).and_then(|o| decode::revolve_fields(seg, o)) {
                    x.is_solid = Some(f.solid);
                    x.is_project_axis = f.project_axis;
                    if f.two_sides {
                        let angles = (pr("AlongAngle"), pr("AgainstAngle"));
                        x.extent_definition = Some(Definition {
                            definition_type: Some("TwoSidesAngleExtentDefinition".into()),
                            angle_one: angles.0,
                            angle_two: angles.1,
                            ..Definition::default()
                        });
                    } else if let Some(e) = x.extent_definition.as_mut() {
                        e.is_symmetric = Some(f.symmetric);
                    }
                }
                if matches!(
                    x.operation.as_deref(),
                    Some("JoinFeatureOperation" | "CutFeatureOperation")
                ) {
                    x.participant_bodies = self.extrude_participants(it);
                }
                for e in inputs_of(seg, fid, ENTITY_REF) {
                    if let Some(a) = self.input_entity(e).or_else(|| self.axis_ref(e)) {
                        x.axis = Some(a);
                    }
                }
                // A cylindrical or conical face's axis, or an edge: the axis
                // slot holds a face or edge input instead of an axis
                // (API reference dumps: the six revolutions about a face).
                if x.axis.is_none() {
                    x.axis = inputs_of(seg, fid, FACE_REF).into_iter().find_map(|i| {
                        match recipe::of_input(seg, i)?.kind.as_str() {
                            "edge" => edge_input(seg, i),
                            _ => face_input(seg, i),
                        }
                    });
                }
                x.profile = self.profiles(fid);
                if let Some(loops) = self.profile_loops(fid) {
                    x.other.insert("_f3d_profile_loops".to_owned(), loops);
                }
                Detail::Revolve(Box::new(x))
            }
            Some("FilletFeature") => {
                let mut x = FilletDetail::default();
                let options = fillet_options(seg, fid);
                if let Some(sets) = self.fillet_sets(it, &rl) {
                    x.edge_sets = Some(sets);
                } else if let Some(radii) = rl.get("Radius").filter(|r| !r.is_empty()) {
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
                // The options of each set and the corners (mitcad#154).
                if let Some(sets) = x.edge_sets.as_mut() {
                    // One tangency weight a set, in the sets' order.
                    let weights = rl.get("TangencyWeight").cloned().unwrap_or_default();
                    let paired = weights.len() == sets.len();
                    for (k, s) in sets.iter_mut().enumerate() {
                        if s.radius.is_some() && s.edge_set_type.is_none() {
                            s.edge_set_type = Some("ConstantRadiusFilletEdgeSet".into());
                        }
                        if s.tangency_weight.is_none() && paired {
                            s.tangency_weight = Some(paramref(weights[k]));
                        }
                    }
                }
                if let Some(FilletOptions {
                    sets: opts,
                    rolling,
                }) = options
                {
                    x.is_rolling_ball_corner = rolling;
                    if let Some(sets) = x.edge_sets.as_mut() {
                        for s in sets.iter_mut() {
                            let chord: Option<Reference> = s
                                .other
                                .get("chordLength")
                                .cloned()
                                .and_then(|v| serde_json::from_value(v).ok());
                            let size = s
                                .radius
                                .as_ref()
                                .or(s.start_radius.as_ref())
                                .or(s.offset_one.as_ref())
                                .or(chord.as_ref());
                            let holder =
                                size.and_then(Reference::parameter)
                                    .and_then(|p| p.name.as_deref())
                                    .and_then(|name| {
                                        self.dec.parameters.iter().find(|p| {
                                            p.owner_feature == Some(fid) && p.name == name
                                        })
                                    })
                                    .and_then(|p| p.holder);
                            let Some(holder) = holder else { continue };
                            let matching: Vec<&FilletSetOptions> = opts
                                .iter()
                                .filter(|(id, _)| {
                                    inputs_of(seg, *id, PARAMETER_HOLDER).contains(&holder)
                                })
                                .map(|(_, options)| options)
                                .collect();
                            let [o] = matching.as_slice() else { continue };
                            s.continuity = Some(
                                if o.curvature {
                                    "CurvatureSurfaceContinuityType"
                                } else {
                                    "TangentSurfaceContinuityType"
                                }
                                .into(),
                            );
                            s.is_tangent_chain = Some(o.tangent_chain);
                            if o.asymmetric {
                                s.is_flipped = Some(o.flipped);
                            }
                        }
                    }
                }
                if let Some(sets) = x.edge_sets.as_mut() {
                    self.edge_set_bodies(sets);
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
                // Its corners (mitcad#154).
                x.corner_type = chamfer_corner(seg, fid).map(Into::into);
                // Each set's tangent-chain option, by the record that holds
                // the holder of the set's first size parameter.
                if let Some(sets) = x.edge_sets.as_mut() {
                    let records = inputs_of(seg, fid, CHAMFER_SET);
                    for s in sets.iter_mut() {
                        let holder = s
                            .distance_one
                            .as_ref()
                            .or(s.distance.as_ref())
                            .and_then(Reference::parameter)
                            .and_then(|p| p.name.as_deref())
                            .and_then(|name| {
                                self.dec
                                    .parameters
                                    .iter()
                                    .find(|p| p.owner_feature == Some(fid) && p.name == name)
                            })
                            .and_then(|p| p.holder);
                        let Some(holder) = holder else { continue };
                        let matching: Vec<u64> = records
                            .iter()
                            .copied()
                            .filter(|&r| inputs_of(seg, r, PARAMETER_HOLDER).contains(&holder))
                            .collect();
                        if let [record] = matching.as_slice() {
                            s.is_tangent_chain = chamfer_set_tangent_chain(seg, *record);
                            if s.distance_one.is_some() || s.angle.is_some() {
                                s.is_flipped = chamfer_set_flipped(seg, *record);
                            }
                        }
                    }
                    let chains: Vec<Option<bool>> =
                        sets.iter().map(|s| s.is_tangent_chain).collect();
                    if let Some(&Some(chain)) = chains.first()
                        && chains.iter().all(|c| *c == Some(chain))
                    {
                        x.is_tangent_chain = Some(chain);
                    }
                }
                if let Some(sets) = x.edge_sets.as_mut() {
                    self.edge_set_bodies(sets);
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
                    is_default_direction: hole_flipped(seg, fid).map(|f| !f),
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
                // The hole's extent is a distance unless it keeps a slot of
                // role 5, as on extrusions: with the bodies as its input the
                // hole goes through all, with a plane or face up to it. The
                // controlled hole models (a distance, through all, up to a
                // plane; also tapped, counterbore, countersink and
                // sketch-point holes through all) and all ten holes of the
                // API reference dumps agree.
                // (The byte this code read before as the through-all flag is
                // 0 on four of those ten: it is 1 where one of the two edges
                // of the position is not referenced, 6 of 6.)
                let slots = decode::extent_slots(seg, fid);
                if let Some(slot) = slots.iter().find(|s| s.is_through_all()) {
                    let bodies = slot
                        .inputs
                        .first()
                        .is_some_and(|&i| seg.guid_of(i) == Some(BODY_REF));
                    x.extent_definition = Some(Definition {
                        definition_type: Some(
                            if bodies {
                                "AllExtentDefinition"
                            } else {
                                "OneSideToExtentDefinition"
                            }
                            .into(),
                        ),
                        ..Definition::default()
                    });
                }
                // The position's reference face (the direction input's face)
                // and the two edges it is measured from (the slots of role
                // 33, in order) and the two distances (the reference
                // dimensions). The stored direction vector is the face's
                // outward normal on all ten holes of the API reference
                // dumps, which go against it; a flipped hole goes with it
                // (`is_default_direction`, [`hole_flipped`]), so the vector
                // itself is not taken. A hole on a cylindrical face is
                // measured from a face (the shaft's end) along the axis: its
                // one slot of role 33 holds that face (mitcad#719).
                if let Some((face, _, _)) = self.directions(it).into_iter().next() {
                    let mut edges = slots.iter().filter(|s| s.role == 33).filter_map(|s| {
                        s.inputs
                            .first()
                            .and_then(|&i| edge_input(seg, i).or_else(|| face_input(seg, i)))
                    });
                    x.hole_position_definition = Some(HolePositionDefinition {
                        definition_type: Some("PlaneAndOffsetsHolePositionDefinition".into()),
                        // A face, or a construction or origin plane the holes start on.
                        planar_entity: face.filter(|f| {
                            matches!(f, Reference::Face(_) | Reference::ConstructionPlane(_))
                        }),
                        edge_one: edges.next(),
                        edge_two: edges.next(),
                        offset_one: pr("RefDim1"),
                        offset_two: pr("RefDim2"),
                        ..HolePositionDefinition::default()
                    });
                }
                // A tapped hole's thread is its sub-item (mitcad#35); its
                // depth is the thread's length. A hole whose tail lists no
                // sub-items has the thread that names it as its hole
                // (mitcad#733: a hole tapped `Rc 1/2`).
                let thread = it
                    .tail
                    .iter()
                    .flat_map(|t| t.sub_features.iter().flatten())
                    .find_map(|&s| Some((s, thread_fields(seg, s)?)))
                    .or_else(|| {
                        seg.objects_of(THREAD).find_map(|o| {
                            let t = thread_fields(seg, o.id)?;
                            (t.hole == Some(fid)).then_some((o.id, t))
                        })
                    });
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
                    if let Some(right) = t.right_handed {
                        feature["isRightHanded"] = right.into();
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
                    x.is_right_handed = t.right_handed;
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
                // The quantity's parameter (mitcad#641).
                if let Some(p) = pr("countU") {
                    x.quantity = Some(p);
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
                // Symmetry, compute option and suppressed elements (mitcad#186).
                let s = patterns::circular(seg, seg.data_of(it.id));
                x.is_symmetric = s.symmetric;
                x.pattern_compute_option = s.compute.map(str::to_owned);
                x.suppressed_elements_ids = s.suppressed;
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
                // The quantities' parameters where the file has them
                // (mitcad#641: every pattern of the corpora has `countU` and
                // `CountV`), so that an expression such as a count parameter
                // of the design stays one.
                if let Some(p) = pr("countU") {
                    x.quantity_one = Some(p);
                }
                if let Some(p) = pr("CountV") {
                    x.quantity_two = Some(p);
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
                let second = dirs.next();
                let two = second.is_some();
                if let Some((entity, vector, _)) = second {
                    x.direction_two_entity = entity;
                    x.direction_two = vector;
                } else {
                    // No second direction input: the vector the data
                    // object keeps for it.
                    x.direction_two = patterns::rectangular_direction_two(seg, seg.data_of(it.id));
                }
                // Distance type, symmetry, compute option and suppressed
                // elements (mitcad#186), where the data object agrees with
                // the directions found.
                let s = patterns::rectangular(seg, seg.data_of(it.id));
                if s.two_directions == Some(two) {
                    x.is_symmetric_in_direction_one = s.symmetric_one;
                    x.is_symmetric_in_direction_two = s.symmetric_two.filter(|_| two);
                    x.pattern_compute_option = s.compute.map(str::to_owned);
                    x.pattern_distance_type = s.extent.map(|e| {
                        if e {
                            "ExtentPatternDistanceType"
                        } else {
                            "SpacingPatternDistanceType"
                        }
                        .to_owned()
                    });
                    x.suppressed_elements_ids = s.suppressed;
                }
                Detail::RectangularPattern(Box::new(x))
            }
            // A path pattern (mitcad#468): its objects as a rectangular
            // pattern's, the quantity, the distance, and from its data
            // object the path (as a sweep's), the distance type, the compute
            // option and an empty suppressed list, the flip, orientation and
            // symmetry, and the objects' centre that places the original on
            // the path (mitcad#673).
            Some("PathPatternFeature") => {
                let mut map = serde_json::Map::new();
                match self.pattern_objects(it) {
                    Some((objects, kind)) => {
                        map.insert("inputEntities".to_owned(), json_value(&objects));
                        map.insert("patternEntityType".to_owned(), kind.into());
                    }
                    None => {
                        if let Some(b) = self.body_inputs(it) {
                            map.insert("inputEntities".to_owned(), json_value(&b));
                        }
                    }
                }
                // The quantity's parameter (mitcad#641), else its value.
                if let Some(p) = pr("countU") {
                    map.insert("quantity".to_owned(), json_value(&p));
                } else if let Some((_, value)) =
                    int_inputs(seg, fid).into_iter().find(|(s, _)| *s == 0)
                {
                    map.insert("quantity".to_owned(), json_value(&count_ref(value)));
                }
                if let Some(p) = pr("PathPatternDelta") {
                    map.insert("distance".to_owned(), json_value(&p));
                }
                if let Some(data) = patterns::path(seg, seg.data_of(it.id)) {
                    map.insert("path".to_owned(), self.path_or_error(data.path));
                    // The flag bytes, and the objects' centre that places
                    // the original on the path (mitcad#673).
                    map.insert("isFlipDirection".to_owned(), data.flip.into());
                    map.insert("isOrientationAlongPath".to_owned(), data.along_path.into());
                    map.insert("isSymmetric".to_owned(), data.symmetric.into());
                    map.insert("_f3d_objects_center".to_owned(), json_value(&data.center));
                    // How many bodies the pattern made: none where the
                    // copies of a new body's feature joined its body.
                    if let Some(n) = self.new_bodies(it) {
                        map.insert("_f3d_new_bodies".to_owned(), n.into());
                    }
                    let s = data.settings;
                    if let Some(c) = s.compute {
                        map.insert("patternComputeOption".to_owned(), c.into());
                    }
                    if let Some(e) = s.extent {
                        let t = if e {
                            "ExtentPatternDistanceType"
                        } else {
                            "SpacingPatternDistanceType"
                        };
                        map.insert("patternDistanceType".to_owned(), t.into());
                    }
                    if let Some(ids) = s.suppressed {
                        map.insert("suppressedElementsIds".to_owned(), json_value(&ids));
                    }
                }
                Detail::Other(map)
            }
            Some("MirrorFeature") => {
                // Objects and plane from its selections (mitcad#67).
                let mut map = selections::mirror_map(self, it);
                // Whether bodies are joined with their images: the data
                // object's flag (mitcad#186), else none of the bodies it
                // makes is new (mitcad#96; wrong for a mirror of features,
                // which never joins).
                let settings = patterns::mirror(seg, seg.data_of(it.id));
                let combine = settings
                    .combine
                    .or_else(|| self.new_bodies(it).map(|n| n == 0));
                if let Some(c) = settings.compute {
                    map.insert("patternComputeOption".to_owned(), serde_json::json!(c));
                }
                if map.contains_key("mirrorPlane") && map.contains_key("inputEntities") {
                    if let Some(c) = combine {
                        map.insert("isCombine".to_owned(), serde_json::json!(c));
                    }
                    return Detail::Other(map);
                }
                // Keep a decoded selection even if the other is missing.
                // An origin or construction plane may also be a direct
                // entity reference, as for sketches.
                if !map.contains_key("mirrorPlane") {
                    for e in inputs_of(seg, fid, ENTITY_REF) {
                        if let Some(p) = self.plane_ref(e) {
                            map.insert("mirrorPlane".to_owned(), json_value(&p));
                        }
                    }
                }
                // Mirrored bodies (mitcad#33).
                if !map.contains_key("inputEntities")
                    && let Some(b) = self.body_inputs(it)
                {
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
                let inputs: Vec<u64> = self
                    .item_inputs(it)
                    .into_iter()
                    .filter(|&i| {
                        !matches!(
                            seg.guid_of(i),
                            Some(BODY_INPUT | BODY_REF | PARAMETER_HOLDER | ENTITY_REF | FACE_REF)
                        )
                    })
                    .collect();
                let matrix = inputs
                    .iter()
                    .find_map(|&i| rigid_matrix(seg.data_of(i)))
                    .or_else(|| {
                        inputs
                            .iter()
                            .any(|&i| identity_move_data(seg, i))
                            .then_some(IDENTITY_MAT4)
                    });
                if let Some(m) = &matrix {
                    map.insert("transform".to_owned(), json_value(m));
                }
                // How the move is defined (the reference models' moves: a
                // rotation, a translation along the axes, a free move): its
                // parameters by their roles (`RotateAngle`, `TranslateX`,
                // ...) and the axis of a rotation (the entity input); a free
                // move has neither parameters nor point, direction or entity
                // inputs, its matrix is the definition.
                let all_inputs = self.item_inputs(it);
                let of_class = |class: &str| {
                    all_inputs
                        .iter()
                        .copied()
                        .filter(|&i| seg.guid_of(i) == Some(class))
                        .collect::<Vec<u64>>()
                };
                let axis = || {
                    let entities = of_class(ENTITY_REF);
                    match entities.as_slice() {
                        [e] => self.entity_target(*e),
                        _ => None,
                    }
                };
                let mut definition = serde_json::Map::new();
                if let (Some(angle), Some(axis)) = (pr("RotateAngle"), axis()) {
                    definition.insert("_type".to_owned(), "MoveFeatureRotateDefinition".into());
                    definition.insert("angle".to_owned(), json_value(&angle));
                    definition.insert("axisEntity".to_owned(), json_value(&axis));
                } else if let (Some(x), Some(y), Some(z)) =
                    (pr("TranslateX"), pr("TranslateY"), pr("TranslateZ"))
                {
                    definition.insert(
                        "_type".to_owned(),
                        "MoveFeatureTranslateXYZDefinition".into(),
                    );
                    definition.insert("xDistance".to_owned(), json_value(&x));
                    definition.insert("yDistance".to_owned(), json_value(&y));
                    definition.insert("zDistance".to_owned(), json_value(&z));
                } else if let Some(m) = &matrix
                    && rl.is_empty()
                    && [
                        PARAMETER_HOLDER,
                        KEY_POINT,
                        DIRECTION_INPUT,
                        ENTITY_REF,
                        FRAME_INPUT,
                    ]
                    .iter()
                    .all(|c| of_class(c).is_empty())
                {
                    definition.insert("_type".to_owned(), "MoveFeatureFreeMoveDefinition".into());
                    definition.insert("transform".to_owned(), json_value(m));
                }
                if !definition.is_empty() {
                    map.insert(
                        "definition".to_owned(),
                        serde_json::Value::Object(definition),
                    );
                }
                Detail::Other(map)
            }
            // The body a removal takes out of the design, and the faces a
            // delete-face takes away, by their names (the items' body and
            // face inputs); the bodies a copy or cut-and-paste of bodies
            // takes, by the same inputs.
            Some("RemoveBodyFeature") => {
                let mut map = serde_json::Map::new();
                if let Some(mut b) = self.body_inputs(it)
                    && b.len() == 1
                {
                    map.insert("itemToRemove".to_owned(), json_value(&b.remove(0)));
                }
                Detail::Other(map)
            }
            // An emboss (mitcad#458): its profiles (and their loops, as an
            // extrusion's), the face it lies on, and its parameters by
            // their roles: the depth (negative: sunk) and the profiles'
            // placement on the face (horizontal and vertical distances,
            // rotation).
            _ if classes::object_type(&it.class) == Some("EmbossFeature") => {
                let mut x = EmbossDetail {
                    input_faces: self.face_inputs(it),
                    depth: pr("Depth"),
                    horizontal_distance: pr("Horizontal Distance"),
                    vertical_distance: pr("Vertical Distance"),
                    rotation_angle: pr("Rotation Angle"),
                    ..EmbossDetail::default()
                };
                // A reference to the profiles' sketch per selected profile
                // (its profile source; the slot holds no profile inputs an
                // extrusion's has).
                if let Some(loops) = self.profile_loops(fid) {
                    let count = loops.as_array().map_or(0, Vec::len);
                    if let Some(sid) = self.profile_sketch(fid)
                        && count > 0
                    {
                        let f = self.feat_of_sk.get(&sid).copied();
                        let r = Reference::Profile(ProfileRef {
                            sketch: Some(self.name(f)),
                            sketch_timeline_index: Some(self.pos(f)),
                            ..ProfileRef::default()
                        });
                        x.profile = Some(vec![r; count]);
                    }
                    x.other.insert("_f3d_profile_loops".to_owned(), loops);
                }
                Detail::Emboss(Box::new(x))
            }
            Some("DeleteFaceFeature") => {
                let mut map = serde_json::Map::new();
                if let Some(f) = self.face_inputs(it) {
                    map.insert("deletedFaces".to_owned(), json_value(&f));
                }
                Detail::Other(map)
            }
            // Moved faces (mitcad#343): the faces by their face inputs, the
            // motion the rigid 4x4 matrix (row-major, cm) of its transform
            // input, as a move's ([`classes::MOVE_DATA`]); a transform that
            // stores no matrix is the identity.
            Some("MoveFaceFeature") => {
                let mut x = MoveFaceDetail {
                    faces: self.face_inputs(it),
                    ..MoveFaceDetail::default()
                };
                let moves: Vec<u64> = self
                    .item_inputs(it)
                    .into_iter()
                    .filter(|&i| seg.guid_of(i) == Some(MOVE_DATA))
                    .collect();
                let matrix = moves
                    .iter()
                    .find_map(|&i| rigid_matrix(seg.data_of(i)))
                    .or_else(|| {
                        moves
                            .iter()
                            .any(|&i| identity_move_data(seg, i))
                            .then_some(IDENTITY_MAT4)
                    });
                if let Some(m) = &matrix {
                    x.other.insert("transform".to_owned(), json_value(m));
                }
                Detail::MoveFace(Box::new(x))
            }
            // An align of bodies (mitcad#641): its bodies by their body
            // inputs and the motion it made, the rigid 4x4 matrix (row-major,
            // cm) of its transform input ([`classes::MOVE_DATA`]), as a
            // move's. The geometry it aligns (key points, directions and
            // frames of faces) is not decoded.
            _ if classes::object_type(&it.class) == Some("AlignFeature") => {
                let mut map = serde_json::Map::new();
                if let Some(b) = self.body_inputs(it) {
                    map.insert("inputEntities".to_owned(), json_value(&b));
                }
                let matrix = self
                    .item_inputs(it)
                    .into_iter()
                    .filter(|&i| seg.guid_of(i) == Some(MOVE_DATA))
                    .find_map(|i| rigid_matrix(seg.data_of(i)));
                if let Some(m) = &matrix {
                    map.insert("transform".to_owned(), json_value(m));
                }
                Detail::Other(map)
            }
            Some("CutPasteBodiesFeature") => {
                let mut map = serde_json::Map::new();
                if let Some(b) = self.body_inputs(it) {
                    map.insert("sourceBody".to_owned(), json_value(&b));
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
            // The removed faces by their names, else the shelled body, and
            // the thickness inside or outside (mitcad#154: the cadmpeg
            // format description's face group and outward flag; the outward
            // flag is the parameter's role here).
            Some("ShellFeature") => {
                // The two bytes after the root part: the tangent-chain flag
                // and the shell type (1 rounded, 0 sharp): 4 of 4 shells of
                // the API reference dumps and the 4 controlled models.
                let d = seg.data_of(fid);
                let flags = seg
                    .root_part(d)
                    .map(|r| (d.get(r.end).copied(), d.get(r.end + 1).copied()));
                let (chain, rounded) = match flags {
                    Some((Some(c @ (0 | 1)), Some(t @ (0 | 1)))) => (Some(c == 1), Some(t == 1)),
                    _ => (None, None),
                };
                Detail::Shell(Box::new(ShellDetail {
                    inside_thickness: pr("innerThickness"),
                    outside_thickness: pr("outerThickness"),
                    input_entities: self.face_inputs(it).or_else(|| self.body_inputs(it)),
                    is_tangent_chain: chain,
                    shell_type: rounded.map(|r| {
                        if r {
                            "RoundedOffsetShellType"
                        } else {
                            "SharpOffsetShellType"
                        }
                        .to_owned()
                    }),
                    ..ShellDetail::default()
                }))
            }
            Some("OffsetFacesFeature") => Detail::OffsetFaces(Box::new(OffsetFacesDetail {
                distance: pr("distance"),
                // Its faces by their names (mitcad#154).
                input_faces: self.face_inputs(it),
                ..OffsetFacesDetail::default()
            })),
            // The choices, plane and placement (mitcad#25).
            Some("CoilFeature") => self.coil(it),
            // Base and edge flanges (mitcad#211).
            None if it.class == sheet::FLANGE => self.flange(it, &rl),
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
            // A construction axis (mitcad#154, mitcad#186): its line from its
            // direction input (a point and a displacement, the cadmpeg format
            // description's solved-axis carrier) and what it is made from.
            Some("ConstructionAxis") => self.construction_axis(it),
            // A construction point (mitcad#186): where it is, from its key
            // point input, and the vertex it was made at.
            None if it.class == CONSTRUCTION_POINT => self.construction_point(it),
            // Box, cylinder, sphere and torus primitives: operation, the
            // plane or face, the placement in its frame and the sizes
            // (mitcad#154, mitcad#25).
            None if matches!(it.class.as_str(), BOX | CYLINDER | SPHERE | TORUS) => {
                self.primitive(it)
            }
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
                self.plane_inputs(it, &mut x);
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
            // The constraints of assembly relationships (mitcad#155).
            Some("GeometricRelationshipFeature") => self.geometric_relationship(it),
            _ if it.class == TANGENT_RELATIONSHIP => self.tangent_relationship(it),
            // What a derive brings from other documents (mitcad#324).
            _ if it.class == DERIVED_CONTEXT => self.derived_context(it),
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
        point_feature: HashMap::new(),
        tags: sweeps::EntityTags::default(),
        guids: joints::Guids::new(seg),
        component_names: dec
            .components
            .iter()
            .filter_map(|c| Some((c.id, c.name.clone().filter(|n| !n.is_empty())?)))
            .collect(),
        sub_item: items
            .iter()
            .flat_map(|it| {
                it.tail
                    .iter()
                    .flat_map(|t| t.sub_features.iter().flatten())
                    .map(move |&s| (s, it.id))
            })
            .filter(|&(s, _)| !items.iter().any(|it| it.id == s) && seg.is_kind_of(s, BASE_FEATURE))
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
    // A sketch that names its feature through another object (a recipe,
    // `7ACC2A03`) is found from the feature, which names its sketch.
    for f in seg.objects_of(SKETCH_FEATURE) {
        if b.sk_of_feat.contains_key(&f.id) {
            continue;
        }
        let sketch = seg
            .ref_ids(seg.data(f))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(SKETCH) && !b.feat_of_sk.contains_key(&r));
        if let Some(s) = sketch {
            b.sk_of_feat.insert(f.id, s);
            b.feat_of_sk.insert(s, f.id);
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
    // (The first such reference is the point's own; later ones may be the
    // design's origin, a point's input.)
    for f in seg.objects_of(CONSTRUCTION_POINT) {
        if let Some(r) = seg
            .ref_ids(seg.data(f))
            .into_iter()
            .find(|&r| seg.guid_of(r) == Some(ORIGIN_POINT))
        {
            b.point_feature.entry(r).or_insert(f.id);
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
    // A component without a name of its own has the default name
    // `Component<n>`: the number is the one in the tail of its occurrence item
    // (the tail has no base name then).
    let mut default_names: HashMap<u64, String> = HashMap::new();
    for it in items {
        if it.name.as_deref() == Some("")
            && classes::object_type(&it.class) == Some("Occurrence")
            && let Some(t) = it
                .tail
                .as_ref()
                .filter(|t| t.base_name.is_empty() && t.index > 0)
            && let Some(c) = occurrence_item_component(seg, it.id)
            && !component_names.contains_key(&c)
        {
            default_names
                .entry(c)
                .or_insert_with(|| format!("Component{}", t.index));
        }
    }
    let owners = item_owners(seg);
    let item_parts = parts::item_parts(seg);
    let mut tl_items = Vec::new();
    // The parts of items in other components follow their items, numbered
    // after the timeline's items (mitcad#169).
    let mut next_index = items.len() as i64;
    let mut entries: Vec<(&TimelineEntry, Option<TimelineEntry>)> = Vec::new();
    for it in items {
        entries.push((it, None));
        for part in parts::part_entries(seg, it, &item_parts, &b.pos_of, &owners) {
            entries.push((it, Some(part)));
        }
    }
    for (master, part) in &entries {
        let it = part.as_ref().unwrap_or(master);
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
            f3d.after = t.after.map(|a| super::stream::hex(&a));
            f3d.sub_features = Some(t.sub_features.clone());
        }
        if let Some(t) = &it.tail {
            f3d.last_result_no = modelled_thread_result(seg, it.id, t);
        }
        if it.class == COMPONENT_FROM_BODIES {
            f3d.made_result_no = made_result(seg, it.id);
        } else if it.tail.as_ref().is_some_and(|t| t.result_no <= 0) {
            f3d.made_result_no = it.tail.as_ref().and_then(|t| sub_base_result(seg, t));
        }
        if matches!(it.type_name, Some("ConstructionPlane" | "ConstructionAxis"))
            || it.class == CONSTRUCTION_POINT
        {
            f3d.datum_code = datum_code(seg, it.id);
        }
        let index = if part.is_some() {
            f3d.part_of = Some(master.pos as i64);
            next_index += 1;
            next_index - 1
        } else {
            it.pos as i64
        };
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
                solid: ex.solid,
                start: ex.start.map(u32::from),
                sides: ex.sides.map(|s| s.to_vec()),
                thin: ex.thin,
                thin_location: ex.thin_location.map(u32::from),
                thin_holder: ex.thin_holder,
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
                    .and_then(|c| {
                        component_names
                            .get(&c)
                            .or_else(|| default_names.get(&c))
                            .cloned()
                    })
                    .or(Some(String::new()))
            }
            None if it.class == GROUP => group_name(seg, it.id).filter(|n| !n.is_empty()),
            n => n.clone(),
        };
        // Suppressed in the file (mitcad#180): its tail's state bytes.
        let is_suppressed = it
            .tail
            .as_ref()
            .is_some_and(|t| t.state() == decode::ItemState::Suppressed)
            .then_some(true);
        // After the timeline marker: the third byte after the
        // item's reference.
        let is_rolled_back = it
            .tail
            .as_ref()
            .is_some_and(decode::ItemTail::rolled_back)
            .then_some(true);
        tl_items.push(TimelineItem {
            index: Some(index),
            name: Some(name),
            is_suppressed,
            is_rolled_back,
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

    let marker = tl_items
        .iter()
        .position(|t| t.is_rolled_back == Some(true))
        .map(|p| p as i64);

    // Parameters: user and model, createdBy for model parameters.
    let (mut user, mut model) = (Vec::new(), Vec::new());
    for p in &dec.parameters {
        let mut rec = Parameter {
            name: Some(p.name.clone()),
            expression: Some(p.expression.clone()),
            // A text parameter has no number.
            value: (p.unit != "Text").then_some(p.value),
            unit: Some(p.unit.clone()),
            comment: p.comment.clone(),
            // (Written when true, as the other flags.)
            is_favorite: p.is_favorite().then_some(true),
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
        for path in hidden_paths(seg) {
            hide(nodes, &path);
        }
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
                    version: urn_version(&x.urn),
                    guids: Some(vec![x.guid_a.to_lowercase(), x.guid_b.to_lowercase()]),
                    document_guid: Some(x.guid_c.to_lowercase()),
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
                    name: Some(
                        c.name
                            .clone()
                            .filter(|n| !n.is_empty())
                            .or_else(|| default_names.get(&c.id).cloned()),
                    ),
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
            // The marker is before the first item rolled back.
            original_marker_position: marker.map(Some),
            restored_marker_position: marker.map(Some),
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

/// The version a document URN names (`…?version=6`).
pub fn urn_version(urn: &str) -> Option<u32> {
    let (_, query) = urn.split_once('?')?;
    query
        .split('&')
        .find_map(|p| p.strip_prefix("version="))?
        .parse()
        .ok()
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

/// The occurrence paths whose light bulbs are off: per
/// [`HIDDEN_OCCURRENCE`] object its last `u32 n | n × ref` list whose
/// references are all occurrences of this file (1 to 16 levels, from the
/// root component down); without one, an object referring to one
/// occurrence of this file only hides that one (a top-level occurrence of a
/// component of another document, whose path list is empty). Paths inside
/// components of other documents (context references) are left out: those
/// documents' own files hold their light bulbs.
fn hidden_paths(seg: &Segment) -> Vec<Vec<u64>> {
    const MAX_LEVELS: u32 = 16;
    let mut out = Vec::new();
    for o in seg.objects_of(HIDDEN_OCCURRENCE) {
        let d = seg.data(o);
        let Some(start) = header_end(d) else {
            continue;
        };
        let mut found = None;
        for p in start..d.len().saturating_sub(4) {
            let Some(n) = u32_at(d, p).filter(|n| (1..=MAX_LEVELS).contains(n)) else {
                continue;
            };
            let mut q = p + 4;
            let mut path = Vec::new();
            while path.len() < n as usize {
                match seg.ref_at(d, q) {
                    Some(r) if r.context.is_none() && seg.guid_of(r.id) == Some(OCCURRENCE) => {
                        path.push(r.id);
                        q = r.end;
                    }
                    _ => break,
                }
            }
            if path.len() == n as usize {
                found = Some(path);
            }
        }
        let inside_other_document = || {
            seg.context_refs_in(d, start, d.len())
                .iter()
                .any(|(_, r)| r.guid == OCCURRENCE)
        };
        if found.is_none() && !inside_other_document() {
            // An occurrence of a component of another document at the top
            // level keeps its path list empty: the one occurrence of this
            // file the object refers to.
            let mut occurrences: Vec<u64> = seg
                .refs_in(d, start, d.len())
                .into_iter()
                .filter(|(_, r)| r.context.is_none() && seg.guid_of(r.id) == Some(OCCURRENCE))
                .map(|(_, r)| r.id)
                .collect();
            occurrences.dedup();
            if let [o] = occurrences[..] {
                found = Some(vec![o]);
            }
        }
        out.extend(found);
    }
    out
}

/// Turns the light bulb of the occurrence at `path` (object ids from the
/// top level down) off.
fn hide(nodes: &mut [OccurrenceNode], path: &[u64]) {
    let Some((first, rest)) = path.split_first() else {
        return;
    };
    let Some(node) = nodes
        .iter_mut()
        .find(|n| n.f3d.as_ref().and_then(|f| f.object_id) == Some(*first))
    else {
        return;
    };
    if rest.is_empty() {
        node.is_light_bulb_on = Some(false);
    } else if let Some(children) = node.children.as_mut() {
        hide(children, rest);
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
            guids: (!i.guids.is_empty()).then(|| i.guids.clone()),
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

    /// Any class stands for the hole here.
    const HOLE_TEST: &str = "E8063467-0000-0000-0000-000000000001";

    fn entry(name: &str, expression: &str) -> ParameterEntry {
        ParameterEntry {
            id: 0,
            number: 0,
            holder: None,
            expression: expression.into(),
            role: String::new(),
            comment: None,
            unit: "mm".into(),
            name: name.into(),
            value: 0.0,
            list: None,
            owner_feature: None,
            class_version: 8,
            gap0: Vec::new(),
            gap1: Vec::new(),
            end: 0,
        }
    }

    #[test]
    fn dependents_are_transitive_and_in_parameter_order() {
        let params = [
            entry("d3", "d2 / 2"),
            entry("width", "10 mm"),
            entry("d2", "width + 1 mm"),
            entry("d4", "d3"),
            entry("d5", "5 mm"),
            // Cycles do not loop.
            entry("d6", "d7"),
            entry("d7", "d6"),
        ];
        let names = |i: usize| dependents(&params[i], &params);
        assert_eq!(names(1), ["d3", "d2", "d4"]);
        assert_eq!(names(2), ["d3", "d4"]);
        assert!(names(4).is_empty());
        assert_eq!(names(5), ["d7"]);
    }

    /// A hole record: root part, a null reference, nine bytes, `u32 type`,
    /// `u32 flipped` (mitcad#25).
    fn hole_object(kind: u32, flipped: u32) -> Vec<u8> {
        let mut d = vec![0, 0, 0, 0, 0, 0];
        d.push(1);
        d.extend(0u64.to_le_bytes());
        d.extend([0, 0]);
        d.extend([0u8; 9]);
        d.extend(kind.to_le_bytes());
        d.extend(flipped.to_le_bytes());
        d.extend([0; 8]);
        d
    }

    #[test]
    fn a_hole_reads_its_direction_flag_behind_the_first_null_reference() {
        const ROOT: &str = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";
        const NULL: &str = "23FD63FB-0000-0000-0000-000000000000";
        let mut flags = Vec::new();
        for (kind, flipped) in [(0, 0), (1, 1), (2, 0), (3, 0), (0, 2)] {
            let mut doc = crate::tests::Doc::default();
            doc.class(ROOT, "", 0);
            doc.class(NULL, ROOT, 0);
            doc.class(HOLE_TEST, ROOT, 7);
            doc.obj(0, NULL, vec![0, 0]);
            doc.obj(60, HOLE_TEST, hole_object(kind, flipped));
            flags.push(hole_flipped(&doc.segment(), 60));
        }
        // A type or a flag out of range is not a hole's.
        assert_eq!(flags, [Some(false), Some(true), Some(false), None, None]);
    }

    /// A reference as the layout of a thread object has them: `01 id 00 00`.
    fn plain_ref_end(d: &[u8], q: usize) -> Option<usize> {
        (d.get(q) == Some(&1) && q + 11 <= d.len()).then_some(q + 11)
    }

    /// The bytes after a thread's pitch diameter (synthetic): the hole
    /// reference or its null byte, two references, the handedness byte.
    #[test]
    fn thread_handedness_follows_the_references() {
        let reference = |id: u8| vec![1, id, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        // A thread item: a null hole reference, two references.
        for (byte, want) in [(1u8, Some(true)), (0, Some(false)), (7, None)] {
            let mut d = vec![0];
            d.extend(reference(5));
            d.extend(reference(6));
            d.extend([byte, 0, 0, 0, 0]);
            assert_eq!(
                thread_handedness(&d, 0, plain_ref_end).map(|(right, _)| right),
                want
            );
        }
        // A tapped hole's thread: the hole is a reference.
        let mut d = reference(4);
        d.extend(reference(5));
        d.extend(reference(6));
        d.extend([1, 0, 0]);
        assert_eq!(thread_handedness(&d, 0, plain_ref_end), Some((true, 33)));
        // Too short, or the references missing.
        assert_eq!(thread_handedness(&[0, 1, 2], 0, plain_ref_end), None);
        assert_eq!(thread_handedness(&[], 0, plain_ref_end), None);
    }

    fn fields(extent_a: u32, sides: [u32; 2], flag: u8) -> decode::ExtrudeFields {
        decode::ExtrudeFields {
            operation_code: 1,
            extent_a,
            extent_b: 0,
            direction: None,
            vector: Some([0.0, 0.0, 1.0]),
            flag: Some(flag),
            full_length: Some(false),
            solid: Some(true),
            start: Some(0),
            sides: Some(sides),
            minimum: [None; 2],
            thin: None,
            thin_location: None,
            thin_holder: None,
            thin_two: None,
            start_entity: None,
        }
    }

    fn distance_side() -> Option<Definition> {
        Some(Definition {
            definition_type: Some("DistanceExtentDefinition".into()),
            ..Definition::default()
        })
    }

    /// The flag byte gives a side through all its direction (0 along the
    /// sketch's normal, 1 against it), for whichever side it is; a symmetric
    /// extent keeps its distance and length byte.
    #[test]
    fn extent_kinds_follow_the_extrude_record() {
        let positive = |x: &ExtrudeDetail, side: usize| {
            let e = if side == 1 {
                &x.extent_one
            } else {
                &x.extent_two
            };
            let e = e.as_ref().unwrap();
            assert_eq!(
                e.definition_type.as_deref(),
                Some("ThroughAllExtentDefinition")
            );
            e.is_positive_direction
        };
        for (flag, want) in [(0, true), (1, false)] {
            let mut x = ExtrudeDetail::default();
            extrude_extent_kinds(&fields(1, [4, 0], flag), &mut x);
            assert_eq!(positive(&x, 1), Some(want));
            assert_eq!(x.has_two_extents, Some(false));
            // Two sides, the second through all.
            let mut x = ExtrudeDetail {
                extent_one: distance_side(),
                ..ExtrudeDetail::default()
            };
            extrude_extent_kinds(&fields(2, [1, 4], flag), &mut x);
            assert_eq!(positive(&x, 2), Some(want));
            assert_eq!(x.has_two_extents, Some(true));
            assert_eq!(x.extent_one.as_ref().unwrap().is_symmetric, Some(false));
        }
        // A byte that is no flag.
        let mut x = ExtrudeDetail::default();
        extrude_extent_kinds(&fields(1, [4, 0], 7), &mut x);
        assert_eq!(positive(&x, 1), None);
        // Symmetric through all: the whole length by definition.
        let mut x = ExtrudeDetail::default();
        extrude_extent_kinds(&fields(3, [4, 4], 0), &mut x);
        assert_eq!(x.extent_type.as_deref(), Some("SymmetricFeatureExtentType"));
        assert_eq!(positive(&x, 1), Some(true));
        assert_eq!(x.symmetric_extent.unwrap().is_full_length, Some(true));
        // A symmetric distance: the extent is the symmetric one.
        let mut x = ExtrudeDetail {
            extent_one: distance_side(),
            ..ExtrudeDetail::default()
        };
        extrude_extent_kinds(&fields(3, [1, 1], 0), &mut x);
        let one = x.extent_one.unwrap();
        assert_eq!(
            one.definition_type.as_deref(),
            Some("SymmetricExtentDefinition")
        );
        assert_eq!(one.is_full_length, Some(false));
        assert_eq!(x.symmetric_extent, Some(one));
    }

    #[test]
    fn name_tokens_follow_the_regex() {
        let t: Vec<&str> = name_tokens("d1 * 2 + 1e5 mm/_x9").collect();
        assert_eq!(t, vec!["d1", "e5", "mm", "_x9"]);
        let t: Vec<&str> = name_tokens("12ab3 (Länge)").collect();
        assert_eq!(t, vec!["ab3", "L", "nge"]);
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;

    /// An object's data: header (a 3-character number, an id), then bytes.
    fn object(body: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        d.extend(3u32.to_le_bytes());
        d.extend(b"406");
        d.extend(2263u64.to_le_bytes());
        d.extend(0u32.to_le_bytes());
        d.extend(body);
        d
    }

    fn doubles(v: &[f64]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn a_kept_plane_is_its_origin_and_two_unit_directions() {
        let mut body = vec![0u8, 0, 0];
        body.extend(doubles(&[0.0, 0.25, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0]));
        body.extend([1, 1]);
        let g = plane_of_frame(&object(&body)).expect("a plane");
        assert_eq!(g.origin, Some([0.0, 0.25, 0.0]));
        assert_eq!(g.u_direction, Some([1.0, 0.0, 0.0]));
        assert_eq!(g.v_direction, Some([0.0, 0.0, -1.0]));
        // u x v
        assert_eq!(g.normal, Some([-0.0, 1.0, 0.0]));
        // Directions that are no unit vectors are no plane.
        let mut bad = vec![0u8; 3];
        bad.extend(doubles(&[0.0, 0.25, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, -1.0]));
        assert!(plane_of_frame(&object(&bad)).is_none());
    }
}
