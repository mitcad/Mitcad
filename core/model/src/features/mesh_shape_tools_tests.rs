// SPDX-License-Identifier: MIT
//! The mesh edits of mitcad#2515 on the mock kernel: Smooth, Combine,
//! Offset and Shell, Align and Plane Cut each make new mesh bodies, hide
//! the mesh bodies they start from in the same undo step, say what they
//! did and refuse what they cannot take, the places painted. The mock
//! kernel's mesh of a mesh body is its box, 10 mm each way (12
//! triangles); the algorithms on synthetic meshes are tested in
//! `mesh_tools/shape_tests.rs`.

use serde_json::{Value, json};

use crate::exchange::{BaseInput, ImportBody, TriangleMesh};
use crate::ids::{BodyUid, FeatureUid};
use crate::notice::{FeatureError, NoticePlace};
use crate::test_support::{block, def};
use crate::testing::{MockKernel, MockShape};
use crate::{Document, FeatureStatus};

/// A mesh body from a base feature named `name`.
fn add_mesh(doc: &mut Document<MockKernel>, name: &str) -> BodyUid {
    let imported = doc
        .add_base_feature(BaseInput {
            name: Some(name.to_owned()),
            ..BaseInput::new(vec![ImportBody {
                name: Some(name.to_owned()),
                color: None,
                shape: MockShape::imported("mesh", 1),
                face_colors: Vec::new(),
            }])
        })
        .unwrap();
    imported.bodies[0].uid
}

/// Block (F2.b0) and a mesh body (F3.b0).
fn block_and_mesh() -> (Document<MockKernel>, BodyUid) {
    let mut b = block();
    let mesh = add_mesh(&mut b.doc, "scan");
    assert_eq!(mesh.to_string(), "F3.b0");
    (b.doc, mesh)
}

fn add(doc: &mut Document<MockKernel>, definition: Value) -> FeatureUid {
    let uid = doc.add_feature(&def(definition), None).unwrap().uid;
    assert!(
        matches!(doc.status(uid), Some(FeatureStatus::Ok)),
        "{:?}",
        doc.status(uid)
    );
    uid
}

fn refused(doc: &mut Document<MockKernel>, definition: Value) -> String {
    doc.add_feature(&def(definition), None)
        .unwrap_err()
        .to_string()
}

/// The feature's new mesh bodies and the mesh bodies hidden, one undo step
/// with the feature, which is named `name` (a base name and a number).
fn assert_new_mesh_bodies(
    doc: &mut Document<MockKernel>,
    hidden: &[BodyUid],
    uid: FeatureUid,
    name: &str,
    bodies: u32,
) {
    let named = doc.feature(uid).unwrap().name.clone();
    assert!(
        named.starts_with(name.trim_end_matches(char::is_numeric)),
        "{named}"
    );
    let name = named.as_str();
    for n in 0..bodies {
        let made = BodyUid::new(uid, n);
        let shape = doc.body_shape(made).unwrap();
        assert!(shape.history.contains(",mesh"), "{}", shape.history);
        assert!(doc.body_attributes(made).visible);
    }
    for mesh in hidden {
        assert!(doc.body_shape(*mesh).is_some());
        assert!(!doc.body_attributes(*mesh).visible);
    }
    assert_eq!(doc.undo().as_deref(), Some(format!("Add {name}").as_str()));
    assert!(doc.feature(uid).is_none());
    for mesh in hidden {
        assert!(doc.body_attributes(*mesh).visible);
    }
}

#[test]
fn smooth_makes_a_new_mesh_body_and_keeps_the_volume() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 0.5, "passes": 2}),
    );
    let info = &doc.infos(uid)[0];
    assert!(
        info.starts_with("Smoothed 8 vertices in 2 passes"),
        "{info}"
    );
    assert_eq!(
        doc.body_shape(BodyUid::new(uid, 0)).unwrap().faces[0].to_string(),
        format!("{uid}:import(0)")
    );
    assert_new_mesh_bodies(&mut doc, &[mesh], uid, "Smooth1", 1);
}

#[test]
fn smooth_keeps_a_cube_whose_edges_are_all_sharp_and_checks_its_values() {
    let (mut doc, mesh) = block_and_mesh();
    // Each corner of the cube is a corner of sharp edges: nothing moves.
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 0.5, "passes": 2,
               "sharp_angle": "30 deg"}),
    );
    assert!(error.contains("no vertex of the mesh is free"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 1.5, "passes": 2}),
    );
    assert!(error.contains("the amount must be more than 0"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 0.5, "passes": 2.5}),
    );
    assert!(error.contains("a whole number from 1"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 0.5, "passes": 2,
               "region": {"brush": [[0, 0, 0]]}}),
    );
    assert!(error.contains("a brush needs its radius"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F2.b0", "amount": 0.5, "passes": 2}),
    );
    assert!(error.contains("Smooth works on mesh bodies"), "{error}");
    assert!(doc.body_attributes(mesh).visible);
}

#[test]
fn smooth_of_a_region_by_its_triangles() {
    let (mut doc, mesh) = block_and_mesh();
    // The bottom's two triangles: their corners are on other triangles
    // too, so nothing is free to move.
    let error = refused(
        &mut doc,
        json!({"type": "mesh_smooth", "body": "F3.b0", "amount": 0.5, "passes": 2,
               "region": {"triangles": [0, 1]}}),
    );
    assert!(error.contains("no vertex of the mesh is free"), "{error}");
    assert!(doc.body_attributes(mesh).visible);
}

#[test]
fn combine_joins_and_merges_meshes_and_hides_them() {
    let (mut doc, scan) = block_and_mesh();
    let other = add_mesh(&mut doc, "other");
    assert_eq!(other.to_string(), "F4.b0");
    let uid = add(
        &mut doc,
        json!({"type": "mesh_combine", "target": "F3.b0", "tools": ["F4.b0"],
               "operation": "join"}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Joined 2 meshes: a closed mesh of 12 triangles enclosing 1000 mm³"]
    );
    assert_new_mesh_bodies(&mut doc, &[scan, other], uid, "MeshCombine1", 1);
    let uid = add(
        &mut doc,
        json!({"type": "mesh_combine", "target": "F3.b0", "tools": ["F4.b0"],
               "operation": "merge"}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Merged 2 meshes into one body of 24 triangles"]
    );
    assert_new_mesh_bodies(&mut doc, &[scan, other], uid, "MeshCombine1", 1);
}

#[test]
fn combine_refuses_what_it_cannot_combine() {
    let (mut doc, _) = block_and_mesh();
    add_mesh(&mut doc, "other");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_combine", "target": "F3.b0", "tools": [], "operation": "join"}),
    );
    assert!(error.contains("needs a tool mesh"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_combine", "target": "F3.b0", "tools": ["F3.b0"],
               "operation": "cut"}),
    );
    assert!(error.contains("one of the tools too"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_combine", "target": "F3.b0", "tools": ["F2.b0"],
               "operation": "cut"}),
    );
    assert!(error.contains("Combine works on mesh bodies"), "{error}");
}

#[test]
fn offset_measures_its_distance_and_shell_opens_its_openings() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_offset", "body": "F3.b0", "distance": 2}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Offset 2 mm outward; the surface lies 2 to 3.4641 mm from the mesh"]
    );
    // (The mock kernel gives every body it adopts the box 0 to 10: the
    // message tells where the triangles went.)
    assert_new_mesh_bodies(&mut doc, &[mesh], uid, "MeshOffset1", 1);
    // A shell 2 mm inward, open at the bottom (its two triangles).
    let uid = add(
        &mut doc,
        json!({"type": "mesh_offset", "body": "F3.b0", "mode": "shell", "side": "inward",
               "distance": 2, "openings": {"triangles": [0, 1]}}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Shelled 2 mm inward: a closed wall of 28 triangles, open at 1 rim"]
    );
    assert_new_mesh_bodies(&mut doc, &[mesh], uid, "MeshOffset1", 1);
    // Both ways: the wall from 1 mm inside to 1 mm outside.
    let uid = add(
        &mut doc,
        json!({"type": "mesh_offset", "body": "F3.b0", "side": "both", "distance": 1}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Offset 1 mm both ways: a closed wall 2 mm thick"]
    );
}

#[test]
fn offset_refuses_a_distance_that_turns_the_mesh_over() {
    let (mut doc, mesh) = block_and_mesh();
    let error = doc
        .add_feature(
            &def(
                json!({"type": "mesh_offset", "body": "F3.b0", "side": "inward",
                        "distance": 6}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    let said = FeatureError::from_text(&error);
    // The 10 mm box is closed: its offset is trimmed where it turns over
    // (mitcad#2564), but 6 mm inward from each side leaves nothing.
    assert!(
        said.message.contains(
            "Offset by 6 mm is more than the mesh can take: where its sides come closer than \
             that allows (12 places), the offset surface would split apart, close a hole or \
             vanish"
        ),
        "{error}"
    );
    // Painted where the sides come too close.
    assert!(!said.places.is_empty());
    assert!(
        said.places
            .iter()
            .all(|p| matches!(p, NoticePlace::Point { .. }))
    );
    let error = refused(
        &mut doc,
        json!({"type": "mesh_offset", "body": "F3.b0", "distance": 1,
               "openings": {"triangles": [0]}}),
    );
    assert!(error.contains("openings are for a shell"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_offset", "body": "F3.b0", "distance": 0}),
    );
    assert!(
        error.contains("the distance must be more than 0"),
        "{error}"
    );
    assert!(doc.body_attributes(mesh).visible);
}

#[test]
fn align_moves_a_copy_onto_its_target() {
    let (mut doc, scan) = block_and_mesh();
    let other = add_mesh(&mut doc, "other");
    let uid = add(
        &mut doc,
        json!({"type": "mesh_align", "body": "F4.b0", "target": "F3.b0"}),
    );
    assert_eq!(
        doc.infos(uid),
        [
            "Moved 0 mm and turned 0°; the mesh lies within 0 mm of the target, 0 mm on average \
          (root mean square)"
        ]
    );
    // The moved mesh is hidden, the target stays.
    assert!(doc.body_attributes(scan).visible);
    assert_new_mesh_bodies(&mut doc, &[other], uid, "MeshAlign1", 1);
    // By three pairs that each put a point 1 mm back along X, without a
    // best fit: the mesh moves by exactly that. Both meshes are the mock
    // kernel's same 10 mm box, so the move takes the mesh 1 mm off the
    // target, and the deviation stated measures it: the four corners at
    // x 9 lie on the target's faces, the four at x -1 lie 1 mm from it
    // (largest 1 mm, root mean square sqrt(4 / 8) = 0.7071 mm).
    let uid = add(
        &mut doc,
        json!({"type": "mesh_align", "body": "F4.b0", "target": "F3.b0", "best_fit": false,
               "pairs": [{"from": [1, 0, 0], "to": [0, 0, 0]},
                         {"from": [11, 0, 0], "to": [10, 0, 0]},
                         {"from": [1, 10, 0], "to": [0, 10, 0]}]}),
    );
    assert_eq!(
        doc.infos(uid),
        [
            "Moved 1 mm and turned 0°; the mesh lies within 1 mm of the target, 0.7071 mm on \
          average (root mean square)"
        ]
    );
}

#[test]
fn align_refuses_too_few_pairs_and_itself() {
    let (mut doc, _) = block_and_mesh();
    add_mesh(&mut doc, "other");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_align", "body": "F4.b0", "target": "F3.b0",
               "pairs": [{"from": [0, 0, 0], "to": [1, 0, 0]}]}),
    );
    assert!(
        error.contains("three point pairs or more, got 1"),
        "{error}"
    );
    let error = refused(
        &mut doc,
        json!({"type": "mesh_align", "body": "F4.b0", "target": "F4.b0"}),
    );
    assert!(error.contains("the target is the mesh itself"), "{error}");
    let error = refused(
        &mut doc,
        json!({"type": "mesh_align", "body": "F4.b0", "target": "F3.b0", "best_fit": false}),
    );
    assert!(error.contains("point pairs, best fit or both"), "{error}");
}

#[test]
fn plane_cut_makes_two_capped_pieces() {
    let (mut doc, mesh) = block_and_mesh();
    add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 5}}),
    );
    let uid = add(
        &mut doc,
        json!({"type": "mesh_plane_cut", "body": "F3.b0", "plane": "F4"}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Cut the mesh, the cut capped: kept the pieces 500 mm³ above and 500 mm³ below"]
    );
    // Above first, below second, each named after its piece.
    for (n, face) in [(0, "import(0)"), (1, "import(1)")] {
        assert_eq!(
            doc.body_shape(BodyUid::new(uid, n)).unwrap().faces[0].to_string(),
            format!("{uid}:{face}")
        );
    }
    assert_new_mesh_bodies(&mut doc, &[mesh], uid, "PlaneCut1", 2);
    let uid = add(
        &mut doc,
        json!({"type": "mesh_plane_cut", "body": "F3.b0", "plane": "F4", "keep": "above",
               "cap": false}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Cut the mesh, the cut left open: kept the piece above"]
    );
    assert!(doc.body_shape(BodyUid::new(uid, 1)).is_none());
    // XY lies under the whole mesh.
    let error = refused(
        &mut doc,
        json!({"type": "mesh_plane_cut", "body": "F3.b0", "plane": "xy"}),
    );
    assert!(error.contains("the whole mesh lies above it"), "{error}");
}

/// A cube of 8 corners from 0 to `size`, its 12 triangles facing out.
fn cube(size: f64, shift: f64) -> TriangleMesh {
    let corner = |i: usize| -> [f64; 3] {
        std::array::from_fn(|axis| shift + if (i >> axis) & 1 == 1 { size } else { 0.0 })
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

// (The mock kernel's mesh of any mesh body is its closed box: the
// refusals of open and crossing meshes are tested on the meshes
// themselves, and through the CLI on the OCCT kernel.)

#[test]
fn an_open_mesh_is_refused_with_its_hole_painted() {
    let mut open = cube(10.0, 0.0);
    open.triangles.drain(0..2);
    let error = super::require_closed(&open, "Combine", "the target mesh").unwrap_err();
    let said = FeatureError::from_text(&error);
    assert_eq!(
        said.message,
        "Combine needs closed meshes, and the target mesh is open: a hole. Repair Mesh fills \
         holes and makes a mesh manifold"
    );
    assert_eq!(
        said.places,
        [NoticePlace::Box {
            min: [0.0, 0.0, 0.0],
            max: [10.0, 10.0, 0.0]
        }]
    );
    assert!(super::require_closed(&cube(10.0, 0.0), "Combine", "the target mesh").is_ok());
}

#[test]
fn a_mesh_that_crosses_itself_is_refused_with_the_crossings_painted() {
    let crossed = crate::mesh_tools::merged(&[cube(10.0, 0.0), cube(10.0, 5.0)]);
    let error = super::require_uncrossed(&crossed, "Combine", "the tool mesh").unwrap_err();
    let said = FeatureError::from_text(&error);
    assert!(
        said.message.starts_with(
            "Combine needs meshes that do not cross themselves, and the tool mesh does: "
        ),
        "{}",
        said.message
    );
    assert!(!said.places.is_empty());
    for place in &said.places {
        let NoticePlace::Point { at } = place else {
            panic!("{place:?}");
        };
        assert!(
            at.iter().all(|x| (5.0 - 1e-9..=10.0 + 1e-9).contains(x)),
            "{at:?}"
        );
    }
    assert!(super::require_uncrossed(&cube(10.0, 0.0), "Combine", "the tool mesh").is_ok());
}
