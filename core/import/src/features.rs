// SPDX-License-Identifier: MIT
//! Timeline items as Mitcad feature definitions (`commands.md`; the IR
//! field mappings of the F1, F2 and F4 import tables).
//!
//! Each item gives candidates, best first. What the dump leaves open (the
//! stream decoder knows how many profiles an extrude uses but not which,
//! the direction may be uncertain, fillet edges are not decoded) becomes
//! several candidates, and the ASM history picks the one whose result
//! the file has; without a history only the first, which is not a guess,
//! is used.

use std::collections::{BTreeSet, HashMap};

use mitcad_f3d::design::ir::{
    ChamferDetail, CoilDetail, Definition, Detail, EdgeSet, ExtrudeDetail, ExtrudeF3d,
    FilletDetail, Fingerprint, Geometry, Reference, RevolveDetail, SketchDetail, TimelineItem,
};
use mitcad_model::assembly::inverse;
use mitcad_model::datum::Datum;
use mitcad_model::features::SketchPlane;
use mitcad_model::sketch::regions::Region;
use mitcad_model::{
    BodyUid, ComponentUid, FaceName, FeatureUid, Kernel, SketchFrame, TopoName, Transform,
};
use serde_json::{Value, json};

use crate::geom::{self, mm, mm3};
use crate::history::{Sig, StoredBody};
use crate::refs::Section;
use crate::report::Outcome;
use crate::sketch::{self, position_error};
use crate::{Candidate, Guesses, Importer, SketchInfo, refs};

/// Items whose candidates depend on the bodies before them (edges found
/// against the replay, faces extruded), so a fallback before them may make
/// them work.
pub fn depends_on_bodies(item: &TimelineItem) -> bool {
    match item.object_type() {
        Some("FilletFeature" | "ChamferFeature" | "CombineFeature" | "ReplaceFaceFeature") => true,
        // Paths, rails or sections of edges (mitcad#34).
        Some("SweepFeature" | "PipeFeature" | "LoftFeature") => crate::sweeps::uses_edges(item),
        // An extrusion of a face (no profile decoded).
        Some("ExtrudeFeature") => match &item.detail {
            Some(Detail::Extrude(d)) => profiles_of(&d.profile, &d.other).is_empty(),
            _ => false,
        },
        _ => false,
    }
}

/// The largest error of solved sketch points accepted, mm.
const SKETCH_TOLERANCE: f64 = 1e-4;

fn param(r: &Option<Reference>) -> Option<&mitcad_f3d::design::ir::ParameterRef> {
    r.as_ref().and_then(Reference::parameter)
}

/// Why items of a type that changes bodies are not translated
/// (mitcad#43); the history's states stand in for them.
fn not_translated(object_type: &str) -> Option<&'static str> {
    Some(match object_type {
        "FlangeFeature" => "a sheet metal flange: not translated (Mitcad has no sheet metal)",
        "EmbossFeature" => "an emboss: not translated (Mitcad has no emboss feature)",
        "PCBFeature" => "an electronics board: not translated",
        "MoveFaceFeature" => "moved faces: not translated (its faces and motion are not decoded)",
        "DraftFeature" => "a draft: not translated (its faces and plane are not decoded)",
        "CylinderFeature" | "SphereFeature" => {
            "a primitive: not translated (the face or plane it is placed on is not decoded)"
        }
        "MirrorComponent" => {
            "mirrors a component into a new one: not translated (the stored bodies stand in)"
        }
        "DerivedContext" => {
            "brings in bodies of another design: not translated (the stored bodies stand in)"
        }
        "ComponentFromBodies" => {
            "moves bodies into a new component: not translated (the stored bodies stand in)"
        }
        "CopyPasteBodies" => "copies or moves bodies: not translated (the stored bodies stand in)",
        _ => return None,
    })
}

/// An extrusion's extent to try: (extent, flip, guess, length swept in mm
/// when known).
type ExtentChoice = (Value, bool, bool, Option<f64>);

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// A parameter reference as a value: the imported parameter's name, or
    /// its value (mm or rad).
    pub(crate) fn value(&self, r: &Option<Reference>) -> Option<Value> {
        let p = param(r)?;
        if let Some(name) = p.name.as_deref().and_then(|n| self.params.get(n)) {
            return Some(json!(name));
        }
        let v = p.value?;
        // Angles are radians in both; counts (unit "") have no unit.
        let raw = p
            .unit
            .as_deref()
            .is_some_and(|u| u.is_empty() || u.contains("deg") || u.contains("rad"));
        Some(json!(if raw { v } else { mm(v) }))
    }

    fn value_or(&self, r: &Option<Reference>, what: &str) -> Result<Value, String> {
        self.value(r).ok_or_else(|| format!("no {what}"))
    }

    /// Whether a parameter's value is zero (an unused taper).
    fn is_zero(r: &Option<Reference>) -> bool {
        param(r).and_then(|p| p.value).is_none_or(|v| v == 0.0)
    }

    // Sketches.

    /// Imports a sketch; `copy` imports it again into the item's component,
    /// moved from its own component's coordinates by the transform (see
    /// [`Importer::copy_sketch`]).
    pub(crate) fn import_sketch(
        &mut self,
        index: i64,
        item: &TimelineItem,
        copy: Option<&Transform>,
    ) -> Result<(FeatureUid, Outcome, String), String> {
        let Some(Detail::Sketch(detail)) = &item.detail else {
            return Err("no sketch data".to_owned());
        };
        let frame = sketch_frame(detail).ok_or("the sketch's placement was not decoded")?;
        let frame = copy.map_or(frame, |t| t.apply_frame(&frame));
        let mut parts = sketch::translate(detail, &self.params);
        let name = item.name();
        let start = self.doc.undo_depth();
        let owned = self.components.is_owned(index);
        let (plane, plane_frame, mut notes) =
            self.sketch_plane(detail, &frame, name, copy, owned)?;
        let frame_def = geom::relative_frame(&plane_frame, &frame);
        let made_params = self.offset_params(&mut parts);
        let levels: [(&[Value], &[Value], &str); 3] = [
            (&parts.constraints, &parts.dimensions, ""),
            (
                &parts.constraints,
                &parts.driven_dimensions,
                "dimensions driven",
            ),
            (
                &[],
                &parts.driven_dimensions,
                "without constraints, dimensions driven",
            ),
        ];
        let mut last = String::new();
        for (level, (constraints, dimensions, what)) in levels.iter().enumerate() {
            let mut def = json!({"type": "sketch", "plane": plane, "entities": parts.entities,
                                 "constraints": constraints, "dimensions": dimensions,
                                 "texts": parts.texts});
            // Patterns and offsets go with the constraints.
            if level < 2 {
                def["patterns"] = json!(parts.patterns);
                def["offsets"] = json!(parts.offsets);
            }
            if let Some(f) = &frame_def {
                def["frame"] = serde_json::to_value(f).expect("serializes");
            }
            let depth = self.doc.undo_depth();
            match self.add(&def, name) {
                Ok(uid) => {
                    let error = self.doc.sketch_output(uid).map_or(f64::INFINITY, |o| {
                        position_error(&parts.positions, &o.solved.points)
                    });
                    if error > SKETCH_TOLERANCE {
                        last = format!("the solved sketch moved by {error:.3e} mm");
                        self.undo_to(depth);
                        continue;
                    }
                    self.sketches.insert(
                        index,
                        SketchInfo {
                            uid,
                            ids: parts.ids.clone(),
                            frame,
                        },
                    );
                    self.adopt(index, uid);
                    if !made_params.is_empty() {
                        let _ = self.doc.adopt_parameters(uid, &made_params);
                    }
                    if level > 0 {
                        notes.push((*what).to_owned());
                    }
                    if !parts.dropped.is_empty() {
                        notes.push(format!("left out: {}", summarize(&parts.dropped)));
                    }
                    let partial = level > 0 || !parts.dropped.is_empty();
                    let outcome = if partial {
                        Outcome::Partial
                    } else {
                        Outcome::Parametric
                    };
                    return Ok((uid, outcome, notes.join("; ")));
                }
                Err(e) => last = e,
            }
        }
        // Not the plane made for it either.
        self.undo_to(start);
        Err(last)
    }

    /// Creates the parameters a sketch's offsets need (the magnitude of a
    /// negative distance parameter, shared by the offset's dimensions and
    /// record) and puts their names in place of the placeholders. Returns
    /// their names, for the sketch to adopt.
    fn offset_params(&mut self, parts: &mut sketch::SketchParts) -> Vec<String> {
        let mut made = Vec::new();
        for (placeholder, suggested, expression) in std::mem::take(&mut parts.new_params) {
            let name = (0..)
                .map(|i| {
                    if i == 0 {
                        suggested.clone()
                    } else {
                        format!("{suggested}_{i}")
                    }
                })
                .find(|n| self.doc.parameters().find(n).is_none())
                .expect("a free name");
            let comment = format!("offset distance, {expression}");
            let text = match self
                .doc
                .add_parameter_expression(&name, &expression, None, &comment)
            {
                Ok(()) => {
                    made.push(name.clone());
                    name
                }
                // Each value its own expression.
                Err(_) => expression,
            };
            for v in parts
                .dimensions
                .iter_mut()
                .chain(parts.offsets.iter_mut())
                .chain(parts.patterns.iter_mut())
            {
                replace_text(v, &placeholder, &text);
            }
        }
        made
    }

    /// The plane of a sketch as the definition's `plane`, with that plane's
    /// frame: an origin plane, an imported construction plane, a planar
    /// face of the replay, or a fixed construction plane made for it. A
    /// copy in another component's coordinates takes planes of that
    /// component only: one of its faces, else a fixed plane (its origin
    /// planes when it is placed as the sketch's own component). So does a
    /// sketch whose component the decoder gives (`owned`, mitcad#37): the
    /// faces of other components are in their own coordinates.
    fn sketch_plane(
        &mut self,
        detail: &SketchDetail,
        frame: &SketchFrame,
        name: Option<&str>,
        copy: Option<&Transform>,
        owned: bool,
    ) -> Result<(Value, SketchFrame, Vec<String>), String> {
        let moved = copy.is_some_and(|t| !t.is_identity());
        let component = (copy.is_some() || owned).then_some(self.component);
        if let Some(Reference::ConstructionPlane(c)) = &detail.reference_plane
            && !moved
        {
            let origin = c.origin.clone().flatten();
            let plane = match origin.as_deref() {
                Some("XY") => Some((SketchPlane::Xy, "xy")),
                Some("XZ") => Some((SketchPlane::Xz, "xz")),
                Some("YZ") => Some((SketchPlane::Yz, "yz")),
                _ => None,
            };
            if let Some((plane, text)) = plane {
                let f = plane.origin_frame().expect("an origin plane");
                return Ok((json!(text), f, Vec::new()));
            }
            if let Some(uid) = c
                .timeline_index
                .flatten()
                .and_then(|i| self.features.get(&i))
                .copied()
                && let Some(Datum::Plane(p)) = self.doc.datum(uid)
            {
                return Ok((json!(uid.to_string()), p.frame(), Vec::new()));
            }
        }
        // A planar face of the replay in the sketch's plane.
        let normal = geom::normal(frame);
        let mut faces: Vec<_> = refs::planar_faces(self.doc)
            .into_iter()
            .filter(|(b, _, p)| {
                geom::coplanar(frame, p.origin, p.normal, 1e-4)
                    && component.is_none_or(|c| self.doc.body_component(*b) == Some(c))
            })
            .collect();
        // A sketch on a face looks along the face's outward normal.
        faces.sort_by_key(|(_, _, p)| geom::dot(p.normal, normal) < 0.0);
        if let Some((body, face, p)) = faces.first() {
            let f = SketchPlane::face_frame(p.origin, p.normal);
            return Ok((
                json!({"face": face.to_string(), "body": body.to_string()}),
                f,
                Vec::new(),
            ));
        }
        // A fixed construction plane in the sketch's place.
        let def = json!({"type": "construction_plane", "definition": {"type": "fixed",
            "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}});
        let plane_name = name.map(|n| format!("{n} plane"));
        let uid = self.add(&def, plane_name.as_deref())?;
        let f = match self.doc.datum(uid) {
            Some(Datum::Plane(p)) => p.frame(),
            _ => *frame,
        };
        Ok((
            json!(uid.to_string()),
            f,
            vec![format!("on a fixed plane {uid} (its face was not found)")],
        ))
    }

    // Construction geometry.

    /// Construction geometry: parametric where the dump tells what it is
    /// measured from, else fixed in place (notes starting "a fixed").
    pub(crate) fn translate_datum(
        &mut self,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let detail = match &item.detail {
            Some(Detail::ConstructionPlane(d)) => d,
            _ => {
                // Axes and points: fixed from their geometry when dumped.
                return self.fixed_datum(item).map(|c| vec![c]);
            }
        };
        let geometry = detail.geometry.clone().unwrap_or_default();
        let (Some(origin), Some(normal)) = (geometry.origin, geometry.normal) else {
            return Err("the plane's geometry was not decoded".to_owned());
        };
        let origin = mm3(origin);
        let normal = geom::unit(normal).ok_or("a plane without a normal")?;
        let mut candidates = Vec::new();
        if let Some(def) = &detail.definition
            && def.definition_type.as_deref() == Some("ConstructionPlaneOffsetDefinition")
            && let Some(offset) = self.value(&def.offset)
        {
            let d = param(&def.offset)
                .and_then(|p| p.value)
                .map(mm)
                .unwrap_or(0.0);
            for base in self.offset_bases(def, origin, normal, d) {
                candidates.push(Candidate::new(json!({"type": "construction_plane",
                    "definition": {"type": "offset", "plane": base, "distance": offset}})));
            }
        }
        // Mid planes and planes at an angle (the `.ipt` import's): each
        // pairing of the planes and lines they may be measured from, an
        // angle either way round; `datum_item` keeps the one in the
        // plane's place.
        let entity = |key: &str| {
            detail
                .definition
                .as_ref()
                .and_then(|d| d.other.get(key))
                .and_then(Value::as_object)
                .map(|m| Reference::from_map(m.clone()))
        };
        match detail
            .definition
            .as_ref()
            .and_then(|d| d.definition_type.as_deref())
        {
            Some("ConstructionPlaneMidplaneDefinition") => {
                let one = self.plane_options(entity("planarEntityOne").as_ref());
                let two = self.plane_options(entity("planarEntityTwo").as_ref());
                for a in &one {
                    for b in &two {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "midplane", "plane1": a, "plane2": b}})));
                    }
                }
            }
            Some("ConstructionPlaneAtAngleDefinition") => {
                let def = detail.definition.as_ref().expect("matched");
                let planes = self.plane_options(def.planar_entity.as_ref());
                let lines = self.axis_options(entity("linearEntity").as_ref());
                if let Some(angle) = self.value(&def.angle) {
                    let negated = match &angle {
                        Value::String(s) => json!(format!("-({s})")),
                        v => json!(-v.as_f64().unwrap_or(0.0)),
                    };
                    for line in &lines {
                        for plane in &planes {
                            for a in [&angle, &negated] {
                                candidates.push(Candidate::new(
                                    json!({"type": "construction_plane",
                                    "definition": {"type": "angle", "line": line, "angle": a,
                                                   "plane": plane}}),
                                ));
                            }
                        }
                    }
                }
            }
            // Planes through points and lines (the `.ipt` import's): each
            // pairing of what they may be, the lines either way round.
            Some("ConstructionPlaneThreePointsDefinition") => {
                let p = |k: &str| self.point_options(entity(k).as_ref());
                let (a, b, c) = (
                    p("pointEntityOne"),
                    p("pointEntityTwo"),
                    p("pointEntityThree"),
                );
                for x in &a {
                    for y in &b {
                        for z in &c {
                            candidates.push(Candidate::new(json!({"type": "construction_plane",
                                "definition": {"type": "three_points", "point1": x,
                                               "point2": y, "point3": z}})));
                        }
                    }
                }
            }
            Some("ConstructionPlaneTwoEdgesDefinition") => {
                let one = self.axis_options(entity("linearEntityOne").as_ref());
                let two = self.axis_options(entity("linearEntityTwo").as_ref());
                for (a, b) in one.iter().flat_map(|a| two.iter().map(move |b| (a, b))) {
                    for (l1, l2) in [(a, b), (b, a)] {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "two_edges", "line1": l1, "line2": l2}})));
                    }
                }
            }
            Some("ConstructionPlaneLineAndPointDefinition") => {
                let lines = self.axis_options(entity("linearEntity").as_ref());
                let points = self.point_options(entity("pointEntity").as_ref());
                for line in &lines {
                    for point in &points {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "edge_and_point", "line": line,
                                           "point": point}})));
                    }
                }
            }
            Some("ConstructionPlaneNormalToLineDefinition") => {
                // Normal to the line where it is nearest to the point: the
                // line as a straight path about that place.
                let line = match entity("linearEntity") {
                    Some(Reference::ConstructionAxis(c)) => {
                        match c.origin.clone().flatten().as_deref() {
                            Some("X") => Some(([0.0; 3], [1.0, 0.0, 0.0])),
                            Some("Y") => Some(([0.0; 3], [0.0, 1.0, 0.0])),
                            Some("Z") => Some(([0.0; 3], [0.0, 0.0, 1.0])),
                            _ => c
                                .geometry
                                .as_ref()
                                .and_then(|g| Some((mm3(g.origin?), geom::unit(g.direction?)?))),
                        }
                    }
                    _ => None,
                };
                if let Some((o, v)) = line {
                    let at = geom::add(o, geom::scale(v, geom::dot(geom::sub(origin, o), v)));
                    let path = json!({"start": geom::sub(at, geom::scale(v, 100.0)),
                                      "end": geom::add(at, geom::scale(v, 100.0))});
                    for point in self.point_options(entity("pointEntity").as_ref()) {
                        candidates.push(Candidate::new(json!({"type": "construction_plane",
                            "definition": {"type": "normal_at_point", "path": path,
                                           "point": point}})));
                    }
                }
            }
            _ => {}
        }
        let x = geometry
            .u_direction
            .and_then(geom::unit)
            .unwrap_or_else(|| default_x(normal));
        let y = geom::cross(normal, x);
        candidates.push(
            Candidate::new(
                json!({"type": "construction_plane", "definition": {"type": "fixed",
                "origin": origin, "x_axis": x, "y_axis": y}}),
            )
            .with_note("a fixed plane"),
        );
        Ok(candidates)
    }

    /// A plane reference as the planes it may be: an origin plane, an
    /// imported construction plane, a face it names, or by its geometry
    /// the origin planes and planar faces in it, else that plane fixed.
    fn plane_options(&self, r: Option<&Reference>) -> Vec<Value> {
        let mut out = Vec::new();
        let Some(r) = r else {
            return out;
        };
        if let Reference::ConstructionPlane(c) = r
            && c.origin.clone().flatten().is_none()
            && c.timeline_index.flatten().is_none()
            && let Some(g) = &c.geometry
            && let (Some(o), Some(n)) = (g.origin, g.normal.and_then(geom::unit))
        {
            let o = mm3(o);
            let on = |p: [f64; 3], m: [f64; 3]| {
                geom::norm(geom::cross(n, m)) < 1e-9 && geom::dot(geom::sub(p, o), n).abs() < 1e-4
            };
            for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
                let mut m = [0.0; 3];
                m[axis] = 1.0;
                if on([0.0; 3], m) {
                    out.push(json!(text));
                }
            }
            for (body, face, p) in refs::planar_faces(self.doc) {
                if on(p.origin, p.normal) {
                    out.push(json!({"body": body.to_string(), "face": face.to_string()}));
                }
            }
        }
        out.extend(self.plane_ref(r));
        out
    }

    /// A line reference as the axes it may be: an origin axis, an imported
    /// construction axis, by its geometry an origin axis on it, else that
    /// axis fixed.
    fn axis_options(&self, r: Option<&Reference>) -> Vec<Value> {
        let mut out = Vec::new();
        let Some(Reference::ConstructionAxis(c)) = r else {
            return out;
        };
        match c.origin.clone().flatten().as_deref() {
            Some(o @ ("X" | "Y" | "Z")) => return vec![json!(o.to_lowercase())],
            _ => {
                if let Some(uid) = c
                    .timeline_index
                    .flatten()
                    .and_then(|i| self.features.get(&i))
                {
                    return vec![json!(uid.to_string())];
                }
            }
        }
        let Some(g) = &c.geometry else {
            return out;
        };
        let (Some(o), Some(v)) = (g.origin, g.direction.and_then(geom::unit)) else {
            return out;
        };
        let o = mm3(o);
        let on = |p: [f64; 3]| {
            let d = geom::sub(p, o);
            geom::norm(geom::sub(d, geom::scale(v, geom::dot(d, v)))) < 1e-4
        };
        for (text, axis) in [("x", 0), ("y", 1), ("z", 2)] {
            let mut m = [0.0; 3];
            m[axis] = 1.0;
            if on([0.0; 3]) && geom::norm(geom::cross(v, m)) < 1e-9 {
                out.push(json!(text));
            }
        }
        out.push(json!({"origin": o, "direction": v}));
        out
    }

    /// A point reference as the points it may be: the origin, an imported
    /// construction point, else that point fixed.
    fn point_options(&self, r: Option<&Reference>) -> Vec<Value> {
        let Some(Reference::ConstructionPoint(c)) = r else {
            return Vec::new();
        };
        if c.origin.clone().flatten().is_some() {
            return vec![json!("origin")];
        }
        if let Some(uid) = c
            .timeline_index
            .flatten()
            .and_then(|i| self.features.get(&i))
        {
            return vec![json!(uid.to_string())];
        }
        c.geometry
            .as_ref()
            .and_then(|g| g.origin)
            .map(|p| vec![json!({"point": mm3(p)})])
            .unwrap_or_default()
    }

    /// The planes an offset plane may be measured from: its planar entity
    /// when the dump has it, else an origin plane or a planar face at
    /// `distance` behind it.
    fn offset_bases(
        &self,
        def: &Definition,
        origin: [f64; 3],
        normal: [f64; 3],
        distance: f64,
    ) -> Vec<Value> {
        let mut out = Vec::new();
        match &def.planar_entity {
            Some(Reference::ConstructionPlane(c)) => {
                match c.origin.clone().flatten().as_deref() {
                    Some(o @ ("XY" | "XZ" | "YZ")) => out.push(json!(o.to_lowercase())),
                    _ => {
                        if let Some(uid) = c
                            .timeline_index
                            .flatten()
                            .and_then(|i| self.features.get(&i))
                        {
                            out.push(json!(uid.to_string()));
                        } else if let Some(g) = &c.geometry
                            && let (Some(o), Some(n)) = (g.origin, g.normal.and_then(geom::unit))
                        {
                            // A plane off the timeline by its geometry (an
                            // `.ipt` part's work plane on a face): an
                            // origin plane or a planar face in it.
                            // With the offset along its normal giving the
                            // plane's place.
                            let o = mm3(o);
                            let close = |a: f64, b: f64| (a - b).abs() < 1e-6 * (1.0 + a.abs());
                            let on = |p: [f64; 3], m: [f64; 3]| {
                                geom::norm(geom::cross(n, m)) < 1e-9
                                    && geom::dot(geom::sub(p, o), n).abs() < 1e-4
                                    && close(geom::dot(geom::sub(origin, p), m), distance)
                            };
                            for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
                                let mut m = [0.0; 3];
                                m[axis] = 1.0;
                                if on([0.0; 3], m) {
                                    out.push(json!(text));
                                }
                            }
                            for (body, face, p) in refs::planar_faces(self.doc) {
                                if on(p.origin, p.normal) {
                                    out.push(json!({"body": body.to_string(),
                                                    "face": face.to_string()}));
                                }
                            }
                        }
                    }
                }
                return out;
            }
            Some(Reference::Face(fp)) => {
                if let Some((body, face)) = refs::resolve_face(self.doc, fp) {
                    out.push(json!({"body": body.to_string(), "face": face.to_string()}));
                }
                return out;
            }
            _ => {}
        }
        // The base is `distance` back along its own normal.
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6 * (1.0 + a.abs());
        for (text, axis) in [("xy", 2), ("xz", 1), ("yz", 0)] {
            let mut n = [0.0; 3];
            n[axis] = 1.0;
            if geom::norm(geom::cross(normal, n)) < 1e-9 && close(origin[axis], distance) {
                out.push(json!(text));
            }
        }
        for (body, face, p) in refs::planar_faces(self.doc) {
            if geom::norm(geom::cross(normal, p.normal)) < 1e-9
                && close(geom::dot(geom::sub(origin, p.origin), p.normal), distance)
            {
                out.push(json!({"body": body.to_string(), "face": face.to_string()}));
            }
        }
        out
    }

    fn fixed_datum(&self, item: &TimelineItem) -> Result<Candidate, String> {
        let detail = match &item.detail {
            Some(Detail::Other(map)) => map,
            _ => return Err("no geometry for the datum".to_owned()),
        };
        let g = detail.get("geometry").ok_or("no geometry for the datum")?;
        let v =
            |key: &str| -> Option<[f64; 3]> { serde_json::from_value(g.get(key)?.clone()).ok() };
        match item.object_type() {
            Some("ConstructionAxis") => {
                let origin = v("origin").ok_or("no axis origin")?;
                let direction = v("direction")
                    .and_then(geom::unit)
                    .ok_or("no axis direction")?;
                Ok(
                    Candidate::new(json!({"type": "construction_axis", "definition":
                    {"type": "fixed", "origin": mm3(origin), "direction": direction}}))
                    .with_note("a fixed axis"),
                )
            }
            Some("ConstructionPoint") => {
                let p: [f64; 3] = serde_json::from_value(g.clone()).map_err(|e| e.to_string())?;
                Ok(
                    Candidate::new(json!({"type": "construction_point", "definition":
                    {"type": "fixed", "point": mm3(p)}}))
                    .with_note("a fixed point"),
                )
            }
            _ => Err("not a datum".to_owned()),
        }
    }

    // Features.

    pub(crate) fn translate(
        &mut self,
        index: i64,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let translated = self.translate_item(index, item);
        // The learning dump keeps them (mitcad#96), outside the item's time.
        self.learn_offered_paused(index, item, &translated);
        translated
    }

    fn translate_item(
        &mut self,
        index: i64,
        item: &TimelineItem,
    ) -> Result<Vec<Candidate>, String> {
        let Some(object_type) = item.object_type() else {
            return Err("unknown timeline item (type not decoded)".to_owned());
        };
        if let Some(reason) = not_translated(object_type) {
            return Err(reason.to_owned());
        }
        match (&item.detail, object_type) {
            (Some(Detail::Extrude(d)), _) => self.extrude(index, item, d),
            (Some(Detail::Revolve(d)), _) => self.revolve(index, d),
            (Some(Detail::Fillet(d)), _) => self.fillet(d),
            (Some(Detail::Chamfer(d)), _) => self.chamfer(d),
            // Sweeps, pipes and lofts (F3).
            (Some(Detail::Sweep(d)), _) => self.sweep(index, d),
            (Some(Detail::Pipe(d)), _) => self.pipe(index, d),
            (Some(_), "LoftFeature") => self.loft(item),
            // Coils of sketch profiles (the `.ipt` import's).
            (Some(Detail::Coil(d)), _) if d.other.contains_key("profile") => {
                self.coil_helix(index, d)
            }
            (_, "BaseFeature") => Err("a base feature: the file's bodies as they are".to_owned()),
            (
                _,
                t @ ("CombineFeature"
                | "MirrorFeature"
                | "CircularPatternFeature"
                | "RectangularPatternFeature"
                | "ShellFeature"
                | "OffsetFacesFeature"
                | "MoveFeature"
                | "SplitBodyFeature"
                | "SplitFaceFeature"
                | "HoleFeature"
                | "ThreadFeature"
                | "ReplaceFaceFeature"),
            ) => self.translate_op(t, item),
            (None, t) => Err(format!("{t}: no details decoded")),
            (_, t) => Err(format!("{t} is not supported yet")),
        }
    }

    /// The sketch of profile references, or of the last sketch before the
    /// item when the dump does not name it.
    pub(crate) fn profile_sketch(
        &self,
        profiles: &[Reference],
        index: i64,
    ) -> Result<(i64, bool), String> {
        let named = profiles.iter().find_map(|p| match p {
            Reference::Profile(p) => p.sketch_timeline_index.flatten(),
            _ => None,
        });
        if let Some(i) = named {
            return if self.sketches.contains_key(&i) {
                Ok((i, false))
            } else {
                Err(format!("its sketch (item {i}) was not imported"))
            };
        }
        self.sketches
            .keys()
            .filter(|k| **k < index)
            .max()
            .map(|k| (*k, true))
            .ok_or_else(|| "no sketch before it".to_owned())
    }

    /// Region sets to try: matched by area and centroid when the dump has
    /// them (a region, each of several alike as a guess, or else a union of
    /// regions first), else every set of as many regions as the item used,
    /// largest first, with their total area (mm²).
    pub(crate) fn region_sets(
        &mut self,
        profiles: &[Reference],
        sketch: i64,
        depth: Option<f64>,
        target_area: Option<f64>,
    ) -> Result<Vec<RegionSet>, String> {
        let info = self.sketches[&sketch].clone();
        let output = self
            .doc
            .sketch_output(info.uid)
            .ok_or("its sketch did not evaluate")?;
        let regions: Vec<Region> = output.region_info.clone();
        let frame = output.frame;
        let regions = &regions[..];
        if regions.is_empty() {
            return Err("its sketch has no profiles".to_owned());
        }
        let key = |r: &Region| r.profile.key.to_string();
        // By area and centroid (external dumps' profiles).
        let measured: Vec<_> = profiles
            .iter()
            .filter_map(|p| match p {
                Reference::Profile(p) => Some((p.area?, p.centroid?)),
                _ => None,
            })
            .collect();
        let mut first: Vec<RegionSet> = Vec::new();
        if !measured.is_empty() && measured.len() == profiles.len() {
            // Each profile's regions of its area and centroid: several
            // when the sketch has regions alike (rings of one area about
            // one centre), and then each choice of regions apart from
            // the other profiles' is a guess. Without one,
            // a union of regions of that area and centroid (a line that
            // bounds another feature's profile splits this one in
            // Mitcad's sketch), tried before the sets found without the
            // measures.
            let mut order: Vec<&Region> = regions.iter().collect();
            order.sort_by(|a, b| b.area.total_cmp(&a.area));
            let areas: Vec<f64> = order.iter().map(|r| r.area).collect();
            let mut unions = false;
            let found: Vec<Vec<Vec<&Region>>> = measured
                .iter()
                .map(|&(area, centroid)| {
                    let area = area * 100.0;
                    let c = [mm(centroid[0]), mm(centroid[1])];
                    let tol = 1e-3 * area.sqrt().max(1.0);
                    let fits = |set: &[&Region]| {
                        let a: f64 = set.iter().map(|r| r.area).sum();
                        let m = set.iter().fold([0.0, 0.0], |m, r| {
                            [m[0] + r.centroid[0] * r.area, m[1] + r.centroid[1] * r.area]
                        });
                        a > 0.0
                            && (a - area).abs() <= 1e-3 * area.abs().max(1e-6)
                            && (m[0] / a - c[0]).hypot(m[1] / a - c[1]) <= tol
                    };
                    let single: Vec<Vec<&Region>> = regions
                        .iter()
                        .filter(|r| fits(&[*r]))
                        .map(|r| vec![r])
                        .collect();
                    if !single.is_empty() {
                        return single;
                    }
                    unions = true;
                    // (The search may give a set more than once.)
                    let mut sets = area_subsets(&areas, area, 12);
                    sets.sort();
                    sets.dedup();
                    sets.into_iter()
                        .map(|s| s.into_iter().map(|i| order[i]).collect::<Vec<_>>())
                        .filter(|s| s.len() > 1 && fits(s))
                        .collect()
                })
                .collect();
            if crate::tracing() {
                eprintln!(
                    "import:   measured profiles {measured:?}: {:?} choices{}",
                    found.iter().map(Vec::len).collect::<Vec<_>>(),
                    if unions { " (unions)" } else { "" }
                );
                for options in found
                    .iter()
                    .filter(|f| f.len() > 1 || f.iter().any(|g| g.len() > 1))
                {
                    for g in options.iter().take(4) {
                        let parts: Vec<String> = g
                            .iter()
                            .map(|r| format!("{:.3}@{:.3?}", r.area, r.centroid))
                            .collect();
                        eprintln!("import:     {}", parts.join(" + "));
                    }
                }
            }
            if found.iter().all(|f| !f.is_empty()) {
                let mut chosen: Vec<Vec<&Region>> = vec![Vec::new()];
                for options in &found {
                    let mut next = Vec::new();
                    for c in &chosen {
                        for group in options {
                            let apart =
                                !group.iter().any(|r| c.iter().any(|d| std::ptr::eq(*d, *r)));
                            if next.len() < 32 && apart {
                                let mut c = c.clone();
                                c.extend(group.iter().copied());
                                next.push(c);
                            }
                        }
                    }
                    chosen = next;
                }
                // One way to take the regions apart is no guess.
                let alike = chosen.len() > 1;
                let sets: Vec<RegionSet> = chosen
                    .into_iter()
                    .map(|c| RegionSet {
                        keys: c.iter().map(|r| key(r)).collect(),
                        guess: alike,
                        area: c.iter().map(|r| r.area).sum(),
                        parts: c.iter().map(|r| (r.centroid, r.area)).collect(),
                        probe: None,
                        measured: unions && !alike,
                    })
                    .collect();
                // Unions come before the sets found without the measures.
                if unions {
                    first = sets;
                } else if !sets.is_empty() {
                    return Ok(sets);
                }
            }
        }
        let n = profiles.len().max(1);
        // Construction curves are not decoded, so the file's profiles may be
        // unions of Mitcad's regions: the regions whose material the next
        // history states change, probed in both directions, come first.
        let mut probed = self.probed_sets(info.uid, frame, regions, depth);
        let mut order: Vec<&Region> = regions.iter().collect();
        order.sort_by(|a, b| b.area.total_cmp(&a.area));
        // The sets of any number of regions whose area is the one the
        // history's change asks for (the file's profiles may be unions of
        // Mitcad's regions where construction lines split them, so the
        // decoded count is no limit).
        if let Some(target) = target_area {
            let areas: Vec<f64> = order.iter().map(|r| r.area).collect();
            for chosen in area_subsets(&areas, target, 6) {
                let set = RegionSet {
                    keys: chosen.iter().map(|&i| key(order[i])).collect(),
                    guess: true,
                    area: chosen.iter().map(|&i| order[i].area).sum(),
                    parts: chosen
                        .iter()
                        .map(|&i| (order[i].centroid, order[i].area))
                        .collect(),
                    probe: None,
                    measured: false,
                };
                if !probed.iter().any(|p| same_keys(&p.keys, &set.keys)) {
                    probed.push(set);
                }
            }
        }
        let mut all = RegionSet {
            keys: order.iter().map(|r| key(r)).collect(),
            guess: false,
            area: order.iter().map(|r| r.area).sum(),
            parts: order.iter().map(|r| (r.centroid, r.area)).collect(),
            probe: None,
            measured: false,
        };
        // (A probe that finds every region keeps what it found.)
        all.probe = probed
            .iter()
            .find(|p| same_keys(&p.keys, &all.keys))
            .and_then(|p| p.probe);
        // (A union a probe also found keeps what the probe found.)
        let with_first = |mut sets: Vec<RegionSet>| {
            let mut first = first.clone();
            for f in &mut first {
                if let Some(s) = sets.iter().find(|s| same_keys(&s.keys, &f.keys)) {
                    f.probe = f.probe.or(s.probe);
                }
            }
            sets.retain(|s| !first.iter().any(|f| same_keys(&f.keys, &s.keys)));
            first.into_iter().chain(sets).collect::<Vec<_>>()
        };
        if n >= regions.len() {
            probed.retain(|p| !same_keys(&p.keys, &all.keys));
            probed.insert(0, all);
            return Ok(with_first(probed));
        }
        let mut sets: Vec<RegionSet> = Vec::new();
        combinations(order.len(), n, 2000, &mut |c| {
            sets.push(RegionSet {
                keys: c.iter().map(|&i| key(order[i])).collect(),
                guess: !sets.is_empty(),
                area: c.iter().map(|&i| order[i].area).sum(),
                parts: c
                    .iter()
                    .map(|&i| (order[i].centroid, order[i].area))
                    .collect(),
                probe: None,
                measured: false,
            });
        });
        sets.push(RegionSet { guess: true, ..all });
        sets.retain(|s| !probed.iter().any(|p| same_keys(&p.keys, &s.keys)));
        probed.extend(sets);
        Ok(with_first(probed))
    }

    /// The regions the decoded profile loops select (`_f3d_profile_loops`
    /// of the item's detail, mitcad#96), mapped by
    /// [`crate::profiles::mapped_regions`] with the file's geometry of the
    /// sketch's curves.
    fn mapped_set(&self, other: &serde_json::Map<String, Value>, sketch: i64) -> Option<RegionSet> {
        let loops = other.get("_f3d_profile_loops")?;
        let info = self.sketches.get(&sketch)?;
        let output = self.doc.sketch_output(info.uid)?;
        let regions = &output.region_info;
        let detail = self
            .dump
            .timeline_items()
            .iter()
            .enumerate()
            .find(|(p, it)| it.index.unwrap_or(*p as i64) == sketch)
            .and_then(|(_, it)| match &it.detail {
                Some(Detail::Sketch(s)) => Some(s),
                _ => None,
            })?;
        let curves: HashMap<String, Geometry> = detail
            .curves
            .iter()
            .flatten()
            .filter_map(|c| Some((c.id.clone()?, c.geometry.clone()?)))
            .collect();
        let back: HashMap<&str, &str> = info
            .ids
            .iter()
            .map(|(file, ours)| (ours.as_str(), file.as_str()))
            .collect();
        let views: Vec<crate::profiles::RegionView> = regions
            .iter()
            .zip(interior_points(regions))
            .map(|(r, inside)| crate::profiles::RegionView {
                inside,
                outer: r.profile.loops.first().and_then(|l| {
                    l.segments
                        .iter()
                        .map(|s| {
                            back.get(s.key.curve.to_string().as_str())
                                .map(|f| f.to_string())
                        })
                        .collect()
                }),
            })
            .collect();
        let chosen = crate::profiles::mapped_regions(loops, &curves, &views)?;
        Some(RegionSet {
            keys: chosen
                .iter()
                .map(|&i| regions[i].profile.key.to_string())
                .collect(),
            guess: false,
            area: chosen.iter().map(|&i| regions[i].area).sum(),
            parts: chosen
                .iter()
                .map(|&i| (regions[i].centroid, regions[i].area))
                .collect(),
            probe: None,
            measured: false,
        })
    }

    /// Puts the decoded profiles' set ([`Importer::mapped_set`]) first in
    /// `sets` (with what a probe found for the same regions); whether there
    /// is one.
    pub(crate) fn mapped_first(
        &self,
        sets: &mut Vec<RegionSet>,
        other: &serde_json::Map<String, Value>,
        sketch: i64,
    ) -> bool {
        let Some(mut mapped) = self.mapped_set(other, sketch) else {
            return false;
        };
        if let Some(k) = sets.iter().position(|s| same_keys(&s.keys, &mapped.keys)) {
            mapped.probe = sets.remove(k).probe;
        }
        sets.insert(0, mapped);
        true
    }

    /// The regions whose material the history's next states change: a
    /// point inside each region, `depth` off the sketch plane on either
    /// side, is in a different number of bodies after than before.
    fn probed_sets(
        &mut self,
        sketch: FeatureUid,
        frame: SketchFrame,
        regions: &[Region],
        depth: Option<f64>,
    ) -> Vec<RegionSet> {
        // Hundreds of regions (text, patterns) are not worth probing.
        if !self.oracle.enabled || regions.is_empty() || regions.len() > 300 {
            return Vec::new();
        }
        let _ = sketch;
        let inner = interior_points(regions);
        let normal = geom::normal(&frame);
        let depth = depth.unwrap_or(0.05);
        let kernel = self.doc.kernel();
        let count = |shapes: &[&K::Shape], points: &[[f64; 3]]| -> Option<Vec<usize>> {
            let mut counts = vec![0; points.len()];
            for shape in shapes {
                let inside = kernel.points_inside(shape, points).ok()?;
                for (c, i) in counts.iter_mut().zip(inside) {
                    *c += usize::from(i);
                }
            }
            Some(counts)
        };
        let current: Vec<(K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.shape.clone(), Sig::of(kernel, b.shape)))
            .collect();
        let now: f64 = current
            .iter()
            .filter_map(|(_, s)| s.map(|s| s.volume))
            .sum();
        let volume = |state: &[(StoredBody<K::Shape>, Sig)]| -> f64 {
            state.iter().map(|(_, s)| s.volume).sum()
        };
        let mut sets: Vec<RegionSet> = Vec::new();
        for q in self.lookahead(3) {
            let Ok(state) = self.oracle.state(kernel, q) else {
                continue;
            };
            // Bodies in both sets count the same before and after: only
            // the ones that differ are probed.
            let mut used = vec![false; state.len()];
            let mut before: Vec<&K::Shape> = Vec::new();
            for (shape, sig) in &current {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => before.push(shape),
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            if before.is_empty() && after.is_empty() {
                continue;
            }
            for side in [1.0, -1.0] {
                let points: Vec<[f64; 3]> = inner
                    .iter()
                    .map(|p| {
                        let at = p.unwrap_or([1e9, 1e9]);
                        geom::add(frame.point(at), geom::scale(normal, side * depth))
                    })
                    .collect();
                let (Some(b), Some(a)) = (count(&before, &points), count(&after, &points)) else {
                    return sets;
                };
                if crate::tracing() {
                    for (k, r) in regions.iter().enumerate() {
                        eprintln!(
                            "import:   probe state {q} side {side}: {} area {:.3} at {:?}: {} -> {}",
                            r.profile.key, r.area, inner[k], b[k], a[k]
                        );
                    }
                }
                let chosen: Vec<&Region> = regions
                    .iter()
                    .zip(&inner)
                    .zip(b.iter().zip(&a))
                    .filter(|((_, p), (b, a))| p.is_some() && b != a)
                    .map(|((r, _), _)| r)
                    .collect();
                if chosen.is_empty() {
                    continue;
                }
                let set = RegionSet {
                    keys: chosen.iter().map(|r| r.profile.key.to_string()).collect(),
                    guess: true,
                    area: chosen.iter().map(|r| r.area).sum(),
                    parts: chosen.iter().map(|r| (r.centroid, r.area)).collect(),
                    probe: Some((side, volume(state) - now)),
                    measured: false,
                };
                if !sets.iter().any(|s| s.keys == set.keys) {
                    sets.push(set);
                }
            }
        }
        sets
    }

    /// The item's operation. A new component's body is a new body of the
    /// component the history puts the item in: the import makes the file's
    /// components from the dump (F6, `components.rs`).
    pub(crate) fn operation(op: Option<&str>) -> &'static str {
        match op {
            Some(o) if o.starts_with("Join") => "join",
            Some(o) if o.starts_with("Cut") => "cut",
            Some(o) if o.starts_with("Intersect") => "intersect",
            _ => "new_body",
        }
    }

    /// The bodies a join, cut or intersection works on, to try in turn:
    /// the dump's (none: all), then, when the dump names none, those of the
    /// sketch's component that the item's history state (or the next one)
    /// changed, when that is not all of them (the stream decoder gives no
    /// participants, and a cut in the file may have left bodies in its way
    /// alone). All bodies come first, so that features keep no
    /// participants they do not need (a pattern of the feature later
    /// would combine with them).
    ///
    /// The stream decoder gives an extrusion's participants (mitcad#96):
    /// when every one is found and they are not all the solids of the
    /// sketch's component, they come first, then all bodies and the
    /// history's guess. The guess stays even where the file names every
    /// body: a join there changes only the bodies its material overlaps,
    /// while Mitcad's also takes in those it only touches.
    pub(crate) fn participant_options(
        &mut self,
        index: i64,
        bodies: &Option<Vec<Reference>>,
        operation: &str,
        sketch: FeatureUid,
    ) -> Vec<Vec<String>> {
        let decoded = self.participants(bodies);
        if !decoded.is_empty() && operation != "new_body" {
            let component = self.doc.feature(sketch).map(|f| f.component);
            let all = self
                .current_bodies()
                .into_iter()
                .filter(|(b, _)| self.doc.body_component(*b) == component)
                .all(|(b, _)| decoded.contains(&b.to_string()));
            let mut options = Vec::new();
            if !all {
                options.push(decoded);
            }
            options.push(Vec::new());
            if let Some(changed) = self.changed_bodies(index, &[], operation, sketch)
                && !options.contains(&changed)
            {
                options.push(changed);
            }
            return options;
        }
        let mut options = vec![decoded.clone()];
        if let Some(changed) = self.changed_bodies(index, &decoded, operation, sketch) {
            options.push(changed);
        }
        options
    }

    /// See [`Importer::participant_options`].
    fn changed_bodies(
        &mut self,
        index: i64,
        decoded: &[String],
        operation: &str,
        sketch: FeatureUid,
    ) -> Option<Vec<String>> {
        if !decoded.is_empty() || operation == "new_body" || !self.oracle.enabled {
            return None;
        }
        let state = self
            .oracle
            .item_state(index)
            .unwrap_or_else(|| self.oracle.next_index());
        let kernel = self.doc.kernel();
        let after = self.oracle.state(kernel, state).ok()?;
        let after: Vec<Sig> = after.iter().map(|(_, s)| *s).collect();
        let component = self.doc.feature(sketch).map(|f| f.component);
        let current: Vec<(BodyUid, Sig)> = self
            .current_bodies()
            .into_iter()
            .filter(|(b, _)| self.doc.body_component(*b) == component)
            .collect();
        let mut used = vec![false; after.len()];
        let mut changed = Vec::new();
        for (uid, sig) in &current {
            match (0..after.len()).find(|&j| !used[j] && after[j].same(sig)) {
                Some(j) => used[j] = true,
                None => changed.push(uid.to_string()),
            }
        }
        (!changed.is_empty() && changed.len() < current.len()).then_some(changed)
    }

    /// The decoded participants, once each; none unless every one is
    /// found (a part of them would leave the others out).
    pub(crate) fn participants(&self, bodies: &Option<Vec<Reference>>) -> Vec<String> {
        bodies
            .as_deref()
            .and_then(|b| self.bodies_of_refs(b))
            .unwrap_or_default()
            .into_iter()
            .map(|b: BodyUid| b.to_string())
            .collect()
    }

    /// An extent object: an origin or construction plane, a face or a
    /// body.
    fn extent_object(&self, entity: &Option<Reference>, def: &Definition) -> Option<Value> {
        match entity.as_ref()? {
            Reference::ConstructionPlane(c) => match c.origin.clone().flatten().as_deref() {
                Some(o @ ("XY" | "XZ" | "YZ")) => {
                    Some(json!({"type": "plane", "plane": o.to_lowercase()}))
                }
                _ => {
                    let uid = c
                        .timeline_index
                        .flatten()
                        .and_then(|i| self.features.get(&i))?;
                    Some(json!({"type": "plane", "plane": uid.to_string()}))
                }
            },
            Reference::Face(fp) => {
                let (body, face) = refs::resolve_face(self.doc, fp).or_else(|| {
                    fp.point_on_face
                        .is_none()
                        .then(|| refs::resolve_cylinder_surface(self.doc, fp))
                        .flatten()
                })?;
                Some(
                    json!({"type": "face", "body": body.to_string(), "face": face.to_string(),
                            "chained": def.is_chained == Some(true)}),
                )
            }
            Reference::Body(fp) => {
                let body = self.body_ref(fp)?;
                Some(json!({"type": "body", "body": body.to_string(),
                            "through": def.is_minimum_solution == Some(false)}))
            }
            _ => None,
        }
    }

    /// A taper that is not zero, as a definition's value.
    fn taper_of(&self, taper: &Option<Reference>) -> Option<Value> {
        (!Self::is_zero(taper)).then(|| self.value(taper)).flatten()
    }

    /// One side's extent; Ok(None) when it is not decoded.
    fn side(
        &self,
        def: &Definition,
        taper: &Option<Reference>,
    ) -> Result<Option<(Value, Option<bool>)>, String> {
        let taper = (!Self::is_zero(taper)).then(|| self.value(taper)).flatten();
        let mut v = match def.definition_type.as_deref() {
            Some("DistanceExtentDefinition") => {
                json!({"type": "distance", "distance": self.value_or(&def.distance, "distance")?})
            }
            Some("ToEntityExtentDefinition") => {
                let object = self
                    .extent_object(&def.entity, def)
                    .ok_or("its extent's object was not found")?;
                let mut v = json!({"type": "to_object", "object": object});
                if !Self::is_zero(&def.offset) {
                    v["offset"] = self.value_or(&def.offset, "offset")?;
                }
                v
            }
            Some("ThroughAllExtentDefinition") => {
                let flip = def.is_positive_direction.map(|p| !p);
                let mut v = json!({"type": "through_all"});
                if let Some(t) = taper {
                    v["taper"] = t;
                }
                return Ok(Some((v, flip)));
            }
            Some(other) => return Err(format!("{other} is not supported")),
            None => return Ok(None),
        };
        if let Some(t) = taper {
            v["taper"] = t;
        }
        Ok(Some((v, None)))
    }

    fn extrude(
        &mut self,
        index: i64,
        item: &TimelineItem,
        d: &ExtrudeDetail,
    ) -> Result<Vec<Candidate>, String> {
        if d.is_solid == Some(false) {
            return Err("surface extrusions are not supported".to_owned());
        }
        let profiles = profiles_of(&d.profile, &d.other);
        // No profile decoded: perhaps a face of a body (tried first), else
        // the last sketch before it.
        let faces = if profiles.is_empty() {
            self.face_profiles(index, item, d)
        } else {
            Vec::new()
        };
        let (sketch, last_sketch) = match self.profile_sketch(&profiles, index) {
            Ok(s) => s,
            Err(_) if !faces.is_empty() => return Ok(faces),
            Err(e) => return Err(e),
        };
        // Regions are probed halfway along a distance extent.
        let probe = d
            .extent_one
            .as_ref()
            .or(d.symmetric_extent.as_ref())
            .filter(|e| e.definition_type.as_deref() != Some("ThroughAllExtentDefinition"))
            .and_then(|e| param(&e.distance))
            .and_then(|p| p.value)
            .map(|v| mm(v).abs() / 2.0)
            .filter(|v| *v > 1e-6);
        let length = |def: &Definition| {
            param(&def.distance)
                .and_then(|p| p.value)
                .map(|v| mm(v).abs())
        };
        let kind = d.extent_type.as_deref().unwrap_or("");
        let one = d.extent_one.clone().unwrap_or_default();
        let raw = item.f3d.as_ref().and_then(|f| f.extrude.as_ref());
        // The stream's extent codes: 1 one side, 2 two sides, 3 symmetric;
        // for one side 2 a distance, 1 up to an object, 0 through all.
        let raw_extent = raw.map(|r| (r.extent_a, r.extent_b));
        let symmetric = kind.starts_with("Symmetric")
            || d.symmetric_extent.is_some()
            || raw_extent.is_some_and(|(a, _)| a == Some(3));
        // The profile area the history's change asks for: the change over
        // the length swept by a distance extent (a prism's volume).
        let target_area = if self.oracle.enabled && d.is_thin_extrude != Some(true) {
            let swept = if symmetric {
                let s = d.symmetric_extent.as_ref().unwrap_or(&one);
                length(s).map(|l| {
                    if s.is_full_length.or(raw.and_then(|r| r.full_length)) == Some(true) {
                        l
                    } else {
                        2.0 * l
                    }
                })
            } else if kind.starts_with("TwoSides") || d.has_two_extents == Some(true) {
                let two = d.extent_two.clone().unwrap_or_default();
                length(&one).zip(length(&two)).map(|(a, b)| a + b)
            } else if one.definition_type.as_deref() == Some("DistanceExtentDefinition") {
                length(&one)
            } else {
                None
            };
            let change = match self.oracle.item_state(index) {
                Some(_) => self.history_change(index),
                None => self.change_to_next(),
            };
            swept
                .filter(|l| *l > 1e-6)
                .zip(change)
                .map(|(l, c)| c.abs() / l)
                .filter(|a| *a > 1e-9)
        } else {
            None
        };
        let mut sets = self.region_sets(&profiles, sketch, probe, target_area)?;
        // The regions the decoded profile loops select first (mitcad#96).
        let mapped = self.mapped_first(&mut sets, &d.other, sketch);
        let sketch_uid = self.sketches[&sketch].uid.to_string();
        let operation = Self::operation(
            d.operation
                .as_deref()
                .or(raw.and_then(|r| r.operation.clone().flatten()).as_deref()),
        );
        let participant_options = self.participant_options(
            index,
            &d.participant_bodies,
            operation,
            self.sketches[&sketch].uid,
        );
        let decoded_flip = self.decoded_flip(raw, self.sketches[&sketch].uid);
        // A join whose tool touches no body makes a new body (the .f3d rule):
        // when the history's state has more bodies, that too.
        let join_as_new = operation == "join" && self.oracle.enabled && {
            let state = self
                .oracle
                .item_state(index)
                .unwrap_or_else(|| self.oracle.next_index());
            let kernel = self.doc.kernel();
            let after = self.oracle.try_sigs(kernel, state).map_or(0, |s| s.len());
            after > self.current_sigs().len()
        };

        // Extents: (extent, flip, guess, length swept in mm when known).
        let mut extents: Vec<ExtentChoice> = Vec::new();
        if symmetric && d.symmetric_extent.is_none() && one.distance.is_none() {
            // Symmetric through all (no distance).
            extents.push((
                json!({"type": "through_all", "both_sides": true}),
                false,
                false,
                None,
            ));
        } else if symmetric {
            let s = d.symmetric_extent.as_ref().unwrap_or(&one);
            // Without the stream decoder's length: a half each way first.
            let fulls = match (s.is_full_length, raw.and_then(|r| r.full_length)) {
                (Some(full), _) => vec![(full, false)],
                // The stream decoder's (mitcad#96) first, the other after it.
                (None, Some(full)) => vec![(full, false), (!full, true)],
                (None, None) => vec![(false, false), (true, true)],
            };
            for (full, guess) in fulls {
                let mut v = json!({"type": "symmetric", "distance": self.value_or(&s.distance, "distance")?,
                                   "full_length": full});
                let taper = s.taper_angle.clone().or(d.taper_angle_one.clone());
                if !Self::is_zero(&taper) {
                    v["taper"] = self.value_or(&taper, "taper")?;
                }
                let swept = length(s).map(|l| if full { l } else { 2.0 * l });
                extents.push((v, false, guess, swept));
            }
        } else if kind.starts_with("TwoSides") || d.has_two_extents == Some(true) {
            let two = d.extent_two.clone().unwrap_or_default();
            // A side without a distance or an object in the stream is
            // through all (its code there is 4).
            let through_all = |def: &Definition, taper: &Option<Reference>| {
                let mut v = json!({"type": "through_all"});
                if !Self::is_zero(taper)
                    && let Some(t) = self.value(taper)
                {
                    v["taper"] = t;
                }
                (raw.is_some() && def.definition_type.is_none()).then_some((v, None))
            };
            // A side up to a face the replay does not have as it is goes
            // up to its plane (mitcad#104).
            let side = |def: &Definition, taper: &Option<Reference>| {
                if def.definition_type.as_deref() == Some("ToEntityExtentDefinition")
                    && self.extent_object(&def.entity, def).is_none()
                    && let Some(mut v) = self.decoded_plane(def)?
                {
                    if !Self::is_zero(taper)
                        && let Some(t) = self.value(taper)
                    {
                        v["taper"] = t;
                    }
                    return Ok(Some((v, None)));
                }
                self.side(def, taper)
            };
            let (s1, _) = match side(&one, &d.taper_angle_one)? {
                Some(s) => s,
                None => through_all(&one, &d.taper_angle_one).ok_or("side one not decoded")?,
            };
            let (s2, _) = match side(&two, &d.taper_angle_two)? {
                Some(s) => s,
                None => through_all(&two, &d.taper_angle_two).ok_or("side two not decoded")?,
            };
            let swept = length(&one).zip(length(&two)).map(|(a, b)| a + b);
            // Side one goes along the decoded direction; up to an object,
            // towards the side of the sketch the object lies on.
            let flip = if s1["type"] == "to_object" {
                self.to_object_flip(&one, raw, sketch).or(decoded_flip)
            } else {
                decoded_flip
            }
            .unwrap_or(false);
            let v = json!({"type": "two_sides", "side1": s1, "side2": s2});
            extents.push((v.clone(), flip, false, swept));
            extents.push((v, !flip, true, swept));
        } else if one.definition_type.as_deref() == Some("ToEntityExtentDefinition")
            && self.extent_object(&one.entity, &one).is_none()
        {
            // Up to a planar face the decoder named but the replay does not
            // have as it is (mitcad#96): its plane first.
            let plane = self.decoded_plane(&one)?;
            if let Some(v) = &plane {
                let flip = self
                    .to_object_flip(&one, raw, sketch)
                    .or(decoded_flip)
                    .unwrap_or(false);
                extents.push((v.clone(), flip, false, None));
                extents.push((v.clone(), !flip, true, None));
            }
            // Up to an object the decoder does not name: planar faces
            // parallel to the sketch, nearest first, and through all.
            if !self.oracle.enabled {
                if plane.is_none() {
                    return Err("its extent's object was not decoded".to_owned());
                }
            } else {
                extents.extend(self.to_object_guesses(sketch, index, &one, None)?);
            }
            // With the extrusion's taper (an `.ipt` extrusion up to a face
            // keeps one).
            if let Some(t) = self.taper_of(&d.taper_angle_one) {
                for e in &mut extents {
                    e.0["taper"] = t.clone();
                }
            }
        } else {
            match self.side(&one, &d.taper_angle_one)? {
                Some((v, flip)) => {
                    let flip = if v["type"] == "to_object" {
                        flip.or(self.to_object_flip(&one, raw, sketch))
                    } else {
                        flip
                    };
                    let flip = flip.or(decoded_flip).unwrap_or(false);
                    let swept = length(&one);
                    extents.push((v.clone(), flip, false, swept));
                    if decoded_flip.is_some() || flip {
                        extents.push((v.clone(), !flip, true, swept));
                    }
                    // The decoder cannot tell a symmetric extent (code 3).
                    if d.extent_type.is_none() && v["type"] == "distance" {
                        let mut s = v.clone();
                        s["type"] = json!("symmetric");
                        extents.push((s, false, true, swept.map(|l| 2.0 * l)));
                    }
                    // The object the decoder named (mitcad#96) first, then
                    // a planar face's whole plane (a face of the replay
                    // bounded otherwise, or another piece of the plane),
                    // then the faces the history may settle.
                    if v["type"] == "to_object" && self.oracle.enabled {
                        let from = extents.len();
                        if let Ok(Some(p)) = self.decoded_plane(&one) {
                            extents.push((p.clone(), flip, true, None));
                            extents.push((p, !flip, true, None));
                        }
                        if let Ok(guesses) =
                            self.to_object_guesses(sketch, index, &one, Some(&v["object"]))
                        {
                            extents.extend(guesses);
                        }
                        if let Some(t) = self.taper_of(&d.taper_angle_one) {
                            for e in &mut extents[from..] {
                                e.0["taper"] = t.clone();
                            }
                        }
                    }
                }
                None => {
                    // Not a distance (the decoder leaves the extent out):
                    // through all is the likeliest, and certain with the
                    // stream's extent code for it, or a one-sided input
                    // slot through all of bodies (mitcad#96), and a decoded
                    // direction.
                    let slot = raw.is_some_and(|r| {
                        r.extent_a == Some(1) && r.slot_roles.iter().flatten().any(|&x| x == 5)
                    });
                    let through_all = raw_extent == Some((Some(1), Some(0))) || slot;
                    let flip = decoded_flip.unwrap_or(false);
                    for (flip, guess) in [
                        (flip, !(through_all && decoded_flip.is_some())),
                        (!flip, true),
                    ] {
                        extents.push((json!({"type": "through_all"}), flip, guess, None));
                    }
                    extents.push((
                        json!({"type": "through_all", "both_sides": true}),
                        false,
                        true,
                        None,
                    ));
                }
            }
        }
        let sign = match operation {
            "cut" => Some(-1.0),
            "intersect" => None,
            _ => Some(1.0),
        };
        let start = match d.start_extent.as_ref() {
            Some(s) if s.definition_type.as_deref() == Some("OffsetStartDefinition") => {
                Some(json!({"type": "offset", "offset": self.value_or(&s.offset, "start offset")?}))
            }
            Some(s) if s.definition_type.as_deref() == Some("FromEntityStartDefinition") => {
                let object = self
                    .extent_object(&s.entity, s)
                    .ok_or("the start object was not found")?;
                let mut v = json!({"type": "object", "object": object});
                if !Self::is_zero(&s.offset) {
                    v["offset"] = self.value_or(&s.offset, "start offset")?;
                }
                Some(v)
            }
            _ => None,
        };
        let thin = if d.is_thin_extrude == Some(true) {
            let location = |l: &Option<String>| match l.as_deref() {
                Some(l) if l.contains("Center") => "center",
                Some(l) if l.contains("Side2") => "side2",
                _ => "side1",
            };
            let mut t = json!({"location": location(&d.thin_extrude_wall_location_one),
                               "thickness": self.value_or(&d.thin_extrude_wall_thickness_one, "wall thickness")?});
            if d.thin_extrude_wall_thickness_two.is_some() {
                t["side2"] = json!({"location": location(&d.thin_extrude_wall_location_two),
                    "thickness": self.value_or(&d.thin_extrude_wall_thickness_two, "wall thickness")?});
            }
            Some(t)
        } else {
            None
        };
        // The side of the sketch plane a one-sided extent goes to (±1 along
        // its normal); None for both sides.
        let distance_sign = param(&one.distance)
            .and_then(|p| p.value)
            .map_or(1.0, |v| if v < 0.0 { -1.0 } else { 1.0 });
        let side_of = |extent: &Value, flip: bool| -> Option<f64> {
            let along = if flip { -1.0 } else { 1.0 };
            match extent["type"].as_str() {
                Some("distance") => Some(along * distance_sign),
                Some("to_object") => Some(along),
                Some("through_all") if extent["both_sides"] != true => Some(along),
                _ => None,
            }
        };
        let mut candidates = Vec::new();
        // Profile sets outside, extents inside: the likeliest extent of
        // each set is tried before the next set.
        for (k, set) in sets.iter().enumerate() {
            let decoded = mapped && k == 0;
            for (extent, flip, extent_guess, swept) in &extents {
                let profiles: Vec<Value> = set
                    .keys
                    .iter()
                    .map(|r| json!({"sketch": sketch_uid, "region": r}))
                    .collect();
                let mut def = json!({"type": "extrude", "profiles": profiles, "extent": extent,
                                     "flip": flip, "operation": operation});
                if let Some(s) = &start {
                    def["start"] = s.clone();
                }
                if let Some(t) = &thin {
                    def["thin"] = t.clone();
                }
                let mut c = Candidate::new(def);
                // Without tapers, a prism's volume (overlaps aside); regions
                // a history state changes on the extrusion's side, the
                // state's change (a cut deeper than the body removes less
                // than its prism), so that they are tried first.
                c.predicted = match set.probe {
                    Some((side, change)) if side_of(extent, *flip).is_none_or(|s| s == side) => {
                        Some(change)
                    }
                    _ => sign.zip(*swept).map(|(s, l)| s * l * set.area),
                };
                let mut notes = Vec::new();
                if last_sketch {
                    notes.push("the sketch is the last one before it".to_owned());
                }
                if set.guess || *extent_guess {
                    c.guess = true;
                }
                c.first = (decoded && !*extent_guess) || set.measured;
                if decoded {
                    notes.push("the profiles as decoded".to_owned());
                } else if sets.len() > 1 {
                    notes.push(format!("profiles {}", set.keys.join(" + ")));
                }
                if !notes.is_empty() {
                    c.note = Some(notes.join("; "));
                }
                candidates.extend(with_participants(&c, &participant_options));
                if join_as_new {
                    let mut n = c.clone();
                    n.defs[0]["operation"] = json!("new_body");
                    n.guess = true;
                    n.first = false;
                    notes.push("a join that touches no body: a new body".to_owned());
                    n.note = Some(notes.join("; "));
                    candidates.push(n);
                }
            }
        }
        let mut all = faces;
        all.extend(candidates);
        Ok(all)
    }

    /// Extents up to an object the history may settle (an object the
    /// decoder does not name, or that did not give the state): planar faces
    /// parallel to the sketch, nearest first, and through all both ways;
    /// all guesses. `decoded` (the object the decoder named) is left out.
    fn to_object_guesses(
        &self,
        sketch: i64,
        index: i64,
        one: &Definition,
        decoded: Option<&Value>,
    ) -> Result<Vec<ExtentChoice>, String> {
        let mut extents = Vec::new();
        let frame = self
            .doc
            .sketch_output(self.sketches[&sketch].uid)
            .map(|o| o.frame)
            .ok_or("its sketch did not evaluate")?;
        let normal = geom::normal(&frame);
        // Faces of the sketch's component as they are; those of other
        // components (the file's assembly context: a cut up to another
        // part) as fixed planes where their occurrences place them in
        // the sketch's component, which the extrusion can work in, at
        // this point of the timeline (mitcad#86).
        let own = self
            .doc
            .feature(self.sketches[&sketch].uid)
            .map(|f| f.component);
        let into_own = |c: ComponentUid| {
            let own = own?;
            Some(inverse(&self.placement_at(own, index)?).after(&self.placement_at(c, index)?))
        };
        let mut faces: Vec<(f64, Value)> = refs::planar_faces(self.doc)
            .into_iter()
            .filter_map(|(body, face, p)| {
                let c = self.doc.body_component(body)?;
                if Some(c) == own {
                    let object = json!({"type": "face", "body": body.to_string(),
                                            "face": face.to_string()});
                    return Some((p.origin, p.normal, object));
                }
                let t = into_own(c)?;
                let (origin, n) = (t.apply_point(p.origin), t.apply_vector(p.normal));
                let object = json!({"type": "plane", "plane": {"origin": origin, "normal": n}});
                Some((origin, n, object))
            })
            .filter(|(_, n, _)| geom::norm(geom::cross(*n, normal)) < 1e-9)
            .map(|(origin, _, object)| (geom::dot(geom::sub(origin, frame.origin), normal), object))
            .filter(|(s, _)| s.abs() > 1e-6)
            .collect();
        faces.sort_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
        faces.truncate(24);
        let offset = (!Self::is_zero(&one.offset))
            .then(|| self.value(&one.offset))
            .flatten();
        for (s, object) in faces {
            let mut v = json!({"type": "to_object", "object": object});
            if let Some(o) = &offset {
                v["offset"] = o.clone();
            }
            if decoded.is_some_and(|o| Self::same_object(o, &v["object"])) {
                continue;
            }
            extents.push((v, s < 0.0, true, Some(s.abs())));
        }
        // Up to the first face of a body of the component it reaches (an
        // object that is not a planar face parallel to the sketch: the next
        // face, a curved one).
        let bodies: Vec<BodyUid> = self.doc.bodies().iter().map(|b| b.uid).collect();
        for body in bodies {
            if self.doc.body_component(body) != own {
                continue;
            }
            let mut v = json!({"type": "to_object",
                               "object": {"type": "body", "body": body.to_string(), "through": false}});
            if let Some(o) = &offset {
                v["offset"] = o.clone();
            }
            if decoded.is_some_and(|o| o["type"] == "body" && o["body"] == v["object"]["body"]) {
                continue;
            }
            for flip in [false, true] {
                extents.push((v.clone(), flip, true, None));
            }
        }
        for flip in [false, true] {
            extents.push((json!({"type": "through_all"}), flip, true, None));
        }
        Ok(extents)
    }

    /// Whether an extent up to the object the decoder named goes against
    /// the sketch's normal (mitcad#96): the side of the sketch's plane the
    /// object's point lies on (a face's, or a plane's origin), else the
    /// stream's flag (1: against the normal; on every extrusion up to an
    /// object of the corpus' older writers, where the stored direction is
    /// the sketch's normal).
    fn to_object_flip(
        &self,
        one: &Definition,
        raw: Option<&ExtrudeF3d>,
        sketch: i64,
    ) -> Option<bool> {
        let point = match one.entity.as_ref()? {
            Reference::Face(fp) => fp.point_on_face,
            Reference::ConstructionPlane(c) => c.geometry.as_ref().and_then(|g| g.origin),
            _ => None,
        };
        let frame = self
            .doc
            .sketch_output(self.sketches[&sketch].uid)
            .map(|o| o.frame);
        if let (Some(p), Some(frame)) = (point, frame) {
            let s = geom::dot(geom::sub(geom::mm3(p), frame.origin), geom::normal(&frame));
            if s.abs() > 1e-6 {
                return Some(s < 0.0);
            }
        }
        let to_object = raw?
            .slot_roles
            .iter()
            .flatten()
            .any(|r| matches!(r, 17 | 18));
        raw?.flag.filter(|_| to_object).map(|f| f == 1)
    }

    /// The plane of the planar face an extent up to an object names
    /// (mitcad#96), as an extent up to that plane (mm), with the extent's
    /// offset.
    fn decoded_plane(&self, one: &Definition) -> Result<Option<Value>, String> {
        let Some(Reference::Face(fp)) = one.entity.as_ref() else {
            return Ok(None);
        };
        let Some(g) = fp
            .geometry
            .as_ref()
            .filter(|g| g.geometry_type.as_deref() == Some("Plane"))
        else {
            return Ok(None);
        };
        let (Some(origin), Some(normal)) = (g.origin, g.normal) else {
            return Ok(None);
        };
        let mut v = json!({"type": "to_object",
                           "object": {"type": "plane",
                                      "plane": {"origin": geom::mm3(origin), "normal": normal}}});
        if !Self::is_zero(&one.offset) {
            v["offset"] = self.value_or(&one.offset, "offset")?;
        }
        Ok(Some(v))
    }

    /// Whether two extent objects name the same face (a decoded one carries
    /// `chained`).
    fn same_object(a: &Value, b: &Value) -> bool {
        a["type"] == b["type"] && a["body"] == b["body"] && a["face"] == b["face"]
    }

    /// Whether the extrusion goes against its sketch's normal: by the
    /// decoded direction vector when there is one (it is stored in the
    /// component's coordinates, so it tells for sketches on any plane),
    /// else by the sign the stream decoder found (axis-aligned directions).
    fn decoded_flip(&self, raw: Option<&ExtrudeF3d>, sketch: FeatureUid) -> Option<bool> {
        let raw = raw?;
        // One side through all: the stored vector points away from the
        // extrusion (every such extrusion the history settled went the
        // other way).
        let away = raw.extent_a == Some(1) && raw.extent_b == Some(0);
        let normal = self
            .doc
            .sketch_output(sketch)
            .map(|o| geom::normal(&o.frame));
        if let (Some(v), Some(n)) = (raw.direction_vector, normal) {
            let along = geom::dot(v, n);
            // A stored vector oblique to the sketch's normal tells no side
            // (mitcad#104): every such extrusion the history settled went
            // along the normal (36 with distance extents, in sketches on
            // tilted planes), against the vector's side in 8 of them.
            if !away && along.abs() > 1e-6 && along.abs() < 1.0 - 1e-6 {
                return Some(false);
            }
            if along.abs() > 1e-6 {
                return Some((along < 0.0) != away);
            }
        }
        raw.direction.flatten().map(|v| (v < 0.0) != away)
    }

    /// Extrusions of a planar face of a body (.f3d designs extrude faces too;
    /// the stream decoder gives no profile then): the faces of the item's
    /// component whose area times the distance is the volume the history
    /// state adds or removes, each as a sketch on the face with its
    /// boundary projected (linked, so it follows the face) and its region
    /// extruded, outwards first for a new body or a join, inwards first for
    /// a cut, and for a new body the side where the state's new body is.
    /// Distance extents only. Guesses the history checks.
    fn face_profiles(
        &mut self,
        index: i64,
        item: &TimelineItem,
        d: &ExtrudeDetail,
    ) -> Vec<Candidate> {
        if !self.oracle.enabled {
            return Vec::new();
        }
        let Some(one) = d.extent_one.as_ref() else {
            return Vec::new();
        };
        if one.definition_type.as_deref() != Some("DistanceExtentDefinition")
            || d.extent_type
                .as_deref()
                .is_some_and(|k| !k.starts_with("OneSide"))
        {
            return Vec::new();
        }
        let (Some(length), Ok(distance)) = (
            param(&one.distance)
                .and_then(|p| p.value)
                .map(|v| mm(v).abs()),
            self.value_or(&one.distance, "distance"),
        ) else {
            return Vec::new();
        };
        let raw = item.f3d.as_ref().and_then(|f| f.extrude.as_ref());
        let operation = Self::operation(
            d.operation
                .as_deref()
                .or(raw.and_then(|r| r.operation.clone().flatten()).as_deref()),
        );
        if operation == "intersect" || length < 1e-9 {
            return Vec::new();
        }
        let state = self
            .oracle
            .item_state(index)
            .unwrap_or_else(|| self.oracle.next_index());
        let kernel = self.doc.kernel();
        let Some(target) = self.oracle.try_sigs(kernel, state) else {
            return Vec::new();
        };
        let current = self.current_sigs();
        let change = target.iter().map(|s| s.volume).sum::<f64>()
            - current.iter().map(|s| s.volume).sum::<f64>();
        if change.abs() < 1e-9 || (operation == "cut") != (change < 0.0) {
            return Vec::new();
        }
        // The new body's centre, for the side of a new body.
        let mut used = vec![false; current.len()];
        let new_body = target
            .iter()
            .filter(
                |t| match (0..current.len()).find(|&j| !used[j] && current[j].same(t)) {
                    Some(j) => {
                        used[j] = true;
                        false
                    }
                    None => true,
                },
            )
            .find(|t| ((t.volume - change.abs()) / change.abs()).abs() < 0.02)
            .map(|t| t.center);
        // Faces by how close their prism comes to the change.
        let mut faces: Vec<(f64, BodyUid, FaceName, f64)> = Vec::new();
        for body in self.doc.bodies() {
            if self.doc.body_component(body.uid) != Some(self.component) {
                continue;
            }
            // (A kernel call per body, mitcad#82.)
            crate::tick();
            let Ok(list) = kernel.faces(body.shape) else {
                continue;
            };
            for f in list.iter().filter(|f| f.surface == "plane") {
                let Some(name) = f.names.first().and_then(|n| n.parse::<FaceName>().ok()) else {
                    continue;
                };
                let off = (f.area * length - change.abs()).abs() / change.abs();
                if off < 0.02 {
                    faces.push((off, body.uid, name, f.area));
                }
            }
        }
        faces.sort_by(|a, b| a.0.total_cmp(&b.0));
        faces.truncate(6);
        let mut out = Vec::new();
        for (_, body, face, area) in faces {
            let depth = self.doc.undo_depth();
            let plane = json!({"type": "sketch", "plane": {"face": face.to_string(), "body": body.to_string()}});
            let made = self.add(&plane, None).and_then(|uid| {
                self.doc
                    .project_into_sketch(uid, &TopoName::Face(face.clone()), Some(body), true)
                    .map(|_| uid)
                    .map_err(|e| e.to_string())
            });
            let sketch = made.ok().and_then(|uid| {
                let output = self.doc.sketch_output(uid)?;
                let region = output
                    .region_info
                    .iter()
                    .find(|r| (r.area - area).abs() <= 1e-3 * area)?;
                let centre = output.frame.point(region.centroid);
                let normal = geom::normal(&output.frame);
                let params = self.doc.parameters();
                let def = self
                    .doc
                    .feature(uid)?
                    .def
                    .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
                    .ok()?;
                Some((
                    serde_json::to_value(&def).ok()?,
                    region.profile.key.to_string(),
                    centre,
                    normal,
                ))
            });
            self.undo_to(depth);
            let Some((sketch, key, centre, normal)) = sketch else {
                continue;
            };
            // Outwards (along the face's normal) first, except for a cut.
            let mut flips = if operation == "cut" {
                [true, false]
            } else {
                [false, true]
            };
            if let Some(c) = new_body {
                let at = |flip: bool| {
                    let s = if flip { -0.5 } else { 0.5 };
                    geom::distance(geom::add(centre, geom::scale(normal, s * length)), c)
                };
                if at(true) < at(false) {
                    flips = [true, false];
                }
            }
            let sign = if operation == "cut" { -1.0 } else { 1.0 };
            for flip in flips {
                out.push(Candidate {
                    defs: vec![
                        sketch.clone(),
                        json!({"type": "extrude", "profiles": [{"sketch": "$0", "region": key}],
                               "extent": {"type": "distance", "distance": distance},
                               "flip": flip, "operation": operation}),
                    ],
                    note: Some(format!(
                        "profile not decoded: face {face} of {body} checked against the history"
                    )),
                    guess: true,
                    predicted: Some(sign * area * length),
                    first: false,
                });
            }
        }
        out
    }

    fn revolve(&mut self, index: i64, d: &RevolveDetail) -> Result<Vec<Candidate>, String> {
        if d.is_solid == Some(false) {
            return Err("surface revolutions are not supported".to_owned());
        }
        let profiles = profiles_of(&d.profile, &d.other);
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let mut sets = self.region_sets(&profiles, sketch, None, None)?;
        // The regions the decoded profile loops select first (mitcad#96).
        let mapped = self.mapped_first(&mut sets, &d.other, sketch);
        let info = self.sketches[&sketch].clone();
        let axes = self.revolve_axes(&d.axis, &info)?;
        let operation = Self::operation(d.operation.as_deref());
        let participant_options =
            self.participant_options(index, &d.participant_bodies, operation, info.uid);
        let ext = d
            .extent_definition
            .clone()
            .or(d.extent_one.clone())
            .unwrap_or_default();
        let mut extents: Vec<(Value, bool)> = Vec::new();
        match ext.definition_type.as_deref() {
            Some("AngleExtentDefinition") | None if ext.angle.is_some() => {
                let angle = self.value_or(&ext.angle, "angle")?;
                if ext.is_symmetric == Some(true) {
                    extents.push((json!({"type": "symmetric", "angle": angle}), false));
                } else {
                    let full = param(&ext.angle)
                        .and_then(|p| p.value)
                        .is_some_and(|a| (a.abs() - std::f64::consts::TAU).abs() < 1e-9);
                    extents.push((json!({"type": "angle", "angle": angle}), false));
                    if !full && ext.is_symmetric.is_none() {
                        // The direction is not decoded.
                        if let Some(name) = angle.as_str() {
                            extents.push((
                                json!({"type": "angle", "angle": format!("-{name}")}),
                                true,
                            ));
                        } else if let Some(v) = angle.as_f64() {
                            extents.push((json!({"type": "angle", "angle": -v}), true));
                        }
                        extents.push((json!({"type": "symmetric", "angle": angle}), true));
                    }
                }
            }
            Some(_) if ext.angle_one.is_some() => {
                extents.push((
                    json!({"type": "two_sides",
                    "angle1": self.value_or(&ext.angle_one, "angle")?,
                    "angle2": self.value_or(&ext.angle_two, "angle")?}),
                    false,
                ));
            }
            Some(other) => return Err(format!("{other} is not supported")),
            None => return Err("its extent was not decoded".to_owned()),
        }
        // The swept angle of each extent, for volume estimates.
        let angle_of = |e: &Value| -> Option<f64> {
            let value = |v: &Value| match v {
                Value::Number(n) => n.as_f64(),
                Value::String(s) => {
                    let name = s.trim_start_matches('-');
                    let id = self.doc.parameters().find(name)?;
                    self.doc.parameters().get(id).map(|p| p.value())
                }
                _ => None,
            };
            match e["type"].as_str()? {
                "angle" => value(&e["angle"]),
                "symmetric" => value(&e["angle"]).map(|a| 2.0 * a),
                "two_sides" => Some(value(&e["angle1"])? + value(&e["angle2"])?),
                "full" => Some(std::f64::consts::TAU),
                _ => None,
            }
        };
        let sign = match operation {
            "cut" => Some(-1.0),
            "intersect" => None,
            _ => Some(1.0),
        };
        let mut candidates = Vec::new();
        for (k, set) in sets.iter().enumerate() {
            for (axis, axis_guess) in &axes {
                let line = self.axis_line(axis, &info);
                for (extent, extent_guess) in &extents {
                    let profiles: Vec<Value> = set
                        .keys
                        .iter()
                        .map(|r| json!({"sketch": info.uid.to_string(), "region": r}))
                        .collect();
                    let mut def = json!({"type": "revolve", "profiles": profiles, "axis": axis,
                                         "extent": extent, "operation": operation});
                    if d.is_project_axis == Some(true) {
                        def["project_axis"] = json!(true);
                    }
                    let mut c = Candidate::new(def);
                    c.predicted = line
                        .zip(angle_of(extent))
                        .zip(sign)
                        .map(|(((a, d), angle), s)| s * set.turned(a, d, angle));
                    c.guess = set.guess || *axis_guess || *extent_guess;
                    // Profiles, axis and extent as decoded (mitcad#96).
                    c.first = mapped && k == 0 && !*axis_guess && !*extent_guess;
                    if last_sketch {
                        c.note = Some("the sketch is the last one before it".to_owned());
                    } else if mapped && k == 0 {
                        c.note = Some("the profiles as decoded".to_owned());
                    }
                    candidates.extend(with_participants(&c, &participant_options));
                }
            }
        }
        Ok(candidates)
    }

    /// A coil that sweeps profiles of a sketch along a helix about an axis
    /// (the `.ipt` import's coils: `profile` and `axis` in the detail),
    /// as Mitcad's helix: the pitch and the turns (a height divided by the
    /// pitch), right-handed and along the axis first (against it where
    /// the detail's `flip` says so), then the other hand and direction as
    /// guesses.
    fn coil_helix(&mut self, index: i64, d: &CoilDetail) -> Result<Vec<Candidate>, String> {
        let profiles: Vec<Reference> = d
            .other
            .get("profile")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_object().map(|m| Reference::from_map(m.clone())))
            .collect();
        if profiles.is_empty() {
            return Err("its profiles were not decoded".to_owned());
        }
        let (sketch, last_sketch) = self.profile_sketch(&profiles, index)?;
        let sets = self.region_sets(&profiles, sketch, None, None)?;
        let info = self.sketches[&sketch].clone();
        let axis = d
            .other
            .get("axis")
            .and_then(Value::as_object)
            .map(|m| Reference::from_map(m.clone()));
        let axes = self.revolve_axes(&axis, &info)?;
        let operation = Self::operation(d.operation.as_deref());
        let participant_options = self.participant_options(index, &None, operation, info.uid);
        // A quotient of two parameters: by their names, else their values.
        let ratio = |a: &Option<Reference>, b: &Option<Reference>| -> Option<Value> {
            match (self.value(a)?, self.value(b)?) {
                (Value::String(x), Value::String(y)) => Some(json!(format!("{x} / {y}"))),
                _ => {
                    let (x, y) = (param(a)?.value?, param(b)?.value?);
                    (y != 0.0).then(|| json!(x / y))
                }
            }
        };
        let (pitch, revolutions) = match d.coil_type.as_deref() {
            Some("PitchAndRevolutionCoilType") => (
                self.value_or(&d.pitch, "pitch")?,
                self.value_or(&d.revolutions, "revolutions")?,
            ),
            Some("PitchAndHeightCoilType") => (
                self.value_or(&d.pitch, "pitch")?,
                ratio(&d.height, &d.pitch).ok_or("no height or pitch")?,
            ),
            Some("RevolutionAndHeightCoilType") => (
                ratio(&d.height, &d.revolutions).ok_or("no height or turns")?,
                self.value_or(&d.revolutions, "revolutions")?,
            ),
            other => return Err(format!("coil type {other:?} is not supported")),
        };
        if !Self::is_zero(&d.angle) {
            return Err("a tapered coil is not supported".to_owned());
        }
        // The direction along the axis the decoder read (`flip`) first.
        let reversed = d.other.get("flip").and_then(Value::as_bool) == Some(true);
        let mut candidates = Vec::new();
        for set in &sets {
            for (axis, axis_guess) in &axes {
                for (k, (left_handed, flip)) in [
                    (false, reversed),
                    (true, reversed),
                    (false, !reversed),
                    (true, !reversed),
                ]
                .into_iter()
                .enumerate()
                {
                    let profiles: Vec<Value> = set
                        .keys
                        .iter()
                        .map(|r| json!({"sketch": info.uid.to_string(), "region": r}))
                        .collect();
                    let def = json!({"type": "helix", "profiles": profiles, "axis": axis,
                        "pitch": pitch, "revolutions": revolutions,
                        "left_handed": left_handed, "flip": flip, "operation": operation});
                    let mut c = Candidate::new(def);
                    c.guess = set.guess || *axis_guess || k > 0;
                    if last_sketch {
                        c.note = Some("the sketch is the last one before it".to_owned());
                    }
                    candidates.extend(with_participants(&c, &participant_options));
                }
            }
        }
        Ok(candidates)
    }

    /// A revolve axis as a line of the sketch plane (a point and a unit
    /// direction, sketch mm): a sketch line, or an origin axis that lies
    /// in the plane.
    fn axis_line(&self, axis: &Value, sketch: &SketchInfo) -> Option<([f64; 2], [f64; 2])> {
        let output = self.doc.sketch_output(sketch.uid)?;
        let frame = output.frame;
        let unit2 = |d: [f64; 2]| {
            let n = d[0].hypot(d[1]);
            (n > 1e-12).then(|| [d[0] / n, d[1] / n])
        };
        match axis {
            Value::String(o) => {
                let dir = match o.as_str() {
                    "x" => [1.0, 0.0, 0.0],
                    "y" => [0.0, 1.0, 0.0],
                    "z" => [0.0, 0.0, 1.0],
                    _ => return None,
                };
                let normal = geom::normal(&frame);
                let in_plane = geom::dot(dir, normal).abs() < 1e-9
                    && geom::dot(geom::sub([0.0; 3], frame.origin), normal).abs() < 1e-6;
                if !in_plane {
                    return None;
                }
                let a = geom::to_sketch(&frame, [0.0; 3]);
                let d = unit2([geom::dot(dir, frame.x_axis), geom::dot(dir, frame.y_axis)])?;
                Some((a, d))
            }
            Value::Object(m) => {
                let curve = m.get("curve")?.as_str()?.strip_prefix('c')?.parse().ok()?;
                // A line of another sketch (mitcad#96): where it lies in
                // this sketch's plane.
                let own = sketch.uid.to_string();
                let other = match m.get("sketch").and_then(Value::as_str) {
                    Some(uid) if uid != own => Some(self.doc.sketch_output(uid.parse().ok()?)?),
                    _ => None,
                };
                let of = other.as_ref().unwrap_or(&output);
                let (a, b) = match of.solved.curves.get(&mitcad_model::EntityUid(curve))? {
                    mitcad_model::sketch::geometry::Curve2::Line { a, b } => (*a, *b),
                    _ => return None,
                };
                let (a, b) = match &other {
                    Some(o) => {
                        let (p, q) = (o.frame.point(a), o.frame.point(b));
                        let normal = geom::normal(&frame);
                        let off = |x| geom::dot(geom::sub(x, frame.origin), normal).abs();
                        if off(p) > 1e-6 || off(q) > 1e-6 {
                            return None;
                        }
                        (geom::to_sketch(&frame, p), geom::to_sketch(&frame, q))
                    }
                    None => (a, b),
                };
                Some((a, unit2([b[0] - a[0], b[1] - a[1]])?))
            }
            _ => None,
        }
    }

    /// The revolve axis: an origin or construction axis, a sketch line or
    /// an edge; without one decoded, the sketch's lines (centre lines
    /// first) and the origin axes, all guesses.
    fn revolve_axes(
        &self,
        axis: &Option<Reference>,
        sketch: &SketchInfo,
    ) -> Result<Vec<(Value, bool)>, String> {
        match axis {
            Some(Reference::ConstructionAxis(c)) => {
                if let Some(o) = c.origin.clone().flatten() {
                    return match o.as_str() {
                        "X" | "Y" | "Z" => Ok(vec![(json!(o.to_lowercase()), false)]),
                        other => Err(format!("axis {other}")),
                    };
                }
                // An axis off the timeline by its geometry (an `.ipt`
                // part's work axis, as its planes take them): fixed where
                // it is, then the guesses.
                if c.timeline_index.flatten().is_none()
                    && let Some((o, v)) = c
                        .geometry
                        .as_ref()
                        .and_then(|g| Some((mm3(g.origin?), geom::unit(g.direction?)?)))
                {
                    let fixed = json!({"origin": o, "direction": v});
                    let mut axes = vec![(fixed, false)];
                    axes.extend(self.revolve_axes(&None, sketch)?);
                    return Ok(axes);
                }
                let uid = c
                    .timeline_index
                    .flatten()
                    .and_then(|i| self.features.get(&i))
                    .ok_or("its construction axis was not imported")?;
                Ok(vec![(json!(uid.to_string()), false)])
            }
            Some(Reference::SketchEntity(e)) => {
                let s = e
                    .sketch_timeline_index
                    .flatten()
                    .and_then(|i| self.sketches.get(&i));
                let s = s.ok_or("the axis' sketch was not imported")?;
                let id = e.id.clone().flatten().ok_or("the axis line has no id")?;
                let curve = s.ids.get(&id).ok_or("the axis line was left out")?;
                // Read from the axis input's curve ids (mitcad#96): the
                // guesses stay after it.
                let decoded = json!({"sketch": s.uid.to_string(), "curve": curve});
                let mut axes = vec![(decoded.clone(), false)];
                axes.extend(
                    self.revolve_axes(&None, sketch)?
                        .into_iter()
                        .filter(|(a, _)| *a != decoded),
                );
                Ok(axes)
            }
            Some(Reference::Edge(fp)) => {
                let (body, edge) =
                    refs::resolve_edge(self.doc, fp).ok_or("the axis edge was not found")?;
                Ok(vec![(
                    json!({"body": body.to_string(), "edge": edge.to_string()}),
                    false,
                )])
            }
            Some(_) => Err("the axis is not supported".to_owned()),
            None => {
                let mut axes = Vec::new();
                if let Some(def) = self.doc.feature(sketch.uid)
                    && let mitcad_model::FeatureDef::Sketch(s) = &def.def
                {
                    let lines: Vec<_> = s
                        .entities
                        .iter()
                        .filter(|e| matches!(e.kind, mitcad_model::sketch::EntityKind::Line { .. }))
                        .collect();
                    let centre = |e: &&mitcad_model::sketch::Entity| match e.kind {
                        mitcad_model::sketch::EntityKind::Line { centerline, .. } => !centerline,
                        _ => true,
                    };
                    let mut lines = lines;
                    lines.sort_by_key(centre);
                    for e in lines.iter().take(12) {
                        axes.push((json!({"sketch": sketch.uid.to_string(), "curve": format!("c{}", e.id.0)}), true));
                    }
                }
                for a in ["x", "y", "z"] {
                    axes.push((json!(a), true));
                }
                Ok(axes)
            }
        }
    }

    // Fillets and chamfers.

    fn fillet(&mut self, d: &FilletDetail) -> Result<Vec<Candidate>, String> {
        let sets = d.edge_sets.clone().unwrap_or_default();
        if sets.is_empty() {
            return Err("no edge sets decoded".to_owned());
        }
        let curvature = |s: &EdgeSet| {
            d.is_g2 == Some(true)
                || s.continuity
                    .as_deref()
                    .is_some_and(|c| c.starts_with("Curvature"))
        };
        let value = |r: &Option<Reference>| param(r).and_then(|p| p.value).map(mm);
        // Per set: its size, the options next to it, and the cross section
        // the history's edges are matched with (a variable one by its mean
        // radius, an asymmetric one by its contact distances).
        let mut sizes = Vec::new();
        let mut options = Vec::new();
        let mut sections: Vec<Vec<Section>> = Vec::new();
        for s in &sets {
            let mut extra = serde_json::Map::new();
            let size = match s.edge_set_type.as_deref() {
                Some("ChordLengthFilletEdgeSet") => {
                    sections.push(vec![Section::Fillet(value(&s.radius).unwrap_or(1.0))]);
                    json!({"type": "chord_length", "length": self.value_or(&s.other_ref("chordLength"), "chord length")?})
                }
                Some("VariableRadiusFilletEdgeSet") => {
                    let mut size = json!({"type": "variable",
                        "start": self.value_or(&s.start_radius, "start radius")?,
                        "end": self.value_or(&s.end_radius, "end radius")?});
                    let radii = s.mid_radii.clone().unwrap_or_default();
                    // Positions as parameters, or as plain numbers.
                    let positions: Vec<Option<f64>> = match &s.mid_positions {
                        Some(refs) => refs
                            .iter()
                            .map(|r| r.parameter().and_then(|p| p.value))
                            .collect(),
                        None => s
                            .other
                            .get("midPositions")
                            .and_then(Value::as_array)
                            .map(|a| a.iter().map(Value::as_f64).collect())
                            .unwrap_or_default(),
                    };
                    if radii.len() != positions.len() {
                        return Err("the mid radii and their positions do not pair up".to_owned());
                    }
                    let mut mid = Vec::new();
                    let mut all = vec![value(&s.start_radius), value(&s.end_radius)];
                    for (radius, position) in radii.iter().zip(&positions) {
                        let radius = Some(radius.clone());
                        let position = position.ok_or("a mid radius position was not decoded")?;
                        mid.push(json!({"position": position,
                            "radius": self.value_or(&radius, "mid radius")?}));
                        all.push(value(&radius));
                    }
                    let known: Vec<f64> = all.into_iter().flatten().collect();
                    let mean = match known.as_slice() {
                        [] => value(&s.radius).unwrap_or(1.0),
                        known => known.iter().sum::<f64>() / known.len() as f64,
                    };
                    if !mid.is_empty() {
                        size["mid"] = Value::Array(mid);
                    }
                    sections.push(vec![Section::Fillet(mean)]);
                    size
                }
                Some(t) if t.contains("Asymmetric") => {
                    let (a, b) = (
                        value(&s.offset_one).unwrap_or(0.1),
                        value(&s.offset_two).unwrap_or(0.1),
                    );
                    sections.push(vec![Section::Chamfer(a, b), Section::Chamfer(b, a)]);
                    json!({"type": "asymmetric",
                        "distance1": self.value_or(&s.offset_one, "first offset")?,
                        "distance2": self.value_or(&s.offset_two, "second offset")?,
                        "flip": s.is_flipped == Some(true)})
                }
                Some(t) if !t.starts_with("ConstantRadius") => {
                    return Err(format!("{t} is not supported"));
                }
                _ => {
                    sections.push(vec![Section::Fillet(value(&s.radius).unwrap_or(1.0))]);
                    json!({"type": "constant", "radius": self.value_or(&s.radius, "radius")?})
                }
            };
            if curvature(s) {
                extra.insert("continuity".to_owned(), json!("curvature"));
                if s.tangency_weight.is_some() {
                    extra.insert(
                        "tangency_weight".to_owned(),
                        self.value_or(&s.tangency_weight, "tangency weight")?,
                    );
                }
            }
            sizes.push(size);
            options.push(extra);
        }
        let rolling = d.is_rolling_ball_corner != Some(false);
        let tangent = |s: &EdgeSet| s.is_tangent_chain.or(d.is_tangent_chain) != Some(false);
        let make = |body: &str, edges: &[SetEdges]| -> Value {
            let sets: Vec<Value> = edges
                .iter()
                .filter(|(_, e)| !e.is_empty())
                .map(|(i, e)| {
                    let mut set =
                        json!({"edges": e, "size": sizes[*i], "tangent_chain": tangent(&sets[*i])});
                    for (key, value) in &options[*i] {
                        set[key] = value.clone();
                    }
                    set
                })
                .collect();
            json!({"type": "fillet", "body": body, "sets": sets, "rolling_ball_corners": rolling})
        };
        let mut candidates = self.dressup(&sets, &sections, &make)?;
        // Asymmetric: which face takes which offset is not settled (K88),
        // the other reading next to each definition (as for chamfers).
        if sizes.iter().any(|s| s["type"] == "asymmetric") {
            candidates = candidates
                .into_iter()
                .flat_map(|c| {
                    let mut flipped = c.clone();
                    for def in &mut flipped.defs {
                        for set in def["sets"].as_array_mut().into_iter().flatten() {
                            if set["size"]["type"] == "asymmetric" {
                                let f = set["size"]["flip"].as_bool().unwrap_or(false);
                                set["size"]["flip"] = json!(!f);
                            }
                        }
                    }
                    flipped.guess = true;
                    [c, flipped]
                })
                .collect();
        }
        Ok(candidates)
    }

    fn chamfer(&mut self, d: &ChamferDetail) -> Result<Vec<Candidate>, String> {
        let mut sets = d.edge_sets.clone().unwrap_or_default();
        if sets.is_empty() && d.edges.is_some() {
            // Retired form: one set of `edges`.
            sets.push(EdgeSet {
                edges: d.edges.clone(),
                ..EdgeSet::default()
            });
        }
        if sets.is_empty() {
            return Err("no edge sets decoded".to_owned());
        }
        let corner = match d.corner_type.as_deref() {
            None => "chamfer",
            Some(c) if c.starts_with("Chamfer") => "chamfer",
            Some(c) if c.starts_with("Miter") => "miter",
            Some(c) if c.starts_with("Blend") => "blend",
            Some(c) => return Err(format!("chamfer corners of type {c} are not supported")),
        };
        let legacy = d.chamfer_type.as_deref().unwrap_or("");
        let mut sizes = Vec::new();
        let mut sections = Vec::new();
        for s in &sets {
            let kind = s.edge_set_type.as_deref().unwrap_or(legacy);
            // The cross section, either way round where it is not decoded.
            let length = |r: &Option<Reference>| mm(param(r).and_then(|p| p.value).unwrap_or(0.1));
            let (size, section) = if kind.contains("TwoDistances") || s.distance_one.is_some() {
                let d1 = self.value_or(&s.distance_one, "first distance")?;
                let d2 = self.value_or(&s.distance_two, "second distance")?;
                let (a, b) = (length(&s.distance_one), length(&s.distance_two));
                (
                    json!({"type": "two_distances", "distance1": d1, "distance2": d2}),
                    vec![Section::Chamfer(a, b), Section::Chamfer(b, a)],
                )
            } else if kind.contains("DistanceAndAngle") || s.angle.is_some() {
                let dist = self.value_or(&s.distance, "distance")?;
                let a = length(&s.distance);
                let angle = param(&s.angle)
                    .and_then(|p| p.value)
                    .unwrap_or(std::f64::consts::FRAC_PI_4);
                (
                    json!({"type": "distance_angle", "distance": dist,
                        "angle": self.value_or(&s.angle, "angle")?}),
                    vec![
                        Section::ChamferAngle(a, angle, false),
                        Section::ChamferAngle(a, angle, true),
                    ],
                )
            } else {
                let dist = self.value_or(&s.distance, "distance")?;
                let a = length(&s.distance);
                (
                    json!({"type": "equal_distance", "distance": dist}),
                    vec![Section::Chamfer(a, a)],
                )
            };
            sizes.push((
                size,
                s.is_flipped == Some(true),
                s.is_tangent_chain != Some(false),
            ));
            sections.push(section);
        }
        let make = |body: &str, edges: &[SetEdges]| -> Value {
            let sets: Vec<Value> = edges
                .iter()
                .filter(|(_, e)| !e.is_empty())
                .map(|(i, e)| {
                    let (size, flip, tangent) = &sizes[*i];
                    json!({"edges": e, "size": size, "flip": flip, "tangent_chain": tangent})
                })
                .collect();
            json!({"type": "chamfer", "body": body, "sets": sets, "corner": corner})
        };
        let mut candidates = self.dressup(&sets, &sections, &make)?;
        // Two distances: which face takes which is not decoded. Each
        // definition with the other reading next to it: the wrong one can
        // come within the tolerance, and the right one is tried before
        // other edges.
        if sizes.iter().any(|(s, _, _)| s["type"] == "two_distances") {
            candidates = candidates
                .into_iter()
                .flat_map(|c| {
                    let mut flipped = c.clone();
                    for def in &mut flipped.defs {
                        for set in def["sets"].as_array_mut().into_iter().flatten() {
                            let f = set["flip"].as_bool().unwrap_or(false);
                            set["flip"] = json!(!f);
                        }
                    }
                    flipped.guess = true;
                    [c, flipped]
                })
                .collect();
        }
        Ok(candidates)
    }

    /// Fillet or chamfer candidates: edges from the fingerprints (external
    /// dumps), else the edges the history's next states no longer have;
    /// with [`Importer::guesses`] set, the next part of them (the edges
    /// found by their names, then one state's guesses at a time).
    fn dressup(
        &mut self,
        sets: &[EdgeSet],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
    ) -> Result<Vec<Candidate>, String> {
        let mut lazy = self.guesses.take();
        let result = self.dressup_part(sets, sections, make, lazy.as_mut());
        self.guesses = lazy;
        result
    }

    /// [`Importer::dressup`]: all candidates, or with `lazy` the next part.
    fn dressup_part(
        &mut self,
        sets: &[EdgeSet],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
        mut lazy: Option<&mut Guesses>,
    ) -> Result<Vec<Candidate>, String> {
        if lazy.as_ref().is_some_and(|g| g.done) {
            return Ok(Vec::new());
        }
        let named_first = lazy.as_ref().is_none_or(|g| !g.named);
        if let Some(g) = lazy.as_deref_mut() {
            g.named = true;
        }
        let fingerprints: Vec<Vec<&Fingerprint>> = sets
            .iter()
            .map(|s| {
                s.edges
                    .iter()
                    .flatten()
                    .filter_map(|r| match r {
                        Reference::Edge(fp) => Some(&**fp),
                        // A face the decoder did not turn into its edges
                        // (mitcad#96): not found.
                        Reference::Face(fp) if fp.f3d.is_some() => Some(&**fp),
                        _ => None,
                    })
                    .collect()
            })
            .collect();
        // Edges the stream decoder found by their names (`_f3d`) come first,
        // then what the history suggests; an external dump's edges are
        // taken as they are.
        let mut first = Vec::new();
        let mut named_error = None;
        if named_first && fingerprints.iter().any(|f| !f.is_empty()) {
            let named = fingerprints.iter().flatten().any(|fp| fp.f3d.is_some());
            let result = self.dressup_by_fingerprints(&fingerprints, sections, make);
            if !named && let Some(g) = lazy.as_deref_mut() {
                g.done = true;
            }
            match result {
                Ok(c) if !named => return Ok(vec![c]),
                Err(e) if !named => return Err(e),
                Ok(mut c) => {
                    c.note = Some(BY_NAME.to_owned());
                    let chains = self.by_chains(&c);
                    first.push(c);
                    first.extend(chains);
                }
                Err(e) => {
                    if crate::tracing() {
                        eprintln!("import: edges by their names: {e}");
                    }
                    named_error = Some(e);
                }
            }
        }
        if !self.oracle.enabled {
            if let Some(g) = lazy.as_deref_mut() {
                g.done = true;
            }
            if !first.is_empty() {
                return Ok(first);
            }
            return Err(match named_error {
                Some(e) => format!("{e} (no history to find them)"),
                None => "its edges were not decoded (no history to find them)".to_owned(),
            });
        }
        // (The edges found by their names are not tried twice.)
        let mut seen: BTreeSet<Vec<String>> = first.iter().map(|c| dressup_key(&c.defs)).collect();
        // One part at a time: the edges found by their names before the
        // states' guesses.
        let mut skip = 0;
        if let Some(g) = lazy.as_deref_mut() {
            if !first.is_empty() {
                g.seen = seen;
                g.given = true;
                return Ok(first);
            }
            seen = std::mem::take(&mut g.seen);
            skip = g.states;
        }
        // The edges the next states lost.
        let kernel = self.doc.kernel();
        let bodies: Vec<(BodyUid, K::Shape, Option<Sig>)> = self
            .doc
            .bodies()
            .iter()
            .map(|b| (b.uid, b.shape.clone(), Sig::of(kernel, b.shape)))
            .collect();
        let mut candidates = Vec::new();
        let mut last_error = None;
        let states = self.lookahead(4);
        let count = states.len();
        for q in states.skip(skip) {
            // One state's guesses at a time: the next ones when these give
            // no state.
            if let Some(g) = lazy.as_deref_mut() {
                if !candidates.is_empty() {
                    break;
                }
                g.states += 1;
            }
            let kernel = self.doc.kernel();
            let state: Vec<(StoredBody<K::Shape>, Sig)> = match self.oracle.state(kernel, q) {
                Ok(s) => s.to_vec(),
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            };
            // Only bodies the state changed: the replay's without an equal
            // in the state, against the state's without one in the replay.
            let mut used = vec![false; state.len()];
            let mut changed = Vec::new();
            for (uid, shape, sig) in &bodies {
                let same =
                    sig.and_then(|s| (0..state.len()).find(|&j| !used[j] && state[j].1.same(&s)));
                match same {
                    Some(j) => used[j] = true,
                    None => changed.push((*uid, shape)),
                }
            }
            let after: Vec<&K::Shape> = state
                .iter()
                .zip(&used)
                .filter(|(_, u)| !**u)
                .map(|((b, _), _)| &b.shape)
                .collect();
            if after.is_empty() {
                continue;
            }
            // Per body, each gone edge with the set it fits best: whose
            // dressup the state has there (where it meets the edge's faces
            // and its middle lie on the state's faces), else whose size
            // leaves the distance the edge's midpoint has from the state.
            let mut found: Vec<(BodyUid, Vec<SetGone>)> = Vec::new();
            for (uid, shape) in changed {
                let clock = std::time::Instant::now();
                let consumed = match refs::consumed_edges(kernel, shape, &after) {
                    Ok(c) => c,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                if consumed.is_empty() {
                    continue;
                }
                let fits = match section_fits(kernel, shape, &consumed, sections, &after) {
                    Ok(f) => f,
                    Err(e) => {
                        last_error = Some(e);
                        continue;
                    }
                };
                let edges: Vec<SetGone> = consumed
                    .iter()
                    .zip(fits)
                    .map(|(gone, fit)| {
                        let wedge = gone.wedge.unwrap_or(std::f64::consts::FRAC_PI_2);
                        let off = |i: usize| {
                            let e = sections[i][0].depth(wedge);
                            (gone.distance - e).abs() / e.max(1e-9)
                        };
                        let set = (0..sets.len())
                            .min_by(|&a, &b| {
                                (fit[a] > FIT)
                                    .cmp(&(fit[b] > FIT))
                                    .then(off(a).total_cmp(&off(b)))
                            })
                            .unwrap_or(0);
                        SetGone {
                            set,
                            name: gone.name.to_string(),
                            distance: gone.distance,
                            off: off(set),
                            fit: fit[set],
                            fits: fit[set] <= FIT,
                            smooth: gone.wedge.is_some_and(|w| w > std::f64::consts::PI - 1e-3),
                        }
                    })
                    .collect();
                if crate::tracing() {
                    eprintln!(
                        "import: state {q} {uid}: {} gone edges in {:.2} s",
                        consumed.len(),
                        clock.elapsed().as_secs_f64()
                    );
                    for (gone, e) in consumed.iter().zip(&edges) {
                        eprintln!(
                            "import: state {q} {uid}: gone {} length {:.3} distance {:.4} wedge {:.1} {} set {} off {:.3} fit {:.4}",
                            e.name,
                            gone.length,
                            gone.distance,
                            gone.wedge.unwrap_or(f64::NAN).to_degrees(),
                            gone.across.map_or("?", |a| if a.convex {
                                "convex"
                            } else {
                                "concave"
                            }),
                            e.set,
                            e.off,
                            e.fit
                        );
                    }
                }
                found.push((uid, edges));
            }
            // The edges whose dressup the state has (at the distance its
            // size leaves at their corner angle, and with its faces through
            // the points where it meets theirs): at once, then one tangent
            // chain after the other in the dump's order of the sets and edges
            // (OCCT fails some contours together that build one by one).
            // Then those edges with the gone edges that continue them
            // tangentially (a rounding that ends where its edge runs on
            // tangentially takes the next edge's middle away too, and the
            // kernel follows the chain); the edges whose dressup the state
            // has whatever their distance (beside a narrow wedge another
            // face of the state can be nearer than the rounding), with their
            // continuations. Then the looser guesses: those at that
            // distance (edges next to the rounded ones can be cut away too);
            // every gone edge; per set the largest group at one distance.
            // Edges between faces that meet tangentially are no dressup's.
            for variant in 0..7 {
                let mut defs = Vec::new();
                for (uid, edges) in &found {
                    let mut by_set: Vec<SetEdges> =
                        (0..sets.len()).map(|i| (i, Vec::new())).collect();
                    let shape = bodies.iter().find(|(b, _, _)| b == uid).map(|(_, s, _)| s);
                    for (i, list) in by_set.iter_mut() {
                        let i = *i;
                        let mine: Vec<&SetGone> =
                            edges.iter().filter(|e| e.set == i && !e.smooth).collect();
                        let chosen: Vec<&SetGone> = match variant {
                            0..=2 => mine
                                .iter()
                                .copied()
                                .filter(|e| e.fits && e.off <= 0.1)
                                .collect(),
                            3 => mine.iter().copied().filter(|e| e.fits).collect(),
                            4 => mine.iter().copied().filter(|e| e.off <= 0.1).collect(),
                            5 => mine.clone(),
                            _ => largest_group(&mine),
                        };
                        list.extend(chosen.into_iter().map(|e| e.name.clone()));
                        if matches!(variant, 2 | 3)
                            && !list.is_empty()
                            && let Some(shape) = shape
                        {
                            let more = tangent_continuations(kernel, shape, list, &mine);
                            list.extend(more);
                        }
                    }
                    if variant == 1 {
                        for (i, list) in &by_set {
                            let chains = match shape {
                                Some(shape) => tangent_chains(kernel, shape, list),
                                None => vec![list.clone()],
                            };
                            for chain in chains {
                                defs.push(make(&uid.to_string(), &[(*i, chain)]));
                            }
                        }
                    } else if by_set.iter().any(|(_, l)| !l.is_empty()) {
                        defs.push(make(&uid.to_string(), &by_set));
                    }
                }
                // One chain is no new candidate.
                if variant == 1 && defs.len() < 2 {
                    continue;
                }
                if !defs.is_empty() && seen.insert(dressup_key(&defs)) {
                    let note = match variant {
                        0 => format!("edges whose dressup ASM history state {q} has"),
                        1 => format!(
                            "edges whose dressup ASM history state {q} has, one tangent chain at a time"
                        ),
                        _ => format!("edges found against ASM history state {q}"),
                    };
                    candidates.push(Candidate {
                        defs,
                        note: Some(note),
                        guess: true,
                        predicted: None,
                        first: false,
                    });
                }
            }
        }
        if let Some(g) = lazy {
            g.done = g.states >= count;
            g.seen = seen;
            if g.given {
                return Ok(candidates);
            }
            g.given = !candidates.is_empty();
        }
        first.extend(candidates);
        if first.is_empty() {
            return Err(match named_error {
                Some(e) => format!(
                    "{e}; {}",
                    last_error.unwrap_or_else(|| {
                        "no edge of the replay is gone in the next history states".to_owned()
                    })
                ),
                None => last_error.unwrap_or_else(|| {
                    "its edges were not decoded, and no edge of the replay is gone in the next history states"
                        .to_owned()
                }),
            });
        }
        Ok(first)
    }

    /// The dressup of edges given by fingerprints (every set's), by body.
    /// The stream decoder's edges (`_f3d`) as [`refs::dressup_edge_match`]
    /// finds them (a two-distance chamfer's sides decoded, mitcad#96:
    /// [`Importer::decoded_sides`]); an external dump's by their middles.
    fn dressup_by_fingerprints(
        &self,
        fingerprints: &[Vec<&Fingerprint>],
        sections: &[Vec<Section>],
        make: &dyn Fn(&str, &[SetEdges]) -> Value,
    ) -> Result<Candidate, String> {
        let mut by_body: Vec<(BodyUid, Vec<SetEdges>)> = Vec::new();
        let replay = fingerprints
            .iter()
            .flatten()
            .any(|fp| fp.f3d.is_some())
            .then(|| refs::ReplayEdges::of(self.doc));
        let mut found_edges: Vec<refs::FoundEdge> = Vec::new();
        for (i, fps) in fingerprints.iter().enumerate() {
            for (j, fp) in fps.iter().enumerate() {
                let edges = match &replay {
                    Some(replay) if fp.f3d.is_some() => refs::dressup_edge_match(replay, fp)
                        .map_err(|e| {
                            let context = fp
                                .f3d
                                .as_ref()
                                .and_then(|f| f.found.as_deref())
                                .map(|found| format!(" (decoder: {found})"))
                                .unwrap_or_default();
                            format!("edge set {i}, input {j}: {e}{context}")
                        })?,
                    _ => refs::resolve_edge(self.doc, fp)
                        .map(|(body, name)| {
                            vec![refs::FoundEdge {
                                body,
                                name,
                                mid: [0.0; 3],
                                direction: None,
                            }]
                        })
                        .ok_or("an edge was not found in the replay")?,
                };
                for e in edges {
                    let entry = match by_body.iter_mut().position(|(b, _)| *b == e.body) {
                        Some(k) => &mut by_body[k].1,
                        None => {
                            let sets = (0..fingerprints.len()).map(|i| (i, Vec::new())).collect();
                            by_body.push((e.body, sets));
                            &mut by_body.last_mut().expect("pushed").1
                        }
                    };
                    let name = e.name.to_string();
                    if !entry[i].1.contains(&name) {
                        entry[i].1.push(name);
                    }
                    found_edges.push(e);
                }
            }
        }
        let defs = by_body
            .iter()
            .map(|(body, edges)| {
                let mut def = make(&body.to_string(), edges);
                self.decoded_sides(&mut def, *body, edges, sections, &found_edges);
                def
            })
            .collect();
        Ok(Candidate {
            defs,
            note: None,
            guess: false,
            predicted: None,
            first: false,
        })
    }

    // Edges found by their names (mitcad#96).

    /// A two-distance chamfer's sides in a definition made of `edges`
    /// (mitcad#96): the file's first distance is taken to lie on the face
    /// on the left of the edge's own direction in the file (the face whose
    /// coedge runs along the edge), Mitcad's on the first face of the edge's
    /// name unless `flip` ([`refs::first_face_on_left`]). A tangent chain
    /// takes the reading of most of its edges (the kernel carries the side
    /// of the chain's first edge along it); a set whose chains differ is
    /// split. Sets of equal distances and edges whose side is not known
    /// keep the set's `flip`. The other reading stays a guess after it.
    fn decoded_sides(
        &self,
        def: &mut Value,
        body: BodyUid,
        edges: &[SetEdges],
        sections: &[Vec<Section>],
        found: &[refs::FoundEdge],
    ) {
        if def["type"] != "chamfer" || found.iter().all(|e| e.direction.is_none()) {
            return;
        }
        let Some(shape) = self
            .doc
            .bodies()
            .iter()
            .find(|b| b.uid == body)
            .map(|b| b.shape.clone())
        else {
            return;
        };
        let kernel = self.doc.kernel();
        let Some(sets) = def["sets"].as_array() else {
            return;
        };
        // The definition's sets are the non-empty ones, in order.
        let index: Vec<usize> = edges
            .iter()
            .filter(|(_, e)| !e.is_empty())
            .map(|(i, _)| *i)
            .collect();
        let mut out = Vec::new();
        for (k, set) in sets.iter().enumerate() {
            let unequal = index
                .get(k)
                .and_then(|&i| sections.get(i))
                .and_then(|s| s.first())
                .is_some_and(|s| match *s {
                    Section::Chamfer(a, b) => (a - b).abs() > 1e-9 * a.max(b),
                    _ => false,
                });
            if set["size"]["type"] != "two_distances" || !unequal {
                out.push(set.clone());
                continue;
            }
            let flip = set["flip"].as_bool().unwrap_or(false);
            let names: Vec<String> = set["edges"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect();
            let mut groups: Vec<(bool, Vec<String>)> = Vec::new();
            for chain in tangent_chains(kernel, &shape, &names) {
                // (Each edge's reading; the kernel takes the side of the
                // chain's first edge in its own order, so the most edges'.)
                let flips: Vec<bool> = chain
                    .iter()
                    .filter_map(|name| {
                        let e = found
                            .iter()
                            .find(|e| e.body == body && e.name.to_string() == *name)?;
                        refs::first_face_on_left(kernel, &shape, e).map(|left| !left)
                    })
                    .collect();
                let flipped = flips.iter().filter(|&&f| f).count();
                let f = match flipped * 2 {
                    _ if flips.is_empty() => flip,
                    n if n == flips.len() => flips[0],
                    n => n > flips.len(),
                };
                match groups.last_mut() {
                    Some((g, list)) if *g == f => list.extend(chain),
                    _ => groups.push((f, chain)),
                }
            }
            for (f, list) in groups {
                let mut s = set.clone();
                s["edges"] = json!(list);
                s["flip"] = json!(f);
                out.push(s);
            }
        }
        def["sets"] = Value::Array(out);
    }

    /// The edges found by their names one tangent chain at a time (a
    /// feature per chain and set: the kernel builds some chains only one by
    /// one, mitcad#96), when that makes more than one feature; a guess
    /// after the edges at once.
    fn by_chains(&self, c: &Candidate) -> Option<Candidate> {
        let kernel = self.doc.kernel();
        let mut defs = Vec::new();
        for def in &c.defs {
            let body = def["body"].as_str()?;
            let shape = self
                .doc
                .bodies()
                .iter()
                .find(|b| b.uid.to_string() == body)?
                .shape
                .clone();
            for set in def["sets"].as_array()? {
                let edges: Vec<String> = set["edges"]
                    .as_array()?
                    .iter()
                    .filter_map(|e| e.as_str().map(str::to_owned))
                    .collect();
                for chain in tangent_chains(kernel, &shape, &edges) {
                    let mut one = set.clone();
                    one["edges"] = json!(chain);
                    let mut d = def.clone();
                    d["sets"] = json!([one]);
                    defs.push(d);
                }
            }
        }
        (defs.len() > c.defs.len()).then(|| Candidate {
            defs,
            note: Some(format!("{BY_NAME}, one tangent chain at a time")),
            guess: true,
            predicted: None,
            first: false,
        })
    }
}

/// The note of a fillet's or chamfer's edges found by their names.
const BY_NAME: &str = "edges found by their names in the file";

/// A candidate once with each set of participants (none: all bodies).
fn with_participants(c: &Candidate, options: &[Vec<String>]) -> Vec<Candidate> {
    options
        .iter()
        .map(|participants| {
            let mut c = c.clone();
            if !participants.is_empty() {
                for def in &mut c.defs {
                    def["participants"] = json!(participants);
                }
            }
            c
        })
        .collect()
}

/// An edge set's index with the names of its edges.
type SetEdges = (usize, Vec<String>);

/// A gone edge and the edge set it fits best.
#[derive(Debug, Clone)]
struct SetGone {
    set: usize,
    name: String,
    /// How far its midpoint is from the next state.
    distance: f64,
    /// How far that is off the set's expected distance (relative).
    off: f64,
    /// How far the set's dressup there is off the next state's faces
    /// (relative to its size, beyond [`FIT_ABSOLUTE`]).
    fit: f64,
    /// Whether the next state has the set's dressup there.
    fits: bool,
    /// Whether its faces meet tangentially (no dressup's input).
    smooth: bool,
}

/// How far (relative to its size) the points of a dressup at an edge may
/// be off the next state's faces, beyond [`FIT_ABSOLUTE`] mm: the contact
/// points are where the faces are tangent to it, and the middle of a
/// rounding between curved faces is only near the plane formula's.
const FIT: f64 = 0.05;
const FIT_ABSOLUTE: f64 = 1e-3;

/// Per gone edge and edge set, how far the points of the set's dressup at
/// the edge (any of its cross sections) are off the faces of `after`:
/// the largest of their distances beyond [`FIT_ABSOLUTE`], relative to the
/// size; infinite where the edge's faces give no cross section.
fn section_fits<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    gone: &[refs::GoneEdge],
    sections: &[Vec<Section>],
    after: &[&K::Shape],
) -> Result<Vec<Vec<f64>>, String> {
    let mut points = Vec::new();
    let mut owners = Vec::new();
    for (k, edge) in gone.iter().enumerate() {
        for (i, list) in sections.iter().enumerate() {
            for section in list {
                if let Some(p) = refs::section_points(kernel, shape, edge, *section) {
                    owners.push((k, i, section.size()));
                    points.extend(p);
                }
            }
        }
    }
    let mut fits = vec![vec![f64::INFINITY; sections.len()]; gone.len()];
    if points.is_empty() {
        return Ok(fits);
    }
    let distances = refs::nearest_boundary(kernel, after, &points)?;
    for (n, (k, i, size)) in owners.into_iter().enumerate() {
        let worst = distances[3 * n..3 * n + 3]
            .iter()
            .fold(0.0_f64, |a, &d| a.max(d));
        let fit = (worst - FIT_ABSOLUTE).max(0.0) / size.max(1e-9);
        fits[k][i] = fits[k][i].min(fit);
    }
    Ok(fits)
}

/// Edges (by name) in tangent chains, as a fillet follows them: edges
/// that share an end where their tangents are parallel, in the order of
/// their first edges.
fn tangent_chains<K: Kernel>(kernel: &K, shape: &K::Shape, names: &[String]) -> Vec<Vec<String>> {
    use mitcad_model::EdgeName;
    use mitcad_model::datum::PathParameter;
    // Each edge's two ends: the point and the tangent there.
    type Ends = Option<[([f64; 3], [f64; 3]); 2]>;
    let ends: Vec<Ends> = names
        .iter()
        .map(|n| {
            let name: EdgeName = n.parse().ok()?;
            let at = |f: f64| {
                kernel
                    .path_point(
                        shape,
                        std::slice::from_ref(&name),
                        PathParameter::Fraction(f),
                    )
                    .ok()
                    .map(|p| (p.point, p.tangent))
            };
            Some([at(0.0)?, at(1.0)?])
        })
        .collect();
    let mut chain: Vec<usize> = (0..names.len()).collect();
    fn root(chain: &mut [usize], mut i: usize) -> usize {
        while chain[i] != i {
            chain[i] = chain[chain[i]];
            i = chain[i];
        }
        i
    }
    for a in 0..names.len() {
        for b in a + 1..names.len() {
            let (Some(ea), Some(eb)) = (&ends[a], &ends[b]) else {
                continue;
            };
            let joined = ea.iter().any(|(pa, ta)| {
                eb.iter().any(|(pb, tb)| {
                    let parallel = geom::unit(*ta)
                        .zip(geom::unit(*tb))
                        .is_some_and(|(u, v)| geom::norm(geom::cross(u, v)) < 1e-3);
                    geom::distance(*pa, *pb) < 1e-6 && parallel
                })
            });
            if joined {
                let (ra, rb) = (root(&mut chain, a), root(&mut chain, b));
                chain[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut out: Vec<(usize, Vec<String>)> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let r = root(&mut chain, i);
        match out.iter_mut().find(|(k, _)| *k == r) {
            Some((_, list)) => list.push(name.clone()),
            None => out.push((r, vec![name.clone()])),
        }
    }
    out.into_iter().map(|(_, list)| list).collect()
}

/// The edges of `pool` (gone edges, by name) not in `chosen` that lie in a
/// tangent chain with one of `chosen`, in the pool's order.
fn tangent_continuations<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    chosen: &[String],
    pool: &[&SetGone],
) -> Vec<String> {
    let mut names = chosen.to_vec();
    names.extend(
        pool.iter()
            .filter(|e| !chosen.contains(&e.name))
            .map(|e| e.name.clone()),
    );
    if names.len() == chosen.len() {
        return Vec::new();
    }
    tangent_chains(kernel, shape, &names)
        .into_iter()
        .filter(|chain| chain.iter().any(|n| chosen.contains(n)))
        .flatten()
        .filter(|n| !chosen.contains(n))
        .collect()
}

/// What tells fillet or chamfer definitions apart: each with its sets'
/// edges sorted.
fn dressup_key(defs: &[Value]) -> Vec<String> {
    defs.iter()
        .map(|d| {
            let mut d = d.clone();
            for set in d["sets"].as_array_mut().into_iter().flatten() {
                if let Some(edges) = set["edges"].as_array_mut() {
                    edges.sort_by_key(Value::to_string);
                }
            }
            d.to_string()
        })
        .collect()
}

/// The largest group of edges at one distance (within 3 %) from the state
/// after a rounding.
fn largest_group<'a>(edges: &[&'a SetGone]) -> Vec<&'a SetGone> {
    let mut best: Vec<&SetGone> = Vec::new();
    for e in edges {
        let d = e.distance;
        let group: Vec<&SetGone> = edges
            .iter()
            .copied()
            .filter(|x| (x.distance - d).abs() <= 0.03 * d.max(1e-9))
            .collect();
        if group.len() > best.len() {
            best = group;
        }
    }
    best
}

/// Up to `k` sets of regions (indices into `areas`, which is sorted,
/// largest first) of any size whose total area is within 5 % of `target`
/// (the regions' areas come from sampled outlines), nearest first. A
/// depth-first search with a bounded number of steps.
fn area_subsets(areas: &[f64], target: f64, k: usize) -> Vec<Vec<usize>> {
    struct Search {
        areas: Vec<f64>,
        /// The area of the regions from i on.
        rest: Vec<f64>,
        target: f64,
        tolerance: f64,
        steps: usize,
        best: Vec<(f64, Vec<usize>)>,
        k: usize,
    }
    impl Search {
        fn visit(&mut self, i: usize, sum: f64, chosen: &mut Vec<usize>) {
            if self.steps == 0 {
                return;
            }
            self.steps -= 1;
            let off = (sum - self.target).abs();
            if !chosen.is_empty() && off <= self.tolerance {
                let worst = self.best.last().map(|(o, _)| *o);
                if self.best.len() < self.k || worst.is_some_and(|w| off < w) {
                    self.best.push((off, chosen.clone()));
                    self.best.sort_by(|a, b| a.0.total_cmp(&b.0));
                    self.best.truncate(self.k);
                }
            }
            if i >= self.areas.len()
                || sum + self.rest[i] < self.target - self.tolerance
                || sum > self.target + self.tolerance
            {
                return;
            }
            // With region i, then without it.
            chosen.push(i);
            self.visit(i + 1, sum + self.areas[i], chosen);
            chosen.pop();
            self.visit(i + 1, sum, chosen);
        }
    }
    if target.is_nan() || target <= 0.0 || areas.is_empty() {
        return Vec::new();
    }
    let mut rest = vec![0.0; areas.len() + 1];
    for i in (0..areas.len()).rev() {
        rest[i] = rest[i + 1] + areas[i];
    }
    let mut search = Search {
        areas: areas.to_vec(),
        rest,
        target,
        tolerance: 0.05 * target,
        steps: 200_000,
        best: Vec::new(),
        k,
    };
    search.visit(0, 0.0, &mut Vec::new());
    search.best.into_iter().map(|(_, c)| c).collect()
}

/// A feature's profile references: a list, or one profile (external dumps
/// may write `ExtrudeFeature.profile` as a single reference).
pub(crate) fn profiles_of(
    list: &Option<Vec<Reference>>,
    other: &serde_json::Map<String, Value>,
) -> Vec<Reference> {
    if let Some(list) = list {
        return list.clone();
    }
    match other.get("profile") {
        Some(Value::Object(map)) => vec![Reference::from_map(map.clone())],
        _ => Vec::new(),
    }
}

/// Whether two lists of region keys name the same regions.
fn same_keys(a: &[String], b: &[String]) -> bool {
    a.len() == b.len() && {
        let mut a: Vec<&String> = a.iter().collect();
        let mut b: Vec<&String> = b.iter().collect();
        a.sort();
        b.sort();
        a == b
    }
}

/// Profile regions of one sketch to try together.
#[derive(Debug, Clone, Default)]
pub(crate) struct RegionSet {
    pub keys: Vec<String>,
    pub guess: bool,
    /// Their total area, mm².
    area: f64,
    /// Each region's centroid (sketch mm) and area.
    parts: Vec<([f64; 2], f64)>,
    /// Regions found by probing a history state: the side of the sketch
    /// plane (±1 along its normal) where the state changes their material,
    /// and the state's volume change (mm³).
    probe: Option<(f64, f64)>,
    /// The one set the dump's measured profiles make up with a union of
    /// regions: tried before the sets found without the measures.
    measured: bool,
}

impl RegionSet {
    /// The volume of a turn by `angle` about a line of the sketch plane
    /// (Pappus), mm³; the line through `a` along the unit `d`.
    fn turned(&self, a: [f64; 2], d: [f64; 2], angle: f64) -> f64 {
        self.parts
            .iter()
            .map(|(c, area)| {
                let r = ((c[0] - a[0]) * d[1] - (c[1] - a[1]) * d[0]).abs();
                area * r * angle.abs()
            })
            .sum()
    }
}

/// Replaces every string `from` in a JSON value by `to`.
fn replace_text(v: &mut Value, from: &str, to: &str) {
    match v {
        Value::String(s) if s == from => *s = to.to_owned(),
        Value::Array(a) => a.iter_mut().for_each(|x| replace_text(x, from, to)),
        Value::Object(m) => m.values_mut().for_each(|x| replace_text(x, from, to)),
        _ => {}
    }
}

/// The sketch's frame: `model_frame` (sketch → model), else `origin`,
/// `xDirection`, `yDirection`.
fn sketch_frame(detail: &SketchDetail) -> Option<SketchFrame> {
    if let Some(m) = detail.model_frame.as_ref().and_then(|f| f.sketch_to_model) {
        return geom::frame_of(&m);
    }
    let (o, x, y) = (detail.origin?, detail.x_direction?, detail.y_direction?);
    let m = [
        [x[0], y[0], 0.0, o[0]],
        [x[1], y[1], 0.0, o[1]],
        [x[2], y[2], 0.0, o[2]],
        [0.0, 0.0, 0.0, 1.0],
    ];
    geom::frame_of(&m)
}

/// The model X axis, or Y when the normal is along X.
fn default_x(normal: [f64; 3]) -> [f64; 3] {
    let x = if normal[0].abs() > 1.0 - 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    geom::unit(geom::sub(x, geom::scale(normal, geom::dot(x, normal)))).unwrap_or(x)
}

/// Whether a point is inside a polygon (even-odd rule).
fn in_polygon(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The box of a polygon: (low corner, high corner).
fn bounds(polygon: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in polygon {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// Where a polygon's edges cross the line at height `y`, sorted.
fn crossings(polygon: &[[f64; 2]], y: f64, out: &mut Vec<f64>) {
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > y) != (b[1] > y) {
            out.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
        }
    }
}

/// A point inside each region: inside its outer outline but not in one of
/// its holes; the centroid when it is, else the middle of the widest
/// stretch of a horizontal scan line inside it; None when the region is
/// too thin to find one.
pub(crate) fn interior_points(regions: &[Region]) -> Vec<Option<[f64; 2]>> {
    regions
        .iter()
        .map(|r| {
            let (lo, hi) = bounds(&r.outline);
            let own = |p: [f64; 2]| {
                in_polygon(&r.outline, p) && !r.holes.iter().any(|h| in_polygon(h, p))
            };
            if own(r.centroid) {
                return Some(r.centroid);
            }
            // The widest stretch of a scan line inside it.
            let lines = 64;
            let mut best: Option<(f64, [f64; 2])> = None;
            let mut xs = Vec::new();
            for i in 0..lines {
                let y = lo[1] + (i as f64 + 0.5) / lines as f64 * (hi[1] - lo[1]);
                xs.clear();
                crossings(&r.outline, y, &mut xs);
                for h in &r.holes {
                    crossings(h, y, &mut xs);
                }
                xs.sort_by(f64::total_cmp);
                for w in xs.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    if b - a <= best.map_or(1e-9, |(width, _)| width) {
                        continue;
                    }
                    let p = [(a + b) / 2.0, y];
                    if own(p) {
                        best = Some((b - a, p));
                    }
                }
            }
            best.map(|(_, p)| p)
        })
        .collect()
}

/// Calls `f` with each set of `k` of `n` indices, in lexicographic order,
/// at most `limit` times.
pub(crate) fn combinations(n: usize, k: usize, limit: usize, f: &mut dyn FnMut(&[usize])) {
    if k == 0 || k > n {
        return;
    }
    let mut c: Vec<usize> = (0..k).collect();
    let mut count = 0;
    loop {
        f(&c);
        count += 1;
        if count >= limit {
            return;
        }
        // Next combination.
        let mut i = k;
        loop {
            if i == 0 {
                return;
            }
            i -= 1;
            if c[i] < n - k + i {
                break;
            }
            if i == 0 {
                return;
            }
        }
        if c[i] >= n - k + i {
            return;
        }
        c[i] += 1;
        for j in i + 1..k {
            c[j] = c[j - 1] + 1;
        }
    }
}

/// The dropped items, counted by reason.
fn summarize(dropped: &[String]) -> String {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for d in dropped {
        let reason = d.split_once(": ").map_or(d.as_str(), |(_, r)| r).to_owned();
        *counts.entry(reason).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(r, n)| format!("{n} × {r}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extra fields of an edge set the IR does not model.
trait OtherRef {
    fn other_ref(&self, key: &str) -> Option<Reference>;
}

impl OtherRef for EdgeSet {
    fn other_ref(&self, key: &str) -> Option<Reference> {
        let v = self.other.get(key)?.as_object()?.clone();
        Some(Reference::from_map(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_in_order() {
        let mut out = Vec::new();
        combinations(4, 2, 100, &mut |c| out.push(c.to_vec()));
        assert_eq!(
            out,
            vec![
                vec![0, 1],
                vec![0, 2],
                vec![0, 3],
                vec![1, 2],
                vec![1, 3],
                vec![2, 3]
            ]
        );
        let mut n = 0;
        combinations(10, 3, 5, &mut |_| n += 1);
        assert_eq!(n, 5);
        let mut one = Vec::new();
        combinations(3, 3, 10, &mut |c| one.push(c.to_vec()));
        assert_eq!(one, vec![vec![0, 1, 2]]);
    }

    #[test]
    fn region_sets_of_any_size_by_area() {
        // A corpus design's Extrude1: two of five regions make the area
        // the history's new body asks for.
        let areas = [215.189, 123.729, 28.540, 4.811, 4.811];
        let found = area_subsets(&areas, 1016.756 / 3.0, 6);
        assert_eq!(found[0], vec![0, 1]);
        assert!(found.iter().all(|s| {
            let a: f64 = s.iter().map(|&i| areas[i]).sum();
            (a - 338.92).abs() <= 0.05 * 338.92
        }));
        assert!(area_subsets(&areas, 1000.0, 6).is_empty());
        assert!(area_subsets(&[], 10.0, 6).is_empty());
    }
}
