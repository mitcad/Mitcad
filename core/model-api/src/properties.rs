// SPDX-License-Identifier: MIT
//! Commands and queries of typed property sets (mitcad#496, see
//! `commands.md`, "Properties").

use std::path::Path;

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::ids::ComponentUid;
use mitcad_model::internal::document::library_kind;
use mitcad_model::kernel::Kernel;
use mitcad_model::print_library::ItemKind;
use mitcad_model::print_profile::{PROFILE_KEY, PrintPropertySet, ProfileKind, print_set_of};
use mitcad_model::properties::{BaseType, PropertyOwner, Source, same_name};
use mitcad_model::{Document, PropertyTypeChange};

/// A field given, also as `null` (`Some(Value::Null)`), told from one left
/// out (`None`).
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

named_enum! {
    /// The variants are the commands' names (`set_property`, ...).
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum PropertyCommand {
        "set_property" => SetProperty {
            owner: String,
            set: String,
            key: String,
            #[serde(default)]
            value: Option<Value>,
            #[serde(default, rename = "type")]
            ty: Option<String>,
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            source: Option<Source>,
            /// The `profile` of `Print Printer` or `Print Process` as a
            /// library's item (`base`, `user` or `project`): copied in, or
            /// the design's copy reused, in the same undo step (mitcad#655).
            #[serde(default)]
            library: Option<String>,
            /// The project's folder, when the design is in one.
            #[serde(default)]
            project: Option<String>,
        },
        "remove_property" => RemoveProperty {
            owner: String,
            set: String,
            key: String,
        },
        "define_property_type" => DefinePropertyType {
            name: String,
            base: String,
            #[serde(default)]
            values: Vec<Value>,
            #[serde(default)]
            unit: Option<String>,
            #[serde(default)]
            default: Option<Value>,
            #[serde(default)]
            description: String,
        },
        "edit_property_type" => EditPropertyType {
            name: String,
            #[serde(default)]
            new_name: Option<String>,
            #[serde(default)]
            base: Option<String>,
            #[serde(default)]
            values: Option<Vec<Value>>,
            #[serde(default, deserialize_with = "present")]
            unit: Option<Value>,
            #[serde(default, deserialize_with = "present")]
            default: Option<Value>,
            #[serde(default)]
            description: Option<String>,
        },
        "remove_property_type" => RemovePropertyType {
            name: String,
        },
    }
}

/// The `property_sets` query: the levels of a selection.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PropertySetsQuery {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    occurrence: Option<String>,
    #[serde(default)]
    component: Option<String>,
    /// Also the values the document works out (volume, area, mass, ...).
    #[serde(default)]
    computed: bool,
}

/// The `property_search` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PropertySearchQuery {
    text: String,
}

fn base_type(name: &str) -> Result<BaseType, ApiError> {
    BaseType::parse(name).ok_or_else(|| {
        let names: Vec<&str> = BaseType::ALL.iter().map(|t| t.as_str()).collect();
        ApiError(format!("{name} is no standard type ({})", names.join(", ")))
    })
}

/// A string field given as text or `null`.
fn text_or_null(value: Value, field: &str) -> Result<Option<String>, ApiError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        other => Err(ApiError(format!(
            "{field} must be text or null, not {other}"
        ))),
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait PropertiesCrateOps<K: Kernel> {
    /// A property owner by id (`C2`, `O3`, `F7.b0`) or by name: a
    /// component's, an occurrence's (`Component1:2`, or a path from the
    /// root) or a body's at the marker.
    fn property_owner_ref(&self, text: &str) -> Result<PropertyOwner, ApiError>;

    fn run_property_command(&mut self, command: PropertyCommand) -> Result<Value, ApiError>;

    /// The `property_sets` query: the body's, the occurrence's and the
    /// component's levels, those given (the component from the occurrence
    /// or the body when not given), most specific first.
    fn property_sets_json(&self, query: &PropertySetsQuery) -> Result<Value, ApiError>;

    fn property_search_query_json(&self, query: &PropertySearchQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> PropertiesCrateOps<K> for Document<K> {
    fn property_owner_ref(&self, text: &str) -> Result<PropertyOwner, ApiError> {
        if let Ok(owner) = text.parse::<PropertyOwner>() {
            return Ok(owner);
        }
        if let Ok(component) = self.component_ref(text) {
            return Ok(PropertyOwner::Component(component));
        }
        if let Ok(occurrence) = self.occurrence_ref(text) {
            return Ok(PropertyOwner::Occurrence(occurrence));
        }
        self.bodies()
            .iter()
            .find(|b| b.name == text)
            .map(|b| PropertyOwner::Body(b.uid))
            .ok_or_else(|| ApiError(format!("no component, occurrence or body “{text}”")))
    }

    fn run_property_command(&mut self, command: PropertyCommand) -> Result<Value, ApiError> {
        Ok(match command {
            PropertyCommand::SetProperty {
                owner,
                set,
                key,
                value,
                ty,
                unit,
                source,
                library,
                project,
            } => {
                let owner = self.property_owner_ref(&owner)?;
                let source = source.unwrap_or(Source::User);
                let property = match library.as_deref() {
                    Some(library) => {
                        // A library's printer or process chosen: copied in
                        // first, in the same undo step (mitcad#655).
                        let kind = match print_set_of(&set) {
                            Some(PrintPropertySet::Layer(layer))
                                if same_name(&key, PROFILE_KEY) =>
                            {
                                ProfileKind::of_set(layer)
                            }
                            _ => None,
                        }
                        .ok_or_else(|| {
                            ApiError(
                                "a library is given only for the profile of Print Printer or \
                                 Print Process"
                                    .to_owned(),
                            )
                        })?;
                        let id = value
                            .as_ref()
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .ok_or_else(|| {
                                ApiError(format!("the {} profile is given by its id", kind.id()))
                            })?
                            .to_owned();
                        let library = library_kind(library)?;
                        let libraries = self.print_libraries(project.as_deref().map(Path::new));
                        self.as_one_step(|doc| {
                            let copy = doc.library_item_in_design(
                                &libraries,
                                library,
                                ItemKind::of_profile(kind),
                                &id,
                            )?;
                            doc.set_property(
                                owner,
                                &set,
                                &key,
                                ty.as_deref(),
                                Some(&json!(copy)),
                                unit.as_deref(),
                                source,
                            )
                        })?
                    }
                    None => self.set_property(
                        owner,
                        &set,
                        &key,
                        ty.as_deref(),
                        value.as_ref(),
                        unit.as_deref(),
                        source,
                    )?,
                };
                json!({"owner": owner, "set": set, "key": property.key,
                       "type": property.ty, "value": property.value.to_json(),
                       "edited": property.edited()})
            }
            PropertyCommand::RemoveProperty { owner, set, key } => {
                let owner = self.property_owner_ref(&owner)?;
                self.remove_property(owner, &set, &key)?;
                json!({})
            }
            PropertyCommand::DefinePropertyType {
                name,
                base,
                values,
                unit,
                default,
                description,
            } => {
                let ty = self.define_property_type(
                    &name,
                    base_type(&base)?,
                    &values,
                    unit.as_deref(),
                    default.as_ref(),
                    &description,
                )?;
                json!({"name": ty.name})
            }
            PropertyCommand::EditPropertyType {
                name,
                new_name,
                base,
                values,
                unit,
                default,
                description,
            } => {
                let change = PropertyTypeChange {
                    new_name,
                    base: base.as_deref().map(base_type).transpose()?,
                    values,
                    unit: unit.map(|u| text_or_null(u, "unit")).transpose()?,
                    default: default.map(|d| (!d.is_null()).then_some(d)),
                    description,
                };
                let ty = self.edit_property_type(&name, &change)?;
                json!({"name": ty.name})
            }
            PropertyCommand::RemovePropertyType { name } => {
                json!({"converted": self.remove_property_type(&name)?})
            }
        })
    }

    fn property_sets_json(&self, query: &PropertySetsQuery) -> Result<Value, ApiError> {
        let body = match &query.body {
            Some(text) => match self.property_owner_ref(text)? {
                PropertyOwner::Body(b) => Some(b),
                _ => return Err(ApiError(format!("“{text}” is no body"))),
            },
            None => None,
        };
        let occurrence = query
            .occurrence
            .as_deref()
            .map(|o| self.occurrence_ref(o))
            .transpose()?;
        let component: Option<ComponentUid> = match &query.component {
            Some(text) => Some(self.component_ref(text)?),
            None => occurrence
                .and_then(|o| self.assembly().occurrence(o).map(|o| o.component))
                .or_else(|| body.and_then(|b| self.body_component(b))),
        };
        if body.is_none() && occurrence.is_none() && component.is_none() {
            return Err(ApiError(
                "property_sets needs a body, an occurrence or a component".to_owned(),
            ));
        }
        let mut levels = Vec::new();
        if let Some(body) = body {
            levels.push(self.property_level_json(PropertyOwner::Body(body), query.computed));
        }
        if let Some(occurrence) = occurrence {
            levels.push(
                self.property_level_json(PropertyOwner::Occurrence(occurrence), query.computed),
            );
        }
        if let Some(component) = component {
            levels.push(
                self.property_level_json(PropertyOwner::Component(component), query.computed),
            );
        }
        Ok(json!({"levels": levels}))
    }

    fn property_search_query_json(&self, query: &PropertySearchQuery) -> Result<Value, ApiError> {
        self.property_search_json(&query.text).map_err(ApiError)
    }
}
