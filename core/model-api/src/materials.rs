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
use mitcad_model::NameConflict;
use mitcad_model::ids::BodyUid;
use mitcad_model::internal::document::{library_kind, with_warnings, writable_library};
use mitcad_model::kernel::Kernel;
use mitcad_model::material::{Material, MaterialChange, PRINTING_KEYS, PROPERTY_DEFS, PropertyDef};
use mitcad_model::material_dataset;
use mitcad_model::material_exchange::{self, FileFormat, MaterialRow};
use mitcad_model::material_units::{
    Quantity, ShownUnit, TemperatureUnit, UnitSystem, shown_unit, value_text,
};
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
        // Materials from a JSON or CSV file (mitcad#1213), as one undo step.
        "import_materials" => ImportMaterials {
            /// The file's text: the application reads the file.
            text: String,
            /// `json` or `csv`.
            format: String,
            /// On a name conflict: `keep` (the default), `replace` or
            /// `keep_both`.
            #[serde(default)]
            conflict: Option<String>,
            /// `user` or `project`: into that library; left out: the
            /// design.
            #[serde(default)]
            library: Option<String>,
            #[serde(default)]
            project: Option<String>,
        },
        // A material of each of several entries of the bundled dataset
        // (mitcad#1706), as one undo step; a name there already skipped.
        "create_dataset_materials" => CreateDatasetMaterials {
            /// The entries' ids (`wood/walnut`, `panel/plywood_pine`).
            entries: Vec<String>,
            /// `user` or `project`: into that library; left out: the
            /// design.
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
                    let written = self.create_library_material(&libraries, library, &change)?;
                    with_warnings(
                        json!({"id": written.id, "library": library.id(),
                               "version": written.version, "path": written.path.to_string_lossy()}),
                        written.warnings,
                    )
                }
                None => json!({"id": self.create_material(&change)?}),
            },
            MaterialCommand::EditMaterial(change) => {
                if change.dataset.is_some() {
                    return Err(ApiError("dataset is only for create_material".to_owned()));
                }
                let id = change
                    .id
                    .clone()
                    .ok_or_else(|| ApiError("edit_material needs the material's id".to_owned()))?;
                match change.library.as_deref() {
                    Some(library) => {
                        let library = writable_library(library)?;
                        let libraries =
                            self.print_libraries(change.project.as_deref().map(Path::new));
                        // A missing default appearance kept is a warning
                        // (mitcad#1242).
                        let written =
                            self.edit_library_material(&libraries, library, &id, &change)?;
                        with_warnings(
                            json!({"version": written.version,
                                   "path": written.path.to_string_lossy()}),
                            written.warnings,
                        )
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
                    let written = self.merge_library_materials(&libraries, library, &ids)?;
                    with_warnings(
                        json!({"id": written.id, "library": library.id(),
                               "version": written.version, "path": written.path.to_string_lossy()}),
                        written.warnings,
                    )
                }
                None => {
                    let (id, warnings) = self.merge_materials(&ids)?;
                    with_warnings(json!({"id": id}), warnings)
                }
            },
            MaterialCommand::ImportMaterials {
                text,
                format,
                conflict,
                library,
                project,
            } => {
                let format = FileFormat::from_id(&format).ok_or_else(|| {
                    ApiError(format!(
                        "{format} is no format of a materials file: json or csv"
                    ))
                })?;
                let conflict = NameConflict::parse(conflict.as_deref().unwrap_or("keep"))?;
                let library = library.as_deref().map(writable_library).transpose()?;
                let outcome = self.import_materials(
                    &text,
                    format,
                    conflict,
                    library,
                    project.as_deref().map(Path::new),
                )?;
                let errors: Vec<Value> = outcome
                    .errors
                    .iter()
                    .map(|e| json!({"row": e.row, "name": e.name, "message": e.message}))
                    .collect();
                json!({"added": outcome.added, "replaced": outcome.replaced,
                       "kept": outcome.kept, "conflicts": outcome.conflicts, "errors": errors})
            }
            MaterialCommand::CreateDatasetMaterials {
                entries,
                library,
                project,
            } => {
                let library = library.as_deref().map(writable_library).transpose()?;
                let made = self.create_dataset_materials(
                    &entries,
                    library,
                    project.as_deref().map(Path::new),
                )?;
                json!({"added": made.added, "skipped": made.skipped})
            }
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

    /// The design's unit system for its materials' values (mitcad#1213):
    /// its length unit's.
    fn material_system(&self) -> UnitSystem;

    /// The `material_units` query (mitcad#1213).
    fn material_units_json(&self, query: &MaterialUnitsQuery) -> Result<Value, ApiError>;

    /// The `export_materials` query (mitcad#1213).
    fn export_materials_json(&self, query: &ExportMaterialsQuery) -> Result<Value, ApiError>;
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

    fn material_system(&self) -> UnitSystem {
        UnitSystem::of_length(self.parameters().context().default_length_unit)
    }

    fn material_units_json(&self, query: &MaterialUnitsQuery) -> Result<Value, ApiError> {
        let system = match query.units.as_deref() {
            Some(text) => system_of(text)?,
            None => self.material_system(),
        };
        let temperature = temperature_of(query.temperature.as_deref())?;
        let unit = |q: Quantity| unit_json(shown_unit(q, system, temperature));
        let mut quantities = serde_json::Map::new();
        for q in Quantity::WITH_UNITS.into_iter().chain([Quantity::Ratio]) {
            quantities.insert(q.id().to_owned(), unit(q));
        }
        let properties: Vec<Value> = PROPERTY_DEFS
            .iter()
            .map(|d| {
                json!({"key": d.key, "name": d.name, "group": d.group.id(),
                       "group_title": d.group.title(), "quantity": d.quantity.id(),
                       "unit": unit(d.quantity)})
            })
            .collect();
        let printing: Vec<Value> = PRINTING_KEYS
            .iter()
            .map(|(key, name)| {
                json!({"key": key, "name": name, "quantity": Quantity::Temperature.id(),
                       "unit": unit(Quantity::Temperature)})
            })
            .collect();
        Ok(json!({
            "units": system.id(),
            "temperature": temperature.id(),
            "quantities": quantities,
            "properties": properties,
            "printing": printing,
        }))
    }

    fn export_materials_json(&self, query: &ExportMaterialsQuery) -> Result<Value, ApiError> {
        let format = FileFormat::from_id(&query.format).ok_or_else(|| {
            ApiError(format!(
                "{} is no format of a materials file: json or csv",
                query.format
            ))
        })?;
        let system = match query.units.as_deref() {
            Some(text) => system_of(text)?,
            None => self.material_system(),
        };
        let temperature = temperature_of(query.temperature.as_deref())?;
        let mut chosen: Vec<Material> = match &query.ids {
            Some(ids) => ids
                .iter()
                .map(|id| {
                    self.material(id)
                        .cloned()
                        .ok_or_else(|| ApiError(format!("there is no material {id}")))
                })
                .collect::<Result<_, _>>()?,
            None if query.materials.is_empty() => self.custom_materials().to_vec(),
            None => Vec::new(),
        };
        for (i, given) in query.materials.iter().enumerate() {
            chosen.push(
                given_material(given)
                    .map_err(|e| ApiError(format!("materials[{i}]: not a material: {e}")))?,
            );
        }
        let text = material_exchange::write(&chosen, format, system, temperature);
        Ok(
            json!({"text": text, "count": chosen.len(), "format": format.id(),
                  "units": system.id(), "temperature": temperature.id()}),
        )
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

/// The `material_units` query (mitcad#1213).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialUnitsQuery {
    /// `metric` or `imperial`; the design's (its length unit's) when left
    /// out.
    #[serde(default)]
    units: Option<String>,
    /// `C` or `F`; the preferred unit when left out.
    #[serde(default)]
    temperature: Option<String>,
}

/// The `material_dataset` query (mitcad#1213).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialDatasetQuery {
    /// Words each listed material's name, material, grade or id holds
    /// (any case); all when left out.
    #[serde(default)]
    search: Option<String>,
}

/// The `export_materials` query (mitcad#1213).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExportMaterialsQuery {
    /// `json` or `csv`.
    format: String,
    /// The materials by id, the design's and the built-in ones; the
    /// design's own when left out and no `materials` are given.
    #[serde(default)]
    ids: Option<Vec<String>>,
    /// Materials given whole (a library's items as `library_items` gives
    /// them), after those of `ids`.
    #[serde(default)]
    materials: Vec<Value>,
    /// `metric` or `imperial`; the design's when left out.
    #[serde(default)]
    units: Option<String>,
    /// `C` or `F`; the preferred unit when left out.
    #[serde(default)]
    temperature: Option<String>,
}

/// A unit system by its id.
fn system_of(text: &str) -> Result<UnitSystem, ApiError> {
    UnitSystem::from_id(text)
        .ok_or_else(|| ApiError(format!("{text} is no unit system: metric or imperial")))
}

/// A temperature unit by its id; the preferred one when none is given
/// (mitcad#1213: the application's, °C until it sets one).
pub(super) fn temperature_of(text: Option<&str>) -> Result<TemperatureUnit, ApiError> {
    match text {
        None => Ok(mitcad_model::material_units::preferred_temperature()),
        Some(text) => TemperatureUnit::from_id(text)
            .ok_or_else(|| ApiError(format!("{text} is no temperature unit: C or F"))),
    }
}

/// A unit as the queries give it: its symbol, and how a value in the
/// model's unit becomes one in it (`value * scale + offset`).
fn unit_json(unit: ShownUnit) -> Value {
    json!({"symbol": unit.symbol, "scale": unit.scale, "offset": unit.offset})
}

/// The fields of a material a query or a library item lists that the
/// material keeps (the `materials` query adds others: `built_in`,
/// `bodies`, ...).
const MATERIAL_FIELDS: [&str; 14] = [
    "id",
    "name",
    "density",
    "appearance",
    "appearances",
    "filament",
    "slicer",
    "variants",
    "based_on",
    "class",
    "version",
    "origin",
    "properties",
    "reference",
];

/// A material given whole, the fields it does not keep left out.
fn given_material(value: &Value) -> Result<Material, String> {
    let Value::Object(fields) = value else {
        return Err("an object".to_owned());
    };
    let mut kept = fields.clone();
    kept.retain(|key, _| MATERIAL_FIELDS.contains(&key.as_str()));
    serde_json::from_value(Value::Object(kept)).map_err(|e| e.to_string())
}

/// A material's value by a word (mitcad#1213: a parts list's column, a
/// query's key): its property's key or name (any case: `youngs_modulus`,
/// `Young's modulus`), or `density`; as text in `system`'s units with
/// temperatures in `temperature` (`200000 MPa`). None for another word or
/// a value the material does not give.
pub(super) fn material_value_text(
    material: &Material,
    word: &str,
    system: UnitSystem,
    temperature: TemperatureUnit,
) -> Option<String> {
    let word = word.trim();
    if word.eq_ignore_ascii_case("density") {
        let unit = shown_unit(Quantity::Density, system, temperature);
        return Some(value_text(material.density, unit, 6));
    }
    let def: &PropertyDef = PROPERTY_DEFS
        .iter()
        .find(|d| d.key.eq_ignore_ascii_case(word) || d.name.eq_ignore_ascii_case(word))?;
    let value = material.properties.get(def.key)?;
    Some(value_text(
        value,
        shown_unit(def.quantity, system, temperature),
        6,
    ))
}

/// The rows Physical Properties shows of a material (mitcad#1213): each
/// property it gives and its print settings' extruder and bed temperature,
/// `{key, name, group, text}`, in `system`'s units with temperatures in
/// `temperature`. A value not given has no row.
pub(super) fn material_property_rows(
    material: &Material,
    system: UnitSystem,
    temperature: TemperatureUnit,
) -> Vec<Value> {
    let mut rows: Vec<Value> = PROPERTY_DEFS
        .iter()
        .filter_map(|d| {
            let text = material_value_text(material, d.key, system, temperature)?;
            Some(json!({"key": d.key, "name": d.name, "group": d.group.id(), "text": text}))
        })
        .collect();
    let unit = shown_unit(Quantity::Temperature, system, temperature);
    for (key, name) in PRINTING_KEYS {
        if let Some(value) = material.filament.get(key).and_then(Value::as_f64) {
            rows.push(json!({"key": key, "name": name, "group": "printing",
                             "text": value_text(value, unit, 6)}));
        }
    }
    rows
}

/// The `material_dataset` query (mitcad#1213): the bundled dataset's name,
/// licence and caution, its `sources` (`{name, license}`: the third-party
/// dataset and MitCad's own woods and panels, mitcad#1552), and its
/// materials a search finds, each with its `id`, `name`, `class`,
/// `density`, `properties`, `filament` and `reference`, in the model's
/// units.
pub(super) fn material_dataset_json(query: &MaterialDatasetQuery) -> Value {
    let rows: Vec<&MaterialRow> = material_dataset::search(query.search.as_deref().unwrap_or(""));
    let materials: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.id,
                "name": row.name,
                "class": row.class.map(|c| c.id()),
                "density": row.density,
                "properties": row.properties,
                "filament": row.filament,
                "reference": row.reference,
            })
        })
        .collect();
    json!({
        "name": material_dataset::NAME,
        "license": material_dataset::LICENSE_ID,
        "caution": material_dataset::CAUTION,
        "sources": material_dataset::sources()
            .iter()
            .map(|(name, license)| json!({"name": name, "license": license}))
            .collect::<Vec<_>>(),
        "count": material_dataset::entries().len(),
        "materials": materials,
    })
}

/// The material a component's rows show (mitcad#1213): the one all its own
/// bodies are made of, else, without bodies, its default material; none
/// when its bodies differ.
pub(super) fn component_made_of<K: Kernel>(
    doc: &Document<K>,
    component: mitcad_model::ids::ComponentUid,
) -> Option<&Material> {
    let mut found: Option<&Material> = None;
    for body in doc.bodies() {
        if doc.body_component(body.uid) != Some(component) {
            continue;
        }
        let made_of = doc.body_material(body.uid, None).0;
        match found {
            Some(other) if other.id != made_of.id => return None,
            _ => found = Some(made_of),
        }
    }
    found.or_else(|| {
        doc.component_material(component)
            .and_then(|id| doc.material(id))
    })
}

#[cfg(test)]
mod material_value_tests {
    use super::*;

    #[test]
    fn a_materials_values_are_written_in_the_designs_units() {
        let mut steel = mitcad_model::material::built_in_material("steel")
            .unwrap()
            .clone();
        steel.properties.youngs_modulus = Some(200_000.0);
        steel.properties.max_service_temperature = Some(100.0);
        let c = TemperatureUnit::Celsius;
        let f = TemperatureUnit::Fahrenheit;
        let text = |word: &str, system, t| {
            material_value_text(&steel, word, system, t).unwrap_or_default()
        };
        assert_eq!(text("Young's modulus", UnitSystem::Metric, c), "200000 MPa");
        assert_eq!(
            text("youngs_modulus", UnitSystem::Imperial, c),
            "29007500 psi"
        );
        assert_eq!(text("density", UnitSystem::Metric, c), "7.85 g/cm³");
        assert_eq!(text("Density", UnitSystem::Imperial, c), "0.283599 lb/in³");
        assert_eq!(
            text("max_service_temperature", UnitSystem::Metric, f),
            "212 °F"
        );
        // Not given: none, never 0.
        assert_eq!(text("yield strength", UnitSystem::Metric, c), "");
        assert_eq!(text("hardness", UnitSystem::Metric, c), "");
        let rows = material_property_rows(&steel, UnitSystem::Metric, c);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["name"], "Young's modulus");
    }
}
