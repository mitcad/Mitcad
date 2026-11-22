// SPDX-License-Identifier: MIT
//! Selection sets (mitcad#2192, `mitcad_model::SelectionSet`; see
//! `commands.md`, "Selection sets"): the commands `add_selection_set`,
//! `set_selection_set_members`, `rename_selection_set` and
//! `delete_selection_set`, and the `selection_sets` query.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::features::OccurrencePath;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, SetMember};

/// A member in a command: a body (`body`), a feature (`feature`: a sketch,
/// construction geometry, a joint or any feature) or an occurrence
/// (`occurrence` alone). A body or a feature in a component placed by an
/// occurrence has that occurrence's path too.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MemberInput {
    /// A body's uid (`F2.b0`) or its name at the marker.
    #[serde(default)]
    body: Option<String>,
    /// A feature's uid (`F5`) or name.
    #[serde(default)]
    feature: Option<String>,
    /// The occurrence path from the root by uids (`O1/O4`) or names
    /// (`Arm:1/Pin:2`).
    #[serde(default)]
    occurrence: Option<String>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum SelectionSetCommand {
        "add_selection_set" => Add {
            /// The next free `SelectionSet<n>` when left out.
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            members: Vec<MemberInput>,
        },
        // Replaces the members (`members`), then adds and removes some.
        "set_selection_set_members" => SetMembers {
            name: String,
            #[serde(default)]
            members: Option<Vec<MemberInput>>,
            #[serde(default)]
            add: Vec<MemberInput>,
            #[serde(default)]
            remove: Vec<MemberInput>,
        },
        "rename_selection_set" => Rename {
            name: String,
            new_name: String,
        },
        "delete_selection_set" => Delete { name: String },
    }
}

fn failed(error: mitcad_model::ModelError) -> ApiError {
    ApiError(error.to_string())
}

/// An occurrence path from the root by uids or names; empty: the root.
fn path_of<K: Kernel>(doc: &Document<K>, text: Option<&str>) -> Result<OccurrencePath, ApiError> {
    let text = text.unwrap_or("").trim();
    doc.assembly()
        .find_path(text)
        .map(OccurrencePath)
        .ok_or_else(|| ApiError(format!("no occurrence path {text}")))
}

/// A member of a command as the model keeps it.
fn member_of<K: Kernel>(doc: &Document<K>, input: &MemberInput) -> Result<SetMember, ApiError> {
    let occurrence = path_of(doc, input.occurrence.as_deref())?;
    match (&input.body, &input.feature) {
        (Some(body), None) => {
            let body = match body.trim().parse::<BodyUid>() {
                Ok(uid) => uid,
                Err(_) => doc
                    .bodies()
                    .into_iter()
                    .find(|b| b.name == body.trim())
                    .map(|b| b.uid)
                    .ok_or_else(|| {
                        ApiError(format!("no body at the timeline marker is named {body}"))
                    })?,
            };
            Ok(SetMember::Body { body, occurrence })
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
            Ok(SetMember::Feature {
                feature: uid,
                occurrence,
            })
        }
        (None, None) if !occurrence.is_empty() => Ok(SetMember::Occurrence { occurrence }),
        (None, None) => Err(ApiError(
            "a member needs a body, a feature or an occurrence".to_owned(),
        )),
        (Some(_), Some(_)) => Err(ApiError(
            "a member is a body or a feature, not both".to_owned(),
        )),
    }
}

fn members_of<K: Kernel>(
    doc: &Document<K>,
    inputs: &[MemberInput],
) -> Result<Vec<SetMember>, ApiError> {
    inputs.iter().map(|m| member_of(doc, m)).collect()
}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: SelectionSetCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        SelectionSetCommand::Add { name, members } => {
            let members = members_of(doc, &members)?;
            let name = doc
                .add_selection_set(name.as_deref(), members)
                .map_err(failed)?;
            let count = doc
                .selection_sets()
                .iter()
                .find(|s| s.name == name)
                .map_or(0, |s| s.members.len());
            json!({"name": name, "members": count})
        }
        SelectionSetCommand::SetMembers {
            name,
            members,
            add,
            remove,
        } => {
            let members = members.map(|m| members_of(doc, &m)).transpose()?;
            let add = members_of(doc, &add)?;
            let remove = members_of(doc, &remove)?;
            let count = doc
                .set_selection_set_members(&name, members, add, &remove)
                .map_err(failed)?;
            json!({"name": name.trim(), "members": count})
        }
        SelectionSetCommand::Rename { name, new_name } => {
            doc.rename_selection_set(&name, &new_name).map_err(failed)?;
            json!({"name": new_name.trim()})
        }
        SelectionSetCommand::Delete { name } => {
            doc.delete_selection_set(&name).map_err(failed)?;
            json!({})
        }
    })
}

/// A member as the query lists it: its kind (`body`, `component`,
/// `feature`), its reference, the occurrence path that places it, the
/// component it is in, its current name, a feature's type, whether its
/// item is in the design (`exists`) and whether it is active there
/// (`active`: false for a suppressed feature, one after the timeline
/// marker, and a body not at the marker).
fn member_json<K: Kernel>(doc: &Document<K>, member: &SetMember) -> Value {
    let a = doc.assembly();
    let exists = doc.selection_member_exists(member);
    let path = member.occurrence();
    let mut value = match member {
        SetMember::Body { body, .. } => {
            let component = doc.body_component(*body);
            json!({
                "kind": "body",
                "body": body,
                "name": doc.body_name(*body),
                "component": component,
                "active": exists && component.is_some(),
            })
        }
        SetMember::Feature { feature, .. } => match doc.feature(*feature) {
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
                    "component": entry.component,
                    "active": exists && before && !entry.suppressed,
                })
            }
            None => json!({
                "kind": "feature",
                "feature": feature,
                "name": feature.to_string(),
                "active": false,
            }),
        },
        SetMember::Occurrence { occurrence } => {
            let last = occurrence.0.last().copied();
            let component: Option<ComponentUid> =
                last.and_then(|o| a.occurrence(o)).map(|o| o.component);
            json!({
                "kind": "component",
                "name": last.map(|o| a.occurrence_name(o)).unwrap_or_default(),
                "component": component,
                "active": exists,
            })
        }
    };
    value["occurrence"] = json!(path);
    if !path.is_empty() && exists {
        value["path_name"] = json!(a.path_name(&path.0));
    }
    value["exists"] = json!(exists);
    value
}

/// The `selection_sets` query: every set in the order added, with its
/// members.
pub(crate) fn sets_json<K: Kernel>(doc: &Document<K>) -> Value {
    Value::Array(
        doc.selection_sets()
            .iter()
            .map(|set| {
                let members: Vec<Value> = set.members.iter().map(|m| member_json(doc, m)).collect();
                json!({"name": set.name, "members": members})
            })
            .collect(),
    )
}
