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
//! | curve 4 on surface | surface_kind surface_ints... degree n_knots n_poles rational mults... | surface_reals... knots... poles(2 each)... weights... |
//! | surface 0 plane | - | origin(3) normal(3) u_dir(3) |
//! | surface 1 cone | - | origin(3) axis(3) ref_dir(3) radius half_angle |
//! | surface 2 sphere | - | center(3) axis(3) ref_dir(3) radius |
//! | surface 3 torus | - | center(3) axis(3) ref_dir(3) major minor |
//! | surface 4 bspline | u_deg v_deg n_uk n_vk nu nv rational u_wraps v_wraps u_mults... v_mults... | u_knots... v_knots... poles(3 each, u-major)... weights... |
//! | surface 5 extrusion | curve_kind curve_ints... | dir(3) curve_reals... |
//! | surface 6 revolution | curve_kind curve_ints... | origin(3) axis(3) curve_reals... |
//! | surface 7 ruled | from_kind from_ints... to_kind to_ints... | from_reals... to_reals... |
//! | surface 8 arc sweep | n (kind ints...) n times | weights(n) reals... n times |
//! | surface 9 rolling ball | (is_curve kind ints...) twice, spine_kind spine_ints... | radius v0 v1 needed0 needed1 support_reals... twice, spine_reals... |
//! | curve 5 unknown | - | - |
//! | surface 10 offset | basis_kind basis_ints... | distance basis_reals... |
//! | surface 11 unknown | - | - |
//!
//! A helix ([`Curve::Helix`]) is encoded as its B-spline (curve 2). An
//! unknown curve or surface (an edge or face the reader could not decode)
//! is left out by the builder, and a shell with a face on an unknown
//! surface is not closed.
//!
//! A face's point loops (`Face::point_loops`) are the range
//! `first_point_loop .. first_point_loop + point_loop_count` of
//! `point_loops`, which holds vertex indices.

use crate::brep::{BlendSupport, Body, Curve, Surface};

pub const CURVE_LINE: i32 = 0;
pub const CURVE_ELLIPSE: i32 = 1;
pub const CURVE_BSPLINE: i32 = 2;
pub const CURVE_INTERPOLATED: i32 = 3;
pub const CURVE_ON_SURFACE: i32 = 4;
pub const CURVE_UNKNOWN: i32 = 5;

pub const SURFACE_PLANE: i32 = 0;
pub const SURFACE_CONE: i32 = 1;
pub const SURFACE_SPHERE: i32 = 2;
pub const SURFACE_TORUS: i32 = 3;
pub const SURFACE_BSPLINE: i32 = 4;
pub const SURFACE_EXTRUSION: i32 = 5;
pub const SURFACE_REVOLUTION: i32 = 6;
pub const SURFACE_RULED: i32 = 7;
pub const SURFACE_ARC_SWEEP: i32 = 8;
pub const SURFACE_ROLLING_BALL: i32 = 9;
pub const SURFACE_OFFSET: i32 = 10;
pub const SURFACE_UNKNOWN: i32 = 11;

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
        Curve::Helix(h) => encode_curve(&Curve::BSpline(h.bspline(h.spans)), ints, reals),
        Curve::Unknown(_) => CURVE_UNKNOWN,
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
        Curve::OnSurface { surface, pcurve } => {
            let at = ints.len();
            ints.push(0);
            ints[at] = encode_surface(surface, ints, reals);
            ints.extend([
                ni(pcurve.degree),
                ni(pcurve.knots.len()),
                ni(pcurve.poles.len()),
                i32::from(pcurve.weights.is_some()),
            ]);
            ints.extend(pcurve.mults.iter().map(|&m| ni(m)));
            reals.extend_from_slice(&pcurve.knots);
            for p in &pcurve.poles {
                reals.extend_from_slice(p);
            }
            if let Some(w) = &pcurve.weights {
                reals.extend_from_slice(w);
            }
            CURVE_ON_SURFACE
        }
    }
}

fn encode_surface(s: &Surface, ints: &mut Vec<i32>, reals: &mut Vec<f64>) -> i32 {
    match s {
        Surface::Offset { basis, distance } => {
            reals.push(*distance);
            let at = ints.len();
            ints.push(0);
            ints[at] = encode_surface(basis, ints, reals);
            SURFACE_OFFSET
        }
        Surface::Unknown(_) => SURFACE_UNKNOWN,
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
                // (A closed surface's parameter wraps as a periodic one's:
                // faces cross its seam, mitcad#499.)
                i32::from(b.u_periodic || b.u_closed),
                i32::from(b.v_periodic || b.v_closed),
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
        Surface::RollingBall {
            supports,
            spine,
            range,
            needed,
            radius,
            stored,
        } => {
            reals.push(*radius);
            reals.extend_from_slice(range);
            reals.extend_from_slice(needed);
            for support in supports.iter() {
                let at = ints.len();
                ints.extend([0, 0]);
                let (is_curve, kind) = match support {
                    BlendSupport::Surface(s) => (0, encode_surface(s, ints, reals)),
                    BlendSupport::Curve(c) => (1, encode_curve(c, ints, reals)),
                };
                ints[at] = is_curve;
                ints[at + 1] = kind;
            }
            let at = ints.len();
            ints.push(0);
            ints[at] = encode_curve(spine, ints, reals);
            // The stored approximation: 0, or 1 and the surface (mitcad#622).
            match stored {
                None => ints.push(0),
                Some(s) => {
                    ints.push(1);
                    let at = ints.len();
                    ints.push(0);
                    ints[at] = encode_surface(s, ints, reals);
                }
            }
            SURFACE_ROLLING_BALL
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
                    // Closed where the builder makes every face too.
                    closed: body.shell_is_closed_solid(shell),
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

    #[test]
    fn encodes_rolling_balls_and_curves_on_them() {
        // mitcad#621: a rolling ball between a plane and a circle along a
        // line, and a curve on it.
        let ball = Surface::RollingBall {
            supports: Box::new([
                BlendSupport::Surface(Surface::Plane {
                    origin: [0.0; 3],
                    normal: [0.0, 0.0, 1.0],
                    u_dir: [1.0, 0.0, 0.0],
                }),
                BlendSupport::Curve(Curve::Ellipse {
                    center: [0.0, 0.0, 3.0],
                    normal: [0.0, 0.0, 1.0],
                    major_dir: [1.0, 0.0, 0.0],
                    major: 10.0,
                    minor: 10.0,
                }),
            ]),
            spine: Curve::Line {
                origin: [0.0; 3],
                dir: [0.0, 1.0, 0.0],
            },
            range: [0.0, 4.0],
            needed: [1.0, 3.0],
            radius: 2.0,
            stored: None,
        };
        let on = Curve::OnSurface {
            surface: Box::new(ball.clone()),
            pcurve: crate::brep::BSplineCurve2d {
                degree: 1,
                knots: vec![1.0, 3.0],
                mults: vec![2, 2],
                poles: vec![[0.0, 1.0], [0.0, 3.0]],
                weights: None,
            },
        };
        let mut body = Body {
            curves: vec![on],
            surfaces: vec![ball],
            ..Body::default()
        };
        let f = FlatBody::from_body(&body);
        let s = f.surfaces[0];
        assert_eq!(s.kind, SURFACE_ROLLING_BALL);
        let ints = &f.ints[s.int_offset as usize..(s.int_offset + s.int_count) as usize];
        assert_eq!(ints, &[0, SURFACE_PLANE, 1, CURVE_ELLIPSE, CURVE_LINE, 0]);
        let reals = &f.reals[s.real_offset as usize..(s.real_offset + s.real_count) as usize];
        assert_eq!(&reals[..5], &[2.0, 0.0, 4.0, 1.0, 3.0]);
        assert_eq!(reals.len(), 5 + 9 + 11 + 6);
        let c = f.curves[0];
        assert_eq!(c.kind, CURVE_ON_SURFACE);
        let ints = &f.ints[c.int_offset as usize..(c.int_offset + c.int_count) as usize];
        assert_eq!(ints[0], SURFACE_ROLLING_BALL);
        assert_eq!(&ints[7..], &[1, 2, 2, 0, 2, 2]);
        let reals = &f.reals[c.real_offset as usize..(c.real_offset + c.real_count) as usize];
        assert_eq!(&reals[31..], &[1.0, 3.0, 0.0, 1.0, 0.0, 3.0]);

        // mitcad#622: with the approximation the file stores, after the
        // spine.
        let Surface::RollingBall { stored, .. } = &mut body.surfaces[0] else {
            unreachable!()
        };
        *stored = Some(Box::new(Surface::Plane {
            origin: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            u_dir: [1.0, 0.0, 0.0],
        }));
        let f = FlatBody::from_body(&body);
        let s = f.surfaces[0];
        let ints = &f.ints[s.int_offset as usize..(s.int_offset + s.int_count) as usize];
        assert_eq!(
            ints,
            &[
                0,
                SURFACE_PLANE,
                1,
                CURVE_ELLIPSE,
                CURVE_LINE,
                1,
                SURFACE_PLANE
            ]
        );
        let reals = &f.reals[s.real_offset as usize..(s.real_offset + s.real_count) as usize];
        assert_eq!(reals.len(), 5 + 9 + 11 + 6 + 9);
    }

    #[test]
    fn encodes_helices_offsets_and_unknown_geometry() {
        // mitcad_private#198: a helix goes as its B-spline, an offset as its
        // distance and basis, unknown geometry as its kind alone; a shell
        // with a face on an unknown surface is not closed.
        let helix = crate::brep::Helix {
            center: [0.0; 3],
            major: [10.0, 0.0, 0.0],
            minor: [0.0, 10.0, 0.0],
            pitch: [0.0, 0.0, 2.0],
            scale: 1.0,
            growth: 0.0,
            range: [0.0, 2.0 * std::f64::consts::TAU],
            spans: 24,
        };
        let plane = Surface::Plane {
            origin: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            u_dir: [1.0, 0.0, 0.0],
        };
        let face = |surface: usize| Face {
            surface,
            reversed: false,
            double_sided: false,
            loops: vec![vec![
                Coedge {
                    edge: 0,
                    forward: true,
                },
                Coedge {
                    edge: 0,
                    forward: false,
                },
            ]],
            point_loops: vec![],
        };
        let body = Body {
            lumps: vec![Lump {
                shells: vec![Shell {
                    faces: vec![0, 1],
                    wire_edges: vec![],
                }],
            }],
            faces: vec![face(0), face(1)],
            edges: vec![Edge {
                curve: 1,
                t: [0.0, 1.0],
                vertices: [0, 0],
                tolerance: 0.0,
            }],
            vertices: vec![Vertex {
                point: [0.0; 3],
                tolerance: 0.0,
            }],
            curves: vec![Curve::Helix(helix), Curve::Unknown("why".into())],
            surfaces: vec![
                Surface::Offset {
                    basis: Box::new(plane),
                    distance: 1.5,
                },
                Surface::Unknown("why".into()),
            ],
            transform: None,
        };
        assert!(body.shell_is_closed(&body.lumps[0].shells[0]));
        let f = FlatBody::from_body(&body);
        let c = f.curves[0];
        assert_eq!(c.kind, CURVE_BSPLINE);
        // Degree 7, 25 knots, 4 * 24 + 4 poles, polynomial, not periodic.
        assert_eq!(
            &f.ints[c.int_offset as usize..c.int_offset as usize + 5],
            &[7, 25, 100, 0, 0]
        );
        assert_eq!(f.curves[1].kind, CURVE_UNKNOWN);
        assert_eq!((f.curves[1].int_count, f.curves[1].real_count), (0, 0));
        let s = f.surfaces[0];
        assert_eq!(s.kind, SURFACE_OFFSET);
        assert_eq!(f.ints[s.int_offset as usize], SURFACE_PLANE);
        assert_eq!(f.reals[s.real_offset as usize], 1.5);
        assert_eq!(s.real_count, 1 + 9);
        assert_eq!(f.surfaces[1].kind, SURFACE_UNKNOWN);
        assert!(!f.shells[0].closed);
    }
}
