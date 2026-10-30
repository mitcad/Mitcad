// SPDX-License-Identifier: MIT
//! One undo step per chat message (mitcad#947): the panel reads the undo
//! depth when a message starts and asks here, when its turn ends, whether
//! the steps after it merge (`merge_undo`) into one named after the
//! message, "AI: <first words>". Nothing merges when the user changed the
//! design during the turn (the agent's steps stay its separate "… (agent)"
//! steps), while a sketch is being edited (the sketch session's own undo
//! rules) or a command panel is open, or when the turn left no step.

use serde_json::{Value, json};

use super::text_field;

/// The words of the message a label keeps at most, and its characters.
const LABEL_WORDS: usize = 8;
const LABEL_CHARS: usize = 40;

pub fn decide(request: &Value) -> Value {
    let flag = |name: &str| request.get(name).and_then(Value::as_bool).unwrap_or(false);
    let depth = |name: &str| request.get(name).and_then(Value::as_i64);
    let label = label(text_field(request, "message"));
    let refuse =
        |reason: &str| json!({"merge": false, "depth": null, "label": label, "reason": reason});
    let (Some(start), Some(end)) = (depth("start_depth"), depth("end_depth")) else {
        return refuse("unknown");
    };
    if flag("user_edited") {
        return refuse("user_edited");
    }
    if flag("sketch") {
        return refuse("sketch");
    }
    if flag("panel") {
        return refuse("panel");
    }
    if end < start {
        return refuse("undone");
    }
    if end == start {
        return refuse("no_change");
    }
    json!({"merge": true, "depth": start, "label": label, "reason": null})
}

/// "AI: " and the message's first words, on one line.
pub fn label(message: &str) -> String {
    let words: Vec<&str> = message
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|w| !w.is_empty())
        .collect();
    let mut kept = String::new();
    let mut count = 0;
    for word in &words {
        let length = kept.chars().count() + usize::from(!kept.is_empty()) + word.chars().count();
        if count == LABEL_WORDS || length > LABEL_CHARS {
            break;
        }
        if !kept.is_empty() {
            kept.push(' ');
        }
        kept.push_str(word);
        count += 1;
    }
    if count == 0 {
        match words.first() {
            Some(word) => {
                kept = super::cut(word, LABEL_CHARS);
            }
            None => return "AI: message".to_owned(),
        }
    }
    if count < words.len() {
        kept.push('…');
    }
    format!("AI: {kept}")
}
