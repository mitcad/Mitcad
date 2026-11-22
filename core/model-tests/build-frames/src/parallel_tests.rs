// SPDX-License-Identifier: MIT
//! Profiles in parallel (mitcad#2201), with the mock kernel: `build_place`
//! with `at.place` `parallel` lays a profile along the profile picked, a
//! long face against the outer plane of the side picked (above, below or
//! beside it), centred across the side or on one of its slots, turned by
//! Rotate in quarter turns. Its length is the target's own parameter unless
//! typed, and From and Offset place it along the target from end A, end B
//! or the middle, overhanging its ends when longer. It follows the target
//! when the target moves, turns or changes length, keeps its reference and
//! offset in a reopened file, and `build_edit` changes its offset, From,
//! Across, Rotate and length. Side and end placements are as before.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::{FeatureDef, GeomRef};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// A target profile `length` long standing at the origin along +Z.
fn target(doc: &mut Document<MockKernel>, component: &str, length: f64) -> Value {
    let t = first(doc, component, [0.0; 3], length);
    all_ok(doc);
    t
}

/// A side face of the target as a pick: its face named after the section's
/// curve, the plane the mock gives it at `half` from the target's axis, and
/// the occurrence. `side` is `x+`, `x-`, `y+` or `y-`.
fn side_face(doc: &Document<MockKernel>, t: &Value, side: &str, half: f64) -> Value {
    let tp = derived(doc, component(t));
    let (curve, normal) = match side {
        "x+" => ("c1", [1.0, 0.0, 0.0]),
        "y+" => ("c2", [0.0, 1.0, 0.0]),
        "x-" => ("c3", [-1.0, 0.0, 0.0]),
        _ => ("c4", [0.0, -1.0, 0.0]),
    };
    let face = format!("{tp}:part(F2:side({curve}))");
    doc.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        mitcad_model::datum::SurfaceGeometry::Plane {
            origin: normal.map(|n| n * half),
            normal,
        },
    );
    json!({"body": format!("{tp}.b0"), "face": face})
}

/// The fields of a parallel placing of `component` on `geometry` of the
/// target, `fields` merged into the command (`at`'s into `at`).
fn parallel(t: &Value, component: &str, geometry: &Value, fields: Value) -> Value {
    let mut command = json!({"part": part(component),
        "at": {"occurrence": t["occurrence"], "geometry": geometry, "place": "parallel"}});
    for (key, value) in fields.as_object().unwrap() {
        if key == "at" {
            for (k, v) in value.as_object().unwrap() {
                command["at"][k] = v.clone();
            }
        } else {
            command[key] = value.clone();
        }
    }
    command
}

fn preview(doc: &Document<MockKernel>, command: &Value) -> Value {
    let mut probe = command.clone();
    probe["query"] = json!("build_preview");
    query(doc, probe)
}

/// The placed part's box in the design.
fn world_box(
    doc: &Document<MockKernel>,
    component_uid: mitcad_model::ids::ComponentUid,
    answer: &Value,
) -> BoundingBox {
    let local = doc.part_bounds(component_uid).unwrap();
    mitcad_model::internal::build::moved_box(&local, &placed(doc, occurrence(answer)))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The box spans x, y and z as given.
fn assert_box(b: &BoundingBox, x: [f64; 2], y: [f64; 2], z: [f64; 2]) {
    for (axis, [low, high]) in [x, y, z].into_iter().enumerate() {
        assert!(
            near(b.min[axis], low) && near(b.max[axis], high),
            "axis {axis}: {b:?}, wanted {low} to {high}"
        );
    }
}

/// A part's entry of `build_parts`.
fn part_entry(doc: &Document<MockKernel>, answer: &Value) -> Value {
    let parts = query(doc, json!({"query": "build_parts"}));
    parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["component"] == answer["component"])
        .cloned()
        .unwrap()
}

fn edit(doc: &mut Document<MockKernel>, answer: &Value, fields: Value) -> Result<Value, String> {
    let mut command_value = fields;
    command_value["cmd"] = json!("build_edit");
    command_value["component"] = answer["component"].clone();
    command(doc, command_value)
}

/// The frame of the target the placed part's joint is on.
fn lies_on(doc: &Document<MockKernel>, answer: &Value) -> String {
    let joint = answer["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    match &j.b.geometry {
        GeomRef::Mount { name, .. } => name.clone(),
        other => panic!("not on a frame: {other:?}"),
    }
}

#[test]
fn one_click_centres_a_20x40_on_top_of_a_crossmember_both_ends_flush() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    let fields = parallel(
        &t,
        "p20x40",
        &top,
        json!({"at": {"point": [3.0, 10.0, 123.0]}}),
    );
    let shown = preview(&doc, &fields);
    assert_eq!(shown["problems"], json!([]), "{shown}");
    assert_eq!(shown["place"], "parallel", "{shown}");
    assert_eq!(shown["parallel"]["length"], "match", "{shown}");
    assert_eq!(shown["parallel"]["frame"], "slot_y+_1", "{shown}");
    // A side with one slot: nothing to choose across it.
    assert_eq!(shown["parallel"]["across"], Value::Null, "{shown}");
    assert!(
        near(shown["length"]["value"].as_f64().unwrap(), 400.0),
        "{shown}"
    );
    assert_eq!(shown["side"]["from"], "a", "{shown}");
    assert!(
        near(shown["side"]["offset"].as_f64().unwrap(), 0.0),
        "{shown}"
    );
    assert_eq!(shown["snaps"], json!([]), "{shown}");
    let p = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(p["place"], "parallel");
    let c = component(&p);
    // On top (+Y) of the crossmember, its 40 mm face down, centred across
    // the 20 mm side, both ends flush with the crossmember's.
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [0.0, 400.0],
    );
    // Its length is the crossmember's own parameter.
    let own = doc.part_info(c).unwrap();
    let other = doc.part_info(component(&t)).unwrap();
    assert_eq!(own.length.unwrap().0, other.length.unwrap().0);
    assert!(own.record.parallel);
    let entry = part_entry(&doc, &p);
    assert_eq!(entry["parallel"]["length"], "match", "{entry}");
    assert_eq!(entry["from"], "a", "{entry}");
    assert!(near(entry["offset"].as_f64().unwrap(), 0.0), "{entry}");
}

#[test]
fn a_typed_length_keeps_one_end_flush_and_overhangs_the_other() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    let fields = parallel(
        &t,
        "p20x40",
        &top,
        json!({"length": 600, "at": {"offset": 0}}),
    );
    let shown = preview(&doc, &fields);
    assert_eq!(shown["parallel"]["length"], "typed", "{shown}");
    assert!(
        near(shown["length"]["value"].as_f64().unwrap(), 600.0),
        "{shown}"
    );
    let p = place(&mut doc, fields);
    all_ok(&doc);
    // Flush with end A, 200 over end B.
    assert_box(
        &world_box(&doc, component(&p), &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [0.0, 600.0],
    );
    // From end B: flush with it, 200 over end A.
    let b = place(
        &mut doc,
        parallel(
            &t,
            "p20x40",
            &top,
            json!({"length": 600, "at": {"from": "b", "offset": 0}}),
        ),
    );
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&b), &b),
        [-20.0, 20.0],
        [10.0, 30.0],
        [-200.0, 400.0],
    );
    // From the middle: 100 over each end.
    let m = place(
        &mut doc,
        parallel(
            &t,
            "p20x40",
            &top,
            json!({"length": 600, "at": {"from": "middle", "offset": 0}}),
        ),
    );
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&m), &m),
        [-20.0, 20.0],
        [10.0, 30.0],
        [-100.0, 500.0],
    );
}

#[test]
fn other_long_sides_place_it_below_or_beside() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let below = side_face(&doc, &t, "y-", 10.0);
    let p = place(&mut doc, parallel(&t, "p20x40", &below, json!({})));
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&p), &p),
        [-20.0, 20.0],
        [-30.0, -10.0],
        [0.0, 400.0],
    );
    let beside = side_face(&doc, &t, "x+", 10.0);
    let p = place(&mut doc, parallel(&t, "p20x40", &beside, json!({})));
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&p), &p),
        [10.0, 30.0],
        [-20.0, 20.0],
        [0.0, 400.0],
    );
    // A side frame picked as well as a face.
    let tp = derived(&doc, component(&t));
    let frame = json!({"mount": format!("{tp}:mount(slot_x-_1)")});
    let p = place(&mut doc, parallel(&t, "p20x40", &frame, json!({})));
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&p), &p),
        [-30.0, -10.0],
        [-20.0, 20.0],
        [0.0, 400.0],
    );
}

#[test]
fn rotate_switches_the_contact_face_and_keeps_contact() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    // A quarter turn: on its 20 mm face, standing 40 high.
    let p = place(
        &mut doc,
        parallel(&t, "p20x40", &top, json!({"rotate": "90 deg"})),
    );
    all_ok(&doc);
    let c = component(&p);
    assert_box(
        &world_box(&doc, c, &p),
        [-10.0, 10.0],
        [10.0, 50.0],
        [0.0, 400.0],
    );
    let entry = part_entry(&doc, &p);
    assert!(
        near(
            entry["rotate"].as_f64().unwrap(),
            std::f64::consts::FRAC_PI_2
        ),
        "{entry}"
    );
    // Turned back by build_edit, on its 40 mm face again; then a half turn.
    let changed = edit(&mut doc, &p, json!({"rotate": "0 deg"})).unwrap();
    assert_eq!(changed["changed"], json!(["rotate"]), "{changed}");
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [0.0, 400.0],
    );
    edit(&mut doc, &p, json!({"rotate": "180 deg"})).unwrap();
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [0.0, 400.0],
    );
    // Not a quarter turn: refused.
    assert!(edit(&mut doc, &p, json!({"rotate": "45 deg"})).is_err());
}

#[test]
fn on_a_side_with_two_slots_it_centres_or_takes_a_slot() {
    let mut doc = design();
    let t = target(&mut doc, "p30x60", 400.0);
    let side = side_face(&doc, &t, "x+", 15.0);
    // Centred by default: on the side's middle between its slots.
    let fields = parallel(
        &t,
        "p20",
        &side,
        json!({"at": {"point": [15.0, 14.0, 50.0]}}),
    );
    let shown = preview(&doc, &fields);
    assert_eq!(shown["parallel"]["across"], "middle", "{shown}");
    let p = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(lies_on(&doc, &p), "side_x+");
    let c = component(&p);
    assert_box(
        &world_box(&doc, c, &p),
        [15.0, 35.0],
        [-10.0, 10.0],
        [0.0, 400.0],
    );
    // On the slot nearest the click.
    let s = place(
        &mut doc,
        parallel(
            &t,
            "p20",
            &side,
            json!({"at": {"point": [15.0, 14.0, 50.0], "across": "slot"}}),
        ),
    );
    all_ok(&doc);
    assert_eq!(lies_on(&doc, &s), "slot_x+_2");
    assert_box(
        &world_box(&doc, component(&s), &s),
        [15.0, 35.0],
        [5.0, 25.0],
        [0.0, 400.0],
    );
    // A slot frame picked, on that slot.
    let tp = derived(&doc, component(&t));
    let frame = json!({"mount": format!("{tp}:mount(slot_x+_1)")});
    let f = place(
        &mut doc,
        parallel(&t, "p20", &frame, json!({"at": {"across": "slot"}})),
    );
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&f), &f),
        [15.0, 35.0],
        [-25.0, -5.0],
        [0.0, 400.0],
    );
    assert_eq!(part_entry(&doc, &f)["parallel"]["across"], "slot");
    // Centred again by build_edit.
    let changed = edit(&mut doc, &f, json!({"at": {"across": "middle"}})).unwrap();
    assert_eq!(changed["changed"], json!(["across"]), "{changed}");
    all_ok(&doc);
    assert_eq!(lies_on(&doc, &f), "side_x+");
    assert_box(
        &world_box(&doc, component(&f), &f),
        [15.0, 35.0],
        [-10.0, 10.0],
        [0.0, 400.0],
    );
}

#[test]
fn it_follows_the_target_s_length() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    let below = side_face(&doc, &t, "y-", 10.0);
    let matched = place(&mut doc, parallel(&t, "p20x40", &top, json!({})));
    let typed = place(
        &mut doc,
        parallel(
            &t,
            "p20x40",
            &below,
            json!({"length": 600, "at": {"from": "b", "offset": 0}}),
        ),
    );
    all_ok(&doc);
    // The target 500 long: the matched one too; the typed one stays 600,
    // flush with the target's end B.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "length": 500}),
    )
    .unwrap();
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&matched), &matched),
        [-20.0, 20.0],
        [10.0, 30.0],
        [0.0, 500.0],
    );
    assert_box(
        &world_box(&doc, component(&typed), &typed),
        [-20.0, 20.0],
        [-30.0, -10.0],
        [-100.0, 500.0],
    );
    assert_eq!(part_entry(&doc, &typed)["parallel"]["length"], "typed");
}

/// The placed part's placement in the target's coordinates.
fn relative(doc: &Document<MockKernel>, t: &Value, p: &Value) -> Transform {
    let target_at = placed(doc, occurrence(t));
    mitcad_model::internal::build::inverse(&target_at).after(&placed(doc, occurrence(p)))
}

/// Two placements that put a few points in the same places.
fn same_place(a: &Transform, b: &Transform) -> bool {
    [
        [0.0; 3],
        [100.0, 0.0, 0.0],
        [0.0, 100.0, 0.0],
        [0.0, 0.0, 100.0],
    ]
    .into_iter()
    .all(|p| close(a.apply_point(p), b.apply_point(p)))
}

#[test]
fn it_follows_the_target_when_it_moves_and_turns() {
    // The target placed on the XY plane by its joint, moved off it and
    // turned about its axis by Placement (build_edit): the joints after its
    // joint, the parallel profile's too, follow it. (A move_occurrence
    // later in the timeline leaves the parts jointed to it where their
    // joints put them, commands.md.)
    let mut doc = design();
    let t = place(
        &mut doc,
        json!({"part": part("p20"),
               "at": {"geometry": "xy", "point": [0.0, 0.0, 0.0], "direction": [0.0, 0.0, 1.0]},
               "length": 400}),
    );
    all_ok(&doc);
    let top = side_face(&doc, &t, "y+", 10.0);
    let p = place(&mut doc, parallel(&t, "p20x40", &top, json!({})));
    all_ok(&doc);
    let c = component(&p);
    let on_target = relative(&doc, &t, &p);
    let before = world_box(&doc, c, &p);
    assert!(
        near(before.min[2], 0.0) && near(before.max[2], 400.0),
        "{before:?}"
    );
    // Off the plane by 50 mm: the profile 50 mm up with it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "at": {"offset": "50 mm"}}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(same_place(&relative(&doc, &t, &p), &on_target));
    let lifted = world_box(&doc, c, &p);
    assert!(
        near(lifted.min[2], 50.0) && near(lifted.max[2], 450.0),
        "{lifted:?}"
    );
    assert!(
        near(lifted.min[0], before.min[0]) && near(lifted.min[1], before.min[1]),
        "{lifted:?}"
    );
    // A quarter turn about its axis: the profile turns with it, still on
    // the same side.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"], "rotate": "90 deg"}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(same_place(&relative(&doc, &t, &p), &on_target));
    let turned = world_box(&doc, c, &p);
    assert!(
        near(turned.min[2], 50.0) && near(turned.max[2], 450.0),
        "{turned:?}"
    );
    assert!(
        !(near(turned.min[0], lifted.min[0]) && near(turned.min[1], lifted.min[1])),
        "the profile did not turn with the target: {turned:?}"
    );
}

#[test]
fn the_reference_and_offset_persist_through_save_and_reopen() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    let below = side_face(&doc, &t, "y-", 10.0);
    let matched = place(
        &mut doc,
        parallel(&t, "p20x40", &top, json!({"at": {"offset": 25}})),
    );
    let typed = place(
        &mut doc,
        parallel(
            &t,
            "p20x40",
            &below,
            json!({"length": 600, "at": {"from": "b", "offset": 50}}),
        ),
    );
    all_ok(&doc);
    assert_box(
        &world_box(&doc, component(&matched), &matched),
        [-20.0, 20.0],
        [10.0, 30.0],
        [25.0, 425.0],
    );
    // From end B 50, towards A: its end B at 350.
    assert_box(
        &world_box(&doc, component(&typed), &typed),
        [-20.0, 20.0],
        [-30.0, -10.0],
        [-250.0, 350.0],
    );
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    all_ok(&reopened);
    for (answer, from, offset, length) in
        [(&matched, "a", 25.0, "match"), (&typed, "b", 50.0, "typed")]
    {
        let entry = part_entry(&reopened, answer);
        assert_eq!(entry["from"], from, "{entry}");
        assert!(near(entry["offset"].as_f64().unwrap(), offset), "{entry}");
        assert_eq!(entry["parallel"]["length"], length, "{entry}");
    }
    assert_box(
        &world_box(&reopened, component(&typed), &typed),
        [-20.0, 20.0],
        [-30.0, -10.0],
        [-250.0, 350.0],
    );
    // The reopened target longer: the typed one keeps its place from end B.
    let id = reopened.part_info(component(&t)).unwrap().length.unwrap().0;
    let name = reopened.parameters().name(id);
    command(
        &mut reopened,
        json!({"cmd": "set_parameter", "name": name, "value": 500}),
    )
    .unwrap();
    all_ok(&reopened);
    assert_box(
        &world_box(&reopened, component(&typed), &typed),
        [-20.0, 20.0],
        [-30.0, -10.0],
        [-150.0, 450.0],
    );
    assert_box(
        &world_box(&reopened, component(&matched), &matched),
        [-20.0, 20.0],
        [10.0, 30.0],
        [25.0, 525.0],
    );
}

#[test]
fn placement_and_length_are_edited_with_build_edit() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let top = side_face(&doc, &t, "y+", 10.0);
    let p = place(&mut doc, parallel(&t, "p20x40", &top, json!({})));
    all_ok(&doc);
    let c = component(&p);
    let changed = edit(&mut doc, &p, json!({"at": {"offset": 30, "from": "a"}})).unwrap();
    assert_eq!(changed["changed"], json!(["offset"]), "{changed}");
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [30.0, 430.0],
    );
    // A new From alone keeps it where it is.
    edit(&mut doc, &p, json!({"at": {"from": "b"}})).unwrap();
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [30.0, 430.0],
    );
    let entry = part_entry(&doc, &p);
    assert_eq!(entry["from"], "b", "{entry}");
    assert!(near(entry["offset"].as_f64().unwrap(), -30.0), "{entry}");
    // A typed length, its end B kept 30 past the target's.
    let changed = edit(&mut doc, &p, json!({"length": 600})).unwrap();
    assert_eq!(changed["changed"], json!(["length"]), "{changed}");
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [-170.0, 430.0],
    );
    assert_eq!(part_entry(&doc, &p)["parallel"]["length"], "typed");
    // The target's length again.
    edit(&mut doc, &p, json!({"length": {"match": "target"}})).unwrap();
    all_ok(&doc);
    assert_box(
        &world_box(&doc, c, &p),
        [-20.0, 20.0],
        [10.0, 30.0],
        [30.0, 430.0],
    );
    assert_eq!(part_entry(&doc, &p)["parallel"]["length"], "match");
    // The target's own length is untouched by all of it.
    let tl = doc.part_info(component(&t)).unwrap().length.unwrap().1;
    assert!(near(tl, 400.0), "{tl}");
    // Flip and tilt do not apply.
    assert!(edit(&mut doc, &p, json!({"flip": true})).is_err());
    assert!(edit(&mut doc, &p, json!({"tilt": "10 deg"})).is_err());
}

#[test]
fn what_a_parallel_placement_refuses() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let tp = derived(&doc, component(&t));
    let top = side_face(&doc, &t, "y+", 10.0);
    let refused = |doc: &mut Document<MockKernel>, fields: Value| {
        let mut command_value = fields;
        command_value["cmd"] = json!("build_place");
        assert!(
            command(doc, command_value.clone()).is_err(),
            "{command_value}"
        );
    };
    // An end face.
    let end = json!({"body": format!("{tp}.b0"), "face": format!("{tp}:end_b")});
    refused(&mut doc, parallel(&t, "p20x40", &end, json!({})));
    // A bound length, a flip, Middle of Gap.
    refused(
        &mut doc,
        parallel(
            &t,
            "p20x40",
            &top,
            json!({"length": {"bind": {"occurrence": t["occurrence"],
            "geometry": {"mount": format!("{tp}:mount(end_b)")}}}}),
        ),
    );
    refused(
        &mut doc,
        parallel(&t, "p20x40", &top, json!({"flip": true})),
    );
    refused(
        &mut doc,
        parallel(&t, "p20x40", &top, json!({"at": {"from": "gap"}})),
    );
    // The target's length on a part that is not placed in parallel.
    let mut side = parallel(&t, "p20x40", &top, json!({"length": {"match": "target"}}));
    side["at"]["place"] = json!("side");
    refused(&mut doc, side);
}

#[test]
fn side_and_end_placements_are_as_before() {
    let mut doc = design();
    let t = target(&mut doc, "p20", 400.0);
    let tp = derived(&doc, component(&t));
    let p = place(
        &mut doc,
        json!({"part": part("p20"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 100},
               "length": 300}),
    );
    all_ok(&doc);
    assert_eq!(p["place"], "side");
    assert!(!doc.part_info(component(&p)).unwrap().record.parallel);
    assert_eq!(part_entry(&doc, &p)["parallel"], Value::Null);
    assert!(close(
        placed(&doc, occurrence(&p)).translation,
        [10.0, 0.0, 110.0]
    ));
    // Across and the target's length are for parallel placements only.
    assert!(edit(&mut doc, &p, json!({"at": {"across": "middle"}})).is_err());
    assert!(edit(&mut doc, &p, json!({"length": {"match": "target"}})).is_err());
    let e = place(
        &mut doc,
        json!({"part": part("p20"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")}},
               "length": 300}),
    );
    all_ok(&doc);
    assert_eq!(e["place"], "end");
    assert!(close(
        placed(&doc, occurrence(&e)).translation,
        [0.0, 0.0, 400.0]
    ));
}
