// SPDX-License-Identifier: MIT
//! References of the history: a FreeCAD object's element (`Face6`,
//! `Edge12`, `Vertex3`: an index into its stored shape's faces, edges or
//! vertices in OCCT's order) as the Mitcad name of the same element, and
//! FreeCAD's planes and axes (origin features, datums, sketch axes and
//! lines, faces and edges) as Mitcad's plane and axis references.
//!
//! An element is taken from the object's stored shape (in its component's
//! coordinates, [`Kernel::indexed_element`]) and found by its geometry (the
//! same curves, either way along; [`SAME_GEOMETRY`]) in the Mitcad body
//! that holds the object's result: first in that body's shape right after
//! the object (Mitcad's names carry through the later features, as
//! FreeCAD's references do), the name then checked in the body now; else
//! in the body now. An edge FreeCAD merged (`Refine`) is every Mitcad edge
//! along it.

use mitcad_freecad::sketch::{self as fc, Curve, PointPos};
use mitcad_freecad::{LinkRef, Object, SubName};
use mitcad_model::{
    BodyUid, ComponentUid, Curve3, ElementKind, Kernel, SketchFrame, TopoName, Transform,
};
use serde_json::{Value, json};

use super::{Importer, placement_of, transform};
use crate::geom::{cross, dot, norm, sub, unit};

/// Geometry of the same element read twice (FreeCAD's stored shapes, the
/// import's bodies) agrees to this, mm, relative to its size.
pub(super) const SAME_GEOMETRY: f64 = 1e-6;

/// A sub-element's name as an index name (`Edge4`, `Axis0`, `V_Axis`):
/// FreeCAD 1.0 writes some references (a sketch's `Axis0`) as another name
/// with the index name in its `shadowed` attribute.
pub(super) fn sub_name(sub: &SubName) -> &str {
    let plain = |name: &str| {
        matches!(name, "H_Axis" | "V_Axis" | "N_Axis" | "RootPoint")
            || ["Face", "Edge", "Vertex", "Axis", "Wire"].iter().any(|p| {
                name.strip_prefix(p)
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
    };
    match &sub.mapped {
        Some(m) if !plain(&sub.name) && plain(m) => m,
        _ => &sub.name,
    }
}

/// An element name (`Edge12`) as its kind and index from 0.
pub(super) fn parse_element(name: &str) -> Option<(ElementKind, usize)> {
    [
        ("Face", ElementKind::Face),
        ("Edge", ElementKind::Edge),
        ("Vertex", ElementKind::Vertex),
    ]
    .into_iter()
    .find_map(|(prefix, kind)| Some((kind, element_index(name, prefix)?)))
}

/// The number of an element name (`Edge12` → 11, from 0).
pub(super) fn element_index(name: &str, prefix: &str) -> Option<usize> {
    let n: usize = name.strip_prefix(prefix)?.parse().ok()?;
    n.checked_sub(1)
}

/// Every element of a kind of a shape: its name and curves.
fn elements<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    kind: ElementKind,
) -> Vec<(Option<String>, Vec<Curve3>)> {
    let mut out = Vec::new();
    for i in 0..1_000_000 {
        match kernel.indexed_element(shape, kind, i) {
            Ok(Some(e)) => out.push((e.name, e.curves)),
            _ => break,
        }
    }
    out
}

/// The name of the element of a shape with these curves.
fn find_same<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    kind: ElementKind,
    curves: &[Curve3],
) -> Option<String> {
    elements(kernel, shape, kind)
        .into_iter()
        .find(|(name, c)| name.is_some() && same_curves(c, curves))
        .and_then(|(name, _)| name)
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// The Mitcad body holding an object's result: the replay's, else the
    /// body the import made of it (its own, or its Body's when it is the
    /// Body's tip).
    pub(super) fn body_of_object(&self, object: &str) -> Option<BodyUid> {
        if let Some(made) = self.replay.results.get(object)
            && let Some((body, _)) = made
                .bodies
                .iter()
                .find(|(b, _)| self.doc.body_shape(*b).is_some())
        {
            return Some(*body);
        }
        let source = &self.sources[0];
        let owner = |name: &str| {
            self.planned
                .iter()
                .find(|p| p.key.doc == 0 && p.key.name == name && p.owner.as_deref() == Some(name))
                .and_then(|p| p.body)
        };
        if let Some(body) = owner(object) {
            return Some(body);
        }
        let b = source.body_of(object)?;
        let tip = source.object(b)?.link("Tip")?;
        (tip.object == object).then(|| owner(b)).flatten()
    }

    /// The Mitcad names of an element (`Edge12`) of an object, in the body
    /// holding its result now: one name, or the pieces of an edge FreeCAD
    /// merged.
    pub(super) fn element_names(
        &mut self,
        object: &str,
        element: &str,
    ) -> Result<(BodyUid, Vec<String>), String> {
        let (kind, index) =
            parse_element(element).ok_or_else(|| format!("{object}.{element}: not an element"))?;
        let body = self
            .body_of_object(object)
            .ok_or_else(|| format!("{object} has no body here"))?;
        let component = self
            .doc
            .body_component(body)
            .ok_or_else(|| format!("{object}: its body is gone"))?;
        let stored = self
            .stored_element(object, kind, index, component)
            .ok_or_else(|| format!("{object}.{element}: not in its stored shape"))?;
        let kernel = self.doc.kernel();
        let now = self
            .doc
            .body_shape(body)
            .ok_or_else(|| format!("{object}: its body is gone"))?
            .clone();
        // The same element now: at FreeCAD's index (a base feature of
        // FreeCAD's shape keeps its order), else anywhere.
        if let Ok(Some(e)) = kernel.indexed_element(&now, kind, index)
            && let Some(name) = e.name
            && same_curves(&e.curves, &stored)
        {
            return Ok((body, vec![name]));
        }
        if let Some(name) = find_same(kernel, &now, kind, &stored) {
            return Ok((body, vec![name]));
        }
        // An element of an earlier state that later features changed (a
        // face a pocket was cut into): its name right after the object,
        // which carries through to the body now.
        if let Some(made) = self.replay.results.get(object) {
            for (_, shape) in &made.bodies {
                if let Some(name) = find_same(kernel, shape, kind, &stored)
                    && let Ok(parsed) = name.parse::<TopoName>()
                    && kernel.curves_of(&now, &parsed).is_ok_and(|c| !c.is_empty())
                {
                    return Ok((body, vec![name]));
                }
            }
        }
        // An edge FreeCAD merged: Mitcad's pieces along it.
        if kind == ElementKind::Edge && stored.len() == 1 {
            let pieces: Vec<String> = elements(kernel, &now, ElementKind::Edge)
                .into_iter()
                .filter(|(name, c)| {
                    name.is_some()
                        && c.len() == 1
                        && key_points(&c[0]).iter().all(|p| on_curve(&stored[0], *p))
                })
                .filter_map(|(name, _)| name)
                .collect();
            if !pieces.is_empty() {
                return Ok((body, pieces));
            }
        }
        Err(format!("{object}.{element} was not found in the body"))
    }

    /// One element as a body and name.
    pub(super) fn element_name(
        &mut self,
        object: &str,
        element: &str,
    ) -> Result<(BodyUid, String), String> {
        let (body, mut names) = self.element_names(object, element)?;
        if names.len() != 1 {
            return Err(format!("{object}.{element} is {} pieces here", names.len()));
        }
        Ok((body, names.remove(0)))
    }

    /// An object's placement in a component's coordinates (None when it
    /// is in another).
    fn placed_in(&self, object: &Object, component: ComponentUid) -> Option<Transform> {
        let place = self.place_of(&object.name, false)?;
        if place.component != component {
            return None;
        }
        Some(place.transform.after(&transform(&placement_of(object))))
    }

    /// A plane reference for FreeCAD's plane references: an origin plane,
    /// a datum plane, a sketch (its plane, or the plane through its H or V
    /// axis along its normal), a planar face.
    pub(super) fn plane_ref(
        &mut self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<Value, String> {
        let sub = link.subs.first().map_or("", sub_name);
        let o = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no object {}", link.object))?;
        let t = o.type_name.as_str();
        if super::is_sketch(t) {
            let frame = self.sketch_frame(&o, component)?;
            let (normal, x) = match sub {
                "" => (frame.normal(), frame.x_axis),
                "V_Axis" => (frame.x_axis, frame.y_axis),
                "H_Axis" => (frame.y_axis, frame.x_axis),
                other => {
                    // A line of the sketch: the plane through it along the
                    // normal.
                    let (origin, direction) = self.sketch_line(&o, other, component)?;
                    let normal =
                        unit(cross(direction, frame.normal())).ok_or("a degenerate line")?;
                    return Ok(json!({"origin": origin, "normal": normal}));
                }
            };
            return Ok(json!({"origin": frame.origin, "normal": normal, "x_axis": x}));
        }
        if t == "App::Plane" || super::history::is_datum(t) && t.ends_with("Plane") {
            if let Some(uid) = self.replay.datums.get(&o.name) {
                return Ok(json!(uid.to_string()));
            }
            let p = self
                .placed_in(&o, component)
                .ok_or("the plane is in another component")?;
            let m = p.linear;
            let normal = [m[0][2], m[1][2], m[2][2]];
            let x = [m[0][0], m[1][0], m[2][0]];
            if t == "App::Plane" {
                // Mitcad's origin plane when it is that plane.
                for (name, n) in [
                    ("xy", [0.0, 0.0, 1.0]),
                    ("xz", [0.0, 1.0, 0.0]),
                    ("yz", [1.0, 0.0, 0.0]),
                ] {
                    if norm(cross(normal, n)) < 1e-12 && dot(p.translation, n).abs() < 1e-9 {
                        return Ok(json!(name));
                    }
                }
            }
            return Ok(json!({"origin": p.translation, "normal": normal, "x_axis": x}));
        }
        if sub.starts_with("Face") {
            let (body, face) = self.element_name(&o.name, sub)?;
            return Ok(json!({"body": body.to_string(), "face": face}));
        }
        Err(format!(
            "{} ({t}) {sub} is not a plane Mitcad takes",
            link.object
        ))
    }

    /// An axis reference for FreeCAD's axis references: an origin axis, a
    /// datum line, a sketch's H, V or N axis, a construction line of it
    /// (`Axis<n>`) or one of its edges, a straight edge.
    pub(super) fn axis_ref(
        &mut self,
        link: &LinkRef,
        component: ComponentUid,
    ) -> Result<Value, String> {
        let sub = link.subs.first().map_or("", sub_name);
        let o = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no object {}", link.object))?;
        let t = o.type_name.as_str();
        if super::is_sketch(t) {
            let frame = self.sketch_frame(&o, component)?;
            let made = self.replay.sketches.get(&o.name).cloned();
            let axis_line = |which: usize| -> Option<Value> {
                let m = made.as_ref()?;
                Some(json!({"sketch": m.uid.to_string(), "curve": format!("c{}", m.axes[which]?)}))
            };
            return match sub {
                "H_Axis" => Ok(axis_line(0)
                    .unwrap_or_else(|| json!({"origin": frame.origin, "direction": frame.x_axis}))),
                "V_Axis" => Ok(axis_line(1)
                    .unwrap_or_else(|| json!({"origin": frame.origin, "direction": frame.y_axis}))),
                "N_Axis" | "" => Ok(json!({"origin": frame.origin, "direction": frame.normal()})),
                other => {
                    if let Some(geo) = self.sketch_geometry(&o, other, component)
                        && let Some(m) = &made
                        && let Some(id) = m.curves.get(&geo)
                    {
                        return Ok(json!({"sketch": m.uid.to_string(), "curve": format!("c{id}")}));
                    }
                    let (origin, direction) = self.sketch_line(&o, other, component)?;
                    Ok(json!({"origin": origin, "direction": direction}))
                }
            };
        }
        if t == "App::Line" || super::history::is_datum(t) && t.ends_with("Line") {
            if let Some(uid) = self.replay.datums.get(&o.name) {
                return Ok(json!(uid.to_string()));
            }
            let p = self
                .placed_in(&o, component)
                .ok_or("the axis is in another component")?;
            let m = p.linear;
            // An origin line runs along its x axis, a datum line along its z.
            let column = usize::from(t != "App::Line") * 2;
            let direction = [m[0][column], m[1][column], m[2][column]];
            if t == "App::Line" && norm(p.translation) < 1e-9 {
                for (name, d) in [
                    ("x", [1.0, 0.0, 0.0]),
                    ("y", [0.0, 1.0, 0.0]),
                    ("z", [0.0, 0.0, 1.0]),
                ] {
                    if (dot(direction, d) - 1.0).abs() < 1e-12 {
                        return Ok(json!(name));
                    }
                }
            }
            return Ok(json!({"origin": p.translation, "direction": direction}));
        }
        if sub.starts_with("Edge") {
            if let Ok((body, edge)) = self.element_name(&o.name, sub) {
                return Ok(json!({"body": body.to_string(), "edge": edge}));
            }
            let index = element_index(sub, "Edge").ok_or("not an edge")?;
            let curves = self
                .stored_element(&o.name, ElementKind::Edge, index, component)
                .ok_or("the edge is not in its stored shape")?;
            if let [Curve3::Line { start, end }] = curves.as_slice() {
                return Ok(json!({"origin": start, "direction": crate::geom::sub(*end, *start)}));
            }
        }
        Err(format!(
            "{} ({t}) {sub} is not an axis Mitcad takes",
            link.object
        ))
    }

    /// A sketch's frame in a component.
    pub(super) fn sketch_frame(
        &self,
        sketch: &Object,
        component: ComponentUid,
    ) -> Result<SketchFrame, String> {
        if let Some(m) = self.replay.sketches.get(&sketch.name)
            && m.component == component
        {
            return Ok(m.frame);
        }
        let place = self
            .place_of(&sketch.name, false)
            .ok_or("the sketch's container was not imported")?;
        if place.component != component {
            return Err("the sketch is in another component".to_owned());
        }
        Ok(super::sketches::frame_of(
            &place.transform,
            &placement_of(sketch),
        ))
    }

    /// The geometry index of a sketch's `Edge<n>` (an edge of its shape)
    /// or `Axis<n>` (its n-th construction line).
    pub(super) fn sketch_geometry(
        &mut self,
        sketch: &Object,
        element: &str,
        component: ComponentUid,
    ) -> Option<i32> {
        let parsed = fc::Sketch::of(sketch);
        if let Some(n) = element
            .strip_prefix("Axis")
            .and_then(|n| n.parse::<usize>().ok())
        {
            return parsed
                .geometry
                .iter()
                .enumerate()
                .filter(|(_, g)| g.construction && matches!(g.curve, Curve::Line { .. }))
                .nth(n)
                .map(|(i, _)| i as i32);
        }
        let index = element_index(element, "Edge")?;
        let stored = self.stored_element(&sketch.name, ElementKind::Edge, index, component)?;
        let frame = self.sketch_frame(sketch, component).ok()?;
        let size = 1.0
            + stored
                .iter()
                .flat_map(key_points)
                .map(norm)
                .fold(0.0, f64::max);
        let close = |a: [f64; 3], b: [f64; 3]| norm(sub(a, b)) <= SAME_GEOMETRY * size;
        parsed
            .geometry
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.construction)
            .find(|(_, g)| {
                let ends: Vec<[f64; 3]> = [PointPos::Start, PointPos::End, PointPos::Mid]
                    .into_iter()
                    .filter_map(|pos| g.curve.point(pos))
                    .map(|p| frame.point(p))
                    .collect();
                let theirs: Vec<[f64; 3]> = stored.iter().flat_map(key_points).collect();
                !ends.is_empty() && ends.iter().all(|p| theirs.iter().any(|q| close(*p, *q)))
            })
            .map(|(i, _)| i as i32)
    }

    /// A straight line of a sketch (`Edge<n>`, `Axis<n>`): a point on it and
    /// its direction, in the component.
    fn sketch_line(
        &mut self,
        sketch: &Object,
        element: &str,
        component: ComponentUid,
    ) -> Result<([f64; 3], [f64; 3]), String> {
        let frame = self.sketch_frame(sketch, component)?;
        let parsed = fc::Sketch::of(sketch);
        let geo = self
            .sketch_geometry(sketch, element, component)
            .ok_or_else(|| format!("{}.{element} was not found", sketch.name))?;
        let g = parsed.geometry(geo).ok_or("no such geometry")?;
        let (Some(a), Some(b)) = (g.curve.point(PointPos::Start), g.curve.point(PointPos::End))
        else {
            return Err(format!("{}.{element} is not a line", sketch.name));
        };
        let (a, b) = (frame.point(a), frame.point(b));
        Ok((a, sub(b, a)))
    }
}

/// Points along a model curve (its ends, middle and centre; a spline's
/// poles), to tell curves apart.
pub(super) fn key_points(c: &Curve3) -> Vec<[f64; 3]> {
    match c {
        Curve3::Point(p) => vec![*p],
        Curve3::Line { start, end } => vec![*start, *end],
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            ..
        } => {
            let y = cross(*normal, *x_axis);
            let at = |t: f64| -> [f64; 3] {
                std::array::from_fn(|i| {
                    center[i] + major * t.cos() * x_axis[i] + minor * t.sin() * y[i]
                })
            };
            vec![*center, at(*start), at((start + end) / 2.0), at(*end)]
        }
        Curve3::BSpline { poles, .. } => poles.clone(),
    }
}

/// Whether a point lies on a curve (lines and circles; other curves by
/// their key points only).
fn on_curve(c: &Curve3, p: [f64; 3]) -> bool {
    let size = 1.0 + key_points(c).into_iter().map(norm).fold(norm(p), f64::max);
    let tol = SAME_GEOMETRY * size;
    match c {
        Curve3::Line { start, end } => {
            let d = sub(*end, *start);
            let l = norm(d);
            if l == 0.0 {
                return norm(sub(p, *start)) <= tol;
            }
            let t = dot(sub(p, *start), d) / (l * l);
            let q: [f64; 3] = std::array::from_fn(|i| start[i] + t * d[i]);
            (-tol / l..=1.0 + tol / l).contains(&t) && norm(sub(p, q)) <= tol
        }
        Curve3::Conic {
            center,
            normal,
            major,
            minor,
            ..
        } if (major - minor).abs() <= tol => {
            let r = sub(p, *center);
            let off = dot(r, *normal);
            let radial = norm(sub(r, std::array::from_fn(|i| off * normal[i])));
            off.abs() <= tol && (radial - major).abs() <= tol
        }
        _ => key_points(c).iter().any(|q| norm(sub(p, *q)) <= tol),
    }
}

/// Whether two lists of curves are the same geometry (in any order, either
/// way along).
pub(super) fn same_curves(a: &[Curve3], b: &[Curve3]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let close = |p: &[f64; 3], q: &[f64; 3]| {
        let size = 1.0 + norm(*p).max(norm(*q));
        norm(sub(*p, *q)) <= SAME_GEOMETRY * size
    };
    let same = |x: &Curve3, y: &Curve3| {
        std::mem::discriminant(x) == std::mem::discriminant(y) && {
            let (px, py) = (key_points(x), key_points(y));
            px.len() == py.len()
                && px.iter().all(|p| py.iter().any(|q| close(p, q)))
                && py.iter().all(|p| px.iter().any(|q| close(p, q)))
        }
    };
    let mut used = vec![false; b.len()];
    a.iter().all(|x| {
        let found = b
            .iter()
            .enumerate()
            .find(|(j, y)| !used[*j] && same(x, y))
            .map(|(j, _)| j);
        if let Some(j) = found {
            used[j] = true;
        }
        found.is_some()
    })
}
