// SPDX-License-Identifier: MIT
//! The sketch Rectangular Pattern's optional second direction
//! (mitcad#1969): rows at another angle than a right angle (a honeycomb),
//! an old pattern without it unchanged, and a parallel one refused.

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

fn rejected(doc: &mut Document<MockKernel>, command: Value) -> String {
    doc.command(&command.to_string())
        .map(|r| panic!("{command} must fail, got {r}"))
        .unwrap_err()
        .to_string()
}

fn sketch(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(
        &doc.query(&json!({"query": "sketch", "uid": "F1"}).to_string())
            .unwrap(),
    )
    .unwrap()
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in the sketch"));
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < 1e-6
}

/// A pointy-topped hexagon about the origin, its corners 5 mm from it.
fn hexagon() -> (Document<MockKernel>, Vec<String>) {
    let mut d = Document::new(MockKernel::default());
    command(&mut d, json!({"cmd": "sketch.create"}));
    let made = command(
        &mut d,
        json!({"cmd": "sketch.polygon", "sketch": "F1", "center": [0, 0], "vertex": [0, 5],
               "sides": 6}),
    );
    let curves = made["made"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_owned())
        .collect();
    (d, curves)
}

/// The original corner points of the pattern and where the copy at
/// `index` puts each of them.
fn corners(s: &Value, index: [u32; 2]) -> Vec<([f64; 2], [f64; 2])> {
    let copy = s["patterns"][0]["copies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["index"] == json!(index))
        .unwrap_or_else(|| panic!("no copy {index:?}"));
    copy["entities"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(original, _)| original.starts_with('p'))
        .map(|(original, made)| (at(s, original), at(s, made.as_str().unwrap())))
        .collect()
}

/// How far the copy at `index` lies from the originals (the same for
/// every corner).
fn offset(s: &Value, index: [u32; 2]) -> [f64; 2] {
    let pairs = corners(s, index);
    assert_eq!(pairs.len(), 6, "{index:?}");
    let (a, b) = pairs[0];
    let d = [b[0] - a[0], b[1] - a[1]];
    for (a, b) in &pairs {
        assert!(near([b[0] - a[0], b[1] - a[1]], d), "{index:?}");
    }
    d
}

#[test]
fn a_sixty_degree_second_direction_makes_a_honeycomb() {
    let (mut d, hexagon) = hexagon();
    // Across the flats: the 5 mm corner distance times 2 cos 30°.
    let across = 10.0 * 30f64.to_radians().cos();
    let sixty = [0.5, 3f64.sqrt() / 2.0];
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": hexagon,
               "direction": [1, 0], "direction2": sixty, "count": [3, 2],
               "spacing": [across, across]}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    let s = sketch(&d);
    assert_eq!(s["patterns"][0]["copies"].as_array().unwrap().len(), 5);
    let stored = &s["patterns"][0]["direction2"];
    assert!(near(
        [stored[0].as_f64().unwrap(), stored[1].as_f64().unwrap()],
        sixty
    ));
    for (index, expected) in [
        ([1, 0], [across, 0.0]),
        ([2, 0], [2.0 * across, 0.0]),
        ([0, 1], [across * sixty[0], across * sixty[1]]),
        ([2, 1], [across * (2.0 + sixty[0]), across * sixty[1]]),
    ] {
        let got = offset(&s, index);
        assert!(near(got, expected), "{index:?}: {got:?}");
    }
    // A honeycomb: each neighbour shares a side (two corners) with the
    // original hexagon.
    for index in [[1, 0], [0, 1]] {
        let pairs = corners(&s, index);
        let shared = pairs
            .iter()
            .filter(|(_, made)| pairs.iter().any(|(original, _)| near(*original, *made)))
            .count();
        assert_eq!(shared, 2, "{index:?}");
    }
    // Edit Pattern keeps the second direction when it is left out, and
    // `null` turns the rows back to a right angle.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "count": [2, 2]}),
    );
    let got = offset(&sketch(&d), [0, 1]);
    assert!(near(got, [across * sixty[0], across * sixty[1]]), "{got:?}");
    let before: Vec<[f64; 2]> = corners(&sketch(&d), [1, 0])
        .iter()
        .map(|(o, _)| *o)
        .collect();
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "direction2": null}),
    );
    let s = sketch(&d);
    assert!(s["patterns"][0].get("direction2").is_none());
    let got = offset(&s, [0, 1]);
    assert!(near(got, [0.0, across]), "{got:?}");
    // The free hexagon stays where it is: the copies move.
    let after: Vec<[f64; 2]> = corners(&s, [1, 0]).iter().map(|(o, _)| *o).collect();
    for (a, b) in before.iter().zip(&after) {
        assert!(near(*a, *b), "{a:?} -> {b:?}");
    }
    // And Edit Pattern sets one again.
    command(
        &mut d,
        json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern,
               "direction2": [-0.5, sixty[1]]}),
    );
    let got = offset(&sketch(&d), [0, 1]);
    assert!(near(got, [-across * 0.5, across * sixty[1]]), "{got:?}");
}

#[test]
fn a_pattern_without_a_second_direction_is_unchanged() {
    // An old pattern (no `direction2`): the rows a quarter turn from the
    // first direction, and nothing new written.
    let (mut d, hexagon) = hexagon();
    command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": hexagon,
               "direction": [0, 1], "count": [2, 2], "spacing": [12, 10]}),
    );
    let s = sketch(&d);
    assert!(s["patterns"][0].get("direction2").is_none());
    assert!(near(offset(&s, [1, 0]), [0.0, 12.0]));
    assert!(near(offset(&s, [0, 1]), [-10.0, 0.0]));
    assert!(near(offset(&s, [1, 1]), [-10.0, 12.0]));
}

#[test]
fn a_second_direction_parallel_to_the_first_is_refused() {
    let (mut d, hexagon) = hexagon();
    for direction2 in [json!([2, 0]), json!([-1, 0]), json!([0, 0])] {
        let error = rejected(
            &mut d,
            json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": hexagon,
                   "direction": [1, 0], "direction2": direction2, "count": [2, 2],
                   "spacing": [10, 10]}),
        );
        assert!(
            error.contains("the second direction is"),
            "{direction2}: {error}"
        );
    }
    let made = command(
        &mut d,
        json!({"cmd": "sketch.rectangular_pattern", "sketch": "F1", "entities": hexagon,
               "direction": [1, 0], "direction2": [1, 1], "count": [2, 2],
               "spacing": [10, 10]}),
    );
    let pattern = made["constraints"][0].as_str().unwrap().to_owned();
    // Turning the first direction onto the second, or the second onto the
    // first, is refused too.
    for change in [json!({"direction": [3, 3]}), json!({"direction2": [1, 0]})] {
        let mut cmd = json!({"cmd": "sketch.edit_pattern", "sketch": "F1", "pattern": pattern});
        for (k, v) in change.as_object().unwrap() {
            cmd[k] = v.clone();
        }
        let error = rejected(&mut d, cmd);
        assert!(
            error.contains("the second direction is parallel to the first"),
            "{change}: {error}"
        );
    }
}
