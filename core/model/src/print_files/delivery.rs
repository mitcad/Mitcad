// SPDX-License-Identifier: MIT
//! Geometry-only delivery and print-project delivery (mitcad#2008).
//!
//! A slicer's 3MF either carries the geometry alone or a print project.
//! In the Bambu family's 3MF the filament definitions are one list for the
//! whole project (`Metadata/project_settings.config`) and each object or
//! part carries only a filament slot number (`extruder` in
//! `Metadata/model_settings.config`); sending any filament sends the whole
//! project's settings, whose missing keys fall to the receiver's defaults,
//! not to its current selections. So:
//!
//! - **Geometry only** ([`Delivery::Geometry`]): when nothing is chosen
//!   (every body's material "None (slicer's own)", no printer, no process,
//!   no value of the design's, no filament change), the file carries the
//!   geometry, the placements, the core 3MF colours and each part's slot
//!   (slot 1: MitCad keeps no slot number of a body's own) in
//!   `model_settings.config` (PrusaSlicer: `Slic3r_PE_model.config`), and
//!   no project settings: no stand-in printer, process or filament, no
//!   machine G-code. The receiver keeps its printer, process and
//!   filaments.
//! - **Print project** ([`Delivery::Project`]): anything chosen sends the
//!   printer, process and materials chosen with their overrides, as
//!   before; [`readiness`] says before Send which parts come from the
//!   receiver's defaults (complete, partial or receiver-owned).
//!
//! A project cannot carry materials for some bodies and leave the others
//! to the slicer, so a print job's materials follow the rules of
//! [`choose`]: the first material chosen fills every other body without
//! one, one body cannot go back to None while others carry a material,
//! and Set to all gives every body the same choice (None for all is the
//! way back to geometry only).
//!
//! UltiMaker Cura gets no project stacks ([`capabilities`]): its export
//! carries geometry, each part's extruder number and its process values as
//! per-model settings; the printer and the materials stay Cura's own, and
//! a chosen one is listed as left out ([`readiness`]).

use std::fmt::Write as _;

use super::{ExportObject, ExportOutput, ExportPart, ExportPlate, Shared, xml_escape};
use crate::document::{EffectivePrint, PrintFrom};
use crate::print_keys::Dialect;

/// How a slicer's 3MF is delivered (mitcad#2008).
/// Ordered: a project above geometry (a file of several plates is a
/// project when any is one).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Delivery {
    /// The geometry alone: the receiver keeps its own printer, process
    /// and filaments.
    #[default]
    Geometry,
    /// A print project: the printer, process and materials chosen.
    Project,
}

impl Delivery {
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::Geometry => "geometry",
            Delivery::Project => "project",
        }
    }
}

/// Whether a part's settings choose anything a slicer's project carries:
/// a material, a printer or process profile, a value of the design's (a
/// property, a profile's or a plate's) or a slicer's own key set on the
/// design or a profile (a material's own keys count only with its
/// material chosen).
pub fn chooses(settings: &EffectivePrint) -> bool {
    settings.material_chosen
        || settings.printer.is_some()
        || settings.process.is_some()
        || !settings.values.is_empty()
        || settings
            .slicer
            .values()
            .flatten()
            .any(|(_, (_, from))| !matches!(from, PrintFrom::Material(_)))
}

/// The delivery of an export's parts: a print project when any part
/// chooses something ([`chooses`]) or a filament change at a print height
/// names a filament (`changes`, mitcad#1596), else geometry only.
pub fn delivery_of(parts: &[ExportPart<'_>], changes: usize) -> Delivery {
    if changes > 0 || parts.iter().any(|p| chooses(p.settings)) {
        Delivery::Project
    } else {
        Delivery::Geometry
    }
}

/// The Bambu family's geometry-only files: `model_settings.config` with
/// each object's name and slot and its parts (name and slot, nothing
/// else), and the print plates with their objects; no project settings.
pub(super) fn bambu_geometry(
    shared: &Shared<'_>,
    model_name: &str,
    objects: &[ExportObject<'_>],
    plates: &[ExportPlate<'_>],
) -> ExportOutput {
    let every: Vec<usize> = (0..shared.parts.len()).collect();
    let listed: Vec<(&str, &[usize])> = if objects.is_empty() {
        vec![(model_name, every.as_slice())]
    } else {
        objects
            .iter()
            .map(|o| (o.name, o.parts.as_slice()))
            .collect()
    };
    let slot = |i: usize| shared.slots.get(i).map_or(1, |s| s + 1);
    let mut model = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n");
    for (index, (name, parts)) in listed.iter().enumerate() {
        let _ = writeln!(
            model,
            "  <object id=\"{}\">",
            mitcad_3mf::build_object_id(shared.parts.len(), index)
        );
        let _ = writeln!(
            model,
            "    <metadata key=\"name\" value=\"{}\"/>",
            xml_escape(name)
        );
        let _ = writeln!(
            model,
            "    <metadata key=\"extruder\" value=\"{}\"/>",
            parts.first().map_or(1, |&i| slot(i))
        );
        for &i in parts.iter() {
            let _ = writeln!(
                model,
                "    <part id=\"{}\" subtype=\"normal_part\">",
                mitcad_3mf::part_object_id(i)
            );
            let _ = writeln!(
                model,
                "      <metadata key=\"name\" value=\"{}\"/>",
                xml_escape(shared.parts[i].name)
            );
            let _ = writeln!(
                model,
                "      <metadata key=\"extruder\" value=\"{}\"/>",
                slot(i)
            );
            model.push_str("    </part>\n");
        }
        model.push_str("  </object>\n");
    }
    for (index, plate) in plates.iter().enumerate() {
        model.push_str("  <plate>\n");
        let _ = writeln!(
            model,
            "    <metadata key=\"plater_id\" value=\"{}\"/>",
            index + 1
        );
        let _ = writeln!(
            model,
            "    <metadata key=\"plater_name\" value=\"{}\"/>",
            xml_escape(plate.name)
        );
        model.push_str("    <metadata key=\"locked\" value=\"false\"/>\n");
        for &object in plate.objects.iter().filter(|&&o| o < listed.len()) {
            let id = mitcad_3mf::build_object_id(shared.parts.len(), object);
            model.push_str("    <model_instance>\n");
            let _ = writeln!(model, "      <metadata key=\"object_id\" value=\"{id}\"/>");
            model.push_str("      <metadata key=\"instance_id\" value=\"0\"/>\n");
            let _ = writeln!(
                model,
                "      <metadata key=\"identify_id\" value=\"{id}\"/>"
            );
            model.push_str("    </model_instance>\n");
        }
        model.push_str("  </plate>\n");
    }
    model.push_str("</config>\n");
    let application = match shared.dialect {
        Dialect::QidiStudio => format!("QIDIStudio-{}", super::bambu::VERSION),
        _ => format!("BambuStudio-{}", super::bambu::VERSION),
    };
    ExportOutput {
        application: Some(application),
        metadata: vec![("BambuStudio:3mfVersion".to_owned(), "1".to_owned())],
        namespaces: vec![(
            super::bambu::NAMESPACE.0.to_owned(),
            super::bambu::NAMESPACE.1.to_owned(),
        )],
        files: vec![(super::bambu::MODEL_SETTINGS.to_owned(), model.into_bytes())],
        part_metadata: vec![Vec::new(); shared.parts.len()],
        delivery: Delivery::Geometry,
        ..ExportOutput::default()
    }
}

/// PrusaSlicer's geometry-only files: `Slic3r_PE_model.config` with each
/// object's name, slot and one volume; no `Slic3r_PE.config`.
pub(super) fn prusa_geometry(shared: &Shared<'_>) -> ExportOutput {
    let mut model = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<config>\n");
    for (i, part) in shared.parts.iter().enumerate() {
        let _ = writeln!(
            model,
            " <object id=\"{}\" instances_count=\"{}\">",
            mitcad_3mf::part_object_id(i),
            part.placements.max(1)
        );
        let _ = writeln!(
            model,
            "  <metadata type=\"object\" key=\"name\" value=\"{}\"/>",
            xml_escape(part.name)
        );
        let _ = writeln!(
            model,
            "  <metadata type=\"object\" key=\"extruder\" value=\"{}\"/>",
            shared.slots.get(i).map_or(1, |s| s + 1)
        );
        let _ = writeln!(
            model,
            "  <volume firstid=\"0\" lastid=\"{}\">",
            part.triangles.max(1) - 1
        );
        let _ = writeln!(
            model,
            "   <metadata type=\"volume\" key=\"name\" value=\"{}\"/>",
            xml_escape(part.name)
        );
        model.push_str("   <metadata type=\"volume\" key=\"volume_type\" value=\"ModelPart\"/>\n");
        model.push_str("  </volume>\n </object>\n");
    }
    model.push_str("</config>\n");
    ExportOutput {
        metadata: vec![("slic3rpe:Version3mf".to_owned(), "1".to_owned())],
        namespaces: vec![(
            super::prusa::NAMESPACE.0.to_owned(),
            super::prusa::NAMESPACE.1.to_owned(),
        )],
        files: vec![(super::prusa::MODEL_SETTINGS.to_owned(), model.into_bytes())],
        part_metadata: vec![Vec::new(); shared.parts.len()],
        delivery: Delivery::Geometry,
        ..ExportOutput::default()
    }
}

// --- A print job's materials ------------------------------------------------

/// The refusal of setting one body back to None while others carry a
/// material, in plain words (`docs/ui-conventions.md`, "What a message
/// says": no names or ids).
pub const MIXED_NONE_MESSAGE: &str = "The slicer's project format cannot leave one body to the \
     slicer's own filament while it sends materials for the others. Choose a material for this \
     body, or choose None above the list and Set to all to send the geometry only.";

/// What a print job's bodies print with together: the material every body
/// has, else mixed (the job-level dropdown shows it as Mixed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Common {
    /// Every body the same: a material, or None ("None (slicer's own)").
    Same(Option<String>),
    /// Bodies differ.
    Mixed,
}

/// The common value of a job's materials (an empty job: None).
pub fn common(materials: &[Option<String>]) -> Common {
    match materials.split_first() {
        None => Common::Same(None),
        Some((first, rest)) if rest.iter().all(|m| m == first) => Common::Same(first.clone()),
        Some(_) => Common::Mixed,
    }
}

/// A job's materials as the design gives them (each body's own or its
/// component's, None without), made sendable: where some bodies have a
/// material and others none, the others take the first body's material,
/// as when the first material is chosen in the list ([`choose`]), so that
/// what is sent is what the list shows.
pub fn prefill(mut materials: Vec<Option<String>>) -> Vec<Option<String>> {
    if let Some(first) = materials.iter().flatten().next().cloned() {
        for m in materials.iter_mut().filter(|m| m.is_none()) {
            *m = Some(first.clone());
        }
    }
    materials
}

/// A choice in a job's material list (mitcad#2008): `body` one body (its
/// index) or None for Set to all, `choice` a material or None ("None
/// (slicer's own)"). Set to all gives every body the choice. A material
/// chosen for one body while every body is None goes to every body; with
/// materials already chosen only that body changes. One body set to None
/// while another carries a material is refused ([`MIXED_NONE_MESSAGE`]).
pub fn choose(
    materials: &[Option<String>],
    body: Option<usize>,
    choice: Option<&str>,
) -> Result<Vec<Option<String>>, String> {
    let choice = choice.map(str::to_owned);
    let Some(body) = body else {
        return Ok(vec![choice; materials.len()]);
    };
    if body >= materials.len() {
        return Err(format!(
            "the job has {} bodies, not {}",
            materials.len(),
            body + 1
        ));
    }
    let mut out = materials.to_vec();
    match &choice {
        None => {
            let others = materials
                .iter()
                .enumerate()
                .any(|(i, m)| i != body && m.is_some());
            if others {
                return Err(MIXED_NONE_MESSAGE.to_owned());
            }
            out[body] = None;
        }
        Some(_) if materials.iter().all(Option::is_none) => {
            out = vec![choice; materials.len()];
        }
        Some(_) => out[body] = choice,
    }
    Ok(out)
}

/// Whether a job's materials can be sent: all None (geometry only) or all
/// chosen; mixed, the refusal in plain words.
pub fn check_job(materials: &[Option<String>]) -> Result<(), String> {
    let chosen = materials.iter().filter(|m| m.is_some()).count();
    if chosen == 0 || chosen == materials.len() {
        Ok(())
    } else {
        Err(MIXED_NONE_MESSAGE.to_owned())
    }
}

// --- What a destination takes and what is sent --------------------------------

/// What a slicer's export carries (mitcad#2008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// A print project with the printer, process and materials (the
    /// Bambu family, PrusaSlicer); else only geometry, each part's
    /// extruder slot and its process values as per-object settings.
    pub project: bool,
    /// Whether a printer and materials chosen are sent; when not, the
    /// panel disables their choices with [`Capabilities::note`].
    pub printer: bool,
    pub materials: bool,
    /// What is sent, as names: `geometry`, `extruder_slots`,
    /// `process_values` (per object), and with a project `printer`,
    /// `process`, `materials`.
    pub sends: Vec<&'static str>,
    /// In words, why the printer and material choices are disabled.
    pub note: Option<&'static str>,
}

/// The line a Cura destination shows by its disabled printer and material
/// choices.
pub const CURA_NOTE: &str = "Cura uses the printer and materials chosen in Cura: MitCad sends \
     the bodies, each body's extruder number and its process settings.";

/// What a slicer's export carries.
pub fn capabilities(dialect: Dialect) -> Capabilities {
    match dialect {
        Dialect::Cura => Capabilities {
            project: false,
            printer: false,
            materials: false,
            sends: vec!["geometry", "extruder_slots", "process_values"],
            note: Some(CURA_NOTE),
        },
        _ => Capabilities {
            project: true,
            printer: true,
            materials: true,
            sends: vec![
                "geometry",
                "extruder_slots",
                "process_values",
                "printer",
                "process",
                "materials",
            ],
            note: None,
        },
    }
}

/// How much of a delivery's print settings the file carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Geometry only: the receiver's own printer, process and filaments.
    Receiver,
    /// A project with a printer, a process and every material chosen.
    Complete,
    /// A project of which some parts fall to the receiver's defaults, or
    /// a destination that leaves chosen settings out.
    Partial,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Receiver => "receiver",
            Status::Complete => "complete",
            Status::Partial => "partial",
        }
    }
}

/// What a delivery sends, before Send (mitcad#2008): the delivery, its
/// status and, in plain words, what comes from the receiver's defaults
/// (`notes`) and which chosen settings the destination leaves out
/// (`left_out`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub delivery: Delivery,
    pub status: Status,
    pub notes: Vec<String>,
    pub left_out: Vec<String>,
}

/// The readiness of a job whose bodies print with `settings` (one each)
/// to a slicer.
pub fn readiness(dialect: Dialect, settings: &[&EffectivePrint]) -> Readiness {
    let project = settings.iter().any(|s| chooses(s));
    let delivery = if project {
        Delivery::Project
    } else {
        Delivery::Geometry
    };
    let mut notes = Vec::new();
    let mut left_out = Vec::new();
    if !project {
        notes.push(
            "Geometry only: the slicer keeps its own printer, process and filaments.".to_owned(),
        );
        return Readiness {
            delivery,
            status: Status::Receiver,
            notes,
            left_out,
        };
    }
    let printer = settings.iter().any(|s| s.printer.is_some());
    let process = settings.iter().any(|s| s.process.is_some());
    let materials = settings.iter().filter(|s| s.material_chosen).count();
    if dialect == Dialect::Cura {
        if printer {
            left_out.push(
                "The printer chosen is not sent: Cura prints with the printer chosen in Cura."
                    .to_owned(),
            );
        }
        if materials > 0 {
            left_out.push(
                "The materials chosen are not sent: each body gets its extruder number, and \
                 Cura prints with the materials loaded there."
                    .to_owned(),
            );
        }
        let process_left = settings.iter().any(|s| {
            s.values.keys().any(|k| {
                crate::print_keys::key(k).is_some_and(|d| {
                    d.set == crate::print_keys::PrintSet::Process && d.dialect(dialect).is_none()
                })
            })
        });
        if process_left {
            left_out.push(
                "Some process settings have no per-model setting in Cura and are not sent."
                    .to_owned(),
            );
        }
        let printer_left = settings.iter().any(|s| {
            s.values.keys().any(|k| {
                crate::print_keys::key(k)
                    .is_some_and(|d| d.set == crate::print_keys::PrintSet::Printer)
            })
        });
        if printer_left && !printer {
            left_out.push(
                "The printer settings are not sent: Cura prints with the printer chosen in Cura."
                    .to_owned(),
            );
        }
        let status = if left_out.is_empty() {
            Status::Receiver
        } else {
            Status::Partial
        };
        return Readiness {
            delivery,
            status,
            notes: vec![CURA_NOTE.to_owned()],
            left_out,
        };
    }
    if !printer {
        notes.push(
            "No printer is chosen: the slicer gets a stand-in printer and fills its settings \
             with its defaults, not with the printer chosen there."
                .to_owned(),
        );
    } else if settings.iter().any(|s| {
        s.printer.is_some()
            && !s.raw.contains_key(dialect.id())
            && s.dialect_text("printer_settings_id", dialect).is_none()
    }) {
        notes.push(
            "The printer has only MitCad's own settings: the slicer fills the others with its \
             defaults."
                .to_owned(),
        );
    }
    if !process {
        notes.push(
            "No process is chosen: the slicer fills the process settings with its defaults."
                .to_owned(),
        );
    }
    if materials < settings.len() {
        notes
            .push("Some bodies have no material: they print with MitCad's generic PLA.".to_owned());
    }
    let status = if notes.is_empty() {
        Status::Complete
    } else {
        Status::Partial
    };
    Readiness {
        delivery,
        status,
        notes,
        left_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(id: &str) -> Option<String> {
        Some(id.to_owned())
    }

    #[test]
    fn the_first_material_fills_every_body_without_one() {
        let job = vec![None, None, None];
        let out = choose(&job, Some(1), Some("petg")).unwrap();
        assert_eq!(out, vec![some("petg"), some("petg"), some("petg")]);
        // Later choices change only their body.
        let out = choose(&out, Some(2), Some("pla")).unwrap();
        assert_eq!(out, vec![some("petg"), some("petg"), some("pla")]);
        assert_eq!(common(&out), Common::Mixed);
    }

    #[test]
    fn one_body_back_to_none_is_refused_in_words() {
        let job = vec![some("pla"), some("pla")];
        let error = choose(&job, Some(0), None).unwrap_err();
        assert_eq!(error, MIXED_NONE_MESSAGE);
        // No names or ids in the text.
        assert!(!error.contains("pla") && !error.contains("Body"), "{error}");
        // A job of one body may go back to None.
        assert_eq!(choose(&[some("pla")], Some(0), None).unwrap(), vec![None]);
    }

    #[test]
    fn set_to_all_gives_every_body_the_choice_none_too() {
        let job = vec![some("pla"), some("petg")];
        assert_eq!(
            choose(&job, None, Some("abs")).unwrap(),
            vec![some("abs"), some("abs")]
        );
        let none = choose(&job, None, None).unwrap();
        assert_eq!(none, vec![None, None]);
        assert_eq!(common(&none), Common::Same(None));
        assert!(check_job(&none).is_ok());
        assert_eq!(
            check_job(&[None, some("pla")]).unwrap_err(),
            MIXED_NONE_MESSAGE
        );
    }

    #[test]
    fn prefill_shows_what_is_sent() {
        assert_eq!(prefill(vec![None, None]), vec![None, None]);
        assert_eq!(
            prefill(vec![None, some("petg"), some("pla")]),
            vec![some("petg"), some("petg"), some("pla")]
        );
    }

    #[test]
    fn nothing_chosen_is_geometry_only_and_receiver_owned() {
        let none = EffectivePrint::default();
        assert!(!chooses(&none));
        let ready = readiness(Dialect::BambuStudio, &[&none]);
        assert_eq!(ready.delivery, Delivery::Geometry);
        assert_eq!(ready.status, Status::Receiver);
        let chosen = EffectivePrint {
            material_chosen: true,
            ..EffectivePrint::default()
        };
        let ready = readiness(Dialect::BambuStudio, &[&chosen]);
        assert_eq!(ready.delivery, Delivery::Project);
        assert_eq!(ready.status, Status::Partial);
        assert_eq!(ready.notes.len(), 2, "{:?}", ready.notes);
    }

    #[test]
    fn cura_lists_the_chosen_printer_and_materials_as_left_out() {
        let caps = capabilities(Dialect::Cura);
        assert!(!caps.printer && !caps.materials && !caps.project);
        assert_eq!(caps.sends, ["geometry", "extruder_slots", "process_values"]);
        let chosen = EffectivePrint {
            material_chosen: true,
            printer: Some((
                "p".to_owned(),
                crate::properties::PropertyOwner::Component(crate::ids::ComponentUid::ROOT),
            )),
            ..EffectivePrint::default()
        };
        let ready = readiness(Dialect::Cura, &[&chosen]);
        assert_eq!(ready.status, Status::Partial);
        assert_eq!(ready.left_out.len(), 2, "{:?}", ready.left_out);
        assert!(capabilities(Dialect::BambuStudio).project);
    }
}
