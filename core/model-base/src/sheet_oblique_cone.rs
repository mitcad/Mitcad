// SPDX-License-Identifier: MIT
//! Oblique (eccentric) conical sheet panels (mitcad#2491): the cone of the
//! lines from an apex through a circle whose centre need not lie below it,
//! and the development of its neutral surface (the cone moved along its
//! normals: developable, but no cone itself). Its generators keep their
//! directions and their lengths; the curve they start from (the apex moved
//! along the normals) develops into a small curve `base` near the chart's
//! origin: chart(theta, rho) = base(theta) + rho (cos phi, sin phi), `phi`
//! the developed angle, `rho` the distance along the generator from the
//! cone's apex.
//!
//! The development is integrated along the angle round the circle from 0
//! with the classical Runge-Kutta rule at steps of a whole turn over
//! [`STEPS`], as `geometry/src/sheet_oblique_cone.cpp` does, so that the
//! model and the kernel read one chart.

use super::Isometry2;
use mitcad_vec3::{Vec3, add, cross, dot, norm, scale, sub};
use serde::{Deserialize, Serialize};

/// Steps of the development's integration over a whole turn.
pub const STEPS: u32 = 4096;

const TAU: f64 = std::f64::consts::TAU;

/// An oblique conical panel's chart (mitcad#2491): the development of its
/// neutral surface by its generators. The front surface is the cone of
/// the lines from `apex` through the circle about `centre` of `radius` in
/// the plane of `radial_start` and `angular_direction` (unit vectors at
/// right angles); the angle `theta` runs round that circle from
/// `radial_start` towards `angular_direction`. The material lies along the
/// cone's normals from the front, the thickness deep: away from the cone's
/// inside when `inner_is_front`, towards it otherwise; the neutral surface
/// is `neutral_depth` (K t) from the inner one. The chart's material lies
/// at the angles `0..=angle_extent` (less than a turn); the kernel reads
/// the chart within `w = pi - angle_extent / 2` beyond them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObliqueConeEmbedding {
    /// The apex of the front surface's cone.
    pub apex: [f64; 3],
    pub centre: [f64; 3],
    pub radius: f64,
    pub radial_start: [f64; 3],
    pub angular_direction: [f64; 3],
    /// From the inner surface to the neutral one, mm.
    pub neutral_depth: f64,
    /// The angle round the circle the chart's material lies in, from 0
    /// (radians, less than a turn).
    pub angle_extent: f64,
    pub inner_is_front: bool,
}

/// The circle an oblique cone's generators run through
/// ([`super::SheetFaceDevelopment::oblique`]), and whether the body is a
/// plate behind the faces: every other face on the surface the thickness
/// behind them along the cone's normals (within 1e-4 mm) or a wall
/// meeting both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObliqueCircle {
    pub centre: [f64; 3],
    pub radius: f64,
    pub plate: bool,
}

/// The unit generator at an angle, its first two derivatives by the angle,
/// and the unit normal away from the cone's inside.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Generator {
    pub g: Vec3,
    pub dg: Vec3,
    pub ddg: Vec3,
    pub n: Vec3,
}

/// The cone of the lines from `apex` through the circle about `centre` of
/// `radius` in the plane of the unit vectors `x` and `y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObliqueCone {
    pub apex: Vec3,
    pub centre: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub radius: f64,
}

impl ObliqueCone {
    pub fn generator(&self, theta: f64) -> Generator {
        let (s, c) = theta.sin_cos();
        let radial = add(scale(self.x, c), scale(self.y, s));
        let w = add(sub(self.centre, self.apex), scale(radial, self.radius));
        let w1 = scale(add(scale(self.x, -s), scale(self.y, c)), self.radius);
        let w2 = scale(radial, -self.radius);
        let length = norm(w);
        let g = scale(w, 1.0 / length);
        let l1 = dot(g, w1);
        let l2 = (dot(w1, w1) + dot(w, w2) - l1 * l1) / length;
        let dg = scale(sub(w1, scale(g, l1)), 1.0 / length);
        let ddg = scale(
            sub(sub(w2, scale(dg, 2.0 * l1)), scale(g, l2)),
            1.0 / length,
        );
        let mut n = cross(g, w1);
        n = scale(n, 1.0 / norm(n));
        // Away from the inside: the inside lies towards the circle's centre.
        if dot(n, radial) < 0.0 {
            n = scale(n, -1.0);
        }
        Generator { g, dg, ddg, n }
    }

    /// The angle of the generator through `p`, in (-pi, pi].
    pub fn angle(&self, p: Vec3) -> f64 {
        let normal = cross(self.x, self.y);
        let d = sub(p, self.apex);
        let s = dot(normal, sub(self.centre, self.apex)) / dot(normal, d);
        let in_plane = add(sub(self.apex, self.centre), scale(d, s));
        dot(in_plane, self.y).atan2(dot(in_plane, self.x))
    }

    /// The angle, the distance along the generator and the offset along
    /// the normal of a point near the cone (`p = apex + rho g + offset
    /// n`): the generator whose normal plane holds it, `(p - apex) . g' =
    /// 0`, by Newton's steps from the generator through it; the angle in
    /// (-pi, pi] give or take the steps. None where they do not settle.
    pub fn foot(&self, p: Vec3) -> Option<(f64, f64, f64)> {
        let d = sub(p, self.apex);
        let mut theta = self.angle(p);
        for _ in 0..50 {
            let at = self.generator(theta);
            let (h, slope) = (dot(d, at.dg), dot(d, at.ddg));
            if h.abs() <= 1e-12 * norm(d).max(1.0) {
                return Some((theta, dot(d, at.g), dot(d, at.n)));
            }
            if slope == 0.0 || !slope.is_finite() {
                return None;
            }
            theta -= h / slope;
        }
        None
    }

    /// `apex + rho g(theta) + offset n(theta)`.
    pub fn point(&self, theta: f64, rho: f64, offset: f64) -> Vec3 {
        let at = self.generator(theta);
        add(add(self.apex, scale(at.g, rho)), scale(at.n, offset))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct State {
    phi: f64,
    base: [f64; 2],
}

/// The development of the surface `offset` from an oblique cone along its
/// normals over the angles `first..=last` (which hold 0): `phi(0) = 0`,
/// `phi' = |g'|`; `base(0) = offset c(0) (1, 0)`, `base' = -offset (n .
/// g'') / |g'| (-sin phi, cos phi)`, `c = -(n . g'') / |g'|^2`. On a cone of
/// revolution `base` is `offset cot(half angle)` along the generator: the
/// neutral cone's apex.
#[derive(Debug, Clone, PartialEq)]
pub struct ObliqueDevelopment {
    cone: ObliqueCone,
    offset: f64,
    first: f64,
    last: f64,
    h: f64,
    low: i64,
    nodes: Vec<State>,
}

impl ObliqueDevelopment {
    pub fn new(cone: ObliqueCone, offset: f64, first: f64, last: f64) -> Self {
        let first = first.min(0.0);
        let last = last.max(0.0);
        let h = TAU / f64::from(STEPS);
        let low = (first / h).floor() as i64;
        let high = (last / h).ceil() as i64;
        let mut out = Self {
            cone,
            offset,
            first,
            last,
            h,
            low,
            nodes: vec![
                State {
                    phi: 0.0,
                    base: [0.0, 0.0]
                };
                (high - low + 1) as usize
            ],
        };
        let start = cone.generator(0.0);
        let speed = norm(start.dg);
        let zero = (-low) as usize;
        out.nodes[zero] = State {
            phi: 0.0,
            base: [-offset * dot(start.n, start.ddg) / (speed * speed), 0.0],
        };
        for i in 1..=high {
            let k = zero + i as usize;
            out.nodes[k] = out.step((i - 1) as f64 * h, out.nodes[k - 1], h);
        }
        for i in (low..0).rev() {
            let k = zero - (-i) as usize;
            out.nodes[k] = out.step((i + 1) as f64 * h, out.nodes[k + 1], -h);
        }
        out
    }

    pub fn first(&self) -> f64 {
        self.first
    }

    pub fn last(&self) -> f64 {
        self.last
    }

    fn slope(&self, theta: f64, s: State) -> State {
        let at = self.cone.generator(theta);
        let speed = norm(at.dg);
        let k = dot(at.n, at.ddg) / speed;
        State {
            phi: speed,
            base: [
                self.offset * k * s.phi.sin(),
                -self.offset * k * s.phi.cos(),
            ],
        }
    }

    fn step(&self, theta: f64, from: State, h: f64) -> State {
        let moved = |s: State, d: State, by: f64| State {
            phi: s.phi + by * d.phi,
            base: [s.base[0] + by * d.base[0], s.base[1] + by * d.base[1]],
        };
        let k1 = self.slope(theta, from);
        let k2 = self.slope(theta + h / 2.0, moved(from, k1, h / 2.0));
        let k3 = self.slope(theta + h / 2.0, moved(from, k2, h / 2.0));
        let k4 = self.slope(theta + h, moved(from, k3, h));
        let sum = |a: f64, b: f64, c: f64, d: f64| a + 2.0 * b + 2.0 * c + d;
        State {
            phi: from.phi + h / 6.0 * sum(k1.phi, k2.phi, k3.phi, k4.phi),
            base: std::array::from_fn(|i| {
                from.base[i] + (h / 6.0) * sum(k1.base[i], k2.base[i], k3.base[i], k4.base[i])
            }),
        }
    }

    fn at(&self, theta: f64) -> State {
        // The node next to theta towards 0, then one step to it: as the
        // steps run from 0.
        let high = self.low + self.nodes.len() as i64 - 1;
        let j = if theta >= 0.0 {
            (theta / self.h).floor() as i64
        } else {
            (theta / self.h).ceil() as i64
        }
        .clamp(self.low, high);
        let node = self.nodes[(j - self.low) as usize];
        let h = theta - j as f64 * self.h;
        if h == 0.0 {
            node
        } else {
            self.step(j as f64 * self.h, node, h)
        }
    }

    pub fn phi(&self, theta: f64) -> f64 {
        self.at(theta).phi
    }

    pub fn base(&self, theta: f64) -> [f64; 2] {
        self.at(theta).base
    }

    pub fn chart(&self, theta: f64, rho: f64) -> [f64; 2] {
        let s = self.at(theta);
        [s.base[0] + rho * s.phi.cos(), s.base[1] + rho * s.phi.sin()]
    }

    /// Where the generator at `theta` lies in the chart: the isometry from
    /// its own frame (the generator along x from the point where it starts
    /// as on a cone of revolution, `base - offset c` along it) to the
    /// chart. At 0 the identity.
    pub fn frame(&self, theta: f64) -> Isometry2 {
        let s = self.at(theta);
        let g = self.cone.generator(theta);
        let speed = norm(g.dg);
        let c = -dot(g.n, g.ddg) / (speed * speed);
        let (sp, cp) = s.phi.sin_cos();
        Isometry2 {
            origin: [
                s.base[0] - self.offset * c * cp,
                s.base[1] - self.offset * c * sp,
            ],
            x_axis: [cp, sp],
            y_axis: [-sp, cp],
        }
    }

    /// The angle and the distance along the generator of a chart point,
    /// the angle within `first..=last`; None where no generator of the
    /// range runs through it on its material side.
    pub fn inverse(&self, p: [f64; 2]) -> Option<(f64, f64)> {
        let across = |t: f64| {
            let s = self.at(t);
            let d = [p[0] - s.base[0], p[1] - s.base[1]];
            let (sn, c) = s.phi.sin_cos();
            (c * d[1] - sn * d[0], c * d[0] + sn * d[1])
        };
        let (phi_first, phi_last) = (self.phi(self.first), self.phi(self.last));
        let mut target = p[1].atan2(p[0]);
        let middle = (phi_first + phi_last) / 2.0;
        target += TAU * ((middle - target) / TAU).round();
        let target = target.clamp(phi_first, phi_last);
        let (mut lo, mut hi) = (self.first, self.last);
        for _ in 0..64 {
            let mid = (lo + hi) / 2.0;
            if self.phi(mid) < target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let guess = (lo + hi) / 2.0;
        let (f, _) = across(guess);
        let (mut a, mut b) = (guess, guess);
        let (mut fa, mut fb) = (f, f);
        let reach = 8.0 * self.h;
        if f >= 0.0 {
            while fb > 0.0 {
                if b >= self.last {
                    return None;
                }
                a = b;
                fa = fb;
                b = (b + reach).min(self.last);
                fb = across(b).0;
            }
        } else {
            while fa < 0.0 {
                if a <= self.first {
                    return None;
                }
                b = a;
                fb = fa;
                a = (a - reach).max(self.first);
                fa = across(a).0;
            }
        }
        let mut t = if fa == fb {
            a
        } else {
            a + (b - a) * fa / (fa - fb)
        };
        for _ in 0..100 {
            let (f, along) = across(t);
            if f == 0.0 {
                break;
            }
            if f > 0.0 {
                a = t;
            } else {
                b = t;
            }
            let g = self.cone.generator(t);
            let speed = norm(g.dg);
            let slope = -speed * along + self.offset * dot(g.n, g.ddg) / speed;
            let mut next = if slope < 0.0 {
                t - f / slope
            } else {
                (a + b) / 2.0
            };
            if !(next > a && next < b) {
                next = (a + b) / 2.0;
            }
            let done = (next - t).abs() <= 1e-15 * t.abs().max(1.0) || b - a <= 1e-15;
            t = next;
            if done {
                break;
            }
        }
        let (_, along) = across(t);
        (along > 0.0).then_some((t, along))
    }
}

/// The inverse of an isometry of the plane.
pub fn inverse_isometry(map: Isometry2) -> Isometry2 {
    let x = [map.x_axis[0], map.y_axis[0]];
    let y = [map.x_axis[1], map.y_axis[1]];
    Isometry2 {
        origin: [
            -(x[0] * map.origin[0] + y[0] * map.origin[1]),
            -(x[1] * map.origin[0] + y[1] * map.origin[1]),
        ],
        x_axis: x,
        y_axis: y,
    }
}

impl ObliqueConeEmbedding {
    /// The front surface's cone, its angle 0 at `radial_start`.
    pub fn cone(&self) -> ObliqueCone {
        ObliqueCone {
            apex: self.apex,
            centre: self.centre,
            x: self.radial_start,
            y: self.angular_direction,
            radius: self.radius,
        }
    }

    /// The neutral surface's offset from the front along the cone's
    /// normals, away from its inside.
    pub fn neutral_offset(&self, thickness: f64) -> f64 {
        if self.inner_is_front {
            self.neutral_depth
        } else {
            self.neutral_depth - thickness
        }
    }

    /// The angles the kernel reads the chart within.
    pub fn window(&self) -> (f64, f64) {
        let free = std::f64::consts::PI - self.angle_extent / 2.0;
        (-free, self.angle_extent + free)
    }

    /// The chart's development within its window.
    pub fn development(&self, thickness: f64) -> ObliqueDevelopment {
        let (first, last) = self.window();
        ObliqueDevelopment::new(self.cone(), self.neutral_offset(thickness), first, last)
    }

    /// The model point of chart point `p` at `depth` from the front surface
    /// towards the back (`0..=thickness`); NaN off the chart's window.
    pub fn point(&self, p: [f64; 2], depth: f64, thickness: f64) -> [f64; 3] {
        let Some((theta, rho)) = self.development(thickness).inverse(p) else {
            return [f64::NAN; 3];
        };
        let offset = if self.inner_is_front { depth } else { -depth };
        self.cone().point(theta, rho, offset)
    }

    /// The chart point and the depth from the front surface of a model
    /// point, or None outside the chart's angles.
    pub fn coordinates(&self, point: [f64; 3], thickness: f64) -> Option<([f64; 2], f64)> {
        let (theta, rho, offset) = self.cone().foot(point)?;
        let (first, _) = self.window();
        let mut theta = theta + TAU * ((first - theta) / TAU).ceil();
        if theta >= first + TAU {
            theta -= TAU;
        }
        if !theta.is_finite() || theta < -1e-9 || theta > self.angle_extent + 1e-9 {
            return None;
        }
        let depth = if self.inner_is_front { offset } else { -offset };
        Some((self.development(thickness).chart(theta, rho), depth))
    }

    /// The front's model point at chart point `p`, the model directions of
    /// the chart's x and y there, and the direction from the front to the
    /// back; None off the chart's window.
    pub fn tangent(&self, p: [f64; 2], thickness: f64) -> Option<[Vec3; 4]> {
        let development = self.development(thickness);
        let (theta, rho) = development.inverse(p)?;
        let cone = self.cone();
        let at = cone.generator(theta);
        let across = scale(at.dg, 1.0 / norm(at.dg));
        let (sp, cp) = development.phi(theta).sin_cos();
        let x = sub(scale(at.g, cp), scale(across, sp));
        let y = add(scale(at.g, sp), scale(across, cp));
        let f = if self.inner_is_front {
            at.n
        } else {
            scale(at.n, -1.0)
        };
        Some([cone.point(theta, rho, 0.0), x, y, f])
    }

    /// The chart started at the angle `theta` instead (its material from
    /// there `extent` round), and the isometry that takes points of this
    /// chart to that one.
    pub fn rebased(&self, theta: f64, extent: f64, thickness: f64) -> (Self, Isometry2) {
        let map = inverse_isometry(self.development(thickness).frame(theta));
        let (s, c) = theta.sin_cos();
        (
            Self {
                radial_start: add(
                    scale(self.radial_start, c),
                    scale(self.angular_direction, s),
                ),
                angular_direction: add(
                    scale(self.radial_start, -s),
                    scale(self.angular_direction, c),
                ),
                angle_extent: extent,
                ..self.clone()
            },
            map,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone of revolution (apex 40 below the circle of radius 30 about
    /// its axis): its development is the circular cone's, `offset cot` the
    /// neutral apex along the generator, `0.6 theta` the developed angle.
    #[test]
    fn a_cone_of_revolution_develops_as_one() {
        let cone = ObliqueCone {
            apex: [0.0, 0.0, -40.0],
            centre: [0.0, 0.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            radius: 30.0,
        };
        let offset = 1.5;
        let d = ObliqueDevelopment::new(cone, offset, -1.0, 4.0);
        for theta in [0.0, 0.7, -0.9, 3.9] {
            assert!((d.phi(theta) - 0.6 * theta).abs() < 1e-11);
            let base = d.base(theta);
            let along = offset * 0.8 / 0.6;
            let phi = 0.6 * theta;
            assert!((base[0] - along * phi.cos()).abs() < 1e-9);
            assert!((base[1] - along * phi.sin()).abs() < 1e-9);
            let frame = d.frame(theta);
            assert!(frame.origin[0].abs() < 1e-9 && frame.origin[1].abs() < 1e-9);
        }
        let p = d.chart(1.3, 42.0);
        let (theta, rho) = d.inverse(p).expect("on the chart");
        assert!((theta - 1.3).abs() < 1e-12 && (rho - 42.0).abs() < 1e-9);
    }

    /// An eccentric cone: its developed lengths along the generators and
    /// across them are the neutral surface's.
    #[test]
    fn an_eccentric_cone_keeps_lengths() {
        let cone = ObliqueCone {
            apex: [25.0, 0.0, 120.0],
            centre: [0.0, 0.0, 0.0],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            radius: 60.0,
        };
        let offset = -1.0;
        let d = ObliqueDevelopment::new(cone, offset, -0.2, 6.5);
        // The developed curve at rho 80 against the neutral surface's
        // curve there, chord by chord.
        let (mut chart_length, mut model_length) = (0.0, 0.0);
        let n = 2000;
        for i in 0..n {
            let (a, b) = (
                6.0 * f64::from(i) / f64::from(n),
                6.0 * f64::from(i + 1) / f64::from(n),
            );
            let (p, q) = (d.chart(a, 80.0), d.chart(b, 80.0));
            chart_length += (q[0] - p[0]).hypot(q[1] - p[1]);
            model_length += norm(sub(
                cone.point(b, 80.0, offset),
                cone.point(a, 80.0, offset),
            ));
        }
        assert!((chart_length - model_length).abs() < 1e-6 * model_length);
        // Lengths along a generator are kept.
        let (p, q) = (d.chart(2.0, 50.0), d.chart(2.0, 90.0));
        assert!(((q[0] - p[0]).hypot(q[1] - p[1]) - 40.0).abs() < 1e-9);
        let (theta, rho) = d.inverse(d.chart(4.2, 75.0)).expect("on the chart");
        assert!((theta - 4.2).abs() < 1e-11 && (rho - 75.0).abs() < 1e-9);
        // The isometry of a generator's frame and its inverse.
        let frame = d.frame(3.0);
        let back = inverse_isometry(frame);
        let at = back.point(frame.point([12.0, -3.0]));
        assert!((at[0] - 12.0).abs() < 1e-12 && (at[1] + 3.0).abs() < 1e-12);
    }

    fn eccentric(inner_is_front: bool) -> ObliqueConeEmbedding {
        ObliqueConeEmbedding {
            apex: [25.0, 0.0, 120.0],
            centre: [0.0, 0.0, 0.0],
            radius: 60.0,
            radial_start: [1.0, 0.0, 0.0],
            angular_direction: [0.0, 1.0, 0.0],
            neutral_depth: 0.4,
            angle_extent: 4.0,
            inner_is_front,
        }
    }

    /// A chart point at a depth and back; a rebased chart's points are the
    /// same model points.
    #[test]
    fn coordinates_invert_points_and_rebasing_keeps_them() {
        for inner in [false, true] {
            let c = eccentric(inner);
            let d = c.development(2.0);
            for (theta, rho, depth) in [(0.5, 70.0, 0.0), (2.0, 90.0, 1.3), (3.7, 60.0, 2.0)] {
                let p = d.chart(theta, rho);
                let at = c.point(p, depth, 2.0);
                let (back, depth_back) = c.coordinates(at, 2.0).expect("on the chart");
                assert!((back[0] - p[0]).abs() < 1e-9 && (back[1] - p[1]).abs() < 1e-9);
                assert!((depth_back - depth).abs() < 1e-9);
                // The front point and the chart's directions there.
                let [front, x, y, f] = c.tangent(p, 2.0).expect("on the chart");
                assert!(norm(sub(front, c.point(p, 0.0, 2.0))) < 1e-9);
                assert!(dot(x, y).abs() < 1e-12 && (norm(x) - 1.0).abs() < 1e-12);
                assert!(dot(f, x).abs() < 1e-12 && dot(f, y).abs() < 1e-12);
            }
            // Started at 1.5: the same model points.
            let (moved, map) = c.rebased(1.5, 2.5, 2.0);
            for (theta, rho) in [(1.6, 70.0), (3.9, 85.0)] {
                let p = d.chart(theta, rho);
                let before = c.point(p, 0.7, 2.0);
                let after = moved.point(map.point(p), 0.7, 2.0);
                assert!(norm(sub(before, after)) < 1e-7);
            }
        }
        // Past the chart's angles: none.
        let c = eccentric(false);
        let outside = c.cone().point(5.0, 70.0, 0.0);
        assert!(c.coordinates(outside, 2.0).is_none());
    }
}
