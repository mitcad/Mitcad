// SPDX-License-Identifier: MIT
//! Comments for agents (mitcad#1119): notes the user pins to features,
//! faces, edges and points. The model keeps them and answers the `comments`
//! query; MCP gives that answer as the live resource `mitcad://comments`,
//! names the client as the author of its comment commands, and refuses
//! `resolve_comment` unless the user allows agents to resolve.

use serde_json::{Value, json};

/// The live resource of the design's comments: the `comments` query's
/// answer for all statuses.
pub const COMMENTS_URI: &str = "mitcad://comments";

/// The refusal of `resolve_comment` without the user's preference.
pub const RESOLVE_REFUSED: &str = "Resolving comments is left to the user (Preferences › AI Assistants › Agents may resolve comments)";

/// The author of a client that gives no usable name.
pub(crate) const DEFAULT_AUTHOR: &str = "agent";

/// The longest author name, in characters.
const AUTHOR_CHARS: usize = 80;

/// The comment commands whose `author` MCP sets: those that write text of
/// their own (`reopen_comment` with its reply).
const AUTHORED: [&str; 4] = [
    "add_comment",
    "reply_comment",
    "address_comment",
    "reopen_comment",
];

/// `resources/list`'s row of the comments resource.
pub(crate) fn resource() -> Value {
    json!({
        "uri": COMMENTS_URI,
        "name": "Comments",
        "description": "The design's comments: notes the user pinned to features, faces, edges and points for an agent, with their status (open, addressed, resolved), target, label and place; open counts the open ones. Read the open ones first; subscribe to hear when they change.",
        "mimeType": "application/json"
    })
}

/// `resources/read`'s result for the comments.
pub(crate) fn contents(comments: &Value) -> Value {
    json!({"contents": [{
        "uri": COMMENTS_URI,
        "mimeType": "application/json",
        "text": comments.to_string()
    }]})
}

/// The notification that a subscribed resource changed.
pub(crate) fn updated(uri: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/resources/updated",
        "params": {"uri": uri}
    })
}

/// A client's `clientInfo.name` as the author of its comments: printable,
/// one line, at most 80 characters. A client never writes as the user: an
/// empty name or `user` becomes `agent`.
pub(crate) fn author_name(name: &str) -> String {
    let printable: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let author: String = printable
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(AUTHOR_CHARS)
        .collect();
    if author.is_empty() || author.eq_ignore_ascii_case("user") {
        DEFAULT_AUTHOR.to_owned()
    } else {
        author
    }
}

/// Sets `author` on the comment commands of a `model_command`'s
/// `command` (an object or an array) to the client's name, and refuses
/// `resolve_comment` unless `may_resolve`: the refusal is the tool's error
/// result, nothing of the call having run.
pub(crate) fn apply_policy(
    arguments: &mut Value,
    author: &str,
    may_resolve: bool,
) -> Result<(), Value> {
    let array = arguments["command"].is_array();
    let entries: Vec<&mut Value> = match arguments.get_mut("command") {
        Some(Value::Array(entries)) => entries.iter_mut().collect(),
        Some(command) if command.is_object() => vec![command],
        _ => return Ok(()),
    };
    for (i, entry) in entries.into_iter().enumerate() {
        let Some(fields) = entry.as_object_mut() else {
            continue;
        };
        let kind = fields.get("cmd").and_then(Value::as_str).unwrap_or("");
        if kind == "resolve_comment" && !may_resolve {
            let mut refusal = json!({"applied": false});
            if array {
                refusal["error"] = json!(format!(
                    "commands[{i}]: {RESOLVE_REFUSED}; nothing of the array ran"
                ));
                refusal["entry"] = json!(i);
                refusal["undone"] = json!(0);
            } else {
                refusal["error"] = json!(RESOLVE_REFUSED);
            }
            return Err(refusal);
        }
        // reopen_comment writes as someone only with its reply.
        let authored = AUTHORED.contains(&kind)
            && (kind != "reopen_comment"
                || fields.contains_key("text")
                || fields.contains_key("author"));
        if authored {
            fields.insert("author".to_owned(), json!(author));
        }
    }
    Ok(())
}
