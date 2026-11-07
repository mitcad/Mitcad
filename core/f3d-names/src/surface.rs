// SPDX-License-Identifier: MIT
//! A point inside a face of any surface type (mitcad#161): feature inputs
//! that are spline faces (a thread's flank as the guide surface of a
//! sweep) are found in the replay by a point inside them, as planar and
//! cylindrical faces are.
//!
//! - Free-form surfaces (B-splines, the ruled surfaces of thread flanks):
//!   the face's edges are taken into the surface's parameters (each edge
//!   point at the parameters of the nearest point of a grid over the
//!   surface, refined by Newton steps), and the point is the one inside
//!   that outline farthest from it ([`super::point_inside`] in the
//!   parameter plane), on the surface. On a ruled surface the point lies
//!   on one of the rulings the surface was built from, where the surface
//!   is exact.
//!   Extruded curves the same way (u along the curve, v along the
//!   direction as far as the edges reach; mitcad#228).
//! - Surfaces of revolution of a curve: half way along the curve between
//!   the edge points (each at the curve's point nearest it in the plane
//!   through the axis), on the far side of the widest gap round the axis
//!   (mitcad#228).
//! - Cones, spheres and tori: as for cylinders, half way between the
//!   lowest and highest edge points (along the axis, in latitude, round
//!   the tube) on the far side of the widest gap between the edges' angles
//!   round the axis.

use mitcad_asm::brep::{self, Curve, P3, Surface};

/// A point inside a face and the face's normal there (with the face's
/// orientation), in the body's coordinates (its transform not applied).
#[derive(Clone, Debug, PartialEq)]
pub struct Inside {
    pub point: P3,
    pub normal: P3,
    /// More points inside a free-form face, spread over it (on lines
    /// across its outline in the surface's parameters), by which the
    /// import finds every piece of it where the replay splits the face
    /// (a thread's flank, one face over many turns in the file, one per
    /// turn in the replay). Empty for other faces.
    pub spread: Vec<P3>,
}

/// A point inside the face of `surface` bounded by `edges` (points along
/// each edge, in order); `reversed`: the face's normal runs against the
/// surface's.
pub fn point_inside_face(
    surface: &Surface,
    reversed: bool,
    edges: &[&[P3]],
) -> Result<Inside, String> {
    if edges.iter().all(|e| e.is_empty()) {
        return Err("the face has no edges".to_owned());
    }
    let inside = match surface {
        Surface::BSpline(_) | Surface::Ruled { .. } | Surface::Extrusion { .. } => {
            free_form(surface, edges)?
        }
        Surface::Revolution {
            curve,
            origin,
            axis,
        } => revolution(surface, curve, *origin, *axis, edges)?,
        Surface::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
        } => revolved(*origin, *axis, *ref_dir, edges, |angle, h| {
            // `h`: along the axis.
            let radial = round(*axis, *ref_dir, angle);
            let (s, c) = half_angle.sin_cos();
            let point = brep::add(
                brep::add(*origin, brep::scale(*axis, h)),
                brep::scale(radial, radius + h * half_angle.tan()),
            );
            (
                point,
                brep::sub(brep::scale(radial, c), brep::scale(*axis, s)),
            )
        }),
        Surface::Sphere {
            center,
            axis,
            ref_dir,
            radius,
        } => {
            // Latitudes instead of heights.
            let latitude = |p: P3| {
                let d = brep::sub(p, *center);
                (brep::dot(d, *axis) / brep::norm(d).max(1e-300))
                    .clamp(-1.0, 1.0)
                    .asin()
            };
            let lats: Vec<f64> = edges
                .iter()
                .flat_map(|e| e.iter())
                .map(|&p| latitude(p))
                .collect();
            let (low, high) = span(&lats);
            let angle = far_angle(*center, *axis, *ref_dir, edges);
            let lat = 0.5 * (low + high);
            let radial = round(*axis, *ref_dir, angle);
            let normal = brep::add(
                brep::scale(radial, lat.cos()),
                brep::scale(*axis, lat.sin()),
            );
            Inside {
                point: brep::add(*center, brep::scale(normal, *radius)),
                normal,
                spread: Vec::new(),
            }
        }
        Surface::Torus {
            center,
            axis,
            ref_dir,
            major,
            minor,
        } => {
            let angle = far_angle(*center, *axis, *ref_dir, edges);
            let radial = round(*axis, *ref_dir, angle);
            // Round the tube, in the half-plane of `angle`: the widest gap
            // between the edges' tube angles, as round the axis.
            let tube = |p: P3| {
                let d = brep::sub(p, *center);
                let h = brep::dot(d, *axis);
                let rho = brep::norm(brep::sub(d, brep::scale(*axis, h)));
                h.atan2(rho - major)
            };
            let angles: Vec<f64> = edges
                .iter()
                .flat_map(|e| e.iter())
                .map(|&p| tube(p))
                .collect();
            let t = gap_angle(angles);
            let normal = brep::add(brep::scale(radial, t.cos()), brep::scale(*axis, t.sin()));
            Inside {
                point: brep::add(
                    brep::add(*center, brep::scale(radial, *major)),
                    brep::scale(normal, *minor),
                ),
                normal,
                spread: Vec::new(),
            }
        }
        other => {
            return Err(format!(
                "no point inside a face of a {} decoded",
                other.kind()
            ));
        }
    };
    let mut inside = inside;
    if reversed {
        inside.normal = brep::scale(inside.normal, -1.0);
    }
    Ok(inside)
}

/// The unit direction at `angle` round `axis` from `ref_dir`.
fn round(axis: P3, ref_dir: P3, angle: f64) -> P3 {
    let y = brep::cross(axis, ref_dir);
    brep::add(
        brep::scale(ref_dir, angle.cos()),
        brep::scale(y, angle.sin()),
    )
}

fn span(values: &[f64]) -> (f64, f64) {
    values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &x| {
            (l.min(x), h.max(x))
        })
}

/// The angle in the middle of a face round an axis: on the far side of the
/// widest gap between the edge points' angles where that gap is wide (a
/// part of a turn), else in the gap (a whole turn: any angle).
fn gap_angle(mut angles: Vec<f64>) -> f64 {
    if angles.is_empty() {
        return 0.0;
    }
    angles.sort_by(f64::total_cmp);
    let tau = std::f64::consts::TAU;
    let last = angles[angles.len() - 1];
    let mut gap = angles[0] + tau - last;
    let mut at = last + gap / 2.0;
    for w in angles.windows(2) {
        if w[1] - w[0] > gap {
            gap = w[1] - w[0];
            at = (w[0] + w[1]) / 2.0;
        }
    }
    if gap > std::f64::consts::FRAC_PI_6 {
        at += std::f64::consts::PI;
    }
    at
}

/// [`gap_angle`] of the edge points round an axis through `origin`.
fn far_angle(origin: P3, axis: P3, ref_dir: P3, edges: &[&[P3]]) -> f64 {
    let y = brep::cross(axis, ref_dir);
    let angles = edges
        .iter()
        .flat_map(|e| e.iter())
        .map(|&p| {
            let d = brep::sub(p, origin);
            brep::dot(d, y).atan2(brep::dot(d, ref_dir))
        })
        .collect();
    gap_angle(angles)
}

/// A face of a surface of revolution about `axis` through `origin`: half
/// way along the axis between the edge points, at [`far_angle`]; `at`
/// gives the point and the surface's normal at an angle and a height.
fn revolved(
    origin: P3,
    axis: P3,
    ref_dir: P3,
    edges: &[&[P3]],
    at: impl Fn(f64, f64) -> (P3, P3),
) -> Inside {
    let heights: Vec<f64> = edges
        .iter()
        .flat_map(|e| e.iter())
        .map(|&p| brep::dot(brep::sub(p, origin), axis))
        .collect();
    let (low, high) = span(&heights);
    let (point, normal) = at(far_angle(origin, axis, ref_dir, edges), 0.5 * (low + high));
    Inside {
        point,
        normal,
        spread: Vec::new(),
    }
}

/// The parameters of a surface's curve a face can use: a bounded curve's
/// own, a whole turn of an ellipse, and of a line those of the edge points
/// projected onto it, widened by the edges' extent.
fn curve_range(curve: &Curve, edges: &[&[P3]]) -> Option<(f64, f64)> {
    match curve {
        Curve::Ellipse { .. } => Some((0.0, std::f64::consts::TAU)),
        Curve::Line { origin, dir } => {
            let d2 = brep::dot(*dir, *dir);
            if d2 <= 0.0 {
                return None;
            }
            let points: Vec<P3> = edges.iter().flat_map(|e| e.iter().copied()).collect();
            let ts: Vec<f64> = points
                .iter()
                .map(|&p| brep::dot(brep::sub(p, *origin), *dir) / d2)
                .collect();
            let (lo, hi) = span(&ts);
            let extent = (0..3)
                .map(|k| {
                    let (a, b) = span(&points.iter().map(|p| p[k]).collect::<Vec<_>>());
                    b - a
                })
                .fold(0.0, f64::max);
            let pad = extent / d2.sqrt();
            (lo.is_finite() && hi.is_finite()).then_some((lo - pad, hi + pad))
        }
        _ => curve.range(),
    }
}

/// A face of a surface of revolution of a curve (mitcad#228): half way
/// along the curve between the edge points (each at the curve's point
/// nearest it in the plane through the axis), at [`far_angle`].
fn revolution(
    surface: &Surface,
    curve: &Curve,
    origin: P3,
    axis: P3,
    edges: &[&[P3]],
) -> Result<Inside, String> {
    let range = curve_range(curve, edges).ok_or("a revolution of an unbounded curve")?;
    let k = brep::normalize(axis);
    // (distance from the axis, height along it)
    let meridian = |p: P3| {
        let d = brep::sub(p, origin);
        let h = brep::dot(d, k);
        (brep::norm(brep::sub(d, brep::scale(k, h))), h)
    };
    let n = 512;
    let vs: Vec<f64> = (0..=n)
        .map(|i| range.0 + (range.1 - range.0) * i as f64 / n as f64)
        .collect();
    let on_curve: Vec<(f64, f64)> = vs.iter().map(|&v| meridian(curve.eval(v))).collect();
    let at: Vec<f64> = edges
        .iter()
        .flat_map(|e| e.iter())
        .map(|&p| {
            let (r, h) = meridian(p);
            let d = |i: usize| (on_curve[i].0 - r).hypot(on_curve[i].1 - h);
            vs[(0..vs.len())
                .min_by(|&a, &b| d(a).total_cmp(&d(b)))
                .unwrap_or(0)]
        })
        .collect();
    let (low, high) = span(&at);
    let v = 0.5 * (low + high);
    let d = brep::sub(curve.eval(v), origin);
    let radial = brep::sub(d, brep::scale(k, brep::dot(d, k)));
    if brep::norm(radial) < 1e-12 {
        return Err("the face's middle lies on the axis of revolution".to_owned());
    }
    let ref_dir = brep::normalize(radial);
    let angle = far_angle(origin, k, ref_dir, edges).rem_euclid(std::f64::consts::TAU);
    let patch = Patch::new(surface, edges)?;
    let point = patch.eval(angle, v);
    let (du, dv) = patch.derivatives(angle, v);
    let normal = brep::normalize(brep::cross(du, dv));
    if !point.iter().chain(&normal).all(|x| x.is_finite()) {
        return Err("no point inside the face found".to_owned());
    }
    Ok(Inside {
        point,
        normal,
        spread: Vec::new(),
    })
}

/// A free-form surface's parameters: its domain, the grid of parameters
/// edge points are first matched to, and its points.
struct Patch<'a> {
    surface: &'a Surface,
    u: (f64, f64),
    v: (f64, f64),
    /// Grid parameters in u and v.
    us: Vec<f64>,
    vs: Vec<f64>,
    periodic: [bool; 2],
}

/// A ruled surface's point at `u` between its curves' points.
fn ruled(a: P3, b: P3, u: f64) -> P3 {
    brep::add(brep::scale(a, 1.0 - u), brep::scale(b, u))
}

/// An extrusion's point `v` along `dir` from its curve's point.
fn extruded(c: P3, dir: P3, v: f64) -> P3 {
    brep::add(c, brep::scale(dir, v))
}

/// A surface of revolution's point: its curve's point turned by `u` round
/// the axis (Rodrigues).
fn turned(c: P3, origin: P3, axis: P3, u: f64) -> P3 {
    let k = brep::normalize(axis);
    let p = brep::sub(c, origin);
    let (s, c) = u.sin_cos();
    let turned = brep::add(
        brep::add(brep::scale(p, c), brep::scale(brep::cross(k, p), s)),
        brep::scale(k, brep::dot(k, p) * (1.0 - c)),
    );
    brep::add(origin, turned)
}

impl<'a> Patch<'a> {
    fn new(surface: &'a Surface, edges: &[&[P3]]) -> Result<Self, String> {
        let even = |(a, b): (f64, f64), n: usize| -> Vec<f64> {
            (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect()
        };
        match surface {
            Surface::BSpline(b) => {
                let (u, v) = (b.u_range(), b.v_range());
                // A few grid lines per knot span (thread flanks run along
                // many spans).
                let nu = (4 * b.u_knots.len()).clamp(32, 2000);
                let nv = (4 * b.v_knots.len()).clamp(32, 2000);
                Ok(Patch {
                    surface,
                    u,
                    v,
                    us: even(u, nu),
                    vs: even(v, nv),
                    periodic: [b.u_periodic, b.v_periodic],
                })
            }
            Surface::Ruled { from, to } => {
                let range = from
                    .range()
                    .filter(|_| to.range() == from.range())
                    .ok_or("a ruled surface of unbounded curves")?;
                // The curves' own parameters: the rulings the surface was
                // built from, where it is exact.
                let vs = match from {
                    Curve::Interpolated { params, .. } => params.clone(),
                    // A thread's flank between two helices: four grid lines
                    // a span of their B-splines (48 a turn).
                    Curve::Helix(h) => even(range, (4 * h.spans).clamp(32, 20_000)),
                    _ => even(range, 512),
                };
                Ok(Patch {
                    surface,
                    u: (0.0, 1.0),
                    v: range,
                    us: even((0.0, 1.0), 32),
                    vs,
                    periodic: [false, false],
                })
            }
            // Extrusions and revolutions (mitcad#228: faces with appearances
            // of their own): u along the curve or round the axis, v along
            // the direction (as far as the edges reach) or the curve.
            Surface::Extrusion { curve, dir } => {
                let u = curve_range(curve, edges).ok_or("an extrusion of an unbounded curve")?;
                let us = even(u, 256);
                let d2 = brep::dot(*dir, *dir);
                if d2 <= 0.0 {
                    return Err("an extrusion without a direction".to_owned());
                }
                let along: Vec<f64> = us
                    .iter()
                    .map(|&t| brep::dot(curve.eval(t), *dir) / d2)
                    .collect();
                let at: Vec<f64> = edges
                    .iter()
                    .flat_map(|e| e.iter())
                    .map(|&p| brep::dot(p, *dir) / d2)
                    .collect();
                let (cl, ch) = span(&along);
                let (pl, ph) = span(&at);
                let v = (pl - ch, ph - cl);
                if v.1.partial_cmp(&v.0) != Some(std::cmp::Ordering::Greater) {
                    return Err("the face's edges do not reach along the extrusion".to_owned());
                }
                let closed = brep::dist(curve.eval(u.0), curve.eval(u.1))
                    <= 1e-9 * (1.0 + brep::norm(curve.eval(u.0)));
                Ok(Patch {
                    surface,
                    u,
                    v,
                    us,
                    vs: even(v, 64),
                    periodic: [closed, false],
                })
            }
            Surface::Revolution { curve, .. } => {
                let v = curve_range(curve, edges).ok_or("a revolution of an unbounded curve")?;
                let u = (0.0, std::f64::consts::TAU);
                Ok(Patch {
                    surface,
                    u,
                    v,
                    us: even(u, 128),
                    vs: even(v, 128),
                    periodic: [true, false],
                })
            }
            other => Err(format!("{} is not a free-form surface", other.kind())),
        }
    }

    fn eval(&self, u: f64, v: f64) -> P3 {
        match self.surface {
            Surface::BSpline(b) => b.eval(u, v),
            Surface::Ruled { from, to } => ruled(from.eval(v), to.eval(v), u),
            Surface::Extrusion { curve, dir } => extruded(curve.eval(u), *dir, v),
            Surface::Revolution {
                curve,
                origin,
                axis,
            } => turned(curve.eval(v), *origin, *axis, u),
            _ => [f64::NAN; 3],
        }
    }

    /// The points of the grid, `us` outside, `vs` inside: [`Patch::eval`]
    /// at each, with each curve point the grid lines share taken once
    /// (mitcad#311: evaluating a curve costs as much as the rest).
    fn grid(&self) -> Vec<P3> {
        match self.surface {
            Surface::Ruled { from, to } => {
                let at: Vec<(P3, P3)> = self
                    .vs
                    .iter()
                    .map(|&v| (from.eval(v), to.eval(v)))
                    .collect();
                self.us
                    .iter()
                    .flat_map(|&u| at.iter().map(move |&(a, b)| ruled(a, b, u)))
                    .collect()
            }
            Surface::Extrusion { curve, dir } => self
                .us
                .iter()
                .flat_map(|&u| {
                    let c = curve.eval(u);
                    self.vs.iter().map(move |&v| extruded(c, *dir, v))
                })
                .collect(),
            Surface::Revolution {
                curve,
                origin,
                axis,
            } => {
                let at: Vec<P3> = self.vs.iter().map(|&v| curve.eval(v)).collect();
                self.us
                    .iter()
                    .flat_map(|&u| at.iter().map(move |&c| turned(c, *origin, *axis, u)))
                    .collect()
            }
            _ => self
                .us
                .iter()
                .flat_map(|&u| self.vs.iter().map(move |&v| (u, v)))
                .map(|(u, v)| self.eval(u, v))
                .collect(),
        }
    }

    /// The partial derivatives at (u, v), by differences inside the
    /// domain.
    fn derivatives(&self, u: f64, v: f64) -> (P3, P3) {
        let hu = (self.u.1 - self.u.0) * 1e-6;
        let hv = (self.v.1 - self.v.0) * 1e-6;
        let (u0, u1) = ((u - hu).max(self.u.0), (u + hu).min(self.u.1));
        let (v0, v1) = ((v - hv).max(self.v.0), (v + hv).min(self.v.1));
        let du = brep::scale(
            brep::sub(self.eval(u1, v), self.eval(u0, v)),
            1.0 / (u1 - u0).max(1e-300),
        );
        let dv = brep::scale(
            brep::sub(self.eval(u, v1), self.eval(u, v0)),
            1.0 / (v1 - v0).max(1e-300),
        );
        (du, dv)
    }

    /// The parameters of the surface point nearest `p`, from the grid
    /// point nearest it.
    fn invert(&self, grid: &Nearest<'_>, p: P3) -> (f64, f64) {
        let nv = self.vs.len();
        let k = grid.nearest(p);
        let (mut u, mut v) = (self.us[k / nv], self.vs[k % nv]);
        // Newton steps on the distance (Gauss-Newton: the normal
        // equations of the tangent plane).
        for _ in 0..8 {
            let s = self.eval(u, v);
            let (du, dv) = self.derivatives(u, v);
            let d = brep::sub(p, s);
            let (a, b, c) = (brep::dot(du, du), brep::dot(du, dv), brep::dot(dv, dv));
            let det = a * c - b * b;
            if det.abs() < 1e-300 {
                break;
            }
            let (ru, rv) = (brep::dot(d, du), brep::dot(d, dv));
            let step_u = (c * ru - b * rv) / det;
            let step_v = (a * rv - b * ru) / det;
            u = (u + step_u).clamp(self.u.0, self.u.1);
            v = (v + step_v).clamp(self.v.0, self.v.1);
            if step_u.abs() <= 1e-12 * (self.u.1 - self.u.0)
                && step_v.abs() <= 1e-12 * (self.v.1 - self.v.0)
            {
                break;
            }
        }
        (u, v)
    }
}

/// The points of a [`Patch`]'s grid in cubic cells, for the grid point
/// nearest a point (mitcad#311). The same one as a search through every
/// grid point, the first of several equally near: a B-spline face's grid
/// has up to 2000 × 2000 points, and searching all of them for each of its
/// edge points took most of the time of reading a design with appearances
/// on such faces.
struct Nearest<'g> {
    grid: &'g [P3],
    /// The lowest corner of the cells and their edge.
    low: P3,
    size: f64,
    /// The cells along each axis.
    dims: [i64; 3],
    /// (cell, grid index) of the finite grid points, sorted.
    cells: Vec<(u64, u32)>,
}

/// The rings of cells searched round a point before [`Nearest`] goes
/// through every grid point instead (a point far from the surface).
const NEAREST_RINGS: i64 = 48;

impl<'g> Nearest<'g> {
    fn new(grid: &'g [P3]) -> Self {
        let finite = |p: &P3| p.iter().all(|c| c.is_finite());
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for p in grid.iter().filter(|p| finite(p)) {
            for k in 0..3 {
                low[k] = low[k].min(p[k]);
                high[k] = high[k].max(p[k]);
            }
        }
        let mut nearest = Nearest {
            grid,
            low,
            size: 0.0,
            dims: [0; 3],
            cells: Vec::new(),
        };
        let count = grid.iter().filter(|p| finite(p)).count();
        if count == 0 || grid.len() > u32::MAX as usize {
            return nearest;
        }
        // Cells about as wide as the grid's spacing: the grid lies on a
        // surface, so its points spread over an area, not a volume.
        let extent = (0..3).map(|k| high[k] - low[k]).fold(0.0, f64::max);
        let mut size = (extent / (count as f64).sqrt()).max(extent * 1e-9);
        if size <= 0.0 || !size.is_finite() {
            // All the points in one place.
            size = 1.0;
        }
        let dims = |size: f64| -> [i64; 3] {
            std::array::from_fn(|k| ((high[k] - low[k]) / size).floor() as i64 + 1)
        };
        // At most 2^21 cells along an axis, so that a cell's number fits.
        while dims(size).iter().any(|&d| d > 1 << 21) {
            size *= 2.0;
        }
        nearest.size = size;
        nearest.dims = dims(size);
        let mut cells: Vec<(u64, u32)> = grid
            .iter()
            .enumerate()
            .filter(|(_, p)| finite(p))
            .map(|(i, &p)| (nearest.key(nearest.cell(p)), i as u32))
            .collect();
        cells.sort_unstable();
        nearest.cells = cells;
        nearest
    }

    /// The cell of a point, clamped to the cells.
    fn cell(&self, p: P3) -> [i64; 3] {
        std::array::from_fn(|k| {
            (((p[k] - self.low[k]) / self.size).floor() as i64).clamp(0, self.dims[k] - 1)
        })
    }

    fn key(&self, c: [i64; 3]) -> u64 {
        ((c[0] as u64) << 42) | ((c[1] as u64) << 21) | c[2] as u64
    }

    /// Every grid point, the first of the nearest.
    fn nearest_of_all(&self, p: P3) -> usize {
        (0..self.grid.len())
            .min_by(|&a, &b| brep::dist(self.grid[a], p).total_cmp(&brep::dist(self.grid[b], p)))
            .unwrap_or(0)
    }

    /// The index of the grid point nearest `p` (the first of several
    /// equally near).
    fn nearest(&self, p: P3) -> usize {
        if self.cells.is_empty() || !p.iter().all(|c| c.is_finite()) {
            return self.nearest_of_all(p);
        }
        let c = self.cell(p);
        // (distance, index)
        let mut best: Option<(f64, usize)> = None;
        for r in 0..=NEAREST_RINGS {
            // The cells r apart from the point's (in the largest of the
            // three directions).
            let range = |k: usize| (c[k] - r).max(0)..=(c[k] + r).min(self.dims[k] - 1);
            for x in range(0) {
                for y in range(1) {
                    let on_ring = (x - c[0]).abs() == r || (y - c[1]).abs() == r;
                    for z in range(2) {
                        if !on_ring && (z - c[2]).abs() != r {
                            continue;
                        }
                        let key = self.key([x, y, z]);
                        let start = self.cells.partition_point(|&(k, _)| k < key);
                        for &(k, i) in &self.cells[start..] {
                            if k != key {
                                break;
                            }
                            let i = i as usize;
                            let d = brep::dist(self.grid[i], p);
                            let better = match best {
                                None => true,
                                Some((bd, bi)) => match d.total_cmp(&bd) {
                                    std::cmp::Ordering::Less => true,
                                    std::cmp::Ordering::Equal => i < bi,
                                    std::cmp::Ordering::Greater => false,
                                },
                            };
                            if better {
                                best = Some((d, i));
                            }
                        }
                    }
                }
            }
            // Every point in a farther ring is at least r cells away (the
            // point lies in its own cell or beyond the cells on its side;
            // one cell less for rounding at the cells' walls).
            if let Some((d, i)) = best
                && d < (r as f64 - 1.0) * self.size
            {
                return i;
            }
            if (0..3).all(|k| c[k] - r <= 0 && c[k] + r >= self.dims[k] - 1) {
                // Every cell seen; the grid's points that are not finite
                // come after any finite one.
                if let Some((_, i)) = best {
                    return i;
                }
                break;
            }
        }
        self.nearest_of_all(p)
    }
}

/// [`point_inside_face`] of a free-form surface: the point inside the
/// face's outline in the surface's parameters farthest from it.
fn free_form(surface: &Surface, edges: &[&[P3]]) -> Result<Inside, String> {
    let patch = Patch::new(surface, edges)?;
    let grid = patch.grid();
    let grid = Nearest::new(&grid);
    let mut flat: Vec<Vec<[f64; 2]>> = edges
        .iter()
        .map(|e| {
            e.iter()
                .map(|&p| {
                    let (u, v) = patch.invert(&grid, p);
                    [u, v]
                })
                .collect()
        })
        .collect();
    // A periodic direction: its seam where the outline leaves its widest
    // gap.
    for (k, periodic) in patch.periodic.iter().enumerate() {
        if !periodic {
            continue;
        }
        let (a, b) = if k == 0 { patch.u } else { patch.v };
        let period = b - a;
        let mut values: Vec<f64> = flat.iter().flatten().map(|p| p[k]).collect();
        values.sort_by(f64::total_cmp);
        let mut cut = a;
        let mut gap = values.first().map_or(0.0, |f| f - a) + values.last().map_or(0.0, |l| b - l);
        for w in values.windows(2) {
            if w[1] - w[0] > gap {
                gap = w[1] - w[0];
                cut = 0.5 * (w[0] + w[1]);
            }
        }
        for p in flat.iter_mut().flatten() {
            if p[k] < cut {
                p[k] += period;
            }
        }
    }
    // The parameter plane with both directions scaled to the outline's
    // extent, so that the point keeps clear of the edges both ways.
    let all: Vec<[f64; 2]> = flat.iter().flatten().copied().collect();
    let (ul, uh) = span(&all.iter().map(|p| p[0]).collect::<Vec<_>>());
    let (vl, vh) = span(&all.iter().map(|p| p[1]).collect::<Vec<_>>());
    if !(uh > ul && vh > vl) {
        return Err("the face's edges do not enclose any of its surface".to_owned());
    }
    let to_plane = |p: [f64; 2]| [(p[0] - ul) / (uh - ul), (p[1] - vl) / (vh - vl), 0.0];
    let lines: Vec<Vec<P3>> = flat
        .iter()
        .map(|e| e.iter().map(|&p| to_plane(p)).collect())
        .collect();
    let line_refs: Vec<&[P3]> = lines.iter().map(Vec::as_slice).collect();
    let mean = all.iter().fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
    let n = all.len() as f64;
    let guess = to_plane([mean[0] / n, mean[1] / n]);
    let q = super::point_inside([0.0, 0.0, 1.0], &line_refs, guess);
    let wrap = |x: f64, (a, b): (f64, f64), periodic: bool| {
        if periodic && x > b {
            x - (b - a)
        } else {
            x.clamp(a, b)
        }
    };
    // A point of the parameter plane on the surface: on a ruled surface on
    // the nearest ruling it was built from.
    let on_surface = |x: f64, y: f64| {
        let u = wrap(ul + x * (uh - ul), patch.u, patch.periodic[0]);
        let mut v = wrap(vl + y * (vh - vl), patch.v, patch.periodic[1]);
        if matches!(surface, Surface::Ruled { .. })
            && let Some(&near) = patch
                .vs
                .iter()
                .min_by(|a, b| (*a - v).abs().total_cmp(&(*b - v).abs()))
        {
            v = near;
        }
        (u, v)
    };
    let (u, v) = on_surface(q[0], q[1]);
    let point = patch.eval(u, v);
    let (du, dv) = patch.derivatives(u, v);
    let normal = brep::normalize(brep::cross(du, dv));
    if !point.iter().chain(&normal).all(|x| x.is_finite()) {
        return Err("no point inside the face found".to_owned());
    }
    // More points: the middles of the stretches inside on lines across
    // the outline at evenly spaced heights in each direction.
    let segments: Vec<([f64; 2], [f64; 2])> = lines
        .iter()
        .flat_map(|e| {
            e.windows(2)
                .map(|w| ([w[0][0], w[0][1]], [w[1][0], w[1][1]]))
        })
        .filter(|(a, b)| a != b)
        .collect();
    let mut spread = Vec::new();
    for axis in 0..2 {
        let other = 1 - axis;
        for k in 0..SPREAD_LINES {
            let at = (k as f64 + 0.5) / SPREAD_LINES as f64;
            let mut xs: Vec<f64> = segments
                .iter()
                .filter(|(a, b)| (a[axis] <= at) != (b[axis] <= at))
                .map(|(a, b)| {
                    a[other] + (at - a[axis]) / (b[axis] - a[axis]) * (b[other] - a[other])
                })
                .collect();
            if !xs.len().is_multiple_of(2) {
                continue;
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks(2) {
                let mid = 0.5 * (pair[0] + pair[1]);
                let (x, y) = if axis == 0 { (at, mid) } else { (mid, at) };
                let (u, v) = on_surface(x, y);
                let p = patch.eval(u, v);
                if p.iter().all(|c| c.is_finite()) {
                    spread.push(p);
                }
            }
        }
    }
    Ok(Inside {
        point,
        normal,
        spread,
    })
}

/// Lines across a free-form face's outline in each direction of its
/// parameters, for [`Inside::spread`].
const SPREAD_LINES: usize = 8;

#[cfg(test)]
mod tests {
    use super::*;

    /// A strip of a ruled surface between two helices (a thread's flank):
    /// the point lies between the helices, half way along the strip.
    #[test]
    fn a_point_inside_a_ruled_flank() {
        let helix = |r: f64, z: f64, t: f64| [r * t.cos(), r * t.sin(), z + t];
        let params: Vec<f64> = (0..=96).map(|i| -0.2 + 12.97 * i as f64 / 96.0).collect();
        let curve = |r: f64, z: f64| Curve::Interpolated {
            params: params.clone(),
            points: params.iter().map(|&t| helix(r, z, t)).collect(),
        };
        let surface = Surface::Ruled {
            from: curve(10.0, 0.0),
            to: curve(12.0, 1.0),
        };
        // The face: from 0 to 4 turns of the parameter, between u 0.1 and
        // 0.9 (two helices and two rulings).
        let at = |u: f64, t: f64| {
            let (a, b) = (helix(10.0, 0.0, t), helix(12.0, 1.0, t));
            brep::add(brep::scale(a, 1.0 - u), brep::scale(b, u))
        };
        let end = 4.0 * std::f64::consts::PI;
        let along =
            |u: f64| -> Vec<P3> { (0..=64).map(|i| at(u, end * i as f64 / 64.0)).collect() };
        let across = |t: f64| -> Vec<P3> { (0..=8).map(|i| at(0.1 + 0.1 * i as f64, t)).collect() };
        let (low, high, start, stop) = (along(0.1), along(0.9), across(0.0), across(end));
        let edges: Vec<&[P3]> = vec![&low, &stop, &high, &start];
        let inside = point_inside_face(&surface, false, &edges).unwrap();
        let p = inside.point;
        // On the surface (a ruling) between the helices.
        let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
        assert!(r > 10.5 && r < 11.5, "{p:?}");
        let t = p[1].atan2(p[0]).rem_euclid(std::f64::consts::TAU);
        let u = (r - 10.0) / 2.0;
        let turns = (p[2] - u) / std::f64::consts::TAU;
        assert!((turns - 1.0).abs() < 0.1, "{p:?} {turns}");
        assert!(brep::dist(p, at(u, p[2] - u)) < 1e-9, "{p:?} {t}");
        // The normal is across the strip.
        let (du, dv) = (
            brep::sub(at(u + 1e-6, p[2] - u), at(u, p[2] - u)),
            brep::sub(at(u, p[2] - u + 1e-6), at(u, p[2] - u)),
        );
        let n = brep::normalize(brep::cross(du, dv));
        assert!(brep::dot(n, inside.normal) > 0.999, "{:?}", inside.normal);
    }

    /// A cone's face from a quarter turn: the point lies in it, half way
    /// up, with the cone's outward normal.
    /// A face of an extruded wave (u 1 to 3 along the curve, 0 to 5 along
    /// z) and a band of a revolved slanted line (a full turn): the points
    /// lie inside them, on the surfaces (mitcad#228).
    #[test]
    fn points_inside_extruded_and_revolved_faces() {
        let params: Vec<f64> = (0..=40).map(|i| i as f64 / 10.0).collect();
        let wave = Curve::Interpolated {
            params: params.clone(),
            points: params.iter().map(|&t| [t, t.sin(), 0.0]).collect(),
        };
        let surface = Surface::Extrusion {
            curve: wave,
            dir: [0.0, 0.0, 1.0],
        };
        let along = |z: f64| -> Vec<P3> {
            (0..=20)
                .map(|i| 1.0 + 2.0 * i as f64 / 20.0)
                .map(|t| [t, t.sin(), z])
                .collect()
        };
        let side = |t: f64| -> Vec<P3> { (0..=10).map(|i| [t, t.sin(), i as f64 / 2.0]).collect() };
        let edges = [along(0.0), side(3.0), along(5.0), side(1.0)];
        let refs: Vec<&[P3]> = edges.iter().map(Vec::as_slice).collect();
        let inside = point_inside_face(&surface, false, &refs).unwrap();
        let [x, y, z] = inside.point;
        assert!((y - x.sin()).abs() < 1e-6, "{:?}", inside.point);
        assert!(
            (1.2..2.8).contains(&x) && (0.5..4.5).contains(&z),
            "{:?}",
            inside.point
        );

        let line = Curve::Interpolated {
            params: vec![0.0, 1.0, 2.0],
            points: vec![[5.0, 0.0, 0.0], [6.0, 0.0, 1.0], [7.0, 0.0, 2.0]],
        };
        let surface = Surface::Revolution {
            curve: line,
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
        };
        let circle = |r: f64, z: f64| -> Vec<P3> {
            (0..=36)
                .map(|i| i as f64 * std::f64::consts::TAU / 36.0)
                .map(|a| [r * a.cos(), r * a.sin(), z])
                .collect()
        };
        let edges = [circle(5.0, 0.0), circle(7.0, 2.0)];
        let refs: Vec<&[P3]> = edges.iter().map(Vec::as_slice).collect();
        let inside = point_inside_face(&surface, false, &refs).unwrap();
        let [x, y, z] = inside.point;
        let r = x.hypot(y);
        assert!((r - (5.0 + z)).abs() < 1e-6, "{:?}", inside.point);
        assert!((0.2..1.8).contains(&z), "{:?}", inside.point);
    }

    /// The cells find the grid point a search through all of them finds
    /// (the first of equally near ones), for points on, near and far from
    /// a curved grid, a flat one and one with points that are not finite
    /// (mitcad#311).
    #[test]
    fn the_nearest_grid_point_is_that_of_a_full_search() {
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut random = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let wave: Vec<P3> = (0..60)
            .flat_map(|i| (0..40).map(move |j| (i as f64 / 7.0, j as f64 / 5.0)))
            .map(|(u, v)| [u, v, (u * 1.3).sin() * v.cos()])
            .collect();
        let flat: Vec<P3> = (0..30)
            .flat_map(|i| (0..30).map(move |j| [i as f64 * 0.5, j as f64 * 0.5, 0.0]))
            .collect();
        let mut holes = wave.clone();
        for k in (0..holes.len()).step_by(7) {
            holes[k] = [f64::NAN; 3];
        }
        let doubled: Vec<P3> = flat.iter().chain(flat.iter()).copied().collect();
        for grid in [&wave, &flat, &holes, &doubled] {
            let nearest = Nearest::new(grid);
            let mut points: Vec<P3> = grid.iter().step_by(13).copied().collect();
            for _ in 0..300 {
                let scale = [1.0, 10.0, 1000.0][points.len() % 3];
                points.push([
                    (random() - 0.3) * scale,
                    (random() - 0.3) * scale,
                    (random() - 0.5) * scale,
                ]);
            }
            // Points half way between grid points (ties).
            points.push([0.25, 0.25, 0.0]);
            points.push([1.0, 1.25, 0.0]);
            for p in points {
                assert_eq!(nearest.nearest(p), nearest.nearest_of_all(p), "{p:?}");
            }
        }
    }

    /// The grid built with each curve point once is the grid of the
    /// surface's points, bit for bit (mitcad#311).
    #[test]
    fn a_patch_grid_is_its_points() {
        let params: Vec<f64> = (0..=40).map(|i| i as f64 / 10.0).collect();
        let wave = |z: f64| Curve::Interpolated {
            params: params.clone(),
            points: params.iter().map(|&t| [t, t.sin(), z + 0.1 * t]).collect(),
        };
        let edges = [vec![
            [1.0, 0.8, 0.0],
            [3.0, 0.1, 0.0],
            [3.0, 0.1, 2.0],
            [1.0, 0.8, 2.0],
        ]];
        let refs: Vec<&[P3]> = edges.iter().map(Vec::as_slice).collect();
        for surface in [
            Surface::Extrusion {
                curve: wave(0.0),
                dir: [0.0, 0.3, 1.0],
            },
            Surface::Revolution {
                curve: wave(0.0),
                origin: [0.0, -2.0, 0.0],
                axis: [0.0, 0.0, 1.0],
            },
            Surface::Ruled {
                from: wave(0.0),
                to: wave(1.0),
            },
        ] {
            let patch = Patch::new(&surface, &refs).unwrap();
            let each: Vec<P3> = patch
                .us
                .iter()
                .flat_map(|&u| patch.vs.iter().map(move |&v| (u, v)))
                .map(|(u, v)| patch.eval(u, v))
                .collect();
            let grid = patch.grid();
            assert_eq!(grid.len(), each.len());
            for (a, b) in grid.iter().zip(&each) {
                assert!(
                    a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits()),
                    "{a:?} {b:?}"
                );
            }
        }
    }

    #[test]
    fn a_point_inside_a_conical_face() {
        let half: f64 = 0.3;
        let surface = Surface::Cone {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 5.0,
            half_angle: half,
        };
        let at = |a: f64, h: f64| {
            let r = 5.0 + h * half.tan();
            [r * a.cos(), r * a.sin(), h]
        };
        let q = std::f64::consts::FRAC_PI_2;
        let arc = |h: f64| -> Vec<P3> { (0..=16).map(|i| at(q * i as f64 / 16.0, h)).collect() };
        let line = |a: f64| -> Vec<P3> { (0..=4).map(|i| at(a, 2.0 * i as f64)).collect() };
        let (bottom, top, left, right) = (arc(0.0), arc(8.0), line(0.0), line(q));
        let edges: Vec<&[P3]> = vec![&bottom, &right, &top, &left];
        let inside = point_inside_face(&surface, false, &edges).unwrap();
        let expected = at(q / 2.0, 4.0);
        assert!(
            brep::dist(inside.point, expected) < 1e-9,
            "{:?}",
            inside.point
        );
        assert!(
            inside.normal[2] < 0.0 && inside.normal[0] > 0.0,
            "{:?}",
            inside.normal
        );
    }
}
