// SPDX-License-Identifier: MIT
//! Convert to Features (mitcad#1815) with the mock kernel: a placed
//! profile replaced by its source's section sketch and extrusion with its
//! joint on a mount frame, an assembly constraint on a side face, a bound
//! length and a sketch projecting its end face all holding and every
//! occurrence where it was; undo bringing the derived part back; a mitred
//! part's cuts as features with the reference to its mitre; and a
//! reference no feature makes refusing with nothing changed.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::features::{FeatureDef, GeomRef};
use mitcad_model::ids::{FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

fn convert(doc: &mut Document<MockKernel>, part: &Value, check: bool) -> Result<Value, String> {
    command(
        doc,
        json!({"cmd": "convert_derived_part", "component": part["component"], "check": check}),
    )
}

/// Every occurrence's placement at the marker.
fn placements(doc: &Document<MockKernel>) -> Vec<(OccurrenceUid, Transform)> {
    doc.assembly()
        .occurrences
        .iter()
        .map(|o| (o.uid, placed(doc, o.uid)))
        .collect()
}

/// Placements equal to 1e-6 mm (their turns to 1e-9).
fn assert_same_placements(was: &[(OccurrenceUid, Transform)], now: &[(OccurrenceUid, Transform)]) {
    assert_eq!(was.len(), now.len());
    for ((a, t), (b, u)) in was.iter().zip(now) {
        assert_eq!(a, b);
        let same = (0..3).all(|r| {
            (t.translation[r] - u.translation[r]).abs() < 1e-6
                && (0..3).all(|c| (t.linear[r][c] - u.linear[r][c]).abs() < 1e-9)
        });
        assert!(same, "{a}: {t:?} {u:?}");
    }
}

/// A plane the mock kernel gives faces of these names.
fn plane_of(doc: &Document<MockKernel>, names: &[String], origin: [f64; 3], normal: [f64; 3]) {
    let mut surfaces = doc.kernel().surfaces.borrow_mut();
    for name in names {
        surfaces.insert(name.clone(), SurfaceGeometry::Plane { origin, normal });
    }
}

/// The uids of the converted features of one type.
fn made_of(doc: &Document<MockKernel>, answer: &Value, type_name: &str) -> Vec<FeatureUid> {
    answer["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["uid"].as_str().unwrap().parse::<FeatureUid>().unwrap())
        .filter(|uid| doc.feature(*uid).unwrap().def.type_name() == type_name)
        .collect()
}

fn names_text(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    // The definition as the `feature` query gives it, its names as text.
    query(doc, json!({"query": "feature", "uid": uid.to_string()}))["def"].to_string()
}

/// Whether a definition's text names the feature `uid` as a whole id (`F1`
/// in `F1.b0` or `F1:end_b`, not in `F15:bottom`), outside the source names
/// of derived parts (`:part(...)`, the source's own ids).
fn names_id(text: &str, uid: FeatureUid) -> bool {
    let id = uid.to_string();
    let mut outside = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(":part(") {
        outside.push_str(&rest[..at]);
        outside.push(' ');
        let after = &rest[at + ":part".len()..];
        let mut depth = 0usize;
        let end = after
            .char_indices()
            .find_map(|(k, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(k + 1);
                        }
                    }
                    _ => {}
                }
                None
            })
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    outside.push_str(rest);
    outside
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|word| word == id)
}

/// The occurrence path from the root of a part placed in the BUILD host.
fn from_root(doc: &Document<MockKernel>, part: &Value) -> String {
    let host = query(doc, json!({"query": "build_host"}));
    let path = host["path"].as_str().unwrap_or("");
    let occurrence = part["occurrence"].as_str().unwrap();
    if path.is_empty() {
        occurrence.to_owned()
    } else {
        format!("{path}/{occurrence}")
    }
}

#[test]
fn a_bound_profile_converts_with_everything_on_it_holding() {
    let mut doc = design();
    // T and X 1000 long along +Z, 500 apart; P on T's x+ slot, its length
    // bound to X's x- slot frame (470).
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let x = first(&mut doc, "p30", [500.0, 0.0, 0.0], 1000.0);
    let tp = derived(&doc, component(&t));
    let xp = derived(&doc, component(&x));
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 100},
               "length": {"bind": {"occurrence": x["occurrence"],
                                   "geometry": {"mount": format!("{xp}:mount(slot_x-_1)")}}}}),
    );
    let pp = derived(&doc, component(&p));
    let record = doc.part_info(component(&p)).unwrap().record;
    let joint = record.joint.unwrap();
    let bound = record.bound[0];
    // Q, free, held by an assembly constraint against P's x+ side face
    // (the mock kernel gives that side's plane to the face by any name
    // it may take; the source's side faces carry their neighbours,
    // `c1[c4,c2]`, as the kernel names them).
    let q = first(&mut doc, "p30", [200.0, 300.0, 300.0], 100.0);
    let qp = derived(&doc, component(&q));
    command(
        &mut doc,
        json!({"cmd": "ground_occurrence", "occurrence": q["occurrence"], "grounded": false}),
    )
    .unwrap();
    let side = "side(c1[c4,c2])";
    let mut side_names: Vec<String> = (1..600).map(|k| format!("F{k}:{side}")).collect();
    side_names.push(format!("{pp}:part(F2:{side})"));
    plane_of(&doc, &side_names, [15.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    let host = query(&doc, json!({"query": "build_host"}))["component"].clone();
    command(
        &mut doc,
        json!({"cmd": "add_constraint_set", "component": host, "rows": [
            {"type": "align",
             "a": {"occurrence": q["occurrence"],
                   "geometry": {"body": format!("{qp}.b0"), "face": format!("{qp}:end_a")}},
             "b": {"occurrence": p["occurrence"],
                   "geometry": {"body": format!("{pp}.b0"),
                                "face": format!("{pp}:part(F2:{side})")}}}]}),
    )
    .unwrap();
    // A sketch on P's end B projecting that face (linked).
    mitcad_model::test_support::allow_attachments(&mut doc);
    *doc.kernel().curves.borrow_mut() = vec![Curve3::Line {
        start: [0.0, 0.0, 470.0],
        end: [10.0, 0.0, 470.0],
    }];
    let path = from_root(&doc, &p);
    let sketch: FeatureUid = command(
        &mut doc,
        json!({"cmd": "sketch.create",
               "plane": {"face": format!("{pp}:end_b"), "body": format!("{pp}.b0")},
               "occurrence": path}),
    )
    .unwrap()["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    command(
        &mut doc,
        json!({"cmd": "sketch.project", "sketch": sketch.to_string(),
               "source": format!("{pp}:end_b"), "body": format!("{pp}.b0"),
               "linked": true, "occurrence": path}),
    )
    .unwrap();
    all_ok(&doc);
    let was = placements(&doc);
    let part_def = doc.feature(pp).unwrap().clone();
    let bounds = doc.part_bounds(component(&p)).unwrap();
    assert!(
        (bounds.max[2] - bounds.min[2] - 470.0).abs() < 1e-6,
        "{bounds:?}"
    );
    let designation = doc
        .parts_list()
        .into_iter()
        .find(|r| r.component == component(&p))
        .unwrap()
        .designation;

    // A check changes nothing and says what it would make.
    let checked = convert(&mut doc, &p, true).unwrap();
    assert_eq!(checked["history"], json!(true), "{checked}");
    assert!(doc.feature(pp).is_some());

    let answer = convert(&mut doc, &p, false).unwrap();
    all_ok(&doc);
    assert_eq!(doc.undo_label(), Some("Convert to Features"));
    assert!(doc.feature(pp).is_none());
    assert_eq!(answer["history"], json!(true), "{answer}");
    // The section's sketch with the source's curve ids, and its extrusion.
    let sketches = made_of(&doc, &answer, "sketch");
    let extrudes = made_of(&doc, &answer, "extrude");
    assert_eq!(sketches.len(), 1, "{answer}");
    assert_eq!(extrudes.len(), 1, "{answer}");
    let extrude = extrudes[0];
    assert!(
        doc.component_bodies(component(&p))
            .iter()
            .any(|b| b.feature == extrude),
        "{answer}"
    );
    // The mount frames are joint origins with their names.
    let mounts: Vec<String> = made_of(&doc, &answer, "joint_origin")
        .into_iter()
        .map(|uid| doc.feature(uid).unwrap().name.clone())
        .collect();
    assert!(mounts.contains(&"mount:end_a".to_owned()), "{mounts:?}");
    assert!(mounts.contains(&"mount:slot_x+_1".to_owned()), "{mounts:?}");
    // Every occurrence is where it was.
    assert_same_placements(&was, &placements(&doc));
    // The joint now names the copied joint origin, nothing of the part.
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    let origin = [&j.a.geometry, &j.b.geometry]
        .into_iter()
        .find_map(|g| match g {
            GeomRef::Datum(uid) => Some(*uid),
            _ => None,
        })
        .expect("the joint is on a joint origin");
    assert!(doc.feature(origin).unwrap().name.starts_with("mount:"));
    for uid in [joint, bound, sketch] {
        let text = names_text(&doc, uid);
        assert!(!names_id(&text, pp), "{text}");
    }
    assert!(names_text(&doc, sketch).contains(&format!("{extrude}:end(")));
    // The constraint is on the extrusion's side face.
    let constraint = doc
        .state()
        .features
        .iter()
        .find(|f| matches!(f.def, FeatureDef::ConstraintSet(_)))
        .unwrap()
        .uid;
    assert!(names_text(&doc, constraint).contains(&format!("{extrude}:{side}")));
    // It is no BUILD part any more; its list keeps what it showed.
    assert!(doc.part_info(component(&p)).is_none());
    let row = doc
        .parts_list()
        .into_iter()
        .find(|r| r.component == component(&p))
        .unwrap();
    assert_eq!(row.designation, designation);
    // The length still follows its bound: X moved 100 further, P is 570.
    let bounds = doc.part_bounds(component(&p)).unwrap();
    assert!(
        (bounds.max[2] - bounds.min[2] - 470.0).abs() < 1e-6,
        "{bounds:?}"
    );
    command(
        &mut doc,
        json!({"cmd": "ground_occurrence", "occurrence": x["occurrence"], "grounded": false}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "set_occurrence_transform", "occurrence": x["occurrence"],
               "transform": {"translation": [600, 0, 0]}}),
    )
    .unwrap();
    all_ok(&doc);
    let bounds = doc.part_bounds(component(&p)).unwrap();
    assert!(
        (bounds.max[2] - bounds.min[2] - 570.0).abs() < 1e-6,
        "{bounds:?}"
    );

    // Undo brings the derived part back as it was, and the placements.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    all_ok(&doc);
    assert_eq!(doc.feature(pp), Some(&part_def));
    assert_eq!(
        doc.part_info(component(&p)).unwrap().record.joint,
        Some(joint)
    );
    assert!(doc.feature(extrude).is_none());
    assert_same_placements(&was, &placements(&doc));
}

#[test]
fn a_mitred_part_cuts_its_ends_with_features() {
    // A 20 x 20 profile 500 long, end B mitred 45 degrees leaning +y, end A
    // 30 degrees leaning -x (the mock kernel clips bodies by the cuts'
    // planes).
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let t = first(&mut doc, "p20", [0.0; 3], 500.0);
    for ends in [
        json!({"b": {"angle": "45 deg", "tilt": "x"}}),
        json!({"a": {"angle": "30 deg", "tilt": "-y"}}),
    ] {
        command(
            &mut doc,
            json!({"cmd": "build_edit", "component": t["component"], "ends": ends}),
        )
        .unwrap();
    }
    let tp = derived(&doc, component(&t));
    // End B's cut plane: through the long side's edge (y = -10) at 500.
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let mut names: Vec<String> = (1..400).map(|k| format!("F{k}:bottom")).collect();
    names.push(format!("{tp}:end_b"));
    plane_of(&doc, &names, [0.0, -10.0, 500.0], [0.0, s, s]);
    // A sketch on the mitre.
    mitcad_model::test_support::allow_attachments(&mut doc);
    let occurrence = from_root(&doc, &t);
    let sketch: FeatureUid = command(
        &mut doc,
        json!({"cmd": "sketch.create",
               "plane": {"face": format!("{tp}:end_b"), "body": format!("{tp}.b0")},
               "occurrence": occurrence}),
    )
    .unwrap()["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    all_ok(&doc);
    let before = doc.part_bounds(component(&t)).unwrap();

    let answer = convert(&mut doc, &t, false).unwrap();
    all_ok(&doc);
    // A plane at each end's angle (end B's moved with the length) and a
    // box cut beyond each.
    let boxes = made_of(&doc, &answer, "box");
    assert_eq!(boxes.len(), 2, "{answer}");
    assert_eq!(
        made_of(&doc, &answer, "construction_plane").len(),
        3,
        "{answer}"
    );
    // The body is cut as the derived part cut it.
    let after = doc.part_bounds(component(&t)).unwrap();
    let same = (0..3).all(|k| {
        (before.min[k] - after.min[k]).abs() < 1e-6 && (before.max[k] - after.max[k]).abs() < 1e-6
    });
    assert!(same, "{before:?} {after:?}");
    // The sketch on the mitre is on end B's box's bottom.
    let text = names_text(&doc, sketch);
    assert!(text.contains(&format!("{}:bottom", boxes[1])), "{text}");
    assert!(!names_id(&text, tp), "{text}");
}

#[test]
fn a_reference_no_feature_makes_refuses_with_nothing_changed() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    // A joint origin in the part's component on a frame its library design
    // does not have.
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "joint_origin",
               "geometry": {"mount": format!("{tp}:mount(nothere)")}},
               "component": t["component"]}),
    )
    .unwrap();
    let label = doc.undo_label().map(str::to_owned);
    let features = doc.state().features.len();
    let error = convert(&mut doc, &t, false).unwrap_err();
    // In words for a person; the name for agents.
    let message = mitcad_model::notice::user_message(&error);
    assert!(message.contains("would lose"), "{error}");
    assert!(!message.contains(&tp.to_string()), "{message}");
    assert!(error.contains("nothere"), "{error}");
    assert!(doc.feature(tp).is_some());
    assert_eq!(doc.state().features.len(), features);
    assert_eq!(doc.undo_label().map(str::to_owned), label);
    assert!(doc.part_info(component(&t)).is_some());
}

#[test]
fn the_cut_list_and_the_parts_list_keep_a_converted_member() {
    // A timber member, 2400 long: its cut list row (material, grade,
    // section, length) and stock, and its parts list row, the same after
    // the conversion as before.
    let mut doc = design();
    let post = place(
        &mut doc,
        json!({"part": part("post98"), "at": {"point": [0, 0, 0]}, "length": 2400}),
    );
    let c = component(&post);
    let options = mitcad_model_build::CutListOptions::default();
    let before = doc.build_cut_list(options);
    let row_before = before
        .rows
        .iter()
        .find(|r| r.components.contains(&c))
        .cloned()
        .expect("the member has a cut list row");
    assert!(row_before.timber.is_some(), "{row_before:?}");
    let parts_before = doc
        .parts_list()
        .into_iter()
        .find(|r| r.component == c)
        .unwrap();

    convert(&mut doc, &post, false).unwrap();
    all_ok(&doc);
    assert!(doc.part_info(c).is_none());
    let after = doc.build_cut_list(options);
    let row_after = after
        .rows
        .iter()
        .find(|r| r.components.contains(&c))
        .cloned()
        .expect("the converted member keeps its cut list row");
    assert_eq!(row_after, row_before);
    assert_eq!(after.stock, before.stock);
    assert_eq!(after.timber, before.timber);
    assert!(after.cut.contains(&c));
    let parts_after = doc
        .parts_list()
        .into_iter()
        .find(|r| r.component == c)
        .unwrap();
    assert_eq!(parts_after.designation, parts_before.designation);
    assert_eq!(parts_after.supplier, parts_before.supplier);
    assert_eq!(parts_after.part_number, parts_before.part_number);
    assert_eq!(parts_after.quantity, parts_before.quantity);
}

#[test]
fn a_source_without_a_section_to_extrude_becomes_a_plain_body() {
    // A cap of a box primitive: no sketch, no extrusion. Its top face
    // carries a sketch of the root.
    let mut doc = design();
    let cap = place(
        &mut doc,
        json!({"part": part("cap30-box"), "at": {"point": [100, 0, 0]}}),
    );
    let cp = derived(&doc, component(&cap));
    // The mock kernel gives the top face's plane to it by any name it may
    // take: the source's top is the box's second face.
    let mut names: Vec<String> = (1..400).map(|k| format!("F{k}:import(1)")).collect();
    names.push(format!("{cp}:part(F1:top)"));
    plane_of(&doc, &names, [0.0, 0.0, 4.0], [0.0, 0.0, 1.0]);
    mitcad_model::test_support::allow_attachments(&mut doc);
    let occurrence = from_root(&doc, &cap);
    let sketch: FeatureUid = command(
        &mut doc,
        json!({"cmd": "sketch.create",
               "plane": {"face": format!("{cp}:part(F1:top)"), "body": format!("{cp}.b0")},
               "occurrence": occurrence}),
    )
    .unwrap()["uid"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    all_ok(&doc);
    let was = placements(&doc);
    let part_def = doc.feature(cp).unwrap().clone();

    let checked = convert(&mut doc, &cap, true).unwrap();
    assert_eq!(checked["history"], json!(false), "{checked}");
    let answer = convert(&mut doc, &cap, false).unwrap();
    all_ok(&doc);
    assert_eq!(answer["history"], json!(false), "{answer}");
    // One base feature holds the body; its frame is a joint origin.
    let bases = made_of(&doc, &answer, "base");
    assert_eq!(bases.len(), 1, "{answer}");
    assert!(made_of(&doc, &answer, "sketch").is_empty(), "{answer}");
    assert!(made_of(&doc, &answer, "box").is_empty(), "{answer}");
    let mounts: Vec<String> = made_of(&doc, &answer, "joint_origin")
        .into_iter()
        .map(|uid| doc.feature(uid).unwrap().name.clone())
        .collect();
    assert_eq!(mounts, ["mount:end"], "{answer}");
    assert_eq!(
        doc.component_bodies(component(&cap)),
        [mitcad_model::ids::BodyUid::new(bases[0], 0)]
    );
    // The sketch is on the plain body's face of the same place.
    let text = names_text(&doc, sketch);
    assert!(text.contains(&format!("{}:import(1)", bases[0])), "{text}");
    assert!(!names_id(&text, cp), "{text}");
    assert_same_placements(&was, &placements(&doc));
    // Undo brings the derived part back.
    command(&mut doc, json!({"cmd": "undo"})).unwrap();
    assert_eq!(doc.feature(cp), Some(&part_def));
    assert!(doc.feature(bases[0]).is_none());
}
