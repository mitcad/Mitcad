// SPDX-License-Identifier: MIT
//! Weld symbols proposed from BUILD's joints on a drawing's view
//! (mitcad#2350) with the mock kernel: a two-member frame, a profile's end
//! on another's side (a T), gets one fillet weld all around on an edge of
//! the joint, keyed by the joint; proposing again keeps it as edited and
//! adds nothing, a deleted one comes back with the weld asked for, and
//! the members asked for are checked.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::CurveGeometry;
use mitcad_model::ids::{BodyUid, ComponentUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_tests_support::test_library::*;

/// The corners of a 30 x 30 profile's section, round its axis.
const SECTION: [[f64; 2]; 4] = [[-15.0, -15.0], [15.0, -15.0], [15.0, 15.0], [-15.0, 15.0]];

/// The named edges of a placed part's body.
fn edge_names(doc: &Document<MockKernel>, part: ComponentUid) -> Vec<String> {
    let body: BodyUid = format!("{}.b0", derived(doc, part)).parse().unwrap();
    doc.kernel()
        .edges(doc.body_shape(body).expect("the part's body"))
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect()
}

/// Whether an edge of a derived part lies round its end A: between a side
/// and the end face, which the derived part names `end_a` when its end is
/// square (`derived_part.rs`, `end_faces`), else its source's `start`.
fn round_end_a(edge: &str) -> bool {
    edge.contains(":side") && (edge.contains(":end_a") || edge.contains(":start"))
}

/// A post 1000 long and a rail 300 long whose end A stands on the post's
/// side half way up (a T joint); the kernel lists the post's four long
/// edges and the rail's four edges round its end A (in the parts' own
/// coordinates, their axes along z); a drawing of the design with its
/// front view at 1:10. Returns the document, the post and the rail.
fn frame() -> (Document<MockKernel>, Value, Value) {
    let mut doc = design();
    let post = first(&mut doc, "p30", [0.0; 3], 1000.0);
    let pd = derived(&doc, component(&post));
    let rail = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"occurrence": post["occurrence"],
                      "geometry": {"mount": format!("{pd}:mount(slot_x+_1)")},
                      "offset": 500.0},
               "length": 300}),
    );
    all_ok(&doc);
    doc.kernel().lists_geometry.set(true);
    let long: Vec<String> = edge_names(&doc, component(&post))
        .into_iter()
        .filter(|name| name.matches(":side").count() == 2)
        .collect();
    let end_a: Vec<String> = edge_names(&doc, component(&rail))
        .into_iter()
        .filter(|name| round_end_a(name))
        .collect();
    assert_eq!(long.len(), 4, "{long:?}");
    assert_eq!(end_a.len(), 4, "{end_a:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        for (k, name) in long.iter().enumerate() {
            let [x, y] = SECTION[k];
            curves.insert(
                name.clone(),
                CurveGeometry::Line {
                    start: [x, y, 0.0],
                    end: [x, y, 1000.0],
                },
            );
        }
        for (k, name) in end_a.iter().enumerate() {
            let ([x0, y0], [x1, y1]) = (SECTION[k], SECTION[(k + 1) % 4]);
            curves.insert(
                name.clone(),
                CurveGeometry::Line {
                    start: [x0, y0, 0.0],
                    end: [x1, y1, 0.0],
                },
            );
        }
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"})).unwrap();
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [200, 150], "scale": "1:10"}),
    )
    .unwrap();
    (doc, post, rail)
}

fn welds(doc: &Document<MockKernel>) -> Vec<Value> {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["type"] == "weld")
        .cloned()
        .collect()
}

fn propose(doc: &mut Document<MockKernel>, extra: Value) -> Value {
    let mut asked = json!({"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1});
    for (key, value) in extra.as_object().unwrap() {
        asked[key] = value.clone();
    }
    command(doc, asked).unwrap_or_else(|e| panic!("propose_drawing_welds: {e}"))
}

#[test]
fn a_t_joint_gets_a_fillet_weld_all_around_on_its_edge() {
    let (mut doc, post, rail) = frame();
    let answer = propose(&mut doc, json!({}));
    assert_eq!(
        answer["annotations"].as_array().unwrap().len(),
        1,
        "{answer}"
    );
    assert_eq!(answer["skipped"], json!([]), "{answer}");
    let id = answer["annotations"][0].as_u64().unwrap();
    let undo = query(&doc, json!({"query": "document"}))["undo"].clone();
    assert_eq!(undo, format!("Propose weld symbol {id} in Drawing1"));
    let found = welds(&doc);
    assert_eq!(found.len(), 1);
    let weld = &found[0];
    assert_eq!(weld["weld"]["arrow_side"]["symbol"], "fillet");
    assert_eq!(
        weld["weld"]["all_around"], true,
        "the end's outline closes: {weld}"
    );
    assert_eq!(
        weld["weld"]["joint"],
        format!("{}:end_a|{}:slot_x+_1", component(&rail), component(&post))
    );
    // On an edge of the rail's end, where the rail's occurrence places it.
    let picked = weld["picks"][0]["edge"].as_str().unwrap();
    assert!(round_end_a(picked), "{picked}");
    assert_eq!(weld["picks"][0]["occurrence"], rail["occurrence"]);
    assert_eq!(weld["label"], "fillet (arrow side) / all around");
}

#[test]
fn a_member_picked_on_the_view_chooses_its_joints() {
    let (mut doc, post, rail) = frame();
    let body = format!("{}.b0", derived(&doc, component(&rail)));
    let edge = edge_names(&doc, component(&rail))
        .into_iter()
        .find(|name| round_end_a(name))
        .unwrap();
    let answer = propose(
        &mut doc,
        json!({"picks": [{"body": body, "occurrence": rail["occurrence"], "edge": edge}]}),
    );
    assert_eq!(
        answer["annotations"].as_array().unwrap().len(),
        1,
        "{answer}"
    );
    assert_eq!(
        welds(&doc)[0]["weld"]["joint"],
        format!("{}:end_a|{}:slot_x+_1", component(&rail), component(&post))
    );
    let error = command(
        &mut doc,
        json!({"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1, "picks": []}),
    )
    .unwrap_err();
    assert!(error.contains("pick the members"), "{error}");
}

#[test]
fn proposing_again_keeps_edited_welds_and_brings_back_deleted_ones() {
    let (mut doc, post, _) = frame();
    let id = propose(&mut doc, json!({}))["annotations"][0]
        .as_u64()
        .unwrap();
    // Edited: a 4 mm throat with a process number.
    let mut edited = welds(&doc)[0]["weld"].clone();
    edited["arrow_side"]["size"] = json!(4.0);
    edited["tail"] = json!("135");
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_annotation", "drawing": "D1", "annotation": id,
               "weld": edited}),
    )
    .unwrap();
    let again = propose(&mut doc, json!({}));
    assert_eq!(again["annotations"], json!([]), "{again}");
    assert_eq!(again["removed"], json!([]), "{again}");
    let found = welds(&doc);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["weld"]["arrow_side"]["size"], 4.0);
    assert_eq!(found[0]["weld"]["tail"], "135");
    // Deleted, then proposed again for the post's joints with the weld
    // asked for: all around still added where the joint closes.
    command(
        &mut doc,
        json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": id}),
    )
    .unwrap();
    assert!(welds(&doc).is_empty());
    let back = propose(
        &mut doc,
        json!({"members": [post["component"]],
               "weld": {"arrow_side": {"symbol": "fillet", "size": 3}, "tail": "141"}}),
    );
    assert_eq!(back["annotations"].as_array().unwrap().len(), 1, "{back}");
    let found = welds(&doc);
    assert_eq!(found[0]["weld"]["arrow_side"]["size"], 3.0);
    assert_eq!(found[0]["weld"]["tail"], "141");
    assert_eq!(found[0]["weld"]["all_around"], true);
    // No members is refused; a weld that cannot be too.
    let error = command(
        &mut doc,
        json!({"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1, "members": []}),
    )
    .unwrap_err();
    assert!(error.contains("members"), "{error}");
    let error = command(
        &mut doc,
        json!({"cmd": "propose_drawing_welds", "drawing": "D1", "view": 1,
               "weld": {"tail": "141"}}),
    )
    .unwrap_err();
    assert!(error.contains("arrow side"), "{error}");
}
