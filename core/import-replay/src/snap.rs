// SPDX-License-Identifier: MIT
//! Linked curves on the model's own geometry.
//!
//! A curve the file links to geometry outside the sketch (a projected edge
//! or face) comes in with the numbers the file stored: single-precision
//! values of the length unit, about 1e-6 mm off the geometry they follow
//! (more on a large body). The edge itself is exact in the replay, so a
//! feature that meets it (a tool face tangent to the projected circle, a
//! join along the wall the circle follows) found the two apart by that
//! little: the boolean produced an invalid shape, or the join no longer
//! touched.
//!
//! [`snap_linked`] puts each linked line, circle, arc and ellipse that an
//! edge of the replay's bodies projects onto, within [`SNAP`], on that
//! projection: the edge the projection rule picks for it (`linked.rs`,
//! mitcad#225) where there is one, else the first found. What is not
//! found stays as it was.
//!
//! A snap must not change the sketch's regions (mitcad#227): a curve is
//! moved only when the curves it meets, and how (a crossing, an end on a
//! curve), stay those of the file's numbers for it and for every curve
//! that moves with it (one that shares a point, or whose point the file
//! holds coincident). Corners two curves share and ends the file holds on
//! a curve are the solver's: they are not counted. A curve whose contacts
//! would change stays where the file stored it, and the sketch's note
//! counts it.

use std::borrow::Cow;
use std::collections::HashMap;
use std::f64::consts::TAU;

use mitcad_model::Curve3;
use mitcad_model::SketchFrame;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::sketch::geometry::{Curve2, angle_after};
use mitcad_model::sketch::intersect::intersections;
use mitcad_model::sketch::project::{Projected, project};
use mitcad_vec3::vec2::distance as dist;
use mitcad_vec3::{cross, dot, sub, unit_div};
use serde_json::{Value, json};

use crate::sketch::SketchParts;

/// How far (mm) a linked curve's stored geometry may lie from an edge's
/// projection to be that edge's: far above the stored numbers' rounding,
/// far below any distance a design means.
pub(crate) const SNAP: f64 = 2e-3;

/// How far (mm) two curves may lie apart to meet, in the file's numbers
/// and after a snap: above the stored numbers' rounding, well below
/// [`SNAP`].
const CONTACT: f64 = 1e-4;

type P = [f64; 2];

/// What a snap did: curves put on the model's edges, and curves left where
/// the file stored them because the snap would have changed the sketch's
/// regions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Snapped {
    pub moved: usize,
    pub left: usize,
}

/// An edge's curve as a model curve for projection (lines, circles and
/// arcs; the rest is no projection this module matches).
fn model_curve(g: &CurveGeometry) -> Option<Curve3> {
    match g {
        CurveGeometry::Line { start, end } => Some(Curve3::Line {
            start: *start,
            end: *end,
        }),
        CurveGeometry::Circle {
            center,
            normal,
            radius,
            start,
            end,
        } => {
            let normal = unit_div(*normal)?;
            let x_axis = unit_div(sub(*start, *center))?;
            let y_axis = cross(normal, x_axis);
            let closed = {
                let d = sub(*end, *start);
                dot(d, d).sqrt() <= 1e-9 * radius.max(1.0)
            };
            let to = sub(*end, *center);
            let angle = dot(to, y_axis).atan2(dot(to, x_axis)).rem_euclid(TAU);
            Some(Curve3::Conic {
                center: *center,
                normal,
                x_axis,
                major: *radius,
                minor: *radius,
                start: 0.0,
                end: if closed { TAU } else { angle },
                closed,
            })
        }
        CurveGeometry::Other { .. } => None,
    }
}

/// The curves the edges project to in the sketch's frame (mm), each with
/// its edge's index, only those that come within `margin` of the box
/// (`low`, `high`).
fn projections(
    edges: &[CurveGeometry],
    frame: &SketchFrame,
    low: P,
    high: P,
    margin: f64,
) -> Vec<(usize, Curve2)> {
    // Edges whose reach misses the window are left out before projecting;
    // their projections would miss it too.
    let mut out = Vec::new();
    for (i, edge) in edges.iter().enumerate() {
        if reach(edge, frame).is_some_and(|(lo, hi)| misses(lo, hi, low, high, margin)) {
            continue;
        }
        let Some(curve) = model_curve(edge) else {
            continue;
        };
        for p in project(std::slice::from_ref(&curve), frame) {
            let Projected::Curve(c) = p else { continue };
            let (lo, hi) = c.bounds();
            if misses(lo, hi, low, high, margin) {
                continue;
            }
            out.push((i, c));
        }
    }
    out
}

/// A box in the sketch's coordinates that holds an edge's projection: a
/// line's projected ends, a circle's projected centre with its radius
/// around (a projection brings no point farther from the centre); None for
/// the curves no projection here matches.
fn reach(edge: &CurveGeometry, frame: &SketchFrame) -> Option<(P, P)> {
    let in_sketch = |p: &[f64; 3]| -> P {
        let d = sub(*p, frame.origin);
        [dot(d, frame.x_axis), dot(d, frame.y_axis)]
    };
    match edge {
        CurveGeometry::Line { start, end } => {
            let (a, b) = (in_sketch(start), in_sketch(end));
            Some((
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            ))
        }
        CurveGeometry::Circle { center, radius, .. } => {
            let c = in_sketch(center);
            let r = radius.abs();
            Some(([c[0] - r, c[1] - r], [c[0] + r, c[1] + r]))
        }
        CurveGeometry::Other { .. } => None,
    }
}

/// Whether the box (`lo`, `hi`) misses the box (`low`, `high`) grown by
/// `margin`.
fn misses(lo: P, hi: P, low: P, high: P, margin: f64) -> bool {
    hi[0] < low[0] - margin
        || lo[0] > high[0] + margin
        || hi[1] < low[1] - margin
        || lo[1] > high[1] + margin
}

/// The edges' projections that [`projections`] gives, found for one curve
/// at a time (mitcad#119): only the edges whose [`reach`] comes near the
/// curve are projected, each once. A sketch over a body of 115 000 edges
/// projected most of them for each of its linked curves' window.
struct Projector<'e> {
    edges: &'e [CurveGeometry],
    frame: &'e SketchFrame,
    /// The window [`projections`] keeps projections in: low, high, margin.
    window: (P, P, f64),
    /// The edges with a reach in the window, by the reach's low x.
    by_x: Vec<(f64, usize, (P, P))>,
    /// Each edge's projections in the window, once projected.
    done: HashMap<usize, Vec<Curve2>>,
}

impl<'e> Projector<'e> {
    fn new(
        edges: &'e [CurveGeometry],
        frame: &'e SketchFrame,
        low: P,
        high: P,
        margin: f64,
    ) -> Self {
        let mut by_x: Vec<(f64, usize, (P, P))> = edges
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let (lo, hi) = reach(e, frame)?;
                (!misses(lo, hi, low, high, margin)).then_some((lo[0], i, (lo, hi)))
            })
            .collect();
        by_x.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        Projector {
            edges,
            frame,
            window: (low, high, margin),
            by_x,
            done: HashMap::new(),
        }
    }

    /// How many edges reach the window.
    fn reaching(&self) -> usize {
        self.by_x.len()
    }

    /// The projections in the window whose bounds come within `pad` of the
    /// box (`lo`, `hi`), each with its edge's index, in the order of
    /// [`projections`] (by edge, then as projected).
    fn near(&mut self, lo: P, hi: P, pad: f64) -> Vec<(usize, Curve2)> {
        let end = self.by_x.partition_point(|(x, _, _)| *x <= hi[0] + pad);
        let mut found: Vec<usize> = self.by_x[..end]
            .iter()
            .filter(|(_, _, (rlo, rhi))| !misses(*rlo, *rhi, lo, hi, pad))
            .map(|(_, i, _)| *i)
            .collect();
        found.sort_unstable();
        let (low, high, margin) = self.window;
        let mut out = Vec::new();
        for i in found {
            let curves = self.done.entry(i).or_insert_with(|| {
                let Some(curve) = model_curve(&self.edges[i]) else {
                    return Vec::new();
                };
                project(std::slice::from_ref(&curve), self.frame)
                    .into_iter()
                    .filter_map(|p| match p {
                        Projected::Curve(c) => {
                            let (clo, chi) = c.bounds();
                            (!misses(clo, chi, low, high, margin)).then_some(c)
                        }
                        Projected::Point(_) => None,
                    })
                    .collect()
            });
            for c in curves.iter() {
                let (clo, chi) = c.bounds();
                if !misses(clo, chi, lo, hi, pad) {
                    out.push((i, c.clone()));
                }
            }
        }
        out
    }
}

/// What a snapped curve changes: points to put elsewhere (by entity id)
/// and the new radii (circles) or minor radii (ellipses).
#[derive(Default)]
struct Change {
    points: Vec<(String, P)>,
    radius: Option<f64>,
    /// The edge whose projection it is (an index into the edges).
    edge: usize,
}

/// The entities of a sketch by id, with the positions of its points.
struct View<'a> {
    entities: &'a [Value],
    index: HashMap<&'a str, usize>,
}

impl<'a> View<'a> {
    fn new(entities: &'a [Value]) -> Self {
        let index = entities
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((e["id"].as_str()?, i)))
            .collect();
        View { entities, index }
    }

    fn point(&self, id: &Value) -> Option<P> {
        let e = &self.entities[*self.index.get(id.as_str()?)?];
        let at = e["at"].as_array()?;
        Some([at.first()?.as_f64()?, at.get(1)?.as_f64()?])
    }
}

/// The curve a round or straight linked entity stands for, and what to
/// change for the projection `to` it matches; None where it does not.
fn fit(entity: &Value, view: &View, projected: &[Curve2]) -> Option<Change> {
    let near = |a: P, b: P| dist(a, b) <= SNAP;
    let id = |key: &str| entity[key].as_str().map(str::to_owned);
    match entity["type"].as_str()? {
        "line" => {
            let (a, b) = (view.point(&entity["start"])?, view.point(&entity["end"])?);
            projected.iter().find_map(|c| match c {
                Curve2::Line { a: x, b: y } if near(a, *x) && near(b, *y) => Some(Change {
                    points: vec![(id("start")?, *x), (id("end")?, *y)],
                    radius: None,
                    edge: 0,
                }),
                Curve2::Line { a: x, b: y } if near(a, *y) && near(b, *x) => Some(Change {
                    points: vec![(id("start")?, *y), (id("end")?, *x)],
                    radius: None,
                    edge: 0,
                }),
                _ => None,
            })
        }
        "circle" => {
            let center = view.point(&entity["center"])?;
            let radius = entity["radius"].as_f64()?;
            projected.iter().find_map(|c| match c {
                Curve2::Circle {
                    center: x,
                    radius: r,
                    ..
                } if near(center, *x) && (radius - r).abs() <= SNAP => Some(Change {
                    points: vec![(id("center")?, *x)],
                    radius: Some(*r),
                    edge: 0,
                }),
                _ => None,
            })
        }
        "arc" => {
            let center = view.point(&entity["center"])?;
            let (a, b) = (view.point(&entity["start"])?, view.point(&entity["end"])?);
            projected.iter().find_map(|c| {
                let Curve2::Arc {
                    center: x,
                    radius: r,
                    ..
                } = c
                else {
                    return None;
                };
                let (s, e) = c.ends()?;
                let radius = dist(center, a);
                if !(near(center, *x) && (radius - r).abs() <= SNAP && near(a, s) && near(b, e)) {
                    return None;
                }
                Some(Change {
                    points: vec![(id("center")?, *x), (id("start")?, s), (id("end")?, e)],
                    radius: None,
                    edge: 0,
                })
            })
        }
        kind @ ("ellipse" | "elliptical_arc") => {
            let center = view.point(&entity["center"])?;
            let major = view.point(&entity["major"])?;
            let minor = entity["minor_radius"].as_f64()?;
            let length = dist(center, major);
            let direction = [
                (major[0] - center[0]) / length,
                (major[1] - center[1]) / length,
            ];
            projected.iter().find_map(|c| {
                let (x, big, small, rotation) = match c {
                    Curve2::Ellipse {
                        center,
                        major,
                        minor,
                        rotation,
                    }
                    | Curve2::EllipticalArc {
                        center,
                        major,
                        minor,
                        rotation,
                        ..
                    } => (*center, *major, *minor, *rotation),
                    _ => return None,
                };
                // The major axis points either way along the line.
                let axis = [rotation.cos(), rotation.sin()];
                let along = direction[0] * axis[0] + direction[1] * axis[1];
                if !(near(center, x)
                    && (length - big).abs() <= SNAP
                    && (minor - small).abs() <= SNAP
                    && (along.abs() - 1.0).abs() <= 1e-6)
                {
                    return None;
                }
                let sign = along.signum();
                let tip = [x[0] + sign * axis[0] * big, x[1] + sign * axis[1] * big];
                let mut points = vec![(id("center")?, x), (id("major")?, tip)];
                if kind == "elliptical_arc" {
                    let (s, e) = c.ends()?;
                    let (a, b) = (view.point(&entity["start"])?, view.point(&entity["end"])?);
                    if !(near(a, s) && near(b, e)) {
                        return None;
                    }
                    points.push((id("start")?, s));
                    points.push((id("end")?, e));
                }
                Some(Change {
                    points,
                    radius: Some(small),
                    edge: 0,
                })
            })
        }
        _ => None,
    }
}

/// The box of a curve entity's points (its ends, centre and major point);
/// None where it has none.
fn entity_box(entity: &Value, view: &View) -> Option<(P, P)> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for key in ["start", "end", "center", "major"] {
        if let Some(p) = view.point(&entity[key]) {
            for k in 0..2 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
            }
        }
    }
    // A circle reaches its radius from the centre.
    if let Some(r) = entity["radius"]
        .as_f64()
        .filter(|_| entity["type"] == "circle")
    {
        low = [low[0] - r, low[1] - r];
        high = [high[0] + r, high[1] + r];
    }
    (low[0].is_finite() && high[0].is_finite()).then_some((low, high))
}

/// The box of the points of `entities` in sketch coordinates; None when it
/// has none.
fn points_box(entities: &[Value], view: &View) -> Option<(P, P)> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for e in entities {
        if e["type"] == "point"
            && let Some(p) = view.point(&e["id"])
        {
            for k in 0..2 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
            }
        }
    }
    (low[0].is_finite() && high[0].is_finite()).then_some((low, high))
}

/// For each linked curve of `parts` (its index in the entities), every
/// edge (an index into `edges`, in the space of the sketch's `frame`)
/// whose projection it lies on within [`SNAP`]: the geometry the curve may
/// follow (mitcad#225). Curves no edge projects onto are left out.
pub(crate) fn sources(
    parts: &SketchParts,
    frame: &SketchFrame,
    edges: &[CurveGeometry],
) -> Vec<(usize, Vec<usize>)> {
    let view = View::new(&parts.entities);
    let Some((low, high)) = points_box(&parts.entities, &view) else {
        return Vec::new();
    };
    let margin = dist(low, high) + 1.0;
    // Each edge's projection near the curves, with the edge's index; for
    // each curve only those whose boxes come near its points, which alone
    // can fit it (mitcad#119: a sketch on a body of 115 000 edges fitted
    // each linked curve to every projection, most of a large design's
    // sketch time). The pad is looser than the fit's, so the same edges are
    // found in the same order.
    let mut projector = Projector::new(edges, frame, low, high, margin);
    let mut all: Option<Vec<(usize, Curve2)>> = None;
    let pad = 2.0 * SNAP;
    let mut out = Vec::new();
    for (i, e) in parts.entities.iter().enumerate() {
        if e["reference"] != json!(true) || e["type"].as_str().is_none_or(|t| t == "point") {
            continue;
        }
        let near = match entity_box(e, &view) {
            Some((lo, hi)) => projector.near(lo, hi, pad),
            None => all
                .get_or_insert_with(|| projections(edges, frame, low, high, margin))
                .clone(),
        };
        let mut found: Vec<usize> = near
            .iter()
            .filter(|(_, c)| fit(e, &view, std::slice::from_ref(c)).is_some())
            .map(|(edge, _)| *edge)
            .collect();
        found.dedup();
        if !found.is_empty() {
            out.push((i, found));
        }
    }
    out
}

/// For the trace: how far the nearest projected curve of the entity's kind
/// is (centre and radius, or ends).
fn nearest(entity: &Value, view: &View, projected: &[Curve2]) -> String {
    let mut best: Option<(f64, String)> = None;
    let mut offer = |d: f64, text: String| {
        if best.as_ref().is_none_or(|(b, _)| d < *b) {
            best = Some((d, text));
        }
    };
    match entity["type"].as_str() {
        Some("circle" | "arc") => {
            let (Some(center), Some(radius)) = (
                view.point(&entity["center"]),
                entity["radius"].as_f64().or_else(|| {
                    Some(dist(
                        view.point(&entity["center"])?,
                        view.point(&entity["start"])?,
                    ))
                }),
            ) else {
                return String::new();
            };
            for c in projected {
                let (x, r) = match c {
                    Curve2::Circle { center, radius, .. } | Curve2::Arc { center, radius, .. } => {
                        (*center, *radius)
                    }
                    _ => continue,
                };
                let d = dist(center, x).max((radius - r).abs());
                offer(
                    d,
                    format!(
                        "center off by {:.2e}, radius {:.4} against {:.4}",
                        dist(center, x),
                        radius,
                        r
                    ),
                );
            }
        }
        Some("line") => {
            let (Some(a), Some(b)) = (view.point(&entity["start"]), view.point(&entity["end"]))
            else {
                return String::new();
            };
            for c in projected {
                if let Curve2::Line { a: x, b: y } = c {
                    let d = (dist(a, *x) + dist(b, *y)).min(dist(a, *y) + dist(b, *x));
                    offer(d, format!("ends off by {d:.2e}"));
                }
            }
        }
        _ => {}
    }
    best.map(|(_, t)| t).unwrap_or_default()
}

/// Puts the linked curves of `parts` that an edge projects onto on that
/// projection; how many it moved. `edges` are the replay's edges in the
/// space of the sketch's `frame` (mm).
#[cfg(test)]
pub(crate) fn snap_linked(
    parts: &mut SketchParts,
    frame: &SketchFrame,
    edges: &[CurveGeometry],
) -> usize {
    snap_linked_keeping(parts, frame, edges, &HashMap::new(), &[]).moved
}

/// [`snap_linked`], with points already placed (`kept`, by entity id: the
/// ends of curves put on the edges of the sketch's face, where two of them
/// meet): a curve that would move one of them elsewhere stays as it was.
/// `preferred` names, by entity index, the edge (an index into `edges`)
/// the projection rule picks for a curve: its projection is tried first,
/// so that the snap and the projection agree on the edge.
pub(crate) fn snap_linked_keeping(
    parts: &mut SketchParts,
    frame: &SketchFrame,
    edges: &[CurveGeometry],
    kept: &HashMap<String, P>,
    preferred: &[(usize, usize)],
) -> Snapped {
    let linked: Vec<usize> = parts
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            e["reference"] == json!(true) && e["type"].as_str().is_some_and(|t| t != "point")
        })
        .map(|(i, _)| i)
        .collect();
    if linked.is_empty() || edges.is_empty() {
        return Snapped::default();
    }
    // The box of the linked curves' points, to leave out far edges.
    let view = View::new(&parts.entities);
    let Some((low, high)) = points_box(&parts.entities, &view) else {
        return Snapped::default();
    };
    let extent = dist(low, high);
    // The projections near each linked curve, projected as asked
    // (mitcad#119; [`Projector`]).
    let mut projector = Projector::new(edges, frame, low, high, extent + 1.0);
    if crate::tracing() {
        eprintln!(
            "import:   snap: {} linked curves, {} edges, {} reaching near",
            linked.len(),
            edges.len(),
            projector.reaching()
        );
    }
    if projector.reaching() == 0 {
        return Snapped::default();
    }
    let mut all: Option<Vec<(usize, Curve2)>> = None;
    // Each linked curve's fits, the projection rule's edge first: the first
    // one that keeps the points already placed and the regions is taken.
    let mut changes: Vec<(usize, Option<usize>, Vec<Change>)> = Vec::new();
    for &i in &linked {
        let first = preferred
            .iter()
            .find(|(entity, _)| *entity == i)
            .map(|(_, edge)| *edge);
        // Only the projections that come near the curve's points.
        let near = match entity_box(&parts.entities[i], &view) {
            Some((lo, hi)) => projector.near(lo, hi, SNAP),
            None => all
                .get_or_insert_with(|| projections(edges, frame, low, high, extent + 1.0))
                .clone(),
        };
        let order = near
            .iter()
            .filter(|(k, _)| Some(*k) == first)
            .chain(near.iter().filter(|(k, _)| Some(*k) != first));
        // (Two edges' projections may differ by nanometres: each is an
        // option of its own, since the points already placed are matched
        // to that closeness.)
        let mut options: Vec<Change> = Vec::new();
        for (k, c) in order {
            if let Some(mut change) = fit(&parts.entities[i], &view, std::slice::from_ref(c)) {
                change.edge = *k;
                options.push(change);
            }
        }
        if !options.is_empty() {
            changes.push((i, first, options));
        } else if crate::tracing() {
            let projected: Vec<Curve2> = all
                .get_or_insert_with(|| projections(edges, frame, low, high, extent + 1.0))
                .iter()
                .map(|(_, c)| c.clone())
                .collect();
            eprintln!(
                "import:   snap: {} {} found no edge: {}",
                parts.entities[i]["type"],
                parts.entities[i]["id"],
                nearest(&parts.entities[i], &view, &projected)
            );
        }
    }
    // A point two curves share goes where the first put it.
    let mut placed: HashMap<String, P> = kept.clone();
    // Where the solver will take the points the file holds coincident
    // with a moved one, or on a moved curve: for the check of the regions
    // only (the solver moves them itself; a later snap may put them
    // elsewhere).
    let mut followed: HashMap<String, P> = HashMap::new();
    let mut moved = 0;
    let mut left = 0;
    let mut radii: Vec<(usize, f64)> = Vec::new();
    let started = std::time::Instant::now();
    let contacts = Contacts::new(parts, &view);
    let mut checking = std::time::Duration::ZERO;
    for (i, first, options) in changes {
        let mut why_left: Option<String> = None;
        for change in options {
            if change
                .points
                .iter()
                .any(|(id, at)| placed.get(id).is_some_and(|p| dist(*p, *at) > 1e-9))
            {
                continue;
            }
            // The move, with the points the file holds coincident with the
            // moved ones (the solver takes them along).
            let mut after = followed.clone();
            after.extend(placed.iter().map(|(k, v)| (k.clone(), *v)));
            let mut follows: Vec<(String, P)> = Vec::new();
            for (id, at) in &change.points {
                let delta = match view.point(&json!(id)) {
                    Some(was) => [at[0] - was[0], at[1] - was[1]],
                    None => [0.0, 0.0],
                };
                after.insert(id.clone(), *at);
                for other in contacts.coincident_with(id) {
                    if other != id
                        && !after.contains_key(other)
                        && let Some(was) = view.point(&json!(other))
                    {
                        let to = [was[0] + delta[0], was[1] + delta[1]];
                        after.insert(other.clone(), to);
                        follows.push((other.clone(), to));
                    }
                }
            }
            let radius = change.radius.map(|r| (i, r));
            // The points the file holds on the curve go onto it where it
            // is now (the solver puts them there); a point off the curve
            // as stored (on a line's extension) is left to the solver.
            let moved_curve = curve_of(
                &parts.entities[i],
                &|id| {
                    id.as_str()
                        .and_then(|s| after.get(s).copied())
                        .or_else(|| view.point(id))
                },
                radius.map(|(_, r)| r),
            );
            if let Some(curve) = &moved_curve {
                for point in
                    contacts.held_on_curve(parts.entities[i]["id"].as_str().unwrap_or_default())
                {
                    if after.contains_key(&point) {
                        continue;
                    }
                    if let Some(was) = view.point(&json!(point)) {
                        let (t, _) = curve.closest(was);
                        let to = curve.point(t);
                        if dist(was, to) <= SNAP + CONTACT {
                            after.insert(point.clone(), to);
                            follows.push((point, to));
                        }
                    }
                }
            }
            let check = std::time::Instant::now();
            let touched: Vec<&str> = change
                .points
                .iter()
                .map(|(id, _)| id.as_str())
                .chain(follows.iter().map(|(id, _)| id.as_str()))
                .collect();
            let changed = contacts.changed_by(&after, &touched, &radii, radius);
            checking += check.elapsed();
            if let Some(why) = changed {
                why_left.get_or_insert(why);
                continue;
            }
            for (id, at) in follows {
                followed.insert(id, at);
            }
            if crate::tracing() {
                let far = change
                    .points
                    .iter()
                    .filter_map(|(id, at)| Some(dist(*at, view.point(&json!(id))?)))
                    .fold(0.0, f64::max);
                eprintln!(
                    "import:   snap: {} onto edge {}{} (moved {:.1e} mm)",
                    parts.entities[i]["id"],
                    change.edge,
                    if Some(change.edge) == first {
                        ", the projection rule's"
                    } else {
                        ""
                    },
                    far
                );
            }
            for (id, at) in change.points {
                placed.insert(id, at);
            }
            if let Some(r) = radius {
                radii.push(r);
            }
            moved += 1;
            why_left = None;
            break;
        }
        if let Some(why) = why_left {
            if crate::tracing() {
                eprintln!(
                    "import:   snap: {} {} left where the file stored it: a snap would change the sketch's regions ({why})",
                    parts.entities[i]["type"], parts.entities[i]["id"]
                );
            }
            left += 1;
        }
    }
    if crate::tracing() {
        eprintln!(
            "import:   snap: {moved} moved, {left} left; the regions checked in {:.2} s of {:.2} s",
            checking.as_secs_f64(),
            started.elapsed().as_secs_f64()
        );
    }
    for e in &mut parts.entities {
        let Some(id) = e["id"].as_str() else { continue };
        if let Some(at) = placed.get(id) {
            e["at"] = json!(at);
        }
    }
    for (i, r) in radii {
        let key = if parts.entities[i]["type"] == "circle" {
            "radius"
        } else {
            "minor_radius"
        };
        parts.entities[i][key] = json!(r);
    }
    // The positions the solved sketch is checked against.
    for (n, at) in &mut parts.positions {
        if let Some(p) = placed.get(&format!("p{n}")) {
            *at = *p;
        }
    }
    Snapped { moved, left }
}

/// The curve an entity is, with its points at `at` (the moved positions
/// first, then the stored ones) and its radius or minor radius `radius`
/// where one replaces the stored; None for a kind no contact is counted
/// for (splines, texts).
pub(crate) fn curve_of(
    entity: &Value,
    at: &dyn Fn(&Value) -> Option<P>,
    radius: Option<f64>,
) -> Option<Curve2> {
    let sub = |a: P, b: P| [a[0] - b[0], a[1] - b[1]];
    Some(match entity["type"].as_str()? {
        "line" => Curve2::Line {
            a: at(&entity["start"])?,
            b: at(&entity["end"])?,
        },
        "circle" => Curve2::Circle {
            center: at(&entity["center"])?,
            radius: radius.or_else(|| entity["radius"].as_f64())?,
            start: entity["start_angle"].as_f64().unwrap_or(0.0),
        },
        "arc" => {
            let (c, a, b) = (
                at(&entity["center"])?,
                at(&entity["start"])?,
                at(&entity["end"])?,
            );
            let (va, vb) = (sub(a, c), sub(b, c));
            let start = va[1].atan2(va[0]);
            Curve2::Arc {
                center: c,
                radius: va[0].hypot(va[1]),
                start,
                end: angle_after(start, vb[1].atan2(vb[0])),
            }
        }
        kind @ ("ellipse" | "elliptical_arc") => {
            let (c, m) = (at(&entity["center"])?, at(&entity["major"])?);
            let minor = radius.or_else(|| entity["minor_radius"].as_f64())?;
            let axis = sub(m, c);
            let major = axis[0].hypot(axis[1]);
            let rotation = axis[1].atan2(axis[0]);
            if kind == "ellipse" {
                Curve2::Ellipse {
                    center: c,
                    major,
                    minor,
                    rotation,
                }
            } else {
                let u = [axis[0] / major, axis[1] / major];
                let param = |p: P| {
                    let d = sub(p, c);
                    ((-d[0] * u[1] + d[1] * u[0]) / minor)
                        .atan2((d[0] * u[0] + d[1] * u[1]) / major)
                };
                let start = param(at(&entity["start"])?);
                Curve2::EllipticalArc {
                    center: c,
                    major,
                    minor,
                    rotation,
                    start,
                    end: angle_after(start, param(at(&entity["end"])?)),
                }
            }
        }
        _ => return None,
    })
}

/// How a curve meets another: a crossing inside both, or an end of one
/// curve on the other (`end_of_first`), where the file does not hold that
/// end on the curve. Corners two curves share (an end on an end) are not
/// contacts: the solver keeps them.
type Contact = (usize, u8);

/// The sketch's curves as they meet, for the check that a snap keeps the
/// regions.
struct Contacts<'a> {
    entities: &'a [Value],
    /// The entities by id, with the stored positions.
    view: View<'a>,
    /// The indices of the curves contacts are counted for (not
    /// construction, of a kind [`curve_of`] makes).
    curves: Vec<usize>,
    /// Each curve as stored, with its box, by entity index (None for the
    /// rest).
    stored: Vec<Option<(Curve2, (P, P))>>,
    /// The point ids of each curve, by entity index.
    points: Vec<Vec<String>>,
    /// Groups of points the file holds coincident, by point id.
    groups: HashMap<String, usize>,
    /// The points of each group.
    members: Vec<Vec<String>>,
    /// Points the file holds on a curve: (point id, curve id).
    on_curve: Vec<(String, String)>,
}

impl<'a> Contacts<'a> {
    fn new(parts: &'a SketchParts, view: &View<'a>) -> Self {
        let entities = &parts.entities[..];
        let stored: Vec<Option<(Curve2, (P, P))>> = entities
            .iter()
            .map(|e| {
                if e["construction"] == json!(true) {
                    return None;
                }
                let c = curve_of(e, &|id| view.point(id), None)?;
                let b = c.bounds();
                Some((c, b))
            })
            .collect();
        let curves = (0..entities.len())
            .filter(|i| stored[*i].is_some())
            .collect();
        let points = entities
            .iter()
            .map(|e| {
                ["start", "end", "center", "major"]
                    .iter()
                    .filter_map(|key| e[*key].as_str().map(str::to_owned))
                    .collect()
            })
            .collect();
        // Union-find over the coincident point pairs.
        let mut parent: HashMap<String, String> = HashMap::new();
        fn root(parent: &mut HashMap<String, String>, id: &str) -> String {
            let p = parent.get(id).cloned().unwrap_or_else(|| id.to_owned());
            if p == id {
                return p;
            }
            let r = root(parent, &p);
            parent.insert(id.to_owned(), r.clone());
            r
        }
        let mut on_curve = Vec::new();
        for c in &parts.constraints {
            if c["type"] != "coincident" {
                continue;
            }
            let (Some(point), Some(entity)) = (c["point"].as_str(), c["entity"].as_str()) else {
                continue;
            };
            if entity.starts_with('p') {
                let (a, b) = (root(&mut parent, point), root(&mut parent, entity));
                if a != b {
                    parent.insert(a, b);
                }
            } else {
                on_curve.push((point.to_owned(), entity.to_owned()));
            }
        }
        let ids: Vec<String> = entities
            .iter()
            .filter(|e| e["type"] == "point")
            .filter_map(|e| e["id"].as_str().map(str::to_owned))
            .collect();
        let mut numbers: HashMap<String, usize> = HashMap::new();
        let mut groups = HashMap::new();
        let mut members: Vec<Vec<String>> = Vec::new();
        for id in ids {
            let r = root(&mut parent, &id);
            let n = numbers.len();
            let n = *numbers.entry(r).or_insert(n);
            if n == members.len() {
                members.push(Vec::new());
            }
            members[n].push(id.clone());
            groups.insert(id, n);
        }
        Contacts {
            entities,
            view: View::new(entities),
            curves,
            stored,
            points,
            groups,
            members,
            on_curve,
        }
    }

    /// Curve `j` with its box, its points at `at` where any of them moved
    /// (`moved`) or its radius replaced, else as stored.
    fn curve(
        &self,
        j: usize,
        at: &dyn Fn(&Value) -> Option<P>,
        radius: Option<f64>,
        moved: &dyn Fn(&str) -> bool,
    ) -> Option<(Cow<'_, Curve2>, (P, P))> {
        if radius.is_none() && !self.points[j].iter().any(|p| moved(p)) {
            let (c, b) = self.stored[j].as_ref()?;
            return Some((Cow::Borrowed(c), *b));
        }
        let c = curve_of(&self.entities[j], at, radius)?;
        let b = c.bounds();
        Some((Cow::Owned(c), b))
    }

    /// The points the file holds coincident with `id` (itself included).
    fn coincident_with(&self, id: &str) -> &[String] {
        match self.groups.get(id) {
            Some(group) => &self.members[*group],
            None => &[],
        }
    }

    /// The points the file holds on curve `curve_id`.
    fn held_on_curve(&self, curve_id: &str) -> Vec<String> {
        self.on_curve
            .iter()
            .filter(|(_, c)| c == curve_id)
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// Whether the file holds point `point` on curve `curve`, or on one of
    /// its points.
    fn held_on(&self, point: &str, curve: &Value) -> bool {
        let curve_id = curve["id"].as_str().unwrap_or_default();
        if self
            .on_curve
            .iter()
            .any(|(p, c)| p == point && c == curve_id)
        {
            return true;
        }
        let group = self.groups.get(point);
        ["start", "end", "center", "major"]
            .iter()
            .filter_map(|key| curve[*key].as_str())
            .any(|p| p == point || (group.is_some() && self.groups.get(p) == group))
    }

    /// The contacts of curve `i` with the other curves, its points and
    /// radii at `at` (`moved` tells which points are not as stored); the
    /// kind tells which curve's end is on the other.
    fn of(
        &self,
        i: usize,
        at: &dyn Fn(&Value) -> Option<P>,
        radius: &dyn Fn(usize) -> Option<f64>,
        moved: &dyn Fn(&str) -> bool,
    ) -> Vec<Contact> {
        let Some((a, (alo, ahi))) = self.curve(i, at, radius(i), moved) else {
            return Vec::new();
        };
        let reach = CONTACT + SNAP;
        let mut out = Vec::new();
        for &j in &self.curves {
            if j == i {
                continue;
            }
            let Some((b, (blo, bhi))) = self.curve(j, at, radius(j), moved) else {
                continue;
            };
            if bhi[0] < alo[0] - reach
                || blo[0] > ahi[0] + reach
                || bhi[1] < alo[1] - reach
                || blo[1] > ahi[1] + reach
            {
                continue;
            }
            let ends = |c: &Curve2, p: P| -> Option<&'static str> {
                let (s, t) = c.ends()?;
                if dist(p, s) <= CONTACT {
                    Some("start")
                } else if dist(p, t) <= CONTACT {
                    Some("end")
                } else {
                    None
                }
            };
            for hit in intersections(a.as_ref(), b.as_ref(), CONTACT) {
                let end_a = ends(a.as_ref(), hit.point);
                let end_b = ends(b.as_ref(), hit.point);
                let kind = match (end_a, end_b) {
                    (None, None) => 0,
                    (Some(key), None) => {
                        let point = self.entities[i][key].as_str().unwrap_or_default();
                        if self.held_on(point, &self.entities[j]) {
                            continue;
                        }
                        1
                    }
                    (None, Some(key)) => {
                        let point = self.entities[j][key].as_str().unwrap_or_default();
                        if self.held_on(point, &self.entities[i]) {
                            continue;
                        }
                        2
                    }
                    (Some(_), Some(_)) => continue,
                };
                out.push((j, kind));
            }
        }
        out.sort_unstable();
        out
    }

    /// Whether moving the points to `after` (with the radii `radii` of
    /// earlier snaps and `radius` of this one) changes a contact of a
    /// curve this move touches (one of the points `touched`, or the
    /// radius); the first difference, for the trace.
    fn changed_by(
        &self,
        after: &HashMap<String, P>,
        touched: &[&str],
        radii: &[(usize, f64)],
        radius: Option<(usize, f64)>,
    ) -> Option<String> {
        let before_at = |id: &Value| self.view.point(id);
        let after_at = |id: &Value| {
            id.as_str()
                .and_then(|s| after.get(s).copied())
                .or_else(|| self.view.point(id))
        };
        let before_radius = |_: usize| None;
        let after_radius = |i: usize| {
            radius
                .filter(|(k, _)| *k == i)
                .or_else(|| radii.iter().find(|(k, _)| *k == i).copied())
                .map(|(_, r)| r)
        };
        let unmoved = |_: &str| false;
        let moved = |p: &str| after.contains_key(p);
        for &i in &self.curves {
            let moves = self.points[i].iter().any(|p| touched.contains(&p.as_str()))
                || radius.is_some_and(|(k, _)| k == i);
            if !moves {
                continue;
            }
            let before = self.of(i, &before_at, &before_radius, &unmoved);
            let now = self.of(i, &after_at, &after_radius, &moved);
            if before != now {
                let name = |j: usize| {
                    self.entities[j]["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                let describe = |c: &[Contact]| {
                    c.iter()
                        .map(|(j, k)| {
                            format!(
                                "{}{}",
                                name(*j),
                                ["", " (end on it)", " (its end)"][*k as usize]
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                return Some(format!(
                    "{} meets [{}], moved [{}]",
                    name(i),
                    describe(&before),
                    describe(&now)
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle_edge(center: [f64; 3], radius: f64) -> CurveGeometry {
        CurveGeometry::Circle {
            center,
            normal: [0.0, 0.0, 1.0],
            radius,
            start: [center[0] + radius, center[1], center[2]],
            end: [center[0] + radius, center[1], center[2]],
        }
    }

    fn parts(entities: Vec<Value>) -> SketchParts {
        let positions = entities
            .iter()
            .filter(|e| e["type"] == "point")
            .filter_map(|e| {
                let n: u32 = e["id"].as_str()?.trim_start_matches('p').parse().ok()?;
                let at = e["at"].as_array()?;
                Some((n, [at[0].as_f64()?, at[1].as_f64()?]))
            })
            .collect();
        SketchParts {
            entities,
            positions,
            ..SketchParts::default()
        }
    }

    #[test]
    fn a_linked_circle_goes_to_its_edge() {
        // The stored radius and centre are off by about 1.2e-6 mm.
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 65.000_001_2], "fixed": true, "reference": true}),
            json!({"id": "c2", "type": "circle", "center": "p1", "radius": 7.000_001_2,
                   "fixed": true, "reference": true}),
        ]);
        // The sketch looks down at the plane z = 20 from the XY plane's side, y flipped.
        let frame = SketchFrame {
            origin: [0.0, 0.0, 20.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        };
        let edges = vec![
            circle_edge([0.0, 65.0, 20.0], 7.0),
            circle_edge([0.0, 0.0, 0.0], 3.0),
        ];
        assert_eq!(snap_linked(&mut p, &frame, &edges), 1);
        assert_eq!(p.entities[1]["radius"], json!(7.0));
        assert_eq!(p.entities[0]["at"], json!([0.0, 65.0]));
        assert_eq!(p.positions, vec![(1, [0.0, 65.0])]);
    }

    /// A point already placed (by the face's edges) keeps its place: the
    /// linked circle that would move it stays as it was.
    #[test]
    fn a_kept_point_stays() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 65.000_001_2], "reference": true}),
            json!({"id": "c2", "type": "circle", "center": "p1", "radius": 7.000_001_2,
                   "reference": true}),
        ]);
        let edges = vec![circle_edge([0.0, 65.0, 0.0], 7.0)];
        let kept = HashMap::from([("p1".to_owned(), [0.0, 65.000_001_2])]);
        assert_eq!(
            snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &kept, &[]).moved,
            0
        );
        assert_eq!(p.entities[1]["radius"], json!(7.000_001_2));
        assert_eq!(p.entities[0]["at"], json!([0.0, 65.000_001_2]));
    }

    #[test]
    fn a_circle_apart_from_every_edge_stays() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0], "reference": true}),
            json!({"id": "c2", "type": "circle", "center": "p1", "radius": 7.01, "reference": true}),
        ]);
        let edges = vec![circle_edge([0.0, 0.0, 0.0], 7.0)];
        assert_eq!(snap_linked(&mut p, &SketchFrame::XY, &edges), 0);
        assert_eq!(p.entities[1]["radius"], json!(7.01));
    }

    #[test]
    fn a_linked_line_and_the_arc_on_its_end_share_the_point() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.000_001, 0.0], "reference": true}),
            json!({"id": "p2", "type": "point", "at": [10.0, 0.000_001], "reference": true}),
            json!({"id": "p3", "type": "point", "at": [10.0, 5.000_001], "reference": true}),
            json!({"id": "p4", "type": "point", "at": [15.0, 5.0], "reference": true}),
            json!({"id": "c5", "type": "line", "start": "p1", "end": "p2", "reference": true}),
            json!({"id": "c6", "type": "arc", "center": "p3", "start": "p2", "end": "p4", "reference": true}),
        ]);
        let edges = vec![
            CurveGeometry::Line {
                start: [0.0, 0.0, 3.0],
                end: [10.0, 0.0, 3.0],
            },
            CurveGeometry::Circle {
                center: [10.0, 5.0, 0.0],
                normal: [0.0, 0.0, 1.0],
                radius: 5.0,
                start: [10.0, 0.0, 0.0],
                end: [15.0, 5.0, 0.0],
            },
        ];
        assert_eq!(snap_linked(&mut p, &SketchFrame::XY, &edges), 2);
        assert_eq!(p.entities[0]["at"], json!([0.0, 0.0]));
        assert_eq!(p.entities[1]["at"], json!([10.0, 0.0]));
        assert_eq!(p.entities[2]["at"], json!([10.0, 5.0]));
        assert_eq!(p.entities[3]["at"], json!([15.0, 5.0]));
    }

    fn line_edge(a: [f64; 3], b: [f64; 3]) -> CurveGeometry {
        CurveGeometry::Line { start: a, end: b }
    }

    /// A drawn line ends on a linked line (a T the file does not hold with
    /// a constraint). The edge projects 1.5e-3 mm aside: snapped, the
    /// drawn line would cross the linked one instead of ending on it (a
    /// sliver region), so the linked line stays where the file stored it.
    #[test]
    fn a_snap_that_would_cross_a_drawn_line_is_left() {
        let sketch = || {
            parts(vec![
                json!({"id": "p1", "type": "point", "at": [0.0, 0.0], "reference": true}),
                json!({"id": "p2", "type": "point", "at": [20.0, 0.0], "reference": true}),
                json!({"id": "c3", "type": "line", "start": "p1", "end": "p2", "reference": true}),
                json!({"id": "p4", "type": "point", "at": [10.0, 0.000_001]}),
                json!({"id": "p5", "type": "point", "at": [10.0, 8.0]}),
                json!({"id": "c6", "type": "line", "start": "p4", "end": "p5"}),
            ])
        };
        let mut p = sketch();
        let edges = vec![line_edge([0.0, 0.0015, 0.0], [20.0, 0.0015, 0.0])];
        let done = snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[]);
        assert_eq!((done.moved, done.left), (0, 1));
        assert_eq!(p.entities[0]["at"], json!([0.0, 0.0]));
        // The edge on the other side opens a gap under the drawn line's
        // end: the region would no longer close, so the line stays too.
        let mut p = sketch();
        let edges = vec![line_edge([0.0, -0.0015, 0.0], [20.0, -0.0015, 0.0])];
        let done = snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[]);
        assert_eq!((done.moved, done.left), (0, 1));
        // An edge along the line: the drawn line still ends on it, so the
        // snap goes ahead.
        let mut p = sketch();
        let edges = vec![line_edge([0.001, 0.0, 0.0], [20.001, 0.0, 0.0])];
        let done = snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[]);
        assert_eq!((done.moved, done.left), (1, 0));
        assert_eq!(p.entities[0]["at"], json!([0.001, 0.0]));
        // With the file holding the drawn line's end on the linked one,
        // the solver keeps it there: the snap goes ahead.
        let mut p = sketch();
        p.constraints
            .push(json!({"type": "coincident", "point": "p4", "entity": "c3"}));
        let edges = vec![line_edge([0.0, 0.0015, 0.0], [20.0, 0.0015, 0.0])];
        let done = snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[]);
        assert_eq!((done.moved, done.left), (1, 0));
    }

    /// A corner two linked lines share through coincident points: for the
    /// check, the other line's point follows the snapped one (the corner
    /// stays a corner, the other line's crossing with a drawn line stays a
    /// crossing); the solver moves it, so it is not placed.
    #[test]
    fn coincident_points_move_along() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0], "reference": true}),
            json!({"id": "p2", "type": "point", "at": [20.0, 0.0], "reference": true}),
            json!({"id": "c3", "type": "line", "start": "p1", "end": "p2", "reference": true}),
            json!({"id": "p4", "type": "point", "at": [20.0, 0.0], "reference": true}),
            json!({"id": "p5", "type": "point", "at": [20.0, 10.0], "reference": true}),
            json!({"id": "c6", "type": "line", "start": "p4", "end": "p5", "reference": true}),
            json!({"id": "p7", "type": "point", "at": [15.0, 5.0]}),
            json!({"id": "p8", "type": "point", "at": [25.0, 5.0]}),
            json!({"id": "c9", "type": "line", "start": "p7", "end": "p8"}),
        ]);
        p.constraints
            .push(json!({"type": "coincident", "point": "p4", "entity": "p2"}));
        // Only the first line's edge is found; its end moves 1 µm, and p4
        // with it.
        let edges = vec![line_edge([0.0, 0.0, 0.0], [20.001, 0.0, 0.0])];
        let done = snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[]);
        assert_eq!((done.moved, done.left), (1, 0));
        assert_eq!(p.entities[1]["at"], json!([20.001, 0.0]));
        assert_eq!(p.entities[3]["at"], json!([20.0, 0.0]));
    }

    /// Two edges project within the snap's reach of a linked line; the
    /// one the projection rule prefers is taken, not the first.
    #[test]
    fn the_preferred_edge_is_taken_first() {
        let mut p = parts(vec![
            json!({"id": "p1", "type": "point", "at": [0.0, 0.0], "reference": true}),
            json!({"id": "p2", "type": "point", "at": [20.0, 0.0], "reference": true}),
            json!({"id": "c3", "type": "line", "start": "p1", "end": "p2", "reference": true}),
        ]);
        let edges = vec![
            line_edge([0.0, 0.001, 0.0], [20.0, 0.001, 0.0]),
            line_edge([0.0, -0.001, 0.0], [20.0, -0.001, 0.0]),
        ];
        let done =
            snap_linked_keeping(&mut p, &SketchFrame::XY, &edges, &HashMap::new(), &[(2, 1)]);
        assert_eq!(done.moved, 1);
        assert_eq!(p.entities[0]["at"], json!([0.0, -0.001]));
    }
}
