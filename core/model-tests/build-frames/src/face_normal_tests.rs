// SPDX-License-Identifier: MIT
//! A profile started on another body's face (mitcad#2000, GitHub #427): a
//! box standing for an imported machine's leg, 40 x 60 and 500 high; a
//! profile placed on its bottom face without a direction (the app's Face
//! normal) runs along the face's outward normal, down, from the face's
//! centre of area, its sides along the face's longest straight edge (the
//! leg's 60 mm side); Rotate turns it a quarter further; a length to a
//! plane below works from there; -Z on a plane, and on the ground at a
//! point, runs it down. The mock kernel's boxes have their faces on their
//! bounds (`box_faces`); the bottom face's plane and edges are set as a
//! kernel describes them.

use serde_json::{Value, json};

use mitcad_vec3::dot;

use mitcad_model::Document;
use mitcad_model::datum::SurfaceGeometry;
use mitcad_model::kernel::Curve3;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::EdgeName;
use mitcad_model::transform::Transform;
use mitcad_model_tests_support::test_library::*;

/// The leg's bottom face's centre of area.
pub(crate) const CENTRE: [f64; 3] = [120.0, 230.0, 0.0];

/// The leg: a box from (100, 200) on the XY plane, 40 along X, 60 along
/// Y, 500 high; its bottom face as `at.geometry` takes it, with the
/// face's plane (out of the material: -Z) and its four straight edges.
pub(crate) fn leg(doc: &mut Document<MockKernel>) -> Value {
    doc.kernel().box_faces.set(true);
    let added = command(
        doc,
        json!({"cmd": "add_feature", "def": {"type": "box",
               "plane": {"type": "origin", "plane": "xy"}, "corner": [100, 200],
               "length": 40, "width": 60, "height": 500, "operation": "new_body"}}),
    )
    .unwrap_or_else(|e| panic!("box: {e}"));
    let feature = added["uid"]
        .as_str()
        .or_else(|| added["feature"].as_str())
        .unwrap_or_else(|| panic!("no feature in {added}"))
        .to_owned();
    let bottom = format!("{feature}:bottom");
    doc.kernel().surfaces.borrow_mut().insert(
        bottom.clone(),
        SurfaceGeometry::Plane {
            origin: CENTRE,
            normal: [0.0, 0.0, -1.0],
        },
    );
    let edge = |side: &str, start: [f64; 3], end: [f64; 3]| {
        let name: EdgeName = format!("E{{{bottom}|{feature}:{side}}}").parse().unwrap();
        (name, Curve3::Line { start, end })
    };
    doc.kernel().face_boundaries.borrow_mut().insert(
        bottom.clone(),
        vec![
            edge("side0", [100.0, 200.0, 0.0], [140.0, 200.0, 0.0]),
            edge("side1", [140.0, 200.0, 0.0], [140.0, 260.0, 0.0]),
            edge("side2", [140.0, 260.0, 0.0], [100.0, 260.0, 0.0]),
            edge("side3", [100.0, 260.0, 0.0], [100.0, 200.0, 0.0]),
        ],
    );
    all_ok(doc);
    json!({"body": format!("{feature}.b0"), "face": bottom})
}

fn axis(t: &Transform) -> [f64; 3] {
    t.apply_vector([0.0, 0.0, 1.0])
}

fn x_of(t: &Transform) -> [f64; 3] {
    t.apply_vector([1.0, 0.0, 0.0])
}

/// Whether a direction runs along `along`, either way.
fn parallel(a: [f64; 3], along: [f64; 3]) -> bool {
    (dot(a, along).abs() - 1.0).abs() < 1e-6
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn numbers(v: &Value) -> [f64; 3] {
    [0, 1, 2].map(|i| v[i].as_f64().unwrap_or_else(|| panic!("no number in {v}")))
}

#[test]
fn a_profile_on_a_legs_bottom_face_runs_down_centred_along_its_edges() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    // Face normal: no direction, no point (the app sends neither for a
    // face it snaps to). The preview shows where it goes before Place.
    let fields = json!({"part": part("p30"), "at": {"geometry": bottom}, "length": 300});
    let mut probe = fields.clone();
    probe["query"] = json!("build_preview");
    let shown = query(&doc, probe);
    assert_eq!(shown["problems"], json!([]), "{shown}");
    let frame = &shown["frame"];
    assert!(close(numbers(&frame["origin"]), CENTRE), "{shown}");
    assert!(
        close(numbers(&frame["z_axis"]), [0.0, 0.0, -1.0]),
        "{shown}"
    );
    assert!(
        parallel(numbers(&frame["x_axis"]), [0.0, 1.0, 0.0]),
        "{shown}"
    );
    let t = place(&mut doc, fields);
    all_ok(&doc);
    assert_eq!(t["place"], "geometry", "{t}");
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, CENTRE), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    // Its sides square to the face's edges, x along the longest (Y).
    assert!(parallel(x_of(&at), [0.0, 1.0, 0.0]), "{at:?}");
    assert!(near(
        doc.part_info(component(&t)).unwrap().length.unwrap().1,
        300.0
    ));

    // Rotate 90 deg: a quarter turn further, x along the 40 mm side (X).
    let turned = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": bottom}, "length": 300,
               "rotate": "90 deg"}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&turned));
    assert!(close(at.translation, CENTRE), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(parallel(x_of(&at), [1.0, 0.0, 0.0]), "{at:?}");

    // A click on the face keeps its point; the sides still line up.
    let clicked = place(
        &mut doc,
        json!({"part": {"source": "S1"}, "at": {"geometry": bottom, "point": [110.0, 250.0, 0.0]},
               "length": 300}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&clicked));
    assert!(close(at.translation, [110.0, 250.0, 0.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    assert!(parallel(x_of(&at), [0.0, 1.0, 0.0]), "{at:?}");
}

#[test]
fn a_length_to_a_plane_below_the_leg_works_from_its_face() {
    let mut doc = design();
    let bottom = leg(&mut doc);
    let floor = command(
        &mut doc,
        json!({"cmd": "add_feature",
               "def": {"type": "construction_plane",
                       "definition": {"type": "offset", "plane": "xy", "distance": -250}}}),
    )
    .unwrap_or_else(|e| panic!("construction plane: {e}"));
    let floor = floor["uid"].as_str().unwrap().to_owned();
    let t = place(
        &mut doc,
        json!({"part": part("p30"), "at": {"geometry": bottom},
               "length": {"mode": "to_face", "face": {"occurrence": "", "geometry": floor},
                          "offset": 0}}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, CENTRE), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    let length = doc.part_info(component(&t)).unwrap().length.unwrap().1;
    assert!(near(length, 250.0), "{length}");
}

#[test]
fn minus_z_on_a_plane_and_on_the_ground_runs_it_down() {
    let mut doc = design();
    // On the XY plane at a click, along -Z.
    let t = place(
        &mut doc,
        json!({"part": part("p30"),
               "at": {"geometry": "xy", "point": [500.0, -200.0, 0.0], "direction": [0.0, 0.0, -1.0]},
               "length": 400}),
    );
    all_ok(&doc);
    let at = placed(&doc, occurrence(&t));
    assert!(close(at.translation, [500.0, -200.0, 0.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
    // On the ground at a point (no plane picked), along -Z.
    let grounded = place(
        &mut doc,
        json!({"part": {"source": "S1"},
               "at": {"point": [0.0, 800.0, 0.0], "direction": [0.0, 0.0, -1.0]}, "length": 400}),
    );
    all_ok(&doc);
    assert_eq!(grounded["place"], "grounded", "{grounded}");
    let at = placed(&doc, occurrence(&grounded));
    assert!(close(at.translation, [0.0, 800.0, 0.0]), "{at:?}");
    assert!(close(axis(&at), [0.0, 0.0, -1.0]), "{at:?}");
}
