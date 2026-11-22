// SPDX-License-Identifier: MIT
//! A refused parameter expression in words (mitcad#2181,
//! `docs/ui-conventions.md` "What a message says"): what a person reads of
//! the refusal (the text before its `[placed: …]` line) says what is wrong
//! and where in words, without the parser's byte offset and without a
//! "parameter X:" prefix, and offers the near miss of an unknown name; the
//! agents' detail keeps the model's text with the offset. The same in the
//! `formula_help` and `evaluate` answers that the dialogs check while a
//! formula is typed.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::{FeatureError, PLACED_TAIL};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn run(doc: &mut Document<MockKernel>, command: Value) {
    doc.command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {}", e.0));
}

/// The refusal of `command`: what a person reads (the text before its
/// `[placed: …]` line, as the application shows it, `userText`) and the
/// agents' text. Not `FeatureError::from_text`'s message: its fallback
/// cleaner of older texts takes a user's parameter names that look like
/// ids (`p1`, `c2`) for a sketch's.
fn refused(doc: &mut Document<MockKernel>, command: Value) -> (String, String) {
    let error = doc.command(&command.to_string()).expect_err("refused").0;
    let shown = error
        .find(PLACED_TAIL)
        .map_or(&error[..], |at| &error[..at]);
    let failure = FeatureError::from_text(&error);
    (shown.to_owned(), failure.agent_text().to_owned())
}

fn query(doc: &Document<MockKernel>, query: Value) -> Result<Value, String> {
    doc.query(&query.to_string())
        .map(|answer| serde_json::from_str(&answer).unwrap())
        .map_err(|e| e.0)
}

fn add(name: &str, expression: &str) -> Value {
    json!({"cmd": "add_parameter", "name": name, "expression": expression, "unit": "mm"})
}

#[test]
fn an_unknown_name_is_said_in_words_and_the_offset_stays_in_the_detail() {
    // The reported case: the parameter it uses is added after the refusal.
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, add("sheet_thickness", "18 mm"));
    let (shown, agents) = refused(
        &mut doc,
        add("cabinet_lenght", "table_lenght+sheet_thickness"),
    );
    assert_eq!(
        shown,
        "The expression of cabinet_lenght uses table_lenght, which is not a parameter."
    );
    assert!(!shown.contains("at byte"), "{shown}");
    assert_eq!(
        agents,
        "parameter cabinet_lenght: unknown parameter table_lenght at byte 0"
    );
    run(&mut doc, add("table_lenght", "1200 mm"));
    run(
        &mut doc,
        add("cabinet_lenght", "table_lenght+sheet_thickness"),
    );
    let parameters = query(&doc, json!({"query": "parameters"})).unwrap();
    let cabinet = parameters
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "cabinet_lenght")
        .expect("added");
    assert_eq!(cabinet["value"], json!(1218.0));
}

#[test]
fn a_near_miss_is_offered() {
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, add("table_length", "1200 mm"));
    let (shown, agents) = refused(&mut doc, add("cabinet_length", "table_lenght + 18 mm"));
    assert_eq!(
        shown,
        "The expression of cabinet_length uses table_lenght, which is not a parameter. \
         Did you mean table_length?"
    );
    assert!(agents.ends_with("at byte 0"), "{agents}");
    // A changed expression, and a limit, the same.
    run(&mut doc, add("depth", "600 mm"));
    let (shown, _) = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "depth", "expression": "table_lenght / 2"}),
    );
    assert_eq!(
        shown,
        "The expression of depth uses table_lenght, which is not a parameter. \
         Did you mean table_length?"
    );
    let (shown, agents) = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "depth", "min": "table_lenght / 4"}),
    );
    assert_eq!(
        shown,
        "The minimum of depth uses table_lenght, which is not a parameter. \
         Did you mean table_length?"
    );
    assert!(agents.contains("parameter depth: minimum:"), "{agents}");
    assert!(agents.contains("at byte 0"), "{agents}");
}

#[test]
fn syntax_and_unit_errors_have_no_offset() {
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, add("width", "40 mm"));
    let (shown, agents) = refused(&mut doc, add("height", "width *"));
    assert!(
        shown.starts_with("The expression of height cannot be read: "),
        "{shown}"
    );
    assert!(!shown.contains("byte"), "{shown}");
    assert!(agents.contains("at byte"), "{agents}");
    run(&mut doc, add("gap", "10 mm"));
    let (shown, agents) = refused(
        &mut doc,
        json!({"cmd": "set_parameter", "name": "gap", "expression": "width * width / (width - width)"}),
    );
    assert_eq!(
        shown,
        "The expression of gap cannot be worked out: division by zero."
    );
    assert!(agents.contains("division by zero at byte"), "{agents}");
}

#[test]
fn a_name_like_an_id_is_the_users_own() {
    // `p1` and `c2` are a sketch's ids elsewhere; here they are the user's
    // parameter names and stay in the message.
    let mut doc = Document::new(MockKernel::default());
    let (shown, _) = refused(&mut doc, add("p1", "c2 + 1 mm"));
    assert_eq!(
        shown,
        "The expression of p1 uses c2, which is not a parameter."
    );
}

#[test]
fn the_formula_checks_say_it_in_words_too() {
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, add("table_length", "1200 mm"));
    let help = query(
        &doc,
        json!({"query": "formula_help", "expression": "2 * table_lenght", "cursor": 0}),
    )
    .unwrap();
    let error = &help["error"];
    assert_eq!(
        error["message"],
        json!("table_lenght is not a parameter. Did you mean table_length?")
    );
    assert_eq!(
        error["detail"],
        json!("unknown parameter table_lenght at byte 4")
    );
    // The field marks the name from these.
    assert_eq!(
        (error["start"].clone(), error["end"].clone()),
        (json!(4), json!(16))
    );
    let help = query(
        &doc,
        json!({"query": "formula_help", "expression": "2 *", "cursor": 0}),
    )
    .unwrap();
    let message = help["error"]["message"].as_str().unwrap();
    assert!(!message.contains("byte"), "{message}");
    // The value fields' check.
    let error = query(
        &doc,
        json!({"query": "evaluate", "expression": "table_lenght * 2"}),
    )
    .unwrap_err();
    let failure = FeatureError::from_text(&error);
    assert_eq!(
        failure.message,
        "table_lenght is not a parameter. Did you mean table_length?"
    );
    assert_eq!(
        failure.agent_text(),
        "unknown parameter table_lenght at byte 0"
    );
}

#[test]
fn formula_help_checks_in_a_new_parameters_unit() {
    // Add User Parameter's check while it is typed (mitcad#2181): the unit
    // of the parameter it adds, in place of a kind.
    let mut doc = Document::new(MockKernel::default());
    run(&mut doc, add("width", "40 mm"));
    let help = |expression: &str, unit: &str| {
        query(
            &doc,
            json!({"query": "formula_help", "expression": expression, "unit": unit}),
        )
        .unwrap()
    };
    assert!(help("width * 2", "mm").get("error").is_none());
    assert_eq!(
        help("width * 2", "deg")["error"]["message"],
        json!("expected angle, found length")
    );
    assert!(help("width * width", "mm^2").get("error").is_none());
    assert!(help("width / 1 mm", "").get("error").is_none());
    assert!(help("'Box'", "Text").get("error").is_none());
    assert!(
        query(
            &doc,
            json!({"query": "formula_help", "expression": "1", "unit": "parsecs"}),
        )
        .is_err()
    );
}
