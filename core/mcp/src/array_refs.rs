// SPDX-License-Identifier: MIT
//! References between the entries of a `model_command` array
//! (mitcad#1897, `core/model-api/src/commands.md` "References between the
//! entries of a command array"). An entry names its result with
//! `"as": "<name>"`; a later entry takes a value of that result with
//! `{"ref": "<name>.<path>"}` wherever it takes an id or a point, so that an
//! agent never predicts the ids an earlier entry will make.
//!
//! - The path is the result's keys and list indexes, joined with dots:
//!   `sk.uid`, `l1.made.0`, `outline.entities.4`. A first key the result
//!   lacks is looked up in its `inputs` (a drawing command's point at each
//!   point input as written): `arc1.end` is `arc1.inputs.end`, the point at
//!   the arc's `end` as written, never the stored end of an arc turned
//!   counter-clockwise; `outline.points.0` is a spline's first point.
//! - [`check_refs`] runs before any entry: malformed names and references,
//!   names used twice, and references to no entry or to this or a later one
//!   refuse the array with nothing run. [`resolve_entry`] runs right before
//!   its entry, on the results of the entries before it: a path the result
//!   lacks, `null`, and a value of the wrong kind (a curve where a drawing
//!   tool takes a point) refuse that entry, and the server undoes the
//!   entries before it as for any refused entry.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::reference::echoed;

/// The longest name of an entry, in characters.
const NAME_CHARS: usize = 64;

/// The drawing commands whose point inputs a reference must give a point
/// (`"p3"` or `[x, y]`): those of [`POINT_FIELDS`] at the top of the entry.
const DRAWING_COMMANDS: &[&str] = &[
    "sketch.add_point",
    "sketch.add_line",
    "sketch.rectangle",
    "sketch.circle",
    "sketch.arc",
    "sketch.polygon",
    "sketch.slot",
    "sketch.ellipse",
    "sketch.spline",
];

/// The point inputs of the drawing commands; `points` is a spline's list.
const POINT_FIELDS: &[&str] = &[
    "at", "start", "end", "center", "through", "a", "b", "c", "corner", "vertex", "major", "points",
];

/// The text of a reference, `{"ref": "<name>.<path>"}`.
fn ref_text(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(map) => map.get("ref"),
        _ => None,
    }
}

/// Whether `entry` holds a reference anywhere.
pub fn has_refs(entry: &Value) -> bool {
    match entry {
        Value::Object(map) => map.contains_key("ref") || map.values().any(has_refs),
        Value::Array(items) => items.iter().any(has_refs),
        _ => false,
    }
}

/// A name an entry may take: a letter or `_`, then letters, digits, `_`
/// and `-`, at most [`NAME_CHARS`].
fn check_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let first = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest = chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if first && rest && name.chars().count() <= NAME_CHARS {
        Ok(())
    } else {
        Err(format!(
            "\"as\": “{}” is no name: a letter or _, then letters, digits, _ or -, at most \
             {NAME_CHARS} characters, such as \"outline\"",
            echoed(name)
        ))
    }
}

/// A reference's name and path: `arc1.end` is (`arc1`, [`end`]).
fn split_ref(text: &str) -> Result<(&str, Vec<&str>), String> {
    let form = || {
        format!(
            "{{\"ref\": \"{}\"}} is no reference: it is {{\"ref\": \"<name>.<path>\"}}, the name \
             an earlier entry gave its result with \"as\" and the keys and list indexes of that \
             result, such as \"sk.uid\", \"l1.made.0\" or \"arc1.end\"",
            echoed(text)
        )
    };
    let (name, path) = text.split_once('.').ok_or_else(form)?;
    let path: Vec<&str> = path.split('.').collect();
    if name.is_empty() || path.iter().any(|segment| segment.is_empty()) {
        return Err(form());
    }
    Ok((name, path))
}

/// Every reference text in `value`, or why one is malformed.
fn collect_refs<'a>(value: &'a Value, found: &mut Vec<&'a str>) -> Result<(), String> {
    match value {
        Value::Object(map) if map.contains_key("ref") => match (map.len(), &map["ref"]) {
            (1, Value::String(text)) => {
                found.push(text);
                Ok(())
            }
            _ => Err(format!(
                "{} is no reference: a reference is {{\"ref\": \"<name>.<path>\"}} alone",
                echoed(&value.to_string())
            )),
        },
        Value::Object(map) => map.values().try_for_each(|v| collect_refs(v, found)),
        Value::Array(items) => items.iter().try_for_each(|v| collect_refs(v, found)),
        _ => Ok(()),
    }
}

/// The names the entries give their results, each with its entry.
fn names(entries: &[&Value]) -> Result<BTreeMap<String, usize>, (usize, String)> {
    let mut named: BTreeMap<String, usize> = BTreeMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let Some(name) = entry.get("as") else {
            continue;
        };
        let Some(name) = name.as_str() else {
            return Err((
                i,
                "\"as\" names the entry's result: a text such as \"outline\"".to_owned(),
            ));
        };
        check_name(name).map_err(|error| (i, error))?;
        if let Some(first) = named.get(name) {
            return Err((
                i,
                format!(
                    "\"as\": “{name}” already names entry {first}: each entry's name is its own"
                ),
            ));
        }
        named.insert(name.to_owned(), i);
    }
    Ok(named)
}

/// Checks a command array's names and references before any of its
/// entries runs (mitcad#1897): each `as` a name of its own, each reference
/// well formed and to an entry before its own. A refusal names the entry
/// (its index) and why; nothing of the array has run. A single command is
/// checked as an array of one, where no reference has an earlier entry.
pub fn check_refs(entries: &[&Value]) -> Result<(), (usize, String)> {
    let named = names(entries)?;
    for (i, entry) in entries.iter().enumerate() {
        if entry.get("cmd").is_some_and(|cmd| ref_text(cmd).is_some()) {
            return Err((
                i,
                "\"cmd\" is the command's name, not a reference".to_owned(),
            ));
        }
        let mut found = Vec::new();
        collect_refs(entry, &mut found).map_err(|error| (i, error))?;
        for text in found {
            let (name, _) = split_ref(text).map_err(|error| (i, error))?;
            let shown = echoed(text);
            match named.get(name) {
                None => {
                    return Err((
                        i,
                        format!(
                            "{{\"ref\": \"{shown}\"}}: no entry of the array is named “{}” (an \
                             earlier entry names its result with \"as\": \"{}\")",
                            echoed(name),
                            echoed(name)
                        ),
                    ));
                }
                Some(&j) if j == i => {
                    return Err((
                        i,
                        format!(
                            "{{\"ref\": \"{shown}\"}}: “{name}” is this entry, whose result is not \
                             there yet: a reference takes the result of an earlier entry"
                        ),
                    ));
                }
                Some(&j) if j > i => {
                    return Err((
                        i,
                        format!(
                            "{{\"ref\": \"{shown}\"}}: “{name}” is entry {j}, which runs after this \
                             one: a reference takes the result of an earlier entry"
                        ),
                    ));
                }
                Some(_) => {}
            }
        }
    }
    Ok(())
}

/// The keys of an object, for a refusal: at most twelve.
fn keys_of(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).take(12).collect();
            if map.len() > keys.len() {
                keys.push("…");
            }
            keys.join(", ")
        }
        _ => String::new(),
    }
}

/// What `path` names in `result`, or why it names nothing: a first key the
/// result lacks is looked up in its `inputs`.
fn lookup<'a>(result: &'a Value, path: &[&str], text: &str, j: usize) -> Result<&'a Value, String> {
    let mut at = result;
    for (k, segment) in path.iter().enumerate() {
        let next = match at {
            Value::Object(map) => map.get(*segment).or_else(|| {
                if k == 0 {
                    map.get("inputs").and_then(|inputs| inputs.get(*segment))
                } else {
                    None
                }
            }),
            Value::Array(items) => segment.parse::<usize>().ok().and_then(|n| items.get(n)),
            _ => None,
        };
        let Some(next) = next else {
            let walked = path[..k].join(".");
            let place = if walked.is_empty() {
                format!("the result of entry {j}")
            } else {
                format!("{walked} of entry {j}'s result")
            };
            let has = match at {
                Value::Object(_) if k == 0 && at.get("inputs").is_some() => format!(
                    "it has {}, and inputs {}",
                    keys_of(at),
                    keys_of(&at["inputs"])
                ),
                Value::Object(_) => format!("it has {}", keys_of(at)),
                Value::Array(items) => format!("it is a list of {} (from 0)", items.len()),
                _ => "it is a single value".to_owned(),
            };
            return Err(format!(
                "{{\"ref\": \"{}\"}}: {place} has no {}: {has}",
                echoed(text),
                echoed(segment)
            ));
        };
        at = next;
    }
    Ok(at)
}

/// Whether `text` is a sketch point id, `p<n>`.
fn is_point_id(text: &str) -> bool {
    text.strip_prefix('p')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Why `value` cannot stand where the reference `text` is: in a drawing
/// command's point input it is a point, in a sketch command's `sketch` a
/// feature's uid; nowhere `null`.
fn wrong_kind(cmd: &str, field: Option<&str>, value: &Value, text: &str) -> Option<String> {
    let shown = echoed(text);
    if value.is_null() {
        return Some(format!(
            "{{\"ref\": \"{shown}\"}} is null: the earlier entry made nothing there (a drawing \
             command makes no point at an arc's through)"
        ));
    }
    let field = field?;
    if DRAWING_COMMANDS.contains(&cmd) && POINT_FIELDS.contains(&field) {
        let point = match value {
            Value::String(id) => is_point_id(id),
            Value::Array(xy) => xy.len() == 2 && xy.iter().all(|v| v.is_number() || v.is_string()),
            _ => false,
        };
        if !point {
            return Some(format!(
                "{field}: {{\"ref\": \"{shown}\"}} is {}, not a point (\"p<n>\" or [x, y]): a \
                 drawing command's point at an input is <name>.<input>, such as \"l1.end\"",
                echoed(&value.to_string())
            ));
        }
    }
    if cmd.starts_with("sketch.") && field == "sketch" && !value.is_string() {
        return Some(format!(
            "sketch: {{\"ref\": \"{shown}\"}} is {}, not a sketch's uid: the sketch.create \
             entry's is <name>.uid",
            echoed(&value.to_string())
        ));
    }
    None
}

/// Resolves one reference's text, given the entry's top-level field it
/// lies under.
type Resolve<'a> = dyn FnMut(&str, Option<&str>) -> Result<Value, String> + 'a;

/// Replaces the references in `value`, which lies under the entry's
/// top-level field `field` (directly, or as an item of a list there).
fn replace(
    value: &mut Value,
    field: Option<&str>,
    resolve: &mut Resolve<'_>,
) -> Result<(), String> {
    if let Some(Value::String(text)) = ref_text(value) {
        let text = text.clone();
        *value = resolve(&text, field)?;
        return Ok(());
    }
    match value {
        Value::Object(map) => {
            for item in map.values_mut() {
                replace(item, None, resolve)?;
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                replace(item, field, resolve)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Entry `i` of `entries` as it runs (mitcad#1897): without its `as`, each
/// reference replaced by what it names in `results`, the results of the
/// entries before it in order. Why it cannot be is a refusal of this entry.
pub fn resolve_entry(entries: &[&Value], i: usize, results: &[Value]) -> Result<Value, String> {
    let Some(entry) = entries.get(i) else {
        return Err(format!("the array has no entry {i}"));
    };
    let mut named: BTreeMap<&str, usize> = BTreeMap::new();
    for (j, earlier) in entries.iter().enumerate().take(i) {
        if let Some(name) = earlier.get("as").and_then(Value::as_str) {
            named.entry(name).or_insert(j);
        }
    }
    let cmd = entry
        .get("cmd")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut resolved = (*entry).clone();
    if let Value::Object(map) = &mut resolved {
        map.remove("as");
    }
    let mut resolve = |text: &str, field: Option<&str>| -> Result<Value, String> {
        let (name, path) = split_ref(text)?;
        let j = *named.get(name).ok_or_else(|| {
            format!(
                "{{\"ref\": \"{}\"}}: no earlier entry is named “{}”",
                echoed(text),
                echoed(name)
            )
        })?;
        let result = results.get(j).ok_or_else(|| {
            format!(
                "{{\"ref\": \"{}\"}}: entry {j} has no result yet",
                echoed(text)
            )
        })?;
        let value = lookup(result, &path, text, j)?;
        match wrong_kind(&cmd, field, value, text) {
            Some(error) => Err(error),
            None => Ok(value.clone()),
        }
    };
    if let Value::Object(map) = &mut resolved {
        for (key, value) in map.iter_mut() {
            if key == "cmd" {
                continue;
            }
            replace(value, Some(key.as_str()), &mut resolve)?;
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn refs(entries: &[Value]) -> Result<(), (usize, String)> {
        let entries: Vec<&Value> = entries.iter().collect();
        check_refs(&entries)
    }

    fn resolved(entries: &[Value], i: usize, results: &[Value]) -> Result<Value, String> {
        let entries: Vec<&Value> = entries.iter().collect();
        resolve_entry(&entries, i, results)
    }

    fn line() -> Value {
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0],
               "as": "l1"})
    }

    /// A line's result: its curve, its points, the points at its inputs.
    fn line_result() -> Value {
        json!({"entities": ["p1", "p2", "c3"], "made": ["c3"],
               "inputs": {"start": "p1", "end": "p2"}})
    }

    #[test]
    fn references_take_values_of_earlier_results_and_inputs_as_written() {
        let entries = [
            json!({"cmd": "sketch.create", "as": "sk"}),
            line(),
            json!({"cmd": "sketch.add_line", "sketch": {"ref": "sk.uid"},
                   "start": {"ref": "l1.end"}, "end": [10, 10]}),
            json!({"cmd": "sketch.add_constraint", "sketch": {"ref": "sk.uid"},
                   "constraint": {"type": "horizontal", "line": {"ref": "l1.made.0"}}}),
        ];
        assert_eq!(refs(&entries), Ok(()));
        let results = [json!({"uid": "F1", "name": "Sketch1"}), line_result()];
        // The name is left out, the references replaced.
        assert_eq!(
            resolved(&entries, 1, &results[..1]).unwrap(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [10, 0]})
        );
        assert_eq!(
            resolved(&entries, 2, &results).unwrap(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": "p2", "end": [10, 10]})
        );
        let constraint = resolved(&entries, 3, &results).unwrap();
        assert_eq!(constraint["constraint"]["line"], "c3");
        // The explicit path to the inputs is the same.
        let explicit = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l1.inputs.end"},
                   "end": [5, 5]}),
        ];
        assert_eq!(
            resolved(&explicit, 1, &[line_result()]).unwrap()["start"],
            "p2"
        );
        // A spline's first point, and a reference in a list of points.
        let spline = [
            json!({"cmd": "sketch.spline", "sketch": "F1", "points": [[0, 0], [5, 5], [9, 0]],
                   "as": "outline"}),
            json!({"cmd": "sketch.spline", "sketch": "F1",
                   "points": [{"ref": "outline.points.2"}, [12, 4], [15, 0]]}),
        ];
        let spline_result = json!({"made": ["c4"], "inputs": {"points": ["p1", "p2", "p3"]}});
        assert_eq!(
            resolved(&spline, 1, &[spline_result]).unwrap()["points"][0],
            "p3"
        );
        // Entries without references are as they were, but for "as".
        assert!(!has_refs(&entries[1]));
        assert!(has_refs(&entries[2]));
    }

    #[test]
    fn unknown_forward_and_malformed_references_refuse_the_array_before_it_runs() {
        let unknown = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l2.end"},
                   "end": [5, 5]}),
        ];
        let (entry, error) = refs(&unknown).unwrap_err();
        assert_eq!(entry, 1);
        assert!(
            error.contains("no entry of the array is named “l2”"),
            "{error}"
        );

        let forward = [
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l1.end"},
                   "end": [5, 5]}),
            line(),
        ];
        let (entry, error) = refs(&forward).unwrap_err();
        assert_eq!(entry, 0);
        assert!(
            error.contains("“l1” is entry 1, which runs after this one"),
            "{error}"
        );

        let own = [json!({"cmd": "sketch.add_line", "sketch": "F1", "as": "l1",
                          "start": [0, 0], "end": {"ref": "l1.start"}})];
        let (entry, error) = refs(&own).unwrap_err();
        assert_eq!(entry, 0);
        assert!(error.contains("is this entry"), "{error}");

        let twice = [line(), line()];
        let (entry, error) = refs(&twice).unwrap_err();
        assert_eq!(entry, 1);
        assert!(error.contains("already names entry 0"), "{error}");

        for (bad, says) in [
            (json!({"ref": "l1"}), "is no reference"),
            (json!({"ref": "l1..end"}), "is no reference"),
            (json!({"ref": 3}), "is no reference"),
            (json!({"ref": "l1.end", "x": 1}), "is no reference"),
        ] {
            let entries = [
                line(),
                json!({"cmd": "sketch.add_line", "sketch": "F1", "start": bad, "end": [5, 5]}),
            ];
            let (entry, error) = refs(&entries).unwrap_err();
            assert_eq!(entry, 1, "{error}");
            assert!(error.contains(says), "{error}");
        }
        for name in [json!("1st"), json!("a b"), json!(7), json!("x".repeat(65))] {
            let entries = [json!({"cmd": "sketch.create", "as": name})];
            let (entry, _) = refs(&entries).unwrap_err();
            assert_eq!(entry, 0, "{name}");
        }
        let cmd = [line(), json!({"cmd": {"ref": "l1.made.0"}})];
        assert_eq!(refs(&cmd).unwrap_err().0, 1);
    }

    #[test]
    fn missing_null_and_wrongly_typed_values_refuse_their_entry() {
        let results = [line_result()];
        let missing = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l1.ends"},
                   "end": [5, 5]}),
        ];
        let error = resolved(&missing, 1, &results).unwrap_err();
        assert!(
            error.contains("the result of entry 0 has no ends"),
            "{error}"
        );
        assert!(error.contains(", and inputs "), "{error}");
        assert!(error.contains("start"), "{error}");
        let past = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l1.made.3"},
                   "end": [5, 5]}),
        ];
        let error = resolved(&past, 1, &results).unwrap_err();
        assert!(
            error.contains("made of entry 0's result has no 3"),
            "{error}"
        );
        assert!(error.contains("a list of 1"), "{error}");
        // A curve where a drawing tool takes a point.
        let curve = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "l1.made.0"},
                   "end": [5, 5]}),
        ];
        let error = resolved(&curve, 1, &results).unwrap_err();
        assert!(error.starts_with("start: "), "{error}");
        assert!(error.contains("not a point"), "{error}");
        // An input that made no point.
        let arc = [
            json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
                   "start": [0, 0], "through": [5, 2], "end": [10, 0], "as": "a1"}),
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": {"ref": "a1.through"},
                   "end": [5, 5]}),
        ];
        let arc_result =
            json!({"made": ["c4"], "inputs": {"start": "p1", "through": null, "end": "p2"}});
        let error = resolved(&arc, 1, &[arc_result]).unwrap_err();
        assert!(error.contains("is null"), "{error}");
        // A sketch's uid is a text.
        let sketch = [
            line(),
            json!({"cmd": "sketch.add_line", "sketch": {"ref": "l1.inputs"},
                   "start": [0, 0], "end": [5, 5]}),
        ];
        let error = resolved(&sketch, 1, &results).unwrap_err();
        assert!(error.starts_with("sketch: "), "{error}");
        // Elsewhere a curve id is what the field takes.
        let fillet = [
            line(),
            json!({"cmd": "sketch.fillet", "sketch": "F1", "a": {"ref": "l1.made.0"},
                   "b": "c9", "radius": 2}),
        ];
        assert_eq!(resolved(&fillet, 1, &results).unwrap()["a"], "c3");
    }
}
