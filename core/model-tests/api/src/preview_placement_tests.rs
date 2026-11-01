// SPDX-License-Identifier: MIT
//! A preview's report for a design whose components are placed off the
//! origin, with a hidden body, a hidden occurrence and a linked component
//! (mitcad#266), with the mock kernel: what a display needs to draw each
//! body where the normal view draws it. The `instances` query lists what
//! the view shows; the preview's `instances` list only the occurrence/body
//! pairs the command makes, each with its placement into the design and
//! whether it is visible; every other body keeps the placement and the
//! visibility it has (the app's side: app/tests/preview_bodies.cpp).

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, ComponentUid, OccurrenceUid};
use mitcad_model::test_support::{extrude, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, InsertOptions};
use mitcad_model_api::DocumentApi;

fn shift(x: f64, y: f64, z: f64) -> Transform {
    Transform::translation([x, y, z])
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

/// A 10 x 10 block in the active component, `height` up (down when
/// negative).
fn block(doc: &mut Document<MockKernel>, height: f64) -> BodyUid {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let uid = doc
        .add_feature(&extrude(sketch, &[&region], height, "new_body", &[]), None)
        .unwrap()
        .uid;
    BodyUid::new(uid, 0)
}

/// The preview of an extrusion of a new sketch in the active component as
/// a new body (its uid in "new").
fn preview_new_body(doc: &mut Document<MockKernel>) -> Value {
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [20.0, 20.0], &num(10.0), &num(10.0))
        .unwrap()
        .region;
    let def = json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                     "extent": {"type": "distance", "distance": 5.0}, "operation": "new_body"});
    let text = json!({"cmd": "add_feature", "def": def}).to_string();
    let mut report: Value = serde_json::from_str(&doc.preview(&text).unwrap()).unwrap();
    assert_eq!(report["status"], "ok", "{report}");
    let made: Vec<Value> = report["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|b| b["changed"] == true)
        .map(|b| b["uid"].clone())
        .collect();
    assert_eq!(made.len(), 1, "{report}");
    report["new"] = made[0].clone();
    report
}

/// A 4x4 placement's translation.
fn translation(matrix: &Value) -> [f64; 3] {
    let at = |row: usize| matrix[row][3].as_f64().unwrap();
    [at(0), at(1), at(2)]
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
}

/// The report's pairs as (body, occurrence, translation, visible).
fn pairs(report: &Value) -> Vec<(String, String, [f64; 3], bool)> {
    report["instances"]
        .as_array()
        .map(|pairs| {
            pairs
                .iter()
                .map(|p| {
                    (
                        p["body"].as_str().unwrap().to_owned(),
                        p["occurrence"].as_str().unwrap().to_owned(),
                        translation(&p["transform"]),
                        p["visible"].as_bool().unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn changed(report: &Value, body: BodyUid) -> Option<bool> {
    report["bodies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["uid"] == body.to_string())
        .map(|b| b["changed"].as_bool().unwrap())
}

/// The visible instances the view shows, as (body, path, translation).
fn shown(doc: &Document<MockKernel>) -> Vec<(String, String, [f64; 3])> {
    query(doc, json!({"query": "instances"}))
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            let path: Vec<&str> = i["path"]
                .as_array()
                .unwrap()
                .iter()
                .map(|uid| uid.as_str().unwrap())
                .collect();
            (
                i["body"].as_str().unwrap().to_owned(),
                path.join("/"),
                translation(&i["transform"]),
            )
        })
        .collect()
}

/// Board placed twice off the origin (60, 0, 0 and 60, 30, 0) with a shown
/// and a hidden body; Cover in a hidden occurrence (0, 0, 40); a root block.
struct Design {
    doc: Document<MockKernel>,
    board: ComponentUid,
    cover: ComponentUid,
    o1: OccurrenceUid,
    o5: OccurrenceUid,
    o2: OccurrenceUid,
    root_block: BodyUid,
    board_block: BodyUid,
    hidden: BodyUid,
    cover_block: BodyUid,
}

fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    let root_block = block(&mut doc, 4.0);
    let (board, o1) = doc
        .create_component(Some("Board"), shift(60.0, 0.0, 0.0), true)
        .unwrap();
    let board_block = block(&mut doc, 2.0);
    let hidden = block(&mut doc, 3.0);
    doc.set_body_visible(hidden, false).unwrap();
    doc.activate_component(ComponentUid::ROOT).unwrap();
    let o5 = doc
        .copy_occurrence(o1, Some(shift(60.0, 30.0, 0.0)))
        .unwrap();
    let (cover, o2) = doc
        .create_component(Some("Cover"), shift(0.0, 0.0, 40.0), true)
        .unwrap();
    let cover_block = block(&mut doc, 1.0);
    doc.activate_component(ComponentUid::ROOT).unwrap();
    doc.set_occurrence_visible(o2, false).unwrap();
    Design {
        doc,
        board,
        cover,
        o1,
        o5,
        o2,
        root_block,
        board_block,
        hidden,
        cover_block,
    }
}

#[test]
fn the_view_shows_placed_bodies_and_leaves_hidden_ones_out() {
    let d = design();
    let shown = shown(&d.doc);
    let board = d.board_block.to_string();
    assert!(
        shown
            .iter()
            .any(|(b, p, t)| *b == board && *p == d.o1.to_string() && near(*t, [60.0, 0.0, 0.0]))
    );
    assert!(
        shown
            .iter()
            .any(|(b, p, t)| *b == board && *p == d.o5.to_string() && near(*t, [60.0, 30.0, 0.0]))
    );
    assert!(!shown.iter().any(|(b, _, _)| *b == d.hidden.to_string()));
    assert!(
        !shown
            .iter()
            .any(|(b, _, _)| *b == d.cover_block.to_string())
    );
    assert_eq!(shown.len(), 3, "{shown:?}");
}

#[test]
fn a_root_extrusion_makes_only_its_body_in_the_root() {
    let mut d = design();
    let report = preview_new_body(&mut d.doc);
    let made = report["new"].as_str().unwrap().to_owned();
    // Only the new body is a new pair, in the root, at the root's placement.
    assert_eq!(
        pairs(&report),
        [(made, String::new(), [0.0, 0.0, 0.0], true)],
        "{report}"
    );
    assert!(report.get("removed_instances").is_none(), "{report}");
    // The others are unchanged: a display keeps them where it shows them,
    // and the hidden ones hidden.
    for body in [d.root_block, d.board_block, d.hidden, d.cover_block] {
        assert_eq!(changed(&report, body), Some(false), "{body}: {report}");
    }
}

#[test]
fn a_new_body_in_a_placed_component_is_where_its_occurrences_put_it() {
    let mut d = design();
    d.doc.activate_component(d.board).unwrap();
    let report = preview_new_body(&mut d.doc);
    let made = report["new"].as_str().unwrap().to_owned();
    // Board's occurrences in the order of the occurrence tree.
    let pairs = pairs(&report);
    let expected = [
        (made.clone(), d.o1.to_string(), [60.0, 0.0, 0.0], true),
        (made, d.o5.to_string(), [60.0, 30.0, 0.0], true),
    ];
    assert_eq!(pairs.len(), 2, "{report}");
    for (pair, expected) in pairs.iter().zip(&expected) {
        assert_eq!(
            (&pair.0, &pair.1, pair.3),
            (&expected.0, &expected.1, expected.3)
        );
        assert!(near(pair.2, expected.2), "{pair:?}");
    }
    // The hidden body of the same component: unchanged, no new pair.
    assert_eq!(changed(&report, d.hidden), Some(false));
}

#[test]
fn a_new_body_in_a_hidden_occurrence_is_reported_hidden() {
    let mut d = design();
    d.doc.activate_component(d.cover).unwrap();
    let report = preview_new_body(&mut d.doc);
    let made = report["new"].as_str().unwrap().to_owned();
    let pairs = pairs(&report);
    assert_eq!(pairs.len(), 1, "{report}");
    let (body, occurrence, at, visible) = &pairs[0];
    assert_eq!((body, occurrence), (&made, &d.o2.to_string()));
    assert!(near(*at, [0.0, 0.0, 40.0]), "{at:?}");
    assert!(!visible, "{report}");
}

#[test]
fn a_linked_components_bodies_keep_their_occurrences_in_a_preview() {
    let dir = mitcad_model::testing::unique_temp_dir("preview-placement");
    let mut part = Document::new(MockKernel::default());
    block(&mut part, 6.0);
    std::fs::write(dir.join("part.mitcad"), part.to_json()).unwrap();

    let mut d = design();
    let options = InsertOptions {
        transform: shift(0.0, 80.0, 0.0),
        base: Some(dir.clone()),
        ..InsertOptions::default()
    };
    let (linked, o3) = d.doc.insert_component("part.mitcad", &options).unwrap();
    assert!(d.doc.assembly().component(linked).unwrap().link.is_some());
    let o4 = d
        .doc
        .copy_occurrence(o3, Some(shift(0.0, -80.0, 0.0)))
        .unwrap();
    d.doc.set_occurrence_visible(o4, false).unwrap();
    let bodies = d.doc.component_bodies(linked);
    assert_eq!(bodies.len(), 1);
    let linked_body = bodies[0].to_string();
    // The view shows the linked body where Part:1 puts it, not Part:2's.
    let shown = shown(&d.doc);
    assert!(
        shown.iter().any(|(b, p, t)| *b == linked_body
            && *p == o3.to_string()
            && near(*t, [0.0, 80.0, 0.0]))
    );
    assert!(!shown.iter().any(|(_, p, _)| *p == o4.to_string()));

    // Extrude in the root: the linked body is unchanged and no new pair, so
    // a display keeps it in Part:1 and leaves Part:2's hidden.
    let report = preview_new_body(&mut d.doc);
    let made = report["new"].as_str().unwrap().to_owned();
    assert_eq!(changed(&report, bodies[0]), Some(false), "{report}");
    assert_eq!(
        pairs(&report),
        [(made, String::new(), [0.0, 0.0, 0.0], true)],
        "{report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The report's tool places as (occurrence, translation, visible).
fn tool_places(report: &Value) -> Vec<(String, [f64; 3], bool)> {
    report["tool_instances"]
        .as_array()
        .map(|places| {
            places
                .iter()
                .map(|p| {
                    (
                        p["occurrence"].as_str().unwrap().to_owned(),
                        translation(&p["transform"]),
                        p["visible"].as_bool().unwrap(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn the_tool_is_where_its_components_occurrences_put_it() {
    // mitcad#636: the tool is in its feature's component's coordinates; a
    // display draws it (and a provisional proxy of it) at each placement.
    let mut d = design();
    let report = preview_new_body(&mut d.doc);
    assert_eq!(report["tool"], true, "{report}");
    let places = tool_places(&report);
    assert_eq!(places.len(), 1, "{report}");
    assert_eq!((places[0].0.as_str(), places[0].2), ("", true));
    assert!(near(places[0].1, [0.0, 0.0, 0.0]));

    d.doc.activate_component(d.board).unwrap();
    let report = preview_new_body(&mut d.doc);
    let places = tool_places(&report);
    assert_eq!(places.len(), 2, "{report}");
    assert_eq!((places[0].0.clone(), places[0].2), (d.o1.to_string(), true));
    assert!(near(places[0].1, [60.0, 0.0, 0.0]), "{places:?}");
    assert_eq!((places[1].0.clone(), places[1].2), (d.o5.to_string(), true));
    assert!(near(places[1].1, [60.0, 30.0, 0.0]), "{places:?}");

    d.doc.activate_component(d.cover).unwrap();
    let report = preview_new_body(&mut d.doc);
    let places = tool_places(&report);
    assert_eq!(places.len(), 1, "{report}");
    assert_eq!(
        (places[0].0.clone(), places[0].2),
        (d.o2.to_string(), false)
    );
    assert!(near(places[0].1, [0.0, 0.0, 40.0]), "{places:?}");
}
