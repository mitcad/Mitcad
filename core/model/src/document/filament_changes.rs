// SPDX-License-Identifier: MIT
//! Filament changes at print heights (mitcad#1596,
//! [`crate::print_changes`]): the design's schedule and print plates' own,
//! `set_filament_changes` (one undo step, which recomputes nothing) and
//! the `filament_changes` query; the events of a 3MF's plates placed on
//! their layers ([`Document::layer_events`], written by
//! `print_files/layer_events.rs`); and the print colours of a body's
//! placement for `model_render` and the USDZ export
//! ([`Document::print_colors`]).

use serde_json::{Value, json};

use super::print_settings::hex_color;
use super::{Document, EffectivePrint, ModelError, PrintFrom, PrintTarget, invalid};
use crate::exchange::Exported;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::print_changes::{self, ChangeKind, ChangeSchedule, FilamentChange, Layers, mm};
use crate::print_files::{
    ChangeExport, ExportPlate, LayerAction, LayerEvent, NEUTRAL_FILAMENT_COLOR, PlateEvents,
    hex_rgb,
};
use crate::print_profile::PrintProfile;
use crate::transform::Transform;

/// The print colours of a body's placement (mitcad#1596): its print
/// direction (a unit vector in the design's coordinates: up from the
/// print bed), the colour it starts with, each change's layer boundary
/// above the bed (mm) with the colour it changes to, and notes on events
/// that are left out.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PrintColors {
    pub up: [f64; 3],
    pub start: [f64; 3],
    pub changes: Vec<(f64, [f64; 3])>,
    pub notes: Vec<String>,
}

/// Millimetres per unit of a 3MF model.
fn mm_per_unit(unit: mitcad_3mf::Unit) -> f64 {
    match unit {
        mitcad_3mf::Unit::Micron => 0.001,
        mitcad_3mf::Unit::Millimeter => 1.0,
        mitcad_3mf::Unit::Centimeter => 10.0,
        mitcad_3mf::Unit::Inch => 25.4,
        mitcad_3mf::Unit::Foot => 304.8,
        mitcad_3mf::Unit::Meter => 1000.0,
    }
}

impl<K: Kernel> Document<K> {
    /// A plate's id by its id or name; none for the design's own schedule.
    fn schedule_plate(&self, plate: Option<&str>) -> Result<Option<String>, ModelError> {
        match plate {
            None => Ok(None),
            Some(plate) => {
                let i = self.plate_index(plate)?;
                Ok(Some(self.state.print_plates[i].id.clone()))
            }
        }
    }

    /// A plate's own schedule (`plate` its id; none: the design's own),
    /// empty without one.
    pub fn filament_changes(&self, plate: Option<&str>) -> &[FilamentChange] {
        self.state
            .filament_changes
            .iter()
            .find(|s| s.plate.as_deref() == plate)
            .map_or(&[][..], |s| s.changes.as_slice())
    }

    /// The schedule a plate prints with (`plate` its id; none: an export
    /// of bodies): its own, else the design's.
    pub(crate) fn plate_schedule(&self, plate: Option<&str>) -> &[FilamentChange] {
        let own = self.filament_changes(plate);
        if own.is_empty() && plate.is_some() {
            self.filament_changes(None)
        } else {
            own
        }
    }

    /// Sets the schedule of the design (`plate` none) or of a print plate
    /// (its id or name): the events checked and sorted upwards, each
    /// change's material and appearance known to the design; an empty list
    /// takes the schedule away (a plate then prints with the design's).
    /// One undo step, which recomputes nothing, unless nothing changes.
    pub fn set_filament_changes(
        &mut self,
        plate: Option<&str>,
        changes: &[FilamentChange],
    ) -> Result<(), ModelError> {
        let plate = self.schedule_plate(plate)?;
        let sorted = print_changes::normalized(changes).map_err(invalid)?;
        for change in &sorted {
            if let Some(material) = &change.material
                && self.material(material).is_none()
            {
                return Err(invalid(format!("unknown material {material}")));
            }
            if let Some(appearance) = &change.appearance
                && self.appearance(appearance).is_none()
            {
                return Err(invalid(format!("unknown appearance {appearance}")));
            }
        }
        if sorted.as_slice() == self.filament_changes(plate.as_deref()) {
            return Ok(());
        }
        let of = plate
            .as_deref()
            .and_then(|id| self.state.print_plates.iter().find(|p| p.id == id))
            .map(|p| format!(" of {}", p.name))
            .unwrap_or_default();
        let label = if sorted.is_empty() {
            format!("Clear Filament Changes{of}")
        } else {
            format!("Set Filament Changes{of}")
        };
        self.apply_with(move |state| {
            state.filament_changes.retain(|s| s.plate != plate);
            if !sorted.is_empty() {
                state.filament_changes.push(ChangeSchedule {
                    plate,
                    changes: sorted,
                });
            }
            Ok((label, (), false))
        })
    }

    /// The colour a filament (a material with an appearance, else its
    /// default one) prints with, as its `filament_colour`.
    pub(crate) fn filament_color(
        &self,
        material: &str,
        appearance: Option<&str>,
    ) -> Option<[f64; 3]> {
        let material = self.material(material)?;
        let look = appearance.or(material.appearance.as_deref())?;
        self.appearance(look).map(|a| a.display_color())
    }

    /// The settings a change's filament prints with on a plate: `base`
    /// (the plate's first part's) with the material, its appearance (else
    /// its default), its variant for the printer, its filament values and
    /// its slicers' own keys in place of the base's; none for a material
    /// the design does not have.
    pub(crate) fn change_filament_print(
        &self,
        base: &EffectivePrint,
        material: &str,
        appearance: Option<&str>,
    ) -> Option<EffectivePrint> {
        let material = self.material(material)?;
        let look = appearance.or(material.appearance.as_deref());
        let printer = base
            .printer
            .as_ref()
            .and_then(|(id, _)| self.print_profile(id))
            .map(PrintProfile::printer_key)
            .unwrap_or_default();
        let variant = material.variant_for(&printer);
        let index = variant.map(|(i, _)| i);
        let mut out = base.clone();
        out.material = material.id.clone();
        out.material_name = material.name.clone();
        out.appearance = look.map(str::to_owned);
        out.variant = variant;
        out.filament = self.filament_values(material, index, look);
        for keys in out.slicer.values_mut() {
            keys.retain(|_, (_, from)| !matches!(from, PrintFrom::Material(_)));
        }
        let (_, material_slicer) = material.values_with(index);
        for (dialect, keys) in &material_slicer {
            let mine = out.slicer.entry(dialect.clone()).or_default();
            for (key, text) in keys {
                mine.entry(key.clone())
                    .or_insert((text.clone(), PrintFrom::Material(material.id.clone())));
            }
        }
        out.slicer.retain(|_, keys| !keys.is_empty());
        Some(out)
    }

    /// The `filament_changes` query: the schedule of the design (`plate`
    /// none) or of a print plate (its id or name), whether the plate takes
    /// the design's (`inherited`), the plate's layers and each event with
    /// the layer boundary it happens at, the `print_z` a slicer gets and
    /// that layer's number (before the export knows the print's top), its
    /// material's name and colour; with every schedule of the design.
    pub fn filament_changes_json(&self, plate: Option<&str>) -> Result<Value, ModelError> {
        let id = self.schedule_plate(plate)?;
        let on = id
            .as_deref()
            .and_then(|id| self.state.print_plates.iter().find(|p| p.id == id));
        let own = self.filament_changes(id.as_deref());
        let inherited = own.is_empty() && id.is_some();
        let changes = self.plate_schedule(id.as_deref());
        let settings = self.effective_print_on(&PrintTarget::Component(ComponentUid::ROOT), on);
        let layers = Layers::of(&settings);
        let (placed, notes) = print_changes::place(changes, layers, None);
        let rows: Vec<Value> = changes
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let mut row = json!({
                    "height": change.height,
                    "kind": change.kind.as_str(),
                    "material": change.material,
                    "appearance": change.appearance,
                    "message": change.message,
                });
                if let Some(material) = change.material.as_deref() {
                    match self.material(material) {
                        Some(m) => {
                            row["material_name"] = json!(m.name);
                            row["color"] = json!(
                                self.filament_color(material, change.appearance.as_deref())
                                    .map(hex_color)
                            );
                        }
                        None => row["error"] = json!(format!("unknown material {material}")),
                    }
                }
                match placed.iter().find(|p| p.index == i) {
                    Some(p) => {
                        row["boundary"] = json!(p.boundary);
                        row["print_z"] = json!(p.print_z);
                        row["layer"] = json!(p.layer);
                    }
                    None => row["left_out"] = json!(true),
                }
                row
            })
            .collect();
        let schedules: Vec<Value> = self
            .state
            .filament_changes
            .iter()
            .map(|s| {
                let name = s
                    .plate
                    .as_deref()
                    .and_then(|id| self.state.print_plates.iter().find(|p| p.id == id))
                    .map(|p| p.name.clone());
                json!({"plate": s.plate, "name": name, "events": s.changes.len()})
            })
            .collect();
        Ok(json!({
            "plate": on.map(|p| json!({"id": p.id, "name": p.name})),
            "inherited": inherited,
            "layer_height": layers.height,
            "first_layer_height": layers.first,
            "changes": rows,
            "notes": notes,
            "schedules": schedules,
        }))
    }

    /// The layer events of a 3MF model's plates (mitcad#1596; see
    /// [`crate::print_changes`]): `settings` are the parts' (in the
    /// model's order), `plates` the export's plates (none: one plate of
    /// every object) and `plate_ids` their ids (none: the design's own
    /// schedule). Each object is measured from its own lowest point in the
    /// file, where the slicer lays it on the bed; a plate's top is its
    /// tallest object's. A plate whose parts print with more than one
    /// filament, an event at or above the top, two events on one layer and
    /// a change to a material the design no longer has are warnings in
    /// `exported`.
    pub(crate) fn layer_events(
        &self,
        model: &mitcad_3mf::Model,
        settings: &[EffectivePrint],
        plates: &[ExportPlate<'_>],
        plate_ids: &[Option<&str>],
        exported: &mut Exported,
    ) -> ChangeExport {
        let mut out = ChangeExport::default();
        let scale = mm_per_unit(model.unit);
        // Each object's placements (part, placement): one object of every
        // placement when the build has none.
        let objects: Vec<Vec<(usize, usize)>> = if model.objects.is_empty() {
            vec![
                model
                    .parts
                    .iter()
                    .enumerate()
                    .flat_map(|(p, part)| (0..part.placements.len()).map(move |k| (p, k)))
                    .collect(),
            ]
        } else {
            model.objects.iter().map(|o| o.components.clone()).collect()
        };
        // An object's height in the file, mm.
        let height_of = |members: &[(usize, usize)]| -> Option<f64> {
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for &(p, k) in members {
                let Some(part) = model.parts.get(p) else {
                    continue;
                };
                let Some(placement) = part.placements.get(k) else {
                    continue;
                };
                for v in &part.mesh.vertices {
                    let z = placement.apply(*v)[2] * scale;
                    low = low.min(z);
                    high = high.max(z);
                }
            }
            (low <= high).then_some(high - low)
        };
        let on_plates: Vec<Vec<usize>> = if plates.is_empty() {
            vec![(0..objects.len()).collect()]
        } else {
            plates.iter().map(|p| p.objects.clone()).collect()
        };
        for (i, objs) in on_plates.iter().enumerate() {
            let id = plate_ids.get(i).copied().flatten();
            let schedule = self.plate_schedule(id);
            if schedule.is_empty() {
                continue;
            }
            let label = plates
                .get(i)
                .map_or_else(|| "The design".to_owned(), |p| p.name.to_owned());
            let mut objs = objs.clone();
            objs.sort_unstable();
            objs.dedup();
            let mut parts: Vec<usize> = objs
                .iter()
                .filter_map(|&o| objects.get(o))
                .flatten()
                .map(|&(p, _)| p)
                .collect();
            parts.sort_unstable();
            parts.dedup();
            let Some(first) = parts.first().and_then(|&p| settings.get(p)) else {
                continue;
            };
            // One filament below the first change: a single extruder.
            let mut starts: Vec<&EffectivePrint> = Vec::new();
            for &p in &parts {
                if let Some(s) = settings.get(p)
                    && !starts
                        .iter()
                        .any(|t| t.material == s.material && t.appearance == s.appearance)
                {
                    starts.push(s);
                }
            }
            if starts.len() > 1 {
                let names: Vec<String> = starts
                    .iter()
                    .map(|s| {
                        let look = s
                            .appearance
                            .as_deref()
                            .and_then(|a| self.appearance(a))
                            .map(|a| a.name.clone());
                        match look {
                            Some(look) => format!("{} ({look})", s.material_name),
                            None => s.material_name.clone(),
                        }
                    })
                    .collect();
                exported.warnings.push(format!(
                    "{label}: the filament changes and pauses are for one filament on a single \
                     extruder, and its parts print with {} filaments ({}): none is written; give \
                     the parts one material and appearance, or put them on plates of their own",
                    starts.len(),
                    names.join(", ")
                ));
                continue;
            }
            let layers = Layers::of(first);
            let top = objs
                .iter()
                .filter_map(|&o| objects.get(o))
                .filter_map(|members| height_of(members))
                .fold(0.0, f64::max);
            let (placed, notes) = print_changes::place(schedule, layers, Some(top));
            exported
                .warnings
                .extend(notes.into_iter().map(|n| format!("{label}: {n}")));
            let mut events = Vec::new();
            for p in placed {
                let change = &schedule[p.index];
                let action = match change.kind {
                    ChangeKind::Pause => {
                        LayerAction::Pause(change.message.clone().unwrap_or_default())
                    }
                    ChangeKind::Change => {
                        let material = change.material.as_deref().unwrap_or_default();
                        let Some(target) = self.change_filament_print(
                            first,
                            material,
                            change.appearance.as_deref(),
                        ) else {
                            exported.warnings.push(format!(
                                "{label}: the change at {} mm is to the material {material}, \
                                 which the design does not have: left out",
                                mm(change.height)
                            ));
                            continue;
                        };
                        let known = out.filaments.iter().position(|f| {
                            f.material == target.material && f.appearance == target.appearance
                        });
                        LayerAction::Change(known.unwrap_or_else(|| {
                            out.filaments.push(target);
                            out.filaments.len() - 1
                        }))
                    }
                };
                events.push(LayerEvent {
                    height: change.height,
                    print_z: p.print_z,
                    layer: p.layer,
                    action,
                });
            }
            if !events.is_empty() {
                out.plates.push(PlateEvents {
                    plate: i,
                    start_part: parts[0],
                    label,
                    events,
                });
            }
        }
        out
    }

    /// The print colours of a body's placement (`path`, placed by
    /// `transform`; see [`PrintColors`]): the schedule of the plate it
    /// prints on (the design's without plates), on that plate's layers;
    /// its print direction away from its print bed face (the design's +Z
    /// without one); its own filament below the first change.
    pub(crate) fn print_colors(
        &self,
        body: BodyUid,
        path: Option<&[OccurrenceUid]>,
        transform: &Transform,
    ) -> PrintColors {
        let target = PrintTarget::Body(body, path.map(<[OccurrenceUid]>::to_vec));
        let plate = self.auto_plate(&target);
        let settings = self.effective_print_on(&target, plate);
        let start = settings
            .filament
            .get("filament_colour")
            .and_then(|(v, _)| v.as_str())
            .and_then(hex_rgb)
            .unwrap_or(NEUTRAL_FILAMENT_COLOR);
        let schedule = self.plate_schedule(plate.map(|p| p.id.as_str()));
        let layers = Layers::of(&settings);
        let (placed, notes) = print_changes::place(schedule, layers, None);
        let changes = placed
            .iter()
            .filter_map(|p| {
                let change = &schedule[p.index];
                if change.kind != ChangeKind::Change {
                    return None;
                }
                let color = change
                    .material
                    .as_deref()
                    .and_then(|m| self.filament_color(m, change.appearance.as_deref()))
                    .unwrap_or(NEUTRAL_FILAMENT_COLOR);
                Some((p.boundary, color))
            })
            .collect();
        let up = self
            .print_bed_face(body)
            .and_then(|face| face.plane.ok())
            .and_then(|plane| mitcad_vec3::unit(transform.apply_vector(plane.normal())))
            .map_or([0.0, 0.0, 1.0], |down| down.map(|c| -c));
        PrintColors {
            up,
            start,
            changes,
            notes,
        }
    }
}
