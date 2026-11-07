// SPDX-License-Identifier: MIT
//! Bodies from their boundaries and from triangles (the IFC import's breps
//! and face sets, mitcad#273): a [`BoundaryBody`] becomes the neutral B-rep
//! model of the `.f3d` reader (`mitcad_asm::brep`), which the B-rep builder
//! of the imports builds with OCCT (`geometry/src/brep_import.cpp`: edges
//! and faces made from their curves and surfaces, healed, shells sewn by
//! their shared edges and closed shells made solids); triangles become a
//! mesh body as an STL file's do.

use cxx::SharedPtr;
use mitcad_asm::brep::{
    BSplineCurve, BSplineSurface, Body, Coedge, Curve, Edge, Face, Lump, Shell, Surface, Vertex,
};
use mitcad_asm::convert::ConvertedBody;
use mitcad_model::KernelError;
use mitcad_model::boundary::{BoundaryBody, BoundaryCurve, BoundaryReport, BoundarySurface};

use super::Shape;
use crate::brep_import::{Source, to_ffi};

/// The tolerance of the vertices and edges given to the builder (mm).
const TOLERANCE: f64 = 1e-4;

fn curve(c: &BoundaryCurve) -> Curve {
    match c {
        BoundaryCurve::Line { origin, dir } => Curve::Line {
            origin: *origin,
            dir: *dir,
        },
        BoundaryCurve::Ellipse {
            center,
            normal,
            major_dir,
            major,
            minor,
        } => Curve::Ellipse {
            center: *center,
            normal: *normal,
            major_dir: *major_dir,
            major: *major,
            minor: *minor,
        },
        BoundaryCurve::BSpline {
            degree,
            knots,
            multiplicities,
            poles,
            weights,
        } => Curve::BSpline(BSplineCurve {
            degree: *degree as usize,
            knots: knots.clone(),
            mults: multiplicities.iter().map(|m| *m as usize).collect(),
            poles: poles.clone(),
            weights: (!weights.is_empty()).then(|| weights.clone()),
            periodic: false,
        }),
    }
}

fn surface(s: &BoundarySurface) -> Surface {
    match s {
        BoundarySurface::Plane {
            origin,
            normal,
            u_dir,
        } => Surface::Plane {
            origin: *origin,
            normal: *normal,
            u_dir: *u_dir,
        },
        BoundarySurface::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
        } => Surface::Cone {
            origin: *origin,
            axis: *axis,
            ref_dir: *ref_dir,
            radius: *radius,
            half_angle: *half_angle,
        },
        BoundarySurface::Sphere {
            center,
            axis,
            ref_dir,
            radius,
        } => Surface::Sphere {
            center: *center,
            axis: *axis,
            ref_dir: *ref_dir,
            radius: *radius,
        },
        BoundarySurface::Torus {
            center,
            axis,
            ref_dir,
            major,
            minor,
        } => Surface::Torus {
            center: *center,
            axis: *axis,
            ref_dir: *ref_dir,
            major: *major,
            minor: *minor,
        },
        BoundarySurface::BSpline {
            u_degree,
            v_degree,
            u_knots,
            u_multiplicities,
            v_knots,
            v_multiplicities,
            poles,
            weights,
        } => {
            let nu = poles.len();
            let nv = poles.first().map_or(0, Vec::len);
            Surface::BSpline(BSplineSurface {
                u_degree: *u_degree as usize,
                v_degree: *v_degree as usize,
                u_knots: u_knots.clone(),
                u_mults: u_multiplicities.iter().map(|m| *m as usize).collect(),
                v_knots: v_knots.clone(),
                v_mults: v_multiplicities.iter().map(|m| *m as usize).collect(),
                nu,
                nv,
                poles: poles.iter().flatten().copied().collect(),
                weights: (!weights.is_empty()).then(|| weights.iter().flatten().copied().collect()),
                u_periodic: false,
                v_periodic: false,
                u_closed: false,
                v_closed: false,
            })
        }
        BoundarySurface::Extrusion { curve: c, dir } => Surface::Extrusion {
            curve: curve(c),
            dir: *dir,
        },
        BoundarySurface::Revolution {
            curve: c,
            origin,
            axis,
        } => Surface::Revolution {
            curve: curve(c),
            origin: *origin,
            axis: *axis,
        },
    }
}

/// The neutral B-rep body of a boundary.
fn neutral(b: &BoundaryBody) -> Body {
    Body {
        vertices: b
            .vertices
            .iter()
            .map(|p| Vertex {
                point: *p,
                tolerance: TOLERANCE,
            })
            .collect(),
        curves: b.curves.iter().map(curve).collect(),
        surfaces: b.surfaces.iter().map(surface).collect(),
        edges: b
            .edges
            .iter()
            .map(|e| Edge {
                curve: e.curve,
                t: e.range,
                vertices: e.vertices,
                tolerance: TOLERANCE,
            })
            .collect(),
        faces: b
            .faces
            .iter()
            .map(|f| Face {
                surface: f.surface,
                reversed: f.reversed,
                double_sided: false,
                loops: f
                    .loops
                    .iter()
                    .map(|l| {
                        l.iter()
                            .map(|(edge, forward)| Coedge {
                                edge: *edge,
                                forward: *forward,
                            })
                            .collect()
                    })
                    .collect(),
                point_loops: Vec::new(),
            })
            .collect(),
        lumps: b
            .shells
            .iter()
            .map(|faces| Lump {
                shells: vec![Shell {
                    faces: faces.clone(),
                    wire_edges: Vec::new(),
                }],
            })
            .collect(),
        transform: None,
    }
}

pub(super) fn boundary_body(
    body: &BoundaryBody,
) -> Result<(SharedPtr<Shape>, BoundaryReport), KernelError> {
    body.check().map_err(KernelError::Failed)?;
    if body.faces.is_empty() {
        return Err(KernelError::failed("a boundary without faces"));
    }
    let converted = ConvertedBody {
        record: 0,
        top_level: true,
        body: neutral(body),
        issues: Vec::new(),
        skipped_faces: 0,
        unknown_edges: 0,
        degenerate_edges: 0,
        tidied: Default::default(),
        check: Default::default(),
        stored_blends: Vec::new(),
    };
    let source = Source {
        document: "",
        blob: "boundary",
        asm_version: "",
        history: false,
        history_step: 0,
    };
    let data = to_ffi(&source, &converted);
    let mut list = super::shape::ffi::new_shape_list();
    let built = crate::kernel::exchange::ffi::build_brep_body(&data, list.pin_mut());
    let shape = list.at(list.size() - 1);
    if !built.built || shape.is_null() {
        let error = built.error.to_string();
        if KernelError::is_out_of_memory(&error) {
            return Err(KernelError::Failed(error));
        }
        return Err(KernelError::Failed(format!(
            "the boundary does not build: {error}"
        )));
    }
    let report = BoundaryReport {
        solid: built.solid,
        valid: built.valid,
        faces: usize::try_from(built.faces).unwrap_or(0),
        left_out: built.issues as usize,
        messages: built.messages.iter().map(ToString::to_string).collect(),
    };
    Ok((shape, report))
}

pub(super) fn triangle_body(
    points: &[[f64; 3]],
    triangles: &[[u32; 3]],
) -> Result<SharedPtr<Shape>, KernelError> {
    crate::kernel::mesh::ffi::mesh_from_triangles(points.as_flattened(), triangles.as_flattened())
        .map_err(super::failed)
}
