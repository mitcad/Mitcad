// SPDX-License-Identifier: MIT
//! Commands and queries of configuration tables and library parts
//! (mitcad#64, mitcad#63; `commands.md`, "Libraries and configurations").
//! Inserting a library part is `insert_component` with `library`
//! (`components.rs`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::configurations::Configurations;
use crate::document::{Document, LibraryChange, LibraryPart};
use crate::kernel::Kernel;

/// A library part in `insert_component`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LibraryInput {
    /// The library's id.
    id: String,
    #[serde(default)]
    url: String,
    rev: String,
    component: String,
    #[serde(default)]
    config: Option<String>,
}

impl LibraryInput {
    pub(crate) fn part(self) -> LibraryPart {
        LibraryPart {
            library: self.id,
            url: self.url,
            rev: self.rev,
            component: self.component,
            config: self.config,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeInput {
    component: String,
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    config: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum LibraryCommand {
    /// The design's configuration table; null or an empty table removes
    /// it.
    SetConfigurations {
        #[serde(default)]
        configurations: Option<Configurations>,
    },
    ApplyConfiguration {
        name: String,
    },
    /// Linked library parts to other versions or rows, one undo step.
    UpdateLibraryParts {
        changes: Vec<ChangeInput>,
    },
}

/// The commands' names, to send them here.
pub(crate) const COMMANDS: [&str; 3] = [
    "set_configurations",
    "apply_configuration",
    "update_library_parts",
];

impl<K: Kernel> Document<K> {
    pub(crate) fn run_library_command(
        &mut self,
        command: LibraryCommand,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            LibraryCommand::SetConfigurations { configurations } => {
                self.set_configurations(configurations.unwrap_or_default())?;
                json!({"rows": self.configurations().rows.len()})
            }
            LibraryCommand::ApplyConfiguration { name } => {
                let changed = self.apply_configuration(&name)?;
                json!({"changed": changed})
            }
            LibraryCommand::UpdateLibraryParts { changes } => {
                let changes = changes
                    .into_iter()
                    .map(|c| {
                        Ok(LibraryChange {
                            component: self.component_ref(&c.component)?,
                            rev: c.rev,
                            config: c.config,
                        })
                    })
                    .collect::<Result<Vec<_>, ApiError>>()?;
                let lines = self.update_library_parts(&changes)?;
                json!({"changes": lines})
            }
        })
    }

    /// The `configurations` query: the table, the values of each selector,
    /// the row the parameters have now and what is wrong with the table.
    pub(crate) fn configurations_json(&self) -> Value {
        let table = self.configurations();
        json!({
            "configurations": if table.is_empty() { Value::Null } else { json!(table) },
            "selector_values": table.selector_values(),
            "current": table.current(self.parameters()).map(|row| row.name.clone()),
            "problems": table.problems(self.parameters()),
        })
    }

    /// The `library_parts` query: the components that came from
    /// libraries.
    pub(crate) fn library_parts_json(&self) -> Value {
        let a = self.assembly();
        let parts: Vec<Value> = a
            .components
            .iter()
            .filter_map(|c| {
                let library = c.library.as_ref()?;
                Some(json!({
                    "component": c.uid,
                    "name": c.name,
                    "linked": c.link.is_some(),
                    "occurrences": a.occurrences_of(c.uid).count(),
                    "library": library,
                    "version": library.version_text(),
                }))
            })
            .collect();
        json!({"parts": parts})
    }

    /// The `parts_list` query (a bill of materials).
    pub(crate) fn parts_list_json(&self) -> Value {
        let rows: Vec<Value> = self
            .parts_list()
            .into_iter()
            .map(|row| {
                let library = row.library.as_ref();
                json!({
                    "component": row.component,
                    "name": row.name,
                    "designation": row.designation,
                    "quantity": row.quantity,
                    "linked": row.linked,
                    "library": library.map(|l| l.library.clone()),
                    "version": library.map(|l| l.version_text()),
                    "config": library.and_then(|l| l.config.clone()),
                    "license": library.map(|l| l.license.clone()),
                    "authors": library.map(|l| l.authors.clone()),
                    "url": library.map(|l| crate::library::shown_url(&l.url)),
                })
            })
            .collect();
        json!({"rows": rows})
    }
}
