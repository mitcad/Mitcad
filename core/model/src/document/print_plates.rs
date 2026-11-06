// SPDX-License-Identifier: MIT
//! Print plates in the design (mitcad#1050, [`crate::print_plates`]): the
//! commands that add, edit, order and delete plates and put bodies and
//! occurrences on them with their counts, the plate a body prints on (the
//! level its settings take in the settings order,
//! `document/print_settings.rs`) and the `print_plates` query.
//!
//! Every command is one undo step that recomputes nothing; the design
//! file keeps the plates (`print_plates`). An item whose body or
//! occurrence is gone stays on its plate (it may come back with an undo
//! or a change of the timeline marker): the query marks it `missing` and
//! the export leaves it out with a warning.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::print_settings::value_text;
use super::{Document, ModelError, PrintTarget, invalid};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_keys;
use crate::print_plates::{
    MAX_COUNT, MAX_PLATES, PlateItem, PlateTarget, PrintPlate, new_plate, path_text,
};

/// A plate's change (`edit_print_plate`): its name, and values to set (a
/// key's `null` takes that value away).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlateChange {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub values: Option<BTreeMap<String, Value>>,
}

impl<K: Kernel> Document<K> {
    /// The design's print plates, in order.
    pub fn print_plates(&self) -> &[crate::print_plates::PrintPlate] {
        &self.state.print_plates
    }

    /// A plate's index by its id (or its name, ignoring case).
    pub fn plate_index(&self, plate: &str) -> Result<usize, ModelError> {
        let plate = plate.trim();
        let plates = &self.state.print_plates;
        plates
            .iter()
            .position(|p| p.id == plate)
            .or_else(|| {
                plates
                    .iter()
                    .position(|p| p.name.trim().eq_ignore_ascii_case(plate))
            })
            .ok_or_else(|| {
                let ids: Vec<&str> = plates.iter().map(|p| p.id.as_str()).collect();
                invalid(format!(
                    "there is no plate {plate} (the design's: {})",
                    if ids.is_empty() {
                        "none".to_owned()
                    } else {
                        ids.join(", ")
                    }
                ))
            })
    }

    /// Whether an occurrence path is one of the design's: each occurrence
    /// placed in the component of the one before it (the first in the
    /// root).
    pub(crate) fn path_exists(&self, path: &[OccurrenceUid]) -> bool {
        let mut parent = ComponentUid::ROOT;
        for uid in path {
            let Some(occurrence) = self.assembly().children(parent).find(|o| o.uid == *uid) else {
                return false;
            };
            parent = occurrence.component;
        }
        !path.is_empty()
    }

    /// Whether a plate item's body or occurrence is in the design now.
    pub(crate) fn plate_target_exists(&self, target: &PlateTarget) -> bool {
        match target {
            PlateTarget::Body(uid) => self.body_shape(*uid).is_some(),
            PlateTarget::Occurrence(path) => self.path_exists(path),
        }
    }

    /// A target's name for people: the body's name, or the occurrence's
    /// path of names (`Arm:1/Pin:2`).
    pub(crate) fn plate_target_name(&self, target: &PlateTarget) -> String {
        match target {
            PlateTarget::Body(uid) => self.body_name(*uid),
            PlateTarget::Occurrence(path) => self.assembly().path_name(path),
        }
    }

    /// The plate a body prints on in a placement (`path`, None for a body
    /// not placed): the plate of its own item, else of the outermost
    /// occurrence item the placement is in, else the first plate; none
    /// when the design has no plates.
    pub fn plate_of_body(&self, body: BodyUid, path: Option<&[OccurrenceUid]>) -> Option<usize> {
        let plates = &self.state.print_plates;
        if plates.is_empty() {
            return None;
        }
        let own = PlateTarget::Body(body);
        if let Some(i) = plates.iter().position(|p| p.item(&own).is_some()) {
            return Some(i);
        }
        path.and_then(|path| self.plate_of_path(path)).or(Some(0))
    }

    /// The plate of the outermost occurrence item that a path is in (the
    /// occurrence itself or one of its parents); none without one.
    pub fn plate_of_path(&self, path: &[OccurrenceUid]) -> Option<usize> {
        self.state
            .print_plates
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                p.items.iter().filter_map(move |item| match &item.target {
                    PlateTarget::Occurrence(o) if path.starts_with(o) => Some((i, o.len())),
                    _ => None,
                })
            })
            .min_by_key(|(_, depth)| *depth)
            .map(|(i, _)| i)
    }

    /// The plate whose settings a target takes on its own (the level of
    /// the settings order between the components and the design): a
    /// body's plate ([`Document::plate_of_body`]), an occurrence's item's;
    /// none for a component.
    pub(crate) fn auto_plate(&self, target: &PrintTarget) -> Option<&PrintPlate> {
        let index = match target {
            PrintTarget::Body(body, path) => self.plate_of_body(*body, path.as_deref()),
            PrintTarget::Occurrence(path) => self.plate_of_path(path),
            PrintTarget::Component(_) => None,
        }?;
        self.state.print_plates.get(index)
    }

    /// Adds a plate after the others (named `Plate <n>` unless a name is
    /// given). Its id and name. One undo step.
    pub fn add_print_plate(&mut self, name: Option<&str>) -> Result<(String, String), ModelError> {
        if self.state.print_plates.len() >= MAX_PLATES {
            return Err(invalid(format!("a design has at most {MAX_PLATES} plates")));
        }
        let (id, default_name) = new_plate(&self.state.print_plates);
        let name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or(default_name, str::to_owned);
        if self.plate_name_taken(&name, None) {
            return Err(invalid(format!("a plate is named {name}")));
        }
        let plate = PrintPlate::new(&id, &name);
        plate.check().map_err(invalid)?;
        let label = format!("Add {name}");
        let out = (id, name);
        self.apply_with(move |state| {
            state.print_plates.push(plate);
            Ok((label, (), false))
        })?;
        Ok(out)
    }

    /// Whether another plate (than the one at `own`) has the name.
    fn plate_name_taken(&self, name: &str, own: Option<usize>) -> bool {
        self.state
            .print_plates
            .iter()
            .enumerate()
            .any(|(i, p)| Some(i) != own && p.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    /// Renames a plate and sets or takes away its values. One undo step,
    /// unless nothing changes.
    pub fn edit_print_plate(
        &mut self,
        plate: &str,
        change: &PlateChange,
    ) -> Result<(), ModelError> {
        let i = self.plate_index(plate)?;
        let mut edited = self.state.print_plates[i].clone();
        if let Some(name) = &change.name {
            let name = name.trim();
            if name.is_empty() {
                return Err(invalid("a plate's name cannot be empty"));
            }
            if self.plate_name_taken(name, Some(i)) {
                return Err(invalid(format!("a plate is named {name}")));
            }
            edited.name = name.to_owned();
        }
        if let Some(values) = &change.values {
            for (key, value) in values {
                if value.is_null() {
                    edited.values.remove(key.trim());
                } else {
                    edited.values.insert(key.trim().to_owned(), value.clone());
                }
            }
        }
        edited.normalize();
        if edited == self.state.print_plates[i] {
            return Ok(());
        }
        edited.check().map_err(invalid)?;
        let old = &self.state.print_plates[i].name;
        let label = match (&change.name, &change.values) {
            (Some(_), None) => format!("Rename {old}"),
            (None, Some(values)) if values.len() == 1 => {
                let (key, value) = values.iter().next().expect("one value");
                let name = print_keys::key(key.trim()).map_or(key.as_str(), |d| d.name);
                if value.is_null() {
                    format!("Clear {name} of {old}")
                } else {
                    format!("Set {name} of {old}")
                }
            }
            _ => format!("Edit {old}"),
        };
        self.apply_with(move |state| {
            state.print_plates[i] = edited;
            Ok((label, (), false))
        })
    }

    /// Deletes a plate; what was on it goes on no plate (the first one's
    /// then). One undo step.
    pub fn delete_print_plate(&mut self, plate: &str) -> Result<(), ModelError> {
        let i = self.plate_index(plate)?;
        let label = format!("Delete {}", self.state.print_plates[i].name);
        self.apply_with(move |state| {
            state.print_plates.remove(i);
            Ok((label, (), false))
        })
    }

    /// Moves a plate to `index` in the order (the last when beyond it). One
    /// undo step, unless it is there already.
    pub fn move_print_plate(&mut self, plate: &str, index: usize) -> Result<(), ModelError> {
        let i = self.plate_index(plate)?;
        let to = index.min(self.state.print_plates.len() - 1);
        if to == i {
            return Ok(());
        }
        let label = format!("Move {}", self.state.print_plates[i].name);
        self.apply_with(move |state| {
            let moved = state.print_plates.remove(i);
            state.print_plates.insert(to, moved);
            Ok((label, (), false))
        })
    }

    /// Checks a target to put on a plate: a printed body at the marker
    /// (a solid or a mesh body), or an occurrence of the design.
    fn check_plate_target(&self, target: &PlateTarget) -> Result<(), ModelError> {
        match target {
            PlateTarget::Body(uid) => {
                if self.body_shape(*uid).is_none() {
                    return Err(invalid(format!(
                        "body {uid} does not exist at the timeline marker"
                    )));
                }
                if !self.printed_body(*uid) {
                    return Err(invalid(format!(
                        "{} is not printed (a surface or empty body)",
                        self.body_name(*uid)
                    )));
                }
            }
            PlateTarget::Occurrence(path) => {
                if !self.path_exists(path) {
                    return Err(invalid(format!(
                        "there is no occurrence {}",
                        path_text(path)
                    )));
                }
            }
        }
        Ok(())
    }

    /// Puts bodies and occurrences on a plate, each with its count (None:
    /// the count it has, here or on another plate, else 1); one on another
    /// plate moves here. One undo step, unless nothing changes.
    pub fn place_on_plate(
        &mut self,
        plate: &str,
        items: &[(PlateTarget, Option<u32>)],
    ) -> Result<(), ModelError> {
        if items.is_empty() {
            return Err(invalid(
                "place_on_plate needs the bodies or occurrences to put on it",
            ));
        }
        let i = self.plate_index(plate)?;
        for (n, (target, count)) in items.iter().enumerate() {
            self.check_plate_target(target)?;
            if let Some(count) = count
                && (*count == 0 || *count > MAX_COUNT)
            {
                return Err(invalid(format!(
                    "{}: a plate takes 1 to {MAX_COUNT} of an item, not {count}",
                    self.plate_target_name(target)
                )));
            }
            if items[..n].iter().any(|(other, _)| other == target) {
                return Err(invalid(format!(
                    "{} is listed more than once",
                    self.plate_target_name(target)
                )));
            }
        }
        let mut plates = self.state.print_plates.clone();
        for (target, count) in items {
            // The count it has, here or on the plate it leaves.
            let here = plates
                .iter()
                .find_map(|p| p.item(target))
                .map(|item| item.count);
            for (p, other) in plates.iter_mut().enumerate() {
                if p != i {
                    other.items.retain(|item| item.target != *target);
                }
            }
            let count = count.or(here).unwrap_or(1);
            match plates[i]
                .items
                .iter_mut()
                .find(|item| item.target == *target)
            {
                Some(item) => item.count = count,
                None => plates[i].items.push(PlateItem {
                    target: target.clone(),
                    count,
                }),
            }
        }
        if plates == self.state.print_plates {
            return Ok(());
        }
        let plate_name = plates[i].name.clone();
        let label = match items {
            [(target, Some(count))] if plates[i].item(target).is_some() && *count > 1 => format!(
                "Put {count} of {} on {plate_name}",
                self.plate_target_name(target)
            ),
            [(target, _)] => format!("Put {} on {plate_name}", self.plate_target_name(target)),
            _ => format!("Put {} items on {plate_name}", items.len()),
        };
        self.apply_with(move |state| {
            state.print_plates = plates;
            Ok((label, (), false))
        })
    }

    /// Takes bodies and occurrences off the plates they are on (they then
    /// go on the first plate as the design places them). One undo step,
    /// unless none was on a plate.
    pub fn remove_from_plates(&mut self, targets: &[PlateTarget]) -> Result<(), ModelError> {
        if targets.is_empty() {
            return Err(invalid(
                "remove_from_plates needs the bodies or occurrences to take off",
            ));
        }
        let mut plates = self.state.print_plates.clone();
        let mut from = Vec::new();
        for plate in &mut plates {
            let before = plate.items.len();
            plate.items.retain(|item| !targets.contains(&item.target));
            if plate.items.len() != before {
                from.push(plate.name.clone());
            }
        }
        if from.is_empty() {
            return Ok(());
        }
        let label = match targets {
            [one] => format!(
                "Take {} off {}",
                self.plate_target_name(one),
                from.join(", ")
            ),
            _ => format!("Take {} items off the plates", targets.len()),
        };
        self.apply_with(move |state| {
            state.print_plates = plates;
            Ok((label, (), false))
        })
    }

    /// The placements on no plate: each shown placement of a printed body
    /// without an item of its own that no occurrence item takes (all of
    /// its placements when none is shown), with its path (None for a body
    /// not placed). They print on the first plate.
    pub(crate) fn unplaced(&self) -> Vec<(BodyUid, Option<Vec<OccurrenceUid>>)> {
        let instances = self.instances();
        let mut out = Vec::new();
        for body in self.bodies() {
            let uid = body.uid;
            if !self.printed_body(uid)
                || self
                    .state
                    .print_plates
                    .iter()
                    .any(|p| p.item(&PlateTarget::Body(uid)).is_some())
            {
                continue;
            }
            let mine: Vec<_> = instances.iter().filter(|i| i.body == uid).collect();
            if mine.is_empty() {
                out.push((uid, None));
                continue;
            }
            let shown: Vec<_> = mine.iter().filter(|i| i.visible).collect();
            let placements: Vec<&&crate::document::InstanceView> = if shown.is_empty() {
                mine.iter().collect()
            } else {
                shown
            };
            for instance in placements {
                if self.plate_of_path(&instance.path).is_none() {
                    out.push((uid, Some(instance.path.clone())));
                }
            }
        }
        out
    }

    /// The `print_plates` query: each plate with its items (their names,
    /// counts and whether they are in the design now), its own values and
    /// the number of pieces it prints; and the placements on no plate,
    /// which print on the first.
    pub fn print_plates_json(&self) -> Value {
        let unplaced = self.unplaced();
        let instances = self.instances();
        let plates: Vec<Value> = self
            .state
            .print_plates
            .iter()
            .enumerate()
            .map(|(index, plate)| {
                let items: Vec<Value> = plate
                    .items
                    .iter()
                    .map(|item| {
                        let mut out = match &item.target {
                            PlateTarget::Body(uid) => json!({"body": uid}),
                            PlateTarget::Occurrence(path) => {
                                json!({"occurrence": path_text(path)})
                            }
                        };
                        out["name"] = json!(self.plate_target_name(&item.target));
                        out["count"] = json!(item.count);
                        if !self.plate_target_exists(&item.target) {
                            out["missing"] = json!(true);
                        }
                        out
                    })
                    .collect();
                let values: Vec<Value> = print_keys::keys_in(print_keys::PrintSet::Process)
                    .filter_map(|def| {
                        let value = plate.values.get(def.key)?;
                        Some(json!({"key": def.key, "name": def.name, "value": value,
                                    "text": value_text(def, value)}))
                    })
                    .collect();
                // As the export makes them: an item whose bodies print on
                // another plate adds none (mitcad#1310).
                let pieces = self.plate_piece_count(index, &instances);
                json!({
                    "id": plate.id,
                    "name": plate.name,
                    "index": index,
                    "items": items,
                    "values": values,
                    "pieces": pieces,
                })
            })
            .collect();
        let unplaced: Vec<Value> = unplaced
            .iter()
            .map(|(uid, path)| {
                json!({
                    "body": uid,
                    "name": self.body_name(*uid),
                    "occurrence": path.as_deref().map(path_text).unwrap_or_default(),
                    "occurrence_name": path
                        .as_deref()
                        .map(|p| self.assembly().path_name(p))
                        .unwrap_or_default(),
                })
            })
            .collect();
        json!({"plates": plates, "unplaced": unplaced})
    }
}
