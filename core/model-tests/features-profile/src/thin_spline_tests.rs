// SPDX-License-Identifier: MIT
//! Thin walls along splines (mitcad#2594): an open spline drawn through
//! points or by its control points, an open chain of a line and a spline,
//! a closed spline and a closed loop of a spline and a line reach the
//! kernel as chains of curves on Side 1, Side 2 and Center, as lines and
//! arcs do (mitcad#463). The walls themselves through OCCT:
//! `tools/cli/tests/f1_thin_splines.json`.

use serde_json::{Value, json};

use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

/// The wall locations and how the kernel's spec names them.
const LOCATIONS: [(&str, &str); 3] = [("side1", "Side1"), ("side2", "Side2"), ("center", "Center")];

fn last_spec(doc: &Document<MockKernel>) -> String {
    doc.kernel()
        .specs
        .borrow()
        .last()
        .cloned()
        .unwrap_or_default()
}

fn status(doc: &Document<MockKernel>, uid: FeatureUid) -> FeatureStatus {
    doc.status(uid).cloned().expect("recomputed")
}

/// A sketch command's first made entity: the curve it drew.
fn made(doc: &mut Document<MockKernel>, value: Value) -> String {
    let result: Value = serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap();
    result["made"][0].as_str().unwrap().to_owned()
}

/// A thin extrusion 10 high with walls 2 thick at `location` along the
/// curves of `sketch`: its uid, the kernel's spec and its status.
fn walls(
    b: &mut Block,
    sketch: FeatureUid,
    curves: &[&str],
    location: &str,
) -> (FeatureUid, String, FeatureStatus) {
    let value = json!({"type": "extrude", "extent": {"type": "distance", "distance": 10},
        "thin": {"location": location, "thickness": 2,
                 "curves": {"sketch": sketch, "curves": curves}},
        "operation": "new_body"});
    let uid = b.doc.add_feature(&def(value), None).unwrap().uid;
    (uid, last_spec(&b.doc), status(&b.doc, uid))
}

/// The names of the faces of the body that feature `uid` made.
fn face_names(b: &Block, uid: FeatureUid) -> Vec<String> {
    b.doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Walls along the open `curves` on each side: the kernel gets them as
/// the one open `chain` (`c3>c1 open from [x, y, z]`), and names the
/// walls after the curves and the ends after the chain's `first` and
/// `last` curves.
fn open_walls(
    b: &mut Block,
    sketch: FeatureUid,
    curves: &[&str],
    chain: &str,
    first: &str,
    last: &str,
) {
    for (location, label) in LOCATIONS {
        let (uid, spec, status) = walls(b, sketch, curves, location);
        assert_eq!(status, FeatureStatus::Ok, "{location} {curves:?}: {spec}");
        let want = format!("distance 10 taper 0 thin {label} 2; chains {chain}");
        assert!(spec.ends_with(&want), "{location} {curves:?}: {spec}");
        let faces = face_names(b, uid);
        for curve in curves {
            for role in ["outer", "inner"] {
                let face = format!("{uid}:{role}({curve})");
                assert!(faces.contains(&face), "{face}: {faces:?}");
            }
        }
        for (end, curve) in [("tip0", first), ("tip1", last)] {
            let face = format!("{uid}:{end}({curve})");
            assert!(faces.contains(&face), "{face}: {faces:?}");
        }
        b.doc.undo();
    }
}

/// An open spline of either kind, alone and in a chain with a line, is a
/// chain that runs so that it bends to the left: the arch by its control
/// points (0, 0), (20, 20), (40, 0) bends right as drawn, so it runs from
/// (40, 0), and with the line on from (40, 0) to (60, -20) from the line's
/// far end; the spline through (0, 50), (20, 40), (40, 50) bends left, so
/// it runs as drawn, and so does its chain with a line on to (60, 50).
#[test]
fn open_splines_and_their_chains_with_lines_grow_walls() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let arch = made(
        &mut b.doc,
        json!({"cmd": "sketch.spline", "sketch": sketch, "points": [[0, 0], [20, 20], [40, 0]],
               "degree": 2}),
    );
    let fit = made(
        &mut b.doc,
        json!({"cmd": "sketch.spline", "sketch": sketch, "points": [[0, 50], [20, 40], [40, 50]]}),
    );
    let slope = made(
        &mut b.doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "start": [40, 0], "end": [60, -20]}),
    );
    let level = made(
        &mut b.doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "start": [40, 50], "end": [60, 50]}),
    );
    let chain = format!("{arch} open from [40, 0, 0]");
    open_walls(&mut b, sketch, &[arch.as_str()], &chain, &arch, &arch);
    let chain = format!("{fit} open from [0, 50, 0]");
    open_walls(&mut b, sketch, &[fit.as_str()], &chain, &fit, &fit);
    let chain = format!("{slope}>{arch} open from [60, -20, 0]");
    open_walls(
        &mut b,
        sketch,
        &[arch.as_str(), slope.as_str()],
        &chain,
        &slope,
        &arch,
    );
    let chain = format!("{fit}>{level} open from [0, 50, 0]");
    open_walls(
        &mut b,
        sketch,
        &[fit.as_str(), level.as_str()],
        &chain,
        &fit,
        &level,
    );
}

/// A closed spline through four points and a closed loop of an arch (a
/// spline by its control points) and the line that closes it are closed
/// chains: walls as a profile's outline has, without ends, on each side.
#[test]
fn closed_splines_and_loops_with_splines_grow_walls() {
    let mut b = block();
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let ring = made(
        &mut b.doc,
        json!({"cmd": "sketch.spline", "sketch": sketch,
               "points": [[100, 0], [120, 10], [140, 0], [120, -10]], "closed": true}),
    );
    let arch = made(
        &mut b.doc,
        json!({"cmd": "sketch.spline", "sketch": sketch, "points": [[0, 100], [20, 120], [40, 100]],
               "degree": 2}),
    );
    let chord = made(
        &mut b.doc,
        json!({"cmd": "sketch.add_line", "sketch": sketch, "start": [40, 100], "end": [0, 100]}),
    );
    for curves in [vec![ring.as_str()], vec![arch.as_str(), chord.as_str()]] {
        for (location, label) in LOCATIONS {
            let (uid, spec, status) = walls(&mut b, sketch, &curves, location);
            assert_eq!(status, FeatureStatus::Ok, "{location} {curves:?}: {spec}");
            let kind = format!("distance 10 taper 0 thin {label} 2; chains ");
            assert!(
                spec.contains(&kind) && spec.ends_with(" closed"),
                "{location} {curves:?}: {spec}"
            );
            assert!(!spec.contains(" open from "), "{spec}");
            let faces = face_names(&b, uid);
            for curve in &curves {
                assert!(spec.contains(curve), "{curve}: {spec}");
                let face = format!("{uid}:outer({curve})");
                assert!(faces.contains(&face), "{face}: {faces:?}");
            }
            assert!(!faces.iter().any(|f| f.contains(":tip")), "{faces:?}");
            b.doc.undo();
        }
    }
}
