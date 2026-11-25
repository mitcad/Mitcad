// SPDX-License-Identifier: MIT
//! A rib's Direction Normal to Curves (mitcad#2398, GitHub
//! mitcad/Mitcad#651) with the mock kernel: its far side is the chain
//! offset by the depth as the sketch's Offset makes it, sent with the rib;
//! sharp or round corners, Flip, a spline's round corner, the draft's
//! limits, and To Next refused.

use serde_json::{Value, json};

use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
    serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> Result<FeatureUid, ModelError> {
    doc.add_feature(&def(value), None).map(|added| added.uid)
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

fn error(doc: &Document<MockKernel>, uid: FeatureUid) -> String {
    match status(doc, uid) {
        FeatureStatus::Failed(message) => message.agent_text().to_owned(),
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

/// A feature's definition as saved, its values by their parameters' names.
fn saved_def(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    let params = doc.parameters();
    let def = doc
        .feature(uid)
        .unwrap()
        .def
        .map_params(&mut |_, id| Ok::<_, ()>(params.name(*id)))
        .unwrap();
    serde_json::to_value(&def).unwrap()
}

/// A line of a sketch; its curve id.
fn line(doc: &mut Document<MockKernel>, sketch: &str, a: [f64; 2], b: [f64; 2]) -> String {
    let result = command(
        doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "start": a, "end": b}),
    );
    result["made"][0].as_str().unwrap().to_owned()
}

/// The block (Body1) and, on XY, the chain of the report: a line along x,
/// a slanted line at 45 degrees up and a line along x again, (0, 0) -
/// (40, 0) - (70, 30) - (110, 30). The sketch and the three curves.
fn with_chain() -> (Block, String, [String; 3]) {
    let mut b = block();
    let sketch = command(&mut b.doc, json!({"cmd": "sketch.create"}))["uid"]
        .as_str()
        .unwrap()
        .to_owned();
    let first = line(&mut b.doc, &sketch, [0.0, 0.0], [40.0, 0.0]);
    let slanted = line(&mut b.doc, &sketch, [40.0, 0.0], [70.0, 30.0]);
    let last = line(&mut b.doc, &sketch, [70.0, 30.0], [110.0, 30.0]);
    (b, sketch, [first, slanted, last])
}

/// A piece of the far side as the mock kernel writes it: its name
/// (`end(c1)`, `corner(c4)`) and its ends.
#[derive(Debug)]
struct Far {
    name: String,
    start: [f64; 3],
    end: [f64; 3],
}

/// The far side of the last rib the kernel built.
fn far_side(doc: &Document<MockKernel>) -> Vec<Far> {
    let spec = last_spec(doc);
    let (_, far) = spec
        .split_once(" far ")
        .unwrap_or_else(|| panic!("no far side in {spec}"));
    let point = |text: &str| -> [f64; 3] {
        let numbers: Vec<f64> = text
            .trim_matches(|c| c == '[' || c == ']')
            .split(", ")
            .map(|n| n.parse().unwrap())
            .collect();
        [numbers[0], numbers[1], numbers[2]]
    };
    far.split("; ")
        .map(|piece| {
            let (name, rest) = piece.split_once(' ').unwrap();
            let (start, end) = rest.split_once("] [").unwrap();
            Far {
                name: name.to_owned(),
                start: point(&format!("{start}]")),
                end: point(&format!("[{end}")),
            }
        })
        .collect()
}

const CHAIN: [[f64; 2]; 4] = [[0.0, 0.0], [40.0, 0.0], [70.0, 30.0], [110.0, 30.0]];

/// How far a point is from the chain (its nearest point on any of its
/// lines).
fn from_chain(p: [f64; 3]) -> f64 {
    CHAIN
        .windows(2)
        .map(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = [b[0] - a[0], b[1] - a[1]];
            let ap = [p[0] - a[0], p[1] - a[1]];
            let t =
                ((ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1])).clamp(0.0, 1.0);
            (ap[0] - t * ab[0]).hypot(ap[1] - t * ab[1])
        })
        .fold(f64::INFINITY, f64::min)
}

/// How far a point is from the line through chain segment `i`, positive
/// to its left.
fn left_of_segment(i: usize, p: [f64; 3]) -> f64 {
    let (a, b) = (CHAIN[i], CHAIN[i + 1]);
    let ab = [b[0] - a[0], b[1] - a[1]];
    let length = ab[0].hypot(ab[1]);
    (ab[0] * (p[1] - a[1]) - ab[1] * (p[0] - a[0])) / length
}

fn close(a: [f64; 3], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9 && a[2].abs() < 1e-9
}

/// The area between the chain and the far side's lines (no arcs), in the
/// sketch plane: the band the rib fills.
fn band_area(far: &[Far]) -> f64 {
    let mut outline: Vec<[f64; 2]> = CHAIN.to_vec();
    for piece in far.iter().rev() {
        outline.push([piece.end[0], piece.end[1]]);
        outline.push([piece.start[0], piece.start[1]]);
    }
    let n = outline.len();
    (0..n)
        .map(|i| {
            let (p, q) = (outline[i], outline[(i + 1) % n]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// The chain grown 30 mm Normal to Curves (the default sharp corners): its
/// far side is each line moved 30 mm to its left, the copies meeting where
/// they cross, from (0, 30) to (110, 60): every point is 30 from its own
/// line and none nearer the chain, and the band is the offset band, so the
/// rib is that band times the thickness (cli.f3_rib_normal checks the
/// volume on the kernel).
#[test]
fn a_rib_normal_to_curves_has_the_offset_chain_as_its_far_side() {
    let (mut b, sketch, [c1, c4, c7]) = with_chain();
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.starts_with(&format!(
            "rib {rib} chains {c1},{c4},{c7} thickness 4 Symmetric depth Some(30.0) flip false"
        )),
        "{spec}"
    );
    // No direction vector: the kernel grows it to the far side.
    assert!(!spec.contains("direction"), "{spec}");
    let far = far_side(&b.doc);
    let names: Vec<&str> = far.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            format!("end({c1})"),
            format!("end({c4})"),
            format!("end({c7})")
        ]
    );
    // Across from the chain's ends.
    assert!(close(far[0].start, [0.0, 30.0]), "{far:?}");
    assert!(close(far[2].end, [110.0, 60.0]), "{far:?}");
    // The copies meet where they cross: at the inner corner 30 tan(22.5°)
    // before the corner, at the outer one as far past it.
    let t = 30.0 * (std::f64::consts::PI / 8.0).tan();
    assert!(close(far[0].end, [40.0 - t, 30.0]), "{far:?}");
    assert!(close(far[1].start, [40.0 - t, 30.0]), "{far:?}");
    assert!(close(far[1].end, [70.0 - t, 60.0]), "{far:?}");
    assert!(close(far[2].start, [70.0 - t, 60.0]), "{far:?}");
    for (i, piece) in far.iter().enumerate() {
        for k in 0..=10 {
            let s = k as f64 / 10.0;
            let p: [f64; 3] =
                std::array::from_fn(|j| piece.start[j] + s * (piece.end[j] - piece.start[j]));
            assert!((left_of_segment(i, p) - 30.0).abs() < 1e-9, "{i} {p:?}");
            assert!(from_chain(p) > 30.0 - 1e-9, "{i} {p:?}");
        }
    }
    // The band: the chain's length times the depth (the inner corner's cut
    // and the outer corner's run on are equal), 2400 + 900 sqrt(2).
    let area = band_area(&far);
    assert!(
        (area - (2400.0 + 900.0 * 2f64.sqrt())).abs() < 1e-7,
        "{area}"
    );
    // Saved as given; Sharp is written as nothing.
    let saved = saved_def(&b.doc, rib);
    assert_eq!(saved["direction"], json!("normal"));
    assert!(saved.get("corners").is_none(), "{saved}");

    // Flipped, the far side is to the chain's right: from (0, -30).
    let flipped = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal", "flip": true}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, flipped), FeatureStatus::Ok);
    let far = far_side(&b.doc);
    assert_eq!(far.len(), 3, "{far:?}");
    assert!(close(far[0].start, [0.0, -30.0]), "{far:?}");
    assert!(close(far[2].end, [110.0, 0.0]), "{far:?}");
    for (i, piece) in far.iter().enumerate() {
        assert!((left_of_segment(i, piece.start) + 30.0).abs() < 1e-9, "{i}");
        assert!((left_of_segment(i, piece.end) + 30.0).abs() < 1e-9, "{i}");
    }
}

/// Corners Round: the outer corner is an arc about the chain's corner, its
/// radius the depth, so every point of the far side is exactly 30 from the
/// chain; the inner corner stays where the copies cross, as Offset makes
/// it. Saved as `"corners": "round"`.
#[test]
fn round_corners_put_every_point_of_the_far_side_at_the_depth() {
    let (mut b, sketch, [c1, c4, c7]) = with_chain();
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal", "corners": "round", "extend_ends": true}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).contains("extend_ends offset 0"),
        "{}",
        last_spec(&b.doc)
    );
    let far = far_side(&b.doc);
    let names: Vec<&str> = far.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            format!("end({c1})"),
            format!("end({c4})"),
            format!("corner({c4})"),
            format!("end({c7})")
        ]
    );
    let r = 30.0 / 2f64.sqrt();
    assert!(close(far[1].end, [70.0 - r, 30.0 + r]), "{far:?}");
    assert!(close(far[2].start, [70.0 - r, 30.0 + r]), "{far:?}");
    assert!(close(far[2].end, [70.0, 60.0]), "{far:?}");
    assert!(close(far[3].start, [70.0, 60.0]), "{far:?}");
    // The lines' points and the arc's ends: 30 from the chain.
    for piece in &far {
        let lines = !piece.name.starts_with("corner");
        for k in 0..=10 {
            let s = k as f64 / 10.0;
            if !lines && k != 0 && k != 10 {
                continue;
            }
            let p: [f64; 3] =
                std::array::from_fn(|j| piece.start[j] + s * (piece.end[j] - piece.start[j]));
            assert!((from_chain(p) - 30.0).abs() < 1e-9, "{} {p:?}", piece.name);
        }
    }
    let saved = saved_def(&b.doc, rib);
    assert_eq!(saved["corners"], json!("round"));
}

/// A chain with a spline is offset as Offset offsets it: its corner where
/// the chain turns away from the far side is round even with Corners
/// Sharp. A draft takes lines and arcs only, and says so.
#[test]
fn a_spline_is_offset_round_and_a_draft_takes_lines_and_arcs() {
    let (mut b, sketch, [c1, _, _]) = with_chain();
    let spline = command(
        &mut b.doc,
        json!({"cmd": "sketch.spline", "sketch": sketch,
               "points": [[40, 0], [80, 10], [120, 0]]}),
    )["made"][0]
        .as_str()
        .unwrap()
        .to_owned();
    // Grown to the right, the chain turns away from the far side where
    // the spline starts.
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, spline]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal", "flip": true}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    let far = far_side(&b.doc);
    let names: Vec<&str> = far.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            format!("end({c1})"),
            format!("corner({c1})"),
            format!("end({spline})")
        ]
    );
    assert!(close(far[0].start, [0.0, -30.0]), "{far:?}");

    let drafted = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, spline]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal", "flip": true, "draft_angle": "1 deg"}),
    )
    .unwrap();
    let message = error(&b.doc, drafted);
    assert!(
        message.contains("takes lines and arcs") && message.contains(&spline),
        "{message}"
    );
}

/// The chain of lines with a draft: the far side goes to the kernel with
/// the angle (the kernel leans the sides with the distance from the
/// curves).
#[test]
fn a_drafted_rib_normal_to_curves_reaches_the_kernel() {
    let (mut b, sketch, [c1, c4, c7]) = with_chain();
    let rib = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30},
               "direction": "normal", "corners": "round", "draft_angle": "1 deg"}),
    )
    .unwrap();
    assert_eq!(status(&b.doc, rib), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(spec.contains(" draft "), "{spec}");
    assert_eq!(far_side(&b.doc).len(), 4);
}

/// Normal to Curves runs a depth: with To Next it is refused in words;
/// Corners belongs to it; a web has no Direction.
#[test]
fn normal_to_curves_needs_a_depth_and_corners_needs_it() {
    let (mut b, sketch, [c1, c4, c7]) = with_chain();
    let message = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "to_next"}, "direction": "normal"}),
    )
    .unwrap_err()
    .to_string();
    assert!(
        message.contains("Normal to Curves runs a depth"),
        "{message}"
    );

    let message = add(
        &mut b.doc,
        json!({"type": "rib", "curves": {"sketch": sketch, "curves": [c1, c4, c7]},
               "thickness": 4, "extent": {"type": "depth", "depth": 30}, "corners": "round"}),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("Corners joins the far side"), "{message}");

    let message = add(
        &mut b.doc,
        json!({"type": "web", "curves": {"sketch": sketch, "curves": [c1]},
               "thickness": 2, "extent": {"type": "depth", "depth": 10}, "direction": "normal"}),
    )
    .unwrap_err()
    .to_string();
    assert!(message.contains("Direction is a rib's"), "{message}");
}
