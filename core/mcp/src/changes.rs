// SPDX-License-Identifier: MIT
//! What a `model_command` result says of the document (mitcad#689), the
//! same in the headless and the live server.
//!
//! By default (`"timeline": "changed"`) only what the command changed: the
//! features it added, whose timeline entries changed, that it recomputed or
//! that are left failed, the bodies it added or changed, and what it
//! removed, beside the document's `state`. `"full"` gives the whole
//! `timeline` and `bodies` as before; `"none"` the `state` alone. The
//! backend reads the document before the command (only for `changed`) and
//! after it with [`SNAPSHOT_QUERIES`], and lists the features the command
//! evaluated (the `recompute_times` query after each command that
//! recomputed).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// How much of the document a `model_command` result holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineMode {
    /// What the command changed (the default).
    Changed,
    /// The whole timeline and bodies.
    Full,
    /// The document's state only.
    None,
}

impl TimelineMode {
    /// The mode a call's arguments ask for (`timeline`).
    pub fn of(arguments: &Value) -> Result<Self, String> {
        match arguments.get("timeline") {
            None | Some(Value::Null) => Ok(Self::Changed),
            Some(value) => Self::parse(
                value
                    .as_str()
                    .ok_or_else(|| "timeline must be changed, full or none".to_owned())?,
            ),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "changed" | "" => Ok(Self::Changed),
            "full" => Ok(Self::Full),
            "none" => Ok(Self::None),
            other => Err(format!(
                "timeline must be changed, full or none, not “{other}”"
            )),
        }
    }

    /// Whether the document is read before the command.
    pub fn needs_before(self) -> bool {
        self == Self::Changed
    }
}

/// The queries of a snapshot, by the key its value goes under: the
/// document's state, the timeline, and the bodies with their shape
/// versions (which tell a changed body) and boxes (where each is,
/// mitcad#714).
pub const SNAPSHOT_QUERIES: [(&str, &str); 3] = [
    ("state", r#"{"query":"document"}"#),
    ("timeline", r#"{"query":"timeline"}"#),
    (
        "bodies",
        r#"{"query":"bodies","versions":true,"bbox":true}"#,
    ),
];

/// The features a command's results say it evaluated: `recompute_times`
/// answers, one per command that recomputed, merged in order.
pub fn evaluated_features<'a>(times: impl IntoIterator<Item = &'a Value>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for answer in times {
        for feature in answer["features"].as_array().into_iter().flatten() {
            if let Some(uid) = feature["uid"].as_str()
                && seen.insert(uid.to_owned())
            {
                out.push(uid.to_owned());
            }
        }
    }
    out
}

/// Whether a command's model result recomputed anything (`recomputed`
/// above zero), so that `recompute_times` tells what.
pub fn recomputed(result: &Value) -> bool {
    result["recomputed"].as_u64().is_some_and(|n| n > 0)
}

/// Whether an entry of a command array stops it (mitcad#689): a feature
/// the entry added or edited failed (its `failed_features`, mitcad#401).
pub fn entry_failed(result: &Value) -> bool {
    result["failed_features"]
        .as_array()
        .is_some_and(|failed| failed.iter().any(|f| f["edited"] == true))
}

/// The document's part of a `model_command` result: `state`, and by
/// `mode` the `changes` against `before`, the whole `timeline` and
/// `bodies`, or nothing more. `before` and `after` are snapshots
/// ([`SNAPSHOT_QUERIES`]); `evaluated` the features the command evaluated.
pub fn command_document(
    mode: TimelineMode,
    before: Option<&Value>,
    after: &Value,
    evaluated: &[String],
) -> Map<String, Value> {
    let mut out = Map::new();
    out.insert("state".to_owned(), after["state"].clone());
    match mode {
        TimelineMode::None => {}
        TimelineMode::Full => {
            out.insert("timeline".to_owned(), after["timeline"].clone());
            out.insert("bodies".to_owned(), without_versions(&after["bodies"]));
        }
        TimelineMode::Changed => {
            out.insert(
                "changes".to_owned(),
                changes(before.unwrap_or(&Value::Null), after, evaluated),
            );
        }
    }
    out
}

/// The entries of a list by their `uid`.
fn by_uid(list: &Value) -> BTreeMap<&str, &Value> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["uid"].as_str().map(|uid| (uid, entry)))
        .collect()
}

fn without_version(body: &Value) -> Value {
    let mut body = body.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.remove("version");
    }
    body
}

fn without_versions(bodies: &Value) -> Value {
    match bodies.as_array() {
        Some(list) => Value::Array(list.iter().map(without_version).collect()),
        None => bodies.clone(),
    }
}

/// What changed from `before` to `after`: each listed feature and body with
/// `change` (`added`, `changed`, `recomputed`, `failed`), the uids of the
/// removed ones, the marker and how many there are.
fn changes(before: &Value, after: &Value, evaluated: &[String]) -> Value {
    let evaluated: BTreeSet<&str> = evaluated.iter().map(String::as_str).collect();
    let old_features = by_uid(&before["timeline"]["features"]);
    let new_features = &after["timeline"]["features"];
    let mut features = Vec::new();
    for feature in new_features.as_array().into_iter().flatten() {
        let Some(uid) = feature["uid"].as_str() else {
            continue;
        };
        let change = match old_features.get(uid) {
            None => Some("added"),
            Some(old) if *old != feature => Some("changed"),
            _ if evaluated.contains(uid) => Some("recomputed"),
            _ if feature["status"] == "error" => Some("failed"),
            _ => None,
        };
        if let Some(change) = change {
            let mut entry = feature.clone();
            entry["change"] = json!(change);
            features.push(entry);
        }
    }
    let new_uids = by_uid(new_features);
    let removed_features: Vec<&str> = old_features
        .keys()
        .filter(|uid| !new_uids.contains_key(*uid))
        .copied()
        .collect();

    let old_bodies = by_uid(&before["bodies"]);
    let new_bodies = after["bodies"].as_array().cloned().unwrap_or_default();
    let mut bodies = Vec::new();
    for body in &new_bodies {
        let Some(uid) = body["uid"].as_str() else {
            continue;
        };
        let change = match old_bodies.get(uid) {
            None => Some("added"),
            Some(old) if *old != body => Some("changed"),
            _ => None,
        };
        if let Some(change) = change {
            let mut entry = without_version(body);
            entry["change"] = json!(change);
            bodies.push(entry);
        }
    }
    let new_body_uids = by_uid(&after["bodies"]);
    let removed_bodies: Vec<&str> = old_bodies
        .keys()
        .filter(|uid| !new_body_uids.contains_key(*uid))
        .copied()
        .collect();
    let mut out = json!({
        "features": features,
        "removed_features": removed_features,
        "bodies": bodies,
        "removed_bodies": removed_bodies,
        "feature_count": new_features.as_array().map_or(0, Vec::len),
        "body_count": new_bodies.len(),
    });
    let marker = &after["timeline"]["marker"];
    if !marker.is_null() {
        out["marker"] = marker.clone();
    }
    out
}
