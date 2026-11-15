// SPDX-License-Identifier: MIT
//! A pipe along a helix whose turns lie closer than the section is wide
//! (mitcad#1788), with the mock kernel: the pipe is refused at once in
//! words, the two turns that would pass through each other as its places,
//! and the kernel never builds it; a section that fits between the turns,
//! a pipe along less than a turn of the helix and an edit of the size down
//! build it.

use serde_json::{Value, json};

use mitcad_model::ids::FeatureUid;
use mitcad_model::notice::NoticePlace;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|e| panic!("{value}: {e}"));
    serde_json::from_str(&result).unwrap()
}

/// Four turns of radius 10 about z rising 5 a turn, as the Helix Curve of
/// the ticket's case makes them.
fn helix() -> Value {
    json!({"helix": {"center": [0, 0, 0], "axis": [0, 0, 1], "x_axis": [1, 0, 0],
        "radius": 10, "pitch": 5, "start_angle": 0, "end_angle": 8.0 * std::f64::consts::PI}})
}

/// Pipe1 (F1) of `size` along the helix, with `extra` fields.
fn pipe(doc: &mut Document<MockKernel>, size: f64, extra: Value) {
    let mut def = json!({"type": "pipe", "path": helix(), "size": size, "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        def[key] = item.clone();
    }
    command(doc, json!({"cmd": "add_feature", "def": def}));
}

fn pipes_built(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|spec| spec.starts_with("pipe "))
        .cloned()
        .collect()
}

#[test]
fn a_section_wider_than_the_turns_are_apart_is_refused_at_once() {
    // The ticket's case: the default section of 8 mm along turns 4.984 mm
    // apart (5 cos α, α the helix's angle).
    let mut doc = Document::new(MockKernel::default());
    pipe(&mut doc, 8.0, json!({}));
    let Some(FeatureStatus::Failed(error)) = doc.status(FeatureUid(1)) else {
        panic!("{:?}", doc.status(FeatureUid(1)));
    };
    assert!(
        error.message.ends_with(
            "the helix's turns lie 4.984 mm apart, so a section of 8 mm makes the pipe's turns \
             pass through each other; make the section smaller than 4.984 mm or the helix's \
             pitch larger"
        ),
        "{error:?}"
    );
    // The two turns as places: points on them, in the middle of the helix
    // (10 mm up), one a little less than the pitch above the other; their
    // coordinates in the detail only.
    let heights: Vec<f64> = error
        .places
        .iter()
        .map(|place| match place {
            NoticePlace::Point { at } => {
                assert!((at[0].hypot(at[1]) - 10.0).abs() < 1e-9, "{at:?}");
                at[2]
            }
            other => panic!("not a point: {other:?}"),
        })
        .collect();
    assert_eq!(heights.len(), 2, "{error:?}");
    assert!(
        ((heights[0] + heights[1]) / 2.0 - 10.0).abs() < 1e-6,
        "{heights:?}"
    );
    assert!((heights[1] - heights[0] - 5.0).abs() < 0.05, "{heights:?}");
    assert!(!error.message.contains('('), "{error:?}");
    assert!(
        error.detail.contains("the turns come that close at ("),
        "{error:?}"
    );
    // Refused before the geometry: the kernel was never asked.
    assert!(pipes_built(&doc).is_empty(), "{:?}", pipes_built(&doc));
}

#[test]
fn a_section_between_the_turns_is_built() {
    let mut doc = Document::new(MockKernel::default());
    pipe(&mut doc, 2.0, json!({}));
    assert_eq!(doc.status(FeatureUid(1)), Some(&FeatureStatus::Ok));
    assert_eq!(pipes_built(&doc).len(), 1, "{:?}", pipes_built(&doc));
    // Hollow too: only the section's outside counts.
    let mut doc = Document::new(MockKernel::default());
    pipe(&mut doc, 4.0, json!({"thickness": 1}));
    assert_eq!(doc.status(FeatureUid(1)), Some(&FeatureStatus::Ok));
}

#[test]
fn a_pipe_along_less_than_a_turn_is_built() {
    // A fifth of four turns is less than one: no turn has another beside it.
    let mut doc = Document::new(MockKernel::default());
    pipe(
        &mut doc,
        8.0,
        json!({"extent": {"type": "partial", "fraction": 0.2}}),
    );
    assert_eq!(doc.status(FeatureUid(1)), Some(&FeatureStatus::Ok));
    assert_eq!(pipes_built(&doc).len(), 1, "{:?}", pipes_built(&doc));
}

#[test]
fn a_refused_pipe_edited_to_a_smaller_size_is_built() {
    // As the panel does when the size typed after the path is 2 mm.
    let mut doc = Document::new(MockKernel::default());
    pipe(&mut doc, 8.0, json!({}));
    assert!(matches!(
        doc.status(FeatureUid(1)),
        Some(FeatureStatus::Failed(_))
    ));
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": "F1",
               "def": {"type": "pipe", "path": helix(), "size": 2, "operation": "new_body"}}),
    );
    assert_eq!(doc.status(FeatureUid(1)), Some(&FeatureStatus::Ok));
    let built = pipes_built(&doc);
    assert_eq!(built.len(), 1, "{built:?}");
    assert!(built[0].contains(" size 2 "), "{built:?}");
}
