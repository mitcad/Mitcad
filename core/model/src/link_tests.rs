// SPDX-License-Identifier: MIT
//! Geometry of other components in sketches (mitcad#100) with the mock
//! kernel: a sketch on a face of a sibling component and a projection of
//! it follow both occurrences' placements; links are checked, refused
//! without them, and kept in the project file.

use serde_json::{Value, json};

use crate::Document;
use crate::document_tests::{extrude, num};
use crate::features::SketchPlane;
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Curve3;
use crate::testing::MockKernel;
use crate::transform::Transform;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string()).unwrap_err().to_string()
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

/// A 10 x 10 x 5 block in the active component: its extrude, body and
/// top face (z = 5 in the component).
fn block(doc: &mut Document<MockKernel>) -> (FeatureUid, BodyUid, String) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    (uid, BodyUid::new(uid, 0), format!("{uid}:end({region})"))
}

/// Root with A:1 (O1) at (100, 0, 10) holding a block and an empty B:1
/// (O2) at (0, 50, -3), side by side; B is active.
fn siblings() -> (Document<MockKernel>, BodyUid, String, ComponentUid) {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("A"), shift(100.0, 0.0, 10.0), true)
        .unwrap();
    let (_, body, face) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (b, _) = doc
        .create_component(Some("B"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    assert_eq!(doc.active_component(), b);
    (doc, body, face, b)
}

fn frame_origin(doc: &Document<MockKernel>, sketch: &Value) -> Vec<f64> {
    let s = query(doc, json!({"query": "sketch", "uid": sketch}));
    let status = doc.status(sketch.as_str().unwrap().parse().unwrap());
    assert!(
        s["frame"].is_object(),
        "the sketch has no frame ({status:?}): {s}"
    );
    s["frame"]["origin"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

fn near(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

#[test]
fn a_sketch_on_a_face_of_a_sibling_component_follows_both_placements() {
    let (mut doc, body, face, b) = siblings();
    // Without the occurrence the face is A's, which B's sketch cannot see.
    let unlinked = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body}}),
    );
    let status = doc
        .status(unlinked["uid"].as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    // Picked where A:1 shows it: linked, in B's coordinates. The face is
    // at z = 5 + 10 in the design, z = 18 in B (placed 3 lower).
    let created = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": "O1"}),
    );
    let sketch = created["uid"].clone();
    let uid: FeatureUid = sketch.as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    let s = query(&doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], json!({"source": "O1", "target": "O2"}));
    assert_eq!(s["frame"]["normal"], json!([0.0, 0.0, 1.0]));
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 18.0]));

    // A moves up 10: the sketch follows. B moves up 5: it is 5 lower in B.
    doc.set_occurrence_transform(OccurrenceUid(1), shift(100.0, 0.0, 20.0), false)
        .unwrap();
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 28.0]));
    doc.set_occurrence_transform(OccurrenceUid(2), shift(0.0, 50.0, 2.0), false)
        .unwrap();
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 23.0]));
    // Undo brings the earlier placement and frame back.
    doc.undo();
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 28.0]));

    // The block's height drives the face, and the sketch with it.
    let d = doc
        .parameters()
        .iter()
        .find(|p| p.comment().ends_with("distance"))
        .unwrap()
        .name()
        .to_owned();
    doc.set_parameter(&d, 8.0).unwrap();
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 31.0]));

    // The sketch cannot go before the block that makes its face.
    assert!(doc.move_feature(uid, 0).is_err());

    // The project file keeps the link.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();
    assert!(near(&frame_origin(&loaded, &sketch), &[0.0, 0.0, 31.0]));
}

#[test]
fn a_projection_of_a_sibling_components_face_follows_it() {
    let (mut doc, body, face, _) = siblings();
    let sketch = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"].clone();
    // The top face's edges in A's coordinates.
    let square: Vec<Curve3> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
        .iter()
        .zip([[10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]])
        .map(|(a, b)| Curve3::Line {
            start: [a[0], a[1], 5.0],
            end: [b[0], b[1], 5.0],
        })
        .collect();
    *doc.kernel().curves.borrow_mut() = square;
    // A's body is not B's: without the occurrence, the projection says so.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": sketch, "source": face, "body": body,
               "linked": true}),
    );
    assert!(error.contains("is a body of A, not of B"), "{error}");
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": sketch, "source": face, "body": body,
               "linked": true, "occurrence": "O1"}),
    );
    let corners = |doc: &Document<MockKernel>| -> Vec<[f64; 2]> {
        let s = query(doc, json!({"query": "sketch", "uid": sketch}));
        let mut at: Vec<[f64; 2]> = s["entities"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == "point")
            .map(|e| [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()])
            .collect();
        at.sort_by(|a, b| a.partial_cmp(b).unwrap());
        at
    };
    // A's (0, 0) is at (100, 0) in the design, (100, -50) in B.
    assert_eq!(corners(&doc).first(), Some(&[100.0, -50.0]));
    let s = query(&doc, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(
        s["projections"][0]["link"],
        json!({"source": "O1", "target": "O2"})
    );
    // A moves 10 along X: the linked projection follows.
    doc.set_occurrence_transform(OccurrenceUid(1), shift(110.0, 0.0, 10.0), false)
        .unwrap();
    assert_eq!(corners(&doc).first(), Some(&[110.0, -50.0]));
}

#[test]
fn a_construction_plane_of_another_component_needs_a_link() {
    let mut doc = Document::new(MockKernel::default());
    doc.create_component(Some("A"), shift(0.0, 0.0, 10.0), true)
        .unwrap();
    let plane = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 4}}}),
    );
    let plane = plane["uid"].clone();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let error = rejected(&mut doc, json!({"cmd": "sketch.create", "plane": plane}));
    assert!(error.contains("is in A"), "{error}");
    // Linked, in the root: 4 above A's origin, which is 10 up.
    let sketch = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": plane, "occurrence": "O1"}),
    )["uid"]
        .clone();
    assert!(near(&frame_origin(&doc, &sketch), &[0.0, 0.0, 14.0]));
    // An occurrence path that does not lead anywhere is refused.
    let error = rejected(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": plane, "occurrence": "O7"}),
    );
    assert!(error.contains("O7"), "{error}");
}

#[test]
fn a_combine_takes_tools_of_another_component_by_their_link() {
    // A:1 (O1) at (100, 0, 10) holds a block; B:1 (O2) at (0, 50, -3)
    // holds one of its own, cut by A's (the file's assembly context).
    let (mut doc, tool, _, b) = siblings();
    let (_, target, _) = block(&mut doc);
    let combine = |link: Value, keep: bool| {
        json!({"cmd": "add_feature", "def": {"type": "combine", "target": target,
               "tools": [tool], "operation": "cut", "keep_tools": keep,
               "tool_links": link}})
    };
    // Without the link the tool is A's, which B's combine cannot see.
    let unlinked = command(&mut doc, combine(json!({}), true))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    // Linked: the tool is moved from A into B, by (100, -50, 13), and kept.
    let link = json!({tool.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, combine(link.clone(), true))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(
        cut.starts_with("cut(") && cut.contains(",100,-50,13)"),
        "{cut}"
    );
    assert_eq!(doc.body_component(tool), Some(ComponentUid(1)));
    // The link is kept in the definition (B's path: its first one).
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["tool_links"], link, "{def}");

    // A moves up 10: the cut follows.
    doc.set_occurrence_transform(OccurrenceUid(1), shift(100.0, 0.0, 20.0), false)
        .unwrap();
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(cut.contains(",100,-50,23)"), "{cut}");

    // Consumed: the tool leaves A.
    let consumed = command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": {"type": "combine",
               "target": target, "tools": [tool], "operation": "cut", "tool_links": link}}),
    );
    assert!(consumed.is_object(), "{consumed}");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.body_component(tool), None);
    assert!(doc.body_shape(tool).is_none());

    // A link for a body that is not a tool is refused.
    let error = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "combine", "target": target,
               "tools": [tool], "operation": "join",
               "tool_links": {target.to_string(): {"source": "O1"}}}}),
    );
    assert!(error.contains("linked but not a tool"), "{error}");
}

struct LinkedAssembly {
    doc: Document<MockKernel>,
    sub: ComponentUid,
    occurrence: OccurrenceUid,
    tool: BodyUid,
    target: BodyUid,
    sketch: FeatureUid,
}

/// A subassembly with a combine, a sketch on its sibling's face and a
/// linked projection. Both paths include the subassembly occurrence.
fn linked_assembly() -> LinkedAssembly {
    let mut doc = Document::new(MockKernel::default());
    let (sub, occurrence) = doc
        .create_component(Some("Sub"), shift(0.0, 0.0, 30.0), true)
        .unwrap();
    let (_, a) = doc
        .create_component(Some("A"), shift(100.0, 0.0, 10.0), true)
        .unwrap();
    let (_, tool, face) = block(&mut doc);
    doc.activate_component(sub).unwrap();
    let (_, b) = doc
        .create_component(Some("B"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    let (_, target, _) = block(&mut doc);
    let source = crate::features::OccurrencePath(vec![occurrence, a]);
    let target_path = crate::features::OccurrencePath(vec![occurrence, b]);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {
        "type": "combine", "target": target, "tools": [tool],
        "operation": "cut", "keep_tools": true,
        "tool_links": {tool.to_string(): {"source": source, "target": target_path}}}}),
    );
    let sketch: FeatureUid = command(
        &mut doc,
        json!({"cmd": "sketch.create",
        "plane": {"face": face, "body": tool}, "occurrence": source}),
    )["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    *doc.kernel().curves.borrow_mut() = vec![Curve3::Line {
        start: [0.0, 0.0, 5.0],
        end: [10.0, 0.0, 5.0],
    }];
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": sketch,
        "source": face, "body": tool, "linked": true, "occurrence": source}),
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    assert!(doc.features().all(|f| doc.status(f.uid).unwrap().is_ok()));
    LinkedAssembly {
        doc,
        sub,
        occurrence,
        tool,
        target,
        sketch,
    }
}

/// Checks the body keys, geometry names and both full occurrence paths.
fn copied_links(
    doc: &Document<MockKernel>,
    original_tool: BodyUid,
    prefix: &[OccurrenceUid],
) -> (OccurrenceUid, OccurrenceUid, FeatureUid, BodyUid) {
    let combine = doc
        .features()
        .find_map(|f| match &f.def {
            crate::features::FeatureDef::Combine(c) if c.tools[0] != original_tool => Some((f, c)),
            _ => None,
        })
        .unwrap();
    let (entry, combine) = combine;
    let tool = combine.tools[0];
    let link = &combine.tool_links[&tool];
    assert_eq!(combine.tool_links.len(), 1);
    assert_ne!(tool, original_tool);
    assert!(link.source.0.starts_with(prefix));
    assert!(link.target.0.starts_with(prefix));
    assert_eq!(
        crate::joints::path_component(doc.assembly(), ComponentUid::ROOT, &link.source.0),
        Ok(doc.body_component(tool).unwrap())
    );
    assert_eq!(
        crate::joints::path_component(doc.assembly(), ComponentUid::ROOT, &link.target.0),
        Ok(entry.component)
    );
    let sketch = doc
        .features()
        .find_map(|f| match &f.def {
            crate::features::FeatureDef::Sketch(s)
                if f.component == entry.component && s.plane_link.is_some() =>
            {
                Some((f.uid, s))
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(sketch.1.plane_link.as_ref(), Some(link));
    assert_eq!(sketch.1.projections.len(), 1);
    let projection = &sketch.1.projections[0];
    assert_eq!(projection.body, Some(tool));
    assert_eq!(projection.link.as_ref(), Some(link));
    let crate::features::SketchPlane::Face { face, body } = &sketch.1.plane else {
        panic!("copied sketch lost its face plane");
    };
    assert_eq!(*body, Some(tool));
    assert_eq!(face.feature, tool.feature);
    assert_eq!(
        projection.source,
        crate::topo::TopoName::Face((**face).clone())
    );
    assert!(doc.features().all(|f| doc.status(f.uid).unwrap().is_ok()));
    (
        *link.source.0.last().unwrap(),
        *link.target.0.last().unwrap(),
        sketch.0,
        combine.target,
    )
}

fn linked_copy_round_trip(doc: &Document<MockKernel>) {
    let kernel = MockKernel::default();
    *kernel.curves.borrow_mut() = doc.kernel().curves.borrow().clone();
    let text = doc.to_json();
    let mut reopened = Document::from_json(&text, kernel).unwrap();
    reopened.recompute();
    assert!(
        reopened
            .features()
            .all(|f| reopened.status(f.uid).unwrap().is_ok())
    );
    assert_eq!(reopened.to_json(), text);
}

#[test]
fn paste_new_remaps_subassembly_links_before_validation() {
    let LinkedAssembly {
        mut doc,
        sub,
        occurrence,
        tool,
        target,
        sketch,
    } = linked_assembly();
    // The copied subassembly is nested under another placed component,
    // so the new prefix includes both the container and the copy.
    let (container, container_occurrence) = doc
        .create_component(Some("Container"), shift(0.0, 0.0, 200.0), true)
        .unwrap();
    let (copy, copy_occurrence) = doc.paste_new(occurrence, None).unwrap();
    assert_ne!(copy, sub);
    assert_eq!(
        doc.assembly().occurrence(copy_occurrence).unwrap().parent,
        container
    );
    let (a, b, copied_sketch, copied_target) =
        copied_links(&doc, tool, &[container_occurrence, copy_occurrence]);
    linked_copy_round_trip(&doc);
    let original_cut = doc.body_shape(target).unwrap().history.clone();
    let original_frame = frame_origin(&doc, &json!(sketch));
    let copied_frame = frame_origin(&doc, &json!(copied_sketch));
    doc.set_occurrence_transform(a, shift(100.0, 0.0, 20.0), false)
        .unwrap();
    let moved = frame_origin(&doc, &json!(copied_sketch));
    assert!((moved[2] - copied_frame[2] - 10.0).abs() < 1e-9);
    assert_eq!(frame_origin(&doc, &json!(sketch)), original_frame);
    assert_eq!(doc.body_shape(target).unwrap().history, original_cut);
    assert!(
        doc.body_shape(copied_target)
            .unwrap()
            .history
            .contains(",100,-50,23)")
    );
    doc.set_occurrence_transform(b, shift(0.0, 50.0, 2.0), false)
        .unwrap();
    let moved_again = frame_origin(&doc, &json!(copied_sketch));
    assert!((moved_again[2] - moved[2] + 5.0).abs() < 1e-9);
    assert_eq!(frame_origin(&doc, &json!(sketch)), original_frame);
    linked_copy_round_trip(&doc);
    doc.undo().unwrap();
    doc.undo().unwrap();
    doc.undo().unwrap();
    assert!(doc.assembly().component(copy).is_none());
    doc.redo().unwrap();
    copied_links(&doc, tool, &[container_occurrence, copy_occurrence]);
    linked_copy_round_trip(&doc);
}

#[test]
fn inserting_a_design_maps_links_under_its_new_root_occurrence() {
    let fixture = linked_assembly();
    let dir = std::env::temp_dir().join(format!("mitcad-linked-copy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("linked.mitcad"), fixture.doc.to_json()).unwrap();
    let mut doc = Document::new(MockKernel::default());
    *doc.kernel().curves.borrow_mut() = fixture.doc.kernel().curves.borrow().clone();
    // Offset the destination ids so an unremapped source cannot accidentally
    // resolve to a component or feature with the same numeric uid.
    block(&mut doc);
    let (_, container_occurrence) = doc
        .create_component(Some("Container"), shift(0.0, 0.0, 100.0), true)
        .unwrap();
    let (_, occurrence) = doc
        .insert_component(
            "linked.mitcad",
            &crate::InsertOptions {
                link: false,
                base: Some(dir.clone()),
                ..crate::InsertOptions::default()
            },
        )
        .unwrap();
    copied_links(&doc, fixture.tool, &[container_occurrence, occurrence]);
    linked_copy_round_trip(&doc);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn add_component_copy_maps_linked_geometry_and_root_prefix() {
    let fixture = linked_assembly();
    let mut doc = Document::new(MockKernel::default());
    *doc.kernel().curves.borrow_mut() = fixture.doc.kernel().curves.borrow().clone();
    block(&mut doc);
    let (container, container_occurrence) = doc
        .create_component(Some("Container"), shift(0.0, 0.0, 100.0), false)
        .unwrap();
    let (_, occurrence) = doc
        .add_component_copy(&fixture.doc, Some("Copied"), container, Transform::IDENTITY)
        .unwrap();
    copied_links(&doc, fixture.tool, &[container_occurrence, occurrence]);
    linked_copy_round_trip(&doc);
}

#[test]
fn paste_new_preserves_links_to_external_geometry() {
    let (mut doc, tool, face, _) = siblings();
    let source_occurrence = OccurrenceUid(1);
    let target_occurrence = OccurrenceUid(2);
    let (_, target, _) = block(&mut doc);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {
        "type": "combine", "target": target, "tools": [tool], "operation": "cut",
        "keep_tools": true, "tool_links": {tool.to_string(): {"source": source_occurrence}}}}),
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": tool},
                            "occurrence": source_occurrence}),
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, occurrence) = doc.paste_new(target_occurrence, None).unwrap();
    for feature in doc.features().filter(|f| f.component == copy) {
        match &feature.def {
            crate::features::FeatureDef::Combine(c) => {
                assert_eq!(c.tools, [tool]);
                assert_eq!(c.tool_links[&tool].source.0, [source_occurrence]);
                assert_eq!(c.tool_links[&tool].target.0, [occurrence]);
            }
            crate::features::FeatureDef::Sketch(s) if s.plane_link.is_some() => {
                let link = s.plane_link.as_ref().unwrap();
                assert_eq!(link.source.0, [source_occurrence]);
                assert_eq!(link.target.0, [occurrence]);
            }
            _ => {}
        }
        assert!(doc.status(feature.uid).unwrap().is_ok());
    }
    linked_copy_round_trip(&doc);
}

#[test]
fn links_picked_in_another_instance_of_the_subassembly_stay_external() {
    let LinkedAssembly {
        mut doc,
        sub,
        occurrence,
        tool,
        target,
        sketch,
    } = linked_assembly();
    let other = doc
        .copy_occurrence(occurrence, Some(shift(0.0, 0.0, 100.0)))
        .unwrap();
    let a = doc
        .assembly()
        .children(sub)
        .find(|o| Some(o.component) == doc.body_component(tool))
        .unwrap()
        .uid;
    let source = crate::features::OccurrencePath(vec![other, a]);
    let combine = doc
        .features()
        .find(|f| f.def.type_name() == "combine")
        .unwrap()
        .uid;
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": combine, "def": {
        "type": "combine", "target": target, "tools": [tool], "operation": "cut",
        "keep_tools": true, "tool_links": {tool.to_string(): {"source": source}}}}),
    );
    let plane = match &doc.feature(sketch).unwrap().def {
        crate::features::FeatureDef::Sketch(s) => s.plane.clone(),
        _ => unreachable!(),
    };
    let b = doc.feature(sketch).unwrap().component;
    doc.activate_component(b).unwrap();
    let external_sketch = command(
        &mut doc,
        json!({"cmd": "sketch.create",
        "plane": plane, "occurrence": source}),
    )["uid"]
        .clone();
    let crate::features::SketchPlane::Face { face, .. } = plane else {
        panic!("fixture has a face plane");
    };
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": external_sketch,
        "source": face, "body": tool, "linked": true, "occurrence": source}),
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, copied_occurrence) = doc.paste_new(occurrence, None).unwrap();
    let copied_b = doc
        .assembly()
        .children(copy)
        .map(|o| o.component)
        .find(|c| {
            doc.features()
                .any(|f| f.component == *c && f.def.type_name() == "combine")
        })
        .unwrap();
    let mut external_sketches = 0;
    for feature in doc.features().filter(|f| f.component == copied_b) {
        match &feature.def {
            crate::features::FeatureDef::Combine(c) => {
                assert_eq!(c.tools, [tool]);
                assert_eq!(c.tool_links[&tool].source, source);
                assert_eq!(c.tool_links[&tool].target.0[0], copied_occurrence);
            }
            crate::features::FeatureDef::Sketch(s)
                if s.plane_link.as_ref().is_some_and(|l| l.source == source) =>
            {
                external_sketches += 1;
                let crate::features::SketchPlane::Face { body, face } = &s.plane else {
                    panic!("external plane remains a face");
                };
                assert_eq!(*body, Some(tool));
                assert_eq!(face.feature, tool.feature);
                assert_eq!(s.projections[0].body, Some(tool));
                assert_eq!(
                    s.projections[0].source,
                    crate::topo::TopoName::Face((**face).clone())
                );
                assert_eq!(s.projections[0].link.as_ref().unwrap().source, source);
            }
            _ => {}
        }
        assert!(doc.status(feature.uid).unwrap().is_ok());
    }
    assert_eq!(external_sketches, 1);
    linked_copy_round_trip(&doc);
}
