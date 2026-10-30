// SPDX-License-Identifier: MIT
//! The elements' geometry as the file keeps it (mitcad#333): each
//! partition's third chain holds a `GElement` per element that has
//! geometry ([`crate::partition::Partition::geometry`]), a tree of nodes:
//!
//! - `GGroup`, `GFilter` (the latter with view conditions, `m_oConditions`)
//!   and `GElement` hold nodes (`m_subNodes`);
//! - `Geometry` is a B-rep body: `m_pFaces` (and `m_pEdges`);
//! - `GInstance` places a type's geometry: `m_instanceInfo` →
//!   `InstanceInfo` (`m_symbolId`, `m_Trf`), or its own embedded
//!   geometry (`m_oEmbeddedSymbolGRep`);
//! - curves (`GLine`, `GArc`, `GEllipse`) are symbolic lines (a profile
//!   family's profile), kept as curves.
//!
//! The B-rep is a winged-edge structure. A `Face` has a surface (`m_pSurf`)
//! and loops (`m_pFirstLoop` → `EdgeLoop`, `m_nextLoop`); a loop names its
//! first edge (`m_next`), and its last edge names the loop as its next. An `Edge`
//! belongs to two faces (`m_pFace[0..2]`),
//! with its next and previous edge in each face's loop (`m_next`,
//! `m_prev`); it has no curve of its own but its points in each face's
//! parameters: the first and the last (`m_firstAndLastEdgePnts`, each an
//! `EdgePnt` with a (u, v) per side) and, along a curved edge, points
//! between (`m_interiorEdgePnts`). The faces and edges name each other by
//! reference ([`crate::walk::Record::deref`]).
//!
//! Surfaces, (u, v) to model point (feet; checked: the two sides of every
//! edge land on the same points, [`Solid::seam_gap`]):
//!
//! - `Plane`: `m_origin + u m_xVec + v m_yVec`;
//! - `CylSurf`: `m_center + m_radius (cos u m_xVec + sin u m_yVec) + v
//!   m_zVec`;
//! - `SurfRev`: its profile curve (`m_pProfileCurve`, in the surface's
//!   frame `m_xVec`, `m_yVec`, `m_zVec` about `m_center`) at parameter v,
//!   turned by u about the frame's z axis;
//! - `RuledSurf`: the points at the fraction u of its two curves
//!   (`m_pProfileCurve1`, `m_pProfileCurve2`; `m_Point1`, `m_Point2` for a
//!   curve that is a point), joined by straight lines along v;
//! - `ConeSurf`: apex `m_center`, axis `m_zVec`: `m_center + v (sin a (cos
//!   u m_xVec + sin u m_yVec) + cos a m_zVec)`, a `m_halfAngle`;
//! - `HermiteSurf`: bicubic Hermite patches between its nodes
//!   (`m_NodeArray`: `m_iPoint`, `m_iTangent` in u and v, `m_iMixedDer`; u
//!   running fastest) at the parameters `m_uParams` by `m_vParams`.
//!
//! Other surfaces are kept by their class name: their faces' points come
//! from the neighbouring faces' surfaces.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use mitcad_vec3::{add, scale};
use serde::Serialize;

use crate::schema::Schema;
use crate::walk::{Object, Record, Value};

pub use mitcad_vec3::Vec3 as P3;
pub type Uv = [f64; 2];

/// A curve a surface is made from (feet; parameters as the file's).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Curve {
    /// `origin + t dir`.
    Line { origin: P3, dir: P3 },
    /// `center + a cos t x + b sin t y` (a circle when `a == b`).
    Ellipse {
        center: P3,
        x: P3,
        y: P3,
        a: f64,
        b: f64,
    },
}

impl Curve {
    pub fn at(&self, t: f64) -> P3 {
        match self {
            Curve::Line { origin, dir } => add(*origin, scale(*dir, t)),
            Curve::Ellipse { center, x, y, a, b } => {
                add(*center, add(scale(*x, a * t.cos()), scale(*y, b * t.sin())))
            }
        }
    }
}

/// A curve with its parameter range.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bounded {
    pub curve: Curve,
    pub range: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Surface {
    Plane {
        origin: P3,
        x: P3,
        y: P3,
    },
    Cylinder {
        center: P3,
        x: P3,
        y: P3,
        z: P3,
        radius: f64,
    },
    Revolution {
        center: P3,
        x: P3,
        y: P3,
        z: P3,
        profile: Curve,
    },
    /// The apex at `center`, the axis `z`: v along the slant, at the
    /// radius v sin(half angle) and the height v cos(half angle).
    Cone {
        center: P3,
        x: P3,
        y: P3,
        z: P3,
        half_angle: f64,
    },
    /// A bicubic Hermite surface: its nodes (point, u and v tangents, mixed
    /// derivative) at the parameters u by v, u running fastest.
    Hermite {
        u: Vec<f64>,
        v: Vec<f64>,
        nodes: Vec<[P3; 4]>,
    },
    Ruled {
        first: Result<Bounded, P3>,
        second: Result<Bounded, P3>,
    },
    /// A surface of a class not read.
    Other(String),
}

/// A bicubic Hermite surface's point: in the span of its parameters
/// that holds (u, v) (the first or last beyond them), the nodes' points
/// and derivatives (scaled by the span) blended by the cubic Hermite
/// functions.
fn hermite(us: &[f64], vs: &[f64], nodes: &[[P3; 4]], u: f64, v: f64) -> P3 {
    let span = |ps: &[f64], x: f64| {
        let i = ps
            .windows(2)
            .position(|w| x <= w[1])
            .unwrap_or(ps.len() - 2);
        let d = ps[i + 1] - ps[i];
        let s = if d.abs() > 0.0 { (x - ps[i]) / d } else { 0.0 };
        (i, d, s)
    };
    let (i, du, s) = span(us, u);
    let (j, dv, t) = span(vs, v);
    let h0 = |x: f64| {
        [
            2.0 * x.powi(3) - 3.0 * x * x + 1.0,
            -2.0 * x.powi(3) + 3.0 * x * x,
        ]
    };
    let h1 = |x: f64| [x.powi(3) - 2.0 * x * x + x, x.powi(3) - x * x];
    let (h0s, h1s, h0t, h1t) = (h0(s), h1(s), h0(t), h1(t));
    let nu = us.len();
    let mut p = [0.0; 3];
    for a in 0..2 {
        for b in 0..2 {
            let n = &nodes[(j + b) * nu + i + a];
            let w = [
                h0s[a] * h0t[b],
                du * h1s[a] * h0t[b],
                dv * h0s[a] * h1t[b],
                du * dv * h1s[a] * h1t[b],
            ];
            for (k, wk) in w.iter().enumerate() {
                p = add(p, scale(n[k], *wk));
            }
        }
    }
    p
}

impl Surface {
    /// The model point (feet) at (u, v), None for a surface not read.
    pub fn at(&self, [u, v]: Uv) -> Option<P3> {
        Some(match self {
            Surface::Plane { origin, x, y } => add(*origin, add(scale(*x, u), scale(*y, v))),
            Surface::Cylinder {
                center,
                x,
                y,
                z,
                radius,
            } => add(
                *center,
                add(
                    add(scale(*x, radius * u.cos()), scale(*y, radius * u.sin())),
                    scale(*z, v),
                ),
            ),
            Surface::Revolution {
                center,
                x,
                y,
                z,
                profile,
            } => {
                // The profile in the frame, turned by u about its z.
                let p = profile.at(v);
                let (c, s) = (u.cos(), u.sin());
                let local = [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]];
                add(
                    *center,
                    add(
                        add(scale(*x, local[0]), scale(*y, local[1])),
                        scale(*z, local[2]),
                    ),
                )
            }
            Surface::Cone {
                center,
                x,
                y,
                z,
                half_angle,
            } => {
                let (r, h) = (v * half_angle.sin(), v * half_angle.cos());
                add(
                    *center,
                    add(
                        add(scale(*x, r * u.cos()), scale(*y, r * u.sin())),
                        scale(*z, h),
                    ),
                )
            }
            Surface::Hermite {
                u: us,
                v: vs,
                nodes,
            } => hermite(us, vs, nodes, u, v),
            Surface::Ruled { first, second } => {
                let at = |c: &Result<Bounded, P3>| match c {
                    Ok(b) => b.curve.at(b.range[0] + u * (b.range[1] - b.range[0])),
                    Err(p) => *p,
                };
                let (a, b) = (at(first), at(second));
                add(scale(a, 1.0 - v), scale(b, v))
            }
            Surface::Other(_) => return None,
        })
    }

    pub fn is_plane(&self) -> bool {
        matches!(self, Surface::Plane { .. })
    }

    /// The surface's class name in the file.
    pub fn class(&self) -> &str {
        match self {
            Surface::Plane { .. } => "Plane",
            Surface::Cylinder { .. } => "CylSurf",
            Surface::Revolution { .. } => "SurfRev",
            Surface::Cone { .. } => "ConeSurf",
            Surface::Hermite { .. } => "HermiteSurf",
            Surface::Ruled { .. } => "RuledSurf",
            Surface::Other(c) => c,
        }
    }
}

/// An edge: its two faces (indices into [`Solid::faces`]; the same face
/// twice for a seam; None for the open side of a sheet's edge, or a face of
/// another body) and its points in each face's (u, v), first to last.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edge {
    pub faces: [Option<usize>; 2],
    pub points: [Vec<Uv>; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Face {
    pub surface: Surface,
    /// Its loops: each a list of (edge, side) in the loop's order, the
    /// side being the one of the edge on this face.
    pub loops: Vec<Vec<(usize, usize)>>,
    /// The face's material (`m_renderStyleId`), when it names one.
    pub material: Option<i64>,
}

/// A B-rep body (`Geometry`).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Solid {
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
}

impl Solid {
    /// A face's loop as points in its (u, v), each edge's points in the
    /// loop's direction, without repeating the joints.
    pub fn loop_uv(&self, face: usize, l: usize) -> Vec<Uv> {
        let uses = &self.faces[face].loops[l];
        let runs: Vec<&Vec<Uv>> = uses
            .iter()
            .map(|(e, side)| &self.edges[*e].points[*side])
            .collect();
        let d = |a: Uv, b: Uv| (a[0] - b[0]).hypot(a[1] - b[1]);
        let mut out: Vec<Uv> = Vec::new();
        for (k, run) in runs.iter().enumerate() {
            let (Some(first), Some(last)) = (run.first(), run.last()) else {
                continue;
            };
            // Forward when its first point joins the loop so far (or, for
            // the first edge, the next edge).
            let forward = match out.last() {
                Some(end) => d(*first, *end) <= d(*last, *end),
                None => match runs.get((k + 1) % runs.len()) {
                    Some(next) if runs.len() > 1 => {
                        let near = |p: Uv| {
                            next.first()
                                .map_or(f64::INFINITY, |n| d(p, *n))
                                .min(next.last().map_or(f64::INFINITY, |n| d(p, *n)))
                        };
                        near(*last) <= near(*first)
                    }
                    _ => true,
                },
            };
            let mut pts: Vec<Uv> = run.to_vec();
            if !forward {
                pts.reverse();
            }
            for p in pts {
                if out.last().is_none_or(|q| d(*q, p) > 1e-12) {
                    out.push(p);
                }
            }
        }
        if out.len() > 1 && d(out[0], out[out.len() - 1]) <= 1e-12 {
            out.pop();
        }
        out
    }

    /// The largest distance (feet) between the model points of an edge's
    /// two sides, over the edges whose two faces' surfaces are read: how
    /// well the surfaces' formulas fit.
    /// The box (feet: min and max corner) of its edges' points, on the
    /// surfaces that read.
    pub fn bounds(&self) -> Option<[f64; 6]> {
        let mut b: Option<[f64; 6]> = None;
        for e in &self.edges {
            for side in 0..2 {
                let Some(f) = e.faces[side] else { continue };
                for uv in &e.points[side] {
                    let Some(p) = self.faces[f].surface.at(*uv) else {
                        continue;
                    };
                    b = Some(match b {
                        None => [p[0], p[1], p[2], p[0], p[1], p[2]],
                        Some(b) => [
                            b[0].min(p[0]),
                            b[1].min(p[1]),
                            b[2].min(p[2]),
                            b[3].max(p[0]),
                            b[4].max(p[1]),
                            b[5].max(p[2]),
                        ],
                    });
                }
            }
        }
        b
    }

    pub fn seam_gap(&self) -> f64 {
        let mut gap: f64 = 0.0;
        for e in &self.edges {
            let (Some(a), Some(b)) = (e.faces[0], e.faces[1]) else {
                continue;
            };
            let (a, b) = (&self.faces[a], &self.faces[b]);
            for (p, q) in e.points[0].iter().zip(&e.points[1]) {
                if let (Some(p), Some(q)) = (a.surface.at(*p), b.surface.at(*q)) {
                    let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2))
                        .sqrt();
                    gap = gap.max(d);
                }
            }
        }
        gap
    }
}

/// A type's geometry placed in an element's (`GInstance`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    /// The type (`m_symbolId`), or None for embedded geometry.
    pub symbol: Option<i64>,
    /// Its transform (the 3×3 matrix by rows, then the origin, feet).
    pub transform: [f64; 12],
    /// Embedded geometry (`m_oEmbeddedSymbolGRep`).
    pub embedded: Option<Box<GRep>>,
}

/// An element's geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct GRep {
    pub solids: Vec<Solid>,
    pub instances: Vec<Placed>,
    /// Its curves (symbolic lines; a profile family's profile).
    pub curves: Vec<Bounded>,
    /// The box the file gives (feet, min and max).
    pub bbox: Option<[f64; 6]>,
    /// What did not read.
    pub notes: Vec<String>,
    /// Bodies and placed types left out: under view conditions a 3D view
    /// does not meet (the plan's and elevations' symbolic faces).
    pub hidden: usize,
}

impl GRep {
    pub fn is_empty(&self) -> bool {
        self.solids.is_empty() && self.instances.is_empty()
    }
}

struct Reader<'a> {
    schema: &'a Schema,
    record: &'a Record,
    /// The detail level its view conditions are met at (1 coarse, 2
    /// medium, 3 fine).
    detail: u64,
    visit: RefCell<Visit>,
    /// Edge steps one walk takes at most ([`MAX_EDGE_STEPS`]; lower in
    /// tests).
    max_edge_steps: usize,
}

/// Nodes (and bodies' faces) one walk of a record's node tree reads at
/// most (mitcad#924). A real tree is a tree: each node and face its own
/// object of the record, read once, so a walk reads at most the record's
/// objects, and a million would take a record of tens of megabytes (a
/// node or face is several objects of tens of bytes). Only nodes shared
/// by several parents (numbered pointers to one object) multiply, which a
/// crafted record can nest into an exponential count.
const MAX_NODES: usize = 1_000_000;

/// Edge steps (an edge's side taken in a face's loop) one walk of a
/// record's node tree takes at most, over all its bodies (mitcad#942).
/// A real tree reads each body once and each edge side of a body in one
/// loop once, so a walk takes at most two steps per edge of the record:
/// ten million would take five million edges, a record of hundreds of
/// megabytes. Only bodies shared by several nodes multiply, as nodes do
/// ([`MAX_NODES`] counts a body's faces, not its edges).
const MAX_EDGE_STEPS: usize = 10_000_000;

/// Where a walk of the node tree is.
#[derive(Default)]
struct Visit {
    /// The pointees on the path from the walk's start: a node whose
    /// sub-nodes name itself or an ancestor would not end.
    path: Vec<usize>,
    /// Nodes and faces read ([`MAX_NODES`]).
    read: usize,
    /// Edge steps taken in the bodies' loops ([`MAX_EDGE_STEPS`]).
    edge_steps: usize,
    /// The notes given, once each per walk: nested too deep, a node in
    /// itself, too many nodes, a face's loop named again, too many edge
    /// steps.
    noted: [bool; 5],
}

/// Index of a pointee object (by address) in the record: from its offset
/// in the record's pointees, as a search through them for every edge of a
/// body took quadratic time (mitcad#413).
fn index_of(record: &Record, o: &Object) -> Option<usize> {
    let size = std::mem::size_of::<Object>();
    let offset = (o as *const Object as usize).checked_sub(record.pointees.as_ptr() as usize)?;
    let i = offset / size;
    (offset % size == 0 && record.pointees.get(i).is_some_and(|p| std::ptr::eq(p, o))).then_some(i)
}

impl<'a> Reader<'a> {
    fn get(&self, o: &'a Object, name: &str) -> Option<&'a Value> {
        o.get(self.schema, name)
    }
    fn obj(&self, o: &'a Object, name: &str) -> Option<&'a Object> {
        self.record.deref(self.get(o, name)?)
    }
    fn class(&self, o: &Object) -> &'a str {
        self.schema.class_name(o.class)
    }
    fn vec3(&self, o: &Object, name: &str) -> Option<P3> {
        let v = self.get(o, name)?.floats();
        (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| [v[0], v[1], v[2]])
    }
    fn f64(&self, o: &Object, name: &str) -> Option<f64> {
        self.get(o, name)?.as_f64().filter(|v| v.is_finite())
    }
    fn list(&self, o: &'a Object, name: &str) -> &'a [Value] {
        self.get(o, name).and_then(Value::as_list).unwrap_or(&[])
    }

    fn curve(&self, o: &Object) -> Option<Bounded> {
        let t = self.get(o, "m_endParams")?.floats();
        if t.len() != 2 {
            return None;
        }
        let range = [t[0], t[1]];
        let curve = match self.class(o) {
            "GLine" => Curve::Line {
                origin: self.vec3(o, "m_origin")?,
                dir: self.vec3(o, "m_dirVec")?,
            },
            "GArc" => {
                let r = self.f64(o, "m_radius")?;
                Curve::Ellipse {
                    center: self.vec3(o, "m_center")?,
                    x: self.vec3(o, "m_xVec")?,
                    y: self.vec3(o, "m_yVec")?,
                    a: r,
                    b: r,
                }
            }
            "GEllipse" => Curve::Ellipse {
                center: self.vec3(o, "m_center")?,
                x: self.vec3(o, "m_xVec")?,
                y: self.vec3(o, "m_yVec")?,
                a: self.f64(o, "m_xLen")?,
                b: self.f64(o, "m_yLen")?,
            },
            _ => return None,
        };
        Some(Bounded { curve, range })
    }

    fn surface(&self, s: &Object) -> Surface {
        let class = self.class(s);
        let read = || -> Option<Surface> {
            Some(match class {
                "Plane" => Surface::Plane {
                    origin: self.vec3(s, "m_origin")?,
                    x: self.vec3(s, "m_xVec")?,
                    y: self.vec3(s, "m_yVec")?,
                },
                "ConeSurf" => Surface::Cone {
                    center: self.vec3(s, "m_center")?,
                    x: self.vec3(s, "m_xVec")?,
                    y: self.vec3(s, "m_yVec")?,
                    z: self.vec3(s, "m_zVec")?,
                    half_angle: self.f64(s, "m_halfAngle")?,
                },
                "CylSurf" => Surface::Cylinder {
                    center: self.vec3(s, "m_center")?,
                    x: self.vec3(s, "m_xVec")?,
                    y: self.vec3(s, "m_yVec")?,
                    z: self.vec3(s, "m_zVec")?,
                    radius: self.f64(s, "m_radius")?,
                },
                "SurfRev" => Surface::Revolution {
                    center: self.vec3(s, "m_center")?,
                    x: self.vec3(s, "m_xVec")?,
                    y: self.vec3(s, "m_yVec")?,
                    z: self.vec3(s, "m_zVec")?,
                    profile: self.curve(self.obj(s, "m_pProfileCurve")?)?.curve,
                },
                "HermiteSurf" => {
                    let u = self.get(s, "m_uParams")?.floats();
                    let v = self.get(s, "m_vParams")?.floats();
                    let nodes: Vec<[P3; 4]> = self
                        .list(s, "m_NodeArray")
                        .iter()
                        .filter_map(|n| {
                            let n = self.record.deref(n)?;
                            let t = self.get(n, "m_iTangent")?.floats();
                            if t.len() != 6 {
                                return None;
                            }
                            Some([
                                self.vec3(n, "m_iPoint")?,
                                [t[0], t[1], t[2]],
                                [t[3], t[4], t[5]],
                                self.vec3(n, "m_iMixedDer")?,
                            ])
                        })
                        .collect();
                    if u.len() < 2 || v.len() < 2 || nodes.len() != u.len() * v.len() {
                        return None;
                    }
                    Surface::Hermite { u, v, nodes }
                }
                "RuledSurf" => {
                    let side = |c: &str, p: &str| match self.obj(s, c) {
                        Some(o) => self.curve(o).ok_or(()).map_err(|_| [f64::NAN; 3]),
                        None => Err(self.vec3(s, p).unwrap_or([f64::NAN; 3])),
                    };
                    let first = side("m_pProfileCurve1", "m_Point1");
                    let second = side("m_pProfileCurve2", "m_Point2");
                    let bad = |c: &Result<Bounded, P3>| c.as_ref().is_err_and(|p| p[0].is_nan());
                    if bad(&first) || bad(&second) {
                        return None;
                    }
                    Surface::Ruled { first, second }
                }
                _ => return None,
            })
        };
        read().unwrap_or_else(|| Surface::Other(class.to_string()))
    }

    fn uv_pairs(&self, list: &[Value]) -> Vec<[Uv; 2]> {
        list.iter()
            .filter_map(|p| {
                let p = self.record.deref(p)?;
                let uv = self.get(p, "uv")?.as_list()?;
                let a = uv.first()?.floats();
                let b = uv.get(1)?.floats();
                (a.len() == 2 && b.len() == 2).then(|| [[a[0], a[1]], [b[0], b[1]]])
            })
            .collect()
    }

    /// A note given once per walk (`which` indexes [`Visit::noted`]).
    fn note_once(&self, which: usize, note: String, notes: &mut Vec<String>) {
        let first = !std::mem::replace(&mut self.visit.borrow_mut().noted[which], true);
        if first {
            notes.push(note);
        }
    }

    /// A `Geometry` node's B-rep, taking at most `left` edge steps (an
    /// edge's side in a face's loop), counted in `steps` (mitcad#942).
    ///
    /// A body takes at most two edge steps per object of the record, and
    /// fails past them: in a real body each side of an edge lies in one
    /// loop of one face, and each edge is an object of the record, so the
    /// steps are at most twice its edges (a loop that would take one side
    /// twice never comes back to its first edge, and fails as not closing
    /// already). A face's loop that names a loop already read on the face
    /// ends the face's loops there, with a note once per walk (a real
    /// face's loops end with a null).
    fn solid(
        &self,
        g: &'a Object,
        left: usize,
        steps: &mut usize,
        notes: &mut Vec<String>,
    ) -> Result<Solid, String> {
        let budget = self.record.pointees.len().saturating_mul(2).min(left);
        let faces: Vec<&Object> = self
            .list(g, "m_pFaces")
            .iter()
            .filter_map(|f| self.record.deref(f))
            .collect();
        // The faces and edges by their pointees' indices (searches made
        // reading a body of many faces quadratic).
        let mut face_of: HashMap<usize, usize> = HashMap::new();
        for (k, f) in faces.iter().enumerate() {
            if let Some(i) = index_of(self.record, f) {
                face_of.entry(i).or_insert(k);
            }
        }
        let face_index =
            |o: &Object| index_of(self.record, o).and_then(|i| face_of.get(&i).copied());
        let mut solid = Solid::default();
        let mut edge_ids: Vec<usize> = Vec::new();
        let mut edge_of: HashMap<usize, usize> = HashMap::new();
        for f in &faces {
            solid.faces.push(Face {
                surface: self
                    .obj(f, "m_pSurf")
                    .map_or(Surface::Other("none".into()), |s| self.surface(s)),
                loops: Vec::new(),
                material: self
                    .get(f, "m_renderStyleId")
                    .and_then(Value::as_id)
                    .filter(|id| *id > 0),
            });
        }
        for (fi, f) in faces.iter().enumerate() {
            let mut lp = self.obj(f, "m_pFirstLoop");
            let mut guard = 0;
            let mut seen: HashSet<usize> = HashSet::new();
            while let Some(l) = lp {
                guard += 1;
                if guard > 10_000 {
                    return Err("a face's loops do not end".into());
                }
                if let Some(i) = index_of(self.record, l)
                    && !seen.insert(i)
                {
                    self.note_once(
                        3,
                        "a face's loop names a loop of the face again: its loops end there".into(),
                        notes,
                    );
                    break;
                }
                let first = self
                    .obj(l, "m_next")
                    .ok_or("a loop without its first edge")?;
                let mut uses: Vec<(usize, usize)> = Vec::new();
                let mut e = first;
                // The edge before (the loop's last for its first).
                let mut prev = self.obj(l, "m_prev").and_then(|p| index_of(self.record, p));
                let mut start: Option<(usize, usize)> = None;
                loop {
                    let ei = index_of(self.record, e).ok_or("an edge outside the record")?;
                    let sides: Vec<usize> = self
                        .list(e, "m_pFace")
                        .iter()
                        .map(|v| self.record.deref(v).and_then(face_index))
                        .enumerate()
                        .filter(|(_, f)| *f == Some(fi))
                        .map(|(s, _)| s)
                        .collect();
                    // A seam has both sides on this face: the side whose
                    // previous edge is the one it was entered from.
                    let prev_of = |s: usize| {
                        self.list(e, "m_prev")
                            .get(s)
                            .and_then(|v| self.record.deref(v))
                            .and_then(|p| index_of(self.record, p))
                    };
                    let side = match sides.as_slice() {
                        [s] => *s,
                        [a, b] => {
                            if prev.is_some() && prev_of(*b) == prev && prev_of(*a) != prev {
                                *b
                            } else {
                                *a
                            }
                        }
                        _ => return Err("an edge of a loop that is not on its face".into()),
                    };
                    if start == Some((ei, side)) {
                        break;
                    }
                    if start.is_none() {
                        start = Some((ei, side));
                    }
                    let k = *edge_of.entry(ei).or_insert_with(|| {
                        edge_ids.push(ei);
                        edge_ids.len() - 1
                    });
                    uses.push((k, side));
                    *steps += 1;
                    if *steps > budget {
                        return Err(format!(
                            "its loops take more than {budget} edge steps \
                             (two per object of the record)"
                        ));
                    }
                    if uses.len() > 100_000 {
                        return Err("a loop that does not close".into());
                    }
                    let next = self
                        .list(e, "m_next")
                        .get(side)
                        .and_then(|v| self.record.deref(v))
                        .ok_or("an edge without its next")?;
                    // The loop's last edge names the loop as its next.
                    if self.class(next) != "Edge" {
                        break;
                    }
                    prev = Some(ei);
                    e = next;
                }
                solid.faces[fi].loops.push(uses);
                lp = self.obj(l, "m_nextLoop");
            }
        }
        for ei in edge_ids {
            let e = &self.record.pointees[ei];
            let ends = self.uv_pairs(self.list(e, "m_firstAndLastEdgePnts"));
            if ends.len() != 2 {
                return Err("an edge without its two end points".into());
            }
            let inner = self.uv_pairs(self.list(e, "m_interiorEdgePnts"));
            let mut sides: [Vec<Uv>; 2] = [Vec::new(), Vec::new()];
            for (s, side) in sides.iter_mut().enumerate() {
                side.push(ends[0][s]);
                side.extend(inner.iter().map(|p| p[s]));
                side.push(ends[1][s]);
            }
            let fs: Vec<Option<usize>> = self
                .list(e, "m_pFace")
                .iter()
                .map(|v| self.record.deref(v).and_then(face_index))
                .collect();
            if fs.len() != 2 {
                return Err("an edge without its two sides".into());
            }
            solid.edges.push(Edge {
                faces: [fs[0], fs[1]],
                points: sides,
            });
        }
        Ok(solid)
    }

    fn transform(&self, info: &Object) -> Option<[f64; 12]> {
        let trf = self.get(info, "m_Trf")?.as_object()?;
        let mut m = self.get(trf, "m_3x3")?.floats();
        m.extend(self.get(trf, "m_or")?.floats());
        (m.len() == 12).then(|| std::array::from_fn(|i| m[i]))
    }

    /// Whether a `GFilter`'s view conditions (`m_oConditions`, all of
    /// which must hold, [`condition_in_3d`]) hold in a 3D view at the
    /// reader's detail level.
    fn shown_in_3d(&self, filter: &Object) -> bool {
        self.list(filter, "m_oConditions").iter().all(|c| {
            let Some(c) = self.record.deref(c) else {
                return true;
            };
            let int = |name| self.get(c, name).and_then(Value::as_u64);
            let negated = self
                .get(c, "m_negateDirCondition")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            condition_in_3d(
                self.class(c),
                negated,
                [int("m_param"), int("m_comp"), int("m_value")],
                self.detail,
            )
        })
    }

    fn node(&self, n: &'a Object, out: &mut GRep, depth: usize) {
        self.node_shown(n, out, depth, true);
    }

    /// A node's geometry; under a filter not shown in 3D (`shown` false)
    /// only its curves (a profile's lines), not its bodies or placed types.
    /// A node on its own path (a pointer back to itself or an ancestor)
    /// is left out, as is everything after [`MAX_NODES`], each with a
    /// note once per walk.
    fn node_shown(&self, n: &'a Object, out: &mut GRep, depth: usize, shown: bool) {
        let pointee = index_of(self.record, n);
        let refused = {
            let mut visit = self.visit.borrow_mut();
            if depth > 64 {
                Some((0, "nodes nest too deep".to_string()))
            } else if pointee.is_some_and(|i| visit.path.contains(&i)) {
                Some((1, "a node holds itself: left out there".to_string()))
            } else if visit.read >= MAX_NODES {
                Some((
                    2,
                    format!("more than {MAX_NODES} nodes and faces: the rest are left out"),
                ))
            } else {
                visit.read += 1;
                if self.class(n) == "Geometry" && shown {
                    visit.read += self.list(n, "m_pFaces").len();
                }
                visit.path.extend(pointee);
                None
            }
        };
        if let Some((which, note)) = refused {
            self.note_once(which, note, &mut out.notes);
            return;
        }
        self.node_read(n, out, depth, shown);
        if pointee.is_some() {
            self.visit.borrow_mut().path.pop();
        }
    }

    /// A node's geometry by its class ([`Reader::node_shown`]).
    fn node_read(&self, n: &'a Object, out: &mut GRep, depth: usize, shown: bool) {
        match self.class(n) {
            "Geometry" | "GInstance" if !shown => out.hidden += 1,
            "Geometry" => {
                if self.list(n, "m_pFaces").is_empty() {
                    return;
                }
                // The edge steps the walk has left (mitcad#942): past
                // them, this and the rest of the bodies are left out.
                let left = self
                    .max_edge_steps
                    .saturating_sub(self.visit.borrow().edge_steps);
                let walk_note = || {
                    format!(
                        "more than {} edge steps in the bodies: the rest are left out",
                        self.max_edge_steps
                    )
                };
                if left == 0 {
                    self.note_once(4, walk_note(), &mut out.notes);
                    return;
                }
                let mut steps = 0;
                let read = self.solid(n, left, &mut steps, &mut out.notes);
                let total = {
                    let mut visit = self.visit.borrow_mut();
                    visit.edge_steps = visit.edge_steps.saturating_add(steps);
                    visit.edge_steps
                };
                match read {
                    Ok(s) if !s.faces.is_empty() => out.solids.push(s),
                    Ok(_) => {}
                    Err(_) if total > self.max_edge_steps => {
                        self.note_once(4, walk_note(), &mut out.notes);
                    }
                    Err(e) => out.notes.push(format!("a body did not read: {e}")),
                }
            }
            "GInstance" => {
                let info = self.obj(n, "m_instanceInfo");
                let transform = info.and_then(|i| self.transform(i));
                let Some(transform) = transform else {
                    out.notes.push("a placed type without its transform".into());
                    return;
                };
                let embedded = self.obj(n, "m_oEmbeddedSymbolGRep").map(|e| {
                    let mut g = GRep::default();
                    self.node(e, &mut g, depth + 1);
                    Box::new(g)
                });
                out.instances.push(Placed {
                    symbol: info
                        .and_then(|i| self.get(i, "m_symbolId"))
                        .and_then(Value::as_id)
                        .filter(|id| *id > 0),
                    transform,
                    embedded,
                });
            }
            "GLine" | "GArc" | "GEllipse" => {
                if let Some(c) = self.curve(n) {
                    out.curves.push(c);
                }
            }
            class => {
                let shown = shown && (class != "GFilter" || self.shown_in_3d(n));
                for s in self.list(n, "m_subNodes") {
                    if let Some(s) = self.record.deref(s) {
                        self.node_shown(s, out, depth + 1, shown);
                    }
                }
            }
        }
    }
}

/// Whether a view condition of a `GFilter` holds in a 3D view at a detail
/// level (1 coarse, 2 medium, 3 fine): a view direction along an axis
/// (`GConditionDir`) or a cut (`GConditionCut`) does not hold there, its
/// negation (`m_negateDirCondition`) does; the detail level
/// (`GConditionInt` of `m_param` 3) is compared with `m_value` by `m_comp`
/// (0 equal, 3 not equal). Other conditions hold. *(seen: the geometry
/// under a direction or cut condition is the plan's, the elevations' and
/// the cut's symbolic faces, the 3D bodies are under their negations; the
/// detail levels' conditions pair up as a representation and its
/// alternative)*
pub fn condition_in_3d(
    class: &str,
    negated: bool,
    [param, comp, value]: [Option<u64>; 3],
    detail: u64,
) -> bool {
    match class {
        "GConditionDir" | "GConditionCut" => negated,
        "GConditionInt" => match (param, comp, value) {
            (Some(3), Some(0), Some(v)) => v == detail,
            (Some(3), Some(3), Some(v)) => v != detail,
            _ => true,
        },
        _ => true,
    }
}

/// The B-reps of the `Geometry` objects a record's pointers point at (a
/// family type's void geometry, `FamilySymbol.m_complexCutGeoms`,
/// mitcad#345), and what did not read.
pub fn solids(schema: &Schema, record: &Record, values: &[Value]) -> (Vec<Solid>, Vec<String>) {
    let r = Reader {
        schema,
        record,
        detail: 3,
        visit: RefCell::default(),
        max_edge_steps: MAX_EDGE_STEPS,
    };
    let mut out = GRep::default();
    for v in values {
        if let Some(o) = record.deref(v) {
            r.node(o, &mut out, 0);
        }
    }
    (out.solids, out.notes)
}

/// An element's geometry from its `GElement` record, as a 3D view shows it
/// at the fine detail level; a type that shows nothing there, at the
/// medium or else the coarse level (*seen*: families whose bodies are
/// shown at the coarse level only, the level of a project's 3D views).
pub fn read(schema: &Schema, record: &Record) -> Option<GRep> {
    if schema.class_name(record.root.class) != "GElement" {
        return None;
    }
    let mut out = GRep::default();
    for detail in [3, 2, 1] {
        let r = Reader {
            schema,
            record,
            detail,
            visit: RefCell::default(),
            max_edge_steps: MAX_EDGE_STEPS,
        };
        out = GRep::default();
        let b = r.get(&record.root, "m_bBox").map(Value::floats);
        out.bbox = b
            .filter(|b| b.len() == 6 && b.iter().all(|v| v.is_finite()))
            .map(|b| [b[0], b[1], b[2], b[3], b[4], b[5]]);
        r.node(&record.root, &mut out, 0);
        if !out.is_empty() || out.hidden == 0 {
            break;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::condition_in_3d;
    use crate::schema::types::*;
    use crate::schema::{Schema, SchemaWriter};
    use crate::walk::{Record, Value, walk};

    /// A record holding a list of `GGroup`s (as a family type holds its
    /// void geometry), the groups' sub-nodes by object number: `subs[k]`
    /// is group k + 1's (numbered from 1), the holder's list naming group
    /// 1.
    fn groups(subs: &[Vec<u32>]) -> (Schema, Record) {
        let mut w = SchemaWriter::new();
        let identifier = w.class("Identifier", None, &[("m_id64", word(U64))]);
        w.class("ElementId", None, &[("m_id", object(identifier))]);
        let group = w.class("GGroup", None, &[("m_subNodes", word(0x510e))]);
        let holder = w.class("Holder", None, &[("m_complexCutGeoms", word(0x510e))]);
        let schema = Schema::parse(&w.finish()).unwrap();
        let mut data = holder.to_le_bytes().to_vec();
        let pointers = |out: &mut Vec<u8>, numbers: &[u32]| {
            out.extend((numbers.len() as u32).to_le_bytes());
            for n in numbers {
                out.extend(n.to_le_bytes());
                out.extend(group.to_le_bytes());
            }
        };
        pointers(&mut data, &[1]);
        for sub in subs {
            pointers(&mut data, sub);
        }
        let record = walk(&schema, &data).unwrap();
        (schema, record)
    }

    #[test]
    fn a_node_that_holds_itself_ends_with_one_note() {
        // mitcad#924: a GGroup whose sub-nodes are itself, twice.
        let (schema, record) = groups(&[vec![1, 1]]);
        let list = record
            .root
            .get(&schema, "m_complexCutGeoms")
            .unwrap()
            .as_list()
            .unwrap()
            .to_vec();
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let (solids, notes) = super::solids(&schema, &record, &list);
            best = best.min(started.elapsed());
            assert!(solids.is_empty());
            assert_eq!(notes.len(), 1, "{notes:?}");
            assert!(notes[0].contains("holds itself"), "{notes:?}");
        }
        assert!(best.as_millis() < 10, "{best:?}");
    }

    #[test]
    fn shared_nodes_stop_at_the_node_limit() {
        // Thirty groups, each naming the next twice: 2^30 paths without
        // a cycle; the walk stops at the limit with one note.
        let mut subs: Vec<Vec<u32>> = (2..=30).map(|k| vec![k, k]).collect();
        subs.push(Vec::new());
        let (schema, record) = groups(&subs);
        let list = record
            .root
            .get(&schema, "m_complexCutGeoms")
            .unwrap()
            .as_list()
            .unwrap()
            .to_vec();
        let (_, notes) = super::solids(&schema, &record, &list);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("left out"), "{notes:?}");
        // A tree of shared nodes within the limit reads without a note.
        let mut subs: Vec<Vec<u32>> = (2..=10).map(|k| vec![k, k]).collect();
        subs.push(Vec::new());
        let (schema, record) = groups(&subs);
        let list = record
            .root
            .get(&schema, "m_complexCutGeoms")
            .unwrap()
            .as_list()
            .unwrap()
            .to_vec();
        assert!(super::solids(&schema, &record, &list).1.is_empty());
    }

    /// A record holding a list that names one `Geometry` `bodies` times
    /// (mitcad#942): one face whose `loops` loops (each the previous one's
    /// `m_nextLoop`; the last names itself when `self_named`) all name the
    /// same closed cycle of `edges` edges, each edge's other side open.
    fn body(edges: u32, loops: u32, self_named: bool, bodies: u32) -> (Schema, Record) {
        fn put(out: &mut Vec<u8>, word: u32) {
            out.extend(word.to_le_bytes());
        }
        fn pointer(out: &mut Vec<u8>, number: u32, tag: u16) {
            put(out, number);
            out.extend(tag.to_le_bytes());
        }
        // Reference pointers (kind 3), alone and in containers.
        let reference = word(0x030e);
        let references = word(0x530e);
        let pointers = word(0x510e);
        let mut w = SchemaWriter::new();
        let identifier = w.class("Identifier", None, &[("m_id64", word(U64))]);
        w.class("ElementId", None, &[("m_id", object(identifier))]);
        let pnt = w.class("EdgePnt", None, &[("uv", matrix(2, 2))]);
        let edge = w.class(
            "Edge",
            None,
            &[
                ("m_pFace", references.clone()),
                ("m_next", references.clone()),
                ("m_prev", references),
                ("m_firstAndLastEdgePnts", pointers.clone()),
            ],
        );
        let edge_loop = w.class(
            "EdgeLoop",
            None,
            &[
                ("m_next", reference.clone()),
                ("m_prev", reference),
                ("m_nextLoop", word(OWNED)),
            ],
        );
        let face = w.class(
            "Face",
            None,
            &[("m_pSurf", word(OWNED)), ("m_pFirstLoop", word(OWNED))],
        );
        let geometry = w.class(
            "Geometry",
            None,
            &[
                ("m_pFaces", pointers.clone()),
                ("m_pEdges", pointers.clone()),
            ],
        );
        let holder = w.class("Holder", None, &[("m_complexCutGeoms", pointers)]);
        let schema = Schema::parse(&w.finish()).unwrap();
        // Object numbers, each range apart from the others (a loop
        // numbered as an edge would be read as that edge).
        let (body_no, face_no, first_pnt, edge0) = (1, 2, 4, 1000);
        let loop0 = edge0 + edges;
        // The objects in the walk's order: the holder, the body, its face
        // and edges, the first loop, the two end points (shared by all
        // edges), the further loops.
        let mut data = holder.to_le_bytes().to_vec();
        put(&mut data, bodies);
        for _ in 0..bodies {
            pointer(&mut data, body_no, geometry);
        }
        put(&mut data, 1);
        pointer(&mut data, face_no, face);
        put(&mut data, edges);
        for i in 0..edges {
            pointer(&mut data, edge0 + i, edge);
        }
        put(&mut data, 0);
        pointer(&mut data, loop0, edge_loop);
        for i in 0..edges {
            for (n, other) in [
                (2, face_no),
                (2, edge0 + (i + 1) % edges),
                (2, edge0 + (i + edges - 1) % edges),
            ] {
                put(&mut data, n);
                put(&mut data, other);
                put(&mut data, 0);
            }
            put(&mut data, 2);
            pointer(&mut data, first_pnt, pnt);
            pointer(&mut data, first_pnt + 1, pnt);
        }
        let a_loop = |data: &mut Vec<u8>, k: u32| {
            put(data, edge0);
            put(data, edge0 + edges - 1);
            if k + 1 < loops {
                pointer(data, loop0 + k + 1, edge_loop);
            } else if self_named {
                pointer(data, loop0 + k, edge_loop);
            } else {
                put(data, 0);
            }
        };
        a_loop(&mut data, 0);
        for uv in [[0.0, 0.0, 1.0, 0.0], [1.0, 0.0, 1.0, 1.0]] {
            for x in uv {
                data.extend(f64::to_le_bytes(x));
            }
        }
        for k in 1..loops {
            a_loop(&mut data, k);
        }
        let record = walk(&schema, &data).unwrap();
        (schema, record)
    }

    fn listed(schema: &Schema, record: &Record) -> Vec<Value> {
        record
            .root
            .get(schema, "m_complexCutGeoms")
            .unwrap()
            .as_list()
            .unwrap()
            .to_vec()
    }

    #[test]
    fn a_face_loop_that_names_itself_ends_with_one_note() {
        // mitcad#942: the face's loop is its own next loop, its edges a
        // closed cycle of 100,000; read once, the loops end there.
        let (schema, record) = body(100_000, 1, true, 1);
        let list = listed(&schema, &record);
        let started = std::time::Instant::now();
        let (solids, notes) = super::solids(&schema, &record, &list);
        let took = started.elapsed();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(
            notes[0].contains("names a loop of the face again"),
            "{notes:?}"
        );
        assert_eq!(solids.len(), 1);
        assert_eq!(solids[0].faces[0].loops.len(), 1);
        assert_eq!(solids[0].faces[0].loops[0].len(), 100_000);
        assert_eq!(solids[0].edges.len(), 100_000);
        assert!(took.as_millis() < 500, "{took:?}");
        // The same face whose loop ends with a null: no note.
        let (schema, record) = body(1_000, 1, false, 1);
        let (solids, notes) = super::solids(&schema, &record, &listed(&schema, &record));
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(solids[0].faces[0].loops[0].len(), 1_000);
    }

    #[test]
    fn a_body_takes_two_edge_steps_per_object_at_most() {
        // 10,000 loops, each naming the same 100,000-edge cycle: 10^9 steps
        // unbounded; the body fails past twice the record's objects.
        let (schema, record) = body(100_000, 10_000, false, 1);
        let list = listed(&schema, &record);
        let started = std::time::Instant::now();
        let (solids, notes) = super::solids(&schema, &record, &list);
        let took = started.elapsed();
        assert!(solids.is_empty());
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("edge steps"), "{notes:?}");
        assert!(took.as_millis() < 500, "{took:?}");
    }

    #[test]
    fn a_walk_stops_at_its_edge_steps() {
        // A body of 1,000 edges named five times, a walk of 2,500 steps:
        // two bodies read, the rest left out with one note.
        let (schema, record) = body(1_000, 1, false, 5);
        let list = listed(&schema, &record);
        let r = super::Reader {
            schema: &schema,
            record: &record,
            detail: 3,
            visit: Default::default(),
            max_edge_steps: 2_500,
        };
        let mut out = super::GRep::default();
        for v in &list {
            r.node(record.deref(v).unwrap(), &mut out, 0);
        }
        assert_eq!(out.solids.len(), 2);
        assert_eq!(out.notes.len(), 1, "{:?}", out.notes);
        assert!(out.notes[0].contains("left out"), "{:?}", out.notes);
        // Within the walk's default steps all five read.
        let (solids, notes) = super::solids(&schema, &record, &list);
        assert!(notes.is_empty(), "{notes:?}");
        assert_eq!(solids.len(), 5);
    }

    #[test]
    fn a_3d_view_shows_what_is_not_only_for_plans_cuts_or_other_detail_levels() {
        // The plan's faces and the cut's: not in 3D; their negations: in 3D.
        assert!(!condition_in_3d("GConditionDir", false, [None; 3], 3));
        assert!(condition_in_3d("GConditionDir", true, [None; 3], 3));
        assert!(!condition_in_3d("GConditionCut", false, [None; 3], 3));
        assert!(condition_in_3d("GConditionCut", true, [None; 3], 3));
        // Detail levels: "fine only" and "coarse and medium" at fine and at
        // coarse.
        let fine_only = [Some(3), Some(0), Some(3)];
        let not_fine = [Some(3), Some(3), Some(3)];
        assert!(condition_in_3d("GConditionInt", false, fine_only, 3));
        assert!(!condition_in_3d("GConditionInt", false, not_fine, 3));
        assert!(!condition_in_3d("GConditionInt", false, fine_only, 1));
        assert!(condition_in_3d("GConditionInt", false, not_fine, 1));
        // Other parameters and classes hold.
        assert!(condition_in_3d(
            "GConditionInt",
            false,
            [Some(6), Some(0), Some(1)],
            3
        ));
        assert!(condition_in_3d(
            "GConditionSomethingElse",
            false,
            [None; 3],
            3
        ));
    }
}
