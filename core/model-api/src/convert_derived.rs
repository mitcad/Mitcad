// SPDX-License-Identifier: MIT
//! Convert to Features (mitcad#1815, see `commands.md`, "Convert to
//! Features"): a placed library part (`derived_part`) replaced by its
//! source's own features, every reference to it rewritten in place.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::ids::{ComponentUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model_build::CutListBuild;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ConvertCommand {
        "convert_derived_part" => ConvertDerivedPart {
            /// The `derived_part` feature, or `component`.
            #[serde(default)]
            feature: Option<FeatureUid>,
            /// The part's component: its derived part.
            #[serde(default)]
            component: Option<ComponentUid>,
            /// Only checks: nothing changes, the answer says what it would
            /// make (or the refusal why not).
            #[serde(default)]
            check: bool,
        },
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait ConvertDerivedOps<K: Kernel> {
    fn run_convert_command(&mut self, command: ConvertCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> ConvertDerivedOps<K> for Document<K> {
    fn run_convert_command(&mut self, command: ConvertCommand) -> Result<Value, ApiError> {
        match command {
            ConvertCommand::ConvertDerivedPart {
                feature,
                component,
                check,
            } => {
                let part = match (feature, component) {
                    (Some(feature), None) => feature,
                    (None, Some(component)) => {
                        self.derived_part_of(component).ok_or_else(|| {
                            ApiError(
                                mitcad_model::notice::FeatureError::new(
                                    "convert_derived_part: the component is no placed library part",
                                )
                                .with_detail(format!(
                                    "convert_derived_part: {component} has no derived part"
                                ))
                                .text(),
                            )
                        })?
                    }
                    _ => {
                        return Err(ApiError(
                            "convert_derived_part takes \"feature\" or \"component\"".to_owned(),
                        ));
                    }
                };
                // What the cut list showed of it, kept with its component so that
                // the list shows it as before.
                let listed = if check {
                    None
                } else {
                    self.feature(part)
                        .and_then(|entry| self.listed_row(entry.component))
                };
                let converted = self.convert_derived_part(part, check, listed)?;
                let mut bodies = Map::new();
                for (old, new) in &converted.bodies {
                    bodies.insert(old.to_string(), json!(new.to_string()));
                }
                Ok(json!({
                    "check": check,
                    "component": converted.component.to_string(),
                    "features": converted
                        .features
                        .iter()
                        .map(|(uid, name)| json!({"uid": uid.to_string(), "name": name}))
                        .collect::<Vec<_>>(),
                    "history": converted.history,
                    "bodies": bodies,
                    "rewritten": converted.rewritten,
                }))
            }
        }
    }
}
