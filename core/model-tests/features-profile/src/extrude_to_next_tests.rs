// SPDX-License-Identifier: MIT
//! Extrude To Next (GitHub mitcad/Mitcad#674) with the mock kernel: the
//! side ends at the first face of the bodies ahead of the profile, found
//! without a pick: which bodies the kernel gets as the target, that the
//! target follows the bodies' changes, and how a part of the profile
//! without an end, no bodies and no bodies ahead are reported. The
//! geometry itself is `geometry/tests/test_extrude_feature.cpp`
//! (`test_to_next_bodies`) and `cli.extrude_to_next`.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::test_support::{Block, block, def, extrude};
use mitcad_model::testing::MockKernel;
use mitcad_model_api::DocumentApi;

const RECT: &str = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}";

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

/// An extrusion of the block's sketch (the 60 x 40 rectangle on xy).
fn extrude_def(extent: Value, extra: Value) -> FeatureDef<mitcad_model::ValueInput> {
    let mut value = json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                           "extent": extent, "operation": "new_body"});
    for (key, field) in extra.as_object().unwrap() {
        value[key] = field.clone();
    }
    def(value)
}

/// Adds the extrusion; its kernel spec and status.
fn added(b: &mut Block, extent: Value, extra: Value) -> (FeatureUid, String, FeatureStatus) {
    let uid = b
        .doc
        .add_feature(&extrude_def(extent, extra), None)
        .unwrap()
        .uid;
    (uid, last_spec(&b.doc), status(&b.doc, uid))
}

/// The extrusion's kernel spec and status, taken back after.
fn tried(b: &mut Block, extent: Value, extra: Value) -> (String, FeatureStatus) {
    let (_, spec, status) = added(b, extent, extra);
    b.doc.undo();
    (spec, status)
}

fn to_next() -> Value {
    json!({"type": "to_next"})
}

#[test]
fn to_next_ends_at_the_first_bodies_ahead() {
    // The block, z = 0 to 20, lies ahead of the sketch plane upwards.
    let mut b = block();
    let (spec, status) = tried(&mut b, to_next(), json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.contains("along [0.0, 0.0, 1.0] from offset 0: to body prism(F2:0..20) through false offset 0 taper 0"),
        "{spec}"
    );
    // Flipped, it lies behind: To Next never turns round to reach a body.
    let (_, status) = tried(&mut b, to_next(), json!({"flip": true}));
    let error = status.error().unwrap_or_default().to_owned();
    assert!(
        error.contains("To Next finds no body ahead of the profile"),
        "{error}"
    );
    // From 40 mm up, flipped, the block is ahead again.
    let (spec, status) = tried(
        &mut b,
        to_next(),
        json!({"start": {"type": "offset", "offset": 40}, "flip": true}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.contains("to body prism(F2:0..20) through false"),
        "{spec}"
    );

    // A second body, z = 30 to 40: from 50 mm up, down, both lie ahead and
    // go to the kernel as one target; from 25 mm up only the block does.
    let (upper, _, status) = added(
        &mut b,
        json!({"type": "distance", "distance": 10}),
        json!({"start": {"type": "offset", "offset": 30}}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    let (spec, status) = tried(
        &mut b,
        to_next(),
        json!({"start": {"type": "offset", "offset": 50}, "flip": true}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(spec.contains("to body compound("), "{spec}");
    assert!(spec.contains("prism(F2:0..20)"), "{spec}");
    let (spec, status) = tried(
        &mut b,
        to_next(),
        json!({"start": {"type": "offset", "offset": 25}, "flip": true}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.contains("to body prism(F2:0..20) through false"),
        "{spec}"
    );
    assert!(!spec.contains("compound("), "{spec}");
    // A Join with participants ends at them alone.
    let (spec, status) = tried(
        &mut b,
        to_next(),
        json!({"start": {"type": "offset", "offset": 50}, "flip": true,
               "operation": "join", "participants": [format!("{upper}.b0")]}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(!spec.contains("compound("), "{spec}");
    assert!(!spec.contains("prism(F2:0..20)"), "{spec}");
    // A side of two sides To Next, the other a distance: from the sketch
    // plane up, the block and the body above it both lie ahead.
    let (spec, status) = tried(
        &mut b,
        json!({"type": "two_sides", "side1": {"type": "to_next"},
               "side2": {"type": "distance", "distance": 5}, "signed": true}),
        json!({}),
    );
    assert_eq!(status, FeatureStatus::Ok);
    assert!(
        spec.contains(
            "to body compound(prism(F2:0..20),prism(F3:30..40)) through false offset 0 taper 0; \
             distance 5 taper 0"
        ),
        "{spec}"
    );
}

#[test]
fn to_next_follows_the_bodies_changes() {
    let mut b = block();
    let (uid, spec, status) = added(&mut b, to_next(), json!({}));
    assert_eq!(status, FeatureStatus::Ok);
    assert!(spec.contains("to body prism(F2:0..20)"), "{spec}");
    // The block made 30 mm high: the extrusion is computed again, up to the
    // block as it is now.
    b.doc
        .edit_feature(
            b.extrude,
            &extrude(b.sketch, &[&b.region], 30.0, "new_body", &[]),
        )
        .unwrap();
    assert!(b.evaluated().contains(&uid), "{:?}", b.evaluated());
    assert_eq!(status_of(&b, uid), FeatureStatus::Ok);
    let spec = last_spec(&b.doc);
    assert!(spec.contains("to body prism(F2:0..30)"), "{spec}");
    // Saved as it was given, without a taper.
    let saved: Value = serde_json::from_str(
        &b.doc
            .query(&json!({"query": "feature", "uid": uid.to_string()}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        saved["def"]["extent"],
        json!({"type": "to_next"}),
        "{saved}"
    );
    // As a side through all, it names no body: the block's feature goes
    // without it, and the extrusion then has no body to end at.
    b.doc.delete_feature(b.extrude, false).unwrap();
    assert_eq!(
        status_of(&b, uid).error(),
        Some("To Next needs a body to end at: there are no bodies")
    );
}

fn status_of(b: &Block, uid: FeatureUid) -> FeatureStatus {
    status(&b.doc, uid)
}

#[test]
fn to_next_reports_a_part_of_the_profile_without_an_end() {
    let mut b = block();
    // The kernel finds a part of the profile that meets no face ahead.
    b.doc.kernel().fail_text.borrow_mut().insert(
        "extrude",
        "the target does not cut across the whole profile".to_owned(),
    );
    let (_, status) = tried(&mut b, to_next(), json!({}));
    assert_eq!(
        status.error(),
        Some(
            "To Next finds no face ahead of a part of the profile: each part of the profile must \
             meet a face of the bodies in the extrusion direction; flip the direction, leave that \
             part out of the profiles or choose another extent"
        )
    );
    // Up to a picked body the kernel's words stay: they may be that side's.
    let (_, status) = tried(
        &mut b,
        json!({"type": "to_object", "object": {"type": "body", "body": "F2.b0"}}),
        json!({}),
    );
    let error = status.error().unwrap_or_default().to_owned();
    assert!(
        error.contains("does not cut across the whole profile"),
        "{error}"
    );
    b.doc.kernel().fail_text.borrow_mut().clear();

    // No bodies at all.
    b.doc.delete_feature(b.extrude, false).unwrap();
    let (_, status) = tried(&mut b, to_next(), json!({}));
    assert_eq!(
        status.error(),
        Some("To Next needs a body to end at: there are no bodies")
    );
}

#[test]
fn to_next_starts_at_the_profile_plane_or_an_offset() {
    let mut b = block();
    let error = b
        .doc
        .add_feature(
            &extrude_def(
                to_next(),
                json!({"start": {"type": "object", "object": {"type": "plane", "plane": "xz"}}}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("an extrusion To Next starts at the profile plane or an offset from it"),
        "{error}"
    );
}
