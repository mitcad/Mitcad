// SPDX-License-Identifier: MIT
//! Selected profile areas (mitcad#199): the area a list of profile
//! references selects (`extrude::selected_regions`, what preview and
//! evaluation extrude) for nested, overlapping, holed and touching shapes,
//! in any order of clicks and with references listed twice or under a key
//! that changed with the sketch; the `profile` query; features and
//! previews of such lists.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::features::ProfileRef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::internal::features::selected_regions;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const F1: FeatureUid = FeatureUid(1);

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// Sketch1 (F1) on XY with rectangles `[x, y, width, height]`.
fn sketch(rectangles: &[[f64; 4]]) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    command(&mut doc, json!({"cmd": "sketch.create"}));
    for [x, y, width, height] in rectangles {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [x, y],
                   "width": width, "height": height}),
        );
    }
    doc
}

/// Even-odd point in polygon.
fn inside(polygon: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    for (a, b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// The profile a click at `at` picks: the region of F1 whose area (its
/// outline less its holes) holds the point, as the view shows it.
fn region_at(doc: &Document<MockKernel>, at: [f64; 2]) -> ProfileRef {
    let output = doc.sketch_output(F1).expect("Sketch1 evaluated");
    let region = output
        .region_info
        .iter()
        .find(|r| inside(&r.outline, at) && !r.holes.iter().any(|h| inside(h, at)))
        .unwrap_or_else(|| panic!("no region at {at:?}"));
    ProfileRef {
        sketch: F1,
        region: region.profile.key.clone(),
    }
}

/// A click on a region as the application's selection inputs take it
/// (`toggleEntry`): a selected region goes out, another one comes in.
fn toggle(selection: &mut Vec<ProfileRef>, profile: &ProfileRef) {
    match selection.iter().position(|p| p == profile) {
        Some(i) => {
            selection.remove(i);
        }
        None => selection.push(profile.clone()),
    }
}

/// The selection after clicks on `clicks`, from nothing.
fn clicked(clicks: &[&ProfileRef]) -> Vec<ProfileRef> {
    let mut selection = Vec::new();
    for profile in clicks {
        toggle(&mut selection, profile);
    }
    selection
}

/// The area `profiles` select in Sketch1, as features extrude it: the
/// measured areas of the regions `selected_regions` gives.
fn selected_area(doc: &Document<MockKernel>, profiles: &[ProfileRef]) -> f64 {
    let output = doc.sketch_output(F1).expect("Sketch1 evaluated");
    let regions =
        selected_regions(output, profiles).unwrap_or_else(|p| panic!("no region for {}", p.region));
    regions
        .iter()
        .map(|r| {
            output
                .region_info
                .iter()
                .find(|info| info.profile.loops == r.loops)
                .expect("a region of the sketch")
                .area
        })
        .sum()
}

/// The keys of the regions `profiles` select, as a set.
fn selected_keys(doc: &Document<MockKernel>, profiles: &[ProfileRef]) -> BTreeSet<String> {
    let output = doc.sketch_output(F1).expect("Sketch1 evaluated");
    selected_regions(output, profiles)
        .unwrap_or_else(|p| panic!("no region for {}", p.region))
        .iter()
        .map(|r| r.key.to_string())
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-6 * expected.abs().max(1.0),
        "area {actual}, expected {expected}"
    );
}

/// Squares A (60), B (40) and C (20) nested about the origin: the regions
/// are the ring a between A and B (B its hole), the ring b between B and
/// C, and the square c. A click at a point toggles the region there.
fn nested() -> (Document<MockKernel>, ProfileRef, ProfileRef, ProfileRef) {
    let doc = sketch(&[
        [-30.0, -30.0, 60.0, 60.0],
        [-20.0, -20.0, 40.0, 40.0],
        [-10.0, -10.0, 20.0, 20.0],
    ]);
    let a = region_at(&doc, [25.0, 0.0]);
    let b = region_at(&doc, [15.0, 0.0]);
    let c = region_at(&doc, [0.0, 0.0]);
    (doc, a, b, c)
}

#[test]
fn nested_areas_compose_by_odd_even() {
    let (doc, a, b, c) = nested();
    let (area_a, area_b, area_c) = (3600.0, 1600.0, 400.0);
    // Each selectable area is a region with its holes: the ring a alone
    // keeps B open, it is not filled and B is not taken away twice.
    close(selected_area(&doc, &clicked(&[&a])), area_a - area_b);
    // Filled A is all three regions; A minus B (A xor B) the ring a; A
    // minus B plus C (A xor B xor C) the ring a and the square c.
    close(selected_area(&doc, &clicked(&[&a, &b, &c])), area_a);
    close(
        selected_area(&doc, &clicked(&[&a, &c])),
        area_a - area_b + area_c,
    );
    // A xor C, A minus C: the rings a and b.
    close(selected_area(&doc, &clicked(&[&a, &b])), area_a - area_c);
    // Clicking b off again leaves what a and c select, in any order.
    let toggled = clicked(&[&a, &b, &c, &b]);
    close(selected_area(&doc, &toggled), area_a - area_b + area_c);
    assert_eq!(
        selected_keys(&doc, &toggled),
        selected_keys(&doc, &clicked(&[&c, &a]))
    );
}

#[test]
fn the_order_of_clicks_does_not_matter() {
    let (doc, a, b, c) = nested();
    let orders: [[&ProfileRef; 3]; 6] = [
        [&a, &b, &c],
        [&a, &c, &b],
        [&b, &a, &c],
        [&b, &c, &a],
        [&c, &a, &b],
        [&c, &b, &a],
    ];
    let expected = selected_keys(&doc, &clicked(&orders[0]));
    for order in &orders {
        let selection = clicked(order);
        assert_eq!(selected_keys(&doc, &selection), expected, "{order:?}");
        close(selected_area(&doc, &selection), 3600.0);
        // A click on a selected region gives back the area before it.
        for profile in order {
            let mut again = selection.clone();
            toggle(&mut again, profile);
            toggle(&mut again, profile);
            assert_eq!(selected_keys(&doc, &again), expected, "{order:?}");
        }
    }
}

#[test]
fn partly_overlapping_areas_leave_out_the_overlap() {
    // P (0, 0) and Q (10, 10), 20 x 20 each: P's own part, Q's own part
    // and their overlap are three regions.
    let doc = sketch(&[[0.0, 0.0, 20.0, 20.0], [10.0, 10.0, 20.0, 20.0]]);
    let p = region_at(&doc, [5.0, 5.0]);
    let q = region_at(&doc, [25.0, 25.0]);
    let both = region_at(&doc, [15.0, 15.0]);
    assert_eq!(
        BTreeSet::from([&p.region, &q.region, &both.region]).len(),
        3
    );
    // The parts covered by exactly one of P and Q: P xor Q.
    close(
        selected_area(&doc, &clicked(&[&p, &q])),
        400.0 + 400.0 - 2.0 * 100.0,
    );
    // The overlap too: their union.
    close(selected_area(&doc, &clicked(&[&q, &both, &p])), 700.0);
    // Clicked off again, the overlap is left out again.
    close(
        selected_area(&doc, &clicked(&[&p, &both, &q, &both])),
        600.0,
    );
}

#[test]
fn a_holed_region_keeps_its_hole() {
    // A plate (0, 0, 100 x 100) with a square (30, 30, 40 x 40) in it.
    let doc = sketch(&[[0.0, 0.0, 100.0, 100.0], [30.0, 30.0, 40.0, 40.0]]);
    let plate = region_at(&doc, [10.0, 10.0]);
    let island = region_at(&doc, [50.0, 50.0]);
    let output = doc.sketch_output(F1).unwrap();
    let regions = selected_regions(output, std::slice::from_ref(&plate)).unwrap();
    assert_eq!(regions.len(), 1);
    assert_eq!(
        regions[0].loops.len(),
        2,
        "the square stays the plate's hole"
    );
    close(selected_area(&doc, &clicked(&[&plate])), 10_000.0 - 1600.0);
    close(selected_area(&doc, &clicked(&[&island])), 1600.0);
    // Both: the whole plate, the hole filled by the island once.
    close(selected_area(&doc, &clicked(&[&island, &plate])), 10_000.0);
}

#[test]
fn touching_regions_and_references_listed_twice_count_once() {
    // Two rectangles sharing an edge.
    let doc = sketch(&[[0.0, 0.0, 20.0, 10.0], [20.0, 0.0, 20.0, 10.0]]);
    let left = region_at(&doc, [5.0, 5.0]);
    let right = region_at(&doc, [35.0, 5.0]);
    close(selected_area(&doc, &[left.clone(), right.clone()]), 400.0);
    // A list given as a whole (a definition) is a set: a reference twice
    // selects its region once, and the order of the list does not matter.
    let listed = [left.clone(), right.clone(), left.clone()];
    close(selected_area(&doc, &listed), 400.0);
    assert_eq!(
        selected_regions(doc.sketch_output(F1).unwrap(), &listed)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        selected_keys(&doc, &listed),
        selected_keys(&doc, &[right.clone(), left.clone()])
    );
    close(selected_area(&doc, &[left.clone(), left]), 200.0);
}

#[test]
fn an_old_key_and_the_current_one_are_one_region() {
    // A 40 x 10 rectangle, then a line across it: its region key is gone,
    // and a reference with it finds both halves, the area it selected
    // (mitcad#407), the half with the most curves in common first.
    let mut doc = sketch(&[[0.0, 0.0, 40.0, 10.0]]);
    let old = region_at(&doc, [5.0, 5.0]);
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 0], "end": [20, 10]}),
    );
    let left = region_at(&doc, [5.0, 5.0]);
    let right = region_at(&doc, [35.0, 5.0]);
    assert!(old != left && old != right, "the key changed");
    // The `profile` query names the regions the old key means now.
    let means = query(
        &doc,
        json!({"query": "profile", "sketch": "F1", "region": old.region}),
    );
    assert_eq!(means["exact"], json!(false));
    let pieces = means["regions"].as_array().unwrap();
    assert_eq!(pieces.len(), 2, "{means}");
    assert_eq!(means["region"], pieces[0]);
    assert!(pieces.contains(&json!(left.region)) && pieces.contains(&json!(right.region)));
    close(selected_area(&doc, std::slice::from_ref(&old)), 400.0);
    // The old reference and a current key select each half once.
    for current in [&left, &right] {
        let listed = [old.clone(), current.clone()];
        assert_eq!(selected_keys(&doc, &listed).len(), 2);
        close(selected_area(&doc, &listed), 400.0);
    }
    // The current key means itself.
    let exact = query(
        &doc,
        json!({"query": "profile", "sketch": "F1", "region": right.region}),
    );
    assert_eq!(exact["region"], json!(right.region));
    assert_eq!(exact["regions"], json!([right.region]));
    assert_eq!(exact["exact"], json!(true));
    // A key that shares no curve with any region means none.
    assert_eq!(
        query(
            &doc,
            json!({"query": "profile", "sketch": "F1", "region": "r{c99}"})
        ),
        Value::Null
    );
}

/// The uids of the bodies at the marker.
fn bodies(doc: &Document<MockKernel>) -> Vec<String> {
    query(doc, json!({"query": "bodies"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["uid"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_feature_lists_a_region_once_and_previews_what_it_adds() {
    let (mut doc, a, _, c) = nested();
    // As selection plumbing could give it: the ring a twice.
    let profiles = json!([a, c, a]);
    let def = json!({"type": "extrude", "profiles": profiles,
                     "extent": {"type": "distance", "distance": 10}, "operation": "new_body"});
    let preview: Value = serde_json::from_str(
        &doc.preview(&json!({"cmd": "add_feature", "def": def}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview["status"], json!("ok"), "{preview}");
    doc.clear_preview();
    command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(doc.status(FeatureUid(2)), Some(&FeatureStatus::Ok));
    // The mock kernel makes a solid per region: the ring and the square.
    assert_eq!(bodies(&doc), ["F2.b0", "F2.b1"]);
    let previewed: Vec<&str> = preview["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["uid"].as_str().unwrap())
        .collect();
    assert_eq!(
        previewed,
        ["F2.b0", "F2.b1"],
        "the preview adds the same bodies"
    );
    // The definition keeps the set: each reference once, in the order first
    // listed.
    let feature = query(&doc, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(feature["def"]["profiles"], json!([a, c]));
    // Undo and redo keep it.
    doc.undo();
    assert!(bodies(&doc).is_empty());
    doc.redo();
    assert_eq!(bodies(&doc), ["F2.b0", "F2.b1"]);
    let feature = query(&doc, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(feature["def"]["profiles"], json!([a, c]));
}

/// The value of a parameter.
fn parameter(doc: &Document<MockKernel>, name: &str) -> f64 {
    query(doc, json!({"query": "parameters"}))
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == json!(name))
        .and_then(|p| p["value"].as_f64())
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

/// The timeline entry of a feature.
fn timeline_entry(doc: &Document<MockKernel>, uid: &str) -> Value {
    query(doc, json!({"query": "timeline"}))["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(uid))
        .cloned()
        .unwrap_or_else(|| panic!("no {uid} in the timeline"))
}

/// The keys of the regions a feature's profiles select now.
fn feature_profiles(doc: &Document<MockKernel>, uid: &str) -> Vec<ProfileRef> {
    let feature = query(doc, json!({"query": "feature", "uid": uid}));
    serde_json::from_value(feature["def"]["profiles"].clone()).unwrap()
}

#[test]
fn a_region_a_change_splits_selects_all_its_pieces() {
    // A window (mitcad#407): circles of 660 and 600, a vertical and a
    // horizontal bar 40 wide from -310 that end between the circles; the
    // bars' nine regions extruded. Lengthening the bars to 340 takes them
    // across the outer circle: the tips at their far ends split in two,
    // and the area the extrusion selected must stay selected.
    let mut doc = sketch(&[]);
    for diameter in [660, 600] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0],
                   "diameter": diameter}),
        );
    }
    for [x, y, width, height] in [[-20, -310, 40, 620], [-310, -20, 620, 40]] {
        command(
            &mut doc,
            json!({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [x, y],
                   "width": width, "height": height}),
        );
    }
    assert_eq!(doc.sketch_output(F1).unwrap().regions.len(), 14);
    let bars: Vec<ProfileRef> = [
        [0.0, 0.0],
        [0.0, 150.0],
        [0.0, -150.0],
        [150.0, 0.0],
        [-150.0, 0.0],
        [0.0, 305.0],
        [0.0, -305.0],
        [305.0, 0.0],
        [-305.0, 0.0],
    ]
    .into_iter()
    .map(|at| region_at(&doc, at))
    .collect();
    close(selected_area(&doc, &bars), 2.0 * 40.0 * 620.0 - 1600.0);
    let def = json!({"type": "extrude", "profiles": bars,
                     "extent": {"type": "distance", "distance": 10}, "operation": "new_body"});
    command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(timeline_entry(&doc, "F2")["status"], json!("ok"));
    let top_tip = region_at(&doc, [0.0, 305.0]);
    let right_tip = region_at(&doc, [305.0, 0.0]);

    // The vertical bar's height and the horizontal one's width.
    assert_eq!(parameter(&doc, "d4"), 620.0);
    assert_eq!(parameter(&doc, "d5"), 620.0);
    for name in ["d4", "d5"] {
        command(
            &mut doc,
            json!({"cmd": "set_parameter", "name": name, "value": 650}),
        );
    }
    let output = doc.sketch_output(F1).unwrap();
    // The ring in two, and the top and right tips each in a piece between
    // the circles and one outside.
    assert_eq!(output.regions.len(), 17);
    let still = |p: &ProfileRef| output.regions.iter().any(|r| r.key == p.region);
    assert!(!still(&top_tip) && !still(&right_tip));
    assert_eq!(bars.iter().filter(|p| still(p)).count(), 7);

    // The definition is unchanged; it selects the whole longer bars.
    let profiles = feature_profiles(&doc, "F2");
    assert_eq!(profiles, bars);
    close(selected_area(&doc, &profiles), 2.0 * 40.0 * 650.0 - 1600.0);
    let pieces = |at: [[f64; 2]; 2]| -> Vec<String> {
        at.into_iter()
            .map(|p| region_at(&doc, p).region.to_string())
            .collect()
    };
    for (old, at) in [
        (&top_tip, [[0.0, 320.0], [0.0, 335.0]]),
        (&right_tip, [[320.0, 0.0], [335.0, 0.0]]),
    ] {
        let means = query(
            &doc,
            json!({"query": "profile", "sketch": "F1", "region": old.region}),
        );
        assert_eq!(means["exact"], json!(false));
        let found: BTreeSet<String> = means["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(found, pieces(at).into_iter().collect(), "{means}");
    }
    // The mock kernel makes a body per region: the seven kept and the four
    // pieces.
    assert_eq!(bodies(&doc).len(), 11);

    // The extrusion warns in the timeline and the feature query.
    let entry = timeline_entry(&doc, "F2");
    assert_eq!(entry["status"], json!("warning"), "{entry}");
    let feature = query(&doc, json!({"query": "feature", "uid": "F2"}));
    let warnings: Vec<&str> = feature["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    for old in [&top_tip, &right_tip] {
        let prefix = format!("region {} found as r{{", old.region);
        let warning = warnings
            .iter()
            .find(|w| w.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no warning for {}: {warnings:?}", old.region));
        assert!(warning.contains("} + r{"), "{warning}");
        assert!(warning.ends_with("after a change in Sketch1"), "{warning}");
        // Each a warning notice of the timeline's entry (mitcad#1040).
        assert!(
            entry["notices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["level"] == "warning" && n["message"].as_str() == Some(*warning)),
            "{entry}"
        );
    }

    // Back to 620: the old keys are the regions' own again, no warning.
    for name in ["d4", "d5"] {
        command(
            &mut doc,
            json!({"cmd": "set_parameter", "name": name, "value": 620}),
        );
    }
    assert_eq!(timeline_entry(&doc, "F2")["status"], json!("ok"));
    assert_eq!(bodies(&doc).len(), 9);
}

#[test]
fn the_pieces_of_a_split_region_keep_its_holes() {
    // A 40 x 10 plate with a hole of diameter 4 at (10, 5), split by a line
    // at x = 30: the plate's old key finds both pieces, and the disc in the
    // hole stays out, as it was.
    let mut doc = sketch(&[[0.0, 0.0, 40.0, 10.0]]);
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [10, 5], "diameter": 4}),
    );
    let plate = region_at(&doc, [35.0, 5.0]);
    let hole = std::f64::consts::PI * 4.0;
    close(
        selected_area(&doc, std::slice::from_ref(&plate)),
        400.0 - hole,
    );
    command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [30, 0], "end": [30, 10]}),
    );
    let left = region_at(&doc, [5.0, 5.0]);
    let right = region_at(&doc, [35.0, 5.0]);
    let disc = region_at(&doc, [10.0, 5.0]);
    let keys = selected_keys(&doc, std::slice::from_ref(&plate));
    assert_eq!(keys.len(), 2, "{keys:?}");
    assert!(!keys.contains(&disc.region.to_string()));
    let means = query(
        &doc,
        json!({"query": "profile", "sketch": "F1", "region": plate.region}),
    );
    let found: BTreeSet<&str> = means["regions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k.as_str().unwrap())
        .collect();
    let expected: BTreeSet<String> = [&left, &right]
        .iter()
        .map(|p| p.region.to_string())
        .collect();
    assert_eq!(
        found,
        expected.iter().map(String::as_str).collect::<BTreeSet<_>>()
    );
    close(
        selected_area(&doc, std::slice::from_ref(&plate)),
        400.0 - hole,
    );
}

#[test]
fn a_region_found_by_the_best_match_warns() {
    // A 40 x 10 rectangle cut by a line at x = 20; the left half extruded.
    // Without the line the half's key names a curve that is gone: no
    // outline can be traced, and the region with the most curves in common
    // (the whole rectangle) is used, with a warning.
    let mut doc = sketch(&[[0.0, 0.0, 40.0, 10.0]]);
    let line = command(
        &mut doc,
        json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [20, 0], "end": [20, 10]}),
    );
    let left = region_at(&doc, [5.0, 5.0]);
    let def = json!({"type": "extrude", "profiles": [left],
                     "extent": {"type": "distance", "distance": 10}, "operation": "new_body"});
    command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    assert_eq!(timeline_entry(&doc, "F2")["status"], json!("ok"));
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [line["made"][0]]}),
    );
    let whole = region_at(&doc, [5.0, 5.0]);
    let means = query(
        &doc,
        json!({"query": "profile", "sketch": "F1", "region": left.region}),
    );
    assert_eq!(means["exact"], json!(false));
    assert_eq!(means["regions"], json!([whole.region]));
    let expected = format!(
        "region {} found as {} after a change in Sketch1",
        left.region, whole.region
    );
    let entry = timeline_entry(&doc, "F2");
    assert_eq!(entry["status"], json!("warning"), "{entry}");
    assert_eq!(entry["error"], Value::Null);
    assert_eq!(
        entry["notices"],
        json!([{"level": "warning", "message": expected, "target": "F2"}])
    );
    let feature = query(&doc, json!({"query": "feature", "uid": "F2"}));
    assert_eq!(feature["warnings"], json!([expected]));
    assert_eq!(bodies(&doc).len(), 1);
}

#[test]
fn a_region_with_nothing_like_it_fails_the_feature() {
    // A circle extruded, then replaced by another one: its region key
    // shares no curve with any region, and the extrusion says so instead
    // of extruding nothing.
    let mut doc = sketch(&[]);
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20}),
    );
    let disc = region_at(&doc, [0.0, 0.0]);
    let def = json!({"type": "extrude", "profiles": [disc],
                     "extent": {"type": "distance", "distance": 10}, "operation": "new_body"});
    command(&mut doc, json!({"cmd": "add_feature", "def": def}));
    command(
        &mut doc,
        json!({"cmd": "sketch.add_circle", "sketch": "F1", "center": [50, 0], "diameter": 20}),
    );
    let curve = disc.region.segments().next().unwrap().curve.to_string();
    command(
        &mut doc,
        json!({"cmd": "sketch.remove", "sketch": "F1", "items": [curve]}),
    );
    assert_eq!(doc.sketch_output(F1).unwrap().regions.len(), 1);
    let entry = timeline_entry(&doc, "F2");
    assert_eq!(entry["status"], json!("error"), "{entry}");
    assert_eq!(
        entry["error"],
        json!(format!("Sketch1 has no profile {}", disc.region))
    );
    assert!(bodies(&doc).is_empty());
}
