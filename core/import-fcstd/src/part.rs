// SPDX-License-Identifier: MIT
//! The Part workbench's objects of a replayed result: each a Mitcad
//! feature on the bodies of its operands (a copy of an operand that a later
//! object uses too), checked against its stored shape like PartDesign's
//! features ([`super::history`]).
//!
//! - Box, Cylinder, Sphere, Torus, Cone, Prism, Wedge, Ellipsoid → new
//!   bodies at their placements ([`super::primitives`]).
//! - Extrusion of a sketch (solid) → `extrude` along the sketch's normal
//!   (or a custom direction along it): forward and reverse lengths,
//!   symmetric, reversed, tapers; along a custom direction or an edge's
//!   off the normal → `extrude` with that `direction` (mitcad#541: the
//!   lengths along it, tapers too).
//! - Extrusion of a sketch into faces (Solid off) → a surface `extrude`
//!   of the sketch's curves (walls without caps; tapered: of its regions),
//!   off the normal along the `direction`.
//! - Extrusion of an object's faces (a Part::Face, a mirrored face, a
//!   scripted object's face; Solid on or off: FreeCAD extrudes a face into
//!   a solid either way) → an `extrude` of each face of its bodies
//!   (`profile_face`) along the face's normal, or FreeCAD's `direction`
//!   off it (a curved face only so), with the tapers (mitcad#541), then a
//!   base feature without bodies that removes the faces' bodies (the
//!   extrusion consumes them).
//! - Extrusion of an object's edges (a wire, circle or polygon without
//!   faces, mitcad#541) → a sketch of its edges as fixed geometry on a
//!   fixed plane, extruded as a sketch is (`partial`: the object's own
//!   definition is not carried over).
//! - Face of sketches (Part::Face) → `patch` of each sketch's regions.
//! - Revolution of a sketch (solid) → `revolve` about its axis (a fixed
//!   axis through its base point).
//! - Cut, Fuse, MultiFuse, Common, MultiCommon → `combine` (the first
//!   operand the target, the others the tools).
//! - Fillet, Chamfer → `fillet`, `chamfer` on the operand's edges
//!   (FreeCAD's edge list: number, first and second size): a fillet's edge
//!   of two radii a variable radius from its start vertex, a chamfer's two
//!   sizes two distances (either face first).
//! - Mirroring → `mirror` of the operand's body, then a base feature that
//!   removes the operand's body (FreeCAD's mirroring consumes it).
//! - Sweep of sketches along a sketch (mitcad#543) → a new body: `sweep`
//!   of one section, `loft` along the spine of several; Frenet as the
//!   Frenet `frame`; Solid off as a surface. Along a Part helix, a body's
//!   edges or other objects' edges, and of other objects' closed edges or
//!   vertices (mitcad#547, [`super::sweep_inputs`]).
//!
//! Other objects of the tree (plain shapes, compounds, objects Mitcad does
//! not build) become base features of their stored shapes, which the
//! objects using them continue on.

use mitcad_freecad::sketch::{self as fc, Curve};
use mitcad_freecad::{LinkRef, Object, Value as FcValue};
use mitcad_model::sketch::project::{entities, project};
use mitcad_model::{
    BodyKind, BodyUid, ComponentUid, Curve3, ElementKind, EntityUid, FaceName, Kernel, SketchFrame,
};
use serde_json::{Value, json};

use super::history::{Candidate, Target};
use super::params::{Kind, Q};
use super::{Importer, NOT_CONSUMING};
use mitcad_import_base::geom::{cross, dot, norm, scale, sub, unit};

impl<K: Kernel> Importer<'_, '_, K> {
    /// A Part workbench object of a replayed result.
    pub(super) fn part_item(&mut self, o: &Object) {
        let Some(place) = self.place_of(&o.name, false) else {
            self.feature_skipped(o, None, "its container was not imported");
            return;
        };
        // Edges only, which Part extrusions alone use (mitcad#541): part of
        // their sketches, no body of their own.
        if self.edges_of_extrusions(o) {
            let report = super::report::FeatureReport {
                object: o.name.clone(),
                label: o.label().to_owned(),
                object_type: o.type_name.clone(),
                outcome: super::report::ObjectOutcome::Included,
                notes: vec![
                    "edges without faces: fixed geometry of the sketches (or paths) of the Part \
                     extrusions and sweeps that use it"
                        .to_owned(),
                ],
                ..super::report::FeatureReport::default()
            };
            self.push_feature(o, report);
            return;
        }
        // The operands' bodies, a copy where another object uses one too.
        let mut operands: Vec<(String, BodyUid)> = Vec::new();
        let mut problems = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let consumed: Vec<String> = o
            .references()
            .filter(|(p, l)| {
                !NOT_CONSUMING.contains(p)
                    && !super::sweep_inputs::reads_only(o, p)
                    && l.file.is_none()
            })
            .map(|(_, l)| l.object.clone())
            .filter(|n| self.replay.replayed.contains(n))
            .collect();
        for name in consumed {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(body) = self.operand_body(&name) else {
                continue;
            };
            let left = self.replay.uses.get(&name).copied().unwrap_or(1);
            if left > 1 {
                match self.copy_body(
                    body,
                    &format!("{} (copy for {})", self.sources[0].label(&name), o.label()),
                    place.component,
                ) {
                    Ok(copy) => operands.push((name.clone(), copy)),
                    Err(e) => problems.push(format!("{name}: no copy: {e}")),
                }
            } else {
                operands.push((name.clone(), body));
            }
            if let Some(n) = self.replay.uses.get_mut(&name) {
                *n = n.saturating_sub(1);
            }
        }
        let target = Target {
            current: operands.iter().map(|(_, b)| *b).collect(),
            place,
            body: None,
        };
        let (candidates, mitcad) = if problems.is_empty() {
            self.part_candidates(o, &target, &operands)
        } else {
            (Err(problems.join("; ")), "")
        };
        let (report, _) = self.replay_object(o, &target, candidates, mitcad);
        self.push_feature(o, report);
    }

    /// Whether an object the replay has no translation of is edges without
    /// faces (a wire, a circle, a polygon) that only Part extrusions use as
    /// their base (mitcad#541): they make sketches of its edges.
    fn edges_of_extrusions(&mut self, o: &Object) -> bool {
        let t = o.type_name.as_str();
        let translated = super::history::is_part_op(t)
            || t.strip_prefix("Part::")
                .and_then(super::primitives::part_kind)
                .is_some();
        if translated {
            return false;
        }
        let doc = &self.sources[0].file.document;
        let mut users = doc.objects.iter().filter(|u| {
            u.references()
                .any(|(p, l)| !NOT_CONSUMING.contains(&p) && l.object == o.name)
        });
        // (Also Part sweeps' spines and sections, mitcad#547.)
        let only_extrusions = users.all(|u| {
            (u.type_name == "Part::Extrusion" && u.link("Base").is_some_and(|l| l.object == o.name))
                || super::sweep_inputs::sweep_uses(u, &o.name)
        });
        let used = doc.objects.iter().any(|u| {
            u.references()
                .any(|(p, l)| !NOT_CONSUMING.contains(&p) && l.object == o.name)
        });
        if !only_extrusions || !used {
            return false;
        }
        let Some(shape) = self.stored_shape(&o.name) else {
            return false;
        };
        self.doc
            .kernel()
            .faces(&shape)
            .is_ok_and(|faces| faces.is_empty())
    }

    /// The Mitcad body holding an operand's result (a Body's, a Part
    /// object's).
    fn operand_body(&self, name: &str) -> Option<BodyUid> {
        if let Some(bodies) = self.replay.bodies.get(name) {
            return bodies
                .iter()
                .copied()
                .find(|b| self.doc.body_shape(*b).is_some());
        }
        self.body_of_object(name)
    }

    /// A copy of a body (a `move` that copies in place).
    fn copy_body(
        &mut self,
        body: BodyUid,
        label: &str,
        component: ComponentUid,
    ) -> Result<BodyUid, String> {
        let def = json!({"type": "move", "bodies": [body.to_string()], "copy": true,
                         "transform": {"type": "translate_xyz", "x": 0.0, "y": 0.0, "z": 0.0}});
        let uid = self.add_named(&def, label, component)?;
        Ok(BodyUid::new(uid, 0))
    }

    fn part_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        operands: &[(String, BodyUid)],
    ) -> (Result<Vec<Candidate>, String>, &'static str) {
        let body_of = |property: &str| -> Result<BodyUid, String> {
            let link = o.link(property).ok_or_else(|| format!("no {property}"))?;
            operands
                .iter()
                .find(|(n, _)| *n == link.object)
                .map(|(_, b)| *b)
                .ok_or_else(|| format!("{} has no body here", link.object))
        };
        let primitive = o
            .type_name
            .strip_prefix("Part::")
            .and_then(super::primitives::part_kind);
        if let Some(kind) = primitive {
            // The type is the main definition's.
            return (self.primitive_candidates(o, &target.place, kind, None), "");
        }
        match o.type_name.as_str() {
            "Part::Extrusion" => (self.extrusion_candidates(o, target, operands), "extrude"),
            "Part::Face" => (self.part_face_candidates(o, target), "patch"),
            "Part::Revolution" => (self.part_revolution_candidates(o, target), "revolve"),
            "Part::Cut" | "Part::Fuse" | "Part::Common" => {
                let result = (|| -> Result<Vec<Candidate>, String> {
                    let (base, tool) = (body_of("Base")?, body_of("Tool")?);
                    Ok(vec![combine(o, base, &[tool])])
                })();
                (result, "combine")
            }
            "Part::MultiFuse" | "Part::MultiCommon" => {
                let bodies: Vec<BodyUid> = o
                    .links("Shapes")
                    .iter()
                    .filter_map(|l| {
                        operands
                            .iter()
                            .find(|(n, _)| *n == l.object)
                            .map(|(_, b)| *b)
                    })
                    .collect();
                let result = match bodies.split_first() {
                    Some((first, rest)) if !rest.is_empty() => Ok(vec![combine(o, *first, rest)]),
                    _ => Err("fewer than two shapes".to_owned()),
                };
                (result, "combine")
            }
            "Part::Fillet" | "Part::Chamfer" => (
                body_of("Base").and_then(|b| self.part_dressup_candidates(o, b)),
                if o.type_name == "Part::Fillet" {
                    "fillet"
                } else {
                    "chamfer"
                },
            ),
            "Part::Mirroring" => (
                body_of("Source").and_then(|b| self.mirroring_candidates(o, target, b)),
                "mirror",
            ),
            // Sweeps of sketches (mitcad#543).
            "Part::Sweep" => (self.part_sweep_candidates(o, target), ""),
            other => (
                Err(super::history::no_counterpart(other)
                    .unwrap_or_else(|| format!("{other} is not translated yet"))),
                "",
            ),
        }
    }

    /// A Part extrusion: of a sketch (into a solid, or into faces with
    /// Solid off), or of the planar faces of another object's body.
    fn extrusion_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        operands: &[(String, BodyUid)],
    ) -> Result<Vec<Candidate>, String> {
        let base = o.link("Base").ok_or("no base")?.clone();
        let of_sketch = self.sources[0]
            .object(&base.object)
            .is_some_and(|b| super::is_sketch(&b.type_name));
        if !of_sketch {
            return self.face_extrusion(o, target, operands, &base.object);
        }
        if o.bool("Solid") != Some(true) {
            return self.sheet_extrusion(o, target, &base);
        }
        let profile = self.profile(o, "Base", target.place.component)?;
        let what = json!({"profiles": sketch_profiles(&profile)});
        let (along, dir) = self.extrusion_direction(o, target, profile.sketch.frame.normal())?;
        Ok(self.part_readings(o, what, along, dir, true))
    }

    /// The direction of a Part extrusion against its profile's normal
    /// (DirMode: Normal, or a direction of its own or an edge's, which
    /// `Dir` holds), with FreeCAD's direction (its length is the length
    /// when the extrusion has none). Reversed turns it.
    fn extrusion_direction(
        &self,
        o: &Object,
        target: &Target,
        normal: [f64; 3],
    ) -> Result<(Along, [f64; 3]), String> {
        let mode = self
            .enum_text(o, "DirMode")
            .unwrap_or_else(|| "Normal".to_owned());
        let flip = o.bool("Reversed").unwrap_or(false);
        let dir = match o.value("Dir") {
            Some(FcValue::Vector(v)) => target.place.transform.apply_vector(*v),
            _ => normal,
        };
        match mode.as_str() {
            "Normal" => Ok((Along::Normal(flip), dir)),
            "Custom" | "Edge" => {
                if norm(cross(dir, normal)) > 1e-9 * norm(dir) {
                    return Ok((Along::Oblique(flip), dir));
                }
                Ok((Along::Normal(flip ^ (dot(dir, normal) < 0.0)), dir))
            }
            other => Err(format!("an extrusion along {other} is not translated yet")),
        }
    }

    /// FreeCAD's forward and reverse lengths of a Part extrusion: the
    /// direction's length forward when both are zero.
    fn extrusion_lengths(&mut self, o: &Object, dir: [f64; 3]) -> (Q, Q) {
        let mut forward = self.prop(o, "LengthFwd", Kind::Length, 0.0);
        let reverse = self.prop(o, "LengthRev", Kind::Length, 0.0);
        if forward.value == 0.0 && reverse.value == 0.0 {
            forward = Q::number(norm(dir), Kind::Length);
        }
        (forward, reverse)
    }

    /// The readings of an extrusion of a sketch (`what`: its `profiles`,
    /// and `surface` for an extrusion into faces, or its `curves`): the
    /// lengths and, with `tapers`, the tapers (both signs), then the other
    /// way (not when symmetric). Along the normal, or off it along
    /// FreeCAD's direction (mitcad#541: the extrusion's `direction`, the
    /// lengths along it, a taper growing the profile in its plane as
    /// FreeCAD grows it).
    fn part_readings(
        &mut self,
        o: &Object,
        what: Value,
        along: Along,
        dir: [f64; 3],
        tapers: bool,
    ) -> Vec<Candidate> {
        let (forward, reverse) = self.extrusion_lengths(o, dir);
        let (taper, taper_rev) = if tapers {
            (
                self.prop(o, "TaperAngle", Kind::Angle, 0.0),
                self.prop(o, "TaperAngleRev", Kind::Angle, 0.0),
            )
        } else {
            (Q::number(0.0, Kind::Angle), Q::number(0.0, Kind::Angle))
        };
        let symmetric = o.bool("Symmetric") == Some(true);
        let (flip, direction) = match along {
            Along::Normal(flip) => (flip, None),
            Along::Oblique(flip) => (flip, Some(dir)),
        };
        let mut variants = vec![(flip, 1.0)];
        if taper.value != 0.0 || taper_rev.value != 0.0 {
            variants.push((flip, -1.0));
        }
        if !symmetric {
            variants.push((!flip, 1.0));
        }
        variants
            .into_iter()
            .map(|(flip, sign)| {
                let (extent, against) = part_extent(
                    &forward,
                    &reverse,
                    symmetric,
                    Some((&taper, &taper_rev, sign)),
                );
                let mut def = json!({"type": "extrude", "extent": extent,
                                     "flip": flip ^ against, "operation": "new_body"});
                if let Some(d) = direction {
                    def["direction"] = json!(d);
                }
                merge(&mut def, &what);
                let candidate = Candidate::new(def);
                if direction.is_some() {
                    candidate.note("along FreeCAD's direction, off the sketch's normal")
                } else {
                    candidate
                }
            })
            .collect()
    }

    /// An extrusion of a sketch into faces (Solid off): FreeCAD's prism of
    /// the sketch's wires, walls without caps: a surface extrusion of the
    /// sketch's curves (not construction), off the normal along FreeCAD's
    /// direction; tapered (curves take no taper), the walls of the
    /// sketch's regions.
    fn sheet_extrusion(
        &mut self,
        o: &Object,
        target: &Target,
        base: &LinkRef,
    ) -> Result<Vec<Candidate>, String> {
        let component = target.place.component;
        let made = self.sketch_of(base, component)?;
        let normal = made.frame.normal();
        let tapered = ["TaperAngle", "TaperAngleRev"]
            .iter()
            .any(|t| o.f64(t).is_some_and(|a| a != 0.0));
        let (along, dir) = self.extrusion_direction(o, target, normal)?;
        if tapered {
            let profile = self.profile(o, "Base", component).map_err(|e| {
                format!("a tapered extrusion into faces of sketch curves without a closed profile is not translated ({e})")
            })?;
            let what = json!({"surface": true, "profiles": sketch_profiles(&profile)});
            return Ok(self
                .part_readings(o, what, along, dir, true)
                .into_iter()
                .map(|c| c.note("tapered into faces: the walls of the sketch's regions"))
                .collect());
        }
        let sketch = self.sources[0]
            .object(&base.object)
            .map(fc::Sketch::of)
            .ok_or("no sketch")?;
        let mut all: Vec<(i32, u32)> = made.curves.iter().map(|(g, id)| (*g, *id)).collect();
        all.sort_unstable();
        // The sketch's shape: its own curves that are not construction, and
        // (1.1) external geometry that is defining.
        let curves: Vec<String> = all
            .into_iter()
            .filter(|(g, _)| {
                sketch.geometry(*g).is_some_and(|x| {
                    !matches!(x.curve, Curve::Point { .. })
                        && if *g >= 0 { !x.construction } else { x.defining }
                })
            })
            .map(|(_, id)| format!("c{id}"))
            .collect();
        if curves.is_empty() {
            return Err("the sketch has no curves to extrude".to_owned());
        }
        let what = json!({"surface": true,
                          "curves": {"sketch": made.uid.to_string(), "curves": curves}});
        Ok(self
            .part_readings(o, what, along, dir, false)
            .into_iter()
            .map(|c| c.note("into faces: the walls of the sketch's curves"))
            .collect())
    }

    /// An extrusion of the faces of another object's bodies (Solid on or
    /// off: FreeCAD extrudes faces into solids either way): an `extrude` of
    /// each face (`profile_face`) by the lengths with the tapers, along the
    /// face's normal, or along FreeCAD's direction where it is off the
    /// normal (`direction`, mitcad#541; a curved face only so, without a
    /// taper), then a base feature without bodies that removes the faces'
    /// bodies, which the extrusion consumes. All of the base's bodies
    /// (mitcad#541); where another object uses the base too, its first body
    /// is the copy made for this one and the others stay for that object.
    /// A base without faces goes to [`Self::edges_extrusion`].
    fn face_extrusion(
        &mut self,
        o: &Object,
        target: &Target,
        operands: &[(String, BodyUid)],
        base: &str,
    ) -> Result<Vec<Candidate>, String> {
        let operand = operands.iter().find(|(n, _)| n == base).map(|(_, b)| *b);
        let Some(operand) = operand else {
            return self.edges_extrusion(o, target, base, None);
        };
        let recorded: Vec<BodyUid> = self
            .replay
            .results
            .get(base)
            .map(|m| m.bodies.iter().map(|(b, _)| *b).collect())
            .unwrap_or_default();
        let recorded: Vec<BodyUid> = recorded
            .into_iter()
            .filter(|b| self.doc.body_shape(*b).is_some())
            .collect();
        // Not the base's own body: a copy of its first one.
        let copied = !recorded.contains(&operand);
        let (bodies, removed) = if copied {
            let mut bodies = vec![operand];
            bodies.extend(recorded.iter().skip(1).copied());
            (bodies, vec![operand])
        } else if recorded.is_empty() {
            (vec![operand], vec![operand])
        } else {
            (recorded.clone(), recorded)
        };
        // The faces: their body, name, whether planar and their normals
        // (out of the material: the face's orientation), before any value
        // is read; a face whose plane the kernel does not give goes along
        // its own normal or FreeCAD's direction (the readings try both
        // ways).
        let mut faces: Vec<(BodyUid, String, bool, Option<[f64; 3]>)> = Vec::new();
        for body in &bodies {
            let shape = self
                .doc
                .body_shape(*body)
                .ok_or("the base's body is gone")?;
            let kernel = self.doc.kernel();
            match kernel.body_kind(shape) {
                Ok(BodyKind::Solid) => {
                    return Err("an extrusion of a solid is not translated".to_owned());
                }
                Ok(BodyKind::Empty) if bodies.len() == 1 => {
                    // A body without faces (the edges' stored shape).
                    return self.edges_extrusion(o, target, base, Some(*body));
                }
                _ => {}
            }
            for face in kernel.faces(shape).map_err(|e| e.to_string())? {
                let name = face.names.first().ok_or("a face of the base has no name")?;
                let parsed = name.parse::<FaceName>().map_err(|e| e.to_string())?;
                let planar = face.surface == "plane";
                let normal = if planar {
                    kernel.face_plane(shape, &parsed).ok().map(|p| p.normal)
                } else {
                    None
                };
                faces.push((*body, name.clone(), planar, normal));
            }
        }
        if faces.is_empty() {
            return self.edges_extrusion(o, target, base, Some(operand));
        }
        // FreeCAD's direction (DirMode Normal: the base's normal, which
        // `Dir` keeps; Custom, Edge: `Dir`): each face along its normal or
        // against it, else along the direction.
        let mode = self
            .enum_text(o, "DirMode")
            .unwrap_or_else(|| "Normal".to_owned());
        let custom = match mode.as_str() {
            "Normal" => false,
            "Custom" | "Edge" => true,
            other => return Err(format!("an extrusion along {other} is not translated yet")),
        };
        let dir = match o.value("Dir") {
            Some(FcValue::Vector(v)) => Some(target.place.transform.apply_vector(*v)),
            _ => None,
        }
        .filter(|d| norm(*d) > 0.0);
        if custom && dir.is_none() {
            return Err("no direction".to_owned());
        }
        let reversed = o.bool("Reversed").unwrap_or(false);
        let tapered = ["TaperAngle", "TaperAngleRev"]
            .iter()
            .any(|t| o.f64(t).is_some_and(|a| a != 0.0));
        // Per face: whether it goes against its normal, and the direction
        // it goes along when that is off its normal.
        let mut ways: Vec<(bool, Option<[f64; 3]>)> = Vec::new();
        for (_, _, planar, normal) in &faces {
            let way = match (dir, normal) {
                (Some(d), Some(n)) if norm(cross(d, *n)) <= 1e-7 * norm(d) => {
                    (dot(d, *n) < 0.0, None)
                }
                (Some(d), _) if custom => (false, Some(d)),
                // FreeCAD fails these itself (mitcad#547): its Normal
                // direction looks for the plane the base lies in, and its
                // taper offsets planar wires only.
                _ if !planar => {
                    return Err(
                        "a curved face along the base's normal: FreeCAD finds no normal of a \
                         face that is not planar (its own extrusion fails)"
                            .to_owned(),
                    );
                }
                _ => (false, None),
            };
            if !planar && tapered {
                return Err(
                    "a tapered curved face: FreeCAD tapers by offsetting planar wires (its own \
                     extrusion fails)"
                        .to_owned(),
                );
            }
            ways.push((way.0 ^ reversed, way.1));
        }
        let (forward, reverse) = self.extrusion_lengths(o, dir.unwrap_or([0.0, 0.0, 1.0]));
        let taper = self.prop(o, "TaperAngle", Kind::Angle, 0.0);
        let taper_rev = self.prop(o, "TaperAngleRev", Kind::Angle, 0.0);
        let symmetric = o.bool("Symmetric") == Some(true);
        let candidate = |turn: bool, sign: f64| {
            let (extent, against) = part_extent(
                &forward,
                &reverse,
                symmetric,
                Some((&taper, &taper_rev, sign)),
            );
            let mut defs: Vec<(Value, &'static str)> = faces
                .iter()
                .zip(&ways)
                .enumerate()
                .map(|(i, ((body, face, _, _), (flip, direction)))| {
                    let mut def = json!({"type": "extrude",
                           "profile_face": {"body": body.to_string(), "face": face},
                           "extent": extent, "flip": flip ^ against ^ turn,
                           "operation": "new_body"});
                    if let Some(d) = direction {
                        def["direction"] = json!(d);
                    }
                    (
                        def,
                        FACE_SUFFIXES.get(i).copied().unwrap_or(" (more faces)"),
                    )
                })
                .collect();
            let removed: Vec<String> = removed.iter().map(ToString::to_string).collect();
            defs.push((
                json!({"type": "base", "bodies": [], "replaces": removed}),
                " (base removed)",
            ));
            Candidate::several(defs).note(
                "the extruded faces' bodies removed by a base feature without bodies \
                 (the extrusion consumes them)",
            )
        };
        let mut out = vec![candidate(false, 1.0)];
        if taper.value != 0.0 || taper_rev.value != 0.0 {
            out.push(candidate(false, -1.0));
        }
        if !symmetric {
            out.push(candidate(true, 1.0));
        }
        Ok(out)
    }

    /// An extrusion of an object's edges (an object without faces that is
    /// not a sketch: a wire, a circle, a polygon; mitcad#541): the edges as
    /// fixed geometry of a sketch on a fixed plane (the object's placement
    /// where they lie in it, else the plane through them and FreeCAD's
    /// direction), then that sketch extruded as a sketch is: Solid on its
    /// one closed profile, Solid off a surface extrusion of its curves.
    /// `consumed`: a body of the edges to remove with it.
    fn edges_extrusion(
        &mut self,
        o: &Object,
        target: &Target,
        base: &str,
        consumed: Option<BodyUid>,
    ) -> Result<Vec<Candidate>, String> {
        let base_object = self.sources[0]
            .object(base)
            .cloned()
            .ok_or_else(|| format!("{base} is not in the document"))?;
        // A Part helix (mitcad#547): its own helix, from its sizes (a
        // conical one its stored curves, mitcad#548).
        if base_object.type_name == "Part::Helix" {
            let helix = self.part_helix(&base_object, target)?;
            return self.helix_extrusion(o, target, base, helix.path, helix.start, consumed);
        }
        let shape = self
            .stored_in(base, &target.place.transform)
            .ok_or_else(|| format!("{base} has no stored shape"))?;
        // Its edges' curves, in its component.
        let mut curves = Vec::new();
        {
            let kernel = self.doc.kernel();
            for index in 0..MAX_EDGES {
                match kernel.indexed_element(&shape, ElementKind::Edge, index) {
                    Ok(Some(edge)) => curves.extend(edge.curves),
                    Ok(None) => break,
                    Err(e) => return Err(format!("the edges of {base}: {e}")),
                }
            }
        }
        if curves.is_empty() {
            return Err(format!("{base} has neither faces nor edges to extrude"));
        }
        // One helix (mitcad#547): swept along it.
        if let [helix @ Curve3::Helix { .. }] = curves.as_slice() {
            let (path, start) = helix_path(helix).ok_or("no helix")?;
            return self.helix_extrusion(o, target, base, path, start, consumed);
        }
        // A helix among other edges (mitcad#548): its walls in pieces
        // (below: no one plane).
        // The plane: the object's placement, else through the edges and
        // FreeCAD's direction (a straight edge).
        let placed =
            super::sketches::frame_of(&target.place.transform, &super::placement_of(&base_object));
        let dir = match o.value("Dir") {
            Some(FcValue::Vector(v)) => target.place.transform.apply_vector(*v),
            _ => [0.0; 3],
        };
        let frame = if lies_in(&curves, &placed) {
            placed
        } else if let Some(frame) = spanned(&curves, dir).filter(|f| lies_in(&curves, f)) {
            frame
        } else if o.bool("Solid") == Some(true) {
            // FreeCAD makes faces of planar wires only.
            return Err(format!(
                "the edges of {base} do not lie in one plane: FreeCAD makes no face of them (its \
                 own extrusion fails)"
            ));
        } else {
            // Walls of edges in several planes (mitcad#547).
            return self.walls_of_pieces(o, base, &curves, dir, consumed);
        };
        // The sketch: the edges as fixed geometry.
        let projected = project(&curves, &frame);
        let mut next = 0u32;
        let made = entities(&projected, &mut || {
            next += 1;
            EntityUid(next)
        });
        let curve_ids: Vec<String> = made
            .iter()
            .filter(|e| !e.is_point())
            .map(|e| e.as_ref().to_string())
            .collect();
        let listed: Vec<Value> = made
            .iter()
            .map(|e| serde_json::to_value(e).expect("serializes"))
            .collect();
        let plane = json!({"type": "construction_plane", "definition": {"type": "fixed",
            "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}});
        let sketch = json!({"type": "sketch", "plane": "$0", "entities": listed});
        let tapered = ["TaperAngle", "TaperAngleRev"]
            .iter()
            .any(|t| o.f64(t).is_some_and(|a| a != 0.0));
        let solid = o.bool("Solid") == Some(true);
        // Solid on (or tapered into faces): the material regions of the
        // edges' closed loops (FreeCAD's face maker fills those inside an
        // even number of wires; mitcad#547: any number of them); Solid off:
        // the walls of the curves.
        let what = match (solid, tapered) {
            (true, _) => json!({"profiles": "$1:profiles"}),
            (false, true) => json!({"surface": true, "profiles": "$1:profiles"}),
            (false, false) => {
                json!({"surface": true, "curves": {"sketch": "$1", "curves": curve_ids}})
            }
        };
        let (along, dir) = self.extrusion_direction(o, target, frame.normal())?;
        let removal =
            consumed.map(|b| json!({"type": "base", "bodies": [], "replaces": [b.to_string()]}));
        Ok(self
            .part_readings(o, what, along, dir, solid || tapered)
            .into_iter()
            .map(|c| {
                let mut defs = vec![(plane.clone(), " (plane)"), (sketch.clone(), " (edges)")];
                defs.extend(c.defs.iter().cloned().map(|d| (d, "")));
                if let Some(removal) = &removal {
                    defs.push((removal.clone(), " (base removed)"));
                }
                let mut out = Candidate::several(defs);
                out.notes = c.notes;
                out.partial(format!(
                    "{base}'s edges are fixed geometry of a sketch (FreeCAD's object keeps no \
                     sketch of them)"
                ))
            })
            .collect())
    }

    /// An extrusion of a helix (mitcad#547): FreeCAD's prism of the helix
    /// along its direction, walls without caps (Solid off: FreeCAD makes
    /// no face of an open wire), as a surface `sweep` of a straight line
    /// from the helix's start along the direction (the lengths: back and
    /// forward) along the helix's `path` (a `helix` path, or a conical
    /// helix's fixed curves: [`super::fixed_paths`]), `parallel` (the line
    /// moved along the helix without turning sweeps the prism). No taper.
    fn helix_extrusion(
        &mut self,
        o: &Object,
        target: &Target,
        base: &str,
        path: Value,
        start: [f64; 3],
        consumed: Option<BodyUid>,
    ) -> Result<Vec<Candidate>, String> {
        if o.bool("Solid") == Some(true) {
            return Err(format!(
                "{base} is a helix: FreeCAD makes no face of an open wire (its own extrusion fails)"
            ));
        }
        if ["TaperAngle", "TaperAngleRev"]
            .iter()
            .any(|t| o.f64(t).is_some_and(|a| a != 0.0))
        {
            return Err(
                "a tapered helix: FreeCAD tapers by offsetting planar wires (its own extrusion \
                 fails)"
                    .to_owned(),
            );
        }
        let dir = match o.value("Dir") {
            Some(FcValue::Vector(v)) => target.place.transform.apply_vector(*v),
            _ => [0.0; 3],
        };
        let d = unit(dir).ok_or(
            "a helix along the base's normal: FreeCAD finds no normal of a helix (its own \
             extrusion fails)",
        )?;
        let why = "a sweep of a line along a helix: the line's length is FreeCAD's value";
        let mut forward = self.prop(o, "LengthFwd", Kind::Length, 0.0).value;
        self.left_out(o, "LengthFwd", why);
        let mut reverse = self.prop(o, "LengthRev", Kind::Length, 0.0).value;
        self.left_out(o, "LengthRev", why);
        if forward == 0.0 && reverse == 0.0 {
            forward = norm(dir);
        }
        if o.bool("Symmetric") == Some(true) {
            forward /= 2.0;
            reverse = forward;
        }
        // The line's sketch: through the start, along the direction.
        let other = if d[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let y = unit(cross(cross(d, other), d)).ok_or("no plane for the line")?;
        let plane = json!({"type": "construction_plane", "definition": {"type": "fixed",
            "origin": start, "x_axis": d, "y_axis": y}});
        let reversed = o.bool("Reversed") == Some(true);
        let mut out = Vec::new();
        for sign in [1.0, -1.0] {
            let s = if reversed { -sign } else { sign };
            let (a, b) = (-s * reverse, s * forward);
            let sketch = json!({"type": "sketch", "plane": "$0", "entities": [
                {"id": "p1", "type": "point", "at": [a, 0.0], "fixed": true},
                {"id": "p2", "type": "point", "at": [b, 0.0], "fixed": true},
                {"id": "c3", "type": "line", "start": "p1", "end": "p2"}]});
            let sweep = json!({"type": "sweep", "surface": true,
                "curves": {"sketch": "$1", "curves": ["c3"]}, "path": path,
                "orientation": "parallel", "operation": "new_body"});
            let mut defs = vec![
                (plane.clone(), " (plane)"),
                (sketch, " (line)"),
                (sweep, ""),
            ];
            if let Some(b) = consumed {
                defs.push((
                    json!({"type": "base", "bodies": [], "replaces": [b.to_string()]}),
                    " (base removed)",
                ));
            }
            out.push(
                Candidate::several(defs)
                    .note("a line swept along the helix without turning: its prism")
                    .partial(format!(
                        "{base} is a fixed helix path and the lengths are a fixed line"
                    )),
            );
            if o.bool("Symmetric") == Some(true) {
                break;
            }
        }
        Ok(out)
    }

    /// Walls of edges that lie in no one plane (mitcad#547; Solid off,
    /// along FreeCAD's direction): the edges grouped by planes they lie in
    /// (a line in the plane through it facing the direction, a conic or a
    /// planar spline in its own), each group a sketch of fixed geometry on
    /// a fixed plane and a surface `extrude` of its curves along the
    /// `direction`; together FreeCAD's prism of the wire.
    fn walls_of_pieces(
        &mut self,
        o: &Object,
        base: &str,
        curves: &[Curve3],
        dir: [f64; 3],
        consumed: Option<BodyUid>,
    ) -> Result<Vec<Candidate>, String> {
        let d = unit(dir).ok_or(
            "edges in no one plane along the base's normal: FreeCAD finds no normal of them (its \
             own extrusion fails)",
        )?;
        if ["TaperAngle", "TaperAngleRev"]
            .iter()
            .any(|t| o.f64(t).is_some_and(|a| a != 0.0))
        {
            return Err(
                "tapered edges in no one plane: FreeCAD tapers by offsetting planar wires (its \
                 own extrusion fails)"
                    .to_owned(),
            );
        }
        let mut groups: Vec<(SketchFrame, Vec<Curve3>)> = Vec::new();
        // Curves in no plane (mitcad#548: a helix, a spline leaving every
        // plane): their fixed paths (`{"helix": …}`, `{"spline": …}`) and
        // start points, each swept by a line along the direction.
        let mut swept: Vec<(Value, [f64; 3])> = Vec::new();
        for curve in curves {
            let one = std::slice::from_ref(curve);
            if let Some((_, members)) = groups.iter_mut().find(|(f, _)| lies_in(one, f)) {
                members.push(curve.clone());
                continue;
            }
            if let Some(path) = fixed_path(curve) {
                swept.push(path?);
                continue;
            }
            let frame = match curve {
                Curve3::Point(_) => continue,
                Curve3::Line { start, end } => {
                    let l = unit(sub(*end, *start)).ok_or("an edge of no length")?;
                    // Along the direction: no wall.
                    let Some(n) = unit(sub(d, scale(l, dot(d, l)))) else {
                        continue;
                    };
                    SketchFrame {
                        origin: *start,
                        x_axis: l,
                        y_axis: cross(n, l),
                    }
                }
                Curve3::Conic {
                    center,
                    normal,
                    x_axis,
                    ..
                } => {
                    let n = unit(*normal).ok_or("a conic without a normal")?;
                    if dot(n, d).abs() < 1e-9 {
                        continue;
                    }
                    SketchFrame {
                        origin: *center,
                        x_axis: *x_axis,
                        y_axis: cross(n, *x_axis),
                    }
                }
                Curve3::BSpline { poles, .. } => {
                    let frame = plane_of(poles).ok_or_else(|| {
                        format!(
                            "a curve of {base} that lies in no plane: Mitcad keeps curves in \
                             planar sketches"
                        )
                    })?;
                    // A spline in a plane along the direction: no wall.
                    if dot(frame.normal(), d).abs() < 1e-9 {
                        continue;
                    }
                    frame
                }
                // (`fixed_path` took it.)
                Curve3::Helix { .. } => continue,
            };
            if !lies_in(one, &frame) {
                return Err(format!(
                    "a curve of {base} that lies in no plane: Mitcad keeps curves in planar \
                     sketches"
                ));
            }
            groups.push((frame, vec![curve.clone()]));
        }
        if groups.is_empty() && swept.is_empty() {
            return Err(format!("{base} has no edges that make walls"));
        }
        let (forward, reverse) = self.extrusion_lengths(o, dir);
        let symmetric = o.bool("Symmetric") == Some(true);
        let (extent, against) = part_extent(&forward, &reverse, symmetric, None);
        let reversed = o.bool("Reversed") == Some(true);
        // The swept lines' lengths: FreeCAD's values (a fixed line).
        let (line_forward, line_reverse) = if symmetric {
            (forward.value / 2.0, forward.value / 2.0)
        } else {
            (forward.value, reverse.value)
        };
        if !swept.is_empty() {
            let why = "a sweep of a line along a fixed curve: the line's length is FreeCAD's value";
            self.left_out(o, "LengthFwd", why);
            self.left_out(o, "LengthRev", why);
        }
        let mut out = Vec::new();
        for turn in [false, true] {
            let mut defs: Vec<(Value, &'static str)> = Vec::new();
            for (k, (frame, members)) in groups.iter().enumerate() {
                let projected = project(members, frame);
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
                let (plane, sketch) = (format!("${}", 3 * k), format!("${}", 3 * k + 1));
                defs.push((
                    json!({"type": "construction_plane", "definition": {"type": "fixed",
                        "origin": frame.origin, "x_axis": frame.x_axis, "y_axis": frame.y_axis}}),
                    " (plane)",
                ));
                defs.push((
                    json!({"type": "sketch", "plane": plane, "entities": listed}),
                    " (edges)",
                ));
                defs.push((
                    json!({"type": "extrude", "surface": true,
                           "curves": {"sketch": sketch, "curves": ids},
                           "extent": extent, "direction": d,
                           "flip": reversed ^ against ^ turn, "operation": "new_body"}),
                    WALL_SUFFIXES.get(k).copied().unwrap_or(" (more walls)"),
                ));
            }
            // The curves in no plane (mitcad#548): a line from each one's
            // start along the direction, swept along it without turning.
            let s = if reversed ^ turn { -1.0 } else { 1.0 };
            for (j, (path, start)) in swept.iter().enumerate() {
                let first = defs.len();
                let main = groups.is_empty() && j == 0;
                defs.extend(line_sweep(
                    *start,
                    d,
                    (-s * line_reverse, s * line_forward),
                    path,
                    first,
                    main,
                )?);
            }
            if let Some(b) = consumed {
                defs.push((
                    json!({"type": "base", "bodies": [], "replaces": [b.to_string()]}),
                    " (base removed)",
                ));
            }
            out.push(
                Candidate::several(defs)
                    .note(format!(
                        "walls of edges in {} planes, each a surface extrusion along the \
                         direction, and of {} curves in no plane, each a line swept along it",
                        groups.len(),
                        swept.len()
                    ))
                    .partial(format!(
                        "{base}'s edges are fixed geometry of sketches (FreeCAD's object keeps \
                         no sketch of them)"
                    )),
            );
            if symmetric {
                break;
            }
        }
        Ok(out)
    }

    /// A face of sketches (Part::Face): a `patch` of each sketch's
    /// regions inside an even number of wires (FreeCAD's face makers
    /// Bullseye and Cheese, and Extrusion's).
    fn part_face_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        if let Some(class) = o.str("FaceMakerClass")
            && !matches!(
                class,
                "Part::FaceMakerBullseye"
                    | "Part::FaceMakerCheese"
                    | "Part::FaceMakerExtrusion"
                    | "Part::FaceMakerSimple"
            )
        {
            return Err(format!("a face made by {class} is not translated"));
        }
        // FreeCAD's simple face maker (mitcad#547) makes a face of each
        // closed wire, holes not cut: the same faces where no wire lies in
        // another.
        let simple = o.str("FaceMakerClass") == Some("Part::FaceMakerSimple");
        let component = target.place.component;
        let mut defs: Vec<(Value, &'static str)> = Vec::new();
        // Nested wires with the simple face maker (mitcad#548): per sketch
        // its uid and its regions' keys with their holes' counts.
        let mut nested: Vec<(String, Vec<(String, usize)>)> = Vec::new();
        for (i, link) in o.links("Sources").to_vec().iter().enumerate() {
            let is_sketch = self.sources[0]
                .object(&link.object)
                .is_some_and(|s| super::is_sketch(&s.type_name));
            if !is_sketch {
                return Err(format!(
                    "a face of {} (not a sketch) is not translated",
                    link.object
                ));
            }
            if !link.subs.is_empty() {
                return Err("a face of chosen parts of a sketch is not translated".to_owned());
            }
            let sketch = self.sketch_of(link, component)?;
            let output = self
                .doc
                .sketch_output(sketch.uid)
                .ok_or("the sketch did not solve")?;
            let depths = super::features::region_depths(&output.region_info);
            if simple && depths.iter().any(|(_, depth)| *depth > 0) {
                // FreeCAD's overlapping faces, one filling each wire: each
                // region's outline filled (below).
                let regions = output
                    .region_info
                    .iter()
                    .map(|r| (r.profile.key.to_string(), r.holes.len()))
                    .collect();
                nested.push((sketch.uid.to_string(), regions));
                continue;
            }
            let profiles: Vec<Value> = depths
                .into_iter()
                .filter(|(_, depth)| depth % 2 == 0)
                .map(|(region, _)| json!({"sketch": sketch.uid.to_string(), "region": region}))
                .collect();
            if profiles.is_empty() {
                return Err(format!("{} has no closed profile", link.object));
            }
            defs.push((
                json!({"type": "patch", "profiles": profiles}),
                SKETCH_SUFFIXES
                    .get(i)
                    .copied()
                    .unwrap_or(" (more sketches)"),
            ));
        }
        if nested.is_empty() {
            if defs.is_empty() {
                return Err("no sources".to_owned());
            }
            return Ok(vec![Candidate::several(defs)]);
        }
        // Each wire's face, overlapping as FreeCAD's simple face maker
        // makes them (mitcad#548): a region without holes is its wire's
        // face; a region with holes is patched, the opening of its outer
        // wire filled (a `patch` of the loop: its plane) and the region's
        // patch removed. Which opening is the outer one is the kernel's
        // geometric order: the first, else the last.
        let mut out = Vec::new();
        for last in [false, true] {
            let mut all = defs.clone();
            for (uid, regions) in &nested {
                for (key, holes) in regions {
                    let main = all.is_empty();
                    let region = json!({"sketch": uid, "region": key});
                    if *holes == 0 {
                        all.push((
                            json!({"type": "patch", "profiles": [region]}),
                            if main { "" } else { WIRE_SUFFIX },
                        ));
                        continue;
                    }
                    let k = all.len();
                    all.push((json!({"type": "patch", "profiles": [region]}), " (region)"));
                    let index = if last { *holes } else { 0 };
                    all.push((
                        json!({"type": "patch", "boundary": {"body": format!("${k}.b0"),
                               "opening": {"faces": [format!("${k}:patch({key})")],
                                           "index": index}}}),
                        if main { "" } else { WIRE_SUFFIX },
                    ));
                    all.push((
                        json!({"type": "base", "bodies": [], "replaces": [format!("${k}.b0")]}),
                        " (region removed)",
                    ));
                }
            }
            out.push(Candidate::several(all).note(
                "FreeCAD's simple face maker of nested wires: a face filling each wire, \
                 overlapping (a region's patch, its outer wire's opening patched, the \
                 region's patch removed)",
            ));
        }
        Ok(out)
    }

    /// A revolution of a sketch about a fixed axis.
    fn part_revolution_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        if o.bool("Solid") != Some(true) {
            return Err("a revolution into faces (Solid off) is not translated".to_owned());
        }
        if o.link("AxisLink").is_some() {
            return Err("a revolution about a linked axis is not translated yet".to_owned());
        }
        let profile = self.profile(o, "Source", target.place.component)?;
        let vector = |name: &str| match o.value(name) {
            Some(FcValue::Vector(v)) => Some(*v),
            _ => None,
        };
        let t = &target.place.transform;
        let base = t.apply_point(vector("Base").unwrap_or([0.0; 3]));
        let direction = t.apply_vector(vector("Axis").ok_or("no axis")?);
        let axis = json!({"origin": base, "direction": direction});
        let angle = self.prop(o, "Angle", Kind::Angle, 360.0);
        let extents: Vec<Value> = if angle.value.to_degrees().abs() >= 360.0 - 1e-9 {
            vec![json!({"type": "full"})]
        } else if o.bool("Symmetric") == Some(true) {
            vec![json!({"type": "symmetric", "angle": angle.scale(0.5).json()})]
        } else {
            vec![
                json!({"type": "angle", "angle": angle.json()}),
                json!({"type": "angle", "angle": angle.neg().json()}),
            ]
        };
        let profiles: Vec<Value> = profile
            .regions
            .iter()
            .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
            .collect();
        Ok(extents
            .into_iter()
            .map(|extent| {
                Candidate::new(
                    json!({"type": "revolve", "profiles": profiles, "axis": axis,
                                      "extent": extent, "operation": "new_body"}),
                )
            })
            .collect())
    }

    /// Part::Fillet and Part::Chamfer: the edge list (number, first and
    /// second size) on the operand's body.
    fn part_dressup_candidates(
        &mut self,
        o: &Object,
        body: BodyUid,
    ) -> Result<Vec<Candidate>, String> {
        let base = o.link("Base").ok_or("no base")?.object.clone();
        let list = match o.value("Edges") {
            Some(FcValue::FilletEdges(list)) => list.clone(),
            _ => return Err("its edge list was not read".to_owned()),
        };
        if list.is_empty() {
            return Err("no edges".to_owned());
        }
        // Edge sets by size.
        let mut sets: Vec<((f64, f64), Vec<String>)> = Vec::new();
        for (edge, r1, r2) in &list {
            let (b, names) = self.element_names(&base, &format!("Edge{edge}"))?;
            if b != body {
                return Err(format!("{base}.Edge{edge} is on another body"));
            }
            match sets.iter_mut().find(|(s, _)| *s == (*r1, *r2)) {
                Some((_, edges)) => edges.extend(names),
                None => sets.push(((*r1, *r2), names)),
            }
        }
        let fillet = o.type_name == "Part::Fillet";
        if fillet {
            return self.part_fillet_candidates(&base, body, &list);
        }
        let make = |flip: bool| {
            let out: Vec<Value> = sets
                .iter()
                .map(|((r1, r2), edges)| {
                    let size = if (r1 - r2).abs() <= 1e-12 {
                        json!({"type": "equal_distance", "distance": r1})
                    } else {
                        json!({"type": "two_distances", "distance1": r1, "distance2": r2})
                    };
                    json!({"edges": edges, "size": size, "flip": flip})
                })
                .collect();
            Candidate::new(json!({"type": "chamfer", "body": body.to_string(), "sets": out}))
        };
        let unequal = sets.iter().any(|((a, b), _)| (a - b).abs() > 1e-12);
        Ok(if unequal {
            vec![make(false), make(true)]
        } else {
            vec![make(false)]
        })
    }

    /// Part::Fillet: the edges of one radius as a set of constant radius;
    /// each edge of two radii (FreeCAD's `BRepFilletAPI_MakeFillet::Add(R1,
    /// R2, E)`: R1 at the start of the edge's curve, R2 at its end, along
    /// its rounding) as a set of a variable radius of its own, from the
    /// first radius at the Mitcad vertex where FreeCAD's edge starts. OCCT
    /// takes two radii closer than its confusion (1e-7 mm) as their mean.
    fn part_fillet_candidates(
        &mut self,
        base: &str,
        body: BodyUid,
        list: &[(i64, f64, f64)],
    ) -> Result<Vec<Candidate>, String> {
        let component = self
            .doc
            .body_component(body)
            .ok_or("the base's body is gone")?;
        let mut constant: Vec<(f64, Vec<String>)> = Vec::new();
        let mut variable: Vec<VariableEdge> = Vec::new();
        for (edge, r1, r2) in list {
            let (_, names) = self.element_names(base, &format!("Edge{edge}"))?;
            if (r1 - r2).abs() < VARIABLE_RADIUS {
                let r = (r1 + r2) / 2.0;
                match constant
                    .iter_mut()
                    .find(|(s, _)| (s - r).abs() < VARIABLE_RADIUS)
                {
                    Some((_, edges)) => edges.extend(names),
                    None => constant.push((r, names)),
                }
            } else {
                let start = self.edge_start_vertex(base, *edge, component);
                variable.push(VariableEdge {
                    names,
                    first: *r1,
                    second: *r2,
                    start,
                });
            }
        }
        let make = |swap_unplaced: bool| {
            let mut sets: Vec<Value> = constant
                .iter()
                .map(
                    |(r, edges)| json!({"edges": edges, "size": {"type": "constant", "radius": r}}),
                )
                .collect();
            for v in &variable {
                let (from, to) = if swap_unplaced && v.start.is_none() {
                    (v.second, v.first)
                } else {
                    (v.first, v.second)
                };
                let mut size = json!({"type": "variable", "start": from, "end": to});
                if let Some(vertex) = &v.start {
                    size["start_vertex"] = json!(vertex);
                }
                sets.push(json!({"edges": v.names, "size": size}));
            }
            let candidate =
                Candidate::new(json!({"type": "fillet", "body": body.to_string(), "sets": sets}));
            match (variable.is_empty(), swap_unplaced) {
                (true, _) => candidate,
                (false, false) => candidate.note(
                    "each edge of two radii a variable radius of its own, from the first at \
                     the start of FreeCAD's edge",
                ),
                (false, true) => candidate.note(
                    "each edge of two radii a variable radius of its own, reversed on edges \
                     without a start vertex",
                ),
            }
        };
        let mut out = vec![make(false)];
        // An edge whose start has no vertex here (a closed edge, or one
        // Mitcad does not name): its radii either way along.
        if variable.iter().any(|v| v.start.is_none()) {
            out.push(make(true));
        }
        Ok(out)
    }

    /// The Mitcad name of the vertex where an edge (`Edge<n>`) of an
    /// object's stored shape starts: the first point of its curve, which
    /// is where OCCT's rounding of the edge alone starts. None for a closed
    /// edge, or where the vertex is not found in the object's body here.
    fn edge_start_vertex(
        &mut self,
        object: &str,
        edge: i64,
        component: ComponentUid,
    ) -> Option<String> {
        use mitcad_model::ElementKind;
        let index = usize::try_from(edge).ok()?.checked_sub(1)?;
        let curves = self.stored_element(object, ElementKind::Edge, index, component)?;
        let (start, end) = super::elements::curve_ends(curves.first()?)?;
        let size = 1.0 + norm(start).max(norm(end));
        let close =
            |a: [f64; 3], b: [f64; 3]| norm(sub(a, b)) <= super::elements::SAME_GEOMETRY * size;
        if close(start, end) {
            return None;
        }
        for k in 0..100_000usize {
            let point = self.stored_element(object, ElementKind::Vertex, k, component)?;
            if let Some(mitcad_model::Curve3::Point(p)) = point.first()
                && close(*p, start)
            {
                return self
                    .element_name(object, &format!("Vertex{}", k + 1))
                    .ok()
                    .map(|(_, name)| name);
            }
        }
        None
    }

    /// Part::Mirroring: the mirror image, the source's body removed.
    fn mirroring_candidates(
        &mut self,
        o: &Object,
        target: &Target,
        source: BodyUid,
    ) -> Result<Vec<Candidate>, String> {
        let plane = match o.link("MirrorPlane") {
            Some(link) => {
                let link = link.clone();
                self.plane_ref(&link, target.place.component)?
            }
            None => {
                let vector = |name: &str| match o.value(name) {
                    Some(FcValue::Vector(v)) => Some(*v),
                    _ => None,
                };
                let t = &target.place.transform;
                let base = t.apply_point(vector("Base").unwrap_or([0.0; 3]));
                let normal = t.apply_vector(vector("Normal").ok_or("no normal")?);
                json!({"origin": base, "normal": normal})
            }
        };
        let mirror = json!({"type": "mirror", "objects": {"type": "bodies", "bodies": [source.to_string()]},
                            "plane": plane});
        let remove = json!({"type": "base", "bodies": [], "replaces": [source.to_string()]});
        Ok(vec![
            Candidate::several(vec![(mirror, ""), (remove, " (source removed)")])
                .note("the mirrored body's source removed by a base feature without bodies"),
        ])
    }

    /// Part::Sweep (mitcad#543): its sketches swept along the spine's
    /// sketch edges, a new body: one section → `sweep`, several → a
    /// `loft` along the spine as its centre line; `Frenet` → the Frenet
    /// `frame` (also the plain reading: along a planar spine they agree but
    /// where its bend changes side), else at right angles to the spine
    /// (FreeCAD's corrected Frenet frame); `Solid` off → a surface. Spines
    /// and sections of other objects (mitcad#547): a Part helix's fixed
    /// helix, a body's edges, other objects' edges and vertices as fixed
    /// geometry ([`super::sweep_inputs`]).
    fn part_sweep_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        // Spines and sections that are not sketches (mitcad#547): their
        // fixed planes and sketches first.
        let mut made = super::sweep_inputs::Made::default();
        let spine = o.link("Spine").ok_or("no spine")?.clone();
        let path = self.sweep_spine(&spine, target, &mut made)?;
        let mut sections = Vec::new();
        for (i, link) in o.links("Sections").to_vec().iter().enumerate() {
            sections.push(self.sweep_section(link, target, i + 1, &mut made)?);
        }
        let solid = o.bool("Solid").unwrap_or(false);
        // Open wires and wires in no plane (mitcad#548): FreeCAD makes no
        // caps of them, a surface; one is a surface sweep of its curves.
        let open = |s: &Value| s["type"] == "curves" || s["type"] == "fixed_curves";
        let mut surface = !solid;
        let mut def = match sections.as_slice() {
            [] => return Err("no sections".to_owned()),
            [section] if section["type"] == "point" => {
                return Err("a sweep of one point".to_owned());
            }
            [section] if section["type"] == "curves" => {
                surface = true;
                json!({"type": "sweep",
                    "curves": {"sketch": section["sketch"], "curves": section["curves"]},
                    "path": path, "operation": "new_body"})
            }
            [section] if section["type"] == "fixed_curves" => {
                surface = true;
                json!({"type": "sweep", "fixed_curves": section["fixed_curves"],
                    "path": path, "operation": "new_body"})
            }
            several if several.iter().any(open) => {
                return Err(
                    "a Part sweep through several sections of open wires or wires in no plane is \
                     not translated: Mitcad's loft of curve sections takes no center line"
                        .to_owned(),
                );
            }
            [section] if section["type"] == "face" => json!({"type": "sweep",
                "profile_face": {"body": section["body"], "face": section["face"]},
                "path": path, "operation": "new_body"}),
            [section] => json!({"type": "sweep",
                "profiles": [{"sketch": section["sketch"], "region": section["region"]}],
                "path": path, "operation": "new_body"}),
            _ => json!({"type": "loft", "sections": sections, "centerline": path,
                        "ruled": false, "closed": false, "operation": "new_body"}),
        };
        if surface {
            def["surface"] = json!(true);
        }
        let candidate = |def: Value| {
            let mut defs = made.defs.clone();
            defs.push((def, std::borrow::Cow::Borrowed("")));
            let mut c = Candidate::several_named(defs);
            for note in &made.notes {
                c = c.partial(note.clone());
            }
            c
        };
        let mut out = Vec::new();
        // (A surface sweep of curves takes no frame: the plain reading.)
        let curves = def.get("curves").is_some() || def.get("fixed_curves").is_some();
        if o.bool("Frenet") == Some(true) && !curves {
            let mut framed = def.clone();
            framed["frame"] = json!({"type": "frenet"});
            out.push(candidate(framed));
        }
        out.push(candidate(def));
        Ok(out)
    }
}

/// Which way a Part extrusion goes against its profile's normal.
#[derive(Debug, Clone, Copy)]
enum Along {
    /// Along the normal; `true`: against it.
    Normal(bool),
    /// Along FreeCAD's direction, off the normal; `true`: against it
    /// (Reversed).
    Oblique(bool),
}

/// The name suffixes of an extrusion's features after the first face's.
const FACE_SUFFIXES: [&str; 9] = [
    "",
    " (face 2)",
    " (face 3)",
    " (face 4)",
    " (face 5)",
    " (face 6)",
    " (face 7)",
    " (face 8)",
    " (face 9)",
];

/// The name suffixes of a Part::Face's patches after the first sketch's.
const SKETCH_SUFFIXES: [&str; 5] = [
    "",
    " (sketch 2)",
    " (sketch 3)",
    " (sketch 4)",
    " (sketch 5)",
];

/// The extent of a Part extrusion by FreeCAD's lengths (its
/// `computeFinalParameters`): symmetric (the forward length in all, half
/// each way), two sides (forward along the direction, reverse against
/// it), or one side; with the tapers (forward, reverse) times the sign.
/// True when the one side goes against the direction (a reverse length
/// alone).
fn part_extent(
    forward: &Q,
    reverse: &Q,
    symmetric: bool,
    tapers: Option<(&Q, &Q, f64)>,
) -> (Value, bool) {
    let with_taper = |mut v: Value, taper: Option<Q>| {
        if let Some(t) = taper
            && t.value != 0.0
        {
            v["taper"] = t.json();
        }
        v
    };
    let taper = tapers.map(|(t, _, sign)| t.signed(sign));
    let taper_rev = tapers.map(|(_, t, sign)| t.signed(sign));
    let distance = |q: &Q| json!({"type": "distance", "distance": q.json()});
    if symmetric {
        (
            with_taper(
                json!({"type": "symmetric", "distance": forward.json(), "full_length": true}),
                taper,
            ),
            false,
        )
    } else if reverse.value != 0.0 && forward.value != 0.0 {
        (
            json!({"type": "two_sides",
                   "side1": with_taper(distance(forward), taper),
                   "side2": with_taper(distance(reverse), taper_rev)}),
            false,
        )
    } else if reverse.value != 0.0 {
        (with_taper(distance(reverse), taper_rev), true)
    } else {
        (with_taper(distance(forward), taper), false)
    }
}

/// The most edges of an object an extrusion of its edges reads.
const MAX_EDGES: usize = 10_000;

/// Whether the curves lie in the frame's plane (their points within 1e-6
/// of their size, conics in planes parallel to it).
pub(super) fn lies_in(curves: &[Curve3], frame: &SketchFrame) -> bool {
    let Some(n) = unit(frame.normal()) else {
        return false;
    };
    let points: Vec<[f64; 3]> = curves
        .iter()
        .flat_map(super::elements::key_points)
        .collect();
    let size = points
        .iter()
        .map(|p| norm(sub(*p, frame.origin)))
        .fold(1.0, f64::max);
    let flat = points
        .iter()
        .all(|p| dot(sub(*p, frame.origin), n).abs() <= 1e-6 * size);
    flat && curves.iter().all(|c| match c {
        Curve3::Conic { normal, .. } => unit(*normal).is_some_and(|m| norm(cross(m, n)) <= 1e-9),
        // Never in a plane (its key points may be: a turn's ends above each
        // other, mitcad#548).
        Curve3::Helix { .. } => false,
        _ => true,
    })
}

/// The name suffixes of the walls of edges in several planes after the
/// first (mitcad#547).
const WALL_SUFFIXES: [&str; 9] = [
    "",
    " (wall 2)",
    " (wall 3)",
    " (wall 4)",
    " (wall 5)",
    " (wall 6)",
    " (wall 7)",
    " (wall 8)",
    " (wall 9)",
];

/// The name suffix of the sweeps of curves in no plane (mitcad#548).
const CURVE_SUFFIX: &str = " (curve)";

/// The name suffix of the faces of the simple face maker's nested wires
/// after the first (mitcad#548).
const WIRE_SUFFIX: &str = " (wire)";

/// A curve that lies in no plane (mitcad#548) as a fixed path, with its
/// start point: a helix as its `helix` path, a spline whose poles leave
/// every plane as a `fixed_curves` path of it (shared with Part sweeps,
/// [`super::fixed_paths::fixed_curves`]); None for a curve in a plane.
fn fixed_path(c: &Curve3) -> Option<Result<(Value, [f64; 3]), String>> {
    match c {
        Curve3::Helix { .. } => {
            Some(helix_path(c).ok_or_else(|| "a helix without its axes".to_owned()))
        }
        Curve3::BSpline { poles, .. } => {
            if plane_of(poles).is_some_and(|f| lies_in(std::slice::from_ref(c), &f)) {
                return None;
            }
            let Some(start) = super::fixed_paths::spline_start(c) else {
                return Some(Err(
                    "a spline in no plane that does not start at its first pole is not \
                     translated (the line swept along it starts there)"
                        .to_owned(),
                ));
            };
            Some(super::fixed_paths::fixed_curves(std::slice::from_ref(c)).map(|p| (p, start)))
        }
        _ => None,
    }
}

/// A straight line from `start` along `d` (from `ends.0` to `ends.1`
/// along it) in a fixed sketch, swept along a fixed `path` without turning
/// (mitcad#548): the prism of the path's curve along `d`. The plane and
/// the sketch are the candidate's definitions `first` and `first + 1`;
/// `main`: the sweep is the candidate's main definition.
fn line_sweep(
    start: [f64; 3],
    d: [f64; 3],
    ends: (f64, f64),
    path: &Value,
    first: usize,
    main: bool,
) -> Result<Vec<(Value, &'static str)>, String> {
    let other = if d[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let y = unit(cross(cross(d, other), d)).ok_or("no plane for the line")?;
    let plane = json!({"type": "construction_plane", "definition": {"type": "fixed",
        "origin": start, "x_axis": d, "y_axis": y}});
    let sketch = json!({"type": "sketch", "plane": format!("${first}"), "entities": [
        {"id": "p1", "type": "point", "at": [ends.0, 0.0], "fixed": true},
        {"id": "p2", "type": "point", "at": [ends.1, 0.0], "fixed": true},
        {"id": "c3", "type": "line", "start": "p1", "end": "p2"}]});
    let sweep = json!({"type": "sweep", "surface": true,
        "curves": {"sketch": format!("${}", first + 1), "curves": ["c3"]}, "path": path,
        "orientation": "parallel", "operation": "new_body"});
    Ok(vec![
        (plane, " (plane)"),
        (sketch, " (line)"),
        (sweep, if main { "" } else { CURVE_SUFFIX }),
    ])
}

/// A plane through points (a spline's poles): through the first, the
/// first point away from it and the first one off their line.
fn plane_of(points: &[[f64; 3]]) -> Option<SketchFrame> {
    let origin = *points.first()?;
    let size = points
        .iter()
        .map(|p| norm(sub(*p, origin)))
        .fold(0.0, f64::max);
    if size <= 0.0 {
        return None;
    }
    let x = points.iter().find_map(|p| {
        let v = sub(*p, origin);
        if norm(v) > 1e-6 * size { unit(v) } else { None }
    })?;
    let n = points.iter().find_map(|p| {
        let c = cross(x, sub(*p, origin));
        if norm(c) > 1e-6 * size { unit(c) } else { None }
    })?;
    Some(SketchFrame {
        origin,
        x_axis: x,
        y_axis: cross(n, x),
    })
}

/// A model helix as a fixed `helix` path, and its start point.
fn helix_path(c: &Curve3) -> Option<(Value, [f64; 3])> {
    let Curve3::Helix {
        center,
        axis,
        x_axis,
        radius,
        pitch,
        start,
        end,
    } = c
    else {
        return None;
    };
    let z = unit(*axis)?;
    let x = unit(*x_axis)?;
    let y = cross(z, x);
    let at: [f64; 3] = std::array::from_fn(|i| {
        center[i]
            + radius * (start.cos() * x[i] + start.sin() * y[i])
            + pitch * start / std::f64::consts::TAU * z[i]
    });
    let path = json!({"helix": {"center": center, "axis": z, "x_axis": x, "radius": radius,
                      "pitch": pitch, "start_angle": start, "end_angle": end}});
    Some((path, at))
}

/// The plane through the curves' first straight edge and a direction (a
/// line's extrusion), when they span one.
fn spanned(curves: &[Curve3], dir: [f64; 3]) -> Option<SketchFrame> {
    let (start, end) = curves.iter().find_map(|c| match c {
        Curve3::Line { start, end } => Some((*start, *end)),
        _ => None,
    })?;
    let x = unit(sub(end, start))?;
    let n = unit(cross(x, dir))?;
    Some(SketchFrame {
        origin: start,
        x_axis: x,
        y_axis: cross(n, x),
    })
}

/// The definition's fields of `what` added to `def`.
fn merge(def: &mut Value, what: &Value) {
    if let (Some(def), Some(what)) = (def.as_object_mut(), what.as_object()) {
        for (key, value) in what {
            def.insert(key.clone(), value.clone());
        }
    }
}

/// A profile's regions as an extrusion's `profiles`.
fn sketch_profiles(profile: &super::features::Profile) -> Vec<Value> {
    profile
        .regions
        .iter()
        .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
        .collect()
}

/// Two radii of a Part fillet's edge closer than this (mm, OCCT's
/// `Precision::Confusion`) are one constant radius, their mean.
const VARIABLE_RADIUS: f64 = 1e-7;

/// An edge of a Part fillet with two radii.
struct VariableEdge {
    /// Its Mitcad names (the pieces of an edge FreeCAD merged).
    names: Vec<String>,
    /// The radius at the start of FreeCAD's edge, and at its end.
    first: f64,
    second: f64,
    /// The Mitcad vertex where FreeCAD's edge starts.
    start: Option<String>,
}

/// A combine of the target with the tools by the object's boolean.
fn combine(o: &Object, target: BodyUid, tools: &[BodyUid]) -> Candidate {
    let operation = match o.type_name.as_str() {
        "Part::Cut" => "cut",
        "Part::Common" | "Part::MultiCommon" => "intersect",
        _ => "join",
    };
    Candidate::new(json!({"type": "combine", "target": target.to_string(),
        "tools": tools.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "operation": operation, "keep_tools": false}))
}
