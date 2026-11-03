// SPDX-License-Identifier: MIT
//! Components and occurrences: which component owns an item, placements, and cut and paste into components.

use super::*;
use mitcad_model_api::DocumentApi;

/// History data without states: which components items changed.
#[derive(Default)]
struct ComponentHistory {
    items: HashMap<i64, Vec<u64>>,
}

impl StoredGeometry<MockShape> for ComponentHistory {
    fn state_count(&mut self) -> usize {
        0
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        Err(format!("there is no history state {index}"))
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        Ok(Vec::new())
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.items.get(&index).cloned()
    }
}

#[test]
fn components_and_occurrences_come_in_with_their_items() {
    for named in [true, false] {
        let mut doc = Document::new(MockKernel::default());
        // An external dump names the extrude's component; the decoder leaves it
        // to the history, which says the extrude changed Plate's bodies.
        let mut geometry = ComponentHistory::default();
        if !named {
            geometry.items.insert(1, vec![10]);
        }
        let report = import_design(
            &mut doc,
            &assembly_dump(named),
            &mut geometry,
            &Options::default(),
        );
        let c = &report.components;
        assert_eq!(
            (c.components, c.occurrences, c.external),
            (2, 3, 1),
            "{}",
            report.text()
        );
        let a = doc.assembly();
        assert_eq!(a.root_name, "Assembly");
        let names: Vec<String> = a
            .occurrences
            .iter()
            .map(|o| a.occurrence_name(o.uid))
            .collect();
        // The occurrence of a component of another document: an empty
        // component stands for it (mitcad#75).
        assert_eq!(
            names,
            ["Plate:1", "Pin:1", "Plate:2", "Inserted component:1"]
        );
        let inserted = a.occurrences[3].component;
        assert!(doc.component_bodies(inserted).is_empty());
        assert!(a.occurrences[0].grounded && !a.occurrences[2].visible);
        assert_eq!(a.occurrences[1].transform.translation, [10.0, 0.0, 0.0]);
        assert_eq!(a.occurrences[2].transform.translation, [100.0, 0.0, 0.0]);
        // The sketch follows the extrude that uses it into Plate.
        let plate = a.components[0].uid;
        assert!(doc.features().all(|f| f.component == plate));
        assert_eq!(report.items[0].component.as_deref(), Some("Plate"));
        assert_eq!(c.items.get("Plate"), Some(&2));
        // The plate's body in each occurrence.
        doc.set_occurrence_visible(a.occurrences[2].uid, true)
            .unwrap();
        let placed: Vec<f64> = doc
            .instances()
            .iter()
            .map(|i| i.transform.translation[0])
            .collect();
        assert_eq!(placed, [0.0, 100.0]);
        assert!(
            report
                .text()
                .contains("components: 2 made, 3 occurrences placed, 1 of other")
        );
    }
}

/// The block's sketch and extrusion owned by the given components
/// (`_f3d.component`, the decoder's), in a design with one occurrence of
/// Plate (object 10) placed 5 cm along X.
fn owned_dump(sketch_owner: u64, extrude_owner: u64) -> Dump {
    let mut value: Value = serde_json::to_value(block_dump()).unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Plate", "_f3d": {"object_id": 10}}]);
    value["occurrences"] = json!([
        {"component": "Plate", "_f3d": {"component_object": 10},
         "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                       [0.0, 0.0, 0.0, 1.0]]}]);
    value["timeline"]["items"][0]["_f3d"] = json!({"component": sketch_owner});
    value["timeline"]["items"][1]["_f3d"]["component"] = json!(extrude_owner);
    Dump::from_json(&value.to_string()).unwrap()
}

#[test]
fn items_go_into_the_components_that_own_them() {
    // Without a history the decoder's owners place the items: both in
    // Plate (before mitcad#37 they went into the root).
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &owned_dump(10, 10),
        &mut NoGeometry,
        &Options::default(),
    );
    let plate = doc.assembly().components[0].uid;
    assert!(
        doc.features().all(|f| f.component == plate),
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    assert_eq!(report.components.items.get("Plate"), Some(&2));

    // A sketch of the root used by Plate's extrusion: its geometry is in
    // the root's coordinates, so it stays there, and the extrusion uses a
    // copy moved into Plate's (5 cm back along X).
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &owned_dump(3, 10),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        doc.feature(FeatureUid(1)).unwrap().component,
        ComponentUid::ROOT
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let extrude: FeatureUid = report.items[1]
        .features
        .iter()
        .filter_map(|f| f.parse().ok())
        .find(|f| {
            matches!(
                doc.feature(*f).map(|e| &e.def),
                Some(FeatureDef::Extrude(_))
            )
        })
        .expect("an extrusion");
    assert_eq!(doc.feature(extrude).unwrap().component, plate);
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{extrude}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let copy: FeatureUid = def["def"]["profiles"][0]["sketch"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(copy, FeatureUid(1));
    assert_eq!(doc.feature(copy).unwrap().component, plate);
    let origin = doc.sketch_output(copy).unwrap().frame.origin;
    assert!((origin[0] + 50.0).abs() < 1e-9, "{origin:?}");
}

/// [`owned_dump`] with Plate's occurrence stored `stored` cm along X and
/// a captured position at timeline index `captured` putting it 20 cm
/// along X; the extrusion is at index 1, or 2 after a captured position
/// at 1.
fn captured_dump(sketch_owner: u64, stored: f64, captured: i64) -> Dump {
    let mut value: Value = serde_json::to_value(owned_dump(sketch_owner, 10)).unwrap();
    value["occurrences"][0]["_f3d"]["object_id"] = json!(100);
    value["occurrences"][0]["transform"][0][3] = json!(stored);
    let position = json!({"index": captured, "name": "Position1", "objectType": "Snapshot",
        "_f3d": {"component": 3},
        "detail": {"positions": [{
            "occurrence": {"kind": "occurrence", "_f3d": {"path": [100], "context_component": 3}},
            "transform": [[1.0, 0.0, 0.0, 20.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                          [0.0, 0.0, 0.0, 1.0]]}]}});
    let items = value["timeline"]["items"].as_array_mut().unwrap();
    if captured == 1 {
        items[1]["index"] = json!(2);
        items.insert(1, position);
        for p in value["parameters"]["model"].as_array_mut().unwrap() {
            if p["createdBy"]["timeline_index"] == json!(1) {
                p["createdBy"]["timeline_index"] = json!(2);
            }
        }
    } else {
        items.push(position);
    }
    Dump::from_json(&value.to_string()).unwrap()
}

/// The X of the origin of the sketch Plate's extrusion uses, and the
/// component it is in.
fn extruded_sketch(doc: &Document<MockKernel>, report: &DesignReport) -> (f64, ComponentUid) {
    let extrude = report
        .items
        .iter()
        .flat_map(|i| &i.features)
        .filter_map(|f| f.parse::<FeatureUid>().ok())
        .find(|f| {
            matches!(
                doc.feature(*f).map(|e| &e.def),
                Some(FeatureDef::Extrude(_))
            )
        })
        .expect("an extrusion");
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{extrude}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let sketch: FeatureUid = def["def"]["profiles"][0]["sketch"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let origin = doc.sketch_output(sketch).unwrap().frame.origin;
    (origin[0], doc.feature(sketch).unwrap().component)
}

/// Items before a captured position were made with the occurrences where
/// they were before it (mitcad#86): a sketch of the root used by Plate's
/// extrusion is moved into Plate by the placements at the extrusion's
/// point of the timeline, although Plate starts at its last captured
/// position (mitcad#75).
#[test]
fn sketches_move_by_the_placements_at_their_users() {
    let run = |dump: &Dump| {
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, dump, &mut NoGeometry, &Options::default());
        assert_eq!(report.joints.positions, 1, "{}", report.text());
        let extrusion = report
            .items
            .iter()
            .find(|i| i.object_type == "ExtrudeFeature")
            .unwrap();
        assert_eq!(extrusion.outcome, Outcome::Parametric, "{}", report.text());
        // Plate starts at its captured position in every case.
        let plate = &doc.assembly().occurrences[0];
        assert_eq!(plate.transform.translation[0], 200.0);
        let (x, component) = extruded_sketch(&doc, &report);
        (x, component == plate.component)
    };
    // Captured after the extrusion: Plate was at its stored 5 cm.
    assert_eq!(run(&captured_dump(3, 5.0, 2)), (-50.0, true));
    // Captured before it: at the captured 20 cm.
    assert_eq!(run(&captured_dump(3, 5.0, 1)), (-200.0, true));
    // Plate stored where the root is: the sketch follows the extrusion into
    // Plate as it is, unless a captured position before the extrusion had
    // moved Plate away (a copy moved by 20 cm then).
    assert_eq!(run(&captured_dump(3, 0.0, 2)), (0.0, true));
    assert_eq!(run(&captured_dump(3, 0.0, 1)), (-200.0, true));
}

/// History states whose bodies belong to components: (shape, component
/// object, body id) per body.
#[derive(Default)]
struct OwnedStates {
    states: Vec<Vec<(MockShape, u64, u64)>>,
    items: HashMap<i64, usize>,
    components: HashMap<i64, Vec<u64>>,
}

impl StoredGeometry<MockShape> for OwnedStates {
    fn state_count(&mut self) -> usize {
        self.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.items.get(&index).copied()
    }

    fn state(&mut self, index: usize) -> Result<Vec<StoredBody<MockShape>>, String> {
        Ok(self.states[index]
            .iter()
            .map(|(shape, component, id)| StoredBody {
                shape: shape.clone(),
                name: None,
                source: format!("state {index} body {id}"),
                component: Some(*component),
                id: Some(*id),
                lineage: None,
            })
            .collect())
    }

    fn final_bodies(&mut self) -> Result<Vec<StoredBody<MockShape>>, String> {
        let last = self.states.len() - 1;
        self.state(last)
    }

    fn item_components(&mut self, index: i64) -> Option<Vec<u64>> {
        self.components.get(&index).cloned()
    }

    fn history_components(&mut self) -> Option<std::collections::HashSet<u64>> {
        Some(self.components.values().flatten().copied().collect())
    }
}

#[test]
fn a_component_made_from_bodies_takes_them_into_the_placed_component() {
    // mitcad#156: the root's block becomes the body of the component Base
    // (placed by the occurrence tree); Base's history starts with it, while
    // the root's keeps it as it was. The item moves the replay's block into
    // Base, and the root's copy leaves the history from its state on.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        ]),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference).remove(0);
    let mut sketch = rectangle_sketch(0);
    sketch["_f3d"] = json!({"component": 3});
    let mut made = extrude(1, 0, "NewBodyFeatureOperation", "d2");
    made["_f3d"]["component"] = json!(3);
    let moved = json!({"index": 2, "name": "Body->Comp1", "objectType": "ComponentFromBodies",
                       "_f3d": {"component": 3, "result_no": -1, "made_result_no": 7}});
    let mut value: Value = serde_json::to_value(timeline_dump(vec![sketch, made, moved])).unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Base", "_f3d": {"object_id": 10}}]);
    value["occurrences"] = json!([{"component": "Base", "_f3d": {"component_object": 10},
                                   "isReferencedComponent": false, "children": []}]);
    let dump = Dump::from_json(&value.to_string()).unwrap();
    let mut geometry = OwnedStates {
        states: vec![
            Vec::new(),
            vec![(block.clone(), 3, 1)],
            vec![(block.clone(), 3, 1), (block.clone(), 10, 2)],
        ],
        items: HashMap::from([(1, 1), (2, 2)]),
        components: HashMap::from([(1, vec![3]), (2, vec![10])]),
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &dump, &mut geometry, &Options::default());
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(item.note.as_deref(), Some("moves its body into Base"));
    let base = doc
        .assembly()
        .components
        .iter()
        .find(|c| c.name == "Base")
        .map(|c| c.uid)
        .unwrap();
    // One body, Base's, made by the root's extrusion; no other component.
    assert_eq!(doc.assembly().components.len(), 1, "{}", report.text());
    let bodies = doc.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(doc.body_component(bodies[0].uid), Some(base));
    assert_eq!(report.history.matched, 2, "{}", report.text());
    assert!(
        report.warnings.iter().all(|w| !w.contains("replaces")),
        "{}",
        report.text()
    );
}

#[test]
fn items_that_make_components_together_each_take_one() {
    // Three items make two components in one history state: the first item
    // moves a body into each of them (one feature per component), the
    // second item takes the second component's feature, and the state is
    // matched once.
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(vec![
            rectangle_sketch(0),
            extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        ]),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference).remove(0);
    let mut sketch = rectangle_sketch(0);
    sketch["_f3d"] = json!({"component": 3});
    let mut first = extrude(1, 0, "NewBodyFeatureOperation", "d2");
    first["_f3d"]["component"] = json!(3);
    let mut second = extrude(2, 0, "NewBodyFeatureOperation", "d4");
    second["_f3d"]["component"] = json!(3);
    let moved = |index: i64, name: &str| {
        json!({"index": index, "name": name, "objectType": "ComponentFromBodies",
               "_f3d": {"component": 3, "result_no": -1, "made_result_no": 7}})
    };
    let mut value: Value = serde_json::to_value(timeline_dump(vec![
        sketch,
        first,
        second,
        moved(3, "Body->Comp1"),
        moved(4, "Body->Comp2"),
    ]))
    .unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "One", "_f3d": {"object_id": 10}},
        {"name": "Two", "_f3d": {"object_id": 11}}]);
    value["occurrences"] = json!([
        {"component": "One", "_f3d": {"component_object": 10},
         "isReferencedComponent": false, "children": []},
        {"component": "Two", "_f3d": {"component_object": 11},
         "isReferencedComponent": false, "children": []}]);
    let dump = Dump::from_json(&value.to_string()).unwrap();
    let mut geometry = OwnedStates {
        states: vec![
            Vec::new(),
            vec![(block.clone(), 3, 1)],
            vec![(block.clone(), 3, 1), (block.clone(), 3, 2)],
            vec![
                (block.clone(), 3, 1),
                (block.clone(), 3, 2),
                (block.clone(), 10, 3),
                (block.clone(), 11, 4),
            ],
        ],
        items: HashMap::from([(1, 1), (2, 2), (3, 3), (4, 3)]),
        components: HashMap::from([
            (1, vec![3]),
            (2, vec![3]),
            (3, vec![10, 11]),
            (4, vec![10, 11]),
        ]),
    };
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(&mut doc, &dump, &mut geometry, &Options::default());
    let one = &report.items[3];
    let two = &report.items[4];
    assert_eq!(one.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(two.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(one.note.as_deref(), Some("moves its body into One"));
    assert_eq!(two.note.as_deref(), Some("moves its body into Two"));
    assert_eq!(one.features.len(), 1);
    assert_eq!(two.features.len(), 1);
    assert_ne!(one.features, two.features);
    let bodies = doc.bodies();
    assert_eq!(bodies.len(), 2, "{}", report.text());
    let mut in_components: Vec<String> = bodies
        .iter()
        .filter_map(|b| doc.body_component(b.uid))
        .map(|c| doc.assembly().name(c))
        .collect();
    in_components.sort();
    assert_eq!(in_components, ["One", "Two"], "{}", report.text());
}

#[test]
fn a_part_of_an_item_in_another_component_is_replayed_after_it() {
    // mitcad#169: the item changed no body of its own component (no result
    // of its own); its part in the other component is an item of its own
    // after the timeline's items, at the item's point of the timeline.
    let mut item = extrude(1, 0, "NewBodyFeatureOperation", "d2");
    item["_f3d"]["result_no"] = json!(-1);
    item["_f3d"]["flags"] = json!("000001");
    let mut part = extrude(2, 0, "NewBodyFeatureOperation", "d2");
    part["name"] = json!("Extrude1");
    part["_f3d"]["part_of"] = json!(1);
    let mut doc = Document::new(MockKernel::default());
    let report = import(
        &mut doc,
        &timeline_dump(vec![rectangle_sketch(0), item, part]),
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Skipped,
        "{}",
        report.text()
    );
    assert_eq!(
        report.items[1].note.as_deref(),
        Some("it changed no body of its own component; its change in another component is item 2")
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.starts_with("part of item 1: its change in this component")),
        "{}",
        report.text()
    );
    assert_eq!(doc.bodies().len(), 1);
}

/// A cut and paste of a body into a component that is not its own: the
/// body and the features that made it move into the existing component,
/// where the body keeps its shape and its place in the design.
#[test]
fn a_cut_and_paste_moves_the_bodies_into_an_existing_component() {
    let mut first = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    for item in &mut first {
        item["component"] = json!("Assembly");
    }
    let mut reference = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let shape = shapes(&reference)[0].clone();
    let points: Vec<[f64; 3]> = shape
        .edges
        .iter()
        .take(3)
        .map(|(a, b)| edge_point(a, b).map(|x| x / 10.0))
        .collect();
    let body = json!({"kind": "body", "objectType": "BRepBody",
                      "_f3d": {"recipe": "body", "entities": [[{"tag": "304", "kind": 7, "ops": []}]],
                               "edge_points": points}});
    let matrix = |x: f64| {
        json!([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0]
        ])
    };
    let mut items = first;
    items.push(
        json!({"index": 2, "name": "CutPasteBodies1", "objectType": "CopyPasteBodies",
                      "component": "Box", "detail": {"sourceBody": [body]},
                      "_f3d": {"base_name": "CutPasteBodies"}}),
    );
    let mut value = serde_json::to_value(timeline_dump(items)).unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Box", "_f3d": {"object_id": 10}}]);
    value["occurrences"] = json!([
        {"component": "Box", "transform": matrix(5.0), "_f3d": {"component_object": 10},
         "children": []}]);
    let dump = Dump::from_json(&value.to_string()).unwrap();
    let mut doc = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    // The body and the features that made it are the box's now, the
    // body's identity and the design's world placement kept.
    let a = doc.assembly();
    let boxed = a.components[0].uid;
    assert_eq!(a.component(boxed).map(|c| c.name.as_str()), Some("Box"));
    let body = doc.bodies()[0].uid;
    assert_eq!(doc.body_component(body), Some(boxed));
    assert!(doc.features().all(|f| f.component == boxed));
    assert!(doc.features().all(|f| doc.status(f.uid).unwrap().is_ok()));
    assert_eq!(doc.instances().len(), 1);
    assert_eq!(doc.instances()[0].transform.translation, [50.0, 0.0, 0.0]);
    // The note tells what moved; no sketch was copied for the item.
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("moves its body into Box")),
        "{}",
        report.text()
    );
    assert!(report.items[2].features.is_empty());
}

/// A cut and paste of a body whose features cannot go into the existing
/// component (a hole, between differently placed components): the body
/// alone moves there by a body transfer at the item's point of the
/// timeline (mitcad#465), keeping its place in the design; the features
/// that made it stay.
#[test]
fn a_cut_and_paste_whose_features_cannot_go_transfers_the_body() {
    let kernel = || TestKernel {
        edge_points: true,
        face_listing: true,
        rigid: true,
        ..TestKernel::default()
    };
    let mut first = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    first.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType", "holeTapType": "SimpleHoleTapType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "5 mm", 0.5, "mm")},
            "position": [2.0, 1.0, 1.0],
            "holePositionDefinition": {"planarEntity": {
                "kind": "face", "geometry": {"type": "Plane"},
                "point_on_face": face_point(&top).map(|x| x / 10.0)}}
        }
    }));
    for item in &mut first {
        item["component"] = json!("Assembly");
    }
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let shape = shapes(&reference)[0].clone();
    let points: Vec<[f64; 3]> = shape
        .edges
        .iter()
        .take(3)
        .map(|(a, b)| edge_point(a, b).map(|x| x / 10.0))
        .collect();
    let body = json!({"kind": "body", "objectType": "BRepBody",
                      "_f3d": {"recipe": "body", "entities": [[{"tag": "304", "kind": 7, "ops": []}]],
                               "edge_points": points}});
    let mut items = first;
    items.push(
        json!({"index": 3, "name": "CutPasteBodies1", "objectType": "CopyPasteBodies",
               "component": "Box", "detail": {"sourceBody": [body]},
               "_f3d": {"base_name": "CutPasteBodies"}}),
    );
    let mut value = serde_json::to_value(timeline_dump(items)).unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Box", "_f3d": {"object_id": 10}}]);
    value["occurrences"] = json!([
        {"component": "Box",
         "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0],
                       [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
         "_f3d": {"component_object": 10}, "children": []}]);
    let dump = Dump::from_json(&value.to_string()).unwrap();
    let mut doc = Document::new(kernel());
    let report = import_design(&mut doc, &dump, &mut NoGeometry, &Options::default());
    assert_eq!(
        report.items[3].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let note = report.items[3].note.clone().unwrap_or_default();
    assert!(
        note.contains("moves its body into Box by a body transfer"),
        "{}",
        report.text()
    );
    let a = doc.assembly();
    let boxed = a.components[0].uid;
    let body = doc.bodies()[0].uid;
    assert_eq!(doc.body_component(body), Some(boxed));
    // The features that made it stay in the root; the transfer is the box's.
    let transfer: Vec<_> = doc
        .features()
        .filter(|f| matches!(f.def, FeatureDef::BodyTransfer(_)))
        .collect();
    assert_eq!(transfer.len(), 1);
    assert_eq!(transfer[0].component, boxed);
    assert!(
        doc.features()
            .filter(|f| !matches!(f.def, FeatureDef::BodyTransfer(_)))
            .all(|f| f.component == ComponentUid::ROOT)
    );
    assert!(doc.features().all(|f| doc.status(f.uid).unwrap().is_ok()));
    // Where it was in the design: the box's placement and the body's
    // coordinates in it make up its place in the root.
    let instance = &doc.instances()[0];
    assert_eq!(instance.transform.translation, [50.0, 0.0, 0.0]);
    let local = doc
        .kernel()
        .bounding_box(doc.body_shape(body).unwrap())
        .unwrap();
    let root = reference
        .kernel()
        .bounding_box(reference.body_shape(reference.bodies()[0].uid).unwrap())
        .unwrap();
    if let (Some(local), Some(root)) = (local, root) {
        assert!(
            (local.min[0] + 50.0 - root.min[0]).abs() < 1e-9,
            "{local:?} {root:?}"
        );
    }
}

/// A cut pasted into the component it came from (mitcad#1175): where the
/// file's history keeps the item as the body and a copy of it, the item
/// is that copy in place, checked against its state, and no base feature
/// brings the copy for the items after it; where its state is the body as
/// it was, nothing is moved or added.
#[test]
fn a_cut_pasted_into_its_own_component_is_the_copy_its_state_keeps() {
    let kernel = || TestKernel {
        edge_points: true,
        ..TestKernel::default()
    };
    let first = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let block = shapes(&reference)[0].clone();
    // The block and its copy in place, as the file stores them.
    add(
        &mut reference,
        json!({"type": "move", "bodies": ["F2.b0"], "copy": true,
               "transform": {"type": "free", "matrix": [[1.0, 0.0, 0.0, 0.0],
                                                        [0.0, 1.0, 0.0, 0.0],
                                                        [0.0, 0.0, 1.0, 0.0]]}}),
    );
    let copied = shapes(&reference);
    assert_eq!(copied.len(), 2);
    let points: Vec<[f64; 3]> = block
        .edges
        .iter()
        .take(3)
        .map(|(a, b)| edge_point(a, b).map(|x| x / 10.0))
        .collect();
    let body = json!({"kind": "body", "objectType": "BRepBody",
                      "_f3d": {"recipe": "body", "entities": [[{"tag": "304", "kind": 7, "ops": []}]],
                               "edge_points": points}});
    let mut items = first;
    items.push(
        json!({"index": 2, "name": "CutPasteBodies1", "objectType": "CopyPasteBodies",
               "detail": {"sourceBody": [body]}, "_f3d": {"base_name": "CutPasteBodies"}}),
    );
    let replay = |pasted: Vec<MockShape>| {
        let mut geometry = TestGeometry {
            states: vec![Vec::new(), vec![block.clone()], pasted],
            items: HashMap::from([(1, 1), (2, 2)]),
            ..TestGeometry::default()
        };
        let mut doc = Document::new(kernel());
        let report = import_design(
            &mut doc,
            &timeline_dump(items.clone()),
            &mut geometry,
            &Options::default(),
        );
        (report, doc)
    };
    // The body and its copy.
    let (report, doc) = replay(copied);
    let text = report.text();
    let paste = &report.items[2];
    assert_eq!(paste.outcome, Outcome::Parametric, "{text}");
    assert_eq!(paste.verified, Some(true), "{text}");
    assert!(
        paste
            .note
            .as_deref()
            .is_some_and(|n| n.contains("adds its copy")),
        "{text}"
    );
    let uid: FeatureUid = paste.features[0].parse().unwrap();
    match &doc.feature(uid).unwrap().def {
        FeatureDef::Move(m) => assert!(m.copy, "{text}"),
        other => panic!("{other:?}\n{text}"),
    }
    assert_eq!(doc.bodies().len(), 2, "{text}");
    assert!(
        doc.features().all(|f| f.def.type_name() != "base"),
        "{text}"
    );
    assert!(report.warnings.is_empty(), "{text}");
    // The body as it was: nothing to move, nothing added.
    let (report, doc) = replay(vec![block.clone()]);
    let text = report.text();
    let paste = &report.items[2];
    assert_eq!(paste.outcome, Outcome::Parametric, "{text}");
    assert!(paste.features.is_empty(), "{text}");
    assert!(
        paste
            .note
            .as_deref()
            .is_some_and(|n| n.contains("in the component already")),
        "{text}"
    );
    assert_eq!(doc.bodies().len(), 1, "{text}");
    assert!(
        doc.features().all(|f| f.def.type_name() != "base"),
        "{text}"
    );
}

/// An occurrence inside a component placed several times (Pin in Plate,
/// Plate placed 10 cm apart twice): the file gives the occurrence's
/// placement after each item per path, in the root component. Its
/// placement in its parent is each step made relative to the parent along
/// the step's own path (Pin 1 cm along X in Plate), not a root placement
/// taken as one in the parent (11 cm, where the second path puts it).
#[test]
fn an_occurrence_in_a_component_placed_twice_keeps_its_place_in_it() {
    let mut value: Value = serde_json::to_value(assembly_dump(true)).unwrap();
    let matrix = |x: f64| {
        json!([
            [1.0, 0.0, 0.0, x],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0]
        ])
    };
    let step = |index: i64, x: f64| json!([{"index": index, "object_id": 900 + index, "transform": matrix(x)}]);
    for (k, (plate, x)) in [(11, 0.0), (12, 10.0)].into_iter().enumerate() {
        let index = 2 * k as i64;
        let occurrence = &mut value["occurrences"][k];
        occurrence["_f3d"]["object_id"] = json!(plate);
        occurrence["_f3d"]["placements"] = step(index, x);
        let pin = &mut occurrence["children"][0]["_f3d"];
        pin["object_id"] = json!(21);
        pin["placements"] = step(index + 1, x + 1.0);
    }
    let dump = Dump::from_json(&value.to_string()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &dump,
        &mut ComponentHistory::default(),
        &Options::default(),
    );
    let assembly = doc.assembly();
    let pin = assembly
        .occurrences
        .iter()
        .find(|o| assembly.occurrence_name(o.uid).starts_with("Pin"))
        .expect("the pin's occurrence");
    let x = pin.transform.translation[0];
    assert!((x - 10.0).abs() < 1e-9, "{x} mm: {}", report.text());
}

/// A construction plane offset from a face of another component's body,
/// linked to it (mitcad#389), goes into the item's component, where its
/// link's target leads: not into the face's component, whose coordinates
/// put it where the other component is placed instead.
#[test]
fn a_linked_plane_goes_into_the_items_component() {
    let mut doc = Document::new(TestKernel::default());
    let (a, _) = doc
        .create_component(
            Some("A"),
            mitcad_model::Transform::translation([0.0, 0.0, 10.0]),
            true,
        )
        .unwrap();
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_rectangle(
            sketch,
            [0.0, 0.0],
            &mitcad_model::ValueInput::Number(10.0),
            &mitcad_model::ValueInput::Number(10.0),
        )
        .unwrap()
        .region;
    let added: Value = serde_json::from_str(
        &doc.command(
            &json!({"cmd": "add_feature", "def": {"type": "extrude",
                    "profiles": [{"sketch": sketch.to_string(), "region": region.to_string()}],
                    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap();
    let body = format!("{}.b0", added["uid"].as_str().unwrap());
    doc.activate_component(mitcad_model::ComponentUid::ROOT)
        .unwrap();
    let (b, _) = doc
        .create_component(Some("B"), mitcad_model::Transform::IDENTITY, true)
        .unwrap();
    let face = format!("{}:end({region})", added["uid"].as_str().unwrap());
    let plane = json!({"type": "construction_plane",
                       "definition": {"type": "offset", "plane": {"body": body, "face": face},
                                      "distance": 5},
                       "links": {body.clone(): {"source": "O1", "target": "O2"}}});
    assert_eq!(def_component(&doc, &plane, b), b);
    // Without the link the face's body names its component.
    let mut unlinked = plane.clone();
    unlinked.as_object_mut().unwrap().remove("links");
    assert_eq!(def_component(&doc, &unlinked, b), a);
}
