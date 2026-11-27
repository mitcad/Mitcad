// SPDX-License-Identifier: MIT
//! A whole design linked with its components (mitcad#2209) with the mock
//! kernel: a design whose Key slides along x on its Base (a slider joint
//! of the design's root, Base:1 grounded) inserted linked comes in with
//! its two components, their bodies, its joint and its grounding, read
//! only; its occurrence can be made Flexible, and the slider is then
//! driven for it alone, also with a joint of the parent on the Key;
//! reading the file again keeps the uids, the occurrence's own value
//! within the joint's new limits, and a part the parent's joint uses when
//! the file no longer has it; a joint gone drops the occurrence's own
//! value with a message. A design without components, and a link saved in
//! the flat form, stay as they were. Through the application:
//! `tools/ui-linked-flexible-test.sh` and `app.ui-windows-linked-flexible`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::joints::Motion;
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
    doc.command(&command.to_string())
        .map(|text| serde_json::from_str(&text).unwrap())
        .map_err(|e| e.to_string())
}

fn run(doc: &mut Document<MockKernel>, cmd: Value) -> Value {
    let text = cmd.to_string();
    command(doc, cmd).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn assert_near(a: Vec3, b: Vec3, what: &str) {
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6),
        "{what}: {a:?} != {b:?}"
    );
}

/// A block of 10 × 10 × 5 at the origin in the active component.
fn block(doc: &mut Document<MockKernel>) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// The design to link: Base:1 at the origin, grounded, and Key:1 at
/// (0, 0, 10), each with a block; Joint1, the root's, slides the Key along
/// x on the Base, within ±`limit` mm. With `key` false the Key and the
/// joint are left out.
fn slider(limit: f64, key: bool) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let (_, base) = doc
        .create_component(Some("Base"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(base, true).unwrap();
    if key {
        doc.create_component(Some("Key"), Transform::translation([0.0, 0.0, 10.0]), true)
            .unwrap();
        block(&mut doc);
        doc.activate_component(ComponentUid::ROOT).unwrap();
        let joint = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
                   "a": {"occurrence": "Key:1", "geometry": {"point": [0, 0, 0]}},
                   "b": {"occurrence": "Base:1", "geometry": {"point": [0, 0, 10]}},
                   "limits": {"tx": {"min": -limit, "max": limit}}}),
        );
        assert_eq!(joint["solved"], true, "{joint}");
    }
    doc
}

/// The design written to `slider.mitcad` in a folder of its own.
fn saved(name: &str, limit: f64) -> PathBuf {
    let dir = mitcad_model::testing::unique_temp_dir(name);
    write_source(&dir, &slider(limit, true));
    dir
}

fn write_source(dir: &Path, doc: &Document<MockKernel>) {
    std::fs::write(dir.join("slider.mitcad"), doc.to_json()).unwrap();
}

fn base(dir: &Path) -> String {
    dir.to_string_lossy().into_owned()
}

/// The design inserted linked, whole, as `Slider`: its component and
/// occurrence.
fn insert(doc: &mut Document<MockKernel>, dir: &Path) -> (ComponentUid, OccurrenceUid) {
    let reply = run(
        doc,
        json!({"cmd": "insert_component", "path": "slider.mitcad", "base": base(dir),
               "name": "Slider"}),
    );
    (
        reply["component"].as_str().unwrap().parse().unwrap(),
        reply["occurrence"].as_str().unwrap().parse().unwrap(),
    )
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

/// The linked component's joint made for the design's Joint1.
fn joint(doc: &Document<MockKernel>, linked: ComponentUid) -> FeatureUid {
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .clone()
        .unwrap();
    let tree = link.tree.expect("linked with its components");
    assert_eq!(tree.features.len(), 1, "{tree:?}");
    tree.features[0].feature
}

/// Where the Key's origin is in the design through the linked occurrence.
fn key_at(doc: &Document<MockKernel>, linked: OccurrenceUid) -> Vec3 {
    let key = named(doc, "Key:1");
    doc.path_transform(&[linked, key]).apply_point([0.0; 3])
}

/// The flexible occurrence's own value of a joint's motion tx, if it has
/// one.
fn own_tx(doc: &Document<MockKernel>, occurrence: OccurrenceUid, joint: FeatureUid) -> Option<f64> {
    let flex = doc
        .assembly()
        .occurrence(occurrence)
        .unwrap()
        .flexible
        .clone()?;
    flex.position.get(&joint)?.get(&Motion::Tx).copied()
}

#[test]
fn a_whole_design_comes_in_with_its_components_joint_and_grounding_read_only() {
    let dir = saved("2209-insert", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, occurrence) = insert(&mut doc, &dir);

    // The two components, placed in the linked one as the design places
    // them, each with its own body; the design's root has none.
    let a = doc.assembly();
    let children: Vec<String> = a
        .children(linked)
        .map(|o| a.occurrence_name(o.uid))
        .collect();
    assert_eq!(children, ["Base:1", "Key:1"]);
    assert!(doc.component_bodies(linked).is_empty());
    let base_part = a.occurrence(named(&doc, "Base:1")).unwrap().clone();
    let key = a.occurrence(named(&doc, "Key:1")).unwrap().clone();
    assert!(base_part.grounded, "grounded as the design grounds it");
    assert!(!key.grounded);
    assert_eq!(doc.component_bodies(base_part.component).len(), 1);
    assert_eq!(doc.component_bodies(key.component).len(), 1);
    assert_near(key_at(&doc, occurrence), [0.0, 0.0, 10.0], "the Key");
    assert_eq!(a.linked_in(key.component), Some(linked));

    // The design's joint, of the linked component, holding.
    let uid = joint(&doc, linked);
    let entry = doc.feature(uid).unwrap();
    assert_eq!(entry.component, linked);
    assert_eq!(entry.name, "Joint1");
    assert!(matches!(entry.def, FeatureDef::Joint(_)));
    let joints = query(&doc, json!({"query": "joints"}));
    let listed = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["uid"] == json!(uid.to_string()))
        .cloned()
        .unwrap_or_else(|| panic!("{joints}"));
    assert_eq!(listed["solved"], true, "{listed}");

    // The components query says so, for the Browser.
    let tree = query(&doc, json!({"query": "components"}));
    let listed: Vec<&Value> = tree["components"].as_array().unwrap().iter().collect();
    let row = |uid: ComponentUid| {
        listed
            .iter()
            .find(|c| c["uid"] == json!(uid.to_string()))
            .copied()
            .unwrap()
    };
    assert_eq!(row(linked)["link_tree"], true, "{tree}");
    assert_eq!(row(key.component)["linked_in"], json!(linked.to_string()));

    // Read only: the parts, their placements, grounding and the joint.
    let refused = doc
        .activate_component(key.component)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("is part of Slider"), "{refused}");
    let refused = doc
        .set_occurrence_transform(key.uid, Transform::translation([5.0, 0.0, 10.0]), false)
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("Slider is linked from slider.mitcad"),
        "{refused}"
    );
    let refused = doc
        .set_occurrence_grounded(key.uid, true)
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("Slider is linked from slider.mitcad"),
        "{refused}"
    );
    let refused = command(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "values": {"tx": 20}}),
    )
    .unwrap_err();
    assert!(refused.contains("linked from"), "{refused}");
    // Saved and opened: the same.
    let text = doc.to_json();
    let path = dir.join("holder.mitcad");
    std::fs::write(&path, &text).unwrap();
    let mut opened = Document::from_json_at(&text, &path, MockKernel::default()).unwrap();
    let messages = opened.update_links(Some(&dir)).unwrap();
    assert!(messages.is_empty(), "{messages:?}");
    let tree = |d: &Document<MockKernel>| d.assembly().component(linked).unwrap().link.clone();
    assert_eq!(tree(&opened), tree(&doc));
    assert_eq!(opened.assembly().occurrences, doc.assembly().occurrences);
}

#[test]
fn choosing_one_linked_part_prunes_the_tree_and_reports_the_dropped_joint() {
    let dir = saved("2252-parts", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, first) = insert(&mut doc, &dir);
    let joint = joint(&doc, linked);
    let key = named(&doc, "Key:1");
    let reply = run(
        &mut doc,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
            "parts": {"occurrences": ["Key:1"]}
        }),
    );
    assert_eq!(reply["changed"], true, "{reply}");
    assert!(
        reply["messages"].to_string().contains("Joint1 was dropped"),
        "{reply}"
    );
    assert!(doc.feature(joint).is_none());
    let parts: Vec<_> = doc.instances().into_iter().map(|item| item.path).collect();
    assert_eq!(parts, vec![vec![first, key]]);
    assert_eq!(doc.undo_label(), Some("Choose Linked Parts of Slider"));

    let mut opened = Document::from_json_at(
        &doc.to_json(),
        &dir.join("holder.mitcad"),
        MockKernel::default(),
    )
    .unwrap();
    write_source(&dir, &slider(25.0, true));
    opened.update_links(Some(&dir)).unwrap();
    assert_eq!(opened.instances()[0].path, vec![first, key]);
    assert!(opened.feature(joint).is_none());
    let reply = run(
        &mut opened,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir)
        }),
    );
    assert_eq!(reply["changed"], true, "{reply}");
    assert_eq!(opened.instances().len(), 2);
    assert_eq!(named(&opened, "Key:1"), key);
    assert_eq!(
        query(&opened, json!({"query": "joints"}))["joints"][0]["solved"],
        true
    );
}

#[test]
fn a_tree_envelope_preserves_flexible_values_and_the_other_full_occurrence() {
    let dir = saved("2252-envelope", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, first) = insert(&mut doc, &dir);
    let second = doc
        .copy_occurrence(first, Some(Transform::translation([200.0, 0.0, 0.0])))
        .unwrap();
    let uid = joint(&doc, linked);
    let key = named(&doc, "Key:1");
    doc.set_occurrence_flexible(first, true).unwrap();
    run(
        &mut doc,
        json!({
            "cmd": "drive_joint", "joint": "Joint1", "occurrence": first.to_string(),
            "values": {"tx": 20}
        }),
    );
    let show = json!({
        "cmd": "set_link_envelope", "occurrence": first.to_string(),
        "detail": 5, "base": base(&dir)
    });
    let reply = run(&mut doc, show.clone());
    assert_eq!(reply["envelope"], true, "{reply}");
    assert_eq!(reply["bodies"].as_array().unwrap().len(), 1, "{reply}");
    let no_change = run(&mut doc, show);
    assert_eq!(no_change["changed"], false, "{no_change}");
    assert_eq!(
        doc.instances().iter().filter(|i| i.path == [first]).count(),
        1
    );
    assert_eq!(
        doc.instances()
            .iter()
            .filter(|i| i.path.first() == Some(&second))
            .count(),
        2
    );
    assert_eq!(own_tx(&doc, first, uid), Some(20.0));

    let mut opened = Document::from_json_at(
        &doc.to_json(),
        &dir.join("holder.mitcad"),
        MockKernel::default(),
    )
    .unwrap();
    write_source(&dir, &slider(30.0, true));
    opened.update_links(Some(&dir)).unwrap();
    assert_eq!(
        opened
            .instances()
            .iter()
            .filter(|i| i.path == [first])
            .count(),
        1
    );
    let reply = run(
        &mut opened,
        json!({
            "cmd": "set_link_envelope", "occurrence": first.to_string(), "envelope": false
        }),
    );
    assert_eq!(reply["envelope"], false, "{reply}");
    assert_eq!(opened.instances().len(), 4);
    assert!(opened.is_flexible(first));
    assert_eq!(joint(&opened, linked), uid);
    assert_eq!(named(&opened, "Key:1"), key);
    assert_eq!(own_tx(&opened, first, uid), Some(20.0));
    assert_near(key_at(&opened, first), [20.0, 0.0, 10.0], "reopened key");
    assert_near(key_at(&opened, second), [200.0, 0.0, 10.0], "other key");
    run(
        &mut opened,
        json!({"cmd": "set_link_envelope", "occurrence": second.to_string(),
               "all": true, "detail": 5, "base": base(&dir)}),
    );
    let displayed = opened.instances();
    assert_eq!(displayed.len(), 2);
    assert_eq!(displayed[0].body, displayed[1].body);
    assert_eq!(displayed[0].path.len(), 1);
    assert_eq!(displayed[1].path.len(), 1);
    run(
        &mut opened,
        json!({"cmd": "set_link_envelope", "occurrence": first.to_string(),
               "envelope": false}),
    );
    assert_eq!(opened.instances().len(), 3);
    assert_eq!(own_tx(&opened, first, uid), Some(20.0));
}

#[test]
fn choosing_children_keeps_a_parent_joints_frame_on_an_unticked_root_body() {
    let dir = saved("2252-root-attachment", 40.0);
    let mut source = slider(40.0, true);
    block(&mut source);
    write_source(&dir, &source);
    let mut doc = Document::new(MockKernel::default());
    let (linked, occurrence) = insert(&mut doc, &dir);
    let body = doc.component_bodies(linked)[0];
    let face = doc.body_shape(body).unwrap().faces[0].to_string();
    doc.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        mitcad_model::datum::SurfaceGeometry::Plane {
            origin: [5.0, 5.0, 0.0],
            normal: [0.0, 0.0, -1.0],
        },
    );
    let joint = run(
        &mut doc,
        json!({
            "cmd": "add_joint", "kind": "rigid",
            "a": {"occurrence": occurrence.to_string(), "geometry": {"body": body, "face": face}},
            "b": {"geometry": {"point": [5, 5, 0]}}
        }),
    );
    let uid: FeatureUid = joint["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(joint["solved"], true, "{joint}");
    run(
        &mut doc,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
            "parts": {"occurrences": ["Key:1"]}
        }),
    );
    assert!(doc.component_bodies(linked).is_empty());
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .as_ref()
        .unwrap();
    assert_eq!(link.unlinked.len(), 1);
    let results = query(&doc, json!({"query": "joints"}));
    assert_eq!(results["joints"][0]["uid"], uid.to_string());
    assert_eq!(results["joints"][0]["solved"], true, "{results}");
}

#[test]
fn repeated_placements_can_select_different_nested_parts_without_losing_a_parent_joint() {
    let dir = saved("2252-repeated-parts", 40.0);
    let mut source = Document::new(MockKernel::default());
    let (subassembly, first_source) = source
        .create_component(Some("Subassembly"), Transform::IDENTITY, true)
        .unwrap();
    source
        .create_component(Some("Left"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    source.activate_component(subassembly).unwrap();
    source
        .create_component(
            Some("Right"),
            Transform::translation([25.0, 0.0, 0.0]),
            true,
        )
        .unwrap();
    block(&mut source);
    source.activate_component(ComponentUid::ROOT).unwrap();
    source
        .copy_occurrence(
            first_source,
            Some(Transform::translation([100.0, 0.0, 0.0])),
        )
        .unwrap();
    write_source(&dir, &source);

    let mut doc = Document::new(MockKernel::default());
    let (_, linked) = insert(&mut doc, &dir);
    doc.set_occurrence_grounded(linked, true).unwrap();
    doc.create_component(
        Some("Locator"),
        Transform::translation([125.0, 0.0, 0.0]),
        false,
    )
    .unwrap();
    let joint = run(
        &mut doc,
        json!({
            "cmd": "add_joint", "kind": "rigid",
            "a": {"occurrence": "Locator:1", "geometry": {"point": [0, 0, 0]}},
            "b": {"occurrence": "Slider:1/Subassembly:2/Right:1", "geometry": {"point": [0, 0, 0]}}
        }),
    );
    let joint_uid = joint["uid"].as_str().unwrap().parse().unwrap();
    assert_eq!(joint["solved"], true, "{joint}");
    let right_path = doc
        .assembly()
        .find_path("Slider:1/Subassembly:2/Right:1")
        .unwrap();
    let left_path = doc
        .assembly()
        .find_path("Slider:1/Subassembly:1/Left:1")
        .unwrap();
    let before = doc.to_json();
    let refused = command(
        &mut doc,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
            "parts": {"occurrences": ["Subassembly:1/Right:1", "Subassembly:2/Left:1"]}
        }),
    )
    .unwrap_err();
    assert!(
        refused.contains("Joint1 uses a part that this choice would leave out"),
        "{refused}"
    );
    assert_eq!(
        doc.to_json(),
        before,
        "the parent joint's placement must be untouched"
    );
    let reply = run(
        &mut doc,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
            "parts": {"occurrences": ["Subassembly:1/Left:1", "Subassembly:2/Right:1"]}
        }),
    );
    assert_eq!(reply["changed"], true, "{reply}");
    let kept = doc.instances();
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[0].path, left_path);
    assert_eq!(&kept[1].path[..2], &right_path[..2]);
    assert_near(kept[0].transform.translation, [0.0, 0.0, 0.0], "left part");
    assert_near(
        kept[1].transform.translation,
        [125.0, 0.0, 0.0],
        "right part",
    );
    let FeatureDef::Joint(joint) = &doc.feature(joint_uid).unwrap().def else {
        panic!("parent joint")
    };
    assert_eq!(joint.b.occurrence.0, kept[1].path);
    assert_eq!(
        query(&doc, json!({"query": "joints"}))["joints"][0]["solved"],
        true
    );

    let new_right = kept[1].path.clone();
    let mut reopened = Document::from_json_at(
        &doc.to_json(),
        &dir.join("holder.mitcad"),
        MockKernel::default(),
    )
    .unwrap();
    run(
        &mut reopened,
        json!({"cmd": "set_link_parts", "component": "Slider", "base": base(&dir)}),
    );
    assert_eq!(reopened.instances().len(), 4);
    assert!(
        reopened
            .instances()
            .iter()
            .any(|item| item.path == new_right)
    );
    assert_eq!(
        query(&reopened, json!({"query": "joints"}))["joints"][0]["solved"],
        true
    );
}

#[test]
fn refreshing_a_whole_link_reuses_a_reparented_part_and_a_new_placement_of_its_definition() {
    let dir = saved("2252-reparent-update", 40.0);
    let mut source = Document::new(MockKernel::default());
    let (_, frame) = source
        .create_component(Some("Frame"), Transform::IDENTITY, false)
        .unwrap();
    let (_, peg) = source
        .create_component(Some("Peg"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    source.activate_component(ComponentUid::ROOT).unwrap();
    write_source(&dir, &source);
    let mut doc = Document::new(MockKernel::default());
    insert(&mut doc, &dir);
    let host_peg = named(&doc, "Peg:1");
    let host_frame = doc
        .assembly()
        .occurrence(named(&doc, "Frame:1"))
        .unwrap()
        .component;
    let host_definition = doc.assembly().occurrence(host_peg).unwrap().component;

    source
        .copy_occurrence(peg, Some(Transform::translation([50.0, 0.0, 0.0])))
        .unwrap();
    run(
        &mut source,
        json!({
            "cmd": "reparent_occurrence", "occurrence": peg.to_string(), "parent": frame.to_string()
        }),
    );
    write_source(&dir, &source);
    doc.update_links(Some(&dir)).unwrap();
    let peg = doc.assembly().occurrence(host_peg).unwrap();
    assert_eq!(peg.component, host_definition);
    assert_eq!(peg.parent, host_frame);
    assert_eq!(doc.assembly().occurrences_of(host_definition).count(), 2);
    assert_eq!(doc.instances().len(), 2);
}

#[test]
fn a_flexible_host_ancestor_keeps_its_values_for_the_retained_flexible_definition() {
    let dir = saved("2252-flexible-ancestor", 40.0);
    let mut source = Document::new(MockKernel::default());
    let (rig, first_source) = source
        .create_component(Some("Rig"), Transform::IDENTITY, true)
        .unwrap();
    let (_, base_source) = source
        .create_component(Some("Base"), Transform::IDENTITY, true)
        .unwrap();
    block(&mut source);
    source.activate_component(rig).unwrap();
    source.set_occurrence_grounded(base_source, true).unwrap();
    source
        .create_component(Some("Key"), Transform::translation([0.0, 0.0, 10.0]), true)
        .unwrap();
    block(&mut source);
    source.activate_component(rig).unwrap();
    let joint_source = run(
        &mut source,
        json!({
            "cmd": "add_joint", "kind": "slider", "slide_axis": "x",
            "a": {"occurrence": "Key:1", "geometry": {"point": [0, 0, 0]}},
            "b": {"occurrence": "Base:1", "geometry": {"point": [0, 0, 10]}}
        }),
    );
    assert_eq!(joint_source["solved"], true, "{joint_source}");
    source
        .create_component(
            Some("Cover"),
            Transform::translation([0.0, 0.0, 25.0]),
            true,
        )
        .unwrap();
    block(&mut source);
    source.activate_component(ComponentUid::ROOT).unwrap();
    let second_source = source
        .copy_occurrence(
            first_source,
            Some(Transform::translation([100.0, 0.0, 0.0])),
        )
        .unwrap();
    source.set_occurrence_grounded(first_source, true).unwrap();
    source.set_occurrence_grounded(second_source, true).unwrap();
    source.set_occurrence_flexible(second_source, true).unwrap();
    write_source(&dir, &source);

    let mut doc = Document::new(MockKernel::default());
    let (_, ancestor) = doc
        .create_component(Some("Frame"), Transform::IDENTITY, true)
        .unwrap();
    let (linked, occurrence) = insert(&mut doc, &dir);
    let second = named(&doc, "Rig:2");
    let old = joint(&doc, linked);
    doc.set_occurrence_grounded(occurrence, true).unwrap();
    doc.set_occurrence_flexible(occurrence, true).unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(ancestor, true).unwrap();
    doc.set_occurrence_flexible(ancestor, true).unwrap();
    run(
        &mut doc,
        json!({
            "cmd": "drive_joint", "joint": old.to_string(), "occurrence": ancestor.to_string(),
            "values": {"tx": 20}
        }),
    );
    assert_eq!(own_tx(&doc, ancestor, old), Some(20.0));

    run(
        &mut doc,
        json!({
            "cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
            "parts": {"occurrences": ["Rig:1", "Rig:2/Base:1", "Rig:2/Key:1"]}
        }),
    );
    let second_owner = doc.assembly().occurrence(second).unwrap().component;
    let new = doc
        .features()
        .find(|entry| entry.component == second_owner && matches!(entry.def, FeatureDef::Joint(_)))
        .unwrap()
        .uid;
    assert_ne!(new, old);
    assert_eq!(own_tx(&doc, ancestor, old), None);
    assert_eq!(own_tx(&doc, ancestor, new), Some(20.0));
    assert!(
        doc.instances()
            .iter()
            .any(|item| item.path.get(2) == Some(&second)
                && (item.transform.translation[0] - 120.0).abs() < 1e-6)
    );
}

#[test]
fn a_source_motion_link_drives_the_linked_slider() {
    let dir = saved("2252-motion", 40.0);
    let mut source = slider(40.0, true);
    source
        .create_component(
            Some("Drive"),
            Transform::translation([0.0, 20.0, 10.0]),
            true,
        )
        .unwrap();
    block(&mut source);
    source.activate_component(ComponentUid::ROOT).unwrap();
    run(
        &mut source,
        json!({
            "cmd": "add_joint", "kind": "slider", "slide_axis": "x",
            "a": {"occurrence": "Drive:1", "geometry": {"point": [0, 0, 0]}},
            "b": {"occurrence": "Base:1", "geometry": {"point": [0, 20, 10]}}
        }),
    );
    let coupling = run(
        &mut source,
        json!({
            "cmd": "add_motion_link",
            "a": {"joint": "Joint2", "motion": "tx"}, "b": {"joint": "Joint1", "motion": "tx"},
            "coupling": {"type": "ratio", "ratio": 2}
        }),
    );
    assert_eq!(coupling["solved"], true, "{coupling}");
    write_source(&dir, &source);
    let mut doc = Document::new(MockKernel::default());
    let (_, occurrence) = insert(&mut doc, &dir);
    doc.set_occurrence_flexible(occurrence, true).unwrap();
    run(
        &mut doc,
        json!({
            "cmd": "drive_joint", "joint": "Joint2", "occurrence": occurrence.to_string(),
            "values": {"tx": 10}
        }),
    );
    assert_near(
        key_at(&doc, occurrence),
        [20.0, 0.0, 10.0],
        "motion link follower",
    );
}

#[test]
fn a_source_constraint_set_keeps_its_fixed_point_relations_read_only() {
    let dir = saved("2252-constraints", 40.0);
    let mut source = slider(40.0, true);
    let source_joint = source
        .features()
        .find(|feature| matches!(feature.def, FeatureDef::Joint(_)))
        .unwrap()
        .uid;
    run(
        &mut source,
        json!({"cmd": "delete_feature", "uid": source_joint.to_string()}),
    );
    let relation = run(
        &mut source,
        json!({
            "cmd": "add_constraint_set",
            "rows": [{
                "type": "align",
                "a": {"occurrence": "Key:1", "geometry": {"point": [0, 0, 0]}},
                "b": {"occurrence": "Base:1", "geometry": {"point": [0, 0, 10]}}
            }]
        }),
    );
    assert_eq!(relation["error"], Value::Null, "{relation}");
    write_source(&dir, &source);
    let mut doc = Document::new(MockKernel::default());
    let (linked, _) = insert(&mut doc, &dir);
    let relation = joint(&doc, linked);
    let sets = query(&doc, json!({"query": "joints"}))["constraints"].clone();
    assert_eq!(sets[0]["solved"], true, "{sets}");
    let refused = command(
        &mut doc,
        json!({"cmd": "delete_feature", "uid": relation.to_string()}),
    )
    .unwrap_err();
    assert!(refused.contains("linked from"), "{refused}");
}

#[test]
fn the_linked_occurrence_is_flexible_and_drives_its_slider_alone() {
    let dir = saved("2209-flexible", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, first) = insert(&mut doc, &dir);
    let second = doc
        .copy_occurrence(first, Some(Transform::translation([200.0, 0.0, 0.0])))
        .unwrap();
    let uid = joint(&doc, linked);

    // Offered and accepted: the linked component has components in it.
    doc.set_occurrence_flexible(first, true).unwrap();
    assert!(doc.is_flexible(first));
    // A part inside it is flexible only as the design has it.
    let key = named(&doc, "Key:1");
    let refused = doc
        .set_occurrence_flexible(key, true)
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("Slider is linked from slider.mitcad"),
        "{refused}"
    );

    // The slider driven for the first occurrence alone.
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": first.to_string(),
               "values": {"tx": 20}}),
    );
    assert_near(
        key_at(&doc, first),
        [20.0, 0.0, 10.0],
        "the first Key moved",
    );
    assert_near(
        key_at(&doc, second),
        [200.0, 0.0, 10.0],
        "the second Key stayed",
    );
    assert_near(
        doc.placement(key).unwrap().apply_point([0.0; 3]),
        [0.0, 0.0, 10.0],
        "the Key's own placement is the design's",
    );
    assert_eq!(own_tx(&doc, first, uid), Some(20.0));
    // The design's joint has no position of its own.
    let FeatureDef::Joint(def) = &doc.feature(uid).unwrap().def else {
        panic!("a joint");
    };
    assert!(def.position.is_empty(), "{def:?}");

    // A drag of the Key in the flexible occurrence moves it alone, as the
    // occurrence's own value.
    let path = format!("{first}/{key}");
    run(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": path, "point": [20, 0, 10],
               "target": [30, 0, 10]}),
    );
    assert_near(key_at(&doc, first), [30.0, 0.0, 10.0], "dragged");
    assert_near(key_at(&doc, second), [200.0, 0.0, 10.0], "the second Key");
    // Rigid, its Key does not drag: it is the design's.
    doc.set_occurrence_flexible(first, false).unwrap();
    let path = format!("{first}/{key}");
    let refused = command(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": path, "point": [0, 0, 10],
               "target": [10, 0, 10]}),
    )
    .unwrap_err();
    assert!(refused.contains("linked from"), "{refused}");
}

#[test]
fn a_joint_of_the_parent_on_the_slider_part_holds_with_the_design_s_joint() {
    let dir = saved("2209-parent-joint", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, occurrence) = insert(&mut doc, &dir);
    let uid = joint(&doc, linked);
    doc.set_occurrence_flexible(occurrence, true).unwrap();
    doc.set_occurrence_grounded(occurrence, true).unwrap();
    let key = named(&doc, "Key:1");
    // The Key's origin held on a point of the root 15 along x: only the
    // design's slider lets it get there.
    let ball = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "ball",
               "a": {"occurrence": format!("{occurrence}/{key}"),
                     "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [15, 0, 10]}}}),
    );
    assert_eq!(ball["solved"], true, "{ball}");
    assert_near(key_at(&doc, occurrence), [15.0, 0.0, 10.0], "the Key");
    let joints = query(&doc, json!({"query": "joints"}));
    let entry = joints["flexible"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["occurrence"] == json!(occurrence.to_string()))
        .cloned()
        .unwrap_or_else(|| panic!("{joints}"));
    let tx = entry["values"][uid.to_string()]["tx"].as_f64().unwrap();
    assert!((tx - 15.0).abs() < 1e-6, "{entry}");
    for j in joints["joints"].as_array().unwrap() {
        assert_eq!(j["solved"], true, "{joints}");
    }

    // The file without the Key (and so its joint): the parent's joint
    // keeps it where it is, with a line in words.
    let mut source = slider(40.0, true);
    let gone = named(&source, "Key:1");
    source.delete_occurrence(gone).unwrap();
    write_source(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "Slider", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(
        messages.contains("Key:1 is no longer in slider.mitcad; it stays where it was"),
        "{messages}"
    );
    assert!(doc.assembly().occurrence(key).is_some());
    let link = doc
        .assembly()
        .component(linked)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert!(link.tree.unwrap().features.is_empty());
    assert!(doc.feature(uid).is_none(), "the design's joint went");
    assert_near(
        key_at(&doc, occurrence),
        [15.0, 0.0, 10.0],
        "the Key stayed",
    );
}

#[test]
fn update_link_keeps_the_own_values_within_the_new_limits_and_drops_a_joint_gone() {
    let dir = saved("2209-update", 40.0);
    let mut doc = Document::new(MockKernel::default());
    let (linked, occurrence) = insert(&mut doc, &dir);
    let uid = joint(&doc, linked);
    let key = named(&doc, "Key:1");
    doc.set_occurrence_flexible(occurrence, true).unwrap();
    run(
        &mut doc,
        json!({"cmd": "drive_joint", "joint": "Joint1", "occurrence": occurrence.to_string(),
               "values": {"tx": 30}}),
    );
    assert_near(key_at(&doc, occurrence), [30.0, 0.0, 10.0], "driven");

    // The slider's limits narrowed to ±10 in the file: the same joint and
    // parts, the occurrence's own value within the new limits.
    write_source(&dir, &slider(10.0, true));
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "Slider", "base": base(&dir)}),
    );
    assert_eq!(reply["updated"], json!([linked.to_string()]), "{reply}");
    assert_eq!(doc.undo_label(), Some("Update Link Slider"));
    assert_eq!(joint(&doc, linked), uid);
    assert_eq!(named(&doc, "Key:1"), key);
    assert_eq!(own_tx(&doc, occurrence, uid), Some(10.0));
    assert_near(key_at(&doc, occurrence), [10.0, 0.0, 10.0], "within ±10");

    // The joint gone from the file: the occurrence's own value of it is
    // dropped, with a line in words.
    let mut source = slider(10.0, true);
    let joints: Vec<FeatureUid> = source
        .features()
        .filter(|f| matches!(f.def, FeatureDef::Joint(_)))
        .map(|f| f.uid)
        .collect();
    run(
        &mut source,
        json!({"cmd": "delete_feature", "uid": joints[0].to_string()}),
    );
    write_source(&dir, &source);
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "Slider", "base": base(&dir)}),
    );
    let messages = reply["messages"].to_string();
    assert!(
        messages.contains(
            "its own value of Joint1 is dropped, as slider.mitcad no longer has the joint"
        ),
        "{messages}"
    );
    assert!(doc.feature(uid).is_none());
    assert_eq!(own_tx(&doc, occurrence, uid), None);
    assert!(
        doc.assembly().occurrence(key).is_some(),
        "the Key is still there"
    );
}

#[test]
fn a_flat_link_and_a_design_without_components_stay_flat() {
    let dir = saved("2209-flat", 40.0);
    let mut doc = Document::new(MockKernel::default());
    // The flat whole-design form a link saved by an earlier version has:
    // chosen parts, then the whole design again.
    let reply = run(
        &mut doc,
        json!({"cmd": "insert_component", "path": "slider.mitcad", "base": base(&dir),
               "name": "Flat", "parts": {"occurrences": ["O1"]}}),
    );
    let flat: ComponentUid = reply["component"].as_str().unwrap().parse().unwrap();
    run(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Flat", "base": base(&dir),
               "parts": null}),
    );
    let link = doc
        .assembly()
        .component(flat)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert!(link.tree.is_none() && link.parts.is_none(), "{link:?}");
    assert_eq!(doc.component_bodies(flat).len(), 2);

    // The file changed: read again, still one base feature of both bodies.
    write_source(&dir, &slider(20.0, true));
    let reply = run(
        &mut doc,
        json!({"cmd": "update_link", "component": "Flat", "base": base(&dir)}),
    );
    assert_eq!(reply["updated"], json!([flat.to_string()]), "{reply}");
    let link = doc
        .assembly()
        .component(flat)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert!(link.tree.is_none(), "{link:?}");
    assert_eq!(doc.assembly().children(flat).count(), 0);
    assert_eq!(doc.component_bodies(flat).len(), 2);

    // A design without components: its bodies as one base feature.
    let plain = mitcad_model::testing::unique_temp_dir("2209-plain");
    let mut lone = Document::new(MockKernel::default());
    block(&mut lone);
    std::fs::write(plain.join("slider.mitcad"), lone.to_json()).unwrap();
    let (component, _) = insert(&mut doc, &plain);
    let link = doc
        .assembly()
        .component(component)
        .unwrap()
        .link
        .clone()
        .unwrap();
    assert!(link.tree.is_none(), "{link:?}");
    assert_eq!(doc.component_bodies(component).len(), 1);
    assert_eq!(doc.assembly().children(component).count(), 0);
}
