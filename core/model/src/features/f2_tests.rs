// SPDX-License-Identifier: MIT
//! The face features (F2) in a document with the mock kernel: references,
//! checks, unsupported options, bodies a split creates, and names that
//! later features refer to.

use serde_json::{Value, json};

use crate::FeatureStatus;
use crate::features::SketchPlane;
use crate::ids::{BodyUid, FeatureUid};
use crate::test_support::{Block, block, corner, def, extrude, num, side, top_edge};
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
        // A spline radius (mitcad#464).
        (
            json!({"size": {"type": "spline", "knots": [0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                   "radii": [1.0, 3.0, 2.0]}}),
            "Spline { start_vertex: None, knots: [0.0, 0.0, 0.0, 1.0, 1.0, 1.0], radii: [1.0, 3.0, 2.0] }",
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
                {"edges": [edge], "size": {"type": "spline", "knots": [0.0, 0.0, 1.0],
                 "radii": [1.0, 2.0]}}]}),
            "degree 1 to 3",
        ),
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"edges": [edge], "size": {"type": "spline", "knots": [0.0, 0.0, 1.0, 1.0],
                 "radii": [1.0, 0.0]}}]}),
            "fillet control radius must be greater than zero",
        ),
        (
            json!({"type": "fillet", "body": b.body, "sets": [
                {"edges": [edge], "size": {"type": "spline", "knots": [0.0, 0.0, 1.0, 1.0],
                 "radii": [1.0, 2.0]}, "continuity": "curvature"}]}),
            "not taken with curvature continuity",
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

    // The end of the wall at the removed top.
    let cap = FaceName::new(
        shell,
        "offset_cap",
        Some(crate::topo::RoleKey::Face(Box::new(top(&b)))),
    );
    let chamfer = add!(
        b,
        json!({"type": "chamfer", "body": b.body, "sets": [
            {"faces": [cap], "size": {"type": "equal_distance", "distance": 0.2}}]}),
    )
    .unwrap();
    assert_eq!(status(&b, chamfer), FeatureStatus::Ok);

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

/// A shell whose wall does not fit under some faces (parts no thicker than
/// the wall, left solid, mitcad#1233) succeeds with the kernel's note as its
/// warning; the outside keeps its names.
#[test]
fn shell_warns_where_the_wall_does_not_fit() {
    let mut b = block();
    const NOTE: &str = "the wall does not fit under face F2:side(c1); it is left solid there";
    b.doc
        .kernel()
        .notes
        .borrow_mut()
        .insert("shell", NOTE.to_owned());
    let shell = add!(
        b,
        json!({"type": "shell", "body": b.body, "faces": [top(&b)], "inside": 3.0}),
    )
    .unwrap();
    assert_eq!(status(&b, shell), FeatureStatus::Ok);
    assert_eq!(b.doc.warnings(shell), [NOTE.to_owned()]);
    // The outside face the note names is a reference as before.
    let fillet = add!(
        b,
        json!({"type": "fillet", "body": b.body, "sets": [
            {"faces": [side(b.extrude, 1, 0)], "size": {"type": "constant", "radius": 0.5}}]}),
    )
    .unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert!(b.doc.warnings(fillet).is_empty());
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

#[test]
fn split_body_by_several_tools_divides_the_pieces_each_reaches() {
    let mut b = block();
    let split = add!(
        b,
        json!({"type": "split_body", "bodies": [b.body],
               "tool": {"type": "origin_plane", "plane": "yz"},
               "more_tools": [{"type": "origin_plane", "plane": "xz"}]}),
    )
    .unwrap();
    assert_eq!(status(&b, split), FeatureStatus::Ok);
    // Two pieces by the first tool, each in two by the second.
    let uids: Vec<BodyUid> = b.doc.bodies().iter().map(|v| v.uid).collect();
    assert_eq!(
        uids,
        vec![
            b.body,
            BodyUid::new(split, 0),
            BodyUid::new(split, 1),
            BodyUid::new(split, 2)
        ]
    );
    // The further tools are read back with the definition.
    let def = b.doc.feature(split).unwrap().def.clone();
    let crate::features::FeatureDef::SplitBody(def) = def else {
        panic!("not a split");
    };
    assert_eq!(def.more_tools.len(), 1);
}

#[test]
fn a_split_previews_its_extended_tool_past_the_bodies() {
    // The block is 60 x 40 x 20 mm from the origin: its box's diagonal is
    // about 74.8 mm, so the preview reaches 5 % of that past every side.
    let mut b = block();
    let margin = 5600f64.sqrt() * 0.05;
    let reaches = |b: &Block| {
        let tool = b.doc.preview_tool().expect("an extended tool");
        let bounds = tool.bounds.expect("bounds");
        for (i, (low, high)) in [(0.0, 60.0), (0.0, 40.0), (0.0, 20.0)]
            .into_iter()
            .enumerate()
        {
            assert!((bounds.min[i] - (low - margin)).abs() < 1e-6, "{bounds:?}");
            assert!((bounds.max[i] - (high + margin)).abs() < 1e-6, "{bounds:?}");
        }
        tool.history.clone()
    };
    let face = json!({"body": b.body, "face": top(&b)});
    // Split Body by a face, extended (the default): the face grown through
    // the body, clipped a little past it.
    let split = def(json!({"type": "split_body", "bodies": [b.body], "tool": face}));
    b.doc.preview_add(&split, None).unwrap();
    assert_eq!(reaches(&b), format!("extended_tool(face {},true)", top(&b)));
    // The same preview again takes the split from the cache and still
    // shows the tool: it is made for every preview, never cached.
    let made = b.doc.kernel().count("extended_tool");
    let splits = b.doc.kernel().count("split_body");
    let report = b.doc.preview_add(&split, None).unwrap();
    assert!(report.tool);
    assert_eq!(b.doc.kernel().count("split_body"), splits);
    assert_eq!(b.doc.kernel().count("extended_tool"), made + 1);
    reaches(&b);
    // Committing reuses the preview's result and makes no tool; nor does
    // a split that is evaluated outside a preview (a recompute, an import).
    let made = b.doc.kernel().count("extended_tool");
    let committed = b.doc.add_feature(&split, None).unwrap().uid;
    assert_eq!(b.doc.kernel().count("split_body"), splits);
    let again = def(
        json!({"type": "split_body", "bodies": [b.body], "tool": face,
                           "more_tools": ["xz"]}),
    );
    b.doc.add_feature(&again, None).unwrap();
    assert!(b.doc.kernel().count("split_body") > splits);
    assert_eq!(b.doc.kernel().count("extended_tool"), made);
    assert!(b.doc.preview_tool().is_none());
    b.doc.undo();
    b.doc.undo();
    assert!(b.doc.feature(committed).is_none());
    // Not extended: the preview shows the tool as picked, nothing more.
    let plain = def(
        json!({"type": "split_body", "bodies": [b.body], "tool": face,
                           "extend": false}),
    );
    b.doc.preview_add(&plain, None).unwrap();
    assert!(b.doc.preview_tool().is_none());
    // Split Face by a plane.
    let split = def(
        json!({"type": "split_face", "body": b.body, "faces": [top(&b)],
                           "tool": "yz", "offset": 20.0}),
    );
    b.doc.preview_add(&split, None).unwrap();
    assert_eq!(reaches(&b), "extended_tool(plane,true)");
    // A body tool is not extended.
    let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap().uid;
    let region = b
        .doc
        .add_rectangle(sketch, [10.0, 10.0], &num(5.0), &num(5.0))
        .unwrap()
        .region;
    let other = b
        .doc
        .add_feature(&extrude(sketch, &[&region], 50.0, "new_body", &[]), None)
        .unwrap()
        .uid;
    let by_body = def(json!({"type": "split_body", "bodies": [b.body],
                             "tool": {"type": "body", "body": BodyUid::new(other, 0)}}));
    b.doc.preview_add(&by_body, None).unwrap();
    assert!(b.doc.preview_tool().is_none());
    // A kernel that cannot extend a tool still splits.
    b.doc.kernel().fail.borrow_mut().insert("extended_tool");
    let split = def(json!({"type": "split_body", "bodies": [b.body], "tool": face}));
    let report = b.doc.preview_add(&split, None).unwrap();
    assert_eq!(report.status, FeatureStatus::Ok);
    assert!(b.doc.preview_tool().is_none());
}

#[test]
fn move_face_moves_faces_by_a_rigid_motion() {
    let mut b = block();
    let moved = add!(
        b,
        json!({"type": "move_face", "body": b.body, "faces": [side(b.extrude, 1, 1)],
               "transform": {"type": "translate_xyz", "x": 3.0, "y": 0.0, "z": 0.0}}),
    )
    .unwrap();
    assert_eq!(status(&b, moved), FeatureStatus::Ok);
    assert_eq!(
        history(&b, b.body),
        "move_faces(prism(F2:0..20),1 faces,3,0,0)"
    );
    // A free move's matrix (rows of a rotation and a translation).
    let free = add!(
        b,
        json!({"type": "move_face", "body": b.body, "faces": [top(&b)],
               "transform": {"type": "free", "matrix": [[1.0, 0.0, 0.0, 0.0],
                                                        [0.0, 1.0, 0.0, -2.0],
                                                        [0.0, 0.0, 1.0, 1.5]]}}),
    )
    .unwrap();
    assert_eq!(status(&b, free), FeatureStatus::Ok);
    assert!(history(&b, b.body).ends_with("1 faces,0,-2,1.5)"));
    // A motion that moves nothing, a face the body lacks.
    let still = add!(
        b,
        json!({"type": "move_face", "body": b.body, "faces": [top(&b)],
               "transform": {"type": "translate_xyz", "x": 0.0, "y": 0.0, "z": 0.0}}),
    )
    .unwrap();
    assert!(matches!(status(&b, still),
        FeatureStatus::Failed(m) if m == "the faces' motion moves nothing"));
    let scaled = add!(
        b,
        json!({"type": "move_face", "body": b.body, "faces": [top(&b)],
               "transform": {"type": "free", "matrix": [[2.0, 0.0, 0.0, 0.0],
                                                        [0.0, 1.0, 0.0, 0.0],
                                                        [0.0, 0.0, 1.0, 0.0]]}}),
    );
    assert!(scaled.is_err() || matches!(status(&b, scaled.unwrap()), FeatureStatus::Failed(_)));
}

/// The faces a fillet or chamfer made on the body.
fn dressed_faces(b: &Block, role: &str) -> usize {
    b.doc.body_shape(b.body).map_or(0, |s| {
        s.faces
            .iter()
            .filter(|f| f.to_string().contains(&format!(":{role}(")))
            .count()
    })
}

#[test]
fn a_set_of_every_edge_follows_the_body() {
    // mitcad#793: a set of every edge (the FreeCAD import's "use all
    // edges") lists none; it rounds the edges the body has when the fillet
    // is computed, also after an edit upstream takes some away.
    let mut b = block();
    let (_, _, boss) = b.join_boss();
    let with_boss = b.doc.body_shape(b.body).unwrap().edges.len();
    assert!(with_boss > 12, "{with_boss}");
    let fillet = add!(
        b,
        json!({"type": "fillet", "body": b.body, "sets": [
            {"all_edges": true, "size": {"type": "constant", "radius": 1.0}}]}),
    )
    .unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert!(dressed_faces(&b, "fillet") >= with_boss);
    // The definition keeps the whole-body set, not a list of edges.
    let crate::features::FeatureDef::Fillet(def) = &b.doc.feature(fillet).unwrap().def else {
        panic!("a fillet");
    };
    assert!(def.sets[0].all_edges && def.sets[0].edges.is_empty());
    let named = def
        .map_params(&mut |slot, _| Ok::<_, ()>(slot.to_owned()))
        .unwrap();
    let written = serde_json::to_value(&named).unwrap();
    assert_eq!(written["sets"][0]["all_edges"], true, "{written}");
    assert!(written.get("edges").is_none(), "{written}");
    // Without the boss the block's twelve edges are rounded, with no error
    // about edges that are gone.
    b.doc.set_suppressed(boss, true).unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert_eq!(dressed_faces(&b, "fillet"), 12);
    b.doc.set_suppressed(boss, false).unwrap();
    assert_eq!(status(&b, fillet), FeatureStatus::Ok);
    assert!(dressed_faces(&b, "fillet") >= with_boss);

    // Chamfers take the same set; a whole-body set lists nothing else and
    // is the only set.
    let mut c = block();
    let chamfer = add!(
        c,
        json!({"type": "chamfer", "body": c.body, "sets": [
            {"all_edges": true, "size": {"type": "equal_distance", "distance": 1.0}}]}),
    )
    .unwrap();
    assert_eq!(status(&c, chamfer), FeatureStatus::Ok);
    assert_eq!(dressed_faces(&c, "chamfer"), 12);
    let edge = corner(c.extrude, 1, 0);
    for (value, expected) in [
        (
            json!({"type": "fillet", "body": c.body, "sets": [
                {"all_edges": true, "edges": [edge], "size": {"type": "constant", "radius": 1.0}}]}),
            "lists no edges or faces",
        ),
        (
            json!({"type": "chamfer", "body": c.body, "sets": [
                {"all_edges": true, "size": {"type": "equal_distance", "distance": 1.0}},
                {"edges": [edge], "size": {"type": "equal_distance", "distance": 2.0}}]}),
            "the only set",
        ),
        (
            json!({"type": "fillet", "body": c.body, "sets": [
                {"all_edges": true, "size": {"type": "variable", "start": 1.0, "end": 2.0,
                 "start_vertex": "V{F2:a|F2:b|F2:c}"}}]}),
            "no start vertex",
        ),
    ] {
        let error = add!(c, value).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}
