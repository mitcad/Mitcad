// SPDX-License-Identifier: MIT
//! Geometry of other components in sketches (mitcad#100) with the mock
//! kernel: a sketch on a face of a sibling component and a projection of
//! it follow both occurrences' placements; links are checked, refused
//! without them, and kept in the design file.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::test_support::allow_attachments;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

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
    allow_attachments(&mut doc);
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

    // The design file keeps the link.
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

/// A sweep of B guided by a face of A's block (mitcad#161: a nut's thread
/// swept along the mating part's flank): the face's body is read in A and
/// moved by its link; without the link the sweep fails, and a link for a
/// body neither the path nor a guide surface is on is refused.
#[test]
fn a_sweep_takes_guide_surfaces_of_another_component_by_their_link() {
    let (mut doc, body, face, b) = siblings();
    let path = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": path, "start": [0, 0], "end": [40, 0]}),
    );
    let profile = doc.add_sketch(SketchPlane::Yz).unwrap().uid;
    let region = doc
        .add_rectangle(profile, [-1.0, -1.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    let sweep = |links: Value| {
        json!({"cmd": "add_feature", "def": {"type": "sweep",
               "profiles": [{"sketch": profile, "region": region}],
               "path": {"sketch": path, "curves": ["c1"]},
               "guide_surfaces": [{"body": body, "face": face}],
               "body_links": links, "operation": "new_body"}})
    };
    let unlinked = command(&mut doc, sweep(json!({})))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    let link = json!({body.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, sweep(link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    let spec = doc
        .kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(spec.ends_with(&format!("surfaces {face}")), "{spec}");
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["body_links"], link, "{def}");

    let error = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "sweep",
               "profiles": [{"sketch": profile, "region": region}],
               "path": {"sketch": path, "curves": ["c1"]},
               "body_links": {body.to_string(): {"source": "O1"}}, "operation": "new_body"}}),
    );
    assert!(
        error.contains("neither the path's nor a guide surface's"),
        "{error}"
    );
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
    allow_attachments(&mut doc);
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

#[test]
fn a_split_takes_a_tool_body_of_another_component_by_its_link() {
    // A:1 (O1) holds a block; B:1 (O2) holds one of its own, split by A's.
    let (mut doc, tool, face, b) = siblings();
    let (_, target, _) = block(&mut doc);
    let split = |tool_def: Value, link: Value| {
        json!({"cmd": "add_feature", "def": {"type": "split_body", "bodies": [target],
               "tool": tool_def, "tool_links": link}})
    };
    // Without the link the tool is A's, which B's split cannot see.
    let unlinked = command(&mut doc, split(json!({"body": tool}), json!({})))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    // Linked: the tool stays in A, the pieces are B's.
    let link = json!({tool.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, split(json!({"body": tool}), link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    assert_eq!(doc.body_component(tool), Some(ComponentUid(1)));
    assert_eq!(doc.body_component(target), Some(b));
    assert!(doc.body_shape(tool).is_some());
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["tool_links"], link, "{def}");
    doc.undo();

    // The body of a tool face is linked the same way.
    let uid: FeatureUid = command(
        &mut doc,
        split(json!({"body": tool, "face": face}), link.clone()),
    )["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    doc.undo();

    // A link for a body that is not a tool is refused.
    let error = rejected(
        &mut doc,
        split(
            json!({"body": tool}),
            json!({target.to_string(): {"source": "O1"}}),
        ),
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
    allow_attachments(&mut doc);
    let source = mitcad_model::features::OccurrencePath(vec![occurrence, a]);
    let target_path = mitcad_model::features::OccurrencePath(vec![occurrence, b]);
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
            mitcad_model::features::FeatureDef::Combine(c) if c.tools[0].body != original_tool => {
                Some((f, c))
            }
            _ => None,
        })
        .unwrap();
    let (entry, combine) = combine;
    let tool = combine.tools[0].body;
    let link = &combine.tool_links[&tool];
    assert_eq!(combine.tool_links.len(), 1);
    assert_ne!(tool, original_tool);
    assert!(link.source.0.starts_with(prefix));
    assert!(link.target.0.starts_with(prefix));
    assert_eq!(
        mitcad_model::internal::joints::path_component(
            doc.assembly(),
            ComponentUid::ROOT,
            &link.source.0
        ),
        Ok(doc.body_component(tool).unwrap())
    );
    assert_eq!(
        mitcad_model::internal::joints::path_component(
            doc.assembly(),
            ComponentUid::ROOT,
            &link.target.0
        ),
        Ok(entry.component)
    );
    let sketch = doc
        .features()
        .find_map(|f| match &f.def {
            mitcad_model::features::FeatureDef::Sketch(s)
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
    let mitcad_model::features::SketchPlane::Face { face, body } = &sketch.1.plane else {
        panic!("copied sketch lost its face plane");
    };
    assert_eq!(*body, Some(tool));
    assert_eq!(face.feature, tool.feature);
    assert_eq!(
        projection.source,
        mitcad_model::topo::TopoName::Face((**face).clone())
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
            &mitcad_model::InsertOptions {
                link: false,
                base: Some(dir.clone()),
                ..mitcad_model::InsertOptions::default()
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
            mitcad_model::features::FeatureDef::Combine(c) => {
                assert_eq!(c.tools, [mitcad_model::features::CombineTool::own(tool)]);
                assert_eq!(c.tool_links[&tool].source.0, [source_occurrence]);
                assert_eq!(c.tool_links[&tool].target.0, [occurrence]);
            }
            mitcad_model::features::FeatureDef::Sketch(s) if s.plane_link.is_some() => {
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
    let source = mitcad_model::features::OccurrencePath(vec![other, a]);
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
        mitcad_model::features::FeatureDef::Sketch(s) => s.plane.clone(),
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
    let mitcad_model::features::SketchPlane::Face { face, .. } = plane else {
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
            mitcad_model::features::FeatureDef::Combine(c) => {
                assert_eq!(c.tools, [mitcad_model::features::CombineTool::own(tool)]);
                assert_eq!(c.tool_links[&tool].source, source);
                assert_eq!(c.tool_links[&tool].target.0[0], copied_occurrence);
            }
            mitcad_model::features::FeatureDef::Sketch(s)
                if s.plane_link.as_ref().is_some_and(|l| l.source == source) =>
            {
                external_sketches += 1;
                let mitcad_model::features::SketchPlane::Face { body, face } = &s.plane else {
                    panic!("external plane remains a face");
                };
                assert_eq!(*body, Some(tool));
                assert_eq!(face.feature, tool.feature);
                assert_eq!(s.projections[0].body, Some(tool));
                assert_eq!(
                    s.projections[0].source,
                    mitcad_model::topo::TopoName::Face((**face).clone())
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

/// An extrusion of B up to A's body or a face of it (mitcad#104): the
/// object is read in its component and moved by its link; without the
/// link the extrusion fails, and a link for a body that is no object is
/// refused.
#[test]
fn an_extrusion_goes_up_to_objects_of_another_component_by_their_link() {
    let (mut doc, body, face, b) = siblings();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    let extrusion = |object: Value, links: Value| {
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "to_object", "object": object},
               "object_links": links, "operation": "new_body"}})
    };
    let to_body = json!({"type": "body", "body": body, "through": true});
    let unlinked = command(&mut doc, extrusion(to_body.clone(), json!({})))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    let link = json!({body.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, extrusion(to_body, link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    let spec = doc
        .kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(spec.contains("to body"), "{spec}");
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["object_links"], link, "{def}");

    // A face of the body, through the same kind of link.
    let to_face = json!({"type": "face", "body": body, "face": face});
    let uid: FeatureUid = command(&mut doc, extrusion(to_face, link))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));

    let error = rejected(
        &mut doc,
        extrusion(
            json!({"type": "plane", "plane": "xy"}),
            json!({body.to_string(): {"source": "O1"}}),
        ),
    );
    assert!(error.contains("linked but not an object's"), "{error}");
}

/// A revolution of B up to a face or the body of A's block (mitcad#762, as
/// an extrusion's): the object is read in its component and moved by its
/// link; without the link the revolution fails, and a link for a body that
/// is no object's is refused.
#[test]
fn a_revolution_turns_up_to_objects_of_another_component_by_their_link() {
    let (mut doc, body, face, b) = siblings();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [5.0, 0.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    let revolution = |extent: Value, links: Value| {
        json!({"cmd": "add_feature", "def": {"type": "revolve",
               "profiles": [{"sketch": sketch, "region": region}],
               "axis": "y", "extent": extent,
               "object_links": links, "operation": "new_body"}})
    };
    let to_face = json!({"type": "to_object",
                         "object": {"type": "face", "body": body, "face": face}});
    let unlinked = command(&mut doc, revolution(to_face.clone(), json!({})))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    let link = json!({body.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, revolution(to_face, link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    let spec = doc
        .kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|s| s.starts_with("revolve"))
        .cloned()
        .unwrap_or_default();
    assert!(spec.ends_with("target true"), "{spec}");
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["object_links"], link, "{def}");

    // The body itself, through the same kind of link.
    let to_body = json!({"type": "to_object",
                         "object": {"type": "body", "body": body, "through": true}});
    let uid: FeatureUid = command(&mut doc, revolution(to_body, link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));

    let error = rejected(&mut doc, revolution(json!({"type": "full"}), link));
    assert!(error.contains("linked but not an object's"), "{error}");
}

#[test]
fn a_derive_copies_bodies_of_another_component_and_follows_them() {
    // A:1 (O1) at (100, 0, 10) holds a block; B:1 (O2) at (0, 50, -3)
    // derives it (mitcad#324).
    let (mut doc, body, _, b) = siblings();
    let derive = |links: Value| {
        json!({"cmd": "add_feature", "def": {"type": "derive", "bodies": [body],
               "links": links}})
    };
    // Without the link the body is A's, which B's derive cannot see.
    let unlinked = command(&mut doc, derive(json!({})))["uid"].clone();
    let status = doc
        .status(unlinked.as_str().unwrap().parse().unwrap())
        .unwrap()
        .clone();
    assert!(
        status.error().is_some_and(|e| e.contains("belongs to A")),
        "{status:?}"
    );
    doc.undo();

    // Linked: a copy in B, moved from A into B by (100, -50, 13), its faces
    // the derive's; the source stays in A.
    let link = json!({body.to_string(): {"source": "O1"}});
    let uid: FeatureUid = command(&mut doc, derive(link.clone()))["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, b);
    assert_eq!(doc.feature(uid).unwrap().name, "Derive1");
    let copy = BodyUid::new(uid, 0);
    assert_eq!(doc.body_component(copy), Some(b));
    assert_eq!(doc.body_component(body), Some(ComponentUid(1)));
    let shape = doc.body_shape(copy).unwrap();
    assert!(
        shape.history.starts_with("moved(") && shape.history.ends_with(",100,-50,13,inst1)"),
        "{}",
        shape.history
    );
    assert!(
        shape
            .faces
            .iter()
            .all(|f| f.to_string().starts_with(&format!("{uid}:inst1("))),
        "{:?}",
        shape.faces
    );
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["links"], link, "{def}");

    // A moves up 10: the copy follows.
    doc.set_occurrence_transform(OccurrenceUid(1), shift(100.0, 0.0, 20.0), false)
        .unwrap();
    let moved = doc.body_shape(copy).unwrap().history.clone();
    assert!(moved.ends_with(",100,-50,23,inst1)"), "{moved}");

    // Kept in the design file.
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    loaded.recompute();
    assert!(loaded.status(uid).unwrap().is_ok());
    assert!(loaded.body_shape(copy).is_some());
    assert_eq!(loaded.to_json(), saved);

    // A link for a body that is not derived, and no bodies, are refused.
    let error = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "derive", "bodies": [body],
               "links": {"F99.b0": {"source": "O1"}}}}),
    );
    assert!(error.contains("linked but not derived"), "{error}");
    let error = rejected(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "derive", "bodies": []}}),
    );
    assert!(error.contains("no bodies"), "{error}");
}

#[test]
fn a_copied_component_keeps_its_derive_on_its_own_hidden_source() {
    // B holds a hidden occurrence of A and derives A's block from it; a
    // Paste New of B derives from its own copy's occurrence (mitcad#324).
    let mut doc = Document::new(MockKernel::default());
    let (a, _) = doc
        .create_component(Some("A"), Transform::IDENTITY, true)
        .unwrap();
    let (_, body, _) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (b, ob) = doc
        .create_component(Some("B"), shift(0.0, 50.0, 0.0), true)
        .unwrap();
    let inner = doc.add_occurrence(a, b, shift(5.0, 0.0, 0.0)).unwrap();
    doc.set_occurrence_visible(inner, false).unwrap();
    allow_attachments(&mut doc);
    let link = json!({body.to_string(): {"source": format!("{ob}/{inner}"),
                      "target": ob.to_string()}});
    let uid: FeatureUid = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "derive", "bodies": [body],
               "links": link}}),
    )["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let copied = doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .history
        .clone();
    assert!(copied.ends_with(",5,0,0,inst1)"), "{copied}");

    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, _) = doc.paste_new(ob, Some(shift(0.0, -50.0, 0.0))).unwrap();
    let derive = doc
        .features()
        .find(|f| {
            f.component == copy && matches!(f.def, mitcad_model::features::FeatureDef::Derive(_))
        })
        .cloned()
        .expect("the copy's derive");
    assert!(
        doc.status(derive.uid).unwrap().is_ok(),
        "{:?}",
        doc.status(derive.uid)
    );
    let mitcad_model::features::FeatureDef::Derive(def) = &derive.def else {
        unreachable!()
    };
    let link = def.links.values().next().unwrap();
    // The copy's own hidden occurrence of A.
    assert_eq!(link.source.0.len(), 2);
    assert_ne!(link.source.0[0], ob);
}

/// A plane of B offset from a face of A's block just after a captured
/// position moved B (mitcad#389): it lies where B is at that point of the
/// timeline, also with B's own placement elsewhere.
#[test]
fn a_linked_plane_after_a_captured_position_is_where_it_moved_the_occurrence() {
    let (mut doc, body, face, b) = siblings();
    // B's own placement is where a later position leaves it; the captured
    // position at this point puts it 10 up from where it starts.
    doc.set_occurrence_transform(OccurrenceUid(2), shift(0.0, 50.0, 17.0), false)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_transform(OccurrenceUid(2), shift(0.0, 50.0, 7.0), true)
        .unwrap();
    doc.activate_component(b).unwrap();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": {"face": face, "body": body},
                              "distance": 5},
               "links": {body.to_string(): {"source": "O1", "target": "O2"}}}}),
    );
    let datum = query(&doc, json!({"query": "datum", "uid": added["uid"]}));
    // A's top face is at z = 15 in the design, B at z = 7: the plane is 5
    // above the face, z = 13 in B.
    let z = datum["origin"][2].as_f64().unwrap();
    assert!((z - 13.0).abs() < 1e-9, "{datum}");
}

/// A sketch on a face of a component turned and moved (mitcad#505): its
/// frame is the face in the coordinates of the sketch's component, so the
/// sketch's occurrence placement, applied once, puts it on the face where
/// A's occurrence shows it, and an extrude of a rectangle at the face's
/// middle starts on the face. With A active (the face is its own: no
/// link), with the root active, and with B active, itself turned.
#[test]
fn a_sketch_on_a_face_of_a_turned_occurrence_lies_on_the_face_in_the_design() {
    let quarter = std::f64::consts::FRAC_PI_2;
    // A:1 (O1): turned 90 degrees about X, then moved; its block's top face
    // (z = 5 in A, the middle at (5, 5, 5)) faces -Y in the design, at
    // y = -5, its middle at (105, -5, 55).
    let a_placement = shift(100.0, 0.0, 50.0)
        .after(&Transform::rotation([0.0; 3], [1.0, 0.0, 0.0], quarter).unwrap());
    // B:1 (O2): turned 90 degrees about Z, then moved.
    let b_placement = shift(0.0, 50.0, -3.0)
        .after(&Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], quarter).unwrap());
    let mut doc = Document::new(MockKernel::default());
    let (a, _) = doc.create_component(Some("A"), a_placement, true).unwrap();
    let (_, body, face) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (b, _) = doc.create_component(Some("B"), b_placement, true).unwrap();
    allow_attachments(&mut doc);
    let middle_in_a = [5.0, 5.0, 5.0];
    let face_middle = [105.0, -5.0, 55.0];
    let face_normal = [0.0, -1.0, 0.0];
    let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6);
    let corners = |min: [f64; 3], max: [f64; 3]| -> Vec<[f64; 3]> {
        let mut all = Vec::new();
        for x in [min[0], max[0]] {
            for y in [min[1], max[1]] {
                for z in [min[2], max[2]] {
                    all.push([x, y, z]);
                }
            }
        }
        all
    };

    for (active, placement, link) in [
        (a, a_placement, Value::Null),
        (
            ComponentUid::ROOT,
            Transform::IDENTITY,
            json!({"source": "O1"}),
        ),
        (b, b_placement, json!({"source": "O1", "target": "O2"})),
    ] {
        doc.activate_component(active).unwrap();
        // Picked where A:1 shows the face.
        let created = command(
            &mut doc,
            json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
                   "occurrence": "O1"}),
        );
        let uid: FeatureUid = created["uid"].as_str().unwrap().parse().unwrap();
        assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
        assert_eq!(doc.feature(uid).unwrap().component, active);
        let s = query(&doc, json!({"query": "sketch", "uid": created["uid"]}));
        assert_eq!(s["plane_link"], link, "{active:?}");
        let vector = |v: &Value| -> [f64; 3] { std::array::from_fn(|i| v[i].as_f64().unwrap()) };
        let frame = mitcad_model::profile::SketchFrame {
            origin: vector(&s["frame"]["origin"]),
            x_axis: vector(&s["frame"]["x_axis"]),
            y_axis: vector(&s["frame"]["y_axis"]),
        };
        // The frame in the design: the sketch's occurrence placement once.
        let world = placement.apply_frame(&frame);
        let normal = mitcad_model::internal::profile::cross(world.x_axis, world.y_axis);
        assert!(close(normal, face_normal), "{active:?}: normal {normal:?}");
        let off_face = mitcad_model::internal::profile::dot(
            std::array::from_fn(|i| world.origin[i] - face_middle[i]),
            face_normal,
        );
        assert!(
            off_face.abs() < 1e-6,
            "{active:?}: origin {:?}",
            world.origin
        );

        // A 4 x 4 rectangle around the face's middle, extruded 3.
        let middle = mitcad_model::assembly::inverse(&placement)
            .apply_point(a_placement.apply_point(middle_in_a));
        let d: [f64; 3] = std::array::from_fn(|i| middle[i] - frame.origin[i]);
        let at = [
            mitcad_model::internal::profile::dot(d, frame.x_axis),
            mitcad_model::internal::profile::dot(d, frame.y_axis),
        ];
        let region = doc
            .add_rectangle(uid, [at[0] - 2.0, at[1] - 2.0], &num(4.0), &num(4.0))
            .unwrap()
            .region;
        let extruded = doc
            .add_feature(&extrude(uid, &[&region], 3.0, "new_body", &[]), None)
            .unwrap()
            .uid;
        assert!(doc.status(extruded).unwrap().is_ok());
        let bounds = doc
            .body_shape(BodyUid::new(extruded, 0))
            .unwrap()
            .bounds
            .unwrap();
        let placed: Vec<[f64; 3]> = corners(bounds.min, bounds.max)
            .into_iter()
            .map(|c| placement.apply_point(c))
            .collect();
        // In the design it starts on the face (y = -5) and goes 3 out of it
        // (-Y), around the face's middle.
        let along = |c: &[f64; 3]| mitcad_model::internal::profile::dot(*c, face_normal) - 5.0;
        let nearest = placed.iter().map(along).fold(f64::INFINITY, f64::min);
        let farthest = placed.iter().map(along).fold(f64::NEG_INFINITY, f64::max);
        assert!(
            nearest.abs() < 1e-6,
            "{active:?}: starts {nearest} off the face"
        );
        assert!(
            (farthest - 3.0).abs() < 1e-6,
            "{active:?}: ends at {farthest}"
        );
        for c in &placed {
            assert!(
                (c[0] - 105.0).hypot(c[2] - 55.0) <= 2.0 * std::f64::consts::SQRT_2 + 1e-6,
                "{active:?}: {c:?} is not around the face's middle"
            );
        }
    }
}

/// Siblings with B's own block and a second occurrence of A, A:2 (O3) at
/// (120, 0, 10): two screws of one part beside B's block (mitcad#544).
fn two_occurrences() -> (Document<MockKernel>, BodyUid, BodyUid, OccurrenceUid) {
    let (mut doc, tool, _, b) = siblings();
    let (_, target, _) = block(&mut doc);
    let second = doc
        .add_occurrence(ComponentUid(1), ComponentUid::ROOT, shift(120.0, 0.0, 10.0))
        .unwrap();
    assert_eq!(doc.active_component(), b);
    (doc, tool, target, second)
}

/// The combine's definition in the design file.
fn saved_combine(doc: &Document<MockKernel>) -> Value {
    let saved: Value = serde_json::from_str(&doc.to_json()).unwrap();
    saved["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "combine")
        .cloned()
        .unwrap()
}

#[test]
fn a_combine_cuts_with_a_body_in_two_occurrences_each_where_it_is() {
    let (mut doc, tool, target, second) = two_occurrences();
    let tools = json!([{"body": tool, "occurrence": "O1"},
                       {"body": tool, "occurrence": second.to_string()}]);
    let combine = |tools: &Value, keep: bool| {
        json!({"type": "combine", "target": target, "tools": tools, "operation": "cut",
               "keep_tools": keep})
    };
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": combine(&tools, true)}),
    );
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().component, ComponentUid(2));
    // Two cuts: A's block moved into B from A:1, by (100, -50, 13), and
    // from A:2, by (120, -50, 13).
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(cut.starts_with("cut("), "{cut}");
    assert!(cut.contains(",100,-50,13)"), "{cut}");
    assert!(cut.contains(",120,-50,13)"), "{cut}");
    // Each tool keeps its own occurrence; no tool_links.
    let def = query(&doc, json!({"query": "feature", "uid": uid.to_string()}));
    assert_eq!(def["def"]["tools"], tools, "{def}");
    assert!(def["def"].get("tool_links").is_none(), "{def}");

    // A:2 moves up 10: its cut follows, A:1's stays.
    doc.set_occurrence_transform(second, shift(120.0, 0.0, 20.0), false)
        .unwrap();
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(cut.contains(",100,-50,13)"), "{cut}");
    assert!(cut.contains(",120,-50,23)"), "{cut}");
    assert!(!cut.contains(",120,-50,13)"), "{cut}");

    // The design file keeps both tools with their occurrences.
    assert_eq!(saved_combine(&doc)["tools"], tools);
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();
    assert_eq!(loaded.body_shape(target).unwrap().history, cut);

    // The same occurrence twice is refused.
    let twice = json!([{"body": tool, "occurrence": "O1"}, {"body": tool, "occurrence": "O1"}]);
    let error = rejected(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": combine(&twice, true)}),
    );
    assert!(
        error.contains(&format!("body {tool} in O1 is listed more than once")),
        "{error}"
    );

    // Consumed: the body leaves A, once for both tools.
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": combine(&tools, false)}),
    );
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.body_component(tool), None);
}

#[test]
fn a_combine_joins_a_body_in_two_occurrences() {
    let (mut doc, tool, target, second) = two_occurrences();
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "combine", "target": target,
               "tools": [{"body": tool, "occurrence": "O1"},
                         {"body": tool, "occurrence": second.to_string()}],
               "operation": "join", "keep_tools": true}}),
    );
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let joined = doc.body_shape(target).unwrap().history.clone();
    assert!(joined.starts_with("unite("), "{joined}");
    assert!(joined.contains(",100,-50,13)"), "{joined}");
    assert!(joined.contains(",120,-50,13)"), "{joined}");
    // Kept tools: A keeps its block.
    assert_eq!(doc.body_component(tool), Some(ComponentUid(1)));
}

#[test]
fn a_combine_of_an_earlier_version_with_tool_links_loads_and_recomputes_as_before() {
    let (mut doc, tool, target, second) = two_occurrences();
    // As files of earlier versions have it: the tool as an id, its link in
    // tool_links.
    let link = json!({tool.to_string(): {"source": "O1"}});
    let legacy = json!({"type": "combine", "target": target, "tools": [tool],
                        "operation": "cut", "keep_tools": true, "tool_links": link});
    let added = command(&mut doc, json!({"cmd": "add_feature", "def": legacy}));
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(cut.contains(",100,-50,13)"), "{cut}");
    assert!(!cut.contains(",120,"), "only A:1 cuts: {cut}");
    // Saved as it was written.
    let combine = saved_combine(&doc);
    assert_eq!(combine["tools"], json!([tool]));
    assert_eq!(combine["tool_links"], link);
    let saved = doc.to_json();
    let mut loaded = Document::from_json(&saved, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), saved);
    loaded.recompute();
    assert!(loaded.status(uid).unwrap().is_ok());
    assert_eq!(loaded.body_shape(target).unwrap().history, cut);

    // A:2 beside it as a tool in its occurrence: both cut.
    let mixed = json!({"type": "combine", "target": target,
                       "tools": [tool, {"body": tool, "occurrence": second.to_string()}],
                       "operation": "cut", "keep_tools": true, "tool_links": link});
    command(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": mixed}),
    );
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    let cut = doc.body_shape(target).unwrap().history.clone();
    assert!(
        cut.contains(",100,-50,13)") && cut.contains(",120,-50,13)"),
        "{cut}"
    );
    // The linked id and the same body in A:1 are one tool twice.
    let twice = json!({"type": "combine", "target": target,
                       "tools": [tool, {"body": tool, "occurrence": "O1"}],
                       "operation": "cut", "keep_tools": true, "tool_links": link});
    let error = rejected(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": twice}),
    );
    assert!(error.contains("listed more than once"), "{error}");
    // A tool_links entry for a body that is a tool only in occurrences is
    // refused.
    let stray = json!({"type": "combine", "target": target,
                       "tools": [{"body": tool, "occurrence": second.to_string()}],
                       "operation": "cut", "keep_tools": true, "tool_links": link});
    let error = rejected(
        &mut doc,
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": stray}),
    );
    assert!(error.contains("linked but not a tool"), "{error}");
}

/// Paste New of B with the combine keeps the tools in A's occurrences,
/// which are outside the copy (mitcad#544).
#[test]
fn paste_new_keeps_a_combines_tools_in_occurrences() {
    let (mut doc, tool, target, second) = two_occurrences();
    let tools = json!([{"body": tool, "occurrence": "O1"},
                       {"body": tool, "occurrence": second.to_string()}]);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "combine", "target": target,
               "tools": tools, "operation": "cut", "keep_tools": true}}),
    );
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (copy, occurrence) = doc.paste_new(OccurrenceUid(2), None).unwrap();
    let combine = doc
        .features()
        .find(|f| f.component == copy && f.def.type_name() == "combine")
        .unwrap();
    match &combine.def {
        mitcad_model::features::FeatureDef::Combine(c) => {
            assert_eq!(c.tools.len(), 2);
            for (t, source) in c.tools.iter().zip([OccurrenceUid(1), second]) {
                assert_eq!(t.body, tool);
                let link = t.link.as_ref().unwrap();
                assert_eq!(link.source.0, [source]);
                assert_eq!(link.target.0, [occurrence]);
            }
        }
        _ => panic!("not a combine"),
    }
    assert!(doc.status(combine.uid).unwrap().is_ok());
}

/// A Join of a component's sketch whose own bodies the extrusion misses but
/// whose child occurrence's body it touches names that body and says that
/// a feature joins only its own component's bodies (mitcad#958): Brace:1
/// (O1) is turned a quarter about X, Bracket:1 (O2) inside it turned 45°
/// about Z and moved 20 along x, with a 10 x 10 x 5 block; Brace has a rod
/// far away and a 4 x 4 profile over the bracket.
#[test]
fn a_join_that_touches_only_a_child_occurrences_body_names_it() {
    let mut doc = Document::new(MockKernel::default());
    let quarter = Transform::rotation(
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        std::f64::consts::FRAC_PI_2,
    )
    .unwrap();
    let (brace, _) = doc.create_component(Some("Brace"), quarter, true).unwrap();
    let oblique = shift(20.0, 0.0, 0.0).after(
        &Transform::rotation(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            std::f64::consts::FRAC_PI_4,
        )
        .unwrap(),
    );
    doc.create_component(Some("Bracket"), oblique, true)
        .unwrap();
    let (_, bracket_body, _) = block(&mut doc);
    doc.activate_component(brace).unwrap();
    // The rod, Brace's own body, far from the profile.
    let rod_sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let rod_region = doc
        .add_rectangle(rod_sketch, [-50.0, -50.0], &num(5.0), &num(5.0))
        .unwrap()
        .region;
    doc.add_feature(
        &extrude(rod_sketch, &[&rod_region], 5.0, "new_body", &[]),
        None,
    )
    .unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [18.0, 4.0], &num(4.0), &num(4.0))
        .unwrap()
        .region;
    // The kernel's join reaches no participant (the rod).
    *doc.kernel().touch.borrow_mut() = Some(vec![false]);
    let join = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "join", &[]), None)
        .unwrap()
        .uid;
    assert_eq!(doc.feature(join).unwrap().component, brace);
    let status = doc.status(join).unwrap().clone();
    let error = status.error().expect("the join fails").to_owned();
    assert!(
        error.starts_with("the extrusion does not touch any participant body of Brace; "),
        "{error}"
    );
    assert!(
        error.contains(&format!("it touches body {bracket_body} (from "))
            && error.contains(") in Bracket:1, a body of another component"),
        "{error}"
    );
    assert!(
        error.contains("a feature of Brace joins only Brace's own bodies")
            && error.contains("choose New Body, or activate Bracket"),
        "{error}"
    );
    // The bracket's body cannot be a participant either: it is Bracket's
    // (refused, or the feature fails saying so).
    let refused = match doc.edit_feature(
        join,
        &extrude(sketch, &[&region], 5.0, "join", &[bracket_body]),
    ) {
        Err(e) => e.to_string(),
        Ok(_) => doc
            .status(join)
            .and_then(|s| s.error())
            .unwrap_or_default()
            .to_owned(),
    };
    assert!(refused.contains("belongs to Bracket"), "{refused}");
    // As a new body it succeeds.
    *doc.kernel().touch.borrow_mut() = None;
    doc.edit_feature(join, &extrude(sketch, &[&region], 5.0, "new_body", &[]))
        .unwrap();
    assert!(doc.status(join).unwrap().is_ok(), "{:?}", doc.status(join));
}
