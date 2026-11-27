// SPDX-License-Identifier: MIT
//! Boundary Surface's Fill a Loop through guide curves (mitcad#2526,
//! GitHub mitcad/Mitcad#711) on the mock kernel: the guides reach the
//! kernel beside the loop, each sketch's curves as listed; they are
//! references of the feature (a moved guide recomputes it); what a guide
//! may not be is refused. The geometry is checked with OCCT by
//! `geometry/tests/test_boundary_guides.cpp` (`geometry.boundary_guides`).

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    doc.add_feature(&def(value), None).unwrap().uid
}

fn refused(doc: &mut Document<MockKernel>, value: Value) -> String {
    match doc.add_feature(&def(value.clone()), None) {
        Ok(added) => panic!("{value} was added as {}", added.uid),
        Err(e) => e.to_string(),
    }
}

fn ok(doc: &Document<MockKernel>, uid: FeatureUid) {
    let status = doc.status(uid).cloned().expect("recomputed");
    assert!(status.is_ok(), "{uid}: {status:?}");
}

fn specs(doc: &Document<MockKernel>) -> Vec<String> {
    doc.kernel()
        .specs
        .borrow()
        .iter()
        .filter(|s| s.starts_with("boundary_surface"))
        .cloned()
        .collect()
}

fn line(doc: &mut Document<MockKernel>, sketch: FeatureUid, a: [f64; 2], b: [f64; 2]) -> String {
    let value = json!({"cmd": "sketch.add_line", "sketch": sketch, "start": a, "end": b});
    let result: Value = serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap();
    result["made"][0].as_str().unwrap().to_owned()
}

/// A rim of four lines (a 20 x 20 square) on XY and two guide lines
/// across it on XZ.
struct Bowl {
    doc: Document<MockKernel>,
    rim: FeatureUid,
    sides: [String; 4],
    depth: FeatureUid,
    guides: [String; 2],
}

fn bowl() -> Bowl {
    let mut doc = Document::new(MockKernel::default());
    let rim = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let sides = [
        line(&mut doc, rim, [0.0, 0.0], [20.0, 0.0]),
        line(&mut doc, rim, [20.0, 0.0], [20.0, 20.0]),
        line(&mut doc, rim, [20.0, 20.0], [0.0, 20.0]),
        line(&mut doc, rim, [0.0, 20.0], [0.0, 0.0]),
    ];
    let depth = doc.add_sketch(SketchPlane::Xz).unwrap().uid;
    let guides = [
        line(&mut doc, depth, [0.0, 0.0], [10.0, 5.0]),
        line(&mut doc, depth, [10.0, 5.0], [20.0, 0.0]),
    ];
    Bowl {
        doc,
        rim,
        sides,
        depth,
        guides,
    }
}

#[test]
fn a_loop_is_filled_through_its_guide_curves() {
    let mut b = bowl();
    let [s1, s2, s3, s4] = b.sides.clone();
    let [g1, g2] = b.guides.clone();
    let uid = add(
        &mut b.doc,
        json!({"type": "boundary_surface",
               "boundary": [{"sketch": b.rim, "curves": [s1, s2, s3, s4]}],
               "guides": [{"sketch": b.depth, "curves": [g1, g2]}]}),
    );
    ok(&b.doc, uid);
    // The loop as before, the guides beside it, as listed.
    assert_eq!(
        specs(&b.doc),
        [format!(
            "boundary_surface({uid}: loop [curves {s1}>{s2}>{s3}>{s4} g0] guides [curves {g1}>{g2} g0])"
        )]
    );
    // The same loop without guides: the plain fill.
    let plain = add(
        &mut b.doc,
        json!({"type": "boundary_surface",
               "boundary": [{"sketch": b.rim, "curves": [s1, s2, s3, s4]}]}),
    );
    ok(&b.doc, plain);
    assert_eq!(
        specs(&b.doc).last().cloned(),
        Some(format!(
            "boundary_surface({plain}: loop [curves {s1}>{s2}>{s3}>{s4} g0])"
        ))
    );
    // A guide moved: the guided surface is made again from it.
    let before = specs(&b.doc)
        .iter()
        .filter(|s| s.contains(&format!("({uid}:")))
        .count();
    let moved = json!({"cmd": "sketch.move", "sketch": b.depth, "entities": [g1], "by": [0, 1]});
    b.doc.command(&moved.to_string()).unwrap();
    ok(&b.doc, uid);
    let after = specs(&b.doc)
        .iter()
        .filter(|s| s.contains(&format!("({uid}:")))
        .count();
    assert!(after > before, "{:?}", specs(&b.doc));
}

#[test]
fn guide_curves_are_refused_where_they_do_not_belong() {
    let mut b = bowl();
    let [s1, s2, s3, s4] = b.sides.clone();
    let [g1, _] = b.guides.clone();
    let rim = json!([{"sketch": b.rim, "curves": [s1, s2, s3, s4]}]);
    for (value, expected) in [
        // A network runs through its curves.
        (
            json!({"type": "boundary_surface",
                   "u": [{"sketch": b.rim, "curves": [s1]}, {"sketch": b.rim, "curves": [s3]}],
                   "guides": [{"sketch": b.depth, "curves": [g1]}]}),
            "guide curves are for a boundary loop",
        ),
        // A guide is sketch curves: not a point, not without curves.
        (
            json!({"type": "boundary_surface", "boundary": rim,
                   "guides": [{"point": [10, 10, -5]}]}),
            "is sketch curves",
        ),
        (
            json!({"type": "boundary_surface", "boundary": rim,
                   "guides": [{"sketch": b.depth}]}),
            "needs its curves",
        ),
        // The surface runs through a guide: no continuity.
        (
            json!({"type": "boundary_surface", "boundary": rim,
                   "guides": [{"sketch": b.depth, "curves": [g1], "continuity": "g1"}]}),
            "has no continuity",
        ),
        // A curve of the loop is no guide of it.
        (
            json!({"type": "boundary_surface", "boundary": rim,
                   "guides": [{"sketch": b.rim, "curves": [s2]}]}),
            "both a boundary curve and a guide curve",
        ),
    ] {
        let message = refused(&mut b.doc, value.clone());
        assert!(message.contains(expected), "{value}: {message}");
    }
}
