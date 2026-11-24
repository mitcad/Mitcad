// SPDX-License-Identifier: MIT
//! Solved sketch curves as parametric curves: evaluation with derivatives,
//! closest points, bounds, sampling and pieces between parameters (with
//! B-spline knot insertion), in sketch coordinates.

use std::f64::consts::{PI, TAU};

use crate::profile::SegmentGeometry;

use mitcad_vec3::vec2;

pub type P2 = [f64; 2];

// The sketch modules take the `[f64; 2]` helpers from here.
pub use mitcad_vec3::vec2::distance as dist;
pub use mitcad_vec3::vec2::{add, cross, dot, lerp, norm, perp, scale, sub};

/// The unit vector along `a` (multiplied by the reciprocal of its length),
/// or `a` itself when its length is not greater than zero.
pub fn unit(a: P2) -> P2 {
    vec2::unit_above(a, 0.0).unwrap_or(a)
}

/// The rays of an angle dimension between the lines `a` and `b` (start,
/// end) on a side (mitcad#296): the lines' unit directions, reversed where
/// `side` says.
pub fn angle_rays(a: (P2, P2), b: (P2, P2), side: [bool; 2]) -> (P2, P2) {
    let ray = |(s, e): (P2, P2), reverse: bool| {
        let d = unit(sub(e, s));
        if reverse { scale(d, -1.0) } else { d }
    };
    (ray(a, side[0]), ray(b, side[1]))
}

/// The angle between the lines `a` and `b` (start, end) on a side, 0 to π
/// (see [`angle_rays`]).
pub fn angle_on_side(a: (P2, P2), b: (P2, P2), side: [bool; 2]) -> f64 {
    let (da, db) = angle_rays(a, b, side);
    dot(da, db).clamp(-1.0, 1.0).acos()
}

/// Where the lines `a` and `b` (start, end), extended, meet; None for
/// parallel lines.
pub fn line_crossing(a: (P2, P2), b: (P2, P2)) -> Option<P2> {
    let (da, db) = (sub(a.1, a.0), sub(b.1, b.0));
    let c = cross(da, db);
    if c.abs() <= 1e-12 * norm(da) * norm(db) {
        return None;
    }
    Some(add(a.0, scale(da, cross(sub(b.0, a.0), db) / c)))
}

/// The side of an angle dimension between the lines `a` and `b` (start,
/// end) whose sector holds `at` (mitcad#296): of the four sectors the lines'
/// extensions make where they meet, the one `at` is in, bounded by the rays
/// `±(end - start)`. None for parallel lines.
pub fn angle_side_at(a: (P2, P2), b: (P2, P2), at: P2) -> Option<[bool; 2]> {
    let corner = line_crossing(a, b)?;
    let (da, db) = (sub(a.1, a.0), sub(b.1, b.0));
    let c = cross(da, db);
    // `at - corner = s da + t db`.
    let v = sub(at, corner);
    Some([cross(v, db) / c < 0.0, cross(da, v) / c < 0.0])
}

/// The side of an angle dimension between the lines `a` and `b` (start,
/// end) that measures `value` (radians, within `tol`) with its value shown
/// at `at` (mitcad#296): the sector `at` is in when its angle is `value`,
/// else the neighbouring sector nearer to `at` when the supplement is.
/// Without `at`, the sector between the directions or a neighbour. None
/// for parallel lines and a value neither angle has.
pub fn angle_side_for(
    a: (P2, P2),
    b: (P2, P2),
    value: f64,
    at: Option<P2>,
    tol: f64,
) -> Option<[bool; 2]> {
    let corner = line_crossing(a, b)?;
    let side = match at {
        Some(at) => angle_side_at(a, b, at)?,
        None => [false, false],
    };
    if (angle_on_side(a, b, side) - value).abs() <= tol {
        return Some(side);
    }
    let near = |s: [bool; 2]| {
        let (da, db) = angle_rays(a, b, s);
        at.map_or(0.0, |at| dot(add(da, db), sub(at, corner)))
    };
    let (first, second) = ([!side[0], side[1]], [side[0], !side[1]]);
    let next = if near(second) > near(first) {
        second
    } else {
        first
    };
    ((angle_on_side(a, b, next) - value).abs() <= tol).then_some(next)
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

#[cfg(test)]
thread_local! {
    /// How many times [`Curve2::closest`] searched a whole spline or
    /// ellipse on this thread (tests of mitcad#878).
    pub(crate) static WHOLE_SEARCHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The highest degree whose evaluation works in stack arrays
/// ([`Nurbs::eval`], [`Nurbs::point`]); higher degrees allocate.
const STACK_DEGREE: usize = 15;

/// A non-uniform rational B-spline: `knots` has `control.len() + degree + 1`
/// values; empty `weights` is non-rational.
#[derive(Debug, Clone, PartialEq)]
pub struct Nurbs {
    pub degree: usize,
    pub control: Vec<P2>,
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
}

/// A knot inserted once ([`Nurbs::insert_knot`], mitcad#836): the same
/// curve with one more control point.
#[derive(Debug, Clone, PartialEq)]
pub struct KnotInsertion {
    /// The refined spline: the same point at every parameter.
    pub spline: Nurbs,
    /// The knot inserted (an existing knot when it was that near one).
    pub knot: f64,
    /// The refined spline's control points `first..=last` are new: they
    /// take the place of the old `first..last` (one fewer). Those before
    /// `first` are the old ones; those after `last` the old ones from
    /// `last` on, one place further.
    pub first: usize,
    pub last: usize,
}

/// A control point removed with a knot ([`Nurbs::remove_control_point`],
/// mitcad#2326): one control point and one knot fewer.
#[derive(Debug, Clone, PartialEq)]
pub struct KnotRemoval {
    /// The spline without them: the old control points before the removed
    /// one at their places, those after it one place earlier.
    pub spline: Nurbs,
    /// The knot value removed.
    pub knot: f64,
    /// The old indices of the control points that moved to keep the curve
    /// near its old shape (empty when none did).
    pub moved: Vec<usize>,
    /// Whether the curve is the same as before (within `1e-9` of the
    /// control points' size): the knot was one insertion made.
    pub exact: bool,
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

    /// Basis functions and their derivatives up to `order` (at most 2) at
    /// `t` in span `i` (The NURBS Book, A2.3) into `ders`, row `k` (the
    /// `k`-th derivative) at `ders[k * (p + 1)..]`; returns the order
    /// computed (at most the degree). The work arrays are on the stack up
    /// to degree [`STACK_DEGREE`]: a sketch's region search evaluates its
    /// splines millions of times (an imported sketch of overlapping thread
    /// projections took 6.8 s to evaluate, mitcad#878).
    fn basis(&self, i: usize, t: f64, order: usize, ders: &mut [f64]) -> usize {
        let p = self.degree;
        let w = p + 1;
        if p <= STACK_DEGREE {
            const W: usize = STACK_DEGREE + 1;
            let mut ndu = [0.0; W * W];
            let mut left = [0.0; W];
            let mut right = [0.0; W];
            let mut a = [0.0; 2 * W];
            self.basis_with(
                i,
                t,
                order,
                &mut ndu[..w * w],
                &mut left[..w],
                &mut right[..w],
                &mut a[..2 * w],
                ders,
            )
        } else {
            let (mut ndu, mut left, mut right, mut a) = (
                vec![0.0; w * w],
                vec![0.0; w],
                vec![0.0; w],
                vec![0.0; 2 * w],
            );
            self.basis_with(i, t, order, &mut ndu, &mut left, &mut right, &mut a, ders)
        }
    }

    /// [`Nurbs::basis`] with its work arrays: `ndu` (p + 1)², `left` and
    /// `right` p + 1, `a` 2 (p + 1).
    #[allow(clippy::too_many_arguments)]
    fn basis_with(
        &self,
        i: usize,
        t: f64,
        order: usize,
        ndu: &mut [f64],
        left: &mut [f64],
        right: &mut [f64],
        a: &mut [f64],
        ders: &mut [f64],
    ) -> usize {
        let p = self.degree;
        let w = p + 1;
        let u = &self.knots;
        // ndu[j][r] at ndu[j * w + r]; a[s][j] at a[s * w + j].
        ndu[0] = 1.0;
        for j in 1..=p {
            left[j] = t - u[i + 1 - j];
            right[j] = u[i + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                ndu[j * w + r] = right[r + 1] + left[j - r];
                let temp = if ndu[j * w + r] == 0.0 {
                    0.0
                } else {
                    ndu[r * w + j - 1] / ndu[j * w + r]
                };
                ndu[r * w + j] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            ndu[j * w + j] = saved;
        }
        let order = order.min(p).min(2);
        for j in 0..=p {
            ders[j] = ndu[j * w + p];
        }
        for r in 0..=p {
            let (mut s1, mut s2) = (0, w);
            a[0] = 1.0;
            for k in 1..=order {
                let mut d = 0.0;
                let rk = r as isize - k as isize;
                let pk = p - k;
                if r >= k {
                    a[s2] = a[s1] / ndu[(pk + 1) * w + rk as usize];
                    d = a[s2] * ndu[rk as usize * w + pk];
                }
                let j1 = if rk >= -1 { 1 } else { (-rk) as usize };
                let j2 = if (r as isize - 1) <= pk as isize {
                    k - 1
                } else {
                    p - r
                };
                for j in j1..=j2 {
                    let idx = (rk + j as isize) as usize;
                    a[s2 + j] = (a[s1 + j] - a[s1 + j - 1]) / ndu[(pk + 1) * w + idx];
                    d += a[s2 + j] * ndu[idx * w + pk];
                }
                if r <= pk {
                    a[s2 + k] = -a[s1 + k - 1] / ndu[(pk + 1) * w + r];
                    d += a[s2 + k] * ndu[r * w + pk];
                }
                ders[k * w + r] = d;
                std::mem::swap(&mut s1, &mut s2);
            }
        }
        let mut factor = p as f64;
        for k in 1..=order {
            for j in 0..=p {
                ders[k * w + j] *= factor;
            }
            factor *= (p - k) as f64;
        }
        order
    }

    /// The point at `t` alone (The NURBS Book, A2.2 and A4.1): no
    /// derivatives, no heap. Equal to `eval(t)[0]`.
    pub fn point(&self, t: f64) -> P2 {
        let p = self.degree;
        if p > STACK_DEGREE {
            return self.eval(t)[0];
        }
        let i = self.span(t);
        let u = &self.knots;
        const W: usize = STACK_DEGREE + 1;
        let mut n = [0.0; W];
        let mut left = [0.0; W];
        let mut right = [0.0; W];
        n[0] = 1.0;
        for j in 1..=p {
            left[j] = t - u[i + 1 - j];
            right[j] = u[i + j] - t;
            let mut saved = 0.0;
            for r in 0..j {
                let sum = right[r + 1] + left[j - r];
                let temp = if sum == 0.0 { 0.0 } else { n[r] / sum };
                n[r] = saved + right[r + 1] * temp;
                saved = left[j - r] * temp;
            }
            n[j] = saved;
        }
        let (mut x, mut y, mut wsum) = (0.0, 0.0, 0.0);
        for (j, nj) in n.iter().take(p + 1).enumerate() {
            let index = i - p + j;
            let wi = self.weight(index);
            let c = self.control[index];
            x += nj * wi * c[0];
            y += nj * wi * c[1];
            wsum += nj * wi;
        }
        // As `eval` scales them, so that both give the same bits.
        scale([x, y], 1.0 / wsum)
    }

    /// Point and first and second derivatives at `t`.
    #[allow(clippy::needless_range_loop)]
    pub fn eval(&self, t: f64) -> [P2; 3] {
        let p = self.degree;
        let width = p + 1;
        let i = self.span(t);
        let mut stack = [0.0; 3 * (STACK_DEGREE + 1)];
        let mut heap = Vec::new();
        let n: &mut [f64] = if p <= STACK_DEGREE {
            &mut stack[..3 * width]
        } else {
            heap.resize(3 * width, 0.0);
            &mut heap
        };
        let order = self.basis(i, t, 2, n);
        // Homogeneous sums: A = Σ N w P, W = Σ N w and their derivatives.
        let mut a = [[0.0; 2]; 3];
        let mut w = [0.0; 3];
        for k in 0..=order {
            let row = &n[k * width..(k + 1) * width];
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
        let d2 = if order >= 2 {
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

    /// Inserts the knot `t` once (Boehm's algorithm; The NURBS Book, A5.1
    /// with one insertion, in homogeneous coordinates; mitcad#836): the same
    /// curve, at the same parameters, with one more control point. Clamped
    /// and unclamped knot vectors alike, so a closed spline stays closed. A
    /// value within `1e-12` of the domain length of an existing knot snaps
    /// to it. None outside the open domain (the ends of a clamped spline
    /// are control points already) and where the knot is there `degree`
    /// times already (the curve passes through a control point there).
    pub fn insert_knot(&self, t: f64) -> Option<KnotInsertion> {
        let p = self.degree;
        let n = self.control.len();
        if p == 0 || n <= p || self.knots.len() != n + p + 1 || !t.is_finite() {
            return None;
        }
        let (lo, hi) = self.domain();
        if hi <= lo {
            return None;
        }
        let snap = 1e-12 * (hi - lo).max(1.0);
        let t = self
            .knots
            .iter()
            .copied()
            .find(|k| (k - t).abs() <= snap)
            .unwrap_or(t);
        if t <= lo || t >= hi {
            return None;
        }
        let s = self.knots.iter().filter(|k| **k == t).count();
        if s >= p {
            return None;
        }
        // The span of t: its last knot at or before t, so `k >= p`, and the
        // new points are `k - p + 1..=k - s`.
        let k = self.span(t);
        let (first, last) = (k + 1 - p, k - s);
        let u = &self.knots;
        // Each point with its weight; the old ones as they were.
        let refined: Vec<(P2, f64)> = (0..=n)
            .map(|i| {
                if i < first {
                    (self.control[i], self.weight(i))
                } else if i > last {
                    (self.control[i - 1], self.weight(i - 1))
                } else {
                    // `u[i] <= t < u[i + p]`, so the span is not empty.
                    let alpha = (t - u[i]) / (u[i + p] - u[i]);
                    let (wa, wb) = ((1.0 - alpha) * self.weight(i - 1), alpha * self.weight(i));
                    let w = wa + wb;
                    let (a, b) = (self.control[i - 1], self.control[i]);
                    (
                        [(wa * a[0] + wb * b[0]) / w, (wa * a[1] + wb * b[1]) / w],
                        w,
                    )
                }
            })
            .collect();
        let mut knots = self.knots.clone();
        knots.insert(k + 1, t);
        let spline = Nurbs {
            degree: p,
            control: refined.iter().map(|(c, _)| *c).collect(),
            weights: if self.weights.is_empty() {
                Vec::new()
            } else {
                refined.iter().map(|(_, w)| *w).collect()
            },
            knots,
        };
        Some(KnotInsertion {
            spline,
            knot: t,
            first,
            last,
        })
    }

    /// Removes the control point `m` with one interior knot (Remove Point
    /// on a spline, mitcad#2326; the inverse of [`Nurbs::insert_knot`]).
    ///
    /// The knot is one whose removal changes the control points around
    /// `m`: of the interior knot values among `knots[m + 1..=m + degree]`
    /// (those `m`'s Greville abscissa averages), one that leaves the curve
    /// as it is wins (so that a point just inserted goes again exactly),
    /// else the one nearest the Greville abscissa. The other control points
    /// it changes (`moved`) are worked out from both sides in homogeneous
    /// coordinates (The NURBS Book, A5.8 with one removal): those before
    /// `m` from the unchanged point before them, those after `m` from the
    /// unchanged point after them, so the change stays within the knot's
    /// `degree + 1` spans. Where no such knot is there (the end point of a
    /// clamped spline, whose knots there are all at the end), the interior
    /// knot nearest the Greville abscissa goes and no other point moves:
    /// the next point becomes the end.
    ///
    /// None when fewer than `degree + 1` points would remain, for an index
    /// out of range and where no knot lies inside the domain.
    pub fn remove_control_point(&self, m: usize) -> Option<KnotRemoval> {
        let p = self.degree;
        let n = self.control.len();
        if p == 0 || n < p + 2 || m >= n || self.knots.len() != n + p + 1 {
            return None;
        }
        let (lo, hi) = self.domain();
        if hi <= lo {
            return None;
        }
        let u = &self.knots;
        let greville = u[m + 1..=m + p].iter().sum::<f64>() / p as f64;
        // Each distinct interior knot value: its last index and how often
        // it is there; the interior indices are `p + 1..n`.
        let mut values: Vec<(usize, usize)> = Vec::new();
        let mut i = p + 1;
        while i < n {
            let v = u[i];
            let mut r = i;
            while r + 1 < n && u[r + 1] == v {
                r += 1;
            }
            if v > lo && v < hi {
                values.push((r, u.iter().filter(|k| **k == v).count()));
            }
            i = r + 1;
        }
        if values.is_empty() {
            return None;
        }
        // The knots whose removal changes `m`, worked out.
        let mut best: Option<(bool, f64, f64, KnotRemoval)> = None;
        for &(r, s) in &values {
            if s > p || m + p < r || m + s > r {
                // Not within `r - p..=r - s`.
                continue;
            }
            let Some((removal, residual)) = self.without_knot(r, s, m) else {
                continue;
            };
            let key = (removal.exact, (u[r] - greville).abs(), residual);
            let better = best.as_ref().is_none_or(|(e, g, d, _)| {
                (key.0 && !*e) || (key.0 == *e && (key.1 < *g || (key.1 == *g && key.2 < *d)))
            });
            if better {
                best = Some((key.0, key.1, key.2, removal));
            }
        }
        if let Some((_, _, _, removal)) = best {
            return Some(removal);
        }
        // Nothing to work out: the knot nearest the Greville abscissa goes
        // with the point, the others stay where they are.
        let &(r, _) = values
            .iter()
            .min_by(|a, b| {
                (u[a.0] - greville)
                    .abs()
                    .total_cmp(&(u[b.0] - greville).abs())
            })
            .expect("one interior knot at least");
        let mut knots = u.clone();
        let knot = knots.remove(r);
        let mut control = self.control.clone();
        control.remove(m);
        let mut weights = self.weights.clone();
        if !weights.is_empty() {
            weights.remove(m);
        }
        Some(KnotRemoval {
            spline: Nurbs {
                degree: p,
                control,
                weights,
                knots,
            },
            knot,
            moved: Vec::new(),
            exact: false,
        })
    }

    /// The knot `knots[r]` (there `s` times, `r` its last index) removed
    /// with the control point `m` (in `r - p..=r - s`), the points between
    /// worked out from both sides; with the distance by which the curve
    /// misses the removed point's equation (0 when the knot was removable).
    /// None where a weight would not stay positive.
    fn without_knot(&self, r: usize, s: usize, m: usize) -> Option<(KnotRemoval, f64)> {
        let p = self.degree;
        let n = self.control.len();
        let (first, last) = (r - p, r - s);
        let mut knots = self.knots.clone();
        let v = knots.remove(r);
        let h = |i: usize| -> [f64; 3] {
            let w = self.weight(i);
            [self.control[i][0] * w, self.control[i][1] * w, w]
        };
        // Inserting `v` into the new spline gives the old: for `i` in
        // `first..=last`, old `i` = (1 - a_i) new `i - 1` + a_i new `i`.
        let alpha = |i: usize| (v - knots[i]) / (knots[i + p] - knots[i]);
        let mut q: Vec<[f64; 3]> = (0..n - 1)
            .map(|j| if j < first { h(j) } else { h(j + 1) })
            .collect();
        for i in first..m {
            let a = alpha(i);
            let (old, before) = (h(i), q[i - 1]);
            q[i] = std::array::from_fn(|k| (old[k] - (1.0 - a) * before[k]) / a);
        }
        for i in (m + 1..=last).rev() {
            let a = alpha(i);
            let (old, after) = (h(i), q[i]);
            q[i - 1] = std::array::from_fn(|k| (old[k] - a * after[k]) / (1.0 - a));
        }
        if q.iter()
            .any(|w| !(w[2] > 0.0 && w.iter().all(|x| x.is_finite())))
        {
            return None;
        }
        // How far the removed point's own equation is off.
        let a = alpha(m);
        let (before, after) = (q[m - 1], q[m]);
        let blend: [f64; 3] = std::array::from_fn(|k| (1.0 - a) * before[k] + a * after[k]);
        let old = h(m);
        let residual = dist(
            [old[0] / old[2], old[1] / old[2]],
            [blend[0] / blend[2], blend[1] / blend[2]],
        );
        let rational = !self.weights.is_empty();
        let spline = Nurbs {
            degree: p,
            control: q.iter().map(|w| [w[0] / w[2], w[1] / w[2]]).collect(),
            weights: if rational {
                q.iter().map(|w| w[2]).collect()
            } else {
                Vec::new()
            },
            knots,
        };
        let removal = KnotRemoval {
            spline,
            knot: v,
            moved: (first..=last).filter(|i| *i != m).collect(),
            exact: residual <= 1e-9 * self.size().max(1.0),
        };
        Some((removal, residual))
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

    /// The piece of a closed curve from `t0` on to its end and on from its
    /// start to `t1` (mitcad#783): the piece across its seam, as one clamped
    /// B-spline.
    pub fn piece_around(&self, t0: f64, t1: f64) -> Nurbs {
        let (lo, hi) = self.domain();
        let tiny = 1e-12 * (hi - lo);
        if t0 >= hi - tiny {
            self.piece(lo, t1)
        } else if t1 <= lo + tiny {
            self.piece(t0, hi)
        } else {
            join(&self.piece(t0, hi), &self.piece(lo, t1))
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
        /// Where the whole circle starts (its seam), as an angle.
        start: f64,
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
            Self::Circle { center, radius, .. } | Self::Arc { center, radius, .. } => {
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
        match self {
            // Without the derivatives (mitcad#878): the same point.
            Self::Nurbs(n) => n.point(t),
            _ => self.eval(t)[0],
        }
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
                let (lo, hi) = n.domain();
                if (spans * 8 * n.degree.max(1) > 512 || t1 > hi) && t1 > t0 {
                    // Many spans (a long contour of pieces, a QR code's,
                    // mitcad#473), or a stretch past the end of a closed
                    // curve: samples from knot to knot, the same number in
                    // each span, so that every corner where pieces meet is
                    // a sample (the knots past the end are the first ones
                    // again).
                    let period = hi - lo;
                    let mut cuts: Vec<f64> = n
                        .knots
                        .iter()
                        .flat_map(|k| [*k, *k + period])
                        .filter(|k| *k > t0 && *k < t1)
                        .collect();
                    cuts.sort_by(f64::total_cmp);
                    cuts.dedup();
                    cuts.insert(0, t0);
                    cuts.push(t1);
                    let spans = cuts.len() - 1;
                    let per = (8 * n.degree.max(1))
                        .max(16usize.div_ceil(spans))
                        .min((512 / spans).max(1));
                    let mut params: Vec<f64> = cuts
                        .windows(2)
                        .flat_map(|w| {
                            (0..per).map(move |i| w[0] + (w[1] - w[0]) * i as f64 / per as f64)
                        })
                        .collect();
                    params.push(t1);
                    return params;
                }
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
        let (t0, t1) = self.domain();
        match self {
            Self::Nurbs(n) => bounds(&n.control),
            Self::Line { .. } => bounds(&self.sample(t0, t1)),
            // Round curves: the ends and the extremes the range holds, not
            // the sampled points (a chord falls short of its arc's top,
            // where a tangent line touches it).
            _ => {
                let extremes: Vec<f64> = match self {
                    Self::Ellipse {
                        major,
                        minor,
                        rotation,
                        ..
                    }
                    | Self::EllipticalArc {
                        major,
                        minor,
                        rotation,
                        ..
                    } => {
                        let (rs, rc) = rotation.sin_cos();
                        let tx = (-minor * rs).atan2(major * rc);
                        let ty = (minor * rc).atan2(major * rs);
                        vec![tx, tx + PI, ty, ty + PI]
                    }
                    _ => vec![0.0, PI / 2.0, PI, 3.0 * PI / 2.0],
                };
                let mut points = vec![self.point(t0), self.point(t1)];
                for t in extremes {
                    if self.is_periodic() {
                        points.push(self.point(t));
                    } else {
                        let s = angle_after(t0 - 1e-12, t);
                        if s <= t1 {
                            points.push(self.point(s));
                        }
                    }
                }
                bounds(&points)
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
            Self::Circle { center, radius, .. } => {
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
                #[cfg(test)]
                WHOLE_SEARCHES.with(|n| n.set(n.get() + 1));
                // Samples, then Newton on (C(t) - p) . C'(t) = 0.
                let params = self.sample_params(lo, hi);
                let mut best = (lo, f64::INFINITY);
                for t in &params {
                    let d = dist(p, self.point(*t));
                    if d < best.1 {
                        best = (*t, d);
                    }
                }
                let (t, d) = self.newton_closest(p, best.0);
                if d <= best.1 { (t, d) } else { best }
            }
        }
    }

    /// A point of the curve near `p` found by Newton's method from the
    /// parameter `hint` alone, and its distance: a local closest point,
    /// for a caller that knows about where to look and only asks whether
    /// the curve passes within some distance (the distance found is never
    /// below the true closest distance). [`Curve2::closest`] samples the
    /// whole curve first, which on a long spline costs hundreds of
    /// evaluations (mitcad#878).
    pub fn closest_near(&self, p: P2, hint: f64) -> (f64, f64) {
        match self {
            Self::Line { .. } | Self::Circle { .. } | Self::Arc { .. } => self.closest(p),
            _ => {
                let (lo, hi) = self.domain();
                let hint = if self.is_periodic() {
                    hint.rem_euclid(TAU)
                } else {
                    hint.clamp(lo, hi)
                };
                let start = (hint, dist(p, self.point(hint)));
                let (t, d) = self.newton_closest(p, hint);
                if d <= start.1 { (t, d) } else { start }
            }
        }
    }

    /// Newton's method on (C(t) - p) . C'(t) = 0 from `t`: the parameter
    /// it ends at and the distance of its point from `p`.
    fn newton_closest(&self, p: P2, mut t: f64) -> (f64, f64) {
        let (lo, hi) = self.domain();
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
        (t, dist(p, self.point(t)))
    }

    /// The piece between parameters `t0 < t1` (for closed curves `t1` may
    /// pass the end of the domain and wrap) as profile geometry.
    pub fn piece(&self, t0: f64, t1: f64) -> SegmentGeometry {
        match self {
            Self::Line { .. } => SegmentGeometry::Line {
                start: self.point(t0),
                end: self.point(t1),
            },
            Self::Circle { center, radius, .. } | Self::Arc { center, radius, .. } => {
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
            Self::Circle {
                center,
                radius,
                start,
            } => SegmentGeometry::Circle {
                center: *center,
                radius: *radius,
                start: *start,
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

    /// A wavy spline of `count` control points, rational when `rational`.
    fn wavy(count: usize, degree: usize, rational: bool) -> Nurbs {
        Nurbs {
            degree,
            control: (0..count)
                .map(|i| [i as f64 * 2.0, if i % 2 == 0 { -3.0 } else { 3.0 }])
                .collect(),
            weights: if rational {
                (0..count).map(|i| 1.0 + 0.25 * (i % 3) as f64).collect()
            } else {
                Vec::new()
            },
            knots: clamped_uniform_knots(count, degree),
        }
    }

    #[test]
    fn a_point_alone_is_the_point_of_the_evaluation() {
        // The point without derivatives (mitcad#878) is the very point
        // `eval` gives, to the bit, on the stack and above its degrees.
        for degree in [1, 2, 3, 5, STACK_DEGREE, STACK_DEGREE + 2] {
            for rational in [false, true] {
                let spline = wavy(degree + 6, degree, rational);
                let (lo, hi) = spline.domain();
                for i in 0..=40 {
                    let t = lo + (hi - lo) * i as f64 / 40.0;
                    assert_eq!(spline.point(t), spline.eval(t)[0], "degree {degree} at {t}");
                    assert_eq!(Curve2::Nurbs(spline.clone()).point(t), spline.eval(t)[0]);
                }
            }
        }
        // Derivatives above the stack's degrees as below: a degree 17
        // spline's tangent agrees with finite differences.
        let spline = wavy(STACK_DEGREE + 8, STACK_DEGREE + 2, true);
        let t = 0.37;
        let h = 1e-6;
        let fd = scale(sub(spline.point(t + h), spline.point(t - h)), 0.5 / h);
        assert!(
            dist(fd, spline.eval(t)[1]) < 1e-4 * norm(fd).max(1.0),
            "{fd:?}"
        );
    }

    #[test]
    fn a_closest_point_near_a_hint() {
        let curve = Curve2::Nurbs(wavy(40, 3, false));
        let (lo, hi) = curve.domain();
        for i in 1..20 {
            let t = lo + (hi - lo) * i as f64 / 20.0;
            let p = curve.point(t);
            // From a hint a little off (a tenth of a span), the point
            // itself.
            let (found, d) = curve.closest_near(p, t + 0.002 * (hi - lo));
            assert!(d < 1e-9, "{t}: {found} at {d}");
            assert!((found - t).abs() < 1e-6);
            // The distance is that of a point of the curve: never below
            // the true closest distance.
            let off = add(p, [0.0, 0.5]);
            let (s, d) = curve.closest_near(off, t);
            assert_eq!(d, dist(off, curve.point(s)));
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

    /// The refined spline is the same curve at every parameter (within
    /// 1e-9) and has one more control point.
    fn same_curve(old: &Nurbs, refined: &KnotInsertion) {
        let new = &refined.spline;
        assert_eq!(new.control.len(), old.control.len() + 1);
        assert_eq!(new.knots.len(), old.knots.len() + 1);
        assert_eq!(new.domain(), old.domain());
        assert_eq!(new.weights.is_empty(), old.weights.is_empty());
        let (lo, hi) = old.domain();
        for i in 0..=200 {
            let t = lo + (hi - lo) * i as f64 / 200.0;
            let (a, b) = (old.eval(t)[0], new.eval(t)[0]);
            assert!(dist(a, b) < 1e-9, "at {t}: {a:?} {b:?}");
        }
        // The control points outside first..=last are the old ones.
        assert_eq!(new.control[..refined.first], old.control[..refined.first]);
        assert_eq!(new.control[refined.last + 1..], old.control[refined.last..]);
    }

    #[test]
    fn a_knot_inserted_keeps_the_curve() {
        // A cubic with uneven interior knots: in a span, at an existing
        // knot (twice, up to a corner) and near the ends.
        let cubic = Nurbs {
            degree: 3,
            control: vec![
                [0.0, 0.0],
                [10.0, 25.0],
                [30.0, 30.0],
                [45.0, -5.0],
                [60.0, 10.0],
                [75.0, 0.0],
            ],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.45, 1.0, 1.0, 1.0, 1.0],
        };
        for t in [0.01, 0.2, 0.3, 0.44, 0.7, 0.99] {
            let refined = cubic.insert_knot(t).expect("inside the domain");
            assert_eq!(refined.knot, t);
            assert_eq!(
                refined.last - refined.first + 1,
                if t == 0.3 { 2 } else { 3 }
            );
            same_curve(&cubic, &refined);
        }
        // A value next to a knot snaps to it.
        assert_eq!(cubic.insert_knot(0.3 + 1e-14).unwrap().knot, 0.3);
        // Twice at 0.3 is three times: the curve passes through a control
        // point there, and a fourth time is refused.
        let twice = cubic.insert_knot(0.3).unwrap().spline;
        let three = twice.insert_knot(0.3).unwrap();
        same_curve(&twice, &three);
        let on = cubic.eval(0.3)[0];
        assert!(three.spline.control.iter().any(|c| dist(*c, on) < 1e-9));
        assert!(three.spline.insert_knot(0.3).is_none());
        // The ends of a clamped spline and outside are refused.
        for t in [0.0, 1.0, -0.5, 1.5, f64::NAN] {
            assert!(cubic.insert_knot(t).is_none(), "{t}");
        }
        // Default knots (clamped uniform) and a line of degree 1, whose new
        // point is on it.
        let quadratic = Nurbs {
            degree: 2,
            control: vec![[0.0, 0.0], [5.0, 8.0], [12.0, 3.0], [20.0, 9.0]],
            weights: Vec::new(),
            knots: clamped_uniform_knots(4, 2),
        };
        same_curve(&quadratic, &quadratic.insert_knot(0.25).unwrap());
        let polyline = Nurbs {
            degree: 1,
            control: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
            weights: Vec::new(),
            knots: clamped_uniform_knots(3, 1),
        };
        let refined = polyline.insert_knot(0.25).unwrap();
        same_curve(&polyline, &refined);
        assert_eq!((refined.first, refined.last), (1, 1));
        assert!(close(refined.spline.control[1], [5.0, 0.0]));
    }

    #[test]
    fn a_knot_inserted_into_a_rational_spline_keeps_the_circle() {
        let arc = quarter_circle();
        for t in [0.1, 0.5, 0.8] {
            let refined = arc.insert_knot(t).unwrap();
            same_curve(&arc, &refined);
            assert_eq!(refined.spline.weights.len(), 4);
            for i in 0..=20 {
                let p = refined.spline.eval(i as f64 / 20.0)[0];
                assert!((norm(p) - 1.0).abs() < 1e-12, "{p:?}");
            }
        }
    }

    #[test]
    fn a_knot_inserted_into_a_closed_spline_keeps_it_closed() {
        // Periodic: a uniform unclamped cubic whose last three control
        // points are its first three, a smooth closed curve on [3, 8].
        let ring = [
            [0.0, -20.0],
            [20.0, -15.0],
            [25.0, 5.0],
            [5.0, 22.0],
            [-18.0, 10.0],
        ];
        let mut control = ring.to_vec();
        control.extend_from_slice(&ring[..3]);
        let periodic = Nurbs {
            degree: 3,
            control,
            weights: Vec::new(),
            knots: (0..12).map(f64::from).collect(),
        };
        assert!(periodic.is_closed());
        let (lo, hi) = periodic.domain();
        assert_eq!((lo, hi), (3.0, 8.0));
        for t in [3.001, 3.5, 5.0, 6.25, 7.999] {
            let refined = periodic.insert_knot(t).unwrap();
            same_curve(&periodic, &refined);
            assert!(refined.spline.is_closed(), "{t}");
            // Smooth where it closes: the same tangent at both ends.
            let (a, b) = (refined.spline.eval(lo)[1], refined.spline.eval(hi)[1]);
            assert!(dist(a, b) < 1e-9, "{t}: {a:?} {b:?}");
        }
        // Clamped and closed: the first control point is the last.
        let clamped = Nurbs {
            degree: 3,
            control: vec![
                [0.0, 0.0],
                [30.0, -5.0],
                [40.0, 20.0],
                [5.0, 18.0],
                [-10.0, 8.0],
                [0.0, 0.0],
            ],
            weights: Vec::new(),
            knots: clamped_uniform_knots(6, 3),
        };
        for t in [0.05, 0.5, 0.95] {
            let refined = clamped.insert_knot(t).unwrap();
            same_curve(&clamped, &refined);
            assert!(refined.spline.is_closed(), "{t}");
            // Its ends stay where they were.
            let last = refined.spline.control.len() - 1;
            assert_eq!(refined.spline.control[0], [0.0, 0.0]);
            assert_eq!(refined.spline.control[last], [0.0, 0.0]);
            assert!(refined.first >= 1 && refined.last < last);
        }
    }

    /// The curves agree within `tolerance` at sampled parameters.
    fn agree(a: &Nurbs, b: &Nurbs, tolerance: f64) {
        assert_eq!(a.domain(), b.domain());
        let (lo, hi) = a.domain();
        for i in 0..=200 {
            let t = lo + (hi - lo) * i as f64 / 200.0;
            let (p, q) = (a.eval(t)[0], b.eval(t)[0]);
            assert!(dist(p, q) < tolerance, "at {t}: {p:?} {q:?}");
        }
    }

    #[test]
    fn a_control_point_removed_undoes_its_insertion() {
        // Mitcad#2326: a knot inserted and the new point removed again
        // give back the spline, knots and control points, open, rational
        // and closed alike.
        let cubic = Nurbs {
            degree: 3,
            control: vec![
                [0.0, 0.0],
                [10.0, 25.0],
                [30.0, 30.0],
                [45.0, -5.0],
                [60.0, 10.0],
                [75.0, 0.0],
            ],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.3, 0.45, 1.0, 1.0, 1.0, 1.0],
        };
        let closed = Nurbs {
            degree: 3,
            control: vec![
                [0.0, 0.0],
                [30.0, -5.0],
                [40.0, 20.0],
                [5.0, 18.0],
                [-10.0, 8.0],
                [0.0, 0.0],
            ],
            weights: Vec::new(),
            knots: clamped_uniform_knots(6, 3),
        };
        for spline in [cubic, quarter_circle(), closed] {
            for t in [0.05, 0.2, 0.31, 0.5, 0.7, 0.95] {
                let refined = spline.insert_knot(t).unwrap();
                let new = &refined.spline;
                // The new point is the one nearest the curve at the knot,
                // as Insert Point picks it.
                let on = new.eval(refined.knot)[0];
                let fresh = (refined.first..=refined.last)
                    .min_by(|a, b| dist(new.control[*a], on).total_cmp(&dist(new.control[*b], on)))
                    .unwrap();
                let back = new.remove_control_point(fresh).unwrap();
                assert!(back.exact, "{t}");
                assert_eq!(back.knot, refined.knot);
                assert_eq!(back.spline.knots, spline.knots);
                assert_eq!(back.spline.weights.len(), spline.weights.len());
                for (a, b) in back.spline.control.iter().zip(&spline.control) {
                    assert!(dist(*a, *b) < 1e-9, "{t}: {a:?} {b:?}");
                }
                agree(&spline, &back.spline, 1e-9);
            }
        }
    }

    #[test]
    fn a_control_point_removed_changes_the_curve_near_it_only() {
        for rational in [false, true] {
            // Twelve points of a cubic: knots at i / 9 inside.
            let cubic = wavy(12, 3, rational);
            // An interior point: one knot fewer, its neighbours moved, the
            // points far from it and both ends where they were.
            let removed = cubic.remove_control_point(6).unwrap();
            let new = &removed.spline;
            assert_eq!(new.control.len(), 11);
            assert_eq!(new.knots.len(), 15);
            assert_eq!(new.weights.len(), if rational { 11 } else { 0 });
            assert_eq!(new.domain(), cubic.domain());
            assert!(!removed.exact);
            // The knot nearest the point's Greville abscissa (5 / 9).
            let r = cubic.knots.iter().position(|k| *k == removed.knot).unwrap();
            assert_eq!(r, 8);
            assert_eq!(removed.moved, vec![5, 7]);
            for i in 0..12 {
                if i == 6 || removed.moved.contains(&i) {
                    continue;
                }
                let j = if i < 6 { i } else { i - 1 };
                assert_eq!(new.control[j], cubic.control[i], "{i}");
            }
            // Outside the removed knot's spans the curve is as it was.
            let (from, to) = (cubic.knots[r - 3], cubic.knots[r + 3]);
            for i in 0..=90 {
                let t = i as f64 / 90.0;
                if t > from && t < to {
                    continue;
                }
                assert!(dist(cubic.eval(t)[0], new.eval(t)[0]) < 1e-9, "{t}");
            }
            // An end point: the next point becomes the end, nothing moves.
            let start = cubic.remove_control_point(0).unwrap();
            assert!(start.moved.is_empty());
            assert_eq!(start.spline.control, cubic.control[1..].to_vec());
            assert!(dist(start.spline.eval(0.0)[0], cubic.control[1]) < 1e-12);
            let end = cubic.remove_control_point(11).unwrap();
            assert_eq!(end.spline.control, cubic.control[..11].to_vec());
            assert!(dist(end.spline.eval(1.0)[0], cubic.control[10]) < 1e-12);
            assert!(cubic.remove_control_point(12).is_none());
        }
        // Too few points would remain: degree + 1 is the least.
        assert!(wavy(4, 3, false).remove_control_point(1).is_none());
        let five = wavy(5, 3, false).remove_control_point(2).unwrap();
        assert_eq!(five.spline.control.len(), 4);
    }

    #[test]
    fn closest_points() {
        let circle = Curve2::Circle {
            center: [1.0, 1.0],
            radius: 2.0,
            start: 0.0,
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
