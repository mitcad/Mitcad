// SPDX-License-Identifier: MIT
//! Occurrence placements in the design file (mitcad#1664): one a little off
//! a rotation (rounding piled up by many drags in an older build) opens
//! straightened with a warning that names the occurrence, a scale or a
//! mirror still refuses the file, and what a command stores is a rotation
//! and a translation.

use serde_json::Value;

use mitcad_model::Document;
use mitcad_model::assembly::{is_rigid, orthonormality_error};
use mitcad_model::ids::OccurrenceUid;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;

/// A turn about a skew axis and a shift.
fn placement() -> Transform {
    Transform::translation([12.0, -4.0, 30.0])
        .after(&Transform::rotation([0.0; 3], [1.0, 2.0, 2.0], 0.7).unwrap())
}

/// A design with one occurrence, `Arm:1`, at [`placement`], as a file.
fn design() -> (Document<MockKernel>, OccurrenceUid, Value) {
    let mut doc = Document::new(MockKernel::default());
    let (_, arm) = doc
        .create_component(Some("Arm"), placement(), false)
        .unwrap();
    let file = serde_json::from_str(&doc.to_json()).unwrap();
    (doc, arm, file)
}

/// The file with its first occurrence's rotation changed by `change`.
fn with_rotation(file: &Value, change: impl Fn(usize, usize, f64) -> f64) -> String {
    let mut file = file.clone();
    let rows = file["occurrences"][0]["transform"].as_array_mut().unwrap();
    for (r, row) in rows.iter_mut().enumerate().take(3) {
        let row = row.as_array_mut().unwrap();
        for (c, entry) in row.iter_mut().enumerate().take(3) {
            *entry = Value::from(change(r, c, entry.as_f64().unwrap()));
        }
    }
    file.to_string()
}

fn transform(doc: &Document<MockKernel>, uid: OccurrenceUid) -> Transform {
    doc.assembly().occurrence(uid).unwrap().transform
}

#[test]
fn a_placement_a_little_off_a_rotation_opens_straightened() {
    let (_, arm, file) = design();
    // 1e-5 off a rotation in two entries: the old check refused it.
    let off = with_rotation(&file, |r, c, v| match (r, c) {
        (0, 1) => v + 1e-5,
        (2, 0) => v - 1e-5,
        _ => v,
    });
    let doc = Document::from_json(&off, MockKernel::default()).unwrap();
    assert_eq!(
        doc.load_warnings(),
        [
            "the placement of Arm:1 was distorted and has been straightened \
          to a rotation and a translation"
        ]
    );
    let straightened = transform(&doc, arm);
    assert!(is_rigid(&straightened));
    assert!(orthonormality_error(&straightened.linear) <= 1e-12);
    assert_eq!(straightened.translation, placement().translation);
    let exact = placement();
    assert!(
        straightened
            .linear
            .iter()
            .flatten()
            .zip(exact.linear.iter().flatten())
            .all(|(a, b)| (a - b).abs() < 1e-4),
        "{straightened:?}"
    );
    // Saved, it opens again as it is, without a warning.
    let again = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert!(
        again.load_warnings().is_empty(),
        "{:?}",
        again.load_warnings()
    );
    assert_eq!(transform(&again, arm), straightened);

    // Within the rounding the file always allowed (1e-6), it opens as it
    // is and says nothing.
    let rounded = with_rotation(&file, |r, c, v| if (r, c) == (1, 2) { v + 1e-8 } else { v });
    let doc = Document::from_json(&rounded, MockKernel::default()).unwrap();
    assert!(doc.load_warnings().is_empty(), "{:?}", doc.load_warnings());
    let expected = placement().linear[1][2] + 1e-8;
    assert_eq!(transform(&doc, arm).linear[1][2], expected);
}

#[test]
fn a_placement_that_drifted_far_opens_straightened() {
    // Shrunk by 0.9236 across two of its axes: off by 0.147, determinant
    // 0.853, as an occurrence of a reported design that older builds had
    // dragged many times (mitcad#1666). It opens with the same warning, at
    // its nearest rotation: the rotation it was before it shrank.
    let (_, arm, file) = design();
    let drifted = with_rotation(&file, |_, c, v| if c < 2 { v * 0.9236 } else { v });
    let doc = Document::from_json(&drifted, MockKernel::default()).unwrap();
    assert_eq!(
        doc.load_warnings(),
        [
            "the placement of Arm:1 was distorted and has been straightened \
             to a rotation and a translation"
        ]
    );
    let straightened = transform(&doc, arm);
    assert!(orthonormality_error(&straightened.linear) <= 1e-12);
    assert_eq!(straightened.translation, placement().translation);
    let exact = placement();
    assert!(
        straightened
            .linear
            .iter()
            .flatten()
            .zip(exact.linear.iter().flatten())
            .all(|(a, b)| (a - b).abs() < 1e-12),
        "{straightened:?}"
    );
    // Saved, it opens again without a warning.
    let again = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    assert!(
        again.load_warnings().is_empty(),
        "{:?}",
        again.load_warnings()
    );
}

#[test]
fn a_scaled_or_mirrored_placement_still_refuses_the_file() {
    let (_, _, file) = design();
    let scaled = with_rotation(&file, |_, _, v| 2.0 * v);
    let mirrored = with_rotation(&file, |_, c, v| if c == 2 { -v } else { v });
    // Shrunk to a third: beyond any drift.
    let shrunk = with_rotation(&file, |_, c, v| if c < 2 { v / 3.0 } else { v });
    for text in [scaled, mirrored, shrunk] {
        let Err(error) = Document::from_json(&text, MockKernel::default()) else {
            panic!("opened {text}");
        };
        let error = error.to_string();
        assert!(
            error.contains(
                "occurrences[0]: an occurrence transform must be a rotation and a translation"
            ),
            "{error}"
        );
    }
}

#[test]
fn a_command_stores_a_rotation_and_a_translation() {
    let (mut doc, arm, _) = design();
    // Off by less than the commands' check allows (1e-6): stored
    // straightened, so that rounding cannot pile up.
    let mut given = placement();
    given.linear[0][1] += 4e-7;
    assert!(is_rigid(&given) && orthonormality_error(&given.linear) > 1e-9);
    doc.set_occurrence_transform(arm, given, false).unwrap();
    let stored = transform(&doc, arm);
    assert!(orthonormality_error(&stored.linear) <= 1e-12);
    assert_eq!(stored.translation, given.translation);
    // A scale is refused as before.
    let scaled = Transform::scale([0.0; 3], [2.0, 2.0, 2.0]);
    let error = doc
        .set_occurrence_transform(arm, scaled, false)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("an occurrence transform must be a rotation and a translation"),
        "{error}"
    );
    assert_eq!(transform(&doc, arm), stored);
}
