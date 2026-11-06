// SPDX-License-Identifier: MIT
//! Trimming the middle out of an arc keeps its two pieces on one circle
//! (mitcad#1314): an arc has no radius of its own, only its ends as far
//! from its centre, so the pieces are made Equal, as a line's pieces are
//! made Collinear; the sketch keeps its degrees of freedom.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, command: Value) -> Value {
    let result = doc
        .command(&command.to_string())
        .unwrap_or_else(|e| panic!("{command}: {e}"));
    serde_json::from_str(&result).unwrap()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

/// The ids and solved radii of the sketch's arcs.
fn arcs(s: &Value) -> Vec<(String, f64)> {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "arc")
        .map(|e| {
            (
                e["id"].as_str().unwrap().to_owned(),
                e["geometry"]["radius"].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn the_pieces_of_a_trimmed_arc_keep_one_radius() {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    // A three-point arc from (10, 0) through (0, 10) to (-10, 0), and two
    // vertical lines across it at x = 5 and x = -5.
    let made = command(
        &mut d,
        json!({"cmd": "sketch.arc", "sketch": "F1", "mode": "three_point",
               "start": [10, 0], "through": [0, 10], "end": [-10, 0]}),
    );
    let first = made["made"][0].as_str().unwrap().to_owned();
    for x in [5, -5] {
        command(
            &mut d,
            json!({"cmd": "sketch.add_line", "sketch": "F1", "start": [x, 0], "end": [x, 15]}),
        );
    }
    let dof = sketch(&d)["dof"].clone();
    command(
        &mut d,
        json!({"cmd": "sketch.trim", "sketch": "F1", "curve": first, "at": [0, 10]}),
    );
    let s = sketch(&d);
    let pieces = arcs(&s);
    assert_eq!(pieces.len(), 2, "{s}");
    assert!(pieces.iter().any(|(id, _)| *id == first), "{pieces:?}");
    // No more freedom than the whole arc had.
    assert_eq!(s["dof"], dof, "{s}");
    // The first piece's radius set to 12: the second follows.
    command(
        &mut d,
        json!({"cmd": "sketch.add_dimension", "sketch": "F1",
               "dimension": {"type": "radius", "curve": first}, "value": 12}),
    );
    let pieces = arcs(&sketch(&d));
    assert_eq!(pieces.len(), 2);
    for (id, radius) in &pieces {
        assert!((radius - 12.0).abs() < 1e-6, "{id}: radius {radius}");
    }
}
