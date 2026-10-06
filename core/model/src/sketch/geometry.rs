// SPDX-License-Identifier: MIT
//! Solved sketch curves as parametric curves: evaluation with derivatives,
//! closest points, bounds, sampling and pieces between parameters (with
//! B-spline knot insertion), in sketch coordinates.

use std::f64::consts::TAU;

use crate::profile::SegmentGeometry;

pub type P2 = [f64; 2];

pub fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub fn scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

pub fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub fn norm(a: P2) -> f64 {
    a[0].hypot(a[1])
}

pub fn dist(a: P2, b: P2) -> f64 {
    norm(sub(a, b))
}

pub fn lerp(a: P2, b: P2, t: f64) -> P2 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Rotated a quarter turn counter-clockwise.
pub fn perp(a: P2) -> P2 {
    [-a[1], a[0]]
}

pub fn unit(a: P2) -> P2 {
    let n = norm(a);
    if n > 0.0 { scale(a, 1.0 / n) } else { a }
}

/// `end` moved past `start` by whole turns, so `start < end <= start + 2π`.
pub fn angle_after(start: f64, end: f64) -> f64 {
    if !start.is_finite() || !end.is_finite() {
        return end;
    }
    // Far apart (a tolerance over a tiny radius): in one step, not turn by
    // turn.
    if (end - start).abs() > 64.0 * TAU {
        let turned = start + (end - start).rem_euclid(TAU);
        return if turned <= start {
            turned + TAU
        } else {
            turned
        };
    }
    let mut end = end;
    while end <= start {
        end += TAU;
    }
    while end > start + TAU {
        end -= TAU;
    }
    end
}

/// A non-uniform rational B-spline: `knots` has `control.len() + degree + 1`
/// values; empty `weights` is non-rational.
#[derive(Debug, Clone, PartialEq)]
pub struct Nurbs {
    pub degree: usize,
    pub control: Vec<P2>,
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
}

impl Nurbs {
    pub fn domain(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.knots.len() - self.degree - 1],
        )
    }

    fn weight(&self, i: usize) -> f64 {
        self.weights.get(i).copied().unwrap_or(1.0)
    }

    /// The knot span index of `t`: `knots[i] <= t < knots[i + 1]`, the last
    /// non-empty span at the end of the domain.
    fn span(&self, t: f64) -> usize {
        let p = self.degree;
        let n = self.control.len() - 1;
        if t >= self.knots[n + 1] {
            let mut i = n;
            while i > p && self.knots[i] >= self.knots[n + 1] {
                i -= 1;
            }
            return i;
        }
        if t <= self.knots[p] {
            let mut i = p;
            while i < n && self.knots[i + 1] <= self.knots[p] {
                i += 1;
            }
            return i;
        }
        let (mut lo, mut hi) = (p, n + 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if t < self.knots[mid] {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        lo
    }

    /// Basis functions and their derivatives up to `order` at `t` in span
    /// `i` (The NURBS Book, A2.3).
    #[allow(clippy::needless_range_loop)]
    fn basis(&self, i: usize, t: f64, order: usize) -> Vec<Vec<f64>> {
        let p = self.degree;
        let u = &self.knots;
        let mut ndu = vec![vec![0.0; p + 1]; p + 1];
        let mut left = vec![0.0; p + 1];
        let mut right = vec![0.0; p + 1];
        ndu[0][0] = 1.0;
        for j in 1..=p {
            left[j] = t - u[i + 1 - j];
            right[j] = u[i + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                ndu[j][r] = right[r + 1] + left[j - r];
                let temp = if ndu[j][r] == 0.0 {
                    0.0
                } else {
                    ndu[r][j - 1] / ndu[j][r]
                };
                ndu[r][j] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            ndu[j][j] = saved;
        }
        let order = order.min(p);
        let mut ders = vec![vec![0.0; p + 1]; order + 1];
        for j in 0..=p {
            ders[0][j] = ndu[j][p];
        }
        let mut a = vec![vec![0.0; p + 1]; 2];
        for r in 0..=p {
            let (mut s1, mut s2) = (0, 1);
            a[0][0] = 1.0;
            for k in 1..=order {
                let mut d = 0.0;
                let rk = r as isize - k as isize;
                let pk = p - k;
                if r >= k {
                    a[s2][0] = a[s1][0] / ndu[pk + 1][rk as usize];
                    d = a[s2][0] * ndu[rk as usize][pk];
                }
                let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
                let j2 = if (r as isize - 1) <= pk as isize {
                    k - 1
                } else {
                    p - r
                };
                for j in j1..=j2 {
                    let idx = (rk + j as isize) as usize;
                    a[s2][j] = (a[s1][j] - a[s1][j - 1]) / ndu[pk + 1][idx];
                    d += a[s2][j] * ndu[idx][pk];
                }
                if r <= pk {
                    a[s2][k] = -a[s1][k - 1] / ndu[pk + 1][r];
                    d += a[s2][k] * ndu[r][pk];
                }
                ders[k][r] = d;
                std::mem::swap(&mut s1, &mut s2);
            }
        }
        let mut factor = p as f64;
        for k in 1..=order {
            for j in 0..=p {
                ders[k][j] *= factor;
            }
            factor *= (p - k) as f64;
        }
        ders
    }

    /// Point and first and second derivatives at `t`.
    #[allow(clippy::needless_range_loop)]
    pub fn eval(&self, t: f64) -> [P2; 3] {
        let p = self.degree;
        let i = self.span(t);
        let n = self.basis(i, t, 2);
        // Homogeneous sums: A = Σ N w P, W = Σ N w and their derivatives.
        let mut a = [[0.0; 2]; 3];
        let mut w = [0.0; 3];
        for (k, row) in n.iter().enumerate() {
            for j in 0..=p {
                let index = i - p + j;
                let wi = self.weight(index);
                let c = self.control[index];
                a[k][0] += row[j] * wi * c[0];
                a[k][1] += row[j] * wi * c[1];
                w[k] += row[j] * wi;
            }
        }
        let c = scale(a[0], 1.0 / w[0]);
        let d1 = scale(sub(a[1], scale(c, w[1])), 1.0 / w[0]);
        let d2 = if n.len() > 2 {
            scale(
                sub(sub(a[2], scale(d1, 2.0 * w[1])), scale(c, w[2])),
                1.0 / w[0],
            )
        } else {
            [0.0, 0.0]
        };
        [c, d1, d2]
    }

    /// Inserts knot `t` until it has multiplicity `degree` (The NURBS Book,
    /// A5.1, in homogeneous coordinates). A value within `1e-12` of the
    /// domain length of an existing knot snaps to it.
    fn insert(&mut self, t: f64) -> f64 {
        let p = self.degree;
        let (lo, hi) = self.domain();
        let snap = 1e-12 * (hi - lo).max(1.0);
        let t = self
            .knots
            .iter()
            .copied()
            .find(|k| (k - t).abs() <= snap)
            .unwrap_or(t);
        let s = self.knots.iter().filter(|k| **k == t).count();
        if s >= p {
            return t;
        }
        let mut pw: Vec<[f64; 3]> = (0..self.control.len())
            .map(|i| {
                let w = self.weight(i);
                [self.control[i][0] * w, self.control[i][1] * w, w]
            })
            .collect();
        for _ in s..p {
            // The last knot at or before t (within the control indices), as
            // Boehm's insertion needs.
            let mut k = pw.len();
            while k > p && self.knots[k] > t {
                k -= 1;
            }
            let mult = self.knots.iter().filter(|x| **x == t).count();
            let mut q = Vec::with_capacity(pw.len() + 1);
            for i in 0..=pw.len() {
                if i + p <= k {
                    q.push(pw[i]);
                } else if i > k - mult {
                    q.push(pw[i - 1]);
                } else {
                    let alpha = (t - self.knots[i]) / (self.knots[i + p] - self.knots[i]);
                    let a = pw[i - 1];
                    let b = pw[i];
                    q.push([
                        (1.0 - alpha) * a[0] + alpha * b[0],
                        (1.0 - alpha) * a[1] + alpha * b[1],
                        (1.0 - alpha) * a[2] + alpha * b[2],
                    ]);
                }
            }
            self.knots.insert(k + 1, t);
            pw = q;
        }
        let rational = !self.weights.is_empty();
        self.control = pw.iter().map(|h| [h[0] / h[2], h[1] / h[2]]).collect();
        self.weights = if rational {
            pw.iter().map(|h| h[2]).collect()
        } else {
            Vec::new()
        };
        t
    }

    /// The piece between `t0` and `t1` as a clamped B-spline.
    pub fn piece(&self, t0: f64, t1: f64) -> Nurbs {
        let p = self.degree;
        let (lo, hi) = self.domain();
        let mut refined = self.clone();
        let t0 = refined.insert(t0.clamp(lo, hi));
        let t1 = refined.insert(t1.clamp(lo, hi));
        // The last copy of t0 and the first of t1.
        let last0 = refined
            .knots
            .iter()
            .rposition(|k| *k == t0)
            .expect("t0 is a knot");
        let first1 = refined
            .knots
            .iter()
            .position(|k| *k == t1)
            .expect("t1 is a knot");
        let start = last0 - p;
        let end = first1 - 1;
        let mut knots = vec![t0; p + 1];
        knots.extend_from_slice(&refined.knots[last0 + 1..first1]);
        knots.extend(std::iter::repeat_n(t1, p + 1));
        Nurbs {
            degree: p,
            control: refined.control[start..=end].to_vec(),
            weights: if refined.weights.is_empty() {
                Vec::new()
            } else {
                refined.weights[start..=end].to_vec()
            },
            knots,
        }
    }

    /// Distinct knots with multiplicities.
    pub fn distinct_knots(&self) -> (Vec<f64>, Vec<u32>) {
        let mut knots: Vec<f64> = Vec::new();
        let mut mults: Vec<u32> = Vec::new();
        for k in &self.knots {
            if knots.last() == Some(k) {
                *mults.last_mut().expect("pushed together") += 1;
            } else {
                knots.push(*k);
                mults.push(1);
            }
        }
        (knots, mults)
    }

    /// Whether it starts and ends at the same point.
    pub fn is_closed(&self) -> bool {
        let (lo, hi) = self.domain();
        dist(self.eval(lo)[0], self.eval(hi)[0]) <= 1e-12 * (1.0 + self.size())
    }

    fn size(&self) -> f64 {
        let (min, max) = bounds(&self.control);
        dist(min, max)
    }
}

fn bounds(points: &[P2]) -> (P2, P2) {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for p in points {
        for k in 0..2 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    (min, max)
}

/// A solved curve. Arcs run counter-clockwise from `start` to `end`
/// (angles, `start < end <= start + 2π`); ellipse parameters likewise.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve2 {
    Line {
        a: P2,
        b: P2,
    },
    Circle {
        center: P2,
        radius: f64,
    },
    Arc {
        center: P2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Ellipse {
        center: P2,
        major: f64,
        minor: f64,
        rotation: f64,
    },
    EllipticalArc {
        center: P2,
        major: f64,
        minor: f64,
        rotation: f64,
        start: f64,
        end: f64,
    },
    Nurbs(Nurbs),
}

impl Curve2 {
    /// The parameter range.
    pub fn domain(&self) -> (f64, f64) {
        match self {
            Self::Line { .. } => (0.0, 1.0),
            Self::Circle { .. } | Self::Ellipse { .. } => (0.0, TAU),
            Self::Arc { start, end, .. } | Self::EllipticalArc { start, end, .. } => (*start, *end),
            Self::Nurbs(n) => n.domain(),
        }
    }

    /// A closed curve has no ends: its parameter wraps around.
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Circle { .. } | Self::Ellipse { .. } => true,
            Self::Nurbs(n) => n.is_closed(),
            _ => false,
        }
    }

    /// Whether the parameter is periodic (circles and ellipses).
    pub fn is_periodic(&self) -> bool {
        matches!(self, Self::Circle { .. } | Self::Ellipse { .. })
    }

    /// Point and first and second derivatives.
    pub fn eval(&self, t: f64) -> [P2; 3] {
        match self {
            Self::Line { a, b } => [lerp(*a, *b, t), sub(*b, *a), [0.0, 0.0]],
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                let (s, c) = t.sin_cos();
                [
                    add(*center, [radius * c, radius * s]),
                    [-radius * s, radius * c],
                    [-radius * c, -radius * s],
                ]
            }
            Self::Ellipse {
                center,
                major,
                minor,
                rotation,
            }
            | Self::EllipticalArc {
                center,
                major,
                minor,
                rotation,
                ..
            } => {
                let (s, c) = t.sin_cos();
                let (rs, rc) = rotation.sin_cos();
                let u = [rc, rs];
                let v = [-rs, rc];
                let at = |x: f64, y: f64| add(scale(u, x), scale(v, y));
                [
                    add(*center, at(major * c, minor * s)),
                    at(-major * s, minor * c),
                    at(-major * c, -minor * s),
                ]
            }
            Self::Nurbs(n) => n.eval(t),
        }
    }

    pub fn point(&self, t: f64) -> P2 {
        self.eval(t)[0]
    }

    /// The start and end points of an open curve.
    pub fn ends(&self) -> Option<(P2, P2)> {
        if self.is_closed() {
            return None;
        }
        let (lo, hi) = self.domain();
        Some((self.point(lo), self.point(hi)))
    }

    /// The parameter on a closed curve wrapped into its domain.
    pub fn wrap(&self, t: f64) -> f64 {
        if self.is_periodic() {
            t.rem_euclid(TAU)
        } else {
            t
        }
    }

    /// Points along the curve at increasing parameters, ends included:
    /// a line two, round curves every 5 degrees or less, splines several
    /// per knot span.
    pub fn sample_params(&self, t0: f64, t1: f64) -> Vec<f64> {
        let count = match self {
            Self::Line { .. } => 1,
            Self::Nurbs(n) => {
                let spans = n
                    .knots
                    .windows(2)
                    .filter(|w| w[1] > w[0] && w[0] < t1 && w[1] > t0)
                    .count()
                    .max(1);
                (spans * 8 * n.degree.max(1)).clamp(16, 512)
            }
            _ => (((t1 - t0).abs() / TAU) * 72.0).ceil().max(2.0) as usize,
        };
        (0..=count)
            .map(|i| t0 + (t1 - t0) * i as f64 / count as f64)
            .collect()
    }

    pub fn sample(&self, t0: f64, t1: f64) -> Vec<P2> {
        self.sample_params(t0, t1)
            .into_iter()
            .map(|t| self.point(t))
            .collect()
    }

    /// Bounds of the whole curve (approximate for splines: their control
    /// polygon).
    pub fn bounds(&self) -> (P2, P2) {
        match self {
            Self::Nurbs(n) => bounds(&n.control),
            _ => {
                let (t0, t1) = self.domain();
                bounds(&self.sample(t0, t1))
            }
        }
    }

    /// The parameter of the point nearest to `p` and its distance.
    pub fn closest(&self, p: P2) -> (f64, f64) {
        let (lo, hi) = self.domain();
        match self {
            Self::Line { a, b } => {
                let d = sub(*b, *a);
                let len2 = dot(d, d);
                let t = if len2 > 0.0 {
                    (dot(sub(p, *a), d) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                (t, dist(p, self.point(t)))
            }
            Self::Circle { center, radius } => {
                let v = sub(p, *center);
                let t = v[1].atan2(v[0]).rem_euclid(TAU);
                (t, (norm(v) - radius).abs())
            }
            Self::Arc {
                center,
                radius,
                start,
                end,
            } => {
                let v = sub(p, *center);
                let t = angle_after(*start - 1e-15, v[1].atan2(v[0]));
                if t <= *end {
                    (t, (norm(v) - radius).abs())
                } else {
                    let (ds, de) = (dist(p, self.point(*start)), dist(p, self.point(*end)));
                    if ds <= de { (*start, ds) } else { (*end, de) }
                }
            }
            _ => {
                // Samples, then Newton on (C(t) - p) . C'(t) = 0.
                let params = self.sample_params(lo, hi);
                let mut best = (lo, f64::INFINITY);
                for t in &params {
                    let d = dist(p, self.point(*t));
                    if d < best.1 {
                        best = (*t, d);
                    }
                }
                let mut t = best.0;
                for _ in 0..50 {
                    let [c, d1, d2] = self.eval(t);
                    let r = sub(c, p);
                    let f = dot(r, d1);
                    let df = dot(d1, d1) + dot(r, d2);
                    if df.abs() < 1e-300 {
                        break;
                    }
                    let mut next = t - f / df;
                    if self.is_periodic() {
                        next = next.rem_euclid(TAU);
                    } else {
                        next = next.clamp(lo, hi);
                    }
                    let done = (next - t).abs() <= 1e-15 * (hi - lo).max(1.0);
                    t = next;
                    if done {
                        break;
                    }
                }
                let d = dist(p, self.point(t));
                if d <= best.1 { (t, d) } else { best }
            }
        }
    }

    /// The piece between parameters `t0 < t1` (for closed curves `t1` may
    /// pass the end of the domain and wrap) as profile geometry.
    pub fn piece(&self, t0: f64, t1: f64) -> SegmentGeometry {
        match self {
            Self::Line { .. } => SegmentGeometry::Line {
                start: self.point(t0),
                end: self.point(t1),
            },
            Self::Circle { center, radius } | Self::Arc { center, radius, .. } => {
                SegmentGeometry::Arc {
                    center: *center,
                    radius: *radius,
                    start_angle: t0,
                    end_angle: t1,
                }
            }
            Self::Ellipse {
                center,
                major,
                minor,
                rotation,
            }
            | Self::EllipticalArc {
                center,
                major,
                minor,
                rotation,
                ..
            } => SegmentGeometry::EllipseArc {
                center: *center,
                major_radius: *major,
                minor_radius: *minor,
                rotation: *rotation,
                start_angle: t0,
                end_angle: t1,
            },
            Self::Nurbs(n) => {
                let (lo, hi) = n.domain();
                let piece = if t1 > hi + 1e-15 {
                    // A closed spline's piece across its seam: join the
                    // two parts' poles (they meet at the seam point).
                    let a = n.piece(t0, hi);
                    let b = n.piece(lo, lo + (t1 - hi));
                    join(&a, &b)
                } else {
                    n.piece(t0.max(lo), t1.min(hi))
                };
                nurbs_geometry(&piece)
            }
        }
    }

    /// The whole curve as profile geometry.
    pub fn whole(&self) -> SegmentGeometry {
        match self {
            Self::Circle { center, radius } => SegmentGeometry::Circle {
                center: *center,
                radius: *radius,
            },
            Self::Ellipse {
                center,
                major,
                minor,
                rotation,
            } => SegmentGeometry::Ellipse {
                center: *center,
                major_radius: *major,
                minor_radius: *minor,
                rotation: *rotation,
            },
            _ => {
                let (lo, hi) = self.domain();
                self.piece(lo, hi)
            }
        }
    }
}

/// Two clamped splines of one degree end to end (the first ends where the
/// second starts) as one, with the second's knots shifted after the first's.
fn join(a: &Nurbs, b: &Nurbs) -> Nurbs {
    let p = a.degree;
    let (_, a_hi) = a.domain();
    let (b_lo, _) = b.domain();
    let shift = a_hi - b_lo;
    let mut knots: Vec<f64> = a.knots[..a.knots.len() - 1].to_vec();
    knots.extend(b.knots[p + 1..].iter().map(|k| k + shift));
    let mut control = a.control.clone();
    control.extend_from_slice(&b.control[1..]);
    let weights = if a.weights.is_empty() && b.weights.is_empty() {
        Vec::new()
    } else {
        let wa: Vec<f64> = (0..a.control.len()).map(|i| a.weight(i)).collect();
        let wb: Vec<f64> = (1..b.control.len()).map(|i| b.weight(i)).collect();
        // Scale the second part so the shared pole has one weight.
        let s = a.weight(a.control.len() - 1) / b.weight(0);
        wa.into_iter()
            .chain(wb.into_iter().map(|w| w * s))
            .collect()
    };
    Nurbs {
        degree: p,
        control,
        weights,
        knots,
    }
}

fn nurbs_geometry(n: &Nurbs) -> SegmentGeometry {
    let (knots, multiplicities) = n.distinct_knots();
    SegmentGeometry::BSpline {
        degree: n.degree as u32,
        poles: n.control.clone(),
        weights: n.weights.clone(),
        knots,
        multiplicities,
        periodic: false,
    }
}

/// Clamped uniform knots on [0, 1] for `count` control points.
pub fn clamped_uniform_knots(count: usize, degree: usize) -> Vec<f64> {
    let interior = count.saturating_sub(degree + 1);
    let mut knots = vec![0.0; degree + 1];
    for i in 1..=interior {
        knots.push(i as f64 / (interior + 1) as f64);
    }
    knots.extend(std::iter::repeat_n(1.0, degree + 1));
    knots
}

/// Signed area of a closed polygon (counter-clockwise positive).
pub fn polygon_area(points: &[P2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| cross(points[i], points[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Area centroid of a closed polygon.
pub fn polygon_centroid(points: &[P2]) -> P2 {
    let n = points.len();
    let area = polygon_area(points);
    if area.abs() < 1e-300 {
        let s = points.iter().fold([0.0, 0.0], |acc, p| add(acc, *p));
        return scale(s, 1.0 / n.max(1) as f64);
    }
    let mut c = [0.0, 0.0];
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        let f = cross(a, b);
        c[0] += (a[0] + b[0]) * f;
        c[1] += (a[1] + b[1]) * f;
    }
    scale(c, 1.0 / (6.0 * area))
}

/// Whether `p` lies inside a closed polygon (even-odd rule).
pub fn polygon_contains(points: &[P2], p: P2) -> bool {
    let n = points.len();
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if x > p[0] {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: P2, b: P2) -> bool {
        dist(a, b) < 1e-9
    }

    fn quarter_circle() -> Nurbs {
        // A rational quadratic quarter circle.
        let w = std::f64::consts::FRAC_1_SQRT_2;
        Nurbs {
            degree: 2,
            control: vec![[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            weights: vec![1.0, w, 1.0],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        }
    }

    #[test]
    fn nurbs_evaluate_with_derivatives() {
        let arc = quarter_circle();
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let [p, d1, _] = arc.eval(t);
            assert!((norm(p) - 1.0).abs() < 1e-12, "{p:?}");
            assert!(dot(p, d1).abs() < 1e-12, "tangent at {t}");
            // Finite differences agree with the derivatives.
            let h = 1e-6;
            let (lo, hi) = ((t - h).max(0.0), (t + h).min(1.0));
            let fd = scale(sub(arc.eval(hi)[0], arc.eval(lo)[0]), 1.0 / (hi - lo));
            assert!(dist(fd, d1) < 1e-5, "{fd:?} {d1:?}");
            let fd2 = scale(sub(arc.eval(hi)[1], arc.eval(lo)[1]), 1.0 / (hi - lo));
            assert!(dist(fd2, arc.eval(t)[2]) < 1e-4);
        }
    }

    #[test]
    fn pieces_follow_the_curve() {
        let arc = quarter_circle();
        let piece = arc.piece(0.25, 0.7);
        assert_eq!(piece.domain(), (0.25, 0.7));
        for i in 0..=10 {
            let t = 0.25 + 0.45 * i as f64 / 10.0;
            assert!(close(piece.eval(t)[0], arc.eval(t)[0]), "{t}");
        }
        // A cubic with interior knots, split at a knot and between knots.
        let cubic = Nurbs {
            degree: 3,
            control: vec![[0.0, 0.0], [1.0, 2.0], [3.0, 3.0], [4.0, 0.0], [6.0, 1.0]],
            weights: Vec::new(),
            knots: clamped_uniform_knots(5, 3),
        };
        for (t0, t1) in [(0.0, 0.5), (0.5, 1.0), (0.1, 0.9), (0.3, 0.35)] {
            let piece = cubic.piece(t0, t1);
            assert_eq!(piece.knots.len(), piece.control.len() + 4);
            for i in 0..=8 {
                let t = t0 + (t1 - t0) * i as f64 / 8.0;
                assert!(
                    close(piece.eval(t)[0], cubic.eval(t)[0]),
                    "{t0}..{t1} at {t}"
                );
            }
        }
        let (knots, mults) = cubic.piece(0.1, 0.9).distinct_knots();
        assert_eq!(knots, vec![0.1, 0.5, 0.9]);
        assert_eq!(mults, vec![4, 1, 4]);
    }

    #[test]
    fn closest_points() {
        let circle = Curve2::Circle {
            center: [1.0, 1.0],
            radius: 2.0,
        };
        let (t, d) = circle.closest([1.0, 5.0]);
        assert!((t - std::f64::consts::FRAC_PI_2).abs() < 1e-12 && (d - 2.0).abs() < 1e-12);
        let ellipse = Curve2::Ellipse {
            center: [0.0, 0.0],
            major: 3.0,
            minor: 1.0,
            rotation: 0.3,
        };
        let p = ellipse.point(2.0);
        let (t, d) = ellipse.closest(add(p, [1e-3, 0.0]));
        assert!((t - 2.0).abs() < 1e-2 && d < 1.1e-3);
        let arc = Curve2::Arc {
            center: [0.0, 0.0],
            radius: 1.0,
            start: 0.0,
            end: 1.0,
        };
        assert_eq!(arc.closest([-1.0, 0.1]).0, 1.0);
        assert!(polygon_contains(
            &[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]],
            [1.0, 1.5]
        ));
        assert_eq!(
            polygon_area(&[[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]),
            4.0
        );
    }
}
