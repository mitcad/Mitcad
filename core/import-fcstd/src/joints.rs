// SPDX-License-Identifier: MIT
//! FreeCAD's assembly joints (the Assembly workbench, 1.0 and later) as
//! MitCad joints (mitcad#1557).
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
//!   placements of their own along the path), its frame FreeCAD's
//!   connector, given as the origin's frame override (a fixed frame in that
//!   component: it does not follow the element it was picked on).
//! - The two frames as the file places the parts give the joint's flip
//!   (the z axes opposed: FreeCAD's solver keeps either way), and what the
//!   kind holds fixed: a rigid joint's turn about z and offset along it, a
//!   revolute joint's offset, a slider's turn. So the joint holds where
//!   FreeCAD left the parts, and no occurrence moves; a joint that would
//!   move one is not added (its reason in the report).
//! - Limits (`EnableLengthMin` …) are left out, with a note.
//! - Grounded joints ground their objects' occurrences (stage 1); the
//!   other kinds (Distance, Angle, Parallel, Perpendicular, gears, belts,
//!   rack and pinion, screw) have no MitCad joint kind and are skipped with
//!   that reason.

use std::collections::BTreeMap;

use mitcad_freecad::{Object, Value as FcValue};
use mitcad_model::{ComponentUid, Kernel, OccurrenceUid, Transform};
use serde_json::{Value, json};

use super::report::ObjectOutcome;
use super::{Importer, Key, placement_of, transform};

/// Distances within this meet, mm.
const MEET: f64 = 1e-6;
/// Directions within this agree (a sine).
const ALIGNED: f64 = 1e-7;

/// One connector of a joint: the occurrence path in the joint's component
/// and the connector's frame in the component that path ends in.
struct Side {
    path: Vec<OccurrenceUid>,
    frame: Transform,
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
                Ok((made, notes)) => (
                    if notes.is_empty() {
                        ObjectOutcome::Parametric
                    } else {
                        ObjectOutcome::Partial
                    },
                    std::iter::once(format!(
                        "a {made}: its connectors are FreeCAD's frames, fixed in the parts \
                         (not linked to the elements they were picked on)"
                    ))
                    .chain(notes)
                    .collect::<Vec<_>>()
                    .join("; "),
                ),
                Err(why) => (
                    ObjectOutcome::Skipped,
                    format!("an assembly joint, not imported: {why}"),
                ),
            };
            self.roles.insert(joint.name.clone(), (state, Some(note)));
        }
    }

    /// One joint: its feature and notes, or why not.
    fn joint(&mut self, o: &Object) -> Result<(String, Vec<String>), String> {
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
            "a": self.side_value(&a), "b": self.side_value(&b)});
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
        let limits: Vec<&str> = [
            "EnableLengthMin",
            "EnableLengthMax",
            "EnableAngleMin",
            "EnableAngleMax",
        ]
        .into_iter()
        .filter(|p| o.bool(p) == Some(true))
        .collect();
        if !limits.is_empty() {
            notes.push(format!("its limits are left out ({})", limits.join(", ")));
        }
        // Added where nothing moves: the occurrences as the file places them.
        let before = self.placements();
        let label = o.label().to_owned();
        let uid = self.add_named(&def, &label, assembly)?;
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
        Ok((format!("{kind} joint {uid}"), notes))
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
    /// assembly's component, and `Placement<k>` with the placements along
    /// the path that are not occurrences.
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
        })
    }

    /// A side as the joint's origin: its path, the component's origin with
    /// the connector's frame given.
    fn side_value(&self, side: &Side) -> Value {
        let path = side
            .path
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("/");
        json!({"occurrence": path, "geometry": "origin",
            "frame_override": {"origin": side.frame.translation,
                "z_axis": column(&side.frame, 2), "x_axis": column(&side.frame, 0)}})
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
