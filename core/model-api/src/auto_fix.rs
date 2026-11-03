// SPDX-License-Identifier: MIT
//! Auto Fix (mitcad#1223, `document/auto_fix.rs` in the model): the
//! `auto_fix` command writes into the features' definitions the references
//! their warnings say were found under other names, as one undo step. See
//! `commands.md`, "Auto Fix".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum AutoFixCommand {
        "auto_fix" => AutoFix {
            /// The features (a selection, a timeline group's members); every
            /// feature when left out or empty.
            #[serde(default)]
            features: Vec<FeatureUid>,
        },
    }
}

/// Runs the command: `{"warnings", "fixed", "changed", "followed", "left",
/// "label", "message"}`.
pub(super) fn run_auto_fix_command<K: Kernel>(
    doc: &mut Document<K>,
    command: AutoFixCommand,
) -> Result<Value, ApiError> {
    let AutoFixCommand::AutoFix { features } = command;
    let report = doc.auto_fix(&features)?;
    let left: Vec<Value> = report
        .left
        .iter()
        .map(|item| {
            json!({"feature": item.feature, "name": item.name, "message": item.message,
                   "advice": item.advice})
        })
        .collect();
    Ok(json!({
        "warnings": report.warnings,
        "fixed": report.fixed,
        "changed": report.changed,
        "followed": report.followed,
        "left": left,
        "label": report.label,
        "message": report.message,
    }))
}
