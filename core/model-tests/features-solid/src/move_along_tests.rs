// SPDX-License-Identifier: MIT
//! Point to Point along one axis (mitcad#2339): `along` keeps only the
//! distance along X, Y or Z of the motion from the origin point to the
//! target point, so that a cylinder is lowered onto a block's top in Z
//! without moving sideways; `all` (left out) is the whole motion, as
//! before. With the mock kernel: its vertices and circular edges are where
//! the test puts them.

use serde_json::{Value, json};

use mitcad_model::datum::CurveGeometry;
use mitcad_model::features::moves::{MoveAlong, MoveSpec};
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::test_support::def;
use mitcad_model::testing::MockKernel;
use mitcad_model::topo::{EdgeName, VertexName};
use mitcad_model::{Document, FeatureStatus};
use mitcad_model_api::DocumentApi;

/// The block's top corner at (40, 30, 20), the target.
const TARGET: [f64; 3] = [40.0, 30.0, 20.0];
/// The cylinder's bottom vertex (on its seam) and the centre of its
/// bottom circle, the origins.
const BOTTOM_VERTEX: [f64; 3] = [85.0, 10.0, 0.0];
const BOTTOM_CENTRE: [f64; 3] = [80.0, 10.0, 0.0];

struct Design {
    doc: Document<MockKernel>,
    /// The block's top corner and the cylinder's bottom vertex.
    corner: String,
    seam: String,
    /// The cylinder's bottom edge, a circle about BOTTOM_CENTRE.
    rim: String,
}

/// A 40 x 30 x 20 block (F1.b0) and a cylinder of diameter 10 and height
/// 15 standing beside it on the XY plane (F2.b0).
fn design() -> Design {
    let mut doc = Document::new(MockKernel::default());
    add(
        &mut doc,
        json!({"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0],
               "length": 40, "width": 30, "height": 20, "operation": "new_body"}),
    );
    add(
        &mut doc,
        json!({"type": "cylinder", "plane": {"type": "origin", "plane": "xy"}, "center": [80, 10],
               "diameter": 10, "height": 15, "operation": "new_body"}),
    );
    let corner = "V{F1:side1|F1:side2|F1:top}"
        .parse::<VertexName>()
        .unwrap()
        .to_string();
    let seam = "V{F2:bottom|F2:side0}"
        .parse::<VertexName>()
        .unwrap()
        .to_string();
    let rim = "E{F2:bottom|F2:side0}"
        .parse::<EdgeName>()
        .unwrap()
        .to_string();
    {
        let kernel = doc.kernel();
        let mut points = kernel.vertex_points.borrow_mut();
        points.insert(corner.clone(), TARGET);
        points.insert(seam.clone(), BOTTOM_VERTEX);
        kernel.edge_curves.borrow_mut().insert(
            rim.clone(),
            CurveGeometry::Circle {
                center: BOTTOM_CENTRE,
                normal: [0.0, 0.0, -1.0],
                radius: 5.0,
                start: BOTTOM_VERTEX,
                end: BOTTOM_VERTEX,
            },
        );
    }
    Design {
        doc,
        corner,
        seam,
        rim,
    }
}

fn add(doc: &mut Document<MockKernel>, value: Value) -> FeatureUid {
    let uid = doc.add_feature(&def(value), None).unwrap().uid;
    let status = doc.status(uid).cloned().expect("recomputed");
    assert_eq!(status, FeatureStatus::Ok, "{status:?}");
    uid
}

/// The low corner of a body's bounds.
fn low(doc: &Document<MockKernel>, body: &str) -> [f64; 3] {
    let uid: BodyUid = body.parse().unwrap();
    doc.body_shape(uid)
        .unwrap_or_else(|| panic!("no body {body}"))
        .bounds
        .expect("bounds")
        .min
}

fn assert_moved(before: [f64; 3], after: [f64; 3], by: [f64; 3]) {
    assert!(
        (0..3).all(|i| (after[i] - before[i] - by[i]).abs() <= 1e-6),
        "moved by {:?}, expected {by:?}",
        std::array::from_fn::<f64, 3, _>(|i| after[i] - before[i])
    );
}

/// A Point to Point move of the cylinder from `from` to the block's top
/// corner; `along` left out when None.
fn move_def(design: &Design, from: Value, along: Option<&str>, copy: bool) -> Value {
    let mut transform = json!({"type": "point_to_point", "from": from,
                               "to": {"body": "F1.b0", "vertex": design.corner}});
    if let Some(along) = along {
        transform["along"] = json!(along);
    }
    json!({"type": "move", "bodies": ["F2.b0"], "copy": copy, "transform": transform})
}

/// The move's transform as the `feature` query gives its definition.
fn saved_transform(doc: &Document<MockKernel>, uid: FeatureUid) -> Value {
    let query = json!({"query": "feature", "uid": uid});
    let feature: Value = serde_json::from_str(&doc.query(&query.to_string()).unwrap()).unwrap();
    feature["def"]["transform"].clone()
}

fn stored_along(doc: &Document<MockKernel>, uid: FeatureUid) -> MoveAlong {
    let FeatureDef::Move(m) = &doc.feature(uid).unwrap().def else {
        panic!("not a move")
    };
    let MoveSpec::PointToPoint { along, .. } = &m.transform else {
        panic!("{:?}", m.transform)
    };
    *along
}

/// Along Z from the cylinder's bottom vertex to the block's top corner:
/// the cylinder goes up 20 mm onto the top's level and nowhere else (X and
/// Y within 1e-6 mm). The definition keeps `along`.
#[test]
fn along_z_from_a_vertex_changes_only_z() {
    let mut d = design();
    let before = low(&d.doc, "F2.b0");
    let block = low(&d.doc, "F1.b0");
    let from = json!({"body": "F2.b0", "vertex": d.seam});
    let moved = move_def(&d, from, Some("z"), false);
    let uid = add(&mut d.doc, moved);
    assert_moved(before, low(&d.doc, "F2.b0"), [0.0, 0.0, 20.0]);
    assert_eq!(stored_along(&d.doc, uid), MoveAlong::Z);
    let saved = saved_transform(&d.doc, uid);
    assert_eq!(saved["along"], "z", "{saved}");
    // The block stays.
    assert_moved(block, low(&d.doc, "F1.b0"), [0.0; 3]);
}

/// The cylinder's round bottom edge as the origin is its centre: along Z
/// the same 20 mm up, X and Y unchanged.
#[test]
fn along_z_from_a_circular_edge_takes_its_centre() {
    let mut d = design();
    let before = low(&d.doc, "F2.b0");
    let from = json!({"body": "F2.b0", "edge": d.rim});
    let moved = move_def(&d, from, Some("z"), false);
    add(&mut d.doc, moved);
    assert_moved(before, low(&d.doc, "F2.b0"), [0.0, 0.0, 20.0]);
}

/// `all`, or `along` left out, is the whole motion, as before; it is not
/// written into the definition. X and Y keep their own distance only.
#[test]
fn all_is_the_whole_motion_and_x_and_y_keep_their_own() {
    let whole = [
        TARGET[0] - BOTTOM_CENTRE[0],
        TARGET[1] - BOTTOM_CENTRE[1],
        TARGET[2] - BOTTOM_CENTRE[2],
    ];
    for (along, by) in [
        (None, whole),
        (Some("all"), whole),
        (Some("x"), [whole[0], 0.0, 0.0]),
        (Some("y"), [0.0, whole[1], 0.0]),
    ] {
        let mut d = design();
        let before = low(&d.doc, "F2.b0");
        let from = json!({"body": "F2.b0", "edge": d.rim});
        let moved = move_def(&d, from, along, false);
        let uid = add(&mut d.doc, moved);
        assert_moved(before, low(&d.doc, "F2.b0"), by);
        let saved = saved_transform(&d.doc, uid);
        if along.is_none_or(|a| a == "all") {
            assert_eq!(stored_along(&d.doc, uid), MoveAlong::All);
            assert!(saved.get("along").is_none(), "{saved}");
        } else {
            assert_eq!(saved["along"], json!(along), "{saved}");
        }
    }
}

/// A copy keeps the same rule: the copy goes up 20 mm only, the cylinder
/// stays where it was. Edited to `all`, the copy goes the whole way.
#[test]
fn a_copy_along_z_moves_only_in_z() {
    let mut d = design();
    let before = low(&d.doc, "F2.b0");
    let from = json!({"body": "F2.b0", "vertex": d.seam});
    let copied = move_def(&d, from.clone(), Some("z"), true);
    let uid = add(&mut d.doc, copied);
    let copy = format!("{uid}.b0");
    assert_moved(before, low(&d.doc, &copy), [0.0, 0.0, 20.0]);
    assert_moved(before, low(&d.doc, "F2.b0"), [0.0; 3]);

    let whole = def(move_def(&d, from, Some("all"), true));
    d.doc.edit_feature(uid, &whole).unwrap();
    assert_eq!(
        d.doc.status(uid).cloned().expect("recomputed"),
        FeatureStatus::Ok
    );
    assert_moved(
        before,
        low(&d.doc, &copy),
        [
            TARGET[0] - BOTTOM_VERTEX[0],
            TARGET[1] - BOTTOM_VERTEX[1],
            TARGET[2] - BOTTOM_VERTEX[2],
        ],
    );
    assert_moved(before, low(&d.doc, "F2.b0"), [0.0; 3]);
}

/// A word other than all, x, y and z is refused when the definition is
/// read.
#[test]
fn an_unknown_axis_is_refused() {
    let d = design();
    let from = json!({"body": "F2.b0", "vertex": d.seam});
    let value = move_def(&d, from, Some("w"), false);
    assert!(serde_json::from_value::<FeatureDef<ValueInput>>(value).is_err());
}

/// The keep rule itself: exactly the one component, the others exactly 0.
#[test]
fn keep_takes_exactly_one_component() {
    let motion = [-45.0, 20.0, 20.0];
    assert_eq!(MoveAlong::All.keep(motion), motion);
    assert_eq!(MoveAlong::X.keep(motion), [-45.0, 0.0, 0.0]);
    assert_eq!(MoveAlong::Y.keep(motion), [0.0, 20.0, 0.0]);
    assert_eq!(MoveAlong::Z.keep(motion), [0.0, 0.0, 20.0]);
}
