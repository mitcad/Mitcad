// SPDX-License-Identifier: MIT
//! A slicer's settings as read from its files, and the key map's values
//! read from them (mitcad#640).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::print_keys::{self, Dialect, Form, PrintSet};

/// A slicer's values by its keys.
pub(crate) trait Source {
    /// The text of a key: a list key's entry `index` (the first when the
    /// list is shorter), another key's whole text.
    fn text(&self, key: &str, list: bool, index: usize) -> Option<String>;
    /// The number of entries of a list key.
    fn count(&self, key: &str) -> Option<usize>;
    /// The keys there are.
    fn keys(&self) -> Vec<String>;
    /// A key's whole value as the slicer's text (a JSON list as JSON).
    fn raw(&self, key: &str) -> Option<String>;
}

/// A JSON value's text: text as it is, numbers and yes/no as written.
pub(crate) fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "1" } else { "0" }.to_owned()),
        _ => None,
    }
}

/// The Bambu family's JSON: text, or lists of text (per filament, per
/// extruder, or the points of an outline).
pub(crate) struct JsonSource<'a>(pub &'a serde_json::Map<String, Value>);

impl Source for JsonSource<'_> {
    fn text(&self, key: &str, list: bool, index: usize) -> Option<String> {
        match self.0.get(key)? {
            Value::Array(items) if list => items
                .get(index)
                .or_else(|| items.first())
                .and_then(json_text),
            // An outline's points, or a list the map takes as one value.
            Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(json_text)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            other => json_text(other),
        }
    }

    fn count(&self, key: &str) -> Option<usize> {
        match self.0.get(key)? {
            Value::Array(items) => Some(items.len()),
            other => json_text(other).map(|t| print_keys::split_list(&t).len()),
        }
    }

    fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    fn raw(&self, key: &str) -> Option<String> {
        self.0.get(key).map(kept_text)
    }
}

/// PrusaSlicer's INI: text, lists as `a,b` or `"a";"b"`.
pub(crate) struct IniSource<'a>(pub &'a BTreeMap<String, String>);

impl Source for IniSource<'_> {
    fn text(&self, key: &str, list: bool, index: usize) -> Option<String> {
        let text = self.0.get(key)?;
        if !list {
            return Some(text.clone());
        }
        let entries = print_keys::split_list(text);
        entries.get(index).or_else(|| entries.first()).cloned()
    }

    fn count(&self, key: &str) -> Option<usize> {
        self.0.get(key).map(|t| print_keys::split_list(t).len())
    }

    fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    fn raw(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// Cura's stacks: the global stack's values and each extruder's.
pub(crate) struct CuraSource<'a> {
    pub global: &'a BTreeMap<String, String>,
    pub extruders: &'a [BTreeMap<String, String>],
}

impl Source for CuraSource<'_> {
    fn text(&self, key: &str, list: bool, index: usize) -> Option<String> {
        let extruder = || {
            self.extruders
                .get(index)
                .or_else(|| self.extruders.first())
                .and_then(|e| e.get(key))
        };
        let text = if list {
            extruder().or_else(|| self.global.get(key))
        } else {
            self.global.get(key).or_else(extruder)
        }?;
        // A formula is Cura's to work out.
        (!text.trim_start().starts_with('=')).then(|| text.clone())
    }

    fn count(&self, _key: &str) -> Option<usize> {
        None
    }

    fn keys(&self) -> Vec<String> {
        self.global
            .keys()
            .chain(self.extruders.iter().flat_map(|e| e.keys()))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    fn raw(&self, key: &str) -> Option<String> {
        self.global
            .get(key)
            .or_else(|| self.extruders.first().and_then(|e| e.get(key)))
            .cloned()
    }
}

/// The key map's values of a set from a slicer's values (`index`: the
/// filament or extruder whose list entries are read); values that do not
/// read are problems.
pub(crate) fn read_layer(
    dialect: Dialect,
    set: PrintSet,
    source: &dyn Source,
    index: usize,
    problems: &mut Vec<String>,
) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for def in print_keys::keys_in(set) {
        let Some(dk) = def.dialect(dialect) else {
            continue;
        };
        if def.per_dialect {
            if let Some(text) = source
                .text(dk.key, dk.list, index)
                .filter(|t| !t.trim().is_empty())
            {
                out.insert(def.key.to_owned(), json!({ dialect.id(): text }));
            }
            continue;
        }
        if dk.form == Form::Count {
            if let Some(n) = source.count(dk.key) {
                out.insert(def.key.to_owned(), json!(n));
            }
            continue;
        }
        let raw = |key: &str| source.text(key, dk.list, index);
        let context = |key: &str| -> Option<print_keys::Value> {
            let other = print_keys::key(key)?;
            let odk = other.dialect(dialect)?;
            print_keys::read(
                other,
                dialect,
                &|k: &str| source.text(k, odk.list, index),
                &print_keys::no_context,
            )
            .ok()
            .flatten()
        };
        match print_keys::read(def, dialect, &raw, &context) {
            Ok(Some(value)) => {
                out.insert(def.key.to_owned(), value.to_json());
            }
            Ok(None) => {}
            Err(e) => problems.push(format!("{}: {e}", dk.key)),
        }
    }
    out
}

/// Every key of a dialect the map reads (a switch's yes/no key and Cura's
/// bed keys too).
pub(crate) fn mapped_keys(dialect: Dialect) -> BTreeSet<&'static str> {
    let mut keys = BTreeSet::new();
    for def in print_keys::keys() {
        let Some(dk) = def.dialect(dialect) else {
            continue;
        };
        keys.insert(dk.key);
        match dk.form {
            Form::Switch { key, .. } => {
                keys.insert(key);
            }
            Form::CuraBed => keys.extend(print_keys::CURA_BED),
            _ => {}
        }
    }
    keys
}

/// The map's keys of a set as the dialect's keys.
pub(crate) fn layer_keys(dialect: Dialect, set: PrintSet) -> BTreeSet<&'static str> {
    let mut keys = BTreeSet::new();
    for def in print_keys::keys_in(set) {
        let Some(dk) = def.dialect(dialect) else {
            continue;
        };
        keys.insert(dk.key);
        match dk.form {
            Form::Switch { key, .. } => {
                keys.insert(key);
            }
            Form::CuraBed => keys.extend(print_keys::CURA_BED),
            _ => {}
        }
    }
    keys
}

/// Keys that describe a slicer's file or preset, not a setting: neither
/// read nor kept.
pub(crate) const BOOKKEEPING: &[&str] = &[
    "name",
    "version",
    "from",
    "type",
    "instantiation",
    "setting_id",
    "base_id",
    "user_id",
    "updated_time",
    "inherits_group",
    "inherits_cummulative",
    "different_settings_to_system",
    "is_custom_defined",
];

/// The layer a key the map does not have most likely belongs to, by its
/// name: filament keys and printer keys by their words, the rest the
/// process's.
pub(crate) fn leftover_set(key: &str) -> PrintSet {
    const PRINTER: &[&str] = &[
        "machine_",
        "printer_",
        "printable_",
        "bed_shape",
        "bed_custom",
        "bed_exclude",
        "bed_mesh",
        "bed_temperature_formula",
        "nozzle_",
        "extruder_",
        "extruders_",
        "retract",
        "wipe",
        "z_hop",
        "deretraction",
        "start_gcode",
        "end_gcode",
        "before_layer",
        "layer_change",
        "toolchange",
        "change_filament_gcode",
        "change_extrusion_role",
        "gcode_flavor",
        "host_type",
        "print_host",
        "thumbnails",
        "use_relative_e",
        "use_firmware_retraction",
        "max_layer_height",
        "min_layer_height",
        "silent_mode",
        "auxiliary_fan",
        "scan_first_layer",
        "default_print_profile",
        "default_filament_profile",
        "single_extruder",
        "cooling_tube",
        "parking_pos",
        "extra_loading",
        "high_current",
        "travel_slope",
        "travel_ramping",
        "template_custom_gcode",
        "pause_print_gcode",
        "color_change_gcode",
        "time_lapse_gcode",
        "best_object_pos",
        "upward_compatible",
        "support_air_filtration",
        "support_chamber",
        "support_multi_bed",
        "default_nozzle",
        "physical_extruder",
        "master_extruder",
        "grab_length",
        "emit_machine_limits",
        "z_offset",
        "binary_gcode",
        "autoemit",
        "use_relative",
        "manual_filament_change",
        "purge_in_prime_tower",
        "enable_long_retraction",
        "long_retractions",
        "preferred_orientation",
        "extruder_clearance",
        "fan_speedup",
        "fan_kickstart",
    ];
    const FILAMENT: &[&str] = &[
        "filament_",
        "compatible_printers",
        "compatible_prints",
        "nozzle_temperature",
        "temperature",
        "first_layer_temperature",
        "first_layer_bed_temperature",
        "bed_temperature",
        "cool_plate",
        "eng_plate",
        "hot_plate",
        "textured_plate",
        "supertack_plate",
        "fan_",
        "min_fan",
        "max_fan",
        "bridge_fan",
        "slow_down",
        "full_fan",
        "close_fan",
        "overhang_fan",
        "reduce_fan",
        "additional_cooling",
        "activate_air",
        "activate_chamber",
        "complete_print_exhaust",
        "during_print_exhaust",
        "chamber_temperature",
        "start_filament_gcode",
        "end_filament_gcode",
        "cooling",
        "required_nozzle_hrc",
        "pressure_advance",
        "enable_pressure_advance",
        "idle_temperature",
        "softening",
        "temperature_vitrification",
        "material_",
        "impact_strength",
        "dont_slow_down",
        "disable_fan",
        "min_print_speed",
        "external_perimeter_fan",
    ];
    let lower = key.to_ascii_lowercase();
    if PRINTER.iter().any(|p| lower.starts_with(p)) {
        PrintSet::Printer
    } else if FILAMENT.iter().any(|p| lower.starts_with(p)) {
        PrintSet::Filament
    } else {
        PrintSet::Process
    }
}

/// Keys outside the filament layer whose lists still hold one entry per
/// filament in a Bambu-family project (the project's resizes them with its
/// filaments): the default colour of each filament, and Bambu Studio's
/// volumetric speed coefficients of each.
pub(crate) const FILAMENT_SIZED: &[&str] =
    &["default_filament_colour", "volumetric_speed_coefficients"];

/// Whether a dialect key's list holds an entry per filament: a filament
/// key of the map, a key the import keeps in the filament layer by its name
/// (but the presets a filament is compatible with), or one of
/// [`FILAMENT_SIZED`].
pub fn per_filament_keys(dialect: Dialect) -> impl Fn(&str) -> bool {
    let mapped = mapped_keys(dialect);
    let filament = layer_keys(dialect, PrintSet::Filament);
    move |key: &str| {
        if FILAMENT_SIZED.contains(&key) {
            true
        } else if mapped.contains(key) {
            filament.contains(key)
        } else {
            leftover_set(key) == PrintSet::Filament
                && !matches!(key, "compatible_printers" | "compatible_prints")
        }
    }
}

/// A value kept as a slicer's text: text as it is, a list as its JSON.
pub(crate) fn kept_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// A kept text as the Bambu family's JSON value: a JSON list when it is
/// one, else the text.
pub(crate) fn kept_value(text: &str) -> Value {
    if text.trim_start().starts_with('[')
        && let Ok(list @ Value::Array(_)) = serde_json::from_str::<Value>(text)
    {
        return list;
    }
    Value::String(text.to_owned())
}
