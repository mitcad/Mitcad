// SPDX-License-Identifier: MIT
//! Convert Mesh's Faceted solid (mitcad#1082): small synthetic meshes made
//! boundaries of planar faces, and the feature on the mock kernel (a new
//! solid body, the mesh body hidden in the same undo step with its Show
//! When condition kept, a failing conversion refused). The OCCT kernel's solid is checked by
//! `cli.mesh_convert_faceted` (`tools/cli/tests/mesh_convert_faceted.json`).

use serde_json::json;

use super::{DEFAULT_ANGLE, MAX_TRIANGLES, faceted_solid};
use crate::exchange::{BaseInput, ImportBody, TriangleMesh};
use crate::features::FeatureDef;
use crate::ids::{BodyUid, FeatureUid};
use crate::test_support::{block, def};
use crate::testing::{MockKernel, MockShape};
use crate::{Document, FeatureStatus};

/// The 12 triangles of the box from `min` to `max`, counter-clockwise seen
/// from outside, sharing its 8 corners.
fn cuboid(min: [f64; 3], max: [f64; 3]) -> TriangleMesh {
    let corner = |i: usize| -> [f64; 3] {
        std::array::from_fn(|axis| {
            if (i >> axis) & 1 == 1 {
                max[axis]
            } else {
                min[axis]
            }
        })
    };
    let quads: [[u32; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    TriangleMesh {
        vertices: (0..8).map(corner).collect(),
        triangles: quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect(),
    }
}

/// The mesh with every triangle's corners its own vertices (a triangle
/// soup, as an STL file stores it), each corner of an odd triangle moved
/// by `shift`.
fn soup(mesh: &TriangleMesh, shift: [f64; 3]) -> TriangleMesh {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for (t, tri) in mesh.triangles.iter().enumerate() {
        let first = vertices.len() as u32;
        for &v in tri {
            let p = mesh.vertices[v as usize];
            vertices.push(if t % 2 == 1 {
                [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]]
            } else {
                p
            });
        }
        triangles.push([first, first + 1, first + 2]);
    }
    TriangleMesh {
        vertices,
        triangles,
    }
}

/// A block 30 x 20 x 10 with a square hole 10 x 10 through it from the
/// bottom to the top: each cap four quads between the outline and the
/// hole, four outer walls and four walls of the hole (32 triangles).
fn block_with_hole() -> TriangleMesh {
    let outline = [[0.0, 0.0], [30.0, 0.0], [30.0, 20.0], [0.0, 20.0]];
    let hole = [[10.0, 5.0], [20.0, 5.0], [20.0, 15.0], [10.0, 15.0]];
    // Bottom: outline 0..4, hole 4..8; top: the same plus 8.
    let mut vertices = Vec::new();
    for z in [0.0, 10.0] {
        for p in outline.iter().chain(hole.iter()) {
            vertices.push([p[0], p[1], z]);
        }
    }
    let top = 8;
    let mut triangles = Vec::new();
    for k in 0..4u32 {
        let (o0, o1) = (k, (k + 1) % 4);
        let (i0, i1) = (4 + k, 4 + (k + 1) % 4);
        // Caps: counter-clockwise from above on top, turned over below.
        triangles.push([o0 + top, o1 + top, i1 + top]);
        triangles.push([o0 + top, i1 + top, i0 + top]);
        triangles.push([o0, i1, o1]);
        triangles.push([o0, i0, i1]);
        // The outer wall along o0 -> o1 and the hole's along i1 -> i0.
        triangles.push([o0, o1, o1 + top]);
        triangles.push([o0, o1 + top, o0 + top]);
        triangles.push([i1, i0, i0 + top]);
        triangles.push([i1, i0 + top, i1 + top]);
    }
    TriangleMesh {
        vertices,
        triangles,
    }
}

/// A cube of 12 triangles is a solid of 6 planar faces, 12 edges and 8
/// vertices: the diagonals of its sides are inside faces.
#[test]
fn a_cube_of_12_triangles_becomes_6_faces() {
    let cube = cuboid([0.0; 3], [10.0; 3]);
    assert_eq!(cube.triangles.len(), 12);
    let faceted = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap();
    let b = &faceted.boundary;
    assert_eq!(b.faces.len(), 6);
    assert_eq!(b.surfaces.len(), 6);
    assert_eq!(b.edges.len(), 12);
    assert_eq!(b.vertices.len(), 8);
    assert_eq!(b.shells, vec![(0..6).collect::<Vec<_>>()]);
    assert_eq!(faceted.shells, 1);
    assert!((faceted.volume - 1000.0).abs() < 1e-9, "{}", faceted.volume);
    assert!((faceted.area - 600.0).abs() < 1e-9, "{}", faceted.area);
    b.check().unwrap();
    for face in &b.faces {
        assert_eq!(face.loops.len(), 1);
        assert_eq!(face.loops[0].len(), 4, "{face:?}");
    }
    // Every edge is used twice, once each way.
    let mut uses = vec![(0, 0); b.edges.len()];
    for (e, forward) in b.faces.iter().flat_map(|f| f.loops.iter().flatten()) {
        if *forward {
            uses[*e].0 += 1;
        } else {
            uses[*e].1 += 1;
        }
    }
    assert!(uses.iter().all(|u| *u == (1, 1)), "{uses:?}");
}

/// Triangles that face inwards, or some of them the wrong way, are turned:
/// the volume comes out positive and the same.
#[test]
fn triangles_are_oriented_consistently_and_outwards() {
    let mut cube = cuboid([0.0; 3], [10.0; 3]);
    for tri in &mut cube.triangles {
        tri.swap(1, 2);
    }
    cube.triangles[3].swap(0, 1);
    let faceted = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.boundary.faces.len(), 6);
    assert!((faceted.volume - 1000.0).abs() < 1e-9, "{}", faceted.volume);
}

/// A block with a through hole is one solid with the block's volume less
/// the hole's (to 1e-6 relative): its top and bottom are one face each
/// with two loops, the outline (larger) first.
#[test]
fn a_block_with_a_through_hole_is_one_solid() {
    let faceted = faceted_solid(&block_with_hole(), 0.01, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.shells, 1);
    let expected = 30.0 * 20.0 * 10.0 - 10.0 * 10.0 * 10.0;
    assert!(
        (faceted.volume - expected).abs() <= 1e-6 * expected,
        "{}",
        faceted.volume
    );
    let b = &faceted.boundary;
    b.check().unwrap();
    assert_eq!(b.faces.len(), 10);
    let holed: Vec<_> = b.faces.iter().filter(|f| f.loops.len() == 2).collect();
    assert_eq!(holed.len(), 2, "{:?}", b.faces);
    for face in holed {
        assert_eq!(face.loops[0].len(), 4);
        assert_eq!(face.loops[1].len(), 4);
        // The outline's edges are 30 and 20 long, the hole's 10.
        let longest = |l: &[(usize, bool)]| {
            l.iter()
                .map(|(e, _)| b.edges[*e].range[1])
                .fold(0.0, f64::max)
        };
        assert!((longest(&face.loops[0]) - 30.0).abs() < 1e-9);
        assert!((longest(&face.loops[1]) - 10.0).abs() < 1e-9);
    }
}

/// A mesh with one triangle removed is refused with its open edges
/// counted; one with a triangle doubled with its non-manifold edges.
#[test]
fn open_and_non_manifold_meshes_are_refused_with_the_count() {
    let mut cube = cuboid([0.0; 3], [10.0; 3]);
    cube.triangles.remove(5);
    let error = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap_err();
    assert!(error.contains("3 open edges"), "{error}");
    assert!(!error.contains("non-manifold"), "{error}");

    let mut cube = cuboid([0.0; 3], [10.0; 3]);
    let extra = cube.triangles[0];
    cube.triangles.push(extra);
    let error = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap_err();
    assert!(error.contains("3 non-manifold edges"), "{error}");
}

/// Vertices 0.005 mm apart are welded at 0.01 mm: a triangle soup of the
/// cube whose odd triangles are 0.005 off becomes the cube; at 0.001 mm
/// they stay apart and every edge is open.
#[test]
fn vertices_within_the_tolerance_are_welded() {
    let cube = soup(&cuboid([0.0; 3], [10.0; 3]), [0.005, 0.0, 0.0]);
    assert_eq!(cube.vertices.len(), 36);
    let faceted = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.boundary.vertices.len(), 8);
    assert_eq!(faceted.welded, 28);
    assert_eq!(faceted.boundary.faces.len(), 6);
    assert!((faceted.volume - 1000.0).abs() < 1.0, "{}", faceted.volume);

    let error = faceted_solid(&cube, 0.001, DEFAULT_ANGLE).unwrap_err();
    assert!(error.contains("open edges"), "{error}");
}

/// Neighbouring triangles merge within the angle and stay apart beyond it;
/// a corner off the first triangle's plane by more than the tolerance
/// keeps them apart too.
#[test]
fn neighbours_merge_within_the_angle_and_the_tolerance() {
    // The cube's top corner at x = y = 10 (vertex 7) raised 0.05 mm: the
    // two triangles of the top (both have it) lie 0.405 degrees apart and
    // the far corner of either 0.05 mm off the other's plane; the sides
    // through it stay planar.
    let mut cube = cuboid([0.0; 3], [10.0; 3]);
    cube.vertices[7][2] += 0.05;
    let merged = faceted_solid(&cube, 0.1, DEFAULT_ANGLE).unwrap();
    assert_eq!(merged.boundary.faces.len(), 6);
    let apart = faceted_solid(&cube, 0.1, 0.1_f64.to_radians()).unwrap();
    assert_eq!(apart.boundary.faces.len(), 7, "the top split");
    let tight = faceted_solid(&cube, 0.01, DEFAULT_ANGLE).unwrap();
    assert_eq!(tight.boundary.faces.len(), 7, "the top split");
    tight.boundary.check().unwrap();
}

/// Two cubes apart are two shells (two solids); a cube inside another (a
/// hollow part) is refused.
#[test]
fn shells_apart_convert_and_nested_shells_are_refused() {
    let mut two = cuboid([0.0; 3], [10.0; 3]);
    let other = cuboid([20.0, 0.0, 0.0], [30.0, 10.0, 10.0]);
    let offset = two.vertices.len() as u32;
    two.vertices.extend(other.vertices);
    two.triangles
        .extend(other.triangles.iter().map(|t| t.map(|v| v + offset)));
    let faceted = faceted_solid(&two, 0.01, DEFAULT_ANGLE).unwrap();
    assert_eq!(faceted.shells, 2);
    assert_eq!(faceted.boundary.shells.len(), 2);
    assert!((faceted.volume - 2000.0).abs() < 1e-9);

    let mut nested = cuboid([0.0; 3], [10.0; 3]);
    let inner = cuboid([2.0; 3], [8.0; 3]);
    let offset = nested.vertices.len() as u32;
    nested.vertices.extend(inner.vertices);
    nested
        .triangles
        .extend(inner.triangles.iter().map(|t| t.map(|v| v + offset)));
    let error = faceted_solid(&nested, 0.01, DEFAULT_ANGLE).unwrap_err();
    assert!(
        error.contains("1 of the mesh's 2 shells lie inside"),
        "{error}"
    );
}

/// A mesh over the triangle limit is refused with its count before
/// anything else is done.
#[test]
fn a_mesh_over_the_limit_is_refused_with_its_count() {
    let mesh = TriangleMesh {
        vertices: vec![[0.0; 3]; 3],
        triangles: vec![[0, 1, 2]; MAX_TRIANGLES + 1],
    };
    let error = faceted_solid(&mesh, 0.01, DEFAULT_ANGLE).unwrap_err();
    assert!(
        error.contains(&format!("{} triangles", MAX_TRIANGLES + 1)),
        "{error}"
    );
    assert!(
        error.contains(&format!("at most {MAX_TRIANGLES}")),
        "{error}"
    );
}

/// A Convert Mesh feature's definition as it is written, its values as
/// numbers (a parameter id is not written by itself).
fn written(doc: &Document<MockKernel>, uid: FeatureUid) -> serde_json::Value {
    let FeatureDef::MeshConvert(convert) = &doc.feature(uid).unwrap().def else {
        panic!("{uid} is no mesh_convert");
    };
    let values = convert.map_params(&mut |_, _| Ok::<f64, ()>(0.0)).unwrap();
    serde_json::to_value(&values).unwrap()
}

/// Block (F2.b0) and a mesh body (F3.b0 from the base feature "scan"; the
/// mock kernel's mesh of it is its box, 10 mm each way: 12 triangles).
fn block_and_mesh() -> (Document<MockKernel>, BodyUid) {
    let mut b = block();
    let imported = b
        .doc
        .add_base_feature(BaseInput {
            name: Some("scan".to_owned()),
            ..BaseInput::new(vec![ImportBody {
                name: Some("Scan".to_owned()),
                color: None,
                shape: MockShape::imported("mesh", 1),
            }])
        })
        .unwrap();
    let mesh = imported.bodies[0].uid;
    assert_eq!(mesh.to_string(), "F3.b0");
    (b.doc, mesh)
}

/// Faceted solid makes a new solid body of the mesh body's 12 triangles: 6
/// faces named after the feature; the mesh body stays, hidden in the same
/// undo step, and comes back shown with Undo.
#[test]
fn faceted_solid_adds_a_solid_body_and_hides_the_mesh() {
    let (mut doc, mesh) = block_and_mesh();
    let before = doc.body_shape(mesh).unwrap().clone();
    let uid = doc
        .add_feature(
            &def(
                json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.01,
                        "mode": "faceted", "angle": "0.5 deg"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert_eq!(uid.to_string(), "F4");
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert_eq!(doc.feature(uid).unwrap().name, "ConvertMesh1");
    let solid = BodyUid::new(uid, 0);
    let shape = doc.body_shape(solid).unwrap();
    let names: Vec<String> = shape.faces.iter().map(ToString::to_string).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(names.contains(&"F4:import(0)".to_owned()), "{names:?}");
    assert!(names.contains(&"F4:import(5)".to_owned()), "{names:?}");
    // The kernel's boundary body, adopted under the feature's face names.
    assert!(
        shape.history.starts_with("import(F4:0..6,boundary"),
        "{}",
        shape.history
    );
    assert!(doc.kernel().count("boundary_body") >= 1);
    // The mesh body is as it was, and hidden.
    assert_eq!(doc.body_shape(mesh), Some(&before));
    assert!(!doc.body_attributes(mesh).visible);
    assert!(doc.body_attributes(solid).visible);
    // The definition keeps its mode and angle.
    let saved = written(&doc, uid);
    assert_eq!(saved["mode"], "faceted", "{saved}");
    assert!(saved.get("angle").is_some(), "{saved}");

    // One undo step: the feature and the hiding go together.
    assert_eq!(doc.undo().as_deref(), Some("Add ConvertMesh1"));
    assert!(doc.feature(uid).is_none());
    assert!(doc.body_attributes(mesh).visible);
}

/// A mesh body shown by a condition (mitcad#809) keeps it through the
/// conversion (mitcad#1157): hidden while the conversion replaces it,
/// whatever the condition gives, with a warning saying why; deleting the
/// feature lets the body follow the condition again.
#[test]
fn faceted_solid_keeps_the_mesh_bodys_show_when_condition() {
    let (mut doc, mesh) = block_and_mesh();
    doc.add_parameter_expression("Variant", "'Full'", None, "")
        .unwrap();
    doc.bind_body_visible(mesh, "Variant == 'Full'").unwrap();
    let condition = doc.body_attributes(mesh).visible_when.clone();
    assert!(condition.is_some());
    assert!(doc.body_attributes(mesh).visible);

    let uid = doc
        .add_feature(
            &def(
                json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.01,
                        "mode": "faceted"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    // Hidden, the condition unchanged.
    assert!(!doc.body_attributes(mesh).visible);
    assert_eq!(doc.body_attributes(mesh).visible_when, condition);
    let warnings = doc.state().body_binding_warnings(mesh);
    assert_eq!(
        warnings,
        ["show when: not followed while ConvertMesh1 replaces this mesh body"],
    );
    // The condition does not show it while the conversion is there.
    doc.set_parameter_expression("Variant", "'Bare'", None)
        .unwrap();
    assert!(!doc.body_attributes(mesh).visible);
    doc.set_parameter_expression("Variant", "'Full'", None)
        .unwrap();
    assert!(!doc.body_attributes(mesh).visible);
    assert_eq!(doc.body_attributes(mesh).visible_when, condition);

    // Deleting the feature: the body follows its condition again.
    doc.delete_feature(uid, false).unwrap();
    assert_eq!(doc.body_attributes(mesh).visible_when, condition);
    assert!(doc.body_attributes(mesh).visible);
    assert!(doc.state().body_binding_warnings(mesh).is_empty());
    doc.set_parameter_expression("Variant", "'Bare'", None)
        .unwrap();
    assert!(!doc.body_attributes(mesh).visible);
    doc.set_parameter_expression("Variant", "'Full'", None)
        .unwrap();
    assert!(doc.body_attributes(mesh).visible);
    assert_eq!(doc.body_attributes(mesh).visible_when, condition);
}

/// Without an angle the default 0.5 degrees is used; without a mode the
/// conversion is the analytic one (definitions saved before mitcad#1082).
#[test]
fn faceted_solid_defaults_and_the_analytic_mode_stay() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = doc
        .add_feature(
            &def(
                json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.01,
                        "mode": "faceted"}),
            ),
            None,
        )
        .unwrap()
        .uid;
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    assert!(doc.body_shape(BodyUid::new(uid, 0)).is_some());
    doc.undo();
    let uid = doc
        .add_feature(
            &def(json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.05})),
            None,
        )
        .unwrap()
        .uid;
    assert!(doc.status(uid).unwrap().is_ok(), "{:?}", doc.status(uid));
    // In place, no new body, nothing hidden.
    assert!(doc.body_shape(BodyUid::new(uid, 0)).is_none());
    assert!(doc.body_attributes(mesh).visible);
    let saved = written(&doc, uid);
    assert!(saved.get("mode").is_none(), "{saved}");
}

/// A faceted conversion that fails is refused with the reason: nothing is
/// added and the body stays shown. Angles out of range and an angle
/// without the faceted mode are refused too.
#[test]
fn a_failing_faceted_solid_is_refused_and_changes_nothing() {
    let (mut doc, _) = block_and_mesh();
    let features = doc.features().count();
    let error = doc
        .add_feature(
            &def(
                json!({"type": "mesh_convert", "body": "F2.b0", "tolerance": 0.01,
                        "mode": "faceted"}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("ConvertMesh1: body F2.b0 (from Extrude1) is not a mesh body"),
        "{error}"
    );
    assert_eq!(doc.features().count(), features);
    assert!(doc.body_attributes(BodyUid::new(FeatureUid(2), 0)).visible);
    assert!(
        doc.features()
            .all(|f| !matches!(doc.status(f.uid), Some(FeatureStatus::Failed(_))))
    );
    for angle in ["0 deg", "31 deg"] {
        let error = doc
            .add_feature(
                &def(
                    json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.01,
                            "mode": "faceted", "angle": angle}),
                ),
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("angle"), "{error}");
    }
    let error = doc
        .add_feature(
            &def(
                json!({"type": "mesh_convert", "body": "F3.b0", "tolerance": 0.01,
                        "angle": "1 deg"}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Faceted solid"), "{error}");
}
