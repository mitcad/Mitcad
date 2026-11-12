// SPDX-License-Identifier: MIT
//! A cut with an automatic assembly scope, inserted as a copy of another
//! design (mitcad#1667): it found its targets among that design's bodies,
//! so in the design it is inserted into it cuts the copied components it
//! cut there and nothing of the new design's, wherever the copy is put.

use serde_json::{Value, json};

use mitcad_model::features::extrude_scope::{ComponentTarget, TargetScope};
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, Transform};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn box_body(doc: &mut Document<MockKernel>) -> BodyUid {
    let uid = doc
        .add_feature(
            &def(json!({"type": "box", "plane": "xy", "corner": [0, 0],
                        "length": 10, "width": 10, "height": 10, "operation": "new_body"})),
            None,
        )
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// A link design: Link (a box with a cut through it, automatic scope) and
/// Washer (a box 5 mm up, in the cut's way, so the cut cuts it too).
fn link_design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let (washer, _) = doc
        .create_component(
            Some("Washer"),
            Transform::translation([0.0, 0.0, 5.0]),
            true,
        )
        .unwrap();
    box_body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (link, o) = doc
        .create_component(Some("Link"), Transform::IDENTITY, true)
        .unwrap();
    box_body(&mut doc);
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [2.0, 2.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    let cut = doc
        .add_feature(
            &def(
                json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                "extent": {"type": "distance", "distance": 30}, "operation": "cut",
                "cut_scope": {"context": {"component": link, "occurrence": o.to_string()},
                              "targets": {"type": "automatic"}}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(doc.status(cut), Some(&FeatureStatus::Ok));
    // In its own design the cut goes through the link and the washer.
    for body in doc.bodies() {
        let component = doc.body_component(body.uid).unwrap();
        assert!(
            body.shape.history.starts_with("cut("),
            "{component:?}: {}",
            body.shape.history
        );
    }
    assert!(doc.assembly().component(washer).is_some());
    doc
}

#[test]
fn an_inserted_cut_cuts_only_the_copied_design() {
    let dir = mitcad_model::testing::unique_temp_dir("inserted-cut-scope");
    let path = dir.join("link.mitcad");
    std::fs::write(&path, link_design().to_json()).unwrap();

    // The design it goes into: a plate where the copy's cut will be.
    let mut doc = Document::new(MockKernel::default());
    let (plate, _) = doc
        .create_component(Some("Plate"), Transform::IDENTITY, true)
        .unwrap();
    let plate_body = box_body(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let inserted = command(
        &mut doc,
        json!({"cmd": "insert_component", "path": path.to_string_lossy(), "link": false}),
    );
    assert_eq!(inserted["error"], Value::Null, "{inserted}");

    // The copy's cut names its targets: the link (its context), then the
    // copied design's root (here a component of its own) and the washer,
    // whose bodies it could cut in its own design, at their paths here.
    let assembly = doc.assembly();
    let named = |name: &str| {
        assembly
            .components
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .uid
    };
    let (link, washer) = (named("Link"), named("Washer"));
    let root: ComponentUid = inserted["component"].as_str().unwrap().parse().unwrap();
    let path_of = |component: ComponentUid| {
        let mut paths = assembly.paths_to(component);
        assert_eq!(paths.len(), 1);
        paths.remove(0)
    };
    let cut = doc
        .features()
        .find(|f| matches!(&f.def, FeatureDef::Extrude(e) if e.cut_scope.is_some()))
        .unwrap();
    let FeatureDef::Extrude(extrude) = &cut.def else {
        unreachable!()
    };
    let scope = extrude.cut_scope.as_ref().unwrap();
    let target = |component: ComponentUid| ComponentTarget {
        component,
        occurrence: mitcad_model::features::OccurrencePath(path_of(component)),
    };
    assert_eq!(scope.context, target(link));
    assert_eq!(
        scope.targets,
        TargetScope::Explicit {
            included: vec![target(link), target(root), target(washer)],
            excluded_bodies: Vec::new(),
        }
    );
    let uid = cut.uid;
    assert_eq!(doc.status(uid), Some(&FeatureStatus::Ok));
    // The link and the washer are cut, the plate it lies in is not.
    for body in doc.bodies() {
        let component = doc.body_component(body.uid).unwrap();
        let cut = body.shape.history.starts_with("cut(");
        assert_eq!(cut, component != plate, "{component:?}");
    }
    assert!(doc.bodies().iter().any(|b| b.uid == plate_body));
}
