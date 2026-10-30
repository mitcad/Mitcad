// SPDX-License-Identifier: MIT
//! The profile features (extrude options, revolve, hole, thread) with the
//! mock kernel: what they ask the kernel for, their parameters, checks,
//! errors, references and the queries about them.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::{Block, block, def};
use mitcad_model::testing::MockKernel;
use mitcad_model::{FeatureStatus, ModelError};
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

fn query(doc: &Document<MockKernel>, query: Value) -> Value {
    serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap()
}

fn extrude_def(extent: Value, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
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
    // The sign of a two-sided extrusion's distance does not turn its side:
    // each goes its own way by the size.
    for (side1, side2, flip, expected) in [
        (
            json!({"type": "distance", "distance": 15}),
            json!({"type": "distance", "distance": -4}),
            false,
            "along [0.0, 0.0, 1.0] from offset 0: distance 15 taper 0; distance 4 taper 0",
        ),
        (
            json!({"type": "distance", "distance": -4}),
            json!({"type": "distance", "distance": -15}),
            true,
            "along [-0.0, -0.0, -1.0] from offset 0: distance 4 taper 0; distance 15 taper 0",
        ),
    ] {
        let (spec, status) = extrude_with(
            &mut b,
            json!({"type": "two_sides", "side1": side1, "side2": side2}),
            json!({"flip": flip}),
        );
        assert!(spec.ends_with(expected), "{side1} {side2} {flip}: {spec}");
        assert_eq!(status, FeatureStatus::Ok);
    }
    // Through all one way with a negative distance the other way.
    let (spec, status) = extrude_with(
        &mut b,
        json!({"type": "two_sides", "side1": {"type": "through_all"},
               "side2": {"type": "distance", "distance": -8}}),
        json!({}),
    );
    assert!(
        spec.contains("distance 26 taper 0; distance 8 taper 0"),
        "{spec}"
    );
    assert_eq!(status, FeatureStatus::Ok);
}

/// A negative distance of a two-sided extrusion without `signed`
/// (mitcad#693): commands and their previews refuse it, whether a number,
/// an expression or a parameter gives it; the document's own API, which
/// the imports add through, still takes its size, and an edit that keeps
/// such a distance passes. With `signed` (mitcad#765) see
/// `signed_two_sides_lie_between_their_ends`.
#[test]
fn commands_refuse_a_negative_two_sided_distance() {
    let mut b = block();
    let two_sides = |d1: Value, d2: Value| {
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
               "extent": {"type": "two_sides", "side1": {"type": "distance", "distance": d1},
                          "side2": {"type": "distance", "distance": d2}},
               "operation": "new_body"})
    };
    b.doc
        .command(&json!({"cmd": "add_parameter", "name": "gap", "value": "-3 mm"}).to_string())
        .unwrap();
    let depth = b.doc.undo_depth();
    let features = b.doc.features().count();
    for (d1, d2, expected) in [
        (json!(15), json!(-4), "side 2's distance is -4"),
        (json!(15), json!("-4 mm"), "side 2's distance is -4"),
        (json!(15), json!("2 mm - 6 mm"), "side 2's distance is -4"),
        (json!(15), json!("depth = -4 mm"), "side 2's distance is -4"),
        (json!(15), json!("gap"), "side 2's distance is -3"),
        (json!(-15), json!(4), "side 1's distance is -15"),
    ] {
        let definition = two_sides(d1.clone(), d2.clone());
        let command = json!({"cmd": "add_feature", "def": definition}).to_string();
        for (what, result) in [
            ("command", b.doc.command(&command)),
            ("preview", b.doc.preview(&command)),
        ] {
            let error = result.unwrap_err().0;
            assert!(
                error.contains(expected) && error.contains("\"signed\": true"),
                "{what} {d1} {d2}: {error}"
            );
        }
    }
    assert_eq!(b.doc.undo_depth(), depth, "nothing was added");
    assert_eq!(b.doc.features().count(), features);
    // Zero and positive distances pass.
    let command = json!({"cmd": "add_feature", "def": two_sides(json!(0), json!(4))});
    b.doc.command(&command.to_string()).unwrap();
    b.doc.undo();
    // The document's own API (the imports') takes the size: 4 the other way.
    let uid = b
        .doc
        .add_feature(&def(two_sides(json!(15), json!(-4))), None)
        .unwrap()
        .uid;
    let spec = last_spec(&b.doc);
    assert!(
        spec.ends_with("distance 15 taper 0; distance 4 taper 0"),
        "{spec}"
    );
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // An edit that keeps that negative distance passes, one to another
    // negative distance is refused, a positive one passes.
    let edit = |d2: Value| {
        json!({"cmd": "edit_feature", "uid": uid.to_string(), "def": two_sides(json!(20), d2)})
            .to_string()
    };
    b.doc.command(&edit(json!(-4))).unwrap();
    let spec = last_spec(&b.doc);
    assert!(
        spec.ends_with("distance 20 taper 0; distance 4 taper 0"),
        "{spec}"
    );
    let error = b.doc.command(&edit(json!(-6))).unwrap_err().0;
    assert!(error.contains("side 2's distance is -6"), "{error}");
    b.doc.command(&edit(json!(6))).unwrap();
    let spec = last_spec(&b.doc);
    assert!(
        spec.ends_with("distance 20 taper 0; distance 6 taper 0"),
        "{spec}"
    );
}

/// Two sides with signed distances (mitcad#765): a negative distance ends
/// on the other side's side and the solid lies between the two ends only
/// (50 and -30: from 30 to 50), mirrored for a negative first side; ends
/// that meet or cross are refused; commands and previews take it; Join and
/// Cut use the solid off the sketch plane; the flag survives the design
/// file and undo. Without the flag (designs saved before, imports) each
/// side still goes its own way by its size.
#[test]
fn signed_two_sides_lie_between_their_ends() {
    let mut b = block();
    let two_sides = |d1: Value, d2: Value, signed: bool| {
        let mut extent = json!({"type": "two_sides",
                                "side1": {"type": "distance", "distance": d1},
                                "side2": {"type": "distance", "distance": d2}});
        if signed {
            extent["signed"] = json!(true);
        }
        extent
    };
    let heights = |doc: &Document<MockKernel>, body: BodyUid| {
        let bounds = doc.body_shape(body).unwrap().bounds.unwrap();
        (bounds.min[2], bounds.max[2])
    };
    // 50 and -30: from 30 to 50 above the sketch, both ends past the
    // block's top (20).
    let depth = b.doc.undo_depth();
    let command = json!({"cmd": "add_feature",
                         "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                                 "extent": two_sides(json!(50), json!("-30 mm"), true),
                                 "operation": "new_body"}})
    .to_string();
    b.doc.preview(&command).unwrap();
    let reply: Value = serde_json::from_str(&b.doc.command(&command).unwrap()).unwrap();
    assert_eq!(reply["uid"], json!("F3"), "{reply}");
    let uid = FeatureUid(3);
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.ends_with(
            "along [0.0, 0.0, 1.0] from offset 0: distance 50 taper 0; distance -30 taper 0"
        ),
        "{spec}"
    );
    let body = BodyUid::new(uid, 0);
    assert_eq!(heights(&b.doc, body), (30.0, 50.0));
    // The design file keeps the flag; undo takes the feature away.
    let text = b.doc.to_json();
    assert!(text.contains("\"signed\""), "{text}");
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    match &loaded.feature(uid).unwrap().def {
        FeatureDef::Extrude(extrude) => assert!(extrude.signed_two_sides()),
        other => panic!("expected an extrusion, got {other:?}"),
    }
    b.doc.undo();
    assert_eq!(b.doc.undo_depth(), depth);
    assert!(b.doc.body_shape(body).is_none());

    let cases = [
        // Mirrored: -30 and 50 lie 30 to 50 below the sketch.
        (json!(-30), json!(50), json!({}), (-50.0, -30.0)),
        // Flipped: side one goes down.
        (json!(50), json!(-30), json!({"flip": true}), (-50.0, -30.0)),
        // From a start offset: measured from there.
        (
            json!(10),
            json!(-4),
            json!({"start": {"type": "offset", "offset": 5}}),
            (9.0, 15.0),
        ),
    ];
    for (d1, d2, extra, expected) in cases {
        let uid = b
            .doc
            .add_feature(
                &extrude_def(two_sides(d1.clone(), d2.clone(), true), extra.clone()),
                None,
            )
            .unwrap()
            .uid;
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok, "{d1} {d2} {extra}");
        assert_eq!(
            heights(&b.doc, BodyUid::new(uid, 0)),
            expected,
            "{d1} {d2} {extra}"
        );
        b.doc.undo();
    }

    // Each side's taper reaches the kernel with the signed distance; the
    // kernel tapers the side that holds the solid from the start.
    let mut tapered = two_sides(json!(50), json!(-30), true);
    tapered["side1"]["taper"] = json!(0.1);
    tapered["side2"]["taper"] = json!(0.2);
    let (spec, status_tapered) = extrude_with(&mut b, tapered, json!({}));
    assert!(
        spec.ends_with("distance 50 taper 0.1; distance -30 taper 0.2"),
        "{spec}"
    );
    assert_eq!(status_tapered, FeatureStatus::Ok);
    // Beside a side through all, the negative side's end goes to the kernel.
    let mut through = two_sides(json!(0), json!(-8), true);
    through["side1"] = json!({"type": "through_all"});
    let (spec, _) = extrude_with(&mut b, through, json!({}));
    assert!(
        spec.ends_with("distance 26 taper 0; distance -8 taper 0"),
        "{spec}"
    );

    // Ends that meet or cross leave nothing: refused, by the command too.
    for (d1, d2) in [(50.0, -50.0), (-30.0, 10.0), (-30.0, 0.0), (0.0, -5.0)] {
        let definition = extrude_def(two_sides(json!(d1), json!(d2), true), json!({}));
        let error = b
            .doc
            .add_feature(&definition, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("the two sides overlap"),
            "{d1} {d2}: {error}"
        );
    }
    let command = json!({"cmd": "add_feature",
                         "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                                 "extent": two_sides(json!(50), json!(-50), true),
                                 "operation": "new_body"}})
    .to_string();
    let error = b.doc.command(&command).unwrap_err().0;
    assert!(
        error.contains("the two sides overlap: side 1 ends at 50 and side 2 at 50"),
        "{error}"
    );
    // A parameter change that makes them cross fails the feature.
    let uid = b
        .doc
        .add_feature(
            &extrude_def(
                two_sides(json!("top_end = 50 mm"), json!(-30), true),
                json!({}),
            ),
            None,
        )
        .unwrap()
        .uid;
    b.doc.set_parameter("top_end", 20.0).unwrap();
    match status(&b.doc, uid) {
        FeatureStatus::Failed(message) => {
            assert!(message.contains("the two sides overlap"), "{message}")
        }
        other => panic!("expected a failure, got {other:?}"),
    }
    b.doc.undo();
    b.doc.undo();

    // Join and Cut take the solid off the sketch plane as it is: a cut from
    // 5 to 15 inside the block, a join from 15 to 25 across its top.
    for (operation, d1, d2, expected) in [
        ("cut", 15, -5, "cut(prism(F2:0..20),prism({uid}:-15..-5))"),
        (
            "join",
            25,
            -15,
            "join(prism(F2:0..20),prism({uid}:-25..-15))",
        ),
    ] {
        let uid = b
            .doc
            .add_feature(
                &extrude_def(
                    two_sides(json!(d1), json!(d2), true),
                    json!({"operation": operation}),
                ),
                None,
            )
            .unwrap()
            .uid;
        assert_eq!(status(&b.doc, uid), FeatureStatus::Ok, "{operation}");
        let history = b.doc.body_shape(b.body).unwrap().history.clone();
        assert_eq!(history, expected.replace("{uid}", &uid.to_string()));
        b.doc.undo();
    }

    // Without the flag the sizes count, as saved designs and the imports
    // have them: 50 up and 30 down.
    let uid = b
        .doc
        .add_feature(
            &extrude_def(two_sides(json!(50), json!(-30), false), json!({})),
            None,
        )
        .unwrap()
        .uid;
    let spec = last_spec(&b.doc);
    assert!(
        spec.ends_with("distance 50 taper 0; distance 30 taper 0"),
        "{spec}"
    );
    assert_eq!(heights(&b.doc, BodyUid::new(uid, 0)), (-30.0, 50.0));
    let text = b.doc.to_json();
    assert!(!text.contains("\"signed\""), "{text}");
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
                json!({"type": "two_sides", "side1": {"type": "distance", "distance": 0},
                       "side2": {"type": "distance", "distance": 0}}),
                json!({}),
            ),
            "add up to more than zero",
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
        // Side two ending past the profile (mitcad#955): the profile turned
        // there, the rest one side.
        (
            json!("y"),
            json!({"type": "two_sides", "angle1": 0.75, "angle2": -0.25}),
            "axis [0.0, 0.0, 0.0] [0.0, 1.0, 0.0] angles 0.5 None",
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
        (
            json!({"type": "two_sides", "angle1": 0.25, "angle2": -0.5}),
            "add up to more than zero",
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

fn hole_def(placement: Value, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "hole", "placement": placement, "diameter": 6,
                           "extent": {"type": "distance", "depth": 10}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

#[test]
fn holes_start_on_a_plane() {
    // A hole placed on an origin plane (mitcad#25): the point is put on the
    // plane, and the hole goes against the plane's normal, or with it
    // flipped.
    let mut b = block();
    let on_plane = json!({"type": "plane", "plane": "xy", "points": [[10, 20, 5]]});
    let uid = b
        .doc
        .add_feature(&hole_def(on_plane.clone(), json!({})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("([10.0, 20.0, 0.0], [-0.0, -0.0, -1.0])"),
        "{spec}"
    );
    b.doc.undo();
    let uid = b
        .doc
        .add_feature(&hole_def(on_plane, json!({"flip": true})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(
        spec.contains("([10.0, 20.0, 0.0], [0.0, 0.0, 1.0])"),
        "{spec}"
    );
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
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    b.doc
        .add_circle(sketch, [30.0, 20.0], &mitcad_model::ValueInput::Number(3.0))
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
            "thread F3:hole0.wall pitch 1 diameters 6.0000/4.9175/5.3505 angle 0.000000 right true part Some((8.0, 1.0, true))"
        ),
        "{}",
        last_spec(&b.doc)
    );
}

fn thread_def(face: &str, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "thread", "faces": [{"body": "F2.b0", "face": face}],
                           "thread": {"designation": "M10x1.5", "class": "6g"}});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

#[test]
fn trapezoidal_threads_are_listed_and_modelled() {
    // A lead screw's thread (mitcad#449): Tr10x2 on the rod, listed by the
    // threads query.
    let mut doc = rod();
    let tr = json!({"standard": "trapezoidal", "designation": "TR10x2", "class": "7e"});
    let uid = doc
        .add_feature(&thread_def("F2:side(c1)", json!({"thread": tr})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads.as_array().unwrap().len(), 1, "{threads}");
    let thread = &threads[0];
    for (key, wanted) in [
        ("feature", json!("F3")),
        ("face", json!("F2:side(c1)")),
        ("standard", json!("trapezoidal")),
        ("designation", json!("Tr10x2")),
        ("class", json!("7e")),
        ("modeled", json!(false)),
        ("major_diameter", json!(10.0)),
        ("minor_diameter", json!(8.0)),
        ("pitch", json!(2.0)),
        ("internal", json!(false)),
        ("radius", json!(5.0)),
    ] {
        assert_eq!(thread[key], wanted, "{key}: {thread}");
    }
    // A single-start thread lists no starts.
    assert!(thread.get("starts").is_none(), "{thread}");
    // A multi-start one by its lead and pitch.
    let multi = json!({"standard": "trapezoidal", "designation": "Tr10x4P2"});
    doc.edit_feature(uid, &thread_def("F2:side(c1)", json!({"thread": multi})))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads[0]["designation"], "Tr10x4(P2)");
    assert_eq!(threads[0]["pitch"], 2.0);
    assert_eq!(threads[0]["starts"], 2, "{threads}");
    assert_eq!(threads[0]["lead"], 4.0, "{threads}");
    // Modelled (mitcad#893): ISO 2901's 30-degree profile, flanks at 15
    // degrees to the radius, the external thread's root d3 = d - 2 (H1 +
    // ac) = 10 - 2 (1 + 0.25) = 7.5, the pitch diameter 9.
    let modeled = thread_def("F2:side(c1)", json!({"thread": tr, "modeled": true}));
    doc.edit_feature(uid, &modeled).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 2 diameters 10.0000/7.5000/9.0000 angle 0.000000 right true \
         part None flanks 15.0000 deg"
    );
    // Tr10x4(P2): the same profile on two starts, a lead of 4.
    let lead_screw = thread_def("F2:side(c1)", json!({"thread": multi, "modeled": true}));
    doc.edit_feature(uid, &lead_screw).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 2 diameters 10.0000/7.5000/9.0000 angle 0.000000 right true \
         part None starts 2 flanks 15.0000 deg"
    );
    // Its designation gives its starts: `starts` may repeat them, not
    // change them.
    let same = thread_def(
        "F2:side(c1)",
        json!({"thread": multi, "modeled": true, "starts": 2}),
    );
    doc.edit_feature(uid, &same).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let other = thread_def(
        "F2:side(c1)",
        json!({"thread": multi, "modeled": true, "starts": 3}),
    );
    let error = doc.edit_feature(uid, &other).unwrap_err().to_string();
    assert!(error.contains("Tr10x4(P2) has 2 starts"), "{error}");
    // A trapezoidal flank clearance is measured on the 15-degree flanks:
    // the crest, 0.366 P = 0.732 wide, takes up to (0.732 - 0.04) cos 15 /
    // 2 = 0.3342 per flank.
    let loose = thread_def(
        "F2:side(c1)",
        json!({"thread": tr, "modeled": true, "flank_clearance": 0.3}),
    );
    doc.edit_feature(uid, &loose).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let too_loose = thread_def(
        "F2:side(c1)",
        json!({"thread": tr, "modeled": true, "flank_clearance": 0.34}),
    );
    doc.edit_feature(uid, &too_loose).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("no tooth left");
    };
    assert!(message.contains("at most 0.3342 mm"), "{message}");
}

#[test]
fn custom_threads_take_their_own_diameter_and_pitch() {
    // A custom thread (mitcad#893): 10 x 2.5, a pitch no ISO metric thread
    // of 10 mm has, on the 60-degree profile: D1 = D - 5H/4 = 7.2937, D2 =
    // D - 3H/4 = 8.3762.
    let mut doc = rod();
    let custom = json!({"standard": "custom"});
    let uid = doc
        .add_feature(
            &thread_def(
                "F2:side(c1)",
                json!({"thread": custom, "custom": {"major": 10, "pitch": 2.5}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // Cosmetic: listed with its size.
    assert_eq!(doc.kernel().count("thread"), 0);
    let threads = query(&doc, json!({"query": "threads"}));
    let thread = &threads[0];
    for (key, wanted) in [
        ("standard", json!("custom")),
        ("designation", json!("10x2.5")),
        ("modeled", json!(false)),
        ("major_diameter", json!(10.0)),
        ("pitch", json!(2.5)),
        ("internal", json!(false)),
    ] {
        assert_eq!(thread[key], wanted, "{key}: {thread}");
    }
    assert!(
        (thread["minor_diameter"].as_f64().unwrap() - 7.293_670_6).abs() < 1e-6,
        "{thread}"
    );
    assert!(thread["class"].is_null(), "{thread}");

    // Modelled, on two starts: the lead is 5.
    let modeled = thread_def(
        "F2:side(c1)",
        json!({"thread": custom, "custom": {"major": 10, "pitch": 2.5}, "modeled": true,
               "starts": 2}),
    );
    doc.edit_feature(uid, &modeled).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 2.5 diameters 10.0000/7.2937/8.3762 angle 0.000000 right true \
         part None starts 2"
    );
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads[0]["starts"], 2, "{threads}");
    assert_eq!(threads[0]["lead"], 5.0, "{threads}");

    // Saved with the feature (no designation, its size and starts) and
    // loaded again as it was.
    let text = doc.to_json();
    assert!(text.contains("\"custom\""), "{text}");
    let FeatureDef::Thread(saved) = &doc.feature(uid).unwrap().def else {
        panic!("a thread");
    };
    assert!(saved.thread.designation.is_empty());
    assert!(saved.custom.is_some() && saved.starts.is_some());
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    assert_eq!(
        loaded.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );

    // Its size by parameters, left-handed: the thread follows them.
    doc.add_parameter("thread_d", 10.0, "").unwrap();
    doc.add_parameter("thread_p", 2.5, "").unwrap();
    let by_name = thread_def(
        "F2:side(c1)",
        json!({"thread": {"standard": "custom", "right_handed": false},
               "custom": {"major": "thread_d", "pitch": "thread_p"}, "modeled": true}),
    );
    doc.edit_feature(uid, &by_name).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).starts_with(
            "thread F2:side(c1) pitch 2.5 diameters 10.0000/7.2937/8.3762 angle 0.000000 right \
             false part None"
        ),
        "{}",
        last_spec(&doc)
    );
    doc.set_parameter("thread_p", 3.0).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).starts_with(
            "thread F2:side(c1) pitch 3 diameters 10.0000/6.7524/8.0514 angle 0.000000 right \
             false"
        ),
        "{}",
        last_spec(&doc)
    );
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads[0]["designation"], "10x3", "{threads}");
    // A pitch too coarse for the diameter fails the feature.
    doc.set_parameter("thread_p", 11.0).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("too coarse");
    };
    assert!(message.contains("too coarse"), "{message}");
    doc.set_parameter("thread_p", 2.5).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);

    // A standard size may take starts too: M10x1.5 on two starts.
    let two_start = thread_def("F2:side(c1)", json!({"modeled": true, "starts": 2}));
    doc.edit_feature(uid, &two_start).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 1.5 diameters 10.0000/8.3762/9.0257 angle 0.000000 right true \
         part None starts 2"
    );

    // What a custom thread must and must not have.
    for (extra, wanted) in [
        (json!({"thread": custom}), "needs its size"),
        (
            json!({"thread": {"standard": "custom", "class": "6g"},
                   "custom": {"major": 10, "pitch": 2.5}}),
            "no tolerance class",
        ),
        (
            json!({"custom": {"major": 10, "pitch": 2.5}}),
            "only a custom thread",
        ),
        (
            json!({"thread": custom, "custom": {"major": 10, "pitch": 9.5}}),
            "too coarse",
        ),
        (
            json!({"thread": custom, "custom": {"major": 10, "pitch": 0}}),
            "pitch must be greater than zero",
        ),
        (json!({"modeled": true, "starts": 0}), "whole number"),
        (json!({"modeled": true, "starts": 1.5}), "whole number"),
    ] {
        let error = doc
            .edit_feature(uid, &thread_def("F2:side(c1)", extra.clone()))
            .unwrap_err()
            .to_string();
        assert!(error.contains(wanted), "{extra}: {error}");
    }
}

#[test]
fn tapped_holes_take_custom_and_trapezoidal_threads() {
    // A hole tapped with a custom thread (mitcad#893), 8 x 2 on two starts:
    // its bore the minor diameter 8 - 5H/4 = 5.8349, the thread cut into
    // its wall.
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                on_top.clone(),
                json!({"thread": {"standard": "custom", "custom": {"major": 8, "pitch": 2},
                                  "modeled": true, "starts": 2}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("thread"), 1);
    let spec = last_spec(&b.doc);
    assert!(
        spec.starts_with(
            "thread F3:hole0.wall pitch 2 diameters 8.0000/5.8349/6.7010 angle 0.000000 right true"
        ),
        "{spec}"
    );
    assert!(spec.ends_with(" starts 2"), "{spec}");
    let threads = query(&b.doc, json!({"query": "threads"}));
    assert_eq!(threads[0]["designation"], "8x2", "{threads}");
    assert_eq!(threads[0]["standard"], "custom", "{threads}");
    assert_eq!(threads[0]["internal"], true, "{threads}");
    assert_eq!(threads[0]["starts"], 2, "{threads}");
    // Saved and loaded as it was.
    let text = b.doc.to_json();
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    b.doc.undo();

    // Tr10x2 in a hole: the bore D1 = 8, the internal design profile's root
    // D4 = d + 2 ac = 10.5, the flanks at 15 degrees.
    b.doc
        .add_feature(
            &hole_def(
                on_top,
                json!({"thread": {"standard": "trapezoidal", "designation": "Tr10x2",
                                  "class": "7H", "modeled": true}}),
            ),
            None,
        )
        .unwrap();
    let spec = last_spec(&b.doc);
    assert!(
        spec.starts_with(
            "thread F3:hole0.wall pitch 2 diameters 10.5000/8.0000/9.0000 angle 0.000000 right true"
        ),
        "{spec}"
    );
    assert!(spec.ends_with(" flanks 15.0000 deg"), "{spec}");
}

#[test]
fn taper_pipe_threads_taper_even_when_cosmetic() {
    // An NPT thread with `taper` on the rod (mitcad#733): a cosmetic one
    // cuts the cone of its major diameter (E0 + h at the free end), a
    // modelled one the profile; the kernel gets the start's diameters and
    // the taper.
    let mut doc = rod();
    let npt = json!({"standard": "npt", "designation": "1/8-27 NPT"});
    let uid = doc
        .add_feature(
            &thread_def("F2:side(c1)", json!({"thread": npt, "taper": true})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // E0 = 0.36351 in, h = 0.8 P = 0.75259 mm.
    let spec = last_spec(&doc);
    assert!(
        spec.starts_with(
            "thread F2:side(c1) pitch 0.9407407407407407 diameters 9.9857/8.4805/9.2331"
        ),
        "{spec}"
    );
    assert!(spec.ends_with(" cosmetic taper 0.0625 high true"), "{spec}");
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads[0]["taper"], 0.0625, "{threads}");
    assert_eq!(threads[0]["designation"], "1/8-27 NPT");
    // Modelled, from the low end; with the diameters a design stores at
    // the start.
    let modeled = thread_def(
        "F2:side(c1)",
        json!({"thread": npt, "modeled": true, "location": "low_end", "taper": true,
               "diameters": {"major": 9.9, "minor": 8.4, "pitch": 9.15}}),
    );
    doc.edit_feature(uid, &modeled).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let spec = last_spec(&doc);
    assert!(spec.contains("diameters 9.9000/8.4000/9.1500"), "{spec}");
    assert!(spec.ends_with(" taper 0.0625 high false"), "{spec}");
    assert!(!spec.contains("cosmetic"), "{spec}");
    // Cosmetic with diameters too (an .f3d design's taper).
    let cosmetic = thread_def(
        "F2:side(c1)",
        json!({"thread": npt, "taper": true,
               "diameters": {"major": 9.9, "minor": 8.4, "pitch": 9.15}}),
    );
    doc.edit_feature(uid, &cosmetic).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    // End chamfers are left out with a warning, not refused.
    let calls = doc.kernel().count("thread");
    let chamfered = thread_def(
        "F2:side(c1)",
        json!({"thread": npt, "taper": true, "chamfer": "start"}),
    );
    doc.edit_feature(uid, &chamfered).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        doc.warnings(uid)
            .iter()
            .any(|w| w.contains("no end chamfers")),
        "{:?}",
        doc.warnings(uid)
    );
    assert!(doc.kernel().count("thread") > calls);
    assert!(!last_spec(&doc).contains("chamfer"), "{}", last_spec(&doc));
    // Only a taper pipe thread tapers; a straight NPT one is not modelled.
    let error = doc
        .edit_feature(uid, &thread_def("F2:side(c1)", json!({"taper": true})))
        .unwrap_err()
        .to_string();
    assert!(error.contains("only a taper pipe thread"), "{error}");
    let error = doc
        .edit_feature(
            uid,
            &thread_def("F2:side(c1)", json!({"thread": npt, "modeled": true})),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot be modelled straight"), "{error}");
    // ISO 7-1 names its side, and is cosmetic only.
    let rc = json!({"standard": "iso_taper", "designation": "Rc 1/8"});
    doc.edit_feature(
        uid,
        &thread_def("F2:side(c1)", json!({"thread": rc, "taper": true})),
    )
    .unwrap();
    assert_eq!(
        status(&doc, uid),
        FeatureStatus::Failed("Rc 1/8 is an internal thread; use R 1/8 for a shaft".to_owned())
    );
    let r = json!({"standard": "iso_taper", "designation": "R 1/8"});
    doc.edit_feature(
        uid,
        &thread_def("F2:side(c1)", json!({"thread": r, "taper": true})),
    )
    .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let error = doc
        .edit_feature(
            uid,
            &thread_def(
                "F2:side(c1)",
                json!({"thread": r, "taper": true, "modeled": true}),
            ),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("cannot be modelled"), "{error}");
}

/// The design file of `doc` opened again (a design written before
/// mitcad#733 has none of its fields: its definitions here have none).
fn reopened(doc: &Document<MockKernel>) -> Document<MockKernel> {
    let json = doc.to_json();
    assert!(!json.contains("\"taper\": true"), "{json}");
    let mut opened = Document::from_json(&json, MockKernel::default()).unwrap();
    opened.recompute();
    opened
}

#[test]
fn taper_pipe_threads_saved_before_stay_straight() {
    // Designs saved before mitcad#733 have no `taper`: their NPT threads
    // and NPT-tapped holes open and evaluate as they did. A cosmetic NPT
    // thread cuts nothing; with Chamfer Ends it cuts only the chamfers.
    let npt = json!({"standard": "npt", "designation": "1/8-27 NPT"});
    let mut doc = rod();
    doc.add_feature(&thread_def("F2:side(c1)", json!({"thread": npt})), None)
        .unwrap();
    let doc = reopened(&doc);
    assert_eq!(status(&doc, FeatureUid(3)), FeatureStatus::Ok);
    assert_eq!(doc.kernel().count("thread"), 0);
    let threads = query(&doc, json!({"query": "threads"}));
    assert!(threads[0].get("taper").is_none(), "{threads}");
    let mut doc = rod();
    doc.add_feature(
        &thread_def("F2:side(c1)", json!({"thread": npt, "chamfer": "both"})),
        None,
    )
    .unwrap();
    let doc = reopened(&doc);
    assert_eq!(status(&doc, FeatureUid(3)), FeatureStatus::Ok);
    let spec = last_spec(&doc);
    assert!(
        spec.contains("cosmetic chamfer") && !spec.contains("taper"),
        "{spec}"
    );
    // Straight, as before: the size's own diameters, not the taper's.
    assert!(spec.contains("diameters 10.2870/8.7818/9.5344"), "{spec}");
    // NPT-tapped holes: the bore is the size's minor diameter, whatever
    // `diameter` holds (the panel's hidden 6, or 0), and nothing tapers.
    for diameter in [6.0, 0.0] {
        let mut b = block();
        let on_top =
            json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
        b.doc
            .add_feature(
                &hole_def(on_top, json!({"diameter": diameter, "thread": npt})),
                None,
            )
            .unwrap();
        let doc = reopened(&b.doc);
        assert_eq!(status(&doc, FeatureUid(3)), FeatureStatus::Ok, "{diameter}");
        assert_eq!(doc.kernel().count("thread"), 0, "{diameter}");
        let hole = last_spec(&doc);
        assert!(hole.contains("diameter 8.781815"), "{diameter}: {hole}");
    }
}

#[test]
fn holes_tapped_with_taper_pipe_threads_keep_their_drill() {
    // A tapped hole of 1/8-27 NPT with `taper` (mitcad#733): drilled at its
    // diameter, the tap drill, and tapered from its start (the high end of
    // the mock wall's axis), cosmetic or modelled.
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    let npt = json!({"standard": "npt", "designation": "1/8-27 NPT", "taper": true});
    let uid = b
        .doc
        .add_feature(
            &hole_def(on_top.clone(), json!({"diameter": 8.4, "thread": npt})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert_eq!(b.doc.kernel().count("thread"), 1);
    let spec = last_spec(&b.doc);
    // E1 = E0 + L1/16 = 0.37360 in: the minor diameter 8.7375 at the start.
    assert!(
        spec.starts_with(
            "thread F3:hole0.wall pitch 0.9407407407407407 diameters 10.2421/8.7369/9.4895"
        ),
        "{spec}"
    );
    assert!(spec.ends_with(" cosmetic taper 0.0625 high true"), "{spec}");
    let hole = b
        .doc
        .kernel()
        .specs
        .borrow()
        .iter()
        .rev()
        .find(|s| s.starts_with("hole"))
        .cloned()
        .unwrap();
    assert!(hole.contains("diameter 8.4"), "{hole}");
    b.doc.undo();
    // A tapering hole needs its diameter.
    let error = b
        .doc
        .add_feature(
            &hole_def(on_top.clone(), json!({"diameter": 0, "thread": npt})),
            None,
        )
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(error.contains("hole diameter"), "{error}");
    // Modelled.
    b.doc
        .add_feature(
            &hole_def(
                on_top,
                json!({"diameter": 8.4, "thread": {"standard": "npt", "designation": "1/8-27 NPT",
                       "modeled": true, "taper": true}}),
            ),
            None,
        )
        .unwrap();
    let spec = last_spec(&b.doc);
    assert!(spec.ends_with(" taper 0.0625 high true"), "{spec}");
    assert!(!spec.contains("cosmetic"), "{spec}");
}

#[test]
fn patterns_of_a_thread_alone_thread_the_faces_at_their_places() {
    // A second rod 30 along X (F4.b0), a modelled thread on the first
    // (F5); patterns of the thread alone (mitcad#733) go on the second
    // rod's side, where the first rod's moves: no copied feature made it,
    // so it is found by its place.
    let mut doc = rod();
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_circle(sketch, [30.0, 0.0], &mitcad_model::ValueInput::Number(10.0))
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
    let thread = doc
        .add_feature(&thread_def("F2:side(c1)", json!({"modeled": true})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&doc, thread), FeatureStatus::Ok);
    let other_side = doc
        .body_shape(BodyUid::new(FeatureUid(4), 0))
        .unwrap()
        .faces
        .iter()
        .find(|f| f.role == "side")
        .unwrap()
        .to_string();
    for pattern in [
        json!({"type": "rectangular_pattern",
               "objects": {"type": "features", "features": [thread]},
               "direction1": {"axis": "x", "quantity": 2, "distance": 30},
               "distance_type": "spacing"}),
        // At points: the anchor is the middle of the threaded face.
        json!({"type": "pattern_at_points",
               "objects": {"type": "features", "features": [thread]},
               "targets": [{"point": {"point": [30.0, 0.0, 10.0]}}]}),
    ] {
        let uid = doc.add_feature(&def(pattern.clone()), None).unwrap().uid;
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{pattern}");
        assert!(doc.warnings(uid).is_empty(), "{:?}", doc.warnings(uid));
        let spec = last_spec(&doc);
        assert!(
            spec.starts_with(&format!("thread {other_side} pitch 1.5")),
            "{spec}"
        );
        doc.undo();
    }
}

/// Sketch1 with a 10 mm circle extruded 20 mm into a rod (F2.b0).
fn rod() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_circle(sketch, [0.0, 0.0], &mitcad_model::ValueInput::Number(10.0))
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
    // The basic profile: D, D1 and D2.
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 1.5 diameters 10.0000/8.3762/9.0257 angle 0.000000 right true part None"
    );
    assert_eq!(
        doc.body_shape(BodyUid::new(FeatureUid(2), 0))
            .unwrap()
            .history,
        "thread(prism(F2:0..20),1.5)"
    );
    // The class's diameters, turned.
    let class = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "diameters": {"major": 9.85, "minor": 8.141, "pitch": 8.928},
               "angle": "30 deg"}),
    );
    doc.edit_feature(uid, &class).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 1.5 diameters 9.8500/8.1410/8.9280 angle 0.523599 right true part None"
    );
    // Diameters out of order; diameters and an angle on a cosmetic thread.
    let wrong = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "diameters": {"major": 9.85, "minor": 8.928, "pitch": 8.141}}),
    );
    let error = doc.edit_feature(uid, &wrong).unwrap_err().to_string();
    assert!(error.contains("must grow from the minor"), "{error}");
    let cosmetic = thread_def("F2:side(c1)", json!({"angle": 0.5}));
    let error = doc.edit_feature(uid, &cosmetic).unwrap_err().to_string();
    assert!(error.contains("only a modelled thread"), "{error}");
    doc.edit_feature(uid, &modeled).unwrap();

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
        FeatureStatus::Failed("class 6H is for internal threads; use 6g for a shaft".to_owned())
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
fn modelled_threads_take_extra_clearance() {
    let mut doc = rod();
    // Without clearance nothing changes: no fields, the same kernel call.
    let plain = thread_def("F2:side(c1)", json!({"modeled": true}));
    let uid = doc.add_feature(&plain, None).unwrap().uid;
    let basic = "thread F2:side(c1) pitch 1.5 diameters 10.0000/8.3762/9.0257 angle 0.000000 \
                 right true part None";
    assert_eq!(last_spec(&doc), basic);
    assert!(!doc.to_json().contains("clearance"), "{}", doc.to_json());

    // Radial and flank clearance go to the kernel with the size unchanged:
    // the same diameters, pitch, hand and part.
    let loose = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": 0.2, "flank_clearance": "0.05 mm",
               "length": 12, "offset": 2, "thread": {"designation": "M10x1.5", "class": "6g",
               "right_handed": false}}),
    );
    doc.edit_feature(uid, &loose).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(
        last_spec(&doc),
        "thread F2:side(c1) pitch 1.5 diameters 10.0000/8.3762/9.0257 angle 0.000000 \
         right false part Some((12.0, 2.0, true)) clearance 0.2000/0.0500"
    );
    // Saved with the feature and loaded again as they were.
    let FeatureDef::Thread(saved) = &doc.feature(uid).unwrap().def else {
        panic!("a thread");
    };
    assert!(saved.radial_clearance.is_some() && saved.flank_clearance.is_some());
    let text = doc.to_json();
    assert!(text.contains("radial_clearance"), "{text}");
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    assert_eq!(
        loaded.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );

    // Undo and redo bring the values back and forth.
    doc.undo();
    assert!(!doc.to_json().contains("clearance"));
    doc.redo();
    assert!(doc.to_json().contains("flank_clearance"));

    // A parameter expression: the thread follows the parameter, and fails
    // (not clamped) once the parameter goes negative.
    doc.add_parameter("gap", 0.1, "").unwrap();
    let by_name = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": "gap * 2"}),
    );
    doc.edit_feature(uid, &by_name).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).ends_with("clearance 0.2000/0.0000"),
        "{}",
        last_spec(&doc)
    );
    doc.set_parameter("gap", 0.15).unwrap();
    assert!(
        last_spec(&doc).ends_with("clearance 0.3000/0.0000"),
        "{}",
        last_spec(&doc)
    );
    doc.set_parameter("gap", -0.1).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("a negative clearance");
    };
    assert!(
        message.contains("radial clearance must be a finite length of 0 or more"),
        "{message}"
    );
    doc.set_parameter("gap", 0.1).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);

    // Zero is no clearance: the same kernel call as without.
    let zero = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": 0, "flank_clearance": 0}),
    );
    doc.edit_feature(uid, &zero).unwrap();
    assert_eq!(last_spec(&doc), basic);

    // Negative values, clearance on a cosmetic thread, and a flank
    // clearance that leaves no tooth (the basic crest is P/8 = 0.1875 wide;
    // 0.1 per flank takes 0.231 off it).
    let negative = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "flank_clearance": -0.05}),
    );
    let error = doc.edit_feature(uid, &negative).unwrap_err().to_string();
    assert!(
        error.contains("flank clearance must be a finite length of 0 or more"),
        "{error}"
    );
    let cosmetic = thread_def("F2:side(c1)", json!({"radial_clearance": 0.2}));
    let error = doc.edit_feature(uid, &cosmetic).unwrap_err().to_string();
    assert!(error.contains("only a modelled thread"), "{error}");
    let too_much = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "flank_clearance": 0.1}),
    );
    doc.edit_feature(uid, &too_much).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("no tooth left");
    };
    assert!(message.contains("leaves no usable tooth"), "{message}");
    // A radial clearance that leaves an external thread no core.
    let no_core = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": 4.5}),
    );
    doc.edit_feature(uid, &no_core).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("no core left");
    };
    assert!(message.contains("no core"), "{message}");
}

#[test]
fn tapped_holes_take_extra_clearance() {
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    let uid = b
        .doc
        .add_feature(
            &hole_def(
                on_top.clone(),
                json!({"thread": {"designation": "M6x1", "class": "6H", "modeled": true,
                                  "radial_clearance": 0.15, "flank_clearance": 0.02}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    // The bore stays the size's minor diameter; the thread gets the
    // clearance with the size's own diameters.
    assert_eq!(
        last_spec(&b.doc),
        "thread F3:hole0.wall pitch 1 diameters 6.0000/4.9175/5.3505 angle 0.000000 \
         right true part None clearance 0.1500/0.0200"
    );
    let text = b.doc.to_json();
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);

    // Checked as the thread feature's: negative, cosmetic, no tooth left
    // (an internal M6x1's crest is P/4 = 0.25 wide; 0.12 per flank takes
    // 0.277 off it).
    let error = b
        .doc
        .edit_feature(
            uid,
            &hole_def(
                on_top.clone(),
                json!({"thread": {"designation": "M6x1", "modeled": true, "radial_clearance": -1}}),
            ),
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("radial clearance must be a finite length of 0 or more"),
        "{error}"
    );
    let error = b
        .doc
        .edit_feature(
            uid,
            &hole_def(
                on_top.clone(),
                json!({"thread": {"designation": "M6x1", "flank_clearance": 0.02}}),
            ),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("only a modelled thread"), "{error}");
    b.doc
        .edit_feature(
            uid,
            &hole_def(
                on_top,
                json!({"thread": {"designation": "M6x1", "modeled": true, "flank_clearance": 0.12}}),
            ),
        )
        .unwrap();
    let FeatureStatus::Failed(message) = status(&b.doc, uid) else {
        panic!("no tooth left");
    };
    assert!(message.contains("leaves no usable tooth"), "{message}");
}

/// mitcad#691: an M6x1 6H modelled in a 5 mm bore (its tap drill) with
/// the Thread panel's defaults, the printed-fit parameters of 0.1 mm. An
/// internal M6x1's crest is P/4 = 0.25 wide and must keep P/50: a flank
/// clearance of at most (0.25 - 0.02) cos 30° / 2 = 0.0996 mm. The
/// printed-fit flank clearance is capped there and the thread is made; a
/// smaller one is used as it is. The same 0.1 mm given as a number is
/// refused, naming the most that fits (rounded down), which is taken.
#[test]
fn printed_fit_flank_clearance_is_capped_on_fine_pitches() {
    let mut doc = bored_rod(12.0, Some(5.0));
    let thread = |extra: Value| {
        let mut value = json!({"type": "thread",
            "faces": [{"body": "F2.b0", "face": "F3:hole0.wall"}],
            "thread": {"designation": "M6x1", "class": "6H"}, "modeled": true});
        for (key, field) in extra.as_object().unwrap() {
            value[key] = field.clone();
        }
        def(value)
    };
    let defaults = thread(json!({"radial_clearance": "thread_radial_clearance",
                                 "flank_clearance": "thread_flank_clearance"}));
    let uid = doc.add_feature(&defaults, None).unwrap().uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).starts_with("thread F3:hole0.wall pitch 1 diameters 6.0000/4.9175/5.3505"),
        "{}",
        last_spec(&doc)
    );
    assert!(
        last_spec(&doc).ends_with("clearance 0.1000/0.0996"),
        "{}",
        last_spec(&doc)
    );
    // The parameter keeps its value; a smaller one is used as it is.
    assert_eq!(
        doc.parameters()
            .value(doc.parameters().find("thread_flank_clearance").unwrap()),
        Some(0.1)
    );
    doc.set_parameter("thread_flank_clearance", 0.05).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).ends_with("clearance 0.1000/0.0500"),
        "{}",
        last_spec(&doc)
    );
    doc.set_parameter("thread_flank_clearance", 0.1).unwrap();

    // Given as a number, 0.1 mm is the user's own: refused with the most
    // that fits, which is then taken.
    let explicit = thread(json!({"radial_clearance": 0.1, "flank_clearance": 0.1}));
    doc.edit_feature(uid, &explicit).unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("0.1 mm leaves no tooth on M6x1");
    };
    assert!(message.contains("leaves no usable tooth"), "{message}");
    assert!(
        message.contains("takes a flank clearance of at most 0.0995 mm"),
        "{message}"
    );
    let largest = thread(json!({"radial_clearance": 0.1, "flank_clearance": 0.0995}));
    doc.edit_feature(uid, &largest).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(
        last_spec(&doc).ends_with("clearance 0.1000/0.0995"),
        "{}",
        last_spec(&doc)
    );

    // A tapped M6x1 hole with the printed-fit flank clearance alike.
    let mut b = block();
    let on_top = json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
    let hole = b
        .doc
        .add_feature(
            &hole_def(
                on_top,
                json!({"thread": {"designation": "M6x1", "class": "6H", "modeled": true,
                                  "flank_clearance": "thread_flank_clearance"}}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(status(&b.doc, hole), FeatureStatus::Ok);
    assert!(
        last_spec(&b.doc).ends_with("clearance 0.0000/0.0996"),
        "{}",
        last_spec(&b.doc)
    );
}

#[test]
fn profile_features_round_trip_through_the_design_file() {
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

/// The default chamfer diameter of a size, external or internal, as the
/// mock kernel's spec prints it.
fn default_chamfer(designation: &str, internal: bool) -> String {
    let data = mitcad_model::features::thread_table::lookup(
        mitcad_model::features::ThreadStandard::IsoMetric,
        designation,
    )
    .unwrap();
    format!(
        "{:.4}",
        mitcad_model::features::thread::default_chamfer_diameter(&data, internal, None)
    )
}

#[test]
fn thread_end_chamfers_default_to_the_size_and_cut_only_ends_the_thread_reaches() {
    // The defaults from the size table: a tenth of a pitch inside the minor
    // diameter outside, outside the major diameter inside (mitcad#304).
    assert_eq!(default_chamfer("M8", false), "6.5218");
    assert_eq!(default_chamfer("M8", true), "8.1250");
    let doc = rod();
    let answer = query(
        &doc,
        json!({"query": "thread_chamfer", "designation": "M8", "internal": true}),
    );
    assert!((answer["diameter"].as_f64().unwrap() - 8.125).abs() < 1e-9);
    assert!((answer["angle"].as_f64().unwrap() - 45.0).abs() < 1e-9);
    let answer = query(
        &doc,
        json!({"query": "thread_chamfer", "designation": "M8"}),
    );
    assert!(
        (answer["diameter"].as_f64().unwrap() - (8.0 - 1.25 * 0.8660254037844386 * 1.25 - 0.125))
            .abs()
            < 1e-9
    );

    // A modelled M10 on the rod, chamfered at both ends: the default
    // diameter from the size (here below the basic minor diameter), 45
    // degrees, both ends of the face.
    let mut doc = rod();
    let chamfered = thread_def("F2:side(c1)", json!({"modeled": true, "chamfer": "both"}));
    let uid = doc.add_feature(&chamfered, None).unwrap().uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let m10 = default_chamfer("M10x1.5", false);
    assert_eq!(
        last_spec(&doc),
        format!(
            "thread F2:side(c1) pitch 1.5 diameters 10.0000/8.3762/9.0257 angle 0.000000 right true \
             part None chamfer 45.0000 deg {m10} ends [(true, true)]"
        )
    );
    // The cones have names of their own.
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    assert!(
        shape
            .faces
            .iter()
            .any(|f| f.to_string() == "F3:thread_chamfer_high(F2:side(c1))")
    );

    // Saved and loaded as they were; no chamfer fields without a chamfer.
    let text = doc.to_json();
    assert!(text.contains("\"chamfer\": \"both\""), "{text}");
    let loaded = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(loaded.to_json(), text);
    assert_eq!(
        loaded.feature(uid).unwrap().def,
        doc.feature(uid).unwrap().def
    );
    doc.edit_feature(uid, &thread_def("F2:side(c1)", json!({"modeled": true})))
        .unwrap();
    assert!(!doc.to_json().contains("chamfer"), "{}", doc.to_json());
    assert!(!last_spec(&doc).contains("chamfer"));

    // The default follows the size; a given diameter and angle stay.
    let fine = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "chamfer": "both",
               "thread": {"designation": "M10x1.25", "class": "6g"}}),
    );
    doc.edit_feature(uid, &fine).unwrap();
    let m10_fine = default_chamfer("M10x1.25", false);
    assert_ne!(m10_fine, m10);
    assert!(
        last_spec(&doc).ends_with(&format!(
            "chamfer 45.0000 deg {m10_fine} ends [(true, true)]"
        )),
        "{}",
        last_spec(&doc)
    );
    doc.add_parameter("sink", 7.0, "mm").unwrap();
    let given = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "chamfer": "both", "chamfer_angle": "30 deg",
               "chamfer_diameter": "sink", "thread": {"designation": "M10x1.25", "class": "6g"}}),
    );
    doc.edit_feature(uid, &given).unwrap();
    assert!(
        last_spec(&doc).ends_with("chamfer 30.0000 deg 7.0000 ends [(true, true)]"),
        "{}",
        last_spec(&doc)
    );
    doc.set_parameter("sink", 7.5).unwrap();
    assert!(
        last_spec(&doc).ends_with("chamfer 30.0000 deg 7.5000 ends [(true, true)]"),
        "{}",
        last_spec(&doc)
    );

    // A partial thread chamfers only ends it reaches: measured from the
    // high end (the start) 12 long, the start; 2 below it, none.
    let partial = |extra: Value| {
        let mut value = json!({"modeled": true, "length": 12});
        for (key, field) in extra.as_object().unwrap() {
            value[key] = field.clone();
        }
        thread_def("F2:side(c1)", value)
    };
    let ends = |doc: &Document<MockKernel>| {
        let spec = last_spec(doc);
        spec.find(" ends ").map(|i| spec[i + 6..].to_owned())
    };
    doc.edit_feature(uid, &partial(json!({"chamfer": "both"})))
        .unwrap();
    assert_eq!(ends(&doc).as_deref(), Some("[(false, true)]"));
    doc.edit_feature(uid, &partial(json!({"chamfer": "start"})))
        .unwrap();
    assert_eq!(ends(&doc).as_deref(), Some("[(false, true)]"));
    // The far end lies inside the face: nothing to chamfer.
    doc.edit_feature(uid, &partial(json!({"chamfer": "end"})))
        .unwrap();
    assert_eq!(ends(&doc), None);
    doc.edit_feature(uid, &partial(json!({"chamfer": "both", "offset": 2})))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(ends(&doc), None);
    // From the low end: its start is the low end; over the whole face
    // measured from the high end, its end is the low end.
    doc.edit_feature(
        uid,
        &partial(json!({"chamfer": "start", "location": "low_end"})),
    )
    .unwrap();
    assert_eq!(ends(&doc).as_deref(), Some("[(true, false)]"));
    doc.edit_feature(uid, &partial(json!({"chamfer": "end", "length": 20})))
        .unwrap();
    assert_eq!(ends(&doc).as_deref(), Some("[(true, false)]"));

    // A cosmetic thread cuts its chamfers too (the kernel builds no
    // profile); without a chamfer it stays data.
    let calls = doc.kernel().count("thread");
    doc.edit_feature(uid, &thread_def("F2:side(c1)", json!({"chamfer": "start"})))
        .unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert_eq!(doc.kernel().count("thread"), calls + 1);
    assert!(
        last_spec(&doc).ends_with(&format!(
            "cosmetic chamfer 45.0000 deg {m10} ends [(false, true)]"
        )),
        "{}",
        last_spec(&doc)
    );
    let shape = doc.body_shape(BodyUid::new(FeatureUid(2), 0)).unwrap();
    assert!(!shape.faces.iter().any(|f| f.role == "thread"));
    doc.edit_feature(uid, &thread_def("F2:side(c1)", json!({})))
        .unwrap();
    assert_eq!(doc.kernel().count("thread"), calls + 1);

    // Values that are none, and fields without a chamfer.
    for (extra, expected) in [
        (
            json!({"chamfer_angle": "30 deg"}),
            "has no chamfer angle or diameter",
        ),
        (
            json!({"chamfer": "both", "chamfer_angle": "90 deg"}),
            "between 0 and 90 degrees",
        ),
        (
            json!({"chamfer": "both", "chamfer_diameter": 0}),
            "chamfer diameter must be greater than zero",
        ),
    ] {
        let error = doc
            .edit_feature(uid, &thread_def("F2:side(c1)", extra.clone()))
            .map(|_| String::new())
            .unwrap_or_else(|e| e.to_string());
        assert!(
            error.contains(expected),
            "'{expected}' not in '{error}' ({extra})"
        );
    }
    // A chamfer wider than the rod cuts nothing.
    doc.edit_feature(
        uid,
        &thread_def(
            "F2:side(c1)",
            json!({"modeled": true, "chamfer": "both", "chamfer_diameter": 12}),
        ),
    )
    .unwrap();
    let FeatureStatus::Failed(message) = status(&doc, uid) else {
        panic!("too wide a chamfer");
    };
    assert!(message.contains("must be smaller than"), "{message}");
}

#[test]
fn modelled_threads_that_take_their_whole_face_are_listed() {
    // The basic M10 on the 10 mm rod: its crest lies on the face, pieces of
    // which stay; the thread runs over the face.
    let mut doc = rod();
    let body = BodyUid::new(FeatureUid(2), 0);
    let has = |doc: &Document<MockKernel>, name: &str| {
        let shape = doc.body_shape(body).unwrap();
        shape.faces.iter().any(|f| f.to_string() == name)
    };
    let uid = doc
        .add_feature(&thread_def("F2:side(c1)", json!({"modeled": true})), None)
        .unwrap()
        .uid;
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(has(&doc, "F2:side(c1)"));
    let listed = json!([{"feature": "F3", "name": "Thread1", "body": "F2.b0",
        "face": "F2:side(c1)", "standard": "iso_metric", "designation": "M10x1.5",
        "class": "6g", "right_handed": true, "modeled": true, "major_diameter": 10.0,
        "minor_diameter": 8.376202367904177, "pitch": 1.5, "internal": false,
        "radius": 5.0, "start": [0.0, 0.0, 0.0], "end": [0.0, 0.0, 20.0]}]);
    assert_eq!(query(&doc, json!({"query": "threads"})), listed);

    // A radial clearance puts the crest inside the face, so no piece of the
    // face is left, and both ends get a cone (mitcad#433). The thread is
    // listed on the face as it was before the feature, from the cones'
    // inner edges, where the cones meet the crest (9.8): 45 degrees from
    // the default diameter, a tenth of a pitch inside the root (8.3762 -
    // 0.2), so (9.8 - 8.0262) / 2 = 0.8869 deep.
    let chamfered = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": 0.1, "chamfer": "both"}),
    );
    doc.edit_feature(uid, &chamfered).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(!has(&doc, "F2:side(c1)"));
    assert!(has(&doc, "F3:thread_chamfer_low(F2:side(c1))"));
    let threads = query(&doc, json!({"query": "threads"}));
    assert_eq!(threads.as_array().map(Vec::len), Some(1), "{threads}");
    let depth = (9.8 - (8.376202367904177 - 0.2 - 0.15)) / 2.0;
    let mut expected = listed.clone();
    expected[0]["start"] = Value::Null;
    expected[0]["end"] = Value::Null;
    let mut got = threads.clone();
    let [start, end] = ["start", "end"].map(|key| got[0][key].take());
    assert_eq!(got, expected);
    let near = |point: &Value, z: f64| {
        let p: Vec<f64> = point
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        p.len() == 3 && p[0].abs() < 1e-9 && p[1].abs() < 1e-9 && (p[2] - z).abs() < 1e-9
    };
    assert!(near(&start, depth) && near(&end, 20.0 - depth), "{threads}");

    // A given chamfer diameter, at the high end only: from the low end of
    // the face to that cone's inner edge, (9.8 - 7) / 2 / tan 30 deep.
    let start_only = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "radial_clearance": 0.1, "chamfer": "start",
               "chamfer_diameter": 7, "chamfer_angle": "30 deg"}),
    );
    doc.edit_feature(uid, &start_only).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    let threads = query(&doc, json!({"query": "threads"}));
    let depth = (9.8 - 7.0) / 2.0 / 30f64.to_radians().tan();
    assert!(
        near(&threads[0]["start"], 0.0) && near(&threads[0]["end"], 20.0 - depth),
        "{threads}"
    );

    // A class's major diameter below the face's, without a chamfer: the
    // whole face, from end to end.
    let class = thread_def(
        "F2:side(c1)",
        json!({"modeled": true, "diameters": {"major": 9.85, "minor": 8.141, "pitch": 8.928}}),
    );
    doc.edit_feature(uid, &class).unwrap();
    assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    assert!(!has(&doc, "F2:side(c1)"));
    assert_eq!(query(&doc, json!({"query": "threads"})), listed);
}

#[test]
fn modelled_tapped_holes_that_take_their_whole_wall_are_listed() {
    // A modelled M6x1 6H tapped hole over its whole wall: the bore is the
    // minor diameter D1, the crest lies on the wall, pieces of which stay.
    let mut b = block();
    let body = b.body;
    let has = |doc: &Document<MockKernel>, name: &str| {
        let shape = doc.body_shape(body).unwrap();
        shape.faces.iter().any(|f| f.to_string() == name)
    };
    let tapped = |thread: Value| {
        let on_top =
            json!({"type": "face", "body": "F2.b0", "face": TOP, "points": [[10, 10, 20]]});
        let mut extra = json!({"designation": "M6x1", "class": "6H", "modeled": true});
        for (key, field) in thread.as_object().unwrap() {
            extra[key] = field.clone();
        }
        hole_def(on_top, json!({"thread": extra}))
    };
    let uid = b.doc.add_feature(&tapped(json!({})), None).unwrap().uid;
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(has(&b.doc, "F3:hole0.wall"));
    assert!(has(&b.doc, "F3:thread(F3:hole0.wall)"));
    let listed = query(&b.doc, json!({"query": "threads"}));
    let minor = 6.0 - 5.0 * 3f64.sqrt() / 8.0;
    let near =
        |value: &Value, expected: f64| value.as_f64().is_some_and(|v| (v - expected).abs() < 1e-9);
    let at = |point: &Value, expected: [f64; 3]| (0..3).all(|i| near(&point[i], expected[i]));
    let thread = &listed[0];
    assert_eq!(listed.as_array().map(Vec::len), Some(1), "{listed}");
    assert_eq!(thread["feature"], "F3");
    assert_eq!(thread["face"], "F3:hole0.wall");
    assert_eq!(thread["designation"], "M6x1");
    assert_eq!(thread["class"], "6H");
    assert_eq!(thread["modeled"], true);
    assert_eq!(thread["internal"], true);
    assert!(near(&thread["major_diameter"], 6.0), "{listed}");
    assert!(near(&thread["minor_diameter"], minor), "{listed}");
    assert!(near(&thread["radius"], minor / 2.0), "{listed}");
    // The mock's wall runs along the block's height.
    assert!(at(&thread["start"], [30.0, 20.0, 0.0]), "{listed}");
    assert!(at(&thread["end"], [30.0, 20.0, 20.0]), "{listed}");

    // A radial clearance puts the crest off the wall (D1 + 0.2), so no
    // piece of the wall is left; the hole made the wall in the same step,
    // so no body before the hole has it (mitcad#436). The thread is listed
    // just the same, on the wall as the hole measured it before the
    // thread.
    b.doc
        .edit_feature(uid, &tapped(json!({"radial_clearance": 0.1})))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(!has(&b.doc, "F3:hole0.wall"));
    assert!(has(&b.doc, "F3:thread(F3:hole0.wall)"));
    assert_eq!(query(&b.doc, json!({"query": "threads"})), listed);

    // Suppressed, it is no thread; back again, it is.
    b.doc.set_suppressed(uid, true).unwrap();
    assert_eq!(query(&b.doc, json!({"query": "threads"})), json!([]));
    b.doc.undo();
    assert_eq!(query(&b.doc, json!({"query": "threads"})), listed);

    // A part 14 long from the start: without a clearance the crest lies on
    // the wall, pieces of which stay along the whole wall; the threads
    // query measures a hole's part from the low end of the wall's axis.
    b.doc
        .edit_feature(uid, &tapped(json!({"length": 14, "offset": 0})))
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    let part = query(&b.doc, json!({"query": "threads"}));
    assert_eq!(part.as_array().map(Vec::len), Some(1), "{part}");
    assert!(at(&part[0]["start"], [30.0, 20.0, 0.0]), "{part}");
    assert!(at(&part[0]["end"], [30.0, 20.0, 14.0]), "{part}");
    // With the clearance the crest lies off the wall: the thread takes all
    // of it along the part and leaves only the 6 mm outside it, which does
    // not show where the thread runs (mitcad#628). The thread is listed on
    // the wall as the hole measured it before the thread, as without the
    // clearance.
    b.doc
        .edit_feature(
            uid,
            &tapped(json!({"radial_clearance": 0.1, "length": 14, "offset": 0})),
        )
        .unwrap();
    assert_eq!(status(&b.doc, uid), FeatureStatus::Ok);
    assert!(has(&b.doc, "F3:hole0.wall"));
    assert_eq!(query(&b.doc, json!({"query": "threads"})), part);
}

/// A rod 26 long along z from 0 (Sketch1 F1, Extrude1 F2), `diameter`
/// across, bored through along its axis when `bore` is given (Hole1 F3,
/// whose wall `F3:hole0.wall` is an internal cylinder along z).
fn bored_rod(diameter: f64, bore: Option<f64>) -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let sketch = doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    let region = doc
        .add_circle(
            sketch,
            [0.0, 0.0],
            &mitcad_model::ValueInput::Number(diameter),
        )
        .unwrap()
        .region;
    doc.add_feature(
        &def(
            json!({"type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                    "extent": {"type": "distance", "distance": 26}, "operation": "new_body"}),
        ),
        None,
    )
    .unwrap();
    if let Some(bore) = bore {
        let on_plane = json!({"type": "plane", "plane": "xy", "points": [[0, 0, 0]]});
        let uid = doc
            .add_feature(
                &hole_def(
                    on_plane,
                    json!({"diameter": bore, "flip": true,
                           "extent": {"type": "distance", "depth": 26}}),
                ),
                None,
            )
            .unwrap()
            .uid;
        assert_eq!(status(&doc, uid), FeatureStatus::Ok);
    }
    doc
}

#[test]
fn modelled_threads_with_a_part_end_where_their_chamfer_meets_the_crest() {
    // mitcad#628: a modelled M48x5 thread 22.888 long, 3.112 from one end
    // of a face 26 long, with an end chamfer at 45 degrees where the part
    // reaches the face's other end. The mock's faces run along +z from
    // z = 0, so `low_end` measures from z = 0 and `high_end` from z = 26;
    // `chamfer: end` is the end away from `location`.
    //
    // Internal: the bore of Ø42.266 (Hole1), the class's diameters with
    // the minor one on the bore, the chamfer Ø45. Without a clearance the
    // crest lies on the bore (radius 21.133), pieces of which stay, and
    // the cone reaches it 22.5 - 21.133 = 1.367 from the end. A radial
    // clearance of 0.05 puts the crest at 21.183, off the bore: the thread
    // takes all of the bore along its part and leaves only the 3.112 mm
    // outside it, which must not be taken for the thread's extent. The
    // cone meets that crest 22.5 - 21.183 = 1.317 from the end.
    //
    // The design in the ticket is the internal `low_end` case upside
    // down: its bore's axis points down from z = 73.432, so its z is
    // 73.432 - z here: the start 3.112 is its 70.32, the end 26 - 1.317 =
    // 24.683 its 48.749 (with the clearances), 26 - 1.367 = 24.633 its
    // 48.799 (without).
    //
    // External: the basic M48x5 6g on a rod of Ø48 (radius 24, the major
    // diameter's: the crest on the face), the chamfer Ø42: the cone
    // reaches the face 24 - 21 = 3 from the end; with the radial clearance
    // the crest at 24 - 0.05 = 23.95, 23.95 - 21 = 2.95 from it.
    let (bore_cone, bore_cone_clear) = (22.5 - 21.133, 22.5 - 21.183);
    let (rod_cone, rod_cone_clear) = (24.0 - 21.0, 23.95 - 21.0);
    let near = |point: &Value, z: f64| {
        let p: Vec<f64> = point
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        p.len() == 3 && p[0].abs() < 1e-9 && p[1].abs() < 1e-9 && (p[2] - z).abs() < 1e-9
    };
    // (internal, location, clearances, start z, end z)
    let cases = [
        (true, "low_end", false, 3.112, 26.0 - bore_cone),
        (true, "low_end", true, 3.112, 26.0 - bore_cone_clear),
        (true, "high_end", false, bore_cone, 22.888),
        (true, "high_end", true, bore_cone_clear, 22.888),
        (false, "low_end", false, 3.112, 26.0 - rod_cone),
        (false, "low_end", true, 3.112, 26.0 - rod_cone_clear),
        (false, "high_end", false, rod_cone, 22.888),
        (false, "high_end", true, rod_cone_clear, 22.888),
    ];
    for (internal, location, clearances, start, end) in cases {
        let case = format!(
            "{} {location} {}",
            if internal { "internal" } else { "external" },
            if clearances {
                "with clearances"
            } else {
                "without"
            }
        );
        let (mut doc, mut value) = if internal {
            (
                bored_rod(60.0, Some(42.266)),
                json!({"type": "thread", "faces": [{"body": "F2.b0", "face": "F3:hole0.wall"}],
                       "thread": {"designation": "M48x5", "class": "6H"}, "modeled": true,
                       "diameters": {"major": 47.92, "minor": 42.266, "pitch": 45.14},
                       "chamfer_diameter": 45}),
            )
        } else {
            (
                bored_rod(48.0, None),
                json!({"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
                       "thread": {"designation": "M48x5", "class": "6g"}, "modeled": true,
                       "chamfer_diameter": 42}),
            )
        };
        for (key, field) in [
            ("location", json!(location)),
            ("offset", json!(3.112)),
            ("length", json!(22.888)),
            ("chamfer", json!("end")),
            ("angle", json!("31.1 deg")),
        ] {
            value[key] = field;
        }
        if clearances {
            value["radial_clearance"] = json!(0.05);
            value["flank_clearance"] = json!(0.05);
        }
        let uid = doc.add_feature(&def(value), None).unwrap().uid;
        assert_eq!(status(&doc, uid), FeatureStatus::Ok, "{case}");
        let threads = query(&doc, json!({"query": "threads"}));
        assert_eq!(
            threads.as_array().map(Vec::len),
            Some(1),
            "{case}: {threads}"
        );
        let thread = &threads[0];
        assert_eq!(thread["internal"], internal, "{case}: {threads}");
        let radius = if internal { 21.133 } else { 24.0 };
        assert!(
            thread["radius"]
                .as_f64()
                .is_some_and(|r| (r - radius).abs() < 1e-9),
            "{case}: {threads}"
        );
        assert!(
            near(&thread["start"], start) && near(&thread["end"], end),
            "{case}: expected z {start} to {end}, got {threads}"
        );
    }
}

// Thin walls along sketch curves (mitcad#463).

/// A line of a sketch; its curve id.
fn sketch_line(
    doc: &mut Document<MockKernel>,
    sketch: FeatureUid,
    a: [f64; 2],
    b: [f64; 2],
) -> String {
    let value = json!({"cmd": "sketch.add_line", "sketch": sketch, "start": a, "end": b});
    let result: Value = serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap();
    result["made"][0].as_str().unwrap().to_owned()
}

/// A thin extrusion along the curves of F3, its kernel spec and status.
fn along_curves(
    b: &mut Block,
    curves: &[&str],
    extra: Value,
) -> (FeatureUid, String, FeatureStatus) {
    let mut value = json!({"type": "extrude", "extent": {"type": "distance", "distance": 10},
        "thin": {"location": "side1", "thickness": 2,
                 "curves": {"sketch": "F3", "curves": curves}},
        "operation": "new_body"});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    let uid = b.doc.add_feature(&def(value), None).unwrap().uid;
    (uid, last_spec(&b.doc), status(&b.doc, uid))
}

#[test]
fn thin_walls_follow_open_and_closed_chains_of_curves() {
    let mut b = block();
    let sketch = b
        .doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    // A zigzag bending left from (0, 0), its middle line drawn downwards.
    let zig = [
        sketch_line(&mut b.doc, sketch, [0.0, 0.0], [40.0, 0.0]),
        sketch_line(&mut b.doc, sketch, [40.0, 30.0], [40.0, 0.0]),
        sketch_line(&mut b.doc, sketch, [40.0, 30.0], [70.0, 30.0]),
    ];
    assert_eq!(zig, ["c1", "c4", "c7"]);
    // Listed from the far end, the chain still runs so that it bends left.
    let (uid, spec, status) = along_curves(&mut b, &["c7", "c4", "c1"], json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.ends_with("distance 10 taper 0 thin Side1 2; chains c1>c4>c7 open from [0, 0, 0]"),
        "{spec}"
    );
    let faces: Vec<String> = b
        .doc
        .body_shape(BodyUid::new(uid, 0))
        .unwrap()
        .faces
        .iter()
        .map(ToString::to_string)
        .collect();
    for face in [
        "F4:outer(c1)",
        "F4:inner(c7)",
        "F4:tip0(c1)",
        "F4:tip1(c7)",
        "F4:start(r{c1,c4,c7})",
        "F4:end(r{c1,c4,c7})",
    ] {
        assert!(faces.iter().any(|f| f == face), "{face}: {faces:?}");
    }
    b.doc.undo();

    // Straight lines run as the first listed was drawn; a circle is closed;
    // three lines that meet at a point are three chains.
    let right = sketch_line(&mut b.doc, sketch, [0.0, 50.0], [30.0, 50.0]);
    let left = sketch_line(&mut b.doc, sketch, [30.0, 60.0], [0.0, 60.0]);
    let circle: Value = serde_json::from_str(
        &b.doc
            .command(
                &json!({"cmd": "sketch.add_circle", "sketch": sketch, "center": [100, 0],
                        "diameter": 10})
                .to_string(),
            )
            .unwrap(),
    )
    .unwrap();
    let circle = circle["curves"][0].as_str().unwrap().to_owned();
    let tee = [
        sketch_line(&mut b.doc, sketch, [0.0, 100.0], [40.0, 100.0]),
        sketch_line(&mut b.doc, sketch, [40.0, 100.0], [80.0, 100.0]),
        sketch_line(&mut b.doc, sketch, [40.0, 130.0], [40.0, 100.0]),
    ];
    let (_, spec, status) = along_curves(&mut b, &[&right, &left, &circle], json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.ends_with(&format!(
            "chains {right} open from [0, 50, 0] | {left} open from [30, 60, 0] | {circle} closed"
        )),
        "{spec}"
    );
    b.doc.undo();
    let curves: Vec<&str> = tee.iter().map(String::as_str).collect();
    let (_, spec, status) = along_curves(&mut b, &curves, json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    assert_eq!(spec.matches(" open from ").count(), 3, "{spec}");
    b.doc.undo();

    // Two sides with their own walls reach the kernel as with profiles.
    let (_, spec, status) = along_curves(
        &mut b,
        &["c1"],
        json!({"extent": {"type": "symmetric", "distance": 4},
               "thin": {"location": "side2", "thickness": 1, "side2": {"location": "center", "thickness": 3},
                        "curves": {"sketch": "F3", "curves": ["c1"]}}}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.contains("distance 4 taper 0 thin Side2 1; distance 4 taper 0 thin Center 3"),
        "{spec}"
    );
    b.doc.undo();

    // Curves of a sketch that is not there, and profiles beside the curves.
    let rejected = [
        (
            json!({"thin": {"location": "center", "thickness": 1,
                            "curves": {"sketch": "F3", "curves": ["c99"]}}}),
            "Sketch2 has no curve c99",
        ),
        (
            json!({"profiles": [{"sketch": "F1", "region": RECT}]}),
            "thin walls along curves take no profiles",
        ),
    ];
    for (extra, expected) in rejected {
        let mut value = json!({"type": "extrude", "extent": {"type": "distance", "distance": 10},
            "thin": {"location": "side1", "thickness": 2,
                     "curves": {"sketch": "F3", "curves": ["c1"]}},
            "operation": "new_body"});
        for (key, field) in extra.as_object().unwrap() {
            value[key] = field.clone();
        }
        let error = b
            .doc
            .add_feature(&def(value), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn walls_on_both_sides_take_a_thickness_each() {
    let mut b = block();
    let (spec, status) = extrude_with(
        &mut b,
        json!({"type": "distance", "distance": 10}),
        json!({"thin": {"location": "both", "thickness": 2, "thickness2": 0.5}}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.ends_with("distance 10 taper 0 thin Both 2 0.5"),
        "{spec}"
    );
    let rejected = [
        (
            json!({"thin": {"location": "both", "thickness": 2}}),
            "a wall on both sides needs thickness2",
        ),
        (
            json!({"thin": {"location": "center", "thickness": 2, "thickness2": 1}}),
            "only a wall on both sides has thickness2",
        ),
        (
            json!({"thin": {"location": "both", "thickness": 2, "thickness2": 0}}),
            "wall thickness must be greater than zero",
        ),
    ];
    for (extra, expected) in rejected {
        let def = extrude_def(json!({"type": "distance", "distance": 10}), extra);
        let error = b.doc.add_feature(&def, None).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn thin_walls_along_curves_save_without_profiles() {
    let mut b = block();
    let sketch = b
        .doc
        .add_sketch(mitcad_model::features::SketchPlane::Xy)
        .unwrap()
        .uid;
    sketch_line(&mut b.doc, sketch, [0.0, 0.0], [40.0, 0.0]);
    let (uid, _, status) = along_curves(&mut b, &["c1"], json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    let listed = query(&b.doc, json!({"query": "feature", "uid": uid}));
    let definition = &listed["def"];
    assert!(definition.get("profiles").is_none(), "{listed}");
    assert_eq!(
        definition["thin"]["curves"],
        json!({"sketch": "F3", "curves": ["c1"]}),
        "{listed}"
    );
    // The browser lists it under the sketch of its curves.
    let timeline = query(&b.doc, json!({"query": "timeline"}));
    let entry = timeline["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["uid"] == json!(uid))
        .cloned()
        .unwrap();
    assert_eq!(entry["sketches"], json!(["F3"]), "{entry}");
}

#[test]
fn an_extrusion_goes_along_its_own_direction() {
    // (mitcad#539) The direction reaches the kernel as a unit vector, with
    // the taper; `flip` and a negative distance reverse it. It round-trips;
    // it is refused in the sketch plane, of no length and with a start
    // object.
    let mut b = block();
    let distance = json!({"type": "distance", "distance": 10, "taper": 0.1});
    let (spec, status) = extrude_with(&mut b, distance.clone(), json!({"direction": [0, 3, 4]}));
    assert!(
        spec.ends_with("along [0.0, 0.6, 0.8] from offset 0: distance 10 taper 0.1"),
        "{spec}"
    );
    assert_eq!(status, FeatureStatus::Ok);
    let (spec, _) = extrude_with(
        &mut b,
        distance.clone(),
        json!({"direction": [0, 3, 4], "flip": true}),
    );
    assert!(
        spec.ends_with("along [-0.0, -0.6, -0.8] from offset 0: distance 10 taper 0.1"),
        "{spec}"
    );
    let (spec, _) = extrude_with(
        &mut b,
        json!({"type": "symmetric", "distance": 6}),
        json!({"direction": [0, 3, 4]}),
    );
    assert!(
        spec.ends_with(
            "along [0.0, 0.6, 0.8] from offset 0: distance 6 taper 0; distance 6 taper 0"
        ),
        "{spec}"
    );
    let (spec, _) = extrude_with(
        &mut b,
        json!({"type": "distance", "distance": -5}),
        json!({"direction": [0, 3, 4]}),
    );
    assert!(spec.contains("along [-0.0, -0.6, -0.8]"), "{spec}");
    // Kept as given.
    let uid = b
        .doc
        .add_feature(
            &extrude_def(distance.clone(), json!({"direction": [0, 3, 4]})),
            None,
        )
        .unwrap()
        .uid;
    let def = query(&b.doc, json!({"query": "feature", "uid": uid}))["def"].clone();
    assert_eq!(def["direction"], json!([0.0, 3.0, 4.0]), "{def}");
    b.doc.undo();
    // In the sketch plane: the extrusion fails.
    let (_, status) = extrude_with(&mut b, distance.clone(), json!({"direction": [1, 0, 0]}));
    assert_eq!(
        status,
        FeatureStatus::Failed("the extrusion direction lies in the sketch plane".to_owned())
    );
    for (extra, expected) in [
        (json!({"direction": [0, 0, 0]}), "of some length"),
        (
            json!({"direction": [0, 3, 4],
                   "start": {"type": "object", "object": {"type": "plane", "plane": "xz"}}}),
            "starts at the sketch plane",
        ),
    ] {
        let error = b
            .doc
            .add_feature(&extrude_def(distance.clone(), extra), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "'{expected}' not in '{error}'");
    }
}
