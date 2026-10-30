// SPDX-License-Identifier: MIT
//! The representation items of products' shapes, decoded into
//! [`Item`]s in millimetres and radians, in the item's own coordinates
//! (the product's object coordinates, or the representation map's).
//!
//! - Curves ([`Seg`], [`Path`]): polylines, lines, circles and ellipses
//!   (trimmed or whole), composite curves, indexed poly curves with arcs,
//!   B-spline curves (rational too); the 3D curve of surface and
//!   intersection curves.
//! - Profiles ([`Profile`]): the parameterized ones (rectangle, rounded
//!   and hollow rectangle, circle and hollow circle, ellipse, I, L, U, T,
//!   Z, C, trapezium and asymmetric I, with their fillet and edge radii),
//!   arbitrary closed profiles with voids, derived and composite profiles.
//! - Solids: extrusions, revolutions, sweeps along a curve on a surface or
//!   with a fixed reference, swept disks, booleans and clippings with half
//!   spaces (plain and polygonally bounded), CSG primitives, faceted breps
//!   and surface models, triangulated and polygonal face sets, advanced
//!   breps (B-spline and elementary surfaces), mapped items, bounding
//!   boxes.
//!
//! What is not understood is an [`Item::Unsupported`] that says why.

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use serde::Serialize;

use crate::math::{self, Matrix};
use crate::units::Units;
use crate::{Entity, IfcFile, Value};

/// A piece of a curve.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Seg {
    Line {
        start: [f64; 3],
        end: [f64; 3],
    },
    /// A circular arc in `frame`'s xy plane about its origin: from angle
    /// `start` (from the frame's x axis) through `sweep` (positive counter-
    /// clockwise about the frame's z axis); a whole circle sweeps ±2π.
    Arc {
        frame: Matrix,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    /// An elliptical arc likewise (`a` along the frame's x axis), by
    /// parameter angles.
    Ellipse {
        frame: Matrix,
        a: f64,
        b: f64,
        start: f64,
        sweep: f64,
    },
    /// A B-spline: distinct knots with their multiplicities; empty weights
    /// for a non-rational one. It runs from its first to its last knot.
    Spline {
        degree: u32,
        poles: Vec<[f64; 3]>,
        weights: Vec<f64>,
        knots: Vec<f64>,
        multiplicities: Vec<u32>,
    },
    /// A helix about `frame`'s z axis (no IFC curve: the `.rvt` import's
    /// spiral rails, mitcad#385): `(radius cos t, radius sin t, pitch t /
    /// 2π)` in the frame from angle `start` through `sweep`; `pitch` is
    /// the rise per turn counter-clockwise about z (negative: a
    /// left-handed helix).
    Helix {
        frame: Matrix,
        radius: f64,
        pitch: f64,
        start: f64,
        sweep: f64,
    },
}

fn on_conic(frame: &Matrix, a: f64, b: f64, t: f64) -> [f64; 3] {
    math::apply(frame, [a * t.cos(), b * t.sin(), 0.0])
}

fn on_helix(frame: &Matrix, radius: f64, pitch: f64, t: f64) -> [f64; 3] {
    math::apply(frame, [radius * t.cos(), radius * t.sin(), pitch * t / TAU])
}

impl Seg {
    pub fn start(&self) -> [f64; 3] {
        match self {
            Seg::Line { start, .. } => *start,
            Seg::Arc {
                frame,
                radius,
                start,
                ..
            } => on_conic(frame, *radius, *radius, *start),
            Seg::Ellipse {
                frame, a, b, start, ..
            } => on_conic(frame, *a, *b, *start),
            Seg::Spline {
                poles,
                weights,
                knots,
                multiplicities,
                degree,
            } => spline_point(*degree, poles, weights, knots, multiplicities, true),
            Seg::Helix {
                frame,
                radius,
                pitch,
                start,
                ..
            } => on_helix(frame, *radius, *pitch, *start),
        }
    }

    pub fn end(&self) -> [f64; 3] {
        match self {
            Seg::Line { end, .. } => *end,
            Seg::Arc {
                frame,
                radius,
                start,
                sweep,
            } => on_conic(frame, *radius, *radius, start + sweep),
            Seg::Ellipse {
                frame,
                a,
                b,
                start,
                sweep,
            } => on_conic(frame, *a, *b, start + sweep),
            Seg::Spline {
                poles,
                weights,
                knots,
                multiplicities,
                degree,
            } => spline_point(*degree, poles, weights, knots, multiplicities, false),
            Seg::Helix {
                frame,
                radius,
                pitch,
                start,
                sweep,
            } => on_helix(frame, *radius, *pitch, start + sweep),
        }
    }

    /// The same piece run the other way.
    pub fn reversed(&self) -> Seg {
        match self {
            Seg::Line { start, end } => Seg::Line {
                start: *end,
                end: *start,
            },
            Seg::Arc {
                frame,
                radius,
                start,
                sweep,
            } => Seg::Arc {
                frame: *frame,
                radius: *radius,
                start: start + sweep,
                sweep: -sweep,
            },
            Seg::Ellipse {
                frame,
                a,
                b,
                start,
                sweep,
            } => Seg::Ellipse {
                frame: *frame,
                a: *a,
                b: *b,
                start: start + sweep,
                sweep: -sweep,
            },
            Seg::Spline {
                degree,
                poles,
                weights,
                knots,
                multiplicities,
            } => {
                let (first, last) = (knots[0], knots[knots.len() - 1]);
                Seg::Spline {
                    degree: *degree,
                    poles: poles.iter().rev().copied().collect(),
                    weights: weights.iter().rev().copied().collect(),
                    knots: knots.iter().rev().map(|k| first + last - k).collect(),
                    multiplicities: multiplicities.iter().rev().copied().collect(),
                }
            }
            Seg::Helix {
                frame,
                radius,
                pitch,
                start,
                sweep,
            } => Seg::Helix {
                frame: *frame,
                radius: *radius,
                pitch: *pitch,
                start: start + sweep,
                sweep: -sweep,
            },
        }
    }

    /// The piece mapped by an affine transform (an arc under a non-uniform
    /// one is no longer circular: it becomes an elliptical arc only when
    /// its frame's axes stay at right angles, else a rational B-spline).
    pub fn transformed(&self, m: &Matrix) -> Seg {
        match self {
            Seg::Line { start, end } => Seg::Line {
                start: math::apply(m, *start),
                end: math::apply(m, *end),
            },
            Seg::Arc {
                frame,
                radius,
                start,
                sweep,
            } => conic_transformed(m, frame, *radius, *radius, *start, *sweep),
            Seg::Ellipse {
                frame,
                a,
                b,
                start,
                sweep,
            } => conic_transformed(m, frame, *a, *b, *start, *sweep),
            Seg::Spline {
                degree,
                poles,
                weights,
                knots,
                multiplicities,
            } => Seg::Spline {
                degree: *degree,
                poles: poles.iter().map(|p| math::apply(m, *p)).collect(),
                weights: weights.clone(),
                knots: knots.clone(),
                multiplicities: multiplicities.clone(),
            },
            // A helix keeps its shape under a similarity (scaled by its
            // factor; a mirror turns its hand); other maps keep the frame's
            // first column's scale.
            Seg::Helix {
                frame,
                radius,
                pitch,
                start,
                sweep,
            } => {
                let f = math::mul(m, frame);
                let (x, y, z) = (
                    math::column(&f, 0),
                    math::column(&f, 1),
                    math::column(&f, 2),
                );
                let s = math::length(x);
                let mirrored = math::dot(math::cross(x, y), z) < 0.0;
                let (xa, ya) = (math::scale(x, 1.0 / s), math::scale(y, 1.0 / s));
                let za = math::cross(xa, ya);
                let rise = if mirrored { -pitch } else { *pitch };
                Seg::Helix {
                    frame: math::from_axes(math::origin(&f), xa, ya, za),
                    radius: radius * s,
                    pitch: rise * math::length(z),
                    start: *start,
                    sweep: *sweep,
                }
            }
        }
    }

    /// Points along the piece (both ends and `n - 1` between).
    pub fn sample(&self, n: usize) -> Vec<[f64; 3]> {
        let n = n.max(1);
        match self {
            Seg::Line { start, end } => (0..=n)
                .map(|i| {
                    let t = i as f64 / n as f64;
                    math::add(*start, math::scale(math::sub(*end, *start), t))
                })
                .collect(),
            Seg::Arc {
                frame,
                radius,
                start,
                sweep,
            } => (0..=n)
                .map(|i| on_conic(frame, *radius, *radius, start + sweep * i as f64 / n as f64))
                .collect(),
            Seg::Ellipse {
                frame,
                a,
                b,
                start,
                sweep,
            } => (0..=n)
                .map(|i| on_conic(frame, *a, *b, start + sweep * i as f64 / n as f64))
                .collect(),
            Seg::Spline {
                degree,
                poles,
                weights,
                knots,
                multiplicities,
            } => {
                let full = full_knots(knots, multiplicities);
                let (a, b) = spline_domain(*degree, poles.len(), &full);
                (0..=n)
                    .map(|i| {
                        let t = a + (b - a) * i as f64 / n as f64;
                        de_boor(*degree as usize, poles, weights, &full, t)
                    })
                    .collect()
            }
            Seg::Helix {
                frame,
                radius,
                pitch,
                start,
                sweep,
            } => (0..=n)
                .map(|i| on_helix(frame, *radius, *pitch, start + sweep * i as f64 / n as f64))
                .collect(),
        }
    }

    /// Whether every point lies in the plane z = 0 of `frame`'s inverse
    /// (used for planar paths).
    fn points_for_plane(&self) -> Vec<[f64; 3]> {
        match self {
            Seg::Spline { poles, .. } => poles.clone(),
            other => other.sample(8),
        }
    }
}

fn conic_transformed(m: &Matrix, frame: &Matrix, a: f64, b: f64, start: f64, sweep: f64) -> Seg {
    let f = math::mul(m, frame);
    let x = math::column(&f, 0);
    let y = math::column(&f, 1);
    let (lx, ly) = (math::length(x), math::length(y));
    let at_right_angles = math::dot(x, y).abs() <= 1e-9 * lx * ly;
    if at_right_angles && lx > 0.0 && ly > 0.0 {
        let xa = math::scale(x, 1.0 / lx);
        let ya = math::scale(y, 1.0 / ly);
        let z = math::cross(xa, ya);
        let frame = math::from_axes(math::origin(&f), xa, ya, z);
        let (a, b) = (a * lx, b * ly);
        if (a - b).abs() <= 1e-9 * a.max(b) {
            return Seg::Arc {
                frame,
                radius: a,
                start,
                sweep,
            };
        }
        return Seg::Ellipse {
            frame,
            a,
            b,
            start,
            sweep,
        };
    }
    // A sheared conic: its rational B-spline mapped.
    conic_spline(frame, a, b, start, sweep).transformed(m)
}

/// A conic arc as a rational quadratic B-spline (pieces of at most 90°).
pub fn conic_spline(frame: &Matrix, a: f64, b: f64, start: f64, sweep: f64) -> Seg {
    let pieces = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
    let step = sweep / pieces as f64;
    let w = (step / 2.0).cos();
    let mut poles = Vec::new();
    let mut weights = Vec::new();
    let mut knots = Vec::new();
    let mut multiplicities = Vec::new();
    for i in 0..pieces {
        let t0 = start + step * i as f64;
        let t1 = t0 + step;
        let p0 = on_conic(frame, a, b, t0);
        // The middle pole: where the tangents at the ends meet.
        let tm = (t0 + t1) / 2.0;
        let r = 1.0 / (step / 2.0).cos();
        let pm = math::apply(frame, [a * r * tm.cos(), b * r * tm.sin(), 0.0]);
        if i == 0 {
            poles.push(p0);
            weights.push(1.0);
        }
        poles.push(pm);
        weights.push(w);
        poles.push(on_conic(frame, a, b, t1));
        weights.push(1.0);
        knots.push(i as f64);
        multiplicities.push(if i == 0 { 3 } else { 2 });
    }
    knots.push(pieces as f64);
    multiplicities.push(3);
    Seg::Spline {
        degree: 2,
        poles,
        weights,
        knots,
        multiplicities,
    }
}

/// The full knot vector of distinct knots and multiplicities.
pub fn full_knots(knots: &[f64], multiplicities: &[u32]) -> Vec<f64> {
    knots
        .iter()
        .zip(multiplicities)
        .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
        .collect()
}

/// A point of a (rational) B-spline by de Boor's algorithm.
pub fn de_boor(
    degree: usize,
    poles: &[[f64; 3]],
    weights: &[f64],
    knots: &[f64],
    t: f64,
) -> [f64; 3] {
    let n = poles.len();
    if n == 0 {
        return [0.0; 3];
    }
    let p = degree.min(n - 1);
    // The span: knots[k] <= t < knots[k + 1], within [p, n - 1].
    let mut k = p;
    while k + 1 < n && knots.get(k + 1).is_some_and(|&kn| kn <= t) {
        k += 1;
    }
    let rational = weights.len() == n;
    let mut d: Vec<[f64; 4]> = (0..=p)
        .map(|j| {
            let i = (k + j).saturating_sub(p).min(n - 1);
            let w = if rational { weights[i] } else { 1.0 };
            let q = poles[i];
            [q[0] * w, q[1] * w, q[2] * w, w]
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k + j - p;
            let left = knots.get(i).copied().unwrap_or(0.0);
            let right = knots.get(i + p + 1 - r).copied().unwrap_or(1.0);
            let alpha = if right - left == 0.0 {
                0.0
            } else {
                (t - left) / (right - left)
            };
            let previous = d[j - 1];
            for (c, value) in d[j].iter_mut().enumerate() {
                *value = (1.0 - alpha) * previous[c] + alpha * *value;
            }
        }
    }
    let w = d[p][3];
    if w == 0.0 {
        return [d[p][0], d[p][1], d[p][2]];
    }
    [d[p][0] / w, d[p][1] / w, d[p][2] / w]
}

fn spline_point(
    degree: u32,
    poles: &[[f64; 3]],
    weights: &[f64],
    knots: &[f64],
    multiplicities: &[u32],
    first: bool,
) -> [f64; 3] {
    if knots.is_empty() || poles.is_empty() {
        return poles.first().copied().unwrap_or([0.0; 3]);
    }
    let full = full_knots(knots, multiplicities);
    let (a, b) = spline_domain(degree, poles.len(), &full);
    let clamped = |m: Option<&u32>| m.is_some_and(|m| *m > degree);
    if first {
        if clamped(multiplicities.first()) {
            return poles[0];
        }
        return de_boor(degree as usize, poles, weights, &full, a);
    }
    if clamped(multiplicities.last()) {
        return poles[poles.len() - 1];
    }
    // The end itself lies past the last span: just inside it.
    de_boor(degree as usize, poles, weights, &full, b - (b - a) * 1e-12)
}

/// Where a B-spline is defined: from knot `degree` to knot `poles` of the
/// full vector.
pub fn spline_domain(degree: u32, poles: usize, full: &[f64]) -> (f64, f64) {
    let p = degree as usize;
    let a = full.get(p).or(full.first()).copied().unwrap_or(0.0);
    let b = full.get(poles).or(full.last()).copied().unwrap_or(1.0);
    (a, b)
}

/// A curve: pieces end to end.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Path {
    pub segments: Vec<Seg>,
}

impl Path {
    pub fn start(&self) -> Option<[f64; 3]> {
        self.segments.first().map(Seg::start)
    }

    pub fn end(&self) -> Option<[f64; 3]> {
        self.segments.last().map(Seg::end)
    }

    pub fn reversed(&self) -> Path {
        Path {
            segments: self.segments.iter().rev().map(Seg::reversed).collect(),
        }
    }

    pub fn transformed(&self, m: &Matrix) -> Path {
        Path {
            segments: self.segments.iter().map(|s| s.transformed(m)).collect(),
        }
    }

    /// Whether the path ends where it starts (within `tolerance`).
    pub fn is_closed(&self, tolerance: f64) -> bool {
        match (self.start(), self.end()) {
            (Some(a), Some(b)) => math::length(math::sub(a, b)) <= tolerance,
            _ => false,
        }
    }

    /// The plane the path lies in (a frame whose xy plane holds it, its
    /// origin the path's start), or None when it is not planar within
    /// `tolerance` or a straight line.
    pub fn plane(&self, tolerance: f64) -> Option<Matrix> {
        let points: Vec<[f64; 3]> = self
            .segments
            .iter()
            .flat_map(Seg::points_for_plane)
            .collect();
        let first = *points.first()?;
        // Arcs give their plane at once.
        let normal = self
            .segments
            .iter()
            .find_map(|s| match s {
                Seg::Arc { frame, .. } | Seg::Ellipse { frame, .. } => Some(math::column(frame, 2)),
                _ => None,
            })
            .or_else(|| {
                let far = points.iter().copied().max_by(|a, b| {
                    math::length(math::sub(*a, first))
                        .total_cmp(&math::length(math::sub(*b, first)))
                })?;
                let d = math::normalize(math::sub(far, first))?;
                points.iter().find_map(|p| {
                    let n = math::cross(d, math::sub(*p, first));
                    (math::length(n) > tolerance * 10.0).then(|| math::normalize(n))?
                })
            })?;
        if points
            .iter()
            .any(|p| math::dot(math::sub(*p, first), normal).abs() > tolerance)
        {
            return None;
        }
        Some(math::frame(first, Some(normal), None))
    }
}

/// A closed area of a profile: the outer loop and its holes (loops of
/// pieces in the profile's xy plane, z = 0).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Area {
    pub outer: Path,
    pub holes: Vec<Path>,
}

/// A profile: one or more areas (composite profiles have several).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Profile {
    /// The profile entity's type (`IFCRECTANGLEPROFILEDEF`).
    pub kind: String,
    pub name: String,
    pub areas: Vec<Area>,
}

impl Profile {
    pub fn transformed(&self, m: &Matrix) -> Profile {
        Profile {
            kind: self.kind.clone(),
            name: self.name.clone(),
            areas: self
                .areas
                .iter()
                .map(|a| Area {
                    outer: a.outer.transformed(m),
                    holes: a.holes.iter().map(|h| h.transformed(m)).collect(),
                })
                .collect(),
        }
    }
}

/// Boolean operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

/// A surface of an advanced face.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Surface {
    /// The frame's xy plane.
    Plane {
        frame: Matrix,
    },
    /// About the frame's z axis.
    Cylinder {
        frame: Matrix,
        radius: f64,
    },
    /// Radius at the frame's origin, widening at `semi_angle`.
    Cone {
        frame: Matrix,
        radius: f64,
        semi_angle: f64,
    },
    Sphere {
        frame: Matrix,
        radius: f64,
    },
    Torus {
        frame: Matrix,
        major: f64,
        minor: f64,
    },
    /// Poles `[u][v]`; empty weights when not rational.
    BSpline {
        u_degree: u32,
        v_degree: u32,
        poles: Vec<Vec<[f64; 3]>>,
        weights: Vec<Vec<f64>>,
        u_knots: Vec<f64>,
        u_multiplicities: Vec<u32>,
        v_knots: Vec<f64>,
        v_multiplicities: Vec<u32>,
    },
    /// A curve swept along a direction (both in model coordinates).
    Extrusion {
        curve: Path,
        direction: [f64; 3],
    },
    /// A curve turned about an axis.
    Revolution {
        curve: Path,
        origin: [f64; 3],
        axis: [f64; 3],
    },
}

/// An edge of a face bound, in the bound's order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edge {
    /// The IfcEdgeCurve (0 for an edge of a poly loop): faces that share an
    /// edge name the same.
    pub id: u64,
    /// The IfcVertexPoints at `start` and `end` (0 for a poly loop's).
    pub vertices: [u64; 2],
    /// The edge's curve as the file gives it.
    pub curve: Path,
    /// The edge's vertices in the bound's order.
    pub start: [f64; 3],
    pub end: [f64; 3],
    /// Whether the curve's own direction runs from `start` to `end`.
    pub forward: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bound {
    pub outer: bool,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AdvancedFace {
    pub surface: Surface,
    pub bounds: Vec<Bound>,
    /// Whether the face's normal is the surface's.
    pub same_sense: bool,
}

/// A decoded representation item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    /// The profile (in the position's xy plane) swept along `direction`
    /// (a unit vector in the position's coordinates) by `depth`.
    Extrusion {
        position: Matrix,
        profile: Profile,
        direction: [f64; 3],
        depth: f64,
    },
    /// A profile extruded into another (IfcExtrudedAreaSolidTapered): the
    /// start profile in the position's xy plane, the end profile in the
    /// plane `depth` along `direction`.
    TaperedExtrusion {
        position: Matrix,
        profile: Profile,
        end_profile: Profile,
        direction: [f64; 3],
        depth: f64,
    },
    /// The profile turned about the axis (in the position's coordinates)
    /// by `angle`.
    Revolution {
        position: Matrix,
        profile: Profile,
        axis_origin: [f64; 3],
        axis: [f64; 3],
        angle: f64,
    },
    /// The profile swept along a curve (IfcSurfaceCurveSweptAreaSolid,
    /// IfcFixedReferenceSweptAreaSolid): the profile's plane at right
    /// angles to the curve at its start, its x axis along `reference` (the
    /// reference surface's normal, or the fixed reference); the curve and
    /// the reference in item coordinates, the profile in the position's.
    Sweep {
        position: Matrix,
        profile: Profile,
        directrix: Path,
        reference: Option<[f64; 3]>,
        /// A fixed binormal direction the profile keeps its angles to along
        /// the curve (mitcad#397: the `.rvt` import's sloped elliptical
        /// rail pieces, their profile level all along); none from IFC.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        binormal: Option<[f64; 3]>,
    },
    /// A circular section swept along the directrix.
    SweptDisk {
        directrix: Path,
        radius: f64,
        inner_radius: Option<f64>,
    },
    Boolean {
        op: BoolOp,
        first: Box<Item>,
        second: Box<Item>,
    },
    /// The half of space on one side of the plane `plane` (its xy plane):
    /// below it (against its z axis) when `below`. With a boundary, only
    /// within the prism of the polygon (`boundary_frame`'s xy plane, along
    /// its z axis).
    HalfSpace {
        plane: Matrix,
        below: bool,
        boundary: Option<(Matrix, Path)>,
    },
    Block {
        position: Matrix,
        size: [f64; 3],
    },
    Cylinder {
        position: Matrix,
        radius: f64,
        height: f64,
    },
    Cone {
        position: Matrix,
        radius: f64,
        height: f64,
    },
    Sphere {
        position: Matrix,
        radius: f64,
    },
    Pyramid {
        position: Matrix,
        size: [f64; 3],
    },
    /// Planar faces: each face's loops (the outer first), each loop's
    /// points; `closed` when the faces bound a solid.
    Faceted {
        faces: Vec<Vec<Vec<[f64; 3]>>>,
        closed: bool,
    },
    /// Triangles (counter-clockwise seen from outside).
    Mesh {
        points: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
        closed: bool,
    },
    Advanced {
        faces: Vec<AdvancedFace>,
        closed: bool,
    },
    /// The items of representation map `map` placed by `transform` (which
    /// may scale and mirror).
    Mapped {
        map: u64,
        transform: Matrix,
    },
    Unsupported {
        id: u64,
        kind: String,
        why: String,
    },
}

impl Item {
    /// A short name of the item's kind for reports.
    pub fn name(&self) -> &'static str {
        match self {
            Item::Extrusion { .. } => "extrusion",
            Item::TaperedExtrusion { .. } => "tapered extrusion",
            Item::Revolution { .. } => "revolution",
            Item::Sweep { .. } => "sweep",
            Item::SweptDisk { .. } => "swept disk",
            Item::Boolean { .. } => "boolean",
            Item::HalfSpace { .. } => "half space",
            Item::Block { .. } => "block",
            Item::Cylinder { .. } => "cylinder",
            Item::Cone { .. } => "cone",
            Item::Sphere { .. } => "sphere",
            Item::Pyramid { .. } => "pyramid",
            Item::Faceted { .. } => "faceted brep",
            Item::Mesh { .. } => "face set",
            Item::Advanced { .. } => "advanced brep",
            Item::Mapped { .. } => "mapped item",
            Item::Unsupported { .. } => "unsupported",
        }
    }

    /// The unsupported items inside (booleans' operands too).
    pub fn unsupported(&self) -> Vec<(u64, String, String)> {
        match self {
            Item::Unsupported { id, kind, why } => vec![(*id, kind.clone(), why.clone())],
            Item::Boolean { first, second, .. } => {
                let mut out = first.unsupported();
                out.extend(second.unsupported());
                out
            }
            _ => Vec::new(),
        }
    }
}

/// Decodes geometry entities of a file with its units.
pub struct Decoder<'a> {
    pub file: &'a IfcFile,
    pub units: &'a Units,
    /// The sub-items, curves and profiles a top-level call decoded
    /// (mitcad#930).
    memo: RefCell<Memo>,
}

type Decoded<T> = Result<T, String>;

const MAX_DEPTH: usize = 64;

/// How much one top-level call ([`Decoder::item`], [`Decoder::curve`],
/// [`Decoder::profile`]) may copy from results it decoded before: the
/// size of every result used a third time or more (roughly its
/// segments, points and nodes). A file's items share little (a profile
/// used by both ends of a tapered extrusion is decoded twice, not
/// copied); a hostile chain of items that each use the one below twice
/// would double the result at every level (mitcad#930).
pub const MAX_SHARED_SIZE: usize = 100_000;

/// Why a top-level call stopped at [`MAX_SHARED_SIZE`].
pub const TOO_MANY_ITEMS: &str = "too many nested items";

/// A result in the memo: seen once (decoded, not kept), or kept with its
/// size after its second use.
enum Cached<T> {
    Seen,
    Kept(T, usize),
}

type Table<T> = HashMap<u64, Cached<T>>;

/// The results of one top-level call, by entity: each is decoded at most
/// twice, then copied.
#[derive(Default)]
struct Memo {
    /// Top-level calls under way (nested ones share the memo).
    active: usize,
    /// The size copied from kept results so far.
    shared: usize,
    /// Whether `shared` passed [`MAX_SHARED_SIZE`]: everything left
    /// fails at once.
    exceeded: bool,
    items: Table<Item>,
    curves: Table<Decoded<Path>>,
    profiles: Table<Decoded<Profile>>,
}

impl Memo {
    fn items(&mut self) -> &mut Table<Item> {
        &mut self.items
    }

    fn curves(&mut self) -> &mut Table<Decoded<Path>> {
        &mut self.curves
    }

    fn profiles(&mut self) -> &mut Table<Decoded<Profile>> {
        &mut self.profiles
    }
}

/// How much copying a decoded result costs, roughly in segments, points
/// and nodes.
trait Size {
    fn size(&self) -> usize;
}

impl Size for Seg {
    fn size(&self) -> usize {
        match self {
            Seg::Spline { poles, .. } => 1 + poles.len(),
            _ => 1,
        }
    }
}

impl Size for Path {
    fn size(&self) -> usize {
        1 + self.segments.iter().map(Size::size).sum::<usize>()
    }
}

impl Size for Profile {
    fn size(&self) -> usize {
        1 + self
            .areas
            .iter()
            .map(|a| a.outer.size() + a.holes.iter().map(Size::size).sum::<usize>())
            .sum::<usize>()
    }
}

impl Size for Surface {
    fn size(&self) -> usize {
        match self {
            Surface::BSpline { poles, .. } => 1 + poles.iter().map(Vec::len).sum::<usize>(),
            Surface::Extrusion { curve, .. } => 1 + curve.size(),
            _ => 1,
        }
    }
}

impl Size for Item {
    fn size(&self) -> usize {
        let inner = match self {
            Item::Extrusion { profile, .. } | Item::Revolution { profile, .. } => profile.size(),
            Item::TaperedExtrusion {
                profile,
                end_profile,
                ..
            } => profile.size() + end_profile.size(),
            Item::Sweep {
                profile, directrix, ..
            } => profile.size() + directrix.size(),
            Item::SweptDisk { directrix, .. } => directrix.size(),
            Item::Boolean { first, second, .. } => first.size().saturating_add(second.size()),
            Item::HalfSpace { boundary, .. } => boundary.as_ref().map_or(0, |(_, p)| p.size()),
            Item::Faceted { faces, .. } => faces.iter().flatten().map(Vec::len).sum::<usize>(),
            Item::Mesh {
                points, triangles, ..
            } => points.len() + triangles.len(),
            Item::Advanced { faces, .. } => faces
                .iter()
                .map(|f| {
                    f.surface.size()
                        + f.bounds
                            .iter()
                            .flat_map(|b| &b.edges)
                            .map(|e| e.curve.size())
                            .sum::<usize>()
                })
                .sum::<usize>(),
            _ => 0,
        };
        inner.saturating_add(1)
    }
}

impl<T: Size> Size for Decoded<T> {
    fn size(&self) -> usize {
        self.as_ref().map_or(1, Size::size)
    }
}

impl<'a> Decoder<'a> {
    pub fn new(file: &'a IfcFile, units: &'a Units) -> Self {
        Self {
            file,
            units,
            memo: RefCell::new(Memo::default()),
        }
    }

    /// Runs a top-level call: an outermost one starts with an empty memo
    /// and fails as a whole (`too_many`) when it copied more than
    /// [`MAX_SHARED_SIZE`]; a nested one shares its caller's memo.
    fn top<T>(&self, too_many: impl FnOnce() -> T, decode: impl FnOnce() -> T) -> T {
        let outermost = {
            let mut memo = self.memo.borrow_mut();
            memo.active += 1;
            memo.active == 1
        };
        let value = decode();
        let exceeded = {
            let mut memo = self.memo.borrow_mut();
            memo.active -= 1;
            if !outermost {
                return value;
            }
            let exceeded = memo.exceeded;
            *memo = Memo::default();
            exceeded
        };
        if exceeded { too_many() } else { value }
    }

    /// The result for entity `id` from the memo, or decoded: the first
    /// use decodes, the second decodes again and keeps the result, later
    /// ones copy it (and count its size towards [`MAX_SHARED_SIZE`]). A
    /// result is kept whatever depth it was decoded at: only an entity
    /// reached both near and beyond [`MAX_DEPTH`] could decode otherwise.
    fn memoised<T: Clone + Size>(
        &self,
        id: u64,
        table: fn(&mut Memo) -> &mut Table<T>,
        too_many: impl FnOnce() -> T,
        decode: impl FnOnce() -> T,
    ) -> T {
        let keep = {
            let mut memo = self.memo.borrow_mut();
            if memo.exceeded {
                return too_many();
            }
            let size = match table(&mut memo).get(&id) {
                None => None,
                Some(Cached::Seen) => Some(None),
                Some(Cached::Kept(_, size)) => Some(Some(*size)),
            };
            match size {
                None => {
                    table(&mut memo).insert(id, Cached::Seen);
                    false
                }
                Some(None) => true,
                Some(Some(size)) => {
                    memo.shared = memo.shared.saturating_add(size.max(1));
                    if memo.shared > MAX_SHARED_SIZE {
                        memo.exceeded = true;
                        return too_many();
                    }
                    if let Some(Cached::Kept(value, _)) = table(&mut memo).get(&id) {
                        return value.clone();
                    }
                    false
                }
            }
        };
        let value = decode();
        if keep {
            let size = value.size();
            let mut memo = self.memo.borrow_mut();
            table(&mut memo).insert(id, Cached::Kept(value.clone(), size));
        }
        value
    }

    fn entity(&self, value: &Value) -> Decoded<&'a Entity> {
        match value.as_ref() {
            Some(id) => self
                .file
                .get(id)
                .ok_or_else(|| format!("#{id} is not in the file")),
            None => Err(format!("a reference expected, {value} found")),
        }
    }

    fn length(&self, value: &Value, what: &str) -> Decoded<f64> {
        value
            .as_f64()
            .map(|v| v * self.units.length)
            .ok_or_else(|| format!("no {what}"))
    }

    fn opt_length(&self, value: &Value) -> Option<f64> {
        value.as_f64().map(|v| v * self.units.length)
    }

    /// An IfcCartesianPoint (2D points get z = 0), in mm.
    pub fn point(&self, value: &Value) -> Decoded<[f64; 3]> {
        let e = self.entity(value)?;
        if !e.is("IFCCARTESIANPOINT") {
            return Err(format!("#{} is {}, not a point", e.id, e.kind));
        }
        let c = e
            .arg(0)
            .numbers()
            .ok_or_else(|| format!("#{}: no coordinates", e.id))?;
        let s = self.units.length;
        Ok([
            c.first().copied().unwrap_or(0.0) * s,
            c.get(1).copied().unwrap_or(0.0) * s,
            c.get(2).copied().unwrap_or(0.0) * s,
        ])
    }

    /// An IfcDirection (not normalized; 2D ones get z = 0).
    pub fn direction(&self, value: &Value) -> Decoded<[f64; 3]> {
        let e = self.entity(value)?;
        let c = e
            .arg(0)
            .numbers()
            .ok_or_else(|| format!("#{}: no direction ratios", e.id))?;
        Ok([
            c.first().copied().unwrap_or(0.0),
            c.get(1).copied().unwrap_or(0.0),
            c.get(2).copied().unwrap_or(0.0),
        ])
    }

    fn opt_direction(&self, value: &Value) -> Decoded<Option<[f64; 3]>> {
        if value.is_null() {
            Ok(None)
        } else {
            self.direction(value).map(Some)
        }
    }

    /// An IfcAxis2Placement3D or 2D (identity when `$`).
    pub fn placement(&self, value: &Value) -> Decoded<Matrix> {
        if value.is_null() {
            return Ok(math::IDENTITY);
        }
        let e = self.entity(value)?;
        if e.is("IFCAXIS2PLACEMENT3D") {
            let origin = self.point(e.arg(0))?;
            let z = self.opt_direction(e.arg(1))?;
            let x = self.opt_direction(e.arg(2))?;
            Ok(math::frame(origin, z, x))
        } else if e.is("IFCAXIS2PLACEMENT2D") {
            let origin = self.point(e.arg(0))?;
            let x = self.opt_direction(e.arg(1))?;
            Ok(math::frame(origin, None, x))
        } else {
            Err(format!("#{} is {}, not an axis placement", e.id, e.kind))
        }
    }

    /// An IfcAxis1Placement: its location and direction.
    fn axis1(&self, value: &Value) -> Decoded<([f64; 3], [f64; 3])> {
        let e = self.entity(value)?;
        let origin = self.point(e.arg(0))?;
        let axis = self.opt_direction(e.arg(1))?.unwrap_or([0.0, 0.0, 1.0]);
        Ok((
            origin,
            math::normalize(axis).ok_or_else(|| format!("#{}: a zero axis", e.id))?,
        ))
    }

    /// An IfcCartesianTransformationOperator (2D or 3D, uniform or not):
    /// the transform, which may scale and mirror.
    pub fn operator(&self, value: &Value) -> Decoded<Matrix> {
        let e = self.entity(value)?;
        let three = e.kind.contains("3D");
        let origin = self.point(e.arg(2))?;
        let scale = e.arg(3).as_f64().unwrap_or(1.0);
        let (scale2, scale3) = if e.kind.ends_with("NONUNIFORM") {
            if three {
                (
                    e.arg(5).as_f64().unwrap_or(scale),
                    e.arg(6).as_f64().unwrap_or(scale),
                )
            } else {
                (e.arg(4).as_f64().unwrap_or(scale), scale)
            }
        } else {
            (scale, scale)
        };
        let axis1 = self.opt_direction(e.arg(0))?;
        let axis2 = self.opt_direction(e.arg(1))?;
        let axis3 = if three {
            self.opt_direction(e.arg(4))?
        } else {
            None
        };
        let d3 = axis3.and_then(math::normalize).unwrap_or([0.0, 0.0, 1.0]);
        let along = |v: [f64; 3]| math::normalize(math::sub(v, math::scale(d3, math::dot(v, d3))));
        let d1 = axis1
            .and_then(along)
            .or_else(|| along([1.0, 0.0, 0.0]))
            .or_else(|| along([0.0, 1.0, 0.0]))
            .unwrap_or([1.0, 0.0, 0.0]);
        let complement = math::cross(d3, d1);
        let d2 = match axis2 {
            Some(v) => {
                let v = math::sub(v, math::scale(d3, math::dot(v, d3)));
                let v = math::sub(v, math::scale(d1, math::dot(v, d1)));
                math::normalize(v).unwrap_or(complement)
            }
            None => complement,
        };
        Ok(math::from_axes(
            origin,
            math::scale(d1, scale),
            math::scale(d2, scale2),
            math::scale(d3, scale3),
        ))
    }

    // Curves.

    /// A curve as a path.
    pub fn curve(&self, value: &Value) -> Decoded<Path> {
        self.top(
            || Err(TOO_MANY_ITEMS.to_owned()),
            || self.curve_at(value, 0),
        )
    }

    /// A curve, with the depth of nesting; each curve entity is decoded at
    /// most twice per top-level call.
    fn curve_at(&self, value: &Value, depth: usize) -> Decoded<Path> {
        if depth > MAX_DEPTH {
            return Err("curves nested too deeply".to_owned());
        }
        let e = self.entity(value)?;
        self.memoised(
            e.id,
            Memo::curves,
            || Err(TOO_MANY_ITEMS.to_owned()),
            || self.curve_of(e, depth),
        )
    }

    fn curve_of(&self, e: &Entity, depth: usize) -> Decoded<Path> {
        let kind = &*e.kind;
        let segments = match kind {
            "IFCPOLYLINE" => {
                let points: Vec<[f64; 3]> = e
                    .arg(0)
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| self.point(p))
                    .collect::<Decoded<_>>()?;
                polyline(&points)
            }
            "IFCINDEXEDPOLYCURVE" => self.indexed_poly_curve(e)?,
            "IFCCOMPOSITECURVE" | "IFCCOMPOSITECURVEONSURFACE" => {
                let mut out = Vec::new();
                for s in e.arg(0).refs() {
                    let segment = self
                        .file
                        .get(s)
                        .ok_or_else(|| format!("#{s} is not in the file"))?;
                    if !segment.is("IFCCOMPOSITECURVESEGMENT")
                        && !segment.is("IFCREPARAMETRISEDCOMPOSITECURVESEGMENT")
                    {
                        return Err(format!(
                            "#{}: a {} in a composite curve is not read",
                            segment.id, segment.kind
                        ));
                    }
                    // (Transition, SameSense, ParentCurve)
                    let mut path = self.curve_at(segment.arg(2), depth + 1)?;
                    if segment.arg(1).as_bool() == Some(false) {
                        path = path.reversed();
                    }
                    out.extend(path.segments);
                }
                out
            }
            "IFCTRIMMEDCURVE" => self.trimmed(e, depth)?,
            "IFCCIRCLE" => {
                let frame = self.placement(e.arg(0))?;
                let radius = self.length(e.arg(1), "radius")?;
                vec![Seg::Arc {
                    frame,
                    radius,
                    start: 0.0,
                    sweep: TAU,
                }]
            }
            "IFCELLIPSE" => {
                let frame = self.placement(e.arg(0))?;
                let a = self.length(e.arg(1), "semi axis")?;
                let b = self.length(e.arg(2), "semi axis")?;
                vec![Seg::Ellipse {
                    frame,
                    a,
                    b,
                    start: 0.0,
                    sweep: TAU,
                }]
            }
            "IFCBSPLINECURVEWITHKNOTS" | "IFCRATIONALBSPLINECURVEWITHKNOTS" => {
                vec![self.bspline_curve(e)?]
            }
            "IFCSURFACECURVE" | "IFCINTERSECTIONCURVE" | "IFCSEAMCURVE" => {
                // (Curve3D, AssociatedGeometry, MasterRepresentation)
                return self.curve_at(e.arg(0), depth + 1);
            }
            "IFCOFFSETCURVE2D" | "IFCOFFSETCURVE3D" | "IFCLINE" => {
                return Err(format!("#{}: an unbounded or offset {kind}", e.id));
            }
            _ => return Err(format!("#{}: curves of type {kind} are not read", e.id)),
        };
        Ok(Path { segments })
    }

    fn indexed_poly_curve(&self, e: &Entity) -> Decoded<Vec<Seg>> {
        // (Points, Segments, SelfIntersect); Points: an IfcCartesianPointList.
        let list = self.entity(e.arg(0))?;
        let s = self.units.length;
        let points: Vec<[f64; 3]> = list
            .arg(0)
            .as_list()
            .unwrap_or_default()
            .iter()
            .map(|p| {
                let c = p.numbers().unwrap_or_default();
                [
                    c.first().copied().unwrap_or(0.0) * s,
                    c.get(1).copied().unwrap_or(0.0) * s,
                    c.get(2).copied().unwrap_or(0.0) * s,
                ]
            })
            .collect();
        let at = |i: i64| -> Decoded<[f64; 3]> {
            one_based(i, points.len())
                .map(|k| points[k])
                .ok_or_else(|| format!("#{}: point index {i} out of range", e.id))
        };
        let Some(segments) = e.arg(1).as_list() else {
            return Ok(polyline(&points));
        };
        let mut out = Vec::new();
        for segment in segments {
            let (kind, inner) = match segment {
                Value::Typed(kind, inner) => (kind.as_str(), inner.as_ref()),
                _ => return Err(format!("#{}: a segment without its type", e.id)),
            };
            let indices: Vec<i64> = inner
                .as_list()
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_i64)
                .collect();
            match kind {
                "IFCLINEINDEX" => {
                    let pts: Vec<[f64; 3]> =
                        indices.iter().map(|&i| at(i)).collect::<Decoded<_>>()?;
                    out.extend(polyline(&pts));
                }
                "IFCARCINDEX" if indices.len() == 3 => {
                    out.push(arc3(at(indices[0])?, at(indices[1])?, at(indices[2])?));
                }
                _ => return Err(format!("#{}: a segment {kind} is not read", e.id)),
            }
        }
        Ok(out)
    }

    fn trimmed(&self, e: &Entity, depth: usize) -> Decoded<Vec<Seg>> {
        // (BasisCurve, Trim1, Trim2, SenseAgreement, MasterRepresentation)
        let basis = self.entity(e.arg(0))?;
        let sense = e.arg(3).as_bool().unwrap_or(true);
        let prefer_points = e.arg(4).as_enum() != Some("PARAMETER");
        let trim = |value: &Value| -> (Option<[f64; 3]>, Option<f64>) {
            let mut point = None;
            let mut parameter = None;
            for v in value.as_list().unwrap_or_default() {
                match v {
                    Value::Typed(_, inner) => parameter = inner.as_f64(),
                    Value::Ref(_) => point = self.point(v).ok(),
                    other => parameter = parameter.or(other.as_f64()),
                }
            }
            (point, parameter)
        };
        let (p1, t1) = trim(e.arg(1));
        let (p2, t2) = trim(e.arg(2));
        match &*basis.kind {
            "IFCLINE" => {
                // (Pnt, Dir: IfcVector(Orientation, Magnitude))
                let origin = self.point(basis.arg(0))?;
                let vector = self.entity(basis.arg(1))?;
                let dir = self.direction(vector.arg(0))?;
                let dir = math::normalize(dir).ok_or("a zero line direction")?;
                let magnitude = self.length(vector.arg(1), "magnitude")?;
                let at = |p: Option<[f64; 3]>, t: Option<f64>| -> Decoded<[f64; 3]> {
                    match (p, t) {
                        (Some(p), _) if prefer_points || t.is_none() => Ok(p),
                        (_, Some(t)) => Ok(math::add(origin, math::scale(dir, t * magnitude))),
                        (Some(p), None) => Ok(p),
                        (None, None) => Err(format!("#{}: a trim without a point", e.id)),
                    }
                };
                let (a, b) = (at(p1, t1)?, at(p2, t2)?);
                // A line runs from trim 1 to trim 2 either way.
                Ok(vec![Seg::Line { start: a, end: b }])
            }
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let frame = self.placement(basis.arg(0))?;
                let a = self.length(basis.arg(1), "radius")?;
                let b = if &*basis.kind == "IFCELLIPSE" {
                    self.length(basis.arg(2), "semi axis")?
                } else {
                    a
                };
                let inverse = math::inverse(&frame).ok_or("a degenerate circle")?;
                let angle_of = |p: [f64; 3]| {
                    let l = math::apply(&inverse, p);
                    (l[1] / b).atan2(l[0] / a)
                };
                let angle = |p: Option<[f64; 3]>, t: Option<f64>| -> Decoded<f64> {
                    match (p, t) {
                        (Some(p), _) if prefer_points || t.is_none() => Ok(angle_of(p)),
                        (_, Some(t)) => Ok(t * self.units.angle),
                        (Some(p), None) => Ok(angle_of(p)),
                        (None, None) => Err(format!("#{}: a trim without a point", e.id)),
                    }
                };
                let (u1, u2) = (angle(p1, t1)?, angle(p2, t2)?);
                // Counter-clockwise from trim 1 to trim 2 when the sense
                // agrees, else clockwise.
                let sweep = if sense {
                    let mut s = (u2 - u1).rem_euclid(TAU);
                    if s < 1e-12 {
                        s = TAU;
                    }
                    s
                } else {
                    let mut s = (u1 - u2).rem_euclid(TAU);
                    if s < 1e-12 {
                        s = TAU;
                    }
                    -s
                };
                Ok(vec![if &*basis.kind == "IFCCIRCLE" {
                    Seg::Arc {
                        frame,
                        radius: a,
                        start: u1,
                        sweep,
                    }
                } else {
                    Seg::Ellipse {
                        frame,
                        a,
                        b,
                        start: u1,
                        sweep,
                    }
                }])
            }
            _ => {
                // Another curve trimmed at its ends' points (B-splines
                // usually run from end to end already).
                let path = self.curve_at(e.arg(0), depth + 1)?;
                if p1.is_some() || p2.is_some() {
                    let path = if sense { path } else { path.reversed() };
                    Ok(path.segments)
                } else {
                    Err(format!(
                        "#{}: a {} trimmed by parameters is not read",
                        e.id, basis.kind
                    ))
                }
            }
        }
    }

    fn bspline_curve(&self, e: &Entity) -> Decoded<Seg> {
        // (Degree, ControlPointsList, CurveForm, ClosedCurve, SelfIntersect,
        // KnotMultiplicities, Knots, KnotSpec[, WeightsData])
        let degree = as_count(e.arg(0).as_i64().ok_or("no degree")?);
        let poles: Vec<[f64; 3]> = e
            .arg(1)
            .as_list()
            .unwrap_or_default()
            .iter()
            .map(|p| self.point(p))
            .collect::<Decoded<_>>()?;
        let multiplicities: Vec<u32> = e
            .arg(5)
            .as_list()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_i64)
            .map(as_count)
            .collect();
        let knots = e.arg(6).numbers().unwrap_or_default();
        let weights = if e.is("IFCRATIONALBSPLINECURVEWITHKNOTS") {
            e.arg(8).numbers().unwrap_or_default()
        } else {
            Vec::new()
        };
        check_spline(e.id, degree, poles.len(), &knots, &multiplicities)?;
        Ok(Seg::Spline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
        })
    }

    // Profiles.

    pub fn profile(&self, value: &Value) -> Decoded<Profile> {
        self.top(
            || Err(TOO_MANY_ITEMS.to_owned()),
            || self.profile_at(value, 0),
        )
    }

    /// A profile, with the depth of nesting; each profile entity is
    /// decoded at most twice per top-level call.
    fn profile_at(&self, value: &Value, depth: usize) -> Decoded<Profile> {
        if depth > MAX_DEPTH {
            return Err("profiles nested too deeply".to_owned());
        }
        let e = self.entity(value)?;
        self.memoised(
            e.id,
            Memo::profiles,
            || Err(TOO_MANY_ITEMS.to_owned()),
            || self.profile_of(e, depth),
        )
    }

    fn profile_of(&self, e: &Entity, depth: usize) -> Decoded<Profile> {
        let kind = e.kind.to_string();
        let name = e.arg(1).as_str().unwrap_or_default().to_owned();
        if e.arg(0).as_enum() == Some("CURVE") && !e.is("IFCARBITRARYCLOSEDPROFILEDEF") {
            return Err(format!(
                "#{}: a curve profile ({kind}) bounds no area",
                e.id
            ));
        }
        let made = |areas: Vec<Area>| Profile {
            kind: kind.clone(),
            name: name.clone(),
            areas,
        };
        match &*e.kind {
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let outer = self.closed(e.arg(2), e.id)?;
                let mut holes = Vec::new();
                if e.is("IFCARBITRARYPROFILEDEFWITHVOIDS") {
                    for h in e.arg(3).as_list().unwrap_or_default() {
                        holes.push(self.closed(h, e.id)?);
                    }
                }
                return Ok(made(vec![Area { outer, holes }]));
            }
            "IFCDERIVEDPROFILEDEF" => {
                // (ProfileType, ProfileName, ParentProfile, Operator, Label)
                let parent = self.profile_at(e.arg(2), depth + 1)?;
                let operator = self.operator(e.arg(3))?;
                let mut p = parent.transformed(&operator);
                if math::determinant(&operator) < 0.0 {
                    for a in &mut p.areas {
                        a.outer = a.outer.reversed();
                        for h in &mut a.holes {
                            *h = h.reversed();
                        }
                    }
                }
                p.kind = kind;
                p.name = name;
                return Ok(p);
            }
            "IFCCOMPOSITEPROFILEDEF" => {
                let mut areas = Vec::new();
                for p in e.arg(2).as_list().unwrap_or_default() {
                    areas.extend(self.profile_at(p, depth + 1)?.areas);
                }
                return Ok(made(areas));
            }
            _ => {}
        }
        // Parameterized profiles: (ProfileType, ProfileName, Position, ...).
        let position = self.placement(e.arg(2))?;
        let l = |i: usize, what: &str| self.length(e.arg(i), what);
        let o = |i: usize| self.opt_length(e.arg(i)).unwrap_or(0.0);
        let areas = match &*e.kind {
            "IFCRECTANGLEPROFILEDEF" => {
                let (x, y) = (l(3, "XDim")? / 2.0, l(4, "YDim")? / 2.0);
                vec![Area {
                    outer: rounded(&rect(x, y), &[0.0; 4]),
                    holes: Vec::new(),
                }]
            }
            "IFCROUNDEDRECTANGLEPROFILEDEF" => {
                let (x, y) = (l(3, "XDim")? / 2.0, l(4, "YDim")? / 2.0);
                let r = o(5);
                vec![Area {
                    outer: rounded(&rect(x, y), &[r; 4]),
                    holes: Vec::new(),
                }]
            }
            "IFCRECTANGLEHOLLOWPROFILEDEF" => {
                // (..., XDim, YDim, WallThickness, InnerFilletRadius,
                // OuterFilletRadius)
                let (x, y) = (l(3, "XDim")? / 2.0, l(4, "YDim")? / 2.0);
                let t = l(5, "wall thickness")?;
                let (inner, outer) = (o(6), o(7));
                vec![Area {
                    outer: rounded(&rect(x, y), &[outer; 4]),
                    holes: vec![rounded(&rect(x - t, y - t), &[inner; 4]).reversed()],
                }]
            }
            "IFCCIRCLEPROFILEDEF" => {
                let r = l(3, "radius")?;
                vec![Area {
                    outer: circle(r),
                    holes: Vec::new(),
                }]
            }
            "IFCCIRCLEHOLLOWPROFILEDEF" => {
                let r = l(3, "radius")?;
                let t = l(4, "wall thickness")?;
                vec![Area {
                    outer: circle(r),
                    holes: vec![circle(r - t).reversed()],
                }]
            }
            "IFCELLIPSEPROFILEDEF" => {
                let (a, b) = (l(3, "semi axis")?, l(4, "semi axis")?);
                vec![Area {
                    outer: Path {
                        segments: vec![Seg::Ellipse {
                            frame: math::IDENTITY,
                            a,
                            b,
                            start: 0.0,
                            sweep: TAU,
                        }],
                    },
                    holes: Vec::new(),
                }]
            }
            "IFCISHAPEPROFILEDEF" => {
                // (..., OverallWidth, OverallDepth, WebThickness,
                // FlangeThickness, FilletRadius[, FlangeEdgeRadius,
                // FlangeSlope (IFC4)])
                let b = l(3, "width")? / 2.0;
                let h = l(4, "depth")? / 2.0;
                let w = l(5, "web thickness")? / 2.0;
                let tf = l(6, "flange thickness")?;
                let (r, e_r) = (o(7), o(8));
                let points = [
                    [-b, -h],
                    [b, -h],
                    [b, -h + tf],
                    [w, -h + tf],
                    [w, h - tf],
                    [b, h - tf],
                    [b, h],
                    [-b, h],
                    [-b, h - tf],
                    [-w, h - tf],
                    [-w, -h + tf],
                    [-b, -h + tf],
                ];
                let radii = [0.0, 0.0, e_r, r, r, e_r, 0.0, 0.0, e_r, r, r, e_r];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCASYMMETRICISHAPEPROFILEDEF" => {
                // (..., BottomFlangeWidth, OverallDepth, WebThickness,
                // BottomFlangeThickness, BottomFlangeFilletRadius,
                // TopFlangeWidth, TopFlangeThickness, TopFlangeFilletRadius,
                // ...)
                let bb = l(3, "bottom flange width")? / 2.0;
                let h = l(4, "depth")? / 2.0;
                let w = l(5, "web thickness")? / 2.0;
                let tb = l(6, "bottom flange thickness")?;
                let rb = o(7);
                let bt = l(8, "top flange width")? / 2.0;
                let tt = self.opt_length(e.arg(9)).unwrap_or(tb);
                let rt = o(10);
                let points = [
                    [-bb, -h],
                    [bb, -h],
                    [bb, -h + tb],
                    [w, -h + tb],
                    [w, h - tt],
                    [bt, h - tt],
                    [bt, h],
                    [-bt, h],
                    [-bt, h - tt],
                    [-w, h - tt],
                    [-w, -h + tb],
                    [-bb, -h + tb],
                ];
                let radii = [0.0, 0.0, 0.0, rb, rt, 0.0, 0.0, 0.0, 0.0, rt, rb, 0.0];
                vec![Area {
                    outer: centred(rounded(&points, &radii)),
                    holes: Vec::new(),
                }]
            }
            "IFCLSHAPEPROFILEDEF" => {
                // (..., Depth, Width, Thickness, FilletRadius, EdgeRadius,
                // LegSlope, ...)
                let d = l(3, "depth")?;
                let wd = self.opt_length(e.arg(4)).unwrap_or(d);
                let t = l(5, "thickness")?;
                let (r, er) = (o(6), o(7));
                let (x, y) = (wd / 2.0, d / 2.0);
                let points = [
                    [-x, -y],
                    [x, -y],
                    [x, -y + t],
                    [-x + t, -y + t],
                    [-x + t, y],
                    [-x, y],
                ];
                let radii = [0.0, 0.0, er, r, er, 0.0];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCUSHAPEPROFILEDEF" => {
                // (..., Depth, FlangeWidth, WebThickness, FlangeThickness,
                // FilletRadius, EdgeRadius, FlangeSlope, ...)
                let y = l(3, "depth")? / 2.0;
                let x = l(4, "flange width")? / 2.0;
                let tw = l(5, "web thickness")?;
                let tf = l(6, "flange thickness")?;
                let (r, er) = (o(7), o(8));
                let points = [
                    [-x, -y],
                    [x, -y],
                    [x, -y + tf],
                    [-x + tw, -y + tf],
                    [-x + tw, y - tf],
                    [x, y - tf],
                    [x, y],
                    [-x, y],
                ];
                let radii = [0.0, 0.0, er, r, r, er, 0.0, 0.0];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCTSHAPEPROFILEDEF" => {
                // (..., Depth, FlangeWidth, WebThickness, FlangeThickness,
                // FilletRadius, FlangeEdgeRadius, WebEdgeRadius, ...)
                let y = l(3, "depth")? / 2.0;
                let x = l(4, "flange width")? / 2.0;
                let w = l(5, "web thickness")? / 2.0;
                let tf = l(6, "flange thickness")?;
                let (r, fe, we) = (o(7), o(8), o(9));
                let points = [
                    [-w, -y],
                    [w, -y],
                    [w, y - tf],
                    [x, y - tf],
                    [x, y],
                    [-x, y],
                    [-x, y - tf],
                    [-w, y - tf],
                ];
                let radii = [we, we, r, fe, 0.0, 0.0, fe, r];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCZSHAPEPROFILEDEF" => {
                // (..., Depth, FlangeWidth, WebThickness, FlangeThickness,
                // FilletRadius, EdgeRadius)
                let y = l(3, "depth")? / 2.0;
                let b = l(4, "flange width")?;
                let w = l(5, "web thickness")? / 2.0;
                let tf = l(6, "flange thickness")?;
                let (r, er) = (o(7), o(8));
                let points = [
                    [-w, -y],
                    [b - w, -y],
                    [b - w, -y + tf],
                    [w, -y + tf],
                    [w, y],
                    [w - b, y],
                    [w - b, y - tf],
                    [-w, y - tf],
                ];
                let radii = [0.0, 0.0, er, r, 0.0, 0.0, er, r];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCCSHAPEPROFILEDEF" => {
                // (..., Depth, Width, WallThickness, Girth,
                // InternalFilletRadius, ...)
                let y = l(3, "depth")? / 2.0;
                let x = l(4, "width")? / 2.0;
                let t = l(5, "wall thickness")?;
                let g = l(6, "girth")?;
                let r = o(7);
                let points = [
                    [-x, -y],
                    [x, -y],
                    [x, -y + g],
                    [x - t, -y + g],
                    [x - t, -y + t],
                    [-x + t, -y + t],
                    [-x + t, y - t],
                    [x - t, y - t],
                    [x - t, y - g],
                    [x, y - g],
                    [x, y],
                    [-x, y],
                ];
                let ro = if r > 0.0 { r + t } else { 0.0 };
                let radii = [ro, ro, 0.0, 0.0, r, r, r, r, 0.0, 0.0, ro, ro];
                vec![Area {
                    outer: rounded(&points, &radii),
                    holes: Vec::new(),
                }]
            }
            "IFCTRAPEZIUMPROFILEDEF" => {
                // (..., BottomXDim, TopXDim, YDim, TopXOffset)
                let bx = l(3, "bottom width")?;
                let tx = l(4, "top width")?;
                let y = l(5, "height")? / 2.0;
                let off = o(6);
                let points = [
                    [-bx / 2.0, -y],
                    [bx / 2.0, -y],
                    [-bx / 2.0 + off + tx, y],
                    [-bx / 2.0 + off, y],
                ];
                vec![Area {
                    outer: rounded(&points, &[0.0; 4]),
                    holes: Vec::new(),
                }]
            }
            other => return Err(format!("#{}: profiles of type {other} are not read", e.id)),
        };
        Ok(made(areas).transformed(&position))
    }

    /// A closed curve of a profile (counter-clockwise as the file gives it;
    /// the importer orients the loops itself).
    fn closed(&self, value: &Value, profile: u64) -> Decoded<Path> {
        let path = self.curve(value)?;
        if path.segments.is_empty() {
            return Err(format!("#{profile}: an empty profile curve"));
        }
        let size = path
            .segments
            .iter()
            .flat_map(|s| s.sample(2))
            .map(math::length)
            .fold(1.0, f64::max);
        if !path.is_closed(1e-6 * size) {
            // A closed profile closes its curve: polylines that leave out
            // the last line back to the first point are common.
            let (a, b) = (
                path.end().expect("not empty"),
                path.start().expect("not empty"),
            );
            let mut path = path;
            path.segments.push(Seg::Line { start: a, end: b });
            return Ok(path);
        }
        Ok(path)
    }

    // Solids and other items.

    /// A representation item as an [`Item`]; what cannot be read is an
    /// [`Item::Unsupported`].
    ///
    /// Sub-items, curves and profiles that the item uses several times
    /// are decoded at most twice; an item that would copy more than
    /// [`MAX_SHARED_SIZE`] of them is unsupported as a whole
    /// ([`TOO_MANY_ITEMS`], mitcad#930).
    pub fn item(&self, id: u64) -> Item {
        let too_many = || Item::Unsupported {
            id,
            kind: self.file.kind(id).unwrap_or_default().to_owned(),
            why: TOO_MANY_ITEMS.to_owned(),
        };
        self.top(too_many, || self.item_at(id, 0))
    }

    fn item_at(&self, id: u64, depth: usize) -> Item {
        let unsupported = |kind: &str, why: String| Item::Unsupported {
            id,
            kind: kind.to_owned(),
            why,
        };
        let Some(e) = self.file.get(id) else {
            return unsupported("", format!("#{id} is not in the file"));
        };
        if depth > MAX_DEPTH {
            return unsupported(&e.kind, "items nested too deeply".to_owned());
        }
        self.memoised(
            id,
            Memo::items,
            || unsupported(&e.kind, TOO_MANY_ITEMS.to_owned()),
            || match self.solid(e, depth) {
                Ok(item) => item,
                Err(why) => unsupported(&e.kind, why),
            },
        )
    }

    fn solid(&self, e: &Entity, depth: usize) -> Decoded<Item> {
        let kind = &*e.kind;
        Ok(match kind {
            "IFCEXTRUDEDAREASOLID" => {
                // (SweptArea, Position, ExtrudedDirection, Depth)
                let profile = self.profile(e.arg(0))?;
                let position = self.placement(e.arg(1))?;
                let direction = math::normalize(self.direction(e.arg(2))?)
                    .ok_or("a zero extrusion direction")?;
                let depth = self.length(e.arg(3), "depth")?;
                Item::Extrusion {
                    position,
                    profile,
                    direction,
                    depth,
                }
            }
            "IFCEXTRUDEDAREASOLIDTAPERED" => {
                // (SweptArea, Position, ExtrudedDirection, Depth, EndSweptArea)
                let profile = self.profile(e.arg(0))?;
                let end_profile = self.profile(e.arg(4))?;
                let position = self.placement(e.arg(1))?;
                let direction = math::normalize(self.direction(e.arg(2))?)
                    .ok_or("a zero extrusion direction")?;
                let depth = self.length(e.arg(3), "depth")?;
                Item::TaperedExtrusion {
                    position,
                    profile,
                    end_profile,
                    direction,
                    depth,
                }
            }
            "IFCREVOLVEDAREASOLID" => {
                // (SweptArea, Position, Axis, Angle)
                let profile = self.profile(e.arg(0))?;
                let position = self.placement(e.arg(1))?;
                let (axis_origin, axis) = self.axis1(e.arg(2))?;
                let angle = e.arg(3).as_f64().ok_or("no angle")? * self.units.angle;
                Item::Revolution {
                    position,
                    profile,
                    axis_origin,
                    axis,
                    angle,
                }
            }
            "IFCSURFACECURVESWEPTAREASOLID" | "IFCFIXEDREFERENCESWEPTAREASOLID" => {
                // (SweptArea, Position, Directrix, StartParam, EndParam,
                // ReferenceSurface | FixedReference)
                let profile = self.profile(e.arg(0))?;
                let position = self.placement(e.arg(1))?;
                let directrix = self.curve(e.arg(2))?;
                if !e.arg(3).is_null() || !e.arg(4).is_null() {
                    return Err("sweeps along a part of the directrix are not read".to_owned());
                }
                let reference = if kind == "IFCFIXEDREFERENCESWEPTAREASOLID" {
                    Some(self.direction(e.arg(5))?)
                } else {
                    let surface = self.entity(e.arg(5))?;
                    if !surface.is("IFCPLANE") {
                        return Err(format!(
                            "sweeps on a {} are not read (planes only)",
                            surface.kind
                        ));
                    }
                    let frame = self.placement(surface.arg(0))?;
                    Some(math::column(&frame, 2))
                };
                Item::Sweep {
                    position,
                    profile,
                    directrix,
                    reference,
                    binormal: None,
                }
            }
            "IFCSWEPTDISKSOLID" | "IFCSWEPTDISKSOLIDPOLYGONAL" => {
                // (Directrix, Radius, InnerRadius, StartParam, EndParam)
                let directrix = self.curve(e.arg(0))?;
                let radius = self.length(e.arg(1), "radius")?;
                let inner_radius = self.opt_length(e.arg(2)).filter(|r| *r > 0.0);
                if !e.arg(3).is_null() || !e.arg(4).is_null() {
                    return Err("swept disks along a part of the directrix are not read".to_owned());
                }
                if kind == "IFCSWEPTDISKSOLIDPOLYGONAL" && !e.arg(5).is_null() {
                    return Err("swept disks with fillet radii are not read".to_owned());
                }
                Item::SweptDisk {
                    directrix,
                    radius,
                    inner_radius,
                }
            }
            "IFCBOOLEANRESULT" | "IFCBOOLEANCLIPPINGRESULT" => {
                let op = match e.arg(0).as_enum() {
                    Some("UNION") => BoolOp::Union,
                    Some("DIFFERENCE") => BoolOp::Difference,
                    Some("INTERSECTION") => BoolOp::Intersection,
                    other => return Err(format!("the boolean operator {other:?}")),
                };
                let first = self.item_at(e.arg(1).as_ref().ok_or("no first operand")?, depth + 1);
                let second = self.item_at(e.arg(2).as_ref().ok_or("no second operand")?, depth + 1);
                Item::Boolean {
                    op,
                    first: Box::new(first),
                    second: Box::new(second),
                }
            }
            "IFCCSGSOLID" => return self.solid(self.entity(e.arg(0))?, depth + 1),
            "IFCHALFSPACESOLID" | "IFCBOXEDHALFSPACE" | "IFCPOLYGONALBOUNDEDHALFSPACE" => {
                // (BaseSurface, AgreementFlag[, Position, PolygonalBoundary
                // | Enclosure])
                let surface = self.entity(e.arg(0))?;
                if !surface.is("IFCPLANE") {
                    return Err(format!(
                        "half spaces of a {} are not read (planes only)",
                        surface.kind
                    ));
                }
                let plane = self.placement(surface.arg(0))?;
                // The agreement flag is true when the plane's normal points
                // away from the half space's material.
                let below = e.arg(1).as_bool().unwrap_or(true);
                let boundary = if kind == "IFCPOLYGONALBOUNDEDHALFSPACE" {
                    let frame = self.placement(e.arg(2))?;
                    let polygon = self.closed(e.arg(3), e.id)?;
                    Some((frame, polygon))
                } else {
                    None
                };
                Item::HalfSpace {
                    plane,
                    below,
                    boundary,
                }
            }
            "IFCBLOCK" => Item::Block {
                position: self.placement(e.arg(0))?,
                size: [
                    self.length(e.arg(1), "x length")?,
                    self.length(e.arg(2), "y length")?,
                    self.length(e.arg(3), "z length")?,
                ],
            },
            "IFCRECTANGULARPYRAMID" => Item::Pyramid {
                position: self.placement(e.arg(0))?,
                size: [
                    self.length(e.arg(1), "x length")?,
                    self.length(e.arg(2), "y length")?,
                    self.length(e.arg(3), "height")?,
                ],
            },
            "IFCRIGHTCIRCULARCYLINDER" => Item::Cylinder {
                position: self.placement(e.arg(0))?,
                height: self.length(e.arg(1), "height")?,
                radius: self.length(e.arg(2), "radius")?,
            },
            "IFCRIGHTCIRCULARCONE" => Item::Cone {
                position: self.placement(e.arg(0))?,
                height: self.length(e.arg(1), "height")?,
                radius: self.length(e.arg(2), "radius")?,
            },
            "IFCSPHERE" => Item::Sphere {
                position: self.placement(e.arg(0))?,
                radius: self.length(e.arg(1), "radius")?,
            },
            "IFCBOUNDINGBOX" => {
                // (Corner, XDim, YDim, ZDim)
                let corner = self.point(e.arg(0))?;
                Item::Block {
                    position: math::frame(corner, None, None),
                    size: [
                        self.length(e.arg(1), "x size")?,
                        self.length(e.arg(2), "y size")?,
                        self.length(e.arg(3), "z size")?,
                    ],
                }
            }
            "IFCFACETEDBREP" | "IFCFACETEDBREPWITHVOIDS" => {
                let mut faces = self.shell_faces(e.arg(0))?;
                if kind == "IFCFACETEDBREPWITHVOIDS" {
                    for void in e.arg(1).as_list().unwrap_or_default() {
                        faces.extend(self.shell_faces(void)?);
                    }
                }
                Item::Faceted {
                    faces,
                    closed: true,
                }
            }
            "IFCFACEBASEDSURFACEMODEL" | "IFCSHELLBASEDSURFACEMODEL" => {
                let mut faces = Vec::new();
                let mut closed = true;
                let mut advanced = Vec::new();
                for shell in e.arg(0).as_list().unwrap_or_default() {
                    let s = self.entity(shell)?;
                    closed &= s.is("IFCCLOSEDSHELL");
                    let has_advanced = s
                        .arg(0)
                        .refs()
                        .first()
                        .and_then(|f| self.file.kind(*f))
                        .is_some_and(|k| k == "IFCADVANCEDFACE");
                    if has_advanced {
                        advanced.extend(self.advanced_faces(shell)?);
                    } else {
                        faces.extend(self.shell_faces(shell)?);
                    }
                }
                if !advanced.is_empty() {
                    if !faces.is_empty() {
                        return Err(
                            "surface models of planar and advanced faces together are not read"
                                .to_owned(),
                        );
                    }
                    Item::Advanced {
                        faces: advanced,
                        closed,
                    }
                } else {
                    Item::Faceted { faces, closed }
                }
            }
            "IFCADVANCEDBREP" | "IFCADVANCEDBREPWITHVOIDS" => {
                let mut faces = self.advanced_faces(e.arg(0))?;
                if kind == "IFCADVANCEDBREPWITHVOIDS" {
                    for void in e.arg(1).as_list().unwrap_or_default() {
                        faces.extend(self.advanced_faces(void)?);
                    }
                }
                Item::Advanced {
                    faces,
                    closed: true,
                }
            }
            "IFCTRIANGULATEDFACESET" | "IFCTRIANGULATEDIRREGULARNETWORK" => {
                // (Coordinates, Normals, Closed, CoordIndex, PnIndex[, Flags])
                let points = self.point_list(e.arg(0))?;
                let closed = e.arg(2).as_bool().unwrap_or(false);
                let pn: Vec<i64> = e
                    .arg(4)
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Value::as_i64)
                    .collect();
                let mut triangles = Vec::new();
                for t in e.arg(3).as_list().unwrap_or_default() {
                    let idx: Vec<i64> = t
                        .as_list()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_i64)
                        .collect();
                    if idx.len() != 3 {
                        return Err("a triangle without three indices".to_owned());
                    }
                    let mut tri = [0u32; 3];
                    for (k, i) in idx.iter().enumerate() {
                        tri[k] = point_index(*i, &pn, points.len())?;
                    }
                    triangles.push(tri);
                }
                Item::Mesh {
                    points,
                    triangles,
                    closed,
                }
            }
            "IFCPOLYGONALFACESET" => {
                // (Coordinates, Closed, Faces, PnIndex)
                let points = self.point_list(e.arg(0))?;
                let closed = e.arg(1).as_bool().unwrap_or(false);
                let pn: Vec<i64> = e
                    .arg(3)
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(Value::as_i64)
                    .collect();
                let mut triangles = Vec::new();
                let indices = |v: &Value| -> Decoded<Vec<u32>> {
                    v.as_list()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_i64)
                        .map(|i| point_index(i, &pn, points.len()))
                        .collect()
                };
                for f in e.arg(2).refs() {
                    let face = self
                        .file
                        .get(f)
                        .ok_or_else(|| format!("#{f} is not in the file"))?;
                    // IfcIndexedPolygonalFace(CoordIndex);
                    // ...WithVoids(CoordIndex, InnerCoordIndices).
                    let outer = indices(face.arg(0))?;
                    let mut holes = Vec::new();
                    if face.is("IFCINDEXEDPOLYGONALFACEWITHVOIDS") {
                        for h in face.arg(1).as_list().unwrap_or_default() {
                            holes.push(indices(h)?);
                        }
                    }
                    triangles.extend(triangulate(&points, &outer, &holes));
                }
                Item::Mesh {
                    points,
                    triangles,
                    closed,
                }
            }
            "IFCMAPPEDITEM" => {
                // (MappingSource: IfcRepresentationMap(MappingOrigin,
                // MappedRepresentation), MappingTarget)
                let map = e.arg(0).as_ref().ok_or("no mapping source")?;
                let source = self.entity(e.arg(0))?;
                let origin = self.placement(source.arg(0))?;
                let target = self.operator(e.arg(1))?;
                // The map's origin lands on the target.
                let inverse = math::inverse(&origin).ok_or("a degenerate mapping origin")?;
                Item::Mapped {
                    map,
                    transform: math::mul(&target, &inverse),
                }
            }
            _ => return Err(format!("items of type {kind} are not read")),
        })
    }

    fn point_list(&self, value: &Value) -> Decoded<Vec<[f64; 3]>> {
        let list = self.entity(value)?;
        let s = self.units.length;
        list.arg(0)
            .as_list()
            .unwrap_or_default()
            .iter()
            .map(|p| {
                let c = p.numbers().ok_or("a point without coordinates")?;
                Ok([
                    c.first().copied().unwrap_or(0.0) * s,
                    c.get(1).copied().unwrap_or(0.0) * s,
                    c.get(2).copied().unwrap_or(0.0) * s,
                ])
            })
            .collect()
    }

    /// The planar faces of an IfcConnectedFaceSet (a shell): each face's
    /// loops (the outer first, each loop in the face's orientation).
    fn shell_faces(&self, value: &Value) -> Decoded<Vec<Vec<Vec<[f64; 3]>>>> {
        let shell = self.entity(value)?;
        let mut faces = Vec::new();
        for f in shell.arg(0).refs() {
            let face = self
                .file
                .get(f)
                .ok_or_else(|| format!("#{f} is not in the file"))?;
            let mut loops: Vec<Vec<[f64; 3]>> = Vec::new();
            for b in face.arg(0).refs() {
                let bound = self
                    .file
                    .get(b)
                    .ok_or_else(|| format!("#{b} is not in the file"))?;
                // IfcFaceBound(Bound, Orientation)
                let lp = self.entity(bound.arg(0))?;
                if !lp.is("IFCPOLYLOOP") {
                    return Err(format!(
                        "#{}: face bounds of {} are not read",
                        lp.id, lp.kind
                    ));
                }
                let mut points: Vec<[f64; 3]> = lp
                    .arg(0)
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| self.point(p))
                    .collect::<Decoded<_>>()?;
                if bound.arg(1).as_bool() == Some(false) {
                    points.reverse();
                }
                if bound.is("IFCFACEOUTERBOUND") {
                    loops.insert(0, points);
                } else {
                    loops.push(points);
                }
            }
            if !loops.is_empty() {
                faces.push(loops);
            }
        }
        Ok(faces)
    }

    fn advanced_faces(&self, value: &Value) -> Decoded<Vec<AdvancedFace>> {
        let shell = self.entity(value)?;
        let mut faces = Vec::new();
        for f in shell.arg(0).refs() {
            let face = self
                .file
                .get(f)
                .ok_or_else(|| format!("#{f} is not in the file"))?;
            // IfcAdvancedFace(Bounds, FaceSurface, SameSense)
            let surface = self.surface(face.arg(1))?;
            let same_sense = face.arg(2).as_bool().unwrap_or(true);
            let mut bounds = Vec::new();
            for b in face.arg(0).refs() {
                let bound = self
                    .file
                    .get(b)
                    .ok_or_else(|| format!("#{b} is not in the file"))?;
                let lp = self.entity(bound.arg(0))?;
                let mut edges = Vec::new();
                if lp.is("IFCEDGELOOP") {
                    for oe in lp.arg(0).refs() {
                        let oriented = self
                            .file
                            .get(oe)
                            .ok_or_else(|| format!("#{oe} is not in the file"))?;
                        // IfcOrientedEdge(EdgeStart*, EdgeEnd*, EdgeElement,
                        // Orientation); IfcEdgeCurve(EdgeStart, EdgeEnd,
                        // EdgeGeometry, SameSense).
                        let edge = self.entity(oriented.arg(2))?;
                        let orientation = oriented.arg(3).as_bool().unwrap_or(true);
                        let vertex = |v: &Value| -> Decoded<[f64; 3]> {
                            let vp = self.entity(v)?;
                            self.point(vp.arg(0))
                        };
                        let a = vertex(edge.arg(0))?;
                        let b = vertex(edge.arg(1))?;
                        let (va, vb) = (
                            edge.arg(0).as_ref().unwrap_or(0),
                            edge.arg(1).as_ref().unwrap_or(0),
                        );
                        let curve = self.curve(edge.arg(2))?;
                        let same = edge.arg(3).as_bool().unwrap_or(true);
                        let (start, end) = if orientation { (a, b) } else { (b, a) };
                        edges.push(Edge {
                            id: edge.id,
                            vertices: if orientation { [va, vb] } else { [vb, va] },
                            curve,
                            start,
                            end,
                            forward: same == orientation,
                        });
                    }
                } else if lp.is("IFCPOLYLOOP") {
                    let points: Vec<[f64; 3]> = lp
                        .arg(0)
                        .as_list()
                        .unwrap_or_default()
                        .iter()
                        .map(|p| self.point(p))
                        .collect::<Decoded<_>>()?;
                    for i in 0..points.len() {
                        let (a, b) = (points[i], points[(i + 1) % points.len()]);
                        edges.push(Edge {
                            id: 0,
                            vertices: [0, 0],
                            curve: Path {
                                segments: vec![Seg::Line { start: a, end: b }],
                            },
                            start: a,
                            end: b,
                            forward: true,
                        });
                    }
                } else {
                    return Err(format!(
                        "#{}: face bounds of {} are not read",
                        lp.id, lp.kind
                    ));
                }
                if bound.arg(1).as_bool() == Some(false) {
                    edges.reverse();
                    for e in &mut edges {
                        std::mem::swap(&mut e.start, &mut e.end);
                        e.vertices.swap(0, 1);
                        e.forward = !e.forward;
                    }
                }
                bounds.push(Bound {
                    outer: bound.is("IFCFACEOUTERBOUND"),
                    edges,
                });
            }
            faces.push(AdvancedFace {
                surface,
                bounds,
                same_sense,
            });
        }
        Ok(faces)
    }

    fn surface(&self, value: &Value) -> Decoded<Surface> {
        let e = self.entity(value)?;
        Ok(match &*e.kind {
            "IFCPLANE" => Surface::Plane {
                frame: self.placement(e.arg(0))?,
            },
            "IFCCYLINDRICALSURFACE" => Surface::Cylinder {
                frame: self.placement(e.arg(0))?,
                radius: self.length(e.arg(1), "radius")?,
            },
            "IFCCONICALSURFACE" => Surface::Cone {
                frame: self.placement(e.arg(0))?,
                radius: self.length(e.arg(1), "radius")?,
                semi_angle: e.arg(2).as_f64().ok_or("no semi angle")? * self.units.angle,
            },
            "IFCSPHERICALSURFACE" => Surface::Sphere {
                frame: self.placement(e.arg(0))?,
                radius: self.length(e.arg(1), "radius")?,
            },
            "IFCTOROIDALSURFACE" => Surface::Torus {
                frame: self.placement(e.arg(0))?,
                major: self.length(e.arg(1), "major radius")?,
                minor: self.length(e.arg(2), "minor radius")?,
            },
            "IFCBSPLINESURFACEWITHKNOTS" | "IFCRATIONALBSPLINESURFACEWITHKNOTS" => {
                // (UDegree, VDegree, ControlPointsList, SurfaceForm, UClosed,
                // VClosed, SelfIntersect, UMultiplicities, VMultiplicities,
                // UKnots, VKnots, KnotSpec[, WeightsData])
                let u_degree = as_count(e.arg(0).as_i64().ok_or("no u degree")?);
                let v_degree = as_count(e.arg(1).as_i64().ok_or("no v degree")?);
                let poles: Vec<Vec<[f64; 3]>> = e
                    .arg(2)
                    .as_list()
                    .unwrap_or_default()
                    .iter()
                    .map(|row| {
                        row.as_list()
                            .unwrap_or_default()
                            .iter()
                            .map(|p| self.point(p))
                            .collect::<Decoded<Vec<_>>>()
                    })
                    .collect::<Decoded<_>>()?;
                let ints = |v: &Value| -> Vec<u32> {
                    v.as_list()
                        .unwrap_or_default()
                        .iter()
                        .filter_map(Value::as_i64)
                        .map(as_count)
                        .collect()
                };
                let weights = if e.is("IFCRATIONALBSPLINESURFACEWITHKNOTS") {
                    e.arg(12)
                        .as_list()
                        .unwrap_or_default()
                        .iter()
                        .map(|row| row.numbers().unwrap_or_default())
                        .collect()
                } else {
                    Vec::new()
                };
                let surface = Surface::BSpline {
                    u_degree,
                    v_degree,
                    weights,
                    u_multiplicities: ints(e.arg(7)),
                    v_multiplicities: ints(e.arg(8)),
                    u_knots: e.arg(9).numbers().unwrap_or_default(),
                    v_knots: e.arg(10).numbers().unwrap_or_default(),
                    poles,
                };
                if let Surface::BSpline {
                    poles,
                    u_knots,
                    u_multiplicities,
                    v_knots,
                    v_multiplicities,
                    ..
                } = &surface
                {
                    check_spline(e.id, u_degree, poles.len(), u_knots, u_multiplicities)?;
                    let columns = poles.first().map_or(0, Vec::len);
                    if poles.iter().any(|r| r.len() != columns) {
                        return Err(format!("#{}: rows of poles of different lengths", e.id));
                    }
                    check_spline(e.id, v_degree, columns, v_knots, v_multiplicities)?;
                }
                surface
            }
            "IFCSURFACEOFLINEAREXTRUSION" => {
                // (SweptCurve: a profile, Position, ExtrudedDirection, Depth)
                let position = self.placement(e.arg(1))?;
                let curve = self.profile_curve(e.arg(0))?.transformed(&position);
                let d = math::normalize(self.direction(e.arg(2))?).ok_or("a zero direction")?;
                let depth = self.length(e.arg(3), "depth")?;
                Surface::Extrusion {
                    curve,
                    direction: math::scale(math::apply_vector(&position, d), depth),
                }
            }
            "IFCSURFACEOFREVOLUTION" => {
                // (SweptCurve, Position, AxisPosition)
                let position = self.placement(e.arg(1))?;
                let curve = self.profile_curve(e.arg(0))?.transformed(&position);
                let (origin, axis) = self.axis1(e.arg(2))?;
                Surface::Revolution {
                    curve,
                    origin: math::apply(&position, origin),
                    axis: math::apply_vector(&position, axis),
                }
            }
            other => return Err(format!("#{}: surfaces of type {other} are not read", e.id)),
        })
    }

    /// The curve of an open or closed profile (a swept surface's).
    fn profile_curve(&self, value: &Value) -> Decoded<Path> {
        let e = self.entity(value)?;
        match &*e.kind {
            // (ProfileType, ProfileName, Curve)
            "IFCARBITRARYOPENPROFILEDEF" | "IFCARBITRARYCLOSEDPROFILEDEF" => self.curve(e.arg(2)),
            _ => {
                let p = self.profile(value)?;
                p.areas
                    .into_iter()
                    .next()
                    .map(|a| a.outer)
                    .ok_or_else(|| format!("#{}: an empty profile", e.id))
            }
        }
    }
}

/// A degree or a knot multiplicity from the file; one that is negative or
/// beyond `u32` becomes `u32::MAX`, which [`check_spline`] refuses.
fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Whether a B-spline's degree, poles, knots and multiplicities fit
/// together. The multiplicities are summed without wrapping and each is at
/// most degree + 1 (mitcad#930: a sum that wrapped to the expected total
/// let [`full_knots`] build a knot vector of billions).
fn check_spline(id: u64, degree: u32, poles: usize, knots: &[f64], mults: &[u32]) -> Decoded<()> {
    let total: u64 = mults.iter().map(|&m| u64::from(m)).sum();
    if degree == 0 || poles <= degree as usize {
        return Err(format!(
            "#{id}: a B-spline of degree {degree} with {poles} poles"
        ));
    }
    let expected = poles as u64 + u64::from(degree) + 1;
    if knots.len() != mults.len() || total != expected {
        return Err(format!(
            "#{id}: {} knots with multiplicities summing to {total} do not fit {poles} poles of degree {degree}",
            knots.len()
        ));
    }
    if let Some(m) = mults
        .iter()
        .find(|&&m| u64::from(m) > u64::from(degree) + 1)
    {
        return Err(format!(
            "#{id}: a knot multiplicity of {m} does not fit degree {degree}"
        ));
    }
    if knots.windows(2).any(|w| w[1] < w[0]) {
        return Err(format!("#{id}: the knots decrease"));
    }
    Ok(())
}

/// A 1-based index from the file as a 0-based one below `len`; none when it
/// is 0 or less or past the end (checked: the file's `i64` may be anything,
/// mitcad#930).
fn one_based(i: i64, len: usize) -> Option<usize> {
    i.checked_sub(1)
        .and_then(|k| usize::try_from(k).ok())
        .filter(|k| *k < len)
}

fn point_index(i: i64, pn: &[i64], count: usize) -> Decoded<u32> {
    // With PnIndex, coordinate indices index it; then 1-based points.
    let i = if pn.is_empty() {
        i
    } else {
        one_based(i, pn.len())
            .map(|k| pn[k])
            .ok_or_else(|| format!("index {i} beyond the point index list"))?
    };
    one_based(i, count)
        .and_then(|k| u32::try_from(k).ok())
        .ok_or_else(|| format!("point index {i} out of range"))
}

fn polyline(points: &[[f64; 3]]) -> Vec<Seg> {
    points
        .windows(2)
        .filter(|w| math::length(math::sub(w[1], w[0])) > 0.0)
        .map(|w| Seg::Line {
            start: w[0],
            end: w[1],
        })
        .collect()
}

/// The arc from `a` through `b` to `c` (a line when they are collinear).
pub fn arc3(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Seg {
    let ab = math::sub(b, a);
    let ac = math::sub(c, a);
    let n = math::cross(ab, ac);
    let nn = math::dot(n, n);
    let scale = math::length(ac).max(math::length(ab));
    if nn.sqrt() <= 1e-12 * scale * scale {
        return Seg::Line { start: a, end: c };
    }
    // The circumcentre.
    let t1 = math::scale(math::cross(n, ab), math::dot(ac, ac));
    let t2 = math::scale(math::cross(ac, n), math::dot(ab, ab));
    let center = math::add(a, math::scale(math::add(t1, t2), 1.0 / (2.0 * nn)));
    let radius = math::length(math::sub(a, center));
    let frame = math::frame(center, Some(n), Some(math::sub(a, center)));
    let inverse = math::inverse(&frame).expect("a frame inverts");
    let angle = |p: [f64; 3]| {
        let l = math::apply(&inverse, p);
        l[1].atan2(l[0])
    };
    // n points so that a, b, c run counter-clockwise.
    let mut sweep = angle(c).rem_euclid(TAU);
    if sweep < 1e-12 {
        sweep = TAU;
    }
    Seg::Arc {
        frame,
        radius,
        start: 0.0,
        sweep,
    }
}

fn rect(x: f64, y: f64) -> [[f64; 2]; 4] {
    [[-x, -y], [x, -y], [x, y], [-x, y]]
}

fn circle(r: f64) -> Path {
    Path {
        segments: vec![Seg::Arc {
            frame: math::IDENTITY,
            radius: r,
            start: 0.0,
            sweep: TAU,
        }],
    }
}

/// A path moved so that its bounding box is centred on the origin.
fn centred(path: Path) -> Path {
    let pts: Vec<[f64; 3]> = path.segments.iter().flat_map(|s| s.sample(4)).collect();
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in &pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let m = [
        [1.0, 0.0, 0.0, -(lo[0] + hi[0]) / 2.0],
        [0.0, 1.0, 0.0, -(lo[1] + hi[1]) / 2.0],
        [0.0, 0.0, 1.0, 0.0],
    ];
    path.transformed(&m)
}

/// A closed polygon (counter-clockwise) with its corners rounded by the
/// radii (0: a sharp corner): lines and tangent arcs.
pub fn rounded(points: &[[f64; 2]], radii: &[f64]) -> Path {
    let n = points.len();
    let p3 = |p: [f64; 2]| [p[0], p[1], 0.0];
    // Each corner: where the arc starts and ends (the corner itself when
    // sharp), and the arc.
    let mut corners: Vec<([f64; 3], [f64; 3], Option<Seg>)> = Vec::with_capacity(n);
    for i in 0..n {
        let c = p3(points[i]);
        let prev = p3(points[(i + n - 1) % n]);
        let next = p3(points[(i + 1) % n]);
        let r = radii.get(i).copied().unwrap_or(0.0);
        let (Some(u), Some(v)) = (
            math::normalize(math::sub(prev, c)),
            math::normalize(math::sub(next, c)),
        ) else {
            corners.push((c, c, None));
            continue;
        };
        let cos = math::dot(u, v).clamp(-1.0, 1.0);
        let theta = cos.acos();
        if r <= 0.0 || theta < 1e-9 || (PI - theta) < 1e-9 {
            corners.push((c, c, None));
            continue;
        }
        let d = r / (theta / 2.0).tan();
        let t1 = math::add(c, math::scale(u, d));
        let t2 = math::add(c, math::scale(v, d));
        let bisector = math::normalize(math::add(u, v)).expect("not straight");
        let center = math::add(c, math::scale(bisector, r / (theta / 2.0).sin()));
        let a1 = (t1[1] - center[1]).atan2(t1[0] - center[0]);
        let a2 = (t2[1] - center[1]).atan2(t2[0] - center[0]);
        let mut sweep = (a2 - a1).rem_euclid(TAU);
        if sweep > PI {
            sweep -= TAU;
        }
        corners.push((
            t1,
            t2,
            Some(Seg::Arc {
                frame: math::frame(center, None, None),
                radius: r,
                start: a1,
                sweep,
            }),
        ));
    }
    let mut segments = Vec::new();
    for i in 0..n {
        let (_, out, arc) = &corners[i];
        if let Some(arc) = arc {
            segments.push(arc.clone());
        }
        let (into, _, _) = corners[(i + 1) % n];
        if math::length(math::sub(into, *out)) > 1e-12 {
            segments.push(Seg::Line {
                start: *out,
                end: into,
            });
        }
    }
    Path { segments }
}

/// Triangles of a planar polygon with holes (indices into `points`), by
/// ear clipping in the polygon's plane after joining the holes to the outer
/// loop. Degenerate polygons give what can be clipped.
pub fn triangulate(points: &[[f64; 3]], outer: &[u32], holes: &[Vec<u32>]) -> Vec<[u32; 3]> {
    if outer.len() < 3 {
        return Vec::new();
    }
    if outer.len() == 3 && holes.is_empty() {
        return vec![[outer[0], outer[1], outer[2]]];
    }
    let p = |i: u32| points[i as usize];
    // The polygon's normal (Newell's method).
    let mut normal = [0.0; 3];
    for k in 0..outer.len() {
        let (a, b) = (p(outer[k]), p(outer[(k + 1) % outer.len()]));
        normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
        normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
        normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let Some(normal) = math::normalize(normal) else {
        return Vec::new();
    };
    let frame = math::frame(p(outer[0]), Some(normal), None);
    let inverse = math::inverse(&frame).expect("a frame inverts");
    let flat = |i: u32| {
        let l = math::apply(&inverse, p(i));
        [l[0], l[1]]
    };
    let area = |ring: &[u32]| -> f64 {
        (0..ring.len())
            .map(|k| {
                let (a, b) = (flat(ring[k]), flat(ring[(k + 1) % ring.len()]));
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0
    };
    // The outer loop counter-clockwise in the frame (it is by Newell), the
    // holes clockwise; each hole joined at its rightmost point to the
    // nearest visible outer vertex.
    let mut ring: Vec<u32> = outer.to_vec();
    let mut sorted: Vec<Vec<u32>> = holes.iter().filter(|h| h.len() >= 3).cloned().collect();
    for h in &mut sorted {
        if area(h) > 0.0 {
            h.reverse();
        }
    }
    sorted.sort_by(|a, b| {
        let ma = a
            .iter()
            .map(|&i| flat(i)[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let mb = b
            .iter()
            .map(|&i| flat(i)[0])
            .fold(f64::NEG_INFINITY, f64::max);
        mb.total_cmp(&ma)
    });
    for hole in sorted {
        let (hk, hp) = hole
            .iter()
            .enumerate()
            .map(|(k, &i)| (k, flat(i)))
            .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
            .expect("not empty");
        // The ring vertex nearest to the hole's point whose join crosses
        // no edge of the ring.
        let crosses = |a: [f64; 2], b: [f64; 2], ring: &[u32]| {
            (0..ring.len()).any(|k| {
                let (c, d) = (flat(ring[k]), flat(ring[(k + 1) % ring.len()]));
                segments_cross(a, b, c, d)
            })
        };
        let mut best: Option<(usize, f64)> = None;
        for (k, &i) in ring.iter().enumerate() {
            let q = flat(i);
            let d = (q[0] - hp[0]).hypot(q[1] - hp[1]);
            if best.is_some_and(|(_, bd)| bd <= d) {
                continue;
            }
            if !crosses(hp, q, &ring) {
                best = Some((k, d));
            }
        }
        let Some((rk, _)) = best else {
            continue;
        };
        let mut joined = Vec::with_capacity(ring.len() + hole.len() + 2);
        joined.extend_from_slice(&ring[..=rk]);
        for j in 0..=hole.len() {
            joined.push(hole[(hk + j) % hole.len()]);
        }
        joined.extend_from_slice(&ring[rk..]);
        ring = joined;
    }
    // Ear clipping.
    let mut out = Vec::new();
    let mut guard = 0;
    while ring.len() > 3 && guard < 4 * points.len().max(ring.len()) * ring.len() {
        guard += 1;
        let m = ring.len();
        let mut clipped = false;
        for k in 0..m {
            let (ia, ib, ic) = (ring[(k + m - 1) % m], ring[k], ring[(k + 1) % m]);
            let (a, b, c) = (flat(ia), flat(ib), flat(ic));
            let cross = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if cross <= 1e-12 * ((b[0] - a[0]).abs() + (c[1] - a[1]).abs()).powi(2) {
                continue;
            }
            let inside = ring.iter().any(|&i| {
                if i == ia || i == ib || i == ic {
                    return false;
                }
                let q = flat(i);
                if q == a || q == b || q == c {
                    return false;
                }
                in_triangle(q, a, b, c)
            });
            if inside {
                continue;
            }
            out.push([ia, ib, ic]);
            ring.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // Nothing convex is left (collinear or tangled points): a fan.
            for k in 1..ring.len() - 1 {
                out.push([ring[0], ring[k], ring[k + 1]]);
            }
            return out;
        }
    }
    if ring.len() == 3 {
        out.push([ring[0], ring[1], ring[2]]);
    }
    out
}

fn in_triangle(p: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    let s = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    s(a, b, p) >= 0.0 && s(b, c, p) >= 0.0 && s(c, a, p) >= 0.0
}

fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let s = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (d1, d2) = (s(a, b, c), s(a, b, d));
    let (d3, d4) = (s(c, d, a), s(c, d, b));
    // Proper crossings only: shared end points do not count.
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IfcFile;

    fn decoder_items(text: &str) -> (IfcFile, Units) {
        let file = IfcFile::parse(text).expect("reads");
        let units = Units::of(&file);
        (file, units)
    }

    fn area(path: &Path) -> f64 {
        // The shoelace formula over sampled points.
        let pts: Vec<[f64; 3]> = path
            .segments
            .iter()
            .flat_map(|s| {
                let mut v = s.sample(2000);
                v.pop();
                v
            })
            .collect();
        (0..pts.len())
            .map(|k| {
                let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
                a[0] * b[1] - b[0] * a[1]
            })
            .sum::<f64>()
            / 2.0
    }

    #[test]
    fn building_items() {
        let (file, units) = decoder_items(crate::testdata::BUILDING);
        let d = Decoder::new(&file, &units);
        let Item::Extrusion {
            profile,
            depth,
            direction,
            ..
        } = d.item(104)
        else {
            panic!("an extrusion: {:?}", d.item(104));
        };
        assert_eq!(depth, 3000.0);
        assert_eq!(direction, [0.0, 0.0, 1.0]);
        assert!((area(&profile.areas[0].outer) - 1e6).abs() < 1e-3);
        let Item::Extrusion { profile, .. } = d.item(204) else {
            panic!("an extrusion");
        };
        let a = area(&profile.areas[0].outer) + area(&profile.areas[0].holes[0]);
        // The hole's circle runs counter-clockwise as the file gives it.
        assert!((area(&profile.areas[0].outer) - 2.4e7).abs() < 1e-3);
        assert!(
            (area(&profile.areas[0].holes[0]) - PI * 250_000.0).abs() < 5.0,
            "{a}"
        );
        let Item::Extrusion { profile, .. } = d.item(306) else {
            panic!("an extrusion");
        };
        assert!((area(&profile.areas[0].outer) - 8700.0).abs() < 1e-6);
        let Item::Boolean { op, first, second } = d.item(406) else {
            panic!("a boolean");
        };
        assert_eq!(op, BoolOp::Difference);
        assert!(matches!(*first, Item::Extrusion { .. }));
        let Item::HalfSpace { plane, below, .. } = *second else {
            panic!("a half space");
        };
        assert!(!below);
        assert_eq!(math::origin(&plane), [0.0, 0.0, 2500.0]);
        let Item::Faceted { faces, closed } = d.item(539) else {
            panic!("faceted");
        };
        assert!(closed);
        assert_eq!(faces.len(), 6);
        let Item::Mesh {
            points, triangles, ..
        } = d.item(601)
        else {
            panic!("a mesh");
        };
        assert_eq!(points.len(), 4);
        assert_eq!(triangles[0], [0, 2, 1]);
        let Item::Mapped { map, transform } = d.item(136) else {
            panic!("mapped");
        };
        assert_eq!(map, 140);
        assert_eq!(transform, math::IDENTITY);
    }

    #[test]
    fn rounded_profiles_keep_their_area() {
        // A 100 x 60 rectangle with 10 mm rounded corners.
        let p = rounded(&rect(50.0, 30.0), &[10.0; 4]);
        let expected = 100.0 * 60.0 - (4.0 - PI) * 100.0;
        assert!((area(&p) - expected).abs() < 1e-2, "{}", area(&p));
        assert!(p.is_closed(1e-9));
        // An I-shape with fillets: the four fillets add (1 - π/4) r² each.
        let (b, h, w, tf, r) = (100.0, 150.0, 5.0, 10.0, 8.0);
        let pts = [
            [-b, -h],
            [b, -h],
            [b, -h + tf],
            [w, -h + tf],
            [w, h - tf],
            [b, h - tf],
            [b, h],
            [-b, h],
            [-b, h - tf],
            [-w, h - tf],
            [-w, -h + tf],
            [-b, -h + tf],
        ];
        let radii = [0.0, 0.0, 0.0, r, r, 0.0, 0.0, 0.0, 0.0, r, r, 0.0];
        let p = rounded(&pts, &radii);
        let plain = 2.0 * (2.0 * b) * tf + 2.0 * w * (2.0 * h - 2.0 * tf);
        let expected = plain + 4.0 * (1.0 - PI / 4.0) * r * r;
        assert!(
            (area(&p) - expected).abs() < 1e-2,
            "{} {}",
            area(&p),
            expected
        );
    }

    #[test]
    fn arcs_through_three_points() {
        let s = arc3([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]);
        let Seg::Arc { radius, sweep, .. } = &s else {
            panic!("an arc");
        };
        assert!((radius - 1.0).abs() < 1e-12);
        assert!((sweep - PI).abs() < 1e-12);
        let end = s.end();
        assert!((end[0] + 1.0).abs() < 1e-12 && end[1].abs() < 1e-12);
        let mid = s.sample(2)[1];
        assert!(mid[0].abs() < 1e-12 && (mid[1] - 1.0).abs() < 1e-12);
        // Clockwise through the points.
        let s = arc3([1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0]);
        let mid = s.sample(2)[1];
        assert!((mid[1] + 1.0).abs() < 1e-12, "{mid:?}");
    }

    #[test]
    fn conic_splines_follow_the_circle() {
        let s = conic_spline(&math::IDENTITY, 2.0, 2.0, 0.3, 4.0);
        for p in s.sample(37) {
            assert!((p[0].hypot(p[1]) - 2.0).abs() < 1e-9, "{p:?}");
        }
        let (a, b) = (s.start(), s.end());
        assert!((a[0] - 2.0 * 0.3f64.cos()).abs() < 1e-12);
        assert!((b[1] - 2.0 * 4.3f64.sin()).abs() < 1e-12);
        let r = s.reversed();
        assert!(math::length(math::sub(r.start(), b)) < 1e-12);
    }

    #[test]
    fn polygons_with_holes_triangulate() {
        let points = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [4.0, 4.0, 0.0],
            [6.0, 4.0, 0.0],
            [6.0, 6.0, 0.0],
            [4.0, 6.0, 0.0],
        ];
        let tris = triangulate(&points, &[0, 1, 2, 3], &[vec![4, 5, 6, 7]]);
        let area: f64 = tris
            .iter()
            .map(|t| {
                let (a, b, c) = (
                    points[t[0] as usize],
                    points[t[1] as usize],
                    points[t[2] as usize],
                );
                math::cross(math::sub(b, a), math::sub(c, a))[2] / 2.0
            })
            .sum();
        assert!((area - 96.0).abs() < 1e-9, "{area}");
        // An L of six points, not convex.
        let points = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let tris = triangulate(&points, &[0, 1, 2, 3, 4, 5], &[]);
        assert_eq!(tris.len(), 4);
    }

    #[test]
    fn indexed_poly_curves_and_trimmed_circles() {
        let text = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\
            #1=IFCPROJECT('0123456789012345678901',$,$,$,$,$,$,$,#2);\
            #2=IFCUNITASSIGNMENT((#3,#4));\
            #3=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\
            #4=IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.);\
            #10=IFCCARTESIANPOINTLIST2D(((0.,0.),(10.,0.),(15.,5.),(10.,10.),(0.,10.)));\
            #11=IFCINDEXEDPOLYCURVE(#10,(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4)),IFCLINEINDEX((4,5,1))),.F.);\
            #20=IFCCIRCLE(#21,5.);#21=IFCAXIS2PLACEMENT2D(#22,$);#22=IFCCARTESIANPOINT((0.,0.));\
            #23=IFCTRIMMEDCURVE(#20,(IFCPARAMETERVALUE(0.)),(IFCPARAMETERVALUE(1.5707963267948966)),.F.,.PARAMETER.);\
            ENDSEC;END-ISO-10303-21;";
        let (file, units) = decoder_items(text);
        let d = Decoder::new(&file, &units);
        let path = d.curve(&Value::Ref(11)).expect("decodes");
        assert_eq!(path.segments.len(), 4);
        assert!(path.is_closed(1e-9));
        let expected = 100.0 + PI * 25.0 / 2.0;
        assert!((area(&path) - expected).abs() < 1e-3, "{}", area(&path));
        // Clockwise from 0 to 90 degrees: three quarters of the circle.
        let t = d.curve(&Value::Ref(23)).expect("decodes");
        let Seg::Arc { sweep, .. } = t.segments[0] else {
            panic!("an arc");
        };
        assert!((sweep + 1.5 * PI).abs() < 1e-12);
    }

    const LIMITS_HEAD: &str = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n\
        #1=IFCPROJECT('0123456789012345678901',$,$,$,$,$,$,$,#2);\n\
        #2=IFCUNITASSIGNMENT((#3));\n\
        #3=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);\n";

    /// `levels` booleans over one extruded solid (#10), each using the one
    /// below twice: #101 the lowest, `#(100 + levels)` the top.
    fn self_doubling_booleans(levels: u64) -> String {
        let mut text = LIMITS_HEAD.to_owned();
        text.push_str(
            "#10=IFCEXTRUDEDAREASOLID(#11,$,#13,100.);\n\
             #11=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,100.,100.);\n\
             #13=IFCDIRECTION((0.,0.,1.));\n\
             #101=IFCBOOLEANRESULT(.UNION.,#10,#10);\n",
        );
        for k in 102..=100 + levels {
            text.push_str(&format!(
                "#{k}=IFCBOOLEANRESULT(.UNION.,#{},#{});\n",
                k - 1,
                k - 1
            ));
        }
        text.push_str("ENDSEC;END-ISO-10303-21;");
        text
    }

    fn leaves(item: &Item) -> usize {
        match item {
            Item::Boolean { first, second, .. } => leaves(first) + leaves(second),
            _ => 1,
        }
    }

    #[test]
    fn shared_sub_items_decode_once() {
        // mitcad#930: a few levels of shared operands still decode to the
        // whole tree.
        let (file, units) = decoder_items(&self_doubling_booleans(5));
        let d = Decoder::new(&file, &units);
        let item = d.item(105);
        assert_eq!(leaves(&item), 32, "{item:?}");
        assert!(item.unsupported().is_empty(), "{:?}", item.unsupported());
        // The memo is the top-level call's: the next call starts afresh.
        assert_eq!(d.item(105), item);
        let Item::Boolean { first, .. } = &item else {
            panic!("a boolean");
        };
        assert_eq!(**first, d.item(104));
    }

    #[test]
    fn self_doubling_booleans_stop_at_the_shared_size() {
        // mitcad#930: 40 levels would be 2^40 extrusions; the item is
        // unsupported as a whole, at once.
        let (file, units) = decoder_items(&self_doubling_booleans(40));
        let d = Decoder::new(&file, &units);
        let started = std::time::Instant::now();
        let item = d.item(140);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        let Item::Unsupported { id, kind, why } = &item else {
            panic!("unsupported: {item:?}");
        };
        assert_eq!(*id, 140);
        assert_eq!(kind, "IFCBOOLEANRESULT");
        assert_eq!(why, TOO_MANY_ITEMS);
        // Other items of the decoder are not affected.
        assert!(matches!(d.item(10), Item::Extrusion { .. }));
    }

    #[test]
    fn self_doubling_composite_curves_stop_at_the_shared_size() {
        let mut text = LIMITS_HEAD.to_owned();
        text.push_str(
            "#10=IFCPOLYLINE((#11,#12));\n\
             #11=IFCCARTESIANPOINT((0.,0.));\n\
             #12=IFCCARTESIANPOINT((1.,0.));\n",
        );
        let mut parent = 10;
        for k in 1..=40u64 {
            let segment = 1000 + 2 * k;
            let curve = segment + 1;
            text.push_str(&format!(
                "#{segment}=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#{parent});\n\
                 #{curve}=IFCCOMPOSITECURVE((#{segment},#{segment}),.F.);\n"
            ));
            parent = curve;
        }
        text.push_str("ENDSEC;END-ISO-10303-21;");
        let (file, units) = decoder_items(&text);
        let d = Decoder::new(&file, &units);
        // A few levels: every segment.
        let path = d.curve(&Value::Ref(1011)).expect("decodes");
        assert_eq!(path.segments.len(), 32);
        let started = std::time::Instant::now();
        let error = d.curve(&Value::Ref(parent)).expect_err("too many");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(error, TOO_MANY_ITEMS);
    }

    #[test]
    fn point_indices_out_of_range_are_errors() {
        // mitcad#930: `i - 1` on the file's i64 overflowed for i64::MIN.
        assert_eq!(point_index(1, &[], 3), Ok(0));
        assert_eq!(point_index(3, &[], 3), Ok(2));
        for i in [0, -1, 4, i64::MIN, i64::MAX] {
            let error = point_index(i, &[], 3).expect_err("out of range");
            assert!(error.contains("out of range"), "{error}");
        }
        // Through PnIndex: its entries index the points.
        assert_eq!(point_index(2, &[3, 1], 3), Ok(0));
        for i in [0, 3, i64::MIN, i64::MAX] {
            let error = point_index(i, &[3, 1], 3).expect_err("beyond");
            assert!(error.contains("beyond the point index list"), "{error}");
        }
        assert!(point_index(1, &[i64::MIN], 3).is_err());
        assert!(point_index(1, &[4], 3).is_err());
        // The item's error: a face set and an indexed poly curve.
        let mut text = LIMITS_HEAD.to_owned();
        text.push_str(
            "#10=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(0.,1.,0.)));\n\
             #11=IFCTRIANGULATEDFACESET(#10,$,.F.,((1,2,-9223372036854775808)),$);\n\
             #12=IFCTRIANGULATEDFACESET(#10,$,.F.,((1,2,3)),$);\n\
             #20=IFCCARTESIANPOINTLIST2D(((0.,0.),(1.,0.)));\n\
             #21=IFCINDEXEDPOLYCURVE(#20,(IFCLINEINDEX((1,-9223372036854775808))),.F.);\n\
             #22=IFCINDEXEDPOLYCURVE(#20,(IFCLINEINDEX((0,1))),.F.);\n\
             ENDSEC;END-ISO-10303-21;",
        );
        let (file, units) = decoder_items(&text);
        let d = Decoder::new(&file, &units);
        let Item::Unsupported { why, .. } = d.item(11) else {
            panic!("unsupported: {:?}", d.item(11));
        };
        assert!(why.contains("out of range"), "{why}");
        assert!(matches!(d.item(12), Item::Mesh { .. }), "{:?}", d.item(12));
        for curve in [21, 22] {
            let error = d.curve(&Value::Ref(curve)).expect_err("out of range");
            assert!(error.contains("out of range"), "{error}");
        }
    }

    #[test]
    fn spline_multiplicities_do_not_wrap() {
        // mitcad#930: 4294967295 + 9 wrapped to 8 = 4 poles + degree 3 + 1
        // in u32, and the knot vector would have had 4.3e9 knots.
        let mut text = LIMITS_HEAD.to_owned();
        text.push_str(
            "#11=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #12=IFCCARTESIANPOINT((1.,0.,0.));\n\
             #13=IFCCARTESIANPOINT((2.,1.,0.));\n\
             #14=IFCCARTESIANPOINT((3.,1.,0.));\n\
             #20=IFCBSPLINECURVEWITHKNOTS(3,(#11,#12,#13,#14),.UNSPECIFIED.,.F.,.U.,(4294967295,9),(0.,1.),.UNSPECIFIED.);\n\
             #21=IFCBSPLINECURVEWITHKNOTS(3,(#11,#12,#13,#14),.UNSPECIFIED.,.F.,.U.,(4,4),(0.,1.),.UNSPECIFIED.);\n\
             #22=IFCBSPLINECURVEWITHKNOTS(3,(#11,#12,#13,#14),.UNSPECIFIED.,.F.,.U.,(-1,9),(0.,1.),.UNSPECIFIED.);\n\
             ENDSEC;END-ISO-10303-21;",
        );
        let (file, units) = decoder_items(&text);
        let d = Decoder::new(&file, &units);
        let error = d.curve(&Value::Ref(20)).expect_err("refused");
        assert!(error.contains("do not fit"), "{error}");
        assert!(error.contains("summing to 4294967304"), "{error}");
        let error = d.curve(&Value::Ref(22)).expect_err("refused");
        assert!(error.contains("do not fit"), "{error}");
        let path = d.curve(&Value::Ref(21)).expect("a clamped cubic");
        assert_eq!(path.segments.len(), 1);
        // Each multiplicity at most degree + 1, even when the sum fits.
        let error = check_spline(5, 1, 3, &[0.0, 1.0], &[3, 2]).expect_err("refused");
        assert!(error.contains("multiplicity of 3"), "{error}");
        assert!(check_spline(5, 1, 3, &[0.0, 0.5, 1.0], &[2, 1, 2]).is_ok());
    }
}
