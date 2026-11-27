// SPDX-License-Identifier: MIT
//! A design of one component inserted as that component (mitcad#2429,
//! GitHub mitcad/Mitcad#618) with the mock kernel: a new design's shape
//! (an empty root and Component1 with a body) inserted as a copy or linked
//! comes in as one occurrence named after the design holding Component1's
//! content, placed where the insert places it composed with Component1's
//! placement in the design; the components and joints inside Component1
//! come along one level up. A design with root content or two components
//! keeps its root. A linked one edited into two components comes in with
//! its whole structure on the next update, with a line saying so, and the
//! parent's joints and picks on its parts follow them to the same parts
//! and geometry (mitcad#2441), a Pattern at Points of such a part too,
//! with the same copies at the same points (mitcad#2466), and decals on
//! their faces (mitcad#2527).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{allow_attachments, extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn run(doc: &mut Document<MockKernel>, cmd: Value) -> Value {
    let text = cmd.to_string();
    doc.command(&text)
        .map(|reply| serde_json::from_str(&reply).unwrap())
        .unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn assert_near(a: Vec3, b: Vec3, what: &str) {
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6),
        "{what}: {a:?} != {b:?}"
    );
}

/// A block of 10 × 10 × 5 at the origin in the active component.
fn block(doc: &mut Document<MockKernel>) {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    doc.add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap();
}

/// A new design (the root and Component1, active) with a block in
/// Component1, which the root places at (0, 0, 10).
fn one_component() -> Document<MockKernel> {
    let mut doc = Document::new_design(MockKernel::default());
    block(&mut doc);
    let only = doc
        .assembly()
        .children(ComponentUid::ROOT)
        .next()
        .unwrap()
        .uid;
    doc.set_occurrence_transform(only, Transform::translation([0.0, 0.0, 10.0]), false)
        .unwrap();
    doc
}

fn write_part(dir: &Path, doc: &Document<MockKernel>) {
    std::fs::write(dir.join("part.mitcad"), doc.to_json()).unwrap();
}

fn saved(name: &str, doc: &Document<MockKernel>) -> PathBuf {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    write_part(&dir, doc);
    dir
}

fn base(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// `part.mitcad` inserted at (5, 0, 0), linked or as a copy: its
/// component and occurrence.
fn insert(doc: &mut Document<MockKernel>, dir: &Path, link: bool) -> (ComponentUid, OccurrenceUid) {
    let reply = run(
        doc,
        json!({"cmd": "insert_component", "path": "part.mitcad", "base": base(dir),
               "link": link,
               "transform": {"translation": [5.0, 0.0, 0.0]}}),
    );
    (
        reply["component"].as_str().unwrap().parse().unwrap(),
        reply["occurrence"].as_str().unwrap().parse().unwrap(),
    )
}

/// The names of the occurrences placed in a component.
fn children(doc: &Document<MockKernel>, component: ComponentUid) -> Vec<String> {
    let a = doc.assembly();
    a.children(component)
        .map(|o| a.occurrence_name(o.uid))
        .collect()
}

/// Where an occurrence's origin is in its parent.
fn origin_of(doc: &Document<MockKernel>, occurrence: OccurrenceUid) -> Vec3 {
    doc.assembly()
        .occurrence(occurrence)
        .unwrap()
        .transform
        .apply_point([0.0; 3])
}

#[test]
fn a_new_design_inserted_comes_in_as_its_one_component() {
    let dir = saved("2429-one", &one_component());
    for link in [true, false] {
        let mut doc = Document::new(MockKernel::default());
        let (made, occurrence) = insert(&mut doc, &dir, link);
        let a = doc.assembly();
        // One level: the occurrence named after the design holds
        // Component1's body, nothing placed in it.
        assert_eq!(a.name(made), "part", "link {link}");
        assert_eq!(a.occurrence_name(occurrence), "part:1", "link {link}");
        assert_eq!(a.occurrence(occurrence).unwrap().parent, ComponentUid::ROOT);
        assert!(
            children(&doc, made).is_empty(),
            "link {link}: {:?}",
            children(&doc, made)
        );
        assert_eq!(a.components.len(), 1, "link {link}: no Component1 here");
        assert_eq!(doc.component_bodies(made).len(), 1, "link {link}");
        // The insert's (5, 0, 0) composed with Component1's (0, 0, 10).
        assert_near(
            origin_of(&doc, occurrence),
            [5.0, 0.0, 10.0],
            &format!("link {link}"),
        );
        let link_record = a.component(made).unwrap().link.clone();
        if link {
            let tree = link_record.unwrap().tree.expect("kept as a tree");
            let top = tree.top.expect("standing for Component1");
            assert_near(
                top.placement.apply_point([0.0; 3]),
                [0.0, 0.0, 10.0],
                "the recorded placement",
            );
        } else {
            assert!(link_record.is_none());
            assert!(
                doc.features().any(|f| f.component == made),
                "the copy holds Component1's features"
            );
        }
    }
}

/// A new design whose Component1 holds Base:1 (grounded) and Key:1 at
/// (0, 0, 10), each with a block, and Joint1 of Component1 sliding the Key
/// along x on the Base.
fn one_component_with_parts() -> Document<MockKernel> {
    let mut doc = Document::new_design(MockKernel::default());
    let top = doc.assembly().active;
    let (_, base_part) = doc
        .create_component(Some("Base"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(top).unwrap();
    doc.set_occurrence_grounded(base_part, true).unwrap();
    let (_, key) = doc
        .create_component(Some("Key"), Transform::translation([0.0, 0.0, 10.0]), true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(top).unwrap();
    let joint = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
               "a": {"occurrence": key.to_string(), "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": base_part.to_string(), "geometry": {"point": [0, 0, 10]}},
               "limits": {"tx": {"min": -40, "max": 40}}}),
    );
    assert_eq!(joint["solved"], true, "{joint}");
    doc
}

#[test]
fn the_components_and_joints_of_the_one_component_come_one_level_up() {
    let dir = saved("2429-parts", &one_component_with_parts());
    for link in [true, false] {
        let mut doc = Document::new(MockKernel::default());
        let (made, occurrence) = insert(&mut doc, &dir, link);
        assert_eq!(
            children(&doc, made),
            ["Base:1", "Key:1"],
            "link {link}: the parts placed in the inserted component"
        );
        assert!(doc.component_bodies(made).is_empty(), "link {link}");
        assert_near(
            origin_of(&doc, occurrence),
            [5.0, 0.0, 0.0],
            &format!("link {link}"),
        );
        // Joint1 belongs to the inserted component, its paths without the
        // wrapper level, and holds.
        let joints: Vec<_> = doc
            .features()
            .filter(|f| f.component == made && matches!(f.def, FeatureDef::Joint(_)))
            .map(|f| f.uid)
            .collect();
        assert_eq!(joints.len(), 1, "link {link}");
        assert!(
            doc.status(joints[0]).unwrap().is_ok(),
            "link {link}: {:?}",
            doc.status(joints[0])
        );
    }
}

#[test]
fn a_design_with_root_content_or_two_components_keeps_its_root() {
    // A body in the root beside Component1.
    let mut root_content = Document::new_design(MockKernel::default());
    block(&mut root_content);
    root_content.activate_component(ComponentUid::ROOT).unwrap();
    block(&mut root_content);
    // A second component beside Component1.
    let mut two = Document::new_design(MockKernel::default());
    block(&mut two);
    two.activate_component(ComponentUid::ROOT).unwrap();
    two.create_component(Some("Second"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut two);

    for (name, source, placed, own) in [
        ("2429-root", &root_content, vec!["Component1:1"], 1),
        ("2429-two", &two, vec!["Component1:1", "Second:1"], 0),
    ] {
        let dir = saved(name, source);
        for link in [true, false] {
            let mut doc = Document::new(MockKernel::default());
            let (made, occurrence) = insert(&mut doc, &dir, link);
            assert_eq!(children(&doc, made), placed, "{name}, link {link}");
            assert_eq!(doc.component_bodies(made).len(), own, "{name}, link {link}");
            assert_near(
                origin_of(&doc, occurrence),
                [5.0, 0.0, 0.0],
                &format!("{name}, link {link}"),
            );
            if link {
                let link = doc.assembly().component(made).unwrap().link.clone();
                assert!(link.unwrap().tree.unwrap().top.is_none(), "{name}");
            }
        }
    }
}

#[test]
fn a_linked_one_component_design_edited_into_two_comes_in_whole_on_update() {
    let mut source = one_component();
    let dir = saved("2429-update", &source);
    let mut doc = Document::new(MockKernel::default());
    let (made, occurrence) = insert(&mut doc, &dir, true);
    assert!(children(&doc, made).is_empty());

    // Still one component, with a second body: read again as it is.
    block(&mut source);
    write_part(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "part", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(!messages.contains("changed its structure"), "{messages}");
    assert!(children(&doc, made).is_empty());
    assert_eq!(doc.component_bodies(made).len(), 2);
    assert_near(origin_of(&doc, occurrence), [5.0, 0.0, 10.0], "kept");

    // A second component in the root: the whole structure comes back.
    source.activate_component(ComponentUid::ROOT).unwrap();
    source
        .create_component(Some("Second"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    write_part(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "part", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(
        messages.contains(
            "part changed its structure: part.mitcad now holds more than its one component"
        ),
        "{messages}"
    );
    assert_eq!(children(&doc, made), ["Component1:1", "Second:1"]);
    assert!(
        doc.component_bodies(made).is_empty(),
        "the root has no body"
    );
    let link = doc
        .assembly()
        .component(made)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert!(link.tree.unwrap().top.is_none(), "linked with its root now");
    // The occurrence stands for the root again: Component1's body stays
    // where it was.
    assert_near(origin_of(&doc, occurrence), [5.0, 0.0, 0.0], "the root");
    let first = doc.assembly().children(made).next().map(|o| o.uid).unwrap();
    assert_eq!(
        doc.component_bodies(doc.assembly().occurrence(first).unwrap().component)
            .len(),
        2
    );
    assert_near(
        doc.path_transform(&[occurrence, first])
            .apply_point([0.0; 3]),
        [5.0, 0.0, 10.0],
        "Component1",
    );
}

/// A new design whose Component1, placed at (0, 0, 10) in the root, holds
/// a block of its own and Key:1 at (0, 0, 10) with a block; its
/// components take attachments from others.
fn one_component_with_key() -> Document<MockKernel> {
    let mut doc = Document::new_design(MockKernel::default());
    let top = doc.assembly().active;
    block(&mut doc);
    doc.create_component(Some("Key"), Transform::translation([0.0, 0.0, 10.0]), true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(top).unwrap();
    let only = doc
        .assembly()
        .children(ComponentUid::ROOT)
        .next()
        .unwrap()
        .uid;
    doc.set_occurrence_transform(only, Transform::translation([0.0, 0.0, 10.0]), false)
        .unwrap();
    allow_attachments(&mut doc);
    doc
}

/// The occurrence named `name` (`Key:1`).
fn named(doc: &Document<MockKernel>, name: &str) -> OccurrenceUid {
    let a = doc.assembly();
    a.occurrences
        .iter()
        .find(|o| a.occurrence_name(o.uid) == name)
        .unwrap_or_else(|| panic!("no occurrence {name}"))
        .uid
}

/// A face of a linked body, made a plane at the body's top (z = 5 in its
/// component) for the mock kernel.
fn top_face(doc: &Document<MockKernel>, body: BodyUid) -> String {
    use mitcad_model::datum::SurfaceGeometry;
    let face = doc.body_shape(body).unwrap().faces[0].to_string();
    doc.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        SurfaceGeometry::Plane {
            origin: [5.0, 5.0, 5.0],
            normal: [0.0, 0.0, 1.0],
        },
    );
    face
}

/// A sketch of the root on `face` of `body`, seen through `path`.
fn sketch_on(doc: &mut Document<MockKernel>, body: BodyUid, face: &str, path: &str) -> FeatureUid {
    let reply = run(
        doc,
        json!({"cmd": "sketch.create", "plane": {"face": face, "body": body},
               "occurrence": path}),
    );
    reply["uid"].as_str().unwrap().parse().unwrap()
}

/// A sketch's frame in the design.
fn frame(doc: &Document<MockKernel>, sketch: FeatureUid) -> Value {
    let reply = doc
        .query(&json!({"query": "sketch", "uid": sketch.to_string()}).to_string())
        .unwrap();
    let frame = serde_json::from_str::<Value>(&reply).unwrap()["frame"].clone();
    assert!(frame.is_object(), "{sketch}: {:?}", doc.status(sketch));
    frame
}

/// Two JSON values alike up to rounding of their numbers.
fn assert_alike(a: &Value, b: &Value, what: &str) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() < 1e-6, "{what}: {a} != {b}");
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{what}: {a} != {b}");
            for (x, y) in x.iter().zip(y) {
                assert_alike(x, y, what);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            assert_eq!(
                x.keys().collect::<Vec<_>>(),
                y.keys().collect::<Vec<_>>(),
                "{what}"
            );
            for (key, x) in x {
                assert_alike(x, &y[key], what);
            }
        }
        _ => assert_eq!(a, b, "{what}"),
    }
}

/// Where an occurrence of the root is placed now.
fn placed_at(doc: &Document<MockKernel>, occurrence: OccurrenceUid) -> Vec3 {
    doc.placement(occurrence).unwrap().apply_point([0.0; 3])
}

#[test]
fn the_parent_s_joints_and_picks_follow_when_the_link_comes_back_whole() {
    let mut source = one_component_with_key();
    let dir = saved("2441-follow", &source);
    let mut doc = Document::new(MockKernel::default());
    allow_attachments(&mut doc);
    let (made, occurrence) = insert(&mut doc, &dir, true);
    doc.set_occurrence_grounded(occurrence, true).unwrap();
    let key = named(&doc, "Key:1");
    let key_component = doc.assembly().occurrence(key).unwrap().component;
    let own_body = doc.component_bodies(made)[0];
    let key_body = doc.component_bodies(key_component)[0];

    // Plate:1's origin held on the Key's (a part inside the link), Lid:1's
    // on a point of the link's own (Component1's coordinates).
    let mut parts = Vec::new();
    for name in ["Plate", "Lid"] {
        let (_, part) = doc
            .create_component(Some(name), Transform::translation([30.0, 0.0, 0.0]), true)
            .unwrap();
        block(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        parts.push(part);
    }
    let (plate, lid) = (parts[0], parts[1]);
    let mut joints = Vec::new();
    for (part, path, point) in [
        (plate, format!("{occurrence}/{key}"), [0.0, 0.0, 0.0]),
        (lid, occurrence.to_string(), [1.0, 2.0, 3.0]),
    ] {
        let reply = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "ball",
                   "a": {"occurrence": part.to_string(), "geometry": {"point": [0, 0, 0]}},
                   "b": {"occurrence": path, "geometry": {"point": point}}}),
        );
        assert_eq!(reply["solved"], true, "{reply}");
        joints.push(
            reply["uid"]
                .as_str()
                .unwrap()
                .parse::<FeatureUid>()
                .unwrap(),
        );
    }
    // The Key at (5, 0, 0) + (0, 0, 10) + (0, 0, 10); the link's own point
    // through its occurrence at (5, 0, 10).
    assert_near(placed_at(&doc, plate), [5.0, 0.0, 20.0], "Plate:1");
    assert_near(placed_at(&doc, lid), [6.0, 2.0, 13.0], "Lid:1");
    // Sketches of the root on a face of the link's own body and on one of
    // the Key's.
    let own_face = top_face(&doc, own_body);
    let key_face = top_face(&doc, key_body);
    let on_own = sketch_on(&mut doc, own_body, &own_face, &occurrence.to_string());
    let on_key = sketch_on(
        &mut doc,
        key_body,
        &key_face,
        &format!("{occurrence}/{key}"),
    );
    let frames = [frame(&doc, on_own), frame(&doc, on_key)];

    // A second component in the design's root: the whole structure comes
    // in, and nothing needs to be picked again.
    source.activate_component(ComponentUid::ROOT).unwrap();
    source
        .create_component(Some("Second"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    write_part(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "part", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(
        messages.contains(
            "part changed its structure: part.mitcad now holds more than its one component, so \
             it comes in with its root and components as the file has them; the references to \
             its parts follow them"
        ),
        "{messages}"
    );
    assert!(!messages.contains("picked again"), "{messages}");
    assert_eq!(children(&doc, made), ["Component1:1", "Second:1"]);
    let first = named(&doc, "Component1:1");
    let first_component = doc.assembly().occurrence(first).unwrap().component;
    // The Key and the link's former own body are the same, one level down.
    assert_eq!(
        doc.assembly().occurrence(key).unwrap().parent,
        first_component
    );
    assert_eq!(doc.component_bodies(first_component), [own_body]);
    assert_eq!(doc.component_bodies(key_component), [key_body]);

    // The joints name them through Component1:1 and hold them where they
    // were.
    let sides = |uid: FeatureUid| match &doc.feature(uid).unwrap().def {
        FeatureDef::Joint(j) => j.b.occurrence.0.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(sides(joints[0]), [occurrence, first, key]);
    assert_eq!(sides(joints[1]), [occurrence, first]);
    for uid in &joints {
        assert!(
            doc.status(*uid).is_some_and(|s| s.is_ok()),
            "{uid}: {:?}",
            doc.status(*uid)
        );
    }
    assert_near(placed_at(&doc, plate), [5.0, 0.0, 20.0], "Plate:1 after");
    assert_near(placed_at(&doc, lid), [6.0, 2.0, 13.0], "Lid:1 after");
    // The sketches' planes are the same faces, where they were.
    for (sketch, path) in [
        (on_own, vec![occurrence, first]),
        (on_key, vec![occurrence, first, key]),
    ] {
        let link = match &doc.feature(sketch).unwrap().def {
            FeatureDef::Sketch(s) => s.plane_link.clone().expect("linked"),
            other => panic!("{other:?}"),
        };
        assert_eq!(link.source.0, path, "{sketch}");
        assert!(
            doc.status(sketch).is_some_and(|s| s.is_ok()),
            "{sketch}: {:?}",
            doc.status(sketch)
        );
    }
    assert_alike(
        &frame(&doc, on_own),
        &frames[0],
        "the sketch on the link's body",
    );
    assert_alike(&frame(&doc, on_key), &frames[1], "the sketch on the Key");
}

/// The occurrences Pattern at Points `pattern` made, with where they are.
fn copies(doc: &Document<MockKernel>, pattern: FeatureUid) -> Vec<(OccurrenceUid, Vec3)> {
    doc.assembly()
        .occurrences
        .iter()
        .filter(|o| o.pattern.as_ref().is_some_and(|p| p.feature == pattern))
        .map(|o| (o.uid, placed_at(doc, o.uid)))
        .collect()
}

#[test]
fn a_pattern_at_points_on_a_part_inside_follows_when_the_link_comes_back_whole() {
    let mut source = one_component_with_key();
    let dir = saved("2466-pattern", &source);
    let mut doc = Document::new(MockKernel::default());
    allow_attachments(&mut doc);
    let (made, occurrence) = insert(&mut doc, &dir, true);
    let key = named(&doc, "Key:1");
    let on_key = format!("{occurrence}/{key}");

    // Copies of the Key (a part inside the link, its origin at (5, 0, 20))
    // at a point of the root, at a point of the Key's own and at one of the
    // link's own (Component1's coordinates).
    let reply = run(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "pattern_at_points",
        "objects": {"type": "components", "occurrences": [on_key]},
        "targets": [
            {"point": {"point": [40.0, 0.0, 0.0]}},
            {"point": {"point": [0.0, 10.0, 0.0]}, "occurrence": on_key},
            {"point": {"point": [1.0, 2.0, 3.0]}, "occurrence": occurrence.to_string()}
        ]}}),
    );
    let pattern: FeatureUid = reply["uid"].as_str().unwrap().parse().unwrap();
    assert!(
        doc.status(pattern).is_some_and(|s| s.is_ok()),
        "{:?}",
        doc.status(pattern)
    );
    let before = copies(&doc, pattern);
    assert_eq!(before.len(), 3, "{before:?}");
    for ((_, at), want) in
        before
            .iter()
            .zip([[40.0, 0.0, 0.0], [5.0, 10.0, 20.0], [6.0, 2.0, 13.0]])
    {
        assert_near(*at, want, "a copy");
    }

    // A second component in the design's root: the whole structure comes
    // in, and nothing needs to be picked again.
    source.activate_component(ComponentUid::ROOT).unwrap();
    source
        .create_component(Some("Second"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    write_part(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "part", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(
        messages.contains(
            "part changed its structure: part.mitcad now holds more than its one component, so \
             it comes in with its root and components as the file has them; the references to \
             its parts follow them\""
        ),
        "{messages}"
    );
    assert!(!messages.contains("picked again"), "{messages}");
    assert!(!messages.contains("Pattern at Points"), "{messages}");
    assert_eq!(children(&doc, made), ["Component1:1", "Second:1"]);
    let first = named(&doc, "Component1:1");

    // The pattern names the Key and the link's own point through
    // Component1:1, and places the same copies at the same points.
    let FeatureDef::PatternAtPoints(def) = &doc.feature(pattern).unwrap().def else {
        panic!("not a pattern");
    };
    let through_first = vec![occurrence, first, key];
    match &def.objects {
        mitcad_model::features::PatternObjects::Components { occurrences } => {
            assert_eq!(occurrences.len(), 1);
            assert_eq!(occurrences[0].0, through_first);
        }
        other => panic!("{other:?}"),
    }
    let paths: Vec<Option<Vec<OccurrenceUid>>> = def
        .targets
        .iter()
        .map(|t| t.occurrence.as_ref().map(|p| p.0.clone()))
        .collect();
    assert_eq!(
        paths,
        [None, Some(through_first), Some(vec![occurrence, first])]
    );
    assert!(
        doc.status(pattern).is_some_and(|s| s.is_ok()),
        "{:?}",
        doc.status(pattern)
    );
    let after = copies(&doc, pattern);
    assert_eq!(after.len(), before.len(), "{after:?}");
    for ((uid, at), (was, was_at)) in after.iter().zip(&before) {
        assert_eq!(uid, was, "the same copy");
        assert_near(*at, *was_at, "the copy after the update");
    }
}

/// A 24-bit BMP file of 4 x 2 grey pixels, base64.
fn bmp_data() -> String {
    let (width, height) = (4u32, 2u32);
    let row = (width * 3).div_ceil(4) * 4;
    let pixels = row * height;
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&(54 + pixels).to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&pixels.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&4000u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.resize(bytes.len() + pixels as usize, 0x80);
    mitcad_model::internal::base64::encode(&bytes)
}

/// The `decals` query.
fn decals(doc: &Document<MockKernel>) -> Vec<Value> {
    let reply = doc.query(&json!({"query": "decals"}).to_string()).unwrap();
    serde_json::from_str::<Value>(&reply)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

/// mitcad#2527: decals of the parent on a face of the link's own body and
/// on one of a part inside it follow when the link comes back whole, as
/// the sketches on those faces do: their paths name the parts through
/// Component1:1, and they lie where they lay.
#[test]
fn decals_on_the_link_s_parts_follow_when_the_link_comes_back_whole() {
    let mut source = one_component_with_key();
    let dir = saved("2527-decals", &source);
    let mut doc = Document::new(MockKernel::default());
    let (made, occurrence) = insert(&mut doc, &dir, true);
    let key = named(&doc, "Key:1");
    let key_component = doc.assembly().occurrence(key).unwrap().component;
    let own_body = doc.component_bodies(made)[0];
    let key_body = doc.component_bodies(key_component)[0];
    let own_face = top_face(&doc, own_body);
    let key_face = top_face(&doc, key_body);
    let through_key = format!("{occurrence}/{key}");
    for (name, body, face, path) in [
        ("Own", own_body, &own_face, occurrence.to_string()),
        ("Key", key_body, &key_face, through_key.clone()),
    ] {
        run(
            &mut doc,
            json!({"cmd": "insert_decal", "data": bmp_data(), "name": name,
                   "face": {"body": body, "face": face}, "occurrence": path,
                   "center": [5, 5], "width": 4}),
        );
    }
    let before = decals(&doc);
    assert_eq!(before.len(), 2);
    for decal in &before {
        assert!(decal.get("error").is_none(), "{decal}");
    }
    assert_eq!(before[0]["occurrence"], json!(occurrence.to_string()));
    assert_eq!(before[1]["occurrence"], json!(through_key));

    // A second component in the design's root: the whole structure comes
    // in (`open_top`).
    source.activate_component(ComponentUid::ROOT).unwrap();
    source
        .create_component(Some("Second"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    write_part(&dir, &source);
    run(
        &mut doc,
        json!({"cmd": "update_link", "component": "part", "base": base(&dir)}),
    );
    let first = named(&doc, "Component1:1");
    let after = decals(&doc);
    assert_eq!(after.len(), 2);
    assert_eq!(
        after[0]["occurrence"],
        json!(format!("{occurrence}/{first}")),
        "{}",
        after[0]
    );
    assert_eq!(
        after[1]["occurrence"],
        json!(format!("{occurrence}/{first}/{key}")),
        "{}",
        after[1]
    );
    for (decal, was) in after.iter().zip(&before) {
        assert!(decal.get("error").is_none(), "{decal}");
        assert_alike(&decal["corners"], &was["corners"], "the decal's corners");
    }
}
