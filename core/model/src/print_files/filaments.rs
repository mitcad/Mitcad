// SPDX-License-Identifier: MIT
//! A filament's presets as one material (mitcad#1054). A slicer keeps a
//! preset of a filament per printer and nozzle ("Shop PLA @Bench X 0.6
//! nozzle"); MitCad keeps the filament once, its values for each printer
//! and nozzle as the material's variants ([`crate::material`]).
//!
//! Presets are of one filament when their names before the `@` are the
//! same (any case) and they print in one colour: a filament whose colours
//! print differently stays a material per colour. A preset is for the
//! printers it is compatible with (its slicer's `compatible_printers`),
//! else for the printer its name's `@` part names, else for any.
//!
//! Merged, the filament is named as the presets before their `@`. A preset
//! for no printer is the base, the others its variants with the values
//! that differ from it; without one the base holds the values all presets
//! share and each variant the rest. Two presets for no printer are not
//! merged. Each variant is a printer and nozzle; a preset for several
//! printers gives each of them one. An import keeps only the variants of
//! the printers and nozzles chosen ([`PrinterChoice`]).

use std::collections::BTreeMap;

use serde_json::Value;

use crate::appearance::Appearance;
use crate::material::{
    self, Material, MaterialVariant, SlicerKeys, same_nozzle, same_printer, split_filament_name,
    split_nozzle, suffix_printer,
};
use crate::print_profile::{free_id, slug};

/// A printer and nozzle chosen for an import (mitcad#1054): only the
/// presets and variants of the printers chosen come in.
#[derive(Debug, Clone, PartialEq)]
pub struct PrinterChoice {
    pub printer: String,
    /// None: any nozzle of the printer.
    pub nozzle: Option<f64>,
}

impl PrinterChoice {
    /// Whether a preset's printer and nozzle (none: not known) is chosen.
    pub fn takes(&self, printer: &str, nozzle: Option<f64>) -> bool {
        same_printer(&self.printer, printer)
            && match (self.nozzle, nozzle) {
                (Some(a), Some(b)) => same_nozzle(a, b),
                _ => true,
            }
    }
}

/// A printer and nozzle (none: any).
pub type PrinterNozzle = (String, Option<f64>);

/// A material an import read, with the appearance it carries.
pub type ImportedMaterial = (Material, Option<Appearance>);

/// What makes materials one filament's: its name (lower case) and its
/// colour.
pub(crate) type GroupKey = (String, Option<String>);

fn same_place(a: &PrinterNozzle, b: &PrinterNozzle) -> bool {
    same_printer(&a.0, &b.0)
        && match (a.1, b.1) {
            (Some(x), Some(y)) => same_nozzle(x, y),
            (None, None) => true,
            _ => false,
        }
}

/// The printers a slicer's own keys say a filament is compatible with
/// (`compatible_printers` of any slicer: a JSON list or `"a";"b"`).
pub fn compatible_printers(slicer: &SlicerKeys) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for keys in slicer.values() {
        let Some(text) = keys.get("compatible_printers") else {
            continue;
        };
        let entries: Vec<String> = match serde_json::from_str::<Value>(text) {
            Ok(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(|t| t.trim().to_owned())
                .collect(),
            _ => text
                .split(';')
                .map(|t| t.trim().trim_matches('"').trim().to_owned())
                .collect(),
        };
        for entry in entries {
            if !entry.is_empty() && !out.contains(&entry) {
                out.push(entry);
            }
        }
    }
    out
}

/// The printers and nozzles a filament preset is for: those of the
/// printer presets it is compatible with, else the one its name's `@`
/// part names; none: any printer.
pub fn preset_printers(name: &str, compatible: &[String]) -> Vec<PrinterNozzle> {
    let mut out: Vec<PrinterNozzle> = Vec::new();
    for printer in compatible {
        let place = split_nozzle(printer);
        if !place.0.is_empty() && !out.iter().any(|p| same_place(p, &place)) {
            out.push(place);
        }
    }
    if out.is_empty()
        && let Some(suffix) = split_filament_name(name).1
        && let Some(place) = suffix_printer(&suffix)
    {
        out.push(place);
    }
    out
}

/// A filament's colour as the presets' groups compare it.
fn colour_of(filament: &BTreeMap<String, Value>) -> Option<String> {
    filament
        .get("filament_colour")
        .and_then(Value::as_str)
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty())
}

/// A preset's values to merge, and where it is for (none: any printer).
#[derive(Debug, Clone)]
pub(crate) struct Entry {
    pub filament: BTreeMap<String, Value>,
    pub slicer: SlicerKeys,
    pub printers: Vec<PrinterNozzle>,
}

impl Entry {
    /// A material read from a preset: its values and the printers its
    /// slicer keys or its name say.
    pub fn of_preset(m: &Material) -> Entry {
        Entry {
            filament: m.filament.clone(),
            slicer: m.slicer.clone(),
            printers: preset_printers(&m.name, &compatible_printers(&m.slicer)),
        }
    }

    /// A material as presets (a merge of materials kept already): one
    /// with variants is its base for any printer and each variant with
    /// the base's values under its own; one without is a preset.
    pub fn of_material(m: &Material) -> Vec<Entry> {
        if m.variants.is_empty() {
            return vec![Entry::of_preset(m)];
        }
        let mut out = vec![Entry {
            filament: m.filament.clone(),
            slicer: m.slicer.clone(),
            printers: Vec::new(),
        }];
        for (i, variant) in m.variants.iter().enumerate() {
            let (filament, slicer) = m.values_with(Some(i));
            out.push(Entry {
                filament,
                slicer,
                printers: vec![(variant.printer.clone(), variant.nozzle)],
            });
        }
        out
    }
}

/// A merged filament's base values and variants.
pub(crate) struct Merged {
    pub filament: BTreeMap<String, Value>,
    pub slicer: SlicerKeys,
    pub variants: Vec<MaterialVariant>,
    /// The entry the base is (one for no printer), if any.
    pub base: Option<usize>,
}

/// The values of `values` that `base` does not have alike.
fn differing<'a>(
    values: &'a BTreeMap<String, Value>,
    base: &'a BTreeMap<String, Value>,
) -> impl Iterator<Item = (&'a String, &'a Value)> {
    values.iter().filter(move |(k, v)| base.get(*k) != Some(*v))
}

/// Merges the entries of one filament: None when two are for no printer.
pub(crate) fn merge_entries(entries: &[Entry]) -> Option<Merged> {
    let anywhere: Vec<usize> = (0..entries.len())
        .filter(|&i| entries[i].printers.is_empty())
        .collect();
    if anywhere.len() > 1 {
        return None;
    }
    let base_entry = anywhere.first().copied();
    let (filament, slicer) = match base_entry {
        Some(i) => (entries[i].filament.clone(), entries[i].slicer.clone()),
        None => {
            // What every entry has alike.
            let first = entries.first()?;
            let filament = first
                .filament
                .iter()
                .filter(|(k, v)| entries[1..].iter().all(|e| e.filament.get(*k) == Some(*v)))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let mut slicer = SlicerKeys::new();
            for (dialect, keys) in &first.slicer {
                let shared: BTreeMap<String, String> = keys
                    .iter()
                    .filter(|(k, v)| {
                        entries[1..].iter().all(|e| {
                            e.slicer
                                .get(dialect)
                                .and_then(|own| own.get(*k))
                                .is_some_and(|own| own == *v)
                        })
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if !shared.is_empty() {
                    slicer.insert(dialect.clone(), shared);
                }
            }
            (filament, slicer)
        }
    };
    let mut variants: Vec<MaterialVariant> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        if Some(i) == base_entry {
            continue;
        }
        // A colour is never a variant's.
        let own: BTreeMap<String, Value> = differing(&entry.filament, &filament)
            .filter(|(k, _)| k.as_str() != "filament_colour")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut own_slicer = SlicerKeys::new();
        for (dialect, keys) in &entry.slicer {
            let empty = BTreeMap::new();
            let base_keys = slicer.get(dialect).unwrap_or(&empty);
            let differ: BTreeMap<String, String> = keys
                .iter()
                .filter(|(k, v)| base_keys.get(*k) != Some(*v))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            if !differ.is_empty() {
                own_slicer.insert(dialect.clone(), differ);
            }
        }
        for (printer, nozzle) in &entry.printers {
            if variants.iter().any(|v| v.is_for(printer, *nozzle)) {
                continue;
            }
            variants.push(MaterialVariant {
                printer: printer.clone(),
                nozzle: *nozzle,
                filament: own.clone(),
                slicer: own_slicer.clone(),
            });
        }
    }
    Some(Merged {
        filament,
        slicer,
        variants,
        base: base_entry,
    })
}

/// The group a material falls in: its filament's name before the `@`
/// (any case) and its colour.
pub(crate) fn group_key(m: &Material) -> GroupKey {
    (
        split_filament_name(&m.name).0.to_lowercase(),
        colour_of(&m.filament),
    )
}

/// An appearance's colour as the presets' groups compare it (`#RRGGBB`).
fn appearance_colour(look: &Appearance) -> String {
    let byte = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b] = look.base_color;
    format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))
}

/// The group an imported material falls in: as [`group_key`], its colour
/// the appearance the import made of it. A read filament keeps its colour
/// only as that appearance (mitcad#1053), so without it two slots of one
/// preset in different colours would merge.
fn imported_group_key((m, appearance): &ImportedMaterial) -> GroupKey {
    let (name, colour) = group_key(m);
    (name, appearance.as_ref().map(appearance_colour).or(colour))
}

/// Merges the materials an import read from filament presets (mitcad#1054):
/// those of one filament become one material with variants (see the
/// module documentation), and with `keep` only the presets and variants of
/// the printers chosen come in (a preset for any printer always does).
/// The answer gives the materials and, for each one given, the one it went
/// into (none: left out).
pub fn merge_filaments(
    materials: Vec<ImportedMaterial>,
    keep: Option<&[PrinterChoice]>,
) -> (Vec<ImportedMaterial>, Vec<Option<usize>>) {
    let mut entries: Vec<Option<Entry>> = materials
        .iter()
        .map(|(m, _)| {
            let mut entry = Entry::of_preset(m);
            if let Some(chosen) = keep
                && !entry.printers.is_empty()
            {
                entry
                    .printers
                    .retain(|(p, n)| chosen.iter().any(|c| c.takes(p, *n)));
                if entry.printers.is_empty() {
                    return None;
                }
            }
            Some(entry)
        })
        .collect();
    // The groups, in the order of their first material.
    let mut groups: Vec<(GroupKey, Vec<usize>)> = Vec::new();
    for (i, material) in materials.iter().enumerate() {
        if entries[i].is_none() {
            continue;
        }
        let key = imported_group_key(material);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    let ids: Vec<String> = materials.iter().map(|(m, _)| m.id.clone()).collect();
    let mut slots: Vec<Option<ImportedMaterial>> = materials.into_iter().map(Some).collect();
    let mut out: Vec<ImportedMaterial> = Vec::new();
    let mut into = vec![None; slots.len()];
    for (_, members) in groups {
        let group_entries: Vec<Entry> = members.iter().filter_map(|&i| entries[i].take()).collect();
        let merged = (members.len() > 1)
            .then(|| merge_entries(&group_entries))
            .flatten();
        let Some(merged) = merged else {
            for &i in &members {
                if let Some(item) = slots[i].take() {
                    into[i] = Some(out.len());
                    out.push(item);
                }
            }
            continue;
        };
        // The base preset's material (else the first's) carries the
        // filament: its density, appearance, class and origin.
        let lead = members[merged.base.unwrap_or(0)];
        let Some((mut material, appearance)) = slots[lead].take() else {
            continue;
        };
        let name = split_filament_name(&material.name).0;
        let wanted = slug(&name);
        let taken_elsewhere = |id: &str| {
            ids.iter()
                .enumerate()
                .any(|(i, own)| own == id && !members.contains(&i))
                || out.iter().any(|(m, _)| m.id == id)
        };
        material.id = free_id(&wanted, taken_elsewhere);
        material.name = name;
        material.filament = merged.filament;
        material.slicer = merged.slicer;
        material.variants = merged.variants;
        for &i in &members {
            slots[i] = None;
            into[i] = Some(out.len());
        }
        out.push((material, appearance));
    }
    (out, into)
}

/// A merge of materials kept already (mitcad#1054): their values as one
/// material's, named as their filament, or None when they cannot be one
/// (two of them for no printer).
pub(crate) fn merge_kept(materials: &[&Material]) -> Option<Material> {
    let first = materials.first()?;
    let entries: Vec<Entry> = materials
        .iter()
        .flat_map(|m| Entry::of_material(m))
        .collect();
    let merged = merge_entries(&entries)?;
    // The material whose values are the base's (one for no printer), else
    // the first, carries the density and the appearance.
    let mut owner = 0;
    if let Some(base) = merged.base {
        let mut seen = 0;
        for (i, m) in materials.iter().enumerate() {
            let count = Entry::of_material(m).len();
            if base < seen + count {
                owner = i;
                break;
            }
            seen += count;
        }
    }
    let mut out = (*materials[owner]).clone();
    out.id = first.id.clone();
    out.name = split_filament_name(&first.name).0;
    out.filament = merged.filament;
    out.slicer = merged.slicer;
    out.variants = merged.variants;
    out.origin = first.origin.clone();
    out.version = first.version;
    Some(out)
}

/// The groups of materials that are one filament's presets kept apart
/// (mitcad#1054): two or more of one filament and colour, one of them for
/// a printer, that can be merged. Each group by the materials' indices.
pub(crate) fn separate_filaments(materials: &[&Material]) -> Vec<Vec<usize>> {
    let mut groups: Vec<(GroupKey, Vec<usize>)> = Vec::new();
    for (i, m) in materials.iter().enumerate() {
        if material::material_class(m) != material::MaterialClass::FdmPlastic {
            continue;
        }
        let key = group_key(m);
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, members)) => members.push(i),
            None => groups.push((key, vec![i])),
        }
    }
    groups
        .into_iter()
        .map(|(_, members)| members)
        .filter(|members| {
            members.len() > 1
                && members.iter().any(|&i| {
                    !Entry::of_preset(materials[i]).printers.is_empty()
                        || !materials[i].variants.is_empty()
                })
                && merge_kept(&members.iter().map(|&i| materials[i]).collect::<Vec<_>>()).is_some()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn preset(name: &str, values: Value, compatible: &[&str]) -> Material {
        let mut m = material::built_in_material("pla").unwrap().clone();
        m.id = slug(name);
        m.name = name.to_owned();
        m.filament = serde_json::from_value(values).unwrap();
        if !compatible.is_empty() {
            m.slicer.insert(
                "bambu_studio".to_owned(),
                BTreeMap::from([(
                    "compatible_printers".to_owned(),
                    serde_json::to_string(compatible).unwrap(),
                )]),
            );
        }
        m
    }

    #[test]
    fn presets_of_one_filament_merge_into_a_base_and_variants() {
        let presets = vec![
            preset(
                "Shop PLA @Bench X",
                json!({"temperature": 220, "bed_temperature": 60, "filament_type": "PLA"}),
                &["Bench X 0.4 nozzle"],
            ),
            preset(
                "Shop PLA @Bench X 0.6 nozzle",
                json!({"temperature": 230, "bed_temperature": 60, "filament_type": "PLA"}),
                &["Bench X 0.6 nozzle"],
            ),
            preset(
                "Shop PLA @Bench Y",
                json!({"temperature": 215, "bed_temperature": 55, "filament_type": "PLA"}),
                &[],
            ),
            preset("Other PETG", json!({"filament_type": "PETG"}), &[]),
        ];
        let (out, into) = merge_filaments(presets.into_iter().map(|m| (m, None)).collect(), None);
        assert_eq!(into, vec![Some(0), Some(0), Some(0), Some(1)]);
        let shop = &out[0].0;
        assert_eq!(shop.name, "Shop PLA");
        assert_eq!(shop.id, "shop-pla");
        assert_eq!(
            shop.filament,
            serde_json::from_value::<BTreeMap<String, Value>>(json!({"filament_type": "PLA"}))
                .unwrap(),
            "the values all share"
        );
        let labels: Vec<String> = shop.variants.iter().map(MaterialVariant::label).collect();
        assert_eq!(
            labels,
            [
                "Bench X, 0.4 mm nozzle",
                "Bench X, 0.6 mm nozzle",
                "Bench Y, any nozzle"
            ]
        );
        assert_eq!(shop.variants[2].filament["bed_temperature"], 55);
        shop.check().unwrap();
        assert_eq!(out[1].0.name, "Other PETG");
    }

    #[test]
    fn chosen_printers_colours_and_a_generic_preset() {
        let presets = vec![
            preset("Shop PLA", json!({"temperature": 210}), &[]),
            preset(
                "Shop PLA @Bench X",
                json!({"temperature": 220}),
                &["Bench X 0.4 nozzle"],
            ),
            preset(
                "Shop PLA @Bench Y",
                json!({"temperature": 205}),
                &["Bench Y 0.4 nozzle"],
            ),
        ];
        let keep = [PrinterChoice {
            printer: "Bench X".to_owned(),
            nozzle: Some(0.4),
        }];
        let (out, into) = merge_filaments(
            presets.into_iter().map(|m| (m, None)).collect(),
            Some(&keep),
        );
        assert_eq!(into, vec![Some(0), Some(0), None], "Bench Y's left out");
        let shop = &out[0].0;
        assert_eq!(
            shop.filament["temperature"], 210,
            "the generic preset is the base"
        );
        assert_eq!(shop.variants.len(), 1);
        assert_eq!(shop.variants[0].filament["temperature"], 220);

        // Black and white printing differently stay two materials.
        let presets = vec![
            preset(
                "Budget PLA @Bench X",
                json!({"filament_colour": "#000000", "temperature": 200}),
                &["Bench X 0.4 nozzle"],
            ),
            preset(
                "Budget PLA @Bench X 0.6 nozzle",
                json!({"filament_colour": "#FFFFFF", "temperature": 215}),
                &["Bench X 0.6 nozzle"],
            ),
        ];
        let (out, _) = merge_filaments(presets.into_iter().map(|m| (m, None)).collect(), None);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(m, _)| m.variants.is_empty()));
    }

    #[test]
    fn an_import_s_colours_are_its_appearances() {
        // As an import reads them: the colour only as the appearance
        // (mitcad#1053), none in the filament values.
        let read = |name: &str, kind: &str, colour: [f64; 3]| -> ImportedMaterial {
            let m = preset(name, json!({"filament_type": kind}), &[]);
            let look = crate::print_files::plastic_appearance(&m.id, name, colour);
            (m, Some(look))
        };
        let white = [1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0];
        // One preset in two colours, and names that differ only in the
        // type word: four materials in their order.
        let slots = vec![
            read("Generic PETG @Bench X", "PETG", white),
            read("Generic PLA @Bench X", "PLA", white),
            read("Generic PETG @Bench X", "PETG", black),
            read("Generic TPU @Bench X", "TPU", white),
        ];
        let (out, into) = merge_filaments(slots, None);
        assert_eq!(into, vec![Some(0), Some(1), Some(2), Some(3)]);
        let kinds: Vec<&Value> = out
            .iter()
            .map(|(m, _)| &m.filament["filament_type"])
            .collect();
        assert_eq!(
            kinds,
            [&json!("PETG"), &json!("PLA"), &json!("PETG"), &json!("TPU")]
        );
        assert_eq!(out[2].1.as_ref().unwrap().base_color, black);
        assert!(out.iter().all(|(m, _)| m.variants.is_empty()));
        // One colour on two printers is still one filament.
        let presets = vec![
            read("Shop PLA @Bench X", "PLA", white),
            read("Shop PLA @Bench Y", "PLA", white),
        ];
        let (out, into) = merge_filaments(presets, None);
        assert_eq!(into, vec![Some(0), Some(0)]);
        assert_eq!(out[0].0.variants.len(), 2);
    }

    #[test]
    fn kept_materials_merge_again() {
        let x = preset(
            "Shop PLA @Bench X",
            json!({"temperature": 220}),
            &["Bench X 0.4 nozzle"],
        );
        let y = preset(
            "Shop PLA @Bench Y",
            json!({"temperature": 205}),
            &["Bench Y 0.4 nozzle"],
        );
        let refs = [&x, &y];
        assert_eq!(separate_filaments(&refs), vec![vec![0, 1]]);
        let merged = merge_kept(&refs).unwrap();
        assert_eq!(merged.id, x.id);
        assert_eq!(merged.name, "Shop PLA");
        assert_eq!(merged.variants.len(), 2);
        // A third preset later merges into the merged one.
        let z = preset(
            "Shop PLA @Bench Z",
            json!({"temperature": 240}),
            &["Bench Z 0.4 nozzle"],
        );
        let again = merge_kept(&[&merged, &z]).unwrap();
        let labels: Vec<String> = again.variants.iter().map(MaterialVariant::label).collect();
        assert_eq!(
            labels,
            [
                "Bench X, 0.4 mm nozzle",
                "Bench Y, 0.4 mm nozzle",
                "Bench Z, 0.4 mm nozzle"
            ]
        );
        assert_eq!(again.variants[2].filament["temperature"], 240);
        // Two generic presets of one name do not merge.
        let a = preset("Shop PLA", json!({"temperature": 200}), &[]);
        let b = preset("shop pla", json!({"temperature": 201}), &[]);
        assert!(separate_filaments(&[&a, &b]).is_empty());
    }
}
