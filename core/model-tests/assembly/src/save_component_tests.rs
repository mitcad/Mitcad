// SPDX-License-Identifier: MIT
//! Save Component as Design (mitcad#1057) with the mock kernel: a
//! component with sketches, a feature, a component inside it and a
//! parameter of the design is written as a design of its own and opened
//! again to the same bodies; its projection of another component's face
//! and its sketch on that face are listed and frozen, and so are a move's
//! and a point pattern's points picked on another component (mitcad#1069);
//! Replace with a Link keeps the design's bodies and placements in one
//! undo step. Review fixes (mitcad#1150): two features reading one outside
//! body share one frozen copy; links inside an unsaved design are recorded
//! from the command's folder; a component placed twice inside refuses
//! Replace; annotations on the replaced bodies follow the base feature.

use serde_json::{Value, json};

use mitcad_model::features::{FeatureDef, OccurrencePath, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::{Curve3, Kernel};
use mitcad_model::test_support::{allow_attachments, def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Annotation, Document, InsertOptions, MeasureKind, MeasurePick};
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    serde_json::from_str(&doc.command(&command.to_string()).unwrap()).unwrap()
}

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

/// A 10 x 10 block as high as the design's parameter `height` in the
/// active component: its body and top face.
fn block(doc: &mut Document<MockKernel>) -> (BodyUid, String) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let extrude = def(json!({"type": "extrude",
        "profiles": [{"sketch": sketch, "region": region}],
        "extent": {"type": "distance", "distance": "height"},
        "operation": "new_body", "participants": []}));
    let uid = doc.add_feature(&extrude, None).unwrap().uid;
    (BodyUid::new(uid, 0), format!("{uid}:end({region})"))
}

/// Root with A:1 (O1) at (100, 0, 10) holding a block, and Box:1 (O2) at
/// (0, 50, -3) holding a block, Pin:1 (O3, a block of its own, where Box
/// is) and two sketches on A's top face: a linked projection of it
/// (`projected`) and a sketch on it (`on_face`). The blocks are as high as
/// the parameter `height` (10). Box is active.
struct Design {
    doc: Document<MockKernel>,
    component: ComponentUid,
    projected: FeatureUid,
    on_face: FeatureUid,
    /// Box's block: its body and top face.
    box_block: (BodyUid, String),
    /// Pin's block: its body and top face.
    pin_block: (BodyUid, String),
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("height", 10.0, "").unwrap();
    doc.create_component(Some("A"), shift(100.0, 0.0, 10.0), true)
        .unwrap();
    let (body, face) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (component, _) = doc
        .create_component(Some("Box"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    let box_block = block(&mut doc);
    doc.create_component(Some("Pin"), Transform::IDENTITY, true)
        .unwrap();
    let pin_block = block(&mut doc);
    doc.activate_component(component).unwrap();
    allow_attachments(&mut doc);
    // A's top face's edges in A's coordinates.
    let square: Vec<Curve3> = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
        .iter()
        .zip([[10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]])
        .map(|(a, b)| Curve3::Line {
            start: [a[0], a[1], 10.0],
            end: [b[0], b[1], 10.0],
        })
        .collect();
    *doc.kernel().curves.borrow_mut() = square;
    let projected = command(&mut doc, json!({"cmd": "sketch.create"}))["uid"].clone();
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": projected, "source": face, "body": body,
               "linked": true, "occurrence": "O1"}),
    );
    let on_face = command(
        &mut doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": "O1"}),
    )["uid"]
        .clone();
    let uid = |v: &Value| -> FeatureUid { v.as_str().unwrap().parse().unwrap() };
    let design = Design {
        projected: uid(&projected),
        on_face: uid(&on_face),
        doc,
        component,
        box_block,
        pin_block,
    };
    for f in design.doc.features() {
        assert!(
            design.doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            design.doc.status(f.uid)
        );
    }
    design
}

/// The boxes of the visible bodies whose occurrence path starts with
/// `under`, moved by `moved` after their placement, sorted.
fn boxes(doc: &Document<MockKernel>, under: &[OccurrenceUid], moved: Transform) -> Vec<[f64; 6]> {
    let mut out: Vec<[f64; 6]> = doc
        .instances()
        .into_iter()
        .filter(|i| i.visible && i.path.starts_with(under))
        .map(|i| {
            let shape = doc.body_shape(i.body).unwrap();
            let placed = doc
                .kernel()
                .transform_shape(shape, &moved.after(&i.transform), None)
                .unwrap();
            let b = placed.bounds.unwrap();
            [b.min[0], b.min[1], b.min[2], b.max[0], b.max[1], b.max[2]]
        })
        .collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn same_boxes(a: &[[f64; 6]], b: &[[f64; 6]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.iter().zip(y).all(|(p, q)| (p - q).abs() <= 1e-6))
}

fn named(doc: &Document<MockKernel>, name: &str) -> FeatureUid {
    doc.features()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no feature {name}"))
        .uid
}

#[test]
fn a_component_saved_as_a_design_opens_to_the_same_bodies() {
    let dir = mitcad_model::testing::unique_temp_dir("1057-save");
    let Design {
        mut doc,
        component,
        projected,
        on_face,
        ..
    } = design();
    let projected_name = doc.feature(projected).unwrap().name.clone();
    let on_face_name = doc.feature(on_face).unwrap().name.clone();

    // The root and a linked component are not offered.
    let root = query(
        &doc,
        json!({"query": "save_component_as_design", "component": "C0"}),
    );
    assert!(
        root["blocked"]
            .as_str()
            .is_some_and(|b| b.contains("the design itself")),
        "{root}"
    );

    // The dialog lists the references outside Box before saving: both on
    // A:1, frozen.
    let listed = query(
        &doc,
        json!({"query": "save_component_as_design", "component": "Box"}),
    );
    assert_eq!(listed["blocked"], Value::Null, "{listed}");
    let references = listed["references"].as_array().unwrap();
    let kinds: Vec<&str> = references
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["projection", "sketch_plane"], "{listed}");
    for r in references {
        assert_eq!(r["target"], "A:1", "{r}");
        assert_eq!(r["frozen"], true, "{r}");
    }
    assert_eq!(
        references[0]["message"],
        format!("{projected_name}: its projection of A:1, frozen at its current geometry")
    );

    let before = doc.to_json();
    let path = dir.join("Box.mitcad");
    let saved = command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": component,
               "path": path, "base": dir}),
    );
    assert_eq!(saved["replaced"], false);
    assert_eq!(saved["link"], Value::Null);
    assert_eq!(saved["references"].as_array().unwrap().len(), 2);
    // The design does not change.
    assert_eq!(doc.to_json(), before);

    // Opened again: Box is the root, Pin a component in it, the parameter
    // the design's own, and every feature builds.
    let text = std::fs::read_to_string(&path).unwrap();
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    for f in opened.features() {
        assert!(
            opened.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            opened.status(f.uid)
        );
        assert!(f.component.is_root() || opened.assembly().name(f.component) == "Pin");
    }
    let names: Vec<&str> = opened
        .assembly()
        .components
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(names, ["Pin"]);
    assert!(opened.parameters().find("height").is_some());

    // The same bodies, in Box's coordinates, within 1e-6 mm.
    let original = boxes(&doc, &[OccurrenceUid(2)], shift(0.0, -50.0, 3.0));
    let reopened = boxes(&opened, &[], Transform::IDENTITY);
    assert_eq!(original.len(), 2);
    assert!(
        same_boxes(&original, &reopened),
        "{original:?} != {reopened:?}"
    );

    // The projection is frozen: its curves where A's face is now (A's
    // (0, 0) at (100, -50) in Box), no longer linked.
    let sketch = named(&opened, &projected_name);
    let s = query(&opened, json!({"query": "sketch", "uid": sketch}));
    assert!(
        s["projections"].as_array().is_none_or(|p| p.is_empty()),
        "{s}"
    );
    let mut corners: Vec<[f64; 2]> = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "point")
        .map(|e| [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()])
        .collect();
    corners.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(corners.len(), 4, "{s}");
    assert!((corners[0][0] - 100.0).abs() < 1e-6 && (corners[0][1] + 50.0).abs() < 1e-6);
    // The sketch on A's face lies where the face is now: z = 10 + 10 in the
    // design, 23 in Box.
    let sketch = named(&opened, &on_face_name);
    let s = query(&opened, json!({"query": "sketch", "uid": sketch}));
    assert_eq!(s["plane_link"], Value::Null, "{s}");
    let origin: Vec<f64> = s["frame"]["origin"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert!(
        origin
            .iter()
            .zip([0.0, 0.0, 23.0])
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{s}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A move and a point pattern of Box take their points from a vertex of
/// A:1 (mitcad#1069): saved, both are listed as cut references, frozen
/// where the vertex is now, and the new design opens to the same bodies.
#[test]
fn points_picked_on_another_component_are_frozen() {
    let dir = mitcad_model::testing::unique_temp_dir("1069-points");
    let mut doc = Document::new(MockKernel::default());
    let (_, a) = doc
        .create_component(Some("A"), shift(100.0, 0.0, 10.0), true)
        .unwrap();
    let corner = doc
        .add_feature(
            &def(
                json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 10,
                        "width": 10, "height": 10, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    // A's vertex at (10, 0, 10) in A: (110, 0, 20) in the design, and
    // (110, -50, 23) in Box.
    let vertex = format!("V{{{corner}:side0|{corner}:side1|{corner}:top}}")
        .parse::<mitcad_model::topo::VertexName>()
        .unwrap()
        .to_string();
    doc.kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [10.0, 0.0, 10.0]);
    let point = json!({"body": BodyUid::new(corner, 0), "vertex": vertex});
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (component, _) = doc
        .create_component(Some("Box"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    let body = BodyUid::new(
        doc.add_feature(
            &def(
                json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 4,
                        "width": 4, "height": 4, "operation": "new_body"}),
            ),
            None,
        )
        .unwrap()
        .uid,
        0,
    );
    let moved = doc
        .add_feature(
            &def(json!({"type": "move", "bodies": [body],
                "transform": {"type": "point_to_point", "from": {"point": [0, 0, 0]},
                    "to": point, "to_occurrence": a.to_string()}})),
            None,
        )
        .unwrap()
        .uid;
    let pattern = doc
        .add_feature(
            &def(json!({"type": "pattern_at_points",
                "objects": {"type": "bodies", "bodies": [body]},
                "targets": [{"point": point, "occurrence": a.to_string()}]})),
            None,
        )
        .unwrap()
        .uid;
    for f in doc.features() {
        assert!(
            doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            doc.status(f.uid)
        );
    }
    let move_name = doc.feature(moved).unwrap().name.clone();
    let pattern_name = doc.feature(pattern).unwrap().name.clone();

    // Listed before saving, not refused.
    let listed = query(
        &doc,
        json!({"query": "save_component_as_design", "component": "Box"}),
    );
    assert_eq!(listed["blocked"], Value::Null, "{listed}");
    let references = listed["references"].as_array().unwrap();
    let features: Vec<&str> = references
        .iter()
        .map(|r| r["feature_name"].as_str().unwrap())
        .collect();
    assert_eq!(features, [move_name.as_str(), pattern_name.as_str()]);
    for r in references {
        assert_eq!(r["kind"], "point", "{r}");
        assert_eq!(r["target"], "A:1", "{r}");
        assert_eq!(r["frozen"], true, "{r}");
    }
    assert_eq!(
        references[0]["message"],
        format!("{move_name}: its point on A:1, frozen at its current geometry")
    );

    let before = doc.to_json();
    let path = dir.join("Box.mitcad");
    let saved = command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": component,
               "path": path, "base": dir}),
    );
    assert_eq!(saved["references"], listed["references"], "{saved}");
    assert_eq!(doc.to_json(), before);

    // Opened again: every feature builds, to the same bodies in Box's
    // coordinates within 1e-6 mm.
    let text = std::fs::read_to_string(&path).unwrap();
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    for f in opened.features() {
        assert!(
            opened.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            opened.status(f.uid)
        );
    }
    let original = boxes(&doc, &[OccurrenceUid(2)], shift(0.0, -50.0, 3.0));
    let reopened = boxes(&opened, &[], Transform::IDENTITY);
    assert_eq!(original.len(), 2);
    assert!(
        same_boxes(&original, &reopened),
        "{original:?} != {reopened:?}"
    );
    // The move's point is a fixed one where A's vertex is in Box.
    let frozen = opened.feature(named(&opened, &move_name)).unwrap();
    let mitcad_model::features::FeatureDef::Move(m) = &frozen.def else {
        panic!("{:?}", frozen.def);
    };
    let mitcad_model::features::moves::MoveSpec::PointToPoint {
        to: mitcad_model::features::GeomRef::FixedPoint(to),
        to_occurrence: None,
        ..
    } = &m.transform
    else {
        panic!("{:?}", m.transform);
    };
    assert!(
        to.iter()
            .zip([110.0, -50.0, 23.0])
            .all(|(p, q)| (p - q).abs() <= 1e-6),
        "{to:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_with_a_link_keeps_the_bodies_and_placements() {
    let dir = mitcad_model::testing::unique_temp_dir("1057-replace");
    let Design {
        mut doc, component, ..
    } = design();
    doc.set_occurrence_grounded(OccurrenceUid(2), true).unwrap();
    let all = boxes(&doc, &[], Transform::IDENTITY);
    let placed = doc.assembly().occurrence(OccurrenceUid(2)).unwrap().clone();

    let path = dir.join("Box.mitcad");
    let saved = command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": "Box",
               "path": path, "base": dir, "replace": true}),
    );
    assert_eq!(saved["replaced"], true, "{saved}");
    assert_eq!(saved["link"], "Box.mitcad");

    // Box is a linked component of the file now: a base feature, no
    // features or components of its own; its occurrence stays where it
    // was, grounded.
    let link = doc
        .assembly()
        .component(component)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert_eq!(link.path, "Box.mitcad");
    let own: Vec<&str> = doc
        .features()
        .filter(|f| f.component == component)
        .map(|f| f.def.type_name())
        .collect();
    assert_eq!(own, ["base"]);
    assert!(!doc.assembly().components.iter().any(|c| c.name == "Pin"));
    let now = doc.assembly().occurrence(OccurrenceUid(2)).unwrap();
    assert_eq!(now.transform, placed.transform);
    assert!(now.grounded);
    for f in doc.features() {
        assert!(
            doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            doc.status(f.uid)
        );
    }
    // The design's bodies and positions are the same as before.
    let after = boxes(&doc, &[], Transform::IDENTITY);
    assert!(same_boxes(&all, &after), "{all:?} != {after:?}");

    // The link is followed when the design opens: the file is as saved.
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let messages = reopened.update_links(Some(&dir)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");

    // One undo step brings Box's features back.
    assert_eq!(
        doc.undo().as_deref(),
        Some("Replace Box with a Link to Box.mitcad")
    );
    assert!(doc.assembly().component(component).unwrap().link.is_none());
    assert!(doc.assembly().components.iter().any(|c| c.name == "Pin"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// The features of `doc` of the component `component` and type `kind`.
fn features_of(doc: &Document<MockKernel>, component: ComponentUid, kind: &str) -> Vec<FeatureUid> {
    doc.features()
        .filter(|f| f.component == component && f.def.type_name() == kind)
        .map(|f| f.uid)
        .collect()
}

fn component_named(doc: &Document<MockKernel>, name: &str) -> ComponentUid {
    doc.assembly()
        .components
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no component {name}"))
        .uid
}

/// Two combines of Box cut its two blocks with A's block, kept
/// (mitcad#1150): the new design holds one hidden copy of A's block, read
/// by both, and no orphan copy.
#[test]
fn two_features_reading_one_outside_body_share_its_frozen_copy() {
    let dir = mitcad_model::testing::unique_temp_dir("1150-shared");
    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("height", 10.0, "").unwrap();
    doc.create_component(Some("A"), shift(100.0, 0.0, 10.0), true)
        .unwrap();
    let (tool, _) = block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let (component, _) = doc
        .create_component(Some("Box"), shift(0.0, 50.0, -3.0), true)
        .unwrap();
    let link = json!({tool.to_string(): {"source": "O1"}});
    for _ in 0..2 {
        let (target, _) = block(&mut doc);
        doc.add_feature(
            &def(json!({"type": "combine", "target": target, "tools": [tool],
                "operation": "cut", "keep_tools": true, "tool_links": link})),
            None,
        )
        .unwrap();
    }
    for f in doc.features() {
        assert!(
            doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            doc.status(f.uid)
        );
    }

    let path = dir.join("Box.mitcad");
    let saved = command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": component,
               "path": path, "base": dir}),
    );
    let kinds: Vec<&str> = saved["references"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["body", "body"], "{saved}");

    let text = std::fs::read_to_string(&path).unwrap();
    let mut opened = Document::from_json(&text, MockKernel::default()).unwrap();
    opened.recompute();
    for f in opened.features() {
        assert!(
            opened.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            opened.status(f.uid)
        );
    }
    let frozen: Vec<FeatureUid> = opened
        .features()
        .filter(|f| {
            matches!(&f.def, FeatureDef::Base(b)
                if b.source.as_deref().is_some_and(|s| s.contains("reads, frozen")))
        })
        .map(|f| f.uid)
        .collect();
    assert_eq!(frozen.len(), 1, "one hidden copy of A's block");
    let copy = BodyUid::new(frozen[0], 0);
    let combines = features_of(&opened, ComponentUid::ROOT, "combine");
    assert_eq!(combines.len(), 2);
    for uid in combines {
        let FeatureDef::Combine(c) = &opened.feature(uid).unwrap().def else {
            unreachable!();
        };
        let tools: Vec<BodyUid> = c.tools.iter().map(|t| t.body).collect();
        assert_eq!(tools, [copy]);
        assert!(c.tool_links.is_empty(), "{:?}", c.tool_links);
    }
    // The bodies Box shows, the copy hidden: it keeps its name and with it
    // its visibility in the file. (Not compared by their boxes: the mock
    // kernel reads a base feature's body back as a 10 mm box wherever it
    // was, and the cuts with it change accordingly.)
    let shown = |doc: &Document<MockKernel>, under: &[OccurrenceUid]| {
        doc.instances()
            .into_iter()
            .filter(|i| i.visible && i.path.starts_with(under))
            .count()
    };
    assert_eq!(shown(&doc, &[OccurrenceUid(2)]), 2);
    assert_eq!(shown(&opened, &[]), 2);
    let hidden = opened
        .instances()
        .into_iter()
        .find(|i| i.body == copy)
        .expect("the copy is placed");
    assert!(!hidden.visible, "the frozen copy is hidden");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A design never saved has no folder of its own: the links of the
/// linked components inside the saved one are read from the folder the
/// command gives, as the replace link is (mitcad#1150), and recorded
/// relative to the new file.
#[test]
fn links_inside_an_unsaved_design_are_read_from_the_given_folder() {
    let dir = mitcad_model::testing::unique_temp_dir("1150-base");
    std::fs::create_dir_all(dir.join("parts")).unwrap();
    let mut part = Document::new(MockKernel::default());
    part.add_feature(
        &def(
            json!({"type": "box", "plane": "xy", "corner": [0, 0], "length": 4,
                    "width": 4, "height": 4, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    std::fs::write(dir.join("parts").join("Pin.mitcad"), part.to_json()).unwrap();

    let mut doc = Document::new(MockKernel::default());
    doc.add_parameter("height", 10.0, "").unwrap();
    let (component, _) = doc
        .create_component(Some("Box"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut doc);
    let options = InsertOptions {
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (pin, _) = doc.insert_component("parts/Pin.mitcad", &options).unwrap();
    let recorded = |doc: &Document<MockKernel>, pin: ComponentUid| {
        doc.assembly()
            .component(pin)
            .unwrap()
            .link
            .as_ref()
            .unwrap()
            .path
            .clone()
    };
    assert_eq!(recorded(&doc, pin), "parts/Pin.mitcad");

    let path = dir.join("Box.mitcad");
    command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": component,
               "path": path, "base": dir}),
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let opened = Document::from_json(&text, MockKernel::default()).unwrap();
    let copied = component_named(&opened, "Pin");
    assert_eq!(recorded(&opened, copied), "parts/Pin.mitcad");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Pin placed twice in Box: its two placements' bodies are one body, which
/// the outer references name without the placement, so Replace with a
/// Link is refused with Pin named, before anything is written
/// (mitcad#1150).
#[test]
fn replace_is_refused_while_a_component_is_placed_twice_inside() {
    let dir = mitcad_model::testing::unique_temp_dir("1150-twice");
    let Design {
        mut doc, component, ..
    } = design();
    let pin = component_named(&doc, "Pin");
    doc.add_occurrence(pin, component, shift(20.0, 0.0, 0.0))
        .unwrap();
    let before = doc.to_json();
    let path = dir.join("Box.mitcad");
    let error = doc
        .command(
            &json!({"cmd": "save_component_as_design", "component": "Box",
                    "path": path, "base": dir, "replace": true})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Pin is placed in it more than once"),
        "{error}"
    );
    assert!(!path.exists());
    assert_eq!(doc.to_json(), before);
    // Saved without Replace, it is a design of its own as before.
    command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": "Box",
               "path": path, "base": dir}),
    );
    assert!(path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A distance from a face of Box's block to Pin's body inside Box: after
/// Replace with a Link both picks name the base feature's bodies, the face
/// one of its faces, Pin's seen at Box's occurrence (mitcad#1150).
#[test]
fn annotations_on_a_replaced_component_follow_its_base_feature() {
    let dir = mitcad_model::testing::unique_temp_dir("1150-annotation");
    let Design {
        mut doc, component, ..
    } = design();
    let pin = component_named(&doc, "Pin");
    let box_body = BodyUid::new(features_of(&doc, component, "extrude")[0], 0);
    let pin_body = BodyUid::new(features_of(&doc, pin, "extrude")[0], 0);
    let face = doc.body_shape(box_body).unwrap().faces[0].clone();
    let mut gap = Annotation::new(
        "Gap",
        MeasureKind::Distance,
        "",
        MeasurePick {
            body: Some(box_body),
            face: Some(face.clone()),
            occurrence: Some(OccurrencePath(vec![OccurrenceUid(2)])),
            ..MeasurePick::default()
        },
    );
    gap.b = Some(MeasurePick {
        body: Some(pin_body),
        occurrence: Some(OccurrencePath(vec![OccurrenceUid(2), OccurrenceUid(3)])),
        ..MeasurePick::default()
    });
    doc.add_annotation(gap).unwrap();

    let path = dir.join("Box.mitcad");
    command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": "Box",
               "path": path, "base": dir, "replace": true}),
    );
    let base = doc
        .assembly()
        .component(component)
        .unwrap()
        .link
        .as_ref()
        .unwrap()
        .feature;
    let gap = doc.annotations().iter().find(|a| a.name == "Gap").unwrap();
    let a = gap.a.body.unwrap();
    let b = gap.b.as_ref().unwrap().body.unwrap();
    for body in [a, b] {
        assert_eq!(body.feature, base, "{gap:?}");
        assert!(doc.body_shape(body).is_some(), "{body} is not there");
    }
    assert_ne!(a, b);
    let renamed = gap.a.face.as_ref().unwrap();
    assert_eq!(renamed.feature, base, "{gap:?}");
    assert_ne!(*renamed, face);
    assert_eq!(
        gap.a.occurrence,
        Some(OccurrencePath(vec![OccurrenceUid(2)]))
    );
    assert_eq!(
        gap.b.as_ref().unwrap().occurrence,
        Some(OccurrencePath(vec![OccurrenceUid(2)])),
        "Pin's placement inside Box is gone"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every feature of the design builds.
fn all_ok(doc: &Document<MockKernel>) {
    for f in doc.features() {
        assert!(
            doc.status(f.uid).is_some_and(|s| s.is_ok()),
            "{}: {:?}",
            f.name,
            doc.status(f.uid)
        );
    }
}

/// An extrusion in the root up to `face` of `body`, read in the occurrence
/// `source` (`object_links`, mitcad#104): its uid.
fn extrude_up_to(
    doc: &mut Document<MockKernel>,
    body: BodyUid,
    face: &str,
    source: &str,
) -> FeatureUid {
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(2.0), &num(2.0))
        .unwrap()
        .region;
    let links = json!({body.to_string(): {"source": source}});
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "extrude",
               "profiles": [{"sketch": sketch, "region": region}],
               "extent": {"type": "to_object",
                          "object": {"type": "face", "body": body, "face": face}},
               "object_links": links, "operation": "new_body"}}),
    );
    let uid: FeatureUid = added["uid"].as_str().unwrap().parse().unwrap();
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    uid
}

/// Replace with a Link of Box with an extrusion of the root up to a face
/// of a body Box shows (mitcad#1144): the extrusion's link map is keyed by
/// the base feature's body that holds it now, its link's source ends at
/// Box:1, and every feature builds, also when the design is opened again.
fn replace_keeps_the_outer_link(source: &str, block: fn(&Design) -> (BodyUid, String), index: u32) {
    let dir = mitcad_model::testing::unique_temp_dir("1144-links");
    let design = design();
    let (body, face) = block(&design);
    let Design {
        mut doc, component, ..
    } = design;
    let extrusion = extrude_up_to(&mut doc, body, &face, source);

    let saved = command(
        &mut doc,
        json!({"cmd": "save_component_as_design", "component": "Box",
               "path": dir.join("Box.mitcad"), "base": dir, "replace": true}),
    );
    assert_eq!(saved["replaced"], true, "{saved}");
    let base: Vec<FeatureUid> = doc
        .features()
        .filter(|f| f.component == component)
        .map(|f| f.uid)
        .collect();
    assert_eq!(base.len(), 1);
    all_ok(&doc);
    let linked = BodyUid::new(base[0], index).to_string();
    let def = query(
        &doc,
        json!({"query": "feature", "uid": extrusion.to_string()}),
    );
    assert_eq!(
        def["def"]["object_links"],
        json!({linked.clone(): {"source": "O2"}}),
        "{def}"
    );
    assert_eq!(
        def["def"]["extent"]["object"]["body"],
        json!(linked),
        "{def}"
    );

    // The design as saved keeps the map: opened again, the extrusion builds.
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    let messages = reopened.update_links(Some(&dir)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    reopened.recompute();
    all_ok(&reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_with_a_link_rekeys_the_outer_body_links() {
    // Box's own block: the base feature's first body.
    replace_keeps_the_outer_link("O2", |d| d.box_block.clone(), 0);
}

#[test]
fn replace_with_a_link_ends_outer_links_through_it_at_its_occurrence() {
    // Pin's block, read through Box:1 in Pin:1: the base feature's second
    // body, read in Box:1.
    replace_keeps_the_outer_link("O2/O3", |d| d.pin_block.clone(), 1);
}

/// A construction plane of the root on Box's construction plane
/// (`datum_links`, mitcad#1035) would lose it, since a linked component
/// keeps no construction geometry: Replace with a Link is refused, the
/// plane named, before anything is written (mitcad#1144).
#[test]
fn replace_with_a_link_refuses_outer_datums_on_its_datums() {
    let dir = mitcad_model::testing::unique_temp_dir("1144-datums");
    let Design { mut doc, .. } = design();
    let plane = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                "definition": {"type": "offset", "plane": "xy", "distance": 20}})),
            None,
        )
        .unwrap()
        .uid;
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let outer = doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                "definition": {"type": "offset", "plane": plane, "distance": 10},
                "datum_links": {plane.to_string(): {"source": "O2"}}})),
            None,
        )
        .unwrap()
        .uid;
    let outer_name = doc.feature(outer).unwrap().name.clone();
    let path = dir.join("Box.mitcad");
    let error = doc
        .command(
            &json!({"cmd": "save_component_as_design", "component": "Box",
                    "path": path, "base": dir, "replace": true})
            .to_string(),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("cannot be replaced by a link") && error.contains(&outer_name),
        "{error}"
    );
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
