// SPDX-License-Identifier: MIT
//! Commands and queries of configuration tables and library parts
//! (mitcad#64, mitcad#63; `commands.md`, "Libraries and configurations").
//! Inserting a library part is `insert_component` with `library`
//! (`components.rs`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::configurations::Configurations;
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, LibraryChange, LibraryPart};
use mitcad_model_build::prelude::*;

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
    /// The standard the part is named by when its row names several
    /// (mitcad#949, the Standard filter's: `DIN 912`).
    #[serde(default)]
    standard: String,
}

impl LibraryInput {
    pub(crate) fn part(self) -> LibraryPart {
        LibraryPart {
            library: self.id,
            url: self.url,
            rev: self.rev,
            component: self.component,
            config: self.config,
            standard: self.standard,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeInput {
    #[serde(default)]
    component: Option<String>,
    /// A BUILD library source instead of a component (mitcad#375).
    #[serde(default)]
    source: Option<mitcad_model_build::SourceId>,
    #[serde(default)]
    rev: Option<String>,
    #[serde(default)]
    config: Option<String>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum LibraryCommand {
        /// The design's configuration table; null or an empty table removes
        /// it.
        "set_configurations" => SetConfigurations {
            #[serde(default)]
            configurations: Option<Configurations>,
        },
        "apply_configuration" => ApplyConfiguration {
            name: String,
        },
        /// Linked library parts to other versions or rows, one undo step.
        "update_library_parts" => UpdateLibraryParts {
            changes: Vec<ChangeInput>,
        },
        /// Option values chosen, with the cascade of the rules (mitcad#797,
        /// `configurations.rs`).
        "configure" => Configure {
            values: serde_json::Map<String, Value>,
        },
        /// One configuration row per key of a table column, setting an
        /// option to it (mitcad#806).
        "generate_configuration_rows" => GenerateConfigurationRows {
            table: String,
            column: String,
            option: String,
        },
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait LibrariesCrateOps<K: Kernel> {
    fn run_library_command(&mut self, command: LibraryCommand) -> Result<Value, ApiError>;

    /// The `configurations` query: the table, the values of each selector,
    /// the row the parameters have now and what is wrong with the table.
    fn configurations_json(&self) -> Value;

    /// The `library_parts` query: the components that came from
    /// libraries.
    fn library_parts_json(&self) -> Value;

    /// The `parts_list` query (a bill of materials).
    fn parts_list_json(&self) -> Value;

    /// The `parts_list` query with columns of property values (mitcad#496):
    /// each row's `properties` has the text of each key (`Key` or
    /// `Set/Key`) the component has, stored, from its library or computed.
    fn parts_list_with(&self, keys: &[String]) -> Value;

    /// The `library_usage` query (mitcad#418, mitcad#518): the library parts
    /// the design uses, BUILD sources and inserted library components
    /// alike, one entry per library and component (whatever its version)
    /// with how many times the design places it, in the order the parts
    /// list has them, and `rows`: the same count per configuration row
    /// (the row each use was placed in, in the order they first appear).
    /// Uses placed without a row (the default row) are in `count` only.
    /// Parts placed nowhere are left out.
    fn library_usage_json(&self) -> Value;
}

impl<K: Kernel> LibrariesCrateOps<K> for Document<K> {
    fn run_library_command(&mut self, command: LibraryCommand) -> Result<Value, ApiError> {
        Ok(match command {
            LibraryCommand::SetConfigurations { configurations } => {
                let set = self.set_configurations(configurations.unwrap_or_default())?;
                json!({
                    "rows": self.configurations().rows.len(),
                    "options": set.options,
                    "added": set.added,
                    "warnings": set.warnings,
                })
            }
            LibraryCommand::ApplyConfiguration { name } => {
                let changed = self.apply_configuration(&name)?;
                json!({
                    "changed": changed,
                    "moves": super::configurations::moves_json(self.configuration_moves()),
                })
            }
            LibraryCommand::Configure { values } => self.configure_command(&values)?,
            LibraryCommand::GenerateConfigurationRows {
                table,
                column,
                option,
            } => {
                let generated = self.generate_configuration_rows(&table, &column, &option)?;
                json!({
                    "rows": generated.rows,
                    "added": generated.added,
                    "updated": generated.updated,
                    "moves": super::configurations::moves_json(self.configuration_moves()),
                })
            }
            LibraryCommand::UpdateLibraryParts { changes } => {
                // BUILD's library sources (mitcad#375).
                if changes.iter().any(|c| c.source.is_some()) {
                    let mut sources = Vec::new();
                    for c in changes {
                        let (Some(source), None, None) = (c.source, &c.component, &c.config) else {
                            return Err(ApiError(
                                "a change names a source or a component; a source's rows are its \
                                 parts' own (build_edit's config)"
                                    .to_owned(),
                            ));
                        };
                        sources.push((source, c.rev));
                    }
                    let lines = self.update_library_sources(&sources)?;
                    return Ok(json!({"changes": lines}));
                }
                let changes = changes
                    .into_iter()
                    .map(|c| {
                        let component = c
                            .component
                            .as_deref()
                            .ok_or_else(|| ApiError("a change names its component".to_owned()))?;
                        Ok(LibraryChange {
                            component: self.component_ref(component)?,
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

    fn configurations_json(&self) -> Value {
        let table = self.configurations();
        json!({
            "configurations": if table.is_empty() { Value::Null } else { json!(table) },
            "selector_values": table.selector_values(),
            "current": table.current(self.parameters()).map(|row| row.name.clone()),
            "problems": table.problems(self.parameters()),
        })
    }

    fn library_parts_json(&self) -> Value {
        let a = self.assembly();
        let parts: Vec<Value> = a
            .components
            .iter()
            .filter_map(|c| {
                let library = c.library.as_ref()?;
                let mut part = json!({
                    "component": c.uid,
                    "name": c.name,
                    "linked": c.link.is_some(),
                    "occurrences": a.occurrences_of(c.uid).count(),
                    "library": library,
                    "version": library.version_text(),
                });
                // Inserted with options (mitcad#801).
                if let Some(options) = super::component_options::options_json(self, c.uid) {
                    part["options"] = options;
                }
                Some(part)
            })
            .collect();
        let mut parts = parts;
        // BUILD's library sources (mitcad#375), with the derived parts
        // made from them.
        for source in self.state().library_sources.iter() {
            let uses: Vec<mitcad_model::ids::FeatureUid> = self
                .features()
                .filter(|f| matches!(&f.def, mitcad_model::features::FeatureDef::DerivedPart(d) if d.source == source.id))
                .map(|f| f.uid)
                .collect();
            let l = &source.library;
            parts.push(json!({
                "source": source.id,
                "component": null,
                "name": source.label(),
                "library": l,
                "url": mitcad_model::library::shown_url(&l.url),
                "rev": l.rev,
                "label": l.label,
                "designation": l.designation,
                "build": source.build,
                "supplier": source.supplier,
                "part_number": source.part_number,
                "supplier_url": source.supplier_url,
                "config": null,
                "version": l.version_text(),
                "uses": uses,
            }));
        }
        json!({"parts": parts})
    }

    fn parts_list_json(&self) -> Value {
        self.parts_list_with(&[])
    }

    fn parts_list_with(&self, keys: &[String]) -> Value {
        let list = self.parts_list();
        // The parts to buy for printable bearings (mitcad#577) follow.
        let purchased = self.purchased_rows(&list);
        let mut rows: Vec<Value> = list
            .into_iter()
            .map(|row| {
                let library = row.library.as_ref();
                let mut value = json!({
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
                    "url": library.map(|l| mitcad_model::library::shown_url(&l.url)),
                    "supplier": row.supplier,
                    "part_number": row.part_number,
                    "supplier_url": row.supplier_url,
                    // The suppliers' articles of its row (mitcad#949), and
                    // as the parts list's column.
                    "supplier_parts": row.supplier_parts,
                    "supplier_parts_text":
                        mitcad_model::library::supplier_parts_text(&row.supplier_parts),
                });
                // A component with options (mitcad#801).
                if let Some(options) = super::component_options::options_json(self, row.component) {
                    value["options"] = options;
                }
                if !keys.is_empty() {
                    let mut properties = serde_json::Map::new();
                    for key in keys {
                        let text = self.component_property_text(row.component, &row, key);
                        properties.insert(key.clone(), json!(text));
                    }
                    value["properties"] = Value::Object(properties);
                }
                value
            })
            .collect();
        for mut row in purchased {
            if !keys.is_empty() {
                let none: serde_json::Map<String, Value> =
                    keys.iter().map(|k| (k.clone(), Value::Null)).collect();
                row["properties"] = Value::Object(none);
            }
            rows.push(row);
        }
        json!({"rows": rows})
    }

    fn library_usage_json(&self) -> Value {
        struct Usage {
            library: String,
            url: String,
            component: String,
            count: usize,
            rows: Vec<(String, usize)>,
        }
        let mut parts: Vec<Usage> = Vec::new();
        for row in self.parts_list() {
            let Some(library) = row.library.as_ref() else {
                continue;
            };
            if row.quantity == 0 {
                continue;
            }
            let at = match parts
                .iter()
                .position(|p| p.library == library.library && p.component == library.component)
            {
                Some(at) => at,
                None => {
                    parts.push(Usage {
                        library: library.library.clone(),
                        url: mitcad_model::library::shown_url(&library.url),
                        component: library.component.clone(),
                        count: 0,
                        rows: Vec::new(),
                    });
                    parts.len() - 1
                }
            };
            let part = &mut parts[at];
            part.count += row.quantity;
            if let Some(config) = library.config.as_ref().filter(|c| !c.is_empty()) {
                match part.rows.iter_mut().find(|r| &r.0 == config) {
                    Some(counted) => counted.1 += row.quantity,
                    None => part.rows.push((config.clone(), row.quantity)),
                }
            }
        }
        let parts: Vec<Value> = parts
            .into_iter()
            .map(|part| {
                let rows: Vec<Value> = part
                    .rows
                    .into_iter()
                    .map(|(config, count)| json!({"config": config, "count": count}))
                    .collect();
                json!({"library": part.library, "url": part.url, "component": part.component,
                       "count": part.count, "rows": rows})
            })
            .collect();
        json!({ "parts": parts })
    }
}
