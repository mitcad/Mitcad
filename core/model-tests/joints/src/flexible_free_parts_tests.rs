// SPDX-License-Identifier: MIT
//! Flexible subassemblies without joints of their own (mitcad#2016) with
//! the mock kernel, built like the report: a Frame holds two rail
//! carriages, each on a slider of the Frame's own; a Gantry holds a
//! Carriage and a left and a right joiner, with no joints between them.
//! The Frame is placed once in the root and grounded; the Gantry twice,
//! Gantry:1 made flexible, Gantry:2 rigid. Joints of the root from the
//! joiners of Gantry:1 to the rail carriages place each joiner alone; the
//! Gantry's Carriage stays where the Gantry has it, and Gantry:2 and the
//! Gantry's design do not change.
//!
//! The components have no bodies: the joints are on fixed points of their
//! coordinates (a frame along the component's axes).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::Vec3;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_api::DocumentApi;

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn run(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let text = command.to_string();
    match doc.command(&text) {
        Ok(result) => serde_json::from_str(&result).unwrap(),
        Err(e) => panic!("{text}: {e}"),
    }
}

fn assert_near(a: Vec3, b: Vec3, what: &str) {
    assert!(
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6),
        "{what}: {a:?} != {b:?}"
    );
}

/// Where the rail carriages are in the Frame, and the Frame in the root:
/// the second 40 further along x than the first, so that the joiners,
/// 300 apart along y in the Gantry, cannot both reach them unless they
/// move apart.
const RAIL_1: Vec3 = [100.0, 0.0, 50.0];
const RAIL_2: Vec3 = [140.0, 300.0, 50.0];

struct Gantries {
    doc: Document<MockKernel>,
    gantry: ComponentUid,
    /// Gantry:1 100 up, Gantry:2 500 up, both at x = 0.
    g1: OccurrenceUid,
    g2: OccurrenceUid,
    /// Placed in the Gantry: the Carriage 150 along y, the left joiner at
    /// its origin, the right joiner 300 along y.
    carriage: OccurrenceUid,
    left: OccurrenceUid,
    right: OccurrenceUid,
}

impl Gantries {
    /// Where a path from the root puts its last occurrence's origin.
    fn at(&self, path: &[OccurrenceUid]) -> Vec3 {
        self.doc.path_transform(path).apply_point([0.0; 3])
    }

    /// A rigid joint of the root from a joiner of Gantry:1 to a rail
    /// carriage: whether it holds.
    fn join(&mut self, joiner: &str, rail: &str) -> Value {
        run(
            &mut self.doc,
            json!({"cmd": "add_joint", "kind": "rigid",
                   "a": {"occurrence": format!("Gantry:1/{joiner}"), "geometry": {"point": [0, 0, 0]}},
                   "b": {"occurrence": format!("Frame:1/{rail}"), "geometry": {"point": [0, 0, 0]}}}),
        )
    }

    /// The Gantry's design and Gantry:2 are where they were made.
    fn assert_design_unchanged(&self) {
        for o in [self.carriage, self.left, self.right] {
            let own = self.doc.placement(o).unwrap();
            let made = self.doc.assembly().occurrence(o).unwrap().transform;
            assert_near(
                own.apply_point([0.0; 3]),
                made.apply_point([0.0; 3]),
                "the Gantry's own placement of its parts",
            );
        }
        assert_near(
            self.at(&[self.g2, self.left]),
            [0.0, 0.0, 500.0],
            "Gantry:2's left joiner",
        );
        assert_near(
            self.at(&[self.g2, self.carriage]),
            [0.0, 150.0, 500.0],
            "Gantry:2's Carriage",
        );
        assert_near(
            self.at(&[self.g2, self.right]),
            [0.0, 300.0, 500.0],
            "Gantry:2's right joiner",
        );
    }
}

/// The Frame with its rail carriages on sliders, grounded in the root, and
/// the Gantry with its three parts, placed twice. `inside` runs with the
/// Gantry active, after its parts are placed (a rigid group, a joint).
fn gantries(inside: impl FnOnce(&mut Document<MockKernel>)) -> Gantries {
    let mut doc = Document::new(MockKernel::default());
    let (_, frame) = doc
        .create_component(Some("Frame"), Transform::IDENTITY, true)
        .unwrap();
    let (_, rail) = doc
        .create_component(Some("RailCarriage"), Transform::translation(RAIL_1), false)
        .unwrap();
    doc.copy_occurrence(rail, Some(Transform::translation(RAIL_2)))
        .unwrap();
    for (name, at) in [("RailCarriage:1", RAIL_1), ("RailCarriage:2", RAIL_2)] {
        let slider = run(
            &mut doc,
            json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
                   "a": {"occurrence": name, "geometry": {"point": [0, 0, 0]}},
                   "b": {"geometry": {"point": at}}}),
        );
        assert_eq!(slider["solved"], true, "{slider}");
    }
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_grounded(frame, true).unwrap();

    let (gantry, g1) = doc
        .create_component(
            Some("Gantry"),
            Transform::translation([0.0, 0.0, 100.0]),
            true,
        )
        .unwrap();
    let (_, carriage) = doc
        .create_component(
            Some("Carriage"),
            Transform::translation([0.0, 150.0, 0.0]),
            false,
        )
        .unwrap();
    let (_, left) = doc
        .create_component(Some("LeftJoiner"), Transform::IDENTITY, false)
        .unwrap();
    let (_, right) = doc
        .create_component(
            Some("RightJoiner"),
            Transform::translation([0.0, 300.0, 0.0]),
            false,
        )
        .unwrap();
    inside(&mut doc);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let g2 = doc
        .copy_occurrence(g1, Some(Transform::translation([0.0, 0.0, 500.0])))
        .unwrap();
    Gantries {
        doc,
        gantry,
        g1,
        g2,
        carriage,
        left,
        right,
    }
}

/// The report's case: Gantry:1 made flexible (the Gantry has no joints of
/// its own), each joiner joined rigidly to a rail carriage by a joint of
/// the root. Both joints hold, each places its joiner alone.
#[test]
fn each_parent_joint_places_its_free_part_alone() {
    let mut g = gantries(|_| {});
    run(
        &mut g.doc,
        json!({"cmd": "set_occurrence_flexible", "occurrence": "Gantry:1"}),
    );
    assert_eq!(g.doc.undo_label(), Some("Make Gantry:1 Flexible"));
    assert!(g.doc.is_flexible(g.g1));

    let left = g.join("LeftJoiner:1", "RailCarriage:1");
    assert_eq!(left["solved"], true, "{left}");
    let right = g.join("RightJoiner:1", "RailCarriage:2");
    assert_eq!(right["solved"], true, "{right}");
    let joints = query(&g.doc, json!({"query": "joints"}));
    for joint in joints["joints"].as_array().unwrap() {
        assert_eq!(joint["solved"], true, "{joint}");
    }

    // Only the joiners moved, each onto its rail carriage.
    assert_near(g.at(&[g.g1, g.left]), RAIL_1, "the left joiner");
    assert_near(g.at(&[g.g1, g.right]), RAIL_2, "the right joiner");
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stayed");
    assert_near(
        g.at(&[g.g1, g.carriage]),
        [0.0, 150.0, 100.0],
        "Gantry:1's Carriage stayed",
    );
    g.assert_design_unchanged();

    // Risk 2 (mitcad#2016): the root's degrees of freedom name the free
    // part nothing holds, inside Gantry:1, with all six motions.
    let dof = joints["dof"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["component"] == ComponentUid::ROOT.to_string())
        .cloned()
        .unwrap_or_else(|| panic!("the root's degrees of freedom: {joints}"));
    let loose = dof["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["names"] == json!(["Carriage:1"]))
        .cloned()
        .unwrap_or_else(|| panic!("the free Carriage is listed: {dof}"));
    assert_eq!(loose["under"], g.g1.to_string(), "{loose}");
    assert_eq!(loose["dof"], 6, "{loose}");
    assert_eq!(loose["joints"], json!([]), "{loose}");

    // The design file keeps it, and opens with the same positions.
    let text = g.doc.to_json();
    assert!(text.contains("\"free_parts\""), "{text}");
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[g.g1, g.left]).apply_point([0.0; 3]),
        RAIL_1,
        "reopened: the left joiner",
    );
    assert_near(
        copy.path_transform(&[g.g1, g.carriage])
            .apply_point([0.0; 3]),
        [0.0, 150.0, 100.0],
        "reopened: the Carriage",
    );
    assert_eq!(copy.to_json(), text);

    // Rigid again: the first joint moves the whole Gantry:1, so the second
    // cannot hold any more.
    g.doc.set_occurrence_flexible(g.g1, false).unwrap();
    let joints = query(&g.doc, json!({"query": "joints"}));
    let solved: Vec<&Value> = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["component"] == ComponentUid::ROOT.to_string())
        .map(|j| &j["solved"])
        .collect();
    assert_eq!(solved, [&json!(true), &json!(false)], "{joints}");
}

/// A rigid group of the Gantry ties the Carriage to the left joiner: the
/// root's joint on the left joiner moves the two together, the one on the
/// right joiner moves it alone.
#[test]
fn a_rigid_group_of_the_component_moves_its_parts_together() {
    let mut g = gantries(|doc| {
        run(
            doc,
            json!({"cmd": "add_rigid_group", "occurrences": ["Carriage:1", "LeftJoiner:1"]}),
        );
    });
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    assert_eq!(g.join("LeftJoiner:1", "RailCarriage:1")["solved"], true);
    assert_eq!(g.join("RightJoiner:1", "RailCarriage:2")["solved"], true);
    // The left joiner moved by (100, 0, -50), and the Carriage with it.
    assert_near(g.at(&[g.g1, g.left]), RAIL_1, "the left joiner");
    assert_near(
        g.at(&[g.g1, g.carriage]),
        [100.0, 150.0, 50.0],
        "the Carriage moved with it",
    );
    assert_near(g.at(&[g.g1, g.right]), RAIL_2, "the right joiner alone");
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stayed");
    g.assert_design_unchanged();
}

/// Risk 3 (mitcad#2016): a joint of the root that works against an inner
/// rigid group says so, naming the group with the joints and painting the
/// group's parts; it does not fail as a solve of the whole occurrence.
#[test]
fn a_parent_joint_against_an_inner_rigid_group_names_both() {
    let mut g = gantries(|doc| {
        run(
            doc,
            json!({"cmd": "add_rigid_group",
                   "occurrences": ["LeftJoiner:1", "RightJoiner:1"]}),
        );
    });
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    // The left joiner's joint moves the group: the right joiner comes 40
    // short of its rail carriage, which the group keeps it from reaching.
    assert_eq!(g.join("LeftJoiner:1", "RailCarriage:1")["solved"], true);
    assert_near(g.at(&[g.g1, g.right]), [100.0, 300.0, 50.0], "the group");
    let _ = g.doc.command(
        &json!({"cmd": "add_joint", "kind": "rigid",
                "a": {"occurrence": "Gantry:1/RightJoiner:1", "geometry": {"point": [0, 0, 0]}},
                "b": {"occurrence": "Frame:1/RailCarriage:2", "geometry": {"point": [0, 0, 0]}}})
        .to_string(),
    );
    let joints = query(&g.doc, json!({"query": "joints"}));
    let failed = joints["joints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["solved"] == false)
        .cloned()
        .unwrap_or_else(|| panic!("the second joint fails: {joints}"));
    // In words, the group among what works against each other.
    let error = failed["error"].as_str().unwrap_or_default();
    assert!(error.contains("RigidGroup1"), "{failed}");
    assert!(error.contains("cannot all hold"), "{failed}");
    // Its parts painted.
    let notices = failed["notices"].to_string();
    assert!(notices.contains("occurrence"), "{failed}");
    // Gantry:1 itself did not move, nor did the Gantry's design.
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stayed");
    g.assert_design_unchanged();
}

/// Risk 1 (mitcad#2016): a design saved before (0.63.0) with a flexible
/// occurrence whose component has a joint of its own, and a joint of the
/// root on a part that joint does not place, opens with the whole
/// occurrence moved, as it was then; made rigid and flexible again, the
/// part is free.
#[test]
fn an_older_flexible_occurrence_keeps_moving_whole() {
    let mut g = gantries(|doc| {
        // The Gantry's own joint: the Carriage slides along y.
        let slider = run(
            doc,
            json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "y",
                   "a": {"occurrence": "Carriage:1", "geometry": {"point": [0, 0, 0]}},
                   "b": {"geometry": {"point": [0, 150, 0]}}}),
        );
        assert_eq!(slider["solved"], true, "{slider}");
    });
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    assert_eq!(g.join("LeftJoiner:1", "RailCarriage:1")["solved"], true);
    // Today: the left joiner alone.
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stays");
    assert_near(g.at(&[g.g1, g.left]), RAIL_1, "the left joiner");

    // As 0.63.0 saved it: the flexible occurrence without free parts.
    let mut file: Value = serde_json::from_str(&g.doc.to_json()).unwrap();
    assert!(drop_key(&mut file, "free_parts"), "{file}");
    let old = file.to_string();
    let mut opened = Document::from_json(&old, MockKernel::default()).unwrap();
    opened.recompute();
    let at = |doc: &Document<MockKernel>, path: &[OccurrenceUid]| {
        doc.path_transform(path).apply_point([0.0; 3])
    };
    assert_near(at(&opened, &[g.g1]), RAIL_1, "Gantry:1 moved whole");
    assert_near(at(&opened, &[g.g1, g.left]), RAIL_1, "the left joiner");
    assert_near(
        at(&opened, &[g.g1, g.carriage]),
        [100.0, 150.0, 50.0],
        "the Carriage moved with Gantry:1",
    );
    let saved: Value = serde_json::from_str(&opened.to_json()).unwrap();
    assert!(
        !saved.to_string().contains("free_parts"),
        "saved again as it was: {saved}"
    );

    // Rigid and flexible again: the left joiner alone.
    opened.set_occurrence_flexible(g.g1, false).unwrap();
    opened.set_occurrence_flexible(g.g1, true).unwrap();
    assert_near(at(&opened, &[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stays");
    assert_near(at(&opened, &[g.g1, g.left]), RAIL_1, "the left joiner");
}

/// Takes `key` out of every object in `value`; whether one had it.
fn drop_key(value: &mut Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            let mut found = map.remove(key).is_some();
            for v in map.values_mut() {
                found |= drop_key(v, key);
            }
            found
        }
        Value::Array(items) => {
            let mut found = false;
            for v in items {
                found |= drop_key(v, key);
            }
            found
        }
        _ => false,
    }
}

/// Risk 4 (mitcad#2016): a free part dragged along a joint of the root
/// keeps where it was left, for Gantry:1 alone; the kept placement follows
/// the part by its uid when the Gantry changes (a part added, a component
/// renamed) and goes without an error with the part.
#[test]
fn a_dragged_free_part_keeps_its_place_by_uid() {
    let mut g = gantries(|_| {});
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    // The left joiner slides along x on the first rail carriage: it goes
    // to the slider's line, (0, 0, 50).
    let slider = run(
        &mut g.doc,
        json!({"cmd": "add_joint", "kind": "slider", "slide_axis": "x",
               "a": {"occurrence": "Gantry:1/LeftJoiner:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": "Frame:1/RailCarriage:1", "geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(slider["solved"], true, "{slider}");
    let before = g.at(&[g.g1, g.left]);
    assert!(
        (before[1]).abs() < 1e-6 && (before[2] - 50.0).abs() < 1e-6,
        "{before:?}"
    );

    // Dragged 60 along the rail (points in the Gantry's coordinates).
    let grab = [before[0], 0.0, -50.0];
    let target = [60.0, 0.0, -50.0];
    g.doc
        .drag_occurrence_at(&[g.g1, g.left], Some(grab), target)
        .unwrap();
    assert_eq!(g.doc.undo_label(), Some("Drag LeftJoiner:1"));
    assert_near(g.at(&[g.g1, g.left]), [60.0, 0.0, 50.0], "dragged");
    assert_near(
        g.at(&[g.g1, g.carriage]),
        [0.0, 150.0, 100.0],
        "the Carriage stayed",
    );
    g.assert_design_unchanged();
    let flex = g
        .doc
        .assembly()
        .occurrence(g.g1)
        .unwrap()
        .flexible
        .clone()
        .unwrap();
    let kept = flex.placement(&[g.left]).expect("kept for Gantry:1");
    assert_near(kept.apply_point([0.0; 3]), [60.0, 0.0, -50.0], "kept");

    // Reopened: where it was left.
    let text = g.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[g.g1, g.left]).apply_point([0.0; 3]),
        [60.0, 0.0, 50.0],
        "reopened",
    );

    // A part added to the Gantry and the joiner's component renamed: the
    // kept placement follows the part.
    g.doc.activate_component(g.gantry).unwrap();
    g.doc
        .create_component(
            Some("Bracket"),
            Transform::translation([0.0, 450.0, 0.0]),
            false,
        )
        .unwrap();
    g.doc.activate_component(ComponentUid::ROOT).unwrap();
    let joiner = g.doc.assembly().occurrence(g.left).unwrap().component;
    g.doc.rename_component(joiner, "Joiner").unwrap();
    assert_near(g.at(&[g.g1, g.left]), [60.0, 0.0, 50.0], "still kept");

    // The joiner deleted: its kept placement goes without an error.
    g.doc.delete_occurrence(g.left).unwrap();
    let joints = query(&g.doc, json!({"query": "joints"}));
    for joint in joints["joints"].as_array().unwrap() {
        assert_eq!(joint["solved"], true, "{joint}");
    }
    let file: Value = serde_json::from_str(&g.doc.to_json()).unwrap();
    let flexible = file["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["uid"] == g.g1.to_string())
        .map(|o| o["flexible"].clone())
        .unwrap_or_else(|| panic!("Gantry:1 in the file: {file}"));
    assert!(flexible.get("placements").is_none(), "{flexible}");
}

/// Risk 6 (mitcad#2016): a free part of a flexible occurrence nested in
/// another flexible occurrence is placed alone by a joint of the root.
#[test]
fn a_free_part_moves_alone_through_nested_flexible_occurrences() {
    let mut doc = Document::new(MockKernel::default());
    // A Portal (in the root, 1000 up) holding a Bridge (in the Portal,
    // 100 along y) holding a Post at its origin and a Beam 50 along x;
    // no joints anywhere yet.
    let (_, portal) = doc
        .create_component(
            Some("Portal"),
            Transform::translation([0.0, 0.0, 1000.0]),
            true,
        )
        .unwrap();
    let (_, bridge) = doc
        .create_component(
            Some("Bridge"),
            Transform::translation([0.0, 100.0, 0.0]),
            true,
        )
        .unwrap();
    let (_, post) = doc
        .create_component(Some("Post"), Transform::IDENTITY, false)
        .unwrap();
    let (_, beam) = doc
        .create_component(
            Some("Beam"),
            Transform::translation([50.0, 0.0, 0.0]),
            false,
        )
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_flexible(portal, true).unwrap();
    doc.set_occurrence_flexible(bridge, true).unwrap();
    // A joint of the root from the Post to a point of the root's own.
    let joined = run(
        &mut doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Portal:1/Bridge:1/Post:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"geometry": {"point": [10, 20, 30]}}}),
    );
    assert_eq!(joined["solved"], true, "{joined}");
    let at = |path: &[OccurrenceUid]| doc.path_transform(path).apply_point([0.0; 3]);
    assert_near(at(&[portal, bridge, post]), [10.0, 20.0, 30.0], "the Post");
    assert_near(
        at(&[portal, bridge, beam]),
        [50.0, 100.0, 1000.0],
        "the Beam stayed",
    );
    assert_near(
        at(&[portal, bridge]),
        [0.0, 100.0, 1000.0],
        "the Bridge stayed",
    );
    assert_near(at(&[portal]), [0.0, 0.0, 1000.0], "the Portal stayed");
    // The Bridge's own placement of its Post did not change.
    let own = doc.placement(post).unwrap();
    assert_near(own.apply_point([0.0; 3]), [0.0; 3], "the Bridge's design");
}

/// An Anchor in the root, 100 along x and 50 up, grounded, joined
/// rigidly to Gantry:1's left joiner by a joint of the root on fixed
/// points: the joiner goes onto the Anchor. The Anchor's occurrence, and
/// with `body` a 10 x 10 x 5 block in it (its extrude and its body).
fn anchored(g: &mut Gantries, body: bool) -> (OccurrenceUid, Option<(FeatureUid, BodyUid)>) {
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    g.doc.activate_component(ComponentUid::ROOT).unwrap();
    let (_, anchor) = g
        .doc
        .create_component(
            Some("Anchor"),
            Transform::translation([100.0, 0.0, 50.0]),
            true,
        )
        .unwrap();
    let block = body.then(|| {
        let sketch = g.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
        let region = g
            .doc
            .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
            .unwrap()
            .region;
        let made = g
            .doc
            .add_feature(&extrude(sketch, &[&region], 5.0, "new_body", &[]), None)
            .unwrap()
            .uid;
        (made, BodyUid::new(made, 0))
    });
    g.doc.activate_component(ComponentUid::ROOT).unwrap();
    g.doc.set_occurrence_grounded(anchor, true).unwrap();
    (anchor, block)
}

/// Where Gantry:1 keeps its left joiner of its own, if it does.
fn kept_left(g: &Gantries) -> Option<Transform> {
    g.doc
        .assembly()
        .occurrence(g.g1)
        .and_then(|o| o.flexible.as_ref())
        .and_then(|f| f.placement(&[g.left]))
}

/// mitcad#2017: deleting the Anchor takes the joint of the root on the
/// left joiner along, and the joiner, a free part of Gantry:1, stays where
/// the joint left it, kept as Gantry:1's own placement of it, also after
/// save and reopen, as an occurrence placed in the root stays when its
/// joint goes with what it is joined to (mitcad#1811). Undo brings the
/// joint back and drops the kept placement; the joint deleted by itself
/// lets the joiner go back where the Gantry places it, as a joint deleted
/// by itself does for an occurrence of the root (mitcad#1820).
#[test]
fn a_free_part_stays_when_its_parent_joint_goes() {
    let mut g = gantries(|_| {});
    let (anchor, _) = anchored(&mut g, false);
    let joined = run(
        &mut g.doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Gantry:1/LeftJoiner:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": "Anchor:1", "geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(joined["solved"], true, "{joined}");
    let joint: FeatureUid = joined["uid"].as_str().unwrap().parse().unwrap();
    assert_near(g.at(&[g.g1, g.left]), [100.0, 0.0, 50.0], "on the Anchor");
    assert!(kept_left(&g).is_none(), "nothing kept yet");

    let deleted = g.doc.delete_occurrence(anchor).unwrap();
    assert!(deleted.contains(&joint), "the joint went: {deleted:?}");
    assert_near(g.at(&[g.g1, g.left]), [100.0, 0.0, 50.0], "stayed");
    assert_near(
        g.at(&[g.g1, g.carriage]),
        [0.0, 150.0, 100.0],
        "the Carriage stayed",
    );
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stayed");
    g.assert_design_unchanged();
    // Kept in the Gantry's coordinates, for Gantry:1 alone.
    let kept = kept_left(&g).expect("kept for Gantry:1");
    assert_near(kept.apply_point([0.0; 3]), [100.0, 0.0, -50.0], "kept");

    // Saved and reopened: where the joint left it.
    let text = g.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[g.g1, g.left]).apply_point([0.0; 3]),
        [100.0, 0.0, 50.0],
        "reopened",
    );
    assert_near(
        copy.path_transform(&[g.g2, g.left]).apply_point([0.0; 3]),
        [0.0, 0.0, 500.0],
        "reopened: Gantry:2's left joiner",
    );
    assert_eq!(copy.to_json(), text);

    // Undo: the Anchor and the joint are back, nothing is kept.
    g.doc.undo().unwrap();
    assert!(g.doc.assembly().occurrence(anchor).is_some());
    assert!(kept_left(&g).is_none(), "undo drops the kept placement");
    assert_near(g.at(&[g.g1, g.left]), [100.0, 0.0, 50.0], "joined again");

    // The joint deleted by itself: the joiner goes back where the Gantry
    // places it, and nothing is kept.
    g.doc.delete_feature(joint, false).unwrap();
    assert_near(g.at(&[g.g1, g.left]), [0.0, 0.0, 100.0], "back");
    assert!(kept_left(&g).is_none(), "nothing kept");
}

/// mitcad#2017 with `delete_feature` and its dependents: the joint of the
/// root from the left joiner to the Anchor's top face goes with the
/// Anchor's extrude, and the joiner stays where the joint left it (its
/// whole placement, within 1e-6 mm), as the joint's other side would
/// (mitcad#1820).
#[test]
fn a_free_part_stays_when_its_parent_joint_goes_as_a_dependent() {
    let mut g = gantries(|_| {});
    let (_, block) = anchored(&mut g, true);
    let (made, body) = block.expect("a block");
    let top = g
        .doc
        .body_shape(body)
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == "end")
        .map(ToString::to_string)
        .unwrap();
    let joined = run(
        &mut g.doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Gantry:1/LeftJoiner:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": "Anchor:1", "geometry": {"body": body, "face": top}}}),
    );
    assert_eq!(joined["solved"], true, "{joined}");
    let joint: FeatureUid = joined["uid"].as_str().unwrap().parse().unwrap();
    let before = g.doc.path_transform(&[g.g1, g.left]);
    // On the Anchor's top face, not where the Gantry places it.
    assert_near(
        before.apply_point([0.0; 3]),
        [105.0, 5.0, 55.0],
        "on the Anchor's top",
    );

    let deleted = g.doc.delete_feature(made, true).unwrap();
    assert!(deleted.contains(&joint), "the joint went: {deleted:?}");
    let after = g.doc.path_transform(&[g.g1, g.left]);
    for p in [
        [0.0; 3],
        [10.0, 0.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 10.0],
    ] {
        assert_near(after.apply_point(p), before.apply_point(p), "stayed");
    }
    assert!(kept_left(&g).is_some(), "kept for Gantry:1");
    g.assert_design_unchanged();

    let text = g.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    let reopened = copy.path_transform(&[g.g1, g.left]);
    for p in [
        [0.0; 3],
        [10.0, 0.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 10.0],
    ] {
        assert_near(reopened.apply_point(p), before.apply_point(p), "reopened");
    }
}

/// mitcad#2017, the drag of a free part with no joint of the parent on it,
/// through the model API: as for an occurrence of the root with no joints
/// (Gantry:2 here), which `drag_occurrence` moves freely, the free part
/// moves freely too, for Gantry:1 alone, and the drag keeps where it left
/// it as Gantry:1's own placement of it. (The application refuses both
/// drags with a status message: components move only through joints
/// there, mitcad#150.)
#[test]
fn an_unjoined_free_part_drags_through_the_api_as_an_unjoined_occurrence() {
    let mut g = gantries(|_| {});
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    let dof = query(
        &g.doc,
        json!({"query": "joint_dof", "occurrence": "Gantry:2"}),
    );
    assert_eq!(dof["dof"], 6, "{dof}");

    // The free Carriage, 40 up, in the Gantry's coordinates.
    run(
        &mut g.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Gantry:1/Carriage:1",
               "point": [0, 150, 0], "target": [0, 150, 40]}),
    );
    assert_eq!(g.doc.undo_label(), Some("Drag Carriage:1"));
    assert_near(g.at(&[g.g1, g.carriage]), [0.0, 150.0, 140.0], "dragged");
    assert_near(g.at(&[g.g1]), [0.0, 0.0, 100.0], "Gantry:1 stayed");
    assert_near(g.at(&[g.g1, g.left]), [0.0, 0.0, 100.0], "the left joiner");
    g.assert_design_unchanged();
    let kept = g
        .doc
        .assembly()
        .occurrence(g.g1)
        .and_then(|o| o.flexible.as_ref())
        .and_then(|f| f.placement(&[g.carriage]))
        .expect("kept for Gantry:1");
    assert_near(kept.apply_point([0.0; 3]), [0.0, 150.0, 40.0], "kept");
    let text = g.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[g.g1, g.carriage])
            .apply_point([0.0; 3]),
        [0.0, 150.0, 140.0],
        "reopened",
    );

    // Gantry:2, with no joints in the root, 200 back along y.
    run(
        &mut g.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Gantry:2",
               "point": [0, 0, 500], "target": [0, -200, 500]}),
    );
    assert_eq!(g.doc.undo_label(), Some("Drag Gantry:2"));
    assert_near(g.at(&[g.g2]), [0.0, -200.0, 500.0], "Gantry:2 dragged");
}

/// A Move feature of `component` (added at the end of the timeline) that
/// moves `occurrence` by `by` in the component's coordinates; its uid.
fn add_move(
    doc: &mut Document<MockKernel>,
    component: ComponentUid,
    occurrence: OccurrenceUid,
    by: Vec3,
) -> FeatureUid {
    let added = run(
        doc,
        json!({"cmd": "add_feature", "component": component.to_string(),
               "def": move_def(occurrence, by)}),
    );
    added["uid"].as_str().unwrap().parse().unwrap()
}

fn move_def(occurrence: OccurrenceUid, by: Vec3) -> Value {
    json!({"type": "move_occurrence", "occurrences": [occurrence.to_string()],
           "transform": {"type": "translate_xyz", "x": by[0], "y": by[1], "z": by[2]}})
}

fn plus(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// mitcad#2025: a Move of the Gantry's own lifts the left joiner before a
/// joint of the root places it on the Anchor; the Anchor deleted, the
/// joiner stays where the joint left it, kept by Gantry:1 where it is
/// before the Move (the Move moves it from there, as it moves the
/// Gantry's own placement of it). Suppressed or deleted, the Move is taken
/// back: the joiner goes to its kept placement; edited, the joiner
/// follows; Gantry:2 shows the Move as the Gantry's design.
#[test]
fn a_move_of_a_kept_free_part_is_taken_back_and_followed() {
    const LIFT: Vec3 = [0.0, 0.0, 30.0];
    const LOWER_LIFT: Vec3 = [0.0, 0.0, 10.0];
    let mut g = gantries(|_| {});
    let (anchor, _) = anchored(&mut g, false);
    let lift = add_move(&mut g.doc, g.gantry, g.left, LIFT);
    // Made where the Gantry has it: the origin, Gantry:1 100 up and
    // Gantry:2 500 up.
    assert_near(g.at(&[g.g1, g.left]), [0.0, 0.0, 130.0], "Gantry:1, lifted");
    assert_near(g.at(&[g.g2, g.left]), [0.0, 0.0, 530.0], "Gantry:2, lifted");

    let joined = run(
        &mut g.doc,
        json!({"cmd": "add_joint", "kind": "rigid",
               "a": {"occurrence": "Gantry:1/LeftJoiner:1", "geometry": {"point": [0, 0, 0]}},
               "b": {"occurrence": "Anchor:1", "geometry": {"point": [0, 0, 0]}}}),
    );
    assert_eq!(joined["solved"], true, "{joined}");
    const ON_ANCHOR: Vec3 = [100.0, 0.0, 50.0];
    assert_near(g.at(&[g.g1, g.left]), ON_ANCHOR, "on the Anchor");

    g.doc.delete_occurrence(anchor).unwrap();
    assert_near(g.at(&[g.g1, g.left]), ON_ANCHOR, "stayed");
    // Kept in the Gantry's coordinates (Gantry:1 is 100 up) before the
    // Move: the Move's lift taken out.
    let in_gantry = [ON_ANCHOR[0], ON_ANCHOR[1], ON_ANCHOR[2] - 100.0];
    let kept = kept_left(&g).expect("kept for Gantry:1");
    assert_near(
        plus(kept.apply_point([0.0; 3]), LIFT),
        in_gantry,
        "kept before the Move",
    );
    let base = plus(kept.apply_point([0.0; 3]), [0.0, 0.0, 100.0]);

    // Suppressed: the joiner goes back to where Gantry:1 keeps it, and
    // Gantry:2's to where the Gantry made it.
    run(
        &mut g.doc,
        json!({"cmd": "suppress_feature", "uid": lift.to_string()}),
    );
    assert_near(g.at(&[g.g1, g.left]), base, "the Move suppressed");
    assert_near(g.at(&[g.g2, g.left]), [0.0, 0.0, 500.0], "Gantry:2");
    run(
        &mut g.doc,
        json!({"cmd": "suppress_feature", "uid": lift.to_string(), "suppressed": false}),
    );
    assert_near(g.at(&[g.g1, g.left]), ON_ANCHOR, "the Move back");

    // Edited: the joiner follows the Move from where it is kept.
    run(
        &mut g.doc,
        json!({"cmd": "edit_feature", "uid": lift.to_string(),
               "def": move_def(g.left, LOWER_LIFT)}),
    );
    let lower = plus(base, LOWER_LIFT);
    assert_near(g.at(&[g.g1, g.left]), lower, "the Move edited");
    assert_near(g.at(&[g.g2, g.left]), [0.0, 0.0, 510.0], "Gantry:2 edited");
    assert_near(
        g.at(&[g.g1, g.carriage]),
        [0.0, 150.0, 100.0],
        "the Carriage stayed",
    );
    assert_eq!(kept_left(&g), Some(kept), "the kept placement is as it was");

    // Saved and reopened: the same.
    let text = g.doc.to_json();
    let mut copy = Document::from_json(&text, MockKernel::default()).unwrap();
    copy.recompute();
    assert_near(
        copy.path_transform(&[g.g1, g.left]).apply_point([0.0; 3]),
        lower,
        "reopened",
    );

    // Deleted: the joiner goes to its kept placement; undo brings the
    // Move back, and the joiner with it.
    g.doc.delete_feature(lift, false).unwrap();
    assert_near(g.at(&[g.g1, g.left]), base, "the Move deleted");
    assert_near(g.at(&[g.g2, g.left]), [0.0, 0.0, 500.0], "Gantry:2");
    g.doc.undo().unwrap();
    assert_near(g.at(&[g.g1, g.left]), lower, "undone");
}

/// mitcad#2025, the other way round: the free Carriage dragged through the
/// model API is kept by Gantry:1, and a Move of the Gantry's own added
/// after it moves it from there for Gantry:1 too (before, Gantry:1's
/// Carriage stayed where the drag left it). A drag with the Move in place
/// keeps the Carriage before it, so that the Move deleted takes its motion
/// back from where the drag left it.
#[test]
fn a_move_after_a_drag_moves_the_kept_free_part() {
    const SHIFT: Vec3 = [25.0, 0.0, 0.0];
    let mut g = gantries(|_| {});
    g.doc.set_occurrence_flexible(g.g1, true).unwrap();
    // 40 up, in the Gantry's coordinates.
    run(
        &mut g.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Gantry:1/Carriage:1",
               "point": [0, 150, 0], "target": [0, 150, 40]}),
    );
    let dragged = [0.0, 150.0, 140.0];
    assert_near(g.at(&[g.g1, g.carriage]), dragged, "dragged");

    let shift = add_move(&mut g.doc, g.gantry, g.carriage, SHIFT);
    assert_near(
        g.at(&[g.g1, g.carriage]),
        plus(dragged, SHIFT),
        "moved from where the drag left it",
    );
    assert_near(
        g.at(&[g.g2, g.carriage]),
        plus([0.0, 150.0, 500.0], SHIFT),
        "Gantry:2's Carriage moved",
    );
    assert_near(g.at(&[g.g1, g.left]), [0.0, 0.0, 100.0], "the left joiner");

    // Dragged 20 further up with the Move in place: kept before the Move.
    let from = plus([0.0, 150.0, 40.0], SHIFT);
    let to = plus([0.0, 150.0, 60.0], SHIFT);
    run(
        &mut g.doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Gantry:1/Carriage:1",
               "point": from, "target": to}),
    );
    let higher = [0.0, 150.0, 160.0];
    assert_near(g.at(&[g.g1, g.carriage]), plus(higher, SHIFT), "dragged");
    let kept = g
        .doc
        .assembly()
        .occurrence(g.g1)
        .and_then(|o| o.flexible.as_ref())
        .and_then(|f| f.placement(&[g.carriage]))
        .expect("kept for Gantry:1");
    assert_near(kept.apply_point([0.0; 3]), [0.0, 150.0, 60.0], "kept");

    // The Move deleted: its motion taken back.
    g.doc.delete_feature(shift, false).unwrap();
    assert_near(g.at(&[g.g1, g.carriage]), higher, "the Move deleted");
    assert_near(
        g.at(&[g.g2, g.carriage]),
        [0.0, 150.0, 500.0],
        "Gantry:2's Carriage",
    );
}

/// mitcad#2025 in a design without joints, whose recompute after a feature
/// added at the end goes on from the one before: the free Lid of the
/// flexible Box:1, dragged and then moved by a Move of the Box's own,
/// stays where they put it when a sketch is added to the root.
#[test]
fn a_moved_kept_free_part_stays_when_the_timeline_goes_on() {
    const UP: Vec3 = [0.0, 0.0, 5.0];
    let mut doc = Document::new(MockKernel::default());
    let (boxed, box1) = doc
        .create_component(Some("Box"), Transform::IDENTITY, true)
        .unwrap();
    let (_, lid) = doc
        .create_component(Some("Lid"), Transform::translation([0.0, 0.0, 10.0]), false)
        .unwrap();
    doc.create_component(Some("Base"), Transform::IDENTITY, false)
        .unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_flexible(box1, true).unwrap();
    run(
        &mut doc,
        json!({"cmd": "drag_occurrence", "occurrence": "Box:1/Lid:1",
               "point": [0, 0, 10], "target": [20, 0, 10]}),
    );
    add_move(&mut doc, boxed, lid, UP);
    let at = |doc: &Document<MockKernel>| doc.path_transform(&[box1, lid]).apply_point([0.0; 3]);
    let expected = plus([20.0, 0.0, 10.0], UP);
    assert_near(at(&doc), expected, "dragged and moved");

    doc.add_sketch(SketchPlane::Xy).unwrap();
    assert_near(at(&doc), expected, "after a sketch at the end");
    assert_near(
        doc.placement(lid).unwrap().apply_point([0.0; 3]),
        plus([0.0, 0.0, 10.0], UP),
        "the Box's own placement of the Lid",
    );
}
