// SPDX-License-Identifier: MIT
//! Timber construction (mitcad#478): members on face sides, members by two
//! points, spacing sets and board rows, wood fasteners, hangers, joinery,
//! sheets on a face and the timber cut list, on the test library's timber
//! parts.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::AnalysisApi;
use mitcad_model_build::prelude::*;
use mitcad_model_tests_support::test_library::{
    all_ok, close, command, component, derived, design, occurrence, part, place, placed, query,
};

/// A member's mount frame reference.
fn mount(doc: &Document<MockKernel>, answer: &Value, frame: &str) -> Value {
    json!({"mount": format!("{}:mount({frame})", derived(doc, component(answer)))})
}

/// A grounded member by two points (its axis through them).
fn member(
    doc: &mut Document<MockKernel>,
    part_id: &str,
    from: [f64; 3],
    to: [f64; 3],
    lie: &str,
) -> Value {
    place(
        doc,
        json!({"part": part(part_id), "at": {"point": from}, "to": {"point": to}, "lie": lie}),
    )
}

fn parts_of(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "build_parts"}))["parts"]
        .as_array()
        .unwrap()
        .clone()
}

fn length_of(doc: &Document<MockKernel>, c: &str) -> f64 {
    parts_of(doc)
        .into_iter()
        .find(|p| p["component"] == c)
        .unwrap()["length"]
        .as_f64()
        .unwrap()
}

#[test]
fn a_member_goes_on_a_face_side_anywhere_along_it() {
    let mut doc = design();
    let post = place(
        &mut doc,
        json!({"part": part("post98"), "at": {"point": [0, 0, 0]}, "length": 2400}),
    );
    // A beam on the post's x+ side, flush with its top (end B).
    let xp = mount(&doc, &post, "face_xp");
    let yp = mount(&doc, &post, "face_yp");
    let beam = place(
        &mut doc,
        json!({"part": part("beam148"),
               "at": {"occurrence": post["occurrence"], "geometry": xp,
                      "from": "b", "offset": 0},
               "length": 1500}),
    );
    assert_eq!(beam["place"], "side");
    all_ok(&doc);
    let t = placed(&doc, occurrence(&beam));
    // It runs out of the side along +X, its end on the side.
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    assert!(
        (t.translation[0] - 49.0).abs() < 1e-6,
        "{:?}",
        t.translation
    );
    // Flush with the top: its far face along the post (148 wide along it
    // when not turned) at the post's top.
    let record = &parts_of(&doc)
        .into_iter()
        .find(|p| p["component"] == beam["component"])
        .unwrap();
    assert_eq!(record["from"], "b");
    assert_eq!(record["kind"], "timber");
    // A free offset from end A, no grid.
    let other = place(
        &mut doc,
        json!({"part": {"source": "S2"},
               "at": {"occurrence": post["occurrence"], "geometry": yp,
                      "offset": 333.3},
               "length": 800}),
    );
    assert_eq!(other["place"], "side");
    // The junction: the beam's end on the post's face.
    let junctions = query(&doc, json!({"query": "build_junctions"}))["junctions"].clone();
    let j = junctions
        .as_array()
        .unwrap()
        .iter()
        .find(|j| j["a"]["component"] == beam["component"])
        .unwrap()
        .clone();
    assert_eq!(j["b"]["at"], "face_xp");
    assert_eq!(j["kind"], "L");
    assert!(j["wood"].is_object(), "{j}");
}

#[test]
fn a_slot_part_is_refused_on_a_member() {
    let mut doc = design();
    let post = place(
        &mut doc,
        json!({"part": part("post98"), "at": {"point": [0, 0, 0]}, "length": 2400}),
    );
    let xp = mount(&doc, &post, "face_xp");
    let refused = command(
        &mut doc,
        json!({"cmd": "build_place", "part": part("slider8"),
               "at": {"occurrence": post["occurrence"], "geometry": xp}}),
    )
    .unwrap_err();
    assert!(refused.contains("no slots"), "{refused}");
}

#[test]
fn a_member_by_two_points_lies_on_the_plane_flat_or_on_edge() {
    let mut doc = design();
    let beam = place(
        &mut doc,
        json!({"part": part("beam148"), "at": {"geometry": "xy", "point": [0, 0, 0]},
               "to": {"point": [3000, 0, 0]}, "lie": "edge"}),
    );
    assert_eq!(beam["place"], "points");
    all_ok(&doc);
    assert!((length_of(&doc, beam["component"].as_str().unwrap()) - 3000.0).abs() < 1e-9);
    let t = placed(&doc, occurrence(&beam));
    // Along +X, on edge: its 148 (X) up, lifted by half of it.
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    assert!(close(t.apply_vector([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
    assert!(
        close(t.translation, [0.0, 0.0, 74.0]),
        "{:?}",
        t.translation
    );
    // Flat: its 48 (Y) up, lifted by 24.
    let flat = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": "xy", "point": [0, 500, 0]},
               "to": {"point": [0, 2500, 0]}}),
    );
    let t = placed(&doc, occurrence(&flat));
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [0.0, 1.0, 0.0]));
    assert!(close(t.apply_vector([0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]));
    assert!(
        close(t.translation, [0.0, 500.0, 24.0]),
        "{:?}",
        t.translation
    );
    // Its end moved: placed anew, longer and flat, the same component.
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": beam["component"],
               "to": {"point": [4000, 0, 0]}, "lie": "flat"}),
    )
    .unwrap();
    assert_eq!(edited["changed"], json!(["points", "length"]));
    all_ok(&doc);
    assert!((length_of(&doc, beam["component"].as_str().unwrap()) - 4000.0).abs() < 1e-9);
    let t = placed(&doc, occurrence(&beam));
    assert!(
        close(t.translation, [0.0, 0.0, 24.0]),
        "{:?}",
        t.translation
    );
    // Grounded without a plane: its axis through the points.
    let free = member(
        &mut doc,
        "post98",
        [5000.0, 0.0, 0.0],
        [5000.0, 0.0, 2400.0],
        "flat",
    );
    let t = placed(&doc, occurrence(&free));
    assert!(close(t.translation, [5000.0, 0.0, 0.0]));
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]));
}

/// The bodies of a preview's shape.
fn preview_bodies(
    doc: &Document<MockKernel>,
    shape: &mitcad_model::testing::MockShape,
) -> Vec<mitcad_model::testing::MockShape> {
    use mitcad_model::kernel::Kernel;
    doc.kernel().solids(shape).unwrap()
}

/// Two beams along X, 2000 apart (centres), on edge.
fn two_beams(doc: &mut Document<MockKernel>) -> (Value, Value) {
    let b1 = member(doc, "beam148", [0.0, 0.0, 0.0], [3000.0, 0.0, 0.0], "edge");
    let b2 = member(
        doc,
        "beam148",
        [0.0, 2000.0, 0.0],
        [3000.0, 2000.0, 0.0],
        "edge",
    );
    (b1, b2)
}

#[test]
fn a_joist_set_at_400_between_two_beams_follows_its_distance() {
    let mut doc = design();
    let (b1, b2) = two_beams(&mut doc);
    // On edge the beams' Y (their 48) runs along -Y: face_yn faces +Y.
    let on = mount(&doc, &b1, "face_yn");
    let to = mount(&doc, &b2, "face_yp");
    // The joists on edge as the beams lie, unturned (mitcad#492).
    let set = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": b1["occurrence"], "geometry": on},
               "length": {"bind": {"occurrence": b2["occurrence"],
                                   "geometry": to}},
               "spacing": {"mode": "centre", "distance": 400}}),
    );
    all_ok(&doc);
    // 3000 long, joists 48 wide: (3000 - 48) / 400 rounded up, and one.
    assert_eq!(set["count"], 9, "{set}");
    let mut preview = json!({"query": "build_preview", "part": {"source": "S1"},
        "at": {"occurrence": b1["occurrence"], "geometry": mount(&doc, &b1, "face_yn")},
        "spacing": {"mode": "centre", "distance": 400}});
    preview["length"] = json!(1952);
    let mut shape_request = preview.clone();
    shape_request.as_object_mut().unwrap().remove("query");
    shape_request["shape"] = json!("build_preview");
    let preview = query(&doc, preview);
    assert_eq!(preview["spacing"]["count"], 9, "{preview}");
    // The preview's shape: every member's body, not the first one's only
    // (mitcad#487).
    let shape = doc
        .analysis_shape(&shape_request.to_string())
        .unwrap()
        .expect("the set's bodies");
    assert_eq!(preview_bodies(&doc, &shape).len(), 9, "{}", shape.history);
    let members: Vec<String> = set["set"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    for m in &members {
        assert!((length_of(&doc, m) - 1952.0).abs() < 1e-6);
    }
    // The last is flush with end B, bound from it.
    let parts = parts_of(&doc);
    let last = parts.iter().find(|p| p["component"] == members[8]).unwrap();
    assert_eq!(last["from"], "b");
    let lead = parts.iter().find(|p| p["component"] == members[0]).unwrap();
    assert_eq!(lead["spacing"]["count"], 9);
    assert_eq!(lead["set"], members[0]);
    // The distance changes: 600 gives six.
    let edited = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": members[3], "spacing": {"distance": 600}}),
    )
    .unwrap();
    assert_eq!(edited["count"], 6, "{edited}");
    all_ok(&doc);
    // The kept members are the first ones, the others gone.
    let now: Vec<String> = edited["set"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(now, members[..6].to_vec());
    assert!(
        doc.assembly()
            .component(members[7].parse().unwrap())
            .is_none()
    );
    let x = |c: &str| {
        let o = doc
            .assembly()
            .occurrences
            .iter()
            .find(|o| o.component.to_string() == c)
            .unwrap()
            .uid;
        placed(&doc, o).translation[0]
    };
    // Centres 600 apart from the first (at 24), the last at 3000 - 24.
    assert!((x(&now[1]) - x(&now[0]) - 600.0).abs() < 1e-6);
    assert!(
        (x(&now[5]) - (3000.0 - 24.0)).abs() < 1e-6,
        "{}",
        x(&now[5])
    );
    // And back to 400: nine again.
    let back = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": now[0], "spacing": {"distance": 400}}),
    )
    .unwrap();
    assert_eq!(back["count"], 9);
    all_ok(&doc);
    // The whole set goes at once.
    let removed = command(
        &mut doc,
        json!({"cmd": "build_remove", "component": now[2], "set": true}),
    )
    .unwrap();
    assert_eq!(
        removed["removed"]["components"].as_array().unwrap().len(),
        9
    );
}

#[test]
fn a_set_between_two_beams_starts_at_its_typed_offset() {
    let mut doc = design();
    let (b1, b2) = two_beams(&mut doc);
    // The panel's Offset and From (mitcad#492): from end A the first
    // member's near face 200 from it, from end B the last one's far face.
    for (from, count, first_near, last_far) in [("a", 11, 200.0, 3000.0), ("b", 11, 0.0, 2800.0)] {
        let fields = json!({"part": part("rows98"),
            "at": {"occurrence": b1["occurrence"], "geometry": mount(&doc, &b1, "face_yn"),
                   "from": from, "offset": 200},
            "length": {"bind": {"occurrence": b2["occurrence"],
                                "geometry": mount(&doc, &b2, "face_yp")}},
            "spacing": {"mode": "centre", "distance": 300}});
        let mut probe = fields.clone();
        probe["query"] = json!("build_preview");
        let shown = query(&doc, probe);
        assert_eq!(shown["spacing"]["count"], count, "{from}: {shown}");
        let set = place(&mut doc, fields);
        assert_eq!(set["count"], count, "{from}: {set}");
        all_ok(&doc);
        let near = |c: &Value| {
            let o = doc
                .assembly()
                .occurrences
                .iter()
                .find(|o| o.component.to_string() == c.as_str().unwrap())
                .unwrap()
                .uid;
            let b = doc
                .part_bounds(c.as_str().unwrap().parse().unwrap())
                .unwrap();
            mitcad_model::internal::build::moved_box(&b, &placed(&doc, o))
        };
        let members = set["set"].as_array().unwrap();
        let (a, z) = (near(&members[0]), near(members.last().unwrap()));
        assert!((a.min[0] - first_near).abs() < 1e-6, "{from}: {a:?}");
        assert!((z.max[0] - last_far).abs() < 1e-6, "{from}: {z:?}");
        command(
            &mut doc,
            json!({"cmd": "build_remove", "component": members[0], "set": true}),
        )
        .unwrap();
    }
}

fn junctions(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "build_junctions"}))["junctions"]
        .as_array()
        .unwrap()
        .clone()
}

/// The junction where `a`'s end meets `b`.
fn junction_of(doc: &Document<MockKernel>, a: &Value, b: &Value) -> Value {
    junctions(doc)
        .into_iter()
        .find(|j| j["a"]["component"] == a["component"] && j["b"]["component"] == b["component"])
        .unwrap_or_else(|| panic!("no junction of {a} on {b}: {:?}", junctions(doc)))
}

fn post_at(doc: &mut Document<MockKernel>, x: f64, y: f64) -> Value {
    place(
        doc,
        json!({"part": part("post98"), "at": {"point": [x, y, 0]}, "length": 2400}),
    )
}

#[test]
fn a_small_frame_of_posts_beams_joists_hangers_a_half_lap_and_screws() {
    let mut doc = design();
    // Four posts 98 x 98, 2400 high.
    let p1 = post_at(&mut doc, 0.0, 0.0);
    let p2 = post_at(&mut doc, 3000.0, 0.0);
    let p3 = post_at(&mut doc, 0.0, 2000.0);
    let p4 = post_at(&mut doc, 3000.0, 2000.0);
    // Beams 48 x 148 on the posts' sides, flush with their tops, on edge
    // (their X, the 148, along the post), reaching the next post.
    let beam = |doc: &mut Document<MockKernel>, from: &Value, to: &Value| {
        let on = mount(doc, from, "face_xp");
        let target = mount(doc, to, "face_xn");
        place(
            doc,
            json!({"part": part("beam148"),
                   "at": {"occurrence": from["occurrence"], "geometry": on, "from": "b", "offset": 0},
                   "length": {"bind": {"occurrence": to["occurrence"], "geometry": target}}}),
        )
    };
    let b1 = beam(&mut doc, &p1, &p2);
    let b2 = beam(&mut doc, &p3, &p4);
    all_ok(&doc);
    let b1c = b1["component"].as_str().unwrap().to_owned();
    assert!((length_of(&doc, &b1c) - 2902.0).abs() < 1e-6);
    // Joists between the beams at 600, on edge, each in a hanger.
    let on = mount(&doc, &b1, "face_yn");
    let to = mount(&doc, &b2, "face_yp");
    let joists = place(
        &mut doc,
        json!({"part": {"source": "S2"},
               "at": {"occurrence": b1["occurrence"], "geometry": on},
               "length": {"bind": {"occurrence": b2["occurrence"], "geometry": to}},
               "spacing": {"distance": 600}}),
    );
    all_ok(&doc);
    assert_eq!(joists["count"], 6);
    let members: Vec<Value> = joists["set"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| json!({"component": c}))
        .collect();
    for m in &members {
        assert!((length_of(&doc, m["component"].as_str().unwrap()) - 1952.0).abs() < 1e-6);
    }
    // The junctions: each beam's end on a post (a corner at its top), each
    // joist's end on the first beam.
    let corner = junction_of(&doc, &b1, &p1);
    assert_eq!(corner["kind"], "L");
    assert_eq!(corner["b"]["at"], "face_xp");
    assert_eq!(corner["wood"]["through"], p1["component"]);
    assert_eq!(corner["wood"]["into"], b1["component"]);
    assert!((corner["wood"]["thickness"].as_f64().unwrap() - 98.0).abs() < 1e-6);
    let hung = junction_of(&doc, &members[2], &b1);
    assert_eq!(hung["kind"], "T");
    assert_eq!(hung["b"]["at"], "face_yn");
    // Hangers: fitted by the joist's section, two screws each.
    for m in &members {
        let j = junction_of(&doc, m, &b1);
        let hanger = command(
            &mut doc,
            json!({"cmd": "build_connect", "junction": j["id"], "part": part("hanger148")}),
        )
        .unwrap();
        assert_eq!(hanger["adds"].as_array().unwrap().len(), 2, "{hanger}");
        assert_eq!(hanger["warnings"], json!([]), "{hanger}");
    }
    all_ok(&doc);
    // A hanger for another section is refused.
    let post_corner = junction_of(&doc, &b2, &p3);
    let refused = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": corner["id"], "part": part("postbase98")}),
    )
    .unwrap_err();
    assert!(refused.contains("post base"), "{refused}");
    // Screws at the beams' corners, sized from the members: through the
    // 98 post, 6 mm (98 / 7 = 14 would allow more; 48 / 7 = 6.86), the
    // longest 6 mm is 120: short of 98 + 8 x 6, it embeds 22 mm, and the
    // upper screw is 37 mm from the post's top.
    let screws = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": corner["id"], "part": part("wood-screw")}),
    )
    .unwrap();
    assert_eq!(screws["adds"].as_array().unwrap().len(), 1);
    let warnings: Vec<String> = screws["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_owned())
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("embeds 22 mm")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w.contains("37 mm from the end of")),
        "{warnings:?}"
    );
    let other = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": post_corner["id"],
               "part": {"library": mitcad_model_tests_support::test_library::LIBRARY, "url": mitcad_model_tests_support::test_library::URL,
                        "rev": "v1.0.0", "component": "wood-screw", "config": "5x100"},
               "count": 3}),
    )
    .unwrap();
    assert_eq!(other["adds"].as_array().unwrap().len(), 2);
    all_ok(&doc);
    // Post bases under the posts.
    for p in [&p1, &p2, &p3, &p4] {
        let base = command(
            &mut doc,
            json!({"cmd": "build_connect", "end": {"component": p["component"], "end": "a"},
                   "part": part("postbase98")}),
        )
        .unwrap();
        let t = placed(&doc, occurrence(&base));
        let at = placed(&doc, occurrence(p)).translation;
        assert!(close(t.translation, at), "{:?}", t.translation);
        // Its mount's z into the post: up.
        assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]));
    }
    all_ok(&doc);
    // A half-lap corner: two plates lying on the posts' tops, overlapping
    // over the first post.
    let top = mount(&doc, &p1, "end_b");
    let plate_a = place(
        &mut doc,
        json!({"part": {"source": "S2"},
               "at": {"occurrence": p1["occurrence"], "geometry": top, "point": [-49, 0, 2400]},
               "to": {"point": [3049, 0, 2400]}}),
    );
    let top = mount(&doc, &p1, "end_b");
    let plate_b = place(
        &mut doc,
        json!({"part": {"source": "S2"},
               "at": {"occurrence": p1["occurrence"], "geometry": top, "point": [0, -49, 2400]},
               "to": {"point": [0, 2049, 2400]}}),
    );
    all_ok(&doc);
    let lap = junctions(&doc)
        .into_iter()
        .find(|j| {
            j["kind"] == "cross"
                && ((j["a"]["component"] == plate_b["component"]
                    && j["b"]["component"] == plate_a["component"])
                    || (j["a"]["component"] == plate_a["component"]
                        && j["b"]["component"] == plate_b["component"]))
        })
        .expect("the plates cross");
    let cut = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": lap["id"], "joint": {"kind": "half_lap"}}),
    )
    .unwrap();
    let cuts = cut["cuts"].as_array().unwrap();
    assert_eq!(cuts.len(), 2, "{cut}");
    for c in cuts {
        let size = c["size"].as_array().unwrap();
        let mut s: Vec<f64> = size.iter().map(|v| v.as_f64().unwrap()).collect();
        s.sort_by(f64::total_cmp);
        // Over the overlap (from the plates' ends at -49 to their far sides
        // at 74), half of 48 deep plus the overshoot.
        assert!((s[0] - 25.0).abs() < 1e-6, "{s:?}");
        assert!(
            (s[1] - 123.0).abs() < 1e-6 && (s[2] - 123.0).abs() < 1e-6,
            "{s:?}"
        );
    }
    all_ok(&doc);
    // The cut list: the timber columns, running metres, m3 and the stock
    // per size.
    let list = query(&doc, json!({"query": "build_cut_list"}));
    assert_eq!(list["timber"], true);
    let rows = list["rows"].as_array().unwrap();
    let posts = rows
        .iter()
        .find(|r| r["designation"] == "Sawn 100x100 L=2400")
        .unwrap();
    assert_eq!(posts["quantity"], 4);
    assert_eq!(posts["kind"], "timber");
    assert_eq!(posts["material"]["species"], "spruce");
    assert_eq!(posts["material"]["finish"], "sawn");
    assert!((posts["running_metres"].as_f64().unwrap() - 9.6).abs() < 1e-9);
    let volume = 0.098 * 0.098 * 2.4 * 4.0;
    assert!((posts["volume"].as_f64().unwrap() - volume).abs() < 1e-9);
    assert!((posts["mass"].as_f64().unwrap() - 450.0 * volume / 4.0).abs() < 1e-9);
    let joist_row = rows
        .iter()
        .find(|r| {
            r["length"]
                .as_f64()
                .is_some_and(|l| (l - 1952.0).abs() < 1e-6)
        })
        .unwrap();
    assert_eq!(joist_row["quantity"], 6);
    let laps: Vec<&Value> = rows
        .iter()
        .filter(|r| {
            r["machining"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m["kind"] == "half_lap")
        })
        .collect();
    assert_eq!(laps.len(), 2, "{rows:?}");
    let metres: f64 = 4.0 * 2.4 + 2.0 * 2.902 + 6.0 * 1.952 + 3.098 + 2.098;
    assert!(
        (list["totals"]["running_metres"].as_f64().unwrap() - metres).abs() < 1e-9,
        "{}",
        list["totals"]
    );
    let sum: f64 = rows.iter().map(|r| r["volume"].as_f64().unwrap()).sum();
    assert!((list["totals"]["volume"].as_f64().unwrap() - sum).abs() < 1e-12);
    // The posts: two to a 5100 stock length (2400 + 3 + 2400).
    let post_stock = list["stock"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source"] == "S1")
        .unwrap()
        .clone();
    assert_eq!(post_stock["needed"], 2);
    assert_eq!(post_stock["stock_lengths"], json!([5100.0, 5100.0]));
    // Removing a plate takes its half-lap out of the other.
    command(
        &mut doc,
        json!({"cmd": "build_remove", "component": plate_a["component"]}),
    )
    .unwrap();
    let left = parts_of(&doc)
        .into_iter()
        .find(|p| p["component"] == plate_b["component"])
        .unwrap();
    assert_eq!(left["cuts"], json!([]));
}

#[test]
fn rows_give_the_species_the_mass_and_the_less_common_mark_and_stock_per_row() {
    let mut doc = design();
    let spruce = place(
        &mut doc,
        json!({"part": part("rows98"), "at": {"point": [0, 0, 0]}, "length": 1000}),
    );
    let mut pine_part = part("rows98");
    pine_part["config"] = json!("pine");
    place(
        &mut doc,
        json!({"part": pine_part, "at": {"point": [500, 0, 0]}, "length": 1000, "grade": "C24"}),
    );
    let _ = spruce;
    let list = query(&doc, json!({"query": "build_cut_list"}));
    let rows = list["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    let pine = rows.iter().find(|r| r["config"] == "pine").unwrap();
    let spruce = rows.iter().find(|r| r["config"] != "pine").unwrap();
    assert_eq!(pine["availability"], "less common");
    assert_eq!(spruce["availability"], "standard");
    assert_eq!(pine["material"]["species"], "pine");
    assert_eq!(pine["material"]["grade"], "C24");
    assert_eq!(pine["section"], json!({"width": 98.0, "height": 48.0}));
    assert_eq!(pine["nominal"], json!({"width": 100.0, "height": 50.0}));
    let v = 0.098 * 0.048 * 1.0;
    assert!((pine["mass"].as_f64().unwrap() - 520.0 * v).abs() < 1e-9);
    assert!((spruce["mass"].as_f64().unwrap() - 450.0 * v).abs() < 1e-9);
    // Stock per row: one entry each.
    let stock = list["stock"].as_array().unwrap();
    assert_eq!(stock.len(), 2);
    assert!(stock.iter().any(|s| s["config"] == "pine"));
    assert!(stock.iter().all(|s| s["stock_lengths"] == json!([2700.0])));
}

#[test]
fn the_library_usage_counts_build_parts_per_row() {
    // The picker's use count is per size (mitcad#518): rows the design
    // does not use are not listed, a part placed without a row is the
    // default row's and is in the component's total only.
    let mut doc = design();
    let mut pine_part = part("rows98");
    pine_part["config"] = json!("pine");
    let mut spruce_part = part("rows98");
    spruce_part["config"] = json!("spruce");
    for i in 0..3 {
        place(
            &mut doc,
            json!({"part": spruce_part, "at": {"point": [0, 200 * i, 0]}, "length": 500}),
        );
    }
    place(
        &mut doc,
        json!({"part": part("rows98"), "at": {"point": [0, 800, 0]}, "length": 500}),
    );
    for i in 0..2 {
        place(
            &mut doc,
            json!({"part": pine_part, "at": {"point": [500, 200 * i, 0]}, "length": 500}),
        );
    }
    let usage = query(&doc, json!({"query": "library_usage"}));
    let parts = usage["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1, "{usage}");
    assert_eq!(parts[0]["component"], "rows98");
    assert_eq!(parts[0]["count"], 6);
    assert_eq!(
        parts[0]["rows"],
        json!([{"config": "spruce", "count": 3}, {"config": "pine", "count": 2}])
    );
}

#[test]
fn a_profile_only_cut_list_has_no_timber_columns() {
    let mut doc = design();
    mitcad_model_tests_support::test_library::first(&mut doc, "p30", [0.0, 0.0, 0.0], 500.0);
    let list = query(&doc, json!({"query": "build_cut_list"}));
    assert!(list.get("timber").is_none());
    assert!(list.get("totals").is_none());
    let row = &list["rows"][0];
    assert!(row.get("kind").is_none() && row.get("volume").is_none());
    assert!(list["stock"][0].get("config").is_none());
    assert!(list["stock"][0].get("stock_lengths").is_none());
}

#[test]
fn a_deck_board_row_with_a_gap_and_a_plywood_sheet_on_a_face() {
    let mut doc = design();
    // Three joists along Y on edge, 1000 apart: their x+ faces up (z 74).
    let joist = |doc: &mut Document<MockKernel>, x: f64| {
        member(doc, "beam148", [x, 0.0, 0.0], [x, 2000.0, 0.0], "edge")
    };
    let j1 = joist(&mut doc, 0.0);
    let _j2 = joist(&mut doc, 1000.0);
    let _j3 = joist(&mut doc, 2000.0);
    let top = mount(&doc, &j1, "face_xp");
    // Deck boards 28 x 120 lying flat across them, gap 5, over 1990:
    // 16 boards, the last ripped to 115.
    let fields = json!({"part": part("deck120"),
           "at": {"occurrence": j1["occurrence"], "geometry": top, "point": [-24, 0, 74]},
           "to": {"point": [2024, 0, 74]},
           "spacing": {"mode": "gap", "distance": 5, "along": [0, 1, 0], "length": 1990}});
    // Previewed first (mitcad#487): every board, the last one ripped.
    let mut shape_request = fields.clone();
    shape_request["shape"] = json!("build_preview");
    let shape = doc
        .analysis_shape(&shape_request.to_string())
        .unwrap()
        .expect("the row's bodies");
    let boards = preview_bodies(&doc, &shape);
    assert_eq!(boards.len(), 16, "{}", shape.history);
    let cuts = |b: &mitcad_model::testing::MockShape| b.history.matches("cut(").count();
    assert!(
        cuts(&boards[15]) == cuts(&boards[14]) + 1,
        "{} / {}",
        boards[14].history,
        boards[15].history
    );
    let deck = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(deck["count"], 16, "{deck}");
    let set: Vec<String> = deck["set"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    let at = |c: &str| {
        let o = doc
            .assembly()
            .occurrences
            .iter()
            .find(|o| o.component.to_string() == c)
            .unwrap()
            .uid;
        placed(&doc, o)
    };
    // Lying on the joists: its axis 14 above their top, its 120 across.
    let first = at(&set[0]);
    assert!(
        close(first.translation, [-24.0, 0.0, 88.0]),
        "{:?}",
        first.translation
    );
    assert!(close(first.apply_vector([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]));
    assert!((at(&set[1]).translation[1] - 125.0).abs() < 1e-6);
    let parts = parts_of(&doc);
    let last = parts.iter().find(|p| p["component"] == set[15]).unwrap();
    assert_eq!(last["cuts"][0]["kind"], "rip");
    assert!(
        last["cuts"][0]["size"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| { s.as_f64().is_some_and(|v| (v - 6.0).abs() < 1e-6) })
    );
    assert!(parts.iter().filter(|p| p["set"] == set[0]).count() == 16);
    // The gap widened instead: 15 boards, the last at the span's end.
    let spread = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": set[0], "spacing": {"fit": "spread"}}),
    )
    .unwrap();
    assert_eq!(spread["count"], 15);
    all_ok(&doc);
    let last_c = spread["set"][14].as_str().unwrap().to_owned();
    let o = doc
        .assembly()
        .occurrences
        .iter()
        .find(|o| o.component.to_string() == last_c)
        .unwrap()
        .uid;
    assert!((placed(&doc, o).translation[1] - 1870.0).abs() < 1e-6);
    let cuts_left = parts_of(&doc)
        .into_iter()
        .filter(|p| p["cuts"].as_array().is_some_and(|c| !c.is_empty()))
        .count();
    assert_eq!(cuts_left, 0);

    // A plywood sheet on a face: the three joists' tops, 2000 along the
    // first joist and 2048 across, its back on them.
    let mut doc = design();
    let j1 = joist(&mut doc, 0.0);
    joist(&mut doc, 1000.0);
    joist(&mut doc, 2000.0);
    let top = mount(&doc, &j1, "face_xp");
    let sheet = command(
        &mut doc,
        json!({"cmd": "build_panel", "part": part("plywood"),
               "at": {"occurrence": j1["occurrence"], "geometry": top}, "thickness": 18}),
    )
    .unwrap();
    assert_eq!(sheet["fits"], true, "{sheet}");
    all_ok(&doc);
    let t = placed(&doc, occurrence(&sheet));
    // Its middle (W/2, H/2) on the middle of the faces, its back at z 74.
    assert!(
        close(t.apply_point([1000.0, 1024.0, 0.0]), [1000.0, 1000.0, 74.0]),
        "{:?}",
        t.apply_point([1000.0, 1024.0, 0.0])
    );
    let panel = parts_of(&doc)
        .into_iter()
        .find(|p| p["component"] == sheet["component"])
        .unwrap();
    assert_eq!(panel["designation"], "Plywood 2000x2048");
}

/// The along-member coordinate of a member's first cut.
fn cut_z(doc: &Document<MockKernel>, member: &Value) -> f64 {
    let part = derived(doc, component(member));
    let Some(mitcad_model::features::FeatureDef::DerivedPart(d)) =
        doc.feature(part).map(|e| &e.def)
    else {
        panic!("no derived part");
    };
    doc.parameters().value(d.cuts[0].origin[2]).unwrap()
}

#[test]
fn a_housing_follows_the_joist_along_its_beam() {
    let mut doc = design();
    let beam = member(
        &mut doc,
        "beam148",
        [0.0, 0.0, 0.0],
        [3000.0, 0.0, 0.0],
        "edge",
    );
    let on = mount(&doc, &beam, "face_yn");
    let joist = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": beam["occurrence"], "geometry": on, "offset": 1000},
               "length": 1000}),
    );
    let j = junction_of(&doc, &joist, &beam);
    let housing = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": j["id"],
               "joint": {"kind": "housing", "depth": 10}}),
    )
    .unwrap();
    assert_eq!(housing["cuts"][0]["kind"], "housing");
    assert_eq!(housing["cuts"][0]["component"], beam["component"]);
    all_ok(&doc);
    let z = cut_z(&doc, &beam);
    // The box from its corner: the joist's near face (1000) or far (1048).
    assert!(
        (z - 1000.0).abs() < 1e-6 || (z - 1048.0).abs() < 1e-6,
        "{z}"
    );
    // The joist moves 500 along the beam: its housing goes with it.
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": joist["component"], "at": {"offset": 1500}}),
    )
    .unwrap();
    all_ok(&doc);
    assert!((cut_z(&doc, &beam) - (z + 500.0)).abs() < 1e-6);
    // Cut anew from where they are: still one housing, at the same place.
    let changed = command(
        &mut doc,
        json!({"cmd": "build_edit", "component": beam["component"], "cuts": "refit"}),
    )
    .unwrap();
    assert_eq!(changed["changed"], json!(["cuts"]));
    assert!((cut_z(&doc, &beam) - (z + 500.0)).abs() < 1e-6);
    let parts = parts_of(&doc);
    let b = parts
        .iter()
        .find(|p| p["component"] == beam["component"])
        .unwrap();
    assert_eq!(b["cuts"].as_array().unwrap().len(), 1);
    // A joint of another kind is refused.
    let refused = command(
        &mut doc,
        json!({"cmd": "build_connect", "junction": j["id"], "joint": {"kind": "dovetail"}}),
    )
    .unwrap_err();
    assert!(refused.contains("housing, notch or half_lap"), "{refused}");
}

#[test]
fn a_steel_tube_takes_no_joint() {
    // A profile without slots that is no timber (mitcad#873): its
    // junction takes no housing or half lap, and nothing is cut into it.
    let mut doc = design();
    let beam = place(
        &mut doc,
        json!({"part": part("tube40"), "at": {"point": [0, 0, 0]}, "length": 3000}),
    );
    let on = mount(&doc, &beam, "face_xp");
    let post = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": beam["occurrence"], "geometry": on, "offset": 1000},
               "length": 1000}),
    );
    all_ok(&doc);
    let j = junction_of(&doc, &post, &beam);
    for joint in [
        json!({"kind": "housing", "depth": 10}),
        json!({"kind": "half_lap"}),
    ] {
        let refused = command(
            &mut doc,
            json!({"cmd": "build_connect", "junction": j["id"], "joint": joint}),
        )
        .unwrap_err();
        assert!(refused.contains("is no timber member"), "{refused}");
    }
    for part in parts_of(&doc) {
        assert!(
            part["cuts"].as_array().is_none_or(|c| c.is_empty()),
            "{part}"
        );
    }
}

#[test]
fn a_member_shows_its_sources_wood_and_a_profile_its_own() {
    let mut doc = design();
    let post = place(
        &mut doc,
        json!({"part": part("post98"), "at": {"point": [0, 0, 0]}, "length": 2400}),
    );
    let body = mitcad_model::ids::BodyUid::new(derived(&doc, component(&post)), 0);
    assert_eq!(
        doc.state()
            .body_attributes
            .get(&body)
            .and_then(|a| a.appearance.clone()),
        Some("wood_oak".to_owned())
    );
    // The end grain of the source's caps on its ends (mitcad#484).
    let part = derived(&doc, component(&post));
    let ends: Vec<(String, String)> = doc.state().body_attributes[&body]
        .face_appearances
        .iter()
        .map(|(face, id)| (face.clone(), id.clone()))
        .collect();
    assert_eq!(
        ends,
        [
            (format!("{part}:end_a"), "wood_walnut".to_owned()),
            (format!("{part}:end_b"), "wood_walnut".to_owned())
        ]
    );
    let profile =
        mitcad_model_tests_support::test_library::first(&mut doc, "p30", [500.0, 0.0, 0.0], 500.0);
    let body = mitcad_model::ids::BodyUid::new(derived(&doc, component(&profile)), 0);
    assert!(!doc.state().body_attributes.contains_key(&body));
}

#[test]
fn a_member_from_a_point_towards_another_runs_its_length_or_to_what_it_meets() {
    let mut doc = design();
    // Two posts 98 x 98, 3000 apart along X.
    let _p1 = post_at(&mut doc, 0.0, 0.0);
    let p2 = post_at(&mut doc, 3000.0, 0.0);
    let towards = |extra: Value| {
        let mut fields = json!({"part": part("beam148"),
                                "at": {"geometry": "xy", "point": [500, 0, 0]},
                                "to": {"point": [1000, 0, 0]}});
        for (key, value) in extra.as_object().unwrap() {
            fields[key] = value.clone();
        }
        fields
    };
    // The preview without a length: the points' distance, and the face of
    // the post the direction meets as the auto length's target.
    let mut request = towards(json!({}));
    request["query"] = json!("build_preview");
    let answer = query(&doc, request);
    assert!(
        (answer["length"]["value"].as_f64().unwrap() - 500.0).abs() < 1e-9,
        "{answer}"
    );
    let target = answer["length"]["target"].clone();
    assert_eq!(target["occurrence"], p2["occurrence"], "{answer}");
    assert_eq!(target["geometry"], mount(&doc, &p2, "face_xn"), "{answer}");
    // A typed length runs that long along the direction.
    let typed = place(&mut doc, towards(json!({"length": 1200})));
    all_ok(&doc);
    assert!((length_of(&doc, typed["component"].as_str().unwrap()) - 1200.0).abs() < 1e-9);
    let t = placed(&doc, occurrence(&typed));
    assert!(close(t.apply_vector([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]));
    // An auto length binds to the post's face: to it, or through it.
    for (mode, expected) in [("face", 2451.0), ("through", 2549.0)] {
        let bound = place(
            &mut doc,
            towards(json!({"length": {"bind": target, "mode": mode, "add": 0}})),
        );
        all_ok(&doc);
        assert_eq!(bound["bound"].as_array().unwrap().len(), 1, "{bound}");
        let length = length_of(&doc, bound["component"].as_str().unwrap());
        assert!((length - expected).abs() < 1e-6, "{mode}: {length}");
    }
    // Grounded (no plane): the ray from the point along the direction.
    let mut grounded = towards(json!({"length": {"bind": target, "mode": "face"}}));
    grounded["at"] = json!({"point": [500, 0, 300]});
    grounded["to"] = json!({"point": [900, 0, 300]});
    let free = place(&mut doc, grounded);
    all_ok(&doc);
    assert!((length_of(&doc, free["component"].as_str().unwrap()) - 2451.0).abs() < 1e-6);
}

/// A planed 28 x 45 member of the timber library (as the parts library's
/// planed timber: the wood's look in its design, its species and density
/// only in its build block), and the test library's other parts.
fn planed_library_design() -> Document<MockKernel> {
    use mitcad_model_tests_support::test_library::{
        Part, TestLibrary, V1, board_build, board_design,
    };
    let mut library = TestLibrary::default();
    library.parts.insert(
        ("planed45".to_owned(), V1.to_owned()),
        Part {
            text: board_design(45.0, 28.0, &[]),
            build: Some(board_build(45.0, 28.0, "planed")),
            part_number: String::new(),
            name: "Planed 28x45".to_owned(),
            designation: "Planed 28x45 L={L}".to_owned(),
        },
    );
    let mut doc = Document::new(MockKernel::default());
    doc.set_link_resolver(Some(std::sync::Arc::new(library)));
    // The mock kernel's mass properties: of the bounding boxes.
    doc.kernel().physical.set(true);
    doc
}

/// mitcad#1817: a member placed from the library is made of its library's
/// wood, Spruce at the block's 450 kg/m³, which gives its mass (28 x 45 x
/// 1800 mm: 1.0206 kg, not steel's 17.8 kg); a design saved without it
/// gets it on open; a body without a material has no mass and is never
/// steel.
#[test]
fn a_member_is_made_of_its_librarys_wood_and_a_body_without_one_has_no_mass() {
    let mut doc = planed_library_design();
    let member = place(
        &mut doc,
        json!({"part": part("planed45"), "at": {"point": [0, 0, 0]}, "length": 1800}),
    );
    all_ok(&doc);
    let body = mitcad_model::ids::BodyUid::new(derived(&doc, component(&member)), 0);
    let attributes = doc.state().body_attributes[&body].clone();
    assert_eq!(attributes.material.as_deref(), Some("spruce"));
    assert_eq!(attributes.appearance.as_deref(), Some("wood_oak"));
    let spruce = doc.material("spruce").unwrap().clone();
    assert_eq!(spruce.name, "Spruce");
    assert!((spruce.density - 0.45).abs() < 1e-12, "{}", spruce.density);
    assert_eq!(
        spruce.class,
        Some(mitcad_model::material::MaterialClass::Wood)
    );
    // The wood's look is its material's: not set by hand.
    assert!(!doc.appearance_set_by_hand(body));

    // Physical Properties: Spruce and the mass its density gives.
    let answer = query(
        &doc,
        json!({"query": "properties", "bodies": [body.to_string()]}),
    );
    let entry = &answer["bodies"][0];
    assert_eq!(entry["material"], "spruce", "{answer}");
    assert_eq!(entry["material_name"], "Spruce", "{answer}");
    assert!((entry["density"].as_f64().unwrap() - 0.45).abs() < 1e-12);
    let mass = entry["mass"].as_f64().unwrap();
    assert!((mass - 1.0206).abs() < 1e-9, "{answer}");
    // The cut list says the same wood.
    let list = query(&doc, json!({"query": "build_cut_list"}));
    let row = &list["rows"][0];
    assert_eq!(row["material"]["species"], "spruce", "{list}");
    assert!(
        (row["mass"].as_f64().unwrap() - mass).abs() < 1e-9,
        "{list}"
    );

    // The bill of materials says Spruce, never Steel.
    let bom = query(
        &doc,
        json!({"query": "parts_list", "properties": ["Material"]}),
    );
    let member_row = bom["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["component"] == member["component"])
        .unwrap_or_else(|| panic!("no row of the member: {bom}"));
    assert_eq!(member_row["properties"]["Material"], "Spruce", "{bom}");

    // A second member finds the same material again.
    place(
        &mut doc,
        json!({"part": part("planed45"), "at": {"point": [200, 0, 0]}, "length": 900}),
    );
    let spruces = doc
        .custom_materials()
        .iter()
        .filter(|m| m.name == "Spruce")
        .count();
    assert_eq!(spruces, 1);

    // A design saved before the members had their material gets it on
    // open, from the library source it keeps.
    let mut file: serde_json::Value = serde_json::from_str(&doc.to_json()).unwrap();
    for entry in file["bodies"].as_array_mut().unwrap() {
        entry.as_object_mut().unwrap().remove("material");
    }
    file.as_object_mut().unwrap().remove("materials");
    let opened = Document::from_json(&file.to_string(), MockKernel::default()).unwrap();
    assert_eq!(
        opened.state().body_attributes[&body].material.as_deref(),
        Some("spruce")
    );
    assert!((opened.material("spruce").unwrap().density - 0.45).abs() < 1e-12);

    // A body without a material anywhere: no mass, no hidden steel, and
    // the total has none either.
    doc.activate_component(mitcad_model::ids::ComponentUid::ROOT)
        .unwrap();
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_rectangle(
            sketch,
            [500.0, 500.0],
            &mitcad_model::ValueInput::Number(20.0),
            &mitcad_model::ValueInput::Number(20.0),
        )
        .unwrap()
        .region;
    let block = doc
        .add_feature(
            &mitcad_model::test_support::extrude(sketch, &[&region], 10.0, "new_body", &[]),
            None,
        )
        .unwrap()
        .uid;
    let block = mitcad_model::ids::BodyUid::new(block, 0);
    let plain = query(
        &doc,
        json!({"query": "properties", "bodies": [block.to_string()]}),
    );
    let entry = &plain["bodies"][0];
    assert_eq!(entry["no_material"], true, "{plain}");
    assert_eq!(entry["material"], serde_json::Value::Null, "{plain}");
    assert!(entry.get("mass").is_none(), "{plain}");
    assert!(entry.get("density").is_none(), "{plain}");
    assert!(plain["total"].get("mass").is_none(), "{plain}");
    assert_eq!(plain["total"]["no_material"], 1, "{plain}");
    // The Center of Mass leaves it out and says so.
    let center = query(&doc, json!({"query": "center_of_mass"}));
    assert_eq!(center["without_material"], 1, "{center}");
}

/// mitcad#1817: a member's material follows its row (the pine row is
/// pine, 520 kg/m³), and a material set by hand stays.
#[test]
fn a_members_material_follows_its_row() {
    let mut doc = design();
    let member = place(
        &mut doc,
        json!({"part": part("rows98"), "at": {"point": [0, 0, 0]}, "length": 1000}),
    );
    let body = mitcad_model::ids::BodyUid::new(derived(&doc, component(&member)), 0);
    let material = |doc: &Document<MockKernel>| {
        doc.state().body_attributes[&body]
            .material
            .clone()
            .unwrap_or_default()
    };
    assert_eq!(material(&doc), "spruce");
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": component(&member), "config": "pine"}),
    )
    .unwrap();
    assert_eq!(material(&doc), "pine");
    assert!((doc.material("pine").unwrap().density - 0.52).abs() < 1e-12);
    // By hand: it stays when the row changes back.
    command(
        &mut doc,
        json!({"cmd": "set_body_material", "uid": body.to_string(), "material": "wood_oak"}),
    )
    .unwrap();
    command(
        &mut doc,
        json!({"cmd": "build_edit", "component": component(&member), "config": "spruce"}),
    )
    .unwrap();
    assert_eq!(material(&doc), "wood_oak");
}
