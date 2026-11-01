// SPDX-License-Identifier: MIT
//! Side and end placements bound to what the click meant (mitcad#417),
//! with the mock kernel: a side placement keeps its reference (end A, end
//! B or the middle) when the target's length changes, and a new reference
//! keeps the part in place; a click on an end face continues straight in
//! its inner half and turns towards the edge of the band it is in; a
//! corner lies in the target's plane, on its end face flush with its outer
//! side (a square corner's default), or mitred with the outer corners
//! meeting (the default at another angle); `at.snap` widens the snaps.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::kernel::BoundingBox;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

/// A 30 profile 1000 long standing at the origin, and its slot `slot_y+_1`
/// as `at.geometry`.
fn target(doc: &mut Document<MockKernel>) -> (Value, Value) {
    let t = first(doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(doc, component(&t));
    (t, json!({"mount": format!("{tp}:mount(slot_y+_1)")}))
}

/// A 30 profile 200 long on the target's y+ side with `at` merged in.
fn on_side(doc: &mut Document<MockKernel>, t: &Value, slot: &Value, at: Value) -> Value {
    place(
        doc,
        json!({"part": {"source": "S1"}, "at": side_at(t, slot, at), "length": 200}),
    )
}

fn side_at(t: &Value, slot: &Value, at: Value) -> Value {
    let mut fields = json!({"occurrence": t["occurrence"], "geometry": slot});
    for (key, value) in at.as_object().unwrap() {
        fields[key] = value.clone();
    }
    fields
}

/// What `build_preview` says of a part 200 long on the side.
fn side_preview(doc: &Document<MockKernel>, t: &Value, slot: &Value, at: Value) -> Value {
    query(
        doc,
        json!({"query": "build_preview", "part": {"source": "S1"}, "length": 200,
               "at": side_at(t, slot, at)}),
    )["side"]
        .clone()
}

/// Where a part on the y+ side stands along the target (its centre's z).
fn along(doc: &Document<MockKernel>, answer: &Value) -> f64 {
    placed(doc, occurrence(answer)).translation[2]
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// Sets the target's length.
fn lengthen(doc: &mut Document<MockKernel>, t: &Value, length: f64) {
    let id = doc.part_info(component(t)).unwrap().length.unwrap().0;
    let name = doc.parameters().name(id);
    command(
        doc,
        json!({"cmd": "set_parameter", "name": name, "value": length}),
    )
    .unwrap();
    all_ok(doc);
}

/// A placed part's box in the design.
fn body_box(doc: &Document<MockKernel>, answer: &Value) -> BoundingBox {
    let local = doc.part_bounds(component(answer)).unwrap();
    mitcad_model::internal::build::moved_box(&local, &placed(doc, occurrence(answer)))
}

/// The record's reference of a placed part.
fn from_of(doc: &Document<MockKernel>, answer: &Value) -> Option<&'static str> {
    doc.part_info(component(answer))
        .and_then(|i| i.record.from)
        .map(|f| f.as_str())
}

#[test]
fn side_placements_keep_end_a_end_b_and_the_middle_when_the_target_grows() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // A click flush with end B: from B, offset 0.
    let shown = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 995.0]}));
    assert_eq!(shown["kind"], "end_b", "{shown}");
    assert_eq!(shown["from"], "b");
    assert!(near(shown["offset"].as_f64().unwrap(), 0.0), "{shown}");
    let flush_b = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 995.0]}));
    assert_eq!(from_of(&doc, &flush_b), Some("b"));
    // Typed 30 from B: its far face 30 from end B.
    let b30 = on_side(&mut doc, &t, &slot, json!({"offset": 30, "from": "b"}));
    assert!(near(along(&doc, &b30), 955.0), "{}", along(&doc, &b30));
    // A click on the middle: from the middle, offset 0; typed 30 from it.
    let shown = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 503.0]}));
    assert_eq!(shown["kind"], "middle", "{shown}");
    assert_eq!(shown["from"], "middle");
    assert!(near(shown["offset"].as_f64().unwrap(), 0.0));
    let middle = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 503.0]}));
    assert!(near(along(&doc, &middle), 500.0));
    assert_eq!(from_of(&doc, &middle), Some("middle"));
    let m30 = on_side(&mut doc, &t, &slot, json!({"offset": 30, "from": "middle"}));
    assert!(near(along(&doc, &m30), 530.0), "{}", along(&doc, &m30));
    // Typed 30 from A.
    let a30 = on_side(&mut doc, &t, &slot, json!({"offset": 30}));
    assert!(near(along(&doc, &a30), 45.0));
    all_ok(&doc);
    // The joint's slide is an expression of the target's length for B and
    // the middle.
    let length = doc.part_info(component(&t)).unwrap().length.unwrap().0;
    let name = doc.parameters().name(length);
    let tx = |answer: &Value| {
        let joint = answer["joint"].as_str().unwrap().parse().unwrap();
        let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
            panic!("a joint");
        };
        let id = j.position[&mitcad_model::joints::Motion::Tx];
        doc.parameters().get(id).unwrap().expression().to_owned()
    };
    assert_eq!(tx(&flush_b), format!("{name} - 15"));
    assert_eq!(tx(&b30), format!("{name} - 45"));
    assert_eq!(tx(&middle), format!("{name} / 2"));
    assert_eq!(tx(&m30), format!("{name} / 2 + 30"));
    // The target grows: each keeps its place from its reference.
    lengthen(&mut doc, &t, 1200.0);
    for (answer, expected) in [
        (&flush_b, 1185.0),
        (&b30, 1155.0),
        (&middle, 600.0),
        (&m30, 630.0),
        (&a30, 45.0),
    ] {
        assert!(
            near(along(&doc, answer), expected),
            "{} instead of {expected}",
            along(&doc, answer)
        );
    }
}

#[test]
fn a_click_between_snaps_is_from_the_nearer_end() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // Near face 407 from end A.
    let shown = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 422.0]}));
    assert!(shown["kind"].is_null(), "{shown}");
    assert_eq!(shown["from"], "a");
    assert!(near(shown["offset"].as_f64().unwrap(), 407.0), "{shown}");
    // Far face 822 from A: 178 from end B.
    let shown = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 807.0]}));
    assert_eq!(shown["from"], "b", "{shown}");
    assert!(near(shown["offset"].as_f64().unwrap(), 178.0), "{shown}");
    let p = on_side(&mut doc, &t, &slot, json!({"point": [0.0, 15.0, 807.0]}));
    assert!(near(along(&doc, &p), 807.0));
    lengthen(&mut doc, &t, 1100.0);
    assert!(near(along(&doc, &p), 907.0), "{}", along(&doc, &p));
    // A reference given with a click: the click's place from it.
    let shown = side_preview(
        &doc,
        &t,
        &slot,
        json!({"point": [0.0, 15.0, 807.0], "from": "a"}),
    );
    assert_eq!(shown["from"], "a");
    assert!(near(shown["offset"].as_f64().unwrap(), 792.0), "{shown}");
}

#[test]
fn a_new_reference_keeps_the_part_in_place() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    let p = on_side(&mut doc, &t, &slot, json!({"offset": 100}));
    assert!(near(along(&doc, &p), 115.0));
    let c = p["component"].clone();
    let answer = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"from": "b"}}),
    )
    .unwrap();
    let changed: Vec<&str> = answer["changed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        changed.contains(&"offset") && changed.contains(&"from"),
        "{answer}"
    );
    assert!(near(along(&doc, &p), 115.0), "{}", along(&doc, &p));
    assert_eq!(from_of(&doc, &p), Some("b"));
    let parts = query(&doc, json!({"query": "build_parts"}));
    let entry = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == c)
        .unwrap();
    assert_eq!(entry["from"], "b");
    // From the middle: still there, now 385 before it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"from": "middle"}}),
    )
    .unwrap();
    assert!(near(along(&doc, &p), 115.0));
    all_ok(&doc);
    lengthen(&mut doc, &t, 1200.0);
    assert!(near(along(&doc, &p), 215.0), "{}", along(&doc, &p));
    // An offset alone is from the reference the part has.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"offset": 0}}),
    )
    .unwrap();
    assert!(near(along(&doc, &p), 600.0), "{}", along(&doc, &p));
}

#[test]
fn at_snap_widens_the_side_snaps() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // Near face 98 from A (8 from a grid point): past the section width's snap.
    let free = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 113.0]}));
    assert!(free["kind"].is_null(), "{free}");
    let wide = side_preview(
        &doc,
        &t,
        &slot,
        json!({"point": [0.0, 15.0, 113.0], "snap": 120}),
    );
    assert_eq!(wide["kind"], "end_a", "{wide}");
    // 63 from the middle: past a quarter width, within a snap of 70.
    let free = side_preview(&doc, &t, &slot, json!({"point": [0.0, 15.0, 563.0]}));
    assert!(free["kind"].is_null(), "{free}");
    let wide = side_preview(
        &doc,
        &t,
        &slot,
        json!({"point": [0.0, 15.0, 563.0], "snap": 70}),
    );
    assert_eq!(wide["kind"], "middle", "{wide}");
    // A placement with the app's snap snaps the same way.
    let p = on_side(
        &mut doc,
        &t,
        &slot,
        json!({"point": [0.0, 15.0, 563.0], "snap": 70}),
    );
    assert!(near(along(&doc, &p), 500.0));
}

/// The end face `end_b` of the target as `at.geometry`, clicked at `x`, `y`.
fn end_at(doc: &Document<MockKernel>, t: &Value, x: f64, y: f64) -> Value {
    let tp = derived(doc, component(t));
    json!({"occurrence": t["occurrence"],
           "geometry": {"body": format!("{tp}.b0"), "face": format!("{tp}:end_b")},
           "point": [x, y, 1000.0]})
}

fn end_preview(doc: &Document<MockKernel>, fields: Value) -> Value {
    let mut request = fields;
    request["query"] = json!("build_preview");
    request["part"] = json!({"source": "S1"});
    request["length"] = json!(400);
    query(doc, request)
}

fn vector(value: &Value) -> [f64; 3] {
    let v: Vec<f64> = value
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_f64().unwrap())
        .collect();
    [v[0], v[1], v[2]]
}

#[test]
fn a_click_on_an_end_face_continues_straight_or_turns_towards_an_edge() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    for ((x, y), zone, direction) in [
        ((2.0, -3.0), "straight", [0.0, 0.0, 1.0]),
        ((12.0, 1.0), "edge", [1.0, 0.0, 0.0]),
        ((-12.0, 1.0), "edge", [-1.0, 0.0, 0.0]),
        ((1.0, 12.0), "edge", [0.0, 1.0, 0.0]),
        ((1.0, -12.0), "edge", [0.0, -1.0, 0.0]),
        // In a corner of the face: towards the nearer edge.
        ((13.0, 9.0), "edge", [1.0, 0.0, 0.0]),
        ((9.0, -14.0), "edge", [0.0, -1.0, 0.0]),
    ] {
        let shown = end_preview(&doc, json!({"at": end_at(&doc, &t, x, y)}));
        assert_eq!(shown["place"], "end", "{shown}");
        assert_eq!(shown["end"]["zone"], zone, "({x}, {y}): {shown}");
        assert!(
            close(vector(&shown["end"]["direction"]), direction),
            "({x}, {y}): {}",
            shown["end"]
        );
        // The four turns square to the target, +x, +y, -x, -y of its end.
        let turns: Vec<[f64; 3]> = shown["end"]["turns"]
            .as_array()
            .unwrap()
            .iter()
            .map(vector)
            .collect();
        assert_eq!(turns.len(), 4);
        assert!(close(turns[0], [1.0, 0.0, 0.0]) && close(turns[1], [0.0, 1.0, 0.0]));
        assert!(close(turns[2], [-1.0, 0.0, 0.0]) && close(turns[3], [0.0, -1.0, 0.0]));
    }
    // Placed: the new part's axis is the turn.
    for ((x, y), axis) in [
        ((0.0, 0.0), [0.0, 0.0, 1.0]),
        ((12.0, 0.0), [1.0, 0.0, 0.0]),
        ((0.0, -12.0), [0.0, -1.0, 0.0]),
    ] {
        let fields = json!({"part": {"source": "S1"}, "at": end_at(&doc, &t, x, y),
                            "length": 400, "mitre": {"cut": "none"}});
        let p = place(&mut doc, fields);
        let at = placed(&doc, occurrence(&p));
        assert!(close(at.apply_vector([0.0, 0.0, 1.0]), axis), "{at:?}");
    }
    all_ok(&doc);
    // The app's snap widens the inner zone: 12 from the middle is inner
    // with a snap of 40 (as far as the face allows).
    let shown = end_preview(
        &doc,
        json!({"at": {"snap": 40, "occurrence": t["occurrence"],
                      "geometry": end_at(&doc, &t, 12.0, 0.0)["geometry"],
                      "point": [12.0, 0.0, 1000.0]}}),
    );
    assert_eq!(shown["end"]["zone"], "straight", "{shown}");
    // A direction given decides; straight has no corner offset.
    let shown = end_preview(
        &doc,
        json!({"at": {"occurrence": t["occurrence"],
                      "geometry": end_at(&doc, &t, 0.0, 0.0)["geometry"],
                      "point": [0.0, 0.0, 1000.0], "direction": [0, -1, 0]}}),
    );
    assert_eq!(shown["end"]["zone"], "given");
    assert!(close(vector(&shown["end"]["direction"]), [0.0, -1.0, 0.0]));
    let straight = end_preview(&doc, json!({"at": end_at(&doc, &t, 0.0, 0.0)}));
    assert!(straight["end"]["offset"].is_null(), "{straight}");
    assert_eq!(straight["end"]["mitre"]["cut"], "none");
}

/// A corner at the target's end B turned to +Y (a click near its +y edge),
/// with `extra` merged into the command.
fn corner(doc: &mut Document<MockKernel>, t: &Value, extra: Value) -> Value {
    let mut fields =
        json!({"part": {"source": "S1"}, "at": end_at(doc, t, 0.0, 12.0), "length": 400});
    for (key, value) in extra.as_object().unwrap() {
        if key == "offset" {
            fields["at"]["offset"] = value.clone();
        } else {
            fields[key.clone()] = value.clone();
        }
    }
    place(doc, fields)
}

#[test]
fn a_corner_without_a_mitre_lies_on_the_end_face_flush_with_the_outer_side() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    // A square corner's default: no mitre, as the preview says.
    let shown = end_preview(&doc, json!({"at": end_at(&doc, &t, 0.0, 12.0)}));
    assert_eq!(shown["end"]["zone"], "edge");
    assert_eq!(shown["end"]["mitre"]["cut"], "none", "{shown}");
    let p = corner(&mut doc, &t, json!({}));
    all_ok(&doc);
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.apply_vector([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]));
    let (n, b) = (body_box(&doc, &p), body_box(&doc, &t));
    // On the end face, centred across the turn, its end flush with the
    // outer side (y = -15).
    assert!(
        near(n.min[2], b.max[2]) && near(n.max[2], 1030.0),
        "{n:?} {b:?}"
    );
    assert!(
        near(n.min[1], b.min[1]) && near(n.max[1], 385.0),
        "{n:?} {b:?}"
    );
    assert!(
        near(n.min[0], b.min[0]) && near(n.max[0], b.max[0]),
        "{n:?} {b:?}"
    );
    // Square ends.
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(&doc, component(&p))).unwrap().def else {
        panic!("a derived part");
    };
    assert!(d.ends.is_square());
    // The corner follows the target's end when it grows.
    lengthen(&mut doc, &t, 1200.0);
    let n = body_box(&doc, &p);
    assert!(near(n.min[2], 1200.0) && near(n.min[1], -15.0), "{n:?}");
}

#[test]
fn a_corner_s_shared_mitre_meets_at_the_outer_corners() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    // A shared mitre asked for on a square corner, at offset 0.
    let shown = end_preview(
        &doc,
        json!({"at": end_at(&doc, &t, 0.0, 12.0), "mitre": {"cut": "shared"}}),
    );
    assert_eq!(shown["end"]["zone"], "edge");
    assert_eq!(shown["end"]["offset"], 0.0);
    assert_eq!(shown["end"]["mitre"]["cut"], "shared", "{shown}");
    let quarter = std::f64::consts::FRAC_PI_4;
    assert!(near(
        shown["end"]["mitre"]["angle"].as_f64().unwrap(),
        quarter
    ));
    let p = corner(&mut doc, &t, json!({"mitre": {"cut": "shared"}}));
    all_ok(&doc);
    // Both ends cut by 45 degrees.
    for (part, b) in [
        (derived(&doc, component(&p)), false),
        (derived(&doc, component(&t)), true),
    ] {
        let FeatureDef::DerivedPart(d) = &doc.feature(part).unwrap().def else {
            panic!("a derived part");
        };
        let cut = if b {
            d.ends.b.clone()
        } else {
            d.ends.a.clone()
        }
        .unwrap();
        assert!(near(doc.parameters().value(cut.angle).unwrap(), quarter));
    }
    // The new part's end at the target's long point (0, -15, 1000), its
    // outer side flush with the target's end, centred across (mitcad#443).
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [0.0, -15.0, 985.0]), "{at:?}");
    // The outer corners meet: each part's outer edge (15 from its axis, on
    // the side away from the corner's inside) meets its own cut plane at
    // (0, -15, 1000). (The cuts are taken as `derived_part` makes them:
    // through the section's edge on the long side at the end, its normal
    // leaning by the angle towards its tilt.)
    let section = BoundingBox {
        min: [-15.0, -15.0, 0.0],
        max: [15.0, 15.0, 1000.0],
    };
    let meets =
        |placement: &mitcad_model::transform::Transform,
         b: bool,
         cut: &mitcad_model::features::derived_part::EndCut<mitcad_model::parameters::ParamId>,
         edge: [f64; 3],
         along: [f64; 3]| {
            let angle = doc.parameters().value(cut.angle).unwrap();
            let length = if b { 1000.0 } else { 400.0 };
            let (point, normal) = mitcad_model::internal::features::end_plane(
                Some(&section),
                b,
                length,
                angle,
                cut.tilt.unwrap_or_default(),
            )
            .unwrap();
            let normal = placement.apply_vector(normal);
            let centre = placement.apply_point(point);
            use mitcad_vec3::dot;
            let delta: [f64; 3] = std::array::from_fn(|i| centre[i] - edge[i]);
            let k = dot(delta, normal) / dot(along, normal);
            std::array::from_fn::<f64, 3, _>(|i| edge[i] + along[i] * k)
        };
    let cut_of = |part: mitcad_model::ids::FeatureUid, b: bool| {
        let FeatureDef::DerivedPart(d) = &doc.feature(part).unwrap().def else {
            panic!("a derived part");
        };
        if b {
            d.ends.b.clone()
        } else {
            d.ends.a.clone()
        }
        .unwrap()
    };
    let target_corner = meets(
        &placed(&doc, occurrence(&t)),
        true,
        &cut_of(derived(&doc, component(&t)), true),
        [0.0, -15.0, 0.0],
        [0.0, 0.0, 1.0],
    );
    let new_corner = meets(
        &at,
        false,
        &cut_of(derived(&doc, component(&p)), false),
        [0.0, 0.0, 1000.0],
        [0.0, 1.0, 0.0],
    );
    assert!(
        close(target_corner, [0.0, -15.0, 1000.0]),
        "{target_corner:?}"
    );
    assert!(close(new_corner, [0.0, -15.0, 1000.0]), "{new_corner:?}");
}

#[test]
fn a_corner_of_two_sections_aligns_their_outer_faces() {
    use mitcad_model_build::internal::corner_shift;
    // A 40 x 40 part turned to +y at the end of a 30 x 30 one 1000 long
    // (its end frame at z = 1000, z out): the new part's end frame and its
    // box with the frame there, turned (its axis +y, its x the target's).
    let target = BoundingBox {
        min: [-15.0, -15.0, 0.0],
        max: [15.0, 15.0, 1000.0],
    };
    let new = BoundingBox {
        min: [-20.0, -20.0, 0.0],
        max: [20.0, 20.0, 400.0],
    };
    let e = [0.0, 0.0, 1000.0];
    let (u, d) = ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]);
    // Its own z along +y from e, its y along -z.
    let to_target = mitcad_model::internal::joints::frame(e, [1.0, 0.0, 0.0], d);
    let moved = |shift: [f64; 3]| {
        let placement = mitcad_model::transform::Transform::translation(shift).after(&to_target);
        mitcad_model::internal::build::moved_box(&new, &placement)
    };
    // On the end face, its end flush with the outer side y = -15.
    let n = moved(corner_shift(e, u, d, &target, &new, &to_target, 0.0, false));
    assert!(near(n.min[2], 1000.0) && near(n.min[1], -15.0), "{n:?}");
    assert!(near(n.min[0], -20.0) && near(n.max[0], 20.0), "{n:?}");
    // 10 towards the turn.
    let n = moved(corner_shift(
        e, u, d, &target, &new, &to_target, 10.0, false,
    ));
    assert!(near(n.min[1], -5.0), "{n:?}");
    // Mitred (mitcad#443): the new part's end at the target's long point
    // (y = -15 at z = 1000), its outer side (z = 1000) flush with the
    // target's end, its axis 20 below it.
    let shift = corner_shift(e, u, d, &target, &new, &to_target, 0.0, true);
    assert!(close(shift, [0.0, -15.0, -20.0]), "{shift:?}");
    let n = moved(shift);
    assert!(near(n.max[2], 1000.0) && near(n.min[1], -15.0), "{n:?}");
}

#[test]
fn a_corner_s_offset_moves_its_end_and_takes_no_mitre() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let fields = end_at(&doc, &t, 0.0, 12.0);
    let mut at = fields.clone();
    at["offset"] = json!(10);
    let shown = end_preview(&doc, json!({"at": at}));
    assert_eq!(shown["end"]["offset"], 10.0, "{shown}");
    assert_eq!(shown["end"]["mitre"]["cut"], "none", "{shown}");
    let p = corner(&mut doc, &t, json!({"offset": 10}));
    all_ok(&doc);
    let n = body_box(&doc, &p);
    // Its end 10 from the outer side, towards the turn, on the end face.
    assert!(near(n.min[1], -5.0) && near(n.min[2], 1000.0), "{n:?}");
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(&doc, component(&p))).unwrap().def else {
        panic!("a derived part");
    };
    assert!(d.ends.is_square());
    // Past the outer side.
    let q = corner(&mut doc, &t, json!({"offset": -20}));
    assert!(near(body_box(&doc, &q).min[1], -35.0));
    // A shared mitre needs offset 0.
    let mut refused = json!({"cmd": "build_place", "part": {"source": "S1"}, "length": 400,
                             "mitre": {"cut": "shared"}});
    refused["at"] = fields;
    refused["at"]["offset"] = json!(10);
    let error = command(&mut doc, refused).unwrap_err();
    assert!(error.contains("a shared miter needs offset 0"), "{error}");
}

#[test]
fn a_square_corner_lies_on_the_end_another_angle_is_mitred() {
    // Owner 18.10.2026: a square corner's default is no mitre (on the end
    // face); a corner at another angle takes the shared mitre at offset 0.
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let mut at = end_at(&doc, &t, 0.0, 0.0);
    at["direction"] = json!([0.0, half, half]);
    let shown = end_preview(&doc, json!({"at": at.clone()}));
    assert_eq!(shown["end"]["zone"], "given", "{shown}");
    assert_eq!(shown["end"]["mitre"]["cut"], "shared", "{shown}");
    at["direction"] = json!([0.0, 1.0, 0.0]);
    let shown = end_preview(&doc, json!({"at": at}));
    assert_eq!(shown["end"]["mitre"]["cut"], "none", "{shown}");
}
