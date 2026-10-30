// SPDX-License-Identifier: MIT
//! Spines and sections of Part sweeps that are not sketches (mitcad#547):
//!
//! - A spine of a sketch: its curves ([`Importer::sketch_path`]). A Part
//!   helix (`Part::Helix`, cylindrical): a fixed `helix` path from its
//!   sizes and placement. Edges of an object with a body here (a Box's
//!   `Edge3`, …): the body's edges by name (`{"body", "edges"}`). Edges of
//!   an object without one (a Part Line, Circle, Polygon, a Draft wire):
//!   one straight edge a fixed line, else the edges as fixed geometry of a
//!   sketch on a fixed plane (they must lie in one), the path its curves.
//! - A section of a sketch or a planar face of a body
//!   ([`Importer::section_of`]). An object of one vertex (a Part Vertex):
//!   a fixed `point` section. An object's closed edges in one plane (a
//!   Part Circle, Polygon, RegularPolygon, Ellipse, a Draft wire): a
//!   sketch of them as fixed geometry on a fixed plane, its one region.
//!
//! Since mitcad#548: a conical Part helix and edges in no plane are paths
//! of fixed curves ([`super::fixed_paths`]); a section of an open wire is
//! a surface sweep of its sketch's curves (`curves` sections), of a wire in
//! no plane one of `fixed_curves`; PartDesign features' results are found
//! in their Body's body; the spine and sections only read their objects
//! ([`reads_only`]).
//!
//! The objects of edges or vertices that only Part extrusions and sweeps
//! use have no body of their own ([`Importer::part_item`]); the sweep
//! makes the planes and sketches as definitions before its own (`$<i>`),
//! `partial`: the objects' own sizes are not carried over.

use std::borrow::Cow;

use mitcad_freecad::{LinkRef, Object};
use mitcad_model::sketch::project::{entities, project};
use mitcad_model::{Curve3, ElementKind, EntityUid, Kernel, SketchFrame};
use serde_json::{Value, json};

use super::Importer;
use super::elements::{element_index, sub_name};
use super::history::Target;
use mitcad_import_base::geom::{cross, dot, norm, scale, sub, unit};

/// The most edges or vertices of an object a sweep reads.
const MAX_ELEMENTS: usize = 10_000;

/// Definitions a sweep's inputs make before it: each with its name suffix,
/// the i-th referred to as `$<i>`; and the notes they leave.
#[derive(Default)]
pub(super) struct Made {
    pub defs: Vec<(Value, Cow<'static, str>)>,
    pub notes: Vec<String>,
}

impl Made {
    /// A fixed plane and a sketch of the curves on it as fixed geometry:
    /// the sketch's index and its curves' ids (`c1`, …).
    fn fixed_sketch(
        &mut self,
        curves: &[Curve3],
        frame: &SketchFrame,
        what: &str,
    ) -> (usize, Vec<String>) {
        let projected = project(curves, frame);
        let mut next = 0u32;
        let made = entities(&projected, &mut || {
            next += 1;
            EntityUid(next)
        });
        let ids: Vec<String> = made
            .iter()
            .filter(|e| !e.is_point())
            .map(|e| e.as_ref().to_string())
            .collect();
        let listed: Vec<Value> = made
            .iter()
            .map(|e| serde_json::to_value(e).expect("serializes"))
            .collect();
        let plane = self.defs.len();
        self.defs.push((
            json!({"type": "construction_plane", "definition": {"type": "fixed",
                "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}}),
            Cow::Owned(format!(" ({what} plane)")),
        ));
        self.defs.push((
            json!({"type": "sketch", "plane": format!("${plane}"), "entities": listed}),
            Cow::Owned(format!(" ({what})")),
        ));
        (plane + 1, ids)
    }
}

/// Whether `user` is a Part sweep taking the object `name` as its spine or
/// one of its sections (mitcad#547).
pub(super) fn sweep_uses(user: &Object, name: &str) -> bool {
    user.type_name == "Part::Sweep"
        && (user.link("Spine").is_some_and(|l| l.object == name)
            || user.links("Sections").iter().any(|l| l.object == name))
}

/// Whether `o`'s link `property` only reads the linked object's edges,
/// faces or vertices without using up its body (mitcad#548): a Part
/// sweep's spine and sections. Such a link makes no copy of the body and
/// counts as no use of it.
pub(super) fn reads_only(o: &Object, property: &str) -> bool {
    o.type_name == "Part::Sweep" && matches!(property, "Spine" | "Sections")
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// A Part sweep's spine as a path; planes and sketches it needs go to
    /// `made`.
    pub(super) fn sweep_spine(
        &mut self,
        link: &LinkRef,
        target: &Target,
        made: &mut Made,
    ) -> Result<Value, String> {
        let component = target.place.component;
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        if super::is_sketch(&object.type_name) {
            return self.sketch_path(link, component);
        }
        // A Part helix: a fixed helix path, or a conical one's stored
        // curves (mitcad#548).
        if object.type_name == "Part::Helix" && link.subs.is_empty() {
            let helix = self.part_helix(&object, target)?;
            made.notes.push(helix.note);
            return Ok(helix.path);
        }
        // Edges of a body here: the body's edges by name.
        if let Some(body) = self.body_of_object(&link.object) {
            if self.doc.body_component(body) != Some(component) {
                return Err(format!(
                    "the spine {} is another component's body",
                    link.object
                ));
            }
            let names: Vec<String> = if link.subs.is_empty() {
                let count = self
                    .stored_elements(&link.object, target, ElementKind::Edge)?
                    .len();
                (1..=count).map(|i| format!("Edge{i}")).collect()
            } else {
                link.subs.iter().map(|s| sub_name(s).to_owned()).collect()
            };
            let mut edges = Vec::new();
            for name in &names {
                if !name.starts_with("Edge") {
                    return Err(format!(
                        "a spine of {}.{name} (not an edge) is not translated",
                        link.object
                    ));
                }
                let (of, pieces) = self.element_names(&link.object, name)?;
                if of != body {
                    return Err(format!("{}.{name} is in another body", link.object));
                }
                edges.extend(pieces);
            }
            return Ok(json!({"body": body.to_string(), "edges": edges}));
        }
        // Edges of an object without a body: a fixed line, or a sketch of
        // them.
        let curves = self.chosen_curves(link, target)?;
        match curves.as_slice() {
            [] => Err(format!("{} has no edges for a spine", link.object)),
            [Curve3::Line { start, end }] => {
                made.notes.push(format!(
                    "{}'s edge is a fixed line (FreeCAD's object is not carried over)",
                    link.object
                ));
                Ok(json!({"start": start, "end": end}))
            }
            _ => {
                if curves.iter().any(|c| matches!(c, Curve3::Helix { .. })) {
                    return Err(format!(
                        "a spine of a helix among other edges of {} is not translated",
                        link.object
                    ));
                }
                // Edges in no plane (mitcad#548): a path of fixed curves.
                let Some(frame) = self.plane_of(&object, &curves, target) else {
                    made.notes.push(format!(
                        "{}'s edges in no plane are a path of fixed curves (FreeCAD's object is \
                         not carried over)",
                        link.object
                    ));
                    return super::fixed_paths::fixed_curves(&curves);
                };
                let (sketch, curves) = made.fixed_sketch(&curves, &frame, "spine");
                made.notes.push(format!(
                    "{}'s edges are fixed geometry of a sketch (FreeCAD's object keeps no sketch \
                     of them)",
                    link.object
                ));
                Ok(json!({"sketch": format!("${sketch}"), "curves": curves}))
            }
        }
    }

    /// A Part sweep's section (`number` from 1) as a loft's section: a
    /// sketch's region or a body's face ([`Self::section_of`]), a fixed
    /// point, or the closed edges of an object as a fixed sketch's region.
    pub(super) fn sweep_section(
        &mut self,
        link: &LinkRef,
        target: &Target,
        number: usize,
        made: &mut Made,
    ) -> Result<Value, String> {
        let component = target.place.component;
        let object = self.sources[0]
            .object(&link.object)
            .cloned()
            .ok_or_else(|| format!("no {}", link.object))?;
        let face = link.subs.iter().any(|s| sub_name(s).starts_with("Face"));
        if super::is_sketch(&object.type_name) || face {
            return self.section_of(link, component);
        }
        // A vertex (an object of one, or one the link names): a point
        // section.
        let vertex = match link.subs.as_slice() {
            [s] => element_index(sub_name(s), "Vertex"),
            _ => None,
        };
        let curves = if vertex.is_some() {
            Vec::new()
        } else {
            self.chosen_curves(link, target)?
        };
        if curves.is_empty() {
            let vertices = self.stored_elements(&link.object, target, ElementKind::Vertex)?;
            let chosen = match vertex {
                Some(i) => vertices.get(i),
                None if vertices.len() == 1 => vertices.first(),
                None => None,
            };
            let Some([Curve3::Point(point)]) = chosen.map(Vec::as_slice) else {
                return Err(format!(
                    "a section of {} without edges and not one vertex is not translated",
                    link.object
                ));
            };
            made.notes.push(format!(
                "{}'s vertex is a fixed point (FreeCAD's object is not carried over)",
                link.object
            ));
            return Ok(json!({"type": "point", "point": {"point": point}}));
        }
        if curves.iter().any(|c| matches!(c, Curve3::Helix { .. })) {
            return Err(format!(
                "a section of a helix ({}) is not translated",
                link.object
            ));
        }
        // One wire (mitcad#548): FreeCAD sweeps a section's one wire.
        let wires = wires_of(&curves);
        if wires.len() != 1 {
            return Err(format!(
                "a section of {} separate wires of {} is not translated: a Part sweep's section \
                 is one wire",
                wires.len(),
                link.object
            ));
        }
        let closed = wires[0];
        // Edges in no plane (mitcad#548): fixed curves, swept into a
        // surface (FreeCAD makes no caps of them).
        let Some(frame) = self.plane_of(&object, &curves, target) else {
            made.notes.push(format!(
                "{}'s edges in no plane are fixed curves (FreeCAD's object is not carried over)",
                link.object
            ));
            let fixed = super::fixed_paths::fixed_curves(&curves)?;
            return Ok(json!({"type": "fixed_curves", "fixed_curves": fixed["fixed_curves"]}));
        };
        let what = format!("section {number}");
        let (sketch, ids) = made.fixed_sketch(&curves, &frame, &what);
        made.notes.push(format!(
            "{}'s edges are fixed geometry of a sketch (FreeCAD's object keeps no sketch of them)",
            link.object
        ));
        if !closed {
            // An open wire (mitcad#548): its curves, swept into a surface
            // (FreeCAD makes no caps of an open wire).
            return Ok(json!({"type": "curves", "sketch": format!("${sketch}"), "curves": ids}));
        }
        Ok(json!({"type": "profile", "sketch": format!("${sketch}"),
                  "region": format!("${sketch}:region")}))
    }

    /// The curves of an object's stored shape in the component: of the
    /// edges the link names (`Edge<n>`), else of all its edges.
    fn chosen_curves(&mut self, link: &LinkRef, target: &Target) -> Result<Vec<Curve3>, String> {
        let all = self.stored_elements(&link.object, target, ElementKind::Edge)?;
        if link.subs.is_empty() {
            return Ok(all.into_iter().flatten().collect());
        }
        let mut out = Vec::new();
        for sub in &link.subs {
            let name = sub_name(sub);
            let curves = element_index(name, "Edge")
                .and_then(|i| all.get(i))
                .ok_or_else(|| format!("{}.{name} is not an edge of it", link.object))?;
            out.extend(curves.iter().cloned());
        }
        Ok(out)
    }

    /// An object's stored elements of a kind in the component: each one's
    /// curves (an edge's curve, a vertex's point as its coordinates'
    /// curve), in FreeCAD's order. Vertices give their points.
    fn stored_elements(
        &mut self,
        object: &str,
        target: &Target,
        kind: ElementKind,
    ) -> Result<Vec<Vec<Curve3>>, String> {
        let shape = self
            .stored_in(object, &target.place.transform)
            .ok_or_else(|| format!("{object} has no stored shape"))?;
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        for index in 0..MAX_ELEMENTS {
            match kernel.indexed_element(&shape, kind, index) {
                Ok(Some(element)) => out.push(element.curves),
                Ok(None) => break,
                Err(e) => return Err(format!("the elements of {object}: {e}")),
            }
        }
        Ok(out)
    }

    /// The plane the curves lie in: the object's placement where they lie
    /// in it, else the plane through their points.
    fn plane_of(&self, object: &Object, curves: &[Curve3], target: &Target) -> Option<SketchFrame> {
        let placed =
            super::sketches::frame_of(&target.place.transform, &super::placement_of(object));
        if super::part::lies_in(curves, &placed) {
            return Some(placed);
        }
        let frame = plane_through(curves)?;
        super::part::lies_in(curves, &frame).then_some(frame)
    }
}

/// A curve's start and end (None for a closed one or a point): a clamped
/// B-spline's ends are its first and last poles.
fn curve_ends(c: &Curve3) -> Option<([f64; 3], [f64; 3])> {
    match c {
        Curve3::Line { start, end } => Some((*start, *end)),
        Curve3::Conic { closed: true, .. } | Curve3::Point(_) => None,
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
            Some((at(*start), at(*end)))
        }
        Curve3::BSpline { poles, .. } => Some((*poles.first()?, *poles.last()?)),
        Curve3::Helix { .. } => None,
    }
}

/// The wires the curves make, joined where their ends meet (within 1e-6
/// of their size; mitcad#548): whether each is closed (a closed curve
/// alone, or every end met by one other).
pub(super) fn wires_of(curves: &[Curve3]) -> Vec<bool> {
    let size = curves
        .iter()
        .flat_map(super::elements::key_points)
        .map(norm)
        .fold(1.0, f64::max);
    let tolerance = 1e-6 * size;
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut node_of = |p: [f64; 3]| -> usize {
        if let Some(i) = nodes.iter().position(|q| norm(sub(*q, p)) <= tolerance) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let links: Vec<Option<(usize, usize)>> = curves
        .iter()
        .map(|c| curve_ends(c).map(|(a, b)| (node_of(a), node_of(b))))
        .collect();
    let count = nodes.len();
    // Curves joined through shared nodes.
    let mut group: Vec<usize> = (0..curves.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    let mut degree = vec![0usize; count];
    for (i, link) in links.iter().enumerate() {
        let Some((a, b)) = *link else { continue };
        degree[a] += 1;
        degree[b] += 1;
        for (j, other) in links.iter().enumerate().take(i) {
            if let Some((c, d)) = *other
                && (a == c || a == d || b == c || b == d)
            {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut wires: Vec<(usize, bool)> = Vec::new();
    for (i, link) in links.iter().enumerate() {
        let r = root(&mut group, i);
        let closed = match *link {
            None => true,
            Some((a, b)) => degree[a] == 2 && degree[b] == 2,
        };
        match wires.iter_mut().find(|(w, _)| *w == r) {
            Some((_, all)) => *all &= closed,
            None => wires.push((r, closed)),
        }
    }
    wires.into_iter().map(|(_, closed)| closed).collect()
}

/// A plane through the curves: a conic's own, else through their first
/// point, the point farthest from it and the one farthest from the line
/// through both.
pub(super) fn plane_through(curves: &[Curve3]) -> Option<SketchFrame> {
    for c in curves {
        if let Curve3::Conic {
            center,
            normal,
            x_axis,
            ..
        } = c
        {
            let n = unit(*normal)?;
            let x = unit(*x_axis)?;
            return Some(SketchFrame {
                origin: *center,
                x_axis: x,
                y_axis: cross(n, x),
            });
        }
    }
    let points: Vec<[f64; 3]> = curves
        .iter()
        .flat_map(super::elements::key_points)
        .collect();
    let first = *points.first()?;
    let far = points
        .iter()
        .copied()
        .max_by(|a, b| norm(sub(*a, first)).total_cmp(&norm(sub(*b, first))))?;
    let x = unit(sub(far, first))?;
    let off = |p: &[f64; 3]| {
        let d = sub(*p, first);
        norm(sub(d, scale(x, dot(d, x))))
    };
    let side = points
        .iter()
        .copied()
        .max_by(|a, b| off(a).total_cmp(&off(b)))?;
    let n = unit(cross(x, sub(side, first)))?;
    Some(SketchFrame {
        origin: first,
        x_axis: x,
        y_axis: cross(n, x),
    })
}
