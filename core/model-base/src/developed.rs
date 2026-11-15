// SPDX-License-Identifier: MIT
//! Sketches on cylindrical and conical faces (mitcad#1768): the face's
//! development, the map between a sketch's flat coordinates and the face,
//! and the sketch's curves as curves on the face.
//!
//! A sketch on such a face keeps its curves in the face's developed
//! coordinates: on a cylinder, the arc length around it at its radius and
//! the distance along its axis; on a cone, its development (the cone
//! unrolled into a flat fan about its apex, lengths and angles kept). The
//! flat coordinates are laid on a *carrier* plane parallel to the axis
//! ([`Development::frame`]), exactly as an emboss's wrap takes a sketch
//! plane (`EmbossWrapSpec`): a carrier point at `s` across the axis and `t`
//! along it goes to the angle `s / radius` about the axis at `t` (a
//! cylinder), or to the distance `hypot(s, t + ρ0)` from the apex along the
//! generator turned by `atan2(s, t + ρ0) / sin(half_angle)` (a cone, ρ0
//! along the generator from the apex to the carrier origin's circle). The
//! wrap of profiles, the curves laid on the face and the projection of
//! edges into the sketch all use this one map, so the kernel's wrap of a
//! sketch's profiles lands where the sketch's curves are drawn.

use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

use crate::curves::{bspline_point, conic_point, helix_point};
use crate::kernel::Curve3;
use crate::profile::SketchFrame;

const TAU: f64 = std::f64::consts::TAU;

/// The kind of a developed face and its size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DevelopedSurface {
    /// A cylinder of this radius about the axis.
    Cylinder { radius: f64 },
    /// A cone of this half angle (radians, strictly between 0 and π/2)
    /// whose apex is the [`Development::origin`].
    Cone { half_angle: f64 },
}

/// A cylindrical or conical face as a sketch's support (mitcad#1768): the
/// surface, its axis and the carrier plane of the sketch's flat
/// coordinates. Worked out again from the face at every evaluation, so the
/// sketch follows the face's radius, length and place.
#[derive(Debug, Clone, PartialEq)]
pub struct Development {
    pub surface: DevelopedSurface,
    /// A point of a cylinder's axis, or a cone's apex.
    pub origin: [f64; 3],
    /// A unit vector along the axis; a cone's points from the apex where
    /// its radius grows.
    pub axis: [f64; 3],
    /// The carrier plane, parallel to the axis: a sketch point `p` is the
    /// carrier point `frame.point(p)`, wrapped onto the surface. Its normal
    /// is the face's outward normal at the sketch's origin, so that the
    /// sketch is seen from outside the material.
    pub frame: SketchFrame,
    /// The material lies outside the surface (a bore's wall, a
    /// countersink): the face's outward normal points to the axis.
    pub internal: bool,
}

/// The wrap's own axes (as the kernel's emboss takes them): `x` the radial
/// direction from the axis towards the carrier plane, `y = axis × x`, the
/// centre on the axis at the carrier origin's height (a cylinder's at the
/// axis origin), and a cone's ρ0.
#[derive(Debug, Clone, Copy)]
struct Wrap {
    centre: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    rho0: f64,
}

impl Development {
    /// A cylinder's development (see [`Development::new`]).
    pub fn cylinder(axis_point: [f64; 3], axis: [f64; 3], radius: f64, internal: bool) -> Self {
        Self {
            surface: DevelopedSurface::Cylinder { radius },
            origin: axis_point,
            axis: unit(axis).unwrap_or(axis),
            frame: SketchFrame::XY,
            internal,
        }
    }

    /// The development of the surface with the sketch's origin on it at
    /// `start` along the axis (from `origin`) and the angle `angle` about
    /// it from `reference` (a direction across the axis, turned
    /// right-handed about `up`), the sketch's y axis along `up` (the axis or
    /// against it) and its x axis to the right as the face is seen from
    /// outside the material. None for an axis or reference of no length,
    /// a cone whose apex is at the origin's height or `up` across the axis.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        surface: DevelopedSurface,
        origin: [f64; 3],
        axis: [f64; 3],
        internal: bool,
        start: f64,
        reference: [f64; 3],
        angle: f64,
        up: [f64; 3],
    ) -> Option<Self> {
        let axis = unit(axis)?;
        let up = unit(up)?;
        if dot(up, axis).abs() < 1.0 - 1e-9 {
            return None;
        }
        let across = unit(sub(reference, scale(axis, dot(reference, axis))))?;
        let radial = unit(crate::datum::rotate(across, up, angle))?;
        let height = start;
        let radius = match surface {
            DevelopedSurface::Cylinder { radius } => radius,
            DevelopedSurface::Cone { half_angle } => height * half_angle.tan(),
        };
        if !(radius.is_finite() && radius > 0.0) {
            return None;
        }
        let at = add(add(origin, scale(axis, height)), scale(radial, radius));
        let outward = if internal {
            scale(radial, -1.0)
        } else {
            radial
        };
        // x = y × normal: the normal x × y is then the outward normal.
        let x_axis = cross(up, outward);
        Some(Self {
            surface,
            origin,
            axis,
            frame: SketchFrame {
                origin: at,
                x_axis,
                y_axis: up,
            },
            internal,
        })
    }

    /// The radius at the carrier origin (a cylinder's radius).
    pub fn radius(&self) -> f64 {
        match self.surface {
            DevelopedSurface::Cylinder { radius } => radius,
            DevelopedSurface::Cone { half_angle } => {
                dot(sub(self.frame.origin, self.origin), self.axis) * half_angle.tan()
            }
        }
    }

    /// A cone's half angle; 0 for a cylinder.
    pub fn half_angle(&self) -> f64 {
        match self.surface {
            DevelopedSurface::Cylinder { .. } => 0.0,
            DevelopedSurface::Cone { half_angle } => half_angle,
        }
    }

    fn wrap(&self) -> Wrap {
        let a = self.axis;
        let centre = match self.surface {
            DevelopedSurface::Cylinder { .. } => self.origin,
            DevelopedSurface::Cone { .. } => {
                let height = dot(sub(self.frame.origin, self.origin), a);
                add(self.origin, scale(a, height))
            }
        };
        let mut n = self.frame.normal();
        if dot(sub(self.frame.origin, centre), n) < 0.0 {
            n = scale(n, -1.0);
        }
        let x = unit(sub(n, scale(a, dot(a, n)))).unwrap_or(n);
        let y = cross(a, x);
        let rho0 = match self.surface {
            DevelopedSurface::Cylinder { .. } => 0.0,
            DevelopedSurface::Cone { half_angle } => {
                dot(sub(self.frame.origin, self.origin), a) / half_angle.cos()
            }
        };
        Wrap { centre, x, y, rho0 }
    }

    /// The carrier point's wrap coordinates: across the axis and along it.
    fn flat(&self, w: &Wrap, p: [f64; 2]) -> [f64; 2] {
        let d = sub(self.frame.point(p), w.centre);
        [dot(d, w.y), dot(d, self.axis)]
    }

    /// The sketch point whose carrier point has these wrap coordinates.
    fn sketch_point(&self, w: &Wrap, [s, t]: [f64; 2]) -> [f64; 2] {
        let f = &self.frame;
        let o = sub(f.origin, w.centre);
        let (a11, a12, b1) = (dot(f.x_axis, w.y), dot(f.y_axis, w.y), s - dot(o, w.y));
        let (a21, a22, b2) = (
            dot(f.x_axis, self.axis),
            dot(f.y_axis, self.axis),
            t - dot(o, self.axis),
        );
        let det = a11 * a22 - a12 * a21;
        if det.abs() < 1e-12 {
            return [0.0, 0.0];
        }
        [(b1 * a22 - a12 * b2) / det, (a11 * b2 - a21 * b1) / det]
    }

    /// The point of the face a sketch point goes to.
    pub fn point(&self, p: [f64; 2]) -> [f64; 3] {
        let w = self.wrap();
        let [s, t] = self.flat(&w, p);
        self.wrapped(&w, s, t)
    }

    fn wrapped(&self, w: &Wrap, s: f64, t: f64) -> [f64; 3] {
        match self.surface {
            DevelopedSurface::Cylinder { radius } => {
                let angle = s / radius;
                let around = add(scale(w.x, angle.cos()), scale(w.y, angle.sin()));
                add(add(self.origin, scale(self.axis, t)), scale(around, radius))
            }
            DevelopedSurface::Cone { half_angle } => {
                let along = t + w.rho0;
                let rho = s.hypot(along);
                let theta = s.atan2(along) / half_angle.sin();
                let around = add(scale(w.x, theta.cos()), scale(w.y, theta.sin()));
                let generator = add(
                    scale(self.axis, half_angle.cos()),
                    scale(around, half_angle.sin()),
                );
                add(self.origin, scale(generator, rho))
            }
        }
    }

    /// The unit normal of the surface at a sketch point, away from the
    /// axis (a cone's tilted towards its apex).
    pub fn normal(&self, p: [f64; 2]) -> [f64; 3] {
        let w = self.wrap();
        let [s, t] = self.flat(&w, p);
        match self.surface {
            DevelopedSurface::Cylinder { radius } => {
                let angle = s / radius;
                add(scale(w.x, angle.cos()), scale(w.y, angle.sin()))
            }
            DevelopedSurface::Cone { half_angle } => {
                let theta = s.atan2(t + w.rho0) / half_angle.sin();
                let around = add(scale(w.x, theta.cos()), scale(w.y, theta.sin()));
                add(
                    scale(around, half_angle.cos()),
                    scale(self.axis, -half_angle.sin()),
                )
            }
        }
    }

    /// The face's outward normal (out of the material) at a sketch point.
    pub fn outward(&self, p: [f64; 2]) -> [f64; 3] {
        let n = self.normal(p);
        if self.internal { scale(n, -1.0) } else { n }
    }

    /// The sketch point of the face's point nearest to `point`, on the turn
    /// of the face nearest to `near` (a sketch point; the sketch's origin
    /// when None): the angle about the axis is taken within half a turn of
    /// `near`'s. None for a point on the axis (a cylinder's) or a cone's
    /// apex side beyond it.
    pub fn sketch_point_of(&self, point: [f64; 3], near: Option<[f64; 2]>) -> Option<[f64; 2]> {
        let w = self.wrap();
        let d = sub(point, self.origin);
        let height = dot(d, self.axis);
        let radial = sub(d, scale(self.axis, height));
        if norm(radial) < 1e-12 {
            return None;
        }
        let mut phi = dot(radial, w.y).atan2(dot(radial, w.x));
        let near_phi = near.map(|q| {
            let [s, t] = self.flat(&w, q);
            match self.surface {
                DevelopedSurface::Cylinder { radius } => s / radius,
                DevelopedSurface::Cone { half_angle } => s.atan2(t + w.rho0) / half_angle.sin(),
            }
        });
        if let Some(target) = near_phi {
            phi += TAU * ((target - phi) / TAU).round();
        }
        let flat = match self.surface {
            DevelopedSurface::Cylinder { radius } => [radius * phi, height],
            DevelopedSurface::Cone { half_angle } => {
                let around = add(scale(w.x, phi.cos()), scale(w.y, phi.sin()));
                let generator = add(
                    scale(self.axis, half_angle.cos()),
                    scale(around, half_angle.sin()),
                );
                let rho = dot(d, generator);
                if rho < 0.0 {
                    return None;
                }
                let psi = phi * half_angle.sin();
                [rho * psi.sin(), rho * psi.cos() - w.rho0]
            }
        };
        Some(self.sketch_point(&w, flat))
    }

    /// Where the ray from `origin` along `direction` meets the (unbounded)
    /// surface ahead of it: the distances along the ray (in units of
    /// `direction`'s length), nearest first, with the sketch points there
    /// (taken nearest to `near`, as [`Development::sketch_point_of`]). A
    /// cone's other nappe, beyond its apex, is not met.
    pub fn ray_hits(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        near: Option<[f64; 2]>,
    ) -> Vec<(f64, [f64; 2])> {
        let a = self.axis;
        let q = sub(origin, self.origin);
        let (qa, da) = (dot(q, a), dot(direction, a));
        let (quad, lin, constant) = match self.surface {
            DevelopedSurface::Cylinder { radius } => {
                let qp = sub(q, scale(a, qa));
                let dp = sub(direction, scale(a, da));
                (
                    dot(dp, dp),
                    2.0 * dot(qp, dp),
                    dot(qp, qp) - radius * radius,
                )
            }
            DevelopedSurface::Cone { half_angle } => {
                let k = 1.0 / (half_angle.cos() * half_angle.cos());
                (
                    dot(direction, direction) - k * da * da,
                    2.0 * (dot(q, direction) - k * qa * da),
                    dot(q, q) - k * qa * qa,
                )
            }
        };
        let scale_of = dot(direction, direction).max(1e-300);
        let mut roots = Vec::new();
        if quad.abs() <= 1e-12 * scale_of {
            if lin.abs() > 1e-300 {
                roots.push(-constant / lin);
            }
        } else {
            let disc = lin * lin - 4.0 * quad * constant;
            if disc >= 0.0 {
                let root = disc.sqrt();
                roots.push((-lin - root) / (2.0 * quad));
                roots.push((-lin + root) / (2.0 * quad));
            }
        }
        roots.retain(|t| t.is_finite() && *t >= 0.0);
        roots.sort_by(f64::total_cmp);
        roots.dedup_by(|x, y| (*x - *y).abs() <= 1e-12);
        roots
            .into_iter()
            .filter_map(|t| {
                let p = add(origin, scale(direction, t));
                if matches!(self.surface, DevelopedSurface::Cone { .. })
                    && dot(sub(p, self.origin), a) < 0.0
                {
                    return None;
                }
                self.sketch_point_of(p, near).map(|uv| (t, uv))
            })
            .collect()
    }

    /// The sketch's straight line from `a` to `b` on the face: a line along
    /// a cylinder's axis or a cone's generator, a circle (an arc) about a
    /// cylinder's axis, a helix on a cylinder; else a curve through its
    /// points (see [`Development::curve`]).
    pub fn line(&self, a: [f64; 2], b: [f64; 2]) -> Curve3 {
        let w = self.wrap();
        let ([sa, ta], [sb, tb]) = (self.flat(&w, a), self.flat(&w, b));
        let size = self.size();
        let tiny = 1e-12 * size;
        match self.surface {
            DevelopedSurface::Cylinder { radius } => {
                let (ds, dt) = (sb - sa, tb - ta);
                if ds.abs() <= tiny {
                    return Curve3::Line {
                        start: self.wrapped(&w, sa, ta),
                        end: self.wrapped(&w, sb, tb),
                    };
                }
                if dt.abs() <= tiny {
                    let (normal, start, end) = if ds > 0.0 {
                        (self.axis, sa / radius, sb / radius)
                    } else {
                        (scale(self.axis, -1.0), -sa / radius, -sb / radius)
                    };
                    return Curve3::Conic {
                        center: add(self.origin, scale(self.axis, ta)),
                        normal,
                        x_axis: w.x,
                        major: radius,
                        minor: radius,
                        start,
                        end,
                        closed: (end - start) >= TAU - 1e-12,
                    };
                }
                let slope = dt / ds;
                let (axis, start, end) = if ds > 0.0 {
                    (self.axis, sa / radius, sb / radius)
                } else {
                    (scale(self.axis, -1.0), -sa / radius, -sb / radius)
                };
                Curve3::Helix {
                    center: add(self.origin, scale(self.axis, ta - slope * sa)),
                    axis,
                    x_axis: w.x,
                    radius,
                    pitch: TAU * radius * slope,
                    start,
                    end,
                }
            }
            DevelopedSurface::Cone { .. } => {
                // Through the apex's image: along a generator.
                let (ua, ub) = ([sa, ta + w.rho0], [sb, tb + w.rho0]);
                let cross2 = ua[0] * ub[1] - ua[1] * ub[0];
                if cross2.abs() <= 1e-12 * size * size && ua[0] * ub[0] + ua[1] * ub[1] >= 0.0 {
                    return Curve3::Line {
                        start: self.wrapped(&w, sa, ta),
                        end: self.wrapped(&w, sb, tb),
                    };
                }
                let f = |k: f64| -> [f64; 2] { std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k) };
                self.curve(&f, 0.0, 1.0)
            }
        }
    }

    /// A sketch arc about the cone's apex image (`center` the sketch point
    /// of the apex) from the angle `start` to `end` (counter-clockwise in
    /// the sketch) at `radius`: a circle about the cone's axis. None for a
    /// cylinder or another centre.
    pub fn arc_about_apex(
        &self,
        center: [f64; 2],
        radius: f64,
        start: f64,
        end: f64,
    ) -> Option<Curve3> {
        let DevelopedSurface::Cone { half_angle } = self.surface else {
            return None;
        };
        let w = self.wrap();
        let [sc, tc] = self.flat(&w, center);
        if sc.abs() > 1e-9 * self.size() || (tc + w.rho0).abs() > 1e-9 * self.size() {
            return None;
        }
        let point = |angle: f64| {
            [
                center[0] + radius * angle.cos(),
                center[1] + radius * angle.sin(),
            ]
        };
        let height = radius * half_angle.cos();
        let ring = radius * half_angle.sin();
        let centre = add(self.origin, scale(self.axis, height));
        // The turn of the arc on the face: the development's angle over
        // sin(half angle), the way a small step along the sketch's arc
        // turns about the axis.
        let sweep = (end - start).abs() / half_angle.sin();
        let p0 = self.point(point(start));
        let ahead = self.point(point(start + 1e-3 * (end - start)));
        let x0 = unit(sub(p0, centre))?;
        let normal = if dot(cross(sub(p0, centre), sub(ahead, centre)), self.axis) >= 0.0 {
            self.axis
        } else {
            scale(self.axis, -1.0)
        };
        Some(Curve3::Conic {
            center: centre,
            normal,
            x_axis: x0,
            major: ring,
            minor: ring,
            start: 0.0,
            end: sweep,
            closed: sweep >= TAU - 1e-12,
        })
    }

    /// A size of the development for tolerances: its radius at the carrier
    /// origin, at least 1 mm.
    fn size(&self) -> f64 {
        self.radius().abs().max(1.0)
    }

    /// The curve on the face through the images of the sketch points
    /// `f(t)`, `t` from `t0` to `t1`: a cubic B-spline of Bézier pieces
    /// through the images of evenly spaced parameters with their tangents,
    /// more pieces until it is within 1e-7 of the size (the radius, at
    /// least 1 mm) of the exact images between them.
    pub fn curve(&self, f: &dyn Fn(f64) -> [f64; 2], t0: f64, t1: f64) -> Curve3 {
        let exact = |t: f64| self.point(f(t));
        fitted(&exact, t0, t1, 1e-7 * self.size())
    }

    /// The development of a 3D curve on (or near) the face, laid on the
    /// carrier plane: each of its points goes to the carrier point of its
    /// sketch point (the angle about the axis followed along the curve from
    /// its start, which is taken on the turn nearest the sketch's origin).
    /// A line along a cylinder's axis or a cone's generator, a circle about
    /// a cylinder's axis and a helix on it are lines; a point is a point;
    /// other curves are curves through their images (as
    /// [`Development::curve`]). None when a point of it lies on the axis.
    pub fn flattened(&self, curve: &Curve3) -> Option<Curve3> {
        if let Curve3::Point(p) = curve {
            let uv = self.sketch_point_of(*p, None)?;
            return Some(Curve3::Point(self.frame.point(uv)));
        }
        let (along, t0, t1) = parametrised(curve)?;
        // The sketch points along it, each angle near the one before.
        const N: usize = 64;
        let mut points = Vec::with_capacity(N + 1);
        let mut near = None;
        for k in 0..=N {
            let t = t0 + (t1 - t0) * k as f64 / N as f64;
            let uv = self.sketch_point_of(along(t), near)?;
            near = Some(uv);
            points.push(uv);
        }
        let (first, last) = (points[0], points[N]);
        let chord = [last[0] - first[0], last[1] - first[1]];
        let length = chord[0].hypot(chord[1]);
        let tolerance = 1e-7 * self.size();
        let straight = length > tolerance
            && points.iter().all(|p| {
                let d = [p[0] - first[0], p[1] - first[1]];
                (d[0] * chord[1] - d[1] * chord[0]).abs() / length <= tolerance
            });
        if straight {
            return Some(Curve3::Line {
                start: self.frame.point(first),
                end: self.frame.point(last),
            });
        }
        // The sketch point of a parameter, on the turn the samples follow.
        let sample = |t: f64| -> [f64; 3] {
            let k = (((t - t0) / (t1 - t0)) * N as f64)
                .round()
                .clamp(0.0, N as f64) as usize;
            let uv = self
                .sketch_point_of(along(t), Some(points[k]))
                .unwrap_or(points[k]);
            self.frame.point(uv)
        };
        Some(fitted(&sample, t0, t1, tolerance))
    }

    /// The length of the curve on the face of the sketch points `f(t)`, `t`
    /// from `t0` to `t1`, summed over chords of its images, more of them
    /// until the sum changes by less than 1e-9 of it.
    pub fn length_on_face(&self, f: &dyn Fn(f64) -> [f64; 2], t0: f64, t1: f64) -> f64 {
        let mut previous = f64::NAN;
        let mut n = 64usize;
        loop {
            let mut sum = 0.0;
            let mut last = self.point(f(t0));
            for k in 1..=n {
                let p = self.point(f(t0 + (t1 - t0) * k as f64 / n as f64));
                sum += norm(sub(p, last));
                last = p;
            }
            if (sum - previous).abs() <= 1e-9 * sum.max(1e-9) || n >= 1 << 16 {
                return sum;
            }
            previous = sum;
            n *= 2;
        }
    }
}

/// A point of the curve at a parameter, and the parameter's range; None
/// for a point or a B-spline whose knots do not fit its poles.
#[allow(clippy::type_complexity)]
pub fn parametrised(curve: &Curve3) -> Option<(Box<dyn Fn(f64) -> [f64; 3] + '_>, f64, f64)> {
    match curve {
        Curve3::Point(_) => None,
        Curve3::Line { start, end } => Some((
            Box::new(move |t: f64| std::array::from_fn(|i| start[i] + (end[i] - start[i]) * t)),
            0.0,
            1.0,
        )),
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            ..
        } => Some((
            Box::new(move |t: f64| conic_point(*center, *normal, *x_axis, (*major, *minor), t)),
            *start,
            *end,
        )),
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return None;
            }
            Some((
                Box::new(move |t: f64| bspline_point(p, poles, weights, knots, t)),
                knots[p],
                knots[poles.len()],
            ))
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => Some((
            Box::new(move |t: f64| helix_point(*center, *axis, *x_axis, (*radius, *pitch), t)),
            *start,
            *end,
        )),
    }
}

/// A cubic B-spline of Bézier pieces through `exact(t)` at evenly spaced
/// parameters from `t0` to `t1`, with its tangents there (by central
/// differences), more pieces (from 8 to 4096) until the pieces are within
/// `tolerance` of `exact` at their quarter points. Its knots are the
/// parameters, so that it runs as `exact` does.
pub fn fitted(exact: &dyn Fn(f64) -> [f64; 3], t0: f64, t1: f64, tolerance: f64) -> Curve3 {
    let span = t1 - t0;
    let h = 1e-6 * span.abs().max(1e-12);
    let tangent = |t: f64| -> [f64; 3] {
        let (a, b) = (exact(t - h), exact(t + h));
        std::array::from_fn(|i| (b[i] - a[i]) / (2.0 * h))
    };
    let mut n = 8usize;
    loop {
        let params: Vec<f64> = (0..=n).map(|k| t0 + span * k as f64 / n as f64).collect();
        let points: Vec<[f64; 3]> = params.iter().map(|t| exact(*t)).collect();
        let tangents: Vec<[f64; 3]> = params.iter().map(|t| tangent(*t)).collect();
        let step = span / n as f64;
        let mut poles = vec![points[0]];
        for k in 0..n {
            let (p0, p3) = (points[k], points[k + 1]);
            let p1 = add(p0, scale(tangents[k], step / 3.0));
            let p2 = sub(p3, scale(tangents[k + 1], step / 3.0));
            poles.extend([p1, p2, p3]);
        }
        let worst = (0..n)
            .flat_map(|k| [0.25, 0.5, 0.75].map(move |f| (k, f)))
            .map(|(k, f)| {
                let piece = [
                    poles[3 * k],
                    poles[3 * k + 1],
                    poles[3 * k + 2],
                    poles[3 * k + 3],
                ];
                let mine = bezier(&piece, f);
                norm(sub(mine, exact(params[k] + step * f)))
            })
            .fold(0.0, f64::max);
        if worst <= tolerance || n >= 4096 {
            let mut knots = vec![t0; 4];
            for p in &params[1..n] {
                knots.extend([*p; 3]);
            }
            knots.extend([t1; 4]);
            return Curve3::BSpline {
                degree: 3,
                poles,
                weights: Vec::new(),
                knots,
            };
        }
        n *= 2;
    }
}

/// A point of a cubic Bézier piece (de Casteljau).
fn bezier(p: &[[f64; 3]; 4], f: f64) -> [f64; 3] {
    let lerp = |a: [f64; 3], b: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
    };
    let (a, b, c) = (lerp(p[0], p[1]), lerp(p[1], p[2]), lerp(p[2], p[3]));
    let (d, e) = (lerp(a, b), lerp(b, c));
    lerp(d, e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near3(a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() <= tolerance)
    }

    fn near2(a: [f64; 2], b: [f64; 2], tolerance: f64) -> bool {
        (0..2).all(|i| (a[i] - b[i]).abs() <= tolerance)
    }

    /// A rod of radius 10 along +Z, the sketch's origin on its +Y side at
    /// z = 5 (the reference +X turned a quarter about +Z).
    fn rod() -> Development {
        Development::new(
            DevelopedSurface::Cylinder { radius: 10.0 },
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            false,
            5.0,
            [1.0, 0.0, 0.0],
            std::f64::consts::FRAC_PI_2,
            [0.0, 0.0, 1.0],
        )
        .unwrap()
    }

    /// A cone with its apex at the origin opening along +Z, half angle
    /// 30 degrees, the sketch's origin at height 10 on its +X side.
    fn cone() -> Development {
        Development::new(
            DevelopedSurface::Cone {
                half_angle: std::f64::consts::FRAC_PI_6,
            },
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            false,
            10.0,
            [1.0, 0.0, 0.0],
            0.0,
            [0.0, 0.0, 1.0],
        )
        .unwrap()
    }

    #[test]
    fn a_cylinder_keeps_arc_lengths_and_heights() {
        let d = rod();
        // The origin on the +Y side at z = 5, seen from outside: x runs
        // towards -X (right-handed about +Y outward with y up).
        assert!(near3(d.point([0.0, 0.0]), [0.0, 10.0, 5.0], 1e-12));
        assert!(near3(d.frame.normal(), [0.0, 1.0, 0.0], 1e-12));
        assert!(near3(d.frame.x_axis, [-1.0, 0.0, 0.0], 1e-12));
        // A quarter turn along x (arc length 5 pi) is the -X side.
        let quarter = 5.0 * std::f64::consts::PI;
        assert!(near3(d.point([quarter, 2.0]), [-10.0, 0.0, 7.0], 1e-12));
        assert!(near3(d.normal([quarter, 2.0]), [-1.0, 0.0, 0.0], 1e-12));
        // Back again: the sketch point of a point of the face.
        let back = d.sketch_point_of([-10.0, 0.0, 7.0], None).unwrap();
        assert!(near2(back, [quarter, 2.0], 1e-12), "{back:?}");
        // On the turn near a given point: the -Y side is half a turn either
        // way; near a point past it, the one past it.
        let half = 10.0 * std::f64::consts::PI;
        let far = d
            .sketch_point_of([0.0, -10.0, 5.0], Some([half + 1.0, 0.0]))
            .unwrap();
        assert!(near2(far, [half, 0.0], 1e-9), "{far:?}");
    }

    #[test]
    fn a_bore_is_seen_from_inside() {
        let d = Development::new(
            DevelopedSurface::Cylinder { radius: 10.0 },
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            true,
            0.0,
            [1.0, 0.0, 0.0],
            0.0,
            [0.0, 0.0, 1.0],
        )
        .unwrap();
        assert!(near3(d.frame.normal(), [-1.0, 0.0, 0.0], 1e-12));
        assert!(near3(d.outward([0.0, 0.0]), [-1.0, 0.0, 0.0], 1e-12));
        // x to the right as seen from the axis: towards -Y.
        assert!(near3(d.frame.x_axis, [0.0, -1.0, 0.0], 1e-12));
        let p = d.point([1.0, 3.0]);
        assert!((p[0].hypot(p[1]) - 10.0).abs() < 1e-12);
        assert!(p[1] < 0.0 && (p[2] - 3.0).abs() < 1e-12, "{p:?}");
        let back = d.sketch_point_of(p, None).unwrap();
        assert!(near2(back, [1.0, 3.0], 1e-12), "{back:?}");
    }

    #[test]
    fn a_cone_is_its_development() {
        let d = cone();
        let sin = 0.5_f64;
        let rho0 = 10.0 / (std::f64::consts::FRAC_PI_6).cos();
        // The origin on the cone at height 10, radius 10 tan 30.
        let r0 = 10.0 * (std::f64::consts::FRAC_PI_6).tan();
        assert!(near3(d.point([0.0, 0.0]), [r0, 0.0, 10.0], 1e-12));
        // Along the generator: v is the slant distance... the carrier's y is
        // the axis, so a point at v along it is rho0 + v from the apex.
        let p = d.point([0.0, 4.0]);
        assert!((norm(p) - (rho0 + 4.0)).abs() < 1e-12, "{p:?}");
        // Lengths across: a point at x = s keeps its distance from the
        // apex's image (0, -rho0).
        let q = d.point([3.0, 0.0]);
        assert!((norm(q) - 3.0_f64.hypot(rho0)).abs() < 1e-12);
        // The turn: atan2(3, rho0) / sin.
        let theta = 3.0_f64.atan2(rho0) / sin;
        let around = q[1].atan2(q[0]).abs();
        assert!((around - theta).abs() < 1e-12, "{around} {theta}");
        let back = d.sketch_point_of(q, None).unwrap();
        assert!(near2(back, [3.0, 0.0], 1e-12), "{back:?}");
        assert!((d.radius() - r0).abs() < 1e-12);
    }

    #[test]
    fn lines_on_a_cylinder_are_lines_circles_and_helices() {
        let d = rod();
        let along = d.line([1.0, 0.0], [1.0, 6.0]);
        assert!(matches!(along, Curve3::Line { .. }), "{along:?}");
        let ring = d.line(
            [-10.0 * std::f64::consts::PI, 1.0],
            [10.0 * std::f64::consts::PI, 1.0],
        );
        match ring {
            Curve3::Conic { closed, major, .. } => {
                assert!(closed);
                assert!((major - 10.0).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
        let backwards = d.line([3.0, 1.0], [-2.0, 1.0]);
        let Some((a, b)) = crate::curves::curve_ends(&backwards) else {
            panic!("{backwards:?}");
        };
        assert!(near3(a, d.point([3.0, 1.0]), 1e-12), "{a:?}");
        assert!(near3(b, d.point([-2.0, 1.0]), 1e-12), "{b:?}");
        let helix = d.line([0.0, 0.0], [20.0, 4.0]);
        let (f, t0, t1) = parametrised(&helix).unwrap();
        assert!(near3(f(t0), d.point([0.0, 0.0]), 1e-12));
        assert!(near3(f(t1), d.point([20.0, 4.0]), 1e-12));
        assert!(near3(f((t0 + t1) / 2.0), d.point([10.0, 2.0]), 1e-12));
        let helix = d.line([20.0, 4.0], [0.0, 0.0]);
        let (f, t0, t1) = parametrised(&helix).unwrap();
        assert!(near3(f(t0), d.point([20.0, 4.0]), 1e-12));
        assert!(near3(f(t1), d.point([0.0, 0.0]), 1e-12));
        assert!(near3(f((t0 + t1) / 2.0), d.point([10.0, 2.0]), 1e-12));
    }

    #[test]
    fn a_curve_on_the_face_follows_its_sketch_points() {
        let d = cone();
        let f = |t: f64| [4.0 * t.cos(), 2.0 + 4.0 * t.sin()];
        let curve = d.curve(&f, 0.0, TAU);
        let (g, t0, t1) = parametrised(&curve).unwrap();
        for k in 0..=37 {
            let t = t0 + (t1 - t0) * k as f64 / 37.0;
            assert!(near3(g(t), d.point(f(t)), 1e-6), "{t}");
        }
        // A circle of radius 4 in the development is 8 pi long on the cone.
        let length = d.length_on_face(&f, 0.0, TAU);
        assert!(
            (length - 8.0 * std::f64::consts::PI).abs() < 1e-6,
            "{length}"
        );
    }

    #[test]
    fn edges_of_the_face_flatten_into_the_sketch() {
        let d = rod();
        // The circle at z = 9 around the rod: a line in the sketch at y = 4.
        let circle = Curve3::Conic {
            center: [0.0, 0.0, 9.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [0.0, 1.0, 0.0],
            major: 10.0,
            minor: 10.0,
            start: -1.0,
            end: 1.0,
            closed: false,
        };
        let flat = d.flattened(&circle).unwrap();
        let Curve3::Line { start, end } = flat else {
            panic!("{flat:?}");
        };
        // From the angle -1 about +Z from +Y (towards +X) to 1 (towards -X,
        // where the sketch's x runs here): x from -10 to 10, at y = 4.
        assert!(near3(start, d.frame.point([-10.0, 4.0]), 1e-9), "{start:?}");
        assert!(near3(end, d.frame.point([10.0, 4.0]), 1e-9), "{end:?}");
        // A line along the axis.
        let line = Curve3::Line {
            start: [0.0, 10.0, 5.0],
            end: [0.0, 10.0, 8.0],
        };
        assert!(matches!(d.flattened(&line), Some(Curve3::Line { .. })));
    }

    #[test]
    fn rays_hit_the_face_nearest_first() {
        let d = rod();
        let hits = d.ray_hits([0.0, 50.0, 7.0], [0.0, -1.0, 0.0], None);
        assert_eq!(hits.len(), 2);
        assert!((hits[0].0 - 40.0).abs() < 1e-12);
        assert!(near2(hits[0].1, [0.0, 2.0], 1e-12), "{:?}", hits[0]);
        assert!((hits[1].0 - 60.0).abs() < 1e-12);
        // Along the axis: no hit.
        assert!(
            d.ray_hits([0.0, 0.0, 50.0], [0.0, 0.0, -1.0], None)
                .is_empty()
        );
        // A cone from outside, square to its axis at height 10.
        let c = cone();
        let r0 = 10.0 * (std::f64::consts::FRAC_PI_6).tan();
        let hits = c.ray_hits([50.0, 0.0, 10.0], [-1.0, 0.0, 0.0], None);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!((hits[0].0 - (50.0 - r0)).abs() < 1e-9);
        assert!(near2(hits[0].1, [0.0, 0.0], 1e-9), "{:?}", hits[0]);
    }
}
