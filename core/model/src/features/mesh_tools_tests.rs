// SPDX-License-Identifier: MIT
//! The mesh edit features (mitcad#1871) on the mock kernel: each makes new
//! bodies of a mesh body, hides it in the same undo step, says what it did
//! and refuses what it cannot take. The mock kernel's mesh of a mesh body is
//! its box, 10 mm each way (12 triangles); the algorithms on synthetic
//! scanned meshes are tested in `mesh_tools/tests.rs`.

use serde_json::{Value, json};

use crate::exchange::{BaseInput, ImportBody};
use crate::features::FeatureDef;
use crate::ids::{BodyUid, FeatureUid};
use crate::test_support::{block, def};
use crate::testing::{MockKernel, MockShape};
use crate::{Document, FeatureStatus};

/// Block (F2.b0) and a mesh body (F3.b0 from the base feature "scan").
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
                face_colors: Vec::new(),
            }])
        })
        .unwrap();
    let mesh = imported.bodies[0].uid;
    assert_eq!(mesh.to_string(), "F3.b0");
    (b.doc, mesh)
}

/// Adds a feature; it evaluates.
fn add(doc: &mut Document<MockKernel>, definition: Value) -> FeatureUid {
    let uid = doc.add_feature(&def(definition), None).unwrap().uid;
    assert!(
        matches!(doc.status(uid), Some(FeatureStatus::Ok)),
        "{:?}",
        doc.status(uid)
    );
    uid
}

/// A new mesh body (its one face named after the feature) and the mesh
/// body hidden, one undo step with the feature.
fn assert_new_mesh_body(
    doc: &mut Document<MockKernel>,
    mesh: BodyUid,
    uid: FeatureUid,
    name: &str,
) {
    assert_eq!(doc.feature(uid).unwrap().name, name);
    let made = BodyUid::new(uid, 0);
    let shape = doc.body_shape(made).unwrap();
    assert!(shape.history.contains(",mesh"), "{}", shape.history);
    assert_eq!(shape.faces[0].to_string(), format!("{uid}:import(0)"));
    assert!(doc.body_shape(mesh).is_some());
    assert!(!doc.body_attributes(mesh).visible);
    assert!(doc.body_attributes(made).visible);
    assert_eq!(doc.undo().as_deref(), Some(format!("Add {name}").as_str()));
    assert!(doc.feature(uid).is_none());
    assert!(doc.body_attributes(mesh).visible);
}

#[test]
fn decimate_makes_a_new_mesh_body_and_hides_the_scan() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_decimate", "body": "F3.b0", "triangles": 8}),
    );
    // A cube's corners are all sharp: nothing to take away.
    assert_eq!(
        doc.infos(uid),
        ["Decimated from 12 triangles to 12 triangles; the surface moved up to 0 mm"]
    );
    assert_new_mesh_body(&mut doc, mesh, uid, "Decimate1");
}

#[test]
fn decimate_needs_a_count_or_a_tolerance_and_takes_mesh_bodies_only() {
    let (mut doc, mesh) = block_and_mesh();
    let error = doc
        .add_feature(
            &def(json!({"type": "mesh_decimate", "body": "F3.b0"})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("a triangle count, a tolerance or both"),
        "{error}"
    );
    let error = doc
        .add_feature(
            &def(json!({"type": "mesh_decimate", "body": "F3.b0", "triangles": 2.5})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("a whole number of at least 4"), "{error}");
    // The block is a solid: refused in words, the body named only for agents.
    let error = doc
        .add_feature(
            &def(json!({"type": "mesh_decimate", "body": "F2.b0", "tolerance": 0.1})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("Decimate works on mesh bodies"), "{error}");
    assert!(doc.body_attributes(mesh).visible);
}

#[test]
fn repair_says_what_it_fixed() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_repair", "body": "F3.b0", "tolerance": 0.01, "max_hole": 5}),
    );
    assert_eq!(doc.infos(uid), ["the mesh needed no repair"]);
    assert!(doc.warnings(uid).is_empty());
    assert_new_mesh_body(&mut doc, mesh, uid, "Repair1");
}

#[test]
fn remove_region_by_triangles_brush_and_lasso() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_remove", "body": "F3.b0", "triangles": [0, 1], "fill": true}),
    );
    assert_eq!(doc.infos(uid), ["Removed 2 triangles; filled 1 hole"]);
    assert_new_mesh_body(&mut doc, mesh, uid, "RemoveRegion1");
    // A lasso round the top seen from above, and a brush at a corner.
    let uid = add(
        &mut doc,
        json!({"type": "mesh_remove", "body": "F3.b0",
               "lassos": [{"normal": [0, 0, 1],
                           "points": [[-1, -1, 20], [11, -1, 20], [11, 11, 20], [-1, 11, 20]]}]}),
    );
    assert_eq!(doc.infos(uid), ["Removed 2 triangles"]);
    doc.undo();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_remove", "body": "F3.b0", "brush": [[0, 0, 0]], "radius": "8 mm"}),
    );
    assert!(
        doc.infos(uid)[0].starts_with("Removed "),
        "{:?}",
        doc.infos(uid)
    );
    doc.undo();
    // Nothing picked, and a brush without a radius: refused.
    let error = doc
        .add_feature(&def(json!({"type": "mesh_remove", "body": "F3.b0"})), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("needs a lasso, a brush stroke or triangles"),
        "{error}"
    );
    let error = doc
        .add_feature(
            &def(json!({"type": "mesh_remove", "body": "F3.b0", "brush": [[0, 0, 0]]})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("a brush needs its radius"), "{error}");
    let error = doc
        .add_feature(
            &def(json!({"type": "mesh_remove", "body": "F3.b0", "triangles": [500]})),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("picks no triangle"), "{error}");
}

#[test]
fn segment_makes_a_body_per_region() {
    let (mut doc, mesh) = block_and_mesh();
    let uid = add(
        &mut doc,
        json!({"type": "mesh_segment", "body": "F3.b0", "angle": "20 deg", "tolerance": 0.05}),
    );
    assert_eq!(
        doc.infos(uid),
        ["Found 6 regions: 6 planes, 0 cylinders and 0 free-form regions"]
    );
    for k in 0..6 {
        let shape = doc.body_shape(BodyUid::new(uid, k)).unwrap();
        assert_eq!(shape.faces[0].to_string(), format!("{uid}:import({k})"));
    }
    assert!(doc.body_shape(BodyUid::new(uid, 6)).is_none());
    assert!(!doc.body_attributes(mesh).visible);
    let error = doc
        .add_feature(
            &def(
                json!({"type": "mesh_segment", "body": "F3.b0", "angle": "120 deg",
                        "tolerance": 0.05}),
            ),
            None,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("at most 90 degrees"), "{error}");
}

#[test]
fn fit_surface_makes_a_surface_body() {
    let (mut doc, mesh) = block_and_mesh();
    // A cube is no plane: the chosen plane is built with a warning.
    let uid = add(
        &mut doc,
        json!({"type": "mesh_fit_surface", "body": "F3.b0", "surface": "plane", "tolerance": 0.05}),
    );
    let made = BodyUid::new(uid, 0);
    let shape = doc.body_shape(made).unwrap();
    assert!(shape.history.contains("boundary"), "{}", shape.history);
    assert!(doc.kernel().count("boundary_body") >= 1);
    assert!(
        doc.infos(uid)[0].starts_with("Fitted a plane; the mesh lies within 5 mm"),
        "{:?}",
        doc.infos(uid)
    );
    assert_eq!(
        doc.warnings(uid),
        ["the fitted plane lies up to 5 mm from the mesh, more than the tolerance 0.05 mm"]
    );
    assert!(!doc.body_attributes(mesh).visible);
    doc.undo();
    // Auto: a cube's eight corners lie on a cylinder (tried before a
    // sphere) about an axis through two faces' middles.
    let uid = add(
        &mut doc,
        json!({"type": "mesh_fit_surface", "body": "F3.b0", "tolerance": 0.05}),
    );
    assert!(
        doc.infos(uid)[0].starts_with("Fitted a cylinder of radius 7.0711 mm"),
        "{:?}",
        doc.infos(uid)
    );
    assert!(doc.warnings(uid).is_empty());
}

/// Sections of the mesh into sketches on a plane 5 above XY and two more
/// 2 apart: a square of four lines each, one undo step; a solid body is
/// refused and leaves nothing behind.
#[test]
fn mesh_sections_into_a_series_of_sketches() {
    use crate::features::SketchPlane;
    use crate::{MeshSectionRequest, SectionTarget};
    let (mut doc, mesh) = block_and_mesh();
    let plane = add(
        &mut doc,
        json!({"type": "construction_plane",
               "definition": {"type": "offset", "plane": "xy", "distance": 5}}),
    );
    let depth = doc.undo_depth();
    let made = doc
        .mesh_sections(&MeshSectionRequest {
            target: SectionTarget::Plane(SketchPlane::Construction(plane)),
            body: mesh,
            tolerance: 0.05,
            count: 3,
            spacing: 2.0,
        })
        .unwrap();
    assert_eq!(made.sketches.len(), 3);
    assert_eq!(
        (made.lines, made.arcs, made.circles, made.missed),
        (12, 0, 0, 0)
    );
    for sketch in &made.sketches {
        let FeatureDef::Sketch(def) = &doc.feature(*sketch).unwrap().def else {
            panic!("{sketch} is no sketch");
        };
        let lines = def
            .entities
            .iter()
            .filter(|e| matches!(e.kind, crate::sketch::EntityKind::Line { .. }))
            .count();
        assert_eq!(lines, 4, "{sketch}");
    }
    assert_eq!(doc.undo_depth(), depth + 1);
    assert_eq!(doc.undo().as_deref(), Some("Mesh Sections"));
    assert!(made.sketches.iter().all(|s| doc.feature(*s).is_none()));

    // A plane beyond the mesh in the series: no sketch for it.
    let made = doc
        .mesh_sections(&MeshSectionRequest {
            target: SectionTarget::Plane(SketchPlane::Construction(plane)),
            body: mesh,
            tolerance: 0.05,
            count: 2,
            spacing: 20.0,
        })
        .unwrap();
    assert_eq!((made.sketches.len(), made.missed), (1, 1));
    doc.undo();

    let features = doc.features().count();
    let error = doc
        .mesh_sections(&MeshSectionRequest {
            target: SectionTarget::Plane(SketchPlane::Construction(plane)),
            body: BodyUid::new(FeatureUid(2), 0),
            tolerance: 0.05,
            count: 1,
            spacing: 0.0,
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("Mesh Section cuts mesh bodies"), "{error}");
    assert_eq!(doc.features().count(), features);
    assert_eq!(doc.undo_depth(), depth);
}
