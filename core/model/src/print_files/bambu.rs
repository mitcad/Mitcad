// SPDX-License-Identifier: MIT
//! The Bambu Studio family's files (Bambu Studio, OrcaSlicer, QIDI
//! Studio; mitcad#640): a 3MF project's `Metadata/project_settings.config`
//! (JSON, values as text, per-filament values as lists) with
//! `Metadata/model_settings.config` (objects, parts, their filament and
//! overrides), preset files (JSON) and exported bundles (zips of presets).
//! What the loader needs to take a 3MF's settings as its own is in the key
//! map's module documentation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use roxmltree::Document;
use serde_json::{Map, Value, json};

use super::source::{self, JsonSource, kept_value};
use super::{
    ExportOutput, Filled, Imported, Layers, Shared, defs_in, dialect_texts, fill_overrides,
    filled_warnings, read_flat, slot_values, xml_escape,
};
use crate::document::PrintFrom;
use crate::print_keys::{self, Dialect, Kind, PrintSet};

pub(super) const PROJECT_FILE: &str = "Metadata/project_settings.config";
pub(super) const MODEL_SETTINGS: &str = "Metadata/model_settings.config";
const LAYER_RANGES: &str = "Metadata/layer_config_ranges.xml";

/// The namespace of the family's metadata names.
pub(super) const NAMESPACE: (&str, &str) =
    ("BambuStudio", "http://schemas.bambulab.com/package/2021");

/// The version an export names: one the family's loaders take as a
/// project of theirs (see the key map's documentation).
pub(super) const VERSION: &str = "02.00.00.00";

/// Which member of the family wrote a project, by its `Application`
/// metadata; `given` when it does not say.
fn dialect_of(application: Option<&str>, given: Option<Dialect>) -> Dialect {
    let given = given.filter(|d| d.bambu_family());
    match application {
        Some(app) if app.starts_with("OrcaSlicer") => Dialect::OrcaSlicer,
        Some(app) if app.starts_with("QIDIStudio") => Dialect::QidiStudio,
        Some(app) if app.starts_with("BambuStudio") => given.unwrap_or(Dialect::BambuStudio),
        _ => given.unwrap_or(Dialect::BambuStudio),
    }
}

/// A list's entry `index` of a JSON value, as text.
fn entry(map: &Map<String, Value>, key: &str, index: usize) -> Option<String> {
    match map.get(key)? {
        Value::Array(items) => items.get(index).and_then(source::json_text),
        other => source::json_text(other),
    }
}

/// The number of filaments a project's settings have.
fn filament_count(map: &Map<String, Value>) -> usize {
    ["filament_colour", "filament_settings_id", "filament_type"]
        .iter()
        .find_map(|key| match map.get(*key) {
            Some(Value::Array(items)) if !items.is_empty() => Some(items.len()),
            _ => None,
        })
        .unwrap_or(1)
}

/// Reads a 3MF project's settings.
pub(super) fn read_project(
    package: &mitcad_3mf::Package,
    given: Option<Dialect>,
) -> Result<Imported, String> {
    let dialect = dialect_of(package.metadata("Application"), given);
    let data = package
        .file(PROJECT_FILE)
        .ok_or_else(|| format!("the 3MF has no {PROJECT_FILE}"))?;
    let map: Map<String, Value> = serde_json::from_slice(data)
        .map_err(|e| format!("{PROJECT_FILE} is not a settings file: {e}"))?;
    let mut out = Imported {
        dialect: Some(dialect),
        ..Imported::default()
    };
    let names = (
        entry(&map, "printer_settings_id", 0),
        entry(&map, "print_settings_id", 0),
    );
    read_flat(
        dialect,
        &JsonSource(&map),
        filament_count(&map),
        Layers::ALL,
        names,
        &mut out,
    );
    // The purge table (mitcad#705): the slicer multiplies it.
    if let Some(Value::Array(items)) = map.get(FLUSH_MATRIX) {
        let n = filament_count(&map);
        let multiplier = entry(&map, MULTIPLIER, 0)
            .and_then(|t| print_keys::parse_number(&t).ok())
            .filter(|m| *m > 0.0)
            .unwrap_or(1.0);
        let texts: Vec<String> = items
            .iter()
            .map(|v| source::json_text(v).unwrap_or_default())
            .collect();
        let pairs = super::table_numbers(texts.iter().map(String::as_str))
            .and_then(|entries| crate::print_purge::table_pairs(&entries, n, multiplier));
        match pairs {
            Some(pairs) => out.purge = pairs,
            None => out.report.left_out.push(format!(
                "the purge volumes (the table is not one of {n} filaments)"
            )),
        }
    }
    // The printer's whole block, written back to this slicer only: its
    // keys of the map's printer layer and the kept ones of the printer,
    // with the preset it inherits from and the keys it says differ.
    let mut raw = printer_block(&map, dialect, false);
    for (field, key) in [
        ("inherits_group", "@inherits"),
        ("different_settings_to_system", "@different"),
    ] {
        if let Some(Value::Array(items)) = map.get(field)
            && let Some(last) = items.last().and_then(source::json_text)
        {
            raw.insert(key.to_owned(), json!(last));
        }
    }
    // The project's filaments by their presets, in its order: an export
    // finds each written filament's place in the lists across filaments
    // the project keeps (the flushing volumes between filaments).
    if let Some(Value::Array(items)) = map.get("filament_settings_id") {
        let names: Vec<Value> = items
            .iter()
            .map(|v| json!(source::json_text(v).unwrap_or_default()))
            .collect();
        raw.insert(FILAMENTS.to_owned(), Value::Array(names));
    }
    if let Some(printer) = out.printers.first_mut()
        && !raw.is_empty()
    {
        printer.raw.insert(dialect.id().to_owned(), raw);
    }
    // What the project has that does not target Mitcad's geometry.
    if let Some(settings) = package.file(MODEL_SETTINGS)
        && let Ok(text) = std::str::from_utf8(settings)
        && let Ok(document) = Document::parse(text)
    {
        let own = |key: &str| {
            !matches!(
                key,
                "name"
                    | "extruder"
                    | "matrix"
                    | "source_file"
                    | "source_object_id"
                    | "source_volume_id"
                    | "source_offset_x"
                    | "source_offset_y"
                    | "source_offset_z"
                    | "mesh_shared"
                    | "module"
                    | "part_guid"
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
        let plates = document
            .descendants()
            .filter(|n| n.has_tag_name("plate"))
            .count();
        if plates > 0 {
            out.report
                .left_out
                .push(format!("{plates} plates (where objects lie)"));
        }
    }
    if package.file(LAYER_RANGES).is_some() {
        out.report
            .left_out
            .push("settings by height ranges of objects".to_owned());
    }
    Ok(out)
}

/// A printer's block as the file gives it, written back to this slicer
/// only: its keys of the map's printer layer and the kept ones of the
/// printer (`preset`: a printer preset's, whose every kept key is the
/// printer's; else those the layer of their name says), never bookkeeping.
/// The lists keep their shape (mitcad#2008: an entry per extruder or per
/// nozzle variant), which the map's one value of a key does not.
fn printer_block(
    map: &Map<String, Value>,
    dialect: Dialect,
    preset: bool,
) -> BTreeMap<String, Value> {
    let printer_keys = source::layer_keys(dialect, PrintSet::Printer);
    let mapped = source::mapped_keys(dialect);
    map.iter()
        .filter(|(key, _)| {
            printer_keys.contains(key.as_str())
                || (!mapped.contains(key.as_str())
                    && !source::is_bookkeeping(key)
                    && (preset || source::leftover_set(key) == PrintSet::Printer))
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// The layer a preset is of: its `type`, else the id key it has.
fn preset_layer(map: &Map<String, Value>) -> Option<PrintSet> {
    match map.get("type").and_then(Value::as_str) {
        Some("machine" | "printer") => return Some(PrintSet::Printer),
        Some("process" | "print") => return Some(PrintSet::Process),
        Some("filament") => return Some(PrintSet::Filament),
        _ => {}
    }
    if map.contains_key("printer_settings_id") {
        Some(PrintSet::Printer)
    } else if map.contains_key("print_settings_id") {
        Some(PrintSet::Process)
    } else if map.contains_key("filament_settings_id") {
        Some(PrintSet::Filament)
    } else {
        None
    }
}

/// Reads one preset into `out`.
fn read_preset_into(
    map: &mut Map<String, Value>,
    dialect: Dialect,
    out: &mut Imported,
) -> Result<(), String> {
    let set = preset_layer(map).ok_or_else(|| {
        "the preset says neither its type nor a printer, process or filament id".to_owned()
    })?;
    let name = map.get("name").and_then(Value::as_str).map(str::to_owned);
    // A filament preset is named by its name.
    if set == PrintSet::Filament
        && !map.contains_key("filament_settings_id")
        && let Some(name) = &name
    {
        map.insert("filament_settings_id".to_owned(), json!([name]));
    }
    let inherits = map
        .get("inherits")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .map(str::to_owned);
    if let Some(inherits) = &inherits {
        out.report.left_out.push(format!(
            "the values {} takes from its system preset {inherits} (only its own are in the file)",
            name.as_deref().unwrap_or("the preset")
        ));
    }
    // Templates it includes that were not filled in (mitcad#2008): the
    // values they give are not read, which the import says.
    let templates = source::include_names(map.get("include"));
    if !templates.is_empty() {
        out.report.left_out.push(format!(
            "the values {} takes from its templates {} (not found: only its own are in the file)",
            name.as_deref().unwrap_or("the preset"),
            templates.join(", ")
        ));
    }
    let notes = out.report.filaments.len();
    let printers = out.printers.len();
    read_flat(
        dialect,
        &JsonSource(&*map),
        1,
        Layers::only(set),
        (name.clone(), name),
        out,
    );
    // A printer keeps its block (mitcad#2008): its lists per extruder and
    // per nozzle variant as the preset gives them.
    if set == PrintSet::Printer
        && let Some(printer) = out.printers.get_mut(printers)
    {
        let raw = printer_block(map, dialect, true);
        if !raw.is_empty() {
            printer.raw.insert(dialect.id().to_owned(), raw);
        }
    }
    // A base not filled in (mitcad#1053): the chain stops there.
    for note in &mut out.report.filaments[notes..] {
        note.missing.clone_from(&inherits);
    }
    Ok(())
}

/// Reads a preset file (JSON).
pub(super) fn read_preset(text: &str, given: Option<Dialect>) -> Result<Imported, String> {
    let dialect = given
        .filter(|d| d.bambu_family())
        .unwrap_or(Dialect::BambuStudio);
    let mut map: Map<String, Value> =
        serde_json::from_str(text).map_err(|e| format!("not a preset file: {e}"))?;
    let mut out = Imported {
        dialect: Some(dialect),
        ..Imported::default()
    };
    read_preset_into(&mut map, dialect, &mut out)?;
    Ok(out)
}

/// Reads an exported bundle: every preset in it.
pub(super) fn read_bundle(
    package: &mitcad_3mf::Package,
    given: Option<Dialect>,
) -> Result<Imported, String> {
    let dialect = given
        .filter(|d| d.bambu_family())
        .unwrap_or(Dialect::BambuStudio);
    let mut out = Imported {
        dialect: Some(dialect),
        ..Imported::default()
    };
    let mut read = 0;
    for (path, data) in &package.files {
        let lower = path.to_ascii_lowercase();
        if !lower.ends_with(".json") || lower.ends_with("bundle_structure.json") {
            continue;
        }
        let Ok(mut map) = serde_json::from_slice::<Map<String, Value>>(data) else {
            out.report
                .problems
                .push(format!("{path} is not a preset file"));
            continue;
        };
        match read_preset_into(&mut map, dialect, &mut out) {
            Ok(()) => read += 1,
            Err(e) => out.report.problems.push(format!("{path}: {e}")),
        }
    }
    if read == 0 {
        return Err("the bundle has no presets".to_owned());
    }
    Ok(out)
}

/// A value of a list key: as many entries as the printer's own block has
/// (several extruders, or nozzle variants), else one. The map keeps one
/// value of the key, its first entry's (mitcad#2008): a block whose
/// entries differ keeps them while that value is unchanged, and one
/// changed in MitCad is written for every entry, which `warnings` says.
fn list_of(key: &str, existing: Option<&Value>, text: String, warnings: &mut Vec<String>) -> Value {
    let Some(Value::Array(items)) = existing.filter(|v| v.as_array().is_some_and(|a| a.len() > 1))
    else {
        return Value::Array(vec![json!(text)]);
    };
    let texts: Vec<Option<String>> = items.iter().map(source::json_text).collect();
    let same = |a: &str, b: &str| {
        a.trim() == b.trim()
            || matches!(
                (print_keys::parse_number(a), print_keys::parse_number(b)),
                (Ok(x), Ok(y)) if (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
            )
    };
    let differ = texts.iter().any(|t| {
        t.as_deref()
            .is_none_or(|t| !texts[0].as_deref().is_some_and(|f| same(f, t)))
    });
    if differ {
        if texts[0].as_deref().is_some_and(|first| same(first, &text)) {
            return Value::Array(items.clone());
        }
        warnings.push(format!(
            "{key}: the printer keeps a different value for each of its {} extruders or nozzle \
             variants, and the design changed it: written as {text} for every one",
            items.len()
        ));
    }
    Value::Array(vec![json!(text); items.len()])
}

/// The plate temperatures a slot falls back to when only the bed
/// temperature is set: every plate's from the bed temperature, the first
/// layer's from the first layer's bed temperature (else the bed
/// temperature).
fn plate_fallback(key: &str, filament: &BTreeMap<String, (Value, PrintFrom)>) -> Option<Value> {
    let get = |k: &str| filament.get(k).map(|(v, _)| v.clone());
    if key.ends_with("plate_temp_initial_layer") {
        get("first_layer_bed_temperature").or_else(|| get("bed_temperature"))
    } else if key.ends_with("plate_temp") {
        get("bed_temperature")
    } else {
        None
    }
}

/// Where an imported printer's raw block keeps the project's filaments (their
/// presets, in the project's order).
const FILAMENTS: &str = "@filaments";

/// Bambu Studio's filament of each entry of the filament lists (from 1).
const SELF_INDEX: &str = "filament_self_index";

/// The purge (flushing) volumes between filaments, a table of `n × n` per
/// extruder (row by row, from then to), two per filament (out, then in)
/// and the multiplier the slicer applies to the table (mitcad#705).
const FLUSH_MATRIX: &str = "flush_volumes_matrix";
const FLUSH_VECTOR: &str = "flush_volumes_vector";
const MULTIPLIER: &str = crate::print_purge::MULTIPLIER;

/// The project's filament each written slot came from (its place in the
/// project's lists), and the number of the project's filaments when the
/// printer's raw block says. A slot's material knows its place by its
/// `filament_self_index`, else by its preset among the project's; a slot
/// without either has none.
fn filament_sources(
    shared: &Shared<'_>,
    raw: Option<&BTreeMap<String, Value>>,
) -> (Vec<Option<usize>>, Option<usize>) {
    let dialect = shared.dialect;
    let names: Option<Vec<String>> = raw
        .and_then(|r| r.get(FILAMENTS))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|v| source::json_text(v).unwrap_or_default())
                .collect()
        });
    let count = names.as_ref().map(Vec::len).filter(|n| *n > 0);
    let sources = (0..shared.slot_settings.len())
        .map(|slot| {
            let settings = shared.slot_settings[slot];
            let own = settings
                .slicer
                .get(dialect.id())
                .and_then(|keys| keys.get(SELF_INDEX))
                .filter(|(_, from)| matches!(from, PrintFrom::Material(_)))
                .and_then(|(text, _)| text.trim().parse::<usize>().ok())
                .and_then(|i| i.checked_sub(1));
            let by_name = || {
                let name = shared
                    .filament(slot)
                    .get("filament_settings_id")
                    .and_then(|(v, _)| crate::print_profile::dialect_value(v, dialect))
                    .and_then(Value::as_str)?;
                names.as_ref()?.iter().position(|n| n == name)
            };
            own.or_else(by_name)
                .filter(|i| count.is_none_or(|n| *i < n))
        })
        .collect();
    (sources, count)
}

/// Writes every list across filaments that was not written per slot (the
/// kept text of a profile, the printer's raw block: of the imported
/// project's number of filaments) with one entry per written slot: each
/// slot's entry of its source filament, else the first entry.
/// `filament_self_index` numbers the slots; the purge volumes are the
/// design's ([`write_purge`]).
fn fit_to_filaments(
    config: &mut Map<String, Value>,
    per_slot: &BTreeSet<String>,
    dialect: Dialect,
    sources: &[Option<usize>],
    count: Option<usize>,
) {
    let slots = sources.len().max(1);
    let per_filament = source::per_filament_keys(dialect);
    let keys: Vec<String> = config.keys().cloned().collect();
    for key in keys {
        let Some(Value::Array(items)) = config.get(&key) else {
            continue;
        };
        if items.is_empty() {
            continue;
        }
        let fitted: Vec<Value> = match key.as_str() {
            SELF_INDEX => (1..=slots).map(|i| json!(i.to_string())).collect(),
            FLUSH_MATRIX | FLUSH_VECTOR => continue,
            _ if per_slot.contains(&key) || !per_filament(key.as_str()) => continue,
            _ => {
                // By the sources when the list is the project's; one
                // already of the slots stays.
                let by_source = match count {
                    Some(n) => items.len() == n,
                    None => items.len() != slots,
                };
                if !by_source && items.len() == slots {
                    continue;
                }
                (0..slots)
                    .map(|slot| {
                        let kept = if by_source {
                            source_of(sources, slot).and_then(|from| items.get(from))
                        } else {
                            None
                        };
                        kept.unwrap_or(&items[0]).clone()
                    })
                    .collect()
            }
        };
        config.insert(key, Value::Array(fitted));
    }
}

/// A slot's source filament.
fn source_of(sources: &[Option<usize>], slot: usize) -> Option<usize> {
    sources.get(slot).copied().flatten()
}

/// Writes the design's purge table of the slots (mitcad#705): the table
/// divided by the multiplier, which the slicer applies, and the
/// multiplier, each once per extruder; each slot's two values (out, then
/// in), unmultiplied. The extruders are the printer's `nozzle_diameter`
/// entries, or `preset` (those of the slicer's own preset of the printer,
/// mitcad#2207) when it has more: a printer named only by its preset
/// keeps one nozzle diameter, and the slicer loads the preset's
/// extruders. The family's loaders (read 21.11.2026: Bambu Studio,
/// OrcaSlicer and QIDI Studio alike) keep the multiplier as a list per
/// extruder and count the matrix's tables by its length
/// (`PresetBundle::update_multi_material_filament_presets`); a list
/// shorter than the extruders crashes Bambu Studio 2.9's slicing of a
/// two-extruder printer.
fn write_purge(
    config: &mut Map<String, Value>,
    purge: &crate::print_purge::Matrix,
    preset: Option<usize>,
) {
    if purge.slots.is_empty() {
        return;
    }
    let listed = match config.get("nozzle_diameter") {
        Some(Value::Array(items)) if !items.is_empty() => items.len(),
        _ => 1,
    };
    let extruders = listed.max(preset.unwrap_or(1));
    let m = purge.multiplier;
    let text = |x: f64| json!(print_keys::number_text((x * 1000.0).round() / 1000.0));
    let table: Vec<Value> = purge.volumes().into_iter().map(|v| text(v / m)).collect();
    let mut matrix = Vec::with_capacity(extruders * table.len());
    for _ in 0..extruders {
        matrix.extend(table.iter().cloned());
    }
    config.insert(FLUSH_MATRIX.to_owned(), Value::Array(matrix));
    config.insert(
        FLUSH_VECTOR.to_owned(),
        Value::Array(
            purge
                .slots
                .iter()
                .flat_map(|v| [text(v.out), text(v.into)])
                .collect(),
        ),
    );
    config.insert(
        MULTIPLIER.to_owned(),
        Value::Array(vec![text(m); extruders]),
    );
}

/// The family's prime (wipe) tower switch, a process setting of the whole
/// project.
const PRIME_TOWER: &str = "enable_prime_tower";

/// Whether the filaments beyond the first are only the manual filament
/// changes' (mitcad#1596): every plate's parts (the build's objects
/// `listed` on each of `plates`; without plates, every part) print with
/// one filament. A change on such a plate is the one-extruder printer's
/// swap of its spool, not a print of several filaments at once.
fn manual_changes_only(
    shared: &Shared<'_>,
    listed: &[(&str, &[usize])],
    plates: &[super::ExportPlate<'_>],
) -> bool {
    if plates.is_empty() {
        return one_filament(shared, 0..shared.parts.len());
    }
    plates.iter().all(|plate| {
        let parts = plate
            .objects
            .iter()
            .filter_map(|&o| listed.get(o))
            .flat_map(|(_, own)| own.iter().copied());
        one_filament(shared, parts)
    })
}

/// Whether parts (indices) print with one filament slot at most.
fn one_filament(shared: &Shared<'_>, parts: impl Iterator<Item = usize>) -> bool {
    parts
        .map(|i| shared.slots[i])
        .collect::<BTreeSet<usize>>()
        .len()
        <= 1
}

/// Turns the prime tower off for manual filament changes (mitcad#1596).
/// The family's slicers build a prime tower whenever a plate prints with
/// more than one filament, the slot a change adds included, at the
/// project's `wipe_tower_x` / `wipe_tower_y` (per plate; the slicers'
/// defaults are a corner of their own printers' beds, which a printer of
/// another size does not have: a tower outside its printable area stops
/// the slicing). A manual change needs no tower: the printer purges the
/// old filament in its own filament-change G-code (the swap of the
/// spool), the tower's other layers would only print waste, and its place
/// would have to follow the bed and every object on it. A design that
/// turned the tower on is told.
fn prime_tower_off(config: &mut Map<String, Value>, warnings: &mut Vec<String>) {
    let on = config
        .get(PRIME_TOWER)
        .and_then(source::json_text)
        .is_some_and(|t| matches!(t.trim(), "1" | "true"));
    if on {
        warnings.push(
            "the prime tower is written off: the filament changes at print heights are manual \
             changes of one extruder, which need none"
                .to_owned(),
        );
    }
    config.insert(PRIME_TOWER.to_owned(), json!("0"));
    differs_from_preset(config, 0, PRIME_TOWER);
}

/// The filament diameters, one per filament.
const DIAMETER: &str = "filament_diameter";

/// The diameter the family's slicers take for a filament that gives none,
/// mm (their default, and every generic filament's of MitCad's).
const DEFAULT_DIAMETER: &str = "1.75";

/// Writes `filament_diameter` with an entry per filament slot (mitcad#1596)
/// when no slot set it: OrcaSlicer (and the family) count a project's
/// filaments by that list (`ToolOrdering`: `num_filaments =
/// filament_diameter.size()`; `emit_custom_gcode_per_print_z`: a print
/// of one filament when it has one entry), and a project of one entry
/// takes a filament change at a height as a colour change, whose G-code
/// the family's export leaves out ("inserting color gcode is removed"):
/// the change went missing. A list of another length is fitted (its
/// first entry for the slots it lacks).
fn filament_count_written(config: &mut Map<String, Value>, slots: usize) {
    if slots < 2 {
        return;
    }
    let fitted: Vec<Value> = match config.get(DIAMETER) {
        Some(Value::Array(items)) if items.len() == slots => return,
        Some(Value::Array(items)) if !items.is_empty() => (0..slots)
            .map(|i| items.get(i).unwrap_or(&items[0]).clone())
            .collect(),
        Some(other) => match source::json_text(other) {
            Some(text) => vec![json!(text); slots],
            None => vec![json!(DEFAULT_DIAMETER); slots],
        },
        None => vec![json!(DEFAULT_DIAMETER); slots],
    };
    config.insert(DIAMETER.to_owned(), Value::Array(fitted));
}

/// The printer's G-code at a change of filament.
const CHANGE_GCODE: &str = "change_filament_gcode";

/// The filament change G-code the export writes for a printer without one
/// (mitcad#1596): M600, the filament change of Marlin and of PrusaSlicer's
/// default colour change, but not at the first filament of the print (the
/// family's slicers run this G-code there too, with `previous_extruder`
/// -1 in their `GCode::set_extruder`).
const MANUAL_CHANGE_GCODE: &str =
    "{if previous_extruder > -1}{if previous_extruder != next_extruder}M600{endif}{endif}";

/// The printer's filament change G-code for manual changes (mitcad#1596):
/// with two filaments in the project the family's slicers emit a change at
/// a height as a tool change, the printer's `change_filament_gcode` and
/// `T<n>`; a printer of one extruder that has no such G-code (a printer
/// of the design's, not the slicer's) would not stop for the new spool,
/// so the export writes [`MANUAL_CHANGE_GCODE`] and says so. A printer's
/// own G-code stays.
///
/// Only when the change G-code that counts is the file's (mitcad#2235,
/// the rule of [`reset_e_each_layer`], mitcad#2211): the printer names no
/// system preset of the export's own slicer (`own_preset` false: none, or
/// a name lent from another family member), or its block lists
/// `change_filament_gcode` in its `different_settings_to_system`. A preset
/// of the slicer's own gives the key from the installed preset (for a
/// printer of several extruders or a filament changer, its real tool
/// change), so it is left to it: not written, not listed, no warning.
fn manual_change_gcode(
    config: &mut Map<String, Value>,
    own_preset: bool,
    warnings: &mut Vec<String>,
) {
    if own_preset && !printer_lists(config, CHANGE_GCODE) {
        return;
    }
    let own = config
        .get(CHANGE_GCODE)
        .and_then(source::json_text)
        .is_some_and(|t| !t.trim().is_empty());
    if own {
        return;
    }
    config.insert(CHANGE_GCODE.to_owned(), json!(MANUAL_CHANGE_GCODE));
    differs_from_preset(config, usize::MAX, CHANGE_GCODE);
    warnings.push(
        "the printer has no filament change G-code: the filament changes at print heights are \
         written with M600 as its change_filament_gcode (the slicer also writes the tool \
         change T<n>, which a printer of one extruder may report as unknown)"
            .to_owned(),
    );
}

/// The family's relative extrusion switch and its G-code at each layer
/// change (before the move to the new layer, then after it).
const RELATIVE_E: &str = "use_relative_e_distances";
const BEFORE_LAYER_GCODE: &str = "before_layer_change_gcode";
const LAYER_GCODE: &str = "layer_change_gcode";

/// The line that resets the extruder's position.
const RESET_E: &str = "G92 E0";

/// Whether a G-code text has a line that resets the extruder's position,
/// as OrcaSlicer's and PrusaSlicer's check of a printer's layer G-code
/// reads one (`Print::validate`, the expression
/// `^[ \t]*[gG]92[ \t]*[eE](0(\.0*)?|\.0+)[ \t]*(;.*)?$` on each line):
/// `G92 E0`, `g92 e0.0`, `G92 E.0 ; reset`.
fn resets_e(gcode: &str) -> bool {
    let blank = |c: char| c == ' ' || c == '\t';
    let zeros = |text: &str| text.chars().all(|c| c == '0');
    gcode.lines().any(|line| {
        let code = line.split(';').next().unwrap_or("");
        let Some(rest) = code
            .trim_start_matches(blank)
            .strip_prefix(['G', 'g'])
            .and_then(|r| r.strip_prefix("92"))
        else {
            return false;
        };
        // A comment must follow blanks or the zero directly; `split`
        // removed it, so the zero ends the line but for blanks.
        let Some(zero) = rest
            .trim_start_matches(blank)
            .strip_prefix(['E', 'e'])
            .map(|z| z.trim_end_matches(blank))
        else {
            return false;
        };
        match zero.split_once('.') {
            None => zero == "0",
            Some(("0", fraction)) => zeros(fraction),
            Some(("", fraction)) => !fraction.is_empty() && zeros(fraction),
            Some(_) => false,
        }
    })
}

/// Makes relative extrusion valid (mitcad#1596): with
/// `use_relative_e_distances` on (the family's default when the printer
/// does not say), the slicers ask the printer's layer G-code to reset the
/// extruder's position at each layer, so that the position does not lose
/// floating point accuracy, and OrcaSlicer refuses a project without
/// (`Print::validate`: "Add G92 E0 to layer_gcode"). When neither the
/// before nor the after layer change G-code resets it, the export writes
/// `G92 E0` as the last line of `before_layer_change_gcode`, where the
/// family's own printer profiles keep it, and says relative extrusion is
/// on. Relative extrusion stays: the prime tower works only with it.
///
/// Only when the layer G-code that counts is the file's (mitcad#2211):
/// the printer names no system preset of the export's own slicer
/// (`own_preset` false: none, or a name lent from another family
/// member, which that slicer may not have, and then its built-in
/// defaults count), or its block lists `before_layer_change_gcode` or
/// `layer_change_gcode` in its `different_settings_to_system`. A preset
/// of the slicer's own gives every layer key not listed from the
/// installed preset, so both keys are left to it: neither written nor
/// listed (adding them made the slicer ask to confirm G-code the design
/// never set).
fn reset_e_each_layer(config: &mut Map<String, Value>, own_preset: bool) {
    if own_preset
        && !printer_lists(config, BEFORE_LAYER_GCODE)
        && !printer_lists(config, LAYER_GCODE)
    {
        return;
    }
    let relative = match config.get(RELATIVE_E) {
        None => true,
        Some(value) => source::json_text(value).is_some_and(|t| matches!(t.trim(), "1" | "true")),
    };
    if !relative {
        return;
    }
    let text_of = |settings: &Map<String, Value>, key: &str| {
        settings
            .get(key)
            .and_then(source::json_text)
            .unwrap_or_default()
    };
    if resets_e(&text_of(config, BEFORE_LAYER_GCODE)) || resets_e(&text_of(config, LAYER_GCODE)) {
        return;
    }
    if !matches!(
        config.get(BEFORE_LAYER_GCODE),
        None | Some(Value::String(_))
    ) {
        return; // not a text the export knows how to extend
    }
    let mut before = text_of(config, BEFORE_LAYER_GCODE);
    if !before.is_empty() && !before.ends_with('\n') {
        before.push('\n');
    }
    before.push_str(RESET_E);
    config.insert(BEFORE_LAYER_GCODE.to_owned(), json!(before));
    differs_from_preset(config, usize::MAX, BEFORE_LAYER_GCODE);
    if !config.contains_key(RELATIVE_E) {
        config.insert(RELATIVE_E.to_owned(), json!("1"));
        differs_from_preset(config, usize::MAX, RELATIVE_E);
    }
}

/// Whether the printer's entry of `different_settings_to_system` (the
/// last) lists `key`.
fn printer_lists(config: &Map<String, Value>, key: &str) -> bool {
    let Some(Value::Array(items)) = config.get("different_settings_to_system") else {
        return false;
    };
    items
        .last()
        .and_then(Value::as_str)
        .is_some_and(|keys| keys.split(';').any(|k| k == key))
}

/// Adds `key` to an entry of `different_settings_to_system` (`entry`: 0
/// the process's, `usize::MAX` the printer's, the last).
fn differs_from_preset(config: &mut Map<String, Value>, entry: usize, key: &str) {
    let Some(Value::Array(items)) = config.get_mut("different_settings_to_system") else {
        return;
    };
    let last = items.len().saturating_sub(1);
    let Some(Value::String(keys)) = items.get_mut(entry.min(last)) else {
        return;
    };
    if keys.split(';').any(|k| k == key) {
        return;
    }
    if !keys.is_empty() {
        keys.push(';');
    }
    keys.push_str(key);
}

/// The project's settings and the parts' filaments and overrides, by the
/// build's objects (`objects`, mitcad#856; none: one object named
/// `model_name` of every part).
pub(super) fn export(
    shared: &Shared<'_>,
    model_name: &str,
    objects: &[super::ExportObject<'_>],
    plates: &[super::ExportPlate<'_>],
) -> ExportOutput {
    let dialect = shared.dialect;
    let mut out = ExportOutput::default();
    let mut warnings = Vec::new();
    let mut config = Map::new();
    let first = shared.first();
    // The printer's own block, when the design keeps one for this slicer.
    let raw = first.and_then(|f| f.raw.get(dialect.id()));
    if let Some(raw) = raw {
        for (key, value) in raw {
            if !key.starts_with('@') {
                config.insert(key.clone(), value.clone());
            }
        }
    }
    let ids = [
        "printer_settings_id",
        "print_settings_id",
        "filament_settings_id",
    ];
    let mut printer_keys: Vec<String> = Vec::new();
    let mut process_keys: Vec<String> = Vec::new();
    for set in [PrintSet::Printer, PrintSet::Process] {
        for (def, value) in defs_in(set, &shared.values) {
            let Some(dk) = def.dialect(dialect) else {
                continue;
            };
            for (key, text) in dialect_texts(dialect, def, value, &shared.values, &mut warnings) {
                let value = if dk.list && key == dk.key {
                    list_of(key, config.get(key), text, &mut warnings)
                } else if def.kind == Kind::Points {
                    Value::Array(text.split(',').map(|p| json!(p)).collect())
                } else {
                    json!(text)
                };
                config.insert(key.to_owned(), value);
                if !ids.contains(&key) {
                    match set {
                        PrintSet::Printer => printer_keys.push(key.to_owned()),
                        _ => process_keys.push(key.to_owned()),
                    }
                }
            }
        }
    }
    // A list per filament key, an entry per slot; a key that no slot has
    // is left to the slicer's presets, one that some slots have is filled
    // for the others (mitcad#1052, `slot_values`), and the colour, the
    // type and the preset's name are always written.
    let slots = shared.slot_settings.len().max(1);
    let mut filament_keys: Vec<Vec<String>> = vec![Vec::new(); slots];
    // The keys written with an entry per slot.
    let mut per_slot_keys: BTreeSet<String> = BTreeSet::new();
    let mut filled = Filled::new();
    for def in print_keys::keys_in(PrintSet::Filament) {
        let Some(dk) = def.dialect(dialect) else {
            continue;
        };
        let values = slot_values(
            shared,
            def,
            |filament| {
                filament
                    .get(def.key)
                    .map(|(v, _)| v.clone())
                    .or_else(|| plate_fallback(def.key, filament))
            },
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
                dialect_texts(dialect, def, &value, &context, &mut warnings)
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
                    "filament_settings_id" => {
                        Some(shared.slot_settings[slot].material_name.clone())
                    }
                    _ => None,
                };
            }
        }
        // A slot that leaves an override to the printer: nil.
        let set = fill_overrides(def, &mut entries);
        if entries.iter().all(Option::is_some) {
            let entries: Vec<String> = entries.into_iter().flatten().collect();
            if def.key != "filament_settings_id" {
                for (keys, set) in filament_keys.iter_mut().zip(&set) {
                    if *set {
                        keys.push(dk.key.to_owned());
                    }
                }
            }
            config.insert(
                dk.key.to_owned(),
                Value::Array(entries.into_iter().map(Value::String).collect()),
            );
            per_slot_keys.insert(dk.key.to_owned());
        }
    }
    // The presets: the slicer's names the design keeps, else the
    // profiles'.
    let printer_preset = first.and_then(|f| f.dialect_text("printer_settings_id", dialect));
    let process_preset = first.and_then(|f| f.dialect_text("print_settings_id", dialect));
    match &printer_preset {
        Some(name) => {
            config.insert("printer_settings_id".to_owned(), json!(name));
        }
        None => {
            if !config.contains_key("printer_settings_id") {
                // No printer at all: a stand-in the slicer does not know,
                // which the export says (mitcad#1727).
                let name = match first.and_then(|f| f.printer_name.clone()) {
                    Some(name) => name,
                    None => {
                        warnings.push(super::NO_PRINTER_PROFILE.to_owned());
                        super::STAND_IN_PRINTER.to_owned()
                    }
                };
                config.insert("printer_settings_id".to_owned(), json!(name));
            }
        }
    }
    let process_name = process_preset
        .clone()
        .or_else(|| first.and_then(|f| f.process_name.clone()))
        .unwrap_or_else(|| "Mitcad process".to_owned());
    config.insert("print_settings_id".to_owned(), json!(process_name));
    // The presets each block inherits from, and the keys that differ.
    let filament_inherits: Vec<Value> = (0..slots)
        .map(|slot| {
            let filament = shared.filament(slot);
            json!(
                filament
                    .get("filament_settings_id")
                    .and_then(|(v, _)| crate::print_profile::dialect_value(v, dialect))
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        })
        .collect();
    let raw_inherits = raw
        .and_then(|r| r.get("@inherits"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    // Whether the printer's entry names a preset of this slicer's own
    // (mitcad#2211): the preset the kept block of this slicer inherits
    // from, else the profile's preset name for this slicer, not one lent
    // from another family member (`dialect_value`).
    let own_printer_preset = match &raw_inherits {
        Some(name) => !name.trim().is_empty(),
        None => first
            .and_then(|f| f.values.get("printer_settings_id"))
            .and_then(|(v, _)| v.as_object())
            .and_then(|names| names.get(dialect.id()))
            .and_then(Value::as_str)
            .is_some_and(|name| !name.trim().is_empty()),
    };
    let printer_inherits = raw_inherits.or(printer_preset).unwrap_or_default();
    let mut inherits = vec![json!(process_preset.unwrap_or_default())];
    inherits.extend(filament_inherits);
    inherits.push(json!(printer_inherits));
    config.insert("inherits_group".to_owned(), Value::Array(inherits));
    let mut printer_different: Vec<String> = raw
        .and_then(|r| r.get("@different"))
        .and_then(Value::as_str)
        .map(|t| {
            t.split(';')
                .filter(|k| !k.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for key in printer_keys {
        if !printer_different.contains(&key) {
            printer_different.push(key);
        }
    }
    let mut different = vec![json!(process_keys.join(";"))];
    different.extend(filament_keys.iter().map(|keys| json!(keys.join(";"))));
    different.push(json!(printer_different.join(";")));
    config.insert(
        "different_settings_to_system".to_owned(),
        Value::Array(different),
    );
    // The slicer's own keys: the profiles' and levels' as they are, the
    // materials' per slot (mitcad#2008: in the project's form, an entry
    // each).
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    if let Some(keys) = shared
        .slot_settings
        .first()
        .and_then(|s| s.slicer.get(dialect.id()))
    {
        for (key, (text, from)) in keys {
            if !matches!(from, PrintFrom::Material(_)) && !config.contains_key(key) {
                config.insert(key.clone(), kept_value(text));
            }
        }
    }
    material_keys(
        &mut config,
        shared,
        &mut per_slot_keys,
        &mut dropped,
        &mut warnings,
    );
    // Every other list across filaments (kept from the project the
    // settings came from, of its number of filaments) at one entry per
    // written filament.
    let (sources, count) = filament_sources(shared, raw);
    fit_to_filaments(&mut config, &per_slot_keys, dialect, &sources, count);
    // The slicers count the filaments by their diameters (mitcad#1596).
    filament_count_written(&mut config, slots);
    // The printer's extruders and nozzle variants (mitcad#2008).
    extruder_structure(&mut config, &mut warnings);
    // The design's purge volumes (mitcad#705).
    write_purge(&mut config, &shared.purge, shared.extruders);
    // The build's objects, each with its parts.
    let every: Vec<usize> = (0..shared.parts.len()).collect();
    let listed: Vec<(&str, &[usize])> = if objects.is_empty() {
        vec![(model_name, every.as_slice())]
    } else {
        objects
            .iter()
            .map(|o| (o.name, o.parts.as_slice()))
            .collect()
    };
    // Manual filament changes on plates of one filament (mitcad#1596): no
    // prime tower.
    if shared.layer_changes && manual_changes_only(shared, &listed, plates) {
        prime_tower_off(&mut config, &mut warnings);
        // Where the change G-code is the file's (mitcad#2235).
        manual_change_gcode(&mut config, own_printer_preset, &mut warnings);
    }
    // Relative extrusion resets E at each layer (mitcad#1596), where the
    // layer G-code is the file's (mitcad#2211).
    reset_e_each_layer(&mut config, own_printer_preset);
    // Only settings, each of the type the loader reads (mitcad#2008).
    project_types(&mut config, &mut dropped, &mut warnings);
    let mut project =
        serde_json::to_string_pretty(&Value::Object(config)).expect("settings serialize");
    project.push('\n');
    // Each object's parts: each part's filament and the values it does not
    // share.
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
            parts
                .first()
                .and_then(|&i| shared.slots.get(i))
                .map_or(1, |s| s + 1)
        );
        for &i in parts.iter() {
            write_part(&mut model, shared, i, !plates.is_empty(), &mut warnings);
        }
        model.push_str("  </object>\n");
    }
    // The print plates (mitcad#1050): each its objects and the plate
    // settings it keeps per plate.
    for (index, plate) in plates.iter().enumerate() {
        let here: Vec<usize> = plate
            .objects
            .iter()
            .filter(|&&o| o < listed.len())
            .copied()
            .collect();
        let parts_here: Vec<usize> = here
            .iter()
            .flat_map(|&o| listed[o].1.iter().copied())
            .collect();
        write_plate(
            &mut model,
            shared,
            index,
            plate,
            &here,
            &parts_here,
            &mut warnings,
        );
    }
    model.push_str("</config>\n");
    let application = match dialect {
        Dialect::QidiStudio => format!("QIDIStudio-{VERSION}"),
        _ => format!("BambuStudio-{VERSION}"),
    };
    out.application = Some(application);
    out.metadata = vec![("BambuStudio:3mfVersion".to_owned(), "1".to_owned())];
    out.namespaces = vec![(NAMESPACE.0.to_owned(), NAMESPACE.1.to_owned())];
    out.files = vec![
        (PROJECT_FILE.to_owned(), project.into_bytes()),
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

/// At most this many keys are named in one warning; the rest are counted.
const NAMED_KEYS: usize = 8;

/// Keys for a warning: the first [`NAMED_KEYS`], then how many more.
fn key_list<'a>(keys: impl IntoIterator<Item = &'a String>) -> String {
    let keys: Vec<&String> = keys.into_iter().collect();
    let mut text = keys
        .iter()
        .take(NAMED_KEYS)
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if keys.len() > NAMED_KEYS {
        let _ = write!(text, " and {} more", keys.len() - NAMED_KEYS);
    }
    text
}

/// A filament preset's nozzle variants of its lists (Bambu Studio's
/// `filament_extruder_variant`: an entry per variant the preset has values
/// for, each list of a variant's setting an entry per variant too).
const FILAMENT_VARIANTS: &str = "filament_extruder_variant";

/// The nozzle variant the printer prints with, as the filament presets
/// name it (`Direct Drive Standard`): its first extruder's type and its
/// nozzle's volume type; None when the printer does not say.
fn printer_variant(config: &Map<String, Value>) -> Option<String> {
    let first = |key: &str| match config.get(key)? {
        Value::Array(items) => items.first().and_then(source::json_text),
        other => source::json_text(other),
    };
    let extruder = first("extruder_type")?;
    let volume = first("nozzle_volume_type").or_else(|| first("default_nozzle_volume_type"))?;
    Some(format!("{} {}", extruder.trim(), volume.trim()))
}

/// The materials' kept keys of the slicer (mitcad#2008), each a list of an
/// entry per slot (`per_slot_keys` gets the keys written): under the
/// project's name of a preset key (a preset's `filament_id` as
/// `filament_ids`); never a preset's bookkeeping (`dropped` gets it) or
/// the printers and processes it is for. A key that only some slots' materials keep is
/// left to the slicer for all (a project's list has an entry for every
/// filament), and a slot's list of several values (the preset's values per
/// nozzle variant) gives the entry of the printer's variant, else its
/// first; both are warnings, never a silent loss.
fn material_keys(
    config: &mut Map<String, Value>,
    shared: &Shared<'_>,
    per_slot_keys: &mut BTreeSet<String>,
    dropped: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    let dialect = shared.dialect;
    let slots = shared.slot_settings.len().max(1);
    let mut per_slot: BTreeMap<String, Vec<Option<String>>> = BTreeMap::new();
    for (slot, settings) in shared.slot_settings.iter().enumerate() {
        let Some(keys) = settings.slicer.get(dialect.id()) else {
            continue;
        };
        for (key, (text, from)) in keys {
            if !matches!(from, PrintFrom::Material(_))
                || source::PRESET_APPLICABILITY.contains(&key.as_str())
            {
                continue;
            }
            if source::PRESET_METADATA.contains(&key.as_str()) {
                dropped.insert(key.clone());
                continue;
            }
            let project = source::project_key(key);
            let entry = per_slot
                .entry(project.to_owned())
                .or_insert_with(|| vec![None; slots]);
            // The project's own form over a preset's.
            if project == key || entry[slot].is_none() {
                entry[slot] = Some(text.clone());
            }
        }
    }
    // Each slot's place among its preset's nozzle variants: the printer's.
    let variant = printer_variant(config);
    let variants: Vec<Option<(usize, Option<usize>)>> = (0..slots)
        .map(|slot| {
            let text = per_slot.get(FILAMENT_VARIANTS)?.get(slot)?.as_deref()?;
            let Value::Array(items) = kept_value(text) else {
                return None;
            };
            let names: Vec<String> = items.iter().filter_map(source::json_text).collect();
            let pick = variant
                .as_deref()
                .and_then(|v| names.iter().position(|n| n.trim() == v));
            Some((names.len(), pick))
        })
        .collect();
    let mut partial: Vec<String> = Vec::new();
    let mut reduced: Vec<String> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for (key, entries) in per_slot {
        if config.contains_key(&key) {
            continue; // the map's, written per slot already
        }
        if !entries.iter().all(Option::is_some) {
            partial.push(key);
            continue;
        }
        let mut list = Vec::with_capacity(slots);
        let mut lossy = false;
        for (slot, text) in entries.into_iter().flatten().enumerate() {
            let items = match kept_value(&text) {
                Value::Array(items) => items,
                other => vec![other],
            };
            let texts: Option<Vec<String>> = items.iter().map(source::json_text).collect();
            let Some(texts) = texts.filter(|t| !t.is_empty()) else {
                break;
            };
            let entry = if texts.iter().all(|t| *t == texts[0]) {
                0
            } else {
                lossy = true;
                match variants[slot] {
                    Some((count, Some(pick))) if count == texts.len() => pick,
                    _ => 0,
                }
            };
            list.push(Value::String(texts[entry].clone()));
        }
        if list.len() != slots {
            unreadable.push(key);
            continue;
        }
        if lossy {
            reduced.push(key.clone());
        }
        per_slot_keys.insert(key.clone());
        config.insert(key, Value::Array(list));
    }
    if !partial.is_empty() {
        warnings.push(format!(
            "{}: kept by the materials of only some filament slots; the project has one list \
             for every filament, so the slicer's own presets give them for all",
            key_list(&partial)
        ));
    }
    if !reduced.is_empty() {
        warnings.push(format!(
            "{}: a material keeps a value for each nozzle variant, and the project one per \
             filament: written as the value of {}",
            key_list(&reduced),
            match &variant {
                Some(v) => format!("the printer's variant ({v}), else the first"),
                None => "the first variant".to_owned(),
            }
        ));
    }
    if !unreadable.is_empty() {
        warnings.push(format!(
            "{}: a material keeps a value the project cannot carry (an empty list or one of \
             lists): left out",
            key_list(&unreadable)
        ));
    }
}

/// Lists with an entry per physical extruder of the printer.
const EXTRUDER_SIZED: &[&str] = &[
    "nozzle_diameter",
    "extruder_type",
    "extruder_printable_area",
    "extruder_printable_height",
    "default_nozzle_volume_type",
    "nozzle_volume_type",
    "physical_extruder_map",
    "extruder_variant_list",
];

/// Lists with an entry per nozzle variant of the printer (a nozzle and
/// its volume on an extruder): `printer_extruder_id` names each one's
/// extruder (from 1).
const VARIANT_SIZED: &[&str] = &[
    "printer_extruder_id",
    "printer_extruder_variant",
    "nozzle_volume",
    "nozzle_flush_dataset",
];

/// The printer's extruders, nozzle variants and the filaments' extruders
/// as distinct lists (mitcad#2008), checked against each other. A design
/// keeps one nozzle diameter: a printer whose other lists per extruder
/// agree on more extruders gets it for each, which a warning says; a
/// printer without its nozzles' volume types gets its defaults. Lists per
/// extruder or per variant of different lengths, a variant or a filament
/// of an extruder the printer does not have: a warning that names them,
/// never a silent fix.
fn extruder_structure(config: &mut Map<String, Value>, warnings: &mut Vec<String>) {
    let len = |config: &Map<String, Value>, key: &str| match config.get(key) {
        Some(Value::Array(items)) => Some(items.len()),
        _ => None,
    };
    if !config.contains_key("nozzle_volume_type")
        && let Some(defaults @ Value::Array(_)) = config.get("default_nozzle_volume_type")
    {
        let defaults = defaults.clone();
        config.insert("nozzle_volume_type".to_owned(), defaults);
    }
    let others: BTreeSet<usize> = EXTRUDER_SIZED
        .iter()
        .filter(|k| **k != "nozzle_diameter")
        .filter_map(|k| len(config, k))
        .collect();
    if let (Some(1), [extruders]) = (
        len(config, "nozzle_diameter"),
        others.iter().copied().collect::<Vec<_>>().as_slice(),
    ) && *extruders > 1
        && let Some(Value::Array(items)) = config.get("nozzle_diameter")
    {
        let nozzle = items[0].clone();
        warnings.push(format!(
            "nozzle_diameter: the printer has {extruders} extruders and the design one nozzle \
             diameter, written for each of them"
        ));
        config.insert(
            "nozzle_diameter".to_owned(),
            Value::Array(vec![nozzle; *extruders]),
        );
    }
    let sizes: Vec<(&str, usize)> = EXTRUDER_SIZED
        .iter()
        .filter_map(|k| Some((*k, len(config, k)?)))
        .collect();
    let extruders = sizes.iter().map(|(_, n)| *n).max();
    if sizes.iter().any(|(_, n)| Some(*n) != extruders) {
        warnings.push(format!(
            "the printer's lists per extruder have different lengths ({}): the slicer may \
             misread its extruders; import the printer from the slicer again",
            sizes
                .iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let numbers = |config: &Map<String, Value>, key: &str| -> Option<Vec<Option<usize>>> {
        match config.get(key)? {
            Value::Array(items) => Some(
                items
                    .iter()
                    .map(|v| source::json_text(v)?.trim().parse::<usize>().ok())
                    .collect(),
            ),
            _ => None,
        }
    };
    let beyond = |ids: &[Option<usize>]| {
        extruders.is_some_and(|n| ids.iter().any(|id| id.is_none_or(|i| i == 0 || i > n)))
    };
    if let Some(ids) = numbers(config, "printer_extruder_id") {
        let variants: Vec<(&str, usize)> = VARIANT_SIZED
            .iter()
            .filter_map(|k| Some((*k, len(config, k)?)))
            .collect();
        if variants.iter().any(|(_, n)| *n != ids.len()) {
            warnings.push(format!(
                "the printer's lists per nozzle variant have different lengths ({}): the slicer \
                 may misread its nozzles",
                variants
                    .iter()
                    .map(|(k, n)| format!("{k} {n}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if beyond(&ids) {
            warnings.push(
                "printer_extruder_id: a nozzle variant is of an extruder the printer does not \
                 have"
                    .to_owned(),
            );
        }
    }
    if let Some(map) = numbers(config, "filament_map")
        && beyond(&map)
    {
        warnings.push(
            "filament_map: a filament is assigned to an extruder the printer does not have"
                .to_owned(),
        );
    }
}

/// Lists of a project however the design kept them: a text is one entry.
const PROJECT_LISTS: &[&str] = &[
    "filament_settings_id",
    "filament_ids",
    "inherits_group",
    "different_settings_to_system",
];

/// Makes the project's settings of the types the family's loader reads
/// (mitcad#2008): every value a text or a list of texts (numbers and
/// yes/no as their text), the project's lists ([`PROJECT_LISTS`]) lists,
/// and none of a preset's bookkeeping ([`source::PRESET_METADATA`]: the
/// loader reads each as one text, and an array there stops its load of
/// the whole project), which a design imported before may keep; a
/// preset's key with a project form of its own written in that form. What
/// is left out is a warning.
fn project_types(
    config: &mut Map<String, Value>,
    dropped: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
) {
    let keys: Vec<String> = config.keys().cloned().collect();
    let mut invalid: Vec<String> = Vec::new();
    for key in keys {
        if source::PRESET_METADATA.contains(&key.as_str()) {
            config.remove(&key);
            dropped.insert(key);
            continue;
        }
        let project = source::project_key(&key);
        if project != key {
            let value = config.remove(&key).expect("a key of the config");
            if !config.contains_key(project) {
                config.insert(project.to_owned(), value);
            }
            continue;
        }
        let Some(value) = config.get_mut(&key) else {
            continue;
        };
        let fixed = match value {
            Value::String(_) => Some(None),
            Value::Number(_) | Value::Bool(_) => source::json_text(value).map(|t| Some(json!(t))),
            Value::Array(items) => items
                .iter()
                .map(|item| source::json_text(item).map(Value::String))
                .collect::<Option<Vec<Value>>>()
                .map(|items| Some(Value::Array(items))),
            Value::Null | Value::Object(_) => None,
        };
        match fixed {
            Some(Some(text)) => *value = text,
            Some(None) => {}
            None => {
                config.remove(&key);
                invalid.push(key);
            }
        }
    }
    for key in PROJECT_LISTS {
        if let Some(text @ Value::String(_)) = config.get(*key) {
            let list = Value::Array(vec![text.clone()]);
            config.insert((*key).to_owned(), list);
        }
    }
    if !invalid.is_empty() {
        warnings.push(format!(
            "{}: not a text or a list of texts, which the slicer reads: left out",
            key_list(&invalid)
        ));
    }
    if !dropped.is_empty() {
        let mut text = format!(
            "{}: the slicer preset's own bookkeeping, not a setting of a project: left out",
            key_list(dropped.iter())
        );
        if dropped.contains("include") {
            text.push_str(
                "; the design was imported before the preset's templates (include) were read, \
                 so its machine G-code and other template values may be its base preset's: \
                 import the preset from the slicer again",
            );
        }
        warnings.push(text);
    }
}

/// A print plate in `model_settings.config` (mitcad#1050), as the family
/// writes one: its number (`plater_id`, from 1), name and lock, the plate
/// settings it keeps per plate that its parts (`parts`) print with and
/// the project does not set for every part (`spiral_mode` as `true` or
/// `false`, `print_sequence` by its name), and a `model_instance` per
/// object on it (`objects`, by their ids). Parts of one plate that differ
/// in such a key are a warning: the first part's value is written.
fn write_plate(
    model: &mut String,
    shared: &Shared<'_>,
    index: usize,
    plate: &super::ExportPlate<'_>,
    objects: &[usize],
    parts: &[usize],
    warnings: &mut Vec<String>,
) {
    let dialect = shared.dialect;
    let meta = |model: &mut String, key: &str, value: &str| {
        let _ = writeln!(
            model,
            "    <metadata key=\"{}\" value=\"{}\"/>",
            xml_escape(key),
            xml_escape(value)
        );
    };
    model.push_str("  <plate>\n");
    meta(model, "plater_id", &(index + 1).to_string());
    meta(model, "plater_name", plate.name);
    meta(model, "locked", "false");
    for key in crate::print_plates::BAMBU_PLATE_KEYS {
        let Some(def) = print_keys::key(key) else {
            continue;
        };
        let Some(dk) = def.dialect(dialect) else {
            continue;
        };
        if shared.values.contains_key(key) {
            continue; // the project's, every part's alike
        }
        let values: Vec<&Value> = parts
            .iter()
            .filter_map(|&i| shared.parts[i].settings.values.get(key).map(|(v, _)| v))
            .collect();
        let Some(first) = values.first().copied() else {
            continue;
        };
        if values.len() != parts.len() || values.iter().any(|v| *v != first) {
            warnings.push(format!(
                "{key}: the parts on {} print with different values, and {} keeps one per \
                 plate: written as the first part's",
                plate.name,
                dialect.name()
            ));
        }
        let text = if def.kind == Kind::Boolean && dk.key == "spiral_mode" {
            first.as_bool().unwrap_or(false).to_string()
        } else {
            let context: BTreeMap<String, Value> = BTreeMap::new();
            match dialect_texts(dialect, def, first, &context, warnings)
                .into_iter()
                .find(|(k, _)| *k == dk.key)
            {
                Some((_, text)) => text,
                None => continue,
            }
        };
        meta(model, dk.key, &text);
    }
    for &object in objects {
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

/// A part of an object in `model_settings.config`: its filament and the
/// values it does not share; on print plates (`on_plates`, mitcad#1050)
/// not those the family keeps per plate.
fn write_part(
    model: &mut String,
    shared: &Shared<'_>,
    i: usize,
    on_plates: bool,
    warnings: &mut Vec<String>,
) {
    let part = &shared.parts[i];
    let _ = writeln!(
        model,
        "    <part id=\"{}\" subtype=\"normal_part\">",
        mitcad_3mf::part_object_id(i)
    );
    let _ = writeln!(
        model,
        "      <metadata key=\"name\" value=\"{}\"/>",
        xml_escape(part.name)
    );
    let _ = writeln!(
        model,
        "      <metadata key=\"extruder\" value=\"{}\"/>",
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
            if on_plates && crate::print_plates::BAMBU_PLATE_KEYS.contains(&def.key) {
                continue;
            }
            for (key, text) in dialect_texts(shared.dialect, def, value, &context, warnings) {
                let _ = writeln!(
                    model,
                    "      <metadata key=\"{}\" value=\"{}\"/>",
                    xml_escape(key),
                    xml_escape(&text)
                );
            }
        }
    }
    model.push_str("    </part>\n");
}

#[cfg(any(test, feature = "testing"))]
pub fn version() -> &'static str {
    VERSION
}
