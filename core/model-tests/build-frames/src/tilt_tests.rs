// SPDX-License-Identifier: MIT
//! Tilted parts on a side (mitcad#444) with the mock kernel: a brace's
//! foot lies flat on the side at its long point, its bound length ends
//! flush on a parallel host, the cut list gives both ends' angles, a typed
//! length keeps the far end square, editing the tilt back to 0 squares the
//! part, and the refusals.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::*;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The cut plane of a placed 20 x 20 part's end in the design (a point on
/// it and its outward normal), as `derived_part` cuts it; None when the end
/// is square.
fn plane(doc: &Document<MockKernel>, answer: &Value, b: bool) -> Option<([f64; 3], [f64; 3])> {
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(doc, component(answer))).unwrap().def
    else {
        panic!("a derived part");
    };
    let cut = d.ends.end(b)?;
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
    Some((at.apply_point(point), at.apply_vector(normal)))
}

/// Two 20 profiles 1000 long, 300 apart along x (their facing sides at
/// x = 10 and x = 290).
fn two_hosts(doc: &mut Document<MockKernel>) -> (Value, Value) {
    let t = first(doc, "p20", [0.0; 3], 1000.0);
    let x = first(doc, "p20", [300.0, 0.0, 0.0], 1000.0);
    (t, x)
}

fn on_x_side(doc: &Document<MockKernel>, t: &Value, offset: Value) -> Value {
    let tp = derived(doc, component(t));
    json!({"occurrence": t["occurrence"],
           "geometry": {"mount": format!("{tp}:mount(slot_x+_1)")},
           "offset": offset})
}

fn bound_to(doc: &Document<MockKernel>, x: &Value) -> Value {
    let xp = derived(doc, component(x));
    json!({"bind": {"occurrence": x["occurrence"],
                    "geometry": {"mount": format!("{xp}:mount(slot_x-_1)")}}})
}

#[test]
fn a_tilted_brace_stands_flat_and_ends_flush_on_a_parallel_host() {
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let (t, x) = two_hosts(&mut doc);
    let at = on_x_side(&doc, &t, json!(100));
    let length = bound_to(&doc, &x);
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": at, "length": length, "tilt": "45 deg"}),
    );
    all_ok(&doc);
    assert_eq!(p["place"], "side");
    let at = placed(&doc, occurrence(&p));
    let h = std::f64::consts::FRAC_1_SQRT_2;
    // It leans towards the host's end B, in the plane of the host's axis
    // and the side's normal.
    assert!(
        close(at.apply_vector([0.0, 0.0, 1.0]), [h, 0.0, h]),
        "{at:?}"
    );
    // Its foot on the side (x = 10, facing into the host), its long point
    // 100 from end A on the slot's centre line.
    let (point, normal) = plane(&doc, &p, false).unwrap();
    assert!(close(normal, [-1.0, 0.0, 0.0]), "{normal:?}");
    assert!(close(point, [10.0, 0.0, 100.0]), "{point:?}");
    // Its far end flush on the other host's side (x = 290), the length to
    // its long point: 280 / cos 45 + 20 tan 45.
    let (point, normal) = plane(&doc, &p, true).unwrap();
    assert!(close(normal, [1.0, 0.0, 0.0]), "{normal:?}");
    assert!(near(point[0], 290.0), "{point:?}");
    let length = doc.part_info(component(&p)).unwrap().length.unwrap().1;
    assert!(near(length, 280.0 * 2f64.sqrt() + 20.0), "{length}");
    // Nothing of it past its length along its axis.
    let local = doc.part_bounds(component(&p)).unwrap();
    assert!(
        local.min[2] > -1e-6 && local.max[2] < length + 1e-6,
        "{local:?}"
    );
    // The cut list gives both ends' angles.
    let list = doc.build_cut_list(mitcad_model_build::CutListOptions::default());
    let row = list
        .rows
        .iter()
        .find(|r| r.components.contains(&component(&p)))
        .unwrap();
    for end in &row.ends {
        let (angle, _) = end.unwrap();
        assert!(near(angle, std::f64::consts::FRAC_PI_4), "{row:?}");
    }
    assert!(near(row.length, length));
    // The parts list and the preview give the tilt.
    let parts = query(&doc, json!({"query": "build_parts"}));
    let entry = parts["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["component"] == p["component"])
        .unwrap();
    assert!(near(
        entry["tilt"].as_f64().unwrap(),
        std::f64::consts::FRAC_PI_4
    ));
    let shown = query(
        &doc,
        json!({"query": "build_preview", "part": {"source": "S1"},
               "at": on_x_side(&doc, &t, json!(400)), "length": 200, "tilt": "45 deg"}),
    );
    assert_eq!(shown["problems"], json!([]), "{shown}");
    assert!(near(
        shown["tilt"].as_f64().unwrap(),
        std::f64::consts::FRAC_PI_4
    ));
    let foot = &shown["side"]["foot"];
    assert!(near(foot[0].as_f64().unwrap(), 10.0) && near(foot[2].as_f64().unwrap(), 400.0));
    // The tilt edited back to 0: square, square ends, between the sides.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "tilt": 0}),
    )
    .unwrap();
    all_ok(&doc);
    let FeatureDef::DerivedPart(d) = &doc.feature(derived(&doc, component(&p))).unwrap().def else {
        panic!("a derived part");
    };
    assert!(d.ends.is_square(), "{:?}", d.ends);
    assert!(doc.part_info(component(&p)).unwrap().record.tilt.is_none());
    let at = placed(&doc, occurrence(&p));
    assert!(
        close(at.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]),
        "{at:?}"
    );
    let length = doc.part_info(component(&p)).unwrap().length.unwrap().1;
    assert!(near(length, 280.0), "{length}");
    // And tilted again by the edit: the cuts come back.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "tilt": "45 deg"}),
    )
    .unwrap();
    all_ok(&doc);
    let (point, normal) = plane(&doc, &p, true).unwrap();
    assert!(
        close(normal, [1.0, 0.0, 0.0]) && near(point[0], 290.0),
        "{point:?}"
    );
    let (point, _) = plane(&doc, &p, false).unwrap();
    assert!(near(point[0], 10.0), "{point:?}");
}

#[test]
fn a_typed_length_keeps_the_far_end_square_and_offsets_go_to_the_long_point() {
    let mut doc = design();
    doc.kernel().half_space_cuts.set(true);
    let (t, _) = two_hosts(&mut doc);
    let mut at = on_x_side(&doc, &t, json!(50));
    at["from"] = json!("b");
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": at, "length": 300, "tilt": "-30 deg"}),
    );
    all_ok(&doc);
    // Towards end A; the long point (on the side towards B) 50 from end B.
    let placement = placed(&doc, occurrence(&p));
    let (s, c) = 30f64.to_radians().sin_cos();
    assert!(close(placement.apply_vector([0.0, 0.0, 1.0]), [c, 0.0, -s]));
    let (point, normal) = plane(&doc, &p, false).unwrap();
    assert!(close(normal, [-1.0, 0.0, 0.0]), "{normal:?}");
    assert!(close(point, [10.0, 0.0, 950.0]), "{point:?}");
    assert!(plane(&doc, &p, true).is_none());
    let info = doc.part_info(component(&p)).unwrap();
    assert!(near(info.length.unwrap().1, 300.0));
    // An offset edited is to the long point too.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": p["component"], "at": {"offset": 80}}),
    )
    .unwrap();
    all_ok(&doc);
    let (point, _) = plane(&doc, &p, false).unwrap();
    assert!(close(point, [10.0, 0.0, 920.0]), "{point:?}");
}

#[test]
fn tilts_are_refused_off_a_side_beyond_the_limit_and_with_a_mitre() {
    let mut doc = design();
    let (t, _) = two_hosts(&mut doc);
    let tp = derived(&doc, component(&t));
    let revision = doc.revision();
    let refused = |doc: &mut Document<MockKernel>, fields: Value| {
        let mut fields = fields;
        fields["cmd"] = json!("build_place");
        command(doc, fields).unwrap_err()
    };
    let side = on_x_side(&doc, &t, json!(100));
    let e = refused(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": side.clone(), "length": 300, "tilt": "85 deg"}),
    );
    assert!(e.contains("from -80 up to 80"), "{e}");
    let e = refused(
        &mut doc,
        json!({"part": {"source": "S1"}, "length": 300, "tilt": "20 deg",
               "at": {"occurrence": t["occurrence"],
                      "geometry": {"mount": format!("{tp}:mount(end_b)")}}}),
    );
    assert!(e.contains("only a part on a side tilts"), "{e}");
    let e = refused(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": side, "length": 300, "tilt": "20 deg",
               "mitre": {"cut": "new", "angle": "10 deg"}}),
    );
    assert!(e.contains("takes no miter"), "{e}");
    assert_eq!(doc.revision(), revision);
}
