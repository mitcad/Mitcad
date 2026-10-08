// SPDX-License-Identifier: MIT
//! Sketches: the file's points, curves, constraints, dimensions and texts as
//! one Mitcad sketch definition (`commands.md`, *sketch*).
//!
//! - Ids: the dump numbers points (`p<i>`) and curves (`c<i>`) separately,
//!   Mitcad shares one number space, so points come first and curves
//!   after them. Points that only Mitcad needs (an ellipse's major axis
//!   end, the control points of a fitted spline) come last.
//! - Positions are the stored solved geometry (cm → mm), so the solver
//!   starts at the solution; arcs run counter-clockwise (a stored arc may
//!   run clockwise when its normal is the sketch's −Z).
//! - Constraints and dimensions are mapped by type; those Mitcad lacks are
//!   left out and counted. Driving dimensions refer to the parameter
//!   created for the file's (`"value": "d3"`).
//! - Polygons, sketch patterns, offsets and concentric circle dimensions
//!   become what Mitcad's own tools make (`groups.rs`); their helper
//!   entities (construction circles and lines, points on circles) come
//!   after all others.

use std::collections::HashMap;

use mitcad_f3d::design::ir::{
    Geometry, RefValue, Reference, SketchConstraint, SketchCurve, SketchDetail, SketchDimension,
    Vec3,
};
use serde_json::{Value, json};

use crate::geom::{add, cross, mm, scale, unit};
use crate::params::ParamMap;

mod groups;

/// What a sketch translates to, without its plane.
#[derive(Debug, Default)]
pub struct SketchParts {
    pub entities: Vec<Value>,
    pub constraints: Vec<Value>,
    /// Dimensions as driving ones (with values).
    pub dimensions: Vec<Value>,
    /// The same dimensions, all driven.
    pub driven_dimensions: Vec<Value>,
    pub texts: Vec<Value>,
    /// Circular and rectangular patterns and offsets (`patterns`,
    /// `offsets` of the definition).
    pub patterns: Vec<Value>,
    pub offsets: Vec<Value>,
    /// Parameters to create before the sketch is added: (placeholder in
    /// the values, suggested name, expression); offsets whose distance
    /// parameter is negative use its negation (`groups.rs`).
    pub new_params: Vec<(String, String, String)>,
    /// Entity id in the file (`p3`, `c5`) → Mitcad id (`p4`, `c50`).
    pub ids: HashMap<String, String>,
    /// Where each Mitcad point should be after solving (sketch mm).
    pub positions: Vec<(u32, [f64; 2])>,
    /// What was left out.
    pub dropped: Vec<String>,
}

/// Point or curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Point,
    Curve,
}

struct Ids {
    map: HashMap<String, (u32, Kind)>,
    next: u32,
}

impl Ids {
    fn get(&self, id: &str) -> Option<(u32, Kind)> {
        self.map.get(id).copied()
    }

    fn name(&self, id: &str) -> Option<String> {
        self.get(id).map(|(n, k)| match k {
            Kind::Point => format!("p{n}"),
            Kind::Curve => format!("c{n}"),
        })
    }

    fn fresh(&mut self) -> u32 {
        self.next += 1;
        self.next - 1
    }
}

fn xy(p: Vec3) -> [f64; 2] {
    [mm(p[0]), mm(p[1])]
}

fn opt<T: Clone>(v: &Option<Option<T>>) -> Option<T> {
    v.clone().flatten()
}

/// Translates the entities, constraints, dimensions and texts.
pub fn translate(detail: &SketchDetail, params: &ParamMap) -> SketchParts {
    let mut parts = SketchParts::default();
    let points = detail.points.as_deref().unwrap_or(&[]);
    let curves = detail.curves.as_deref().unwrap_or(&[]);
    let mut ids = Ids {
        map: HashMap::new(),
        next: 1,
    };
    let mut point_at: HashMap<u32, [f64; 2]> = HashMap::new();

    // Points.
    for p in points {
        let (Some(id), Some(at)) = (p.id.as_deref(), p.xyz) else {
            continue;
        };
        let n = ids.fresh();
        ids.map.insert(id.to_owned(), (n, Kind::Point));
        let at = xy(at);
        point_at.insert(n, at);
        let mut e = json!({"id": format!("p{n}"), "type": "point", "at": at});
        if p.is_fixed == Some(true) || p.is_reference == Some(true) {
            e["fixed"] = json!(true);
        }
        if p.is_reference == Some(true) {
            e["reference"] = json!(true);
        }
        parts.entities.push(e);
    }
    // Curves get their numbers first, so that synthesized points come last.
    let curve_numbers: Vec<Option<u32>> = curves
        .iter()
        .map(|c| {
            let id = c.id.as_deref()?;
            let n = ids.fresh();
            ids.map.insert(id.to_owned(), (n, Kind::Curve));
            Some(n)
        })
        .collect();
    let mut extra_points = Vec::new();
    for (c, n) in curves.iter().zip(curve_numbers) {
        let Some(n) = n else { continue };
        match curve(c, n, &mut ids, &mut point_at, &mut extra_points) {
            Ok(mut e) => {
                let id = c.id.as_deref().unwrap_or("?");
                if c.is_construction == Some(true) {
                    e["construction"] = json!(true);
                }
                if c.is_fixed == Some(true) || c.is_reference == Some(true) {
                    e["fixed"] = json!(true);
                }
                if c.is_reference == Some(true) {
                    e["reference"] = json!(true);
                }
                if c.is_center_line == Some(true) && e["type"] == "line" {
                    e["centerline"] = json!(true);
                }
                let _ = id;
                parts.entities.push(e);
            }
            Err(reason) => {
                ids.map.remove(c.id.as_deref().unwrap_or_default());
                parts
                    .dropped
                    .push(format!("{}: {reason}", c.id.as_deref().unwrap_or("?")));
            }
        }
    }
    parts.entities.extend(extra_points);
    // Points that no curve uses keep their place; every point is checked
    // after solving.
    parts.positions = point_at.iter().map(|(n, at)| (*n, *at)).collect();
    parts.positions.sort_by_key(|(n, _)| *n);

    let mut added = groups::Group::default();
    let mut dropped = Vec::new();
    let mut constraints = Vec::new();
    let mut dimensions = Vec::new();
    let dims = detail.dimensions.as_deref().unwrap_or(&[]);
    let mut sk = groups::Sketch {
        entities: parts
            .entities
            .iter()
            .filter_map(|e| Some((e["id"].as_str()?.to_owned(), e)))
            .collect(),
        at: &point_at,
        ids: &ids,
        params,
        next: ids.next,
    };
    // Offset dimensions that their offsets stand for.
    let mut offset_dimensions = std::collections::HashSet::new();
    for c in detail.constraints.as_deref().unwrap_or(&[]) {
        let group = match c.constraint_type.as_deref() {
            Some("PolygonConstraint") => Some(groups::polygon(c, &mut sk)),
            Some("CircularPatternConstraint") => Some(groups::circular_pattern(c, &mut sk)),
            Some("RectangularPatternConstraint") => Some(groups::rectangular_pattern(c, &mut sk)),
            Some("OffsetConstraint") => Some(groups::offset(c, &mut sk, dims).map(|(g, d)| {
                offset_dimensions.extend(d);
                g
            })),
            _ => None,
        };
        let result = match group {
            Some(g) => g
                .map(|g| added.append(g))
                .map_err(|e| format!("{}: {e}", c.constraint_type.as_deref().unwrap_or_default())),
            None => constraint(c, &ids).map(|k| constraints.extend(k)),
        };
        if let Err(reason) = result {
            dropped.push(format!("{}: {reason}", c.id.as_deref().unwrap_or("?")));
        }
    }
    for d in dims {
        let result = match d.dimension_type.as_deref() {
            Some("SketchOffsetCurvesDimension") => {
                if d.id
                    .as_ref()
                    .is_some_and(|id| offset_dimensions.contains(id))
                {
                    Ok(())
                } else {
                    Err("SketchOffsetCurvesDimension: an offset that was left out".to_owned())
                }
            }
            Some(kind @ "SketchConcentricCircleDimension") => driving_value(d, kind, params)
                .and_then(|v| groups::concentric_dimension(d, &mut sk, v))
                .map(|g| added.append(g))
                .map_err(|e| format!("{kind}: {e}")),
            _ => dimension(d, &ids, params).map(|pair| dimensions.push(pair)),
        };
        if let Err(reason) = result {
            dropped.push(format!("{}: {reason}", d.id.as_deref().unwrap_or("?")));
        }
    }
    let next = sk.next;
    ids.next = next;
    parts.dropped.extend(dropped);
    parts.constraints = constraints;
    parts.constraints.extend(added.constraints);
    parts.entities.extend(added.entities);
    for (driving, driven) in dimensions.into_iter().chain(added.dimensions) {
        parts.dimensions.push(driving);
        parts.driven_dimensions.push(driven);
    }
    parts.new_params = added.params;
    // Constraint, dimension, pattern and offset ids share one space.
    for (i, item) in parts
        .constraints
        .iter_mut()
        .chain(parts.dimensions.iter_mut())
        .enumerate()
    {
        item["id"] = json!(format!("k{}", i + 1));
    }
    let first_dimension = parts.constraints.len();
    for (i, item) in parts.driven_dimensions.iter_mut().enumerate() {
        item["id"] = json!(format!("k{}", first_dimension + i + 1));
    }
    let first_record = parts.constraints.len() + parts.dimensions.len();
    parts.patterns = added.patterns;
    parts.offsets = added.offsets;
    for (i, item) in parts
        .patterns
        .iter_mut()
        .chain(parts.offsets.iter_mut())
        .enumerate()
    {
        item["id"] = json!(format!("k{}", first_record + i + 1));
    }
    let positions: HashMap<u32, [f64; 2]> = parts.positions.iter().copied().collect();
    for (i, t) in detail.texts.as_deref().unwrap_or(&[]).iter().enumerate() {
        let n = ids.next + i as u32;
        if let Some(text) = text(t, n, &ids, &parts.entities, &positions) {
            parts.texts.push(text);
        }
    }
    parts.ids = ids
        .map
        .keys()
        .filter_map(|k| Some((k.clone(), ids.name(k)?)))
        .collect();
    parts
}

/// A Mitcad point for a stored point id, or a new point at `at` when the
/// curve has none.
fn point_ref(
    stored: Option<&str>,
    at: Option<[f64; 2]>,
    ids: &mut Ids,
    point_at: &mut HashMap<u32, [f64; 2]>,
    extra: &mut Vec<Value>,
) -> Result<u32, String> {
    if let Some((n, Kind::Point)) = stored.and_then(|f| ids.get(f)) {
        return Ok(n);
    }
    let at = at.ok_or("a point of the curve is missing")?;
    let n = ids.fresh();
    point_at.insert(n, at);
    extra.push(json!({"id": format!("p{n}"), "type": "point", "at": at}));
    Ok(n)
}

fn curve(
    c: &SketchCurve,
    n: u32,
    ids: &mut Ids,
    point_at: &mut HashMap<u32, [f64; 2]>,
    extra: &mut Vec<Value>,
) -> Result<Value, String> {
    let kind = c.curve_type.as_deref().ok_or("curve type not decoded")?;
    let g = c.geometry.clone().unwrap_or_default();
    let id = format!("c{n}");
    let p = |n: u32| format!("p{n}");
    match kind {
        "SketchLine" => {
            let start = point_ref(
                opt(&c.start_sketch_point).as_deref(),
                g.start_point.map(xy),
                ids,
                point_at,
                extra,
            )?;
            let end = point_ref(
                opt(&c.end_sketch_point).as_deref(),
                g.end_point.map(xy),
                ids,
                point_at,
                extra,
            )?;
            if start == end {
                return Err("a line of zero length".to_owned());
            }
            Ok(json!({"id": id, "type": "line", "start": p(start), "end": p(end)}))
        }
        "SketchCircle" => {
            let center = point_ref(
                opt(&c.center_sketch_point).as_deref(),
                g.center.map(xy),
                ids,
                point_at,
                extra,
            )?;
            let radius = g.radius.or(c.radius).ok_or("no radius")?;
            Ok(json!({"id": id, "type": "circle", "center": p(center), "radius": mm(radius)}))
        }
        "SketchArc" => {
            let center_at = g.center.map(xy);
            let center = point_ref(
                opt(&c.center_sketch_point).as_deref(),
                center_at,
                ids,
                point_at,
                extra,
            )?;
            let (from, to) = arc_ends(&g);
            let start = point_ref(
                opt(&c.start_sketch_point).as_deref(),
                from,
                ids,
                point_at,
                extra,
            )?;
            let end = point_ref(
                opt(&c.end_sketch_point).as_deref(),
                to,
                ids,
                point_at,
                extra,
            )?;
            let (start, end) = counter_clockwise(&g, start, end, point_at);
            Ok(
                json!({"id": id, "type": "arc", "center": p(center), "start": p(start), "end": p(end)}),
            )
        }
        "SketchEllipse" | "SketchEllipticalArc" => {
            let center_at = g.center.map(xy);
            let center = point_ref(
                opt(&c.center_sketch_point).as_deref(),
                center_at,
                ids,
                point_at,
                extra,
            )?;
            let major_dir = g
                .major_axis
                .or(c.major_axis)
                .and_then(unit)
                .ok_or("no major axis")?;
            let major_radius = g
                .major_radius
                .or(c.major_axis_radius)
                .ok_or("no major radius")?;
            let minor = g
                .minor_radius
                .or(c.minor_axis_radius)
                .ok_or("no minor radius")?;
            let c_at = center_at
                .or_else(|| point_at.get(&center).copied())
                .ok_or("no centre")?;
            let major_at = [
                c_at[0] + mm(major_dir[0] * major_radius),
                c_at[1] + mm(major_dir[1] * major_radius),
            ];
            let major = point_ref(None, Some(major_at), ids, point_at, extra)?;
            let mut e = json!({"id": id, "type": "ellipse", "center": p(center),
                               "major": p(major), "minor_radius": mm(minor)});
            if kind == "SketchEllipticalArc" {
                let start = point_ref(
                    opt(&c.start_sketch_point).as_deref(),
                    None,
                    ids,
                    point_at,
                    extra,
                )?;
                let end = point_ref(
                    opt(&c.end_sketch_point).as_deref(),
                    None,
                    ids,
                    point_at,
                    extra,
                )?;
                let (start, end) = counter_clockwise(&g, start, end, point_at);
                e["type"] = json!("elliptical_arc");
                e["start"] = json!(p(start));
                e["end"] = json!(p(end));
            }
            Ok(e)
        }
        "SketchControlPointSpline"
        | "SketchFittedSpline"
        | "SketchFixedSpline"
        | "SketchConicCurve" => {
            let nurbs = g.nurbs.as_ref().ok_or("no NURBS data")?;
            spline(c, kind, nurbs, &id, ids, point_at, extra)
        }
        other => Err(format!("{other} is not supported")),
    }
}

/// The 2D end points of a stored arc's geometry, start then end (along the
/// arc's normal).
fn arc_ends(g: &Geometry) -> (Option<[f64; 2]>, Option<[f64; 2]>) {
    let (Some(c), Some(n), Some(r0), Some(radius), Some(a0), Some(a1)) = (
        g.center,
        g.normal,
        g.reference_vector,
        g.radius,
        g.start_angle,
        g.end_angle,
    ) else {
        return (g.start_point.map(xy), g.end_point.map(xy));
    };
    let r0 = unit(r0).unwrap_or([1.0, 0.0, 0.0]);
    let y = cross(unit(n).unwrap_or([0.0, 0.0, 1.0]), r0);
    let at = |a: f64| {
        xy(add(
            c,
            add(scale(r0, radius * a.cos()), scale(y, radius * a.sin())),
        ))
    };
    (Some(at(a0)), Some(at(a1)))
}

/// Start and end in Mitcad's counter-clockwise order: stored arcs run along
/// the arc normal, which may be the sketch's −Z; the points are also
/// checked against the geometry, so swapped references do not matter.
fn counter_clockwise(
    g: &Geometry,
    start: u32,
    end: u32,
    point_at: &HashMap<u32, [f64; 2]>,
) -> (u32, u32) {
    let normal_z = g.normal.map_or(1.0, |n| n[2]);
    let (mut s, mut e) = (start, end);
    // The referenced start should be at the geometry's start angle.
    if let ((Some(gs), Some(_)), Some(ps), Some(pe)) =
        (arc_ends(g), point_at.get(&start), point_at.get(&end))
    {
        let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        if d(gs, *pe) < d(gs, *ps) {
            std::mem::swap(&mut s, &mut e);
        }
    }
    if normal_z < 0.0 { (e, s) } else { (s, e) }
}

fn spline(
    c: &SketchCurve,
    kind: &str,
    nurbs: &Value,
    id: &str,
    ids: &mut Ids,
    point_at: &mut HashMap<u32, [f64; 2]>,
    extra: &mut Vec<Value>,
) -> Result<Value, String> {
    if nurbs["isPeriodic"] == true {
        return Err("periodic splines are not supported".to_owned());
    }
    let degree = nurbs["degree"].as_u64().ok_or("no degree")? as u32;
    let poles: Vec<[f64; 2]> = nurbs["controlPoints"]
        .as_array()
        .ok_or("no control points")?
        .iter()
        .map(|p| {
            let v: Vec3 = serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
            Ok(xy(v))
        })
        .collect::<Result<_, String>>()?;
    let knots: Vec<f64> = serde_json::from_value(nurbs["knots"].clone()).unwrap_or_default();
    let weights: Vec<f64> = if nurbs["isRational"] == true {
        serde_json::from_value(nurbs["weights"].clone()).unwrap_or_default()
    } else {
        Vec::new()
    };
    if knots.len() != poles.len() + degree as usize + 1 {
        return Err(format!(
            "{} knots for {} control points of degree {degree}",
            knots.len(),
            poles.len()
        ));
    }
    // Control point splines use their sketch points; the others get fixed
    // points of their own.
    let given: Vec<String> = if kind == "SketchControlPointSpline" {
        c.control_points.clone().unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut control = Vec::new();
    for (i, at) in poles.iter().enumerate() {
        let stored = given.get(i).map(String::as_str);
        let n = point_ref(stored, Some(*at), ids, point_at, extra)?;
        if stored.is_none()
            && let Some(e) = extra.last_mut()
        {
            e["fixed"] = json!(true);
        }
        control.push(format!("p{n}"));
    }
    let mut e = json!({"id": id, "type": "spline", "degree": degree, "control": control,
                       "knots": knots});
    if !weights.is_empty() {
        e["weights"] = json!(weights);
    }
    if kind != "SketchControlPointSpline" {
        e["fixed"] = json!(true);
    }
    Ok(e)
}

/// The sketch-local ids a constraint or dimension refers to, in order:
/// the decoder's `entities` list, or an external dump's properties.
fn refs(map: Option<&std::collections::BTreeMap<String, RefValue>>, keys: &[&str]) -> Vec<RefOf> {
    let Some(map) = map else {
        return Vec::new();
    };
    if let Some(RefValue::List(list)) = map.get("entities") {
        return list.iter().map(RefOf::of).collect();
    }
    keys.iter()
        .filter_map(|k| map.get(*k))
        .map(RefOf::of)
        .collect()
}

/// A reference inside or outside the sketch.
#[derive(Debug, Clone)]
enum RefOf {
    Local(String),
    Outside,
}

impl RefOf {
    fn of(v: &RefValue) -> RefOf {
        match v {
            RefValue::Id(id) => RefOf::Local(id.clone()),
            RefValue::Ref(Reference::SketchEntity(e)) => {
                e.id.clone().flatten().map_or(RefOf::Outside, RefOf::Local)
            }
            _ => RefOf::Outside,
        }
    }
}

/// Resolved references: Mitcad ids with their kinds.
fn resolve(refs: &[RefOf], ids: &Ids) -> Result<Vec<(String, Kind)>, String> {
    refs.iter()
        .map(|r| match r {
            RefOf::Local(id) => ids
                .get(id)
                .map(|(n, k)| {
                    let name = match k {
                        Kind::Point => format!("p{n}"),
                        Kind::Curve => format!("c{n}"),
                    };
                    (name, k)
                })
                .ok_or_else(|| format!("refers to {id}, which was left out")),
            RefOf::Outside => Err("refers to geometry outside the sketch".to_owned()),
        })
        .collect()
}

fn constraint(c: &SketchConstraint, ids: &Ids) -> Result<Option<Value>, String> {
    let Some(kind) = c.constraint_type.as_deref() else {
        return Err("constraint type not decoded".to_owned());
    };
    let keys: &[&str] = match kind {
        "CoincidentConstraint" => &["point", "entity"],
        "HorizontalConstraint" | "VerticalConstraint" => &["line"],
        "HorizontalPointsConstraint" | "VerticalPointsConstraint" => &["pointOne", "pointTwo"],
        "ParallelConstraint" | "PerpendicularConstraint" | "CollinearConstraint" => {
            &["lineOne", "lineTwo"]
        }
        "TangentConstraint" | "SmoothConstraint" | "EqualConstraint" => &["curveOne", "curveTwo"],
        "ConcentricConstraint" => &["entityOne", "entityTwo"],
        "MidPointConstraint" => &["point", "midPointCurve"],
        "SymmetryConstraint" => &["entityOne", "entityTwo", "symmetryLine"],
        other => return Err(format!("{other} is not supported")),
    };
    let r = resolve(&refs(c.refs.as_ref(), keys), ids)?;
    let points: Vec<&String> = r
        .iter()
        .filter(|(_, k)| *k == Kind::Point)
        .map(|(n, _)| n)
        .collect();
    let curves: Vec<&String> = r
        .iter()
        .filter(|(_, k)| *k == Kind::Curve)
        .map(|(n, _)| n)
        .collect();
    let two_curves = |t: &str| -> Result<Value, String> {
        match curves[..] {
            [a, b] if points.is_empty() => Ok(json!({"type": t, "a": a, "b": b})),
            _ => Err(format!("{kind} needs two curves")),
        }
    };
    let value = match kind {
        "CoincidentConstraint" => match (&points[..], &curves[..]) {
            ([p, q], []) => json!({"type": "coincident", "point": p, "entity": q}),
            ([p], [c]) => json!({"type": "coincident", "point": p, "entity": c}),
            _ => return Err("a coincidence needs a point and a point or curve".to_owned()),
        },
        "HorizontalConstraint" | "VerticalConstraint" => {
            let t = if kind == "HorizontalConstraint" {
                "horizontal"
            } else {
                "vertical"
            };
            match (&points[..], &curves[..]) {
                ([], [l]) => json!({"type": t, "line": l}),
                ([a, b], []) => json!({"type": format!("{t}_points"), "a": a, "b": b}),
                _ => return Err(format!("{kind}: unexpected entities")),
            }
        }
        "HorizontalPointsConstraint" | "VerticalPointsConstraint" => {
            let t = if kind == "HorizontalPointsConstraint" {
                "horizontal_points"
            } else {
                "vertical_points"
            };
            match points[..] {
                [a, b] => json!({"type": t, "a": a, "b": b}),
                _ => return Err(format!("{kind} needs two points")),
            }
        }
        "ParallelConstraint" => two_curves("parallel")?,
        "PerpendicularConstraint" => two_curves("perpendicular")?,
        "CollinearConstraint" => two_curves("collinear")?,
        "TangentConstraint" => two_curves("tangent")?,
        "SmoothConstraint" => two_curves("smooth")?,
        "EqualConstraint" => two_curves("equal")?,
        "ConcentricConstraint" => two_curves("concentric")?,
        "MidPointConstraint" => match (&points[..], &curves[..]) {
            ([p], [c]) => json!({"type": "midpoint", "point": p, "curve": c}),
            _ => return Err("a midpoint needs a point and a curve".to_owned()),
        },
        "SymmetryConstraint" => {
            // The axis is the last entity (the decoder's order and the
            // external dumps' symmetryLine).
            let (axis, rest) = r.split_last().ok_or("no entities")?;
            match rest {
                [(a, ka), (b, kb)] if axis.1 == Kind::Curve && ka == kb => {
                    json!({"type": "symmetric", "a": a, "b": b, "axis": axis.0})
                }
                _ => return Err("a symmetry needs two entities and a line".to_owned()),
            }
        }
        _ => unreachable!("checked above"),
    };
    Ok(Some(value))
}

/// The driving and the driven form of a dimension.
fn dimension(d: &SketchDimension, ids: &Ids, params: &ParamMap) -> Result<(Value, Value), String> {
    let Some(kind) = d.dimension_type.as_deref() else {
        return Err("dimension type not decoded".to_owned());
    };
    let keys: &[&str] = match kind {
        "SketchLinearDimension" => &["entityOne", "entityTwo"],
        "SketchOffsetDimension" => &["line", "entityTwo"],
        "SketchAngularDimension" => &["lineOne", "lineTwo"],
        "SketchRadialDimension" | "SketchDiameterDimension" => &["entity"],
        "SketchEllipseMajorRadiusDimension" | "SketchEllipseMinorRadiusDimension" => &["ellipse"],
        "SketchLinearDiameterDimension" => &["line", "entityTwo"],
        "SketchArcLengthDimension" => &["arc"],
        other => return Err(format!("{other} is not supported")),
    };
    let r = resolve(&refs(d.refs.as_ref(), keys), ids)?;
    let points: Vec<&String> = r
        .iter()
        .filter(|(_, k)| *k == Kind::Point)
        .map(|(n, _)| n)
        .collect();
    let curves: Vec<&String> = r
        .iter()
        .filter(|(_, k)| *k == Kind::Curve)
        .map(|(n, _)| n)
        .collect();
    let orientation = d
        .props
        .as_ref()
        .and_then(|p| p.get("orientation"))
        .and_then(Value::as_str)
        .unwrap_or("AlignedDimensionOrientation");
    let mut value = match kind {
        "SketchLinearDimension" => match (&points[..], &curves[..]) {
            ([a, b], []) => {
                let t = match orientation {
                    "HorizontalDimensionOrientation" => "horizontal_distance",
                    "VerticalDimensionOrientation" => "vertical_distance",
                    _ => "distance",
                };
                json!({"type": t, "a": a, "b": b})
            }
            ([], [l]) => json!({"type": "length", "line": l}),
            ([p], [l]) => json!({"type": "point_line_distance", "point": p, "line": l}),
            _ => return Err("a linear dimension of unexpected entities".to_owned()),
        },
        "SketchOffsetDimension" => match (&points[..], &curves[..]) {
            ([], [a, b]) => json!({"type": "line_distance", "a": a, "b": b}),
            ([p], [l]) => json!({"type": "point_line_distance", "point": p, "line": l}),
            _ => return Err("an offset dimension of unexpected entities".to_owned()),
        },
        "SketchAngularDimension" => match curves[..] {
            [a, b] => json!({"type": "angle", "a": a, "b": b}),
            _ => return Err("an angle needs two lines".to_owned()),
        },
        "SketchRadialDimension" | "SketchDiameterDimension" => match curves[..] {
            [c] => {
                let t = if kind == "SketchRadialDimension" {
                    "radius"
                } else {
                    "diameter"
                };
                json!({"type": t, "curve": c})
            }
            _ => return Err(format!("{kind} needs one curve")),
        },
        "SketchEllipseMajorRadiusDimension" | "SketchEllipseMinorRadiusDimension" => {
            match curves[..] {
                [c] => {
                    let t = if kind.contains("Major") {
                        "major_radius"
                    } else {
                        "minor_radius"
                    };
                    json!({"type": t, "ellipse": c})
                }
                _ => return Err(format!("{kind} needs an ellipse")),
            }
        }
        "SketchLinearDiameterDimension" => match (&points[..], &curves[..]) {
            ([p], [axis]) => json!({"type": "linear_diameter", "axis": axis, "entity": p}),
            ([], [axis, other]) => {
                json!({"type": "linear_diameter", "axis": axis, "entity": other})
            }
            _ => return Err("a linear diameter of unexpected entities".to_owned()),
        },
        "SketchArcLengthDimension" => match curves[..] {
            [c] => json!({"type": "arc_length", "arc": c}),
            _ => return Err("an arc length needs an arc".to_owned()),
        },
        _ => unreachable!("checked above"),
    };
    if let Some(t) = d.text_position {
        value["text"] = json!(xy(t));
    }
    let mut driven = value.clone();
    driven["driven"] = json!(true);
    match driving_value(d, kind, params)? {
        Some(v) => value["value"] = v,
        None => value = driven.clone(),
    }
    Ok((value, driven))
}

/// The value of a driving dimension: its parameter by its Mitcad name
/// (negated for a concentric circle dimension with a negative value),
/// else its value; None for a driven dimension.
fn driving_value(
    d: &SketchDimension,
    kind: &str,
    params: &ParamMap,
) -> Result<Option<Value>, String> {
    let parameter = d.parameter.clone().flatten();
    if d.is_driving == Some(false) {
        return Ok(None);
    }
    let Some(p) = parameter.as_ref().and_then(Reference::parameter) else {
        return Ok(None);
    };
    let v = p.value;
    let negate = kind == "SketchConcentricCircleDimension" && v.is_some_and(|v| v < 0.0);
    let name = p.name.as_deref().unwrap_or_default();
    Ok(Some(match params.get(name) {
        Some(mitcad) if negate => json!(format!("-({mitcad})")),
        Some(mitcad) => json!(mitcad),
        None => {
            let v = v.ok_or("its parameter has no value")?;
            let v = if negate { -v } else { v };
            let angle = matches!(kind, "SketchAngularDimension");
            json!(if angle { v } else { mm(v) })
        }
    }))
}

/// A sketch text (SCHEMA.md `Text`): content, height, position, angle,
/// font; `textStyle` bits bold (1) and italic (2) (underline, 4, has no
/// outline); the flips; and from its `definition` a multi-line text's
/// frame (its rectangle's corners), alignment and character spacing, or a
/// text along or fitted on a sketch curve.
fn text(
    t: &Value,
    n: u32,
    ids: &Ids,
    entities: &[Value],
    positions: &HashMap<u32, [f64; 2]>,
) -> Option<Value> {
    let text = t["text"].as_str()?;
    let height = t["height"].as_f64()?;
    let at: Vec3 = serde_json::from_value(t["position"].clone()).ok()?;
    let mut v = json!({"id": format!("t{n}"), "text": text, "at": xy(at), "height": mm(height)});
    let angle = t["angle"].as_f64().unwrap_or(0.0);
    if let Some(a) = t["angle"].as_f64() {
        v["angle"] = json!(a);
    }
    if let Some(f) = t["fontName"].as_str() {
        v["font"] = json!(f);
    }
    if let Some(style) = t["textStyle"].as_i64() {
        if style & 1 != 0 {
            v["bold"] = json!(true);
        }
        if style & 2 != 0 {
            v["italic"] = json!(true);
        }
    }
    for (key, mitcad) in [("isHorizontalFlip", "flip_x"), ("isVerticalFlip", "flip_y")] {
        if t[key] == true {
            v[mitcad] = json!(true);
        }
    }
    let definition = &t["definition"];
    // The dump's enums (values or names): left, centre, right; top, middle,
    // bottom.
    let choice = |value: &Value, names: [&'static str; 3]| -> Option<&'static str> {
        let i = match value {
            Value::Number(k) => usize::try_from(k.as_u64()?).ok()?,
            Value::String(s) => names.iter().position(|n| s.to_lowercase().starts_with(n))?,
            _ => return None,
        };
        names.get(i).copied()
    };
    let align = |v: &mut Value| {
        if let Some(a) = choice(
            &definition["horizontalAlignment"],
            ["left", "center", "right"],
        ) && a != "left"
        {
            v["align"] = json!(a);
        }
        if let Some(s) = definition["characterSpacing"].as_f64()
            && s != 0.0
        {
            v["spacing"] = json!(s);
        }
    };
    match definition["_type"].as_str() {
        Some("MultiLineTextDefinition") => {
            if let Some(frame) = frame_corners(
                &definition["rectangleLines"],
                ids,
                entities,
                positions,
                angle,
            ) {
                v["frame"] = json!(frame);
            }
            align(&mut v);
            if let Some(a) = choice(
                &definition["verticalAlignment"],
                ["top", "middle", "bottom"],
            ) {
                v["valign"] = json!(a);
            }
        }
        Some(kind @ ("AlongPathTextDefinition" | "FitOnPathTextDefinition")) => {
            let curve = definition["path"]["id"]
                .as_str()
                .and_then(|id| ids.name(id))
                .filter(|name| name.starts_with('c'));
            if let Some(curve) = curve {
                let above = definition["isAbovePath"].as_bool().unwrap_or(true);
                let mut path = json!({"curve": curve});
                if !above {
                    path["above"] = json!(false);
                }
                if kind == "FitOnPathTextDefinition" {
                    path["fit"] = json!(true);
                }
                v["path"] = path;
                align(&mut v);
            }
        }
        _ => {}
    }
    Some(v)
}

/// The corners of a multi-line text's rectangle as Mitcad points: the one
/// at the text's lower left, the next along its baseline and the next up.
fn frame_corners(
    lines: &Value,
    ids: &Ids,
    entities: &[Value],
    positions: &HashMap<u32, [f64; 2]>,
    angle: f64,
) -> Option<Vec<String>> {
    let mut corners: Vec<(String, [f64; 2])> = Vec::new();
    let mut sides: Vec<(String, String)> = Vec::new();
    for line in lines.as_array()? {
        let name = ids.name(line["id"].as_str()?)?;
        let e = entities.iter().find(|e| e["id"] == name.as_str())?;
        let ends = [
            e["start"].as_str()?.to_owned(),
            e["end"].as_str()?.to_owned(),
        ];
        for p in &ends {
            let at = *positions.get(&p.strip_prefix('p')?.parse::<u32>().ok()?)?;
            if !corners.iter().any(|(q, _)| q == p) {
                corners.push((p.clone(), at));
            }
        }
        sides.push((ends[0].clone(), ends[1].clone()));
    }
    if corners.len() != 4 || sides.len() != 4 {
        return None;
    }
    let (s, c) = angle.sin_cos();
    let along = |p: [f64; 2]| p[0] * c + p[1] * s;
    let up = |p: [f64; 2]| -p[0] * s + p[1] * c;
    let origin = corners
        .iter()
        .min_by(|a, b| (along(a.1) + up(a.1)).total_cmp(&(along(b.1) + up(b.1))))?
        .clone();
    let next: Vec<&(String, [f64; 2])> = corners
        .iter()
        .filter(|(p, _)| {
            sides
                .iter()
                .any(|(a, b)| (a == &origin.0 && b == p) || (b == &origin.0 && a == p))
        })
        .collect();
    if next.len() != 2 {
        return None;
    }
    let x = next
        .iter()
        .max_by(|a, b| along(a.1).total_cmp(&along(b.1)))?;
    let y = next.iter().max_by(|a, b| up(a.1).total_cmp(&up(b.1)))?;
    Some(vec![origin.0, x.0.clone(), y.0.clone()])
}

/// How far the solved points are from where the file had them, mm.
pub fn position_error(
    expected: &[(u32, [f64; 2])],
    solved: &std::collections::BTreeMap<mitcad_model::EntityUid, [f64; 2]>,
) -> f64 {
    expected
        .iter()
        .filter_map(|(n, at)| {
            let s = solved.get(&mitcad_model::EntityUid(*n))?;
            Some((s[0] - at[0]).hypot(s[1] - at[1]))
        })
        .fold(0.0, f64::max)
}
