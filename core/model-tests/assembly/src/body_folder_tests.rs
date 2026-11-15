// SPDX-License-Identifier: MIT
//! Folders of the Bodies folder (mitcad#1802) with the mock kernel: the
//! command and its undo step, what the `bodies` query lists, the design
//! file, and an import's names and folders set at once.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::BodyUid;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

/// A 10 x 10 x 5 block (Sketch and Extrude) at `x`.
fn block(doc: &mut Document<MockKernel>, x: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [x, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// A body's entry in the `bodies` query.
fn body_entry(doc: &Document<MockKernel>, uid: BodyUid) -> Value {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == json!(uid))
        .cloned()
        .unwrap()
}

#[test]
fn a_body_folder_is_set_listed_saved_and_undone() {
    let mut doc = Document::new(MockKernel::default());
    let pane = block(&mut doc, 0.0);
    let frame = block(&mut doc, 20.0);
    let pane_name = doc.body_name(pane);
    assert!(body_entry(&doc, pane).get("folder").is_none());

    command(
        &mut doc,
        json!({"cmd": "set_body_folder", "uid": pane, "folder": "Glass"}),
    );
    assert_eq!(body_entry(&doc, pane)["folder"], "Glass");
    assert!(body_entry(&doc, frame).get("folder").is_none());
    // The same again is no undo step.
    command(
        &mut doc,
        json!({"cmd": "set_body_folder", "uid": pane, "folder": "Glass"}),
    );

    // Saved with the body and read back.
    let json = doc.to_json();
    let file: Value = serde_json::from_str(&json).unwrap();
    let saved = file["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == json!(pane))
        .cloned()
        .unwrap();
    assert_eq!(saved["folder"], "Glass");
    let loaded = Document::from_json(&json, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), json);
    assert_eq!(
        loaded.body_attributes(pane).folder.as_deref(),
        Some("Glass")
    );

    // Out of it again, then the undo steps.
    command(&mut doc, json!({"cmd": "set_body_folder", "uid": pane}));
    assert!(body_entry(&doc, pane).get("folder").is_none());
    assert_eq!(
        doc.undo().as_deref(),
        Some(format!("Take {pane_name} out of its Folder").as_str())
    );
    assert_eq!(
        doc.undo().as_deref(),
        Some(format!("Move {pane_name} to Glass").as_str())
    );
    assert!(doc.body_attributes(pane).folder.is_none());

    // Refused: an empty name, a body that is not there.
    for refused in [
        json!({"cmd": "set_body_folder", "uid": pane, "folder": "  "}),
        json!({"cmd": "set_body_folder", "uid": "F9.b0", "folder": "Glass"}),
    ] {
        assert!(doc.command(&refused.to_string()).is_err(), "{refused}");
    }
}

#[test]
fn an_import_names_bodies_and_sets_their_folders_at_once() {
    let mut doc = Document::new(MockKernel::default());
    let a = block(&mut doc, 0.0);
    let b = block(&mut doc, 20.0);
    let c = block(&mut doc, 40.0);
    // C keeps its name; A and B are both named Window (B gets " (2)"), and
    // A goes into Windows.
    let c_name = doc.body_name(c);
    let given = doc
        .name_bodies(&[
            (a, Some("Window".to_owned()), Some("Windows".to_owned())),
            (b, Some("Window".to_owned()), None),
            (c, None, Some("Windows".to_owned())),
        ])
        .unwrap();
    assert_eq!(
        given,
        vec![(a, "Window".to_owned()), (b, "Window (2)".to_owned())]
    );
    assert_eq!(doc.body_name(a), "Window");
    assert_eq!(doc.body_name(b), "Window (2)");
    assert_eq!(doc.body_name(c), c_name);
    assert_eq!(doc.body_attributes(a).folder.as_deref(), Some("Windows"));
    assert!(doc.body_attributes(b).folder.is_none());
    assert_eq!(doc.body_attributes(c).folder.as_deref(), Some("Windows"));
    // One undo step for all of them.
    assert_eq!(doc.undo().as_deref(), Some("Name Bodies"));
    assert_ne!(doc.body_name(a), "Window");
    assert!(doc.body_attributes(c).folder.is_none());

    // A name another body has gets a number; a body that is not there is
    // left out.
    let taken = doc.body_name(c);
    let given = doc
        .name_bodies(&[
            (a, Some(taken.clone()), None),
            (
                BodyUid::new(mitcad_model::ids::FeatureUid(99), 0),
                Some("Gone".to_owned()),
                None,
            ),
        ])
        .unwrap();
    assert_eq!(given, vec![(a, format!("{taken} (2)"))]);
}

#[test]
fn bodies_of_different_components_may_share_a_name() {
    let mut doc = Document::new(MockKernel::default());
    let root = block(&mut doc, 0.0);
    doc.create_component(
        Some("Porch"),
        mitcad_model::transform::Transform::IDENTITY,
        true,
    )
    .unwrap();
    let porch = block(&mut doc, 20.0);
    let other = block(&mut doc, 40.0);
    doc.activate_component(mitcad_model::ids::ComponentUid::ROOT)
        .unwrap();
    doc.rename_body(root, "Column").unwrap();
    // Another component's body takes the same name (as an imported
    // design's do); within that component it stays unique.
    doc.rename_body(porch, "Column").unwrap();
    assert_eq!(doc.body_name(porch), "Column");
    let error = doc.rename_body(other, "Column").unwrap_err();
    assert!(error.to_string().contains("already named"), "{error}");
    // An import's names are unique per component the same way.
    let given = doc
        .name_bodies(&[(other, Some("Column".to_owned()), None)])
        .unwrap();
    assert_eq!(given, vec![(other, "Column (2)".to_owned())]);
}
