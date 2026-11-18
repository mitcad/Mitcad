// SPDX-License-Identifier: MIT
//! New content in sight (mitcad#1995, GitHub mitcad/Mitcad#430): a command
//! that adds content (`add_feature`, `create_component`,
//! `insert_component`, `import_file`, `paste_new`) to a component that
//! only hidden occurrences place shows those occurrences again, as part of
//! its own undo step, and says so in its reply: `shown_parents`, each
//! `occurrence` (uid), `name` (`Fairlead:1`) and `path` (its names from
//! the root). The application says it in the status bar; nothing else
//! changes.
//!
//! What the command added is told from the bodies and occurrences it did
//! not have before: a new occurrence's component, a new body's component.
//! The model's `Document::show_hiding_occurrences` decides what hides
//! them (`mitcad_model`'s `document/hidden_parents.rs`).

use std::collections::BTreeSet;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;

use crate::ApiError;

/// The commands that add content to a component.
pub(crate) fn adds_content(cmd: Option<&str>) -> bool {
    matches!(
        cmd,
        Some("add_feature" | "create_component" | "insert_component" | "import_file" | "paste_new")
    )
}

/// The bodies, occurrences and undo depth before a command.
pub(crate) struct ContentBefore {
    bodies: BTreeSet<BodyUid>,
    occurrences: BTreeSet<OccurrenceUid>,
    depth: usize,
}

impl ContentBefore {
    pub(crate) fn of<K: Kernel>(doc: &Document<K>) -> Self {
        Self {
            bodies: doc.body_uids(),
            occurrences: doc.assembly().occurrences.iter().map(|o| o.uid).collect(),
            depth: doc.undo_depth(),
        }
    }
}

/// After a command that added content: the components it added to that
/// only hidden occurrences place, those occurrences shown and the step
/// merged into the command's last one; `shown_parents` in its reply.
/// Nothing when the command made no undo step or all is in sight.
pub(crate) fn show_new_content<K: Kernel>(
    doc: &mut Document<K>,
    reply: &mut Value,
    before: &ContentBefore,
) -> Result<(), ApiError> {
    let depth = doc.undo_depth();
    if depth <= before.depth {
        return Ok(());
    }
    let mut components: Vec<ComponentUid> = Vec::new();
    for o in &doc.assembly().occurrences {
        if !before.occurrences.contains(&o.uid) && !components.contains(&o.component) {
            components.push(o.component);
        }
    }
    for body in doc.body_uids() {
        if before.bodies.contains(&body) {
            continue;
        }
        if let Some(component) = doc.body_component(body)
            && !components.contains(&component)
        {
            components.push(component);
        }
    }
    if components.is_empty() {
        return Ok(());
    }
    let label = doc.undo_label().unwrap_or_default().to_owned();
    let shown = doc.show_hiding_occurrences(&components, &before.occurrences)?;
    if shown.is_empty() {
        return Ok(());
    }
    // One undo step with the command's: undo takes both back.
    doc.merge_undo(depth - 1, &label);
    let a = doc.assembly();
    let parents: Vec<Value> = shown
        .iter()
        .map(|o| {
            let path = a
                .paths_to(
                    a.occurrence(*o)
                        .map_or(ComponentUid::ROOT, |occurrence| occurrence.parent),
                )
                .into_iter()
                .next()
                .map(|mut path| {
                    path.push(*o);
                    a.path_name(&path)
                })
                .unwrap_or_else(|| a.occurrence_name(*o));
            json!({"occurrence": o, "name": a.occurrence_name(*o), "path": path})
        })
        .collect();
    reply["shown_parents"] = json!(parents);
    Ok(())
}
