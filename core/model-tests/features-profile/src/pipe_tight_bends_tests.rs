// SPDX-License-Identifier: MIT
//! A pipe whose path bends tighter than its section (mitcad#1497), with the
//! mock kernel: an L path filleted with a radius smaller than the section's
//! warns with the place and the numbers, a larger one does not; a chamfer
//! too short for the miters at its two ends warns, unless the corners are
//! rounded; a pipe along part of its path is not checked.

use serde_json::{Value, json};

use mitcad_model::ids::FeatureUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    let result = doc
        .command(&value.to_string())
        .unwrap_or_else(|e| panic!("{value}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn line(doc: &mut Document<MockKernel>, a: [f64; 2], b: [f64; 2]) -> String {
    let result = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": a, "end": b}),
    );
    result["made"][0].as_str().unwrap().to_owned()
}

/// Sketch1 (F1) on XY with an L: c1 from the origin along X to (40, 0),
/// then up to (40, 30). Its curve ids.
fn l_path() -> (Document<MockKernel>, String, String) {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create", "plane": "xy"}));
    let first = line(&mut doc, [0.0, 0.0], [40.0, 0.0]);
    let second = line(&mut doc, [40.0, 0.0], [40.0, 30.0]);
    (doc, first, second)
}

/// Pipe1 (F2) of size 10 (a section reaching 5 mm from the path) along
/// the chain of curves from `first`, with `extra` fields; its warnings.
fn pipe(doc: &mut Document<MockKernel>, first: &str, extra: Value) -> Vec<String> {
    let mut def = json!({"type": "pipe", "path": {"sketch": "F1", "curves": [first], "chain": true},
        "size": 10, "operation": "new_body"});
    for (key, item) in extra.as_object().unwrap() {
        def[key] = item.clone();
    }
    command(doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(doc.status(FeatureUid(2)), Some(&FeatureStatus::Ok));
    doc.warnings(FeatureUid(2)).to_vec()
}

fn fillet(doc: &mut Document<MockKernel>, a: &str, b: &str, radius: f64) -> String {
    let result = command(
        doc,
        json!({"cmd": "sketch.fillet", "sketch": "F1", "a": a, "b": b, "radius": radius}),
    );
    result["made"][0].as_str().unwrap_or_default().to_owned()
}

#[test]
fn a_fillet_tighter_than_the_section_warns_with_the_numbers() {
    let (mut doc, first, second) = l_path();
    fillet(&mut doc, &first, &second, 3.0);
    let said = pipe(&mut doc, &first, json!({}));
    assert_eq!(said.len(), 1, "{said:?}");
    // The curve by its sketch's name, no id (mitcad#1612).
    let warning = &said[0];
    assert_eq!(
        warning,
        "the path bends with radius 3 mm at a curve of Sketch1; the pipe needs at least 5 mm"
    );
}

#[test]
fn a_fillet_wider_than_the_section_does_not_warn() {
    let (mut doc, first, second) = l_path();
    fillet(&mut doc, &first, &second, 6.0);
    let said = pipe(&mut doc, &first, json!({}));
    assert!(said.is_empty(), "{said:?}");
}

#[test]
fn a_chamfer_too_short_for_its_miters_warns() {
    // A chamfer 2 mm back from the corner: 2√2 = 2.828 mm long, turning
    // 45° at each end. Each miter cuts the inner side back by
    // 5 tan(22.5°) = 2.071 mm, so the two need 4.142 mm.
    let chamfered = || {
        let (mut doc, first, second) = l_path();
        command(
            &mut doc,
            json!({"cmd": "sketch.chamfer", "sketch": "F1", "a": first, "b": second,
                   "distance": 2}),
        );
        (doc, first)
    };
    let (mut doc, first) = chamfered();
    let said = pipe(&mut doc, &first, json!({}));
    assert_eq!(said.len(), 1, "{said:?}");
    let warning = &said[0];
    // The curve by its sketch's name, no id (mitcad#1612).
    assert!(warning.starts_with("a curve of Sketch1 is "), "{warning}");
    assert!(
        warning.ends_with(
            " is 2.828 mm long; the pipe's mitered corners need at least 4.142 mm there"
        ),
        "{warning}"
    );
    // Rounded corners end the legs square at each corner: no miters.
    let (mut doc, first) = chamfered();
    let said = pipe(&mut doc, &first, json!({"corners": "rounded"}));
    assert!(said.is_empty(), "{said:?}");
}

#[test]
fn a_chamfer_long_enough_for_its_miters_does_not_warn() {
    // 5 mm back: 7.071 mm long, more than the 4.142 mm the miters need.
    let (mut doc, first, second) = l_path();
    command(
        &mut doc,
        json!({"cmd": "sketch.chamfer", "sketch": "F1", "a": first, "b": second, "distance": 5}),
    );
    let said = pipe(&mut doc, &first, json!({}));
    assert!(said.is_empty(), "{said:?}");
}

#[test]
fn a_pipe_along_part_of_its_path_is_not_checked() {
    // The path is about 68.7 mm long; the first half ends before the
    // fillet.
    let (mut doc, first, second) = l_path();
    fillet(&mut doc, &first, &second, 3.0);
    let said = pipe(
        &mut doc,
        &first,
        json!({"extent": {"type": "partial", "fraction": 0.5}}),
    );
    assert!(said.is_empty(), "{said:?}");
}

#[test]
fn folds_the_geometry_built_in_pieces_are_notes() {
    // (mitcad#1697) Where the geometry builds the pipe in pieces fused into
    // one solid and says so, the tight fillet and the short chamfer are
    // notes, not warnings.
    let (mut doc, first, second) = l_path();
    fillet(&mut doc, &first, &second, 3.0);
    doc.kernel()
        .infos
        .borrow_mut()
        .insert("pipe", mitcad_model::sweeps::FOLDS_RESOLVED.to_owned());
    let said = pipe(&mut doc, &first, json!({}));
    assert!(said.is_empty(), "{said:?}");
    assert_eq!(
        doc.infos(FeatureUid(2)),
        [
            "the path bends with radius 3 mm at a curve of Sketch1, tighter than the 5 mm the pipe \
          reaches; the pipe was built in pieces there, joined into one solid"
        ]
    );

    let (mut doc, first, second) = l_path();
    command(
        &mut doc,
        json!({"cmd": "sketch.chamfer", "sketch": "F1", "a": first, "b": second, "distance": 2}),
    );
    doc.kernel()
        .infos
        .borrow_mut()
        .insert("pipe", mitcad_model::sweeps::FOLDS_RESOLVED.to_owned());
    let said = pipe(&mut doc, &first, json!({}));
    assert!(said.is_empty(), "{said:?}");
    let notes = doc.infos(FeatureUid(2));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].ends_with(
            " is 2.828 mm long, shorter than the 4.142 mm the pipe's mitered corners need; the \
             pipe was built in pieces there, joined into one solid"
        ),
        "{notes:?}"
    );
}
