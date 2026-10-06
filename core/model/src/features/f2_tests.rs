// SPDX-License-Identifier: MIT
//! The face features (F2) in a document with the mock kernel: references,
//! checks, unsupported options, bodies a split creates, and names that
//! later features refer to.

use serde_json::{Value, json};

use crate::FeatureStatus;
use crate::document_tests::{Block, block, corner, def, side, top_edge};
use crate::ids::{BodyUid, FeatureUid};
use crate::topo::{EdgeName, FaceName};

/// Adds a feature; the definition is built before the document is borrowed.
macro_rules! add {
    ($b:ident, $value:expr $(,)?) => {{
        let value: Value = $value;
        $b.doc.add_feature(&def(value), None).map(|added| added.uid)
    }};
}

fn status(b: &Block, uid: FeatureUid) -> FeatureStatus {
    b.doc.status(uid).cloned().expect("recomputed")
}

fn history(b: &Block, body: BodyUid) -> String {
    b.doc
        .body_shape(body)
        .map_or_else(|| "none".to_owned(), |s| s.history.clone())
}

fn top(b: &Block) -> FaceName {
    FaceName::end(b.extrude, b.region.clone())
}

#[test]
fn fillet_sets_and_unsupported_options() {
    let mut b = block();
    let set = |edge: &EdgeName, size: Value| json!({"edges": [edge], "size": size});
    let fillet = add!(
        b,
        json!({"type": "fillet", "body": b.body, "sets": [
            set(&corner(b.extrude, 1, 0), json!({"type": "constant", "radius": 5.0})),
            set(&corner(b.extrude, 1, 2), json!({"type": "chord_length", "length": 2.0})),
            {"faces": [top(&b)], "size": {"type": "variable", "start": 1.0, "end": 2.0}}]}),
    )
    .unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert!(history(&b, b.body).starts_with("fillet(prism(F2:0..20),5;ChordLength"));
    // The parameters are the set's slots.
    let names: Vec<String> = b
        .doc
        .parameters()
        .iter()
        .map(|p| p.comment().to_owned())
        .collect();
    assert!(names.contains(&"Fillet1 radius".to_owned()), "{names:?}");
    assert!(
        names.contains(&"Fillet1 chord_length".to_owned()),
        "{names:?}"
    );
    assert!(
        names.contains(&"Fillet1 end_radius".to_owned()),
        "{names:?}"
    );

    // G2 with its weight, asymmetric distances on a reference face and mid
    // radii reach the kernel (P6).
    for (set, expected) in [
        (
            json!({"size": {"type": "constant", "radius": 1.0}, "continuity": "curvature",
                   "tangency_weight": 1.5}),
            "1,G2 weight 1.5",
        ),
        (
            json!({"size": {"type": "asymmetric", "distance1": 1.0, "distance2": 2.0, "flip": true},
                   "reference_face": side(b.extrude, 1, 1)}),
            "Asymmetric { distance1: 1.0, distance2: 2.0, reference: Some(",
        ),
        (
            json!({"size": {"type": "variable", "start": 1.0, "end": 2.0,
                   "mid": [{"position": 0.5, "radius": 3.0}]}}),
            "mid: [(0.5, 3.0)]",
        ),
    ] {
        let mut set = set;
        set["edges"] = json!([corner(b.extrude, 1, 1)]);
        let uid = add!(b, json!({"type": "fillet", "body": b.body, "sets": [set]}),).unwrap();
        assert_eq!(status(&b, uid), FeatureStatus::Ok);
        let made = history(&b, b.body);
        assert!(made.contains(expected), "{made}");
        b.doc.delete_feature(uid, false).unwrap();
    }
    // The weight's range and the reference face's use are checked.
    for (set, error) in [
        (
            json!({"size": {"type": "constant", "radius": 1.0}, "continuity": "curvature",
                   "tangency_weight": 3.0}),
            "tangency weight must be between 0.1 and 2",
        ),
        (
            json!({"size": {"type": "constant", "radius": 1.0},
                   "reference_face": side(b.extrude, 1, 1)}),
            "only for asymmetric fillets",
        ),
    ] {
        let mut set = set;
        set["edges"] = json!([corner(b.extrude, 1, 1)]);
        let message = add!(b, json!({"type": "fillet", "body": b.body, "sets": [set]}))
            .unwrap_err()
            .to_string();
        assert!(message.contains(error), "{message}");
    }
    assert!(history(&b, b.body).starts_with("fillet(prism"));
}

#[test]
fn fillet_and_chamfer_checks() {
    let mut b = block();
    let edge = corner(b.extrude, 1, 0);
    for (value, expected) in [
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"edges": [edge], "size": {"type": "constant", "radius": 1.0}},
                {"edges": [edge], "size": {"type": "constant", "radius": 2.0}}]}),
            "sets[1]: edge",
        ),
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"size": {"type": "constant", "radius": 1.0}}]}),
            "no edges selected",
        ),
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"edges": [edge], "size": {"type": "variable", "start": 1.0, "end": 2.0,
                 "mid": [{"position": 1.5, "radius": 3.0}]}}]}),
            "between 0 and 1",
        ),
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"faces": ["F9:side(c1)"], "size": {"type": "constant", "radius": 1.0}}]}),
            "face F9:side(c1): feature F9 does not exist",
        ),
        (
            json!({"type": "chamfer", "body": b.body, "sets": [
                {"edges": [edge], "size": {"type": "two_distances", "distance1": 1.0, "distance2": 0.0},
                 "reference_face": side(b.extrude, 1, 0)}]}),
            "chamfer distance must be greater than zero",
        ),
    ] {
        let error = add!(b, value).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }

    // A chamfer set with a reference face reaches the kernel with it, and so
    // does the corner type.
    let chamfer = add!(
        b,
        json!({"type": "chamfer", "body": b.body, "sets": [
            {"edges": [edge], "size": {"type": "two_distances", "distance1": 1.0, "distance2": 3.0},
             "reference_face": side(b.extrude, 1, 0)}]}),
    )
    .unwrap();
    assert_eq!(status(&b, chamfer), FeatureStatus::Ok);
    assert!(history(&b, b.body).contains(&format!(",on {}", side(b.extrude, 1, 0))));
    let miter = add!(
        b,
        json!({"type": "chamfer", "body": b.body, "edges": [corner(b.extrude, 1, 2)],
               "size": {"type": "equal_distance", "distance": 1.0}, "corner": "miter"}),
    )
    .unwrap();
    assert_eq!(status(&b, miter), FeatureStatus::Ok);
    assert!(history(&b, b.body).contains(",Miter corners"));
}

#[test]
fn shell_faces_are_references_for_later_features() {
    let mut b = block();
    let shell = add!(
        b,
        json!({"type": "shell", "body": b.body, "faces": [top(&b)], "inside": 2.0}),
    )
    .unwrap();
    assert_eq!(status(&b, shell), FeatureStatus::Ok);
    assert_eq!(history(&b, b.body), "shell(prism(F2:0..20),2,0)");
    // The inner wall of side 0 meets the inner bottom.
    let inner = |face: FaceName| {
        FaceName::new(
            shell,
            "offset",
            Some(crate::topo::RoleKey::Face(Box::new(face))),
        )
    };
    let wall = inner(side(b.extrude, 1, 0));
    let fillet = add!(
        b,
        json!({"type": "fillet", "body": b.body, "sets": [
            {"faces": [wall], "size": {"type": "constant", "radius": 0.5}}]}),
    )
    .unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    // The shell cannot be deleted while the fillet refers to its face.
    let error = b.doc.delete_feature(shell, false).unwrap_err().to_string();
    assert!(error.contains("is used by"), "{error}");

    // Neither thickness, or a negative one, is rejected.
    for value in [
        json!({"type": "shell", "body": b.body}),
        json!({"type": "shell", "body": b.body, "inside": 0.0}),
        json!({"type": "shell", "body": b.body, "inside": -1.0, "outside": 2.0}),
    ] {
        let error = add!(b, value).unwrap_err().to_string();
        assert!(error.contains("thickness"), "{error}");
    }
}

#[test]
fn missing_faces_are_explained() {
    let mut b = block();
    let gone = FaceName::end(FeatureUid(1), b.region.clone());
    let delete = add!(
        b,
        json!({"type": "delete_face", "body": b.body, "faces": [gone]}),
    )
    .unwrap();
    let FeatureStatus::Failed(message) = status(&b, delete) else {
        panic!("expected a failure");
    };
    assert_eq!(message, format!("the body has no face {gone}"));

    // A face the kernel lost after a feature.
    let mut b = block();
    let offset = add!(
        b,
        json!({"type": "delete_face", "body": b.body, "faces": [side(b.extrude, 1, 0)]}),
    )
    .unwrap();
    assert_eq!(status(&b, offset), FeatureStatus::Ok);
    let after = add!(
        b,
        json!({"type": "offset_face", "body": b.body, "faces": [side(b.extrude, 1, 0)],
               "distance": 1.0}),
    )
    .unwrap();
    let FeatureStatus::Failed(message) = status(&b, after) else {
        panic!("expected a failure");
    };
    assert!(
        message.contains("no longer exists after DeleteFace1"),
        "{message}"
    );

    // Replace face is not in the mock kernel: unsupported, with the mark.
    let replace = add!(
        b,
        json!({"type": "replace_face", "body": b.body, "faces": [top(&b)],
               "target": {"type": "plane", "origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 2.0]}}),
    )
    .unwrap();
    assert!(matches!(status(&b, replace),
        FeatureStatus::Failed(m) if m == "unsupported: the geometry kernel does not support replace face"));
}

#[test]
fn replace_face_targets_any_surface_of_another_body() {
    let mut b = block();
    // A second body: Extrude2 (F3) of the same rectangle.
    let other = add!(
        b,
        json!({"type": "extrude", "profiles": [{"sketch": b.sketch, "region": b.region}],
               "extent": {"type": "distance", "distance": 30.0}, "operation": "new_body"}),
    )
    .unwrap();
    let other_body = BodyUid::new(other, 0);
    let other_side = side(other, 1, 0);
    // A face (of any surface type) or a body passes the check; the mock
    // kernel has no replace face.
    for target in [
        json!({"body": other_body, "face": other_side}),
        json!({"body": other_body}),
        json!({"body": b.body, "face": side(b.extrude, 1, 0)}),
    ] {
        let uid = add!(
            b,
            json!({"type": "replace_face", "body": b.body, "faces": [top(&b)], "target": target}),
        )
        .unwrap();
        assert!(
            matches!(status(&b, uid), FeatureStatus::Failed(m) if m.starts_with("unsupported:"))
        );
    }
    let saved = serde_json::to_value(def(json!({"type": "replace_face", "body": b.body,
        "faces": [top(&b)], "target": {"body": other_body}})))
    .unwrap();
    assert_eq!(saved["target"], json!({"body": other_body.to_string()}));
    for (target, offset, expected) in [
        (
            json!({"body": b.body}),
            None,
            "the target: body F2.b0 cannot replace its own faces",
        ),
        (
            json!({"body": other_body, "face": other_side}),
            Some(2.0),
            "only a plane can be offset",
        ),
        (
            json!({"sketch": b.sketch, "curve": "c1"}),
            None,
            "is not a plane, a face or a body",
        ),
    ] {
        let mut value =
            json!({"type": "replace_face", "body": b.body, "faces": [top(&b)], "target": target});
        if let Some(offset) = offset {
            value["offset"] = json!(offset);
        }
        let error = add!(b, value).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn split_body_creates_bodies_named_like_new_bodies() {
    let mut b = block();
    let split = add!(
        b,
        json!({"type": "split_body", "bodies": [b.body],
               "tool": {"type": "origin_plane", "plane": "yz", "offset": 20.0}}),
    )
    .unwrap();
    assert_eq!(status(&b, split), FeatureStatus::Ok);
    let bodies: Vec<(BodyUid, String)> = b
        .doc
        .bodies()
        .iter()
        .map(|v| (v.uid, v.name.clone()))
        .collect();
    assert_eq!(
        bodies,
        vec![
            (b.body, "Body1".to_owned()),
            (BodyUid::new(split, 0), "Body2".to_owned())
        ]
    );
    assert_eq!(history(&b, b.body), "split(prism(F2:0..20))#0");
    // The offset is the split's parameter; a later feature works on the new body.
    let parameters = b.doc.parameters();
    let offset = parameters
        .iter()
        .find(|p| parameters.owner(p.id()) == Some(split))
        .unwrap();
    assert_eq!(offset.comment(), "SplitBody1 offset");
    let piece = BodyUid::new(split, 0);
    let fillet = add!(
        b,
        json!({"type": "fillet", "body": piece, "edges": [top_edge(b.extrude, 1, &b.region, 0)],
               "radius": 1.0}),
    )
    .unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert!(history(&b, piece).starts_with("fillet(split(prism(F2:0..20))#1"));

    for (value, expected) in [
        (
            json!({"type": "split_body", "bodies": [b.body], "tool": {"type": "body", "body": b.body}}),
            "cannot split itself",
        ),
        (
            json!({"type": "split_body", "bodies": [],
                   "tool": {"type": "plane", "origin": [0.0, 0.0, 0.0], "normal": [1.0, 0.0, 0.0]}}),
            "no bodies",
        ),
        (
            json!({"type": "split_body", "bodies": [b.body],
                   "tool": {"type": "plane", "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 0.0]}}),
            "normal is zero",
        ),
        (
            json!({"type": "draft", "body": b.body, "faces": [side(b.extrude, 1, 0)],
                   "plane": {"type": "body", "body": b.body}, "angle": 0.1}),
            "the fixed plane: body F2.b0 is not a plane or a planar face",
        ),
        (
            json!({"type": "split_face", "body": b.body, "faces": [top(&b)],
                   "tool": {"type": "sketch", "sketch": b.sketch, "curves": ["c9"]}}),
            "has no curve c9",
        ),
        (
            json!({"type": "split_body", "bodies": [b.body], "tool": {"body": b.body,
                   "face": top(&b)}, "offset": 2.0}),
            "only a plane can be offset",
        ),
    ] {
        let error = add!(b, value).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
    // A splitting face names its body: the earlier form without one is not
    // read (the other face features take their own body).
    let error = serde_json::from_value::<crate::features::FeatureDef<crate::ValueInput>>(
        json!({"type": "split_body", "bodies": [b.body], "tool": {"type": "face", "face": top(&b)}}),
    )
    .unwrap_err();
    assert!(error.to_string().contains("needs its body"), "{error}");
}

#[test]
fn earlier_tool_forms_are_read_and_written_unified() {
    let b = block();
    let read = |value: Value| serde_json::to_value(def(value)).unwrap();
    // The origin plane's offset moves to the feature.
    let split = read(json!({"type": "split_body", "bodies": [b.body],
                            "tool": {"type": "origin_plane", "plane": "yz", "offset": 20.0}}));
    assert_eq!(split["tool"], json!("yz"));
    assert_eq!(split["offset"], json!(20.0));
    // A face without a body is on the feature's own body.
    let draft = read(
        json!({"type": "draft", "body": b.body, "faces": [side(b.extrude, 1, 0)],
                            "plane": {"type": "face", "face": top(&b)}, "angle": 0.1}),
    );
    assert_eq!(
        draft["plane"],
        json!({"body": b.body.to_string(), "face": top(&b).to_string()})
    );
    // Sketch curves' direction moves to the feature.
    let face = read(
        json!({"type": "split_face", "body": b.body, "faces": [top(&b)],
                           "tool": {"type": "sketch", "sketch": b.sketch, "curves": ["c1"],
                                    "direction": [0.0, 0.0, 1.0]}}),
    );
    assert_eq!(
        face["tool"],
        json!({"sketch": b.sketch.to_string(), "curve": "c1"})
    );
    assert_eq!(face["direction"], json!([0.0, 0.0, 1.0]));
    let fixed = read(
        json!({"type": "replace_face", "body": b.body, "faces": [top(&b)],
                            "target": {"type": "plane", "origin": [0.0, 0.0, 25.0],
                                       "normal": [0.0, 0.0, 1.0]}}),
    );
    assert_eq!(
        fixed["target"],
        json!({"origin": [0.0, 0.0, 25.0], "normal": [0.0, 0.0, 1.0]})
    );
}

#[test]
fn split_face_pieces_keep_the_name() {
    let mut b = block();
    let split = add!(
        b,
        json!({"type": "split_face", "body": b.body, "faces": [top(&b)],
               "tool": {"type": "sketch", "sketch": b.sketch}}),
    )
    .unwrap();
    assert_eq!(status(&b, split), FeatureStatus::Ok);
    let shape = b.doc.body_shape(b.body).unwrap();
    assert!(shape.faces.contains(&top(&b).piece(0)));
    assert!(shape.faces.contains(&top(&b).piece(1)));
    // A reference to the face means every piece.
    let offset = add!(
        b,
        json!({"type": "offset_face", "body": b.body, "faces": [top(&b).piece(1)], "distance": 2.0}),
    )
    .unwrap();
    assert_eq!(status(&b, offset), FeatureStatus::Ok);
    let draft = add!(
        b,
        json!({"type": "draft", "body": b.body, "faces": [side(b.extrude, 1, 0)],
               "plane": {"type": "face", "face": FaceName::start(b.extrude, b.region.clone())},
               "angle": 0.1, "symmetric": true}),
    )
    .unwrap();
    assert_eq!(status(&b, draft), FeatureStatus::Ok);
    // Files keep the definitions.
    let json = b.doc.to_json();
    let loaded = crate::Document::from_json(&json, crate::testing::MockKernel::default()).unwrap();
    assert_eq!(loaded.state(), b.doc.state());
    assert_eq!(loaded.to_json(), json);
}
