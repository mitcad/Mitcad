// SPDX-License-Identifier: MIT
//! UltiMaker Cura's files (mitcad#640): a 3MF project's stacks under
//! `Cura/` (INI: the global stack `*.global.cfg` and the extruder stacks
//! `*.extruder.cfg` list their containers, nearest first; the file carries
//! the instance containers `*.inst.cfg` with the user's, the profile's and
//! the machine's changed values, and the materials' own files
//! `*.xml.fdm_material`), and per-model settings (an object's `cura:`
//! metadata).
//!
//! An export gives each part its extruder and its process values as
//! per-model settings; the printer and the global settings stay the ones
//! chosen in Cura, where a project of its own would have to name Cura's
//! installed machine, quality and material containers.

use std::collections::BTreeMap;

use roxmltree::Document;
use serde_json::Value;

use super::source::CuraSource;
use super::{
    ExportOutput, Imported, Layers, Shared, defs_in, dialect_texts, hex_rgb, plastic_appearance,
    read_flat,
};
use crate::print_keys::{self, Dialect, PrintSet};

const DIALECT: Dialect = Dialect::Cura;

/// The namespace of Cura's metadata names.
const NAMESPACE: (&str, &str) = ("cura", "http://software.ultimaker.com/xml/cura/3mf/2015/10");

/// An INI file's sections: their keys and values (a value goes on in the
/// indented lines after it).
pub(super) fn parse_ini(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut section = String::new();
    let mut last: Option<String> = None;
    for line in text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with(['#', ';']) {
            continue;
        }
        if line.starts_with([' ', '\t'])
            && let Some(key) = &last
        {
            if let Some(value) = sections.get_mut(&section).and_then(|s| s.get_mut(key)) {
                value.push('\n');
                value.push_str(line.trim());
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            section = trimmed[1..trimmed.len() - 1].trim().to_owned();
            sections.entry(section.clone()).or_default();
            last = None;
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_owned();
            sections
                .entry(section.clone())
                .or_default()
                .insert(key.clone(), value.trim().to_owned());
            last = Some(key);
        }
    }
    sections
}

/// A stack's values: its containers' `[values]` the file carries, the
/// nearest first; with its containers' ids.
fn stack_values(
    files: &BTreeMap<String, &[u8]>,
    stack: &BTreeMap<String, BTreeMap<String, String>>,
    formulas: &mut usize,
) -> (BTreeMap<String, String>, Vec<String>) {
    let mut containers: Vec<(usize, String)> = stack
        .get("containers")
        .map(|c| {
            c.iter()
                .filter_map(|(k, v)| Some((k.parse().ok()?, v.clone())))
                .collect()
        })
        .unwrap_or_default();
    containers.sort();
    let mut values = BTreeMap::new();
    for (_, id) in &containers {
        let Some(data) = files.get(&format!("Cura/{id}.inst.cfg")) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(data) else {
            continue;
        };
        let container = parse_ini(text);
        for (key, value) in container.get("values").into_iter().flatten() {
            if value.trim_start().starts_with('=') {
                *formulas += 1;
                continue;
            }
            values.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    (values, containers.into_iter().map(|(_, id)| id).collect())
}

/// What a material's own file says: its type, maker, colour and density.
struct MaterialFile {
    material: Option<String>,
    brand: Option<String>,
    color: Option<String>,
    density: Option<f64>,
}

fn material_file(data: &[u8]) -> Option<MaterialFile> {
    let text = std::str::from_utf8(data).ok()?;
    let document = Document::parse(text).ok()?;
    let find = |name: &str| {
        document
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    };
    Some(MaterialFile {
        material: find("material"),
        brand: find("brand"),
        color: find("color_code"),
        density: find("density").and_then(|d| d.parse().ok()),
    })
}

/// Reads a 3MF project's stacks.
pub(super) fn read_project(package: &mitcad_3mf::Package) -> Result<Imported, String> {
    let files: BTreeMap<String, &[u8]> = package
        .files
        .iter()
        .filter(|(p, _)| p.starts_with("Cura/"))
        .map(|(p, d)| (p.clone(), d.as_slice()))
        .collect();
    let ini = |path: &str| -> Option<BTreeMap<String, BTreeMap<String, String>>> {
        Some(parse_ini(std::str::from_utf8(files.get(path)?).ok()?))
    };
    let global_path = files
        .keys()
        .find(|p| p.ends_with(".global.cfg"))
        .cloned()
        .ok_or_else(|| "the 3MF has no Cura global stack".to_owned())?;
    let global_stack = ini(&global_path).ok_or_else(|| format!("{global_path} is not text"))?;
    let mut formulas = 0;
    let (mut global, containers) = stack_values(&files, &global_stack, &mut formulas);
    // The machine's definition: the stack's last container.
    if let Some(definition) = containers.last() {
        global.insert("definition".to_owned(), definition.clone());
    }
    let machine = global_stack
        .get("general")
        .and_then(|g| g.get("name"))
        .cloned();
    // The profile's name: its quality changes container's.
    let profile = containers.iter().find_map(|id| {
        let container = ini(&format!("Cura/{id}.inst.cfg"))?;
        (container.get("metadata")?.get("type")? == "quality_changes")
            .then(|| container.get("general")?.get("name").cloned())
            .flatten()
    });
    // The extruders by their positions, each with its material's file.
    let mut extruders: Vec<(usize, BTreeMap<String, String>, Option<MaterialFile>)> = Vec::new();
    for path in files.keys().filter(|p| p.ends_with(".extruder.cfg")) {
        let Some(stack) = ini(path) else {
            continue;
        };
        let position = stack
            .get("metadata")
            .and_then(|m| m.get("position"))
            .and_then(|p| p.trim().parse().ok())
            .unwrap_or(extruders.len());
        let (mut values, containers) = stack_values(&files, &stack, &mut formulas);
        let material = containers
            .iter()
            .find_map(|id| material_file(files.get(&format!("Cura/{id}.xml.fdm_material"))?));
        if let Some(file) = &material {
            if let Some(kind) = &file.material {
                values
                    .entry("material_type".to_owned())
                    .or_insert_with(|| kind.clone());
            }
            if let Some(brand) = &file.brand {
                values
                    .entry("material_brand".to_owned())
                    .or_insert_with(|| brand.clone());
            }
        }
        extruders.push((position, values, material));
    }
    extruders.sort_by_key(|(position, ..)| *position);
    let materials: Vec<Option<MaterialFile>> =
        extruders.iter_mut().map(|(_, _, m)| m.take()).collect();
    let extruders: Vec<BTreeMap<String, String>> =
        extruders.into_iter().map(|(_, values, _)| values).collect();
    let mut out = Imported {
        dialect: Some(DIALECT),
        ..Imported::default()
    };
    read_flat(
        DIALECT,
        &CuraSource {
            global: &global,
            extruders: &extruders,
        },
        extruders.len().max(1),
        Layers::ALL,
        (machine, profile),
        &mut out,
    );
    // The materials' colours and densities from their own files.
    // (Each material has a note of its own, in the same order.)
    for (((material, appearance), note), file) in out
        .materials
        .iter_mut()
        .zip(out.report.filaments.iter_mut())
        .zip(&materials)
    {
        let Some(file) = file else {
            continue;
        };
        if let Some(density) = file.density.filter(|d| *d > 0.0) {
            material.density = density;
        }
        // The material file's colour in place of the neutral appearance
        // of a filament the stacks gave no colour (mitcad#1053).
        if let Some(hex) = file.color.as_deref().and_then(print_keys::color)
            && let Some(color) = hex_rgb(&hex)
            && note.colour.is_none()
        {
            let look = plastic_appearance(&material.id, &material.name, color);
            material.appearance = Some(look.id.clone());
            *appearance = Some(look);
            note.colour = Some(hex.chars().take(7).collect());
        }
    }
    if formulas > 0 {
        out.report
            .left_out
            .push(format!("{formulas} values Cura works out with formulas"));
    }
    let per_model = package
        .objects
        .iter()
        .filter(|o| {
            o.metadata.iter().any(|(name, _)| {
                name.starts_with("cura:")
                    && !matches!(
                        name.as_str(),
                        "cura:extruder_nr" | "cura:print_order" | "cura:drop_to_buildplate"
                    )
            })
        })
        .count();
    if per_model > 0 {
        out.report.left_out.push(format!(
            "the per-model settings of {per_model} objects (they do not target MitCad's bodies)"
        ));
    }
    Ok(out)
}

/// Cura's platform adhesion from the brim, raft and skirt values: a raft
/// when it has layers, else a brim when there is one, else a skirt when it
/// has loops, else none.
fn adhesion(values: &BTreeMap<String, Value>) -> Option<&'static str> {
    let raft = values
        .get("raft_layers")
        .and_then(Value::as_i64)
        .is_some_and(|n| n > 0);
    let brim = values
        .get("brim_type")
        .and_then(Value::as_str)
        .map(|t| t != "no_brim");
    let skirt = values.get("skirts").and_then(Value::as_i64).map(|n| n > 0);
    if !values.contains_key("raft_layers") && brim.is_none() && skirt.is_none() {
        return None;
    }
    Some(if raft {
        "raft"
    } else if brim == Some(true) {
        "brim"
    } else if skirt == Some(true) {
        "skirt"
    } else {
        "none"
    })
}

/// Each part's extruder and process values as its per-model settings.
pub(super) fn export(shared: &Shared<'_>) -> ExportOutput {
    let mut out = ExportOutput::default();
    let mut warnings = Vec::new();
    for (i, part) in shared.parts.iter().enumerate() {
        let values: BTreeMap<String, Value> = part
            .settings
            .values
            .iter()
            .map(|(k, (v, _))| (k.clone(), v.clone()))
            .collect();
        let mut metadata = vec![("cura:extruder_nr".to_owned(), shared.slots[i].to_string())];
        for (def, value) in defs_in(PrintSet::Process, &values) {
            for (key, text) in dialect_texts(DIALECT, def, value, &values, &mut warnings) {
                metadata.push((format!("cura:{key}"), text));
            }
        }
        if let Some(adhesion) = adhesion(&values)
            && !metadata.iter().any(|(k, _)| k == "cura:adhesion_type")
        {
            metadata.push(("cura:adhesion_type".to_owned(), adhesion.to_owned()));
        }
        out.part_metadata.push(metadata);
    }
    out.namespaces = vec![(NAMESPACE.0.to_owned(), NAMESPACE.1.to_owned())];
    warnings.sort();
    warnings.dedup();
    out.warnings = warnings;
    out
}
