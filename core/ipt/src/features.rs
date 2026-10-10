// SPDX-License-Identifier: MIT
//! Features of the definitions segment as the dump IR's feature details
//! (SCHEMA.md §5.3), as far as their records are decoded.
//!
//! A feature record (`914d8790…`): the header, an i32, a u32 (the
//! feature's kind within the file), a reference list (kind 2) of its
//! properties by slot, and a u32. Properties are records of their own:
//! parameters, enumerations (`28be9a72…` operation, `297d6392…` extent:
//! the prefix (`dc`), u16 kind, u16 value), booleans (`284d8790…`: a name, a u32
//! and the value as the last byte), directions (`40df52ce…`: three f64 at
//! the end) and lists of other records.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::dc::{self, Definitions, Reader, type_id};
use crate::params::Parameters;

pub const ENUM_OPERATION: [u8; 16] = type_id("28be9a72d111440900084eba32a3dc09");
pub const ENUM_EXTENT: [u8; 16] = type_id("297d6392d1113cb9000831bd0663dc09");
pub const BOOLEAN: [u8; 16] = type_id("284d8790d011f8d10008cabc0663dc09");
pub const DIRECTION: [u8; 16] = type_id("40df52ced011d0d20008ccbc0663dc09");
pub const BOUNDARY_PATCH: [u8; 16] = type_id("91739422d11107cf000835bd0663dc09");

/// What a feature's translation needs besides its record.
pub struct Context<'a> {
    pub dc: &'a Definitions,
    pub params: &'a Parameters,
    /// Sketch records → their timeline indices and names.
    pub sketches: &'a HashMap<usize, (i64, String)>,
    /// The selected profiles' geometry.
    pub profiles: &'a std::cell::RefCell<crate::profile::Profiles>,
    /// Sketch records → their entity records' ids in the dump.
    pub sketch_ids: &'a HashMap<usize, HashMap<usize, String>>,
    /// Body records → the bodies' producers ([`Producers`]).
    pub bodies: &'a Producers,
}

/// A solid body of the part (`474d8790…`: the header, the prefix and a
/// u32, the body's number *(seen)*); the browser's label names it.
pub const BODY: [u8; 16] = type_id("474d8790d011f8d10008cabc0663dc09");
/// A list of bodies (`ae70680e…`, `83aadad5…`: the prefix and a list of
/// body records) *(seen)*: every feature refers to one (the bodies it
/// made or changed); a combine's target is the other kind.
pub const BODIES: [u8; 16] = type_id("ae70680ed14a1e86d76248b03c2a96e1");
pub const TARGET_BODIES: [u8; 16] = type_id("83aadad54f4375dacd1003b260a8a422");

/// The bodies' producers: body record → the timeline index of the item
/// that made it, its index among that item's bodies, and its name.
#[derive(Clone, Debug, Default)]
pub struct Producers {
    pub of: HashMap<usize, (i64, usize, Option<String>)>,
}

impl Producers {
    /// The body as the dump's body reference: by its producer where it is
    /// known (`_f3d.producer`, `_f3d.body_index`), by its name always.
    pub fn reference(&self, body: usize, name: Option<&str>) -> Value {
        let mut r = json!({"kind": "body", "objectType": "BRepBody"});
        let known = self.of.get(&body);
        if let Some(n) = name.or_else(|| known.and_then(|k| k.2.as_deref())) {
            r["name"] = json!(n);
        }
        if let Some((producer, index, _)) = known {
            r["_f3d"] = json!({"producer": producer, "body_index": index});
        }
        r
    }

    /// Records the bodies the item `index` refers to: those not seen
    /// before are its own (the first item to refer to a body made it).
    pub fn record(&mut self, index: i64, bodies: &[usize], name: impl Fn(usize) -> Option<String>) {
        for &b in bodies {
            if self.of.contains_key(&b) {
                continue;
            }
            let k = self.of.values().filter(|v| v.0 == index).count();
            self.of.insert(b, (index, k, name(b)));
        }
    }
}

/// A feature's detail, and the parameter records it made (its model
/// parameters).
#[derive(Clone, Debug, Default)]
pub struct Translated {
    pub detail: Value,
    /// Decoder data of the item (`_f3d` keys the import reads).
    pub raw: Option<Value>,
    pub parameters: Vec<usize>,
    /// Properties not decoded (the import works them out from the history
    /// or guesses): for the report.
    pub notes: Vec<String>,
}

impl Context<'_> {
    /// The feature's property slots.
    pub fn slots(&self, record: usize) -> Result<Vec<Option<usize>>, String> {
        let mut r = self.dc.body(record);
        (|| -> dc::Result<Vec<Option<usize>>> {
            r.i32()?;
            r.u32()?;
            Ok(r.list()?.1.into_iter().map(dc::reference).collect())
        })()
        .map_err(|e| format!("feature {record}: {e}"))
    }

    /// An enumeration's value.
    pub fn enumeration(&self, record: Option<usize>, t: &[u8; 16]) -> Option<u16> {
        let record = record.filter(|&r| self.dc.is(r, t))?;
        let mut r = self.dc.fields(record);
        r.skip(2).ok()?;
        r.u16().ok()
    }

    /// A boolean's value.
    pub fn boolean(&self, record: Option<usize>) -> Option<bool> {
        let record = record.filter(|&r| self.dc.is(r, &BOOLEAN))?;
        self.dc.bytes(record).last().map(|&b| b != 0)
    }

    /// A direction (unit vector, model coordinates).
    pub fn direction(&self, record: Option<usize>) -> Option<[f64; 3]> {
        let record = record.filter(|&r| self.dc.is(r, &DIRECTION))?;
        let bytes = self.dc.bytes(record);
        let mut r = Reader::at(bytes, bytes.len().checked_sub(24)?);
        let v = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        ((n - 1.0).abs() < 1e-6).then_some(v)
    }

    /// A parameter property as the dump's parameter reference.
    pub fn parameter(&self, record: Option<usize>, made: &mut Vec<usize>) -> Option<Value> {
        let p = self.params.by_record(record?)?;
        if p.in_table {
            made.push(p.record);
        }
        Some(json!({"kind": "parameter", "name": p.name, "value": p.value}))
    }

    /// The records of a list property (a boundary patch's profiles).
    pub fn list(&self, record: Option<usize>) -> Option<Vec<usize>> {
        let mut r = self.dc.fields(record?);
        r.references().ok()
    }

    /// The body records of a list of bodies (`t`: [`BODIES`] or
    /// [`TARGET_BODIES`]).
    pub fn bodies_in(&self, record: Option<usize>, t: &[u8; 16]) -> Vec<usize> {
        self.list(record.filter(|&r| self.dc.is(r, t)))
            .unwrap_or_default()
            .into_iter()
            .filter(|&b| self.dc.is(b, &BODY))
            .collect()
    }

    /// The bodies a feature refers to: those of its lists of bodies, in
    /// the order of its slots.
    pub fn feature_bodies(&self, record: usize) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for s in self.slots(record).unwrap_or_default() {
            for t in [&TARGET_BODIES, &BODIES] {
                for b in self.bodies_in(s, t) {
                    if !out.contains(&b) {
                        out.push(b);
                    }
                }
            }
        }
        out
    }

    /// Body references for the bodies of a list.
    fn body_references(&self, record: Option<usize>, t: &[u8; 16]) -> Vec<Value> {
        self.bodies_in(record, t)
            .into_iter()
            .map(|b| self.bodies.reference(b, None))
            .collect()
    }
}

/// An extrusion: slots 0 operation, 1 profiles, 2 direction, 3 reversed,
/// 4 distance, 5 taper, 6 extent, 7 symmetric *(verified on the test
/// files' extrusions)*. Operations 1 new body, 2 cut, 3 join, 4
/// intersection; extents 1 distance, 2 symmetric, 5 through all, 4 up to
/// the next face (slot 24: the body it reaches), 7 up to a face or work
/// plane (slot 11 a work plane, slot 18 the face's surface; slot 13 not
/// decoded). The extrusion goes along the direction, against it when
/// reversed.
pub fn extrude(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
    items: &HashMap<usize, i64>,
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = match cx.enumeration(slot(0), &ENUM_OPERATION) {
        Some(1) => "NewBodyFeatureOperation",
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("operation {other:?} is not known")),
    };
    let extent = cx.enumeration(slot(6), &ENUM_EXTENT);
    let distance = cx.parameter(slot(4), &mut t.parameters);
    let taper = cx.parameter(slot(5), &mut t.parameters);
    let mut d = json!({"operation": operation, "isSolid": true});
    participants(cx, &slots, operation, &mut d);
    match extent {
        Some(1) => {
            d["extentType"] = json!("OneSideFeatureExtentType");
            d["extentOne"] = json!({"_type": "DistanceExtentDefinition", "distance": distance});
        }
        Some(2) => {
            d["extentType"] = json!("SymmetricFeatureExtentType");
            d["symmetricExtent"] = json!({"_type": "SymmetricExtentDefinition",
                "distance": distance});
        }
        // Through all both ways when slot 7 is set (as it is in every
        // symmetric extent, and in 9 of the 279 through all *(seen)*).
        Some(5) => {
            let both = cx.boolean(slot(7)) == Some(true);
            d["extentType"] = json!(if both {
                "SymmetricFeatureExtentType"
            } else {
                "OneSideFeatureExtentType"
            });
            d["extentOne"] = json!({"_type": "ThroughAllExtentDefinition"});
        }
        // Up to a work plane (slot 11), else up to the surface of the face
        // slot 18 keeps (a plane or a cylinder), which the import looks for.
        Some(7) => {
            d["extentType"] = json!("OneSideFeatureExtentType");
            let mut one = json!({"_type": "ToEntityExtentDefinition"});
            let entity = slot(11)
                .and_then(|p| plane_reference(cx, p, items))
                .or_else(|| slot(18).and_then(|s| extent_surface(cx.dc, s)));
            match entity {
                Some(e) => one["entity"] = e,
                None => t
                    .notes
                    .push("the face or plane it extends to is not decoded".to_owned()),
            }
            d["extentOne"] = one;
        }
        // Extent 4, up to the next face: up to the first face of the body
        // its slot 24 names (a list of bodies `83aadad5…` *(seen)*), else
        // the import looks for the face.
        Some(4) => {
            d["extentType"] = json!("OneSideFeatureExtentType");
            let mut one = json!({"_type": "ToEntityExtentDefinition"});
            match cx.body_references(slot(24), &TARGET_BODIES).first() {
                Some(body) => {
                    one["entity"] = body.clone();
                    one["isMinimumSolution"] = json!(true);
                }
                None => t
                    .notes
                    .push("an extent up to the next face, which is looked for".to_owned()),
            }
            d["extentOne"] = one;
        }
        other => return Err(format!("extent {other:?} is not known")),
    }
    let reversed = cx.boolean(slot(3)) == Some(true);
    if let Some(taper) = taper {
        d["taperAngleOne"] = taper;
    }
    if let Some((index, name)) = sketch {
        d["profile"] = json!(profiles(cx, slot(1), index, &name, sketch_record));
    }
    if let Some(v) = cx.direction(slot(2)) {
        let v = if reversed { v.map(|x| -x) } else { v };
        t.raw = Some(json!({"extrude": {"direction_vector": v}}));
    }
    t.detail = d;
    Ok(t)
}

/// The bodies a join, cut or intersection works on: its list of bodies
/// (the first slot of [`BODIES`]) *(seen)*, as `participantBodies`, in parts
/// of more than one body.
fn participants(cx: &Context, slots: &[Option<usize>], operation: &str, d: &mut Value) {
    // A part of one body: its only body takes part anyway (and the
    // import's tries with and without participants cost time).
    if operation == "NewBodyFeatureOperation" || cx.dc.of_type(&BODY).nth(1).is_none() {
        return;
    }
    let list = slots
        .iter()
        .copied()
        .find(|s| s.is_some_and(|r| cx.dc.is(r, &BODIES)))
        .flatten();
    let bodies = cx.body_references(list, &BODIES);
    if !bodies.is_empty() {
        d["participantBodies"] = json!(bodies);
    }
}

/// The surface of the face an extrusion extends up to (slot 18).
pub const EXTENT_SURFACE: [u8; 16] = type_id("f0801215d4119e8eef90a8ac7f9ac3ff");

/// The surface of the face an extrusion up to a face extends to
/// ([`EXTENT_SURFACE`]): after the prefix zeros, a byte 1 and a u32 kind
/// *(seen: 144 extrusions)*. Kind `0x19` a plane: four bytes, then its
/// origin (cm), x and y axes (f64; both across the extrusion's direction
/// in all 130 such records); kind `0x49` a cone: eight bytes,
/// then the cosine and sine of its half angle, a point on its axis (cm),
/// its two radii (cm), its axis and its reference direction, a cylinder
/// when the sine is 0 and the radii agree. As the dump's face reference
/// with that geometry and no point on it (a plane's origin need not lie on
/// the face, and another face may pass through it).
fn extent_surface(dc: &Definitions, record: usize) -> Option<Value> {
    if !dc.is(record, &EXTENT_SURFACE) {
        return None;
    }
    let bytes = dc.bytes(record);
    let from = dc.header_len();
    let head = |k: u8| [1, k, 0, 0, 0];
    let find = |k: u8| {
        bytes
            .get(from..(from + 48).min(bytes.len()))?
            .windows(5)
            .position(|w| w == head(k))
            .map(|i| from + i)
    };
    let read = |at: usize, n: usize| -> Option<Vec<f64>> {
        let mut r = Reader::at(bytes, at);
        let v: Vec<f64> = (0..n).map(|_| r.f64().ok()).collect::<Option<_>>()?;
        v.iter().all(|x| x.is_finite()).then_some(v)
    };
    let is_unit = |v: &[f64]| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() < 1e-6;
    if let Some(at) = find(0x19) {
        let v = read(at + 9, 9)?;
        let (origin, x, y) = (&v[0..3], &v[3..6], &v[6..9]);
        if !is_unit(x) || !is_unit(y) {
            return None;
        }
        let normal = unit(cross([x[0], x[1], x[2]], [y[0], y[1], y[2]]))?;
        return Some(json!({"kind": "face", "objectType": "BRepFace",
                           "geometry": {"type": "Plane", "origin": origin, "normal": normal}}));
    }
    let at = find(0x49)?;
    let v = read(at + 13, 13)?;
    let (sine, origin, radii, axis) = (v[1], &v[2..5], (v[5], v[6]), &v[7..10]);
    if sine.abs() > 1e-9 || (radii.0 - radii.1).abs() > 1e-9 || radii.0 <= 0.0 || !is_unit(axis) {
        return None;
    }
    Some(json!({"kind": "face", "objectType": "BRepFace",
                "geometry": {"type": "Cylinder", "origin": origin, "axis": axis, "radius": radii.0}}))
}

/// A combine: slots 0 the target (`83aadad5…`, one body), 1 the tools
/// (`ae70680e…`), 2 the operation (2 cut, 3 join, 4 intersection), 3 keep
/// the tools *(verified: every combine of the test files)*.
pub fn combine(cx: &Context, record: usize) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let operation = match cx.enumeration(slot(2), &ENUM_OPERATION) {
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("combine operation {other:?} is not known")),
    };
    let target = cx.body_references(slot(0), &TARGET_BODIES);
    let tools = cx.body_references(slot(1), &BODIES);
    let [target] = &target[..] else {
        return Err(format!("{} target bodies", target.len()));
    };
    if tools.is_empty() {
        return Err("no tool bodies".to_owned());
    }
    // Read from the record, not guessed (`_ipt_inputs`).
    let mut d = json!({"operation": operation, "targetBody": target, "toolBodies": tools,
                       "_ipt_inputs": true});
    if let Some(keep) = cx.boolean(slot(3)) {
        d["isKeepToolBodies"] = json!(keep);
    }
    Ok(Translated {
        detail: d,
        ..Translated::default()
    })
}

pub const ENUM_SPLIT: [u8; 16] = type_id("1990e1b8d311467bc000e39545df724f");

/// A split: slots 0 its kind (`1990e1b8…`: 2 split faces, 3 split bodies),
/// 2 the faces to split (topological names, not decoded), 4 the tool's
/// sketch profiles (`91739422…`, as an extrusion's), 5 a work plane as the
/// tool, 7 the bodies (`83aadad5…`) *(seen: every split of the test
/// files, of 78 the kinds 2 and 3)*. The object type it comes in as, and
/// its detail.
pub fn split(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
    items: &HashMap<usize, i64>,
) -> Result<(&'static str, Translated), String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let plane = slot(5).and_then(|p| plane_reference(cx, p, items));
    let tool = match (plane, &sketch) {
        (Some(p), _) => json!([p]),
        (None, Some((index, name))) => json!(profiles(cx, slot(4), *index, name, sketch_record)),
        (None, None) => return Err("its tool was not decoded".to_owned()),
    };
    let bodies = cx.body_references(slot(7), &TARGET_BODIES);
    match cx.enumeration(slot(0), &ENUM_SPLIT) {
        Some(2) => {
            t.detail = json!({"splittingTool": tool});
            t.notes
                .push("the faces it splits are found with the history".to_owned());
            if let [body] = &bodies[..] {
                t.detail["body"] = body.clone();
            }
            Ok(("SplitFaceFeature", t))
        }
        Some(3) => {
            let [tool] = tool.as_array().map_or(&[][..], Vec::as_slice) else {
                return Err("a body split by several profiles".to_owned());
            };
            if bodies.is_empty() {
                return Err("no bodies to split".to_owned());
            }
            t.detail = json!({"splitBodies": bodies, "splittingTool": tool});
            Ok(("SplitBodyFeature", t))
        }
        other => Err(format!("split kind {other:?} is not known")),
    }
}

/// A sweep: slots 0 the profiles (as an extrusion's), 1 the path (a
/// profile selection or a path selection `473f20fc…`, whose wire body is
/// the path), 2 the operation, 3 the taper *(seen: every sweep of the test
/// files)*. Its label shows the profile's sketch and the path's: the
/// profile's is the one whose plane holds the selected profile. The path
/// is given as points along it (model coordinates, cm) with the label's
/// sketches (`_ipt_path`): [`path_curves`] names the curves on it.
pub fn sweep(
    cx: &Context,
    record: usize,
    sketches: &[(usize, i64, String)],
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = match cx.enumeration(slot(2), &ENUM_OPERATION) {
        Some(1) => "NewBodyFeatureOperation",
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("operation {other:?} is not known")),
    };
    let selections = cx
        .list(slot(0).filter(|&r| cx.dc.is(r, &BOUNDARY_PATCH)))
        .unwrap_or_default();
    let first = selections.first().ok_or("no profiles selected")?;
    let loops = cx
        .profiles
        .borrow_mut()
        .loops(cx.dc, *first)
        .ok_or("its profile was not read")?;
    let on_plane = |sketch: usize| {
        crate::sketch::sketch_matrix(cx.dc, sketch).is_ok_and(|m| {
            let n = [m[0][2], m[1][2], m[2][2]];
            let o = [m[0][3], m[1][3], m[2][3]];
            loops.iter().flatten().all(|p| {
                let d = [p[0] / 10.0 - o[0], p[1] / 10.0 - o[1], p[2] / 10.0 - o[2]];
                (d[0] * n[0] + d[1] * n[1] + d[2] * n[2]).abs() < 1e-6
            })
        })
    };
    let (profile_record, index, name) = sketches
        .iter()
        .find(|(s, _, _)| on_plane(*s))
        .ok_or("its profile lies in none of its sketches")?;
    let mut d = json!({"operation": operation, "isSolid": true,
        "profile": profiles(cx, slot(0), *index, name, Some(*profile_record))});
    participants(cx, &slots, operation, &mut d);
    if let Some(taper) = cx.parameter(slot(3), &mut t.parameters) {
        d["taperAngle"] = taper;
    }
    let path = slot(1)
        .and_then(|p| cx.profiles.borrow_mut().loops(cx.dc, p))
        .ok_or("its path was not read")?;
    let points: Vec<[f64; 3]> = path
        .into_iter()
        .flatten()
        .map(|p| p.map(|v| v / 10.0))
        .collect();
    let all: Vec<i64> = sketches.iter().map(|s| s.1).collect();
    d["_ipt_path"] = json!({"sketches": all, "points": points});
    t.detail = d;
    Ok(t)
}

/// The list of a loft's sections (`509e2f31…`: the prefix and a list).
pub const LOFT_SECTIONS: [u8; 16] = type_id("509e2f31d1110344000881ba32a3dc09");

/// A loft: slots 0 its sections (a list `509e2f31…` of sections
/// `e850314b…`, `profile::LOFT_SECTION`, whose wire bodies are their
/// boundaries), 1 the operation *(seen: every loft of the test files)*.
/// Each section is the profile of the first of `sketches` whose plane
/// holds its boundary, measured there; rails and conditions are not
/// decoded.
pub fn loft(
    cx: &Context,
    record: usize,
    sketches: &[(usize, i64, String)],
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = match cx.enumeration(slot(1), &ENUM_OPERATION) {
        Some(1) => "NewBodyFeatureOperation",
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("operation {other:?} is not known")),
    };
    let sections = cx
        .list(slot(0).filter(|&r| cx.dc.is(r, &LOFT_SECTIONS)))
        .ok_or("its sections were not read")?;
    if sections.len() < 2 {
        return Err(format!("{} sections", sections.len()));
    }
    let mut out = Vec::new();
    for (k, &s) in sections.iter().enumerate() {
        let at = |e: &str| format!("section {}: {e}", k + 1);
        let loops = cx
            .profiles
            .borrow_mut()
            .loops(cx.dc, s)
            .ok_or_else(|| at("its boundary was not read"))?;
        let on_plane = |sketch: usize| {
            crate::sketch::sketch_matrix(cx.dc, sketch).is_ok_and(|m| {
                let n = [m[0][2], m[1][2], m[2][2]];
                let o = [m[0][3], m[1][3], m[2][3]];
                loops.iter().flatten().all(|p| {
                    let d = [p[0] / 10.0 - o[0], p[1] / 10.0 - o[1], p[2] / 10.0 - o[2]];
                    (d[0] * n[0] + d[1] * n[1] + d[2] * n[2]).abs() < 1e-6
                })
            })
        };
        let (sketch, index, name) = sketches
            .iter()
            .find(|(s, _, _)| on_plane(*s))
            .ok_or_else(|| at("its boundary lies in none of its sketches"))?;
        let m = crate::sketch::sketch_matrix(cx.dc, *sketch)?;
        let measured = cx
            .profiles
            .borrow_mut()
            .measure(cx.dc, s, &m)
            .ok_or_else(|| at("its boundary was not measured"))?;
        cx.profiles
            .borrow_mut()
            .outlines
            .extend(measured.loops.iter().map(|l| (*sketch, s, l.clone())));
        out.push(
            json!({"index": k, "entity": {"kind": "profile", "sketch": name,
            "sketch_timeline_index": index, "area": measured.area,
            "centroid": [measured.centroid[0], measured.centroid[1], 0.0]}}),
        );
    }
    let mut d = json!({"operation": operation, "isSolid": true, "loftSections": out});
    participants(cx, &slots, operation, &mut d);
    t.detail = d;
    Ok(t)
}

/// The curves of a sweep's sketches that lie on its path (`_ipt_path`,
/// [`sweep`]): those of the sketch with the most of them, in the order
/// along the path, as the dump's path (`PathEntity` items of sketch
/// curves).
pub fn path_curves(items: &mut [Value]) {
    let vec3 = |v: &Value| -> Option<[f64; 3]> { serde_json::from_value(v.clone()).ok() };
    for k in 0..items.len() {
        let Some(path) = items[k]["detail"]
            .as_object_mut()
            .and_then(|d| d.remove("_ipt_path"))
        else {
            continue;
        };
        let points: Vec<[f64; 3]> = path["points"]
            .as_array()
            .map(|a| a.iter().filter_map(vec3).collect())
            .unwrap_or_default();
        let mut best: Option<(i64, Vec<(usize, String)>)> = None;
        for sketch in path["sketches"].as_array().into_iter().flatten() {
            let Some(sketch) = sketch.as_i64() else {
                continue;
            };
            let Some(item) = usize::try_from(sketch).ok().and_then(|i| items.get(i)) else {
                continue;
            };
            let on = curves_on_path(&item["detail"], &points);
            if on.len() > best.as_ref().map_or(0, |b| b.1.len()) {
                best = Some((sketch, on));
            }
        }
        let Some((sketch, on)) = best else {
            continue;
        };
        items[k]["detail"]["path"] = Value::Array(
            on.into_iter()
                .map(|(_, id)| {
                    json!({"entity": {"kind": "sketch_entity", "objectType": "SketchCurve",
                                      "sketch_timeline_index": sketch, "id": id}})
                })
                .collect(),
        );
    }
}

/// The lines, arcs and circles of a sketch's detail that lie on a polyline (model
/// coordinates, cm), with where along it each lies (its middle's segment),
/// in that order.
fn curves_on_path(detail: &Value, points: &[[f64; 3]]) -> Vec<(usize, String)> {
    let vec3 = |v: &Value| -> Option<[f64; 3]> { serde_json::from_value(v.clone()).ok() };
    let frame = &detail["model_frame"];
    let (Some(o), Some(x), Some(y)) = (
        vec3(&frame["origin"]),
        vec3(&frame["x_axis"]),
        vec3(&frame["y_axis"]),
    ) else {
        return Vec::new();
    };
    let model = |p: [f64; 2]| -> [f64; 3] { [0, 1, 2].map(|i| o[i] + p[0] * x[i] + p[1] * y[i]) };
    // The nearest segment of the path to a point, and how far off it.
    let along = |p: [f64; 3]| -> (usize, f64) {
        let mut best = (0, f64::INFINITY);
        for i in 0..points.len().saturating_sub(1) {
            let (a, b) = (points[i], points[i + 1]);
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
            let t = if l2 > 0.0 {
                (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1] + (p[2] - a[2]) * ab[2]) / l2)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = [a[0] + t * ab[0], a[1] + t * ab[1], a[2] + t * ab[2]];
            let dist =
                ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
            if dist < best.1 {
                best = (i, dist);
            }
        }
        best
    };
    let xy = |v: &Value| -> Option<[f64; 2]> { Some([v[0].as_f64()?, v[1].as_f64()?]) };
    let mut on: Vec<(usize, String)> = Vec::new();
    for c in detail["curves"].as_array().into_iter().flatten() {
        let g = &c["geometry"];
        let samples: Option<(Vec<[f64; 2]>, f64)> = match g["type"].as_str() {
            Some("Line3D") => xy(&g["startPoint"]).zip(xy(&g["endPoint"])).map(|(a, b)| {
                let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
                (vec![a, mid, b], 1e-5)
            }),
            // A full circle too (a path round it) *(seen)*.
            Some(t @ ("Arc3D" | "Circle3D")) => (|| {
                let center = xy(&g["center"])?;
                let r = g["radius"].as_f64()?;
                let (a0, a1) = if t == "Arc3D" {
                    (g["startAngle"].as_f64()?, g["endAngle"].as_f64()?)
                } else {
                    (0.0, std::f64::consts::TAU)
                };
                let points = (0..=4)
                    .map(|k| {
                        let a = a0 + (a1 - a0) * f64::from(k) / 4.0;
                        [center[0] + r * a.cos(), center[1] + r * a.sin()]
                    })
                    .collect();
                // The path samples an arc with 720 chords.
                Some((
                    points,
                    r * (1.0 - (std::f64::consts::PI / 720.0).cos()) + 1e-5,
                ))
            })(),
            _ => None,
        };
        let (Some((samples, tolerance)), Some(id)) = (samples, c["id"].as_str()) else {
            continue;
        };
        let at: Vec<(usize, f64)> = samples.iter().map(|&p| along(model(p))).collect();
        if at.iter().all(|(_, d)| *d <= tolerance) {
            on.push((at[at.len() / 2].0, id.to_owned()));
        }
    }
    on.sort();
    on
}

pub const ENUM_SHELL: [u8; 16] = type_id("e469a1b3d111382960008cb801f31bb0");
/// A list of faces (`f1b2bc24…`: the prefix and a list of face names
/// `ccb59f50…`, not decoded).
pub const FACES: [u8; 16] = type_id("f1b2bc24d01121dd0008d0bc0663dc09");

/// A shell: slots 0 its direction (`e469a1b3…`: 0 inside), 1 the faces it
/// removes (their topological names, not decoded: the import finds them
/// with the history; their number is given), 3 the thickness, 9 its body
/// *(seen: every shell of the test files, inside, with no face or one)*.
pub fn shell(cx: &Context, record: usize) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let thickness = cx
        .parameter(slot(3), &mut t.parameters)
        .ok_or("its thickness was not read")?;
    let side = match cx.enumeration(slot(0), &ENUM_SHELL) {
        Some(0) => "insideThickness",
        other => return Err(format!("shell direction {other:?} is not known")),
    };
    let removed = cx
        .list(slot(1).filter(|&r| cx.dc.is(r, &FACES)))
        .ok_or("its faces were not read")?
        .len();
    let bodies = cx.body_references(slot(9), &BODIES);
    let [body] = &bodies[..] else {
        return Err(format!("{} bodies shelled", bodies.len()));
    };
    t.detail = json!({"inputEntities": [body], side: thickness, "isTangentChain": false,
                      "shellType": "SharpOffsetShellType", "_ipt_removed_faces": removed});
    if removed > 0 {
        t.notes
            .push("the faces it removes are found with the history".to_owned());
    }
    Ok(t)
}

pub const ENUM_HOLE: [u8; 16] = type_id("117ccd43d2119658a00021803603c8c9");
pub const ENUM_FILLET: [u8; 16] = type_id("2788f278c54dd7be4313b3986039b52e");
pub const ENUM_CHAMFER: [u8; 16] = type_id("3200aa7dd2112b836000f3a89dccefb0");
pub const FILLET_SETS: [u8; 16] = type_id("dae9481bd211dc2c00083eab1b14dc09");
pub const FILLET_SET: [u8; 16] = type_id("1641d6aad211db2c00083eab1b14dc09");
pub const TRANSFORM: [u8; 16] = crate::sketch::TRANSFORM;
pub const HOLE_POINTS: [u8; 16] = type_id("b1226593d2115359a00021803603c8c9");

/// One profile reference per selected profile, with its sketch, and its
/// area and centroid where they are measured (all or none).
fn profiles(
    cx: &Context,
    patch: Option<usize>,
    index: i64,
    name: &str,
    sketch: Option<usize>,
) -> Vec<Value> {
    let selections = cx
        .list(patch.filter(|&r| cx.dc.is(r, &BOUNDARY_PATCH)))
        .unwrap_or_default();
    let m = sketch.and_then(|s| crate::sketch::sketch_matrix(cx.dc, s).ok());
    let each: Option<Vec<Option<crate::profile::Measured>>> = m.map(|m| {
        selections
            .iter()
            .map(|&s| cx.profiles.borrow_mut().measure(cx.dc, s, &m))
            .collect()
    });
    // The boundaries the file selected, for the sketch's curves on them.
    if let (Some(s), Some(each), Some(patch)) = (sketch, &each, patch) {
        let mut p = cx.profiles.borrow_mut();
        p.outlines.extend(
            each.iter()
                .flatten()
                .flat_map(|m| m.loops.iter().map(move |l| (s, patch, l.clone()))),
        );
    }
    // A boundary selected more than once (the regions of coincident
    // curves, as a pattern's copies on their original *(seen)*) is one
    // region: the extrusion of the same region again adds nothing.
    let same = |a: &crate::profile::Measured, b: &crate::profile::Measured| {
        let tol = 1e-9 * a.area.abs().max(1e-12);
        (a.area - b.area).abs() <= tol
            && (a.centroid[0] - b.centroid[0]).abs() <= 1e-9
            && (a.centroid[1] - b.centroid[1]).abs() <= 1e-9
    };
    // Selections inside one another make up their regions by the
    // even-odd rule (`profile::even_odd`), where a boundary selected twice
    // is a ring's hole and the next ring's outside *(seen: four loops,
    // the middle one twice, for the ring between the outer and the inner
    // one)*.
    let measured: Option<Vec<crate::profile::Measured>> = each
        .and_then(|each| each.into_iter().collect::<Option<Vec<_>>>())
        .and_then(|all| {
            let mut distinct: Vec<crate::profile::Measured> = Vec::new();
            for m in &all {
                if !distinct.iter().any(|d| same(d, m)) {
                    distinct.push(m.clone());
                }
            }
            if crate::profile::nested(&distinct) {
                crate::profile::even_odd(&all)
            } else {
                Some(distinct)
            }
        })
        .filter(|v| !v.is_empty());
    let reference = |m: Option<&crate::profile::Measured>| {
        let mut p = json!({"kind": "profile", "sketch": name, "sketch_timeline_index": index});
        if let Some(m) = m {
            p["area"] = json!(m.area);
            p["centroid"] = json!([m.centroid[0], m.centroid[1], 0.0]);
        }
        p
    };
    match &measured {
        Some(v) => v.iter().map(|m| reference(Some(m))).collect(),
        None => (0..selections.len().max(1))
            .map(|_| reference(None))
            .collect(),
    }
}

/// A hole: slots 0 form (0 drilled, 1 countersink, 2 counterbore, 3 spot
/// face), 1 diameter, 2 depth, 3 counterbore or countersink diameter, 4
/// counterbore depth, 5 countersink angle, 6 drill point angle, 8 its
/// placement (a transform whose origin is the hole's start point, model
/// coordinates), 9 extent (1 distance, 5 through all, 7 up to a face), 16
/// its direction *(seen)*.
pub fn hole(cx: &Context, record: usize, sketch: Option<usize>) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let mut d = json!({});
    let form = cx.enumeration(slot(0), &ENUM_HOLE);
    let size = |key: &str, i: usize, d: &mut Value, made: &mut Vec<usize>| -> Result<(), String> {
        let p = cx
            .parameter(slot(i), made)
            .ok_or_else(|| format!("its {key} was not read"))?;
        d[key] = p;
        Ok(())
    };
    size("holeDiameter", 1, &mut d, &mut t.parameters)?;
    match form {
        Some(0) => d["holeType"] = json!("SimpleHoleType"),
        Some(1) => {
            d["holeType"] = json!("CountersinkHoleType");
            size("countersinkDiameter", 3, &mut d, &mut t.parameters)?;
            size("countersinkAngle", 5, &mut d, &mut t.parameters)?;
        }
        Some(2) | Some(3) => {
            d["holeType"] = json!("CounterboreHoleType");
            size("counterboreDiameter", 3, &mut d, &mut t.parameters)?;
            size("counterboreDepth", 4, &mut d, &mut t.parameters)?;
            if form == Some(3) {
                t.notes
                    .push("a spot face, made as a counterbore".to_owned());
            }
        }
        other => return Err(format!("hole form {other:?} is not known")),
    }
    if let Some(mut tip) = cx.parameter(slot(6), &mut t.parameters) {
        // A drill point angle of 0: a flat bottom (the import's flat tip
        // is a straight angle).
        if tip["value"].as_f64() == Some(0.0) {
            tip = json!({"kind": "parameter", "value": std::f64::consts::PI});
        }
        d["tipAngle"] = tip;
    }
    match cx.enumeration(slot(9), &ENUM_EXTENT) {
        Some(1) => {
            let mut depth = json!({"_type": "DistanceExtentDefinition"});
            size("distance", 2, &mut depth, &mut t.parameters)?;
            d["extentDefinition"] = depth;
        }
        Some(5) => d["extentDefinition"] = json!({"_type": "ThroughAllExtentDefinition"}),
        Some(7) => {
            d["extentDefinition"] = json!({"_type": "ThroughAllExtentDefinition"});
            t.notes
                .push("a hole up to a face, made through all".to_owned());
        }
        other => return Err(format!("hole extent {other:?} is not known")),
    }
    // A hole on sketch points: slot 7 lists them (`b1226593…`), in the
    // sketch the feature's label shows under it; else the placement's
    // origin.
    let on_points = sketch.zip(slot(7).filter(|&r| cx.dc.is(r, &HOLE_POINTS)));
    if let Some((sketch, list)) = on_points {
        let m = crate::sketch::sketch_matrix(cx.dc, sketch)?;
        let points: Vec<[f64; 3]> = cx
            .list(Some(list))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| crate::sketch::point(cx.dc, p))
            .map(|[x, y]| {
                let at = |r: usize| m[r][0] * x + m[r][1] * y + m[r][3];
                [at(0), at(1), at(2)]
            })
            .collect();
        if points.is_empty() {
            return Err("its sketch points were not read".to_owned());
        }
        d["position"] = json!(points[0]);
        d["_f3d_positions"] = json!(points);
    } else if let Some(m) = slot(8)
        .filter(|&r| cx.dc.is(r, &TRANSFORM))
        .and_then(|r| crate::sketch::matrix(cx.dc, r).ok())
    {
        d["position"] = json!([m[0][3], m[1][3], m[2][3]]);
    }
    if let Some(v) = cx.direction(slot(16)) {
        d["direction"] = json!(v);
    }
    t.detail = d;
    Ok(t)
}

/// A fillet: slot 0 the edge sets (`dae9481b…`, a list of `1641d6aa…`:
/// the prefix, the edges, the radius, the selection and a boolean), 11 its
/// form (0 edge fillet). The edges are named by the file's topological
/// names, which are not decoded: the import finds them from the history
/// state. The boolean was taken for the tangent chain option, but it reads
/// false in every fillet of the test files while their history states
/// show whole tangent chains rounded from edges inside them: the sets
/// follow tangent chains (the dump's default).
pub fn fillet(cx: &Context, record: usize) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    match cx.enumeration(slot(11), &ENUM_FILLET) {
        Some(0) => {}
        other => return Err(format!("fillet form {other:?} is not decoded")),
    }
    let sets = cx
        .list(slot(0).filter(|&r| cx.dc.is(r, &FILLET_SETS)))
        .ok_or("its edge sets were not read")?;
    let mut edge_sets = Vec::new();
    for set in sets.into_iter().filter(|&s| cx.dc.is(s, &FILLET_SET)) {
        let mut r = cx.dc.fields(set);
        let refs = (0..4)
            .map(|_| r.reference())
            .collect::<dc::Result<Vec<Option<usize>>>>()
            .map_err(|e| format!("edge set {set}: {e}"))?;
        let radius = cx
            .parameter(refs[1], &mut t.parameters)
            .ok_or("an edge set without its radius")?;
        edge_sets.push(json!({"_type": "ConstantRadiusFilletEdgeSet", "radius": radius}));
    }
    if edge_sets.is_empty() {
        return Err("no edge sets read".to_owned());
    }
    t.detail = json!({"edgeSets": edge_sets});
    Ok(t)
}

/// A chamfer: slot 0 its edges, 2 the distance, 3 the second distance, 4
/// its form (0 equal distances, 1 distance and angle, 2 two distances), 10
/// the angle *(seen)*.
pub fn chamfer(cx: &Context, record: usize) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let p = |i: usize, made: &mut Vec<usize>| {
        cx.parameter(slot(i), made)
            .ok_or_else(|| format!("slot {i} was not read"))
    };
    let set = match cx.enumeration(slot(4), &ENUM_CHAMFER) {
        Some(0) => {
            json!({"_type": "EqualDistanceChamferEdgeSet", "distance": p(2, &mut t.parameters)?})
        }
        Some(1) => json!({"_type": "DistanceAndAngleChamferEdgeSet",
            "distance": p(2, &mut t.parameters)?, "angle": p(10, &mut t.parameters)?}),
        Some(2) => json!({"_type": "TwoDistancesChamferEdgeSet",
            "distanceOne": p(2, &mut t.parameters)?, "distanceTwo": p(3, &mut t.parameters)?}),
        other => return Err(format!("chamfer form {other:?} is not known")),
    };
    t.detail = json!({"edgeSets": [set]});
    Ok(t)
}

pub const WORK_PLANE: [u8; 16] = type_id("42df52ced011d0d20008ccbc0663dc09");
pub const WORK_AXIS: [u8; 16] = type_id("896cf08ed1113c0460007cb801f31bb0");
pub const PATTERN_DIRECTION: [u8; 16] = type_id("7b4544a2ab4d9e16850c48a21edf9e3a");

/// A work plane's origin and x and y axes (model coordinates, cm): the
/// last nine f64 of its record *(verified on the origin planes)*.
pub fn plane(dc: &Definitions, record: usize) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
    if !dc.is(record, &WORK_PLANE) {
        return None;
    }
    let bytes = dc.bytes(record);
    let mut r = Reader::at(bytes, bytes.len().checked_sub(72)?);
    let mut v = [[0.0; 3]; 3];
    for p in &mut v {
        for x in p.iter_mut() {
            *x = r.f64().ok()?;
        }
    }
    v.iter()
        .flatten()
        .all(|x| x.is_finite())
        .then_some((v[0], v[1], v[2]))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n > 1e-12).then(|| v.map(|x| x / n))
}

/// A work plane as the dump's construction plane reference: an origin
/// plane by its name, an imported work plane by its item, else its
/// geometry.
pub fn plane_reference(cx: &Context, record: usize, items: &HashMap<usize, i64>) -> Option<Value> {
    let (origin, x, y) = plane(cx.dc, record)?;
    let normal = unit(cross(x, y))?;
    let on_origin = origin.iter().all(|v| v.abs() < 1e-9);
    let axis = |i: usize| (normal[i].abs() - 1.0).abs() < 1e-9;
    if on_origin
        && let Some(name) = [(2, "XY"), (1, "XZ"), (0, "YZ")]
            .into_iter()
            .find(|&(i, _)| axis(i))
            .map(|(_, n)| n)
    {
        return Some(json!({"kind": "construction_plane", "origin": name, "timeline_index": null}));
    }
    let geometry = json!({"type": "Plane", "origin": origin, "normal": normal, "uDirection": x});
    Some(json!({"kind": "construction_plane", "origin": null,
                "timeline_index": items.get(&record), "geometry": geometry}))
}

/// An offset plane's definition (`c0d8280d…`): the header, an i32, the
/// plane, the plane it is offset from and the offset (a parameter)
/// *(seen)*.
pub const PLANE_OFFSET: [u8; 16] = type_id("c0d8280dd211c82b60008db7b035c3b0");

/// The plane a work plane is offset from, and the offset.
pub fn plane_offset(dc: &Definitions, record: usize) -> Option<(usize, usize)> {
    dc.of_type(&PLANE_OFFSET).find_map(|d| {
        let mut r = dc.body(d);
        r.i32().ok()?;
        let plane = r.reference().ok()??;
        let base = r.reference().ok()??;
        let offset = r.reference().ok()??;
        (plane == record && dc.is(base, &WORK_PLANE) && dc.is(offset, &crate::params::PARAMETER))
            .then_some((base, offset))
    })
}

/// A mid plane's definition (`560a6ccb…`: the header, an i32, the plane
/// and the two planes it lies midway between) *(seen)*.
pub const PLANE_MID: [u8; 16] = type_id("560a6ccba54b56ceec60569ccfa621b0");
/// A plane at an angle (`8af4e674…`: the header, an i32, the plane, the
/// work axis it turns about, the plane it is at an angle to, the angle (a
/// parameter), then data not decoded) *(seen)*.
pub const PLANE_ANGLE: [u8; 16] = type_id("8af4e674d2117f3160008db7b035c3b0");

/// A plane through three work points (`8a924209…`: the header, an i32, the
/// plane and the three points) *(seen)*.
pub const PLANE_THREE_POINTS: [u8; 16] = type_id("8a924209d211042860008cb7b035c3b0");
/// A plane through two work axes (`5abeff8a…`: the header, an i32, the
/// plane and the two axes) *(seen)*.
pub const PLANE_TWO_AXES: [u8; 16] = type_id("5abeff8ad211e92b60008db7b035c3b0");
/// A plane of a work point and a work axis (`62b73858…`: the header, an
/// i32, the plane, the point and the axis): through the axis and the point,
/// or through the point normal to the axis (its geometry tells) *(seen)*.
pub const PLANE_POINT_AXIS: [u8; 16] = type_id("62b73858d2116e3160008db7b035c3b0");
/// A work point (`3edf52ce…`): after the header, the prefix and 12 bytes
/// (up to major 18; 16 from major 24) its position (three f64, model
/// coordinates, cm) and two lists, empty *(seen)*.
pub const WORK_POINT: [u8; 16] = type_id("3edf52ced011d0d20008ccbc0663dc09");

/// A work point's position (cm).
pub fn work_point(dc: &Definitions, record: usize) -> Option<[f64; 3]> {
    if !dc.is(record, &WORK_POINT) {
        return None;
    }
    // Its position is followed by two lists, empty in every work point
    // seen (12 bytes before it up to major 18, 16 from 24).
    let bytes = dc.bytes(record);
    let n = bytes.len().checked_sub(40)?;
    let empty = [2, 0, 0, 0x30, 0, 0, 0, 0];
    if bytes[n + 24..n + 32] != empty || bytes[n + 32..] != empty {
        return None;
    }
    let mut r = Reader::at(bytes, n);
    let p = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
    // Within 10 km: other bytes read as a position are not.
    p.iter().all(|x| x.abs() < 1e6).then_some(p)
}

/// A work point as the dump's construction point reference: the origin
/// by its name, else its position (cm).
fn point_entity(dc: &Definitions, record: usize) -> Option<Value> {
    let p = work_point(dc, record)?;
    Some(if p.iter().all(|v| v.abs() < 1e-12) {
        json!({"kind": "construction_point", "origin": "Origin", "timeline_index": null})
    } else {
        json!({"kind": "construction_point", "origin": null, "timeline_index": null,
               "geometry": {"type": "Point3D", "origin": p}})
    })
}

/// The references after the plane in a definition of `record` of type
/// `t` (the header, an i32, the plane, then `n` references).
fn plane_definition(dc: &Definitions, record: usize, t: &[u8; 16], n: usize) -> Option<Vec<usize>> {
    dc.of_type(t).find_map(|d| {
        let mut r = dc.body(d);
        r.i32().ok()?;
        if r.reference().ok()?? != record {
            return None;
        }
        (0..n).map(|_| r.reference().ok().flatten()).collect()
    })
}

/// A work axis as the dump's construction axis reference: an origin axis
/// by its name, else its geometry (cm).
fn axis_entity(dc: &Definitions, record: usize) -> Option<Value> {
    let (p, v) = axis(dc, record)?;
    let along = p[0] * v[0] + p[1] * v[1] + p[2] * v[2];
    let through_origin = (0..3).all(|i| (p[i] - along * v[i]).abs() < 1e-9);
    let origin = [(0, "X"), (1, "Y"), (2, "Z")]
        .into_iter()
        .find(|&(i, _)| (v[i].abs() - 1.0).abs() < 1e-9)
        .map(|(_, n)| n);
    Some(match origin {
        Some(name) if through_origin => {
            json!({"kind": "construction_axis", "origin": name, "timeline_index": null})
        }
        _ => json!({"kind": "construction_axis", "origin": null, "timeline_index": null,
                    "geometry": {"type": "Line3D", "origin": p, "direction": v}}),
    })
}

/// A work plane item: offset from another plane where the file says so
/// (`ConstructionPlaneOffsetDefinition`), midway between two
/// (`ConstructionPlaneMidplaneDefinition`: `planarEntityOne` and `Two`) or
/// at an angle to one about a work axis
/// (`ConstructionPlaneAtAngleDefinition`: `linearEntity`, `planarEntity`,
/// `angle`), else fixed where it is; its geometry either way.
pub fn work_plane(
    cx: &Context,
    record: usize,
    items: &HashMap<usize, i64>,
) -> Result<Translated, String> {
    let (origin, x, y) = plane(cx.dc, record).ok_or("its geometry was not read")?;
    let normal = unit(cross(x, y)).ok_or("its axes are parallel")?;
    let mut t = Translated {
        detail: json!({"geometry": {"type": "Plane", "origin": origin, "normal": normal,
                                    "uDirection": x}}),
        ..Translated::default()
    };
    if let Some((base, offset)) = plane_offset(cx.dc, record)
        && let Some(base) = plane_reference(cx, base, items)
        && let Some(offset) = cx.parameter(Some(offset), &mut t.parameters)
    {
        t.detail["definition"] = json!({"_type": "ConstructionPlaneOffsetDefinition",
            "planarEntity": base, "offset": offset});
    } else if let Some(refs) = plane_definition(cx.dc, record, &PLANE_MID, 2)
        && let (Some(a), Some(b)) = (
            plane_reference(cx, refs[0], items),
            plane_reference(cx, refs[1], items),
        )
    {
        t.detail["definition"] = json!({"_type": "ConstructionPlaneMidplaneDefinition",
            "planarEntityOne": a, "planarEntityTwo": b});
    } else if let Some(refs) = plane_definition(cx.dc, record, &PLANE_ANGLE, 3)
        && let Some(line) = axis_entity(cx.dc, refs[0])
        && let Some(base) = plane_reference(cx, refs[1], items)
        && let Some(angle) = cx.parameter(Some(refs[2]), &mut t.parameters)
    {
        t.detail["definition"] = json!({"_type": "ConstructionPlaneAtAngleDefinition",
            "linearEntity": line, "planarEntity": base, "angle": angle});
    } else if let Some(refs) = plane_definition(cx.dc, record, &PLANE_THREE_POINTS, 3)
        && let Some(points) = refs
            .iter()
            .map(|&p| point_entity(cx.dc, p))
            .collect::<Option<Vec<Value>>>()
    {
        t.detail["definition"] = json!({"_type": "ConstructionPlaneThreePointsDefinition",
            "pointEntityOne": points[0], "pointEntityTwo": points[1],
            "pointEntityThree": points[2]});
    } else if let Some(refs) = plane_definition(cx.dc, record, &PLANE_TWO_AXES, 2)
        && let (Some(a), Some(b)) = (axis_entity(cx.dc, refs[0]), axis_entity(cx.dc, refs[1]))
    {
        t.detail["definition"] = json!({"_type": "ConstructionPlaneTwoEdgesDefinition",
            "linearEntityOne": a, "linearEntityTwo": b});
    } else if let Some(refs) = plane_definition(cx.dc, record, &PLANE_POINT_AXIS, 2)
        && let (Some(point), Some(line), Some((_, v))) = (
            point_entity(cx.dc, refs[0]),
            axis_entity(cx.dc, refs[1]),
            axis(cx.dc, refs[1]),
        )
    {
        // Through the axis, or normal to it.
        let along = (normal[0] * v[0] + normal[1] * v[1] + normal[2] * v[2]).abs();
        let kind = if along < 1e-9 {
            "ConstructionPlaneLineAndPointDefinition"
        } else {
            "ConstructionPlaneNormalToLineDefinition"
        };
        t.detail["definition"] = json!({"_type": kind, "linearEntity": line, "pointEntity": point});
    } else if let Some(base) = [PLANE_PARALLEL_POINT, PLANE_PARALLEL_GEOMETRY]
        .iter()
        .filter_map(|ty| plane_definition(cx.dc, record, ty, 2))
        .find_map(|refs| refs.into_iter().find(|&r| cx.dc.is(r, &WORK_PLANE)))
        && let Some((reference, offset)) = parallel_offset(cx, base, items, origin, normal)
    {
        // Parallel to a plane through a point: offset from that plane to
        // where the point puts it (the point fixed where it is, as work
        // points are).
        t.detail["definition"] = json!({"_type": "ConstructionPlaneOffsetDefinition",
            "planarEntity": reference, "offset": {"kind": "parameter", "value": offset}});
    }
    Ok(t)
}

/// A plane parallel to a work plane through a work point (`63b73858…`:
/// the header, an i32, the plane, the point and the plane it is parallel
/// to) *(seen: the plane's origin is the point)*.
pub const PLANE_PARALLEL_POINT: [u8; 16] = type_id("63b73858d2116e3160008db7b035c3b0");
/// A plane parallel to a work plane through a point of the model's
/// geometry (`eea9eab3…`: the header, an i32, the plane, the plane it is
/// parallel to, a reference to the model's geometry `4a068a52…` (an edge
/// with its curve, not decoded), then two f64) *(seen)*.
pub const PLANE_PARALLEL_GEOMETRY: [u8; 16] = type_id("eea9eab3d211dd3260008db7b035c3b0");

/// The base plane's reference and the offset (cm) along its normal (an
/// origin plane's own: +z for XY, +y for XZ, +x for YZ) that puts a plane
/// parallel to it at `origin`; None when the planes are not parallel.
fn parallel_offset(
    cx: &Context,
    base: usize,
    items: &HashMap<usize, i64>,
    origin: [f64; 3],
    normal: [f64; 3],
) -> Option<(Value, f64)> {
    let (o, x, y) = plane(cx.dc, base)?;
    let n = unit(cross(x, y))?;
    let c = cross(n, normal);
    if c.iter().map(|v| v * v).sum::<f64>().sqrt() > 1e-9 {
        return None;
    }
    let reference = plane_reference(cx, base, items)?;
    let n = match reference["origin"].as_str() {
        Some("XY") => [0.0, 0.0, 1.0],
        Some("XZ") => [0.0, 1.0, 0.0],
        Some("YZ") => [1.0, 0.0, 0.0],
        _ => n,
    };
    let d: f64 = (0..3).map(|i| (origin[i] - o[i]) * n[i]).sum();
    Some((reference, d))
}

/// The features a pattern or mirror copies and its properties: the record
/// is a feature's (the header, i32, u32, a reference list) followed by a
/// u32, the list of the copied features, and its properties as
/// references at no fixed offsets *(seen)*.
fn copies(cx: &Context, record: usize) -> Result<(Vec<usize>, Vec<usize>), String> {
    let mut r = cx.dc.body(record);
    let inputs = (|| -> dc::Result<Vec<usize>> {
        r.i32()?;
        r.u32()?;
        r.list()?;
        r.u32()?;
        r.references()
    })()
    .map_err(|e| format!("pattern {record}: {e}"))?;
    let bytes = cx.dc.bytes(record);
    let mut props = Vec::new();
    let mut at = r.pos;
    while at + 4 <= bytes.len() {
        let v = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        match dc::reference(v) {
            Some(p) if v & 0x8000_0000 != 0 && p < cx.dc.len() => {
                props.push(p);
                at += 4;
            }
            _ => at += 1,
        }
    }
    Ok((inputs, props))
}

/// Feature references for the copied features that are on the timeline,
/// in the timeline's order (the file lists them in the order they were
/// picked).
fn inputs(items: &HashMap<usize, i64>, records: &[usize]) -> Result<Vec<Value>, String> {
    let mut indices = records
        .iter()
        .map(|r| {
            items
                .get(r)
                .copied()
                .ok_or_else(|| format!("its input (record {r}) is not on the timeline"))
        })
        .collect::<Result<Vec<i64>, String>>()?;
    indices.sort_unstable();
    Ok(indices
        .into_iter()
        .map(|i| json!({"kind": "feature", "timeline_index": i}))
        .collect())
}

/// A rectangular pattern: the copied features; among its properties the
/// counts (integer parameters), the spacings and the two directions
/// (`7b4544a2…`: u32, a flip (boolean), a reference, u32, u16, u32, then
/// a start point, a vector and an end point, f64 each) in that order.
pub fn rectangular_pattern(
    cx: &Context,
    record: usize,
    items: &HashMap<usize, i64>,
) -> Result<Translated, String> {
    let (copied, props) = copies(cx, record)?;
    let mut t = Translated::default();
    let mut d = json!({"inputEntities": inputs(items, &copied)?,
                       "patternEntityType": "FeaturesPatternType"});
    let counts: Vec<usize> = props
        .iter()
        .copied()
        .filter(|&p| cx.dc.is(p, &crate::params::INTEGER))
        .filter(|&p| {
            cx.params
                .by_record(p)
                .is_some_and(|q| !q.name.starts_with("RDx"))
        })
        .collect();
    let spacings: Vec<usize> = props
        .iter()
        .copied()
        .filter(|&p| cx.dc.is(p, &crate::params::PARAMETER))
        .collect();
    let directions: Vec<usize> = props
        .iter()
        .copied()
        .filter(|&p| cx.dc.is(p, &PATTERN_DIRECTION))
        .collect();
    for (k, (quantity, distance, vector)) in [
        ("quantityOne", "distanceOne", "directionOne"),
        ("quantityTwo", "distanceTwo", "directionTwo"),
    ]
    .into_iter()
    .enumerate()
    {
        let (Some(&c), Some(&s), Some(&v)) = (counts.get(k), spacings.get(k), directions.get(k))
        else {
            // A direction of another record (a direction `40df52ce…`
            // whose flip is not decoded, a path `aa7b97a5…`) *(seen)*: the
            // count and spacing, the direction found with the history.
            if k == 0
                && directions.is_empty()
                && let (Some(&c), Some(&s)) = (counts.first(), spacings.first())
            {
                d[quantity] = cx.parameter(Some(c), &mut t.parameters).ok_or("a count")?;
                d[distance] = cx
                    .parameter(Some(s), &mut t.parameters)
                    .ok_or("a spacing")?;
                t.notes
                    .push("its direction is found with the history".to_owned());
                break;
            }
            if k == 0 {
                return Err("its first direction was not read".to_owned());
            }
            break;
        };
        d[quantity] = cx.parameter(Some(c), &mut t.parameters).ok_or("a count")?;
        d[distance] = cx
            .parameter(Some(s), &mut t.parameters)
            .ok_or("a spacing")?;
        d[vector] = json!(pattern_direction(cx, v).ok_or("a direction was not read")?);
    }
    t.detail = d;
    Ok(t)
}

/// A pattern direction's vector, reversed when flipped.
fn pattern_direction(cx: &Context, record: usize) -> Option<[f64; 3]> {
    let mut r = cx.dc.body(record);
    r.u32().ok()?;
    let flip = cx.boolean(r.reference().ok()?);
    r.reference().ok()?;
    r.skip(4 + 2 + 4 + 24).ok()?;
    let v = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
    let v = unit(v)?;
    Some(if flip == Some(true) { v.map(|x| -x) } else { v })
}

/// A circular pattern: the copied features; among its properties the
/// count (an integer parameter), the angle and the axis (a work axis: its
/// last 48 bytes before a byte are a point and a direction) *(seen)*.
pub fn circular_pattern(
    cx: &Context,
    record: usize,
    items: &HashMap<usize, i64>,
) -> Result<Translated, String> {
    let (copied, props) = copies(cx, record)?;
    let mut t = Translated::default();
    let count = props
        .iter()
        .copied()
        .find(|&p| {
            cx.dc.is(p, &crate::params::INTEGER)
                && cx
                    .params
                    .by_record(p)
                    .is_some_and(|q| !q.name.starts_with("RDx"))
        })
        .ok_or("its count was not read")?;
    let angle = props
        .iter()
        .copied()
        .find(|&p| cx.dc.is(p, &crate::params::PARAMETER))
        .ok_or("its angle was not read")?;
    let axis = props
        .iter()
        .copied()
        .find(|&p| cx.dc.is(p, &WORK_AXIS))
        .and_then(|a| axis(cx.dc, a));
    let mut d = json!({"inputEntities": inputs(items, &copied)?,
        "patternEntityType": "FeaturesPatternType",
        "quantity": cx.parameter(Some(count), &mut t.parameters),
        "totalAngle": cx.parameter(Some(angle), &mut t.parameters)});
    match axis {
        Some((p, v)) => {
            d["axis"] = json!({"kind": "construction_axis", "origin": null, "timeline_index": null,
                "geometry": {"type": "InfiniteLine3D", "origin": p, "direction": v}});
        }
        None => t.notes.push("its axis was not read".to_owned()),
    }
    t.detail = d;
    Ok(t)
}

/// A mirror: the copied features and its plane (a work plane among its
/// properties).
pub fn mirror(
    cx: &Context,
    record: usize,
    items: &HashMap<usize, i64>,
) -> Result<Translated, String> {
    let (copied, props) = copies(cx, record)?;
    let plane = props
        .iter()
        .copied()
        .find(|&p| cx.dc.is(p, &WORK_PLANE))
        .and_then(|p| plane_reference(cx, p, items));
    let mut t = Translated::default();
    let mut d = json!({"inputEntities": inputs(items, &copied)?,
                       "patternEntityType": "FeaturesPatternType"});
    match plane {
        Some(p) => d["mirrorPlane"] = p,
        None => t.notes.push("its plane was not read".to_owned()),
    }
    t.detail = d;
    Ok(t)
}

/// A thread's ends (`9984474c…`, slot 10 of a thread).
pub const THREAD_ENDS: [u8; 16] = type_id("9984474cf54708717e794c81ae021d41");
/// A thread's size (`1f7e08a4…`): its context is the thread feature.
pub const THREAD_SIZE: [u8; 16] = type_id("1f7e08a4d4115f956000c6a4cae1fbb0");

/// A thread's size from its record (`1f7e08a4…`, the thread feature its
/// header's context): after the header a reference, then texts: the
/// nominal size, the designation (`M3x0.5`), the thread type (`ISO Metric
/// profile`), an empty text, a u32, four empty texts and the class (`6g`,
/// `6H`), then the limits of its diameters and its pitch as texts in the
/// saving machine's number format *(seen)*.
fn thread_size(dc: &Definitions, feature: usize) -> Option<(String, String, String)> {
    let record = dc
        .of_type(&THREAD_SIZE)
        .find(|&r| dc.header(r).is_some_and(|h| h.context == Some(feature)))?;
    let mut r = dc.body(record);
    r.u32().ok()?;
    r.text().ok()?;
    let designation = r.text().ok()?;
    let kind = r.text().ok()?;
    r.text().ok()?;
    r.u32().ok()?;
    for _ in 0..4 {
        r.text().ok()?;
    }
    let class = r.text().ok()?;
    (!designation.is_empty()).then_some((designation, kind, class))
}

/// A thread: slots 0 its faces (named by the file's topological names, not
/// decoded), 1 full length (a boolean), 3 the length and 4 the offset of a
/// thread that is not, 6 modelled (a boolean) and 10 its ends
/// (`9984474c…`: after the prefix two points, cm, on its cylinder's
/// surface or axis, where the thread starts and where it ends) *(seen)*;
/// its size in a record of its own ([`thread_size`]). The face is the
/// cylinder through both points, which the import looks for.
pub fn thread(cx: &Context, record: usize) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let ends = slot(10)
        .filter(|&r| cx.dc.is(r, &THREAD_ENDS))
        .and_then(|r| {
            let mut rd = cx.dc.fields(r);
            let mut p = [[0.0; 3]; 2];
            for q in &mut p {
                for x in q.iter_mut() {
                    *x = rd.f64().ok()?;
                }
            }
            p.iter().flatten().all(|x| x.is_finite()).then_some(p)
        })
        .ok_or("its ends were not read")?;
    let along = [
        ends[1][0] - ends[0][0],
        ends[1][1] - ends[0][1],
        ends[1][2] - ends[0][2],
    ];
    let axis = unit(along).ok_or("its ends are one point")?;
    let (designation, kind, class) = thread_size(cx.dc, record).ok_or("its size was not read")?;
    let full = cx.boolean(slot(1)).ok_or("its extent was not read")?;
    let mut d = json!({
        "inputCylindricalFaces": [{"kind": "face", "geometry": {"type": "Cylinder", "axis": axis},
                                   "start_point": ends[0], "end_point": ends[1]}],
        "threadInfo": {"threadType": kind, "threadDesignation": designation,
                       "threadClass": class},
        "isModeled": cx.boolean(slot(6)) == Some(true),
        "isFullLength": full,
        // It starts at its first end: the low end along the face's axis
        // given above.
        "threadLocation": "LowEndThreadLocation",
    });
    if !full {
        d["threadLength"] = cx
            .parameter(slot(3), &mut t.parameters)
            .ok_or("its length was not read")?;
        if let Some(offset) = cx.parameter(slot(4), &mut t.parameters) {
            d["threadOffset"] = offset;
        }
    }
    t.detail = d;
    Ok(t)
}

/// A work axis: its last 48 bytes before a byte are a point and a
/// direction (model coordinates, cm).
pub fn axis(dc: &Definitions, record: usize) -> Option<([f64; 3], [f64; 3])> {
    if !dc.is(record, &WORK_AXIS) {
        return None;
    }
    let bytes = dc.bytes(record);
    let mut r = Reader::at(bytes, bytes.len().checked_sub(49)?);
    let p = [r.f64().ok()?, r.f64().ok()?, r.f64().ok()?];
    let v = unit([r.f64().ok()?, r.f64().ok()?, r.f64().ok()?])?;
    p.iter().all(|x| x.is_finite()).then_some((p, v))
}

/// The dump id of a line of a sketch that lies on the axis through `p`
/// along `v` (model coordinates, cm), and the line's direction from its
/// start to its end.
fn sketch_line_on(
    cx: &Context,
    sketch: usize,
    p: [f64; 3],
    v: [f64; 3],
) -> Option<(String, [f64; 3])> {
    let ids = cx.sketch_ids.get(&sketch)?;
    let m = crate::sketch::sketch_matrix(cx.dc, sketch).ok()?;
    let model = |q: [f64; 2]| {
        let at = |r: usize| m[r][0] * q[0] + m[r][1] * q[1] + m[r][3];
        [at(0), at(1), at(2)]
    };
    let off_axis = |q: [f64; 3]| {
        let d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
        let c = cross(d, v);
        (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
    };
    let mut lines: Vec<(&usize, &String)> = ids
        .iter()
        .filter(|(r, _)| cx.dc.is(**r, &crate::sketch::LINE))
        .collect();
    lines.sort();
    lines.into_iter().find_map(|(&r, id)| {
        let mut rd = cx.dc.fields(r);
        rd.skip(8).ok()?;
        let ends = rd.references().ok()?;
        let a = model(crate::sketch::point(cx.dc, *ends.first()?)?);
        let b = model(crate::sketch::point(cx.dc, *ends.get(1)?)?);
        (off_axis(a) < 1e-6 && off_axis(b) < 1e-6)
            .then(|| (id.clone(), [b[0] - a[0], b[1] - a[1], b[2] - a[2]]))
    })
}

/// A revolution: slots 0 operation, 1 profiles, 2 axis (a work axis), 3
/// extent (1 an angle, 3 a full turn *(verified)*), 4 the angle, 5 its
/// direction (an enumeration `c26aa0c7…`, not decoded) *(seen)*. An axis along an origin axis
/// is named so; another is left to the import, which tries the sketch's
/// lines.
pub fn revolve(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = match cx.enumeration(slot(0), &ENUM_OPERATION) {
        Some(1) => "NewBodyFeatureOperation",
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("operation {other:?} is not known")),
    };
    let mut d = json!({"operation": operation, "isSolid": true});
    participants(cx, &slots, operation, &mut d);
    match cx.enumeration(slot(3), &ENUM_EXTENT) {
        // 1 an angle.
        Some(1) => {
            let angle = cx
                .parameter(slot(4), &mut t.parameters)
                .ok_or("its angle was not read")?;
            d["extentDefinition"] = json!({"_type": "AngleExtentDefinition", "angle": angle});
        }
        // 3 a full turn: the angle parameter keeps a value the extent does
        // not use (0, or the 90° it was made with).
        Some(3) => {
            d["extentDefinition"] = json!({"_type": "AngleExtentDefinition",
                "angle": {"kind": "parameter", "value": std::f64::consts::TAU, "unit": "rad"}});
        }
        other => return Err(format!("revolution extent {other:?} is not known")),
    }
    if let Some((index, name)) = &sketch {
        d["profile"] = json!(profiles(cx, slot(1), *index, name, sketch_record));
    }
    if let Some(a) = slot(2) {
        match axis_reference(cx, a, sketch.as_ref(), sketch_record) {
            Some(a) => d["axis"] = a,
            None => t
                .notes
                .push("its axis is found among the sketch's lines".to_owned()),
        }
    }
    t.detail = d;
    Ok(t)
}

/// A work axis as the dump's reference: a line of the sketch on it, else
/// an origin axis it lies on, else its geometry (cm).
fn axis_reference(
    cx: &Context,
    record: usize,
    sketch: Option<&(i64, String)>,
    sketch_record: Option<usize>,
) -> Option<Value> {
    axis_reference_along(cx, record, sketch, sketch_record).map(|(a, _)| a)
}

/// [`axis_reference`], and whether the reference runs against the work
/// axis' stored direction (a line from its start to its end, an origin
/// axis along +X, +Y or +Z).
fn axis_reference_along(
    cx: &Context,
    record: usize,
    sketch: Option<&(i64, String)>,
    sketch_record: Option<usize>,
) -> Option<(Value, bool)> {
    let (p, v) = axis(cx.dc, record)?;
    let against = |d: [f64; 3]| d[0] * v[0] + d[1] * v[1] + d[2] * v[2] < 0.0;
    let through_origin = {
        // The axis passes through the origin: p minus its part along v
        // is zero.
        let along = p[0] * v[0] + p[1] * v[1] + p[2] * v[2];
        (0..3).all(|i| (p[i] - along * v[i]).abs() < 1e-9)
    };
    let origin = [(0, "X"), (1, "Y"), (2, "Z")]
        .into_iter()
        .find(|&(i, _)| (v[i].abs() - 1.0).abs() < 1e-9);
    let line = sketch.zip(sketch_record).and_then(|((index, _), s)| {
        let (id, d) = sketch_line_on(cx, s, p, v)?;
        Some((
            json!({"kind": "sketch_entity", "objectType": "SketchLine",
                "sketch_timeline_index": index, "id": id}),
            against(d),
        ))
    });
    match (origin, line) {
        (_, Some(line)) => Some(line),
        (Some((i, name)), None) if through_origin => Some((
            json!({"kind": "construction_axis", "origin": name, "timeline_index": null}),
            v[i] < 0.0,
        )),
        // Another work axis (a cylinder's axis, a line of another sketch):
        // by its geometry, fixed where it is as work axes are.
        _ => Some((
            json!({"kind": "construction_axis", "origin": null, "timeline_index": null,
                   "geometry": {"type": "Line3D", "origin": p, "direction": v}}),
            false,
        )),
    }
}

/// A coil's type (`b80cb14f…`: the prefix, two u16, the second the type)
/// *(seen)*.
pub const COIL_TYPE: [u8; 16] = type_id("b80cb14fd21158d6600013a99dccefb0");

/// A coil: slots 0 operation, 1 profiles, 2 axis (a work axis), 3 against
/// the axis (a boolean), 5 its type (0 pitch and turns, 1 turns and
/// height, 2 pitch and height; 3, a spiral, not seen), 6 pitch, 7 height,
/// 8 turns, 9 taper *(seen)*; the hand is not decoded (the import tries
/// both).
pub fn coil(
    cx: &Context,
    record: usize,
    sketch: Option<(i64, String)>,
    sketch_record: Option<usize>,
) -> Result<Translated, String> {
    let slots = cx.slots(record)?;
    let slot = |i: usize| slots.get(i).copied().flatten();
    let mut t = Translated::default();
    let operation = match cx.enumeration(slot(0), &ENUM_OPERATION) {
        Some(1) => "NewBodyFeatureOperation",
        Some(2) => "CutFeatureOperation",
        Some(3) => "JoinFeatureOperation",
        Some(4) => "IntersectFeatureOperation",
        other => return Err(format!("operation {other:?} is not known")),
    };
    let kind = slot(5).filter(|&r| cx.dc.is(r, &COIL_TYPE)).and_then(|r| {
        let mut rd = cx.dc.fields(r);
        rd.skip(2).ok()?;
        rd.u16().ok()
    });
    let (kind, used): (&str, [usize; 2]) = match kind {
        Some(0) => ("PitchAndRevolutionCoilType", [6, 8]),
        Some(1) => ("RevolutionAndHeightCoilType", [8, 7]),
        Some(2) => ("PitchAndHeightCoilType", [6, 7]),
        other => return Err(format!("coil type {other:?} is not known")),
    };
    let mut d = json!({"operation": operation, "coilType": kind});
    for (key, i) in [
        ("pitch", 6),
        ("height", 7),
        ("revolutions", 8),
        ("angle", 9),
    ] {
        // The sizes its type uses are its parameters; the others keep
        // values the coil does not use.
        let mut scratch = Vec::new();
        let made = if used.contains(&i) || i == 9 {
            &mut t.parameters
        } else {
            &mut scratch
        };
        if let Some(p) = cx.parameter(slot(i), made) {
            d[key] = p;
        }
    }
    if let Some((index, name)) = &sketch {
        d["profile"] = json!(profiles(cx, slot(1), *index, name, sketch_record));
    }
    match slot(2).and_then(|a| axis_reference_along(cx, a, sketch.as_ref(), sketch_record)) {
        Some((a, against)) => {
            d["axis"] = a;
            // Slot 3 turns the coil against the work axis' direction
            // *(seen: the coils of the older parts that came in, all
            // right-handed)*: the import tries that direction first.
            if let Some(reversed) = cx.boolean(slot(3)) {
                d["flip"] = json!(reversed != against);
            }
        }
        None => t
            .notes
            .push("its axis is found among the sketch's lines".to_owned()),
    }
    t.detail = d;
    Ok(t)
}
