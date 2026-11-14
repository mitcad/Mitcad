// SPDX-License-Identifier: MIT
//! The router spindle's design read into the CAM spindle model
//! (mitcad#1013): its components become the parts by their names, the tool
//! is left out, and the meshes are placed by the occurrences. The mock
//! kernel meshes a body as its box, so the parts here are boxes.

use mitcad_cam::spindle::PartShape;
use mitcad_cam::{PartKind, Tool};
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;
use mitcad_model_api::cam_spindle::{
    ROUTER_SPINDLE, SPINDLE_MESH_DEVIATION, router_spindle, spindle_model,
};

fn z_range(shape: &PartShape) -> (f64, f64) {
    let PartShape::Mesh { triangles } = shape else {
        panic!("the mock's boxes are not round: {shape:?}");
    };
    triangles
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[2]), hi.max(p[2]))
        })
}

#[test]
fn the_router_spindle_design_builds_into_its_parts() {
    let mut doc = Document::from_json(ROUTER_SPINDLE, MockKernel::default()).unwrap();
    let stats = doc.recompute();
    assert!(stats.error.is_none(), "{:?}", stats.error);
    let model = spindle_model(&doc, SPINDLE_MESH_DEVIATION).unwrap();
    let kinds: Vec<PartKind> = model.parts.iter().map(|p| p.kind).collect();
    assert_eq!(
        kinds,
        vec![PartKind::Nut, PartKind::Spindle, PartKind::Mount]
    );
    let names: Vec<&str> = model.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Nut", "Spindle", "Mount"]);
    assert!(model.parts.iter().all(|p| !p.cutting));
    assert!(
        model
            .parts
            .iter()
            .all(|p| p.margin == SPINDLE_MESH_DEVIATION)
    );
    // The nut up from the gauge line, the mount 90 mm up the body.
    let (lo, hi) = z_range(&model.parts[0].shape);
    assert!(lo.abs() < 1e-9 && (hi - 20.0).abs() < 1e-9, "{lo} {hi}");
    let (lo, hi) = z_range(&model.parts[2].shape);
    assert!(
        (lo - 122.0).abs() < 1e-9 && (hi - 182.0).abs() < 1e-9,
        "{lo} {hi}"
    );

    // With a tool in it, the tool's parts come first and the spindle's are
    // raised by its stick-out.
    let tool = Tool::flat_end_mill(1, 6.0);
    let assembly = model.assembly(&tool);
    assert_eq!(assembly.parts[0].kind, PartKind::Cutter);
    assert_eq!(assembly.parts[1].kind, PartKind::Shank);
    let (lo, _) = z_range(&assembly.parts[2].shape);
    assert!((lo - tool.length).abs() < 1e-9, "{lo}");
}

#[test]
fn the_shipped_spindle_reads_with_a_kernel() {
    let model = router_spindle(MockKernel::default()).unwrap();
    assert_eq!(model.parts.len(), 3);
}
