// SPDX-License-Identifier: MIT
//! Fasteners chosen and swapped (mitcad#497): a frame's fastener, size and
//! count, a spacing set fastened to its hosts, fasteners swapped in place
//! for another kind and size in one undo step, and the warnings of a size
//! too short, on the test library's timber parts.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::ids::ComponentUid;
use mitcad_model::testing::MockKernel;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::{all_ok, command, design, part, place, query};

fn parts_of(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .clone()
}

fn fasteners(doc: &Document<MockKernel>) -> Vec<Value> {
    parts_of(doc)
        .into_iter()
        .filter(|p| p["role"] == "fastener")
        .collect()
}

/// The fasteners by their junction.
fn by_junction(doc: &Document<MockKernel>) -> BTreeMap<String, Vec<Value>> {
    let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for f in fasteners(doc) {
        let j = f["junction"].as_str().unwrap_or_default().to_owned();
        out.entry(j).or_default().push(f);
    }
    out
}

/// A wall along X from the origin, 2400 high, studs at 600.
fn wall(doc: &mut Document<MockKernel>, length: f64, extra: Value) -> Value {
    let mut fields = json!({"cmd": "build_frame", "kind": "wall", "part": part("rows98"),
                            "start": [0, 0, 0], "end": [length, 0, 0], "height": 2400,
                            "spacing": 600});
    for (k, v) in extra.as_object().unwrap() {
        fields[k] = v.clone();
    }
    command(doc, fields).unwrap_or_else(|e| panic!("build_frame: {e}"))
}

/// A grounded member by two points.
fn member(doc: &mut Document<MockKernel>, from: [f64; 3], to: [f64; 3]) -> Value {
    place(
        doc,
        json!({"part": part("beam148"), "at": {"point": from}, "to": {"point": to},
               "lie": "edge"}),
    )
}

fn mount(doc: &Document<MockKernel>, answer: &Value, frame: &str) -> Value {
    let c: ComponentUid = answer["component"].as_str().unwrap().parse().unwrap();
    json!({"mount": format!("{}:mount({frame})", mitcad_model_tests_support::test_library::derived(doc, c))})
}

/// A component's occurrence placement, in the design.
fn at(doc: &Document<MockKernel>, component: &str) -> mitcad_model::transform::Transform {
    let o = doc
        .assembly()
        .occurrences
        .iter()
        .find(|o| o.component.to_string() == component)
        .unwrap()
        .uid;
    doc.placement(o).unwrap()
}

#[test]
fn a_frame_with_a_chosen_nail_row_and_count_has_them_at_every_junction() {
    let mut doc = design();
    let mut nail = part("ring-nail");
    nail["config"] = json!("3.1x90");
    let made = wall(
        &mut doc,
        1800.0,
        json!({"fastener": nail, "fastener_count": 3}),
    );
    all_ok(&doc);
    // Studs at 0, 600 (centre), 1200 and the end: each end on both plates.
    assert_eq!(made["fasteners"], 8, "{made}");
    let groups = by_junction(&doc);
    assert_eq!(groups.len(), 8, "{groups:?}");
    for (junction, group) in &groups {
        assert_eq!(group.len(), 3, "{junction}: {group:?}");
        for f in group {
            assert_eq!(f["config"], "3.1x90", "{f}");
            assert_eq!(f["designation"], "Ring nail 3.1x90", "{f}");
        }
    }
    // Kept with the frame: laid out again with the same nails and count.
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": made["frame"], "frame": {"spacing": 400}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(edited["fasteners"], 12, "{edited}");
    let groups = by_junction(&doc);
    assert!(groups.values().all(|g| g.len() == 3), "{groups:?}");
    assert!(
        fasteners(&doc).iter().all(|f| f["config"] == "3.1x90"),
        "{:?}",
        fasteners(&doc)
    );
    // Without a row the nail is sized from the members: two by default.
    let mut doc = design();
    let made = wall(&mut doc, 1800.0, json!({"fastener": part("ring-nail")}));
    let groups = by_junction(&doc);
    assert!(groups.values().all(|g| g.len() == 2), "{groups:?}");
    // Named by its source in the design (as the app names a part the
    // design has), it is kept as its library's part: laid out again after
    // the frame's fasteners and so the source went, it is found again.
    let source = fasteners(&doc)[0]["source"].clone();
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": made["frame"],
               "frame": {"spacing": 400, "fastener": {"source": source, "config": "2.8x50"}}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(edited["fasteners"], 12, "{edited}");
    assert!(
        fasteners(&doc).iter().all(|f| f["config"] == "2.8x50"),
        "{:?}",
        fasteners(&doc)
    );
}

#[test]
fn a_spacing_set_with_fasteners_fastens_each_member_to_its_hosts() {
    let mut doc = design();
    let b1 = member(&mut doc, [0.0, 0.0, 0.0], [3000.0, 0.0, 0.0]);
    let b2 = member(&mut doc, [0.0, 2000.0, 0.0], [3000.0, 2000.0, 0.0]);
    let set = joists(
        &mut doc,
        &b1,
        &b2,
        json!({"mode": "centre", "distance": 600, "fastener": part("wood-screw"),
               "fastener_count": 2}),
    );
    all_ok(&doc);
    // (3000 - 48) / 600 rounded up, and one: six joists, each on both beams.
    assert_eq!(set["count"], 6, "{set}");
    assert_eq!(set["fasteners"], 12, "{set}");
    let label = doc.undo_label().unwrap_or_default().to_owned();
    assert!(
        label.starts_with("Place ") && label.ends_with(" x6"),
        "{label}"
    );
    let members: Vec<String> = set["set"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    let groups = by_junction(&doc);
    for m in &members {
        for beam in [&b1, &b2] {
            let beam = beam["component"].as_str().unwrap();
            let fastened = groups.iter().any(|(j, g)| {
                j.split('|').any(|s| s.starts_with(&format!("{m}:")))
                    && j.split('|').any(|s| s.starts_with(&format!("{beam}:")))
                    && g.len() == 2
            });
            assert!(fastened, "{m} on {beam}: {:?}", groups.keys());
        }
    }
    // One undo step takes the set and its fasteners.
    doc.undo();
    assert_eq!(parts_of(&doc).len(), 2, "{:?}", parts_of(&doc));
    doc.redo();
    // Laid out again: the fasteners anew with the members.
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": members[1], "spacing": {"distance": 400}}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(edited["count"], 9, "{edited}");
    assert_eq!(edited["fasteners"], 18, "{edited}");
    assert_eq!(fasteners(&doc).len(), 36);
    // Off by default: a set without a fastener has none.
    let mut doc = design();
    let b1 = member(&mut doc, [0.0, 0.0, 0.0], [3000.0, 0.0, 0.0]);
    let b2 = member(&mut doc, [0.0, 2000.0, 0.0], [3000.0, 2000.0, 0.0]);
    joists(
        &mut doc,
        &b1,
        &b2,
        json!({"mode": "centre", "distance": 600}),
    );
    assert!(fasteners(&doc).is_empty());
}

/// Joists between two beams along X (on edge), as a spacing set.
fn joists(doc: &mut Document<MockKernel>, b1: &Value, b2: &Value, spacing: Value) -> Value {
    let on = mount(doc, b1, "face_yn");
    let to = mount(doc, b2, "face_yp");
    place(
        doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": b1["occurrence"], "geometry": on},
               "rotate": "90 deg",
               "length": {"bind": {"occurrence": b2["occurrence"], "geometry": to}},
               "spacing": spacing}),
    )
}

#[test]
fn two_wood_screws_swapped_for_a_ring_nail_keep_their_places_and_joints() {
    let mut doc = design();
    wall(&mut doc, 1800.0, json!({"fastener": part("wood-screw")}));
    all_ok(&doc);
    let groups = by_junction(&doc);
    let (junction, pair) = groups.iter().next().unwrap();
    let pair = pair.clone();
    assert_eq!(pair.len(), 2);
    let before: Vec<(Value, Value, mitcad_model::transform::Transform)> = pair
        .iter()
        .map(|f| {
            (
                f["joint"].clone(),
                f["part"].clone(),
                at(&doc, f["component"].as_str().unwrap()),
            )
        })
        .collect();
    let screw = pair[0]["source"].clone();
    let others = fasteners(&doc).len();
    let mut nail = part("ring-nail");
    nail["config"] = json!("3.1x90");
    let swapped = command(
        &mut doc,
        json!({"cmd": "build_swap_fasteners",
               "fasteners": [pair[0]["component"], pair[1]["component"]], "part": nail}),
    )
    .unwrap();
    all_ok(&doc);
    assert_eq!(swapped["count"], 2, "{swapped}");
    assert_eq!(swapped["sizes"][0]["config"], "3.1x90", "{swapped}");
    assert_eq!(
        doc.undo_label(),
        Some("Swap 2 fasteners for Ring nail 3.1x90")
    );
    let now = fasteners(&doc);
    assert_eq!(now.len(), others, "the same number of fasteners");
    for (f, (joint, derived, placement)) in pair.iter().zip(&before) {
        let after = now
            .iter()
            .find(|p| p["component"] == f["component"])
            .unwrap();
        // The same component, joint, derived part and junction, in the
        // same place: another source and row.
        assert_eq!(after["joint"], *joint);
        assert_eq!(after["part"], *derived);
        assert_eq!(after["junction"], json!(junction));
        assert_eq!(after["config"], "3.1x90");
        assert_eq!(after["designation"], "Ring nail 3.1x90");
        assert_ne!(after["source"], screw);
        let p = at(&doc, f["component"].as_str().unwrap());
        assert!(
            mitcad_model_tests_support::test_library::close(p.translation, placement.translation),
            "{:?} / {:?}",
            p.translation,
            placement.translation
        );
    }
    // The other junctions keep their screws.
    assert_eq!(
        now.iter().filter(|f| f["source"] == screw).count(),
        others - 2
    );
    // One undo step back to the screws.
    doc.undo();
    let back = fasteners(&doc);
    for f in &pair {
        let again = back
            .iter()
            .find(|p| p["component"] == f["component"])
            .unwrap();
        assert_eq!(again["source"], screw);
        assert_eq!(again["config"], f["config"]);
    }
    // A slot connector's nut is no wood fastener's to swap.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_swap_fasteners", "fasteners": [pair[0]["component"]],
               "part": part("tnut8")}),
    )
    .unwrap_err();
    assert!(refused.contains("is no wood screw or nail"), "{refused}");
    let refused = command(
        &mut doc,
        json!({"cmd": "build_swap_fasteners", "fasteners": [pair[0]["component"]],
               "part": part("beam148")}),
    )
    .unwrap_err();
    assert!(refused.contains("is no fastener"), "{refused}");
}

#[test]
fn a_size_too_short_warns() {
    let mut doc = design();
    // A 2.8 x 50 nail through a 48 plate into a stud's end: 2 mm in it.
    let mut short = part("ring-nail");
    short["config"] = json!("2.8x50");
    let made = wall(&mut doc, 1200.0, json!({"fastener": short}));
    let warnings: Vec<&str> = made["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.starts_with("nail 1 embeds 2 mm in") && w.ends_with("at least 16.8 mm")),
        "{warnings:?}"
    );
    // Swapped for the short nail: the same warning for its new size.
    let mut doc = design();
    wall(&mut doc, 1200.0, json!({"fastener": part("wood-screw")}));
    let groups = by_junction(&doc);
    let pair = groups.values().next().unwrap().clone();
    let swapped = command(
        &mut doc,
        json!({"cmd": "build_swap_fasteners",
               "fasteners": [pair[0]["component"], pair[1]["component"]], "part": short}),
    )
    .unwrap();
    let warnings: Vec<&str> = swapped["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("embeds 2 mm in") && w.ends_with("at least 16.8 mm")),
        "{warnings:?}"
    );
    // Auto: sized from the members, the longest nail warns it embeds
    // fewer than 8 diameters.
    let pair_ids = [pair[0]["component"].clone(), pair[1]["component"].clone()];
    let auto = command(
        &mut doc,
        json!({"cmd": "build_swap_fasteners", "fasteners": pair_ids, "part": part("ring-nail")}),
    )
    .unwrap();
    assert_eq!(auto["sizes"][0]["config"], "3.1x90", "{auto}");
    all_ok(&doc);
}

/// Check actual body extents against the member's original entry face,
/// independently of the seat metadata and requested joint override.
fn check_seats(doc: &Document<MockKernel>, wanted_top: f64, wanted_tip: f64) {
    use mitcad_model::internal::build::{dot, sub};
    let mut count = 0;
    for f in fasteners(doc) {
        let c: ComponentUid = f["component"].as_str().unwrap().parse().unwrap();
        let info = doc.part_info(c).unwrap();
        let entry = doc.entry_point(info.record.joint.unwrap()).unwrap();
        let member = doc.member(c).unwrap();
        let axis = member.placement.apply_vector([0.0, 0.0, 1.0]);
        let (top, tip) = member
            .bounds
            .corners()
            .map(|p| dot(sub(member.placement.apply_point(p), entry), axis))
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
        assert!(
            (top - wanted_top).abs() <= 1e-6,
            "{f}: top {top}, expected {wanted_top}"
        );
        assert!(
            (tip - wanted_tip).abs() <= 1e-6,
            "{f}: tip {tip}, expected {wanted_tip}"
        );
        let queried = f["seating"]["head_top_distance"].as_f64().unwrap();
        assert!((queried - top).abs() <= 1e-6, "{f}");
        count += 1;
    }
    assert!(count > 0);
}

#[test]
fn generated_frame_fasteners_seat_their_own_head_kind_without_cutting_members() {
    for (part_id, top, tip) in [
        ("proud-screw", 0.0, 63.0),
        ("proud-nail", 0.0, 61.5),
        ("pan-screw", -3.0, 60.0),
        ("hex-bolt", -4.0, 60.0),
        ("named-seat", 0.0, 63.0),
        ("bearing-seat", -3.0, 60.0),
    ] {
        let mut doc = design();
        wall(&mut doc, 1200.0, json!({"fastener": part(part_id)}));
        all_ok(&doc);
        check_seats(&doc, top, tip);
        let boards: Vec<Value> = parts_of(&doc)
            .into_iter()
            .filter(|p| p["kind"] == "timber")
            .collect();
        assert!(
            boards
                .iter()
                .all(|b| b["cuts"].as_array().unwrap().is_empty()
                    && b["machining"].as_array().unwrap().is_empty()),
            "{boards:?}"
        );
        let before = doc.to_json();
        let mut reopened = Document::from_json(&before, MockKernel::default()).unwrap();
        // Deserialization restores definitions only, including embedded
        // library sources. Rebuild the geometry before measuring it.
        reopened.recompute();
        all_ok(&reopened);
        assert_eq!(reopened.to_json(), before);
        check_seats(&reopened, top, tip);
    }
}

#[test]
fn swap_fastener_reseats_the_new_kind_and_keeps_member_entry_frames() {
    let mut doc = design();
    wall(&mut doc, 1200.0, json!({"fastener": part("proud-screw")}));
    let initial = fasteners(&doc);
    let ids: Vec<Value> = initial.iter().map(|f| f["component"].clone()).collect();
    let entries: Vec<_> = initial
        .iter()
        .map(|f| {
            let c = f["component"].as_str().unwrap().parse().unwrap();
            let joint = doc.part_info(c).unwrap().record.joint.unwrap();
            (joint, doc.entry_point(joint).unwrap())
        })
        .collect();
    for (part_id, top, tip) in [
        ("proud-nail", 0.0, 61.5),
        ("pan-screw", -3.0, 60.0),
        ("hex-bolt", -4.0, 60.0),
        ("proud-screw", 0.0, 63.0),
    ] {
        command(
            &mut doc,
            json!({"cmd": "build_swap_fasteners", "fasteners": ids, "part": part(part_id)}),
        )
        .unwrap();
        all_ok(&doc);
        check_seats(&doc, top, tip);
        for (joint, entry) in &entries {
            assert!(mitcad_model_tests_support::test_library::close(
                doc.entry_point(*joint).unwrap(),
                *entry
            ));
        }
    }
    doc.undo();
    check_seats(&doc, -4.0, 60.0);
    doc.redo();
    check_seats(&doc, 0.0, 63.0);
}

#[test]
fn near_axis_shafts_need_an_explicit_parallel_seat_and_no_box_measurement() {
    let doc = design();
    let source = |id| {
        doc.source_use(&serde_json::from_value(part(id)).unwrap())
            .unwrap()
    };
    let refused = doc
        .wood_seat(&source("oblique-implicit"))
        .unwrap_err()
        .to_string();
    assert!(refused.contains("oblique shaft"), "{refused}");
    let refused = doc
        .wood_seat(&source("misaligned-seat"))
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("does not point along its shaft"),
        "{refused}"
    );
    assert_eq!(doc.wood_seat(&source("oblique-seat")).unwrap().0, "drive");
    for i in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut z = [0.0; 3];
            z[i] = sign;
            assert!(mitcad_model_build::internal::shaft_axis_is_aligned(z));
            z[(i + 1) % 3] = 1e-5;
            assert!(!mitcad_model_build::internal::shaft_axis_is_aligned(z));
        }
    }
    let mut doc = design();
    wall(&mut doc, 1200.0, json!({"fastener": part("oblique-seat")}));
    all_ok(&doc);
    let fasteners = fasteners(&doc);
    assert!(!fasteners.is_empty());
    assert!(
        fasteners.iter().all(|part| part["seating"].is_null()),
        "{fasteners:?}"
    );
}
