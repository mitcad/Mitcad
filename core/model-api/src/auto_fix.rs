// SPDX-License-Identifier: MIT
//! Auto Fix (mitcad#1223, `document/auto_fix.rs` in the model): the
//! `auto_fix` command writes into the features' definitions the references
//! their warnings say were found under other names, as one undo step. See
//! `commands.md`, "Auto Fix".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::{AutoFixFit, Document, SharedChoice};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum AutoFixCommand {
        "auto_fix" => AutoFix {
            /// The features (a selection, a timeline group's members); every
            /// feature when left out or empty.
            #[serde(default)]
            features: Vec<FeatureUid>,
            /// What a thread's fit does with a named parameter other places
            /// use too (mitcad#1632): `parameter` changes it, `own` gives
            /// the dimension its own value; without it such a fit is left
            /// and `choices` names the parameter.
            #[serde(default)]
            shared: Option<SharedChoice>,
        },
    }
}

/// A fit as JSON (mitcad#1632): the thread, the face (its name, for
/// agents), the diameters, the feature and parameter changed with its old
/// and new expression, and how many other places use a named parameter
/// changed.
pub(super) fn fit_json(fit: &AutoFixFit) -> Value {
    json!({
        "thread": fit.thread, "name": fit.thread_name, "face": fit.face,
        "diameter": fit.diameter, "needed": fit.needed, "internal": fit.internal,
        "source": fit.source, "source_name": fit.source_name,
        "parameter": fit.parameter, "named": fit.named,
        "old": fit.old, "new": fit.new, "others": fit.others,
        "message": fit.summary(),
    })
}

/// Runs the command: `{"warnings", "fixed", "changed", "followed", "left",
/// "fitted", "choices", "label", "message"}`.
pub(super) fn run_auto_fix_command<K: Kernel>(
    doc: &mut Document<K>,
    command: AutoFixCommand,
) -> Result<Value, ApiError> {
    let AutoFixCommand::AutoFix { features, shared } = command;
    let report = doc.auto_fix_with(&features, shared)?;
    let left: Vec<Value> = report
        .left
        .iter()
        .map(|item| {
            json!({"feature": item.feature, "name": item.name, "message": item.message,
                   "advice": item.advice})
        })
        .collect();
    let fitted: Vec<Value> = report.fitted.iter().map(fit_json).collect();
    let choices: Vec<Value> = report
        .choices
        .iter()
        .map(|(thread, parameter, others)| {
            json!({"feature": thread, "parameter": parameter, "others": others})
        })
        .collect();
    Ok(json!({
        "warnings": report.warnings,
        "fixed": report.fixed,
        "changed": report.changed,
        "followed": report.followed,
        "left": left,
        "fitted": fitted,
        "choices": choices,
        "filament_faces": report.filament_faces,
        "label": report.label,
        "message": report.message,
    }))
}
