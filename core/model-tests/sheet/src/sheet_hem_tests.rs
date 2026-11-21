// SPDX-License-Identifier: MIT
//! The hem forms of the Hem command (mitcad#2112) on the mock kernel: the
//! flat, open and rolled forms saved and reopened with their parameters,
//! an edit that changes the form, and the refusals that come before any
//! geometry, in words with the hem's edge painted. The bodies themselves
//! are checked with OCCT by the CLI fixture `sheet_hem_forms`.

use mitcad_model::features::sheet_folded::SheetHem;
use mitcad_model::features::{FeatureDef, SketchPlane};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::notice::NoticePlace;
use mitcad_model::test_support::{def, num};
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FeatureStatus};
use serde_json::{Value, json};

/// The 35 x 20 sheet of `cli.sheet_hem_forms` (t 2, r 3) and its sketch.
fn sheet() -> (Document<MockKernel>, BodyUid, FeatureUid) {
    let mut doc = Document::new(MockKernel::default());
    let rules = doc
        .add_feature(
            &def(json!({"type":"sheet_rules","thickness":2,"bend_radius":3,
                "allowance":{"type":"k_factor","k":0.3},"reference_side":"back",
                "bend_relief":{"type":"rectangular","width":2,"depth":3},
                "corner_relief":{"type":"round","width":2,"depth":3}})),
            None,
        )
        .unwrap()
        .uid;
    let sketch = doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = doc
        .add_rectangle(sketch, [-30.0, 0.0], &num(35.0), &num(20.0))
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

/// A hem on the edge x = 5 at Tangent to Side.
fn hem(body: BodyUid, sketch: FeatureUid, length: f64, hem: Value) -> Value {
    json!({"type":"sheet_flange","body":body,
        "boundary":{"panel":{"feature":body.feature,"slot":0},
                    "curve":{"type":"sketch","sketch":sketch,"segment":"c2[c1,c3]"}},
        "angle":std::f64::consts::PI,"length":{"mode":"outer_tangent","value":length},
        "position":"side_tangent","hem":hem})
}

#[test]
fn flat_open_and_rolled_hems_are_kept_and_reopened() {
    let (mut doc, body, sketch) = sheet();
    let uid = doc
        .add_feature(
            &def(hem(
                body,
                sketch,
                0.0,
                json!({"form":"rolled","angle":"270 deg","radius":"3 mm"}),
            )),
            None,
        )
        .unwrap()
        .uid;
    let FeatureDef::SheetFlange(d) = &doc.feature(uid).unwrap().def else {
        panic!("flange definition");
    };
    assert!(matches!(
        d.hem,
        Some(SheetHem::Rolled {
            radius: Some(_),
            ..
        })
    ));
    let mut slots = Vec::new();
    d.map_params(&mut |slot, p| {
        slots.push(slot.to_owned());
        Ok::<_, ()>(*p)
    })
    .unwrap();
    assert!(slots.iter().any(|s| s == "hem.angle"), "{slots:?}");
    assert!(slots.iter().any(|s| s == "hem.radius"), "{slots:?}");
    // An edit changes the form: a flat hem of the default gap, then open.
    for (form, check) in [
        (json!({"form":"flat"}), "flat"),
        (json!({"form":"open","gap":"2 mm"}), "open"),
    ] {
        doc.edit_feature(uid, &def(hem(body, sketch, 10.0, form)))
            .unwrap();
        let FeatureDef::SheetFlange(d) = &doc.feature(uid).unwrap().def else {
            panic!("flange definition");
        };
        assert_eq!(d.hem.as_ref().map(SheetHem::form), Some(check));
    }
    let saved = doc.to_json();
    // The design file is pretty-printed: `"form": "open"`.
    assert!(saved.contains("\"form\": \"open\""), "{saved}");
    let reopened = Document::from_json(&saved, MockKernel::default()).unwrap();
    let FeatureDef::SheetFlange(again) = &reopened.feature(uid).unwrap().def else {
        panic!("flange definition");
    };
    assert!(matches!(again.hem, Some(SheetHem::Open { .. })));
}

/// A hem's refusal says what is wrong in words, and its place is the wall
/// of the hem's edge, painted on hover (mitcad#2112).
#[test]
fn hem_refusals_are_in_words_with_their_edge_painted() {
    let (mut doc, body, sketch) = sheet();
    let wall = NoticePlace::Face {
        body: Some(body.to_string()),
        name: format!(
            "{}:sheet_side0({sketch}:sheet_curve(c2[c1,c3]))",
            body.feature
        ),
    };
    for (length, form, part) in [
        (
            10.0,
            json!({"form":"open","gap":0}),
            "an open hem's gap must be more than 0",
        ),
        (
            2.5,
            json!({"form":"open","gap":2}),
            "an open hem's length must be at least its bend's outer radius, 3.0000 mm",
        ),
        (
            1.0,
            json!({"form":"flat"}),
            "a flat hem's length must be at least",
        ),
        (
            0.0,
            json!({"form":"rolled","angle":"180 deg","radius":3}),
            "a rolled hem's angle must be more than 180 degrees and less than a full turn",
        ),
        (
            25.0,
            json!({"form":"teardrop","gap":6}),
            "inside radius must be more than half its gap",
        ),
    ] {
        let uid = doc
            .add_feature(&def(hem(body, sketch, length, form)), None)
            .unwrap()
            .uid;
        let Some(FeatureStatus::Failed(error)) = doc.status(uid) else {
            panic!("not refused: {:?}", doc.status(uid));
        };
        assert!(error.message.contains(part), "{}", error.message);
        assert!(!error.message.contains("sheet_side"), "{}", error.message);
        assert!(!error.message.contains("c2["), "{}", error.message);
        assert!(error.places.contains(&wall), "{:?}", error.places);
        doc.undo();
    }
}
