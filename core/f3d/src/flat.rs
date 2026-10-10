// SPDX-License-Identifier: MIT
//! Flat encoding of a neutral body for the C++ builder (plain arrays that
//! cross the CXX bridge). The layout is mirrored by
//! `geometry/include/mitcad/geometry/brep_import.hpp`.
//!
//! Each curve and surface is a kind plus a slice of `ints` and of `reals`:
//!
//! | kind | ints | reals |
//! |---|---|---|
//! | curve 0 line | - | origin(3) dir(3) |
//! | curve 1 ellipse | - | center(3) normal(3) major_dir(3) major minor |
//! | curve 2 bspline | degree n_knots n_poles rational periodic mults... | knots... poles(3 each)... weights... |
//! | curve 3 interpolated | n_points | params... points(3 each)... |
//! | surface 0 plane | - | origin(3) normal(3) u_dir(3) |
//! | surface 1 cone | - | origin(3) axis(3) ref_dir(3) radius half_angle |
//! | surface 2 sphere | - | center(3) axis(3) ref_dir(3) radius |
//! | surface 3 torus | - | center(3) axis(3) ref_dir(3) major minor |
//! | surface 4 bspline | u_deg v_deg n_uk n_vk nu nv rational u_periodic v_periodic u_mults... v_mults... | u_knots... v_knots... poles(3 each, u-major)... weights... |
//! | surface 5 extrusion | curve_kind curve_ints... | dir(3) curve_reals... |
//! | surface 6 revolution | curve_kind curve_ints... | origin(3) axis(3) curve_reals... |
//! | surface 7 ruled | from_kind from_ints... to_kind to_ints... | from_reals... to_reals... |
//! | surface 8 arc sweep | n (kind ints...) n times | weights(n) reals... n times |
//!
//! A face's point loops (`Face::point_loops`) are the range
//! `first_point_loop .. first_point_loop + point_loop_count` of
//! `point_loops`, which holds vertex indices.

use crate::brep::{Body, Curve, Surface};

pub const CURVE_LINE: i32 = 0;
pub const CURVE_ELLIPSE: i32 = 1;
pub const CURVE_BSPLINE: i32 = 2;
pub const CURVE_INTERPOLATED: i32 = 3;

pub const SURFACE_PLANE: i32 = 0;
pub const SURFACE_CONE: i32 = 1;
pub const SURFACE_SPHERE: i32 = 2;
pub const SURFACE_TORUS: i32 = 3;
pub const SURFACE_BSPLINE: i32 = 4;
pub const SURFACE_EXTRUSION: i32 = 5;
pub const SURFACE_REVOLUTION: i32 = 6;
pub const SURFACE_RULED: i32 = 7;
pub const SURFACE_ARC_SWEEP: i32 = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatGeometry {
    pub kind: i32,
    pub int_offset: u32,
    pub int_count: u32,
    pub real_offset: u32,
    pub real_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlatEdge {
    pub curve: u32,
    pub v0: u32,
    pub v1: u32,
    pub t0: f64,
    pub t1: f64,
    pub tolerance: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatFace {
    pub surface: u32,
    pub reversed: bool,
    pub double_sided: bool,
    pub first_loop: u32,
    pub loop_count: u32,
    /// Range in `FlatBody::point_loops`.
    pub first_point_loop: u32,
    pub point_loop_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatLoop {
    pub first_coedge: u32,
    pub coedge_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatCoedge {
    pub edge: u32,
    pub forward: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlatShell {
    pub lump: u32,
    /// Range in `shell_faces`.
    pub first_face: u32,
    pub face_count: u32,
    pub closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FlatBody {
    pub ints: Vec<i32>,
    pub reals: Vec<f64>,
    pub curves: Vec<FlatGeometry>,
    pub surfaces: Vec<FlatGeometry>,
    /// x, y, z, tolerance per vertex.
    pub vertices: Vec<f64>,
    pub edges: Vec<FlatEdge>,
    pub faces: Vec<FlatFace>,
    pub loops: Vec<FlatLoop>,
    pub coedges: Vec<FlatCoedge>,
    pub shells: Vec<FlatShell>,
    pub shell_faces: Vec<u32>,
    /// Vertices of the faces' point loops.
    pub point_loops: Vec<u32>,
    pub lump_count: u32,
    /// Empty, or 12 values: rows of the 3x3 matrix, then the translation.
    pub transform: Vec<f64>,
}

fn n(v: usize) -> u32 {
    u32::try_from(v).expect("model too large for the flat encoding")
}

fn ni(v: usize) -> i32 {
    i32::try_from(v).expect("model too large for the flat encoding")
}

/// Appends the curve's data; returns its kind.
fn encode_curve(c: &Curve, ints: &mut Vec<i32>, reals: &mut Vec<f64>) -> i32 {
    match c {
        Curve::Line { origin, dir } => {
            reals.extend_from_slice(origin);
            reals.extend_from_slice(dir);
            CURVE_LINE
        }
        Curve::Ellipse {
            center,
            normal,
            major_dir,
            major,
            minor,
        } => {
            reals.extend_from_slice(center);
            reals.extend_from_slice(normal);
            reals.extend_from_slice(major_dir);
            reals.push(*major);
            reals.push(*minor);
            CURVE_ELLIPSE
        }
        Curve::BSpline(b) => {
            ints.extend([
                ni(b.degree),
                ni(b.knots.len()),
                ni(b.poles.len()),
                i32::from(b.weights.is_some()),
                i32::from(b.periodic),
            ]);
            ints.extend(b.mults.iter().map(|&m| ni(m)));
            reals.extend_from_slice(&b.knots);
            for p in &b.poles {
                reals.extend_from_slice(p);
            }
            if let Some(w) = &b.weights {
                reals.extend_from_slice(w);
            }
            CURVE_BSPLINE
        }
        Curve::Interpolated { params, points } => {
            ints.push(ni(points.len()));
            reals.extend_from_slice(params);
            for p in points {
                reals.extend_from_slice(p);
            }
            CURVE_INTERPOLATED
        }
    }
}

fn encode_surface(s: &Surface, ints: &mut Vec<i32>, reals: &mut Vec<f64>) -> i32 {
    match s {
        Surface::Plane {
            origin,
            normal,
            u_dir,
        } => {
            reals.extend_from_slice(origin);
            reals.extend_from_slice(normal);
            reals.extend_from_slice(u_dir);
            SURFACE_PLANE
        }
        Surface::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
        } => {
            reals.extend_from_slice(origin);
            reals.extend_from_slice(axis);
            reals.extend_from_slice(ref_dir);
            reals.push(*radius);
            reals.push(*half_angle);
            SURFACE_CONE
        }
        Surface::Sphere {
            center,
            axis,
            ref_dir,
            radius,
        } => {
            reals.extend_from_slice(center);
            reals.extend_from_slice(axis);
            reals.extend_from_slice(ref_dir);
            reals.push(*radius);
            SURFACE_SPHERE
        }
        Surface::Torus {
            center,
            axis,
            ref_dir,
            major,
            minor,
        } => {
            reals.extend_from_slice(center);
            reals.extend_from_slice(axis);
            reals.extend_from_slice(ref_dir);
            reals.push(*major);
            reals.push(*minor);
            SURFACE_TORUS
        }
        Surface::BSpline(b) => {
            ints.extend([
                ni(b.u_degree),
                ni(b.v_degree),
                ni(b.u_knots.len()),
                ni(b.v_knots.len()),
                ni(b.nu),
                ni(b.nv),
                i32::from(b.weights.is_some()),
                i32::from(b.u_periodic),
                i32::from(b.v_periodic),
            ]);
            ints.extend(b.u_mults.iter().map(|&m| ni(m)));
            ints.extend(b.v_mults.iter().map(|&m| ni(m)));
            reals.extend_from_slice(&b.u_knots);
            reals.extend_from_slice(&b.v_knots);
            for p in &b.poles {
                reals.extend_from_slice(p);
            }
            if let Some(w) = &b.weights {
                reals.extend_from_slice(w);
            }
            SURFACE_BSPLINE
        }
        Surface::Extrusion { curve, dir } => {
            reals.extend_from_slice(dir);
            let at = ints.len();
            ints.push(0);
            ints[at] = encode_curve(curve, ints, reals);
            SURFACE_EXTRUSION
        }
        Surface::Revolution {
            curve,
            origin,
            axis,
        } => {
            reals.extend_from_slice(origin);
            reals.extend_from_slice(axis);
            let at = ints.len();
            ints.push(0);
            ints[at] = encode_curve(curve, ints, reals);
            SURFACE_REVOLUTION
        }
        Surface::Ruled { from, to } => {
            for c in [from, to] {
                let at = ints.len();
                ints.push(0);
                ints[at] = encode_curve(c, ints, reals);
            }
            SURFACE_RULED
        }
        Surface::ArcSweep { sections, weights } => {
            ints.push(ni(sections.len()));
            reals.extend_from_slice(weights);
            for c in sections {
                let at = ints.len();
                ints.push(0);
                ints[at] = encode_curve(c, ints, reals);
            }
            SURFACE_ARC_SWEEP
        }
    }
}

impl FlatBody {
    pub fn from_body(body: &Body) -> FlatBody {
        let mut f = FlatBody::default();
        for c in &body.curves {
            let (io, ro) = (f.ints.len(), f.reals.len());
            let kind = encode_curve(c, &mut f.ints, &mut f.reals);
            f.curves.push(FlatGeometry {
                kind,
                int_offset: n(io),
                int_count: n(f.ints.len() - io),
                real_offset: n(ro),
                real_count: n(f.reals.len() - ro),
            });
        }
        for s in &body.surfaces {
            let (io, ro) = (f.ints.len(), f.reals.len());
            let kind = encode_surface(s, &mut f.ints, &mut f.reals);
            f.surfaces.push(FlatGeometry {
                kind,
                int_offset: n(io),
                int_count: n(f.ints.len() - io),
                real_offset: n(ro),
                real_count: n(f.reals.len() - ro),
            });
        }
        for v in &body.vertices {
            f.vertices.extend_from_slice(&v.point);
            f.vertices.push(v.tolerance);
        }
        for e in &body.edges {
            f.edges.push(FlatEdge {
                curve: n(e.curve),
                v0: n(e.vertices[0]),
                v1: n(e.vertices[1]),
                t0: e.t[0],
                t1: e.t[1],
                tolerance: e.tolerance,
            });
        }
        for face in &body.faces {
            f.faces.push(FlatFace {
                surface: n(face.surface),
                reversed: face.reversed,
                double_sided: face.double_sided,
                first_loop: n(f.loops.len()),
                loop_count: n(face.loops.len()),
                first_point_loop: n(f.point_loops.len()),
                point_loop_count: n(face.point_loops.len()),
            });
            f.point_loops.extend(face.point_loops.iter().map(|&v| n(v)));
            for lp in &face.loops {
                f.loops.push(FlatLoop {
                    first_coedge: n(f.coedges.len()),
                    coedge_count: n(lp.len()),
                });
                for c in lp {
                    f.coedges.push(FlatCoedge {
                        edge: n(c.edge),
                        forward: c.forward,
                    });
                }
            }
        }
        for (li, lump) in body.lumps.iter().enumerate() {
            for shell in &lump.shells {
                f.shells.push(FlatShell {
                    lump: n(li),
                    first_face: n(f.shell_faces.len()),
                    face_count: n(shell.faces.len()),
                    closed: body.shell_is_closed(shell),
                });
                f.shell_faces.extend(shell.faces.iter().map(|&i| n(i)));
            }
        }
        f.lump_count = n(body.lumps.len());
        if let Some(t) = &body.transform {
            for row in &t.m {
                f.transform.extend_from_slice(row);
            }
            f.transform.extend_from_slice(&t.t);
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brep::{BSplineCurve, Coedge, Edge, Face, Lump, Shell, Vertex};

    #[test]
    fn encodes_geometry_and_topology() {
        let body = Body {
            lumps: vec![Lump {
                shells: vec![Shell {
                    faces: vec![0],
                    wire_edges: vec![],
                }],
            }],
            faces: vec![Face {
                surface: 0,
                reversed: true,
                double_sided: false,
                loops: vec![vec![Coedge {
                    edge: 0,
                    forward: false,
                }]],
                point_loops: vec![0],
            }],
            edges: vec![Edge {
                curve: 1,
                t: [0.0, 1.0],
                vertices: [0, 0],
                tolerance: 0.0,
            }],
            vertices: vec![Vertex {
                point: [1.0, 2.0, 3.0],
                tolerance: 0.5,
            }],
            curves: vec![
                Curve::Line {
                    origin: [0.0; 3],
                    dir: [1.0, 0.0, 0.0],
                },
                Curve::BSpline(BSplineCurve {
                    degree: 1,
                    knots: vec![0.0, 1.0],
                    mults: vec![2, 2],
                    poles: vec![[0.0; 3], [1.0, 0.0, 0.0]],
                    weights: None,
                    periodic: false,
                }),
            ],
            surfaces: vec![Surface::Extrusion {
                curve: Curve::Line {
                    origin: [0.0; 3],
                    dir: [1.0, 0.0, 0.0],
                },
                dir: [0.0, 0.0, 1.0],
            }],
            transform: None,
        };
        let f = FlatBody::from_body(&body);
        assert_eq!(f.curves[1].kind, CURVE_BSPLINE);
        assert_eq!(f.curves[1].int_count, 7);
        assert_eq!(&f.ints[..7], &[1, 2, 2, 0, 0, 2, 2]);
        assert_eq!(f.curves[1].real_count, 2 + 6);
        let s = f.surfaces[0];
        assert_eq!(s.kind, SURFACE_EXTRUSION);
        assert_eq!(f.ints[s.int_offset as usize], CURVE_LINE);
        assert_eq!(
            &f.reals[s.real_offset as usize..s.real_offset as usize + 3],
            &[0.0, 0.0, 1.0]
        );
        assert_eq!(f.vertices, vec![1.0, 2.0, 3.0, 0.5]);
        assert!(f.faces[0].reversed);
        assert_eq!(
            f.coedges[0],
            FlatCoedge {
                edge: 0,
                forward: false
            }
        );
        assert_eq!(f.faces[0].point_loop_count, 1);
        assert_eq!(f.point_loops, vec![0]);
        assert_eq!(f.shells[0].face_count, 1);
        assert!(!f.shells[0].closed);
    }
}
