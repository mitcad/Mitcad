// SPDX-License-Identifier: MIT
//! Joints, as-built joints, joint origins and ground items (mitcad#66)
//! *(read from the corpus' joints, as-built joints and joint origins, whose
//! stored occurrence placements check the frames, and the reference
//! model with a ground item; no reference model has a joint)*.
//!
//! A joint ([`JOINT`], class versions 3 and 4) after its root part:
//!
//! ```text
//! u8 opposed | 2 bytes (version 3) or 10 bytes (version 4)
//! 2 × side: ref geometry | 6 bytes (version 4) | u8 1 (no matrix) or
//!           u8 0 + f64[16] frame | [str16 GUID (later writers)]
//! u32 0 | refs alignAngle, alignOffsetZ, alignOffsetX, alignOffsetY
//! 6 bytes | u32 2 | 2 × ref placement | ref state | ...
//! ```
//!
//! A side's geometry is a frame input ([`FRAME_INPUT`]: the frame and the
//! key point and direction inputs it was built from, with the faces and
//! edges they name), a joint origin ([`JOINT_ORIGIN`]), or an entity input
//! whose target is in another document (a fastener's origin; no matrix:
//! the frame is the identity of its component). The stored frames are in
//! the side's component. The two placements ([`PLACEMENT`]) give each
//! side's occurrence (a path of occurrence GUIDs, from the joint's
//! component down) and a frame: side one's frame with the alignment
//! applied, side two's frame. With the occurrences' placements after the
//! joint (`placements.rs`), `world(one) · frame one ·
//! Rx(π if opposed) · T(−offset) · Rz(−angle) = world(two) · frame two`
//! *(the joint test model's rigid joints at 30°, flipped and not,
//! mitcad#81)*; `opposed` is the joint's flip (`isFlipped`): without it
//! the frames' z axes point the same way.
//!
//! The state ([`JOINT_STATE`]) gives the motion: `u8 1 | u32 n | n × (ref
//! occurrence, u32) | u8 f | [u32 if f = 1] | 6 × (u32 0, or u32 1 +
//! 51-byte motion) | u32 type | u32 ordinal | u8 | u32 placements`. The six
//! slots are the turns about x, y, z and the slides along x, y, z
//! ([`MOTION_SLOTS`]); a motion holds its maximum, minimum and rest values
//! (f64 at 23, 31, 39, whether enabled or not) and an axis code (u8 at
//! 47), not the current value *(the joint test model, mitcad#81: its
//! current values are only in the occurrences' placements and a captured
//! position's parameters)*.
//!
//! An as-built joint ([`AS_BUILT_JOINT`]): `ref frame input or 00 | ref
//! placement of the geometry | u32 n | n limit parameters | u32 c | c ×
//! placement | ref state | ...`; its occurrences are the first two of the
//! `c` placements, whose frames record the placement of each occurrence
//! relative to the joint's frame. With the motion type 11 the item is a
//! rigid group (`RigidGroup`): its members are the `c` placements' paths
//! (with "include children", the members' children are listed too), each
//! frame the group's frame in that member's component.
//!
//! A ground item ([`GROUND_OCCURRENCE`]) refers to a placement whose path
//! names the grounded occurrence (the reference model's ground item names
//! the occurrence its dump grounds).
//!
//! A captured position ([`SNAPSHOT`], mitcad#75) lists placements, each
//! with the occurrence's matrix in the item's component *(read from the
//! designs whose joints and as-built joints after a captured position hold
//! at its matrices)*. A path level ([`CONTEXT_LEVEL`]) names `n`
//! occurrences: one in the file, several for a path inside a component of
//! another document *(captured positions of an inserted assembly's
//! parts)*.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::super::classes::*;
use super::super::decode::{self, ParameterEntry, TimelineEntry};
use super::super::ir::*;
use super::super::recipe;
use super::super::stream::{Segment, f64_at, f64s_at, header_end, str16_at, u32_at};
use super::{Builder, edge_input, face_input, inputs_of, json_value, paramref, rigid_matrix};

/// Occurrence and component objects by the GUIDs the occurrence paths
/// name: every GUID-shaped `str16` of the objects, kept where only one
/// object of the class has it.
#[derive(Default)]
pub(super) struct Guids {
    occurrences: HashMap<String, Option<u64>>,
    components: HashMap<String, Option<u64>>,
}

/// The 36-character `str16` values of object data, in order.
fn guid_strings(d: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 4 < d.len() {
        if u32_at(d, p) == Some(36)
            && let Some((s, e)) = str16_at(d, p)
        {
            if is_guid(&s) {
                out.push(s.to_lowercase());
            }
            p = e;
        } else {
            p += 1;
        }
    }
    out
}

fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.char_indices().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

impl Guids {
    pub(super) fn new(seg: &Segment) -> Self {
        let mut out = Self::default();
        for (class, map) in [
            (OCCURRENCE, &mut out.occurrences),
            (COMPONENT, &mut out.components),
        ] {
            for o in seg.objects_of(class) {
                for g in guid_strings(seg.data(o)) {
                    map.entry(g)
                        .and_modify(|v| {
                            if *v != Some(o.id) {
                                *v = None;
                            }
                        })
                        .or_insert(Some(o.id));
                }
            }
        }
        out
    }
}

/// One level of an occurrence path ([`CONTEXT_LEVEL`]).
struct Level {
    /// The GUIDs of the occurrences, from the outermost down: one for a
    /// level in the file, several for a path inside a component of
    /// another document *(captured positions of an inserted assembly's
    /// parts)*, none for the component itself.
    occurrences: Vec<String>,
    /// The GUID of the component the (first) occurrence sits in.
    context: Option<String>,
}

/// A level: `u32 n`, the GUIDs of `n` occurrences, of the document and
/// the component the last one places, then of the document and the
/// component the first one sits in.
fn level(seg: &Segment, id: u64) -> Option<Level> {
    let d = seg.data_of(id);
    let p = seg.root_part(d)?.end;
    let n = u32_at(d, p)? as usize;
    let g = guid_strings(&d[p..]);
    match n {
        0 if g.len() >= 3 => Some(Level {
            occurrences: Vec::new(),
            context: Some(g[2].clone()),
        }),
        1..=64 if g.len() >= n + 4 => Some(Level {
            occurrences: g[..n].to_vec(),
            context: Some(g[n + 3].clone()),
        }),
        _ => None,
    }
}

/// A rigid matrix at `p`: `f64[16]` row-major, last row `0 0 0 1`.
fn matrix_at(d: &[u8], p: usize) -> Option<Mat4> {
    f64s_at(d, p, 16)?;
    rigid_matrix(&d[p..p + 128])
}

/// The end of a reference at `p` that need not resolve (to a deleted
/// object, or through an assembly context to another document).
fn any_ref_end(seg: &Segment, d: &[u8], p: usize) -> Option<usize> {
    if let Some(r) = seg.ref_at(d, p) {
        return Some(r.end);
    }
    if *d.get(p)? != 1 {
        return None;
    }
    let q = p + 9 + if seg.long_refs { 40 } else { 0 };
    match d.get(q..q + 2)? {
        [0, 0] => Some(q + 2),
        [1, _] => Some(q + 51),
        _ => None,
    }
}

/// A joint's or as-built joint's state ([`JOINT_STATE`]).
#[derive(Debug, PartialEq)]
pub(super) struct JointState {
    /// The motion's type code.
    pub kind: u32,
    /// The free motions, in the order of [`MOTION_SLOTS`]: the slot's
    /// motion, the axis code and the stored limits (maximum, minimum, rest;
    /// raw values, whether enabled or not).
    pub motions: Vec<Motion>,
    pub ordinal: u32,
}

/// A free motion of a joint state.
#[derive(Debug, PartialEq)]
pub(super) struct Motion {
    /// `rx`, `ry`, `rz`, `tx`, `ty` or `tz`.
    pub motion: &'static str,
    pub axis: u8,
    pub limits: [f64; 3],
}

/// The six motion slots of a joint state, in their order *(the joint test
/// model: a revolute's turn is in the third slot, sliders along x, y and
/// z in the fourth to sixth, a ball's three turns in the first three)*.
pub(super) const MOTION_SLOTS: [&str; 6] = ["rx", "ry", "rz", "tx", "ty", "tz"];

pub(super) fn joint_state(seg: &Segment, id: u64) -> Option<JointState> {
    if seg.guid_of(id) != Some(JOINT_STATE) {
        return None;
    }
    let d = seg.data_of(id);
    let mut p = header_end(d)?;
    if *d.get(p)? != 1 {
        return None;
    }
    let n = u32_at(d, p + 1)?;
    if n > 10_000 {
        return None;
    }
    p += 5;
    for _ in 0..n {
        p = any_ref_end(seg, d, p)? + 4;
    }
    let f = *d.get(p)?;
    if f > 1 {
        return None;
    }
    p += 1 + if f == 1 { 4 } else { 0 };
    let mut motions = Vec::new();
    for motion in MOTION_SLOTS {
        let present = u32_at(d, p)?;
        p += 4;
        match present {
            0 => {}
            1 => {
                let limits = [f64_at(d, p + 23)?, f64_at(d, p + 31)?, f64_at(d, p + 39)?];
                if limits.iter().any(|v| !v.is_finite()) {
                    return None;
                }
                motions.push(Motion {
                    motion,
                    axis: *d.get(p + 47)?,
                    limits,
                });
                p += 51;
            }
            _ => return None,
        }
    }
    let kind = u32_at(d, p)?;
    let ordinal = u32_at(d, p + 4)?;
    if kind > 64 {
        return None;
    }
    Some(JointState {
        kind,
        motions,
        ordinal,
    })
}

/// The kind of a motion type code, and its free motions: rigid 0,
/// revolute 1, slider 2, cylindrical 4, pin-slot 6, planar 7, ball 8; 11
/// a rigid group *(the joint test model: one of each, two rigid groups;
/// the corpus' rigid joints and revolute as-built joints)*.
fn kind_of(code: u32) -> Option<(&'static str, usize)> {
    Some(match code {
        0 => ("Rigid", 0),
        1 => ("Revolute", 1),
        2 => ("Slider", 1),
        4 => ("Cylindrical", 2),
        6 => ("PinSlot", 2),
        7 => ("Planar", 3),
        8 => ("Ball", 3),
        _ => return None,
    })
}

/// The motion type code of a rigid group (an as-built joint's class).
pub(super) const RIGID_GROUP: u32 = 11;

/// A key point input ([`KEY_POINT`]): the point (cm) and its code. The
/// point is at 16 bytes after the root part in class version 0, 21 in the
/// later ones, followed by the code *(the reference models' hole points
/// and the corpus' joint origins)*.
pub(super) fn key_point(seg: &Segment, id: u64) -> Option<(Vec3, u32)> {
    if seg.guid_of(id) != Some(KEY_POINT) {
        return None;
    }
    let o = seg.object(id)?;
    let d = seg.data(o);
    let p = seg.root_part(d)?.end + if seg.version(o)? == 0 { 16 } else { 21 };
    let v = f64s_at(d, p, 3)?;
    let code = u32_at(d, p + 24)?;
    (v.iter().all(|x| x.is_finite() && x.abs() < 1e7) && code < 256)
        .then(|| ([v[0], v[1], v[2]], code))
}

/// A direction input ([`DIRECTION_INPUT`]): `u32 | f64 point[3] | f64
/// direction[3] | f64[2] | u32 code` after the root part.
pub(super) fn direction_input(seg: &Segment, id: u64) -> Option<(Vec3, Vec3, u32)> {
    if seg.guid_of(id) != Some(DIRECTION_INPUT) {
        return None;
    }
    let d = seg.data_of(id);
    let p = seg.root_part(d)?.end;
    let v = f64s_at(d, p + 4, 6)?;
    let code = u32_at(d, p + 68)?;
    let dir = [v[3], v[4], v[5]];
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    ((len - 1.0).abs() < 1e-6 && v.iter().all(|x| x.is_finite()) && code < 256)
        .then(|| ([v[0], v[1], v[2]], dir, code))
}

fn identity_json() -> Value {
    json_value(&IDENTITY)
}

/// A side of a joint.
struct Side {
    geometry: u64,
    /// The stored frame; `None`: none stored (the identity).
    frame: Option<Mat4>,
}

/// The opposed byte and the two sides of a joint.
fn joint_sides(seg: &Segment, id: u64) -> Option<(u8, [Side; 2])> {
    let o = seg.object(id)?;
    let v4 = seg.version(o)? >= 4;
    let d = seg.data(o);
    let mut p = seg.root_part(d)?.end;
    let opposed = *d.get(p)?;
    if opposed > 1 {
        return None;
    }
    p += if v4 { 11 } else { 3 };
    let mut side = || -> Option<Side> {
        let r = seg.ref_at(d, p)?;
        if !matches!(
            seg.guid_of(r.id),
            Some(FRAME_INPUT | JOINT_ORIGIN | ENTITY_REF)
        ) {
            return None;
        }
        let mut q = r.end + if v4 { 6 } else { 0 };
        let frame = match *d.get(q)? {
            0 => {
                let m = matrix_at(d, q + 1)?;
                q += 129;
                Some(m)
            }
            1 => {
                q += 1;
                None
            }
            _ => return None,
        };
        if let Some((s, e)) = str16_at(d, q)
            && is_guid(&s)
        {
            q = e;
        }
        p = q;
        Some(Side {
            geometry: r.id,
            frame,
        })
    };
    let one = side()?;
    let two = side()?;
    Some((opposed, [one, two]))
}

impl Builder<'_> {
    /// A face, edge or plane named by a key point's or direction's input.
    fn snap_entity(&self, snap: u64) -> Option<Reference> {
        let seg = self.seg;
        for r in seg.ref_ids(seg.data_of(snap)) {
            match seg.guid_of(r) {
                Some(FACE_REF) => {
                    return match recipe::of_input(seg, r)?.kind.as_str() {
                        "edge" => edge_input(seg, r),
                        _ => face_input(seg, r),
                    };
                }
                Some(ENTITY_REF) => {
                    return self.entity_target(r);
                }
                _ => {}
            }
        }
        None
    }

    /// A frame input as a `JointGeometry`: its frame's origin and axes (the
    /// identity when it stores no matrix), the
    /// entities its first key point and direction name, and in `_f3d` the
    /// frame, key points and directions.
    pub(super) fn joint_geometry(&self, frame_input: u64) -> Option<Value> {
        let seg = self.seg;
        let d = seg.data_of(frame_input);
        if seg.guid_of(frame_input) != Some(FRAME_INPUT) {
            return None;
        }
        // No matrix stored: the component's own frame.
        let m = rigid_matrix(d).unwrap_or(IDENTITY);
        let col = |c: usize| json!([m[0][c], m[1][c], m[2][c]]);
        let mut out = json!({
            "_type": "JointGeometry",
            "origin": col(3),
            "primaryAxisVector": col(0),
            "secondaryAxisVector": col(1),
            "thirdAxisVector": col(2),
        });
        let mut points = Vec::new();
        let mut directions = Vec::new();
        for r in seg.ref_ids(d) {
            let entity = || self.snap_entity(r).map(|e| json_value(&e));
            if let Some((p, code)) = key_point(seg, r) {
                let mut v = json!({"point": p, "code": code});
                if let Some(e) = entity() {
                    v["entity"] = e;
                }
                points.push(v);
            } else if let Some((p, dir, code)) = direction_input(seg, r) {
                let mut v = json!({"point": p, "direction": dir, "code": code});
                if let Some(e) = entity() {
                    v["entity"] = e;
                }
                directions.push(v);
            }
        }
        let first = |v: &[Value]| v.first().and_then(|x| x.get("entity")).cloned();
        let (one, two) = (first(&points), first(&directions));
        if let Some(e) = &one {
            out["entityOne"] = e.clone();
        }
        if let Some(e) = two.filter(|e| Some(e) != one.as_ref()) {
            out["entityTwo"] = e;
        }
        out["_f3d"] = json!({
            "object_id": frame_input,
            "frame": m,
            "key_points": points,
            "directions": directions,
        });
        Some(out)
    }

    /// A joint side's geometry: a joint origin on the timeline as a
    /// feature reference, another joint origin's or a frame input's
    /// geometry, or the target of an entity input.
    fn side_geometry(&self, geometry: u64) -> Value {
        let seg = self.seg;
        match seg.guid_of(geometry) {
            Some(JOINT_ORIGIN) if self.pos_of.contains_key(&geometry) => {
                json_value(&self.feature_ref(geometry))
            }
            Some(JOINT_ORIGIN) => {
                let g = inputs_of(seg, geometry, FRAME_INPUT)
                    .first()
                    .and_then(|&f| self.joint_geometry(f));
                let mut v = g.unwrap_or_else(|| json!({"_type": "JointOrigin"}));
                v["_f3d"]["joint_origin"] = json!(geometry);
                v
            }
            Some(FRAME_INPUT) => self
                .joint_geometry(geometry)
                .unwrap_or_else(|| json!({"_type": "JointGeometry"})),
            _ => {
                // An entity input: its target, here or in another document.
                let target = decode::target_id(seg, geometry);
                let mut v = json!({"_type": "JointOrigin",
                    "_f3d": {"entity_input": geometry, "target": target}});
                if let Some(t) = target.filter(|&t| seg.guid_of(t) == Some(JOINT_ORIGIN)) {
                    v = self.side_geometry(t);
                }
                v
            }
        }
    }

    /// The occurrence a placement's path names, as a reference: the
    /// component's name, and in `_f3d` the path (occurrence object ids, from
    /// the component the item belongs to down; `null` for a level in
    /// another document), its GUIDs and the component it starts in.
    pub(super) fn placement_occurrence(&self, placement: u64) -> Option<Value> {
        let seg = self.seg;
        let path = inputs_of(seg, placement, CONTEXT_PATH).into_iter().next()?;
        let levels: Vec<Level> = inputs_of(seg, path, CONTEXT_LEVEL)
            .into_iter()
            .filter_map(|l| level(seg, l))
            .collect();
        let context = levels
            .first()
            .and_then(|l| l.context.as_ref())
            .and_then(|g| self.guids.components.get(g).copied().flatten());
        let guids: Vec<&String> = levels.iter().flat_map(|l| &l.occurrences).collect();
        let ids: Vec<Option<u64>> = guids
            .iter()
            .map(|g| self.guids.occurrences.get(*g).copied().flatten())
            .collect();
        let mut v = json!({"kind": "occurrence",
            "_f3d": {"path": ids, "path_guids": guids, "context_component": context}});
        let last = ids.last().copied().flatten();
        if let Some(name) = last
            .and_then(|o| super::occurrence_info(seg, o))
            .filter(|i| i.context.is_none())
            .and_then(|i| self.component_names.get(&i.component))
        {
            v["component"] = json!(name);
        }
        Some(v)
    }

    /// The placements of an item, in order: occurrence and stored frame.
    fn placements(&self, item: u64) -> Vec<(u64, Option<Value>, Option<Mat4>)> {
        inputs_of(self.seg, item, PLACEMENT)
            .into_iter()
            .map(|p| {
                (
                    p,
                    self.placement_occurrence(p),
                    rigid_matrix(self.seg.data_of(p)),
                )
            })
            .collect()
    }

    /// The motion of a joint state, with the limits among the item's
    /// parameters (roles `Rotate…` and `Slide…` ending in `Minimum`,
    /// `Maximum` or `Rest`; one stored is taken as enabled [assumed]).
    fn joint_motion(&self, state: &JointState, params: &[&ParameterEntry]) -> Option<Value> {
        let (kind, free) = kind_of(state.kind)?;
        if free != state.motions.len() {
            return None;
        }
        let mut v = json!({"_type": format!("{kind}JointMotion"),
            "jointType": format!("{kind}JointType")});
        let limits = |prefix: &str| {
            let mut m = Map::new();
            for (suffix, value, enabled) in [
                ("Minimum", "minimumValue", "isMinimumValueEnabled"),
                ("Maximum", "maximumValue", "isMaximumValueEnabled"),
                ("Rest", "restValue", "isRestValueEnabled"),
            ] {
                if let Some(p) = params
                    .iter()
                    .find(|p| p.role == format!("{prefix}{suffix}"))
                {
                    m.insert(value.to_owned(), json_value(&paramref(p)));
                    m.insert(enabled.to_owned(), json!(true));
                }
            }
            (!m.is_empty()).then(|| {
                m.insert("_type".to_owned(), json!("JointLimits"));
                Value::Object(m)
            })
        };
        if let Some(l) = limits("Rotate") {
            v["rotationLimits"] = l;
        }
        if let Some(l) = limits("Slide") {
            v["slideLimits"] = l;
        }
        let motions: Vec<Value> = state
            .motions
            .iter()
            .map(|m| json!({"motion": m.motion, "axis": m.axis, "limits": m.limits}))
            .collect();
        v["_f3d"] = json!({"type_code": state.kind, "motions": motions});
        Some(v)
    }

    fn own_params(&self, fid: u64) -> Vec<&ParameterEntry> {
        self.dec
            .parameters
            .iter()
            .filter(|p| p.owner_feature == Some(fid))
            .collect()
    }

    fn state_of(&self, fid: u64) -> Option<JointState> {
        inputs_of(self.seg, fid, JOINT_STATE)
            .first()
            .and_then(|&s| joint_state(self.seg, s))
    }

    /// A joint's detail (SCHEMA.md §5.3, `Joint`).
    pub(super) fn joint(&self, it: &TimelineEntry) -> Detail {
        let seg = self.seg;
        let fid = it.id;
        let params = self.own_params(fid);
        let role = |r: &str| {
            params
                .iter()
                .find(|p| p.role == r)
                .map(|p| json_value(&paramref(p)))
        };
        let mut map = Map::new();
        let mut f3d = Map::new();
        if let Some((opposed, sides)) = joint_sides(seg, fid) {
            map.insert(
                "geometryOrOriginOne".into(),
                self.side_geometry(sides[0].geometry),
            );
            map.insert(
                "geometryOrOriginTwo".into(),
                self.side_geometry(sides[1].geometry),
            );
            // Flipped: the z axes of the two frames are opposed.
            map.insert("isFlipped".into(), json!(opposed == 1));
            f3d.insert("opposed".into(), json!(opposed));
            f3d.insert(
                "frames".into(),
                json!(
                    sides
                        .iter()
                        .map(|s| s.frame.map_or_else(identity_json, |m| json_value(&m)))
                        .collect::<Vec<_>>()
                ),
            );
        }
        let placements = self.placements(fid);
        for (key, k) in [("occurrenceOne", 0), ("occurrenceTwo", 1)] {
            if let Some(o) = placements.get(k).and_then(|p| p.1.clone()) {
                map.insert(key.into(), o);
            }
        }
        if let Some(m) = placements.first().and_then(|p| p.2) {
            f3d.insert("aligned_frame".into(), json_value(&m));
        }
        if let Some(motion) = self
            .state_of(fid)
            .and_then(|s| self.joint_motion(&s, &params))
        {
            map.insert("jointMotion".into(), motion);
        }
        for (key, r) in [
            ("angle", "alignAngle"),
            ("offset", "alignOffsetZ"),
            ("offsetX", "alignOffsetX"),
            ("offsetY", "alignOffsetY"),
        ] {
            if let Some(p) = role(r) {
                map.insert(key.into(), p);
            }
        }
        if !f3d.is_empty() {
            map.insert("_f3d".into(), Value::Object(f3d));
        }
        Detail::Other(map)
    }

    /// Whether an as-built joint item is a rigid group (its motion type).
    pub(super) fn is_rigid_group(&self, fid: u64) -> bool {
        self.state_of(fid)
            .is_some_and(|s| s.kind == RIGID_GROUP && s.motions.is_empty())
    }

    /// A rigid group's detail (SCHEMA.md §5.3, `RigidGroup`): its members.
    fn rigid_group(&self, fid: u64) -> Detail {
        // As an as-built joint's: the first placement is the geometry's.
        let placements = self.placements(fid);
        let members = placements.get(1..).unwrap_or_default();
        let occurrences: Vec<Value> = members.iter().filter_map(|p| p.1.clone()).collect();
        let recorded: Vec<Value> = members
            .iter()
            .map(|(_, o, m)| {
                json!({"occurrence": o, "frame": m.map_or_else(identity_json, |m| json_value(&m))})
            })
            .collect();
        let mut map = Map::new();
        map.insert("occurrences".into(), json!(occurrences));
        map.insert("_f3d".into(), json!({"placements": recorded}));
        Detail::Other(map)
    }

    /// An as-built joint's detail (SCHEMA.md §5.3, `AsBuiltJoint`).
    pub(super) fn as_built_joint(&self, it: &TimelineEntry) -> Detail {
        let seg = self.seg;
        let fid = it.id;
        if self.is_rigid_group(fid) {
            return self.rigid_group(fid);
        }
        let d = seg.data_of(fid);
        let mut map = Map::new();
        let geometry = seg
            .root_part(d)
            .and_then(|r| seg.ref_at(d, r.end))
            .filter(|r| seg.guid_of(r.id) == Some(FRAME_INPUT))
            .map(|r| r.id);
        map.insert(
            "geometry".into(),
            geometry
                .and_then(|g| self.joint_geometry(g))
                .unwrap_or(Value::Null),
        );
        // The first placement is the geometry's; the occurrences' follow.
        let placements = self.placements(fid);
        let occurrences: Vec<&(u64, Option<Value>, Option<Mat4>)> =
            placements.iter().skip(1).collect();
        for (key, k) in [("occurrenceOne", 0), ("occurrenceTwo", 1)] {
            if let Some(o) = occurrences.get(k).and_then(|p| p.1.clone()) {
                map.insert(key.into(), o);
            }
        }
        let params = self.own_params(fid);
        if let Some(motion) = self
            .state_of(fid)
            .and_then(|s| self.joint_motion(&s, &params))
        {
            map.insert("jointMotion".into(), motion);
        }
        let recorded: Vec<Value> = occurrences
            .iter()
            .map(|(_, o, m)| {
                json!({"occurrence": o, "frame": m.map_or_else(identity_json, |m| json_value(&m))})
            })
            .collect();
        map.insert("_f3d".into(), json!({"placements": recorded}));
        Detail::Other(map)
    }

    /// A joint origin's geometry: its frame input.
    pub(super) fn joint_origin_geometry(&self, fid: u64) -> Option<Value> {
        let f = *inputs_of(self.seg, fid, FRAME_INPUT).first()?;
        self.joint_geometry(f)
    }

    /// A ground item's detail: the grounded occurrence.
    pub(super) fn ground(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        if let Some(o) = inputs_of(self.seg, it.id, PLACEMENT)
            .first()
            .and_then(|&p| self.placement_occurrence(p))
        {
            map.insert("occurrence".into(), o);
        }
        Detail::Other(map)
    }

    /// An occurrence item's detail (an insert, a fastener, a paste): the
    /// occurrence it made, as a path from the item's component
    /// (mitcad#75).
    pub(super) fn occurrence_item(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        if let Some(o) = super::item_occurrence(self.seg, it.id) {
            map.insert(
                "occurrence".into(),
                json!({"kind": "occurrence", "_f3d": {"path": [o]}}),
            );
        }
        Detail::Other(map)
    }

    /// A captured position's detail (SCHEMA.md §5.3, `Snapshot`): each
    /// occurrence it places with its matrix.
    pub(super) fn snapshot(&self, it: &TimelineEntry) -> Detail {
        let mut map = Map::new();
        if let Some(entries) = snapshot_entries(self.seg, it.id) {
            let positions: Vec<Value> = entries
                .iter()
                .map(|(placement, m)| {
                    let mut v =
                        json!({"transform": m.map_or_else(identity_json, |m| json_value(&m))});
                    if let Some(o) = self.placement_occurrence(*placement) {
                        v["occurrence"] = o;
                    }
                    v
                })
                .collect();
            map.insert("positions".into(), json!(positions));
        }
        Detail::Other(map)
    }
}

/// The entries of a captured position ([`SNAPSHOT`]): after the root part
/// 8 bytes (class version 6; 4 in version 5) and `u32 n`, then `n` × (`u8
/// | ref placement | u8 0 + f64[16] matrix, or u8 1`).
fn snapshot_entries(seg: &Segment, id: u64) -> Option<Vec<(u64, Option<Mat4>)>> {
    if seg.guid_of(id) != Some(SNAPSHOT) {
        return None;
    }
    let o = seg.object(id)?;
    let d = seg.data(o);
    let gap = if seg.version(o)? >= 6 { 8 } else { 4 };
    let mut p = seg.root_part(d)?.end + gap;
    let n = u32_at(d, p)?;
    if n > 100_000 {
        return None;
    }
    p += 4;
    let mut out = Vec::new();
    for _ in 0..n {
        let r = seg.ref_at(d, p + 1)?;
        if seg.guid_of(r.id) != Some(PLACEMENT) {
            return None;
        }
        let matrix = match *d.get(r.end)? {
            0 => Some(matrix_at(d, r.end + 1)?),
            1 => None,
            _ => return None,
        };
        p = r.end + if matrix.is_some() { 129 } else { 1 };
        out.push((r.id, matrix));
    }
    Some(out)
}

/// The occurrence object ids the ground items ground (the last level of
/// each path).
pub(super) fn grounded(items: &[TimelineItem]) -> Vec<u64> {
    items
        .iter()
        .filter(|i| i.object_type() == Some("GroundOccurrence"))
        .filter_map(|i| match &i.detail {
            Some(Detail::Other(m)) => m
                .get("occurrence")?
                .pointer("/_f3d/path")?
                .as_array()?
                .last()?
                .as_u64(),
            _ => None,
        })
        .collect()
}

/// Whether `free` motion values fit the motion type `code` (tests).
#[cfg(test)]
pub(crate) fn kind_fits(code: u32, free: usize) -> bool {
    kind_of(code).is_some_and(|(_, n)| n == free)
}
