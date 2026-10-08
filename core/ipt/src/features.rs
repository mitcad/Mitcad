// SPDX-License-Identifier: MIT
//! Features of the definitions segment as the dump IR's feature details
//! (SCHEMA.md §5.3), as far as their records are decoded.
//!
//! A feature record (`914d8790…`): the header, an i32, a u32 (the
//! feature's kind within the file), a reference list (kind 2) of its
//! properties by slot, and a u32. Properties are records of their own:
//! parameters, enumerations (`28be9a72…` operation, `297d6392…` extent:
//! eight bytes, u16 kind, u16 value), booleans (`284d8790…`: a name, a u32
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
        let mut r = self.dc.body(record);
        r.skip(10).ok()?;
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
        let mut r = self.dc.body(record?);
        r.skip(8).ok()?;
        r.references().ok()
    }
}

/// An extrusion: slots 0 operation, 1 profiles, 2 direction, 3 reversed,
/// 4 distance, 5 taper, 6 extent, 7 symmetric *(verified on the test
/// files' extrusions)*. Operations 1 new body, 2 cut, 3 join, 4
/// intersection; extents 1 distance, 2 symmetric, 5 through all, 7 up to
/// a face or work plane (slots 11, 13 and 18; not decoded: the import
/// looks for the face). The extrusion goes along the direction, against it
/// when reversed.
pub fn extrude(
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
    let extent = cx.enumeration(slot(6), &ENUM_EXTENT);
    let distance = cx.parameter(slot(4), &mut t.parameters);
    let taper = cx.parameter(slot(5), &mut t.parameters);
    let mut d = json!({"operation": operation, "isSolid": true});
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
        Some(5) => {
            d["extentType"] = json!("OneSideFeatureExtentType");
            d["extentOne"] = json!({"_type": "ThroughAllExtentDefinition"});
        }
        Some(7) => {
            d["extentType"] = json!("OneSideFeatureExtentType");
            d["extentOne"] = json!({"_type": "ToEntityExtentDefinition"});
            t.notes
                .push("the face or plane it extends to is not decoded".to_owned());
        }
        other => return Err(format!("extent {other:?} is not known")),
    }
    if let Some(taper) = taper {
        d["taperAngleOne"] = taper;
    }
    if let Some((index, name)) = sketch {
        d["profile"] = json!(profiles(cx, slot(1), index, &name, sketch_record));
    }
    if let Some(v) = cx.direction(slot(2)) {
        let reversed = cx.boolean(slot(3)) == Some(true);
        let v = if reversed { v.map(|x| -x) } else { v };
        t.raw = Some(json!({"extrude": {"direction_vector": v}}));
    }
    t.detail = d;
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
    let measured: Option<Vec<crate::profile::Measured>> = m
        .and_then(|m| {
            selections
                .iter()
                .map(|&s| cx.profiles.borrow_mut().measure(cx.dc, s, &m))
                .collect()
        })
        .filter(|m: &Vec<crate::profile::Measured>| !crate::profile::nested(m));
    let reference = |k: usize| {
        let mut p = json!({"kind": "profile", "sketch": name, "sketch_timeline_index": index});
        if let Some(m) = measured.as_ref().and_then(|v| v.get(k)) {
            p["area"] = json!(m.area);
            p["centroid"] = json!([m.centroid[0], m.centroid[1], 0.0]);
        }
        p
    };
    (0..selections.len().max(1)).map(reference).collect()
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
/// eight bytes, the edges, the radius, the selection and the tangent
/// chain), 11 its form (0 edge fillet). The edges are named by the file's
/// topological names, which are not decoded: the import finds them from
/// the history state.
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
        let mut r = cx.dc.body(set);
        let refs = (|| -> dc::Result<Vec<Option<usize>>> {
            r.skip(8)?;
            (0..4).map(|_| r.reference()).collect()
        })()
        .map_err(|e| format!("edge set {set}: {e}"))?;
        let radius = cx
            .parameter(refs[1], &mut t.parameters)
            .ok_or("an edge set without its radius")?;
        let mut s = json!({"_type": "ConstantRadiusFilletEdgeSet", "radius": radius});
        if let Some(chain) = cx.boolean(refs[3]) {
            s["isTangentChain"] = json!(chain);
        }
        edge_sets.push(s);
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

/// A work plane item: offset from another plane where the file says so
/// (`ConstructionPlaneOffsetDefinition`), else fixed where it is; its
/// geometry either way.
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
    }
    Ok(t)
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
/// along `v` (model coordinates, cm).
fn sketch_line_on(cx: &Context, sketch: usize, p: [f64; 3], v: [f64; 3]) -> Option<String> {
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
        let mut rd = cx.dc.body(r);
        rd.skip(16).ok()?;
        let ends = rd.references().ok()?;
        let a = model(crate::sketch::point(cx.dc, *ends.first()?)?);
        let b = model(crate::sketch::point(cx.dc, *ends.get(1)?)?);
        (off_axis(a) < 1e-6 && off_axis(b) < 1e-6).then(|| id.clone())
    })
}

/// A revolution: slots 0 operation, 1 profiles, 2 axis (a work axis), 3
/// extent (3 an angle), 4 the angle *(seen)*. An axis along an origin axis
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
    match cx.enumeration(slot(3), &ENUM_EXTENT) {
        Some(3) => {
            let angle = cx
                .parameter(slot(4), &mut t.parameters)
                .ok_or("its angle was not read")?;
            d["extentDefinition"] = json!({"_type": "AngleExtentDefinition", "angle": angle});
        }
        other => return Err(format!("revolution extent {other:?} is not known")),
    }
    if let Some((index, name)) = &sketch {
        d["profile"] = json!(profiles(cx, slot(1), *index, name, sketch_record));
    }
    if let Some((p, v)) = slot(2).and_then(|a| axis(cx.dc, a)) {
        let through_origin = {
            // The axis passes through the origin: p minus its part along v
            // is zero.
            let along = p[0] * v[0] + p[1] * v[1] + p[2] * v[2];
            (0..3).all(|i| (p[i] - along * v[i]).abs() < 1e-9)
        };
        let origin = [(0, "X"), (1, "Y"), (2, "Z")]
            .into_iter()
            .find(|&(i, _)| (v[i].abs() - 1.0).abs() < 1e-9)
            .map(|(_, n)| n);
        let line = sketch
            .as_ref()
            .zip(sketch_record)
            .and_then(|((index, _), s)| {
                let id = sketch_line_on(cx, s, p, v)?;
                Some(json!({"kind": "sketch_entity", "objectType": "SketchLine",
                        "sketch_timeline_index": index, "id": id}))
            });
        match (origin, line) {
            (_, Some(line)) => d["axis"] = line,
            (Some(name), None) if through_origin => {
                d["axis"] =
                    json!({"kind": "construction_axis", "origin": name, "timeline_index": null});
            }
            _ => t
                .notes
                .push("its axis is found among the sketch's lines".to_owned()),
        }
    }
    t.detail = d;
    Ok(t)
}
