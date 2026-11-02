// SPDX-License-Identifier: MIT
//! Material and process notes on the model (mitcad#1049; `annotations.rs`,
//! commands.md "Material and process notes"): annotations of the kinds
//! `material` and `process_note` on a body (a face, edge or vertex of it,
//! or the body where it was clicked), whose text is written from the
//! design whenever the annotations are asked for: a material note the
//! name of the body's material (its own, else its component's default, as
//! `body_material` gives it); a process note the body's printer and
//! process profiles and every printer or process value set on the body,
//! its occurrences or its components rather than taken from a profile (as
//! the `print_settings` query gives them), so that it follows a change of
//! the material or of a print value. A pick whose body is gone makes the
//! note broken, as any annotation's.
//!
//! The `make_differences` query gives the Make tab's Browser markers: the
//! bodies whose material or print settings differ from their component's.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::ids::{BodyUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::print_keys::{self, PrintSet};
use mitcad_model::print_profile::{PROFILE_KEY, PrintPropertySet, print_set_of};
use mitcad_model::properties::{PropertyOwner, same_name};
use mitcad_model::{Document, MeasureKind, MeasurePick, PrintTarget};

/// The words of a profile not chosen: the slicer's own settings.
const NO_PROFILE: &str = "Slicer default";

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait BodyNotesOps<K: Kernel> {
    /// A material or process note's lines for its pick, as the design is
    /// now; why there are none when the pick is no body's.
    fn body_note_lines(&self, kind: MeasureKind, pick: &MeasurePick)
    -> Result<Vec<String>, String>;

    /// The `make_differences` query (mitcad#1049): each body whose material
    /// or print settings differ from its component's, with `body`, `name`,
    /// `material` (the name of a material of its own other than the one
    /// its component gives; null when it has none such) and `print` (the
    /// plain names of the print values set on the body itself, in their
    /// sets' order: `Walls`, `Process profile`, a slicer's own key with
    /// the slicer's name).
    fn make_differences_json(&self) -> Value;
}

impl<K: Kernel> BodyNotesOps<K> for Document<K> {
    fn body_note_lines(
        &self,
        kind: MeasureKind,
        pick: &MeasurePick,
    ) -> Result<Vec<String>, String> {
        let Some(body) = pick.body else {
            return Err(format!("a {} points at a body", words(kind)));
        };
        let path = pick
            .occurrence
            .as_ref()
            .map(|p| p.0.clone())
            .filter(|p| !p.is_empty());
        match kind {
            MeasureKind::Material => {
                let (material, _) = self.body_material(body, path.as_deref());
                Ok(vec![material.name.clone()])
            }
            MeasureKind::ProcessNote => Ok(process_lines(
                &self.print_settings_json(&PrintTarget::Body(body, path)),
            )),
            _ => Err(format!("a {} is no material or process note", kind.key())),
        }
    }

    fn make_differences_json(&self) -> Value {
        let mut out = Vec::new();
        // Each body's placements: the occurrence paths its components'
        // defaults are taken up (mitcad#1150).
        let mut placements: BTreeMap<BodyUid, Vec<Vec<OccurrenceUid>>> = BTreeMap::new();
        for instance in self.instances() {
            placements
                .entry(instance.body)
                .or_default()
                .push(instance.path);
        }
        for body in self.bodies() {
            let uid = body.uid;
            // A material of its own, other than the one its components give
            // it in one of its placements (the nearest default up the
            // occurrence path, else the root's, else the built-in default),
            // as `body_material` takes it.
            let material = self.body_attributes(uid).material.and_then(|id| {
                let differs = match placements.get(&uid) {
                    Some(paths) => paths
                        .iter()
                        .any(|path| self.inherited_material(uid, Some(path.as_slice())).0.id != id),
                    None => self.inherited_material(uid, None).0.id != id,
                };
                differs.then(|| {
                    self.material(&id)
                        .map_or_else(|| id.clone(), |m| m.name.clone())
                })
            });
            let mut print = Vec::new();
            for group in self.property_store().sets(PropertyOwner::Body(uid)) {
                let Some(set) = print_set_of(&group.name) else {
                    continue;
                };
                for property in &group.properties {
                    let key = property.key.trim();
                    let name = match set {
                        // The filament is the material's.
                        PrintPropertySet::Layer(PrintSet::Filament) => continue,
                        PrintPropertySet::Layer(layer) if same_name(key, PROFILE_KEY) => {
                            match layer {
                                PrintSet::Printer => "Printer profile".to_owned(),
                                _ => "Process profile".to_owned(),
                            }
                        }
                        PrintPropertySet::Layer(_) => print_keys::key(key)
                            .map_or_else(|| key.to_owned(), |d| d.name.to_owned()),
                        PrintPropertySet::Slicer(dialect) => {
                            format!("{key} ({})", dialect.name())
                        }
                    };
                    print.push(name);
                }
            }
            if material.is_some() || !print.is_empty() {
                out.push(json!({
                    "body": uid,
                    "name": body.name,
                    "material": material,
                    "print": print,
                }));
            }
        }
        Value::Array(out)
    }
}

/// A material or process note in words, as refusals name it.
fn words(kind: MeasureKind) -> &'static str {
    match kind {
        MeasureKind::ProcessNote => "process note",
        _ => "material note",
    }
}

/// A process note's lines from a `print_settings` answer: the printer and
/// the process profile chosen (`Printer: Bench`, `Process: Standard`, the
/// slicer's default when none is), then each printer or process value set
/// on a level rather than taken from a profile (`Wall loops: 5`).
pub(super) fn process_lines(settings: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for (layer, title) in [("printer", "Printer"), ("process", "Process")] {
        let name = settings[layer]["profile"]["name"]
            .as_str()
            .unwrap_or(NO_PROFILE);
        lines.push(format!("{title}: {name}"));
    }
    for layer in ["printer", "process"] {
        let Some(values) = settings[layer]["values"].as_array() else {
            continue;
        };
        for value in values.iter().filter(|v| v["from"] == "property") {
            if let (Some(name), Some(text)) = (value["name"].as_str(), value["text"].as_str()) {
                lines.push(format!("{name}: {text}"));
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_process_note_lists_the_profiles_and_the_values_set_on_levels() {
        let settings = json!({
            "printer": {"profile": {"id": "bench", "name": "Bench"},
                        "values": [{"key": "nozzle_diameter", "name": "Nozzle diameter",
                                    "text": "0.4 mm", "from": "profile"}]},
            "process": {"profile": null,
                        "values": [{"key": "layer_height", "name": "Layer height",
                                    "text": "0.12 mm", "from": "property", "level": "body"},
                                   {"key": "fill_density", "name": "Sparse infill density",
                                    "text": "15%", "from": "profile"}]},
        });
        assert_eq!(
            process_lines(&settings),
            [
                "Printer: Bench",
                "Process: Slicer default",
                "Layer height: 0.12 mm"
            ]
        );
    }
}
