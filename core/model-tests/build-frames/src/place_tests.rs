// SPDX-License-Identifier: MIT
//! Placing BUILD parts with the mock kernel: the first part grounded and
//! on a plane at a click far from its origin (mitcad#1196), a
//! profile on a side's slot and on an end for each system, the side or end
//! a pick decides, offsets from either end, rotation and flip, mitres
//! (new, existing, shared), the derived part's names and frames, and the
//! refusals.

use serde_json::json;

use mitcad_vec3::dot;

use mitcad_model::features::FeatureDef;
use mitcad_model::transform::Transform;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

fn axis(t: &Transform) -> [f64; 3] {
    t.apply_vector([0.0, 0.0, 1.0])
}

#[test]
fn the_first_part_is_grounded_where_it_is_placed() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [10.0, 20.0, 0.0], 1000.0);
    all_ok(&doc);
    assert_eq!(t["place"], "grounded");
    assert_eq!(t["name"], "Profile 30x30 B8 L=1000");
    let o = occurrence(&t);
    assert!(doc.assembly().occurrence(o).unwrap().grounded);
    assert!(close(placed(&doc, o).translation, [10.0, 20.0, 0.0]));
    // Its source is kept once; its derived part names its faces after the
    // source's and its ends end_a and end_b.
    let source = &doc.state().library_sources;
    assert_eq!(source.0.len(), 1);
    assert_eq!(source.0[0].supplier, "Test supplier");
    let part = derived(&doc, component(&t));
    let body = mitcad_model::ids::BodyUid::new(part, 0);
    let shape = doc.body_shape(body).unwrap();
    let names: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert!(names.contains(&format!("{part}:end_a")), "{names:?}");
    assert!(names.contains(&format!("{part}:end_b")), "{names:?}");
    assert!(
        names
            .iter()
            .any(|n| n.starts_with(&format!("{part}:part(")) && n.contains(":side(")),
        "{names:?}"
    );
    // Its frames are the source's mount frames.
    let frames = doc.part_info(component(&t)).unwrap().mounts;
    assert!(close(frames["end_b"].translation, [0.0, 0.0, 1000.0]));
    assert!(close(frames["slot_x+_1"].translation, [15.0, 0.0, 0.0]));
    // The design records what BUILD made.
    let record = doc
        .assembly()
        .component(component(&t))
        .unwrap()
        .build
        .clone()
        .unwrap();
    assert_eq!(record.role, mitcad_model_build::BuildRole::Profile);
    assert_eq!(record.part, Some(part));
    assert_eq!(doc.undo_label(), Some("Place Profile 30x30 B8 L=1000"));
}

#[test]
fn the_first_part_starts_where_the_click_puts_it_on_a_plane() {
    // mitcad#1196: the app's first part on a chosen plane, the click far
    // from the plane's origin: its end A there, standing on the plane and
    // jointed to it (not grounded), where the preview shows it.
    let mut doc = design();
    let fields = json!({"part": part("p30"),
        "at": {"geometry": "xy", "point": [1500.0, -800.0, 0.0], "direction": [0.0, 0.0, 1.0]},
        "length": 500});
    let mut probe = fields.clone();
    probe["query"] = json!("build_preview");
    let shown = query(&doc, probe);
    let t = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(t["place"], "geometry");
    let o = occurrence(&t);
    assert!(!doc.assembly().occurrence(o).unwrap().grounded);
    let at = placed(&doc, o);
    assert!(close(at.translation, [1500.0, -800.0, 0.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, 1.0]));
    let origin = &shown["frame"]["origin"];
    let preview = [0, 1, 2].map(|i| origin[i].as_f64().unwrap());
    assert!(close(preview, [1500.0, -800.0, 0.0]), "{shown}");
    // On a construction plane 200 above XY, far from its origin; and with
    // no point (the app's Enter before the click) at the plane's origin.
    let added = command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 200}}}),
    )
    .unwrap_or_else(|e| panic!("construction plane: {e}"));
    let plane = added["uid"].as_str().unwrap().to_owned();
    let high = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"geometry": plane.as_str(), "point": [-900.0, 1200.0, 200.0]}, "length": 300}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&high));
    assert!(close(at.translation, [-900.0, 1200.0, 200.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, 1.0]));
    let enter = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": plane.as_str()}, "length": 300}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&enter));
    assert!(close(at.translation, [0.0, 0.0, 200.0]), "{at:?}");
}

#[test]
fn a_grounded_first_part_moves_and_stays_grounded() {
    // mitcad#1196: Move/Copy of a grounded first part (placed without a
    // plane) moves it and it stays grounded there, one undo step.
    let mut doc = design();
    let t = first(&mut doc, "p30", [10.0, 20.0, 0.0], 500.0);
    let o = occurrence(&t);
    assert!(doc.assembly().occurrence(o).unwrap().grounded);
    command(
        &mut doc,
        json!({"cmd": "add_feature", "def": {"type": "move_occurrence",
               "occurrences": [t["occurrence"]],
               "transform": {"type": "translate_xyz", "x": 100, "y": 0, "z": 0}}}),
    )
    .unwrap_or_else(|e| panic!("move_occurrence: {e}"));
    all_ok(&doc);
    assert!(close(placed(&doc, o).translation, [110.0, 20.0, 0.0]));
    assert!(doc.assembly().occurrence(o).unwrap().grounded);
    doc.undo();
    assert!(close(placed(&doc, o).translation, [10.0, 20.0, 0.0]));
    assert!(doc.assembly().occurrence(o).unwrap().grounded);
}

#[test]
fn a_profile_on_a_side_stands_on_the_slot_for_each_system() {
    for (component_id, w) in [("p20", 20.0), ("p30", 30.0), ("p40", 40.0)] {
        let mut doc = design();
        let t = first(&mut doc, component_id, [0.0; 3], 1000.0);
        let tp = derived(&doc, component(&t));
        let p = place(
            &mut doc,
            json!({"part": {"source": "S1"},
                   "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                          "offset": 100},
                   "length": 500}),
        );
        all_ok(&doc);
        assert_eq!(p["place"], "side");
        let at = placed(&doc, occurrence(&p));
        // End A on the slot's centre line, its near face 100 from the
        // target's end A (its centre half a section further), the profile
        // out along +X.
        assert!(
            close(at.translation, [w / 2.0, 0.0, 100.0 + w / 2.0]),
            "{component_id}: {at:?}"
        );
        assert!(close(axis(&at), [1.0, 0.0, 0.0]), "{component_id}: {at:?}");
        // A slider along the slot.
        let joint = p["joint"].as_str().unwrap().parse().unwrap();
        let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
            panic!("a joint");
        };
        assert_eq!(j.kind, mitcad_model::joints::JointKind::Slider);
        assert!(j.flip);
    }
}

#[test]
fn a_rail_lies_along_a_slot_on_its_base_frame() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let r = place(
        &mut doc,
        json!({"part": part("rail15"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 100},
               "length": 400}),
    );
    all_ok(&doc);
    assert_eq!(r["place"], "side");
    // A slider on the slot by its face frame, not its end.
    let rp = derived(&doc, component(&r));
    let joint = r["joint"].as_str().unwrap().parse().unwrap();
    let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
        panic!("a joint");
    };
    assert_eq!(j.kind, mitcad_model::joints::JointKind::Slider);
    assert_eq!(
        j.a.geometry,
        mitcad_model::features::GeomRef::Mount {
            feature: rp,
            name: "base".to_owned()
        }
    );
    // Flat on the slot's face (x = 15), its length along the profile from
    // 100 to 500.
    let local = doc.part_bounds(component(&r)).unwrap();
    let b = mitcad_model::internal::build::moved_box(&local, &placed(&doc, occurrence(&r)));
    assert!(close(b.min, [15.0, -7.5, 100.0]), "{b:?}");
    assert!(close(b.max, [30.0, 7.5, 500.0]), "{b:?}");
}

#[test]
fn a_pick_on_a_face_decides_side_or_end() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    // An end face: on the end, inline.
    let e = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"body": format!("{tp}.b0"), "face": format!("{tp}:end_b")}},
               "length": 300}),
    );
    all_ok(&doc);
    assert_eq!(e["place"], "end");
    let at = placed(&doc, occurrence(&e));
    assert!(close(at.translation, [0.0, 0.0, 1000.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, 1.0]));
    // A side face near a point: the nearest slot, centred on the point's
    // distance along the profile (between the snaps: its near face 407
    // from end A, not near a grid point of 30).
    let s = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"body": format!("{tp}.b0"), "face": format!("{tp}:part(F2:side(c1))")},
                      "point": [0.0, 15.0, 422.0]},
               "length": 200}),
    );
    all_ok(&doc);
    assert_eq!(s["place"], "side");
    let at = placed(&doc, occurrence(&s));
    assert!(close(at.translation, [0.0, 15.0, 422.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 1.0, 0.0]));
    // `place: end` on a side pick takes the nearest end.
    let n = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"body": format!("{tp}.b0"), "face": format!("{tp}:part(F2:side(c1))")},
                      "point": [0.0, 15.0, 20.0], "place": "end"},
               "length": 200}),
    );
    assert_eq!(n["place"], "end");
    let at = placed(&doc, occurrence(&n));
    assert!(close(at.translation, [0.0, 0.0, 0.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]));
}

#[test]
fn offsets_from_end_b_follow_the_target_s_length() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_y+_1)")},
                      "offset": 50, "from": "b"},
               "length": 200, "rotate": "90 deg"}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&p));
    assert!(close(at.translation, [0.0, 15.0, 935.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 1.0, 0.0]));
    // Rotated a quarter turn about its axis.
    let x = at.apply_vector([1.0, 0.0, 0.0]);
    assert!((x[0].abs() - 1.0).abs() < 1e-6, "{x:?}");
    // The target gets longer: the part keeps 50 from end B.
    let t_length = doc.part_info(component(&t)).unwrap().length.unwrap().0;
    let name = doc.parameters().name(t_length);
    command(
        &mut doc,
        json!({"cmd": "set_parameter", "name": name, "value": 1200}),
    )
    .unwrap();
    all_ok(&doc);
    assert!(close(
        placed(&doc, occurrence(&p)).translation,
        [0.0, 15.0, 1135.0]
    ));
    // An offset alone keeps its reference (mitcad#417): 80 from end B.
    let c = p["component"].clone();
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"offset": 80}}),
    )
    .unwrap();
    assert!(close(
        placed(&doc, occurrence(&p)).translation,
        [0.0, 15.0, 1105.0]
    ));
    // Edited to 80 from end A, then flipped: end B on the slot.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "at": {"offset": 80, "from": "a"}}),
    )
    .unwrap();
    assert!(close(
        placed(&doc, occurrence(&p)).translation,
        [0.0, 15.0, 95.0]
    ));
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": c, "flip": true}),
    )
    .unwrap();
    all_ok(&doc);
    let at = placed(&doc, occurrence(&p));
    // End B on the slot: end A 200 out along +Y.
    assert!(
        close(at.apply_point([0.0, 0.0, 200.0]), [0.0, 15.0, 95.0]),
        "{at:?}"
    );
    assert!(close(at.translation, [0.0, 215.0, 95.0]), "{at:?}");
}

#[test]
fn a_bent_end_and_the_shared_mitre() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [0, 1, 0]},
               "length": 400, "mitre": {"cut": "shared"}}),
    );
    all_ok(&doc);
    assert_eq!(p["place"], "end");
    let at = placed(&doc, occurrence(&p));
    // The outer corners meet at the target's long point (0, -15, 1000): the
    // new part's end there, its outer side (+z) flush with the target's end
    // (mitcad#443).
    assert!(close(at.translation, [0.0, -15.0, 985.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 1.0, 0.0]), "{at:?}");
    let quarter = std::f64::consts::FRAC_PI_4;
    let end = |part: mitcad_model::ids::FeatureUid, b: bool| {
        let FeatureDef::DerivedPart(d) = &doc.feature(part).unwrap().def else {
            panic!("a derived part");
        };
        let cut = if b {
            d.ends.b.clone()
        } else {
            d.ends.a.clone()
        };
        cut.map(|c| (doc.parameters().value(c.angle).unwrap(), c.tilt))
    };
    let pp = derived(&doc, component(&p));
    let (angle, tilt) = end(pp, false).unwrap();
    assert!((angle - quarter).abs() < 1e-9);
    assert_eq!(tilt, Some(mitcad_model::features::derived_part::Tilt::X));
    let (angle, tilt) = end(tp, true).unwrap();
    assert!((angle - quarter).abs() < 1e-9);
    assert_eq!(tilt, Some(mitcad_model::features::derived_part::Tilt::X));
    assert!(end(tp, false).is_none());
    // Both ends are cut, their faces keep the end names.
    for part in [pp, tp] {
        let shape = doc
            .body_shape(mitcad_model::ids::BodyUid::new(part, 0))
            .unwrap();
        assert!(shape.history.contains("cut("), "{}", shape.history);
    }
    let shape = doc
        .body_shape(mitcad_model::ids::BodyUid::new(tp, 0))
        .unwrap();
    assert!(
        shape
            .faces
            .iter()
            .any(|f| f.to_string() == format!("{tp}:end_b"))
    );
}

#[test]
fn mitres_new_and_existing_on_an_end_and_new_on_a_side() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let existing = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [1, 0, 0]},
               "length": 300, "mitre": {"cut": "existing", "angle": "30 deg"}}),
    );
    all_ok(&doc);
    let FeatureDef::DerivedPart(target) = &doc.feature(tp).unwrap().def else {
        panic!();
    };
    let cut = target.ends.b.clone().unwrap();
    assert!((doc.parameters().value(cut.angle).unwrap() - 30f64.to_radians()).abs() < 1e-9);
    assert_eq!(
        cut.tilt,
        Some(mitcad_model::features::derived_part::Tilt::Y)
    );
    let ep = derived(&doc, component(&existing));
    let FeatureDef::DerivedPart(new) = &doc.feature(ep).unwrap().def else {
        panic!();
    };
    assert!(new.ends.is_square());
    // New on a side: only the new part's end.
    let side = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x-_1)")},
                      "offset": 500},
               "length": 300, "mitre": {"cut": "new", "angle": "45 deg"}}),
    );
    all_ok(&doc);
    let sp = derived(&doc, component(&side));
    let FeatureDef::DerivedPart(d) = &doc.feature(sp).unwrap().def else {
        panic!();
    };
    assert!(d.ends.a.is_some() && d.ends.b.is_none());
    let error = command(
        &mut doc,
        json!({"cmd": "build_place", "part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_y-_1)")}},
               "length": 300, "mitre": {"cut": "shared"}}),
    )
    .unwrap_err();
    assert!(error.contains("only the new part's end"), "{error}");
}

#[test]
fn refusals_change_nothing() {
    let mut doc = design();
    let t = first(&mut doc, "p20", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let revision = doc.revision();
    let refused = |doc: &mut mitcad_model::Document<mitcad_model::testing::MockKernel>,
                   fields: serde_json::Value| {
        let mut fields = fields;
        fields["cmd"] = json!("build_place");
        command(doc, fields).unwrap_err()
    };
    // A panel through build_place.
    let e = refused(&mut doc, json!({"part": part("panel")}));
    assert!(e.contains("build_panel"), "{e}");
    // A cap of another system on the end.
    let e = refused(
        &mut doc,
        json!({"part": part("cap30"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")}}}),
    );
    assert!(e.contains("the systems do not fit"), "{e}");
    // A length beyond the part's limits.
    let e = refused(&mut doc, json!({"part": {"source": "S1"}, "length": 7000}));
    assert!(e.contains("longer than the part's longest"), "{e}");
    // A rotation that is not a quarter turn.
    let e = refused(
        &mut doc,
        json!({"part": {"source": "S1"}, "rotate": "30 deg"}),
    );
    assert!(e.contains("0, 90, 180 or 270"), "{e}");
    // A part without build.
    let e = refused(&mut doc, json!({"part": part("screw6")}));
    assert!(e.contains("has no build"), "{e}");
    assert_eq!(doc.revision(), revision);
    // A closed side.
    let mut doc = design();
    let c = first(&mut doc, "p30-2n", [0.0; 3], 1000.0);
    let cp = derived(&doc, component(&c));
    let face = format!("{cp}:part(F2:side(c2))");
    doc.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        mitcad_model::datum::SurfaceGeometry::Plane {
            origin: [0.0, 15.0, 0.0],
            normal: [0.0, 1.0, 0.0],
        },
    );
    let e = refused(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": c["occurrence"],
                      "geometry": {"body": format!("{cp}.b0"), "face": face},
                      "point": [0.0, 15.0, 100.0]}}),
    );
    assert!(e.contains("closed side"), "{e}");
}

#[test]
fn a_cap_goes_on_an_end_of_its_system() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let cap = place(
        &mut doc,
        json!({"part": part("cap30"),
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")}}}),
    );
    all_ok(&doc);
    assert_eq!(cap["place"], "end");
    let at = placed(&doc, occurrence(&cap));
    // Its frame (z out of the cap, −Z) against end B (z +Z): the cap on
    // top.
    assert!(close(at.translation, [0.0, 0.0, 1000.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, 1.0]), "{at:?}");
}

#[test]
fn derived_parts_save_reopen_and_undo() {
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
                      "offset": 100},
               "length": 500}),
    );
    let text = doc.to_json();
    assert!(text.contains("\"library_sources\""), "{text}");
    assert!(text.contains("\"derived_part\""));
    assert!(text.contains("\"build\""));
    let mut reopened =
        mitcad_model::Document::from_json(&text, mitcad_model::testing::MockKernel::default())
            .unwrap();
    reopened.recompute();
    all_ok(&reopened);
    assert_eq!(reopened.to_json(), text);
    assert_eq!(
        reopened.state().library_sources,
        doc.state().library_sources
    );
    // Undo takes the part away; undoing the first one takes the source too.
    doc.undo();
    doc.undo();
    assert!(doc.state().library_sources.is_empty());
    doc.redo();
    assert_eq!(doc.state().library_sources.0.len(), 1);
}

/// The body box of a placed part in the design (the mock kernel's cuts
/// clip it as half-spaces, `MockKernel::half_space_cuts`).
fn world_box(
    doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>,
    answer: &serde_json::Value,
) -> mitcad_model::kernel::BoundingBox {
    let local = doc.part_bounds(component(answer)).unwrap();
    mitcad_model::internal::build::moved_box(&local, &placed(doc, occurrence(answer)))
}

/// The cut plane of a placed 20 x 20 part's end in the design, as
/// `derived_part` cuts it: a point on it and its outward normal.
fn world_plane(
    doc: &mitcad_model::Document<mitcad_model::testing::MockKernel>,
    answer: &serde_json::Value,
    b: bool,
) -> ([f64; 3], [f64; 3]) {
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(doc, component(answer))).unwrap().def
    else {
        panic!("a derived part");
    };
    let cut = d.ends.end(b).unwrap();
    let angle = doc.parameters().value(cut.angle).unwrap();
    let length = doc.part_info(component(answer)).unwrap().length.unwrap().1;
    let section = mitcad_model::kernel::BoundingBox {
        min: [-10.0, -10.0, 0.0],
        max: [10.0, 10.0, length],
    };
    let (point, normal) = mitcad_model::internal::features::end_plane(
        Some(&section),
        b,
        length,
        angle,
        cut.tilt.unwrap_or_default(),
    )
    .unwrap();
    let at = placed(doc, occurrence(answer));
    (at.apply_point(point), at.apply_vector(normal))
}

#[test]
fn an_angled_end_keeps_the_part_within_its_length() {
    // mitcad#443: the length is the long point; the cut runs through the
    // section's edge on the long side, the short point 20 x tan 45 shorter.
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let t = first(&mut doc, "p20", [0.0; 3], 500.0);
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"],
               "ends": {"b": {"angle": "45 deg", "tilt": "x"}}}),
    )
    .unwrap();
    all_ok(&doc);
    let b = world_box(&doc, &t);
    assert!(
        (b.min[2] - 0.0).abs() < 1e-6 && (b.max[2] - 500.0).abs() < 1e-6,
        "{b:?}"
    );
    // Long point at y = -10 (the cut leans towards +y), short point at
    // y = +10, 480.
    let (point, normal) = world_plane(&doc, &t, true);
    assert!(close(point, [0.0, -10.0, 500.0]), "{point:?}");
    let short = [0.0, 10.0, 480.0];
    let gap = dot(normal, std::array::from_fn(|i| short[i] - point[i]));
    assert!(gap.abs() < 1e-9, "{gap}");
    // The same at end A, and the cut list keeps the length.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": t["component"],
               "ends": {"a": {"angle": "30 deg", "tilt": "-y"}}}),
    )
    .unwrap();
    all_ok(&doc);
    let b = world_box(&doc, &t);
    assert!(
        (b.min[2] - 0.0).abs() < 1e-6 && (b.max[2] - 500.0).abs() < 1e-6,
        "{b:?}"
    );
    let (point, _) = world_plane(&doc, &t, false);
    assert!(close(point, [10.0, 0.0, 0.0]), "{point:?}");
    let list = doc.build_cut_list(mitcad_model_build::CutListOptions::default());
    assert_eq!(list.rows.len(), 1, "{:?}", list.rows);
    assert!((list.rows[0].length - 500.0).abs() < 1e-9);
}

#[test]
fn a_shared_mitre_corner_measures_its_lengths_outside() {
    // Two 500 parts at a square corner with a shared mitre: 500 x 500
    // outside, both 500 in the cut list (mitcad#443).
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let t = first(&mut doc, "p20", [0.0; 3], 500.0);
    let tp = derived(&doc, component(&t));
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [0, 1, 0]},
               "length": 500, "mitre": {"cut": "shared"}}),
    );
    all_ok(&doc);
    let (a, b) = (world_box(&doc, &t), world_box(&doc, &p));
    let all = a.union(&b);
    assert!(
        (all.max[1] - all.min[1] - 500.0).abs() < 1e-6,
        "{a:?} {b:?}"
    );
    assert!(
        (all.max[2] - all.min[2] - 500.0).abs() < 1e-6,
        "{a:?} {b:?}"
    );
    // The new part's outer side flush with the target's end, its end flush
    // with the target's outer side.
    assert!(
        (b.max[2] - 500.0).abs() < 1e-6 && (b.min[1] + 10.0).abs() < 1e-6,
        "{b:?}"
    );
    // One cut plane through the shared outer corner.
    let (tp_point, tp_normal) = world_plane(&doc, &t, true);
    let (pp_point, pp_normal) = world_plane(&doc, &p, false);
    assert!(close(tp_point, [0.0, -10.0, 500.0]), "{tp_point:?}");
    assert!(close(pp_point, [0.0, -10.0, 500.0]), "{pp_point:?}");
    assert!(
        close(tp_normal, pp_normal.map(|v| -v)),
        "{tp_normal:?} {pp_normal:?}"
    );
    let list = doc.build_cut_list(mitcad_model_build::CutListOptions::default());
    assert_eq!(
        list.rows.iter().map(|r| r.quantity).sum::<usize>(),
        2,
        "{:?}",
        list.rows
    );
    for row in &list.rows {
        assert!((row.length - 500.0).abs() < 1e-9, "{row:?}");
    }
    // The two ends meet: an L of ends, though the end frames are apart.
    let junctions = doc.build_junctions(None);
    assert_eq!(junctions.len(), 1, "{junctions:?}");
    assert_eq!(junctions[0].kind, "L");
    assert_eq!(
        (junctions[0].a.at.as_str(), junctions[0].b.at.as_str()),
        ("end_a", "end_b")
    );
}

#[test]
fn a_shared_mitre_at_another_angle_meets_at_the_outer_corner() {
    // A 45 degree turn (the default there is a shared mitre of 22.5 each):
    // both parts' long points at the target's outer edge at its end, on one
    // cut plane (mitcad#443).
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let t = first(&mut doc, "p20", [0.0; 3], 500.0);
    let tp = derived(&doc, component(&t));
    let half = std::f64::consts::FRAC_1_SQRT_2;
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": t["occurrence"], "geometry": {"mount": format!("{tp}:mount(end_b)")},
                      "direction": [0.0, half, half]},
               "length": 300}),
    );
    all_ok(&doc);
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(&doc, component(&p))).unwrap().def else {
        panic!("a derived part");
    };
    let angle = doc
        .parameters()
        .value(d.ends.a.as_ref().unwrap().angle)
        .unwrap();
    assert!(
        (angle - std::f64::consts::FRAC_PI_8).abs() < 1e-9,
        "{angle}"
    );
    let (tp_point, tp_normal) = world_plane(&doc, &t, true);
    let (pp_point, pp_normal) = world_plane(&doc, &p, false);
    assert!(close(tp_point, [0.0, -10.0, 500.0]), "{tp_point:?}");
    assert!(close(pp_point, [0.0, -10.0, 500.0]), "{pp_point:?}");
    assert!(
        close(tp_normal, pp_normal.map(|v| -v)),
        "{tp_normal:?} {pp_normal:?}"
    );
    // The target does not reach past its length.
    assert!((world_box(&doc, &t).max[2] - 500.0).abs() < 1e-6);
}

#[test]
fn a_slider_goes_on_the_slot_of_the_side_picked() {
    // mitcad#448: a part that slides in a slot, placed with a side face of
    // a four-slot profile, goes on that side's slot (as a profile on a side
    // does), not on the first slot frame the profile lists.
    let mut doc = design();
    let t = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(&doc, component(&t));
    let sides = [
        ("x+", "c1", [1.0, 0.0, 0.0]),
        ("y+", "c2", [0.0, 1.0, 0.0]),
        ("x-", "c3", [-1.0, 0.0, 0.0]),
        ("y-", "c4", [0.0, -1.0, 0.0]),
    ];
    let mut frames = Vec::new();
    for (side, curve, normal) in sides {
        let face = format!("{tp}:part(F2:side({curve}))");
        let on_face = normal.map(|n| n * 15.0);
        doc.kernel().surfaces.borrow_mut().insert(
            face.clone(),
            mitcad_model::datum::SurfaceGeometry::Plane {
                origin: on_face,
                normal,
            },
        );
        let at = json!({"occurrence": t["occurrence"],
                        "geometry": {"body": format!("{tp}.b0"), "face": face},
                        "point": [on_face[0], on_face[1], 422.0]});
        // The hover's preview shows it there before the click.
        let shown = query(
            &doc,
            json!({"query": "build_preview", "part": part("slider8"), "at": at.clone()}),
        );
        assert_eq!(shown["place"], "side", "{side}: {shown}");
        let s = place(&mut doc, json!({"part": part("slider8"), "at": at}));
        all_ok(&doc);
        assert_eq!(s["place"], "side", "{side}: {s}");
        // On the side's slot, centred on the click along the profile, its
        // top out of the side.
        let at = placed(&doc, occurrence(&s));
        assert!(
            close(at.translation, [on_face[0], on_face[1], 422.0]),
            "{side}: {at:?}"
        );
        assert!(close(axis(&at), normal), "{side}: {at:?}");
        // A slider along the slot, by the frame its mate names.
        let sp = derived(&doc, component(&s));
        let joint = s["joint"].as_str().unwrap().parse().unwrap();
        let FeatureDef::Joint(j) = &doc.feature(joint).unwrap().def else {
            panic!("a joint");
        };
        assert_eq!(j.kind, mitcad_model::joints::JointKind::Slider, "{side}");
        assert_eq!(
            j.a.geometry,
            mitcad_model::features::GeomRef::Mount {
                feature: sp,
                name: "slot".to_owned()
            },
            "{side}"
        );
        assert_eq!(
            j.b.geometry,
            mitcad_model::features::GeomRef::Mount {
                feature: tp,
                name: format!("slot_{side}_1")
            },
            "{side}"
        );
        frames.push(format!("{:?}", j.b.geometry));
    }
    // Four different frames.
    frames.sort();
    frames.dedup();
    assert_eq!(frames.len(), 4, "{frames:?}");
}
