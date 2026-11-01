// SPDX-License-Identifier: MIT
//! Notices and their severity levels (mitcad#1040): what the model tells
//! the user about a feature, a command's result, an import, an annotation,
//! a sheet metal body or a sketch, with one level of three. The levels are
//! those of `docs/ui-conventions.md` ("Messages and severity"):
//!
//! - [`NoticeLevel::Error`]: the result is missing; the work stopped (a
//!   feature that produces nothing, an export not written);
//! - [`NoticeLevel::Warning`]: it ran, but something is wrong or left out
//!   (a fillet that lost some of its edges, skipped import items, a broken
//!   annotation);
//! - [`NoticeLevel::Info`]: a neutral fact that needs no action (an import
//!   note on a feature).
//!
//! Success, hint and progress are the frontends' own; states (suppressed,
//! rolled back, inactive, hidden, pending, driven) are not severities.
//!
//! On the wire a notice is `{"level": "error" | "warning" | "info",
//! "message": …, "target": …}` (`target` only when it names something: a
//! feature, an annotation, a body, a sketch entity), and a result's notices
//! are its `"notices"` array.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// How serious a notice is. Ordered from the least to the most serious, so
/// the worst of several is their maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    /// A neutral fact that needs no action.
    Info,
    /// It ran, but something is wrong or left out; the user should check it.
    Warning,
    /// The result is missing: the work stopped.
    Error,
}

impl NoticeLevel {
    /// `info`, `warning` or `error`: the level on the wire.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// The level of its wire name; None for any other text.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

/// One thing the model tells the user, with its level and, when it is
/// about something in particular, what it names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub level: NoticeLevel,
    pub message: String,
    /// What it is about: a feature's uid, an annotation's id, a body's uid,
    /// a sketch entity; None for the result as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Notice {
    pub fn new(level: NoticeLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            target: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Warning, message)
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(NoticeLevel::Info, message)
    }

    /// The same notice about `target`.
    pub fn with_target(mut self, target: impl ToString) -> Self {
        self.target = Some(target.to_string());
        self
    }

    /// The notice as JSON (`level`, `message`, `target` when it has one).
    pub fn to_json(&self) -> Value {
        let mut value = json!({"level": self.level.as_str(), "message": self.message});
        if let Some(target) = &self.target {
            value["target"] = json!(target);
        }
        value
    }
}

/// The worst level of `notices`; None when there are none.
pub fn worst<'a>(notices: impl IntoIterator<Item = &'a Notice>) -> Option<NoticeLevel> {
    notices.into_iter().map(|n| n.level).max()
}

/// Notices of one level, one per message.
pub fn of_level<I>(level: NoticeLevel, messages: I) -> Vec<Notice>
where
    I: IntoIterator,
    I::Item: Into<String>,
{
    messages
        .into_iter()
        .map(|m| Notice::new(level, m))
        .collect()
}

/// The notices of a result's `"warnings"` array (mitcad#1040: the key that
/// command results had before `"notices"`): a text is a warning; an object
/// is a warning with its `message` (or `text`) and its `target` (or `uid`,
/// `id`) when it names one.
pub fn from_warnings(warnings: &Value) -> Vec<Notice> {
    warnings
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|warning| match warning {
            Value::String(text) if !text.is_empty() => Some(Notice::warning(text.as_str())),
            Value::Object(object) => {
                let message = ["message", "text"]
                    .iter()
                    .find_map(|key| object.get(*key).and_then(Value::as_str))
                    .map_or_else(|| warning.to_string(), str::to_owned);
                let target = ["target", "uid", "id"]
                    .iter()
                    .find_map(|key| object.get(*key))
                    .and_then(|t| match t {
                        Value::String(s) => Some(s.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    });
                let notice = Notice::warning(message);
                Some(match target {
                    Some(target) => notice.with_target(target),
                    None => notice,
                })
            }
            Value::String(_) | Value::Null => None,
            other => Some(Notice::warning(other.to_string())),
        })
        .collect()
}

/// Gives a command's or query's result (a JSON object) its `"notices"`
/// (mitcad#1040): those it has, then one warning per entry of its
/// `"warnings"`, which stays beside them for one release so that readers
/// of that key keep working (`commands.md`, "Notices"). A result without
/// either gets none; a value that is not an object is left as it is.
pub fn add_notices(result: &mut Value) {
    let Some(object) = result.as_object_mut() else {
        return;
    };
    let mut notices: Vec<Value> = object
        .get("notices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(warnings) = object.get("warnings") {
        for notice in from_warnings(warnings) {
            let value = notice.to_json();
            // A command that gave its warnings as notices already lists them.
            let listed = notices
                .iter()
                .any(|n| n["message"] == value["message"] && n["level"] == value["level"]);
            if !listed {
                notices.push(value);
            }
        }
    }
    if !notices.is_empty() {
        object.insert("notices".to_owned(), Value::Array(notices));
    }
}

/// The notices of a result as JSON values.
pub fn to_json(notices: &[Notice]) -> Value {
    Value::Array(notices.iter().map(Notice::to_json).collect())
}

/// An import's notices from its items' outcomes (mitcad#1040): one per
/// outcome whose level is a warning or an error and that some items had,
/// the worst first, each saying with `describe` what that many items did.
/// The outcomes keep their own words in the import's report rows; the
/// information ones (items that came in as asked) say nothing here.
pub fn outcome_notices<O: Copy>(
    counts: impl IntoIterator<Item = (O, usize)>,
    level: impl Fn(O) -> NoticeLevel,
    describe: impl Fn(O, usize) -> String,
) -> Vec<Notice> {
    let mut notices: Vec<Notice> = counts
        .into_iter()
        .filter(|(outcome, count)| *count > 0 && level(*outcome) > NoticeLevel::Info)
        .map(|(outcome, count)| Notice::new(level(outcome), describe(outcome, count)))
        .collect();
    // Stable: outcomes of one level keep their order.
    notices.sort_by_key(|notice| std::cmp::Reverse(notice.level));
    notices
}

/// "1 item", "3 items": a count with its noun.
pub fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_from_info_to_error() {
        assert!(NoticeLevel::Info < NoticeLevel::Warning);
        assert!(NoticeLevel::Warning < NoticeLevel::Error);
        let notices = [Notice::info("a"), Notice::error("b"), Notice::warning("c")];
        assert_eq!(worst(&notices), Some(NoticeLevel::Error));
        assert_eq!(worst(std::iter::empty()), None);
    }

    #[test]
    fn levels_have_their_wire_names() {
        for level in [NoticeLevel::Info, NoticeLevel::Warning, NoticeLevel::Error] {
            assert_eq!(NoticeLevel::parse(level.as_str()), Some(level));
            assert_eq!(serde_json::to_value(level).unwrap(), json!(level.as_str()));
        }
        assert_eq!(NoticeLevel::parse("hint"), None);
    }

    #[test]
    fn a_notice_serializes_with_its_target_only_when_it_has_one() {
        assert_eq!(
            Notice::warning("lost 2 edges").to_json(),
            json!({"level": "warning", "message": "lost 2 edges"})
        );
        let targeted = Notice::error("broken").with_target("A3");
        assert_eq!(
            targeted.to_json(),
            json!({"level": "error", "message": "broken", "target": "A3"})
        );
        assert_eq!(serde_json::to_value(&targeted).unwrap(), targeted.to_json());
        let read: Notice =
            serde_json::from_value(json!({"level": "info", "message": "m"})).unwrap();
        assert_eq!(read, Notice::info("m"));
    }

    #[test]
    fn warnings_become_warning_notices() {
        let notices = from_warnings(&json!([
            "a text",
            {"message": "an object", "uid": 7},
            {"text": "a text field", "target": "F2"},
            null,
            ""
        ]));
        assert_eq!(
            notices,
            vec![
                Notice::warning("a text"),
                Notice::warning("an object").with_target("7"),
                Notice::warning("a text field").with_target("F2"),
            ]
        );
        assert!(from_warnings(&json!(null)).is_empty());
    }

    #[test]
    fn a_result_gets_notices_from_its_warnings_and_keeps_them() {
        let mut result = json!({"uid": 3, "warnings": ["angle over one turn"]});
        add_notices(&mut result);
        assert_eq!(result["warnings"], json!(["angle over one turn"]));
        assert_eq!(
            result["notices"],
            json!([{"level": "warning", "message": "angle over one turn"}])
        );
        // Notices it has stay first and are not repeated.
        let mut both = json!({
            "notices": [{"level": "info", "message": "kept"},
                        {"level": "warning", "message": "w"}],
            "warnings": ["w", "x"]
        });
        add_notices(&mut both);
        assert_eq!(
            both["notices"],
            json!([{"level": "info", "message": "kept"},
                   {"level": "warning", "message": "w"},
                   {"level": "warning", "message": "x"}])
        );
        // None without either.
        let mut plain = json!({"uid": 1});
        add_notices(&mut plain);
        assert!(plain.get("notices").is_none());
        let mut empty = json!({"warnings": []});
        add_notices(&mut empty);
        assert!(empty.get("notices").is_none());
        let mut text = json!("not an object");
        add_notices(&mut text);
        assert_eq!(text, json!("not an object"));
    }

    #[test]
    fn outcomes_give_notices_of_their_levels_the_worst_first() {
        #[derive(Clone, Copy)]
        enum Outcome {
            Built,
            Skipped,
            Failed,
        }
        let level = |o: Outcome| match o {
            Outcome::Built => NoticeLevel::Info,
            Outcome::Skipped => NoticeLevel::Warning,
            Outcome::Failed => NoticeLevel::Error,
        };
        let describe = |o: Outcome, n: usize| match o {
            Outcome::Built => format!("{} built", count(n, "item", "items")),
            Outcome::Skipped => format!("{} skipped", count(n, "item", "items")),
            Outcome::Failed => format!("{} failed", count(n, "item", "items")),
        };
        let counts = [
            (Outcome::Built, 5),
            (Outcome::Skipped, 3),
            (Outcome::Failed, 1),
        ];
        assert_eq!(
            outcome_notices(counts, level, describe),
            vec![
                Notice::error("1 item failed"),
                Notice::warning("3 items skipped")
            ]
        );
        let clean = [(Outcome::Built, 5), (Outcome::Skipped, 0)];
        assert!(outcome_notices(clean, level, describe).is_empty());
    }
}
