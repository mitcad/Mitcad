// SPDX-License-Identifier: MIT
//! Groups of user parameters (mitcad#2180, `mitcad_model::parameter_groups`;
//! see `commands.md`, "Parameter groups"): the commands
//! `add_parameter_group`, `rename_parameter_group`, `delete_parameter_group`
//! and `set_parameter_group`, and the `parameter_groups` query.
//! `add_parameter` and `set_parameter` take a `group` too, and the
//! `parameters` query gives each parameter's.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::kernel::Kernel;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum GroupCommand {
        // A group, with the parents it lacks; it may hold no parameter.
        "add_parameter_group" => Add { group: String },
        // `new_group` is the whole new path.
        "rename_parameter_group" => Rename {
            group: String,
            new_group: String,
        },
        // Its parameters move to its parent group, never deleted.
        "delete_parameter_group" => Delete { group: String },
        // Parameters moved into a group in one undo step; "" takes them
        // out of their groups.
        "set_parameter_group" => Set {
            names: Vec<String>,
            #[serde(default)]
            group: String,
        },
    }
}

/// The `parameter_groups` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GroupsQuery {}

pub(crate) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: GroupCommand,
) -> Result<Value, ApiError> {
    Ok(match command {
        GroupCommand::Add { group } => json!({"group": doc.add_parameter_group(&group)?}),
        GroupCommand::Rename { group, new_group } => {
            json!({"group": doc.rename_parameter_group(&group, &new_group)?})
        }
        GroupCommand::Delete { group } => {
            json!({"moved": doc.delete_parameter_group(&group)?})
        }
        GroupCommand::Set { names, group } => {
            json!({"group": doc.set_parameters_group(&names, &group)?})
        }
    })
}

/// The `parameter_groups` query: every group with its path, its own name,
/// its parent (`""` at the top) and the names of the parameters in it
/// (not in its subgroups), a parent before its subgroups, in the order
/// the groups were made.
pub(crate) fn groups_json<K: Kernel>(doc: &Document<K>) -> Value {
    use mitcad_model::parameter_groups::{group_name, parent_path};
    let groups = doc.parameter_groups();
    let parameters = doc.parameters();
    let list: Vec<Value> = groups
        .paths()
        .iter()
        .map(|path| {
            let names: Vec<String> = groups
                .members_of(path)
                .into_iter()
                .map(|id| parameters.name(id))
                .collect();
            json!({
                "path": path,
                "name": group_name(path),
                "parent": parent_path(path),
                "parameters": names,
            })
        })
        .collect();
    json!({"groups": list})
}
