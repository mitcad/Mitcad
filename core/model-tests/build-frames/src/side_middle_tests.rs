// SPDX-License-Identifier: MIT
//! A profile centred across a side with two slots (mitcad#2002), with the
//! mock kernel: on the long side of a 30x60 (slots 15 either side of its
//! middle) a click near the side's centre line puts a profile on the
//! side's middle between the slots (`side_x+`), so that matching sections
//! have flush side faces and one centre plane; a click near a slot keeps
//! it on that slot. How it stands across is apart from the snaps along
//! the side; the preview says which (`side.across`). Unequal sections,
//! a turned, a flipped part and the other side are centred the same way;
//! a side with one slot is as before. Preview, placement, recompute and a
//! reopened file agree, and the part stays on the middle when the host
//! gets longer or its section larger. Only a profile standing on its end
//! is centred: a part that goes in a slot is refused there.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::{FeatureDef, GeomRef};
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// Where a click goes along the host: its near face 408 from end A, clear
/// of every snap (the ends, the middle, the grid of 30).
const ALONG: f64 = 423.0;

/// A host profile 1000 long standing at the origin along +Z.
fn host(doc: &mut Document<MockKernel>, component: &str) -> Value {
    let t = first(doc, component, [0.0; 3], 1000.0);
    all_ok(doc);
    t
}

/// A side face of the host as a pick: its face named after the section's
/// curve, the plane the mock gives it at `half` from the host's axis, and
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

/// The fields of a placing of `component` 300 long clicked at `point` on
/// `face` of the host, `fields` merged into the command.
fn on_face(t: &Value, component: &str, face: &Value, point: [f64; 3], fields: Value) -> Value {
    let mut command = json!({"part": part(component),
        "at": {"occurrence": t["occurrence"], "geometry": face, "point": point},
        "length": 300});
    for (key, value) in fields.as_object().unwrap() {
        command[key] = value.clone();
    }
    command
}

fn preview(doc: &Document<MockKernel>, command: &Value) -> Value {
    let mut probe = command.clone();
    probe["query"] = json!("build_preview");
    query(doc, probe)
}

/// The frame of the host the placed part's joint stands on.
fn stands_on(doc: &Document<MockKernel>, answer: &Value) -> String {
    let joint = answer["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    match &j.b.geometry {
        GeomRef::Mount { name, .. } => name.clone(),
        other => panic!("not on a frame: {other:?}"),
    }
}

/// The placed part's box in the design.
fn world_box(doc: &Document<MockKernel>, answer: &Value) -> BoundingBox {
    let local = doc.part_bounds(component(answer)).unwrap();
    mitcad_model::internal::build::moved_box(&local, &placed(doc, occurrence(answer)))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The box reaches from `low` to `high` across the side (along Y).
fn assert_across(b: &BoundingBox, low: f64, high: f64) {
    assert!(near(b.min[1], low) && near(b.max[1], high), "{b:?}");
}

fn origin_of(shown: &Value) -> [f64; 3] {
    let o = &shown["frame"]["origin"];
    [0, 1, 2].map(|i| o[i].as_f64().unwrap())
}

#[test]
fn a_click_near_the_side_s_centre_line_centres_a_matching_profile_on_it() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    let fields = on_face(&t, "p30x60", &face, [15.0, 2.0, ALONG], json!({}));
    let shown = preview(&doc, &fields);
    assert_eq!(shown["place"], "side", "{shown}");
    assert_eq!(shown["side"]["across"], "middle", "{shown}");
    // Along the side as any click: between the snaps, from end A.
    assert_eq!(shown["side"]["kind"], Value::Null, "{shown}");
    assert_eq!(shown["side"]["from"], "a", "{shown}");
    let p = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    // Centred on the side: its end A on the side's centre line, its 60
    // across the side flush with the host's faces (y -30 to 30).
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [15.0, 0.0, ALONG]), "{at:?}");
    assert!(close(at.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    assert!(close(origin_of(&shown), at.translation), "{shown}");
    let b = world_box(&doc, &p);
    assert_across(&b, -30.0, 30.0);
    assert!(near(b.min[0], 15.0) && near(b.max[0], 315.0), "{b:?}");
    assert!(
        near(b.min[2], ALONG - 15.0) && near(b.max[2], ALONG + 15.0),
        "{b:?}"
    );
}

#[test]
fn a_click_near_a_slot_keeps_the_profile_on_that_slot() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    for (y, slot, centre) in [(-12.0, "slot_x+_1", -15.0), (13.0, "slot_x+_2", 15.0)] {
        let fields = on_face(&t, "p30x60", &face, [15.0, y, ALONG], json!({}));
        let shown = preview(&doc, &fields);
        assert_eq!(shown["side"]["across"], "slot", "{y}: {shown}");
        let p = place(&mut doc, fields);
        all_ok(&doc);
        assert_eq!(stands_on(&doc, &p), slot, "{y}");
        let at = placed(&doc, occurrence(&p));
        assert!(close(at.translation, [15.0, centre, ALONG]), "{y}: {at:?}");
        assert!(close(origin_of(&shown), at.translation), "{y}: {shown}");
        assert_across(&world_box(&doc, &p), centre - 30.0, centre + 30.0);
        command(&mut doc, json!({"cmd": "undo"})).unwrap();
    }
    // Halfway between the centre line and a slot, the slot.
    let shown = preview(
        &doc,
        &on_face(&t, "p30x60", &face, [15.0, -7.5, ALONG], json!({})),
    );
    assert_eq!(shown["side"]["across"], "slot", "{shown}");
}

#[test]
fn across_is_apart_from_the_snaps_along_the_side() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    // Near end B and near the middle: the snap along, the middle across.
    for (z, kind, from) in [(990.0, "end_b", "b"), (503.0, "middle", "middle")] {
        let fields = on_face(&t, "p30x60", &face, [15.0, -3.0, z], json!({}));
        let shown = preview(&doc, &fields);
        assert_eq!(shown["side"]["kind"], kind, "{z}: {shown}");
        assert_eq!(shown["side"]["from"], from, "{z}: {shown}");
        assert_eq!(shown["side"]["across"], "middle", "{z}: {shown}");
    }
    // A typed offset from end B: centred across, 100 from end B along.
    let fields = on_face(&t, "p30x60", &face, [15.0, 1.0, 600.0], json!({}));
    let mut typed = fields.clone();
    typed["at"]["offset"] = json!(100);
    typed["at"]["from"] = json!("b");
    let p = place(&mut doc, typed);
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [15.0, 0.0, 885.0]), "{at:?}");
}

#[test]
fn unequal_turned_flipped_and_mirrored_profiles_are_centred_too() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let right = side_face(&doc, &t, "x+", 15.0);
    let left = side_face(&doc, &t, "x-", 15.0);
    // A 30x30 on the 60 side: its centre plane on the host's.
    let p = place(
        &mut doc,
        on_face(&t, "p30", &right, [15.0, 4.0, ALONG], json!({})),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    assert_across(&world_box(&doc, &p), -15.0, 15.0);
    // A 30x60 turned a quarter: its 30 across, still centred.
    let p = place(
        &mut doc,
        on_face(
            &t,
            "p30x60",
            &right,
            [15.0, -4.0, ALONG + 150.0],
            json!({"rotate": "90 deg"}),
        ),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    assert_across(&world_box(&doc, &p), -15.0, 15.0);
    // Flipped (end B on the side): centred.
    let p = place(
        &mut doc,
        on_face(
            &t,
            "p30x60",
            &right,
            [15.0, 3.0, ALONG + 300.0],
            json!({"flip": true}),
        ),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    let b = world_box(&doc, &p);
    assert_across(&b, -30.0, 30.0);
    assert!(near(b.min[0], 15.0) && near(b.max[0], 315.0), "{b:?}");
    // The other long side, out along -X: its middle, centred.
    let p = place(
        &mut doc,
        on_face(&t, "p30x60", &left, [-15.0, -2.0, ALONG], json!({})),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x-");
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [-15.0, 0.0, ALONG]), "{at:?}");
    assert!(close(at.apply_vector([0.0, 0.0, 1.0]), [-1.0, 0.0, 0.0]));
    let b = world_box(&doc, &p);
    assert_across(&b, -30.0, 30.0);
    assert!(near(b.min[0], -315.0) && near(b.max[0], -15.0), "{b:?}");
    // A 30x60 on a 60x60's side: flush with its faces.
    let mut doc = design();
    let t = host(&mut doc, "p60");
    let face = side_face(&doc, &t, "x+", 30.0);
    let p = place(
        &mut doc,
        on_face(&t, "p30x60", &face, [30.0, 5.0, ALONG], json!({})),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    assert_across(&world_box(&doc, &p), -30.0, 30.0);
}

#[test]
fn a_side_with_one_slot_is_as_before() {
    // A 30x30: one slot on each side, nothing to choose across.
    let mut doc = design();
    let t = host(&mut doc, "p30");
    let face = side_face(&doc, &t, "x+", 15.0);
    let fields = on_face(&t, "p30", &face, [15.0, 1.0, ALONG], json!({}));
    let shown = preview(&doc, &fields);
    assert_eq!(shown["side"]["across"], Value::Null, "{shown}");
    let p = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "slot_x+_1");
    assert!(doc.part_info(component(&t)).unwrap().middles.is_empty());
    // The short side of a 30x60 has one slot too.
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "y+", 30.0);
    let fields = on_face(&t, "p30", &face, [2.0, 30.0, ALONG], json!({}));
    let shown = preview(&doc, &fields);
    assert_eq!(shown["side"]["across"], Value::Null, "{shown}");
    let p = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "slot_y+_1");
    // Its middles are those of its long sides only; they are no design
    // frames.
    let info = doc.part_info(component(&t)).unwrap();
    assert_eq!(
        info.middles.keys().cloned().collect::<Vec<_>>(),
        ["side_x+", "side_x-"]
    );
    assert!(!info.mounts.contains_key("side_x+"));
    assert_eq!(info.frame_kind("side_x+"), "side");
    assert!(info.is_side("side_x+") && info.is_side_middle("side_x+"));
    assert!(!info.is_side_middle("slot_x+_1"));
}

#[test]
fn a_centred_profile_stays_centred_through_recompute_reopen_and_host_changes() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    let centred = place(
        &mut doc,
        on_face(&t, "p30x60", &face, [15.0, 1.0, ALONG], json!({})),
    );
    // On slot 1 between the snaps (its near face 285 from end A).
    let on_slot = place(
        &mut doc,
        on_face(&t, "p30", &face, [15.0, -14.0, 300.0], json!({})),
    );
    all_ok(&doc);
    let both = [occurrence(&centred), occurrence(&on_slot)];
    let before = both.map(|o| placed(&doc, o).translation);
    assert!(close(before[0], [15.0, 0.0, ALONG]), "{before:?}");
    assert!(close(before[1], [15.0, -15.0, 300.0]), "{before:?}");
    let kept = |doc: &Document<MockKernel>| {
        both.iter()
            .zip(before)
            .all(|(o, was)| close(placed(doc, *o).translation, was))
    };
    doc.recompute();
    all_ok(&doc);
    assert!(kept(&doc), "a recompute moved them");
    let mut reopened = Document::from_json(&doc.to_json(), MockKernel::default()).unwrap();
    reopened.recompute();
    all_ok(&reopened);
    assert!(kept(&reopened), "reopened, they moved");
    assert_eq!(stands_on(&reopened, &centred), "side_x+");
    // The host longer: both keep their places from end A.
    let id = doc.part_info(component(&t)).unwrap().length.unwrap().0;
    let name = doc.parameters().name(id);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": name, "value": 1400}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(kept(&doc), "the host's length moved them");
    // The host's section larger (the source at its next version, a 40x80
    // whose slots are 20 from its middle): the centred one on the new
    // side's middle, the other on its slot.
    command(
        &mut doc,
        json!({"cmd": "update_library_parts", "changes": [{"source": "S1", "rev": "v1.1.0"}]}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(close(placed(&doc, both[0]).translation, [20.0, 0.0, ALONG]));
    assert!(close(
        placed(&doc, both[1]).translation,
        [20.0, -20.0, 300.0]
    ));
    assert_eq!(stands_on(&doc, &centred), "side_x+");
}

#[test]
fn a_centred_profile_and_one_on_a_slot_of_its_side_are_in_each_other_s_way() {
    // The free stretches of a slot count a profile centred on its side,
    // and those of the side's middle the profiles on its slots
    // (mitcad#1401's Middle of Gap).
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    place(
        &mut doc,
        on_face(&t, "p30x60", &face, [15.0, 1.0, ALONG], json!({})),
    );
    all_ok(&doc);
    let gaps = |shown: &Value| -> Vec<f64> {
        shown["snaps"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "gap")
            .map(|s| s["offset"].as_f64().unwrap())
            .collect()
    };
    // On slot 1: the stretches 0 to 408 and 438 to 1000, their middles the
    // near faces 189 and 704.
    let shown = preview(
        &doc,
        &on_face(&t, "p30", &face, [15.0, -14.0, 800.0], json!({})),
    );
    assert_eq!(shown["side"]["across"], "slot", "{shown}");
    let found = gaps(&shown);
    assert!(
        found.len() == 2 && near(found[0], 189.0) && near(found[1], 704.0),
        "{found:?}"
    );
    // A profile on slot 2 counts for the side's middle.
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    place(
        &mut doc,
        on_face(&t, "p30", &face, [15.0, 14.0, ALONG], json!({})),
    );
    let shown = preview(
        &doc,
        &on_face(&t, "p30", &face, [15.0, 1.0, 800.0], json!({})),
    );
    assert_eq!(shown["side"]["across"], "middle", "{shown}");
    let found = gaps(&shown);
    assert!(
        found.len() == 2 && near(found[0], 189.0) && near(found[1], 704.0),
        "{found:?}"
    );
}

#[test]
fn a_centred_profile_tilts_and_moves_on_the_side_s_middle() {
    let mut doc = design();
    // The tilted foot's end cut clips the part's box as a half-space (as
    // in the tilt tests); otherwise the mock adds the cutting box's bounds.
    doc.kernel().half_space_cuts.set(true);
    let t = host(&mut doc, "p30x60");
    let face = side_face(&doc, &t, "x+", 15.0);
    let p = place(
        &mut doc,
        on_face(&t, "p30x60", &face, [15.0, 1.0, ALONG], json!({})),
    );
    all_ok(&doc);
    // An offset edited: still centred across.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "at": {"offset": 200, "from": "a"}}),
    )
    .unwrap();
    all_ok(&doc);
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [15.0, 0.0, 215.0]), "{at:?}");
    assert_eq!(
        mitcad_model_build::side_offset(&doc, component(&p)),
        Some(200.0)
    );
    // Tilted along the host: on the middle still, its foot across it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "tilt": "20 deg"}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x+");
    assert_across(&world_box(&doc, &p), -30.0, 30.0);
}

#[test]
fn a_part_that_goes_in_a_slot_is_not_centred_between_the_slots() {
    let mut doc = design();
    let t = host(&mut doc, "p30x60");
    let tp = derived(&doc, component(&t));
    // A slider clicked near the centre line goes on the nearer slot.
    let face = side_face(&doc, &t, "x+", 15.0);
    let s = place(
        &mut doc,
        json!({"part": part("slider8"),
               "at": {"occurrence": t["occurrence"], "geometry": face, "point": [15.0, -1.0, ALONG]}}),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &s), "slot_x+_1");
    // Named on the side's middle, it is refused with what to do.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("slider8"),
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"mount": format!("{tp}:mount(side_x+)")}}}),
    )
    .unwrap_err();
    assert!(refused.contains("goes in a slot"), "{refused}");
    assert!(!refused.contains("side_x+"), "{refused}");
    // A profile named on it stands there (an agent's placing).
    let p = place(
        &mut doc,
        json!({"part": part("p30"), "length": 200,
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"mount": format!("{tp}:mount(side_x-)")}, "offset": 300}}),
    );
    all_ok(&doc);
    assert_eq!(stands_on(&doc, &p), "side_x-");
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [-15.0, 0.0, 315.0]), "{at:?}");
}
