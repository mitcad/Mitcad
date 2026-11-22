// SPDX-License-Identifier: MIT
//! The `print_service_check` query (mitcad#2214, `commands.md`, "Print
//! service check"): which bodies of the design, or of the bodies and
//! components chosen, a cloud print service may take. A library may say
//! that its components are not for print services (the manifest's
//! `non_printable`, kept on each part's library record in the design); the
//! bodies placed inside such a component (also nested, also a BUILD part
//! of such a source) may not go, every other body may
//! (`document/print_service.rs` decides it per body and place). The answer
//! names each such part in words: the component, where it is placed and
//! its library. The user's own export and Send to Slicer do not ask it.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::physical_extent::{ComponentArg, PhysicalExtentOps};
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::{BodyUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, PrintServiceBlock};

/// The bodies to check: those of `bodies` and those of `components` (with
/// their sub-components', as the `properties` query takes them); every
/// body of the design when both are empty.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrintServiceQuery {
    #[serde(default)]
    bodies: Vec<BodyUid>,
    #[serde(default)]
    components: Vec<ComponentArg>,
}

/// Answers the query: `printable` and `not_printable` (body uids),
/// `bodies` (each with its places), `parts` (each non-printable part's
/// placement in words, once), `components` (the components asked for)
/// and `message`.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &PrintServiceQuery,
) -> Result<Value, ApiError> {
    let scope = doc.properties_scope(&query.bodies, &query.components)?;
    let assembly = doc.assembly();
    let mut printable = Vec::new();
    let mut not_printable = Vec::new();
    let mut bodies = Vec::new();
    // Each non-printable part's placement once: its component and the
    // occurrences that place it.
    let mut seen: BTreeSet<(String, Vec<OccurrenceUid>)> = BTreeSet::new();
    let mut parts = Vec::new();
    for (view, placements) in &scope.bodies {
        // A body placed nowhere is checked by its own component only.
        let paths: Vec<Vec<OccurrenceUid>> = if placements.is_empty() {
            vec![Vec::new()]
        } else {
            placements.iter().map(|p| p.path.clone()).collect()
        };
        let mut places = Vec::new();
        let mut allowed = true;
        for path in &paths {
            let block = doc.print_service_block(view.uid, path);
            let mut place = json!({"printable": block.is_none()});
            if !path.is_empty() {
                place["path"] = json!(OccurrencePath(path.clone()).to_string());
                place["path_name"] = json!(assembly.path_name(path));
            }
            if let Some(block) = block {
                allowed = false;
                let part = part_json(doc, &block, path, &view.name);
                place["component"] = json!(block.component);
                place["component_name"] = json!(assembly.name(block.component));
                place["library"] = json!(block.record.library);
                place["message"] = part["message"].clone();
                let key = (block.component.to_string(), path[..block.depth].to_vec());
                if seen.insert(key) {
                    parts.push(part);
                }
            }
            // A body placed nowhere has no places to list.
            if !placements.is_empty() {
                places.push(place);
            }
        }
        let mut body = json!({
            "body": view.uid,
            "name": view.name,
            "printable": allowed,
            "places": places,
        });
        if let Some(component) = doc.body_component(view.uid).filter(|c| !c.is_root()) {
            body["component"] = json!(component);
            body["component_name"] = json!(assembly.name(component));
        }
        bodies.push(body);
        if allowed {
            printable.push(view.uid);
        } else {
            not_printable.push(view.uid);
        }
    }
    let message = message(printable.len(), not_printable.len(), &parts);
    Ok(json!({
        "printable": printable,
        "not_printable": not_printable,
        "bodies": bodies,
        "parts": parts,
        "components": scope.components,
        "message": message,
    }))
}

/// A non-printable part in words: `{"component", "component_name",
/// "occurrence" (its path by names; none for the root and a part placed
/// nowhere), "library", "designation", "license", "message"}`. The message
/// names the part (its name, with its designation when that differs; the
/// body's name for a part made in the root), where it is placed and its
/// library, without ids.
fn part_json<K: Kernel>(
    doc: &Document<K>,
    block: &PrintServiceBlock,
    path: &[OccurrenceUid],
    body_name: &str,
) -> Value {
    let assembly = doc.assembly();
    let record = &block.record;
    let name = if block.component.is_root() {
        body_name.to_owned()
    } else {
        assembly.name(block.component).to_owned()
    };
    let designation = record.designation.trim();
    let named = if designation.is_empty() || designation == name {
        name.clone()
    } else {
        format!("{name} ({designation})")
    };
    let occurrence = (block.depth > 0).then(|| assembly.path_name(&path[..block.depth]));
    // One whole message per case (docs/translations.md).
    let library = &record.library;
    let message = match &occurrence {
        Some(at) => {
            format!("{named} at {at} from the library {library}: not for print services")
        }
        None => format!("{named} from the library {library}: not for print services"),
    };
    let mut value = json!({
        "component": block.component,
        "component_name": assembly.name(block.component),
        "library": record.library,
        "designation": record.designation,
        "license": record.license,
        "message": message,
    });
    if let Some(occurrence) = occurrence {
        value["occurrence"] = json!(occurrence);
    }
    value
}

/// The answer in words: how many bodies may go, and which parts may not
/// (`parts`' messages). One whole message per case, with only numbers and
/// messages as values (docs/translations.md).
fn message(printable: usize, blocked: usize, parts: &[Value]) -> String {
    let all = printable + blocked;
    let named = parts
        .iter()
        .filter_map(|p| p["message"].as_str())
        .collect::<Vec<_>>()
        .join("; ");
    match (printable, blocked) {
        (0, 0) => "There are no bodies to check.".to_owned(),
        (1, 0) => "The body may be sent to a print service.".to_owned(),
        (_, 0) => format!("All {all} bodies may be sent to a print service."),
        (0, 1) => format!("The body may not be sent to a print service: {named}."),
        (0, _) => format!("None of the {all} bodies may be sent to a print service: {named}."),
        (_, 1) => format!(
            "1 body of {all} is not for print services and is left out: {named}. The other \
             {printable} may be sent to a print service."
        ),
        _ => format!(
            "{blocked} bodies of {all} are not for print services and are left out: {named}. \
             The other {printable} may be sent to a print service."
        ),
    }
}
