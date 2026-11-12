// SPDX-License-Identifier: MIT
//! A radius on a fillet round a turned part seen from the side
//! (mitcad#1694) on the mock kernel: the fillet's torus face offers its
//! outline's arc either side of its axis, a click with the Dimension
//! tool's Auto on either picks that arc (not the end of an edge merely
//! within reach) and makes a radius of the fillet's.

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
