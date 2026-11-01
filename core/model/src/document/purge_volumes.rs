// SPDX-License-Identifier: MIT
//! Purge volumes on filament changes (mitcad#705, [`crate::print_purge`]):
//! the pairs of materials whose volume the design sets by hand, and the
//! table of the filament slots a 3MF export gives the bodies printed.
//!
//! The slots are as the export makes them: the bodies in their order (the
//! design's bodies when none are given; sheet and empty bodies are not
//! printed), a slot per distinct material in the order of first use, each
//! body's material and settings in each occurrence that shows it (all of
//! them when none does): a body whose placements print otherwise brings
//! each of their materials (mitcad#1061).
//! `set_purge_volumes` is an undo step that recomputes nothing; the design
//! file keeps the pairs (`purge_volumes`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::print_settings::hex_color;
use super::{DocState, Document, EffectivePrint, ModelError, PrintTarget, invalid};
use crate::exchange::BodyKind;
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_purge::{self, PurgeOverride};

/// A pair's change: its volume (mm³), or none to take the value set by
/// hand away (the computed volume again).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeChange {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub volume: Option<f64>,
}

impl DocState {
    /// Sets a pair's volume, or takes it away (None).
    pub(crate) fn set_purge_override(&mut self, from: &str, to: &str, volume: Option<f64>) {
        set_override(&mut self.purge_volumes, from, to, volume);
    }

    /// Takes away the pairs of a material the design no longer has
    /// (mitcad#758), so that a new material that reuses its id (the first
    /// free `custom{n}`) starts computed; undo brings them back with the
    /// material. A built-in material of that id is still there and keeps
    /// them, as the bodies that named the deleted one take it.
    pub(crate) fn drop_purge_overrides_of(&mut self, id: &str) {
        if crate::material::built_in_material(id).is_none() {
            self.purge_volumes.retain(|p| p.from != id && p.to != id);
        }
    }
}

/// Sets a pair's volume in a list of pairs, or takes it away (None).
fn set_override(pairs: &mut Vec<PurgeOverride>, from: &str, to: &str, volume: Option<f64>) {
    let at = pairs.iter().position(|o| o.from == from && o.to == to);
    match (at, volume) {
        (Some(i), Some(volume)) => pairs[i].volume = volume,
        (None, Some(volume)) => pairs.push(PurgeOverride {
            from: from.to_owned(),
            to: to.to_owned(),
            volume,
        }),
        (Some(i), None) => {
            pairs.remove(i);
        }
        (None, None) => {}
    }
}

/// The filament slots of a print: each slot's material settings (its first
/// body's) and the bodies of each.
pub(crate) struct PurgeSlots {
    pub settings: Vec<EffectivePrint>,
    pub bodies: Vec<Vec<BodyUid>>,
}

impl<K: Kernel> Document<K> {
    /// The purge volumes the design sets by hand.
    pub fn purge_overrides(&self) -> &[PurgeOverride] {
        &self.state.purge_volumes
    }

    /// Sets or resets the purge volumes of pairs of materials (from one to
    /// another): one undo step, which recomputes nothing, unless nothing
    /// changes.
    pub fn set_purge_volumes(&mut self, changes: &[PurgeChange]) -> Result<(), ModelError> {
        if changes.is_empty() {
            return Err(invalid("set_purge_volumes needs the pairs to set"));
        }
        for change in changes {
            for id in [&change.from, &change.to] {
                if self.material(id).is_none() {
                    return Err(invalid(format!("unknown material {id}")));
                }
            }
            let pair = PurgeOverride {
                from: change.from.clone(),
                to: change.to.clone(),
                volume: change.volume.unwrap_or(0.0),
            };
            pair.check().map_err(invalid)?;
        }
        let mut next = self.state.purge_volumes.clone();
        for change in changes {
            set_override(&mut next, &change.from, &change.to, change.volume);
        }
        if next == self.state.purge_volumes {
            return Ok(());
        }
        let name = |id: &str| {
            self.material(id)
                .map_or_else(|| id.to_owned(), |m| m.name.clone())
        };
        let label = match changes {
            [one] => format!(
                "{} Purge Volume {} to {}",
                if one.volume.is_some() { "Set" } else { "Reset" },
                name(&one.from),
                name(&one.to)
            ),
            _ if changes.iter().all(|c| c.volume.is_none()) => "Reset Purge Volumes".to_owned(),
            _ => "Set Purge Volumes".to_owned(),
        };
        let changes = changes.to_vec();
        self.apply_with(move |state| {
            for change in &changes {
                state.set_purge_override(&change.from, &change.to, change.volume);
            }
            Ok((label, (), false))
        })
    }

    /// The filament slots of the bodies printed (all the design's when
    /// `bodies` is empty), as the 3MF export makes them.
    pub(crate) fn purge_slots(&self, bodies: &[BodyUid]) -> Result<PurgeSlots, ModelError> {
        let all = self.bodies();
        let chosen: Vec<BodyUid> = if bodies.is_empty() {
            all.iter().map(|b| b.uid).collect()
        } else {
            for uid in bodies {
                if !all.iter().any(|b| b.uid == *uid) {
                    return Err(invalid(format!(
                        "body {uid} does not exist at the timeline marker"
                    )));
                }
            }
            bodies.to_vec()
        };
        let instances = self.instances();
        // A body's placements printed: the shown ones, all of them when
        // none is shown; each prints with its own settings, so a body
        // whose placements print otherwise may bring several materials
        // (mitcad#1061).
        let paths_of = |body: BodyUid| -> Vec<Option<Vec<OccurrenceUid>>> {
            let mine: Vec<_> = instances.iter().filter(|i| i.body == body).collect();
            let any_shown = mine.iter().any(|i| i.visible);
            let paths: Vec<Option<Vec<OccurrenceUid>>> = mine
                .iter()
                .filter(|i| i.visible || !any_shown)
                .map(|i| Some(i.path.clone()))
                .collect();
            if paths.is_empty() { vec![None] } else { paths }
        };
        let mut slots = PurgeSlots {
            settings: Vec::new(),
            bodies: Vec::new(),
        };
        for uid in chosen {
            let Some(body) = all.iter().find(|b| b.uid == uid) else {
                continue;
            };
            // Sheet and empty bodies are not printed.
            let printed = !matches!(
                self.kernel().body_kind(body.shape),
                Ok(BodyKind::Sheet | BodyKind::Empty)
            );
            if !printed {
                continue;
            }
            // A slot per material and appearance (mitcad#1055), each
            // placement of the body read on its own (mitcad#1061).
            for path in paths_of(uid) {
                let settings = self.effective_print(&PrintTarget::Body(uid, path));
                match slots.settings.iter().position(|s| {
                    s.material == settings.material && s.appearance == settings.appearance
                }) {
                    Some(slot) => {
                        if !slots.bodies[slot].contains(&uid) {
                            slots.bodies[slot].push(uid);
                        }
                    }
                    None => {
                        slots.settings.push(settings);
                        slots.bodies.push(vec![uid]);
                    }
                }
            }
        }
        Ok(slots)
    }

    /// The `purge_volumes` query: the slots of the bodies printed (each its
    /// `material`, `appearance` (mitcad#1055), `name`, `color`, `purge_into` and `purge_out` and its
    /// `bodies`), the `multiplier`, and `volumes[from][to]`: each pair's
    /// `computed` volume, the `volume` used and whether it is `edited`
    /// (set by hand).
    pub fn purge_volumes_json(&self, bodies: &[BodyUid]) -> Result<Value, ModelError> {
        let slots = self.purge_slots(bodies)?;
        let firsts: Vec<&EffectivePrint> = slots.settings.iter().collect();
        let matrix = crate::print_files::purge_matrix(&firsts, &self.state.purge_volumes);
        let rows: Vec<Value> = slots
            .settings
            .iter()
            .zip(&matrix.slots)
            .zip(&slots.bodies)
            .map(|((settings, values), bodies)| {
                let color = settings
                    .filament
                    .get("filament_colour")
                    .and_then(|(v, _)| v.as_str())
                    .and_then(crate::print_files::hex_rgb)
                    .map(hex_color);
                // A material in two colours (mitcad#1055): each slot named
                // with its appearance.
                let twice = slots
                    .settings
                    .iter()
                    .filter(|s| s.material == settings.material)
                    .count()
                    > 1;
                let look = settings
                    .appearance
                    .as_deref()
                    .and_then(|a| self.appearance(a))
                    .map(|a| a.name.clone());
                let name = match look {
                    Some(look) if twice => format!("{} ({look})", settings.material_name),
                    _ => settings.material_name.clone(),
                };
                json!({
                    "material": settings.material,
                    "appearance": settings.appearance,
                    "name": name,
                    "color": color,
                    "purge_into": values.into,
                    "purge_out": values.out,
                    "bodies": bodies,
                })
            })
            .collect();
        let volumes: Vec<Value> = matrix
            .cells
            .iter()
            .map(|row| {
                row.iter()
                    .map(
                        |c| json!({"computed": c.computed, "volume": c.volume, "edited": c.edited}),
                    )
                    .collect()
            })
            .collect();
        Ok(json!({
            "multiplier": matrix.multiplier,
            "slots": rows,
            "volumes": volumes,
        }))
    }
}

/// The `purge_defaults` query: a colour's purge values (`purge_into`,
/// `purge_out`), what a material without its own takes; `color` `#RRGGBB`,
/// none for mid grey.
pub fn purge_defaults_json(color: Option<&str>) -> Result<Value, ModelError> {
    let rgb = match color {
        Some(text) => Some(
            crate::print_files::hex_rgb(text)
                .ok_or_else(|| invalid(format!("“{text}” is no color #RRGGBB")))?,
        ),
        None => None,
    };
    Ok(json!(print_purge::defaults_json(rgb)))
}
