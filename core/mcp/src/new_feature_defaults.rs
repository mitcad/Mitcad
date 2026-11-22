// SPDX-License-Identifier: MIT
//! The defaults MCP gives a new feature where the design file's own default
//! keeps the designs saved before a choice as they were built. A loft's
//! Tangent Edges (mitcad#2162): left out in a file it is `keep`; a new loft
//! from MCP, as from the panel, merges its tangent edges unless it says
//! otherwise.

use serde_json::{Value, json};

/// An `add_feature` command with the defaults of a new feature filled in
/// where it leaves them out: a loft's `tangent_edges` becomes `merge`.
/// Other commands, and an edited feature (`edit_feature` keeps what the
/// definition says, as the file does), are left as they are.
pub fn new_feature_defaults(command: &mut Value) {
    if command.get("cmd").and_then(Value::as_str) != Some("add_feature") {
        return;
    }
    let Some(def) = command.get_mut("def").and_then(Value::as_object_mut) else {
        return;
    };
    if def.get("type").and_then(Value::as_str) == Some("loft") && !def.contains_key("tangent_edges")
    {
        def.insert("tangent_edges".to_owned(), json!("merge"));
    }
}

#[cfg(test)]
mod tests {
    use super::new_feature_defaults;
    use serde_json::json;

    #[test]
    fn a_new_loft_merges_its_tangent_edges() {
        let mut command = json!({"cmd": "add_feature", "def": {"type": "loft", "sections": []}});
        new_feature_defaults(&mut command);
        assert_eq!(command["def"]["tangent_edges"], "merge");
        // Its own choice stays.
        let mut kept =
            json!({"cmd": "add_feature", "def": {"type": "loft", "tangent_edges": "keep"}});
        new_feature_defaults(&mut kept);
        assert_eq!(kept["def"]["tangent_edges"], "keep");
    }

    #[test]
    fn edits_and_other_features_stay_as_they_are() {
        for command in [
            json!({"cmd": "edit_feature", "uid": "F4", "def": {"type": "loft", "sections": []}}),
            json!({"cmd": "add_feature", "def": {"type": "extrude"}}),
            json!({"cmd": "add_feature"}),
            json!({"cmd": "undo"}),
        ] {
            let mut changed = command.clone();
            new_feature_defaults(&mut changed);
            assert_eq!(changed, command);
        }
    }
}
