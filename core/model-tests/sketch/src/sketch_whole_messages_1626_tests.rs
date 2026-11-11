// SPDX-License-Identifier: MIT
//! Sketch refusals a person reads as whole messages (mitcad#1626): a
//! curve's kind, the axis of an equal spacing and how far apart two ends
//! are are written into the message itself, not put in as an English word
//! for a value, so that the application translates each message whole
//! (docs/translations.md "The model's messages").

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::notice::FeatureError;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn with_sketch() -> Document<MockKernel> {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    d
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// What a person reads of the refusal of `command`: its text before the
/// tail that carries the agents' detail.
fn refused(doc: &mut Document<MockKernel>, command: Value) -> String {
    let error = doc
        .command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string();
    FeatureError::from_text(&error).message
}

fn first(result: &Value, key: &str) -> String {
    result[key][0]
        .as_str()
        .unwrap_or_else(|| panic!("no {key} in {result}"))
        .to_owned()
}

fn line(d: &mut Document<MockKernel>, start: [f64; 2], end: [f64; 2]) -> String {
    first(
        &command(
            d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": start, "end": end}),
        ),
        "made",
    )
}

fn point(d: &mut Document<MockKernel>, at: [f64; 2]) -> String {
    first(
        &command(
            d,
            json!({"cmd": "sketch.add_point", "sketch": "F1", "at": at}),
        ),
        "entities",
    )
}

#[test]
fn a_curve_that_is_no_line_is_named_by_its_kind_in_the_message() {
    let mut d = with_sketch();
    let circle = first(
        &command(
            &mut d,
            json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10}),
        ),
        "curves",
    );
    assert_eq!(
        refused(
            &mut d,
            json!({"cmd": "sketch.set_centerline", "sketch": "F1", "lines": [circle]}),
        ),
        "the curve is a circle, not a line"
    );
}

#[test]
fn a_tie_of_an_equal_spacing_names_its_axis_in_the_message() {
    let mut d = with_sketch();
    // Two of the three at the same x, and at the same y.
    let anchors = [
        point(&mut d, [0.0, 0.0]),
        point(&mut d, [0.0, 10.0]),
        point(&mut d, [20.0, 0.0]),
    ];
    let tail = "; give their order (keep the listed order) or measure another way";
    assert_eq!(
        refused(
            &mut d,
            json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": anchors,
                   "mode": "horizontal"}),
        ),
        format!("equal spacing: two items are at the same place along the x axis{tail}")
    );
    assert_eq!(
        refused(
            &mut d,
            json!({"cmd": "sketch.add_equal_spacing", "sketch": "F1", "anchors": anchors,
                   "mode": "vertical"}),
        ),
        format!("equal spacing: two items are at the same place along the y axis{tail}")
    );
}

#[test]
fn ends_that_do_not_join_say_how_far_apart_in_the_message() {
    // Ends at one place that are two points.
    let mut d = with_sketch();
    let a = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut d, [10.0, 0.0], [10.0, 10.0]);
    assert_eq!(
        refused(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [a, b], "distance": 1}),
        ),
        "the curves are not one connected chain: a curve ends at another curve's end but on a \
         point of its own; a Coincident constraint joins them"
    );

    // Ends a few micrometres apart.
    let mut d = with_sketch();
    let a = line(&mut d, [0.0, 0.0], [10.0, 0.0]);
    let b = line(&mut d, [10.005, 0.0], [10.005, 10.0]);
    assert_eq!(
        refused(
            &mut d,
            json!({"cmd": "sketch.offset", "sketch": "F1", "curves": [a, b], "distance": 1}),
        ),
        "the curves are not one connected chain: a curve ends 0.005 mm from another curve's end \
         but on a point of its own; a Coincident constraint joins them"
    );
}
