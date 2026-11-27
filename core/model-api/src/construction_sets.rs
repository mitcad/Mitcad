// SPDX-License-Identifier: MIT
//! Construction sets (mitcad#2514, `mitcad_model::ConstructionSet`; see
//! `commands.md`, "Construction sets"): the commands
//! `add_construction_set`, `activate_construction_set`,
//! `deactivate_construction_set`, `set_construction_set_members`,
//! `rename_construction_set` and `delete_construction_set`, and the
//! `construction_sets` query.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use crate::prelude::*;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{ConstructionMember, Document};

/// A member in a command: a sketch or construction feature (`feature`) or
/// a body (`body`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemberInput {
    /// A body's uid (`F7.b0`) or its name at the marker.
    #[serde(default)]
    body: Option<String>,
    /// A feature's uid (`F5`) or name.
    #[serde(default)]
    feature: Option<String>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ConstructionSetCommand {
        "add_construction_set" => Add {
            /// The next free `ConstructionSet<n>` when left out.
            #[serde(default)]
            name: Option<String>,
            /// The active component when left out.
            #[serde(default)]
            component: Option<String>,
            /// Made the active set (default true).
            #[serde(default = "yes")]
            activate: bool,
        },
        "activate_construction_set" => Activate { name: String },
        "deactivate_construction_set" => Deactivate {},
        // Adds items (each leaves the set it was in) and takes others out.
        "set_construction_set_members" => SetMembers {
            name: String,
            #[serde(default)]
            add: Vec<MemberInput>,
            #[serde(default)]
            remove: Vec<MemberInput>,
        },
        "rename_construction_set" => Rename {
            name: String,
            new_name: String,
        },
        "delete_construction_set" => Delete { name: String },
    }
}

fn yes() -> bool {
    true
}

fn failed(error: mitcad_model::ModelError) -> ApiError {
    ApiError(error.to_string())
}

/// A member of a command as the model keeps it.
fn member_of<K: Kernel>(
    doc: &Document<K>,
    input: &MemberInput,
) -> Result<ConstructionMember, ApiError> {
    match (&input.body, &input.feature) {
        (Some(body), None) => {
            let text = body.trim();
            let uid = match text.parse::<BodyUid>() {
                Ok(uid) => uid,
                Err(_) => doc
                    .bodies()
                    .into_iter()
                    .find(|b| b.name == text)
                    .map(|b| b.uid)
                    .ok_or_else(|| {
                        ApiError(format!("no body at the timeline marker is named {text}"))
                    })?,
            };
            Ok(ConstructionMember::Body(uid))
        }
        (None, Some(feature)) => {
            let text = feature.trim();
            let uid = match text.parse::<FeatureUid>() {
                Ok(uid) => uid,
                Err(_) => {
                    let mut named = doc.features().filter(|f| f.name == text);
                    match (named.next(), named.next()) {
                        (Some(f), None) => f.uid,
                        (Some(_), Some(_)) => {
                            return Err(ApiError(format!(
                                "several features are named {text}: give its uid"
                            )));
                        }
                        (None, _) => return Err(ApiError(format!("no feature {text}"))),
                    }
                }
            };
            Ok(ConstructionMember::Feature(uid))
        }
        (None, None) => Err(ApiError("a member needs a feature or a body".to_owned())),
        (Some(_), Some(_)) => Err(ApiError(
            "a member is a feature or a body, not both".to_owned(),
        )),
    }
}

fn members_of<K: Kernel>(
    doc: &Document<K>,
    inputs: &[MemberInput],
) -> Result<Vec<ConstructionMember>, ApiError> {
    inputs.iter().map(|m| member_of(doc, m)).collect()
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: ConstructionSetCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        ConstructionSetCommand::Add {
            name,
            component,
            activate,
        } => {
            let component = component.map(|c| doc.component_ref(c.trim())).transpose()?;
            let name = doc
                .add_construction_set(name.as_deref(), component, activate)
                .map_err(failed)?;
            let set = doc.construction_sets().iter().find(|s| s.name == name);
            json!({
                "name": name,
                "component": set.map(|s| s.component),
                "active": set.is_some_and(|s| s.active),
            })
        }
        ConstructionSetCommand::Activate { name } => {
            doc.activate_construction_set(&name).map_err(failed)?;
            json!({"name": name.trim(), "component": doc.active_component()})
        }
        ConstructionSetCommand::Deactivate {} => {
            let name = doc.deactivate_construction_set().map_err(failed)?;
            json!({"name": name})
        }
        ConstructionSetCommand::SetMembers { name, add, remove } => {
            let add = members_of(doc, &add)?;
            let remove = members_of(doc, &remove)?;
            let count = doc
                .set_construction_set_members(&name, &add, &remove)
                .map_err(failed)?;
            json!({"name": name.trim(), "members": count})
        }
        ConstructionSetCommand::Rename { name, new_name } => {
            doc.rename_construction_set(&name, &new_name)
                .map_err(failed)?;
            json!({"name": new_name.trim()})
        }
        ConstructionSetCommand::Delete { name } => {
            doc.delete_construction_set(&name).map_err(failed)?;
            json!({})
        }
    })
}

/// A member as the query lists it: its kind (`feature`, `body`), its uid,
/// its current name, a feature's type, whether its item is in the design
/// (`exists`) and whether it is active there (`active`: false for a
/// suppressed feature, one after the timeline marker, and a body not at
/// the marker).
fn member_json<K: Kernel>(doc: &Document<K>, member: &ConstructionMember) -> Value {
    match member {
        ConstructionMember::Feature(feature) => match doc.feature(*feature) {
            Some(entry) => {
                let before = doc
                    .state()
                    .position(*feature)
                    .is_some_and(|i| i < doc.marker());
                json!({
                    "kind": "feature",
                    "feature": feature,
                    "name": entry.name,
                    "type": entry.def.type_name(),
                    "exists": true,
                    "active": before && !entry.suppressed,
                })
            }
            None => json!({
                "kind": "feature",
                "feature": feature,
                "name": feature.to_string(),
                "exists": false,
                "active": false,
            }),
        },
        ConstructionMember::Body(body) => {
            let at_marker = doc.body_component(*body).is_some();
            json!({
                "kind": "body",
                "body": body,
                "name": doc.body_name(*body),
                "exists": doc.feature(body.feature).is_some(),
                "active": at_marker,
            })
        }
    }
}

/// The `construction_sets` query: every set in the order added, with its
/// component, whether it is the active one, and its members.
pub(crate) fn sets_json<K: Kernel>(doc: &Document<K>) -> Value {
    let a = doc.assembly();
    Value::Array(
        doc.construction_sets()
            .iter()
            .map(|set| {
                let members: Vec<Value> = set.members.iter().map(|m| member_json(doc, m)).collect();
                json!({
                    "name": set.name,
                    "component": set.component,
                    "component_name": a.name(set.component),
                    "active": set.active,
                    "members": members,
                })
            })
            .collect(),
    )
}
