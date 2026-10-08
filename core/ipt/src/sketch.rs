// SPDX-License-Identifier: MIT
//! Planar sketches of the definitions segment as the dump IR's sketch
//! detail (SCHEMA.md §5.3): the frame, points, lines, circles and arcs,
//! constraints and dimensions.
//!
//! - A sketch (`114d8790…`): the header, an i32, a u32, the list (kind 8)
//!   of its entities, constraints and dimensions, its transform and its
//!   normal (`40df52ce…`), two u32 and a list.
//! - Its transform (`184d8790…`): the header, eight
//!   bytes, an optional u32 `0x203`, a u16 mask of elements that are ±1 and
//!   a u16 mask of elements that are 0 or −1; the other elements follow as
//!   f64, row by row. The matrix maps sketch to model coordinates (cm):
//!   its columns are the sketch's x and y axes, its normal and its origin
//!   *(verified: the extrusions built on them give their history states)*.
//! - Entities: the header, eight bytes, u32 flags (0x40 construction,
//!   0x80000 centre line, 0x40000 projected from the model), the sketch;
//!   then a point's x and y (f64, cm); a line's list of its two points
//!   (start, end, then points that lie on it); a circle's list of its two end points (an arc, running
//!   counter-clockwise from the first to the second *(verified on every
//!   arc of the test files that meets a line tangentially)*; none for a
//!   full circle); a circle ends with its centre point, radius (f64) and
//!   a byte (1: full circle).
//! - Constraints and dimensions: the header, an i32, a reference, two
//!   maps (kind 6: reference and f64, reference and reference), the
//!   parameter (dimensions) and the entities.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::dc::{self, Definitions, Reader, type_id};
use crate::params::{Parameters, Quantity};

pub const TRANSFORM: [u8; 16] = type_id("184d8790d011f8d10008cabc0663dc09");
pub const NORMAL: [u8; 16] = type_id("40df52ced011d0d20008ccbc0663dc09");
pub const POINT: [u8; 16] = type_id("35df52ced011d0d20008ccbc0663dc09");
pub const LINE: [u8; 16] = type_id("3adf52ced011d0d20008ccbc0663dc09");
pub const CIRCLE: [u8; 16] = type_id("3bdf52ced011d0d20008ccbc0663dc09");

/// Constraints by record type: (type, the dump's constraint type, the
/// keys of its entities in order).
const CONSTRAINTS: [(&str, &str, &[&str]); 12] = [
    (
        "944d8790d011f8d10008cabc0663dc09",
        "CoincidentConstraint",
        &["point", "entity"],
    ),
    (
        "954d8790d011f8d10008cabc0663dc09",
        "ParallelConstraint",
        &["lineOne", "lineTwo"],
    ),
    (
        "964d8790d011f8d10008cabc0663dc09",
        "PerpendicularConstraint",
        &["lineOne", "lineTwo"],
    ),
    (
        "974d8790d011f8d10008cabc0663dc09",
        "TangentConstraint",
        &["curveOne", "curveTwo"],
    ),
    (
        "984d8790d011f8d10008cabc0663dc09",
        "HorizontalConstraint",
        &["line"],
    ),
    (
        "994d8790d011f8d10008cabc0663dc09",
        "VerticalConstraint",
        &["line"],
    ),
    (
        "e07b9a5ad1115de0800066b1e13554c7",
        "CollinearConstraint",
        &["lineOne", "lineTwo"],
    ),
    (
        "e0c281c6d11110e680006eb1e13554c7",
        "EqualConstraint",
        &["curveOne", "curveTwo"],
    ),
    (
        "d07d2c44d11189e680006fb1e13554c7",
        "EqualConstraint",
        &["curveOne", "curveTwo"],
    ),
    (
        "107cdd0dd1119ce680006fb1e13554c7",
        "SymmetryConstraint",
        &["entityOne", "entityTwo", "symmetryLine"],
    ),
    (
        "bf70e821d011d0d20008ccbc0663dc09",
        "MidPointConstraint",
        &["midPointCurve", "point"],
    ),
    // A circle's centre point: Mitcad's circles have it by definition.
    ("008c10e1d11102e680006db1e13554c7", "", &[]),
];

/// Dimensions by record type: (type, the dump's dimension type, where its
/// entities start among the u32 after the parameter, how many).
const DIMENSIONS: [(&str, &str, usize, usize); 6] = [
    // Aligned distance (points, a point and a line, two lines).
    (
        "58850511d211e39560000cb38932edb0",
        "SketchLinearDimension",
        1,
        2,
    ),
    (
        "00c0ac00d1115fe0800066b1e13554c7",
        "HorizontalDistance",
        0,
        2,
    ),
    ("40ff8336d1115fe0800066b1e13554c7", "VerticalDistance", 0, 2),
    (
        "e096df74d11169e0800066b1e13554c7",
        "SketchDiameterDimension",
        1,
        1,
    ),
    (
        "00b71b67d11168e0800066b1e13554c7",
        "SketchRadialDimension",
        1,
        1,
    ),
    (
        "100a0d59d111cae680006fb1e13554c7",
        "SketchAngularDimension",
        1,
        2,
    ),
];

/// The entity flags.
const CONSTRUCTION: u32 = 0x40;
const CENTER_LINE: u32 = 0x8_0000;
const PROJECTED: u32 = 0x4_0000;

/// A 4x4 matrix stored with masks (see the module docs).
pub fn matrix(dc: &Definitions, record: usize) -> Result<[[f64; 4]; 4], String> {
    let mut r = dc.body(record);
    let read = |r: &mut Reader| -> dc::Result<[[f64; 4]; 4]> {
        r.skip(8)?;
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
    };
    let m = read(&mut r).map_err(|e| format!("transform {record}: {e}"))?;
    if m.iter().flatten().any(|v| !v.is_finite()) || m[3] != [0.0, 0.0, 0.0, 1.0] {
        return Err(format!("transform {record} is not an affine transform"));
    }
    Ok(m)
}

/// A sketch's transform (sketch to model coordinates, cm).
pub fn sketch_matrix(dc: &Definitions, record: usize) -> Result<[[f64; 4]; 4], String> {
    let mut r = dc.body(record);
    let transform = (|| -> dc::Result<Option<usize>> {
        r.i32()?;
        r.u32()?;
        r.references()?;
        r.reference()
    })()
    .map_err(|e| format!("sketch {record}: {e}"))?;
    let transform = transform
        .filter(|&t| dc.is(t, &TRANSFORM))
        .ok_or("no transform")?;
    matrix(dc, transform)
}

/// A sketch point's position in its sketch (cm).
pub fn point(dc: &Definitions, record: usize) -> Option<[f64; 2]> {
    if !dc.is(record, &POINT) {
        return None;
    }
    let (_, mut r) = entity(dc, record).ok()?;
    let p = [r.f64().ok()?, r.f64().ok()?];
    p.iter().all(|v| v.is_finite()).then_some(p)
}

/// A sketch entity's flags and the rest of it.
fn entity<'a>(dc: &'a Definitions, record: usize) -> dc::Result<(u32, Reader<'a>)> {
    let mut r = dc.body(record);
    r.skip(8)?;
    let flags = r.u32()?;
    r.u32()?;
    Ok((flags, r))
}

/// A constraint's or dimension's parameter and the u32 after it.
fn constraint_body(dc: &Definitions, record: usize) -> dc::Result<(Option<usize>, Vec<u32>)> {
    let mut r = dc.body(record);
    r.i32()?;
    r.u32()?;
    for pair in [12, 8] {
        let at = r.pos;
        let kind = r.u16()?;
        let tag = r.u16()?;
        if kind != 6 || tag != 0x3000 {
            return Err(dc::DcError(format!("no map at {at}")));
        }
        let n = r.u32()? as usize;
        if n > 0 {
            r.skip(8)?;
            r.skip(pair * n)?;
        }
    }
    let parameter = r.reference()?;
    let mut rest = Vec::new();
    while r.left() >= 4 {
        rest.push(r.u32()?);
    }
    Ok((parameter, rest))
}

/// A decoded sketch.
#[derive(Clone, Debug)]
pub struct Sketch {
    /// The dump's sketch detail.
    pub detail: Value,
    /// Entity records → the dump's ids (`p3`, `c5`).
    pub ids: HashMap<usize, String>,
    /// Parameter records its dimensions use.
    pub parameters: Vec<usize>,
    /// What was left out, with why.
    pub dropped: Vec<String>,
}

/// Reads a sketch. `params` gives its dimensions' parameters; a
/// dimension whose parameter is not in the part's table is driven.
pub fn sketch(
    dc: &Definitions,
    record: usize,
    name: &str,
    params: &Parameters,
) -> Result<Sketch, String> {
    let mut r = dc.body(record);
    let (entities, transform) = (|| -> dc::Result<(Vec<usize>, Option<usize>)> {
        r.i32()?;
        r.u32()?;
        let entities = r.references()?;
        Ok((entities, r.reference()?))
    })()
    .map_err(|e| format!("sketch {record}: {e}"))?;
    let transform = transform
        .filter(|&t| dc.is(t, &TRANSFORM))
        .ok_or("no transform")?;
    let m = matrix(dc, transform)?;
    let column = |c: usize| [m[0][c], m[1][c], m[2][c]];
    let (x, y, z, origin) = (column(0), column(1), column(2), column(3));
    let unit = |v: [f64; 3]| ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() - 1.0).abs() < 1e-6;
    if !(unit(x) && unit(y) && unit(z)) {
        return Err("its transform is not a rotation".to_owned());
    }
    let mut ids: HashMap<usize, String> = HashMap::new();
    let mut points = Vec::new();
    let mut at: HashMap<usize, [f64; 2]> = HashMap::new();
    let mut dropped = Vec::new();
    let tname = |e: usize| dc.type_of(e).map_or_else(|| "?".to_owned(), dc::type_text);
    for &e in &entities {
        if !dc.is(e, &POINT) {
            continue;
        }
        match entity(dc, e).and_then(|(flags, mut r)| Ok((flags, r.f64()?, r.f64()?))) {
            Ok((flags, px, py)) if px.is_finite() && py.is_finite() => {
                let id = format!("p{}", points.len());
                let mut p = json!({"id": id, "xyz": [px, py, 0.0]});
                if flags & PROJECTED != 0 {
                    p["isReference"] = json!(true);
                }
                at.insert(e, [px, py]);
                ids.insert(e, id);
                points.push(p);
            }
            Ok(_) => dropped.push(format!("point {e}: not a finite position")),
            Err(err) => dropped.push(format!("point {e}: {err}")),
        }
    }
    let mut curves = Vec::new();
    let point_id = |p: Option<&usize>, ids: &HashMap<usize, String>| -> Option<String> {
        p.and_then(|p| ids.get(p)).cloned()
    };
    for &e in &entities {
        let is_line = dc.is(e, &LINE);
        if !is_line && !dc.is(e, &CIRCLE) {
            continue;
        }
        let id = format!("c{}", curves.len());
        let read = (|| -> Result<Value, String> {
            let (flags, mut r) = entity(dc, e).map_err(|err| err.to_string())?;
            let ends = r.list().map_err(|err| err.to_string())?.1;
            let ends: Vec<Option<usize>> = ends.into_iter().map(dc::reference).collect();
            let mut c = if is_line {
                // Its start and end, then points on it (the line's
                // coincidences).
                let (Some(&Some(s)), Some(&Some(t))) = (ends.first(), ends.get(1)) else {
                    return Err(format!("a line with {} points", ends.len()));
                };
                let (Some(a), Some(b)) = (at.get(&s), at.get(&t)) else {
                    return Err("an end point was not read".to_owned());
                };
                json!({"id": id, "type": "SketchLine",
                    "startSketchPoint": point_id(Some(&s), &ids),
                    "endSketchPoint": point_id(Some(&t), &ids),
                    "geometry": {"type": "Line3D", "startPoint": [a[0], a[1], 0.0],
                                 "endPoint": [b[0], b[1], 0.0]}})
            } else {
                let bytes = dc.bytes(e);
                let n = bytes.len();
                if n < 13 {
                    return Err("a circle too short".to_owned());
                }
                let mut tail = Reader::at(bytes, n - 13);
                let center = tail.reference().map_err(|err| err.to_string())?;
                let radius = tail.f64().map_err(|err| err.to_string())?;
                let full = tail.u8().map_err(|err| err.to_string())? == 1;
                let center = center.ok_or("a circle without its centre")?;
                let c_at = *at.get(&center).ok_or("its centre was not read")?;
                if !(radius.is_finite() && radius > 0.0) {
                    return Err(format!("radius {radius}"));
                }
                let g_center = [c_at[0], c_at[1], 0.0];
                if full {
                    json!({"id": id, "type": "SketchCircle",
                        "centerSketchPoint": point_id(Some(&center), &ids), "radius": radius,
                        "geometry": {"type": "Circle3D", "center": g_center,
                                     "normal": [0.0, 0.0, 1.0], "radius": radius}})
                } else {
                    let [Some(s), Some(t)] = ends[..] else {
                        return Err(format!("an arc with {} end points", ends.len()));
                    };
                    let (Some(a), Some(b)) = (at.get(&s), at.get(&t)) else {
                        return Err("an end point was not read".to_owned());
                    };
                    let angle = |p: &[f64; 2]| (p[1] - c_at[1]).atan2(p[0] - c_at[0]);
                    let (a0, mut a1) = (angle(a), angle(b));
                    while a1 <= a0 {
                        a1 += std::f64::consts::TAU;
                    }
                    json!({"id": id, "type": "SketchArc",
                        "centerSketchPoint": point_id(Some(&center), &ids),
                        "startSketchPoint": point_id(Some(&s), &ids),
                        "endSketchPoint": point_id(Some(&t), &ids), "radius": radius,
                        "geometry": {"type": "Arc3D", "center": g_center,
                            "normal": [0.0, 0.0, 1.0], "referenceVector": [1.0, 0.0, 0.0],
                            "radius": radius, "startAngle": a0, "endAngle": a1}})
                }
            };
            // A centre line bounds no profile in the file (Mitcad's centre
            // lines do): construction geometry.
            if flags & (CONSTRUCTION | CENTER_LINE) != 0 {
                c["isConstruction"] = json!(true);
            }
            if flags & PROJECTED != 0 {
                c["isReference"] = json!(true);
            }
            Ok(c)
        })();
        match read {
            Ok(c) => {
                ids.insert(e, id);
                curves.push(c);
            }
            Err(err) => dropped.push(format!(
                "{} {e}: {err}",
                if is_line { "line" } else { "circle" }
            )),
        }
    }
    let mut constraints = Vec::new();
    let mut dimensions = Vec::new();
    let mut used = Vec::new();
    for &e in &entities {
        if dc.is(e, &POINT) || dc.is(e, &LINE) || dc.is(e, &CIRCLE) || dc.is(e, &TRANSFORM) {
            continue;
        }
        let t = tname(e);
        let local = |v: u32| dc::reference(v).and_then(|r| ids.get(&r).cloned());
        if let Some((_, kind, keys)) = CONSTRAINTS.iter().find(|(c, _, _)| *c == t) {
            if kind.is_empty() {
                continue;
            }
            match constraint_body(dc, e) {
                Ok((_, rest)) if rest.len() >= keys.len() => {
                    let refs: Option<serde_json::Map<String, Value>> = keys
                        .iter()
                        .zip(&rest)
                        .map(|(k, &v)| Some(((*k).to_owned(), json!(local(v)?))))
                        .collect();
                    match refs {
                        Some(refs) => constraints.push(json!({
                            "id": format!("k{}", constraints.len()),
                            "type": kind, "refs": refs})),
                        None => dropped.push(format!("{kind} {e}: an entity that was not read")),
                    }
                }
                Ok(_) => dropped.push(format!("{kind} {e}: too few entities")),
                Err(err) => dropped.push(format!("{kind} {e}: {err}")),
            }
            continue;
        }
        if let Some(&(_, kind, first, count)) = DIMENSIONS.iter().find(|(c, _, _, _)| *c == t) {
            let dim = (|| -> Result<Value, String> {
                let (parameter, rest) = constraint_body(dc, e).map_err(|err| err.to_string())?;
                let entities: Vec<u32> = rest
                    .get(first..first + count)
                    .ok_or("too few entities")?
                    .to_vec();
                let local: Vec<String> = entities
                    .iter()
                    .map(|&v| local(v).ok_or("an entity that was not read"))
                    .collect::<Result<_, _>>()?;
                // The horizontal and vertical distances name their
                // parameter after their points.
                let parameter = match kind {
                    "HorizontalDistance" | "VerticalDistance" => {
                        rest.get(2).and_then(|&v| dc::reference(v)).or(parameter)
                    }
                    _ => parameter,
                };
                let p = parameter
                    .and_then(|r| params.by_record(r))
                    .ok_or("its parameter was not read")?;
                let driving = p.in_table;
                if driving {
                    used.push(p.record);
                }
                let points = |i: usize| local[i].starts_with('p');
                let (dtype, refs, orientation) = match kind {
                    "SketchLinearDimension" if !points(0) && !points(1) => (
                        "SketchOffsetDimension",
                        json!({"line": local[0], "entityTwo": local[1]}),
                        None,
                    ),
                    "SketchLinearDimension" => (
                        kind,
                        json!({"entityOne": local[0], "entityTwo": local[1]}),
                        None,
                    ),
                    "HorizontalDistance" | "VerticalDistance" => (
                        "SketchLinearDimension",
                        json!({"entityOne": local[0], "entityTwo": local[1]}),
                        Some(if kind == "HorizontalDistance" {
                            "HorizontalDimensionOrientation"
                        } else {
                            "VerticalDimensionOrientation"
                        }),
                    ),
                    "SketchAngularDimension" => (
                        kind,
                        json!({"lineOne": local[0], "lineTwo": local[1]}),
                        None,
                    ),
                    _ => (kind, json!({"entity": local[0]}), None),
                };
                let angle = p.quantity == Ok(Quantity::Angle);
                let mut d = json!({
                    "id": format!("d{}", dimensions.len()),
                    "type": dtype,
                    "isDriving": driving,
                    "value": p.value,
                    "refs": refs,
                    "parameter": {"kind": "parameter", "name": p.name, "value": p.value,
                                  "unit": if angle { "deg" } else { "" }},
                });
                if let Some(o) = orientation {
                    d["props"] = json!({"orientation": o});
                }
                Ok(d)
            })();
            match dim {
                Ok(d) => dimensions.push(d),
                Err(err) => dropped.push(format!("{kind} {e}: {err}")),
            }
            continue;
        }
        dropped.push(format!("record {e} of type {t}: not decoded"));
    }
    // What was left out here makes the sketch partial: the import leaves
    // out constraints of types it does not know and says so.
    for d in &dropped {
        constraints.push(json!({
            "id": format!("k{}", constraints.len()),
            "type": format!("NotDecoded({d})"),
        }));
    }
    // A sketch on an origin plane names it.
    let axis = |v: [f64; 3], i: usize| (v[i].abs() - 1.0).abs() < 1e-9;
    let origin_plane = [(2, "XY"), (1, "XZ"), (0, "YZ")]
        .into_iter()
        .find(|&(i, _)| axis(z, i) && origin[i].abs() < 1e-9)
        .map(|(_, plane)| plane);
    let mut detail = json!({
        "name": name,
        "model_frame": {
            "origin": origin, "x_axis": x, "y_axis": y, "z_axis": z,
            "sketch_to_model": m,
        },
        "origin": origin,
        "xDirection": x,
        "yDirection": y,
        "points": points,
        "curves": curves,
        "constraints": constraints,
        "dimensions": dimensions,
        "counts": {"points": points.len(), "curves": curves.len(),
                   "constraints": constraints.len(), "dimensions": dimensions.len()},
    });
    if let Some(plane) = origin_plane {
        detail["referencePlane"] = json!({"kind": "construction_plane", "name": format!("{plane} Plane"),
                                          "origin": plane, "timeline_index": null});
    }
    Ok(Sketch {
        detail,
        ids,
        parameters: used,
        dropped,
    })
}
