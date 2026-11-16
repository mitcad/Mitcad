// SPDX-License-Identifier: MIT
//! A slicer's own key where a Mitcad key is wanted (mitcad#1897): a
//! plate's, a profile's or a property's value given by the key a slicer
//! writes (`wall_generator` for `perimeter_generator`). The key map's
//! dialect keys say which Mitcad key it is, as the import reads it; a
//! refusal names that key, in the set it belongs to, and the value in
//! Mitcad's form where it converts. Nothing is accepted under the slicer's
//! key and no key is guessed: a slicer key that means different settings
//! in different slicers (`support_angle`) names every candidate.

use super::{Dialect, KeyDef, PrintSet, Value, by_dialect_key, no_context, number_text, read_text};

/// The Mitcad keys a slicer's key reads as, each with the dialects whose
/// key it is, in the table's order: none for a key no slicer writes,
/// several for one that means different settings in different slicers.
pub fn aliases(dialect_key: &str) -> Vec<(&'static KeyDef, Vec<Dialect>)> {
    let mut found: Vec<(&'static KeyDef, Vec<Dialect>)> = Vec::new();
    for dialect in Dialect::ALL {
        for def in by_dialect_key(dialect, dialect_key) {
            match found.iter_mut().find(|(known, _)| known.key == def.key) {
                Some((_, dialects)) => dialects.push(dialect),
                None => found.push((def, vec![dialect])),
            }
        }
    }
    found
}

/// What to add to "<key> is no key of the <set> set" (mitcad#1897): `None`
/// when `key` is no Mitcad key and no slicer's; else the Mitcad key a
/// slicer's key is (with its plain name, and `value` in Mitcad's form where
/// it converts the same for each of those slicers), the candidates of an
/// ambiguous one, or the set a Mitcad key of another set belongs to.
pub fn key_hint(key: &str, set: PrintSet, value: Option<&serde_json::Value>) -> Option<String> {
    let found = aliases(key);
    let in_set = found.iter().any(|(def, _)| def.set == set);
    if let Some(own) = crate::key(key).filter(|_| !in_set) {
        return Some(format!(
            ": it is a key of the {} set",
            own.set.property_set()
        ));
    }
    match found.as_slice() {
        [] => None,
        [(def, dialects)] if def.set == set => {
            let write = match value.and_then(|value| converted(def, dialects, value)) {
                Some(value) => format!("\"{}\": {value}", def.key),
                None => def.key.to_owned(),
            };
            Some(format!(
                ": it is the {} key of {} ({}); write {write} instead (the print_keys query lists the keys and their values)",
                dialect_names(dialects),
                def.key,
                def.name
            ))
        }
        [(def, dialects)] => Some(format!(
            ": it is the {} key of {} ({}), a key of the {} set",
            dialect_names(dialects),
            def.key,
            def.name,
            def.set.property_set()
        )),
        several => {
            let candidates: Vec<String> = several
                .iter()
                .map(|(def, dialects)| {
                    let other = if def.set == set {
                        String::new()
                    } else {
                        format!(", a key of the {} set", def.set.property_set())
                    };
                    format!(
                        "{} ({}{other}) in {}",
                        def.key,
                        def.name,
                        dialect_names(dialects)
                    )
                })
                .collect();
            Some(format!(
                "; slicers use it for different settings: {}. Write the key of the one meant; the print_keys query lists the keys and their values",
                candidates.join("; ")
            ))
        }
    }
}

/// `value`, given as the slicers' value of their key, in Mitcad's form:
/// only when it reads the same in each of them.
fn converted(
    def: &KeyDef,
    dialects: &[Dialect],
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    let mut read: Option<Value> = None;
    for &dialect in dialects {
        let text = match value {
            serde_json::Value::String(text) => text.clone(),
            serde_json::Value::Number(number) => number_text(number.as_f64()?),
            serde_json::Value::Bool(yes) => dialect.bool_text(*yes).to_owned(),
            _ => return None,
        };
        let one = read_text(def, dialect, &text, &no_context).ok()??;
        if read.as_ref().is_some_and(|earlier| *earlier != one) {
            return None;
        }
        read = Some(one);
    }
    read.map(|value| value.to_json())
}

/// The slicers' names, `A, B and C`.
fn dialect_names(dialects: &[Dialect]) -> String {
    let names: Vec<String> = dialects.iter().map(|d| d.name().to_owned()).collect();
    and_list(&names)
}

/// `A`, `A and B`, `A, B and C`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_slicer_s_key_names_the_mitcad_key_and_its_value() {
        let found = aliases("wall_generator");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.key, "perimeter_generator");
        assert_eq!(
            found[0].1,
            [
                Dialect::BambuStudio,
                Dialect::OrcaSlicer,
                Dialect::QidiStudio
            ]
        );
        let hint = key_hint("wall_generator", PrintSet::Process, Some(&json!("arachne")));
        assert_eq!(
            hint.as_deref(),
            Some(
                ": it is the Bambu Studio, OrcaSlicer and QIDI Studio key of perimeter_generator \
                 (Wall generator); write \"perimeter_generator\": \"arachne\" instead (the \
                 print_keys query lists the keys and their values)"
            )
        );
        // A value that is none of the key's: the key alone.
        let hint = key_hint("wall_generator", PrintSet::Process, Some(&json!("fast"))).unwrap();
        assert!(hint.contains("write perimeter_generator instead"), "{hint}");
    }

    #[test]
    fn a_slicer_s_value_converts_to_mitcad_s() {
        // The Bambu family's threshold angle, read as the import reads it.
        let hint = key_hint(
            "support_threshold_angle",
            PrintSet::Process,
            Some(&json!(35)),
        )
        .unwrap();
        assert!(
            hint.contains("key of support_material_threshold (Support threshold angle)"),
            "{hint}"
        );
        assert!(
            hint.contains("write \"support_material_threshold\": 35 instead"),
            "{hint}"
        );
        // A choice by Cura's name: Mitcad's.
        let hint = key_hint("infill_pattern", PrintSet::Process, Some(&json!("lines"))).unwrap();
        assert!(
            hint.contains("UltiMaker Cura key of fill_pattern (Sparse infill pattern)"),
            "{hint}"
        );
        assert!(
            hint.contains("write \"fill_pattern\": \"rectilinear\" instead"),
            "{hint}"
        );
        // A choice the slicers sharing the key read differently (Bambu
        // Studio's zig-zag is rectilinear, OrcaSlicer has none of that
        // name): the key alone, no value guessed.
        let hint = key_hint(
            "sparse_infill_pattern",
            PrintSet::Process,
            Some(&json!("zig-zag")),
        )
        .unwrap();
        assert!(hint.contains("write fill_pattern instead"), "{hint}");
        let hint = key_hint(
            "sparse_infill_pattern",
            PrintSet::Process,
            Some(&json!("gyroid")),
        )
        .unwrap();
        assert!(
            hint.contains("write \"fill_pattern\": \"gyroid\" instead"),
            "{hint}"
        );
    }

    #[test]
    fn an_ambiguous_slicer_key_names_every_candidate() {
        // The Bambu family's support_angle turns the support pattern; Cura's
        // is the overhang threshold.
        let found = aliases("support_angle");
        let keys: Vec<&str> = found.iter().map(|(def, _)| def.key).collect();
        assert!(keys.contains(&"support_material_angle"), "{keys:?}");
        assert!(keys.contains(&"support_material_threshold"), "{keys:?}");
        let hint = key_hint("support_angle", PrintSet::Process, Some(&json!(40))).unwrap();
        assert!(hint.contains("different settings"), "{hint}");
        assert!(
            hint.contains("support_material_angle (Support pattern angle) in Bambu Studio"),
            "{hint}"
        );
        assert!(
            hint.contains("support_material_threshold (Support threshold angle) in UltiMaker Cura"),
            "{hint}"
        );
        // No value is offered for either.
        assert!(!hint.contains("\": "), "{hint}");
    }

    #[test]
    fn keys_of_other_sets_and_unknown_keys() {
        // A Mitcad key of another set.
        assert_eq!(
            key_hint("bed_shape", PrintSet::Process, None).as_deref(),
            Some(": it is a key of the Print Printer set")
        );
        // A slicer's key of a filament setting.
        let hint = key_hint("nozzle_temperature", PrintSet::Process, Some(&json!(215))).unwrap();
        assert!(
            hint.contains(
                "key of temperature (Nozzle temperature), a key of the Print Filament set"
            ),
            "{hint}"
        );
        // No slicer's key, and nothing close to one is guessed.
        assert_eq!(key_hint("wall_generatr", PrintSet::Process, None), None);
        assert_eq!(key_hint("generator", PrintSet::Process, None), None);
    }
}
