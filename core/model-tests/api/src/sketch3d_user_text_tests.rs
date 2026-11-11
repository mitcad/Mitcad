// SPDX-License-Identifier: MIT
//! What a person reads of a 3D sketch's refusals and failures names no
//! ids, topological names or points (mitcad#1644, as mitcad#1612 for the
//! planar sketch; `docs/ui-conventions.md` "What a message says"): a
//! relation that contradicts the route, an over-constrained route, a
//! segment without length, a lost reference and a path that branches,
//! through the api with the mock kernel. The text before a refusal's tail
//! is checked as it is written (not as the fallback cleaner would show
//! it), so that a new id in a message fails here; agents keep the ids in
//! the detail, and the places say where.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::{FeatureError, PLACED_TAIL, user_text_leaks};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(
        &doc.query(&query.to_string())
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}

/// The text a refusal shows as it was written: before its tail, not
/// cleaned.
fn written(text: &str) -> &str {
    text.find(PLACED_TAIL).map_or(text, |at| &text[..at])
}

/// That `text`, shown to a person, names nothing it may not.
fn assert_plain(text: &str) {
    assert_eq!(user_text_leaks(text), None, "a shown text leaks: {text:?}");
}

/// The route of three lines, A at the origin, along X, Y and Z with their
/// lengths: its results.
fn route(doc: &mut Document<MockKernel>) -> [Value; 3] {
    let ab = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0],
               "end": [37, 2, 1], "axis": "x", "length": 40, "start_source": "origin"}),
    );
    let bc = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": ab["end"],
               "end": [42, 27, 0], "axis": "y", "length": 30}),
    );
    let cd = command(
        doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"],
               "end": [40, 33, 21], "axis": "z", "sense": "positive", "length": 20}),
    );
    [ab, bc, cd]
}

fn made(result: &Value) -> Value {
    result["made"][0].clone()
}

#[test]
fn a_skew_line_and_an_over_constrained_route_are_refused_in_words() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, _, _] = route(&mut doc);
    let line = made(&ab);
    // AB along Y too: the relations contradict.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_constraint", "sketch": "F1", "constraint":
               {"type": "align", "line": line, "axis": "y", "sense": "positive"}}),
    );
    let shown = written(&error);
    assert_plain(shown);
    assert!(shown.contains("3DSketch1"), "{error}");
    assert!(
        shown.contains("contradict each other") || shown.contains("invalid geometry"),
        "{error}"
    );
    // Agents: the relations by their ids, in a detail of its own.
    let read = FeatureError::from_text(&error);
    assert!(!read.detail.is_empty(), "{error}");

    // AB 50 along X by an offset, beside its length of 40.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_dimension", "sketch": "F1",
               "dimension": {"type": "offset", "a": ab["start"], "b": ab["end"], "axis": "x"},
               "value": 50}),
    );
    let shown = written(&error);
    assert_plain(shown);
    assert!(shown.starts_with("3DSketch1: "), "{error}");
}

#[test]
fn a_segment_without_length_and_a_missing_point_are_refused_in_words() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [_, _, cd] = route(&mut doc);
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": cd["end"],
               "end": [40, 30, 20]}),
    );
    assert_plain(written(&error));
    assert!(written(&error).contains("no length"), "{error}");
    // A point the sketch does not have: words for a person, the id for
    // agents.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": ["p99"]}),
    );
    assert_plain(written(&error));
    assert!(
        FeatureError::from_text(&error).agent_text().contains("p99"),
        "{error}"
    );
    // A point fixed twice.
    let fixed = json!({"cmd": "sketch3d.fix", "sketch": "F1", "points": [cd["end"]]});
    command(&mut doc, fixed.clone());
    let error = rejected(&mut doc, fixed);
    assert_plain(written(&error));
    assert!(written(&error).contains("fixed already"), "{error}");
}

/// A box's edges for the mock kernel, as `sketch3d_tests` gives them.
fn box_edges(doc: &Document<MockKernel>) {
    for (name, start, end) in [
        ("E{F1:side0|F1:top}", [0.0, 0.0, 20.0], [40.0, 0.0, 20.0]),
        ("E{F1:side0|F1:side1}", [40.0, 0.0, 0.0], [40.0, 0.0, 20.0]),
    ] {
        let key = name
            .parse::<mitcad_model::topo::TopoName>()
            .unwrap()
            .to_string();
        doc.kernel()
            .named_curves
            .borrow_mut()
            .insert(key, mitcad_model::kernel::Curve3::Line { start, end });
    }
}

#[test]
fn a_lost_reference_says_it_in_words_with_its_edge_painted() {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
               "length": 40, "width": 30, "height": 20, "operation": "new_body"}}),
    );
    box_edges(&doc);
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let edge = "E{F1:side0|F1:top}";
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_point", "sketch": "F2", "at": [19, 1, 19],
               "source": {"body": "F1.b0", "edge": edge, "at": "middle"}}),
    );
    // The box suppressed: the attachment's source is lost.
    command(&mut doc, json!({"cmd": "suppress_feature", "uid": "F1"}));
    let s = query(&doc, json!({"query": "sketch3d", "uid": "F2"}));
    let failure = &s["failure"];
    assert_eq!(failure["kind"], "reference", "{s}");
    let message = failure["message"].as_str().unwrap();
    assert_plain(message);
    assert!(message.contains("source of an attachment"), "{s}");
    // Agents: the point and the edge by name; the edge is a place.
    let named = edge
        .parse::<mitcad_model::topo::TopoName>()
        .unwrap()
        .to_string();
    let detail = failure["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("attached to") && detail.contains(&named),
        "{s}"
    );
    let places = failure["places"].as_array().cloned().unwrap_or_default();
    assert!(
        places
            .iter()
            .any(|p| p["kind"] == "edge" && p["name"].as_str().is_some()),
        "{s}"
    );
    // The record's own message, likewise.
    for record in s["references"].as_array().unwrap() {
        if let Some(message) = record["message"].as_str() {
            assert_plain(message);
        }
    }
}

#[test]
fn a_path_that_branches_says_it_in_words_with_its_point_painted() {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch3d.create"}));
    let [ab, bc, _] = route(&mut doc);
    // A line on from C: three curves meet there.
    command(
        &mut doc,
        json!({"cmd": "sketch3d.add_line", "sketch": "F1", "start": bc["end"],
               "end": [60, 30, 0]}),
    );
    let r = query(
        &doc,
        json!({"query": "sketch3d", "uid": "F1",
               "path": {"curves": [made(&ab)], "chain": true}}),
    )["route"]
        .clone();
    let message = r["message"].as_str().unwrap_or_else(|| panic!("{r}"));
    assert_plain(message);
    assert!(message.contains("the path branches at a point"), "{r}");
    assert!(
        r["error"]
            .as_str()
            .unwrap()
            .contains("the path branches at p"),
        "{r}"
    );
    let places = r["places"].as_array().cloned().unwrap_or_default();
    assert!(places.iter().any(|p| p["kind"] == "point"), "{r}");
    // A curve the sketch does not have: words, and the id for agents.
    let r = query(
        &doc,
        json!({"query": "sketch3d", "uid": "F1", "path": {"curves": ["c99"]}}),
    )["route"]
        .clone();
    assert_plain(r["message"].as_str().unwrap_or_else(|| panic!("{r}")));
    assert!(r["error"].as_str().unwrap().contains("c99"), "{r}");
}

// The scan of the 3D sketches' sources (mitcad#1644, as the geometry's
// `geometry.user-text-scan` of mitcad#1612): a message literal of these
// files that puts an id, a topological name or a source into its text (a
// placeholder after `p`, `c`, `k` or `t`, or one named `id`, `uid`,
// `source`, `edge`, ...) is refused, unless its statement pairs it with a
// person's words (`said`, `said_at`, `said_after` and the files' own
// helpers) or builds an agents' text. A message that names the API's own
// JSON form (with `\"`) is the agents'.

/// The sources whose messages the scan reads.
const SOURCES: &[(&str, &str)] = &[
    (
        "core/sketch/src/spatial.rs",
        include_str!("../../../sketch/src/spatial.rs"),
    ),
    (
        "core/sketch/src/spatial_edit.rs",
        include_str!("../../../sketch/src/spatial_edit.rs"),
    ),
    (
        "core/sketch/src/spatial_convert.rs",
        include_str!("../../../sketch/src/spatial_convert.rs"),
    ),
    (
        "core/sketch/src/spatial_planar.rs",
        include_str!("../../../sketch/src/spatial_planar.rs"),
    ),
    (
        "core/model/src/features/spatial_sketch.rs",
        include_str!("../../../model/src/features/spatial_sketch.rs"),
    ),
    (
        "core/model/src/features/spatial_path.rs",
        include_str!("../../../model/src/features/spatial_path.rs"),
    ),
    (
        "core/model/src/document/spatial_sketches.rs",
        include_str!("../../../model/src/document/spatial_sketches.rs"),
    ),
    (
        "core/model/src/document/sketch_conversion.rs",
        include_str!("../../../model/src/document/sketch_conversion.rs"),
    ),
    (
        "core/model-api/src/sketch3d.rs",
        include_str!("../../../model-api/src/sketch3d.rs"),
    ),
];

/// Calls that pair a person's words with the agents' text, and the
/// agents' own texts (a source's `Display`, the lists of a conversion's
/// result).
const PAIRED: &[&str] = &[
    "said(",
    "said_at(",
    "said_after(",
    "said_around(",
    "about_source(",
    "failure_about(",
    "cannot_become(",
    "off_plane(",
    "twice(",
    "write!(",
    ".kept",
    ".removed",
    ".projected",
];

/// Agents' texts by a piece of them, with why: a request that names a
/// feature uid the document does not have, and the route query's
/// `error` for a uid that is no 3D sketch (the application sends the
/// uids it has); the details a conversion's refusal joins.
const ALLOWED: &[&str] = &[
    "feature {sketch} does not exist",
    "feature {uid} does not exist",
    "{uid} is not a 3D sketch",
    "keeps p{} on {source}",
];

/// Placeholders that hold an id, a topological name or a source here.
const NAMED: &[&str] = &[
    "id", "uid", "owner", "source", "sketch", "edge", "face", "vertex", "what", "point", "curve",
    "line", "r",
];

/// A statement of a source: from after one `;` or brace to the next, its
/// first line, its code (comments left out) and its string literals (as
/// written, escapes kept, a line continuation joined).
struct Statement {
    line: usize,
    text: String,
    literals: Vec<String>,
}

fn statements(source: &str) -> Vec<Statement> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut text = String::new();
    let mut literals = Vec::new();
    let (mut line, mut start, mut i) = (1, 1, 0);
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // A statement starts on the line of its first character.
        if !c.is_whitespace() && text.trim().is_empty() {
            start = line;
        }
        match c {
            '\n' => {
                line += 1;
                text.push(' ');
                i += 1;
            }
            // A char literal ('a', '\n', '"'), or a lifetime.
            '\'' if chars.get(i + 1) == Some(&'\\') => {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
            }
            '\'' if chars.get(i + 2) == Some(&'\'') => i += 3,
            '"' => {
                let mut literal = String::new();
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '"' {
                    if chars[j] == '\\' {
                        if chars.get(j + 1) == Some(&'\n') {
                            j += 1;
                            while j < chars.len() && chars[j].is_whitespace() {
                                if chars[j] == '\n' {
                                    line += 1;
                                }
                                j += 1;
                            }
                            continue;
                        }
                        literal.push('\\');
                        literal.extend(chars.get(j + 1));
                        j += 2;
                        continue;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    literal.push(chars[j]);
                    j += 1;
                }
                text.push('"');
                text.push_str(&literal);
                text.push('"');
                literals.push(literal);
                i = j + 1;
            }
            ';' | '{' | '}' => {
                if !text.trim().is_empty() {
                    out.push(Statement {
                        line: start,
                        text: std::mem::take(&mut text),
                        literals: std::mem::take(&mut literals),
                    });
                }
                text.clear();
                literals.clear();
                i += 1;
            }
            _ => {
                text.push(c);
                i += 1;
            }
        }
    }
    out
}

/// A format string with each placeholder as a space, and each
/// placeholder's name with the text before it.
fn placeholders(literal: &str) -> (String, Vec<(String, String)>) {
    let chars: Vec<char> = literal.chars().collect();
    let mut fixed = String::new();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if (c == '{' || c == '}') && chars.get(i + 1) == Some(&c) {
            fixed.push(if c == '{' { '(' } else { ')' });
            i += 2;
            continue;
        }
        if c == '{'
            && let Some(close) = chars[i..].iter().position(|d| *d == '}')
        {
            let inner: String = chars[i + 1..i + close].iter().collect();
            let name = inner.split(':').next().unwrap_or_default();
            if name.chars().all(|d| d.is_ascii_alphanumeric() || d == '_') {
                found.push((fixed.clone(), name.to_owned()));
                fixed.push(' ');
                i += close + 1;
                continue;
            }
        }
        fixed.push(c);
        i += 1;
    }
    (fixed, found)
}

/// Two words of letters side by side: a message, as the translations'
/// scanner takes it (`tools/translations`).
fn is_message(fixed: &str) -> bool {
    let chars: Vec<char> = fixed.chars().collect();
    chars.windows(5).any(|w| {
        w[2] == ' '
            && w[0].is_ascii_alphabetic()
            && w[1].is_ascii_alphabetic()
            && w[3].is_ascii_alphabetic()
            && w[4].is_ascii_alphabetic()
    })
}

/// Whether a placeholder (its name, the text before it) holds an id.
fn names_id(before: &str, name: &str) -> bool {
    let mut back = before.chars().rev();
    let prefix = matches!(back.next(), Some('p' | 'c' | 'k' | 't'))
        && back
            .next()
            .is_none_or(|d| !(d.is_ascii_alphanumeric() || d == '_' || d == '.'));
    prefix || NAMED.contains(&name)
}

/// The messages of `source` (named `path`) that put an id into a text a
/// person reads, as `path:line: {name} in "text"`.
fn leaks(path: &str, source: &str) -> Vec<String> {
    let code = source
        .find("#[cfg(test)]\nmod ")
        .map_or(source, |at| &source[..at]);
    let mut found = Vec::new();
    for statement in statements(code) {
        if PAIRED.iter().any(|call| statement.text.contains(call)) {
            continue;
        }
        for literal in &statement.literals {
            if literal.contains("\\\"") || ALLOWED.iter().any(|a| literal.contains(a)) {
                continue;
            }
            let (fixed, holders) = placeholders(literal);
            if !is_message(&fixed) {
                continue;
            }
            if let Some((_, name)) = holders.iter().find(|(b, n)| names_id(b, n)) {
                found.push(format!(
                    "{path}:{}: {{{name}}} in \"{literal}\"",
                    statement.line
                ));
            }
        }
    }
    found
}

#[test]
fn the_3d_sketch_sources_put_no_ids_into_messages() {
    let found: Vec<String> = SOURCES
        .iter()
        .flat_map(|(path, source)| leaks(path, source))
        .collect();
    assert!(
        found.is_empty(),
        "messages that name ids where a person reads them (give the words to `said` and the \
         ids to its detail, mitcad#1644):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_scan_finds_ids_and_passes_paired_words() {
    let planted = r#"
fn refuse(id: EntityUid, k: ConstraintUid) -> Result<(), String> {
    if id.0 == 0 {
        return Err(format!("point p{} does not exist", id.0));
    }
    let said_one = crate::said("the point does not exist", format!("point p{} does not exist", id.0));
    let shape = format!("{} c{}", "line", id.0);
    Err(format!("{k} is fixed already; the source is {source}"))
}
"#;
    let found = leaks("planted.rs", planted);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].starts_with("planted.rs:4: {}"), "{found:#?}");
    assert!(
        found[1].contains("{source}") || found[1].contains("{k}"),
        "{found:#?}"
    );
}
