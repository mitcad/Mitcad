// SPDX-License-Identifier: MIT
//! Extents up to a point and starts from a point (mitcad#1329): a sketch
//! point, a vertex or a construction point stands for the plane through it
//! parallel to the profile's plane (an extrusion's) or at right angles to
//! the axis (a hole's), with the mock kernel.

use serde_json::{Value, json};

use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::VertexName;
use mitcad_model::{Document, FeatureStatus, ModelError};
use mitcad_model_api::DocumentApi;

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";
const TOP: &str = "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})";

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

/// An extrusion of the block's rectangle (Sketch1) with `extent` and the
/// fields of `extra`.
fn extrude_def(extent: Value, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                           "extent": extent, "operation": "new_body"});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// Adds the extrusion, returns its uid, kernel spec and status.
fn add_extrude(b: &mut Block, extent: Value, extra: Value) -> (FeatureUid, String, FeatureStatus) {
    let uid = b
        .doc
        .add_feature(&extrude_def(extent, extra), None)
        .unwrap()
        .uid;
    (uid, last_spec(&b.doc), status(&b.doc, uid))
}

fn up_to(point: Value) -> Value {
    json!({"type": "to_object", "object": {"type": "point", "point": point}})
}

/// Sketch2 (F3) on the XZ plane, at right angles to the block's sketch,
/// with a point p1 at x 10, z 35 (the XZ plane's sketch y runs along -Z).
fn orthogonal_sketch(b: &mut Block) -> FeatureUid {
    b.doc
        .add_feature(
            &def(json!({"type": "sketch", "plane": "xz",
                        "entities": [{"id": "p1", "type": "point", "at": [10, -35]}]})),
            None,
        )
        .unwrap()
        .uid
}

#[test]
fn an_extrusion_up_to_a_sketch_point_of_another_sketch_ends_on_the_plane_through_it() {
    let mut b = block();
    let sketch = orthogonal_sketch(&mut b);
    assert_eq!(sketch, FeatureUid(3));
    let (uid, spec, state) = add_extrude(
        &mut b,
        up_to(json!({"sketch": "F3", "point": "p1"})),
        json!({}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    // The plane through the point parallel to the profile's plane.
    assert!(
        spec.contains("to plane [10.0, 0.0, 35.0] [0.0, 0.0, 1.0] offset 0"),
        "{spec}"
    );
    // The object is kept by its sketch and id, and saved as it was given.
    let saved: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["def"]["extent"]["object"],
        json!({"type": "point", "point": {"sketch": "F3", "point": "p1"}}),
        "{saved}"
    );
    // The extrusion depends on the sketch.
    match b.doc.delete_feature(sketch, false) {
        Err(ModelError::Dependents { dependents, .. }) => {
            assert_eq!(dependents, vec!["Extrude2".to_owned()]);
        }
        other => panic!("{other:?}"),
    }
    // Moving the point moves the end with it: the plane follows the model.
    b.doc
        .edit_feature(
            sketch,
            &def(json!({"type": "sketch", "plane": "xz",
                        "entities": [{"id": "p1", "type": "point", "at": [10, -50]}]})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("to plane [10.0, 0.0, 50.0] [0.0, 0.0, 1.0] offset 0"),
        "{spec}"
    );
    // A sketch that no longer has the point is refused, as an edit that
    // takes away a plane an extrusion ends at.
    let error = b
        .doc
        .edit_feature(sketch, &def(json!({"type": "sketch", "plane": "xz"})))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Extrude2 would no longer work"), "{error}");
    assert!(error.contains("has no point p1"), "{error}");
}

#[test]
fn an_extrusion_up_to_a_vertex_or_a_construction_point_ends_on_the_plane_through_it() {
    let mut b = block();
    // A corner of the block's top face.
    let vertex = format!("V{{{TOP}|F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}}")
        .parse::<VertexName>()
        .unwrap()
        .to_string();
    b.doc
        .kernel()
        .vertex_points
        .borrow_mut()
        .insert(vertex.clone(), [60.0, 0.0, 20.0]);
    let (uid, spec, state) = add_extrude(
        &mut b,
        up_to(json!({"body": "F2.b0", "vertex": vertex})),
        json!({"start": {"type": "offset", "offset": 40}, "flip": true}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    // The normal is the profile plane's, whichever way the extrusion goes.
    assert!(
        spec.contains("from offset 40: to plane [60.0, 0.0, 20.0] [0.0, 0.0, 1.0] offset 0"),
        "{spec}"
    );
    // A vertex the kernel does not find on the body (the mock knows only
    // the corner above) fails the feature, as a missing face does.
    b.doc
        .edit_feature(
            uid,
            &extrude_def(
                up_to(json!({"body": "F2.b0",
                             "vertex": format!("V{{{TOP}|F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}}")})),
                json!({}),
            ),
        )
        .unwrap();
    match status(&b.doc, uid) {
        FeatureStatus::Failed(message) => assert!(message.contains("vertex"), "{message}"),
        other => panic!("{other:?}"),
    }
    b.doc.undo();
    b.doc.undo();

    // A construction point, with an offset beyond it.
    let point = b
        .doc
        .add_feature(
            &def(json!({"type": "construction_point",
                        "definition": {"type": "fixed", "point": [5, 5, 30]}})),
            None,
        )
        .unwrap()
        .uid;
    let mut extent = up_to(json!(point.to_string()));
    extent["offset"] = json!(2);
    let (uid, spec, state) = add_extrude(&mut b, extent, json!({}));
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    assert!(
        spec.contains("to plane [5.0, 5.0, 30.0] [0.0, 0.0, 1.0] offset 2"),
        "{spec}"
    );
    // The construction point is not deleted from under the extrusion.
    match b.doc.delete_feature(point, false) {
        Err(ModelError::Dependents { dependents, .. }) => {
            assert_eq!(dependents, vec![b.doc.feature(uid).unwrap().name.clone()]);
        }
        other => panic!("{other:?}"),
    }
    // A construction plane is not a point.
    let plane = b
        .doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": "xy", "distance": 30}})),
            None,
        )
        .unwrap()
        .uid;
    let error = b
        .doc
        .add_feature(
            &extrude_def(up_to(json!(plane.to_string())), json!({})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a point"), "{error}");
}

#[test]
fn an_extrusion_starts_from_a_point_and_sides_end_at_points() {
    let mut b = block();
    orthogonal_sketch(&mut b);
    let point = json!({"type": "point", "point": {"sketch": "F3", "point": "p1"}});
    // The profile projected onto the plane through the point.
    let (_, spec, state) = add_extrude(
        &mut b,
        json!({"type": "distance", "distance": 5}),
        json!({"start": {"type": "object", "object": point, "offset": 3}}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    assert!(
        spec.contains("from object plane [10.0, 0.0, 35.0] [0.0, 0.0, 1.0]: distance 5"),
        "{spec}"
    );
    b.doc.undo();
    // Side two of a two-sided extrusion up to a point.
    let (_, spec, state) = add_extrude(
        &mut b,
        json!({"type": "two_sides", "side1": {"type": "distance", "distance": 10},
               "side2": {"type": "to_object", "object": point}}),
        json!({}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    assert!(
        spec.contains("distance 10 taper 0; to plane [10.0, 0.0, 35.0] [0.0, 0.0, 1.0] offset 0"),
        "{spec}"
    );
}

#[test]
fn a_hole_ends_at_the_plane_through_a_point_and_a_revolution_takes_none() {
    let mut b = block();
    let point = b
        .doc
        .add_feature(
            &def(json!({"type": "construction_point",
                        "definition": {"type": "fixed", "point": [5, 5, 8]}})),
            None,
        )
        .unwrap()
        .uid;
    let hole = def(json!({"type": "hole",
        "placement": {"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]},
        "diameter": 6,
        "extent": {"type": "to_object", "object": {"type": "point", "point": point.to_string()}}}));
    let uid = b.doc.add_feature(&hole, None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // At right angles to the hole's axis, which points into the body.
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("target plane [5.0, 5.0, 8.0] [0.0, 0.0, -1.0] offset 0"),
        "{spec}"
    );
    b.doc.undo();
    // A revolution ends at a plane, a face or a body only.
    let revolve = def(
        json!({"type": "revolve", "profiles": [{"sketch": "F1", "region": RECT}],
        "axis": "y", "operation": "new_body",
        "extent": {"type": "to_object", "object": {"type": "point", "point": point.to_string()}}}),
    );
    let error = b.doc.add_feature(&revolve, None).unwrap_err().to_string();
    assert!(error.contains("not a point"), "{error}");
}

#[test]
fn point_objects_serialize() {
    for point in [
        json!({"sketch": "F3", "point": "p1"}),
        json!({"body": "F2.b0", "vertex": format!("V{{{TOP}|F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}}")}),
        json!("F5"),
        json!("origin"),
    ] {
        let object = json!({"type": "point", "point": point});
        let parsed: mitcad_model::features::ExtentObject =
            serde_json::from_value(object.clone()).unwrap();
        assert!(parsed.is_point_or_line() && parsed.is_planar());
        assert_eq!(parsed.body(), None);
        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back["type"], "point", "{back}");
    }
    let line: mitcad_model::features::ExtentObject =
        serde_json::from_value(json!({"type": "line", "line": {"sketch": "F3", "curve": "c1"}}))
            .unwrap();
    assert!(line.is_point_or_line() && line.is_planar());
    assert_eq!(
        serde_json::to_value(&line).unwrap(),
        json!({"type": "line", "line": {"sketch": "F3", "curve": "c1"}})
    );
}

/// Sketch2 (F3) on the XZ plane (its sketch y runs along -Z) with three
/// lines: c1 along X at z 35 and c3 along X at z 8, both at right angles to
/// the block's sketch normal, and c2 slanted from (0, 0, 10) to (60, 0,
/// 40), 26.565 degrees out of the planes parallel to the block's sketch.
fn line_sketch(b: &mut Block) -> FeatureUid {
    b.doc
        .add_feature(
            &def(json!({"type": "sketch", "plane": "xz", "entities": [
                // Entity ids share one numbering: the lines are c1 to c3.
                {"id": "p4", "type": "point", "at": [0, -35]},
                {"id": "p5", "type": "point", "at": [60, -35]},
                {"id": "p6", "type": "point", "at": [0, -10]},
                {"id": "p7", "type": "point", "at": [60, -40]},
                {"id": "p8", "type": "point", "at": [0, -8]},
                {"id": "p9", "type": "point", "at": [60, -8]},
                {"id": "c1", "type": "line", "start": "p4", "end": "p5"},
                {"id": "c2", "type": "line", "start": "p6", "end": "p7"},
                {"id": "c3", "type": "line", "start": "p8", "end": "p9"}]})),
            None,
        )
        .unwrap()
        .uid
}

fn line_object(curve: &str) -> Value {
    json!({"type": "line", "line": {"sketch": "F3", "curve": curve}})
}

/// The message a feature fails with: refused when added, or failed when
/// evaluated.
fn failure(b: &mut Block, def: &FeatureDef<mitcad_model::ValueInput>) -> String {
    match b.doc.add_feature(def, None) {
        Ok(added) => match status(&b.doc, added.uid) {
            FeatureStatus::Failed(message) => message,
            other => panic!("{other:?}"),
        },
        Err(error) => error.to_string(),
    }
}

#[test]
fn an_extrusion_up_to_or_from_a_line_parallel_to_the_profile_ends_on_the_plane_through_it() {
    let mut b = block();
    assert_eq!(line_sketch(&mut b), FeatureUid(3));
    // Up to the line at z 35: the plane through it parallel to the profile.
    let (uid, spec, state) = add_extrude(
        &mut b,
        json!({"type": "to_object", "object": line_object("c1")}),
        json!({}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    assert!(
        spec.contains("to plane [0.0, 0.0, 35.0] [0.0, 0.0, 1.0] offset 0"),
        "{spec}"
    );
    let saved: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["def"]["extent"]["object"],
        line_object("c1"),
        "{saved}"
    );
    b.doc.undo();
    // From the line at z 8, 5 mm on.
    let (_, spec, state) = add_extrude(
        &mut b,
        json!({"type": "distance", "distance": 5}),
        json!({"start": {"type": "object", "object": line_object("c3")}}),
    );
    assert_eq!(state, FeatureStatus::Ok, "{spec}");
    assert!(
        spec.contains("from object plane [0.0, 0.0, 8.0] [0.0, 0.0, 1.0]: distance 5"),
        "{spec}"
    );
    b.doc.undo();
    // A slanted line lies in no plane parallel to the profile: refused,
    // up to it and from it.
    for extra in [
        json!({}),
        json!({"start": {"type": "object", "object": line_object("c2")}}),
    ] {
        let extent = if extra.as_object().unwrap().is_empty() {
            json!({"type": "to_object", "object": line_object("c2")})
        } else {
            json!({"type": "distance", "distance": 5})
        };
        let message = failure(&mut b, &extrude_def(extent, extra));
        assert!(
            message.contains("does not lie in a plane parallel to the profile's plane"),
            "{message}"
        );
        assert!(message.contains("26.565 degrees off"), "{message}");
    }
    // A point is no line.
    let message = failure(
        &mut b,
        &extrude_def(
            json!({"type": "to_object", "object": {"type": "line", "line": "origin"}}),
            json!({}),
        ),
    );
    assert!(
        message.contains("not an axis or a straight edge"),
        "{message}"
    );
}

#[test]
fn a_hole_ends_at_a_line_at_right_angles_to_its_axis_and_a_revolution_takes_none() {
    let mut b = block();
    line_sketch(&mut b);
    let hole = |curve: &str| {
        def(json!({"type": "hole",
            "placement": {"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]},
            "diameter": 6,
            "extent": {"type": "to_object", "object": line_object(curve)}}))
    };
    let uid = b.doc.add_feature(&hole("c3"), None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("target plane [0.0, 0.0, 8.0] [0.0, 0.0, -1.0] offset 0"),
        "{spec}"
    );
    b.doc.undo();
    let message = failure(&mut b, &hole("c2"));
    assert!(
        message.contains("does not lie in a plane at right angles to the hole's axis"),
        "{message}"
    );
    let revolve = def(
        json!({"type": "revolve", "profiles": [{"sketch": "F1", "region": RECT}],
        "axis": "y", "operation": "new_body",
        "extent": {"type": "to_object", "object": line_object("c1")}}),
    );
    let error = b.doc.add_feature(&revolve, None).unwrap_err().to_string();
    assert!(error.contains("not a point or a line"), "{error}");
}
