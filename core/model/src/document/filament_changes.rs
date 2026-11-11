// SPDX-License-Identifier: MIT
//! Filament changes at print heights (mitcad#1596,
//! [`crate::print_changes`]): the design's schedule and print plates' own,
//! `set_filament_changes` (one undo step, which recomputes nothing) and
//! the `filament_changes` query; the events of a 3MF's plates placed on
//! their layers ([`Document::layer_events`], written by
//! `print_files/layer_events.rs`); and the print colours of a body's
//! placement for `model_render` and the USDZ export
//! ([`Document::print_colors`]). An event at a plane or a face
//! (mitcad#1643) has its height worked out here: from the lowest point of
//! the body it is measured from along the print direction to the plane,
//! in the design for the query and the print colours, in the 3MF's own
//! placements for the export; [`Document::filament_change_preview_json`]
//! gives the app the print colours and each change's ring.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde_json::{Value, json};

use super::print_settings::hex_color;
use super::{Document, EffectivePrint, ModelError, PrintFrom, PrintTarget, invalid};
use crate::exchange::{Exported, MeshTolerance};
use crate::features::geom_ref::GeomRef;
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::notice::NoticePlace;
use crate::print_changes::{
    self, Bands, ChangeKind, ChangeSchedule, FilamentChange, Layers, Place, Resolved, mm,
};
use crate::print_files::{
    ChangeExport, ExportPlate, LayerAction, LayerEvent, NEUTRAL_FILAMENT_COLOR, PlateEvents,
    hex_rgb,
};
use crate::print_plates::{PlateTarget, PrintPlate};
use crate::print_profile::PrintProfile;
use crate::topo::FaceName;
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
            // An event at a plane or face (mitcad#1643): a plane at the
            // timeline marker, of a body that is printed.
            if let Some(at) = &change.at {
                if self.resolve_plane(at).is_err() {
                    let said = match self.change_place(change) {
                        Place::Face(body) => format!(
                            "the face of {body} that the event is at is gone or not planar at the \
                             timeline marker"
                        ),
                        Place::Named(plane) => format!(
                            "{plane} is no plane at the timeline marker: a filament change cannot \
                             be at it"
                        ),
                        _ => "the construction plane of the event is gone".to_owned(),
                    };
                    return Err(invalid(
                        match self.change_places(change).into_iter().next() {
                            Some(place) => crate::notice::placed(said, place, format!("{at:?}")),
                            None => said,
                        },
                    ));
                }
                if let Some(body) = Self::measured_body(change, None)
                    && !self.printed_body(body)
                {
                    return Err(invalid(format!(
                        "{} is no solid or mesh body at the timeline marker: a filament change \
                         cannot be measured from it",
                        self.body_name(body)
                    )));
                }
            }
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
    /// material's name and colour; with every schedule of the design. An
    /// event at a plane or face (mitcad#1643) has its `height` worked out
    /// as the design places the body it is measured from (`body`), its
    /// `source` (`height`, `plane` or `face`), its `place` in words and
    /// its `places`; one that gives no height is `left_out` with its
    /// `warning`.
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
        let first = self.first_body_for(changes, on);
        let (resolved, missing) = self.resolve_in_design(changes, first);
        let (placed, mut notes) = print_changes::place(&resolved, layers, None);
        notes.extend(missing.values().cloned());
        let rows: Vec<Value> = changes
            .iter()
            .enumerate()
            .map(|(i, change)| {
                let height = resolved.iter().find(|r| r.index == i).map(|r| r.height);
                let mut row = json!({
                    "event": change,
                    "height": height,
                    "kind": change.kind.as_str(),
                    "material": change.material,
                    "appearance": change.appearance,
                    "message": change.message,
                    "place": self.change_place(change).text(),
                    "source": match &change.at {
                        None => "height",
                        Some(GeomRef::Face { .. }) => "face",
                        Some(_) => "plane",
                    },
                });
                if let Some(at) = &change.at {
                    row["at"] = json!(at);
                    row["body"] = json!(Self::measured_body(change, first));
                    let places = self.change_places(change);
                    if !places.is_empty() {
                        row["places"] = json!(places);
                    }
                    // A face an edit renamed: found again under this name,
                    // which Auto Fix writes into the schedule.
                    if let Some(found) = self.renamed_change_face(change) {
                        row["found_as"] = json!(found.to_string());
                    }
                }
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
                    None => {
                        row["left_out"] = json!(true);
                        if let Some(note) = missing.get(&i) {
                            row["warning"] = json!(note);
                        }
                    }
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
        // What a change can change to (mitcad#1643, Send to Slicer's
        // rows): each of the design's own materials in each appearance it
        // offers, and the filament the plate starts with.
        let named = |material: &str, appearance: Option<&str>| {
            let name = self
                .material(material)
                .map_or(material, |m| m.name.as_str());
            match appearance.and_then(|a| self.appearance(a)) {
                Some(look) => format!("{name} ({})", look.name),
                None => name.to_owned(),
            }
        };
        let mut choices: Vec<Value> = Vec::new();
        for material in self.custom_materials() {
            // Its default appearance and the others it offers, each once;
            // none: the material alone.
            let mut looks: Vec<Option<&str>> = Vec::new();
            for look in material.appearance.iter().chain(&material.appearances) {
                if !looks.contains(&Some(look.as_str())) {
                    looks.push(Some(look.as_str()));
                }
            }
            if looks.is_empty() {
                looks.push(None);
            }
            for look in looks {
                choices.push(json!({
                    "material": material.id,
                    "appearance": look,
                    "name": named(&material.id, look),
                    "color": self.filament_color(&material.id, look).map(hex_color),
                }));
            }
        }
        let start = self.plate_first_body(on).map(|body| {
            let print = self.effective_print_on(&PrintTarget::Body(body, None), on);
            json!({
                "material": print.material,
                "appearance": print.appearance,
                "name": named(&print.material, print.appearance.as_deref()),
            })
        });
        Ok(json!({
            "plate": on.map(|p| json!({"id": p.id, "name": p.name})),
            "inherited": inherited,
            "start": start,
            "choices": choices,
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
        design: &[Vec<Transform>],
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
            // The events at planes and faces (mitcad#1643): measured from
            // their bodies as the file places them, each object from its
            // own lowest point.
            let (resolved, measured) =
                self.resolve_in_file(model, schedule, &objs, &objects, design, &exported.parts);
            let (placed, notes) = print_changes::place(&resolved, layers, Some(top));
            exported
                .warnings
                .extend(notes.into_iter().map(|n| format!("{label}: {n}")));
            let missing: Vec<String> = measured.missing.values().cloned().collect();
            exported
                .warnings
                .extend(missing.into_iter().map(|n| format!("{label}: {n}")));
            if let Some(body) = measured.uneven {
                exported.warnings.push(format!(
                    "{label}: the filament changes at planes and faces are measured from {}, and \
                     other objects of the plate stand at other levels: each change happens on \
                     the same layer for all of them",
                    self.body_name(body)
                ));
            }
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
                            let height = resolved
                                .iter()
                                .find(|r| r.index == p.index)
                                .map_or(p.boundary, |r| r.height);
                            exported.warnings.push(format!(
                                "{label}: the change at {} mm is to the material {material}, \
                                 which the design does not have: left out",
                                mm(height)
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
                let height = resolved
                    .iter()
                    .find(|r| r.index == p.index)
                    .map_or(p.boundary, |r| r.height);
                events.push(LayerEvent {
                    height,
                    at: change.at.clone(),
                    body: change
                        .at
                        .as_ref()
                        .and_then(|_| measured.bodies.get(&p.index).copied()),
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
        // The events at planes and faces measured as the design places
        // their bodies (mitcad#1643).
        let first = self.first_body_for(schedule, plate);
        let (resolved, missing) = self.resolve_in_design(schedule, first);
        let (placed, mut notes) = print_changes::place(&resolved, layers, None);
        notes.extend(missing.into_values());
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
        let up = self.print_up(body, transform);
        PrintColors {
            up,
            start,
            changes,
            notes,
        }
    }
}

/// How far from level a plane may lean and still give a height: the cosine
/// of its normal with the print direction at least 1 minus this (about 0.08
/// degrees).
const LEVEL: f64 = 1e-6;

/// Why an event at a plane or face has no height (mitcad#1643).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Missing {
    /// The plane or face is gone, or is no longer a plane.
    Unresolved,
    /// Not parallel to the print bed in the print orientation.
    NotLevel,
    /// At or below the lowest point of the body it is measured from.
    Below,
    /// A construction plane of another component than the body's (but the
    /// root's).
    OtherComponent,
    /// The body it is measured from is not printed on the plate.
    NotPrinted,
    /// There is no body to measure from.
    NoBody,
}

/// A placement of the body an event is measured from: its own coordinates
/// into the print's (`map`, mm) and into the design's (`design`), the print
/// direction in the print's coordinates (`up`, a unit vector) and the
/// lowest point of its object along it (`low`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Frame {
    pub map: Transform,
    pub design: Transform,
    pub up: [f64; 3],
    pub low: f64,
}

/// The inverse of a rigid motion or a mirror; none for another map.
fn orthogonal_inverse(t: &Transform) -> Option<Transform> {
    if !t.is_orthogonal() {
        return None;
    }
    let linear: [[f64; 3]; 3] = std::array::from_fn(|r| std::array::from_fn(|c| t.linear[c][r]));
    let back = Transform {
        linear,
        translation: [0.0; 3],
    };
    let moved = back.apply_vector(t.translation);
    Some(Transform {
        linear,
        translation: moved.map(|c| -c),
    })
}

impl<K: Kernel> Document<K> {
    /// Where an event is (mitcad#1643, mitcad#1327: by the names the
    /// timeline and the browser show, never topological names): its
    /// height, a construction plane's name, an origin plane's (`XY`), a
    /// face of a body (its name), or a construction plane that is gone.
    pub(crate) fn change_place(&self, change: &FilamentChange) -> Place {
        match &change.at {
            None => Place::Height(change.height.unwrap_or(0.0)),
            Some(GeomRef::Datum(uid)) => self
                .feature(*uid)
                .map_or(Place::Gone, |f| Place::Named(f.name.clone())),
            Some(GeomRef::Origin(origin)) => Place::Named(origin.display_name().to_owned()),
            Some(GeomRef::Face { body, .. }) => Place::Face(self.body_name(*body)),
            Some(_) => Place::Gone,
        }
    }

    /// The places of an event for the hover and agents: a face's.
    pub(crate) fn change_places(&self, change: &FilamentChange) -> Vec<NoticePlace> {
        match &change.at {
            Some(GeomRef::Face { body, face }) => {
                vec![NoticePlace::face(face.to_string()).in_body(&body.to_string())]
            }
            _ => Vec::new(),
        }
    }

    /// Why an event is left out, in words (mitcad#1643): `body` the body it
    /// is measured from.
    pub(crate) fn missing_note(
        &self,
        change: &FilamentChange,
        body: Option<BodyUid>,
        why: Missing,
    ) -> String {
        let name = body.map(|b| self.body_name(b)).unwrap_or_default();
        // Whole sentences for each kind of event and place, so that each
        // translates as one (docs/translations.md): a plane by its name, a
        // face of a body.
        let pause = change.kind == ChangeKind::Pause;
        match self.change_place(change) {
            Place::Face(face) => face_note(why, pause, &face, &name),
            Place::Named(plane) => plane_note(why, pause, &plane, &name),
            Place::Height(height) => plane_note(why, pause, &format!("{} mm", mm(height)), &name),
            Place::Gone if pause => {
                "a pause is at a construction plane that is gone, so it is left out".to_owned()
            }
            Place::Gone => {
                "a filament change is at a construction plane that is gone, so it is left out"
                    .to_owned()
            }
        }
    }

    /// The body an event at a plane or face is measured from: its face's
    /// own, the one it names, else `first` (the plate's first body).
    pub(crate) fn measured_body(
        change: &FilamentChange,
        first: Option<BodyUid>,
    ) -> Option<BodyUid> {
        match &change.at {
            Some(GeomRef::Face { body, .. }) => Some(*body),
            _ => change.body.or(first),
        }
    }

    /// The plane of an event (its origin and unit normal) in the own
    /// coordinates of the body it is measured from; `design` takes them
    /// into the design's (for a plane of the root while the body is in an
    /// occurrence).
    fn change_plane(
        &self,
        at: &GeomRef,
        body: BodyUid,
        design: &Transform,
    ) -> Result<([f64; 3], [f64; 3]), Missing> {
        let plane = self.resolve_plane(at).map_err(|_| Missing::Unresolved)?;
        let (origin, normal) = (plane.origin, plane.normal());
        let of = match at {
            GeomRef::Face { body: owner, .. } => {
                return if *owner == body {
                    Ok((origin, normal))
                } else {
                    Err(Missing::OtherComponent)
                };
            }
            GeomRef::Datum(uid) => self.state.entry(*uid).map(|f| f.component),
            _ => Some(ComponentUid::ROOT),
        };
        if of.is_some() && of == self.body_component(body) {
            return Ok((origin, normal));
        }
        if of == Some(ComponentUid::ROOT) {
            let back = orthogonal_inverse(design).ok_or(Missing::OtherComponent)?;
            return Ok((back.apply_point(origin), back.apply_vector(normal)));
        }
        Err(Missing::OtherComponent)
    }

    /// The height of an event at a plane or face above the print bed, mm,
    /// measured from `body` placed by `frame`, or why it has none.
    pub(crate) fn change_height(
        &self,
        change: &FilamentChange,
        body: BodyUid,
        frame: &Frame,
    ) -> Result<f64, Missing> {
        let at = change.at.as_ref().ok_or(Missing::Unresolved)?;
        let (origin, normal) = self.change_plane(at, body, &frame.design)?;
        let normal =
            mitcad_vec3::unit(frame.map.apply_vector(normal)).ok_or(Missing::Unresolved)?;
        if mitcad_vec3::dot(normal, frame.up).abs() < 1.0 - LEVEL {
            return Err(Missing::NotLevel);
        }
        let height = mitcad_vec3::dot(frame.map.apply_point(origin), frame.up) - frame.low;
        // Rounded to a nanometre, so that a plane on a layer boundary stays
        // on it.
        let height = (height * 1e6).round() / 1e6;
        if height <= 0.0 {
            return Err(Missing::Below);
        }
        Ok(height)
    }

    /// A body's print direction in the design at a placement: away from its
    /// print bed face, the design's +Z without one.
    pub(crate) fn print_up(&self, body: BodyUid, transform: &Transform) -> [f64; 3] {
        self.print_bed_face(body)
            .and_then(|face| face.plane.ok())
            .and_then(|plane| mitcad_vec3::unit(transform.apply_vector(plane.normal())))
            .map_or([0.0, 0.0, 1.0], |down| down.map(|c| -c))
    }

    /// The lowest point of a body placed by `transform` along `up`: of its
    /// mesh, else of its box's corners.
    fn lowest_along(&self, body: BodyUid, transform: &Transform, up: [f64; 3]) -> Option<f64> {
        let shape = self.body_shape(body)?;
        let lowest = |points: &mut dyn Iterator<Item = [f64; 3]>| {
            points
                .map(|p| mitcad_vec3::dot(transform.apply_point(p), up))
                .fold(f64::INFINITY, f64::min)
        };
        let corners = self.kernel().bounding_box(shape).ok().flatten();
        let diagonal = corners
            .as_ref()
            .map_or(100.0, |b| mitcad_vec3::norm(mitcad_vec3::sub(b.max, b.min)));
        let tolerance = MeshTolerance {
            deviation: (diagonal / 2000.0).max(0.005),
            angle: 12f64.to_radians(),
            face_triangles: 20_000,
            max_edge: 0.0,
        };
        if let Ok(mesh) = self.kernel().face_meshes(shape, &tolerance)
            && !mesh.vertices.is_empty()
        {
            return Some(lowest(&mut mesh.vertices.iter().copied()));
        }
        let b = corners?;
        let low = lowest(&mut (0..8).map(|i| {
            [
                if i & 1 == 0 { b.min[0] } else { b.max[0] },
                if i & 2 == 0 { b.min[1] } else { b.max[1] },
                if i & 4 == 0 { b.min[2] } else { b.max[2] },
            ]
        }));
        low.is_finite().then_some(low)
    }

    /// A body as the design places it, for measuring (its first shown
    /// placement, else its first): the frame of the design (`map` the
    /// placement, `up` its print direction, `low` its lowest point).
    fn design_frame(&self, body: BodyUid) -> Option<Frame> {
        let instances = self.instances();
        let instance = instances
            .iter()
            .filter(|i| i.body == body)
            .min_by_key(|i| !i.visible)?;
        let up = self.print_up(body, &instance.transform);
        let low = self.lowest_along(body, &instance.transform, up)?;
        Some(Frame {
            map: instance.transform,
            design: instance.transform,
            up,
            low,
        })
    }

    /// The first body a plate prints (none: the design's, its first printed
    /// body as the design places them), which events at planes measure
    /// from unless they name another.
    pub(crate) fn plate_first_body(&self, plate: Option<&PrintPlate>) -> Option<BodyUid> {
        let instances = self.instances();
        let printed = |b: BodyUid| self.printed_body(b);
        match plate {
            Some(plate) => plate.items.iter().find_map(|item| match &item.target {
                PlateTarget::Body(body) => printed(*body).then_some(*body),
                PlateTarget::Occurrence(path) => instances
                    .iter()
                    .find(|i| i.path.starts_with(path) && printed(i.body))
                    .map(|i| i.body),
            }),
            None => instances
                .iter()
                .filter(|i| printed(i.body))
                .min_by_key(|i| !i.visible)
                .map(|i| i.body),
        }
    }

    /// The plate's first body when a schedule has events at planes or faces
    /// (none otherwise: nothing is measured).
    pub(crate) fn first_body_for(
        &self,
        changes: &[FilamentChange],
        plate: Option<&PrintPlate>,
    ) -> Option<BodyUid> {
        if changes.iter().any(|c| c.at.is_some()) {
            self.plate_first_body(plate)
        } else {
            None
        }
    }

    /// A schedule's events with their heights in the design (mitcad#1643):
    /// those at heights as given, those at planes and faces measured from
    /// their bodies as the design places them (`first` the plate's first
    /// body); each event left out with its note by its index.
    pub(crate) fn resolve_in_design(
        &self,
        changes: &[FilamentChange],
        first: Option<BodyUid>,
    ) -> (Vec<Resolved>, BTreeMap<usize, String>) {
        let mut resolved = Resolved::heights(changes);
        let mut notes = BTreeMap::new();
        let mut frames: BTreeMap<BodyUid, Option<Frame>> = BTreeMap::new();
        for (index, change) in changes.iter().enumerate() {
            if change.at.is_none() {
                continue;
            }
            let body = Self::measured_body(change, first);
            let height = match body {
                None => Err(Missing::NoBody),
                Some(body) if !self.printed_body(body) => Err(Missing::NotPrinted),
                Some(body) => match *frames
                    .entry(body)
                    .or_insert_with(|| self.design_frame(body))
                {
                    None => Err(Missing::NotPrinted),
                    Some(frame) => self.change_height(change, body, &frame),
                },
            };
            match height {
                Ok(height) => resolved.push(Resolved {
                    index,
                    height,
                    kind: change.kind,
                    place: self.change_place(change),
                }),
                Err(why) => {
                    notes.insert(index, self.missing_note(change, body, why));
                }
            }
        }
        (resolved, notes)
    }

    /// The `filament_change_preview` query (mitcad#1643): what the
    /// application draws over the bodies while Send to Slicer's Filament
    /// changes section is open: each shown placement of a printed body
    /// (`bodies`: only these; all when empty) cut at its plate's layer
    /// boundaries in the colours of the filaments it prints with by height
    /// (as `model_render`'s `print_colors`), its triangles grouped by
    /// colour, flat (`[x, y, z, x, y, z, x, y, z, …]`, design mm); and each
    /// change's ring round it at its boundary (`rings`, two points per
    /// segment), with the notes of events left out.
    pub fn filament_change_preview_json(&self, bodies: &[BodyUid]) -> Value {
        let mut bands: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        let mut rings: Vec<f64> = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        let mut meshes: BTreeMap<BodyUid, Option<crate::exchange::FaceMeshes>> = BTreeMap::new();
        for instance in self.instances() {
            if !instance.visible
                || (!bodies.is_empty() && !bodies.contains(&instance.body))
                || !self.printed_body(instance.body)
            {
                continue;
            }
            let mesh = meshes.entry(instance.body).or_insert_with(|| {
                let shape = self.body_shape(instance.body)?;
                let diagonal = match self.kernel().bounding_box(shape) {
                    Ok(Some(b)) => mitcad_vec3::norm(mitcad_vec3::sub(b.max, b.min)),
                    _ => 100.0,
                };
                let tolerance = MeshTolerance {
                    deviation: (diagonal / 1000.0).max(0.01),
                    angle: 12f64.to_radians(),
                    face_triangles: 20_000,
                    max_edge: 0.0,
                };
                self.kernel().face_meshes(shape, &tolerance).ok()
            });
            let Some(mesh) = mesh else {
                continue;
            };
            let colors = self.print_colors(
                instance.body,
                Some(instance.path.as_slice()),
                &instance.transform,
            );
            for note in colors.notes {
                let note = format!("{}: {note}", instance.name);
                if !notes.contains(&note) {
                    notes.push(note);
                }
            }
            if colors.changes.is_empty() {
                continue;
            }
            let vertices: Vec<[f64; 3]> = mesh
                .vertices
                .iter()
                .map(|&v| instance.transform.apply_point(v))
                .collect();
            let height = |p: [f64; 3]| mitcad_vec3::dot(p, colors.up);
            let (low, high) = vertices
                .iter()
                .map(|&p| height(p))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), h| {
                    (lo.min(h), hi.max(h))
                });
            if low > high {
                continue;
            }
            let cut = Bands::new(colors.start, &colors.changes, low, high);
            let banded =
                print_changes::band_mesh(&vertices, &[], &mesh.triangles, height, &cut.cuts);
            for (triangle, &band) in banded.triangles.iter().zip(&banded.band) {
                let color = hex_color(cut.colors[band.min(cut.colors.len() - 1)]);
                let flat = bands.entry(color).or_default();
                for &i in triangle {
                    flat.extend_from_slice(&banded.vertices[i as usize]);
                }
            }
            for &at in &cut.cuts {
                for [p, q] in print_changes::cut_segments(&vertices, &mesh.triangles, height, at) {
                    rings.extend_from_slice(&p);
                    rings.extend_from_slice(&q);
                }
            }
        }
        let round = |v: &[f64]| -> Vec<Value> {
            v.iter().map(|c| json!((c * 1e4).round() / 1e4)).collect()
        };
        json!({
            "bands": bands
                .iter()
                .map(|(color, flat)| json!({"color": color, "triangles": round(flat)}))
                .collect::<Vec<_>>(),
            "rings": round(&rings),
            "notes": notes,
        })
    }
}

/// What measuring a plate's events at planes and faces in a 3MF found
/// (mitcad#1643): the note of each event left out and the body each event
/// is measured from, by the event's index, and the body measured from when
/// other objects of the plate stood at other levels in the design.
#[derive(Debug, Clone, Default)]
pub(crate) struct Measured {
    pub missing: BTreeMap<usize, String>,
    pub bodies: BTreeMap<usize, BodyUid>,
    pub uneven: Option<BodyUid>,
}

/// Two objects' lowest points this far apart (mm) stand at other levels.
const OTHER_LEVEL: f64 = 0.01;

impl<K: Kernel> Document<K> {
    /// A plate's events with their heights in a 3MF model (mitcad#1643):
    /// those at heights as given; those at planes and faces measured from
    /// their bodies' first placement on the plate, as the file places it
    /// (the print direction the file's +Z, the lowest point that of its
    /// object, where the slicer lays it on the bed). `objs` are the plate's
    /// objects, `objects` each object's placements (part, placement),
    /// `design` each part's placements in the design and `parts` each
    /// part's body.
    pub(crate) fn resolve_in_file(
        &self,
        model: &mitcad_3mf::Model,
        changes: &[FilamentChange],
        objs: &[usize],
        objects: &[Vec<(usize, usize)>],
        design: &[Vec<Transform>],
        parts: &[(BodyUid, String)],
    ) -> (Vec<Resolved>, Measured) {
        let mut resolved = Resolved::heights(changes);
        let mut measured = Measured::default();
        if changes.iter().all(|c| c.at.is_none()) {
            return (resolved, measured);
        }
        let scale = mm_per_unit(model.unit);
        // The plate's placements: (object, part, placement).
        let members: Vec<(usize, usize, usize)> = objs
            .iter()
            .flat_map(|&o| {
                objects
                    .get(o)
                    .into_iter()
                    .flatten()
                    .map(move |&(p, k)| (o, p, k))
            })
            .collect();
        let body_of = |p: usize| parts.get(p).map(|(b, _)| *b);
        let design_of = |p: usize, k: usize| {
            design
                .get(p)
                .and_then(|d| d.get(k))
                .copied()
                .unwrap_or(Transform::IDENTITY)
        };
        // The lowest point of an object along `height` of a placement's
        // points in mm (its file or design place).
        let lowest =
            |o: usize, at: &dyn Fn(usize, usize, [f64; 3]) -> f64| {
                members
                    .iter()
                    .filter(|m| m.0 == o)
                    .flat_map(|&(_, p, k)| {
                        model.parts.get(p).into_iter().flat_map(move |part| {
                            part.mesh.vertices.iter().map(move |&v| (p, k, v))
                        })
                    })
                    .map(|(p, k, v)| at(p, k, v))
                    .fold(f64::INFINITY, f64::min)
            };
        let in_file = |p: usize, k: usize, v: [f64; 3]| {
            model.parts[p]
                .placements
                .get(k)
                .map_or(f64::INFINITY, |placement| placement.apply(v)[2] * scale)
        };
        let first = members.first().and_then(|&(_, p, _)| body_of(p));
        let mut used: Option<(usize, usize, usize)> = None;
        for (index, change) in changes.iter().enumerate() {
            if change.at.is_none() {
                continue;
            }
            let Some(body) = Self::measured_body(change, first) else {
                measured
                    .missing
                    .insert(index, self.missing_note(change, None, Missing::NoBody));
                continue;
            };
            measured.bodies.insert(index, body);
            let Some(&(o, p, k)) = members.iter().find(|m| body_of(m.1) == Some(body)) else {
                measured.missing.insert(
                    index,
                    self.missing_note(change, Some(body), Missing::NotPrinted),
                );
                continue;
            };
            let Some(placement) = model.parts[p].placements.get(k) else {
                continue;
            };
            let frame = Frame {
                map: Transform {
                    linear: placement.linear,
                    translation: placement.translation.map(|c| c * scale),
                },
                design: design_of(p, k),
                up: [0.0, 0.0, 1.0],
                low: lowest(o, &in_file),
            };
            match self.change_height(change, body, &frame) {
                Ok(height) => {
                    resolved.push(Resolved {
                        index,
                        height,
                        kind: change.kind,
                        place: self.change_place(change),
                    });
                    used.get_or_insert((o, p, k));
                }
                Err(why) => {
                    measured
                        .missing
                        .insert(index, self.missing_note(change, Some(body), why));
                }
            }
        }
        // The other objects at other levels in the design, along the print
        // direction of the placement measured from: the slicer lays each on
        // the bed, so a change at its plane's height is on another place of
        // theirs (said once per plate).
        if let Some((o, p, k)) = used {
            let file = model.parts[p].placements[k].linear;
            let own = design_of(p, k);
            // The file's +Z in the body's coordinates, then in the design's.
            let local: [f64; 3] = std::array::from_fn(|c| file[2][c]);
            if let Some(up) = mitcad_vec3::unit(own.apply_vector(local)) {
                let in_design = |p: usize, k: usize, v: [f64; 3]| {
                    mitcad_vec3::dot(design_of(p, k).apply_point(v.map(|c| c * scale)), up)
                };
                let reference = lowest(o, &in_design);
                let mut others: Vec<usize> = members.iter().map(|m| m.0).collect();
                others.dedup();
                if others
                    .into_iter()
                    .filter(|&other| other != o)
                    .any(|other| (lowest(other, &in_design) - reference).abs() > OTHER_LEVEL)
                {
                    measured.uneven = body_of(p);
                }
            }
        }
        (resolved, measured)
    }
}

impl<K: Kernel> Document<K> {
    /// The name the face of an event at a face has now, when an edit
    /// renamed it (mitcad#1643, as a feature's reference is found again):
    /// the one face its stored name finds, under another name; none when it
    /// is found as stored, is gone or is ambiguous.
    pub(crate) fn renamed_change_face(&self, change: &FilamentChange) -> Option<FaceName> {
        let Some(GeomRef::Face { body, face }) = &change.at else {
            return None;
        };
        let shape = self.body_shape(*body)?;
        let listed = self.kernel().faces(shape).ok()?;
        let names: Vec<Vec<FaceName>> = listed
            .iter()
            .map(|f| {
                f.names
                    .iter()
                    .filter_map(|n| FaceName::from_str(n).ok())
                    .collect()
            })
            .collect();
        if names.iter().flatten().any(|n| n.matches(face)) {
            return None;
        }
        match crate::features::renamed::faces_found(&names, face).as_slice() {
            [(_, found)] => Some((*found).clone()),
            _ => None,
        }
    }

    /// The events of every schedule whose faces an edit renamed: (the
    /// schedule's index, the event's, the face's name now), which Auto Fix
    /// writes into the schedules (mitcad#1643).
    pub(crate) fn renamed_change_faces(&self) -> Vec<(usize, usize, FaceName)> {
        let mut out = Vec::new();
        for (s, schedule) in self.state.filament_changes.iter().enumerate() {
            for (e, change) in schedule.changes.iter().enumerate() {
                if let Some(found) = self.renamed_change_face(change) {
                    out.push((s, e, found));
                }
            }
        }
        out
    }
}

/// Why an event at a plane (`plane` its name) is left out, in a whole
/// sentence (`name` the body it is measured from).
fn plane_note(why: Missing, pause: bool, plane: &str, name: &str) -> String {
    match (why, pause) {
        (Missing::Unresolved, false) => format!(
            "the filament change at {plane} refers to a plane that is gone, so it is left out"
        ),
        (Missing::Unresolved, true) => {
            format!("the pause at {plane} refers to a plane that is gone, so it is left out")
        }
        (Missing::NotLevel, false) => format!(
            "the filament change at {plane} is not level in the print orientation, so it is left \
             out"
        ),
        (Missing::NotLevel, true) => {
            format!("the pause at {plane} is not level in the print orientation, so it is left out")
        }
        (Missing::Below, false) => format!(
            "the filament change at {plane} is at or below the bottom of {name} on the print bed, \
             so it is left out"
        ),
        (Missing::Below, true) => format!(
            "the pause at {plane} is at or below the bottom of {name} on the print bed, so it is \
             left out"
        ),
        (Missing::OtherComponent, false) => format!(
            "the filament change at {plane} is a plane of another component than {name}, which it \
             is measured from, so it is left out"
        ),
        (Missing::OtherComponent, true) => format!(
            "the pause at {plane} is a plane of another component than {name}, which it is \
             measured from, so it is left out"
        ),
        (Missing::NotPrinted, false) => format!(
            "the filament change at {plane} is measured from {name}, which is not printed here, so \
             it is left out"
        ),
        (Missing::NotPrinted, true) => format!(
            "the pause at {plane} is measured from {name}, which is not printed here, so it is left \
             out"
        ),
        (Missing::NoBody, false) => format!(
            "the filament change at {plane} has no printed body to be measured from, so it is left \
             out"
        ),
        (Missing::NoBody, true) => format!(
            "the pause at {plane} has no printed body to be measured from, so it is left out"
        ),
    }
}

/// Why an event at a face of a body (`body` its name) is left out, in a
/// whole sentence (`name` the body it is measured from: the same).
fn face_note(why: Missing, pause: bool, body: &str, name: &str) -> String {
    match (why, pause) {
        (Missing::Unresolved, false) => format!(
            "the filament change at a face of {body} refers to a face that is gone or no longer \
             flat, so it is left out"
        ),
        (Missing::Unresolved, true) => format!(
            "the pause at a face of {body} refers to a face that is gone or no longer flat, so it \
             is left out"
        ),
        (Missing::NotLevel, false) => format!(
            "the filament change at a face of {body} is not level in the print orientation, so it \
             is left out"
        ),
        (Missing::NotLevel, true) => format!(
            "the pause at a face of {body} is not level in the print orientation, so it is left out"
        ),
        (Missing::Below, false) => format!(
            "the filament change at a face of {body} is at or below the bottom of {name} on the \
             print bed, so it is left out"
        ),
        (Missing::Below, true) => format!(
            "the pause at a face of {body} is at or below the bottom of {name} on the print bed, \
             so it is left out"
        ),
        // A face is measured from its own body: these cannot be, said
        // all the same.
        (Missing::OtherComponent | Missing::NotPrinted | Missing::NoBody, false) => format!(
            "the filament change at a face of {body} is measured from {name}, which is not printed \
             here, so it is left out"
        ),
        (Missing::OtherComponent | Missing::NotPrinted | Missing::NoBody, true) => format!(
            "the pause at a face of {body} is measured from {name}, which is not printed here, so \
             it is left out"
        ),
    }
}
