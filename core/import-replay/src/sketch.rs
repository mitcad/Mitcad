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

use std::collections::{HashMap, HashSet};

use mitcad_dump::{
    Geometry, RefValue, Reference, SketchConstraint, SketchCurve, SketchDetail, SketchDimension,
    Vec3,
};
use serde_json::{Value, json};

use crate::geom::{add, cross, mm, scale, unit};
use crate::params::ParamMap;

pub mod edges;
mod groups;
// Inactive constraints and dimensions (mitcad#721), and the references of
// driven dimensions, shared with the FreeCAD import (`mitcad-import-base`).
pub(crate) use mitcad_import_base::sketch::inactive;
pub use mitcad_import_base::sketch::{has_references, without_references};

/// What a sketch translates to, without its plane.
#[derive(Debug, Default)]
pub struct SketchParts {
    pub entities: Vec<Value>,
    pub constraints: Vec<Value>,
    /// Dimensions as the file has them: driving ones with values, driven
    /// ones driven.
    pub dimensions: Vec<Value>,
    pub texts: Vec<Value>,
    /// The file's texts that were translated: their index among the
    /// file's texts, the Mitcad id (`t12`) and the string (mitcad#664).
    pub text_ids: Vec<(usize, String, String)>,
    /// Texts that follow the file's text parameters (`text_values` of the
    /// definition, mitcad#664; [`text_values`]).
    pub text_values: Vec<Value>,
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
    /// Curves the file links to geometry outside the sketch (projected or
    /// included). They come in as fixed reference entities; the source
    /// they follow is not stored in the curve (no linked projection).
    pub linked: usize,
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
                if c.is_linked == Some(true) {
                    parts.linked += 1;
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

    let fit_points: HashSet<String> = curves
        .iter()
        .filter_map(|c| c.fit_points.as_ref())
        .flatten()
        .cloned()
        .collect();
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
            Some("MidPointConstraint") if groups::is_curve_pair(c, &ids) => {
                Some(groups::midpoint_on_curve(c, &mut sk))
            }
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
            // A fitted spline's tangent handles are no curves of the sketch,
            // and its curve is kept as it is: what only places the fit
            // points at their middles binds nothing.
            None if handle_constraint(c, &fit_points) => Ok(()),
            None => match fixed_smooth(c, &ids, &sk.entities) {
                Some(reason) => Err(reason),
                None => constraint(c, &ids, &sk.entities).map(|k| constraints.extend(k)),
            },
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
            _ => dimension(d, &ids, params, &sk.entities, sk.at).map(|v| dimensions.push(v)),
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
    parts.dimensions = dimensions;
    parts.dimensions.extend(added.dimensions);
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
            start_path_circle(&text, t, &mut parts.entities);
            let string = text["text"].as_str().unwrap_or_default().to_owned();
            parts.text_ids.push((i, format!("t{n}"), string));
            parts.texts.push(text);
        }
    }
    parts.ids = ids
        .map
        .keys()
        .filter_map(|k| Some((k.clone(), ids.name(k)?)))
        .collect();
    // The texts too (mitcad#1177: an extrusion names the texts whose
    // letters it selects).
    let texts = detail.texts.as_deref().unwrap_or(&[]);
    for (i, ours, _) in &parts.text_ids {
        if let Some(file) = texts.get(*i).and_then(|t| t["id"].as_str()) {
            parts.ids.insert(file.to_owned(), ours.clone());
        }
    }
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
            let mut circle =
                json!({"id": id, "type": "circle", "center": p(center), "radius": mm(radius)});
            // Where the file's circle starts (its seam, read from the stored
            // bodies): the cylinder it makes has its seam edge there.
            if let Some(start) = c.f3d_seam.filter(|a| a.abs() > 1e-9) {
                circle["start_angle"] = json!(start);
            }
            Ok(circle)
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
                .ok_or("no center")?;
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
    let degree = nurbs["degree"].as_u64().ok_or("no degree")? as u32;
    let poles: Vec<[f64; 2]> = nurbs["controlPoints"]
        .as_array()
        .ok_or("no control points")?
        .iter()
        .map(|p| {
            let v: Vec3 = serde_json::from_value(p.clone()).map_err(|e| e.to_string())?;
            // A spline out of the sketch's plane (a 3D sketch) would be
            // flattened (sketch space, cm).
            if v[2].abs() > 1e-6 {
                return Err("a spline out of the sketch's plane".to_owned());
            }
            Ok(xy(v))
        })
        .collect::<Result<_, String>>()?;
    // The decoder calls a spline "periodic" where its knot vector is not
    // clamped; such a curve is a periodic one only where its control polygon
    // wraps (the first control points are the last ones). Without it the
    // curve is open and a B-spline like any other.
    let wraps = degree > 0
        && poles.len() > 2 * degree as usize
        && poles[..degree as usize]
            .iter()
            .zip(&poles[poles.len() - degree as usize..])
            .all(|(a, b)| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if nurbs["isPeriodic"] == true && wraps {
        return Err("periodic splines are not supported".to_owned());
    }
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

/// Why a smooth constraint (mitcad#186) cannot be kept: one of its curves
/// is a fixed spline (a fitted spline gets control points of its own here,
/// which the other curve's end does not share), and the sketch would not
/// take it, losing its other constraints with it.
fn fixed_smooth(
    c: &SketchConstraint,
    ids: &Ids,
    entities: &HashMap<String, &Value>,
) -> Option<String> {
    if c.constraint_type.as_deref() != Some("SmoothConstraint") {
        return None;
    }
    let curves = resolve(&refs(c.refs.as_ref(), &["curveOne", "curveTwo"]), ids).ok()?;
    curves
        .iter()
        .any(|(name, _)| {
            entities
                .get(name)
                .is_some_and(|e| e["type"] == "spline" && e["fixed"] == true)
        })
        .then(|| {
            "SmoothConstraint: a smooth join with a fixed spline is not supported (a fitted \
             spline's control points are fixed)"
                .to_owned()
        })
}

/// A midpoint constraint between a fit point of a fitted spline and a curve
/// the sketch does not list: the point at the middle of the spline's
/// tangent handle there.
fn handle_constraint(c: &SketchConstraint, fit_points: &HashSet<String>) -> bool {
    if c.constraint_type.as_deref() != Some("MidPointConstraint") {
        return false;
    }
    matches!(
        &refs(c.refs.as_ref(), &["point", "midPointCurve"])[..],
        [RefOf::Outside, RefOf::Local(p)] | [RefOf::Local(p), RefOf::Outside] if fit_points.contains(p)
    )
}

fn constraint(
    c: &SketchConstraint,
    ids: &Ids,
    entities: &HashMap<String, &Value>,
) -> Result<Option<Value>, String> {
    let Some(kind) = c.constraint_type.as_deref() else {
        return Err("constraint type not decoded".to_owned());
    };
    // What ties the construction lines of a control point spline's polygon
    // or of a text's box to their spline or text constrains none of the
    // geometry: the lines share the control points' places and the text
    // keeps its own frame (mitcad#186).
    if matches!(
        kind,
        "ControlPolygonConstraint" | "TextBoxConstraint" | "TextPathConstraint"
    ) {
        return Ok(None);
    }
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
        // Three points on a line (a centre line's ends and a point it
        // passes, such as the origin): the stored constraint names the
        // points, not a line. The sketch's line between two of them takes
        // the third.
        "CollinearConstraint" if curves.is_empty() && points.len() == 3 => {
            let ends = |line: &Value| -> Option<(String, String)> {
                Some((
                    line["start"].as_str()?.to_owned(),
                    line["end"].as_str()?.to_owned(),
                ))
            };
            let mut found = None;
            for (name, line) in entities {
                if line["type"] != "line" {
                    continue;
                }
                let Some((a, b)) = ends(line) else { continue };
                let others: Vec<&&String> = points
                    .iter()
                    .filter(|p| p.as_str() != a && p.as_str() != b)
                    .collect();
                if others.len() == 1
                    && points.iter().any(|p| **p == a)
                    && points.iter().any(|p| **p == b)
                {
                    // The smallest line name first, so the choice does not
                    // depend on the map's order.
                    if found
                        .as_ref()
                        .is_none_or(|(n, _): &(String, String)| name < n)
                    {
                        found = Some((name.clone(), others[0].to_string()));
                    }
                }
            }
            let (line, point) =
                found.ok_or("collinear points without a line through two of them")?;
            json!({"type": "coincident", "point": point, "entity": line})
        }
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

/// A dimension as the file has it: driving with its value, or driven.
fn dimension(
    d: &SketchDimension,
    ids: &Ids,
    params: &ParamMap,
    entities: &HashMap<String, &Value>,
    at: &HashMap<u32, [f64; 2]>,
) -> Result<Value, String> {
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
        "SketchTangentDistanceDimension" => &["circleOrArc", "entityOne"],
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
            [a, b] => {
                let mut angle = json!({"type": "angle", "a": a, "b": b});
                if let Some([reverse_a, reverse_b]) = angle_side(d, a, b, entities, at) {
                    angle["reverse_a"] = json!(reverse_a);
                    angle["reverse_b"] = json!(reverse_b);
                }
                angle
            }
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
        // The gap between a circle or arc and a line (the entities come in
        // either order).
        "SketchTangentDistanceDimension" => {
            let round = |c: &&String| {
                entities
                    .get(c.as_str())
                    .is_some_and(|e| matches!(e["type"].as_str(), Some("circle" | "arc")))
            };
            let line = |c: &&String| {
                entities
                    .get(c.as_str())
                    .is_some_and(|e| e["type"] == "line")
            };
            match (&points[..], &curves[..]) {
                ([], [a, b]) if round(a) && line(b) => {
                    json!({"type": "tangent_distance", "circle": a, "line": b})
                }
                ([], [a, b]) if line(a) && round(b) => {
                    json!({"type": "tangent_distance", "circle": b, "line": a})
                }
                _ => return Err("a tangent distance needs a circle or arc and a line".to_owned()),
            }
        }
        _ => unreachable!("checked above"),
    };
    if let Some(t) = d.text_position {
        value["text"] = json!(xy(t));
    }
    match driving_value(d, kind, params)? {
        Some(v) => {
            // An ellipse's dimension that holds the whole axis: its radius
            // is half.
            let axis = d.f3d.as_ref().and_then(|f| f.axis_length).unwrap_or(false);
            value["value"] = match (axis, v) {
                (true, Value::String(s)) => json!(format!("({s}) / 2")),
                (true, v) => json!(v.as_f64().unwrap_or(0.0) / 2.0),
                (false, v) => v,
            }
        }
        None => {
            value["driven"] = json!(true);
            // The file's parameter of a driven dimension is its reference
            // (mitcad#250), not a parameter of its own.
            if let Some(name) = driven_parameter(d, params) {
                value["reference"] = json!(name);
            }
        }
    }
    Ok(value)
}

/// The side of an angle dimension between the lines `a` and `b`
/// (mitcad#296): of the four sectors the lines' extensions make, the one
/// the file shows its value in when that sector's angle is the file's
/// value, else the neighbouring one nearer to it when the supplement is
/// (the file's points are where its value holds). None when the file gives
/// no value or neither angle is it (the model then takes the angle closer
/// to the value, as for sketches stored without a side).
fn angle_side(
    d: &SketchDimension,
    a: &str,
    b: &str,
    entities: &HashMap<String, &Value>,
    at: &HashMap<u32, [f64; 2]>,
) -> Option<[bool; 2]> {
    let ends = |line: &str| {
        let e = entities.get(line)?;
        let point = |key: &str| {
            let n = e[key].as_str()?.strip_prefix('p')?.parse::<u32>().ok()?;
            at.get(&n).copied()
        };
        Some((point("start")?, point("end")?))
    };
    let parameter = d.parameter.clone().flatten();
    let value = parameter
        .as_ref()
        .and_then(Reference::parameter)
        .and_then(|p| p.value)
        .or(d.value)?;
    mitcad_model::sketch::geometry::angle_side_for(
        ends(a)?,
        ends(b)?,
        value,
        d.text_position.map(xy),
        1e-3,
    )
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

/// The Mitcad name of the parameter the file has for a dimension it keeps
/// driven (mitcad#250); None for a driving one or one without a parameter
/// that was created.
fn driven_parameter(d: &SketchDimension, params: &ParamMap) -> Option<String> {
    if d.is_driving != Some(false) {
        return None;
    }
    let parameter = d.parameter.clone().flatten()?;
    let name = parameter.parameter()?.name.clone()?;
    params.get(&name).map(str::to_owned)
}

/// The sketch's texts that follow the text parameters the file made for
/// them (mitcad#664), as the definition's `text_values`: each parameter of
/// `params` (the text's number in its role and its Mitcad name, see
/// [`ParamMap::texts_of`]) goes to the text its role numbers when that
/// text's string is the parameter's text, else to the only other text with
/// that string; `text_of` gives a parameter's text. A parameter whose text
/// no text has is left alone, so a text never changes on import.
pub(crate) fn text_values(
    parts: &SketchParts,
    params: &[(Option<usize>, String)],
    text_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<Value> {
    let mut used: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for (number, name) in params {
        let Some(value) = text_of(name).filter(|v| !v.is_empty()) else {
            continue;
        };
        let free = |(_, id, string): &&(usize, String, String)| {
            *string == value && !used.contains(&id.as_str())
        };
        let numbered = number
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| parts.text_ids.iter().find(|(index, _, _)| *index == i))
            .filter(|t| free(t));
        let found = numbered.or_else(|| {
            let mut same = parts.text_ids.iter().filter(|t| free(t));
            let first = same.next();
            if same.next().is_none() { first } else { None }
        });
        if let Some((_, id, _)) = found {
            used.push(id);
            out.push(json!({"text": id, "value": name}));
        }
    }
    out
}

/// A sketch definition without its texts' text parameters (mitcad#664):
/// an import falls back on it when the model refuses one.
pub(crate) fn without_text_values(def: &Value) -> Value {
    let mut def = def.clone();
    if let Some(map) = def.as_object_mut() {
        map.remove("text_values");
    }
    def
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
    // A text in a box or on a path needs no position of its own (the API
    // gives none: the box or the path places it).
    let placed_by = matches!(
        t["definition"]["_type"].as_str(),
        Some("AlongPathTextDefinition" | "FitOnPathTextDefinition")
    ) || t["definition"]["rectangleLines"].is_array();
    let at: Option<Vec3> = serde_json::from_value(t["position"].clone()).ok();
    if at.is_none() && !placed_by {
        return None;
    }
    let mut v = json!({
        "id": format!("t{n}"), "text": text,
        "at": at.map_or([0.0, 0.0], xy), "height": mm(height),
    });
    // Without a stored angle, a box's first line runs along the text's
    // baseline: the file's box starts at the text's corner.
    let angle = t["angle"]
        .as_f64()
        .or_else(|| first_line_angle(&t["definition"]["rectangleLines"], ids, entities, positions))
        .unwrap_or(0.0);
    if angle != 0.0 {
        v["angle"] = json!(angle);
    }
    // The file's height is that of the capital letters, and a text in a
    // box wraps its lines at its width.
    v["cap_height"] = json!(true);
    if t["definition"]["rectangleLines"].is_array() {
        v["wrap"] = json!(true);
    }
    // The `.ipt` decoder's measures (mitcad#414): the lines' own spacing,
    // aligned by their pens, the top at the first line's highest outline.
    if let Some(s) = t["lineSpacing"].as_f64().filter(|s| *s > 0.0) {
        v["line_spacing"] = json!(mm(s));
    }
    for (key, mitcad) in [("penAligned", "pen_align"), ("inkTop", "ink_top")] {
        if t[key] == true {
            v[mitcad] = json!(true);
        }
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
    if let Some(glyphs) = stored_glyphs(t, &v, height) {
        v["glyphs"] = glyphs;
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
                // The file's characters along a path keep their own widths
                // (mitcad#1177: the drill index's pair of ones stands as
                // far apart as two ones alone).
                v["no_kerning"] = json!(true);
                align(&mut v);
                // A text below its path is the file's text above the path
                // turned round, so its ends swap (mitcad#1177: the drill
                // index's left-aligned text ends where its circle starts).
                if !above {
                    match v["align"].as_str() {
                        None => v["align"] = json!("right"),
                        Some("right") => {
                            v.as_object_mut().map(|o| o.remove("align"));
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    Some(v)
}

/// A whole construction circle that a text runs along starts where the
/// file's circle starts as a path (`_f3d.pathStart`, mitcad#1177): the
/// model lays the text out from the circle's start. A circle that makes
/// faces keeps its start where the stored bodies have their seam.
fn start_path_circle(text: &Value, t: &Value, entities: &mut [Value]) {
    let (Some(curve), Some(start)) = (
        text["path"]["curve"].as_str(),
        t["_f3d"]["pathStart"].as_f64(),
    ) else {
        return;
    };
    let Some(circle) = entities
        .iter_mut()
        .find(|e| e["id"] == curve && e["type"] == "circle" && e["construction"] == true)
    else {
        return;
    };
    if start.abs() > 1e-9 {
        circle["start_angle"] = json!(start);
    } else {
        circle.as_object_mut().map(|o| o.remove("start_angle"));
    }
}

/// The glyph outlines a file stores with a text (SCHEMA.md `Text.glyphs`,
/// cm) as the model text's `glyphs` (mitcad#416): in units of the text's
/// height (`height`, cm), for the string, font and style of the model text
/// `v`. The model uses them while that font is not installed.
fn stored_glyphs(t: &Value, v: &Value, height: f64) -> Option<Value> {
    let list = t["glyphs"].as_array()?;
    if list.len() != v["text"].as_str()?.chars().count() || height <= 0.0 {
        return None;
    }
    let glyphs = list
        .iter()
        .map(|g| {
            let advance = g["advance"].as_f64()? / height;
            let contours: Vec<Vec<Vec<[f64; 2]>>> = match &g["contours"] {
                Value::Null => Vec::new(),
                c => serde_json::from_value(c.clone()).ok()?,
            };
            let contours: Vec<Vec<Vec<[f64; 2]>>> = contours
                .iter()
                .map(|contour| {
                    contour
                        .iter()
                        .map(|piece| {
                            piece
                                .iter()
                                .map(|p| [p[0] / height, p[1] / height])
                                .collect()
                        })
                        .collect()
                })
                .collect();
            let mut out = json!({});
            if advance != 0.0 {
                out["advance"] = json!(advance);
            }
            if !contours.is_empty() {
                out["contours"] = json!(contours);
            }
            Some(out)
        })
        .collect::<Option<Vec<Value>>>()?;
    let mut stored = json!({
        "text": v["text"],
        "font": v["font"].as_str().unwrap_or(""),
        "glyphs": glyphs,
    });
    for key in ["bold", "italic"] {
        if v[key] == true {
            stored[key] = json!(true);
        }
    }
    Some(stored)
}

/// The direction of the first of a text box's lines, start to end, as the
/// text's angle.
fn first_line_angle(
    lines: &Value,
    ids: &Ids,
    entities: &[Value],
    positions: &HashMap<u32, [f64; 2]>,
) -> Option<f64> {
    let name = ids.name(lines.as_array()?.first()?["id"].as_str()?)?;
    let e = entities.iter().find(|e| e["id"] == name.as_str())?;
    let at = |key: &str| -> Option<[f64; 2]> {
        positions
            .get(&e[key].as_str()?.strip_prefix('p')?.parse::<u32>().ok()?)
            .copied()
    };
    let (a, b) = (at("start")?, at("end")?);
    Some((b[1] - a[1]).atan2(b[0] - a[0]))
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

/// The entity ids (`p3`, `c7`) a JSON value names, outside its own `id`.
fn named_entities(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            let mut chars = s.chars();
            if matches!(chars.next(), Some('p' | 'c'))
                && s.len() > 1
                && chars.all(|c| c.is_ascii_digit())
            {
                out.push(s.clone());
            }
        }
        Value::Array(a) => a.iter().for_each(|x| named_entities(x, out)),
        Value::Object(m) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "id")
            .for_each(|(_, x)| named_entities(x, out)),
        _ => {}
    }
}

/// The points of a sketch whose stored position may differ a little from
/// where the solver puts them: those of the reference (linked) entities,
/// and of the entities the constraints tie to them (a stub whose end lies
/// on a linked line).
pub fn reference_points(
    entities: &[Value],
    constraints: &[Value],
    dimensions: &[Value],
) -> HashSet<u32> {
    let points_of = |e: &Value| {
        let mut ids = Vec::new();
        if e["type"] == "point" {
            if let Some(id) = e["id"].as_str() {
                ids.push(id.to_owned());
            }
        } else {
            named_entities(e, &mut ids);
        }
        ids
    };
    let mut near: HashSet<String> = HashSet::new();
    for e in entities.iter().filter(|e| e["reference"] == true) {
        near.extend(points_of(e));
        if let Some(id) = e["id"].as_str() {
            near.insert(id.to_owned());
        }
    }
    let mut tied: Vec<String> = Vec::new();
    for v in constraints.iter().chain(dimensions) {
        let mut ids = Vec::new();
        named_entities(v, &mut ids);
        if ids.iter().any(|i| near.contains(i)) {
            tied.extend(ids);
        }
    }
    near.extend(tied);
    // The points of the curves named, and the points themselves.
    let mut out = HashSet::new();
    for e in entities {
        let Some(id) = e["id"].as_str() else { continue };
        if !near.contains(id) {
            continue;
        }
        for p in points_of(e) {
            if let Some(n) = p.strip_prefix('p').and_then(|n| n.parse::<u32>().ok()) {
                out.insert(n);
            }
        }
    }
    out
}

/// The ids of the points of reference (linked) curves that hold the
/// sketch's own curves: a point a constraint or dimension ties to an own
/// curve (not reference, not a point) or to one of its points, such as
/// a linked arc's end that an own circle passes through. Solved with the
/// reference curves free, these stay where the file has them where the
/// reference curves' inconsistency can be taken up by their other points
/// (`Importer`'s sketch levels): otherwise the solve shares it with the
/// own curves, and a circle through such an end comes out micrometres off
/// the file's (round 11 of mitcad#343).
pub fn pinned_reference_points(
    entities: &[Value],
    constraints: &[Value],
    dimensions: &[Value],
) -> HashSet<String> {
    let mut reference_points: HashSet<String> = HashSet::new();
    let mut own: HashSet<String> = HashSet::new();
    for e in entities {
        if e["type"] == "point" {
            continue;
        }
        let mut ids = Vec::new();
        named_entities(e, &mut ids);
        if e["reference"] == true {
            reference_points.extend(ids);
        } else if let Some(id) = e["id"].as_str() {
            own.insert(id.to_owned());
            own.extend(ids);
        }
    }
    // Ends an own curve shares with a reference curve, and the reference
    // points the constraints tie to the own curves.
    let mut out: HashSet<String> = reference_points.intersection(&own).cloned().collect();
    for v in constraints.iter().chain(dimensions) {
        let mut ids = Vec::new();
        named_entities(v, &mut ids);
        if ids.iter().any(|i| own.contains(i)) {
            out.extend(
                ids.into_iter()
                    .filter(|i| i.starts_with('p') && reference_points.contains(i)),
            );
        }
    }
    out
}

/// How far the solved points are from where the file had them, mm,
/// where the points of the reference entities may be off by `slack`
/// (the file's linked curves can be a few hundredths of a millimetre
/// inconsistent among themselves: an arc whose end lies off its circle).
pub fn position_error_slack(
    expected: &[(u32, [f64; 2])],
    solved: &std::collections::BTreeMap<mitcad_model::EntityUid, [f64; 2]>,
    reference: &HashSet<u32>,
    slack: f64,
) -> f64 {
    expected
        .iter()
        .filter_map(|(n, at)| {
            let s = solved.get(&mitcad_model::EntityUid(*n))?;
            let d = (s[0] - at[0]).hypot(s[1] - at[1]);
            Some(if reference.contains(n) && d <= slack {
                0.0
            } else {
                d
            })
        })
        .fold(0.0, f64::max)
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

#[cfg(test)]
mod reference_slack_tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn points_tied_to_reference_curves_may_move_a_little() {
        let entities = vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0]}),
            json!({"id": "p2", "type": "point", "at": [1.0, 0.0]}),
            json!({"id": "p3", "type": "point", "at": [0.5, 0.0]}),
            json!({"id": "p4", "type": "point", "at": [5.0, 5.0]}),
            json!({"id": "p5", "type": "point", "at": [6.0, 5.0]}),
            json!({"id": "c6", "type": "line", "start": "p1", "end": "p2",
                   "reference": true, "fixed": true}),
            json!({"id": "c7", "type": "line", "start": "p3", "end": "p4"}),
            json!({"id": "c8", "type": "line", "start": "p4", "end": "p5"}),
        ];
        // p3 lies on the reference line c6; nothing ties c8 to it.
        let constraints =
            vec![json!({"id": "k0", "type": "coincident", "point": "p3", "entity": "c6"})];
        let near = reference_points(&entities, &constraints, &[]);
        let expected = |n: u32| near.contains(&n);
        assert!(expected(1) && expected(2) && expected(3));
        assert!(!expected(4) && !expected(5), "{near:?}");
        // The solver moved p3 by 0.01 mm and p5 by the same: only the
        // point tied to the reference curve is forgiven.
        let positions = vec![(3, [0.5, 0.0]), (5, [6.0, 5.0])];
        let solved = BTreeMap::from([
            (mitcad_model::EntityUid(3), [0.5, 0.01]),
            (mitcad_model::EntityUid(5), [6.0, 5.01]),
        ]);
        let error = position_error_slack(&positions, &solved, &near, 0.05);
        assert!((error - 0.01).abs() < 1e-12, "{error}");
        // Beyond the slack nothing is forgiven.
        let far = BTreeMap::from([(mitcad_model::EntityUid(3), [0.5, 0.2])]);
        let error = position_error_slack(&positions[..1], &far, &near, 0.05);
        assert!((error - 0.2).abs() < 1e-12, "{error}");
    }

    #[test]
    fn reference_points_holding_own_curves_are_pinned() {
        let entities = vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0]}),
            json!({"id": "p2", "type": "point", "at": [1.0, 0.0]}),
            json!({"id": "p3", "type": "point", "at": [2.0, 0.0]}),
            json!({"id": "p4", "type": "point", "at": [0.0, 1.0]}),
            json!({"id": "p5", "type": "point", "at": [3.0, 0.0]}),
            json!({"id": "p6", "type": "point", "at": [9.0, 9.0]}),
            json!({"id": "c7", "type": "arc", "center": "p1", "start": "p2", "end": "p3",
                   "reference": true, "fixed": true}),
            json!({"id": "c8", "type": "line", "start": "p3", "end": "p5",
                   "reference": true, "fixed": true}),
            json!({"id": "c9", "type": "circle", "center": "p4", "radius": 2.0}),
            json!({"id": "c10", "type": "line", "start": "p5", "end": "p6"}),
        ];
        // p2 lies on the own circle c9; p1 is held at a distance from its
        // centre; p5 is an end the own line c10 shares; p3 holds nothing.
        let constraints =
            vec![json!({"id": "k0", "type": "coincident", "point": "p2", "entity": "c9"})];
        let dimensions = vec![json!({"id": "k1", "type": "distance", "a": "p1", "b": "p4"})];
        let pinned = pinned_reference_points(&entities, &constraints, &dimensions);
        let mut got: Vec<&str> = pinned.iter().map(String::as_str).collect();
        got.sort_unstable();
        assert_eq!(got, ["p1", "p2", "p5"]);
    }
}
