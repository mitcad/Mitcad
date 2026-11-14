// SPDX-License-Identifier: MIT
//! The spindle of a CNC machine as a MitCad design (mitcad#1013,
//! docs/cam.md "Spindle assembly"): the reader that turns the design's
//! bodies into the spindle model of the CAM collision check
//! ([`mitcad_cam::SpindleModel`]) through the kernel's body meshes, and the
//! design of a typical router spindle that ships with MitCad
//! (`resources/cam/router-spindle.json`).
//!
//! A spindle design is drawn in the frame of the spindle's gauge line: the
//! axis along +Z, `z = 0` at the face a tool's stick-out is measured from
//! (the collet nut's or the holder's), the spindle above it. Its components
//! say what each part is by their names ("Nut", "Collet", "Holder",
//! "Spindle", "Mount", "Changer"; [`mitcad_cam::PartKind::from_component_name`]);
//! a "Tool" component is only for the picture: each operation's tool comes
//! from its tool definition.
//!
//! The CAM setups (`cam` of the api) call [`spindle_model`] on the design a
//! machine names, or [`router_spindle`] when it names none and has no nose
//! of its own.

use mitcad_cam::SpindleModel;
use mitcad_cam::spindle::{DesignBody, group_by_component};
use mitcad_model::Document;
use mitcad_model::exchange::MeshTolerance;
use mitcad_model::kernel::Kernel;

/// The design of a router spindle: an 80 mm body on a 40 mm nose, an
/// ER20 collet nut of 35 mm, a clamp mount, and a 6 mm end mill 20 mm out
/// for the picture.
pub const ROUTER_SPINDLE: &str = include_str!("../../../resources/cam/router-spindle.json");

/// How far the meshes the spindle's parts are read from may be from the
/// parts' surfaces (mm); the check counts it against the parts.
pub const SPINDLE_MESH_DEVIATION: f64 = 0.01;

/// The spindle model of a design's bodies at the timeline marker, each
/// body meshed within `deviation` (mm) and placed by its occurrence; the
/// bodies of a component make one part. A root body counts by its own name.
pub fn spindle_model<K: Kernel>(doc: &Document<K>, deviation: f64) -> Result<SpindleModel, String> {
    let tolerance = MeshTolerance {
        deviation,
        angle: 15.0_f64.to_radians(),
        face_triangles: 0,
        max_edge: 0.0,
    };
    let assembly = doc.assembly();
    let mut bodies = Vec::new();
    for instance in doc.instances() {
        let component = if instance.component.is_root() {
            instance.name.clone()
        } else {
            match assembly.component(instance.component) {
                Some(c) => c.name.clone(),
                None => continue,
            }
        };
        let Some(shape) = doc.body_shape(instance.body) else {
            continue;
        };
        let mesh = doc
            .kernel()
            .triangle_mesh(shape, &tolerance)
            .map_err(|e| format!("cannot mesh a body of the spindle design: {e}"))?;
        bodies.push(DesignBody {
            component,
            vertices: mesh
                .vertices
                .iter()
                .map(|p| instance.transform.apply_point(*p))
                .collect(),
            triangles: mesh.triangles,
        });
    }
    SpindleModel::from_bodies(&group_by_component(bodies), deviation).map_err(|e| e.message)
}

/// The spindle model of the router spindle that ships with MitCad
/// ([`ROUTER_SPINDLE`]), built with `kernel`.
pub fn router_spindle<K: Kernel>(kernel: K) -> Result<SpindleModel, String> {
    let mut doc = Document::from_json(ROUTER_SPINDLE, kernel)
        .map_err(|e| format!("cannot read the router spindle's design: {e}"))?;
    doc.recompute();
    spindle_model(&doc, SPINDLE_MESH_DEVIATION)
}
