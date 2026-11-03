// SPDX-License-Identifier: MIT
//! PrusaSlicer's files (mitcad#640): a 3MF project's
//! `Metadata/Slic3r_PE.config` (`; key = value` lines, per-extruder values
//! as lists) with `Metadata/Slic3r_PE_model.config` (each object's own
//! settings and its volumes), configurations (`key = value`) and config
//! bundles (`[print:...]`, `[filament:...]`, `[printer:...]` sections).
//!
//! PrusaSlicer makes each mesh object of a 3MF an object of its own, so an
//! export gives each body its filament and its own values as its object's
//! settings, and the printer as many extruders as there are filament
//! slots.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use roxmltree::Document;
use serde_json::Value;

use super::source::IniSource;
use super::{
    ExportOutput, Filled, Imported, Layers, Shared, defs_in, dialect_texts, fill_overrides,
    filled_warnings, read_flat, slot_values, xml_escape,
};
use crate::document::PrintFrom;
use crate::print_keys::{self, Dialect, Kind, PrintSet};

pub(super) const PROJECT_FILE: &str = "Metadata/Slic3r_PE.config";
const MODEL_SETTINGS: &str = "Metadata/Slic3r_PE_model.config";
const LAYER_RANGES: &str = "Metadata/Prusa_Slicer_layer_config_ranges.xml";
/// The presets a project's process, filaments and printer inherit from
/// (PrusaSlicer's own spelling).
const INHERITS: &str = "inherits_cummulative";

/// The namespace of PrusaSlicer's metadata names.
const NAMESPACE: (&str, &str) = ("slic3rpe", "http://schemas.slic3r.org/3mf/2017/06");

const DIALECT: Dialect = Dialect::PrusaSlicer;

/// A line's key and value, `key = value` (a 3MF's `; key = value`).
fn key_value(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    let line = line.strip_prefix(';').map_or(line, str::trim);
    let (key, value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) || key.starts_with('#') {
        return None;
    }
    Some((key, value.trim()))
}

/// The `key = value` lines of a configuration; comments left out.
fn parse_lines(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(key_value)
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// The number of extruders (filaments) of a configuration.
fn extruders(map: &BTreeMap<String, String>) -> usize {
    ["filament_settings_id", "nozzle_diameter", "extruder_colour"]
        .iter()
        .find_map(|key| map.get(*key).map(|t| print_keys::split_list(t).len()))
        .filter(|n| *n > 0)
        .unwrap_or(1)
}

/// A non-list key's text.
fn single(map: &BTreeMap<String, String>, key: &str) -> Option<String> {
    let text = map.get(key)?;
    let entries = print_keys::split_list(text);
    entries.first().cloned().filter(|t| !t.trim().is_empty())
}

/// Reads a 3MF project's settings.
pub(super) fn read_project(package: &mitcad_3mf::Package) -> Result<Imported, String> {
    let data = package
        .file(PROJECT_FILE)
        .ok_or_else(|| format!("the 3MF has no {PROJECT_FILE}"))?;
    let text = std::str::from_utf8(data).map_err(|_| format!("{PROJECT_FILE} is not text"))?;
    let map = parse_lines(text);
    let mut out = Imported {
        dialect: Some(DIALECT),
        ..Imported::default()
    };
    read_flat(
        DIALECT,
        &IniSource(&map),
        extruders(&map),
        Layers::ALL,
        (
            single(&map, "printer_settings_id"),
            single(&map, "print_settings_id"),
        ),
        &mut out,
    );
    read_purge(&map, &mut out);
    if let Some(settings) = package.file(MODEL_SETTINGS)
        && let Ok(text) = std::str::from_utf8(settings)
        && let Ok(document) = Document::parse(text)
    {
        let own = |key: &str| {
            !matches!(
                key,
                "name"
                    | "extruder"
                    | "volume_type"
                    | "modifier"
                    | "matrix"
                    | "source_file"
                    | "source_object_id"
                    | "source_volume_id"
                    | "source_offset_x"
                    | "source_offset_y"
                    | "source_offset_z"
                    | "source_in_inches"
                    | "source_in_meters"
            )
        };
        let objects = document
            .descendants()
            .filter(|n| n.has_tag_name("object"))
            .filter(|object| {
                object
                    .descendants()
                    .filter(|n| n.has_tag_name("metadata"))
                    .any(|m| m.attribute("key").is_some_and(own))
            })
            .count();
        if objects > 0 {
            out.report.left_out.push(format!(
                "the per-object settings of {objects} objects (they do not target MitCad's bodies)"
            ));
        }
    }
    if package.file(LAYER_RANGES).is_some() {
        out.report
            .left_out
            .push("settings by height ranges of objects".to_owned());
    }
    Ok(out)
}

/// The purge table of a project (mitcad#705), as the slicer purges it:
/// read unless the project says its own table is not used.
fn read_purge(map: &BTreeMap<String, String>, out: &mut Imported) {
    let Some(text) = map.get(PURGE_MATRIX) else {
        return;
    };
    if map
        .get(PURGE_CUSTOM)
        .is_some_and(|on| matches!(print_keys::parse_bool(on), Ok(false)))
    {
        return;
    }
    let n = extruders(map);
    let entries = print_keys::split_list(text);
    let pairs = super::table_numbers(entries.iter().map(String::as_str))
        .and_then(|entries| crate::print_purge::table_pairs(&entries, n, 1.0));
    match pairs {
        Some(pairs) => out.purge = pairs,
        None => out.report.left_out.push(format!(
            "the purge volumes (the table is not one of {n} filaments)"
        )),
    }
}

/// PrusaSlicer's purge table (mitcad#705): `n × n` row by row (from, to),
/// whether it is used, and each extruder's two values (out, then in).
const PURGE_MATRIX: &str = "wiping_volumes_matrix";
const PURGE_CUSTOM: &str = "wiping_volumes_use_custom_matrix";
const PURGE_EXTRUDERS: &str = "wiping_volumes_extruders";

/// A section of a config bundle: `[print:0.20mm QUALITY]`.
struct Section {
    kind: String,
    name: String,
    values: BTreeMap<String, String>,
}

/// The values of a bundle's preset with what it inherits from presets of
/// the bundle (theirs first, its own over them).
fn resolved(sections: &[Section], index: usize, depth: usize) -> BTreeMap<String, String> {
    let section = &sections[index];
    let mut values = BTreeMap::new();
    if depth < 8
        && let Some(inherits) = section.values.get("inherits")
    {
        for parent in inherits.split(';') {
            let parent = parent.trim().trim_matches('"');
            if parent.is_empty() {
                continue;
            }
            if let Some(i) = sections
                .iter()
                .position(|s| s.kind == section.kind && s.name == parent)
            {
                values.extend(resolved(sections, i, depth + 1));
            }
        }
    }
    values.extend(section.values.clone());
    values
}

/// Reads a configuration: a config bundle's presets, or one configuration.
pub(super) fn read_config(text: &str) -> Result<Imported, String> {
    let mut out = Imported {
        dialect: Some(DIALECT),
        ..Imported::default()
    };
    let bundle = text
        .lines()
        .any(|l| l.trim_start().starts_with('[') && l.contains(':'));
    if !bundle {
        let map = parse_lines(text);
        if map.is_empty() {
            return Err("the file has no settings".to_owned());
        }
        read_flat(
            DIALECT,
            &IniSource(&map),
            extruders(&map),
            Layers::ALL,
            (
                single(&map, "printer_settings_id"),
                single(&map, "print_settings_id"),
            ),
            &mut out,
        );
        read_purge(&map, &mut out);
        return Ok(out);
    }
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<usize> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let header = &trimmed[1..trimmed.len() - 1];
            current = match header.split_once(':') {
                Some((kind, name)) if matches!(kind, "print" | "filament" | "printer") => {
                    sections.push(Section {
                        kind: kind.to_owned(),
                        name: name.trim().to_owned(),
                        values: BTreeMap::new(),
                    });
                    Some(sections.len() - 1)
                }
                _ => None,
            };
            continue;
        }
        if let (Some(i), Some((key, value))) = (current, key_value(line)) {
            sections[i].values.insert(key.to_owned(), value.to_owned());
        }
    }
    let mut abstract_presets = 0;
    for i in 0..sections.len() {
        let name = sections[i].name.clone();
        // A preset only others inherit from.
        if name.starts_with('*') && name.ends_with('*') {
            abstract_presets += 1;
            continue;
        }
        let mut values = resolved(&sections, i, 0);
        values.remove("inherits");
        let set = match sections[i].kind.as_str() {
            "printer" => PrintSet::Printer,
            "filament" => PrintSet::Filament,
            _ => PrintSet::Process,
        };
        if set == PrintSet::Filament {
            values.insert(
                "filament_settings_id".to_owned(),
                print_keys::join_list(std::slice::from_ref(&name), true),
            );
        }
        read_flat(
            DIALECT,
            &IniSource(&values),
            1,
            Layers::only(set),
            (Some(name.clone()), Some(name)),
            &mut out,
        );
    }
    if abstract_presets > 0 {
        out.report.left_out.push(format!(
            "{abstract_presets} presets that only others inherit from"
        ));
    }
    if out.printers.is_empty() && out.processes.is_empty() && out.materials.is_empty() {
        return Err("the bundle has no print, filament or printer presets".to_owned());
    }
    Ok(out)
}

/// Whether a key's list is one of strings (`;`-separated, quoted where an
/// entry needs it). PrusaSlicer's lists of numbers, percentages, yes/no
/// and of choices (enums, such as `cooling_slowdown_logic`) are
/// `,`-separated and never quoted: it refuses a quoted choice.
fn text_list(kind: Kind) -> bool {
    matches!(kind, Kind::Text | Kind::Color)
}

/// A kept key's entries as a list: numbers (and `nil`) `,`-separated,
/// anything else as strings.
fn join_any(entries: &[String]) -> String {
    let number = |e: &String| {
        let e = e.trim();
        e == "nil" || print_keys::parse_number(e.trim_end_matches('%')).is_ok()
    };
    print_keys::join_list(entries, !entries.iter().all(number))
}

/// The project's settings and each object's filament and own values.
/// The keys PrusaSlicer keeps for the whole project, never per object
/// (mitcad#1050): on print plates, a part's own value of one is left out
/// with a warning.
const PROJECT_ONLY: [&str; 2] = ["spiral_vase", "complete_objects"];

pub(super) fn export(shared: &Shared<'_>, on_plates: bool) -> ExportOutput {
    let mut out = ExportOutput::default();
    let mut warnings = Vec::new();
    let slots = shared.slot_parts.len().max(1);
    let mut lines: BTreeMap<String, String> = BTreeMap::new();
    for set in [PrintSet::Printer, PrintSet::Process] {
        for (def, value) in defs_in(set, &shared.values) {
            let Some(dk) = def.dialect(DIALECT) else {
                continue;
            };
            for (key, text) in dialect_texts(DIALECT, def, value, &shared.values, &mut warnings) {
                let text = if dk.list && key == dk.key {
                    // One per extruder: as many as the filament slots.
                    print_keys::join_list(&vec![text; slots], text_list(def.kind))
                } else {
                    text
                };
                lines.insert(key.to_owned(), text);
            }
        }
    }
    // A key some slots set is filled for the others (mitcad#1052).
    let mut filled = Filled::new();
    for def in print_keys::keys_in(PrintSet::Filament) {
        let Some(dk) = def.dialect(DIALECT) else {
            continue;
        };
        let values = slot_values(
            shared,
            def,
            |filament| filament.get(def.key).map(|(v, _)| v.clone()),
            &mut filled,
        );
        let mut entries: Vec<Option<String>> = values
            .into_iter()
            .enumerate()
            .map(|(slot, value)| {
                let value = value?;
                let filament = shared.filament(slot);
                let context: BTreeMap<String, Value> = filament
                    .iter()
                    .map(|(k, (v, _))| (k.clone(), v.clone()))
                    .collect();
                dialect_texts(DIALECT, def, &value, &context, &mut warnings)
                    .into_iter()
                    .find(|(k, _)| *k == dk.key)
                    .map(|(_, t)| t)
            })
            .collect();
        for (slot, entry) in entries.iter_mut().enumerate() {
            if entry.is_none() {
                *entry = match def.key {
                    "filament_colour" => Some("#FFFFFF".to_owned()),
                    "filament_type" => Some("PLA".to_owned()),
                    "filament_settings_id" => Some(
                        shared.parts[shared.slot_parts[slot]]
                            .settings
                            .material_name
                            .clone(),
                    ),
                    _ => None,
                };
            }
        }
        // A slot that leaves an override to the printer: nil.
        fill_overrides(def, &mut entries);
        if entries.iter().all(Option::is_some) {
            let entries: Vec<String> = entries.into_iter().flatten().collect();
            let text = if dk.list {
                print_keys::join_list(&entries, text_list(def.kind))
            } else {
                entries[0].clone()
            };
            lines.insert(dk.key.to_owned(), text);
        }
    }
    let first = shared.first();
    let preset = |key: &str, fallback: Option<String>, default: &str| {
        first
            .and_then(|f| f.dialect_text(key, DIALECT))
            .or(fallback)
            .unwrap_or_else(|| default.to_owned())
    };
    lines.insert(
        "printer_settings_id".to_owned(),
        preset(
            "printer_settings_id",
            first.and_then(|f| f.printer_name.clone()),
            "Mitcad printer",
        ),
    );
    lines.insert(
        "print_settings_id".to_owned(),
        preset(
            "print_settings_id",
            first.and_then(|f| f.process_name.clone()),
            "Mitcad process",
        ),
    );
    // The slicer's own keys: the profiles' and levels' as they are, the
    // materials' as a list of the slots'.
    let mut per_slot: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for slot in 0..slots {
        let settings = shared.parts[shared.slot_parts[slot]].settings;
        let Some(keys) = settings.slicer.get(DIALECT.id()) else {
            continue;
        };
        for (key, (text, from)) in keys {
            if matches!(from, PrintFrom::Material(_)) {
                per_slot
                    .entry(key.clone())
                    .or_insert_with(|| vec![None; slots])[slot] = Some(text.clone());
            } else if slot == 0 {
                lines.entry(key.clone()).or_insert_with(|| text.clone());
            }
        }
    }
    for (key, entries) in per_slot {
        if lines.contains_key(&key) || !entries.iter().all(Option::is_some) {
            continue;
        }
        let entries: Vec<String> = entries.into_iter().flatten().collect();
        let text = if slots == 1 {
            entries[0].clone()
        } else {
            join_any(&entries)
        };
        lines.insert(key, text);
    }
    // The presets each block inherits from (mitcad#1065): PrusaSlicer's
    // project config gathers the presets' `inherits` into
    // `inherits_cummulative` (sic), one entry each: the process, each
    // extruder's filament, then the printer (`PresetBundle::full_fff_config`,
    // split again by `load_config_file_config`), and leaves it out when all
    // are empty. A filament inherits from the material's PrusaSlicer preset,
    // the process and printer from the presets the design names for it.
    let mut inherits = vec![
        first
            .and_then(|f| f.dialect_text("print_settings_id", DIALECT))
            .unwrap_or_default(),
    ];
    inherits.extend((0..slots).map(|slot| {
        shared
            .filament(slot)
            .get("filament_settings_id")
            .and_then(|(v, _)| crate::print_profile::dialect_value(v, DIALECT))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    }));
    inherits.push(
        first
            .and_then(|f| f.dialect_text("printer_settings_id", DIALECT))
            .unwrap_or_default(),
    );
    if inherits.iter().any(|e| !e.is_empty()) {
        lines.insert(INHERITS.to_owned(), print_keys::join_list(&inherits, true));
    } else {
        lines.remove(INHERITS);
    }
    // The design's purge table (mitcad#705): PrusaSlicer has no multiplier,
    // so the table as it purges, and the slots' values multiplied.
    if !shared.purge.slots.is_empty() {
        let text = |x: f64| print_keys::number_text((x * 1000.0).round() / 1000.0);
        let table: Vec<String> = shared.purge.volumes().into_iter().map(text).collect();
        let m = shared.purge.multiplier;
        let pairs: Vec<String> = shared
            .purge
            .slots
            .iter()
            .flat_map(|v| [text(v.out * m), text(v.into * m)])
            .collect();
        lines.insert(
            PURGE_MATRIX.to_owned(),
            print_keys::join_list(&table, false),
        );
        lines.insert(
            PURGE_EXTRUDERS.to_owned(),
            print_keys::join_list(&pairs, false),
        );
        lines.insert(PURGE_CUSTOM.to_owned(), "1".to_owned());
    }
    // PrusaSlicer makes a wipe tower only with relative extruder addressing,
    // a machine setting the design's printer does not carry: several
    // filaments with the wipe tower on ask for it (mitcad#640).
    if slots > 1 && lines.get("wipe_tower").is_some_and(|on| on == "1") {
        lines
            .entry("use_relative_e_distances".to_owned())
            .or_insert_with(|| "1".to_owned());
    }
    // Several filaments through one nozzle (an AMS-style printer, as the
    // design's printer is: one nozzle, several slots): PrusaSlicer takes the
    // purge table only for such a printer (mitcad#705, checked with
    // PrusaSlicer 2.9). A project's own value (a tool changer's 0) stays.
    if slots > 1 {
        lines
            .entry("single_extruder_multi_material".to_owned())
            .or_insert_with(|| "1".to_owned());
    }
    let mut config = format!("; generated by MitCad {}\n\n", env!("CARGO_PKG_VERSION"));
    for (key, value) in &lines {
        let _ = writeln!(config, "; {key} = {value}");
    }
    // Each object: its filament and its own values, and its one volume.
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
            shared.slots[i] + 1
        );
        let context: BTreeMap<String, Value> = part
            .settings
            .values
            .iter()
            .map(|(k, (v, _))| (k.clone(), v.clone()))
            .collect();
        for set in [PrintSet::Printer, PrintSet::Process] {
            for (def, value) in defs_in(set, &shared.overrides[i]) {
                if on_plates && PROJECT_ONLY.contains(&def.key) {
                    warnings.push(format!(
                        "{}: {} keeps {} for the whole project, so {}'s own value is left out",
                        def.key,
                        DIALECT.name(),
                        def.name,
                        part.name
                    ));
                    continue;
                }
                for (key, text) in dialect_texts(DIALECT, def, value, &context, &mut warnings) {
                    let _ = writeln!(
                        model,
                        "  <metadata type=\"object\" key=\"{}\" value=\"{}\"/>",
                        xml_escape(key),
                        xml_escape(&text)
                    );
                }
            }
        }
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
    out.metadata = vec![("slic3rpe:Version3mf".to_owned(), "1".to_owned())];
    out.namespaces = vec![(NAMESPACE.0.to_owned(), NAMESPACE.1.to_owned())];
    out.files = vec![
        (PROJECT_FILE.to_owned(), config.into_bytes()),
        (MODEL_SETTINGS.to_owned(), model.into_bytes()),
    ];
    out.part_metadata = vec![Vec::new(); shared.parts.len()];
    // The slots that took values they do not set (mitcad#1052).
    warnings.extend(filled_warnings(shared, &filled));
    warnings.sort();
    warnings.dedup();
    out.warnings = warnings;
    out
}
