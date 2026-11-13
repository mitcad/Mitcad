// SPDX-License-Identifier: MIT
//! The objects of a .3dm file as neutral bodies ([`mitcad_asm::brep::Body`]), which
//! OCCT builds as the `.f3d` and `.dwg` imports' bodies are built:
//!
//! - A B-rep keeps its topology. An edge on a polycurve becomes one edge per
//!   segment, so that each has a curve of one kind with the file's own
//!   parameters (lines by length, arcs by angle, NURBS as they are). A
//!   trim's direction is relative to the face's normal (as the neutral
//!   model has it), the trim curves are left to OCCT. Singular trims (at
//!   the poles of a surface) have no edge and are left out of their loops.
//! - Surfaces: NURBS (clamped), planes, surfaces of revolution (a curve
//!   turned about an axis), sum surfaces (as the exact B-spline).
//! - An extrusion is built as its B-rep would be: a side face per
//!   segment of each profile, and the caps, its faces facing out.
//!
//! Coordinates are multiplied by the file's millimetres per unit.

use crate::geom::{self, Brep, Curve, Extrusion, LoopType, P3, Plane, Surface, TrimType};
use crate::nurbs::{self, Spline};
use mitcad_asm::brep::{self, Body, Coedge, Edge, Face, Lump, Shell, Vertex};
use mitcad_asm::convert::{LoopTidy, tidy_loop};
use mitcad_vec3::{add, cross, distance, dot, norm, scale, sub, unit};

/// A body converted from an object, with what the conversion could not
/// take.
#[derive(Clone, Debug, Default)]
pub struct Converted {
    pub body: Body,
    pub issues: Vec<String>,
    /// Faces whose surface could not be converted, kept on an unknown
    /// surface (the builder leaves them out), and faces without loops.
    pub skipped_faces: usize,
    /// Edges whose curve could not be converted.
    pub unknown_edges: usize,
    /// Singular trims and edges without a curve, left out of their loops.
    pub degenerate_edges: usize,
    pub tidied: LoopTidy,
}

/// The parameters of a curve.
pub fn domain(c: &Curve) -> Option<[f64; 2]> {
    match c {
        Curve::Nurbs(n) => {
            let p = n.order - 1;
            Some([n.knots[p - 1], n.knots[n.cvs.len() - 1]])
        }
        Curve::Line { domain, .. } | Curve::Arc { domain, .. } => Some(*domain),
        Curve::Polyline { params, .. } | Curve::Poly { params, .. } => {
            Some([*params.first()?, *params.last()?])
        }
        Curve::Other(_) => None,
    }
}

/// The curve as one B-spline (exact shape; an arc's parameter is not its
/// angle).
pub fn to_spline(c: &Curve) -> Option<Spline> {
    match c {
        Curve::Nurbs(n) => Some(Spline::from_nurbs(n)),
        Curve::Line { from, to, domain } => Some(Spline::line(*from, *to, domain[0], domain[1])),
        Curve::Arc {
            plane,
            radius,
            angles,
            domain,
        } => Some(Spline::arc(plane, *radius, *angles, *domain)),
        Curve::Polyline { points, params } => {
            (points.len() >= 2).then(|| Spline::polyline(points, params))
        }
        Curve::Poly { segments, params } => {
            let parts: Option<Vec<Spline>> = segments
                .iter()
                .enumerate()
                .map(|(i, s)| Some(to_spline(s)?.reparametrized(params[i], params[i + 1])))
                .collect();
            Spline::join(parts?)
        }
        Curve::Other(_) => None,
    }
}

fn spline_point(s: &Spline, t: f64) -> P3 {
    s.eval(t)
}

/// A point of the curve (file units).
pub fn point_at(c: &Curve, t: f64) -> Option<P3> {
    match c {
        Curve::Line { from, to, domain } => {
            let f = if domain[1] != domain[0] {
                (t - domain[0]) / (domain[1] - domain[0])
            } else {
                0.0
            };
            Some(add(*from, scale(sub(*to, *from), f)))
        }
        Curve::Arc {
            plane,
            radius,
            angles,
            domain,
        } => {
            let f = if domain[1] != domain[0] {
                (t - domain[0]) / (domain[1] - domain[0])
            } else {
                0.0
            };
            let a = angles[0] + f * (angles[1] - angles[0]);
            let (s, co) = a.sin_cos();
            Some(add(
                plane.origin,
                add(scale(plane.x, radius * co), scale(plane.y, radius * s)),
            ))
        }
        Curve::Nurbs(_) | Curve::Polyline { .. } => to_spline(c).map(|s| spline_point(&s, t)),
        Curve::Poly { segments, params } => {
            let i = (0..segments.len())
                .find(|&i| t <= params[i + 1])
                .unwrap_or(segments.len() - 1);
            let d = domain(&segments[i])?;
            let f = (t - params[i]) / (params[i + 1] - params[i]).max(f64::MIN_POSITIVE);
            point_at(&segments[i], d[0] + f * (d[1] - d[0]))
        }
        Curve::Other(_) => None,
    }
}

pub fn curve_is_closed(c: &Curve) -> bool {
    match (domain(c), c) {
        (Some(d), _) => match (point_at(c, d[0]), point_at(c, d[1])) {
            (Some(a), Some(b)) => distance(a, b) <= 1e-9 * (1.0 + norm(a)),
            _ => false,
        },
        _ => false,
    }
}

/// A piece of an edge: a neutral curve and its parameters (millimetres).
#[derive(Clone, Debug)]
pub struct Piece {
    pub curve: brep::Curve,
    pub t: [f64; 2],
}

fn scaled(p: P3, s: f64) -> P3 {
    scale(p, s)
}

/// The part `[ta, tb]` of the curve as pieces of one kind each, with the
/// file's parameters where they are kept (millimetres).
pub fn pieces(c: &Curve, ta: f64, tb: f64, s: f64, out: &mut Vec<Piece>) -> Result<(), String> {
    match c {
        Curve::Line { from, to, domain } => {
            let d = sub(*to, *from);
            let len = norm(d);
            let dir = unit(d).ok_or("a line of length 0")?;
            let span = domain[1] - domain[0];
            if span == 0.0 {
                return Err("a line of empty domain".to_owned());
            }
            let at = |t: f64| (t - domain[0]) / span * len * s;
            out.push(Piece {
                curve: brep::Curve::Line {
                    origin: scaled(*from, s),
                    dir,
                },
                t: [at(ta), at(tb)],
            });
        }
        Curve::Arc {
            plane,
            radius,
            angles,
            domain,
        } => {
            let span = domain[1] - domain[0];
            if span == 0.0 || *radius <= 0.0 {
                return Err("a degenerate arc".to_owned());
            }
            let at = |t: f64| angles[0] + (t - domain[0]) / span * (angles[1] - angles[0]);
            let normal = unit(plane.z).ok_or("an arc without a plane")?;
            let major_dir = unit(plane.x).ok_or("an arc without a plane")?;
            let (a, b) = (at(ta), at(tb));
            // A parameter that runs against the angle: the circle turned
            // over (its angle then increases with the parameter).
            let (normal, a, b) = if b >= a {
                (normal, a, b)
            } else {
                (scale(normal, -1.0), -a, -b)
            };
            out.push(Piece {
                curve: brep::Curve::Ellipse {
                    center: scaled(plane.origin, s),
                    normal,
                    major_dir,
                    major: radius * s,
                    minor: radius * s,
                },
                t: [a, b],
            });
        }
        Curve::Nurbs(n) => {
            let spline = Spline::from_nurbs(n);
            out.push(Piece {
                curve: brep::Curve::BSpline(spline.to_brep(s)),
                t: [ta, tb],
            });
        }
        Curve::Polyline { points, params } => {
            for i in 0..points.len().saturating_sub(1) {
                let (p0, p1) = (params[i], params[i + 1]);
                let (a, b) = (ta.max(p0), tb.min(p1));
                if b - a <= (p1 - p0).abs() * 1e-12 {
                    continue;
                }
                if distance(points[i], points[i + 1]) == 0.0 {
                    continue;
                }
                let line = Curve::Line {
                    from: points[i],
                    to: points[i + 1],
                    domain: [p0, p1],
                };
                pieces(&line, a, b, s, out)?;
            }
        }
        Curve::Poly { segments, params } => {
            for (i, seg) in segments.iter().enumerate() {
                let (p0, p1) = (params[i], params[i + 1]);
                let (a, b) = (ta.max(p0), tb.min(p1));
                if b - a <= (p1 - p0).abs() * 1e-12 {
                    continue;
                }
                let d = domain(seg)
                    .ok_or_else(|| format!("a polycurve segment that is an {}", other_name(seg)))?;
                let map = |t: f64| d[0] + (t - p0) / (p1 - p0) * (d[1] - d[0]);
                pieces(seg, map(a), map(b), s, out)?;
            }
        }
        Curve::Other(name) => return Err(format!("a curve that is a {name}")),
    }
    Ok(())
}

fn other_name(c: &Curve) -> &str {
    match c {
        Curve::Other(n) => n,
        _ => "unknown curve",
    }
}

/// A neutral curve of the whole of a curve, for surfaces (the shape, not
/// the parameters: an arc of a polycurve is a B-spline).
fn shape_curve(c: &Curve, s: f64) -> Result<brep::Curve, String> {
    let d = domain(c).ok_or_else(|| format!("a curve that is a {}", other_name(c)))?;
    let mut out = Vec::new();
    if matches!(c, Curve::Line { .. } | Curve::Arc { .. } | Curve::Nurbs(_)) {
        pieces(c, d[0], d[1], s, &mut out)?;
        if let [p] = out.as_slice() {
            return Ok(p.curve.clone());
        }
    }
    let spline = to_spline(c).ok_or("a curve that cannot be converted")?;
    Ok(brep::Curve::BSpline(spline.to_brep(s)))
}

/// The neutral surface, and whether its normal is against the file's.
pub fn surface(srf: &Surface, s: f64) -> Result<(brep::Surface, bool), String> {
    match srf {
        Surface::Nurbs(n) => Ok((brep::Surface::BSpline(nurbs::surface_to_brep(n, s)), false)),
        Surface::Plane {
            plane,
            domain,
            extents,
        } => {
            let normal = unit(plane.z).ok_or("a plane without a normal")?;
            let u_dir = unit(plane.x).ok_or("a plane without axes")?;
            let sign = |k: usize| (extents[k][1] - extents[k][0]) * (domain[k][1] - domain[k][0]);
            Ok((
                brep::Surface::Plane {
                    origin: scaled(plane.origin, s),
                    normal,
                    u_dir,
                },
                sign(0) * sign(1) < 0.0,
            ))
        }
        Surface::Revolution {
            axis,
            transposed,
            curve,
            ..
        } => {
            let dir =
                unit(sub(axis[1], axis[0])).ok_or("a surface of revolution without an axis")?;
            Ok((
                brep::Surface::Revolution {
                    curve: shape_curve(curve, s)?,
                    origin: scaled(axis[0], s),
                    axis: dir,
                },
                *transposed,
            ))
        }
        Surface::Sum { base, curves } => {
            let a = to_spline(&curves[0]).ok_or("a sum surface of an unknown curve")?;
            let b = to_spline(&curves[1]).ok_or("a sum surface of an unknown curve")?;
            Ok((
                brep::Surface::BSpline(nurbs::sum_surface(&a, &b, *base, s)),
                false,
            ))
        }
        Surface::Other(name) => Err(format!("a surface that is an {name}")),
    }
}

/// The surface of a face that covers all of it (its loop only the
/// surface's seams and poles), as a bounded surface, which the builder
/// takes with its natural bounds, seams and poles included: a surface of
/// revolution as its exact B-spline ([`nurbs::revolution_surface`]), the
/// others as [`surface`] makes them. And whether its normal is against
/// the file's.
fn whole_surface(srf: &Surface, s: f64) -> Result<(brep::Surface, bool), String> {
    match srf {
        Surface::Revolution {
            axis,
            angles,
            transposed,
            curve,
        } => {
            let dir =
                unit(sub(axis[1], axis[0])).ok_or("a surface of revolution without an axis")?;
            let profile = to_spline(curve).ok_or("a surface of revolution of an unknown curve")?;
            Ok((
                brep::Surface::BSpline(nurbs::revolution_surface(
                    &profile, axis[0], dir, *angles, s,
                )),
                *transposed,
            ))
        }
        Surface::Plane { .. } => Err("a plane surface whose loop has no edge".to_owned()),
        _ => surface(srf, s),
    }
}

/// Whether a face's loops hold only seams and singular trims (and at
/// least one trim): the face is all of its closed surface, a sphere whose
/// loop runs from pole to pole along its seam and back.
fn covers_its_surface(b: &Brep, f: &geom::BrepFace) -> bool {
    let mut trims = 0;
    for &li in &f.loops {
        let Some(lp) = usize::try_from(li).ok().and_then(|i| b.loops.get(i)) else {
            return false;
        };
        for &ti in &lp.trims {
            let Some(trim) = usize::try_from(ti).ok().and_then(|i| b.trims.get(i)) else {
                return false;
            };
            if !matches!(trim.kind, TrimType::Seam | TrimType::Singular) {
                return false;
            }
            trims += 1;
        }
    }
    trims > 0
}

fn finite_tolerance(t: f64, s: f64) -> f64 {
    if t.is_finite() && t > 0.0 && t < 1e10 {
        t * s
    } else {
        0.0
    }
}

/// A B-rep as a neutral body (millimetres: `s` per file unit).
pub fn brep_body(b: &Brep, s: f64) -> Converted {
    let mut out = Converted::default();
    let body = &mut out.body;
    body.vertices = b
        .vertices
        .iter()
        .map(|v| Vertex {
            point: scaled(v.point, s),
            tolerance: finite_tolerance(v.tolerance, s),
        })
        .collect();
    let vertex = |i: i32| usize::try_from(i).ok().filter(|&i| i < b.vertices.len());

    // Each file edge as a chain of neutral edges in its own direction.
    let mut chains: Vec<Option<Vec<(usize, bool)>>> = Vec::with_capacity(b.edges.len());
    for (ei, e) in b.edges.iter().enumerate() {
        let (Some(v0), Some(v1)) = (vertex(e.vertices[0]), vertex(e.vertices[1])) else {
            out.issues.push(format!("edge {ei}: no vertices"));
            chains.push(None);
            continue;
        };
        // In the curve's direction: from a to b.
        let (a, z) = if e.reversed { (v1, v0) } else { (v0, v1) };
        let tolerance = finite_tolerance(e.tolerance, s);
        let curve = usize::try_from(e.curve)
            .ok()
            .and_then(|c| b.curves.get(c))
            .and_then(Option::as_ref);
        let mut list = Vec::new();
        let made = match curve {
            Some(c) => pieces(c, e.proxy[0], e.proxy[1], s, &mut list),
            None => Err("no curve".to_owned()),
        };
        if let Err(why) = made {
            out.issues.push(format!("edge {ei}: {why}"));
            out.unknown_edges += 1;
            list = vec![Piece {
                curve: brep::Curve::Unknown(why),
                t: [0.0, 1.0],
            }];
        }
        let mut chain = Vec::with_capacity(list.len());
        let mut start = a;
        let n = list.len();
        for (k, piece) in list.into_iter().enumerate() {
            let end = if k + 1 == n {
                z
            } else {
                body.vertices.push(Vertex {
                    point: piece.curve.eval(piece.t[1]),
                    tolerance,
                });
                body.vertices.len() - 1
            };
            body.curves.push(piece.curve);
            body.edges.push(Edge {
                curve: body.curves.len() - 1,
                t: piece.t,
                vertices: [start, end],
                tolerance,
            });
            chain.push((body.edges.len() - 1, true));
            start = end;
        }
        if e.reversed {
            chain.reverse();
            for c in &mut chain {
                c.1 = !c.1;
            }
        }
        chains.push(Some(chain));
    }

    // The surfaces, each once.
    let mut surfaces: Vec<Option<(usize, bool)>> = vec![None; b.surfaces.len()];
    let inward = b.is_solid == 2;
    let mut faces = Vec::new();
    for (fi, f) in b.faces.iter().enumerate() {
        let si = usize::try_from(f.surface)
            .ok()
            .filter(|&i| i < b.surfaces.len());
        // A face that is all of its closed surface (a sphere: its seam
        // from pole to pole and back): no loops on a bounded surface,
        // which the builder takes whole, with its seam and poles; its seam
        // edge and singular trims are not needed.
        if covers_its_surface(b, f)
            && let Some(Some(srf)) = si.map(|si| &b.surfaces[si])
        {
            match whole_surface(srf, s) {
                Ok((neutral, flip)) => {
                    body.surfaces.push(neutral);
                    body.faces.push(Face {
                        surface: body.surfaces.len() - 1,
                        reversed: f.reversed ^ inward ^ flip,
                        double_sided: false,
                        loops: Vec::new(),
                        point_loops: Vec::new(),
                    });
                    faces.push(body.faces.len() - 1);
                }
                Err(why) => {
                    out.issues.push(format!("face {fi}: {why}"));
                    out.skipped_faces += 1;
                }
            }
            continue;
        }
        let (surface, flip) = match si {
            Some(si) => match surfaces[si] {
                Some(done) => done,
                None => {
                    let made = match &b.surfaces[si] {
                        Some(srf) => surface(srf, s),
                        None => Err("no surface".to_owned()),
                    };
                    let (neutral, flip) = made.unwrap_or_else(|why| {
                        out.issues.push(format!("face {fi}: {why}"));
                        (brep::Surface::Unknown(why), false)
                    });
                    body.surfaces.push(neutral);
                    surfaces[si] = Some((body.surfaces.len() - 1, flip));
                    (body.surfaces.len() - 1, flip)
                }
            },
            None => {
                out.issues.push(format!("face {fi}: no surface"));
                body.surfaces
                    .push(brep::Surface::Unknown("no surface".to_owned()));
                (body.surfaces.len() - 1, false)
            }
        };
        if matches!(body.surfaces[surface], brep::Surface::Unknown(_)) {
            out.skipped_faces += 1;
        }
        // The face's normal against the surface's, geometrically.
        let face_rev = f.reversed ^ inward;
        let planar = matches!(body.surfaces[surface], brep::Surface::Plane { .. });
        let mut loops = Vec::new();
        for &li in &f.loops {
            let Some(lp) = usize::try_from(li).ok().and_then(|i| b.loops.get(i)) else {
                continue;
            };
            if matches!(lp.kind, LoopType::CurveOnSurface | LoopType::PointOnSurface) {
                continue;
            }
            let mut coedges = Vec::new();
            for &ti in &lp.trims {
                let Some(trim) = usize::try_from(ti).ok().and_then(|i| b.trims.get(i)) else {
                    continue;
                };
                let chain = usize::try_from(trim.edge)
                    .ok()
                    .and_then(|e| chains.get(e))
                    .and_then(Option::as_ref);
                let Some(chain) = chain.filter(|_| trim.kind != TrimType::Singular) else {
                    out.degenerate_edges += 1;
                    continue;
                };
                // In the trim's direction, then relative to the face's
                // normal.
                let along: Vec<(usize, bool)> = if trim.reversed_3d {
                    chain.iter().rev().map(|&(e, f)| (e, !f)).collect()
                } else {
                    chain.clone()
                };
                coedges.extend(along.into_iter().map(|(edge, fw)| Coedge {
                    edge,
                    forward: fw ^ face_rev,
                }));
            }
            if face_rev {
                coedges.reverse();
            }
            let (parts, spurs, pinches) = tidy_loop(coedges, &body.edges, planar);
            out.tidied.spurs += spurs;
            out.tidied.pinches += pinches;
            loops.extend(parts);
        }
        if loops.is_empty() {
            out.issues
                .push(format!("face {fi}: none of its loops has an edge"));
            out.skipped_faces += 1;
            continue;
        }
        body.faces.push(Face {
            surface,
            reversed: face_rev ^ flip,
            double_sided: false,
            loops,
            point_loops: Vec::new(),
        });
        faces.push(body.faces.len() - 1);
    }
    body.lumps.push(Lump {
        shells: vec![Shell {
            faces,
            wire_edges: Vec::new(),
        }],
    });
    body.split_disconnected_shells();
    out
}

/// A planar loop's signed area in the frame (x, y): positive when it runs
/// counterclockwise about x × y.
fn signed_area(points: &[P3], x: P3, y: P3) -> f64 {
    let n = points.len();
    let mut a = 0.0;
    for i in 0..n {
        let (p, q) = (points[i], points[(i + 1) % n]);
        a += dot(p, x) * dot(q, y) - dot(q, x) * dot(p, y);
    }
    a / 2.0
}

/// Points along a piece, for its profile's area.
fn samples(p: &Piece, out: &mut Vec<P3>) {
    let n = match p.curve {
        brep::Curve::Line { .. } => 1,
        _ => 16,
    };
    for k in 0..n {
        let t = p.t[0] + (p.t[1] - p.t[0]) * k as f64 / n as f64;
        out.push(p.curve.eval(t));
    }
}

/// The curve moved by an affine map (`m` rows, then `t`), file units.
fn mapped(c: &Curve, m: &[[f64; 3]; 3], t: P3) -> Curve {
    let point = |p: P3| {
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + t[0],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + t[1],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + t[2],
        ]
    };
    let vector = |p: P3| {
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2],
        ]
    };
    match c {
        Curve::Nurbs(n) => {
            let mut n = n.clone();
            for cv in &mut n.cvs {
                let w = cv[3];
                let p = vector([cv[0], cv[1], cv[2]]);
                *cv = [p[0] + t[0] * w, p[1] + t[1] * w, p[2] + t[2] * w, w];
            }
            n.dim = 3;
            Curve::Nurbs(n)
        }
        Curve::Line { from, to, domain } => Curve::Line {
            from: point(*from),
            to: point(*to),
            domain: *domain,
        },
        Curve::Arc {
            plane,
            radius,
            angles,
            domain,
        } => Curve::Arc {
            plane: Plane {
                origin: point(plane.origin),
                x: vector(plane.x),
                y: vector(plane.y),
                z: cross(vector(plane.x), vector(plane.y)),
            },
            radius: *radius,
            angles: *angles,
            domain: *domain,
        },
        Curve::Polyline { points, params } => Curve::Polyline {
            points: points.iter().map(|&p| point(p)).collect(),
            params: params.clone(),
        },
        Curve::Poly { segments, params } => Curve::Poly {
            segments: segments.iter().map(|c| mapped(c, m, t)).collect(),
            params: params.clone(),
        },
        Curve::Other(n) => Curve::Other(n.clone()),
    }
}

fn translated(c: &brep::Curve, d: P3) -> brep::Curve {
    match c {
        brep::Curve::Line { origin, dir } => brep::Curve::Line {
            origin: add(*origin, d),
            dir: *dir,
        },
        brep::Curve::Ellipse {
            center,
            normal,
            major_dir,
            major,
            minor,
        } => brep::Curve::Ellipse {
            center: add(*center, d),
            normal: *normal,
            major_dir: *major_dir,
            major: *major,
            minor: *minor,
        },
        brep::Curve::BSpline(b) => {
            let mut b = b.clone();
            for p in &mut b.poles {
                *p = add(*p, d);
            }
            brep::Curve::BSpline(b)
        }
        other => other.clone(),
    }
}

/// An extrusion as a neutral body (millimetres: `s` per file unit).
pub fn extrusion_body(e: &Extrusion, s: f64) -> Result<Converted, String> {
    if e.miter.iter().any(Option::is_some) {
        return Err("a mitered extrusion".to_owned());
    }
    let line = |t: f64| add(e.path[0], scale(sub(e.path[1], e.path[0]), t));
    let (p0, p1) = (line(e.t[0]), line(e.t[1]));
    let z = unit(sub(e.path[1], e.path[0])).ok_or("an extrusion of a path of length 0")?;
    let y = unit(e.up).ok_or("an extrusion without its up direction")?;
    let x = unit(cross(y, z)).ok_or("an extrusion whose up direction is along its path")?;
    let d_file = sub(p1, p0);
    let d = scale(d_file, s);
    let height = norm(d);
    let dz = unit(d).ok_or("an extrusion of height 0")?;
    // The profile's plane (local x, y, z) in the world, at the path's start.
    let m = [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]];
    let profiles: Vec<&Curve> = match &e.profile {
        Curve::Poly { segments, .. } if e.profile_count > 1 => segments.iter().collect(),
        c => vec![c],
    };
    let mut out = Converted::default();
    let body = &mut out.body;
    let mut faces = Vec::new();
    // Each profile's bottom and top edges, for the caps.
    let mut cap_loops: Vec<(Vec<usize>, Vec<usize>, bool)> = Vec::new();
    let mut all_closed = true;
    for (k, profile) in profiles.iter().enumerate() {
        let world = mapped(profile, &m, p0);
        let dom =
            domain(&world).ok_or_else(|| format!("a profile that is a {}", other_name(&world)))?;
        let mut list = Vec::new();
        pieces(&world, dom[0], dom[1], s, &mut list)?;
        if list.is_empty() {
            return Err("an empty profile".to_owned());
        }
        let closed = curve_is_closed(&world);
        all_closed &= closed;
        let mut pts = Vec::new();
        for p in &list {
            samples(p, &mut pts);
        }
        // Counterclockwise about the extrusion's direction.
        let ccw = signed_area(&pts, x, y) * dot(z, dz) > 0.0;
        let outer = k == 0;
        // The vertices at the pieces' starts (and the open end).
        let n = list.len();
        let corners = if closed { n } else { n + 1 };
        let first_bottom = body.vertices.len();
        for i in 0..corners {
            let p = if i < n {
                list[i].curve.eval(list[i].t[0])
            } else {
                list[n - 1].curve.eval(list[n - 1].t[1])
            };
            body.vertices.push(Vertex {
                point: p,
                tolerance: 0.0,
            });
            body.vertices.push(Vertex {
                point: add(p, d),
                tolerance: 0.0,
            });
        }
        let bottom = |i: usize| first_bottom + 2 * (i % corners);
        let top = |i: usize| first_bottom + 2 * (i % corners) + 1;
        // The vertical edges at the corners.
        let first_vertical = body.edges.len();
        for i in 0..corners {
            body.curves.push(brep::Curve::Line {
                origin: body.vertices[bottom(i)].point,
                dir: dz,
            });
            body.edges.push(Edge {
                curve: body.curves.len() - 1,
                t: [0.0, height],
                vertices: [bottom(i), top(i)],
                tolerance: 0.0,
            });
        }
        let vertical = |i: usize| first_vertical + (i % corners);
        let mut bottoms = Vec::new();
        let mut tops = Vec::new();
        for (i, piece) in list.iter().enumerate() {
            body.curves.push(piece.curve.clone());
            body.edges.push(Edge {
                curve: body.curves.len() - 1,
                t: piece.t,
                vertices: [bottom(i), bottom(i + 1)],
                tolerance: 0.0,
            });
            let eb = body.edges.len() - 1;
            body.curves.push(translated(&piece.curve, d));
            body.edges.push(Edge {
                curve: body.curves.len() - 1,
                t: piece.t,
                vertices: [top(i), top(i + 1)],
                tolerance: 0.0,
            });
            let et = body.edges.len() - 1;
            bottoms.push(eb);
            tops.push(et);
            body.surfaces.push(brep::Surface::Extrusion {
                curve: piece.curve.clone(),
                dir: dz,
            });
            // The surface's normal (along the curve × up) faces out of an
            // outer profile that runs counterclockwise, out of a hole that
            // runs clockwise.
            let reversed = if outer { !ccw } else { ccw };
            let mut lp = vec![
                Coedge {
                    edge: eb,
                    forward: true,
                },
                Coedge {
                    edge: vertical(i + 1),
                    forward: true,
                },
                Coedge {
                    edge: et,
                    forward: false,
                },
                Coedge {
                    edge: vertical(i),
                    forward: false,
                },
            ];
            if reversed {
                lp.reverse();
                for c in &mut lp {
                    c.forward = !c.forward;
                }
            }
            body.faces.push(Face {
                surface: body.surfaces.len() - 1,
                reversed,
                double_sided: false,
                loops: vec![lp],
                point_loops: Vec::new(),
            });
            faces.push(body.faces.len() - 1);
        }
        cap_loops.push((bottoms, tops, ccw));
    }
    if all_closed {
        for (end, normal_sign) in [(0usize, -1.0), (1, 1.0)] {
            if !e.cap[end] {
                continue;
            }
            let normal = scale(dz, normal_sign);
            let origin = if end == 0 {
                scale(p0, s)
            } else {
                add(scale(p0, s), d)
            };
            body.surfaces.push(brep::Surface::Plane {
                origin,
                normal,
                u_dir: x,
            });
            let mut loops = Vec::new();
            for (k, (bottoms, tops, ccw)) in cap_loops.iter().enumerate() {
                let edges = if end == 0 { bottoms } else { tops };
                // Counterclockwise about the cap's normal for the outer
                // loop, clockwise for the holes.
                let about_normal = if end == 0 { !ccw } else { *ccw };
                let forward = if k == 0 { about_normal } else { !about_normal };
                let mut lp: Vec<Coedge> = edges
                    .iter()
                    .map(|&edge| Coedge {
                        edge,
                        forward: true,
                    })
                    .collect();
                if !forward {
                    lp.reverse();
                    for c in &mut lp {
                        c.forward = false;
                    }
                }
                loops.push(lp);
            }
            body.faces.push(Face {
                surface: body.surfaces.len() - 1,
                reversed: false,
                double_sided: false,
                loops,
                point_loops: Vec::new(),
            });
            faces.push(body.faces.len() - 1);
        }
    }
    body.lumps.push(Lump {
        shells: vec![Shell {
            faces,
            wire_edges: Vec::new(),
        }],
    });
    Ok(out)
}

/// An untrimmed surface object as a body of one face: a NURBS surface
/// whole, a plane surface its rectangle.
pub fn surface_body(srf: &Surface, s: f64) -> Result<Converted, String> {
    let mut out = Converted::default();
    let body = &mut out.body;
    match srf {
        Surface::Nurbs(_) => {
            let (neutral, _) = surface(srf, s)?;
            body.surfaces.push(neutral);
            body.faces.push(Face {
                surface: 0,
                reversed: false,
                double_sided: false,
                loops: Vec::new(),
                point_loops: Vec::new(),
            });
        }
        Surface::Plane { plane, extents, .. } => {
            // The face faces along the surface's normal: against the
            // plane's where one extent runs against its domain (`flip`).
            let (neutral, flip) = surface(srf, s)?;
            body.surfaces.push(neutral);
            let (x0, x1) = (
                extents[0][0].min(extents[0][1]),
                extents[0][0].max(extents[0][1]),
            );
            let (y0, y1) = (
                extents[1][0].min(extents[1][1]),
                extents[1][0].max(extents[1][1]),
            );
            let at = |u: f64, v: f64| {
                scaled(
                    add(plane.origin, add(scale(plane.x, u), scale(plane.y, v))),
                    s,
                )
            };
            let corners = [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)];
            for p in corners {
                body.vertices.push(Vertex {
                    point: p,
                    tolerance: 0.0,
                });
            }
            let mut lp = Vec::new();
            for i in 0..4 {
                let (a, b) = (corners[i], corners[(i + 1) % 4]);
                let dir = unit(sub(b, a)).ok_or("a plane surface of size 0")?;
                body.curves.push(brep::Curve::Line { origin: a, dir });
                body.edges.push(Edge {
                    curve: i,
                    t: [0.0, distance(a, b)],
                    vertices: [i, (i + 1) % 4],
                    tolerance: 0.0,
                });
                lp.push(Coedge {
                    edge: i,
                    forward: true,
                });
            }
            // The corners run counterclockwise about the plane's normal;
            // the loop runs so about the face's.
            if flip {
                lp.reverse();
                for c in &mut lp {
                    c.forward = false;
                }
            }
            body.faces.push(Face {
                surface: 0,
                reversed: flip,
                double_sided: false,
                loops: vec![lp],
                point_loops: Vec::new(),
            });
        }
        Surface::Revolution { .. } | Surface::Sum { .. } => {
            return Err(
                "an untrimmed surface of revolution or sum surface (not a B-rep)".to_owned(),
            );
        }
        Surface::Other(name) => return Err(format!("a surface that is an {name}")),
    }
    body.lumps.push(Lump {
        shells: vec![Shell {
            faces: vec![0],
            wire_edges: Vec::new(),
        }],
    });
    Ok(out)
}

/// The render meshes of a B-rep's faces as one mesh (file units), where
/// every face has one.
pub fn render_mesh(b: &Brep) -> Option<geom::Mesh> {
    if b.render_meshes.len() != b.faces.len() || b.render_meshes.iter().any(Option::is_none) {
        return None;
    }
    let mut mesh = geom::Mesh::default();
    for m in b.render_meshes.iter().flatten() {
        // A face's index past its own mesh's vertices stays past all of
        // them, so that `triangles` leaves the face out.
        let base = u32::try_from(mesh.vertices.len()).ok()?;
        let end = base.checked_add(u32::try_from(m.vertices.len()).ok()?)?;
        mesh.vertices.extend_from_slice(&m.vertices);
        mesh.faces.extend(
            m.faces
                .iter()
                .map(|f| f.map(|i| i.checked_add(base).filter(|&j| j < end).unwrap_or(u32::MAX))),
        );
    }
    (!mesh.faces.is_empty()).then_some(mesh)
}
