// SPDX-License-Identifier: MIT
//! Filament changes and pauses at print heights in a slicer's 3MF project
//! (mitcad#1596, [`crate::print_changes`]): the events of each plate,
//! placed on its layers by the export (`document/filament_changes.rs`),
//! written as the slicer's own per-layer custom G-code file. No printer
//! G-code is written: the slicer emits its printer profile's own
//! filament-change and pause G-code for each event.
//!
//! - **PrusaSlicer** (`Metadata/Prusa_Slicer_custom_gcode_per_print_z.xml`,
//!   PrusaSlicer 2.9.7 `src/libslic3r/Format/3mf.cpp`,
//!   `_add_custom_gcode_per_print_z_file_to_archive` and
//!   `_extract_custom_gcode_per_print_z_from_archive`): a
//!   `custom_gcodes_per_print_z` element per bed (`bed_idx`, from 0; bed 0
//!   always), a `code` per event (`print_z`, `type`, `extruder`, `color`,
//!   `extra`; `type` 0 ColorChange, 1 PausePrint of `CustomGCode::Type`)
//!   and its `mode`. A change is a colour change (M600) of the plate's
//!   one extruder: the filament's settings stay the first filament's.
//! - **The Bambu family** (Bambu Studio, OrcaSlicer, QIDI Studio:
//!   `Metadata/custom_gcode_per_layer.xml`, `bbs_3mf.cpp` / `qds_3mf.cpp`):
//!   `custom_gcodes_per_layer` with a `plate` per plate (`plate_info id`,
//!   from 1), a `layer` per event (`top_z`, `type`, `extruder`, `color`,
//!   `extra`) and its `mode`. Their G-code export leaves a colour change
//!   (type 0) out, so a change is a filament change (type 2, ToolChange)
//!   to the filament's own slot, in `MultiAsSingle` mode: the project gets
//!   a filament slot for each filament changed to, and no prime tower when
//!   every plate's parts print with one filament (`bambu.rs`,
//!   `prime_tower_off`).
//! - **Cura** keeps no such events in a project (they are post-processing
//!   scripts of the machine's settings): each event is a warning that names
//!   its layer, to add there in Cura.
//!
//! Every event's height is the top of the first layer printed after it,
//! which PrusaSlicer (`ToolOrdering::assign_custom_gcodes`: the lowest
//! layer whose `print_z` is at or above it) and the Bambu family (the
//! nearest layer) both give that very layer.

use std::fmt::Write as _;

use super::{ExportOutput, Shared, xml_escape};
use crate::document::EffectivePrint;
use crate::print_changes::{ChangeKind, mm};
use crate::print_keys::Dialect;

/// PrusaSlicer's file of custom G-code per print height.
pub const PRUSA_FILE: &str = "Metadata/Prusa_Slicer_custom_gcode_per_print_z.xml";
/// The Bambu family's file of custom G-code per layer.
pub const BAMBU_FILE: &str = "Metadata/custom_gcode_per_layer.xml";

/// What happens at an event's layer.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerAction {
    /// A change to a filament: an index into [`ChangeExport::filaments`].
    Change(usize),
    /// A pause, with its message (empty: none).
    Pause(String),
}

/// An event placed on a plate's layers: the height it was given (mm above
/// the bed), the top of the first layer printed after it (mm) and that
/// layer's number (from 1).
#[derive(Debug, Clone, PartialEq)]
pub struct LayerEvent {
    pub height: f64,
    pub print_z: f64,
    pub layer: u64,
    pub action: LayerAction,
}

/// A plate's events: the plate (an index into the export's plates; 0 for
/// an export without plates), a part on it (its filament is the one the
/// plate starts with), its name for warnings and its events, upwards.
#[derive(Debug, Clone, PartialEq)]
pub struct PlateEvents {
    pub plate: usize,
    pub start_part: usize,
    pub label: String,
    pub events: Vec<LayerEvent>,
}

/// The layer events of an export: the filaments they change to (each
/// once) and each plate's events.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangeExport {
    pub filaments: Vec<EffectivePrint>,
    pub plates: Vec<PlateEvents>,
}

/// An event written into the project: its plate, height, `print_z`,
/// layer, kind and the material it changes to.
#[derive(Debug, Clone, PartialEq)]
pub struct WrittenEvent {
    pub plate: usize,
    pub height: f64,
    pub print_z: f64,
    pub layer: u64,
    pub kind: ChangeKind,
    pub material: Option<String>,
}

/// A filament's colour as the slicers write it, `#RRGGBB`; white when it
/// has none.
fn colour_of(settings: &EffectivePrint) -> String {
    let rgb = settings
        .filament
        .get("filament_colour")
        .and_then(|(v, _)| v.as_str())
        .and_then(super::hex_rgb)
        .unwrap_or([1.0; 3]);
    let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// A filament's name for people: its material's, with its colour.
fn filament_name(settings: &EffectivePrint) -> String {
    format!("{} ({})", settings.material_name, colour_of(settings))
}

/// A height in the files: plain decimals, a nanometre at most.
fn z_text(z: f64) -> String {
    mm(z)
}

/// Writes the events of `changes` for the export's slicer into `out` (its
/// files, warnings and the events written).
pub(super) fn write(shared: &Shared<'_>, changes: &ChangeExport, out: &mut ExportOutput) {
    if changes.plates.is_empty() {
        return;
    }
    match shared.dialect {
        Dialect::BambuStudio | Dialect::OrcaSlicer | Dialect::QidiStudio => {
            write_bambu(shared, changes, out);
        }
        Dialect::PrusaSlicer => write_prusa(shared, changes, out),
        Dialect::Cura => cura_warnings(changes, out),
    }
}

/// The Bambu family's `custom_gcode_per_layer.xml` (see the module
/// documentation): a ToolChange to the filament's slot per change, a
/// PausePrint per pause.
fn write_bambu(shared: &Shared<'_>, changes: &ChangeExport, out: &mut ExportOutput) {
    let name = shared.dialect.name();
    let mut xml =
        String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<custom_gcodes_per_layer>\n");
    let mut any = false;
    for plate in &changes.plates {
        let start = shared.slots.get(plate.start_part).copied().unwrap_or(0);
        let mut current = start;
        let mut lines = String::new();
        let mut tool_change = false;
        for event in &plate.events {
            let z = z_text(event.print_z);
            match &event.action {
                LayerAction::Change(f) => {
                    let target = &changes.filaments[*f];
                    let Some(slot) = shared.slot_of(target) else {
                        continue;
                    };
                    if slot == current {
                        out.warnings.push(format!(
                            "{}: the change at {} mm is to {}, the filament already printing, \
                             which {name} does not change: left out (a pause stops for a new \
                             spool)",
                            plate.label,
                            mm(event.height),
                            filament_name(target)
                        ));
                        continue;
                    }
                    let _ = writeln!(
                        lines,
                        "<layer top_z=\"{z}\" type=\"2\" extruder=\"{}\" color=\"{}\" extra=\"\"/>",
                        slot + 1,
                        colour_of(target)
                    );
                    current = slot;
                    tool_change = true;
                    out.layer_events.push(written(plate, event, Some(target)));
                }
                LayerAction::Pause(message) => {
                    let _ = writeln!(
                        lines,
                        "<layer top_z=\"{z}\" type=\"1\" extruder=\"{}\" color=\"\" extra=\"{}\"/>",
                        current + 1,
                        xml_escape(message)
                    );
                    out.layer_events.push(written(plate, event, None));
                }
            }
        }
        if lines.is_empty() {
            continue;
        }
        any = true;
        let mode = if tool_change {
            "MultiAsSingle"
        } else {
            "SingleExtruder"
        };
        let _ = writeln!(
            xml,
            "<plate>\n<plate_info id=\"{}\"/>\n{lines}<mode value=\"{mode}\"/>\n</plate>",
            plate.plate + 1
        );
    }
    xml.push_str("</custom_gcodes_per_layer>\n");
    if any {
        out.files.push((BAMBU_FILE.to_owned(), xml.into_bytes()));
    }
}

/// PrusaSlicer's `Prusa_Slicer_custom_gcode_per_print_z.xml` (see the
/// module documentation): a ColorChange of the plate's extruder per
/// change, a PausePrint per pause.
fn write_prusa(shared: &Shared<'_>, changes: &ChangeExport, out: &mut ExportOutput) {
    // The project's extruders: one per filament slot. A project of several
    // is MultiExtruder, or MultiAsSingle on a printer of one nozzle that
    // takes several filaments (`single_extruder_multi_material`), which
    // PrusaSlicer compares with the file's mode before it takes a change.
    let several = shared.slot_settings.len() > 1;
    let one_nozzle = shared.slot_settings.first().is_some_and(|s| {
        s.slicer
            .get(shared.dialect.id())
            .and_then(|keys| keys.get("single_extruder_multi_material"))
            .is_some_and(|(text, _)| text.trim() == "1")
    });
    let mode = match (several, one_nozzle) {
        (false, _) => "SingleExtruder",
        (true, true) => "MultiAsSingle",
        (true, false) => "MultiExtruder",
    };
    let mut beds: Vec<(usize, String)> = Vec::new();
    for plate in &changes.plates {
        let slot = shared.slots.get(plate.start_part).copied().unwrap_or(0);
        let start = shared.slot_settings.get(slot).copied();
        let extruder = if several { slot + 1 } else { 1 };
        let mut lines = String::new();
        for event in &plate.events {
            let z = z_text(event.print_z);
            match &event.action {
                LayerAction::Change(f) => {
                    let target = &changes.filaments[*f];
                    if let Some(start) = start
                        && differs(start, target)
                    {
                        out.warnings.push(format!(
                            "{}: PrusaSlicer changes the colour only at {} mm (a colour change of \
                             the one filament): {} prints with the settings of {}",
                            plate.label,
                            mm(event.height),
                            filament_name(target),
                            filament_name(start)
                        ));
                    }
                    let _ = writeln!(
                        lines,
                        "<code print_z=\"{z}\" type=\"0\" extruder=\"{extruder}\" color=\"{}\" extra=\"\"/>",
                        colour_of(target)
                    );
                    out.layer_events.push(written(plate, event, Some(target)));
                }
                LayerAction::Pause(message) => {
                    let _ = writeln!(
                        lines,
                        "<code print_z=\"{z}\" type=\"1\" extruder=\"{extruder}\" color=\"\" extra=\"{}\"/>",
                        xml_escape(message)
                    );
                    out.layer_events.push(written(plate, event, None));
                }
            }
        }
        if !lines.is_empty() {
            beds.push((plate.plate, lines));
        }
    }
    if beds.is_empty() {
        return;
    }
    // Bed 0 always (PrusaSlicer's own writer does, so that older versions
    // see it), the others with events, in the beds' order.
    if !beds.iter().any(|(bed, _)| *bed == 0) {
        beds.push((0, String::new()));
    }
    beds.sort_by_key(|(bed, _)| *bed);
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    for (bed, lines) in beds {
        let _ = writeln!(
            xml,
            "<custom_gcodes_per_print_z bed_idx=\"{bed}\">\n{lines}<mode value=\"{mode}\"/>\n</custom_gcodes_per_print_z>"
        );
    }
    out.files.push((PRUSA_FILE.to_owned(), xml.into_bytes()));
}

/// Whether a filament prints otherwise than another: its type or a
/// temperature.
fn differs(a: &EffectivePrint, b: &EffectivePrint) -> bool {
    [
        "filament_type",
        "temperature",
        "first_layer_temperature",
        "bed_temperature",
    ]
    .iter()
    .any(|key| a.filament.get(*key).map(|(v, _)| v) != b.filament.get(*key).map(|(v, _)| v))
}

/// Cura: no events in the project; each a warning with its layer.
fn cura_warnings(changes: &ChangeExport, out: &mut ExportOutput) {
    for plate in &changes.plates {
        let listed: Vec<String> = plate
            .events
            .iter()
            .map(|event| match &event.action {
                LayerAction::Change(f) => format!(
                    "layer {} ({} mm) a change to {}",
                    event.layer,
                    mm(event.print_z),
                    filament_name(&changes.filaments[*f])
                ),
                LayerAction::Pause(message) if message.is_empty() => {
                    format!("layer {} ({} mm) a pause", event.layer, mm(event.print_z))
                }
                LayerAction::Pause(message) => format!(
                    "layer {} ({} mm) a pause: {message}",
                    event.layer,
                    mm(event.print_z)
                ),
            })
            .collect();
        out.warnings.push(format!(
            "{}: a Cura project keeps no filament changes or pauses at heights, so none is \
             written; add them in Cura (Extensions, Post Processing: Filament Change or Pause at \
             Height): {}",
            plate.label,
            listed.join("; ")
        ));
    }
}

/// The record of an event written.
fn written(
    plate: &PlateEvents,
    event: &LayerEvent,
    target: Option<&EffectivePrint>,
) -> WrittenEvent {
    WrittenEvent {
        plate: plate.plate,
        height: event.height,
        print_z: event.print_z,
        layer: event.layer,
        kind: if target.is_some() {
            ChangeKind::Change
        } else {
            ChangeKind::Pause
        },
        material: target.map(|t| t.material.clone()),
    }
}
