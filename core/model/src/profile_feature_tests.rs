// SPDX-License-Identifier: MIT
//! The profile features (extrude options, revolve, hole, thread) with the
//! mock kernel: what they ask the kernel for, their parameters, checks,
//! errors, references and the queries about them.

use serde_json::{Value, json};

use crate::Document;
use crate::document_tests::{Block, block, def};
use crate::features::FeatureDef;
use crate::ids::{BodyUid, FeatureUid};
use crate::testing::MockKernel;
use crate::{FeatureStatus, ModelError};

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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn extrude_def(extent: Value, extra: Value) -> FeatureDef<crate::ValueInput> {
    let mut value = json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                           "extent": extent, "operation": "new_body"});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// A second extrude of the block's sketch, its kernel spec and status.
fn extrude_with(b: &mut Block, extent: Value, extra: Value) -> (String, FeatureStatus) {
    let uid = b
        .doc
        .add_feature(&extrude_def(extent, extra), None)
        .unwrap()
        .uid;
    let spec = last_spec(&b.doc);
    let status = status(&b.doc, uid);
    b.doc.undo();
    (spec, status)
}

#[test]
fn extents_starts_tapers_and_walls_reach_the_kernel() {
    let mut b = block();
    let cases = [
        (
            json!({"type": "distance", "distance": 20, "taper": -0.1}),
            json!({}),
            "along [0.0, 0.0, 1.0] from offset 0: distance 20 taper -0.1",
        ),
        (
            json!({"type": "symmetric", "distance": 20, "full_length": true, "taper": 0.2}),
            json!({}),
            "from offset 0: distance 10 taper 0.2; distance 10 taper 0.2",
        ),
        (
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 15},
                   "side2": {"type": "through_all", "taper": 0.1}}),
            json!({"flip": true}),
            "along [-0.0, -0.0, -1.0] from offset 0: distance 15 taper 0; distance 26 taper 0.1",
        ),
        (
            json!({"type": "to_object", "object": {"type": "face", "body": "F2.b0", "face": TOP},
                   "offset": 2}),
            json!({"start": {"type": "offset", "offset": 40}, "flip": true}),
            "from offset 40: to face F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}) extend true offset 2",
        ),
        (
            json!({"type": "to_object", "object": {"type": "body", "body": "F2.b0", "through": true}}),
            json!({}),
            "to body prism(F2:0..20) through true offset 0",
        ),
        (
            json!({"type": "distance", "distance": 5}),
            json!({"start": {"type": "object", "object": {"type": "plane", "plane": "xz"}, "offset": 3}}),
            "from object plane [0.0, 0.0, 0.0] [0.0, 1.0, 0.0]: distance 5",
        ),
        (
            json!({"type": "symmetric", "distance": 4}),
            json!({"thin": {"location": "side1", "thickness": 2,
                            "side2": {"location": "center", "thickness": 1}}}),
            "distance 4 taper 0 thin Side1 2; distance 4 taper 0 thin Center 1",
        ),
    ];
    for (extent, extra, expected) in cases {
        let (spec, status) = extrude_with(&mut b, extent.clone(), extra);
        assert!(spec.contains(expected), "{extent}: {spec}");
        assert_eq!(status, FeatureStatus::Ok, "{extent}");
    }
    // A side of nothing leaves one side.
    let (spec, _) = extrude_with(
        &mut b,
        json!({"type": "two_sides", "side1": {"type": "distance", "distance": 0},
               "side2": {"type": "distance", "distance": 5}}),
        json!({}),
    );
    assert!(
        spec.ends_with("along [-0.0, -0.0, -1.0] from offset 0: distance 5 taper 0"),
        "{spec}"
    );
}

#[test]
fn extrude_values_get_parameters_and_checks() {
    let mut b = block();
    let added = b
        .doc
        .add_feature(
            &extrude_def(
                json!({"type": "distance", "distance": 10, "taper": 0.1}),
                json!({"start": {"type": "offset", "offset": 3},
                       "thin": {"location": "center", "thickness": 2}}),
            ),
            None,
        )
        .unwrap();
    let comments: Vec<String> = added
        .parameters
        .iter()
        .map(|name| {
            let id = b.doc.parameters().find(name).unwrap();
            b.doc.parameters().get(id).unwrap().comment().to_owned()
        })
        .collect();
    assert_eq!(
        comments,
        [
            "Extrude2 offset",
            "Extrude2 distance",
            "Extrude2 taper",
            "Extrude2 thickness"
        ]
    );
    let rejected = [
        (
            extrude_def(
                json!({"type": "two_sides", "side1": {"type": "distance", "distance": -1},
                       "side2": {"type": "distance", "distance": 5}}),
                json!({}),
            ),
            "must not be negative",
        ),
        (
            extrude_def(
                json!({"type": "distance", "distance": 5}),
                json!({"thin": {"location": "center", "thickness": 0}}),
            ),
            "wall thickness must be greater than zero",
        ),
        (
            extrude_def(
                json!({"type": "distance", "distance": 5}),
                json!({"thin": {"location": "center", "thickness": 1,
                                "side2": {"location": "side1", "thickness": 1}}}),
            ),
            "only an extrusion to two sides has a second wall",
        ),
        (
            extrude_def(
                json!({"type": "distance", "distance": 5}),
                json!({"start": {"type": "object", "object": {"type": "body", "body": "F2.b0"}}}),
            ),
            "starts from a plane or a planar face",
        ),
        (
            extrude_def(
                json!({"type": "to_object", "object": {"type": "face", "body": "F2.b0",
                       "face": "F9:end(r{c1})"}}),
                json!({}),
            ),
            "feature F9 does not exist",
        ),
    ];
    for (definition, expected) in rejected {
        let error = b
            .doc
            .add_feature(&definition, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}

#[test]
fn extrusions_to_an_object_depend_on_its_body() {
    let mut b = block();
    let uid = b
        .doc
        .add_feature(
            &extrude_def(
                json!({"type": "to_object", "object": {"type": "face", "body": "F2.b0", "face": TOP}}),
                json!({}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    match b.doc.delete_feature(b.extrude, false) {
        Err(ModelError::Dependents { dependents, .. }) => {
            assert_eq!(dependents, vec!["Extrude2".to_owned()]);
        }
        other => panic!("{other:?}"),
    }
    // A missing face fails the feature.
    let mut edited = extrude_def(
        json!({"type": "to_object", "object": {"type": "face", "body": "F2.b0",
               "face": "F2:end(r{c9})"}}),
        json!({}),
    );
    b.doc.edit_feature(uid, &edited).unwrap();
    assert_eq!(
        status(&b.doc, uid),
        FeatureStatus::Failed("body F2.b0 has no face F2:end(r{c9})".to_owned())
    );
    // A sketch is not a plane to extrude to.
    edited = extrude_def(
        json!({"type": "to_object", "object": {"type": "plane", "plane": "F1"}}),
        json!({}),
    );
    let error = b.doc.edit_feature(uid, &edited).unwrap_err().to_string();
    assert!(error.contains("is not construction geometry"), "{error}");

    // A construction plane before the extrusion.
    let plane = b
        .doc
        .add_feature(
            &def(json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": "xy", "distance": 30}})),
            None,
        )
        .unwrap()
        .uid;
    let uid = b
        .doc
        .add_feature(
            &extrude_def(
                json!({"type": "to_object", "object": {"type": "plane", "plane": plane.to_string()}}),
                json!({}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("plane [0.0, 0.0, 30.0] [0.0, 0.0, 1.0]"),
        "{spec}"
    );
    match b.doc.delete_feature(plane, false) {
        Err(ModelError::Dependents { dependents, .. }) => {
            assert_eq!(dependents, vec!["Extrude3".to_owned()]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn new_component_operations_make_components() {
    let mut b = block();
    b.doc
        .add_feature(
            &extrude_def(
                json!({"type": "distance", "distance": 5}),
                json!({"operation": "new_component"}),
            ),
            None,
        )
        .unwrap();
    let bodies = query(&b.doc, json!({"query": "bodies"}));
    assert_eq!(
        bodies,
        json!([{"uid": "F2.b0", "name": "Body1"},
               {"uid": "F3.b0", "name": "Body2", "component": "C1"}])
    );
    let components = query(&b.doc, json!({"query": "components"}));
    assert_eq!(components["components"][1]["name"], "Component1");
    assert_eq!(components["components"][1]["created_by"], "F3");
    assert_eq!(components["occurrences"][0]["name"], "Component1:1");
}

#[test]
fn revolutions_take_axes_and_angles() {
    let mut b = block();
    let revolve = |axis: Value, extent: Value| {
        def(
            json!({"type": "revolve", "profiles": [{"sketch": "F1", "region": RECT}],
                   "axis": axis, "extent": extent, "operation": "new_body"}),
        )
    };
    let cases = [
        (
            json!("y"),
            json!({"type": "full"}),
            "axis [0.0, 0.0, 0.0] [0.0, 1.0, 0.0] angles 6.283185307179586 None",
        ),
        // Line c4 of the rectangle runs from (0, 40) down to (0, 0).
        (
            json!({"sketch": "F1", "curve": "c4"}),
            json!({"type": "symmetric", "angle": 0.5}),
            "axis [0.0, 40.0, 0.0] [0.0, -1.0, 0.0] angles 0.5 Some(0.5)",
        ),
        // A negative angle turns about the reversed axis.
        (
            json!("x"),
            json!({"type": "angle", "angle": -1}),
            "axis [0.0, 0.0, 0.0] [-1.0, -0.0, -0.0] angles 1 None",
        ),
        (
            json!({"body": "F2.b0", "edge": "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"}),
            json!({"type": "two_sides", "angle1": 0.25, "angle2": 0.5}),
            "axis [0.0, 0.0, 0.0] [1.0, 0.0, 0.0] angles 0.25 Some(0.5)",
        ),
        (
            json!("y"),
            json!({"type": "to_object", "object": {"type": "plane", "plane": "yz"}}),
            "target true",
        ),
    ];
    for (axis, extent, expected) in cases {
        let uid = b
            .doc
            .add_feature(&revolve(axis.clone(), extent), None)
            .unwrap()
            .uid;
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok, "{axis}");
        let spec = last_spec(&b.doc);
        assert!(spec.contains(expected), "{axis}: {spec}");
        assert!(
            b.doc.body_shape(BodyUid::new(uid, 0)).is_some(),
            "a revolution makes a body"
        );
        b.doc.undo();
    }
    for (extent, expected) in [
        (json!({"type": "angle", "angle": 0}), "must not be zero"),
        (
            json!({"type": "symmetric", "angle": 4}),
            "more than a full turn",
        ),
        (
            json!({"type": "two_sides", "angle1": -1, "angle2": 2}),
            "must not be negative",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&revolve(json!("y"), extent), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
    // A line the sketch does not have: references are checked when added.
    let error = b
        .doc
        .add_feature(
            &revolve(
                json!({"sketch": "F1", "curve": "c7"}),
                json!({"type": "full"}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Sketch1 has no curve c7"), "{error}");
    // Revolve profiles are consumed like extrude profiles.
    assert!(b.doc.profiles().iter().all(|p| p.consumed));
}

fn hole_def(placement: Value, extra: Value) -> FeatureDef<crate::ValueInput> {
    let mut value = json!({"type": "hole", "placement": placement, "diameter": 6,
                           "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

#[test]
fn holes_go_into_faces_and_sketch_points() {
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 50], [50, 30, 20]]});
    let uid = b
        .doc
        .add_feature(&hole_def(on_top.clone(), json!({})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // Points are put on the face's plane; the holes point into the body.
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains(
            "([10.0, 10.0, 20.0], [-0.0, -0.0, -1.0]), ([50.0, 30.0, 20.0], [-0.0, -0.0, -1.0])"
        ),
        "{spec}"
    );
    assert!(
        spec.contains("diameter 6.000000 Simple tip Some(\"2.0594885173533086\") depth 10"),
        "{spec}"
    );
    let shape = b.doc.body_shape(b.body).unwrap();
    assert_eq!(shape.history, "cut(prism(F2:0..20),hole(F3:2))");
    b.doc.undo();

    // Flat, counterbore, flipped, at offsets from two edges.
    let offsets = json!({"type": "face_offsets", "body": "F2.b0", "face": TOP,
        "edge1": "E{F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})|F2:side(c1[c4,c2])}", "offset1": 5,
        "edge2": "E{F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})|F2:side(c4[c3,c1])}", "offset2": 7});
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                offsets,
                json!({"flat": true, "flip": true,
                       "kind": {"type": "counterbore", "diameter": 10, "depth": 3}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    // The mock's edges all run along x from the body's corner, so both are
    // parallel: no point.
    assert_eq!(
        status(&b.doc, uid),
        FeatureStatus::Failed("the edges of the hole's position are parallel".to_owned())
    );
    b.doc.undo();

    // Sketch points: coordinates, a circle's centre and a sketch point (the
    // circle's centre point p2), against the normal.
    let sketch = b
        .doc
        .add_sketch(crate::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    b.doc
        .add_circle(sketch, [30.0, 20.0], &crate::ValueInput::Number(3.0))
        .unwrap();
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                json!({"type": "sketch_points", "sketch": sketch, "points": [[5, 5], "c1", "p2"]}),
                json!({"extent": {"type": "through_all"}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains(
            "([5.0, 5.0, 0.0], [-0.0, -0.0, -1.0]), ([30.0, 20.0, 0.0], [-0.0, -0.0, -1.0]), \
             ([30.0, 20.0, 0.0], [-0.0, -0.0, -1.0])"
        ),
        "{spec}"
    );
    assert!(spec.ends_with("through 1 bodies"), "{spec}");
    b.doc.undo();

    // A tapered, counterdrilled hole (FreeCAD's, mitcad#4): the taper and
    // the counterdrill's sizes reach the kernel; the definition keeps them.
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                on_top,
                json!({"flat": true, "taper": 0.1,
                       "kind": {"type": "counterdrill", "diameter": 10, "depth": 3, "angle": 1.5}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains(
            "Counterdrill { diameter: 10.0, depth: 3.0, angle: 1.5 } tip None depth 10 taper 0.1"
        ),
        "{spec}"
    );
    let def = query(&b.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def["kind"]["type"], "counterdrill");
    assert!(def["taper"].is_string(), "{def}");
}

#[test]
fn hole_values_are_checked() {
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    for (extra, expected) in [
        (
            json!({"kind": {"type": "counterbore", "diameter": 5, "depth": 2}}),
            "counterbore must be wider than the hole",
        ),
        (
            json!({"kind": {"type": "countersink", "diameter": 12, "angle": 4}}),
            "countersink angle must be between 0 and 180 degrees",
        ),
        (json!({"tip_angle": 0}), "drill point angle"),
        (
            json!({"kind": {"type": "counterdrill", "diameter": 10, "depth": 3, "angle": 0}}),
            "counterdrill angle must be between 0 and 180 degrees",
        ),
        (
            json!({"kind": {"type": "counterdrill", "diameter": 4, "depth": 3, "angle": 1}}),
            "counterdrill must be wider than the hole",
        ),
        (json!({"taper": 2}), "taper must be less than 90 degrees"),
        (
            json!({"taper": 0.1, "thread": {"designation": "M6x1"}}),
            "a tapered hole has no thread",
        ),
        (
            json!({"thread": {"designation": "M7x3"}}),
            "no ISO metric thread M7x3",
        ),
        (
            json!({"thread": {"designation": "M6x1", "class": "6g"}}),
            "class 6g is for external threads",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&hole_def(on_top.clone(), extra), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
    let error = b
        .doc
        .add_feature(
            &hole_def(
                json!({"type": "face", "body": "F2.b0", "face": TOP, "points": []}),
                json!({}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("no position"), "{error}");
}

#[test]
fn tapped_holes_take_the_thread_size_and_list_their_threads() {
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                on_top.clone(),
                json!({"thread": {"designation": "M6x1", "class": "6H"}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // The bore is the minor diameter; a cosmetic thread changes nothing more.
    assert!(
        last_spec(&b.doc).contains("diameter 4.917468"),
        "{}",
        last_spec(&b.doc)
    );
    assert_eq!(b.doc.kernel().count("thread"), 0);
    let threads = query(&b.doc, json!({"query": "threads"}));
    assert_eq!(threads.as_array().unwrap().len(), 1);
    let thread = &threads[0];
    assert_eq!(thread["feature"], "F3");
    assert_eq!(thread["face"], "F3:hole0.wall");
    assert_eq!(thread["designation"], "M6x1");
    assert_eq!(thread["internal"], true);
    assert_eq!(thread["modeled"], false);
    b.doc.undo();

    // Modelled: cut into the wall, 8 long from 1 below the start (the high
    // end of the mock wall's axis, which points up).
    b.doc
        .add_feature(
            &hole_def(
                on_top,
                json!({"thread": {"designation": "M6x1", "modeled": true, "length": 8, "offset": 1}}),
            ),
            None,
        )
        .unwrap();
    assert_eq!(b.doc.kernel().count("thread"), 1);
    assert!(
        last_spec(&b.doc).starts_with(
            "thread F3:hole0.wall pitch 1 depth 0.541266 right true part Some((8.0, 1.0, true))"
        ),
        "{}",
        last_spec(&b.doc)
    );
}

fn thread_def(face: &str, extra: Value) -> FeatureDef<crate::ValueInput> {
    let mut value = json!({"type": "thread", "faces": [{"body": "F2.b0", "face": face}],
                           "thread": {"designation": "M10x1.5", "class": "6g"}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// Sketch1 with a 10 mm circle extruded 20 mm into a rod (F2.b0).
fn rod() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc
        .add_sketch(crate::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_circle(sketch, [0.0, 0.0], &crate::ValueInput::Number(10.0))
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    doc
}

#[test]
fn cosmetic_threads_are_data_and_modelled_ones_geometry() {
    let mut doc = rod();
    let uid = doc
        .add_feature(
            &thread_def("F2:side(c1)", json!({"length": 12, "offset": 2})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.kernel().count("thread"), 0);
    assert_eq!(
        doc.body_shape(BodyUid::new(FeatureUid(2), 0))
            .unwrap()
            .history,
        "prism(F2:0..20)"
    );
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(
        threads,
        json!([{"feature": "F3", "name": "Thread1", "body": "F2.b0", "face": "F2:side(c1)",
                "standard": "iso_metric", "designation": "M10x1.5", "class": "6g",
                "right_handed": true, "modeled": false, "major_diameter": 10.0,
                "minor_diameter": 8.376202367904177, "pitch": 1.5, "internal": false,
                "radius": 5.0, "start": [0.0, 0.0, 6.0], "end": [0.0, 0.0, 18.0]}])
    );
    // Suppressed, it is no thread.
    doc.set_suppressed(uid, true).unwrap();
    assert_eq!(query(&doc, json!({"query": "threads"})), json!([]));
    doc.undo();

    // Modelled.
    let modeled = thread_def("F2:side(c1)", json!({"modeled": true}));
    doc.edit_feature(uid, &modeled).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 1.5 depth 0.811899 right true part None"
    );
    assert_eq!(
        doc.body_shape(BodyUid::new(FeatureUid(2), 0))
            .unwrap()
            .history,
        "thread(prism(F2:0..20),1.5)"
    );

    // Not a cylinder; an internal class on an external face.
    doc.edit_feature(uid, &thread_def("F2:end(r{c1})", json!({})))
        .unwrap();
    assert_eq!(
        status(&doc, uid),
        FeatureStatus::Failed("face F2:end(r{c1}) is not cylindrical".to_owned())
    );
    let internal_class = def(json!({"type": "thread",
        "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
        "thread": {"designation": "M10x1.5", "class": "6H"}}));
    doc.edit_feature(uid, &internal_class).unwrap();
    assert_eq!(
        status(&doc, uid),
        FeatureStatus::Failed("class 6H is for internal threads".to_owned())
    );
    // A modelled thread too big for the rod.
    let too_big = def(
        json!({"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
        "thread": {"designation": "M20x2.5"}, "modeled": true}),
    );
    doc.edit_feature(uid, &too_big).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("too big");
    };
    assert!(message.contains("does not fit"), "{message}");
    // Values and references are checked.
    let error = doc
        .add_feature(&thread_def("F2:side(c1)", json!({"length": 0})), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("length must be greater than zero"),
        "{error}"
    );
    let error = doc
        .add_feature(&thread_def("F2:side(c1)", json!({"offset": 1})), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("has no offset"), "{error}");
}

#[test]
fn profile_features_round_trip_through_the_project_file() {
    let mut b = block();
    let commands = [
        extrude_def(
            json!({"type": "two_sides", "side1": {"type": "distance", "distance": 5, "taper": 0.1},
                   "side2": {"type": "to_object", "object": {"type": "plane", "plane": "xy"}, "offset": 1}}),
            json!({"start": {"type": "offset", "offset": 2},
                   "thin": {"location": "side2", "thickness": 1}, "operation": "new_component"}),
        ),
        def(
            json!({"type": "revolve", "profiles": [{"sketch": "F1", "region": RECT}],
                   "axis": {"sketch": "F1", "curve": "c4"}, "project_axis": true,
                   "extent": {"type": "two_sides", "angle1": 1, "angle2": 0.5}, "operation": "new_body"}),
        ),
        hole_def(
            json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]}),
            json!({"kind": {"type": "countersink", "diameter": 12, "angle": 1.5}, "tip_angle": 2,
                   "thread": {"designation": "M6x1", "class": "6H", "modeled": true, "length": 5}}),
        ),
    ];
    for command in &commands {
        b.doc.add_feature(command, None).unwrap();
    }
    let text = b.doc.to_json();
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    let defs: Vec<_> = loaded.features().map(|f| f.def.clone()).collect();
    let original: Vec<_> = b.doc.features().map(|f| f.def.clone()).collect();
    assert_eq!(defs, original);
}
