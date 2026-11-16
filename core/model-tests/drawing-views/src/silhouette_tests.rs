// SPDX-License-Identifier: MIT
//! Dimensions to a curved face's outline and its extreme points in a
//! projected view, and a section's or a detail's label dragged
//! (mitcad#1909, GitHub #363), on the mock kernel:
//!
//! - a pipe bend (a quarter torus) seen along its axis: its outer and
//!   inner outlines are the silhouettes of its torus face, no edges; a
//!   click on the outer one picks it whole (a radius of 70), a click at
//!   its top its extreme point, and a vertical dimension from the bend's
//!   straight bottom edge to that point reads the overall height, 70; the
//!   model made larger, it follows (80); the silhouette gone, it is broken
//!   and says so in words, without names;
//! - a cylinder lying along x seen from the side: the second pick of a
//!   dimension (with the first's point, `from`) finds the top of its
//!   outline, and a vertical dimension from the bottom edge reads 30, then
//!   32 when the cylinder grows;
//! - a section's and a detail's label dragged: the offset kept, following
//!   the view when it moves, put back by an offset of zero.

use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2};

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::{Curve3, Kernel};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    let answer = doc
        .query(&query.to_string())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::from_str(&answer).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn point(value: &Value) -> [f64; 2] {
    [value[0].as_f64().unwrap(), value[1].as_f64().unwrap()]
}

fn sheet(doc: &Document<MockKernel>) -> Value {
    query(doc, json!({"query": "drawing", "drawing": "D1"}))["sheets"][0].clone()
}

fn view(doc: &Document<MockKernel>, id: u64) -> Value {
    sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == id)
        .cloned()
        .unwrap_or_else(|| panic!("no view {id}"))
}

fn annotation(doc: &Document<MockKernel>, id: &Value) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == *id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

fn line(start: [f64; 3], end: [f64; 3]) -> CurveGeometry {
    CurveGeometry::Line { start, end }
}

/// A 40 x 30 x 20 block (F2.b0) whose top face (returned) stands for a
/// curved face and whose edges' curves `place` gives: each edge of the top
/// face with its index among them, the others with theirs among the
/// others (an edge `place` leaves out lies far away). Its geometry is
/// listed, and a drawing has its front view at 1:1 (the model's x and z
/// at the view's place plus (x, z) on the sheet).
fn block(place: impl Fn(bool, usize) -> Option<CurveGeometry>) -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": extrude(20.0)},
        ]),
    );
    doc.kernel().lists_geometry.set(true);
    let face = format!("F2:end({R})");
    let body: BodyUid = "F2.b0".parse().unwrap();
    let names: Vec<String> = doc
        .kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect();
    let (own, others): (Vec<&String>, Vec<&String>) = names.iter().partition(|n| n.contains(&face));
    assert!(own.len() >= 2 && !others.is_empty(), "{names:?}");
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        let mut far = 0.0;
        for (top, list) in [(true, &own), (false, &others)] {
            for (k, name) in list.iter().enumerate() {
                let curve = place(top, k).unwrap_or_else(|| {
                    far += 5.0;
                    line([300.0 + far, 0.0, 300.0], [301.0 + far, 0.0, 300.0])
                });
                curves.insert((*name).clone(), curve);
            }
        }
    }
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [200, 150], "scale": "1:1"}),
    );
    (doc, face)
}

fn extrude(distance: f64) -> Value {
    json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
           "extent": {"type": "distance", "distance": distance}, "operation": "new_body"})
}

/// The block made `distance` high (its names kept, its shape a new
/// version) and recomputed: what the kernel gives of it now is what the
/// drawing reads.
fn remade(doc: &mut Document<MockKernel>, distance: f64) {
    command(
        doc,
        json!({"cmd": "edit_feature", "uid": "F2", "def": extrude(distance)}),
    );
    command(doc, json!({"cmd": "recompute"}));
}

/// Where a point of the front view (the model's x and z) is on the sheet.
fn on_front(doc: &Document<MockKernel>, at: [f64; 2]) -> [f64; 2] {
    let place = point(&view(doc, 1)["place"]);
    [place[0] + at[0], place[1] + at[1]]
}

fn pick_at(doc: &Document<MockKernel>, at: [f64; 2], from: Option<[f64; 2]>) -> Value {
    let mut asked = json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at});
    if let Some(from) = from {
        asked["from"] = json!(from);
    }
    query(doc, asked)
}

/// A quarter torus about the y axis (major `major`, minor 20) in the front
/// view: its outer outline a quarter circle of `major + 20` about the
/// origin as a conic, its inner one of `major - 20` as a rational
/// B-spline (as the kernel's outline curves come).
fn bend(doc: &Document<MockKernel>, face: &str, major: f64) {
    let (outer, inner) = (major + 20.0, major - 20.0);
    doc.kernel().surfaces.borrow_mut().insert(
        face.to_owned(),
        SurfaceGeometry::Torus {
            center: [0.0, 0.0, 0.0],
            axis: [0.0, 1.0, 0.0],
            major_radius: major,
            minor_radius: 20.0,
        },
    );
    doc.kernel().silhouettes.borrow_mut().insert(
        face.to_owned(),
        vec![
            Curve3::Conic {
                center: [0.0, 0.0, 0.0],
                normal: [0.0, -1.0, 0.0],
                x_axis: [1.0, 0.0, 0.0],
                major: outer,
                minor: outer,
                start: 0.0,
                end: FRAC_PI_2,
                closed: false,
            },
            Curve3::BSpline {
                degree: 2,
                poles: vec![[inner, 0.0, 0.0], [inner, 0.0, inner], [0.0, 0.0, inner]],
                weights: vec![1.0, FRAC_1_SQRT_2, 1.0],
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            },
        ],
    );
    // The bend's straight bottom edge, from the inner outline to the outer.
    let bottom = edge_names(doc)
        .into_iter()
        .find(|n| !n.contains(face))
        .unwrap();
    doc.kernel()
        .edge_curves
        .borrow_mut()
        .insert(bottom, line([inner, 0.0, 0.0], [outer, 0.0, 0.0]));
}

fn edge_names(doc: &Document<MockKernel>) -> Vec<String> {
    let body: BodyUid = "F2.b0".parse().unwrap();
    doc.kernel()
        .edges(doc.body_shape(body).unwrap())
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .collect()
}

#[test]
fn a_bends_outline_seen_along_its_axis_is_dimensioned_to_its_top() {
    let (mut doc, face) = block(|_, _| None);
    bend(&doc, &face, 50.0);

    // The outer outline picked whole: an arc of 70, a radius alone.
    let off = 70.0 * FRAC_1_SQRT_2;
    let on_arc = pick_at(&doc, on_front(&doc, [off, off]), None);
    assert_eq!(on_arc["seen"], "circle", "{on_arc}");
    assert_eq!(on_arc["pick"]["face"], face.as_str(), "{on_arc}");
    assert!(on_arc["pick"]["outline"].is_array(), "{on_arc}");
    assert_eq!(on_arc["alone"], true, "{on_arc}");
    let at = on_front(&doc, [70.0, 70.0]);
    let radius = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "dimension", "picks": [on_arc["pick"]],
               "at": at}),
    );
    let entry = annotation(&doc, &radius["annotation"]);
    assert_eq!(entry["type"], "radius", "{entry}");
    assert!(near(entry["value"].as_f64().unwrap(), 70.0), "{entry}");
    // The inner one (a B-spline from the kernel) is the circle of 30.
    let off = 30.0 * FRAC_1_SQRT_2;
    let inner = pick_at(&doc, on_front(&doc, [off, off]), None);
    assert_eq!(inner["seen"], "circle", "{inner}");
    assert!(
        near(inner["items"][0]["arc"]["radius"].as_f64().unwrap(), 30.0),
        "{inner}"
    );

    // The top of the outer outline: its extreme point along the view's y.
    let top = pick_at(&doc, on_front(&doc, [0.3, 69.8]), None);
    assert_eq!(top["seen"], "point", "{top}");
    let toward = &top["pick"]["extreme"];
    assert!(
        near(toward[0].as_f64().unwrap(), 0.0) && near(toward[2].as_f64().unwrap(), 1.0),
        "{top}"
    );
    let at = point(&top["at"]);
    let want = on_front(&doc, [0.0, 70.0]);
    assert!(near(at[0], want[0]) && near(at[1], want[1]), "{top}");
    // The bottom edge, and the overall height from it, placed beside them.
    let bottom = pick_at(&doc, on_front(&doc, [50.0, 0.0]), None);
    assert_eq!(bottom["seen"], "line", "{bottom}");
    let at = on_front(&doc, [-20.0, 35.0]);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "dimension", "picks": [bottom["pick"], top["pick"]],
               "at": at}),
    );
    let height = added["annotation"].clone();
    let entry = annotation(&doc, &height);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["type"], "linear", "{entry}");
    assert_eq!(entry["along"], "vertical", "{entry}");
    assert!(near(entry["value"].as_f64().unwrap(), 70.0), "{entry}");

    // The model made larger: the dimensions follow it.
    bend(&doc, &face, 60.0);
    remade(&mut doc, 21.0);
    let entry = annotation(&doc, &height);
    assert_eq!(entry["broken"], false, "{entry}");
    assert!(near(entry["value"].as_f64().unwrap(), 80.0), "{entry}");
    let entry = annotation(&doc, &radius["annotation"]);
    assert!(near(entry["value"].as_f64().unwrap(), 80.0), "{entry}");

    // The face's outline gone: broken, said in words.
    doc.kernel().silhouettes.borrow_mut().remove(&face);
    remade(&mut doc, 22.0);
    for id in [&height, &radius["annotation"]] {
        let entry = annotation(&doc, id);
        assert_eq!(entry["broken"], true, "{entry}");
        let error = entry["error"].as_str().unwrap();
        assert!(error.contains("outline of a curved face"), "{error}");
        assert!(!error.contains("F2"), "{error}");
    }
}

#[test]
fn a_cylinders_outline_seen_from_the_side_gives_its_top_to_a_second_pick() {
    // A cylinder of radius 10 along x at z 20, from x 0 to 40 (the top face
    // stands for it, its ends two of its edges); the bottom edge along x at
    // z 0.
    let ends = |radius: f64| {
        move |top: bool, k: usize| match (top, k) {
            (true, 0 | 1) => {
                let x = 40.0 * k as f64;
                let rim = [x, 0.0, 20.0 + radius];
                Some(CurveGeometry::Circle {
                    center: [x, 0.0, 20.0],
                    normal: [1.0, 0.0, 0.0],
                    radius,
                    start: rim,
                    end: rim,
                })
            }
            (false, 0) => Some(line([0.0, 0.0, 0.0], [40.0, 0.0, 0.0])),
            _ => None,
        }
    };
    let (mut doc, face) = block(ends(10.0));
    let cylinder = |radius: f64| SurfaceGeometry::Cylinder {
        origin: [0.0, 0.0, 20.0],
        axis: [1.0, 0.0, 0.0],
        radius,
    };
    doc.kernel()
        .surfaces
        .borrow_mut()
        .insert(face.clone(), cylinder(10.0));

    let bottom = pick_at(&doc, on_front(&doc, [20.0, 0.0]), None);
    assert_eq!(bottom["seen"], "line", "{bottom}");
    // Alone, the outline picks the round face (a diameter) ...
    let alone = pick_at(&doc, on_front(&doc, [20.0, 29.8]), None);
    assert_eq!(alone["seen"], "axis", "{alone}");
    // ... and as a second pick its top, the extreme point along y.
    let top = pick_at(
        &doc,
        on_front(&doc, [20.0, 29.8]),
        Some(point(&bottom["at"])),
    );
    assert_eq!(top["seen"], "point", "{top}");
    assert_eq!(top["pick"]["face"], face.as_str(), "{top}");
    let toward = &top["pick"]["extreme"];
    assert!(near(toward[2].as_f64().unwrap(), 1.0), "{top}");
    let at = on_front(&doc, [-15.0, 15.0]);
    let added = command(
        &mut doc,
        json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
               "type": "dimension", "picks": [bottom["pick"], top["pick"]],
               "at": at}),
    );
    let id = added["annotation"].clone();
    let entry = annotation(&doc, &id);
    assert_eq!(entry["broken"], false, "{entry}");
    assert_eq!(entry["along"], "vertical", "{entry}");
    assert!(near(entry["value"].as_f64().unwrap(), 30.0), "{entry}");

    // The cylinder grown to a radius of 12: the top follows it.
    doc.kernel()
        .surfaces
        .borrow_mut()
        .insert(face.clone(), cylinder(12.0));
    let own: Vec<String> = edge_names(&doc)
        .into_iter()
        .filter(|n| n.contains(&face))
        .collect();
    {
        let place = ends(12.0);
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        for (k, name) in own.iter().take(2).enumerate() {
            curves.insert(name.clone(), place(true, k).unwrap());
        }
    }
    remade(&mut doc, 21.0);
    let entry = annotation(&doc, &id);
    assert_eq!(entry["broken"], false, "{entry}");
    assert!(near(entry["value"].as_f64().unwrap(), 32.0), "{entry}");
}

/// The middle of a box of the `drawing` query.
fn middle(b: &Value) -> [f64; 2] {
    let (lo, hi) = (point(&b[0]), point(&b[1]));
    [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0]
}

#[test]
fn a_sections_and_a_details_labels_are_dragged_kept_and_put_back() {
    let (mut doc, _) = block(|top, k| match (top, k) {
        (false, 0) => Some(line([0.0, 0.0, 0.0], [40.0, 0.0, 0.0])),
        (false, 1) => Some(line([0.0, 0.0, 20.0], [40.0, 0.0, 20.0])),
        _ => None,
    });
    doc.kernel().cuts_sections.set(true);
    let base = point(&view(&doc, 1)["place"]);
    // A section across the front view at x 20, beside it on the right.
    let section = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
               "line": [[base[0] + 20.0, base[1] - 5.0], [base[0] + 20.0, base[1] + 25.0]],
               "at": [330, 150]}),
    )["view"]
        .as_u64()
        .unwrap();
    let detail = command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "detail", "parent": 1,
               "center": [base[0] + 5.0, base[1] + 5.0], "radius": 6, "at": [200, 60]}),
    )["view"]
        .as_u64()
        .unwrap();
    for id in [section, detail] {
        let before = view(&doc, id);
        assert!(before["label_offset"].is_null(), "{before}");
        let label = middle(&before["label_box"]);
        let shown = middle(&before["box"]);
        assert!(near(label[0], shown[0]), "{before}");
        // Dragged down and to the right: the offset is kept.
        command(
            &mut doc,
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id,
                   "label_move": [12, -4]}),
        );
        let moved = view(&doc, id);
        assert_eq!(moved["label_offset"], json!([12.0, -4.0]), "{moved}");
        let now = middle(&moved["label_box"]);
        assert!(
            near(now[0], label[0] + 12.0) && near(now[1], label[1] - 4.0),
            "{moved}"
        );
        // A second drag adds to it.
        command(
            &mut doc,
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id,
                   "label_move": [-2, 1]}),
        );
        assert_eq!(view(&doc, id)["label_offset"], json!([10.0, -3.0]));
        // The view moved (a section along its parent only): its label goes
        // along.
        command(
            &mut doc,
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id,
                   "move": [15, -10]}),
        );
        let after = view(&doc, id);
        let shift = {
            let (a, b) = (middle(&after["box"]), middle(&moved["box"]));
            [a[0] - b[0], a[1] - b[1]]
        };
        assert!(shift[0].abs() > 1.0, "{after}");
        let label_now = middle(&after["label_box"]);
        assert!(
            near(label_now[0], now[0] - 2.0 + shift[0])
                && near(label_now[1], now[1] + 1.0 + shift[1]),
            "{after}"
        );
        // Put back under the view.
        command(
            &mut doc,
            json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": id,
                   "label_offset": [0, 0]}),
        );
        let reset = view(&doc, id);
        assert!(reset["label_offset"].is_null(), "{reset}");
        assert!(
            near(middle(&reset["label_box"])[0], middle(&reset["box"])[0]),
            "{reset}"
        );
    }
    // Kept in the file.
    command(
        &mut doc,
        json!({"cmd": "edit_drawing_view", "drawing": "D1", "view": section,
               "label_offset": [5, 5]}),
    );
    let file: Value = serde_json::from_str(&doc.to_json()).unwrap();
    let kept = file["drawings"][0]["sheets"][0]["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == section)
        .cloned()
        .unwrap();
    assert_eq!(kept["label_offset"], json!([5.0, 5.0]), "{kept}");
}
