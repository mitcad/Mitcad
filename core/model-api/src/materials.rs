// SPDX-License-Identifier: MIT
//! Commands and queries of materials (mitcad#640, `document/materials.rs`):
//! the document's own materials, components' default materials, what a
//! body is made of, and the print key map (`mitcad_model::print_keys`). See
//! `commands.md`, "Materials".

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::internal::document::{library_kind, writable_library};
use mitcad_model::kernel::Kernel;
use mitcad_model::material::MaterialChange;
use mitcad_model::print_keys::{self, Dialect, Form, Kind, PrintSet};
use mitcad_model::print_library::ItemKind;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    // The commands are named by their wire names (create_material, ...).
    #[allow(clippy::enum_variant_names)]
    pub(crate) enum MaterialCommand {
        "create_material" => CreateMaterial(MaterialChange),
        "edit_material" => EditMaterial(MaterialChange),
        "delete_material" => DeleteMaterial {
            id: String,
            /// `user` or `project`: the item is deleted from that library
            /// (mitcad#655).
            #[serde(default)]
            library: Option<String>,
            #[serde(default)]
            project: Option<String>,
        },
        "set_component_material" => SetComponentMaterial {
            /// A component by uid (`C2`, `C0` the root) or name.
            component: String,
            #[serde(default)]
            material: Option<String>,
            /// One of the material's appearances for the component's bodies
            /// without one of their own (mitcad#1055); left out: the
            /// material's default.
            #[serde(default)]
            appearance_id: Option<String>,
            /// `base`, `user` or `project`: the material is that library's
            /// item, copied in (or the design's copy reused) in the same
            /// undo step (mitcad#655).
            #[serde(default)]
            library: Option<String>,
            #[serde(default)]
            project: Option<String>,
        },
        // One filament's separate materials as one with variants
        // (mitcad#1054).
        "merge_materials" => MergeMaterials {
            /// The materials, two or more: the first takes the others.
            ids: Vec<String>,
            /// `user` or `project`: the library's items are merged.
            #[serde(default)]
            library: Option<String>,
            #[serde(default)]
            project: Option<String>,
        },
    }
}

/// The `body_material` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BodyMaterialQuery {
    body: BodyUid,
    /// The occurrence path that places the body (`O1/O4`, or names);
    /// without it, the body's own component and the root count.
    #[serde(default)]
    occurrence: Option<String>,
}

/// The `material_conflicts` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialConflictsQuery {
    bodies: Vec<BodyUid>,
    /// The material they would get; null or left out: their component's.
    #[serde(default)]
    material: Option<String>,
}

/// The `print_keys` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PrintKeysQuery {
    /// `printer`, `process` or `filament`; all three when left out.
    #[serde(default)]
    set: Option<String>,
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait MaterialsCrateOps<K: Kernel> {
    fn run_material_command(&mut self, command: MaterialCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> MaterialsCrateOps<K> for Document<K> {
    fn run_material_command(&mut self, command: MaterialCommand) -> Result<Value, ApiError> {
        Ok(match command {
            MaterialCommand::CreateMaterial(change) => match change.library.as_deref() {
                Some(library) => {
                    let library = writable_library(library)?;
                    let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
                    let (id, path, version) =
                        self.create_library_material(&libraries, library, &change)?;
                    json!({"id": id, "library": library.id(), "version": version,
                           "path": path.to_string_lossy()})
                }
                None => json!({"id": self.create_material(&change)?}),
            },
            MaterialCommand::EditMaterial(change) => {
                let id = change
                    .id
                    .clone()
                    .ok_or_else(|| ApiError("edit_material needs the material's id".to_owned()))?;
                match change.library.as_deref() {
                    Some(library) => {
                        let library = writable_library(library)?;
                        let libraries =
                            self.print_libraries(change.project.as_deref().map(Path::new));
                        let (path, version) =
                            self.edit_library_material(&libraries, library, &id, &change)?;
                        json!({"version": version, "path": path.to_string_lossy()})
                    }
                    None => {
                        self.edit_material(&id, &change)?;
                        json!({})
                    }
                }
            }
            MaterialCommand::DeleteMaterial {
                id,
                library,
                project,
            } => match library.as_deref() {
                Some(library) => {
                    let library = writable_library(library)?;
                    let libraries = self.print_libraries(project.as_deref().map(Path::new));
                    let (path, kept, removed) =
                        self.delete_library_material(&libraries, library, &id)?;
                    json!({"path": path.to_string_lossy(), "kept": kept, "removed": removed})
                }
                None => {
                    let deleted = self.delete_material(&id)?;
                    json!({"bodies": deleted.bodies, "components": deleted.components})
                }
            },
            MaterialCommand::SetComponentMaterial {
                component,
                material,
                appearance_id,
                library,
                project,
            } => {
                let uid = self.component_ref(&component)?;
                let look = appearance_id.as_deref();
                match (library.as_deref(), material) {
                    (Some(library), Some(material)) => {
                        let library = library_kind(library)?;
                        let libraries = self.print_libraries(project.as_deref().map(Path::new));
                        self.as_one_step(|doc| {
                            let id = doc.library_item_in_design(
                                &libraries,
                                library,
                                ItemKind::Material,
                                &material,
                            )?;
                            doc.set_component_material_look(uid, Some(id.as_str()), look)
                        })?;
                    }
                    (Some(_), None) => {
                        return Err(ApiError(
                            "a library is given with the material it has".to_owned(),
                        ));
                    }
                    (None, material) => {
                        self.set_component_material_look(uid, material.as_deref(), look)?
                    }
                }
                json!({})
            }
            MaterialCommand::MergeMaterials {
                ids,
                library,
                project,
            } => match library.as_deref() {
                Some(library) => {
                    let library = writable_library(library)?;
                    let libraries = self.print_libraries(project.as_deref().map(Path::new));
                    let (id, path, version) =
                        self.merge_library_materials(&libraries, library, &ids)?;
                    json!({"id": id, "library": library.id(), "version": version,
                           "path": path.to_string_lossy()})
                }
                None => json!({"id": self.merge_materials(&ids)?}),
            },
        })
    }
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait MaterialsOps<K: Kernel> {
    fn body_material_query(&self, query: &BodyMaterialQuery) -> Result<Value, ApiError>;

    /// The `material_conflicts` query: the bodies whose appearance was set
    /// by hand and would be replaced by the material's, each with its
    /// `name` and `appearance`.
    fn material_conflicts_json(&self, query: &MaterialConflictsQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> MaterialsOps<K> for Document<K> {
    fn body_material_query(&self, query: &BodyMaterialQuery) -> Result<Value, ApiError> {
        let exists = self.body_shape(query.body).is_some()
            || self.state().body_names.contains_key(&query.body);
        if !exists {
            return Err(ApiError(format!("there is no body {}", query.body)));
        }
        let path = match &query.occurrence {
            Some(text) => Some(
                self.assembly()
                    .find_path(text)
                    .ok_or_else(|| ApiError(format!("no occurrence path {text}")))?,
            ),
            None => None,
        };
        Ok(self.body_material_json(query.body, path.as_deref()))
    }

    fn material_conflicts_json(&self, query: &MaterialConflictsQuery) -> Result<Value, ApiError> {
        let conflicts = self.appearance_conflicts(&query.bodies, query.material.as_deref())?;
        let attributes = |uid: BodyUid| self.body_attributes(uid).appearance;
        Ok(json!({
            "bodies": conflicts
                .into_iter()
                .map(|uid| json!({"uid": uid, "name": self.body_name(uid), "appearance": attributes(uid)}))
                .collect::<Vec<_>>(),
        }))
    }
}

/// A range's bounds as `min` and `max`, each only where it is finite
/// (mitcad#746): a missing one is no bound.
fn add_limits(value: &mut Value, min: f64, max: f64) {
    if min.is_finite() {
        value["min"] = json!(min);
    }
    if max.is_finite() {
        value["max"] = json!(max);
    }
}

/// The `print_keys` query: the key map's keys of a set (or all) in display
/// order, each with `key`, `set`, `name`, `kind` (`boolean`, `integer`,
/// `number`, `percent`, `number_or_percent`, `text`, `color`, `choice`,
/// `points`), `unit`, `category`, `group`, `advanced`, `of` (a
/// percentage's basis), `choices` (`{value, name}`), `per_dialect`, for a
/// number `min` and `max` (mitcad#746: what its slicers take, each only
/// where bounded), and `dialects`: `{dialect, key, list}` per slicer that
/// has it, with the dialect's own `min` and `max` where they are narrower;
/// and `layout`: per set its categories and their groups in display order
/// (mitcad#655).
pub(super) fn print_keys_json(query: &PrintKeysQuery) -> Result<Value, ApiError> {
    let set = match query.set.as_deref() {
        None => None,
        Some(text) => Some(
            PrintSet::ALL
                .into_iter()
                .find(|s| s.id() == text)
                .ok_or_else(|| {
                    ApiError(format!(
                        "“{text}” is no set of print settings: printer, process or filament"
                    ))
                })?,
        ),
    };
    let keys: Vec<Value> = print_keys::keys()
        .iter()
        .filter(|k| set.is_none_or(|s| k.set == s))
        .map(|k| {
            let (kind, of, choices) = match k.kind {
                Kind::Boolean => ("boolean", None, None),
                Kind::Integer(_) => ("integer", None, None),
                Kind::Number(_) => ("number", None, None),
                Kind::Percent => ("percent", None, None),
                Kind::OrPercent(_, of) => {
                    ("number_or_percent", (!of.is_empty()).then_some(of), None)
                }
                Kind::Text => ("text", None, None),
                Kind::Color => ("color", None, None),
                Kind::Choice(names) => ("choice", None, Some(names)),
                Kind::Points => ("points", None, None),
            };
            let dialects: Vec<Value> = Dialect::ALL
                .into_iter()
                .filter_map(|d| {
                    let dk = k.dialect(d)?;
                    let key = match dk.form {
                        Form::CuraBed => "machine_width, machine_depth, machine_center_is_zero",
                        _ => dk.key,
                    };
                    let mut entry = json!({"dialect": d.id(), "key": key, "list": dk.list});
                    // Whether the dialect takes a percentage for a number-or-
                    // percentage key (mitcad#655): the editors offer [mm | %]
                    // only where the design's slicer does.
                    if matches!(k.kind, Kind::OrPercent(..)) {
                        let percent = matches!(
                            dk.form,
                            Form::Same | Form::OrPercent(_) | Form::PercentOf(_)
                        );
                        entry["percent"] = json!(percent);
                    }
                    // A dialect's narrower range (mitcad#746).
                    if let Some((min, max)) = k.dialect_limits(d) {
                        add_limits(&mut entry, min, max);
                    }
                    Some(entry)
                })
                .collect();
            let mut value = json!({
                "key": k.key,
                "set": k.set.id(),
                "name": k.name,
                "kind": kind,
                "unit": k.unit().symbol(),
                "category": k.category().id(),
                "group": k.group.id(),
                "advanced": k.advanced,
                "dialects": dialects,
            });
            if let Some(of) = of {
                value["of"] = json!(of);
            }
            if let Some(choices) = choices {
                value["choices"] = choices
                    .iter()
                    .map(|(v, name)| json!({"value": v, "name": name}))
                    .collect();
            }
            if k.per_dialect {
                value["per_dialect"] = json!(true);
            }
            if let Some((min, max)) = k.limits() {
                add_limits(&mut value, min, max);
            }
            value
        })
        .collect();
    let layout: Vec<Value> = PrintSet::ALL
        .into_iter()
        .filter(|s| set.is_none_or(|wanted| *s == wanted))
        .map(|s| {
            let categories: Vec<Value> = print_keys::layout(s)
                .into_iter()
                .map(|(category, groups)| {
                    let groups: Vec<Value> = groups
                        .into_iter()
                        .map(|g| json!({"id": g.id(), "title": g.title()}))
                        .collect();
                    json!({"id": category.id(), "title": category.title(), "groups": groups})
                })
                .collect();
            json!({"set": s.id(), "categories": categories})
        })
        .collect();
    Ok(json!({"keys": keys, "layout": layout}))
}
