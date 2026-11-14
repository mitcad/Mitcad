// SPDX-License-Identifier: MIT
//! A radius on a fillet round a turned part seen from the side
//! (mitcad#1694) on the mock kernel: the fillet's torus face offers its
//! outline's arc either side of its axis, a click with the Dimension
//! tool's Auto on either picks that arc (not the end of an edge merely
//! within reach) and makes a radius of the fillet's; its arrow sits on its
//! leader wherever its text is placed (mitcad#1770).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::datum::{CurveGeometry, SurfaceGeometry};
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const R: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

/// The fillet's radius, and the boss's (a cylinder round the z axis above
/// the flange's top at z 10).
const FILLET: f64 = 1.5;
const BOSS: f64 = 10.0;

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

fn annotation(doc: &Document<MockKernel>, id: &Value) -> Value {
    sheet(doc)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == *id)
        .cloned()
        .unwrap_or_else(|| panic!("no annotation {id}"))
}

fn circle(center: [f64; 3], radius: f64) -> CurveGeometry {
    let rim = [center[0] + radius, center[1], center[2]];
    CurveGeometry::Circle {
        center,
        normal: [0.0, 0.0, 1.0],
        radius,
        start: rim,
        end: rim,
    }
}

/// A turned part seen from the front (view 1, the model's x and z at 1:1):
/// a boss of radius 10 round the z axis standing on a flange whose top is
/// at z 10, the fillet between them a torus face (the block's top face,
/// F2.b0) of radius 1.5 round the axis: its tube's centre circle of radius
/// 11.5 at z 11.5, its ends the circles where it meets the boss (radius 10
/// at z 11.5) and the flange (radius 11.5 at z 10), edges of that face.
/// The boss's seam, a straight edge up from the fillet's upper end on the
/// right (x 10), ends where the arc does; the face's other edges lie far
/// away. Returns the face's name.
fn turned() -> (Document<MockKernel>, String) {
    let mut doc = Document::new(MockKernel::default());
    command(
        &mut doc,
        json!([
            {"cmd": "sketch.create"},
            {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
            {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": R}],
              "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
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
    let own: Vec<&String> = names.iter().filter(|n| n.contains(&face)).collect();
    assert!(own.len() >= 3, "the top face's edges: {names:?}");
    let tube = BOSS + FILLET;
    {
        let mut curves = doc.kernel().edge_curves.borrow_mut();
        curves.insert(own[0].clone(), circle([0.0, 0.0, tube], BOSS));
        curves.insert(own[1].clone(), circle([0.0, 0.0, 10.0], tube));
        curves.insert(
            own[2].clone(),
            CurveGeometry::Line {
                start: [BOSS, 0.0, tube],
                end: [BOSS, 0.0, 30.0],
            },
        );
        for (k, other) in names.iter().filter(|n| !own[..3].contains(n)).enumerate() {
            let x = 300.0 + 5.0 * k as f64;
            curves.insert(
                other.clone(),
                CurveGeometry::Line {
                    start: [x, 0.0, 300.0],
                    end: [x + 1.0, 0.0, 300.0],
                },
            );
        }
    }
    doc.kernel().surfaces.borrow_mut().insert(
        face.clone(),
        SurfaceGeometry::Torus {
            center: [0.0, 0.0, tube],
            axis: [0.0, 0.0, 1.0],
            major_radius: tube,
            minor_radius: FILLET,
        },
    );
    command(&mut doc, json!({"cmd": "add_drawing", "size": "A3"}));
    command(
        &mut doc,
        json!({"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front",
               "at": [200, 150], "scale": "1:1"}),
    );
    (doc, face)
}

/// Where a point of the front view (the model's x and z) is on the sheet.
fn on_front(doc: &Document<MockKernel>, at: [f64; 2]) -> [f64; 2] {
    let view = sheet(doc)["views"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == 1)
        .cloned()
        .expect("the front view");
    let place = point(&view["place"]);
    [place[0] + at[0], place[1] + at[1]]
}

#[test]
fn a_fillet_seen_from_the_side_is_dimensioned_on_either_side_by_auto() {
    let (mut doc, face) = turned();
    let tube = BOSS + FILLET;
    // The middle of each side's arc, toward the corner it rounds: on the
    // right about (11.5, 11.5), on the left about (-11.5, 11.5). The right
    // one lies 1.15 mm from the seam's end, within a point's reach.
    let off = FILLET * std::f64::consts::FRAC_1_SQRT_2;
    for (side, on_arc, placed) in [
        (1.0, [tube - off, tube - off], [25.0, 25.0]),
        (-1.0, [-tube + off, tube - off], [-25.0, 25.0]),
    ] {
        let at = on_front(&doc, on_arc);
        let hovered = query(
            &doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at}),
        );
        assert_eq!(hovered["seen"], "circle", "{side}: {hovered}");
        assert_eq!(hovered["pick"]["face"], face.as_str(), "{side}: {hovered}");
        assert_eq!(hovered["alone"], true, "{side}: {hovered}");
        let toward = hovered["pick"]["side"][0].as_f64().unwrap();
        assert!(toward * side > 0.99, "{side}: {hovered}");
        // The Radius type picks it too.
        let typed = query(
            &doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at,
                   "type": "radius"}),
        );
        assert_eq!(typed["pick"], hovered["pick"], "{side}: {typed}");
        // Auto: a radius of the fillet's.
        let text_at = on_front(&doc, placed);
        let added = command(
            &mut doc,
            json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                   "type": "dimension", "picks": [at], "at": text_at}),
        );
        let entry = annotation(&doc, &added["annotation"]);
        assert_eq!(entry["broken"], false, "{side}: {entry}");
        assert_eq!(entry["type"], "radius", "{side}: {entry}");
        assert!(
            near(entry["value"].as_f64().unwrap(), FILLET),
            "{side}: {entry}"
        );
        assert_eq!(entry["label"], "R1.5", "{side}: {entry}");
    }
    // At the seam's end itself the point still wins.
    let corner = query(
        &doc,
        json!({"query": "drawing_pick", "drawing": "D1", "view": 1,
               "at": on_front(&doc, [BOSS + 0.05, tube + 0.05])}),
    );
    assert_eq!(corner["seen"], "point", "{corner}");
    // A torus face picked on no side is refused.
    let error = doc
        .command(
            &json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                    "type": "radius", "picks": [{"body": "F2.b0", "face": face}]})
            .to_string(),
        )
        .expect_err("a torus face without its side");
    assert!(error.to_string().contains("side of its axis"), "{error}");
}

/// The drawn items of annotation `id` on the sheet.
fn drawn(doc: &Document<MockKernel>, id: &Value) -> Vec<Value> {
    let group = format!("A{id}");
    sheet(doc)["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == group.as_str())
        .unwrap_or_else(|| panic!("no group {group}"))["items"]
        .as_array()
        .unwrap()
        .clone()
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn length(a: [f64; 2]) -> f64 {
    a[0].hypot(a[1])
}

fn unit(a: [f64; 2]) -> [f64; 2] {
    let l = length(a);
    [a[0] / l, a[1] / l]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

#[test]
fn a_fillets_radius_arrow_sits_on_its_leader_wherever_its_text_is() {
    // mitcad#1770 (GitHub #323): the radius of the fillet between the boss
    // and the flange, picked on either side, its text placed in each
    // quadrant round the arc's centre: in the arc's sweep (out over the
    // corner it rounds), on its hollow side (up and out, where the report
    // had it) and beyond either end. The sheet draws one arrow, its tip on
    // the arc (within 0.01 sheet mm) at the end of the leader from the
    // text, pointing along the leader's last segment (within half a
    // degree).
    let (mut doc, _) = turned();
    let tube = BOSS + FILLET;
    let off = FILLET * std::f64::consts::FRAC_1_SQRT_2;
    let cos_half_degree = 0.5_f64.to_radians().cos();
    // Each side: a point on its arc, its centre, and its sweep (degrees,
    // counter-clockwise on the sheet): on the right from the boss's side
    // (180) down to the flange's (270), on the left from the flange's
    // (270) round to the boss's (360).
    for (on_arc, center, sweep) in [
        ([tube - off, tube - off], [tube, tube], (180.0, 270.0)),
        ([-tube + off, tube - off], [-tube, tube], (270.0, 360.0)),
    ] {
        let at = on_front(&doc, on_arc);
        let picked = query(
            &doc,
            json!({"query": "drawing_pick", "drawing": "D1", "view": 1, "at": at,
                   "type": "radius"}),
        );
        let c = on_front(&doc, center);
        for degrees in [45.0_f64, 135.0, 225.0, 315.0, 20.0, 110.0, 200.0, 290.0] {
            let (s, co) = degrees.to_radians().sin_cos();
            let text_at = [c[0] + 12.0 * co, c[1] + 12.0 * s];
            let case = format!("sweep {sweep:?}, text at {degrees} degrees");
            let added = command(
                &mut doc,
                json!({"cmd": "add_drawing_annotation", "drawing": "D1", "view": 1,
                       "type": "radius", "picks": [picked["pick"]], "at": text_at}),
            );
            let id = added["annotation"].clone();
            let entry = annotation(&doc, &id);
            assert_eq!(entry["broken"], false, "{case}: {entry}");
            assert!(
                near(entry["value"].as_f64().unwrap(), FILLET),
                "{case}: {entry}"
            );
            let items = drawn(&doc, &id);
            let arrows: Vec<Vec<[f64; 2]>> = items
                .iter()
                .filter_map(|i| i.get("fill"))
                .map(|f| f.as_array().unwrap().iter().map(point).collect())
                .collect();
            assert_eq!(arrows.len(), 1, "{case}: one arrow: {items:?}");
            let tip = arrows[0][0];
            let base = [
                (arrows[0][1][0] + arrows[0][2][0]) / 2.0,
                (arrows[0][1][1] + arrows[0][2][1]) / 2.0,
            ];
            let along = unit(sub(tip, base));
            // Its tip on the arc.
            let r = sub(tip, c);
            assert!(
                (length(r) - FILLET).abs() < 0.01,
                "{case}: the tip {tip:?} is off the arc's circle"
            );
            let angle = r[1].atan2(r[0]).to_degrees();
            assert!(
                (angle - sweep.0 + 1e-6).rem_euclid(360.0) <= sweep.1 - sweep.0 + 2e-6,
                "{case}: the tip at {angle} degrees is off the arc"
            );
            // The one line that ends at the tip: the leader's last
            // segment, along the arrow towards the arc.
            let ending: Vec<[f64; 2]> = items
                .iter()
                .filter_map(|i| i.get("line"))
                .filter_map(|l| {
                    let (a, b) = (point(&l[0]), point(&l[1]));
                    if length(sub(b, tip)) < 1e-9 {
                        Some(a)
                    } else if length(sub(a, tip)) < 1e-9 {
                        Some(b)
                    } else {
                        None
                    }
                })
                .collect();
            assert_eq!(ending.len(), 1, "{case}: {items:?}");
            let from = ending[0];
            assert!(
                dot(unit(sub(tip, from)), along) >= cos_half_degree,
                "{case}: the arrow {along:?} is not along the leader's last segment \
                 {from:?}-{tip:?}"
            );
            // That leader reaches the text: straight, or through a bend.
            let reaches = length(sub(from, text_at)) < 1e-6
                || items.iter().filter_map(|i| i.get("line")).any(|l| {
                    let (a, b) = (point(&l[0]), point(&l[1]));
                    (length(sub(a, text_at)) < 1e-6 && length(sub(b, from)) < 1e-9)
                        || (length(sub(b, text_at)) < 1e-6 && length(sub(a, from)) < 1e-9)
                });
            assert!(
                reaches,
                "{case}: the leader does not reach the text: {items:?}"
            );
            command(
                &mut doc,
                json!({"cmd": "delete_drawing_annotation", "drawing": "D1", "annotation": id}),
            );
        }
    }
}
