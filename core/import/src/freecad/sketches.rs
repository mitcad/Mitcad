// SPDX-License-Identifier: MIT
//! Sketches onto the timeline (stage 2): every sketch of the document, after
//! the base features, in the document's order, editable.
//!
//! - Component and frame: a sketch goes into the component of its Body or
//!   App::Part (the root's at the document's top), its frame the stored
//!   placement in that component's coordinates (through the placements of
//!   containers without components of their own).
//! - Plane: a sketch attached to an origin plane lies on Mitcad's origin
//!   plane there; one attached to a face of a body the import made, on that
//!   face (`<feature>:import(j)`, checked to be the sketch's plane); else on
//!   the origin plane parallel to it, else on a fixed construction plane
//!   made for it (hidden). The definition's `frame` keeps the exact
//!   placement (FreeCAD's origin planes have other frames than Mitcad's,
//!   and attachment offsets move sketches).
//! - External geometry: an element of a body the import made in the
//!   sketch's component (the body's object, or the tip of its Body) is
//!   projected linked to the body's element by its Mitcad name, checked
//!   against the element of the stored shape; other elements become fixed
//!   reference geometry from the stored shapes (or from the sketch's own
//!   copy of them, 1.0). FreeCAD's external points and curves are the
//!   projected ones at the same places; defining ones (1.1) are in the
//!   profiles, the others construction geometry.
//! - Check: the solved points must stay within 1e-4 mm of FreeCAD's (radii
//!   too); else the dimensions go driven, else the constraints are left
//!   out (`partial`), as in the .f3d import. The solution is then compared
//!   with the sketch's stored shape (its edges projected into the sketch:
//!   each a curve of the solution, within 1e-4 mm, of the same length).
//! - The sketch keeps FreeCAD's visibility.

use mitcad_freecad::sketch::{self as fc, Curve, PointPos};
use mitcad_freecad::{Object, Placement};
use mitcad_model::features::SketchPlane;
use mitcad_model::sketch::Entity;
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::sketch::project::{Projected, entities, project};
use mitcad_model::{
    BodyUid, ComponentUid, Curve3, ElementKind, EntityUid, FaceName, FeatureDef, FeatureUid,
    Kernel, SketchFrame, Transform, ValueInput,
};
use serde_json::{Value, json};

use super::elements::{SAME_GEOMETRY, element_index};
use super::history::SketchMade;
use super::report::{ObjectOutcome, ShapeCheck, SketchReport};
use super::sketch::{self, ExternalMap, Kind, Translated};
use super::{Importer, Key, placement_of, transform};
use crate::geom::{self, add, cross, dot, norm, scale, sub};

/// The largest error of solved sketch points accepted, mm.
const SKETCH_TOLERANCE: f64 = 1e-4;

/// External geometry made for a sketch.
#[derive(Default)]
struct External {
    entities: Vec<Value>,
    projections: Vec<Value>,
    maps: Vec<Option<ExternalMap>>,
    /// The next free entity id.
    next: u32,
    dropped: Vec<String>,
    notes: Vec<String>,
}

/// How a FreeCAD object's stored shape is placed in a component.
#[derive(Debug, Clone, Copy)]
pub(super) struct Placed {
    pub component: ComponentUid,
    /// From the object's stored shape's coordinates to the component's.
    pub transform: Transform,
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// Imports a sketch of the document (in the timeline's order).
    pub(super) fn sketch_item(&mut self, name: &str) {
        let report = self.import_sketch(name);
        let outcome = report.outcome;
        let note = match outcome {
            ObjectOutcome::Skipped => report.notes.first().cloned(),
            _ => Some(format!("a sketch ({})", report.plane)),
        };
        self.roles.insert(name.to_owned(), (outcome, note));
        self.report.sketches.push(report);
    }

    fn import_sketch(&mut self, name: &str) -> SketchReport {
        let object = self.sources[0]
            .object(name)
            .cloned()
            .expect("a sketch of the document");
        let label = object.label().to_owned();
        let mut report = SketchReport {
            object: name.to_owned(),
            label: label.clone(),
            outcome: ObjectOutcome::Skipped,
            ..SketchReport::default()
        };
        if object.property("Geometry").is_none() {
            // The old SketchFlat sketches keep only a file of their own.
            report
                .notes
                .push("no sketch geometry in the file (an old kind of sketch)".to_owned());
            return report;
        }
        let sketch = fc::Sketch::of(&object);
        report.geometry = sketch.geometry.len();
        report.constraints = sketch.constraints.len();
        for problem in &sketch.problems {
            report.notes.push(format!("not read: {problem}"));
        }
        let Some(place) = self.place_of(name, false) else {
            report
                .notes
                .push("its container was not imported".to_owned());
            return report;
        };
        if !place.component.is_root() {
            report.component = self.component_names.get(&place.component).cloned();
        }
        let frame = frame_of(&place.transform, &placement_of(&object));
        let start = self.doc.undo_depth();
        let plane = match self.sketch_plane(&object, &frame, place.component, &label) {
            Ok(plane) => plane,
            Err(e) => {
                report.notes.push(format!("no plane: {e}"));
                return report;
            }
        };
        report.plane = plane.3.clone();
        let external = self.external_geometry(&sketch, &frame, place.component);
        let axes = self.replay.axis_uses.get(name).copied().unwrap_or_default();
        let mut t = sketch::translate(&sketch, &external.maps, external.next, axes);
        // Dimensions of the constraints' parameters and expressions.
        let adopt = self.dimension_values(&object, &mut t);
        let frame_def = geom::relative_frame(&plane.1, &frame);
        let mut entities = external.entities.clone();
        entities.extend(t.entities.iter().cloned());
        let levels: [(&[Value], &[Value], &str); 3] = [
            (&t.constraints, &t.dimensions, ""),
            (&t.constraints, &t.driven_dimensions, "dimensions driven"),
            (
                &[],
                &t.driven_dimensions,
                "without constraints, dimensions driven",
            ),
        ];
        let mut last = String::new();
        for (level, (constraints, dimensions, what)) in levels.iter().enumerate() {
            let mut def = json!({"type": "sketch", "plane": plane.0, "entities": entities,
                                 "constraints": constraints, "dimensions": dimensions,
                                 "projections": external.projections});
            if let Some(f) = &frame_def {
                def["frame"] = serde_json::to_value(f).expect("serializes");
            }
            let depth = self.doc.undo_depth();
            let uid = match self.add_named(&def, &label, place.component) {
                Ok(uid) => uid,
                Err(e) => {
                    last = e;
                    continue;
                }
            };
            let error = self.solution_error(uid, &t);
            if error > SKETCH_TOLERANCE {
                last = format!("the solved sketch moved by {error:.3e} mm");
                self.undo_to(depth);
                continue;
            }
            // The named constraints' parameters are the sketch's.
            if level == 0
                && !adopt.is_empty()
                && let Err(e) = self.doc.adopt_parameters(uid, &adopt)
            {
                report.notes.push(format!("parameters: {e}"));
            }
            report.feature = Some(uid.to_string());
            report.error = error;
            report.entities = entities.len();
            report.mitcad_constraints = constraints.len();
            report.dimensions = dimensions.len();
            report.notes.extend(plane.2.iter().cloned());
            if level > 0 {
                report.notes.push((*what).to_owned());
            }
            report.notes.extend(external.notes.iter().cloned());
            report.notes.extend(t.notes.iter().cloned());
            report.dropped = external.dropped.clone();
            report.dropped.extend(t.dropped.iter().cloned());
            report.dimension_sources = t.sources.clone();
            for (i, expression) in &sketch.expressions {
                report.expressions.push((*i, expression.clone()));
            }
            report.shape = self.check_shape(&object, &place, uid);
            if object.state.is_stale() {
                report.notes.push(
                    "FreeCAD had not recomputed it: its stored shape was not compared".to_owned(),
                );
            }
            let shape_ok = report.shape.as_ref().is_none_or(|s| s.pass);
            if let Some(s) = &report.shape
                && !s.pass
                && sketch.geometry.is_empty()
            {
                report.notes.push(format!(
                    "FreeCAD's sketch has no geometry, its stored shape {} edges (the file \
                     disagrees with itself)",
                    s.edges
                ));
            } else if let Some(s) = &report.shape
                && !s.pass
            {
                report.notes.push(format!(
                    "differs from FreeCAD's stored shape: {} of its {} edges matched by {} curves",
                    s.matched, s.edges, s.curves
                ));
            }
            report.outcome = if level == 0 && report.dropped.is_empty() && shape_ok {
                ObjectOutcome::Parametric
            } else {
                ObjectOutcome::Partial
            };
            self.measure_sketch(uid, &place, &mut report);
            self.measured_sketches
                .push((self.report.sketches.len(), uid, place));
            let visible = self.sources[0].file.visible(&object);
            if !visible && let Err(e) = self.doc.set_feature_visible(uid, false) {
                report.notes.push(format!("visibility: {e}"));
            }
            // What the history's features refer to.
            let output_frame = self.doc.sketch_output(uid).map_or(frame, |o| o.frame);
            self.replay.sketches.insert(
                name.to_owned(),
                SketchMade {
                    uid,
                    component: place.component,
                    frame: output_frame,
                    curves: t.curves.iter().copied().collect(),
                    points: t.points.iter().copied().collect(),
                    axes: t.axes,
                },
            );
            return report;
        }
        // Not the plane made for it either.
        self.undo_to(start);
        report.notes.insert(0, format!("not imported: {last}"));
        report
    }

    pub(super) fn undo_to(&mut self, depth: usize) {
        while self.doc.undo_depth() > depth {
            if self.doc.undo().is_none() {
                break;
            }
        }
    }

    /// How far the solved sketch is from FreeCAD's: points and radii, mm.
    fn solution_error(&self, uid: FeatureUid, t: &Translated) -> f64 {
        let Some(output) = self.doc.sketch_output(uid) else {
            return f64::INFINITY;
        };
        let solved = &output.solved;
        let points = t.positions.iter().map(|(id, at)| {
            solved
                .points
                .get(&EntityUid(*id))
                .map_or(f64::INFINITY, |s| (s[0] - at[0]).hypot(s[1] - at[1]))
        });
        let radii = t
            .radii
            .iter()
            .map(|(id, r)| match solved.radii.get(&EntityUid(*id)) {
                Some(s) => (s - r).abs(),
                None => match solved.curves.get(&EntityUid(*id)) {
                    Some(Curve2::Arc { radius, .. }) => (radius - r).abs(),
                    _ => f64::INFINITY,
                },
            });
        points.chain(radii).fold(0.0, f64::max)
    }

    // Placements.

    /// Where an object's stored shape goes: the component of its container
    /// (a part, a Body with a component of its own), the root's otherwise,
    /// through the placements of containers in between. `own`: the object
    /// is a body the import made, which then says.
    pub(super) fn place_of(&self, name: &str, own: bool) -> Option<Placed> {
        if own
            && let Some(p) = self
                .planned
                .iter()
                .find(|p| p.key.doc == 0 && p.key.name == name && p.owner.as_deref() == Some(name))
        {
            return Some(Placed {
                component: p.component,
                transform: p.transform.unwrap_or(Transform::IDENTITY),
            });
        }
        let source = &self.sources[0];
        let mut at = name.to_owned();
        let mut t = Transform::IDENTITY;
        for _ in 0..64 {
            let Some(parent) = source.container.get(&at) else {
                return Some(Placed {
                    component: ComponentUid::ROOT,
                    transform: t,
                });
            };
            let key = Key {
                doc: 0,
                name: parent.clone(),
            };
            if let Some(&component) = self.components.get(&key) {
                return Some(Placed {
                    component,
                    transform: t,
                });
            }
            let o = source.object(parent)?;
            if o.type_name == "PartDesign::Body" || super::is_component(&o.type_name) {
                // A part not placed: its objects are not imported.
                if super::is_component(&o.type_name) {
                    return None;
                }
                t = transform(&placement_of(o)).after(&t);
            }
            at = parent.clone();
        }
        None
    }

    // Planes.

    /// The sketch's plane as the definition's `plane`, that plane's frame,
    /// notes and a description. A sketch whose attachment offset along its
    /// normal an expression drives lies on a construction plane offset by
    /// it from its support (stage 4), one whose offset turns it about its
    /// support's x or y axis by an expression on a plane turned by it.
    fn sketch_plane(
        &mut self,
        object: &Object,
        frame: &SketchFrame,
        component: ComponentUid,
        label: &str,
    ) -> Result<(Value, SketchFrame, Vec<String>, String), String> {
        // A side offset an expression drives that a quarter turn makes the
        // sketch's offset along its normal: the origin plane it is
        // parallel to, offset by that expression.
        if let Some((def, axis, note)) = self.side_offset_def(object, frame, component, true) {
            match self.added_plane(&def, frame, component, &format!("{label} plane")) {
                Ok((uid, made)) => {
                    return Ok((
                        json!(uid.to_string()),
                        made,
                        vec![format!("on {uid}, {note}")],
                        format!("on {uid}"),
                    ));
                }
                Err(why) => {
                    let path = format!("AttachmentOffset.Base.{}", ["x", "y", "z"][axis]);
                    self.left_out(object, &path, &format!("its plane: {why}"));
                }
            }
        }
        // Side offsets an expression drives move the sketch within its
        // plane: Mitcad's sketch frame keeps its place as numbers.
        for axis in ["x", "y"] {
            self.kept_value(
                object,
                &format!("AttachmentOffset.Base.{axis}"),
                "a side offset moves the sketch within its plane, whose place Mitcad's sketch frame keeps as numbers",
            );
        }
        if let Some(tilt) = self.tilt_quantity(object) {
            match self.tilted_plane(object, frame, component, label, &tilt) {
                Ok(plane) => return Ok(plane),
                Err(why) => {
                    let why = format!("its attachment offset's turn is left out: {why}");
                    self.left_out(object, "AttachmentOffset.Rotation.Angle", &why);
                    let mut plane = self.support_plane(object, frame, component, label)?;
                    plane.2.push(why);
                    return Ok(plane);
                }
            }
        }
        let Some(offset) = self.offset_quantity(object) else {
            return self.support_plane(object, frame, component, label);
        };
        match self.offset_sketch_plane(object, frame, component, label, &offset) {
            Ok(plane) => Ok(plane),
            Err(why) => {
                let why = format!("its attachment offset's expression is left out: {why}");
                self.left_out(object, "AttachmentOffset.Base.z", &why);
                let mut plane = self.support_plane(object, frame, component, label)?;
                plane.2.push(why);
                Ok(plane)
            }
        }
    }

    /// A construction plane offset from the sketch's support (an origin
    /// plane, a datum plane, a planar face) by its attachment offset along
    /// the normal, `offset` (FreeCAD's value and expression).
    fn offset_sketch_plane(
        &mut self,
        object: &Object,
        frame: &SketchFrame,
        component: ComponentUid,
        label: &str,
        offset: &super::params::Q,
    ) -> Result<(Value, SketchFrame, Vec<String>, String), String> {
        let doc = &self.sources[0].file.document;
        let mode = doc.enum_text(object, "MapMode").unwrap_or_default();
        if !matches!(mode.as_str(), "FlatFace" | "ObjectXY") {
            return Err(format!("attached by {mode}"));
        }
        if let Some(mitcad_freecad::Value::Placement(p)) = object.value("AttachmentOffset")
            && (p.rotation[0].abs() > 1e-12 || p.rotation[1].abs() > 1e-12)
        {
            return Err("the offset turns the sketch out of its support's plane".to_owned());
        }
        let support: Vec<(String, String)> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| object.links(p))
            .flat_map(|l| {
                if l.subs.is_empty() {
                    vec![(l.object.clone(), String::new())]
                } else {
                    l.subs
                        .iter()
                        .map(|s| (l.object.clone(), super::elements::sub_name(s).to_owned()))
                        .collect()
                }
            })
            .collect();
        let [(target, element)] = support.as_slice() else {
            return Err("not attached to one plane".to_owned());
        };
        // The support: its plane reference and frame.
        let (plane, base): (Value, SketchFrame) =
            if let Some(index) = element_index(element, "Face") {
                let (body, face) = self
                    .linked_element(target, ElementKind::Face, index, component)
                    .ok_or_else(|| format!("{target}.{element} is not a face of the bodies"))?;
                let face_name = face.parse::<FaceName>().map_err(|e| e.to_string())?;
                let shape = self.doc.body_shape(body).ok_or("the body is gone")?;
                let p = self
                    .doc
                    .kernel()
                    .face_plane(shape, &face_name)
                    .map_err(|e| format!("{face}: {e}"))?;
                (
                    json!({"face": face, "body": body.to_string()}),
                    SketchPlane::face_frame(p.origin, p.normal),
                )
            } else if let Some(uid) = self.replay.datums.get(target).copied()
                && let Some(mitcad_model::Datum::Plane(p)) = self.doc.datum(uid)
            {
                (json!(uid.to_string()), p.frame())
            } else if let Some(o) = self.sources[0].object(target)
                && o.type_name.ends_with("Plane")
                && let Some(place) = self.place_of(target, false)
                && place.component == component
                && let Some((p, text)) =
                    origin_plane(&frame_of(&place.transform, &placement_of(o)), true)
            {
                (json!(text), p.origin_frame().expect("an origin plane"))
            } else {
                return Err(format!("its support {target} is not a plane of the import"));
            };
        let n = base.normal();
        if norm(cross(n, frame.normal())) > 1e-9 {
            return Err("its support is not parallel to it".to_owned());
        }
        // The offset along the support's normal, FreeCAD's along the
        // sketch's.
        let distance = dot(sub(frame.origin, base.origin), n);
        let sign = if dot(n, frame.normal()) < 0.0 {
            -1.0
        } else {
            1.0
        };
        if (distance - sign * offset.value).abs() > SKETCH_TOLERANCE {
            return Err(format!(
                "its offset {distance} from the support is not the attachment offset's {}",
                offset.value
            ));
        }
        let q = offset.signed(sign);
        let def = json!({"type": "construction_plane",
            "definition": {"type": "offset", "plane": plane, "distance": q.json()}});
        let uid = self.add_named(&def, &format!("{label} plane"), component)?;
        let _ = self.doc.set_feature_visible(uid, false);
        let made = match self.doc.datum(uid) {
            Some(mitcad_model::Datum::Plane(p)) => p.frame(),
            _ => return Err("the offset plane was not made".to_owned()),
        };
        if !geom::coplanar(frame, made.origin, made.normal(), SKETCH_TOLERANCE) {
            self.doc.undo();
            return Err("the offset plane is not the sketch's".to_owned());
        }
        Ok((
            json!(uid.to_string()),
            made,
            vec![format!(
                "on {uid}, offset from its support by its attachment offset ({})",
                q.text.as_deref().unwrap_or_default()
            )],
            format!("on {uid}"),
        ))
    }

    /// A construction plane turned from the object's support (an origin
    /// plane, a datum plane) about the support's x or y axis by its
    /// attachment offset's turn, `tilt` (the axis, FreeCAD's angle and
    /// expression): its reference, frame, notes and a description. It must
    /// be the object's plane (`frame`); the other way round is tried too.
    pub(super) fn tilted_plane(
        &mut self,
        object: &Object,
        frame: &SketchFrame,
        component: ComponentUid,
        label: &str,
        tilt: &([f64; 2], super::params::Q),
    ) -> Result<(Value, SketchFrame, Vec<String>, String), String> {
        let name = format!("{label} plane");
        let mut last = "the turned plane is not the sketch's".to_owned();
        for def in self.tilted_defs(object, component, tilt)? {
            match self.added_plane(&def, frame, component, &name) {
                Ok((uid, made)) => {
                    return Ok((
                        json!(uid.to_string()),
                        made,
                        vec![format!(
                            "on {uid}, turned from its support by its attachment offset ({})",
                            tilt.1.text.as_deref().unwrap_or_default()
                        )],
                        format!("on {uid}"),
                    ));
                }
                Err(why) => last = why,
            }
        }
        Err(last)
    }

    /// Adds a construction plane named `name`, hidden, that must be the
    /// object's plane (`frame`): its uid and frame; Err (nothing added)
    /// when it does not evaluate or is another plane.
    pub(super) fn added_plane(
        &mut self,
        def: &Value,
        frame: &SketchFrame,
        component: ComponentUid,
        name: &str,
    ) -> Result<(FeatureUid, SketchFrame), String> {
        let uid = self.add_named(def, name, component)?;
        let made = match self.doc.datum(uid) {
            Some(mitcad_model::Datum::Plane(p)) => p.frame(),
            _ => {
                self.doc.undo();
                return Err("the plane was not made".to_owned());
            }
        };
        if !geom::coplanar(frame, made.origin, made.normal(), SKETCH_TOLERANCE) {
            self.doc.undo();
            return Err("the plane is not the object's".to_owned());
        }
        let _ = self.doc.set_feature_visible(uid, false);
        Ok((uid, made))
    }

    /// The definitions of a construction plane turned from the object's
    /// support (an origin plane, a datum plane) about a line in it through
    /// its origin by its attachment offset's turn, `tilt`: the angle one way
    /// round, then the other (which is the object's plane is to be seen).
    pub(super) fn tilted_defs(
        &mut self,
        object: &Object,
        component: ComponentUid,
        tilt: &([f64; 2], super::params::Q),
    ) -> Result<Vec<Value>, String> {
        let doc = &self.sources[0].file.document;
        let mode = doc.enum_text(object, "MapMode").unwrap_or_default();
        if !matches!(mode.as_str(), "FlatFace" | "ObjectXY") {
            return Err(format!("attached by {mode}"));
        }
        let support: Vec<String> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| object.links(p))
            .filter(|l| l.subs.iter().all(|s| s.name.is_empty()))
            .map(|l| l.object.clone())
            .collect();
        let [target] = support.as_slice() else {
            return Err("not attached to one plane object".to_owned());
        };
        let o = self.sources[0]
            .object(target)
            .cloned()
            .ok_or_else(|| format!("no support {target}"))?;
        if !o.type_name.ends_with("Plane") {
            return Err(format!("its support {target} is not a plane"));
        }
        let place = self
            .place_of(target, false)
            .filter(|p| p.component == component)
            .ok_or("its support is in another component")?;
        // FreeCAD's frame of the support: the turn is about its x or y axis
        // through its origin.
        let base = frame_of(&place.transform, &placement_of(&o));
        let plane = match self.replay.datums.get(target) {
            Some(uid) => json!(uid.to_string()),
            None => match origin_plane(&base, true) {
                Some((_, text)) => json!(text),
                None => return Err(format!("its support {target} is no plane of the import")),
            },
        };
        let (along, angle) = tilt;
        let direction = add(scale(base.x_axis, along[0]), scale(base.y_axis, along[1]));
        // An origin axis when the line is one (the angle about its
        // direction), else a fixed axis.
        let mut line = json!({"origin": base.origin, "direction": direction});
        let mut sign = 1.0;
        if norm(base.origin) < 1e-9 {
            for (name, axis) in [
                ("x", [1.0, 0.0, 0.0]),
                ("y", [0.0, 1.0, 0.0]),
                ("z", [0.0, 0.0, 1.0]),
            ] {
                let along = dot(direction, axis);
                if (along.abs() - 1.0).abs() < 1e-12 {
                    line = json!(name);
                    sign = along.signum();
                }
            }
        }
        Ok([sign, -sign]
            .into_iter()
            .map(|s| {
                json!({"type": "construction_plane", "definition": {"type": "angle",
                    "line": line, "angle": angle.signed(s).json(), "plane": plane}})
            })
            .collect())
    }

    /// The plane of an object (`frame`) attached flat to an origin plane of
    /// its component whose attachment offset turns it a quarter turn, or
    /// not at all, about the support's axes: one coordinate of the offset
    /// then moves it along its normal (a side offset of the support turned
    /// into the plane's offset), the others only within it. That plane is
    /// the origin plane parallel to it offset by that coordinate, its
    /// expression when one drives it (`bound`: only then). A construction
    /// plane's definition, the coordinate's axis and a note.
    pub(super) fn side_offset_def(
        &mut self,
        object: &Object,
        frame: &SketchFrame,
        component: ComponentUid,
        bound: bool,
    ) -> Option<(Value, usize, String)> {
        let doc = &self.sources[0].file.document;
        let mode = doc.enum_text(object, "MapMode")?;
        if !matches!(mode.as_str(), "FlatFace" | "ObjectXY") {
            return None;
        }
        let support: Vec<String> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| object.links(p))
            .filter(|l| l.subs.iter().all(|s| s.name.is_empty()))
            .map(|l| l.object.clone())
            .collect();
        let [target] = support.as_slice() else {
            return None;
        };
        // An origin plane (no datum the import made) of the component.
        if self.replay.datums.contains_key(target) {
            return None;
        }
        let o = self.sources[0].object(target)?.clone();
        if !o.type_name.ends_with("Plane") {
            return None;
        }
        let place = self
            .place_of(target, false)
            .filter(|p| p.component == component)?;
        let base = frame_of(&place.transform, &placement_of(&o));
        origin_plane(&base, true)?;
        let Some(mitcad_freecad::Value::Placement(offset)) = object.value("AttachmentOffset")
        else {
            return None;
        };
        // A turn an expression drives is no fixed quarter turn.
        if self.is_bound(object, "AttachmentOffset.Rotation.Angle") {
            return None;
        }
        // The object's normal along the support's axis k.
        let turned = transform(offset).apply_vector([0.0, 0.0, 1.0]);
        let k = (0..3).find(|&k| (turned[k].abs() - 1.0).abs() < 1e-9)?;
        let (plane, plane_name) = origin_plane(frame, false)?;
        let n = plane.origin_frame().expect("an origin plane").normal();
        let axis = [base.x_axis, base.y_axis, base.normal()][k];
        let s = dot(axis, n);
        if (s.abs() - 1.0).abs() > 1e-9 {
            return None;
        }
        let distance = dot(sub(frame.origin, base.origin), n);
        let along = offset.position[k];
        if (s * along - distance).abs() > SKETCH_TOLERANCE {
            return None;
        }
        let q = match self.position_quantity(object, k) {
            Some(q) => q.signed(s),
            None if bound => return None,
            None => super::params::Q::number(s * along, super::params::Kind::Length),
        };
        let note = match &q.text {
            Some(text) => format!("offset from {plane_name} by its attachment offset ({text})"),
            None => format!("offset from {plane_name} by its attachment offset"),
        };
        Some((
            json!({"type": "construction_plane",
                   "definition": {"type": "offset", "plane": plane_name, "distance": q.json()}}),
            k,
            note,
        ))
    }

    /// The plane of the sketch's support, else a plane in its place.
    fn support_plane(
        &mut self,
        object: &Object,
        frame: &SketchFrame,
        component: ComponentUid,
        label: &str,
    ) -> Result<(Value, SketchFrame, Vec<String>, String), String> {
        let doc = &self.sources[0].file.document;
        let mode = doc.enum_text(object, "MapMode").unwrap_or_default();
        let support: Vec<(String, String)> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| object.links(p))
            .flat_map(|l| {
                if l.subs.is_empty() {
                    vec![(l.object.clone(), String::new())]
                } else {
                    l.subs
                        .iter()
                        .map(|s| (l.object.clone(), super::elements::sub_name(s).to_owned()))
                        .collect()
                }
            })
            .collect();
        let normal = frame.normal();
        if mode != "Deactivated" && support.len() == 1 {
            let (target, element) = &support[0];
            if element.is_empty() || !element.starts_with("Face") {
                // A datum plane the history made, in the sketch's plane.
                if let Some(uid) = self.replay.datums.get(target).copied()
                    && self
                        .doc
                        .feature(uid)
                        .is_some_and(|f| f.component == component)
                    && let Some(mitcad_model::Datum::Plane(p)) = self.doc.datum(uid)
                    && geom::coplanar(
                        frame,
                        p.frame().origin,
                        p.frame().normal(),
                        SKETCH_TOLERANCE,
                    )
                {
                    let text = format!("on {uid}");
                    return Ok((json!(uid.to_string()), p.frame(), Vec::new(), text));
                }
                // An origin plane in the sketch's coordinates.
                if let Some(o) = self.sources[0].object(target)
                    && o.type_name.ends_with("Plane")
                    && let Some(place) = self.place_of(target, false)
                    && place.component == component
                {
                    let support_frame = frame_of(&place.transform, &placement_of(o));
                    if let Some((plane, text)) = origin_plane(&support_frame, true) {
                        let f = plane.origin_frame().expect("an origin plane");
                        return Ok((json!(text), f, Vec::new(), format!("on {text}")));
                    }
                }
            } else if let Some(index) = element_index(element, "Face")
                && let Some((body, face)) =
                    self.linked_element(target, ElementKind::Face, index, component)
                && let Ok(face_name) = face.parse::<FaceName>()
                && let Some(shape) = self.doc.body_shape(body)
                && let Ok(p) = self.doc.kernel().face_plane(shape, &face_name)
                && geom::coplanar(frame, p.origin, p.normal, SKETCH_TOLERANCE)
            {
                let f = SketchPlane::face_frame(p.origin, p.normal);
                return Ok((
                    json!({"face": face, "body": body.to_string()}),
                    f,
                    Vec::new(),
                    format!("on {face}"),
                ));
            }
        }
        // A planar face of the component's bodies in the sketch's plane
        // (a face of a feature the import has as part of a body).
        if mode != "Deactivated"
            && support.iter().any(|(_, e)| e.starts_with("Face"))
            && let Some((body, face, p)) = crate::refs::planar_faces(self.doc)
                .into_iter()
                .filter(|(b, _, p)| {
                    self.doc.body_component(*b) == Some(component)
                        && geom::coplanar(frame, p.origin, p.normal, SKETCH_TOLERANCE)
                })
                .min_by_key(|(_, _, p)| dot(p.normal, normal) < 0.0)
        {
            let f = SketchPlane::face_frame(p.origin, p.normal);
            return Ok((
                json!({"face": face.to_string(), "body": body.to_string()}),
                f,
                vec![format!(
                    "on the face {face} in its plane (its support is not part of the bodies)"
                )],
                format!("on {face}"),
            ));
        }
        // The origin plane parallel to it.
        if let Some((plane, text)) = origin_plane(frame, false) {
            let f = plane.origin_frame().expect("an origin plane");
            let notes = if mode == "Deactivated" || support.is_empty() {
                Vec::new()
            } else {
                vec![format!(
                    "{text} parallel to it (its support is not imported)"
                )]
            };
            return Ok((json!(text), f, notes, format!("on {text}")));
        }
        // A fixed construction plane in its place, hidden.
        let def = json!({"type": "construction_plane", "definition": {"type": "fixed",
            "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}});
        let def: FeatureDef<ValueInput> =
            serde_json::from_value(def).map_err(|e| format!("definition: {e}"))?;
        let plane_name = format!("{label} plane");
        let added = self
            .doc
            .add_feature_to(&def, Some(&plane_name), Some(component))
            .or_else(|_| self.doc.add_feature_to(&def, None, Some(component)))
            .map_err(|e| e.to_string())?;
        let uid = added.uid;
        let _ = self.doc.set_feature_visible(uid, false);
        let f = match self.doc.datum(uid) {
            Some(mitcad_model::Datum::Plane(p)) => p.frame(),
            _ => *frame,
        };
        Ok((
            json!(uid.to_string()),
            f,
            Vec::new(),
            format!("on a fixed plane {uid}"),
        ))
    }

    // External geometry and elements.

    /// An object's stored shape, read once.
    pub(super) fn stored_shape(&mut self, object: &str) -> Option<K::Shape> {
        if let Some(shape) = self.stored.get(object) {
            return shape.clone();
        }
        let shape = self.sources[0]
            .file
            .shape_data(object)
            .ok()
            .flatten()
            .and_then(|data| {
                self.doc
                    .kernel()
                    .import_brep(FeatureUid(0), &data, 0)
                    .ok()
                    .map(|(s, _)| s)
            });
        self.stored.insert(object.to_owned(), shape.clone());
        shape
    }

    /// An element of an object's stored shape in a component's
    /// coordinates: its curves.
    pub(super) fn stored_element(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Option<Vec<Curve3>> {
        let place = self.place_of(object, true)?;
        if place.component != component {
            return None;
        }
        let shape = self.stored_shape(object)?;
        let element = self
            .doc
            .kernel()
            .indexed_element(&shape, kind, index)
            .ok()??;
        Some(
            element
                .curves
                .iter()
                .map(|c| transform_curve(c, &place.transform))
                .collect(),
        )
    }

    /// An element of an object as the element of a body of the import in
    /// the component: the body and the element's Mitcad name, when the
    /// body has it ([`Importer::element_name`]).
    fn linked_element(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Option<(BodyUid, String)> {
        let body = self.body_of_object(object)?;
        if self.doc.body_component(body) != Some(component) {
            return None;
        }
        let prefix = match kind {
            ElementKind::Face => "Face",
            ElementKind::Edge => "Edge",
            ElementKind::Vertex => "Vertex",
        };
        self.element_name(object, &format!("{prefix}{}", index + 1))
            .ok()
    }

    /// The sketch's external geometry: linked projections, else fixed
    /// reference geometry, with FreeCAD's external points and curves
    /// mapped to them.
    fn external_geometry(
        &mut self,
        sketch: &fc::Sketch,
        frame: &SketchFrame,
        component: ComponentUid,
    ) -> External {
        let mut out = External {
            maps: vec![None; sketch.external.len()],
            next: 1,
            ..External::default()
        };
        for (l, link) in sketch.links.iter().enumerate() {
            let geos: Vec<usize> = (0..sketch.external.len())
                .filter(|k| sketch.external_links[*k] == Some(l))
                .collect();
            if geos.is_empty() {
                continue;
            }
            let what = format!(
                "external geometry {} ({}.{})",
                geos[0], link.object, link.element
            );
            let element = [
                ("Edge", ElementKind::Edge),
                ("Vertex", ElementKind::Vertex),
                ("Face", ElementKind::Face),
            ]
            .into_iter()
            .find_map(|(prefix, kind)| Some((kind, element_index(&link.element, prefix)?)));
            let defining = geos
                .iter()
                .any(|k| sketch.external[*k].as_ref().is_some_and(|g| g.defining));
            // Linked to the body's element.
            if let Some((kind, index)) = element
                && let Some((body, name)) =
                    self.linked_element(&link.object, kind, index, component)
                && let Ok(source) = name.parse::<mitcad_model::TopoName>()
                && let Some(shape) = self.doc.body_shape(body)
                && let Ok(curves) = self.doc.kernel().curves_of(shape, &source)
                && !curves.is_empty()
            {
                let projected = project(&curves, frame);
                if let Some((made, maps)) = map_external(sketch, &geos, &projected, out.next) {
                    out.next += made.len() as u32;
                    let refs: Vec<String> = made.iter().map(|e| e.as_ref().to_string()).collect();
                    out.projections
                        .push(json!({"source": name, "body": body.to_string(),
                                                "entities": refs}));
                    push_entities(&mut out.entities, &made, defining);
                    for (k, map) in geos.iter().zip(maps) {
                        out.maps[*k] = Some(map);
                    }
                    continue;
                }
                out.notes.push(format!(
                    "{what}: its projection of {name} does not match FreeCAD's"
                ));
            }
            // Fixed: from the stored shape, else from the sketch's copy.
            let mut projected = None;
            if let Some((kind, index)) = element
                && let Some(curves) = self.stored_element(&link.object, kind, index, component)
                && !curves.is_empty()
            {
                projected = Some(project(&curves, frame));
            }
            let mut result = projected.and_then(|p| map_external(sketch, &geos, &p, out.next));
            if result.is_none() {
                let copies: Option<Vec<Projected>> = geos
                    .iter()
                    .map(|k| {
                        sketch.external[*k]
                            .as_ref()
                            .and_then(|g| projected_of(&g.curve))
                    })
                    .collect();
                result = copies.and_then(|p| map_external(sketch, &geos, &p, out.next));
            }
            match result {
                Some((made, maps)) => {
                    out.next += made.len() as u32;
                    push_entities(&mut out.entities, &made, defining);
                    for (k, map) in geos.iter().zip(maps) {
                        out.maps[*k] = Some(map);
                    }
                    out.dropped
                        .push(format!("{what}: fixed in place (not linked to the bodies)"));
                }
                None => out.dropped.push(format!("{what}: could not be made")),
            }
        }
        out
    }

    // Checks and measures.

    /// The solved sketch against its stored shape.
    fn check_shape(
        &mut self,
        object: &Object,
        place: &Placed,
        uid: FeatureUid,
    ) -> Option<ShapeCheck> {
        if object.state.is_stale() {
            return None;
        }
        let shape = self.stored_shape(&object.name)?;
        let kernel = self.doc.kernel();
        let count = kernel.edges(&shape).ok()?.len();
        let output = self.doc.sketch_output(uid)?;
        let frame = output.frame;
        let mut stored = Vec::new();
        for i in 0..count {
            let element = kernel
                .indexed_element(&shape, ElementKind::Edge, i)
                .ok()??;
            let curves: Vec<Curve3> = element
                .curves
                .iter()
                .map(|c| transform_curve(c, &place.transform))
                .collect();
            for p in project(&curves, &frame) {
                if let Projected::Curve(c) = p {
                    stored.push(c);
                }
            }
        }
        let mine = profile_curves(self.doc, uid);
        let mut used = vec![false; mine.len()];
        let mut matched = 0;
        let mut deviation: f64 = 0.0;
        for s in &stored {
            let length = curve_length(s);
            let (t0, t1) = s.domain();
            let samples: Vec<[f64; 2]> = (0..=16)
                .map(|k| s.point(t0 + (t1 - t0) * f64::from(k) / 16.0))
                .collect();
            let best = mine
                .iter()
                .enumerate()
                .filter(|(j, _)| !used[*j])
                .map(|(j, m)| {
                    let far = samples.iter().map(|q| m.closest(*q).1).fold(0.0, f64::max);
                    (j, far.max((curve_length(m) - length).abs()))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((j, d)) = best
                && d <= SKETCH_TOLERANCE
            {
                used[j] = true;
                matched += 1;
                deviation = deviation.max(d);
            }
        }
        Some(ShapeCheck {
            edges: stored.len(),
            curves: mine.len(),
            matched,
            deviation,
            pass: matched == stored.len() && stored.len() == mine.len(),
        })
    }

    /// The length and world centre (by length) of the sketch's profile
    /// curves, for the comparison with FreeCAD's measures.
    pub(super) fn measure_sketch(
        &self,
        uid: FeatureUid,
        place: &Placed,
        report: &mut SketchReport,
    ) {
        let Some(output) = self.doc.sketch_output(uid) else {
            return;
        };
        let frame = output.frame;
        let (mut length, mut sum) = (0.0, [0.0; 2]);
        for c in profile_curves(self.doc, uid) {
            let (l, m) = curve_moment(&c);
            length += l;
            sum[0] += m[0];
            sum[1] += m[1];
        }
        report.length = length;
        report.edges = profile_curves(self.doc, uid).len();
        if length > 0.0 {
            let center = frame.point([sum[0] / length, sum[1] / length]);
            let world = match self.component_paths.get(&place.component) {
                Some(path) => self.doc.path_transform(path),
                None => Transform::IDENTITY,
            };
            report.center = Some(world.apply_point(center));
        }
    }
}

/// The sketch's frame: its placement in its container placed in the
/// component.
pub(super) fn frame_of(to_component: &Transform, placement: &Placement) -> SketchFrame {
    let local = SketchFrame {
        origin: placement.position,
        x_axis: placement_dir(placement, [1.0, 0.0, 0.0]),
        y_axis: placement_dir(placement, [0.0, 1.0, 0.0]),
    };
    to_component.apply_frame(&local)
}

fn placement_dir(p: &Placement, v: [f64; 3]) -> [f64; 3] {
    let m = p.matrix();
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

/// Mitcad's origin plane in a frame's plane (`coplanar`), or parallel to
/// it.
fn origin_plane(frame: &SketchFrame, coplanar: bool) -> Option<(SketchPlane, &'static str)> {
    let n = frame.normal();
    [
        (SketchPlane::Xy, "xy"),
        (SketchPlane::Xz, "xz"),
        (SketchPlane::Yz, "yz"),
    ]
    .into_iter()
    .find(|(plane, _)| {
        let f = plane.origin_frame().expect("an origin plane");
        let m = f.normal();
        norm(cross(n, m)) < 1e-9 && (!coplanar || dot(sub(frame.origin, f.origin), m).abs() < 1e-9)
    })
}

pub(super) fn transform_curve(c: &Curve3, t: &Transform) -> Curve3 {
    match c {
        Curve3::Point(p) => Curve3::Point(t.apply_point(*p)),
        Curve3::Line { start, end } => Curve3::Line {
            start: t.apply_point(*start),
            end: t.apply_point(*end),
        },
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => Curve3::Conic {
            center: t.apply_point(*center),
            normal: t.apply_vector(*normal),
            x_axis: t.apply_vector(*x_axis),
            major: *major,
            minor: *minor,
            start: *start,
            end: *end,
            closed: *closed,
        },
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => Curve3::BSpline {
            degree: *degree,
            poles: poles.iter().map(|p| t.apply_point(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
    }
}

/// A FreeCAD curve of the sketch (external geometry it keeps) as projected
/// geometry.
fn projected_of(curve: &Curve) -> Option<Projected> {
    let angle = |center: [f64; 2], q: [f64; 2]| (q[1] - center[1]).atan2(q[0] - center[0]);
    Some(match curve {
        Curve::Point { at } => Projected::Point(*at),
        Curve::Line { start, end } => Projected::Curve(Curve2::Line { a: *start, b: *end }),
        Curve::Circle { center, radius } => Projected::Curve(Curve2::Circle {
            center: *center,
            radius: *radius,
        }),
        Curve::Arc { center, radius, .. } => {
            let (s, e) = (curve.point(PointPos::Start)?, curve.point(PointPos::End)?);
            let start = angle(*center, s);
            Projected::Curve(Curve2::Arc {
                center: *center,
                radius: *radius,
                start,
                end: mitcad_model::sketch::geometry::angle_after(start, angle(*center, e)),
            })
        }
        Curve::Ellipse {
            center,
            major,
            minor,
            frame,
        } => Projected::Curve(Curve2::Ellipse {
            center: *center,
            major: *major,
            minor: *minor,
            rotation: frame.angle(),
        }),
        Curve::ArcOfEllipse {
            center,
            major,
            minor,
            frame,
            ..
        } => {
            let rotation = frame.angle();
            let ellipse = Curve2::Ellipse {
                center: *center,
                major: *major,
                minor: *minor,
                rotation,
            };
            let (s, e) = (curve.point(PointPos::Start)?, curve.point(PointPos::End)?);
            let start = ellipse.closest(s).0;
            Projected::Curve(Curve2::EllipticalArc {
                center: *center,
                major: *major,
                minor: *minor,
                rotation,
                start,
                end: mitcad_model::sketch::geometry::angle_after(start, ellipse.closest(e).0),
            })
        }
        Curve::BSpline(b) => {
            let flat = b.flat()?;
            Projected::Curve(Curve2::Nurbs(mitcad_model::sketch::geometry::Nurbs {
                degree: flat.degree,
                control: flat.poles,
                weights: flat.weights,
                knots: flat.knots,
            }))
        }
        _ => return None,
    })
}

/// Entities for projected geometry with ids from `first`, and FreeCAD's
/// external geometry `geos` mapped to them: each point of each geometry
/// at an entity point, each curve to a curve through them. None when they
/// do not match. Without FreeCAD's copy (0.21) the projection is the
/// geometry: one per link.
fn map_external(
    sketch: &fc::Sketch,
    geos: &[usize],
    projected: &[Projected],
    first: u32,
) -> Option<(Vec<Entity>, Vec<ExternalMap>)> {
    let mut next = first;
    let made = entities(projected, &mut || {
        next += 1;
        EntityUid(next - 1)
    });
    let at = |id: EntityUid| -> Option<[f64; 2]> {
        made.iter().find(|e| e.id == id).and_then(|e| match e.kind {
            mitcad_model::sketch::EntityKind::Point { at } => Some(at),
            _ => None,
        })
    };
    let size = made
        .iter()
        .filter_map(|e| match e.kind {
            mitcad_model::sketch::EntityKind::Point { at } => Some(at[0].abs().max(at[1].abs())),
            _ => None,
        })
        .fold(1.0, f64::max);
    let tol = SAME_GEOMETRY * size;
    let near = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]) <= tol;
    let curves: Vec<&Entity> = made.iter().filter(|e| !e.is_point()).collect();
    let mut maps = Vec::new();
    for k in geos {
        let fc_geo = sketch.external[*k].as_ref();
        let mut map = ExternalMap::default();
        match fc_geo.map(|g| &g.curve) {
            Some(Curve::Point { at: q }) => {
                let p = made
                    .iter()
                    .find(|e| e.is_point() && at(e.id).is_some_and(|a| near(a, *q)))?;
                map.points.push((PointPos::Start, p.id.0));
            }
            Some(curve) => {
                // The curve whose points are the geometry's points.
                let wanted: Vec<(PointPos, [f64; 2])> =
                    [PointPos::Start, PointPos::End, PointPos::Mid]
                        .into_iter()
                        .filter_map(|pos| Some((pos, curve.point(pos)?)))
                        .collect();
                let found = curves.iter().find_map(|e| {
                    let points = e.kind.points();
                    let mut pairs = Vec::new();
                    for (pos, q) in &wanted {
                        let id = points
                            .iter()
                            .find(|id| at(**id).is_some_and(|a| near(a, *q)))?;
                        pairs.push((*pos, id.0));
                    }
                    Some((e, pairs))
                })?;
                map.curve = Some((found.0.id.0, kind_of(&found.0.kind)?));
                map.points = found.1;
            }
            None => {
                // 0.21: the link's projection is the geometry.
                if projected.len() != 1 || geos.len() != 1 {
                    return None;
                }
                match &projected[0] {
                    Projected::Point(_) => {
                        let p = made.iter().find(|e| e.is_point())?;
                        map.points.push((PointPos::Start, p.id.0));
                    }
                    Projected::Curve(_) => {
                        let e = curves.first()?;
                        let kind = kind_of(&e.kind)?;
                        map.curve = Some((e.id.0, kind));
                        let pts = e.kind.points();
                        match kind {
                            Kind::Line => {
                                map.points.push((PointPos::Start, pts[0].0));
                                map.points.push((PointPos::End, pts[1].0));
                            }
                            Kind::Circle | Kind::Ellipse => {
                                map.points.push((PointPos::Mid, pts[0].0))
                            }
                            Kind::Arc => {
                                map.points.push((PointPos::Mid, pts[0].0));
                                map.points.push((PointPos::Start, pts[1].0));
                                map.points.push((PointPos::End, pts[2].0));
                            }
                            Kind::EllipticalArc => {
                                map.points.push((PointPos::Mid, pts[0].0));
                                map.points.push((PointPos::Start, pts[2].0));
                                map.points.push((PointPos::End, pts[3].0));
                            }
                            Kind::Spline => {
                                map.points.push((PointPos::Start, pts[0].0));
                                map.points.push((PointPos::End, pts[pts.len() - 1].0));
                            }
                        }
                    }
                }
            }
        }
        maps.push(map);
    }
    Some((made, maps))
}

fn kind_of(kind: &mitcad_model::sketch::EntityKind) -> Option<Kind> {
    use mitcad_model::sketch::EntityKind as E;
    Some(match kind {
        E::Line { .. } => Kind::Line,
        E::Circle { .. } => Kind::Circle,
        E::Arc { .. } => Kind::Arc,
        E::Ellipse { .. } => Kind::Ellipse,
        E::EllipticalArc { .. } => Kind::EllipticalArc,
        E::Spline { .. } | E::FittedSpline { .. } => Kind::Spline,
        E::Point { .. } => return None,
    })
}

/// External entities into the definition: construction unless defining.
fn push_entities(out: &mut Vec<Value>, made: &[Entity], defining: bool) {
    for e in made {
        let mut e = e.clone();
        if !defining && !e.is_point() {
            e.construction = true;
        }
        out.push(serde_json::to_value(&e).expect("serializes"));
    }
}

/// The solved curves of a sketch that bound profiles (not construction,
/// not points).
fn profile_curves<K: Kernel>(doc: &mitcad_model::Document<K>, uid: FeatureUid) -> Vec<Curve2> {
    let (Some(output), Some(FeatureDef::Sketch(def))) =
        (doc.sketch_output(uid), doc.feature(uid).map(|f| &f.def))
    else {
        return Vec::new();
    };
    def.entities
        .iter()
        .filter(|e| !e.is_point() && !e.construction)
        .filter_map(|e| output.solved.curves.get(&e.id).cloned())
        .collect()
}

/// Gauss–Legendre nodes and weights on [−1, 1] (5 points).
const GAUSS: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_08),
    (0.906_179_845_938_664, 0.236_926_885_056_189_08),
];

/// A curve's length and its first moment (∫ point ds).
fn curve_moment(c: &Curve2) -> (f64, [f64; 2]) {
    let (t0, t1) = c.domain();
    // Pieces: a spline's knot spans, each cut in 16; others in 64.
    let mut cuts: Vec<f64> = match c {
        Curve2::Nurbs(n) => n
            .knots
            .iter()
            .copied()
            .filter(|k| *k > t0 && *k < t1)
            .collect(),
        _ => Vec::new(),
    };
    cuts.insert(0, t0);
    cuts.push(t1);
    cuts.dedup();
    let pieces = if matches!(c, Curve2::Nurbs(_)) {
        16
    } else {
        64
    };
    let (mut length, mut moment) = (0.0, [0.0; 2]);
    for w in cuts.windows(2) {
        for k in 0..pieces {
            let a = w[0] + (w[1] - w[0]) * k as f64 / pieces as f64;
            let b = w[0] + (w[1] - w[0]) * (k + 1) as f64 / pieces as f64;
            let half = (b - a) / 2.0;
            for (x, weight) in GAUSS {
                let t = a + half * (x + 1.0);
                let [p, d, _] = c.eval(t);
                let ds = d[0].hypot(d[1]) * weight * half;
                length += ds;
                moment[0] += p[0] * ds;
                moment[1] += p[1] * ds;
            }
        }
    }
    (length, moment)
}

fn curve_length(c: &Curve2) -> f64 {
    curve_moment(c).0
}
