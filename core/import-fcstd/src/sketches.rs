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
//!   face (`<feature>:import(j)`, checked to be the sketch's plane); one
//!   attached to a face of a binder of a sketch on the plane of the
//!   binder's sketch, with its `plane_link` (mitcad#963); else on
//!   the origin plane parallel to it, else on a fixed construction plane
//!   made for it (hidden). The definition's `frame` keeps the exact
//!   placement (FreeCAD's origin planes have other frames than Mitcad's,
//!   and attachment offsets move sketches).
//! - External geometry: an element of a body the import made (the body's
//!   object, or the tip of its Body; any of a result's bodies) is
//!   projected linked to the body's element by its Mitcad name, checked
//!   against the element of the stored shape; an edge, vertex or face of an
//!   earlier sketch, or of a binder of one (its `Support`), is projected
//!   linked to that sketch's curves or point (`{"sketch": …, "curve": …}`,
//!   mitcad#904, mitcad#952), a binder of a feature as that feature's
//!   element. Another component's geometry comes through the occurrences
//!   where the document places the two components (the record's `link`).
//!   Other elements become fixed reference geometry from the stored shapes
//!   (or from the sketch's own copy of them, 1.0), with the reason in the
//!   report's warning. FreeCAD's external points and curves are the
//!   projected ones at the same places; defining ones (1.1) are in the
//!   profiles, the others construction geometry.
//! - Check: the solved points must stay within 1e-4 mm of FreeCAD's (radii
//!   too); else the fewest constraints and dimensions that keep it off are
//!   imported inactive, else all of them (`partial`, mitcad#721), as in the
//!   .f3d import (`mitcad_import_base::sketch::inactive`). The solution is then compared
//!   with the sketch's stored shape (its edges projected into the sketch:
//!   each a curve of the solution, within 1e-4 mm, of the same length).
//! - The sketch keeps FreeCAD's visibility.

use std::collections::BTreeSet;

use mitcad_freecad::sketch::{self as fc, Curve, PointPos};
use mitcad_freecad::{Object, Placement};
use mitcad_model::features::{FrameDef, SketchPlane};
use mitcad_model::sketch::Entity;
use mitcad_model::sketch::geometry::Curve2;
use mitcad_model::sketch::project::{Projected, entities, project};
use mitcad_model::{
    BodyUid, ComponentUid, Curve3, ElementKind, EntityUid, FaceName, FeatureDef, FeatureUid,
    Kernel, OccurrenceUid, SketchFrame, Transform, ValueInput,
};
use serde_json::{Value, json};

use super::elements::{
    SAME_GEOMETRY, defining_external_along, element_index, geometry_along, same_curves,
};
use super::history::SketchMade;
use super::report::{ObjectOutcome, ShapeCheck, SketchReport};
use super::sketch::{self, ExternalMap, Kind, Translated};
use super::{Importer, Key, placement_of, transform};
use mitcad_import_base::geom::{self, add, cross, dot, norm, scale, sub};
use mitcad_import_base::sketch::inactive;

/// The entities a source element projects to, each with its curves.
type EntityPieces = Vec<(Value, Vec<Curve3>)>;

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
        // A sketch on a face of a binder of a sketch: the binder's sketch in
        // this component first, kept whatever becomes of this one
        // (mitcad#963).
        if let Some(binder) = self.face_binder(&object) {
            let _ = self.binder_sketch(&binder, place.component);
        }
        let start = self.doc.undo_depth();
        let mut plane = match self.sketch_plane(&object, &frame, place.component, &label) {
            Ok(plane) => plane,
            Err(e) => {
                report.notes.push(format!("no plane: {e}"));
                return report;
            }
        };
        // Another component's plane comes as `{"plane": …, "plane_link":
        // …}` (mitcad#963).
        let mut plane_link = None;
        if let Some(inner) = plane.0.get("plane").cloned() {
            plane_link = plane.0.get("plane_link").cloned();
            plane.0 = inner;
        }
        report.plane = plane.3.clone();
        let external = self.external_geometry(&sketch, &frame, place.component);
        let axes = self.replay.axis_uses.get(name).copied().unwrap_or_default();
        let mut t = sketch::translate(&sketch, &external.maps, external.next, axes);
        // Dimensions of the constraints' parameters and expressions.
        let adopt = self.dimension_values(&object, &mut t);
        self.reference_values(&object, &mut t);
        let frame_def = geom::relative_frame(&plane.1, &frame);
        let mut entities = external.entities.clone();
        entities.extend(t.entities.iter().cloned());
        // Every constraint and dimension active (level 0); the fewest that
        // keep the sketch off FreeCAD's solution inactive and everything else
        // active (level 1, mitcad#721); all of them inactive (level 2).
        let all_items = inactive::item_ids(&t.constraints, &t.dimensions);
        let def_with = |off: &BTreeSet<String>| -> Value {
            let mut def = json!({"type": "sketch", "plane": plane.0, "entities": entities,
                                 "constraints": inactive::mark(&t.constraints, off),
                                 "dimensions": inactive::mark(&t.dimensions, off),
                                 "projections": external.projections});
            if let Some(f) = &frame_def {
                def["frame"] = serde_json::to_value(f).expect("serializes");
            }
            if let Some(link) = &plane_link {
                def["plane_link"] = link.clone();
            }
            def
        };
        let mut last = String::new();
        for level in 0..3 {
            let off: BTreeSet<String> = match level {
                0 => BTreeSet::new(),
                1 => {
                    // Everything inactive first: whether FreeCAD's positions
                    // solve at all, and what of the constraints and
                    // dimensions they do not meet. Then the fewest of them
                    // inactive.
                    let everything: BTreeSet<String> = all_items.iter().cloned().collect();
                    if everything.is_empty() {
                        continue;
                    }
                    let Some(unmet) =
                        self.sketch_attempt(&def_with(&everything), &label, place.component, &t)
                    else {
                        continue;
                    };
                    inactive::fewest(&unmet, &all_items, &mut |set| {
                        self.sketch_attempt(&def_with(set), &label, place.component, &t)
                            .is_some()
                    })
                }
                _ => all_items.iter().cloned().collect(),
            };
            let def = def_with(&off);
            let depth = self.doc.undo_depth();
            let uid = match self.add_sketch_def(&def, &label, place.component) {
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
            // The named constraints' parameters are the sketch's (an
            // inactive dimension keeps its value).
            if !adopt.is_empty()
                && let Err(e) = self.doc.adopt_parameters(uid, &adopt)
            {
                report.notes.push(format!("parameters: {e}"));
            }
            report.feature = Some(uid.to_string());
            report.error = error;
            report.entities = entities.len();
            report.mitcad_constraints = t.constraints.len();
            report.dimensions = t.dimensions.len();
            report.notes.extend(plane.2.iter().cloned());
            if level == 2 {
                report.notes.push(
                    "no subset of its constraints and dimensions solves to FreeCAD's \
                     solution: all of them inactive"
                        .to_owned(),
                );
            } else if let Some(note) = inactive::summary(&t.constraints, &t.dimensions, &off) {
                report.notes.push(note);
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
                    externals: external
                        .maps
                        .iter()
                        .enumerate()
                        .filter_map(|(k, m)| Some((k, m.as_ref()?.curve.as_ref()?.0)))
                        .collect(),
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

    /// Adds a sketch definition, without its driven dimensions' references
    /// where the model refuses a constraint's parameter as one (a feature
    /// before the sketch uses it): the dimensions get their own
    /// (mitcad#250).
    fn add_sketch_def(
        &mut self,
        def: &Value,
        label: &str,
        component: ComponentUid,
    ) -> Result<FeatureUid, String> {
        match self.add_named(def, label, component) {
            Err(_) if mitcad_import_base::sketch::has_references(def) => self.add_named(
                &mitcad_import_base::sketch::without_references(def),
                label,
                component,
            ),
            other => other,
        }
    }

    /// Adds a sketch definition, checks that it solves to FreeCAD's
    /// solution and takes it back (mitcad#721): Some with the inactive
    /// constraints and dimensions its solution does not meet when it does,
    /// None when it does not.
    fn sketch_attempt(
        &mut self,
        def: &Value,
        label: &str,
        component: ComponentUid,
        t: &Translated,
    ) -> Option<Vec<String>> {
        let depth = self.doc.undo_depth();
        let result = self
            .add_sketch_def(def, label, component)
            .ok()
            .filter(|uid| self.solution_error(*uid, t) <= SKETCH_TOLERANCE)
            .map(|uid| {
                self.doc
                    .sketch_output(uid)
                    .map(|o| {
                        o.status()
                            .unmet_inactive
                            .iter()
                            .map(ToString::to_string)
                            .collect()
                    })
                    .unwrap_or_default()
            });
        self.undo_to(depth);
        result
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
            // An origin's plane named as its element (FreeCAD 1.x's
            // `(Origin, XY_Plane013)`, mitcad#955): that plane.
            let (target, element) = match self.origin_member(&support[0].0, &support[0].1) {
                Some(plane) => (plane, String::new()),
                None => support[0].clone(),
            };
            let (target, element) = (&target, &element);
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
            } else if let Some(found) = self.binder_face_plane(target, frame, component) {
                // A face of a binder of one whole sketch (mitcad#963).
                return Ok(found);
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
            && let Some((body, face, p)) = mitcad_import_base::planes::planar_faces(self.doc)
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

    /// An element of an object's stored shape where the import puts the
    /// object: its component and the element's curves in that component's
    /// coordinates.
    pub(super) fn stored_element_own(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
    ) -> Option<(ComponentUid, Vec<Curve3>)> {
        let place = self.place_of(object, true)?;
        let shape = self.stored_shape(object)?;
        let element = self
            .doc
            .kernel()
            .indexed_element(&shape, kind, index)
            .ok()??;
        Some((
            place.component,
            element
                .curves
                .iter()
                .map(|c| transform_curve(c, &place.transform))
                .collect(),
        ))
    }

    /// An element of an object's stored shape in a component's
    /// coordinates: its curves (None in another component).
    pub(super) fn stored_element(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Option<Vec<Curve3>> {
        let (own, curves) = self.stored_element_own(object, kind, index)?;
        (own == component).then_some(curves)
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
        let prefix = match kind {
            ElementKind::Face => "Face",
            ElementKind::Edge => "Edge",
            ElementKind::Vertex => "Vertex",
        };
        let (body, name) = self
            .element_name(object, &format!("{prefix}{}", index + 1))
            .ok()?;
        (self.doc.body_component(body) == Some(component)).then_some((body, name))
    }

    /// How geometry of the component `from` is seen in the component `to`
    /// (mitcad#100, mitcad#952): the transform from `from`'s coordinates
    /// into `to`'s and the occurrence link a projection keeps (None in the
    /// same component), through the occurrences where the document places
    /// each component first. None when either is not placed.
    pub(super) fn seen_in(
        &self,
        from: ComponentUid,
        to: ComponentUid,
    ) -> Option<(Transform, Option<Value>)> {
        if from == to {
            return Some((Transform::IDENTITY, None));
        }
        let path = |c: ComponentUid| -> Option<Vec<OccurrenceUid>> {
            if c.is_root() {
                Some(Vec::new())
            } else {
                self.component_paths.get(&c).cloned()
            }
        };
        let (source, target) = (path(from)?, path(to)?);
        let into = mitcad_model::assembly::inverse(&self.doc.path_transform(&target))
            .after(&self.doc.path_transform(&source));
        let text = |p: &[OccurrenceUid]| {
            p.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/")
        };
        Some((
            into,
            Some(json!({"source": text(&source), "target": text(&target)})),
        ))
    }

    /// An element of an object as the element of a body of the import:
    /// the projection's record without its entities (`source`, `body`, and
    /// the `link` of another component's body, mitcad#952) and the
    /// element's curves in the sketch's component; Err with the reason
    /// when the body has no such element.
    fn body_source(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Result<(Value, Vec<Curve3>), String> {
        let prefix = match kind {
            ElementKind::Face => "Face",
            ElementKind::Edge => "Edge",
            ElementKind::Vertex => "Vertex",
        };
        let (body, name) = self.element_name(object, &format!("{prefix}{}", index + 1))?;
        let (into, link) = self
            .doc
            .body_component(body)
            .and_then(|from| self.seen_in(from, component))
            .ok_or_else(|| format!("{object}'s component is not placed"))?;
        let source = name
            .parse::<mitcad_model::TopoName>()
            .map_err(|e| format!("{name}: {e}"))?;
        let shape = self
            .doc
            .body_shape(body)
            .ok_or_else(|| format!("{object}: its body is gone"))?;
        let curves = self
            .doc
            .kernel()
            .curves_of(shape, &source)
            .map_err(|e| format!("{name}: {e}"))?;
        if curves.is_empty() {
            return Err(format!("{name} has no curves"));
        }
        let mut record = json!({"source": name, "body": body.to_string()});
        if let Some(link) = link {
            record["link"] = link;
        }
        Ok((
            record,
            curves.iter().map(|c| transform_curve(c, &into)).collect(),
        ))
    }

    /// The sketches a sketch or a binder stands for: the sketch itself, or
    /// those a binder's `Support` names (also through a Body's path); None
    /// for another object.
    pub(super) fn sketches_of(&self, object: &Object) -> Option<Vec<Object>> {
        if super::is_sketch(&object.type_name) {
            return Some(vec![object.clone()]);
        }
        if !is_binder(&object.type_name) {
            return None;
        }
        let source = &self.sources[0];
        Some(
            object
                .links("Support")
                .iter()
                .filter(|l| l.file.is_none())
                .flat_map(|l| source.path_objects(l))
                .filter_map(|name| source.object(name))
                .filter(|s| super::is_sketch(&s.type_name))
                .cloned()
                .collect(),
        )
    }

    /// An edge or a vertex of a sketch, or of a binder of a sketch, as
    /// that sketch's curve or point (mitcad#952), when the import made the
    /// sketch earlier (in this component, or in another one placed in the
    /// document): the projection's record without its entities (mitcad#904,
    /// `{"sketch": …, "curve": …}` or `{"sketch": …, "point": …}`, with the
    /// `link` of another component) and where the sketch solved it, in the
    /// sketch's component. The element is found by its place in the
    /// object's stored shape: the sketch's curve along an edge (its own
    /// geometry, or external geometry 1.1 makes part of its shape), its
    /// point at a vertex. None for another object; Err with the reason.
    fn sketch_entity(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Option<Result<EntityPieces, String>> {
        let o = self.sources[0].object(object)?.clone();
        let sketches = self.sketches_of(&o)?;
        let found = self.sketch_entity_of(&o, &sketches, kind, index, component);
        Some(found.map(|pieces| {
            pieces
                .into_iter()
                .map(|(record, curve)| (record, vec![curve]))
                .collect()
        }))
    }

    fn sketch_entity_of(
        &mut self,
        o: &Object,
        sketches: &[Object],
        kind: ElementKind,
        index: usize,
        component: ComponentUid,
    ) -> Result<Vec<(Value, Curve3)>, String> {
        if sketches.is_empty() {
            return Err(format!("{} binds no sketch", o.name));
        }
        let (from, stored) = self
            .stored_element_own(&o.name, kind, index)
            .ok_or_else(|| format!("{} has no such element stored", o.name))?;
        let mut why = String::new();
        for sketch in sketches {
            let Some(made) = self.replay.sketches.get(&sketch.name).cloned() else {
                why = format!("{} is not a sketch made before this one", sketch.name);
                continue;
            };
            // The stored element in the source sketch's component, and that
            // component seen from the sketch made now.
            let (Some((into_source, _)), Some((to_sketch, link))) = (
                self.seen_in(from, made.component),
                self.seen_in(made.component, component),
            ) else {
                why = format!("{}'s component is not placed", sketch.name);
                continue;
            };
            let stored: Vec<Curve3> = stored
                .iter()
                .map(|c| transform_curve(c, &into_source))
                .collect();
            let Some(output) = self.doc.sketch_output(made.uid) else {
                why = format!("{} has no result", sketch.name);
                continue;
            };
            let frame = output.frame;
            let record = |entity: Value| {
                let mut record = json!({"source": {"sketch": made.uid.to_string()}});
                for (key, value) in entity.as_object().into_iter().flatten() {
                    record["source"][key.as_str()] = value.clone();
                }
                if let Some(link) = &link {
                    record["link"] = link.clone();
                }
                record
            };
            let parsed = fc::Sketch::of(sketch);
            // The sketch's curve along one model curve.
            let curve_along = |c: &Curve3| -> Option<(Value, Curve3)> {
                let along = std::slice::from_ref(c);
                let id = geometry_along(&parsed, &frame, along)
                    .and_then(|geo| made.curves.get(&geo))
                    .or_else(|| {
                        defining_external_along(&parsed, &frame, along)
                            .and_then(|k| made.externals.get(&k))
                    })
                    .copied()?;
                let solved = model_curve(&frame, output.solved.curves.get(&EntityUid(id))?)?;
                Some((
                    record(json!({"curve": format!("c{id}")})),
                    transform_curve(&solved, &to_sketch),
                ))
            };
            match (kind, stored.as_slice()) {
                (ElementKind::Edge, [one]) => {
                    if let Some(found) = curve_along(one) {
                        return Ok(vec![found]);
                    }
                    why = format!("{} has no curve along {}", sketch.name, o.name);
                }
                (ElementKind::Face, _) => {
                    // A face of a binder of the sketch: the sketch's curves
                    // around it, each a projection of its own.
                    let found: Vec<(Value, Curve3)> =
                        stored.iter().filter_map(curve_along).collect();
                    if !found.is_empty() {
                        return Ok(found);
                    }
                    why = format!("{} has no curves around {}'s face", sketch.name, o.name);
                }
                (ElementKind::Vertex, [Curve3::Point(at)]) => {
                    // The sketch's point there (FreeCAD's solution, which
                    // the import's keeps within the sketch tolerance).
                    let found = output
                        .solved
                        .points
                        .iter()
                        .map(|(id, p)| {
                            let p = frame.point(*p);
                            (id.0, p, norm(sub(p, *at)))
                        })
                        .filter(|(_, _, d)| *d <= SKETCH_TOLERANCE)
                        .min_by(|a, b| a.2.total_cmp(&b.2));
                    if let Some((id, point, _)) = found {
                        return Ok(vec![(
                            record(json!({"point": format!("p{id}")})),
                            Curve3::Point(to_sketch.apply_point(point)),
                        )]);
                    }
                    why = format!("{} has no point at {}'s vertex", sketch.name, o.name);
                }
                _ => return Err(format!("{}'s element is not one curve or point", o.name)),
            }
        }
        Err(why)
    }

    /// An edge, face or vertex of a binder of a body's result (a feature,
    /// not a sketch, in its `Support`): that object and the index of the
    /// same element of its stored shape, found by geometry (mitcad#952).
    pub(super) fn binder_support_element(
        &mut self,
        object: &str,
        kind: ElementKind,
        index: usize,
    ) -> Option<(String, usize)> {
        let o = self.sources[0].object(object)?.clone();
        if !is_binder(&o.type_name) {
            return None;
        }
        let supports: Vec<String> = {
            let source = &self.sources[0];
            o.links("Support")
                .iter()
                .filter(|l| l.file.is_none())
                .flat_map(|l| source.path_objects(l))
                .filter(|name| {
                    source
                        .object(name)
                        .is_some_and(|s| !super::is_sketch(&s.type_name) && s.shape().is_some())
                })
                .map(str::to_owned)
                .collect()
        };
        let (from, stored) = self.stored_element_own(object, kind, index)?;
        for support in supports {
            let Some(place) = self.place_of(&support, true) else {
                continue;
            };
            let Some((into, _)) = self.seen_in(from, place.component) else {
                continue;
            };
            let Some(shape) = self.stored_shape(&support) else {
                continue;
            };
            let wanted: Vec<Curve3> = stored.iter().map(|c| transform_curve(c, &into)).collect();
            let kernel = self.doc.kernel();
            for i in 0..1_000_000 {
                let Ok(Some(element)) = kernel.indexed_element(&shape, kind, i) else {
                    break;
                };
                let curves: Vec<Curve3> = element
                    .curves
                    .iter()
                    .map(|c| transform_curve(c, &place.transform))
                    .collect();
                if same_curves(&curves, &wanted) {
                    return Some((support, i));
                }
            }
        }
        None
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
            // Linked: to a body's element (also another component's, through
            // its occurrence), to an earlier sketch's curve or point (also
            // through a binder of it, mitcad#952, mitcad#904), or to the
            // element of a body a binder binds. Why not, for the report.
            let mut why = "not an element".to_owned();
            if let Some((kind, index)) = element {
                let exists = self.sources[0].object(&link.object).is_some();
                let attempts: Vec<Result<EntityPieces, String>> =
                    match self.sketch_entity(&link.object, kind, index, component) {
                        Some(found) => {
                            let mut attempts = vec![found];
                            // A binder of a body's result.
                            if let Some((support, i)) =
                                self.binder_support_element(&link.object, kind, index)
                            {
                                attempts.push(
                                    self.body_source(&support, kind, i, component)
                                        .map(|one| vec![one]),
                                );
                            }
                            attempts
                        }
                        None if !exists => {
                            vec![Err(format!("no object {}", link.object))]
                        }
                        None => vec![
                            self.body_source(&link.object, kind, index, component)
                                .map(|one| vec![one]),
                        ],
                    };
                let mut linked = false;
                for attempt in attempts {
                    match attempt {
                        Ok(pieces) => {
                            if push_pieces(&mut out, sketch, &geos, frame, defining, &pieces) {
                                linked = true;
                                break;
                            }
                            let Some((record, _)) = pieces.first() else {
                                continue;
                            };
                            let source = &record["source"];
                            let named = match source.as_str() {
                                Some(name) => name.to_owned(),
                                None => format!(
                                    "{} of the sketch {}",
                                    source["curve"]
                                        .as_str()
                                        .or_else(|| source["point"].as_str())
                                        .unwrap_or_default(),
                                    source["sketch"].as_str().unwrap_or_default()
                                ),
                            };
                            out.notes.push(format!(
                                "{what}: its projection of {named} does not match FreeCAD's"
                            ));
                            why = format!("its projection of {named} does not match FreeCAD's");
                        }
                        Err(e) => why = e,
                    }
                }
                if linked {
                    continue;
                }
            }
            // Fixed: from the stored shape (another component's moved
            // here), else from the sketch's copy.
            let mut projected = None;
            if let Some((kind, index)) = element
                && let Some((from, curves)) = self.stored_element_own(&link.object, kind, index)
                && let Some((into, _)) = self.seen_in(from, component)
                && !curves.is_empty()
            {
                let curves: Vec<Curve3> =
                    curves.iter().map(|c| transform_curve(c, &into)).collect();
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
                        .push(format!("{what}: fixed in place (not linked: {why})"));
                }
                None => out
                    .dropped
                    .push(format!("{what}: could not be made (not linked: {why})")),
            }
        }
        // External geometry whose link the file no longer has (FreeCAD
        // keeps its last copy, the source missing; mitcad#955): fixed in
        // place from the copy, so that the constraints on it still hold.
        for k in 0..sketch.external.len() {
            let linked = sketch.external_links.get(k).copied().flatten().is_some();
            if linked || out.maps[k].is_some() {
                continue;
            }
            let Some(g) = sketch.external[k].as_ref() else {
                continue;
            };
            let what = format!("external geometry {k}");
            let made =
                projected_of(&g.curve).and_then(|p| map_external(sketch, &[k], &[p], out.next));
            match made {
                Some((made, maps)) => {
                    out.next += made.len() as u32;
                    push_entities(&mut out.entities, &made, g.defining);
                    out.maps[k] = maps.into_iter().next();
                    out.dropped.push(format!(
                        "{what}: fixed in place (its link is missing in the file)"
                    ));
                }
                None => out.dropped.push(format!(
                    "{what}: could not be made (its link is missing in the file)"
                )),
            }
        }
        out
    }

    /// A binder of one whole sketch as a feature's sketch (a `Profile`, a
    /// hole's sketch; mitcad#955): in the sketch's own component the sketch
    /// itself; in another a sketch of the binder's, hidden, on the bound
    /// sketch's plane seen through the occurrences where the document
    /// places the two components first (that plane with the `plane_link`,
    /// mitcad#963; where it is not found, the origin plane parallel to it
    /// in the binder's component, else a fixed plane), each of the bound sketch's profile
    /// curves and points a linked projection of it (`{"sketch": …,
    /// "curve": …}` with the `link`, mitcad#904, mitcad#952), so that it
    /// follows the sketch. Kept for the binder's other users; Err with the
    /// reason.
    pub(super) fn binder_sketch(
        &mut self,
        binder: &str,
        component: ComponentUid,
    ) -> Result<SketchMade, String> {
        if let Some(made) = self.replay.sketches.get(binder)
            && made.component == component
        {
            return Ok(made.clone());
        }
        let o = self.sources[0]
            .object(binder)
            .cloned()
            .ok_or_else(|| format!("no object {binder}"))?;
        if !is_binder(&o.type_name) {
            return Err(format!("{binder} is not a sketch or a binder of one"));
        }
        let sketches = self.sketches_of(&o).unwrap_or_default();
        let [source] = sketches.as_slice() else {
            return Err(format!(
                "{binder} binds {} sketches (a feature takes one whole sketch)",
                sketches.len()
            ));
        };
        let whole = o.links("Support").iter().all(|l| {
            l.subs
                .iter()
                .all(|s| s.name.is_empty() || s.name.ends_with('.'))
        });
        if !whole {
            return Err(format!(
                "{binder} binds elements of {} (a feature takes one whole sketch)",
                source.name
            ));
        }
        let p = placement_of(&o);
        let moved = p.position.iter().any(|v| v.abs() > 1e-9)
            || p.rotation[..3].iter().any(|v| v.abs() > 1e-12)
            || o.f64("Offset").is_some_and(|v| v.abs() > 1e-12);
        if moved {
            return Err(format!(
                "{binder} moves or offsets the sketch it binds (not translated)"
            ));
        }
        let made = self
            .replay
            .sketches
            .get(&source.name)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "{} (which {binder} binds) is not an imported sketch",
                    source.name
                )
            })?;
        if made.component == component {
            self.replay.sketches.insert(binder.to_owned(), made.clone());
            return Ok(made);
        }
        let (into, link) = self
            .seen_in(made.component, component)
            .ok_or_else(|| format!("{}'s component is not placed", source.name))?;
        // The bound sketch's plane as its definition has it, its own frame
        // and the link of a plane of a third component (mitcad#963); none
        // when it is placed on its plane by a placement of its own.
        let bound_plane = match self.doc.feature(made.uid).map(|f| &f.def) {
            Some(FeatureDef::Sketch(def)) if def.placement.is_none() => {
                serde_json::to_value(&def.plane).ok().map(|plane| {
                    (
                        plane,
                        def.frame
                            .as_ref()
                            .map(|f| serde_json::to_value(f).expect("serializes")),
                        def.plane_link.clone(),
                    )
                })
            }
            _ => None,
        };
        // The bound sketch's profile curves and points, in this component.
        let (source_frame, curves, points) = {
            let output = self
                .doc
                .sketch_output(made.uid)
                .ok_or_else(|| format!("{} has no result", source.name))?;
            let Some(FeatureDef::Sketch(def)) = self.doc.feature(made.uid).map(|f| &f.def) else {
                return Err(format!("{} is not a sketch here", source.name));
            };
            let frame = output.frame;
            let mut curves: Vec<(u32, Curve3)> = Vec::new();
            for e in def
                .entities
                .iter()
                .filter(|e| !e.is_point() && !e.construction)
            {
                let c = output
                    .solved
                    .curves
                    .get(&e.id)
                    .and_then(|c| model_curve(&frame, c))
                    .ok_or_else(|| format!("{}'s curve c{} has no place", source.name, e.id.0))?;
                curves.push((e.id.0, transform_curve(&c, &into)));
            }
            let mut points: Vec<(i32, u32, Curve3)> = made
                .points
                .iter()
                .filter_map(|(g, id)| {
                    let at = output.solved.points.get(&EntityUid(*id))?;
                    Some((*g, *id, Curve3::Point(into.apply_point(frame.point(*at)))))
                })
                .collect();
            points.sort_by_key(|(g, _, _)| *g);
            (frame, curves, points)
        };
        let frame = into.apply_frame(&source_frame);
        let geo_of: std::collections::HashMap<u32, i32> =
            made.curves.iter().map(|(g, id)| (*id, *g)).collect();
        let mut next = 1u32;
        let mut sketch_entities = Vec::new();
        let mut projections = Vec::new();
        let mut curve_ids = std::collections::HashMap::new();
        let mut point_ids = std::collections::HashMap::new();
        let mut external_ids = std::collections::HashMap::new();
        let external_of: std::collections::HashMap<u32, usize> =
            made.externals.iter().map(|(k, id)| (*id, *k)).collect();
        let pieces = curves
            .iter()
            .map(|(id, c)| {
                (
                    json!({"curve": format!("c{id}")}),
                    c,
                    geo_of.get(id),
                    Some(*id),
                )
            })
            .chain(
                points
                    .iter()
                    .map(|(g, id, c)| (json!({"point": format!("p{id}")}), c, Some(g), None)),
            );
        for (key, curve, geo, source_curve) in pieces {
            let point = source_curve.is_none();
            let projected = project(std::slice::from_ref(curve), &frame);
            let made_here = entities(&projected, &mut || {
                next += 1;
                EntityUid(next - 1)
            });
            let Some(main) = made_here.iter().find(|e| e.is_point() == point) else {
                return Err(format!(
                    "{}'s {} does not project into the binder's sketch",
                    source.name,
                    key.as_object()
                        .and_then(|m| m.values().next())
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                ));
            };
            if let Some(g) = geo {
                if point {
                    point_ids.insert(*g, main.id.0);
                } else {
                    curve_ids.insert(*g, main.id.0);
                }
            }
            // (1.1's defining external geometry: a hole drills at its
            // circles too.)
            if let Some(k) = source_curve.and_then(|id| external_of.get(&id)) {
                external_ids.insert(*k, main.id.0);
            }
            let mut record = json!({"source": {"sketch": made.uid.to_string()}});
            for (k, v) in key.as_object().into_iter().flatten() {
                record["source"][k.as_str()] = v.clone();
            }
            if let Some(link) = &link {
                record["link"] = link.clone();
            }
            let refs: Vec<String> = made_here.iter().map(|e| e.as_ref().to_string()).collect();
            record["entities"] = json!(refs);
            projections.push(record);
            push_entities(&mut sketch_entities, &made_here, true);
        }
        let label = o.label().to_owned();
        // On the bound sketch's own plane, seen through the occurrences
        // (`plane_link`, mitcad#963), so that it follows that plane too;
        // the plane of a third component (the bound sketch's own
        // `plane_link`) through its occurrence seen from here.
        let linked = bound_plane.and_then(|(plane, own_frame, own_link)| {
            let plane_link = match own_link {
                None => link.clone(),
                Some(l) => {
                    let here = self.occurrence_text(component)?;
                    let source = l.source.to_string();
                    (source != here).then(|| json!({"source": source, "target": here}))
                }
            };
            let mut def = json!({"type": "sketch", "plane": plane, "entities": sketch_entities,
                                 "constraints": [], "dimensions": [],
                                 "projections": projections});
            if let Some(f) = own_frame {
                def["frame"] = f;
            }
            if let Some(l) = plane_link {
                def["plane_link"] = l;
            }
            Some(def)
        });
        let depth = self.doc.undo_depth();
        let on_bound_plane = match linked {
            Some(def) => match self.add_sketch_def(&def, &label, component) {
                Ok(uid) if self.sketch_at(uid, &frame) => Some(uid),
                Ok(_) => {
                    self.undo_to(depth);
                    None
                }
                Err(_) => None,
            },
            None => None,
        };
        let uid = match on_bound_plane {
            Some(uid) => uid,
            None => {
                // Its plane not found here (a face that later features of
                // the other Body took away): the origin plane parallel to
                // it, else a fixed plane.
                let (plane, plane_frame) = match origin_plane(&frame, false) {
                    Some((p, text)) => (json!(text), p.origin_frame().expect("an origin plane")),
                    None => {
                        let def = json!({"type": "construction_plane",
                            "definition": {"type": "fixed", "origin": frame.origin,
                                           "x_axis": frame.x_axis, "y_axis": frame.y_axis}});
                        let (uid, f) =
                            self.added_plane(&def, &frame, component, &format!("{label} plane"))?;
                        (json!(uid.to_string()), f)
                    }
                };
                let mut def = json!({"type": "sketch", "plane": plane,
                                     "entities": sketch_entities, "constraints": [],
                                     "dimensions": [], "projections": projections});
                if let Some(f) = geom::relative_frame(&plane_frame, &frame) {
                    def["frame"] = serde_json::to_value(f).expect("serializes");
                }
                self.add_sketch_def(&def, &label, component)?
            }
        };
        let _ = self.doc.set_feature_visible(uid, false);
        let output_frame = self.doc.sketch_output(uid).map_or(frame, |o| o.frame);
        let made = SketchMade {
            uid,
            component,
            frame: output_frame,
            curves: curve_ids,
            points: point_ids,
            axes: [None, None],
            externals: external_ids,
        };
        self.replay.sketches.insert(binder.to_owned(), made.clone());
        Ok(made)
    }

    /// Whether a sketch solved where `frame` is (its origin within the
    /// sketch tolerance, its axes the same).
    fn sketch_at(&self, uid: FeatureUid, frame: &SketchFrame) -> bool {
        self.doc.sketch_output(uid).is_some_and(|o| {
            let f = o.frame;
            norm(sub(f.origin, frame.origin)) <= SKETCH_TOLERANCE
                && norm(sub(f.x_axis, frame.x_axis)) <= 1e-6
                && norm(sub(f.y_axis, frame.y_axis)) <= 1e-6
        })
    }

    /// A component's occurrence path from the root as a link names it
    /// (`"O1/O4"`, empty for the root); None when it is not placed.
    fn occurrence_text(&self, component: ComponentUid) -> Option<String> {
        let path = if component.is_root() {
            Vec::new()
        } else {
            self.component_paths.get(&component).cloned()?
        };
        Some(
            path.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/"),
        )
    }

    /// The binder a sketch is attached to a face of (its one support, a
    /// binder's `Face<n>`), mitcad#963.
    fn face_binder(&self, object: &Object) -> Option<String> {
        let support: Vec<(String, String)> = ["AttachmentSupport", "Support"]
            .iter()
            .flat_map(|p| object.links(p))
            .flat_map(|l| {
                l.subs
                    .iter()
                    .map(|s| (l.object.clone(), super::elements::sub_name(s).to_owned()))
                    .collect::<Vec<_>>()
            })
            .collect();
        let [(target, element)] = support.as_slice() else {
            return None;
        };
        (element.starts_with("Face")
            && self.sources[0]
                .object(target)
                .is_some_and(|o| is_binder(&o.type_name)))
        .then(|| target.clone())
    }

    /// A sketch attached to a face of a binder of one whole sketch (the
    /// face the binder makes of it): on the plane of the binder's sketch in
    /// the sketch's component ([`Self::binder_sketch`], made before the
    /// sketch's attempts), with its `plane_link`, when the binder's sketch
    /// lies in the sketch's plane (mitcad#963). Another component's plane
    /// comes as `{"plane": …, "plane_link": …}`.
    fn binder_face_plane(
        &self,
        binder: &str,
        frame: &SketchFrame,
        component: ComponentUid,
    ) -> Option<(Value, SketchFrame, Vec<String>, String)> {
        let o = self.sources[0].object(binder)?;
        if !is_binder(&o.type_name) {
            return None;
        }
        let bound = self.sketches_of(o)?.first()?.name.clone();
        let made = self
            .replay
            .sketches
            .get(binder)
            .filter(|m| m.component == component)?;
        let Some(FeatureDef::Sketch(def)) = self.doc.feature(made.uid).map(|f| &f.def) else {
            return None;
        };
        if def.placement.is_some() {
            return None;
        }
        // The binder's sketch lies where the bound sketch does (its plane
        // may be a parallel one, its own frame offset from it).
        let base = self.doc.sketch_output(made.uid)?.base;
        if !geom::coplanar(frame, base.origin, base.normal(), SKETCH_TOLERANCE) {
            return None;
        }
        let on = plane_frame(&base, def.frame.as_deref());
        let plane = serde_json::to_value(&def.plane).ok()?;
        let value = match &def.plane_link {
            Some(link) => json!({"plane": plane, "plane_link": serde_json::to_value(link).ok()?}),
            None => plane,
        };
        Some((
            value,
            on,
            vec![format!("on the plane of {bound}, which {binder} binds")],
            format!("on {bound}'s plane"),
        ))
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
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => Curve3::Helix {
            center: t.apply_point(*center),
            axis: t.apply_vector(*axis),
            x_axis: t.apply_vector(*x_axis),
            radius: *radius,
            pitch: *pitch,
            start: *start,
            end: *end,
        },
    }
}

/// A solved sketch curve in the sketch's component, as the model places a
/// sketch's curve to project it (mitcad#904): arcs counter-clockwise from
/// their start about the frame's normal.
fn model_curve(frame: &SketchFrame, curve: &Curve2) -> Option<Curve3> {
    use std::f64::consts::TAU;
    let normal = frame.normal();
    let direction = |angle: f64| -> [f64; 3] {
        std::array::from_fn(|i| angle.cos() * frame.x_axis[i] + angle.sin() * frame.y_axis[i])
    };
    let after = |start: f64, end: f64| if end <= start { end + TAU } else { end };
    let conic = |center, x_axis, major, minor, start, end, closed| Curve3::Conic {
        center: frame.point(center),
        normal,
        x_axis,
        major,
        minor,
        start,
        end,
        closed,
    };
    Some(match curve {
        Curve2::Line { a, b } => Curve3::Line {
            start: frame.point(*a),
            end: frame.point(*b),
        },
        Curve2::Circle {
            center,
            radius,
            start,
        } => conic(
            *center,
            frame.x_axis,
            *radius,
            *radius,
            *start,
            start + TAU,
            true,
        ),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => conic(
            *center,
            frame.x_axis,
            *radius,
            *radius,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Ellipse {
            center,
            major,
            minor,
            rotation,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            0.0,
            TAU,
            true,
        ),
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            rotation,
            start,
            end,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Nurbs(n) => Curve3::BSpline {
            degree: u32::try_from(n.degree).ok()?,
            poles: n.control.iter().map(|p| frame.point(*p)).collect(),
            weights: n.weights.clone(),
            knots: n.knots.clone(),
        },
    })
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
            start: 0.0,
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

/// Projects `curves` (in the sketch's component) for the external geometry
/// `geos` of one link and keeps them as a linked projection (`record`
/// with its entities) when they match FreeCAD's; false otherwise.
fn push_projection(
    out: &mut External,
    sketch: &fc::Sketch,
    geos: &[usize],
    frame: &SketchFrame,
    defining: bool,
    mut record: Value,
    curves: &[Curve3],
) -> bool {
    let projected = project(curves, frame);
    let Some((made, maps)) = map_external(sketch, geos, &projected, out.next) else {
        return false;
    };
    out.next += made.len() as u32;
    let refs: Vec<String> = made.iter().map(|e| e.as_ref().to_string()).collect();
    record["entities"] = json!(refs);
    out.projections.push(record);
    push_entities(&mut out.entities, &made, defining);
    for (k, map) in geos.iter().zip(maps) {
        out.maps[*k] = Some(map);
    }
    true
}

/// Keeps one link's external geometry as linked projections of `pieces`
/// (records and their curves): one projection of them all for one piece; a
/// projection per FreeCAD curve for several (a binder's face: each of
/// FreeCAD's curves the sketch's curve it lies along, mitcad#952, which
/// needs FreeCAD's copies of them). False, and nothing kept, when they do
/// not match FreeCAD's.
fn push_pieces(
    out: &mut External,
    sketch: &fc::Sketch,
    geos: &[usize],
    frame: &SketchFrame,
    defining: bool,
    pieces: &[(Value, Vec<Curve3>)],
) -> bool {
    if let [(record, curves)] = pieces {
        return push_projection(out, sketch, geos, frame, defining, record.clone(), curves);
    }
    if pieces.is_empty() || geos.iter().any(|k| sketch.external[*k].is_none()) {
        return false;
    }
    let mut trial = External {
        maps: vec![None; out.maps.len()],
        next: out.next,
        ..External::default()
    };
    for k in geos {
        let one = std::slice::from_ref(k);
        let found = pieces.iter().any(|(record, curves)| {
            push_projection(
                &mut trial,
                sketch,
                one,
                frame,
                defining,
                record.clone(),
                curves,
            )
        });
        if !found {
            return false;
        }
    }
    out.next = trial.next;
    out.entities.extend(trial.entities);
    out.projections.extend(trial.projections);
    for (k, map) in trial.maps.into_iter().enumerate() {
        if map.is_some() {
            out.maps[k] = map;
        }
    }
    true
}

/// A PartDesign binder of other objects' shapes.
pub(super) fn is_binder(t: &str) -> bool {
    matches!(t, "PartDesign::SubShapeBinder" | "PartDesign::ShapeBinder")
}

/// The frame of the plane of a sketch whose base frame is `base` and whose
/// own frame in that plane is `own` ([`FrameDef::apply`] undone): the plane
/// frame's axes are the base frame's turned back by the own frame's
/// (mitcad#963).
fn plane_frame(base: &SketchFrame, own: Option<&FrameDef>) -> SketchFrame {
    let Some(f) = own else {
        return *base;
    };
    let z = cross(f.x_axis, f.y_axis);
    let n = base.normal();
    let back = |i: usize| -> [f64; 3] {
        add(
            add(
                scale(base.x_axis, f.x_axis[i]),
                scale(base.y_axis, f.y_axis[i]),
            ),
            scale(n, z[i]),
        )
    };
    let (x, y, normal) = (back(0), back(1), back(2));
    let o = f.origin;
    let offset = add(add(scale(x, o[0]), scale(y, o[1])), scale(normal, o[2]));
    SketchFrame {
        origin: sub(base.origin, offset),
        x_axis: x,
        y_axis: y,
    }
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
