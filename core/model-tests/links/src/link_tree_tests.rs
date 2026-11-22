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
    // Not by its envelope, no chosen parts.
    let refused = command(
        &mut doc,
        json!({"cmd": "set_link_parts", "component": "Slider", "base": base(&dir),
               "parts": {"occurrences": ["O2"]}}),
    )
    .unwrap_err();
    assert!(
        refused.contains("keeps the components of its design"),
        "{refused}"
    );

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
