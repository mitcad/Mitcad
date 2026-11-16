// SPDX-License-Identifier: MIT
//! Side snaps in line with the frame's other profiles (mitcad#1886), with
//! the mock kernel: a 30 profile standing at the origin (the side) and a
//! 40 member crossing its line behind it, square to it. A click on the
//! side near the member's line puts the new part's centre plane on the
//! member's (`inline`), a click near one of its faces puts the part's face
//! flush with it (`flush_face`); the preview says which member, and the
//! part is bound to the nearer end as between snaps. Where a line-up
//! falls on the side's middle, the middle snap stays.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// A 30 profile 1000 long standing at the origin (the side), and its slot
/// `slot_y+_1` as `at.geometry`.
fn target(doc: &mut Document<MockKernel>) -> (Value, Value) {
    let t = first(doc, "p30", [0.0; 3], 1000.0);
    let tp = derived(doc, component(&t));
    (t, json!({"mount": format!("{tp}:mount(slot_y+_1)")}))
}

/// A member 600 long along X, grounded with its end A at `at`: square to
/// the side; its section starts at `at` and spans its width along the side.
fn crossing(doc: &mut Document<MockKernel>, profile: &str, at: [f64; 3]) -> Value {
    place(
        doc,
        json!({"part": part(profile), "at": {"point": at, "direction": [1, 0, 0]}, "length": 600}),
    )
}

fn side_at(t: &Value, slot: &Value, at: Value) -> Value {
    let mut fields = json!({"occurrence": t["occurrence"], "geometry": slot});
    for (key, value) in at.as_object().unwrap() {
        fields[key] = value.clone();
    }
    fields
}

/// What `build_preview` says of a 30 profile 200 long on the side.
fn preview(doc: &Document<MockKernel>, t: &Value, slot: &Value, at: Value) -> Value {
    query(
        doc,
        json!({"query": "build_preview", "part": {"source": "S1"}, "length": 200,
               "at": side_at(t, slot, at)}),
    )
}

/// A click whose part would be centred at `z` along the side.
fn click(z: f64) -> Value {
    json!({"point": [0.0, 15.0, z]})
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The snaps of a kind: their offsets (the near face from end A).
fn snaps_of(shown: &Value, kind: &str) -> Vec<f64> {
    shown["snaps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == kind)
        .map(|s| s["offset"].as_f64().unwrap())
        .collect()
}

#[test]
fn a_side_click_lines_up_with_a_member_crossing_the_sides_line() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // A 40 member crossing behind the side at 437: from 437 to 477 along
    // it, its centre at 457. No grid point (every 30) and no other snap is
    // at 437, 442, 447.
    let m = crossing(&mut doc, "p40", [-300.0, -100.0, 437.0]);
    all_ok(&doc);

    // The preview's snaps: in line (the near face at 457 - 15) and flush
    // with either face (437, and 477 - 30), each with the member.
    let shown = preview(&doc, &t, &slot, click(300.0));
    let inline = snaps_of(&shown, "inline");
    let flush = snaps_of(&shown, "flush_face");
    assert_eq!(inline.len(), 1, "{}", shown["snaps"]);
    assert!(near(inline[0], 442.0), "{inline:?}");
    assert_eq!(flush.len(), 2, "{}", shown["snaps"]);
    assert!(near(flush[0], 437.0) && near(flush[1], 447.0), "{flush:?}");
    for s in shown["snaps"].as_array().unwrap() {
        if s["kind"] == "inline" || s["kind"] == "flush_face" {
            assert_eq!(s["with"]["component"], m["component"], "{s}");
            assert!(
                s["with"]["name"].as_str().is_some_and(|n| !n.is_empty()),
                "{s}"
            );
        } else {
            assert!(s.get("with").is_none(), "{s}");
        }
    }
    // A click far from them takes none of them.
    let side = &shown["side"];
    assert_ne!(side["kind"], "inline", "{side}");
    assert_ne!(side["kind"], "flush_face", "{side}");
    assert!(side["with"].is_null(), "{side}");

    // A click 1 past the member's line: the part's centre plane on the
    // member's, from the nearer end (A), offset 442; the result says
    // which member.
    let shown = preview(&doc, &t, &slot, click(458.0));
    let side = &shown["side"];
    assert_eq!(side["kind"], "inline", "{side}");
    assert_eq!(side["with"]["component"], m["component"], "{side}");
    assert_eq!(side["from"], "a", "{side}");
    assert!(near(side["offset"].as_f64().unwrap(), 442.0), "{side}");

    // A click near the member's face towards end A: the part's face
    // towards end A flush with it.
    let shown = preview(&doc, &t, &slot, click(451.0));
    let side = &shown["side"];
    assert_eq!(side["kind"], "flush_face", "{side}");
    assert_eq!(side["with"]["component"], m["component"], "{side}");
    assert!(near(side["offset"].as_f64().unwrap(), 437.0), "{side}");
    // And near its face towards end B: the part's face towards end B
    // flush with it.
    let shown = preview(&doc, &t, &slot, click(463.0));
    let side = &shown["side"];
    assert_eq!(side["kind"], "flush_face", "{side}");
    assert!(near(side["offset"].as_f64().unwrap(), 447.0), "{side}");

    // A typed offset overrides it.
    let mut typed = click(458.0);
    typed["offset"] = json!(100);
    typed["from"] = json!("a");
    let shown = preview(&doc, &t, &slot, typed);
    let side = &shown["side"];
    assert!(side["kind"].is_null(), "{side}");
    assert!(near(side["offset"].as_f64().unwrap(), 100.0), "{side}");

    // Placed by the click in line: its centre at the member's (457).
    let p = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "length": 200, "at": side_at(&t, &slot, click(458.0))}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&p)).translation[2];
    assert!(near(at, 457.0), "{at} instead of 457");
}

#[test]
fn the_middle_snap_stays_where_a_line_up_falls_on_it() {
    let mut doc = design();
    let (t, slot) = target(&mut doc);
    // A 30 member crossing behind the side at its middle (from 485 to
    // 515, its centre at 500): in line and flush there are the middle
    // snap's place (485).
    crossing(&mut doc, "p30", [-300.0, -100.0, 485.0]);
    all_ok(&doc);
    let shown = preview(&doc, &t, &slot, click(501.0));
    assert!(snaps_of(&shown, "inline").is_empty(), "{}", shown["snaps"]);
    assert!(
        snaps_of(&shown, "flush_face").is_empty(),
        "{}",
        shown["snaps"]
    );
    let middle = snaps_of(&shown, "middle");
    assert!(middle.len() == 1 && near(middle[0], 485.0), "{middle:?}");
    assert_eq!(shown["side"]["kind"], "middle", "{}", shown["side"]);
}
