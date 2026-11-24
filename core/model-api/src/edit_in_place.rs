// SPDX-License-Identifier: MIT
//! Edit in Place (mitcad#2321, `commands.md` "Edit in Place"): a linked
//! component edited inside the assembly that holds it. Two documents take
//! part, so the start and the end are functions of both, not commands of
//! one: [`begin_edit_in_place`] shows the holder's assembly around the
//! linked design's own document, [`finish_edit_in_place`] ends it and lets
//! the holder take the design's bodies in one undo step, and
//! [`linked_saved_in_place`] records a file the holder's Save wrote. While
//! the design is edited in place, every command it runs freezes what it
//! refers to in the assembly ([`after_command`]) and its answer says so
//! (`in_place_notices`); the `document` query tells the mode (`in_place`).

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ApiError, parse, read_json};
use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Kernel;

/// What [`begin_edit_in_place`] reads.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Begin {
    /// The linked component in the holder.
    component: ComponentUid,
    /// The holder's name as the user knows it (its file's name).
    #[serde(default)]
    holder: Option<String>,
}

/// What [`finish_edit_in_place`] and [`linked_saved_in_place`] read.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Finish {
    /// The linked component in the holder.
    component: ComponentUid,
    /// The holder's folder, which its link is followed from; the folder it
    /// was read from when left out.
    #[serde(default)]
    base: Option<PathBuf>,
}

/// Starts editing `linked`, the design of a linked component of `holder`,
/// in place (mitcad#2321): `linked` shows the assembly around the
/// component in a read-only component of its own and has its root active,
/// without an undo step. `request`: `component` (the holder's linked
/// component), `holder` (its name in the messages; the holder's root name
/// when left out). The answer: `component`, `name` (its name in the
/// holder), `context` (the component that shows the assembly in
/// `linked`), `occurrence` (its occurrence), `feature` (its base feature,
/// null without other bodies), `bodies` (the assembly's bodies there).
pub fn begin_edit_in_place<K: Kernel>(
    linked: &mut Document<K>,
    holder: &Document<K>,
    request: &str,
) -> Result<String, ApiError> {
    let begin: Begin = parse(read_json(request)?, "edit in place")?;
    let context = holder.in_place_context(begin.component)?;
    let label = begin
        .holder
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| holder.assembly().root_name.clone());
    let mode = linked.begin_in_place(context, &label)?;
    let bodies: Vec<String> = mode
        .feature
        .map(|f| {
            linked
                .component_bodies(mode.component)
                .into_iter()
                .filter(|b| b.feature == f)
                .map(|b| b.to_string())
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({
        "component": begin.component,
        "name": mode.name,
        "context": mode.component,
        "occurrence": mode.occurrence,
        "feature": mode.feature,
        "bodies": bodies,
    })
    .to_string())
}

/// Ends editing `linked` in place (mitcad#2321): the assembly around it
/// goes, without an undo step; when it changed in the mode, `holder`'s
/// linked component takes its bodies as Update Link takes a file, one undo
/// step `Edit in Place <name>`, its link recording the file's digest when
/// `linked` is saved (else the one it had). `request`: `component`,
/// `base` (the holder's folder). The answer: `changed` (the design changed
/// in the mode), `saved` (its file holds it), `updated` (the components
/// that took it), `messages` (a line each, as Update Link's), `label` (the
/// holder's undo step, null without one).
pub fn finish_edit_in_place<K: Kernel>(
    holder: &mut Document<K>,
    linked: &mut Document<K>,
    request: &str,
) -> Result<String, ApiError> {
    let finish: Finish = parse(read_json(request)?, "edit in place")?;
    let changed = linked.end_in_place()?;
    let saved = !linked.is_modified();
    if !changed {
        return Ok(json!({
            "changed": false, "saved": saved, "updated": [], "messages": [], "label": null,
        })
        .to_string());
    }
    let taken = holder.take_in_place_edit(
        finish.component,
        linked.state(),
        saved,
        finish.base.as_deref(),
    )?;
    let label = (!taken.updated.is_empty()).then(|| holder.undo_label().map(str::to_owned));
    Ok(json!({
        "changed": true,
        "saved": saved,
        "updated": taken.updated,
        "messages": taken.messages,
        "label": label.flatten(),
    })
    .to_string())
}

/// The holder's Save wrote the design it took from editing in place
/// (mitcad#2321): its link records the file's digest, without an undo
/// step. `request`: `component`, `base`. The answer: `changed` (the digest
/// changed).
pub fn linked_saved_in_place<K: Kernel>(
    holder: &mut Document<K>,
    request: &str,
) -> Result<String, ApiError> {
    let finish: Finish = parse(read_json(request)?, "edit in place")?;
    let changed = holder.link_saved_in_place(finish.component, finish.base.as_deref())?;
    Ok(json!({"changed": changed}).to_string())
}

/// After a command of a design edited in place (mitcad#2321): what it
/// refers to in the assembly is frozen in its undo step, and the answer
/// (the last one of an array) says so in `in_place_notices`. Undo and redo
/// freeze nothing: their states were frozen when made.
pub(crate) fn after_command<K: Kernel>(
    doc: &mut Document<K>,
    command: &Value,
    result: &mut Value,
) -> Result<(), ApiError> {
    if doc.in_place_mode().is_none() {
        return Ok(());
    }
    let step = |c: &Value| matches!(c.get("cmd").and_then(Value::as_str), Some("undo" | "redo"));
    let only_steps = match command {
        Value::Array(commands) => commands.iter().all(step),
        command => step(command),
    };
    if only_steps {
        return Ok(());
    }
    let notices = doc.freeze_in_place_picks()?;
    if notices.is_empty() {
        return Ok(());
    }
    let target = match result {
        Value::Array(results) => results.last_mut(),
        value => Some(value),
    };
    if let Some(Value::Object(answer)) = target {
        answer.insert("in_place_notices".to_owned(), json!(notices));
    }
    Ok(())
}

/// The `document` query's `in_place` while the design is edited in place
/// (mitcad#2321): `holder`, `name`, `context`, `occurrence`, `feature`;
/// null otherwise.
pub(crate) fn mode_json<K: Kernel>(doc: &Document<K>) -> Value {
    match doc.in_place_mode() {
        Some(mode) => json!({
            "holder": mode.holder,
            "name": mode.name,
            "context": mode.component,
            "occurrence": mode.occurrence,
            "feature": mode.feature,
        }),
        None => Value::Null,
    }
}
