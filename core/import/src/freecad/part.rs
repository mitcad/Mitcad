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
//!   off the normal → `sweep` along a fixed line, without turning.
//! - Revolution of a sketch (solid) → `revolve` about its axis (a fixed
//!   axis through its base point).
//! - Cut, Fuse, MultiFuse, Common, MultiCommon → `combine` (the first
//!   operand the target, the others the tools).
//! - Fillet, Chamfer → `fillet`, `chamfer` on the operand's edges
//!   (FreeCAD's edge list: number, first and second size; constant sizes).
//! - Mirroring → `mirror` of the operand's body, then a base feature that
//!   removes the operand's body (FreeCAD's mirroring consumes it).
//!
//! Other objects of the tree (plain shapes, compounds, objects Mitcad does
//! not build) become base features of their stored shapes, which the
//! objects using them continue on.

use mitcad_freecad::{Object, Value as FcValue};
use mitcad_model::{BodyUid, ComponentUid, Kernel};
use serde_json::{Value, json};

use super::history::{Candidate, Target};
use super::params::{Kind, Q};
use super::{Importer, NOT_CONSUMING};
use crate::geom::{add, cross, dot, norm, scale, sub, unit};

impl<K: Kernel> Importer<'_, '_, K> {
    /// A Part workbench object of a replayed result.
    pub(super) fn part_item(&mut self, o: &Object) {
        let Some(place) = self.place_of(&o.name, false) else {
            self.feature_skipped(o, None, "its container was not imported");
            return;
        };
        // The operands' bodies, a copy where another object uses one too.
        let mut operands: Vec<(String, BodyUid)> = Vec::new();
        let mut problems = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let consumed: Vec<String> = o
            .references()
            .filter(|(p, l)| !NOT_CONSUMING.contains(p) && l.file.is_none())
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
            "Part::Extrusion" => (self.extrusion_candidates(o, target), "extrude"),
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
            other => (Err(format!("{other} is not translated yet")), ""),
        }
    }

    /// An extrusion of a sketch.
    fn extrusion_candidates(
        &mut self,
        o: &Object,
        target: &Target,
    ) -> Result<Vec<Candidate>, String> {
        if o.bool("Solid") != Some(true) {
            return Err("an extrusion into faces (Solid off) is not translated".to_owned());
        }
        let profile = self.profile(o, "Base", target.place.component)?;
        let normal = profile.sketch.frame.normal();
        let mode = self
            .enum_text(o, "DirMode")
            .unwrap_or_else(|| "Normal".to_owned());
        let mut flip = o.bool("Reversed").unwrap_or(false);
        let dir = match o.value("Dir") {
            Some(FcValue::Vector(v)) => target.place.transform.apply_vector(*v),
            _ => normal,
        };
        match mode.as_str() {
            "Normal" => {}
            // A direction of its own or an edge's (`Dir` holds it).
            "Custom" | "Edge" => {
                if norm(cross(dir, normal)) > 1e-9 * norm(dir) {
                    return self.oblique_extrusion(o, &profile, dir);
                }
                flip ^= dot(dir, normal) < 0.0;
            }
            other => return Err(format!("an extrusion along {other} is not translated yet")),
        }
        let mut forward = self.prop(o, "LengthFwd", Kind::Length, 0.0);
        if forward.value == 0.0 {
            forward = Q::number(norm(dir), Kind::Length);
        }
        let reverse = self.prop(o, "LengthRev", Kind::Length, 0.0);
        let taper = self.prop(o, "TaperAngle", Kind::Angle, 0.0);
        let taper_rev = self.prop(o, "TaperAngleRev", Kind::Angle, 0.0);
        let with_taper = |mut v: Value, t: Q| {
            if t.value != 0.0 {
                v["taper"] = t.json();
            }
            v
        };
        let (forward, reverse_value, reverse) = (forward.json(), reverse.value, reverse.json());
        let extent = |sign: f64| {
            if o.bool("Symmetric") == Some(true) {
                with_taper(
                    json!({"type": "symmetric", "distance": forward, "full_length": true}),
                    taper.signed(sign),
                )
            } else if reverse_value != 0.0 {
                json!({"type": "two_sides",
                       "side1": with_taper(json!({"type": "distance", "distance": forward}), taper.signed(sign)),
                       "side2": with_taper(json!({"type": "distance", "distance": reverse}), taper_rev.signed(sign))})
            } else {
                with_taper(
                    json!({"type": "distance", "distance": forward}),
                    taper.signed(sign),
                )
            }
        };
        let profiles: Vec<Value> = profile
            .regions
            .iter()
            .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
            .collect();
        let mut variants = vec![(flip, 1.0)];
        if taper.value != 0.0 || taper_rev.value != 0.0 {
            variants.push((flip, -1.0));
        }
        variants.push((!flip, 1.0));
        Ok(variants
            .into_iter()
            .map(|(flip, sign)| {
                Candidate::new(
                    json!({"type": "extrude", "profiles": profiles, "extent": extent(sign),
                                      "flip": flip, "operation": "new_body"}),
                )
            })
            .collect())
    }

    /// An extrusion along a direction off the sketch's normal: the profile
    /// swept along a fixed line through the sketch's origin without
    /// turning, the lengths along the direction (forward and reverse, or
    /// the forward length symmetric about the sketch); no taper.
    fn oblique_extrusion(
        &mut self,
        o: &Object,
        profile: &super::features::Profile,
        dir: [f64; 3],
    ) -> Result<Vec<Candidate>, String> {
        let d = unit(dir).ok_or("no direction")?;
        for taper in ["TaperAngle", "TaperAngleRev"] {
            if o.f64(taper).is_some_and(|t| t != 0.0) {
                return Err("a taper along a custom direction is not translated".to_owned());
            }
        }
        let why = "a sweep along a fixed line: the direction's length is FreeCAD's value";
        let mut forward = self.prop(o, "LengthFwd", Kind::Length, 0.0).value;
        self.left_out(o, "LengthFwd", why);
        if forward == 0.0 {
            forward = norm(dir);
        }
        let mut reverse = self.prop(o, "LengthRev", Kind::Length, 0.0).value;
        self.left_out(o, "LengthRev", why);
        if o.bool("Symmetric") == Some(true) {
            forward /= 2.0;
            reverse = forward;
        }
        let sign = if o.bool("Reversed") == Some(true) {
            -1.0
        } else {
            1.0
        };
        let origin = profile.sketch.frame.origin;
        let profiles: Vec<Value> = profile
            .regions
            .iter()
            .map(|r| json!({"sketch": profile.sketch.uid.to_string(), "region": r}))
            .collect();
        Ok([sign, -sign]
            .into_iter()
            .map(|s| {
                let start = sub(origin, scale(d, s * reverse));
                let end = add(origin, scale(d, s * forward));
                Candidate::new(json!({"type": "sweep", "profiles": profiles,
                    "path": {"start": start, "end": end}, "orientation": "parallel",
                    "operation": "new_body"}))
                .note("along FreeCAD's direction: a sweep along a fixed line")
            })
            .collect())
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
            let mut out = Vec::new();
            for ((r1, r2), edges) in &sets {
                if (r1 - r2).abs() > 1e-12 {
                    return Err("a fillet of varying radius is not translated yet".to_owned());
                }
                out.push(json!({"edges": edges, "size": {"type": "constant", "radius": r1}}));
            }
            return Ok(vec![Candidate::new(
                json!({"type": "fillet", "body": body.to_string(), "sets": out}),
            )]);
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
