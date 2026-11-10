// SPDX-License-Identifier: MIT
//! Short modeling feedback for features a command actually added or edited.
//! Rules belong to the model's design_review query; these helpers only select
//! direct targets and keep its feedback bounded for a command response.

use std::collections::BTreeSet;

use serde_json::{Value, json};

const RULES: [&str; 3] = [
    "sketch-underconstrained",
    "fixed-placement",
    "volatile-parent",
];

/// Whether a model_command asks for feedback. An explicit false avoids the
/// before snapshot and the review query when the response mode needs neither.
pub fn command_hints_enabled(arguments: &Value) -> Result<bool, String> {
    match arguments.get("hints") {
        None => Ok(true),
        Some(Value::Bool(enabled)) => Ok(*enabled),
        Some(_) => Err("hints must be a boolean".to_owned()),
    }
}

fn feature_ids(snapshot: &Value) -> BTreeSet<&str> {
    snapshot["timeline"]["features"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|feature| feature["uid"].as_str())
        .collect()
}

/// A cheap, scoped review request for direct edits. Recomputed dependants and
/// existing failures are not targets. Only entries that actually ran have a
/// result; a stopped array's remaining entries are ignored.
pub fn command_hint_query(
    commands: &[Value],
    results: &[Value],
    before: &Value,
    after: &Value,
) -> Option<Value> {
    let old = feature_ids(before);
    let existing = feature_ids(after);
    let mut targets = BTreeSet::new();
    let mut editable = false;
    for (command, result) in commands.iter().zip(results) {
        if matches!(command["cmd"].as_str(), Some("undo" | "redo" | "recompute")) {
            continue;
        }
        editable = true;
        for field in ["uid", "sketch"] {
            if let Some(uid) = command[field].as_str() {
                targets.insert(uid);
            }
        }
        if let Some(uid) = result["uid"].as_str() {
            targets.insert(uid);
        }
        // Moving existing history can rewrite its placement without allocating
        // new feature ids. Only these commands return actual moved features in
        // these fields; another command's descriptive feature list is not an edit.
        match command["cmd"].as_str() {
            Some("move_history_into_component") => {
                for value in result["features"].as_array().into_iter().flatten() {
                    if let Some(uid) = value.as_str() {
                        targets.insert(uid);
                    }
                }
            }
            Some("move_to_component") => {
                for component in result["components"].as_array().into_iter().flatten() {
                    for value in component["features"].as_array().into_iter().flatten() {
                        if let Some(uid) = value.as_str() {
                            targets.insert(uid);
                        }
                    }
                }
            }
            _ => {}
        }
        // These are feature ids for creation and automatic reference repair,
        // respectively. Numbers, parameter names and removed ids do not pass
        // the intersection with the final timeline below.
        for field in ["uids", "changed", "followed"] {
            for value in result[field].as_array().into_iter().flatten() {
                if let Some(uid) = value.as_str() {
                    targets.insert(uid);
                }
            }
        }
    }
    if editable {
        targets.extend(existing.difference(&old).copied());
    }
    let targets: Vec<&str> = targets.intersection(&existing).copied().collect();
    if targets.is_empty() {
        None
    } else {
        Some(json!({"query": "design_review", "scope": targets, "checks": RULES, "print": false}))
    }
}

/// At most three short findings, with the shared rule and guide identifiers.
/// Advice is included in the line so that a hint says how to improve the edit.
pub fn command_hints(review: &Value) -> Value {
    Value::Array(
        review["findings"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|finding| {
                finding["rule"]
                    .as_str()
                    .is_some_and(|rule| RULES.contains(&rule))
            })
            .take(3)
            .map(|finding| {
                let message = finding["message"].as_str().unwrap_or_default();
                let advice = finding["advice"].as_str().unwrap_or_default();
                let text = format!("{message} {advice}");
                json!({"rule": finding["rule"], "target": finding["target"],
                       "message": crate::reference::echoed(text.trim()), "guide": finding["guide"]})
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(ids: &[&str]) -> Value {
        json!({"timeline": {"features": ids.iter().map(|id| json!({"uid": id})).collect::<Vec<_>>()}})
    }

    #[test]
    fn defaults_on_and_rejects_non_boolean_options() {
        assert_eq!(command_hints_enabled(&json!({})), Ok(true));
        assert_eq!(command_hints_enabled(&json!({"hints": false})), Ok(false));
        assert_eq!(command_hints_enabled(&json!({"hints": true})), Ok(true));
        for value in [Value::Null, json!(0), json!("false"), json!([])] {
            assert!(command_hints_enabled(&json!({"hints": value})).is_err());
        }
    }

    #[test]
    fn only_actual_direct_edits_are_reviewed() {
        let before = snapshot(&["F1", "F2", "F3"]);
        let after = snapshot(&["F1", "F2", "F3", "F4"]);
        let commands = vec![
            json!({"cmd": "edit_feature", "uid": "F2"}),
            json!({"cmd": "add_feature"}),
            json!({"cmd": "edit_feature", "uid": "F3"}),
        ];
        // The last entry never ran. F1 is an unrelated old failure/recompute.
        let results = vec![
            json!({"failed_features": [{"uid": "F1", "edited": false}]}),
            json!({"uid": "F4"}),
        ];
        let query = command_hint_query(&commands, &results, &before, &after).unwrap();
        assert_eq!(query["scope"], json!(["F2", "F4"]));
        assert_eq!(query["checks"], json!(RULES));
        assert_eq!(query["print"], false);
    }

    #[test]
    fn sketch_edits_and_reference_repairs_exclude_removed_and_non_feature_ids() {
        let commands = vec![
            json!({"cmd": "sketch.add_line", "sketch": "F1"}),
            json!({"cmd": "auto_fix"}),
        ];
        let results = vec![
            json!({"entities": ["c2"]}),
            json!({"changed": ["F2", "F9", "width", 3], "followed": ["F3"]}),
        ];
        let query = command_hint_query(
            &commands,
            &results,
            &snapshot(&["F1", "F2", "F3", "F9"]),
            &snapshot(&["F1", "F2", "F3"]),
        )
        .unwrap();
        assert_eq!(query["scope"], json!(["F1", "F2", "F3"]));
        for cmd in ["recompute", "undo", "redo"] {
            assert!(
                command_hint_query(
                    &[json!({"cmd": cmd})],
                    &[json!({})],
                    &snapshot(&["F1"]),
                    &snapshot(&["F1", "F2"])
                )
                .is_none()
            );
        }
        assert!(
            command_hint_query(
                &[json!({"cmd": "set_parameter", "name": "L"})],
                &[json!({})],
                &snapshot(&["F1"]),
                &snapshot(&["F1"])
            )
            .is_none()
        );
    }

    #[test]
    fn moving_existing_history_reviews_only_the_returned_feature_ids() {
        let before = snapshot(&["F1", "F2", "F3", "F4"]);
        // No new feature id: placement changes to the existing history still
        // need feedback. F3 and F4 are unrelated and must stay out.
        let after = before.clone();
        for (command, result) in [
            (
                json!({"cmd": "move_history_into_component", "feature": "F9"}),
                json!({"features": ["F1", "F2", "F9", "width", 2]}),
            ),
            (
                json!({"cmd": "move_to_component", "features": ["F1"]}),
                json!({"components": [{"features": ["F1", "F9"]},
                                   {"features": ["F2", "width", 2]}]}),
            ),
        ] {
            let query = command_hint_query(&[command], &[result], &before, &after).unwrap();
            assert_eq!(query["scope"], json!(["F1", "F2"]));
        }
        // The same shape on a non-move result is not evidence of an edit.
        assert!(
            command_hint_query(
                &[json!({"cmd": "set_parameter", "name": "L"})],
                &[json!({"features": ["F3"], "components": [{"features": ["F4"]}]})],
                &before,
                &after,
            )
            .is_none()
        );
    }

    #[test]
    fn findings_keep_each_rule_and_guide_but_stay_short_and_bounded() {
        let findings: Vec<Value> = RULES
            .iter()
            .map(|rule| {
                json!({"rule": rule, "target": "F2",
            "message": "Sketch2 needs attention.", "advice": "Constrain its geometry.",
            "guide": {"topic": "sketches", "anchor": "constraints"}})
            })
            .collect();
        let hints = command_hints(&json!({"findings": findings}));
        assert_eq!(hints.as_array().unwrap().len(), 3);
        for (hint, rule) in hints.as_array().unwrap().iter().zip(RULES) {
            assert_eq!(hint["rule"], rule);
            assert_eq!(hint["target"], "F2");
            assert_eq!(
                hint["guide"],
                json!({"topic": "sketches", "anchor": "constraints"})
            );
            assert!(hint["message"].as_str().unwrap().contains("Constrain"));
        }
        assert_eq!(command_hints(&json!({"findings": []})), json!([]));
        let repeated = vec![json!({"rule": RULES[0], "message": "x".repeat(2000)}); 20];
        let hints = command_hints(&json!({"findings": repeated}));
        assert_eq!(hints.as_array().unwrap().len(), 3);
        assert!(hints[0]["message"].as_str().unwrap().chars().count() < 500);
    }
}
