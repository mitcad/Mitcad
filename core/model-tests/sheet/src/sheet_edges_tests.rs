// SPDX-License-Identifier: MIT
//! A flange on several connected edges with mitred corners (mitcad#1872)
//! on the mock kernel: the definition saved and reopened, its corner gap a
//! parameter, and the refusals that come before any geometry. The mitred
//! walls themselves are checked with OCCT by the CLI fixtures
//! `sheet_flange_edges` and `sheet_flange_edges_bend`.

use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::notice::NoticePlace;
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use serde_json::{Value, json};

fn rules() -> Value {
    json!({"type":"sheet_rules","thickness":1,"bend_radius":1,
        "allowance":{"type":"k_factor","k":0.44},"reference_side":"front",
        "bend_relief":{"type":"round","width":1,"depth":2},
        "corner_relief":{"type":"round","width":1,"depth":2}})
}

/// A 40 x 20 sheet face and its sketch.
fn sheet() -> (Document<MockKernel>, BodyUid, FeatureUid) {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc.add_feature(&def(rules()), None).unwrap().uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [0.0, 0.0], &num(40.0), &num(20.0))
        .unwrap()
        .region;
    let face = doc
        .add_feature(
            &def(json!({"type":"sheet_face","rules":rules,
                "profiles":[{"sketch":sketch,"region":region}]})),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(doc.status(face), Some(&FeatureStatus::Ok));
    (doc, BodyUid::new(face, 0), sketch)
}

fn edge(body: BodyUid, sketch: FeatureUid, segment: &str) -> Value {
    json!({"panel":{"feature":body.feature,"slot":0},
        "curve":{"type":"sketch","sketch":sketch,"segment":segment}})
}

/// A flange on the edges y = 0 and x = 40 (meeting at their corner).
fn flange(body: BodyUid, sketch: FeatureUid) -> Value {
    json!({"type":"sheet_flange","body":body,
        "boundary":edge(body, sketch, "c1[c4,c2]"),
        "boundaries":[edge(body, sketch, "c2[c1,c3]")],
        "angle":std::f64::consts::FRAC_PI_2,"length":{"mode":"tangent","value":10},
        "position":"inside","corner_gap":"0.5 mm","miter":"earlier_runs_past",
        "corner_miters":[{"edges":[0,1],"miter":"bisect"}]})
}

/// The flange is refused with a message containing `part` and no curve
/// names, when it is added or when it is evaluated; a failed flange is
/// taken back (it would block the sheet for the next).
fn refused(doc: &mut Document<MockKernel>, definition: Value, part: &str) {
    match doc.add_feature(&def(definition), None) {
        Err(error) => assert!(error.to_string().contains(part), "{error}"),
        Ok(added) => {
            let message = match doc.status(added.uid) {
                Some(FeatureStatus::Failed(m)) => m.clone(),
                other => panic!("not refused: {other:?}"),
            };
            assert!(message.contains(part), "{message}");
            assert!(!message.contains("c1["), "{message}");
            doc.undo();
        }
    }
}

#[test]
fn several_edges_their_gap_and_miters_are_kept_and_reopened() {
    let (mut doc, body, sketch) = sheet();
    let uid = doc
        .add_feature(&def(flange(body, sketch)), None)
        .unwrap()
        .uid;
    let FeatureDef::SheetFlange(d) = &doc.feature(uid).unwrap().def else {
        panic!("flange definition");
    };
    assert_eq!(d.edges().len(), 2);
    let mut slots = Vec::new();
    d.map_params(&mut |slot, p| {
        slots.push(slot.to_owned());
        Ok::<_, ()>(*p)
    })
    .unwrap();
    assert!(slots.iter().any(|slot| slot == "corner_gap"), "{slots:?}");
    let saved = doc.to_json();
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    let FeatureDef::SheetFlange(again) = &reopened.feature(uid).unwrap().def else {
        panic!("flange definition");
    };
    assert_eq!(again.boundaries, d.boundaries);
    assert_eq!(again.miter, d.miter);
    assert_eq!(again.corner_miters, d.corner_miters);
    assert!(again.corner_gap.is_some());
}

#[test]
fn several_edges_are_refused_in_words_before_any_geometry() {
    let (mut doc, body, sketch) = sheet();
    // Opposite edges meet nowhere.
    let mut apart = flange(body, sketch);
    apart["boundaries"] = json!([edge(body, sketch, "c3[c2,c4]")]);
    apart["corner_miters"] = json!([]);
    refused(&mut doc, apart, "must be connected at their ends");
    // A corner's miter on edges the flange does not have.
    let mut beyond = flange(body, sketch);
    beyond["corner_miters"] = json!([{"edges":[0,2],"miter":"bisect"}]);
    refused(&mut doc, beyond, "names edges the flange does not have");
    // The same edge twice.
    let mut twice = flange(body, sketch);
    twice["boundaries"] = json!([edge(body, sketch, "c1[c4,c2]")]);
    twice["corner_miters"] = json!([]);
    refused(&mut doc, twice, "the same edge twice");
    // A hem, and At Edge.
    let mut hem = flange(body, sketch);
    hem["hem"] = json!({"form":"teardrop","gap":1});
    hem["length"] = json!({"mode":"outer_tangent","value":10});
    refused(
        &mut doc,
        hem,
        "takes no profile, hem, corner joint, offsets or width",
    );
    let mut at_edge = flange(body, sketch);
    at_edge["position"] = json!("edge");
    refused(&mut doc, at_edge, "cannot be At Edge");
    // A negative gap.
    let mut negative = flange(body, sketch);
    negative["corner_gap"] = json!(-1);
    refused(&mut doc, negative, "must not be negative");
}

/// The places a corner refusal paints (mitcad#1877): the two walls that
/// meet there, as the sheet has them before the flange, and the corner.
#[test]
fn corner_refusals_paint_the_walls_and_the_corner() {
    let (mut doc, body, sketch) = sheet();
    // A wall running past the other needs a gap above 0.
    let mut touching = flange(body, sketch);
    touching["corner_gap"] = json!(0);
    touching["corner_miters"] = json!([]);
    let uid = doc.add_feature(&def(touching), None).unwrap().uid;
    let Some(FeatureStatus::Failed(error)) = doc.status(uid) else {
        panic!("not refused: {:?}", doc.status(uid));
    };
    assert!(
        error.message.contains("needs a Corner Gap above 0"),
        "{}",
        error.message
    );
    // In words: no names in the message, the walls in its places.
    assert!(!error.message.contains("sheet_side"), "{}", error.message);
    let wall = |segment: &str| NoticePlace::Face {
        body: Some(body.to_string()),
        name: format!(
            "{}:sheet_side0({sketch}:sheet_curve({segment}))",
            body.feature
        ),
    };
    for segment in ["c1[c4,c2]", "c2[c1,c3]"] {
        assert!(
            error.places.contains(&wall(segment)),
            "{segment}: {:?}",
            error.places
        );
    }
    // The corner, where the edges y = 0 and x = 40 meet.
    assert!(
        error.places.iter().any(|place| matches!(
            place,
            NoticePlace::Point { at } if (at[0] - 40.0).abs() < 1e-6 && at[1].abs() < 1e-6
        )),
        "{:?}",
        error.places
    );
    assert_eq!(error.places.len(), 3, "{:?}", error.places);
    doc.undo();
    // A corner's miter on edges that do not meet paints those two walls.
    let mut apart = flange(body, sketch);
    apart["boundaries"] = json!([
        edge(body, sketch, "c2[c1,c3]"),
        edge(body, sketch, "c3[c2,c4]")
    ]);
    apart["corner_miters"] = json!([{"edges":[0,2],"miter":"bisect"}]);
    let uid = doc.add_feature(&def(apart), None).unwrap().uid;
    let Some(FeatureStatus::Failed(error)) = doc.status(uid) else {
        panic!("not refused: {:?}", doc.status(uid));
    };
    assert!(error.message.contains("do not meet"), "{}", error.message);
    for segment in ["c1[c4,c2]", "c3[c2,c4]"] {
        assert!(
            error.places.contains(&wall(segment)),
            "{segment}: {:?}",
            error.places
        );
    }
}

/// The corners the Flange panel maps a picked corner to (mitcad#1877, the
/// `sheet_flange_corners` query): each pair of edges that meet and where.
#[test]
fn the_corners_of_a_flange_on_several_edges_are_found() {
    let (doc, body, sketch) = sheet();
    let edges: Vec<mitcad_model::sheet::BoundaryRef> = ["c1[c4,c2]", "c2[c1,c3]", "c3[c2,c4]"]
        .iter()
        .map(|segment| serde_json::from_value(edge(body, sketch, segment)).unwrap())
        .collect();
    let found = doc.sheet_flange_corners_json(body, &edges).unwrap();
    let corners = found["corners"].as_array().unwrap();
    assert_eq!(corners.len(), 2, "{found}");
    let at = |i: usize| -> [f64; 2] {
        let at = &corners[i]["at"];
        [at[0].as_f64().unwrap(), at[1].as_f64().unwrap()]
    };
    assert_eq!(corners[0]["edges"], json!([0, 1]), "{found}");
    assert!(
        (at(0)[0] - 40.0).abs() < 1e-6 && at(0)[1].abs() < 1e-6,
        "{found}"
    );
    assert_eq!(corners[1]["edges"], json!([1, 2]), "{found}");
    assert!(
        (at(1)[0] - 40.0).abs() < 1e-6 && (at(1)[1] - 20.0).abs() < 1e-6,
        "{found}"
    );
    // Edges that meet nowhere are refused in words.
    let apart = [edges[0].clone(), edges[2].clone()];
    assert!(doc.sheet_flange_corners_json(body, &apart).is_err());
}
