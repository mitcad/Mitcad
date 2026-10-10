// SPDX-License-Identifier: MIT
//! Planar sketches of the definitions segment as the dump IR's sketch
//! detail (SCHEMA.md §5.3): the frame, points, lines, circles and arcs,
//! constraints and dimensions.
//!
//! - A sketch (`114d8790…`): the header, an i32, a u32, the list (kind 8)
//!   of its entities, constraints and dimensions, its transform and its
//!   normal (`40df52ce…`), two u32 and a list.
//! - Its transform (`184d8790…`): the header, the prefix (`dc`), an
//!   optional u32 `0x203`, a u16 mask of elements that are ±1 and
//!   a u16 mask of elements that are 0 or −1; the other elements follow as
//!   f64, row by row. The matrix maps sketch to model coordinates (cm):
//!   its columns are the sketch's x and y axes, its normal and its origin
//!   *(verified: the extrusions built on them give their history states)*.
//! - Entities: the header, the prefix (`dc`), u32 flags (0x40 construction,
//!   0x80000 centre line, 0x40000 projected from the model), the sketch;
//!   then a point's x and y (f64, cm); a line's list of its two points
//!   (start, end, then points that lie on it); a circle's list of its two
//!   end points (an arc, running counter-clockwise from the first to the
//!   second *(verified on every arc of the test files that meets a line
//!   tangentially)*, then points that lie on it; none for a full circle);
//!   a circle ends with its centre point, radius (f64) and a byte (1:
//!   full circle).
//! - Polygons and patterns of sketch entities: `groups`.
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
/// A rectangle (`0ead987a…`), its frame (`08db93dc…`: its corner, width
/// and height) and its anchor (`5f33ec8f…`: a corner, the sides and the
/// frame) *(seen)*.
pub const RECTANGLE: [u8; 16] = type_id("0ead987ab9495ceefc186787a2a20643");
pub const RECTANGLE_FRAME: [u8; 16] = type_id("08db93dcc149f26ff229ecb1804d9784");
pub const RECTANGLE_ANCHOR: [u8; 16] = type_id("5f33ec8f6c4fb6be9fcc71963ad1b9df");
/// A straight segment between two points (`46737b31…`: an entity whose
/// list holds its two points and no geometry follows; one from a point to
/// itself is nothing) *(seen)*.
pub const SEGMENT: [u8; 16] = type_id("46737b31d3117c7a60001cb3d1c1fbb0");
/// A driven angle of three points (`845c3bbf…`).
pub const POINTS_ANGLE: [u8; 16] = type_id("845c3bbfd2112ae960004bb38932edb0");
/// An ellipse or elliptical arc *(seen)*.
pub const ELLIPSE: [u8; 16] = type_id("60d40745d111bee680006fb1e13554c7");

/// Constraints by record type: (type, the dump's constraint type, the
/// keys of its entities in order).
const CONSTRAINTS: [(&str, &str, &[&str]); 14] = [
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
    // Two points on a horizontal or vertical line (by their geometry
    // *(verified)*).
    (
        "c0a3558fd1115ee0800066b1e13554c7",
        "HorizontalPointsConstraint",
        &["pointOne", "pointTwo"],
    ),
    (
        "5052da64d1115ee0800066b1e13554c7",
        "VerticalPointsConstraint",
        &["pointOne", "pointTwo"],
    ),
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

/// A fixed point or curve (`b07b170f…`): one entity after the parameter
/// *(seen)*.
pub const FIX: [u8; 16] = type_id("b07b170fd11196e680006fb1e13554c7");

/// The entity flags.
const CONSTRUCTION: u32 = 0x40;
const CENTER_LINE: u32 = 0x8_0000;
const PROJECTED: u32 = 0x4_0000;

/// A 4x4 matrix stored with masks (see the module docs).
pub fn matrix(dc: &Definitions, record: usize) -> Result<[[f64; 4]; 4], String> {
    let mut r = dc.fields(record);
    let read = |r: &mut Reader| -> dc::Result<[[f64; 4]; 4]> {
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

/// Two dump curves with the same geometry: lines with the same ends
/// (either way round), circles, or arcs over the same angles.
pub fn same_curve(a: &Value, b: &Value) -> bool {
    let (ga, gb) = (&a["geometry"], &b["geometry"]);
    let p = |v: &Value| -> Option<[f64; 2]> { Some([v[0].as_f64()?, v[1].as_f64()?]) };
    let near = |x: Option<[f64; 2]>, y: Option<[f64; 2]>| {
        x.zip(y)
            .is_some_and(|(x, y)| (x[0] - y[0]).hypot(x[1] - y[1]) < 1e-9)
    };
    let num = |v: &Value, w: &Value| {
        v.as_f64()
            .zip(w.as_f64())
            .is_some_and(|(v, w)| (v - w).abs() < 1e-9)
    };
    match (ga["type"].as_str(), gb["type"].as_str()) {
        (Some("Line3D"), Some("Line3D")) => {
            let (s, e) = (p(&ga["startPoint"]), p(&ga["endPoint"]));
            let (s2, e2) = (p(&gb["startPoint"]), p(&gb["endPoint"]));
            (near(s, s2) && near(e, e2)) || (near(s, e2) && near(e, s2))
        }
        (Some("Circle3D"), Some("Circle3D")) => {
            near(p(&ga["center"]), p(&gb["center"])) && num(&ga["radius"], &gb["radius"])
        }
        (Some("Arc3D"), Some("Arc3D")) => {
            let tau = std::f64::consts::TAU;
            let angle = |v: &Value, w: &Value| {
                v.as_f64().zip(w.as_f64()).is_some_and(|(v, w)| {
                    let d = (v - w).rem_euclid(tau);
                    d < 1e-9 || tau - d < 1e-9
                })
            };
            near(p(&ga["center"]), p(&gb["center"]))
                && num(&ga["radius"], &gb["radius"])
                && angle(&ga["startAngle"], &gb["startAngle"])
                && angle(&ga["endAngle"], &gb["endAngle"])
        }
        _ => false,
    }
}

/// A circle's or arc's centre (in its sketch, cm) and radius.
pub fn circle(dc: &Definitions, record: usize) -> Option<([f64; 2], f64)> {
    if !dc.is(record, &CIRCLE) {
        return None;
    }
    let bytes = dc.bytes(record);
    let mut tail = Reader::at(bytes, bytes.len().checked_sub(13)?);
    let centre = point(dc, tail.reference().ok()??)?;
    let radius = tail.f64().ok()?;
    radius.is_finite().then_some((centre, radius))
}

/// A spline (`d42f37f9…`): after the entity's list of its two end points
/// and other lists its NURBS data, twice: u32 degree, f64 tolerance, the
/// knots as an array (u32 count, u32 capacity, u32 8, the f64), eight
/// bytes, the control points as an array (u32 8, u32 count, u32 capacity,
/// u32 8, two f64 each, cm) *(seen)*. Found by its form and
/// checked: the knots do not decrease and are as many as the control
/// points and the degree and one, and the ends of the curve (clamped) are
/// its end points.
pub const SPLINE: [u8; 16] = type_id("d42f37f9d11115d3000847b00524dc09");
/// A spline's control polygon (`9d858c5d…`, in the sketch's list as a
/// constraint: after the parameter its spline, the control points and the
/// lines between them) *(seen: the points lie on the spline's poles, one
/// each)*. A spline drawn so comes in with these points as its control
/// points; a projected one's polygon holds nothing.
pub const CONTROL_POLYGON: [u8; 16] = type_id("9d858c5d534666ea4bf6daa182a838f2");

/// A spline's degree, knots and control points (see [`SPLINE`]), its end
/// points at `start` and `end`.
fn spline_nurbs(
    bytes: &[u8],
    start: [f64; 2],
    end: [f64; 2],
) -> Option<(u32, Vec<f64>, Vec<[f64; 2]>)> {
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let f64_at = |at: usize| -> Option<f64> {
        Some(f64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
    };
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9;
    let count = |at: usize| u32_at(at).map(|v| v as usize);
    for at in 0..bytes.len().saturating_sub(24) {
        let Some(degree) = u32_at(at).filter(|d| (1..=9).contains(d)) else {
            continue;
        };
        // The tolerance.
        if !f64_at(at + 4).is_some_and(|t| t > 0.0 && t < 1e-3) {
            continue;
        }
        let Some(nk) = count(at + 12) else {
            continue;
        };
        if nk < 2 * (degree as usize + 1)
            || nk > 4096
            || !count(at + 16).is_some_and(|c| c >= nk)
            || count(at + 20) != Some(8)
        {
            continue;
        }
        let k0 = at + 24;
        let knots: Option<Vec<f64>> = (0..nk).map(|i| f64_at(k0 + 8 * i)).collect();
        let Some(knots) = knots.filter(|k| k.windows(2).all(|w| w[0] <= w[1])) else {
            continue;
        };
        let p0 = k0 + 8 * nk + 8;
        let Some(np) = count(p0 + 4) else {
            continue;
        };
        if np + degree as usize + 1 != nk
            || count(p0) != Some(8)
            || !count(p0 + 8).is_some_and(|c| c >= np)
            || count(p0 + 12) != Some(8)
        {
            continue;
        }
        let poles: Option<Vec<[f64; 2]>> = (0..np)
            .map(|i| Some([f64_at(p0 + 16 + 16 * i)?, f64_at(p0 + 24 + 16 * i)?]))
            .collect();
        let Some(poles) = poles else {
            continue;
        };
        if poles.iter().flatten().all(|v| v.is_finite())
            && near(poles[0], start)
            && near(poles[np - 1], end)
        {
            return Some((degree, knots, poles));
        }
    }
    None
}

/// A circle's or arc's centre point record.
pub fn centre_point(dc: &Definitions, record: usize) -> Option<usize> {
    if !dc.is(record, &CIRCLE) {
        return None;
    }
    let bytes = dc.bytes(record);
    Reader::at(bytes, bytes.len().checked_sub(13)?)
        .reference()
        .ok()?
}

/// A sketch entity's flags and the rest of it.
pub fn entity<'a>(dc: &'a Definitions, record: usize) -> dc::Result<(u32, Reader<'a>)> {
    let mut r = dc.fields(record);
    let flags = r.u32()?;
    r.u32()?;
    Ok((flags, r))
}

/// A constraint's or dimension's parameter and the u32 after it.
pub fn constraint_body(dc: &Definitions, record: usize) -> dc::Result<(Option<usize>, Vec<u32>)> {
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
    /// Curves construction geometry by the flag 0x40 alone (not centre
    /// lines): those that bound a profile a feature selected are not
    /// (`design`).
    pub flagged: Vec<String>,
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
    // The control polygons of control point splines (`CONTROL_POLYGON`):
    // spline → its points, in the record's order.
    let mut polygons: HashMap<usize, Vec<usize>> = HashMap::new();
    for &e in entities.iter().filter(|&&e| dc.is(e, &CONTROL_POLYGON)) {
        let Ok((_, rest)) = constraint_body(dc, e) else {
            continue;
        };
        let refs: Vec<usize> = rest.iter().filter_map(|&v| dc::reference(v)).collect();
        let splines: Vec<usize> = refs
            .iter()
            .copied()
            .filter(|&r| dc.is(r, &SPLINE))
            .collect();
        if let [spline] = splines[..] {
            polygons.insert(
                spline,
                refs.into_iter().filter(|&r| dc.is(r, &POINT)).collect(),
            );
        }
    }
    // The splines that came in with their control points.
    let mut controlled: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut curves = Vec::new();
    let mut flagged = Vec::new();
    let mut degenerate: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let point_id = |p: Option<&usize>, ids: &HashMap<usize, String>| -> Option<String> {
        p.and_then(|p| ids.get(p)).cloned()
    };
    for &e in &entities {
        let is_segment = dc.is(e, &SEGMENT);
        let is_line = dc.is(e, &LINE) || is_segment;
        let is_ellipse = dc.is(e, &ELLIPSE);
        let is_spline = dc.is(e, &SPLINE);
        if !is_line && !is_ellipse && !is_spline && !dc.is(e, &CIRCLE) {
            continue;
        }
        // A segment from a point to itself is nothing, and so is a line
        // whose ends lie on each other (it bounds nothing; what constrains
        // it holds nothing either) *(seen)*.
        let ends = entity(dc, e)
            .ok()
            .and_then(|(_, mut r)| r.list().ok())
            .map(|(_, ends)| ends);
        if let Some(ends) = &ends
            && is_line
            && ends.len() >= 2
        {
            let (s, t) = (dc::reference(ends[0]), dc::reference(ends[1]));
            let at_s = s.and_then(|s| at.get(&s));
            let at_t = t.and_then(|t| at.get(&t));
            let same = s == t
                || at_s
                    .zip(at_t)
                    .is_some_and(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-12);
            if same {
                degenerate.insert(e);
                continue;
            }
        }
        let id = format!("c{}", curves.len());
        let read = (|| -> Result<(Value, bool), String> {
            let (flags, mut r) = entity(dc, e).map_err(|err| err.to_string())?;
            let ends = r.list().map_err(|err| err.to_string())?.1;
            let ends: Vec<Option<usize>> = ends.into_iter().map(dc::reference).collect();
            let mut c = if is_spline {
                // A closed spline has one end point, where it starts and
                // ends *(seen)*.
                let (s, t) = match ends[..] {
                    [Some(s), Some(t), ..] => (s, t),
                    [Some(s)] => (s, s),
                    _ => return Err(format!("a spline with {} end points", ends.len())),
                };
                let (Some(a), Some(b)) = (at.get(&s), at.get(&t)) else {
                    return Err("an end point was not read".to_owned());
                };
                let (degree, knots, poles) =
                    spline_nurbs(dc.bytes(e), *a, *b).ok_or("its NURBS data was not read")?;
                // A spline drawn by its control points: each pole is a
                // point of its control polygon.
                let control: Option<Vec<String>> = polygons
                    .get(&e)
                    .filter(|_| flags & PROJECTED == 0)
                    .and_then(|polygon| {
                        poles
                            .iter()
                            .map(|q| {
                                polygon
                                    .iter()
                                    .find(|p| {
                                        at.get(p).is_some_and(|a| {
                                            (a[0] - q[0]).hypot(a[1] - q[1]) < 1e-9
                                        })
                                    })
                                    .and_then(|p| ids.get(p).cloned())
                            })
                            .collect()
                    })
                    .filter(|c: &Vec<String>| {
                        // A closed spline's first and last poles are its
                        // one end point.
                        let open = match &c[..] {
                            [first, .., last] if s == t && first == last => &c[..c.len() - 1],
                            all => all,
                        };
                        let mut d = open.to_vec();
                        d.sort();
                        d.dedup();
                        d.len() == open.len()
                    });
                let poles: Vec<[f64; 3]> = poles.iter().map(|p| [p[0], p[1], 0.0]).collect();
                let mut c = json!({"id": id, "type": "SketchFixedSpline",
                    "startSketchPoint": point_id(Some(&s), &ids),
                    "endSketchPoint": point_id(Some(&t), &ids),
                    "geometry": {"type": "NurbsCurve3D", "nurbs": {"degree": degree,
                        "controlPoints": poles, "knots": knots,
                        "isPeriodic": false, "isRational": false}}});
                if let Some(control) = control {
                    c["type"] = json!("SketchControlPointSpline");
                    c["controlPoints"] = json!(control);
                    controlled.insert(e);
                }
                c
            } else if is_line {
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
            } else if is_ellipse {
                // At its end the centre point, the major axis's direction
                // (two f64), the major and minor radii and a byte (1:
                // full); an elliptical arc's ends as an arc's.
                let bytes = dc.bytes(e);
                let n = bytes.len();
                if n < 37 {
                    return Err("an ellipse too short".to_owned());
                }
                let mut tail = Reader::at(bytes, n - 37);
                let center = tail.reference().map_err(|err| err.to_string())?;
                let mut v = [0.0; 4];
                for x in &mut v {
                    *x = tail.f64().map_err(|err| err.to_string())?;
                }
                let full = tail.u8().map_err(|err| err.to_string())? == 1;
                let [dx, dy, major, minor] = v;
                let center = center.ok_or("an ellipse without its centre")?;
                let c_at = *at.get(&center).ok_or("its centre was not read")?;
                if !(major.is_finite() && minor.is_finite() && minor > 0.0 && major >= minor)
                    || ((dx * dx + dy * dy).sqrt() - 1.0).abs() > 1e-6
                {
                    return Err(format!("radii {major}, {minor}"));
                }
                let mut c = json!({"id": id, "type": "SketchEllipse",
                    "centerSketchPoint": point_id(Some(&center), &ids),
                    "geometry": {"type": "Ellipse3D", "center": [c_at[0], c_at[1], 0.0],
                                 "normal": [0.0, 0.0, 1.0], "majorAxis": [dx, dy, 0.0],
                                 "majorRadius": major, "minorRadius": minor}});
                if !full {
                    let [Some(s), Some(t), ..] = ends[..] else {
                        return Err(format!("an elliptical arc with {} end points", ends.len()));
                    };
                    c["type"] = json!("SketchEllipticalArc");
                    c["geometry"]["type"] = json!("EllipticalArc3D");
                    c["startSketchPoint"] = json!(point_id(Some(&s), &ids));
                    c["endSketchPoint"] = json!(point_id(Some(&t), &ids));
                }
                c
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
                    // Its two ends, then points that lie on it (as on a
                    // line).
                    let [Some(s), Some(t), ref on @ ..] = ends[..] else {
                        return Err(format!("an arc with {} end points", ends.len()));
                    };
                    for p in on {
                        let q = (*p)
                            .and_then(|p| at.get(&p))
                            .ok_or("a point on it was not read")?;
                        let off = (q[0] - c_at[0]).hypot(q[1] - c_at[1]) - radius;
                        if off.abs() > 1e-6 * radius.max(1.0) {
                            return Err("a point listed with it is not on it".to_owned());
                        }
                    }
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
            Ok((c, flags & CONSTRUCTION != 0 && flags & CENTER_LINE == 0))
        })();
        match read {
            Ok((c, by_flag)) => {
                if by_flag {
                    flagged.push(id.clone());
                }
                ids.insert(e, id);
                curves.push(c);
            }
            Err(err) => dropped.push(format!(
                "{} {e}: {err}",
                if is_line {
                    "line"
                } else if is_ellipse {
                    "ellipse"
                } else if is_spline {
                    "spline"
                } else {
                    "circle"
                }
            )),
        }
    }
    // A curve drawn twice bounds no region of its own (its copy would
    // split none and break the loops): the later copy is construction
    // geometry *(seen)*.
    for i in 0..curves.len() {
        if curves[i]["isConstruction"] != true
            && curves[..i]
                .iter()
                .any(|c| c["isConstruction"] != true && same_curve(c, &curves[i]))
        {
            curves[i]["isConstruction"] = json!(true);
        }
    }
    let mut constraints = Vec::new();
    let mut dimensions = Vec::new();
    let mut used = Vec::new();
    // Polygons and patterns of sketch entities.
    let (grouped, done, group_dropped) = crate::groups::decode(dc, &entities, &ids, params);
    for mut c in grouped {
        c["id"] = json!(format!("k{}", constraints.len()));
        constraints.push(c);
    }
    dropped.extend(group_dropped);
    // A curve's end points (the first two of its list).
    let ends_of = |c: usize| -> Vec<usize> {
        if !(dc.is(c, &LINE)
            || dc.is(c, &CIRCLE)
            || dc.is(c, &ELLIPSE)
            || dc.is(c, &SEGMENT)
            || dc.is(c, &SPLINE))
        {
            return Vec::new();
        }
        entity(dc, c)
            .ok()
            .and_then(|(_, mut r)| r.references().ok())
            .map(|mut v| {
                v.truncate(2);
                v
            })
            .unwrap_or_default()
    };
    // Geometry projected from the model and the end and centre points of
    // projected curves: Mitcad keeps them where they are.
    let projected = |r: usize| entity(dc, r).is_ok_and(|(f, _)| f & PROJECTED != 0);
    let mut held_points: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for &c in &entities {
        if projected(c) && !dc.is(c, &POINT) {
            held_points.extend(ends_of(c));
            held_points.extend(centre_point(dc, c));
        }
    }
    let held = |r: usize| projected(r) || held_points.contains(&r);
    let mut offsets = Vec::new();
    let mut fixed: Vec<String> = Vec::new();
    for &e in &entities {
        if dc.is(e, &POINT)
            || dc.is(e, &LINE)
            || dc.is(e, &CIRCLE)
            || dc.is(e, &ELLIPSE)
            || dc.is(e, &SEGMENT)
            || dc.is(e, &SPLINE)
            || dc.is(e, &TRANSFORM)
            || done.contains(&e)
        {
            continue;
        }
        // What constrains a line of no length holds nothing.
        if !degenerate.is_empty()
            && constraint_body(dc, e).is_ok_and(|(_, rest)| {
                rest.iter()
                    .any(|&v| dc::reference(v).is_some_and(|r| degenerate.contains(&r)))
            })
        {
            continue;
        }
        // Offsets, once the dimensions are read.
        if dc.is(e, &crate::groups::OFFSET) {
            offsets.push(e);
            continue;
        }
        // A fixed point or curve: its one entity after the parameter.
        if dc.is(e, &FIX) {
            match constraint_body(dc, e)
                .ok()
                .and_then(|(_, rest)| dc::reference(*rest.first()?))
                .and_then(|r| ids.get(&r).cloned())
            {
                Some(id) => fixed.push(id),
                None => dropped.push(format!("fix {e}: an entity that was not read")),
            }
            continue;
        }
        let t = tname(e);
        let local = |v: u32| dc::reference(v).and_then(|r| ids.get(&r).cloned());
        // A rectangle: its four sides in order (then its corners), at right
        // angles; its frame and anchor records keep nothing the sides do
        // not *(seen)*.
        if dc.is(e, &RECTANGLE) {
            let sides: Option<Vec<String>> = constraint_body(dc, e)
                .ok()
                .and_then(|(_, rest)| rest.get(..4).map(|s| s.iter().map(|&v| local(v)).collect()))
                .flatten();
            match sides {
                Some(sides) => {
                    for pair in sides.windows(2) {
                        constraints.push(json!({
                            "id": format!("k{}", constraints.len()),
                            "type": "PerpendicularConstraint",
                            "refs": {"lineOne": pair[0], "lineTwo": pair[1]}}));
                    }
                }
                None => dropped.push(format!("rectangle {e}: a side that was not read")),
            }
            continue;
        }
        if dc.is(e, &RECTANGLE_FRAME) || dc.is(e, &RECTANGLE_ANCHOR) {
            continue;
        }
        // A driven angle of three points (`845c3bbf…`: the origin point and
        // a point twice, its parameter computed): it holds nothing
        // *(verified: the 37 of the corpus)*.
        if dc.is(e, &POINTS_ANGLE)
            && constraint_body(dc, e)
                .ok()
                .and_then(|(p, _)| params.by_record(p?))
                .is_some_and(|p| p.is_computed())
        {
            continue;
        }
        if let Some((_, kind, keys)) = CONSTRAINTS.iter().find(|(c, _, _)| *c == t) {
            if kind.is_empty() {
                continue;
            }
            match constraint_body(dc, e) {
                // Between geometry projected from the model, which stays
                // where it is (Mitcad fixes it): it holds nothing, and
                // Mitcad would take it for a contradiction.
                Ok((_, rest))
                    if rest.len() >= keys.len()
                        && rest[..keys.len()].iter().all(|&v| {
                            dc::reference(v).is_some_and(|r| ids.contains_key(&r) && held(r))
                        }) => {}
                // A curve's own end on it holds nothing either.
                Ok((_, rest))
                    if *kind == "CoincidentConstraint"
                        && rest.len() >= 2
                        && match (dc::reference(rest[0]), dc::reference(rest[1])) {
                            (Some(a), Some(b)) => {
                                ends_of(a).contains(&b) || ends_of(b).contains(&a)
                            }
                            _ => false,
                        } => {}
                Ok((_, mut rest)) if rest.len() >= keys.len() => {
                    // A line parallel or perpendicular to a circle or arc
                    // (along its radius) is stored either way round;
                    // Mitcad's constraints take the line first.
                    let straight = |v: u32| {
                        dc::reference(v).is_some_and(|r| dc.is(r, &LINE) || dc.is(r, &SEGMENT))
                    };
                    if keys[..] == ["lineOne", "lineTwo"][..]
                        && !straight(rest[0])
                        && straight(rest[1])
                    {
                        rest.swap(0, 1);
                    }
                    // A line perpendicular to a circle or arc runs along its
                    // radius: the circle's centre lies on the line *(seen:
                    // radial lines at an arc's end)*.
                    if *kind == "PerpendicularConstraint"
                        && straight(rest[0])
                        && let Some(c) = dc::reference(rest[1]).filter(|&c| dc.is(c, &CIRCLE))
                    {
                        match (
                            local(rest[0]),
                            centre_point(dc, c).and_then(|p| ids.get(&p)),
                        ) {
                            (Some(line), Some(centre)) => constraints.push(json!({
                                "id": format!("k{}", constraints.len()),
                                "type": "CoincidentConstraint",
                                "refs": {"point": centre, "entity": line}})),
                            _ => dropped.push(format!("{kind} {e}: an entity that was not read")),
                        }
                        continue;
                    }
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
                // Between geometry projected from the model (fixed in
                // Mitcad) it measures, as a driven dimension.
                let on_held = entities.iter().all(|&v| dc::reference(v).is_some_and(held));
                let driving = p.in_table && !on_held;
                if driving {
                    used.push(p.record);
                }
                let points = |i: usize| local[i].starts_with('p');
                let line = |i: usize| dc::reference(entities[i]).is_some_and(|r| dc.is(r, &LINE));
                let centre = |i: usize| {
                    entities
                        .get(i)
                        .and_then(|&v| dc::reference(v))
                        .and_then(|r| circle(dc, r))
                };
                // A distance between concentric circles or arcs: the
                // difference of their radii *(verified: 141 of the 152
                // distances to circles of the older parts)*.
                let concentric = kind == "SketchLinearDimension"
                    && match (centre(0), centre(1)) {
                        (Some((a, _)), Some((b, _))) => (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-9,
                        _ => false,
                    };
                // Another distance to a circle or arc (to its edge *(seen)*):
                // Mitcad has no such dimension.
                if kind == "SketchLinearDimension"
                    && !concentric
                    && ((!points(0) && !line(0)) || (!points(1) && !line(1)))
                {
                    return Err("a distance to a circle or arc is not supported".to_owned());
                }
                // A distance to a centre line (or a construction line) that
                // is twice what the geometry gives: a diameter about it (of
                // a revolution's profile) *(seen)*.
                let diameter = (|| -> Option<(usize, usize)> {
                    if kind != "SketchLinearDimension" {
                        return None;
                    }
                    let r = |i: usize| dc::reference(*entities.get(i)?);
                    // A centre line, or a construction line in older
                    // parts.
                    let axis = (0..2).find(|&i| {
                        r(i).is_some_and(|l| {
                            dc.is(l, &LINE)
                                && entity(dc, l)
                                    .is_ok_and(|(f, _)| f & (CENTER_LINE | CONSTRUCTION) != 0)
                        })
                    })?;
                    let other = 1 - axis;
                    let ends = ends_of(r(axis)?);
                    let (a, b) = (at.get(ends.first()?)?, at.get(ends.get(1)?)?);
                    let q = match r(other)? {
                        o if dc.is(o, &POINT) => *at.get(&o)?,
                        o if dc.is(o, &LINE) => *at.get(ends_of(o).first()?)?,
                        _ => return None,
                    };
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let n = dx.hypot(dy);
                    let dist = ((q[0] - a[0]) * dy - (q[1] - a[1]) * dx).abs() / n.max(1e-300);
                    let tol = 1e-6 * p.value.abs().max(1.0);
                    ((2.0 * dist - p.value).abs() < tol && (dist - p.value).abs() > tol)
                        .then_some((axis, other))
                })();
                let (dtype, refs, orientation) = match kind {
                    "SketchLinearDimension" if diameter.is_some() => {
                        let (axis, other) = diameter.expect("checked");
                        (
                            "SketchLinearDiameterDimension",
                            json!({"line": local[axis], "entityTwo": local[other]}),
                            None,
                        )
                    }
                    "SketchLinearDimension" if concentric => (
                        "SketchConcentricCircleDimension",
                        json!({"circleOne": local[0], "circleTwo": local[1]}),
                        None,
                    ),
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
        // The control polygon of a spline that came in with its control
        // points.
        if dc.is(e, &CONTROL_POLYGON)
            && constraint_body(dc, e).is_ok_and(|(_, rest)| {
                rest.iter()
                    .filter_map(|&v| dc::reference(v))
                    .any(|r| controlled.contains(&r))
            })
        {
            continue;
        }
        // A record of another kind laid out as a constraint, of geometry
        // projected from the model only (a projected spline's control
        // polygon `9d858c5d…`): it holds nothing *(seen)*.
        if constraint_body(dc, e).is_ok_and(|(_, rest)| {
            let geometry: Vec<usize> = rest
                .iter()
                .filter_map(|&v| dc::reference(v))
                .filter(|&r| {
                    [POINT, LINE, CIRCLE, ELLIPSE, SEGMENT, SPLINE]
                        .iter()
                        .any(|t| dc.is(r, t))
                })
                .collect();
            !geometry.is_empty() && geometry.iter().all(|&r| ids.contains_key(&r) && held(r))
        }) {
            continue;
        }
        dropped.push(format!("record {e} of type {t}: not decoded"));
    }
    let (chains, offset_dropped) = crate::groups::offsets(dc, &offsets, &ids, &dimensions);
    for mut c in chains {
        c["id"] = json!(format!("k{}", constraints.len()));
        constraints.push(c);
    }
    dropped.extend(offset_dropped);
    for e in points.iter_mut().chain(curves.iter_mut()) {
        if e["id"]
            .as_str()
            .is_some_and(|id| fixed.iter().any(|f| f == id))
        {
            e["isFixed"] = json!(true);
        }
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
        flagged,
    })
}
