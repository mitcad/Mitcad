// SPDX-License-Identifier: MIT
//! Placement expressions of Bodies and parts as moves of their occurrences
//! (mitcad#1560).
//!
//! A Body with a placement of its own and a part (App::Part, an Assembly)
//! are components of their own, each placed by an occurrence at the
//! object's placement (a Body whose position an expression drives gets one
//! even where the file has it at the origin). Where expressions drive the
//! placement's position (`.Placement.Base.x`, `.y`, `.z`), the occurrence
//! is placed without the driven coordinates and a `move_occurrence` of its
//! parent component, early on the timeline (after the bodies, before the
//! history), adds them by the translated expressions: the occurrence is
//! where the file places the object and follows the parameters.
//!
//! - A turn of the placement driven by an expression keeps FreeCAD's
//!   number (the occurrence moves by translations only), with the reason;
//!   so does a position whose occurrence is grounded, or whose move does
//!   not land where the file places the object.
//! - A clone of such a Body (a base feature, `bases.rs`) is a linked copy
//!   that follows the Body's occurrence; its move takes the Body's driven
//!   coordinates off again ([`Importer::driven_terms`]), so that the copy
//!   goes where the clone's own placement puts it, as in FreeCAD.
//! - Links keep their numbers: a link shows its object at the link's own
//!   placement, which no Body expression drives.

use std::collections::BTreeMap;

use mitcad_freecad::{Object, Value as FcValue};
use mitcad_model::{ComponentUid, Kernel, OccurrenceUid, Transform};
use serde_json::json;

use super::params::{Kind, Q, Source};
use super::{Importer, Key};

/// Placements within this agree, mm.
const MEET: f64 = 1e-6;

const AXES: [&str; 3] = ["x", "y", "z"];

/// Whether an expression of the object drives its placement's position.
pub(super) fn position_driven(o: &Object) -> bool {
    let Some(FcValue::Expressions(list)) = o.value("ExpressionEngine") else {
        return false;
    };
    list.iter().any(|e| {
        e.path
            .trim_start_matches('.')
            .starts_with("Placement.Base.")
    })
}

/// The driven coordinates of an occurrence's position: each axis's
/// expression (its value FreeCAD's coordinate), None where a number places
/// it.
pub(super) type Driven = [Option<Q>; 3];

impl<K: Kernel> Importer<'_, '_, K> {
    /// The moves of the occurrences of Bodies and parts whose positions
    /// expressions drive (before the history).
    pub(super) fn import_placements(&mut self) {
        let objects: Vec<Object> = self.sources[0]
            .file
            .document
            .objects
            .iter()
            .filter(|o| o.type_name == "PartDesign::Body" || super::is_component(&o.type_name))
            .filter(|o| {
                AXES.iter()
                    .any(|a| self.is_bound(o, &format!("Placement.Base.{a}")))
            })
            .cloned()
            .collect();
        for o in objects {
            if let Err(why) = self.placement_moves(&o) {
                let why = format!("a position of a Body or part, FreeCAD's number: {why}");
                for axis in AXES {
                    let path = format!("Placement.Base.{axis}");
                    self.left_out(&o, &path, &why);
                    self.kept_value(&o, &path, &why);
                }
            }
        }
    }

    /// The occurrences of one object moved by its position's expressions:
    /// placed without the driven coordinates, then a `move_occurrence` in
    /// each parent component adds them. Err (and nothing changed) with the
    /// reason where it cannot be done.
    fn placement_moves(&mut self, o: &Object) -> Result<(), String> {
        let key = Key {
            doc: 0,
            name: o.name.clone(),
        };
        let occurrences = self.occurrences_of.get(&key).cloned().unwrap_or_default();
        if occurrences.is_empty() {
            return Err("it is not placed as an occurrence of its own".to_owned());
        }
        let stored = o.placement().unwrap_or_default();
        let mut axes: Driven = [None, None, None];
        for (k, axis) in AXES.iter().enumerate() {
            let path = format!("Placement.Base.{axis}");
            if !self.is_bound(o, &path) {
                continue;
            }
            let q = self.quantity(
                Source::property(&o.name, &path),
                Kind::Length,
                stored.position[k],
            );
            if q.text.is_some() {
                axes[k] = Some(q);
            }
        }
        // (An expression that does not translate says why in its row.)
        if axes.iter().all(Option::is_none) {
            return Ok(());
        }
        let mut by_parent: BTreeMap<ComponentUid, Vec<OccurrenceUid>> = BTreeMap::new();
        let mut before: Vec<(OccurrenceUid, Transform)> = Vec::new();
        {
            let a = self.doc.assembly();
            for u in &occurrences {
                let x = a.occurrence(*u).ok_or("its occurrence is gone")?;
                if x.grounded {
                    return Err(
                        "its occurrence is grounded in the assembly, and a grounded occurrence \
                         does not move"
                            .to_owned(),
                    );
                }
                by_parent.entry(x.parent).or_default().push(*u);
                before.push((*u, x.transform));
            }
        }
        let depth = self.doc.undo_depth();
        if let Err(why) = self.add_placement_moves(o, &axes, &stored, &by_parent, &before) {
            while self.doc.undo_depth() > depth {
                if self.doc.undo().is_none() {
                    break;
                }
            }
            return Err(why);
        }
        for u in occurrences {
            self.driven.insert(u, axes.clone());
        }
        Ok(())
    }

    fn add_placement_moves(
        &mut self,
        o: &Object,
        axes: &Driven,
        stored: &mitcad_freecad::Placement,
        by_parent: &BTreeMap<ComponentUid, Vec<OccurrenceUid>>,
        before: &[(OccurrenceUid, Transform)],
    ) -> Result<(), String> {
        // Where the occurrences start: without the driven coordinates.
        for (u, t) in before {
            let mut start = *t;
            for (k, q) in axes.iter().enumerate() {
                if q.is_some() {
                    start.translation[k] -= stored.position[k];
                }
            }
            self.doc
                .set_occurrence_transform(*u, start, false)
                .map_err(|e| e.to_string())?;
        }
        let value = |k: usize| axes[k].as_ref().map_or(json!(0.0), Q::json);
        let spec = json!({"type": "translate_xyz", "x": value(0), "y": value(1), "z": value(2)});
        let label = format!("{} placement", self.sources[0].label(&o.name));
        for (parent, list) in by_parent {
            let def = json!({"type": "move_occurrence",
                "occurrences": list.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "transform": spec.clone()});
            self.add_named(&def, &label, *parent)?;
        }
        // Where the file places them again.
        for (u, t) in before {
            let now = self.doc.placement(*u).ok_or("its occurrence is gone")?;
            let apart = (0..3)
                .map(|k| (now.translation[k] - t.translation[k]).abs())
                .chain(
                    (0..3)
                        .flat_map(|r| (0..3).map(move |c| (r, c)))
                        .map(|(r, c)| (now.linear[r][c] - t.linear[r][c]).abs()),
                )
                .fold(0.0, f64::max);
            if apart > MEET {
                return Err(format!(
                    "the move by its expressions puts it {apart:.3e} mm from where the file \
                     places it"
                ));
            }
        }
        Ok(())
    }

    /// The driven coordinates of the occurrences between two components,
    /// as signed terms of a translation of `from`'s geometry seen in `to`
    /// (through the occurrences where the document places each component
    /// first, as [`Importer::seen_in`] reads it): each driven coordinate of
    /// an occurrence on `from`'s path below the paths' common part taken
    /// off (−1), of one on `to`'s path added (+1), where that occurrence's
    /// parent is not turned against `to`. Per axis of `to`.
    pub(super) fn driven_terms(&self, from: ComponentUid, to: ComponentUid) -> [Vec<(f64, Q)>; 3] {
        let mut out: [Vec<(f64, Q)>; 3] = Default::default();
        if self.driven.is_empty() || from == to {
            return out;
        }
        let path = |c: ComponentUid| -> Option<Vec<OccurrenceUid>> {
            if c.is_root() {
                Some(Vec::new())
            } else {
                self.component_paths.get(&c).cloned()
            }
        };
        let (Some(source), Some(target)) = (path(from), path(to)) else {
            return out;
        };
        let common = source
            .iter()
            .zip(&target)
            .take_while(|(a, b)| a == b)
            .count();
        let into = mitcad_model::assembly::inverse(&self.doc.path_transform(&target));
        for (list, sign) in [(&source, -1.0), (&target, 1.0)] {
            for (i, occurrence) in list.iter().enumerate().skip(common) {
                let Some(axes) = self.driven.get(occurrence) else {
                    continue;
                };
                let parent = into.after(&self.doc.path_transform(&list[..i]));
                if !turns_nothing(&parent) {
                    continue;
                }
                for (k, q) in axes.iter().enumerate() {
                    if let Some(q) = q {
                        out[k].push((sign, q.clone()));
                    }
                }
            }
        }
        out
    }
}

fn turns_nothing(t: &Transform) -> bool {
    (0..3).all(|r| (0..3).all(|c| (t.linear[r][c] - if r == c { 1.0 } else { 0.0 }).abs() <= 1e-12))
}
