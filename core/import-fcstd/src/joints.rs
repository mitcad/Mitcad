// SPDX-License-Identifier: MIT
//! FreeCAD's assembly joints (the Assembly workbench, 1.0 and later) as
//! MitCad joints (mitcad#1557, mitcad#1560).
//!
//! A FreeCAD joint connects two connectors: `Reference1` and `Reference2`
//! name an object of the assembly (a link, a part) and a path into it to the
//! element picked (`Body.Face24`, `Part__Feature027.Edge18`), and
//! `Placement1` and `Placement2` hold each connector's frame in the
//! coordinates of the object that path ends in (FreeCAD computes it from
//! the element; the attachment offset is in it). In the assembly a
//! connector is that object's placement, the placements along the path and
//! the connector's frame.
//!
//! - Fixed, Revolute, Cylindrical, Slider and Ball joints become MitCad's
//!   `joint` of the kind with the same free motions (rigid, revolute,
//!   cylindrical, slider along z, ball), in the assembly's component.
//!   Each side is the occurrence path of the referenced object (the
//!   occurrences the import made for it and for the parts and Bodies of
//!   placements of their own along the path).
//! - A side's origin is the element FreeCAD picked, where it is an element
//!   of a body the import made in the component the path ends in and
//!   MitCad's joints take a frame from it (a planar or round face, a
//!   straight or circular edge, a vertex; mitcad#1560): that face, edge or
//!   vertex, its frame shifted along its own axes to FreeCAD's connector
//!   (`frame_override.shift`), so that it follows the element when the
//!   part changes. Where the element's z axis is not along the connector's
//!   (a vertex's), the override gives FreeCAD's axes as well and the origin
//!   still follows the element. Otherwise (an element of a sketch, of a
//!   part's compound, of a body not found) the origin is the component's
//!   origin with FreeCAD's connector as its frame override, fixed in the
//!   part, and the report says why.
//! - The two frames as the file places the parts give the joint's flip
//!   (the z axes opposed: FreeCAD's solver keeps either way), and what the
//!   kind holds fixed: a rigid joint's turn about z and offset along it, a
//!   revolute joint's offset, a slider's turn. So the joint holds where
//!   FreeCAD left the parts, and no occurrence moves; a joint that would
//!   move one is not added (its reason in the report). Where the linked
//!   origins do not hold so, FreeCAD's frames fixed are tried.
//! - Limits (`EnableLengthMin` … with `LengthMin` …, mitcad#1560): the
//!   length limits bound a slider's and a cylindrical joint's slide (`tz`),
//!   the angle limits a revolute and a cylindrical joint's turn (`rz`),
//!   with their expressions. They are taken as they are where they hold
//!   the value where the file places the parts, else turned round (the
//!   other sense), else left out with the reason; so are limits of a kind
//!   without that motion.
//! - Grounded joints ground their objects' occurrences (stage 1); the
//!   other kinds (Distance, Angle, Parallel, Perpendicular, gears, belts,
//!   rack and pinion, screw) have no MitCad joint kind and are skipped with
//!   that reason.

use std::collections::BTreeMap;

use mitcad_freecad::{Object, Value as FcValue};
use mitcad_import_base::geom::{cross, norm, sub};
use mitcad_model::features::{FrameOverride, GeomRef};
use mitcad_model::{ComponentUid, ElementKind, FeatureUid, Kernel, OccurrenceUid, Transform};
use serde_json::{Map, Value, json};

use super::elements::parse_element;
use super::params::{Kind, Q};
use super::report::ObjectOutcome;
use super::{Importer, Key, Role, placement_of, transform};

/// Distances within this meet, mm.
const MEET: f64 = 1e-6;
/// Directions within this agree (a sine).
const ALIGNED: f64 = 1e-7;

/// One connector of a joint: the occurrence path in the joint's component,
/// the connector's frame in the component that path ends in, and the
/// element FreeCAD picked (its object and name: `Body015`, `Face24`).
struct Side {
    path: Vec<OccurrenceUid>,
    frame: Transform,
    element: Option<(String, String)>,
}

/// A connector as a joint origin: its definition, its frame in the
/// component its path ends in, and the element it follows (or why it is
/// FreeCAD's frame, fixed in the part).
struct Origin {
    value: Value,
    path: Vec<OccurrenceUid>,
    frame: Transform,
    on: Result<String, String>,
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// Every assembly joint of the document (not the grounded ones): a
    /// MitCad joint, or skipped with the reason.
    pub(super) fn import_joints(&mut self) {
        let joints: Vec<Object> = self.sources[0]
            .file
            .document
            .objects
            .iter()
            .filter(|o| o.property("JointType").is_some() && o.property("Reference1").is_some())
            .cloned()
            .collect();
        for joint in joints {
            let outcome = self.joint(&joint);
            let (state, note) = match outcome {
                Ok((made, notes, partial)) => (
                    if partial {
                        ObjectOutcome::Partial
                    } else {
                        ObjectOutcome::Parametric
                    },
                    format!("a {made}: {}", notes.join("; ")),
                ),
                Err(why) => (
                    ObjectOutcome::Skipped,
                    format!("an assembly joint, not imported: {why}"),
                ),
            };
            self.roles.insert(joint.name.clone(), (state, Some(note)));
        }
    }

    /// One joint: what it became, its notes, and whether something of it
    /// is not carried (a connector fixed, limits left out); or why not.
    fn joint(&mut self, o: &Object) -> Result<(String, Vec<String>, bool), String> {
        let kind_text = self
            .enum_text(o, "JointType")
            .ok_or("its type is not known")?;
        let kind = match kind_text.as_str() {
            "Fixed" => "rigid",
            "Revolute" => "revolute",
            "Cylindrical" => "cylindrical",
            "Slider" => "slider",
            "Ball" => "ball",
            other => {
                return Err(format!(
                    "a {other} joint: MitCad's joints have no such kind (rigid, revolute, \
                     cylindrical, slider, pin and slot, planar, ball)"
                ));
            }
        };
        if o.bool("Suppressed") == Some(true) {
            return Err("suppressed in FreeCAD".to_owned());
        }
        let assembly = self.joint_assembly(o)?;
        let a = self.joint_side(o, "1", assembly)?;
        let b = self.joint_side(o, "2", assembly)?;
        if a.path.is_empty() || b.path.is_empty() {
            return Err("a connector is on the assembly's own geometry".to_owned());
        }
        if a.path[0] == b.path[0] {
            return Err("both connectors are in the same part".to_owned());
        }
        // On the elements picked where they can be, else FreeCAD's frames
        // fixed in the parts.
        let linked = [self.linked_origin(&a), self.linked_origin(&b)];
        let mut fixed = [fixed_origin(&a), fixed_origin(&b)];
        let mut extra = Vec::new();
        let sides = if linked.iter().any(|s| s.on.is_ok()) {
            match self.place_joint(o, kind, assembly, &linked) {
                Ok(done) => Some((done, linked)),
                Err(why) => {
                    for (side, l) in fixed.iter_mut().zip(&linked) {
                        if l.on.is_ok() {
                            side.on = Err(format!(
                                "on its element the joint does not hold where the file places \
                                 the parts: {why}"
                            ));
                        } else {
                            side.on = l.on.clone();
                        }
                    }
                    None
                }
            }
        } else {
            for (side, l) in fixed.iter_mut().zip(&linked) {
                side.on = l.on.clone();
            }
            None
        };
        let ((uid, notes), origins) = match sides {
            Some(done) => done,
            None => (self.place_joint(o, kind, assembly, &fixed)?, fixed),
        };
        let mut partial = !notes.is_empty();
        for (k, side) in origins.iter().enumerate() {
            match &side.on {
                Ok(on) => extra.push(format!("connector {} on {on}, following it", k + 1)),
                Err(why) => {
                    partial = true;
                    extra.push(format!(
                        "connector {} is FreeCAD's frame, fixed in the part ({why})",
                        k + 1
                    ));
                }
            }
        }
        extra.extend(notes);
        Ok((format!("{kind} joint {uid}"), extra, partial))
    }

    /// The joint of two origins, added where it holds as the file places
    /// the parts (with FreeCAD's limits where they hold too): its uid and
    /// notes, or why not.
    fn place_joint(
        &mut self,
        o: &Object,
        kind: &str,
        assembly: ComponentUid,
        sides: &[Origin; 2],
    ) -> Result<(FeatureUid, Vec<String>), String> {
        let [a, b] = sides;
        // The frames in the assembly as the file places the parts.
        let fa = self.doc.path_transform(&a.path).after(&a.frame);
        let fb = self.doc.path_transform(&b.path).after(&b.frame);
        let za = column(&fa, 2);
        let zb = column(&fb, 2);
        let flip = dot(za, zb) < 0.0;
        // b⁻¹·a, with a turned back over its x axis when flipped: what the
        // kind leaves is a turn about z and a shift along it.
        let mut relative = mitcad_model::assembly::inverse(&fb).after(&fa);
        if flip {
            relative = relative.after(&half_turn_x());
        }
        let z = column(&relative, 2);
        if (z[0].powi(2) + z[1].powi(2)).sqrt() > ALIGNED || z[2] < 0.0 {
            return Err(format!(
                "its connectors' z axes are not along each other where the file places the \
                 parts ({:.1e} apart)",
                (z[0].powi(2) + z[1].powi(2)).sqrt()
            ));
        }
        let t = relative.translation;
        let off_axis = (t[0].powi(2) + t[1].powi(2)).sqrt();
        if off_axis > MEET {
            return Err(format!(
                "its connectors are {off_axis:.3e} mm apart across their z axis where the file \
                 places the parts"
            ));
        }
        let turn = relative.linear[1][0].atan2(relative.linear[0][0]);
        let along = t[2];
        let mut def = json!({"type": "joint", "kind": kind,
            "a": a.value, "b": b.value});
        if flip {
            def["flip"] = json!(true);
        }
        // What the kind holds: a turn and a shift it does not let move.
        let (holds_turn, holds_shift) = match kind {
            "rigid" => (true, true),
            "revolute" => (false, true),
            "slider" => (true, false),
            "ball" => (false, true),
            _ => (false, false),
        };
        if kind == "ball" && along.abs() > MEET {
            return Err(format!(
                "its connectors are {along:.3e} mm apart where the file places the parts"
            ));
        }
        if holds_turn && turn.abs() > ALIGNED {
            def["angle"] = json!(format!("{} deg", turn.to_degrees()));
        }
        if holds_shift && kind != "ball" && along.abs() > MEET {
            def["offset"] = json!(format!("{along} mm"));
        }
        let mut notes = Vec::new();
        let label = o.label().to_owned();
        let limits = self.joint_limits(o, kind, turn, along, &mut notes);
        if !limits.is_empty() {
            let mut limited = def.clone();
            limited["limits"] = Value::Object(limits);
            match self.add_unmoved(&limited, &label, assembly) {
                Ok(uid) => return Ok((uid, notes)),
                Err(why) => {
                    let why =
                        format!("its limits are left out (the joint does not take them: {why})");
                    for p in ["LengthMin", "LengthMax", "AngleMin", "AngleMax"] {
                        self.left_out(o, p, &why);
                    }
                    notes.push(why);
                }
            }
        }
        let uid = self.add_unmoved(&def, &label, assembly)?;
        Ok((uid, notes))
    }

    /// A joint added where nothing moves: the occurrences as the file
    /// places them; Err (and nothing added) otherwise.
    fn add_unmoved(
        &mut self,
        def: &Value,
        label: &str,
        assembly: ComponentUid,
    ) -> Result<FeatureUid, String> {
        let before = self.placements();
        let uid = self.add_named(def, label, assembly)?;
        let after = self.placements();
        let moved = before
            .iter()
            .filter_map(|(o, t)| Some((o, t, after.get(o)?)))
            .map(|(_, t, u)| gap(t, u))
            .fold(0.0, f64::max);
        if moved > MEET {
            self.doc.undo();
            return Err(format!(
                "the joint would move parts by {moved:.3e} (mm or radians) from where the file \
                 places them"
            ));
        }
        Ok(uid)
    }

    /// FreeCAD's enabled limits as MitCad's, per motion of the kind: the
    /// length limits a slide's (`tz`), the angle limits a turn's (`rz`),
    /// as they are where they hold the motion's value where the file places
    /// the parts (`turn`, `along`), else turned round, else left out with a
    /// note.
    fn joint_limits(
        &mut self,
        o: &Object,
        kind: &str,
        turn: f64,
        along: f64,
        notes: &mut Vec<String>,
    ) -> Map<String, Value> {
        let mut out = Map::new();
        let families = [
            (
                "length",
                "Length",
                Kind::Length,
                matches!(kind, "slider" | "cylindrical"),
                "tz",
                along,
            ),
            (
                "angle",
                "Angle",
                Kind::Angle,
                matches!(kind, "revolute" | "cylindrical"),
                "rz",
                turn,
            ),
        ];
        for (what, name, unit, has, motion, value) in families {
            let min_on = o.bool(&format!("Enable{name}Min")) == Some(true);
            let max_on = o.bool(&format!("Enable{name}Max")) == Some(true);
            if !min_on && !max_on {
                continue;
            }
            if !has {
                notes.push(format!(
                    "its {what} limits are left out (a {kind} joint has no such motion)"
                ));
                continue;
            }
            let min = min_on.then(|| self.prop(o, &format!("{name}Min"), unit, 0.0));
            let max = max_on.then(|| self.prop(o, &format!("{name}Max"), unit, 0.0));
            // A turn's value is one of a whole turn's apart.
            let values: Vec<f64> = if unit == Kind::Angle {
                vec![
                    value,
                    value - std::f64::consts::TAU,
                    value + std::f64::consts::TAU,
                ]
            } else {
                vec![value]
            };
            let holds = |lo: Option<f64>, hi: Option<f64>| {
                values
                    .iter()
                    .any(|v| lo.is_none_or(|l| *v >= l - MEET) && hi.is_none_or(|h| *v <= h + MEET))
            };
            let (lo, hi) = (min.as_ref().map(|q| q.value), max.as_ref().map(|q| q.value));
            let chosen = if holds(lo, hi) {
                Some((min.clone(), max.clone()))
            } else if holds(hi.map(|h| -h), lo.map(|l| -l)) {
                Some((max.as_ref().map(Q::neg), min.as_ref().map(Q::neg)))
            } else {
                None
            };
            match chosen {
                Some((lo, hi)) => {
                    let mut limit = Map::new();
                    if let Some(q) = lo {
                        limit.insert("min".to_owned(), q.json());
                    }
                    if let Some(q) = hi {
                        limit.insert("max".to_owned(), q.json());
                    }
                    out.insert(motion.to_owned(), Value::Object(limit));
                }
                None => {
                    let why = format!(
                        "its {what} limits are left out (they do not hold its {motion} of \
                         {value:.6} where the file places the parts, either way round)"
                    );
                    for p in ["Min", "Max"] {
                        self.left_out(o, &format!("{name}{p}"), &why);
                    }
                    notes.push(why);
                }
            }
        }
        out
    }

    /// The component of the assembly a joint is in (its joint group's
    /// assembly).
    fn joint_assembly(&self, o: &Object) -> Result<ComponentUid, String> {
        let source = &self.sources[0];
        let mut at = o.name.clone();
        for _ in 0..64 {
            let parent = source
                .container
                .get(&at)
                .ok_or("it is in no assembly")?
                .clone();
            let key = Key {
                doc: 0,
                name: parent.clone(),
            };
            if let Some(&c) = self.components.get(&key) {
                return Ok(c);
            }
            at = parent;
        }
        Err("it is in no assembly".to_owned())
    }

    /// A connector: `Reference<k>`'s object and path as occurrences of the
    /// assembly's component, `Placement<k>` with the placements along the
    /// path that are not occurrences, and the element picked.
    fn joint_side(&self, o: &Object, k: &str, assembly: ComponentUid) -> Result<Side, String> {
        let reference = o
            .link(&format!("Reference{k}"))
            .ok_or_else(|| format!("no Reference{k}"))?;
        if reference.file.is_some() {
            return Err(format!("Reference{k} is in another document"));
        }
        let placement = match o.value(&format!("Placement{k}")) {
            Some(FcValue::Placement(p)) => *p,
            _ => return Err(format!("no Placement{k}")),
        };
        let source = &self.sources[0];
        let element = reference
            .subs
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        let mut steps: Vec<String> = vec![reference.object.clone()];
        let parts: Vec<&str> = element.split('.').collect();
        steps.extend(
            parts[..parts.len().saturating_sub(1)]
                .iter()
                .map(|s| (*s).to_owned()),
        );
        let picked = parts
            .last()
            .filter(|e| !e.is_empty())
            .map(|e| (steps.last().cloned().unwrap_or_default(), (*e).to_owned()));
        let mut path: Vec<OccurrenceUid> = Vec::new();
        let mut at = assembly;
        let mut local = Transform::IDENTITY;
        let a = self.doc.assembly();
        for (i, step) in steps.iter().enumerate() {
            let key = Key {
                doc: 0,
                name: step.clone(),
            };
            let list = self.occurrences_of.get(&key).cloned().unwrap_or_default();
            // An occurrence in the component reached so far; the object a
            // reference starts from may also be one of a sub-assembly's
            // (FreeCAD names a nested part directly): the path down to it.
            let here: Option<Vec<OccurrenceUid>> = list
                .iter()
                .find(|u| a.occurrence(**u).is_some_and(|x| x.parent == at))
                .map(|u| vec![*u])
                .or_else(|| {
                    (i == 0)
                        .then(|| list.iter().find_map(|u| path_down(a, at, *u, 8)))
                        .flatten()
                });
            match here {
                Some(down) if local == Transform::IDENTITY => {
                    let last = *down.last().expect("a path down has an occurrence");
                    path.extend(down);
                    at = a
                        .occurrence(last)
                        .map(|x| x.component)
                        .ok_or("an occurrence is gone")?;
                }
                _ => {
                    let object = source
                        .object(step)
                        .ok_or_else(|| format!("no object {step}"))?;
                    if path.is_empty() {
                        return Err(format!(
                            "{} is not placed in the assembly",
                            source.label(step)
                        ));
                    }
                    local = local.after(&transform(&placement_of(object)));
                }
            }
        }
        Ok(Side {
            path,
            frame: local.after(&transform(&placement)),
            element: picked,
        })
    }

    /// A side's origin on the element FreeCAD picked, else FreeCAD's frame
    /// fixed in the part with the reason.
    fn linked_origin(&mut self, side: &Side) -> Origin {
        match self.element_origin(side) {
            Ok(origin) => origin,
            Err(why) => {
                let mut origin = fixed_origin(side);
                origin.on = Err(why);
                origin
            }
        }
    }

    /// A side's origin on the face, edge or vertex FreeCAD picked, as the
    /// element of the imported body: MitCad's frame of it, moved along its
    /// own axes to FreeCAD's connector (with FreeCAD's axes where the
    /// element's z axis is not along the connector's). Err with the reason
    /// where it is not one.
    fn element_origin(&mut self, side: &Side) -> Result<Origin, String> {
        let (owner, element) = side
            .element
            .clone()
            .ok_or("FreeCAD's reference names no element")?;
        let (kind, index) = parse_element(&element)
            .ok_or_else(|| format!("{element} is not a face, edge or vertex"))?;
        let owner = self.link_end(&owner)?;
        let label = self.sources[0].label(&owner);
        if self.sources[0]
            .object(&owner)
            .is_some_and(|x| super::is_sketch(&x.type_name))
        {
            return Err(format!(
                "{element} of the sketch {label}: joints follow elements of bodies, not of \
                 sketches"
            ));
        }
        // The face itself, else a face on its surface: a face a hole or a
        // pocket cut has other cut edges in MitCad's body than in FreeCAD's
        // shape, and the frame only takes the surface (and the shift).
        let (body, name) = match self.element_name(&owner, &element) {
            Ok(found) => found,
            Err(e) if kind == ElementKind::Face => {
                self.face_on_same_surface(&owner, index).map_err(|f| {
                    format!("{element} of {label} is not an element of a body here ({e}; {f})")
                })?
            }
            Err(e) => {
                return Err(format!(
                    "{element} of {label} is not an element of a body here ({e})"
                ));
            }
        };
        let at = side
            .path
            .last()
            .and_then(|u| self.doc.assembly().occurrence(*u))
            .map(|x| x.component)
            .ok_or("its occurrence is gone")?;
        if self.doc.body_component(body) != Some(at) {
            return Err(format!(
                "{element} of {label} is in another component than the part the joint names"
            ));
        }
        let field = match kind {
            ElementKind::Face => "face",
            ElementKind::Edge => "edge",
            ElementKind::Vertex => "vertex",
        };
        let geometry = json!({"body": body.to_string(), field: name});
        let parsed: GeomRef =
            serde_json::from_value(geometry.clone()).map_err(|e| format!("{name}: {e}"))?;
        let own = self
            .doc
            .joint_frame(at, "", &parsed, None)
            .map_err(|e| format!("{element} of {label} gives MitCad's joints no frame ({e})"))?
            .local;
        let wanted = side.frame;
        let zw = column(&wanted, 2);
        let aligned = norm(cross(column(&own, 2), zw)) <= ALIGNED;
        let offset = sub(wanted.translation, own.translation);
        // The shift along the axes the frame has after the override.
        let axes = if aligned { own } else { wanted };
        let shift = [0, 1, 2].map(|c| dot(offset, column(&axes, c)));
        let mut over = Map::new();
        if !aligned {
            over.insert("z_axis".to_owned(), json!(zw));
            over.insert("x_axis".to_owned(), json!(column(&wanted, 0)));
        }
        if norm(shift) > MEET {
            over.insert("shift".to_owned(), json!(shift));
        }
        let frame_override = (!over.is_empty()).then_some(Value::Object(over));
        let parsed_override: Option<FrameOverride> = match &frame_override {
            Some(v) => Some(serde_json::from_value(v.clone()).map_err(|e| format!("{name}: {e}"))?),
            None => None,
        };
        let frame = self
            .doc
            .joint_frame(at, "", &parsed, parsed_override.as_ref())
            .map_err(|e| format!("{element} of {label} gives MitCad's joints no frame ({e})"))?
            .local;
        // FreeCAD's connector where the file places the parts.
        let size = 1.0 + norm(wanted.translation);
        if norm(sub(frame.translation, wanted.translation)) > MEET * size
            || norm(cross(column(&frame, 2), zw)) > ALIGNED
        {
            return Err(format!(
                "MitCad's frame on {element} of {label} does not meet FreeCAD's connector"
            ));
        }
        let mut value = json!({"occurrence": path_text(&side.path), "geometry": geometry});
        if let Some(o) = frame_override {
            value["frame_override"] = o;
        }
        let on = if aligned {
            format!("{element} of {label}")
        } else {
            format!("{element} of {label} (with FreeCAD's axes)")
        };
        Ok(Origin {
            value,
            path: side.path.clone(),
            frame,
            on: Ok(on),
        })
    }

    /// The object at the end of a chain of links of this document (the
    /// object itself when it is no link).
    fn link_end(&self, name: &str) -> Result<String, String> {
        let source = &self.sources[0];
        let mut at = name.to_owned();
        for _ in 0..8 {
            let Some(o) = source.object(&at) else {
                break;
            };
            if source.role(o) != Role::Link {
                break;
            }
            let Some(link) = o.link("LinkedObject") else {
                break;
            };
            if link.file.is_some() {
                return Err(format!(
                    "{} shows an object of another document",
                    source.label(&at)
                ));
            }
            if link.subs.iter().any(|s| !s.name.is_empty()) {
                break;
            }
            at = link.object.clone();
        }
        Ok(at)
    }

    /// Every occurrence's placement now.
    fn placements(&self) -> BTreeMap<OccurrenceUid, Transform> {
        self.occurrences_of
            .values()
            .flatten()
            .filter_map(|u| Some((*u, self.doc.placement(*u)?)))
            .collect()
    }
}

/// A side as the joint's origin: its path, the component's origin with
/// the connector's frame given (fixed in the part).
fn fixed_origin(side: &Side) -> Origin {
    let f = &side.frame;
    Origin {
        value: json!({"occurrence": path_text(&side.path), "geometry": "origin",
            "frame_override": {"origin": f.translation,
                "z_axis": column(f, 2), "x_axis": column(f, 0)}}),
        path: side.path.clone(),
        frame: side.frame,
        on: Err("FreeCAD's frame".to_owned()),
    }
}

fn path_text(path: &[OccurrenceUid]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// The occurrences from `from` down to `occurrence` (it last), at most
/// `depth` levels deep.
fn path_down(
    a: &mitcad_model::assembly::Assembly,
    from: ComponentUid,
    occurrence: OccurrenceUid,
    depth: usize,
) -> Option<Vec<OccurrenceUid>> {
    let o = a.occurrence(occurrence)?;
    if o.parent == from {
        return Some(vec![occurrence]);
    }
    if depth == 0 {
        return None;
    }
    a.children(from).find_map(|child| {
        let mut rest = path_down(a, child.component, occurrence, depth - 1)?;
        rest.insert(0, child.uid);
        Some(rest)
    })
}

fn column(t: &Transform, c: usize) -> [f64; 3] {
    [t.linear[0][c], t.linear[1][c], t.linear[2][c]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Half a turn about x (MitCad's joint flip).
fn half_turn_x() -> Transform {
    Transform {
        linear: [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]],
        translation: [0.0; 3],
    }
}

/// How far apart two placements are: the largest difference of their
/// entries (mm for the translation, the rotation's entries otherwise).
fn gap(a: &Transform, b: &Transform) -> f64 {
    let mut most: f64 = 0.0;
    for r in 0..3 {
        for c in 0..3 {
            most = most.max((a.linear[r][c] - b.linear[r][c]).abs());
        }
        most = most.max((a.translation[r] - b.translation[r]).abs());
    }
    most
}
