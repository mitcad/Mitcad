// SPDX-License-Identifier: MIT
//! Rational B-spline evaluation (Cox-de Boor recursion), generic over
//! [`Scalar`] so that residuals through a spline are differentiated exactly,
//! including with respect to the curve parameter.

use crate::scalar::{Scalar, V2};

/// Degree, knots and weights of a spline; fixed while solving.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SplineBasis {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub weights: Vec<f64>,
}

/// Curve point and derivatives at a parameter.
pub(crate) struct CurveEval<T> {
    pub p: V2<T>,
    pub d1: V2<T>,
    pub d2: V2<T>,
}

impl SplineBasis {
    pub fn n_ctrl(&self) -> usize {
        self.weights.len()
    }

    /// Parameter range of the curve.
    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.n_ctrl()])
    }

    /// True when the first and last `degree + 1` knots are equal, so the curve
    /// starts at the first control point tangent to the first control leg
    /// (and likewise at the end).
    pub fn is_clamped(&self) -> bool {
        let p = self.degree;
        let k = &self.knots;
        let m = k.len() - 1;
        (0..p).all(|i| k[i] == k[p]) && (0..p).all(|i| k[m - i] == k[m - p])
    }

    /// Knot span index `s` (`knots[s] <= t < knots[s + 1]`, `degree <= s < n_ctrl`).
    pub fn span(&self, t: f64) -> usize {
        let p = self.degree;
        let n = self.n_ctrl() - 1;
        if t >= self.knots[n + 1] {
            // Last non-empty span.
            let mut s = n;
            while s > p && self.knots[s] == self.knots[s + 1] {
                s -= 1;
            }
            return s;
        }
        if t <= self.knots[p] {
            let mut s = p;
            while s < n && self.knots[s] == self.knots[s + 1] {
                s += 1;
            }
            return s;
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

    /// Basis functions of degree `k` on span `s` from those of degree `k - 1`.
    fn raise<T: Scalar>(&self, s: usize, k: usize, t: T, prev: &[T]) -> Vec<T> {
        let u = &self.knots;
        let mut out = vec![T::cst(0.0); k + 1];
        for (r, o) in out.iter_mut().enumerate() {
            let i = s + r - k;
            let mut acc = T::cst(0.0);
            if r >= 1 {
                let den = u[i + k] - u[i];
                if den != 0.0 {
                    acc = acc + (t - T::cst(u[i])) * prev[r - 1].mulf(1.0 / den);
                }
            }
            if r < k {
                let den = u[i + k + 1] - u[i + 1];
                if den != 0.0 {
                    acc = acc + (T::cst(u[i + k + 1]) - t) * prev[r].mulf(1.0 / den);
                }
            }
            *o = acc;
        }
        out
    }

    /// Derivatives of the degree `k` basis functions on span `s`, given the
    /// (derivatives of the) degree `k - 1` functions `lower`.
    fn derive<T: Scalar>(&self, s: usize, k: usize, lower: &[T]) -> Vec<T> {
        let u = &self.knots;
        let mut out = vec![T::cst(0.0); k + 1];
        for (r, o) in out.iter_mut().enumerate() {
            let i = s + r - k;
            let mut acc = T::cst(0.0);
            if r >= 1 {
                let den = u[i + k] - u[i];
                if den != 0.0 {
                    acc = acc + lower[r - 1].mulf(k as f64 / den);
                }
            }
            if r < k {
                let den = u[i + k + 1] - u[i + 1];
                if den != 0.0 {
                    acc = acc - lower[r].mulf(k as f64 / den);
                }
            }
            *o = acc;
        }
        out
    }

    /// Evaluates the curve with control points `ctrl` (all of them) at `t`.
    /// `ders` is the highest derivative needed (0, 1 or 2); higher ones are
    /// zero in the result.
    pub fn eval<T: Scalar>(&self, ctrl: &[V2<T>], t: T, ders: usize) -> CurveEval<T> {
        let p = self.degree;
        let s = self.span(t.val());
        // Basis tables of degree 0..=p.
        let mut tables: Vec<Vec<T>> = Vec::with_capacity(p + 1);
        tables.push(vec![T::cst(1.0)]);
        for k in 1..=p {
            let next = self.raise(s, k, t, &tables[k - 1]);
            tables.push(next);
        }
        let zero = T::cst(0.0);
        let n0 = &tables[p];
        let n1 = if ders >= 1 && p >= 1 {
            self.derive(s, p, &tables[p - 1])
        } else {
            vec![zero; p + 1]
        };
        let n2 = if ders >= 2 && p >= 2 {
            let lower = self.derive(s, p - 1, &tables[p - 2]);
            self.derive(s, p, &lower)
        } else {
            vec![zero; p + 1]
        };
        // Homogeneous sums A^(k) = sum N^(k) w P, W^(k) = sum N^(k) w.
        let mut a = [V2::new(zero, zero); 3];
        let mut w = [zero; 3];
        for r in 0..=p {
            let i = s - p + r;
            let wi = self.weights[i];
            for (k, n) in [n0, &n1, &n2].iter().enumerate() {
                let c = n[r].mulf(wi);
                a[k] = a[k].add(ctrl[i].mul(c));
                w[k] = w[k] + c;
            }
        }
        let pt = V2::new(a[0].x / w[0], a[0].y / w[0]);
        let d1 = a[1].sub(pt.mul(w[1]));
        let d1 = V2::new(d1.x / w[0], d1.y / w[0]);
        let d2 = a[2].sub(d1.mul(w[1]).mulf(2.0)).sub(pt.mul(w[2]));
        let d2 = V2::new(d2.x / w[0], d2.y / w[0]);
        CurveEval { p: pt, d1, d2 }
    }

    /// Index range of the control points that influence the curve at `t`.
    pub fn support(&self, t: f64) -> std::ops::RangeInclusive<usize> {
        let s = self.span(t);
        s - self.degree..=s
    }
}

/// Clamped knot vector with uniformly spaced interior knots on [0, 1].
pub(crate) fn clamped_uniform_knots(n_ctrl: usize, degree: usize) -> Vec<f64> {
    let mut k = vec![0.0; degree + 1];
    let segments = n_ctrl - degree;
    for i in 1..segments {
        k.push(i as f64 / segments as f64);
    }
    k.extend(std::iter::repeat_n(1.0, degree + 1));
    k
}

/// Cubic interpolation through `fit` points: chord length parameters, the
/// knot vector (interior knots at the interior parameters) and control points
/// `[fit[0], h_1, ..., h_{k+1}, fit[k]]` with natural end conditions
/// (zero second derivative at both ends).
pub(crate) struct Interpolation {
    pub params: Vec<f64>,
    pub basis: SplineBasis,
    pub ctrl: Vec<[f64; 2]>,
}

pub(crate) fn interpolate(fit: &[[f64; 2]]) -> Option<Interpolation> {
    let k = fit.len().checked_sub(1).filter(|&k| k >= 1)?;
    let mut params = vec![0.0];
    let mut total = 0.0;
    for w in fit.windows(2) {
        total += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt();
        params.push(total);
    }
    if total <= 0.0 {
        return None;
    }
    for t in &mut params {
        *t /= total;
    }
    for w in params.windows(2) {
        if w[1] - w[0] <= 1e-12 {
            return None;
        }
    }
    let p = 3;
    let n_ctrl = k + 3;
    let mut knots = vec![0.0; p + 1];
    knots.extend_from_slice(&params[1..k]);
    knots.extend(std::iter::repeat_n(1.0, p + 1));
    let basis = SplineBasis {
        degree: p,
        knots,
        weights: vec![1.0; n_ctrl],
    };
    // Linear system in all control points: rows are the end points, the
    // interior fit points and the two natural end conditions.
    let mut rows: Vec<(Vec<f64>, [f64; 2])> = Vec::new();
    let unit = |i: usize| {
        let mut v = vec![0.0; n_ctrl];
        v[i] = 1.0;
        v
    };
    rows.push((unit(0), fit[0]));
    rows.push((unit(n_ctrl - 1), fit[k]));
    let basis_row = |t: f64, der: usize| {
        let mut row = vec![0.0; n_ctrl];
        for (i, r) in row.iter_mut().enumerate() {
            let ctrl: Vec<V2<f64>> = (0..n_ctrl)
                .map(|j| V2::new(if j == i { 1.0 } else { 0.0 }, 0.0))
                .collect();
            let e = basis.eval(&ctrl, t, der);
            *r = match der {
                0 => e.p.x,
                _ => e.d2.x,
            };
        }
        row
    };
    for (j, &t) in params.iter().enumerate().take(k).skip(1) {
        rows.push((basis_row(t, 0), fit[j]));
    }
    rows.push((basis_row(0.0, 2), [0.0, 0.0]));
    rows.push((basis_row(1.0, 2), [0.0, 0.0]));
    let mut a: Vec<Vec<f64>> = rows.iter().map(|r| r.0.clone()).collect();
    let mut b: Vec<[f64; 2]> = rows.iter().map(|r| r.1).collect();
    let ctrl = solve_dense(&mut a, &mut b)?;
    Some(Interpolation {
        params,
        basis,
        ctrl,
    })
}

/// Gaussian elimination with partial pivoting for a small square system with
/// two right-hand sides.
fn solve_dense(a: &mut [Vec<f64>], b: &mut [[f64; 2]]) -> Option<Vec<[f64; 2]>> {
    let n = a.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-14 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        let (top, rest) = a.split_at_mut(c + 1);
        let pivot_row = &top[c];
        for (k, row) in rest.iter_mut().enumerate() {
            let r = c + 1 + k;
            let f = row[c] / pivot_row[c];
            if f != 0.0 {
                for (x, &p) in row[c..n].iter_mut().zip(&pivot_row[c..n]) {
                    *x -= f * p;
                }
                b[r][0] -= f * b[c][0];
                b[r][1] -= f * b[c][1];
            }
        }
    }
    let mut x = vec![[0.0; 2]; n];
    for c in (0..n).rev() {
        let mut s = b[c];
        for k in c + 1..n {
            s[0] -= a[c][k] * x[k][0];
            s[1] -= a[c][k] * x[k][1];
        }
        x[c] = [s[0] / a[c][c], s[1] / a[c][c]];
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::Dual;

    fn pts(c: &[[f64; 2]]) -> Vec<V2<f64>> {
        c.iter().map(|p| V2::new(p[0], p[1])).collect()
    }

    #[test]
    fn bezier_matches_bernstein() {
        let b = SplineBasis {
            degree: 3,
            knots: clamped_uniform_knots(4, 3),
            weights: vec![1.0; 4],
        };
        let c = [[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]];
        let t = 0.3;
        let e = b.eval(&pts(&c), t, 2);
        let s = 1.0 - t;
        let bern = [s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t];
        let x: f64 = (0..4).map(|i| bern[i] * c[i][0]).sum();
        let y: f64 = (0..4).map(|i| bern[i] * c[i][1]).sum();
        assert!((e.p.x - x).abs() < 1e-12 && (e.p.y - y).abs() < 1e-12);
        // Derivatives against finite differences.
        let h = 1e-5;
        let ep = b.eval(&pts(&c), t + h, 0).p;
        let em = b.eval(&pts(&c), t - h, 0).p;
        assert!((e.d1.x - (ep.x - em.x) / (2.0 * h)).abs() < 1e-7);
        assert!((e.d2.y - (ep.y - 2.0 * e.p.y + em.y) / (h * h)).abs() < 1e-3);
        // End tangent along the first control leg.
        let e0 = b.eval(&pts(&c), 0.0, 1);
        assert!((e0.d1.x - 3.0).abs() < 1e-12 && (e0.d1.y - 6.0).abs() < 1e-12);
    }

    #[test]
    fn rational_derivatives_match_finite_differences() {
        let b = SplineBasis {
            degree: 2,
            knots: vec![0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0],
            weights: vec![1.0, 0.7, 1.6, 1.0],
        };
        let c = [[0.0, 0.0], [1.0, 2.0], [3.0, 2.5], [4.0, 0.0]];
        for &t in &[0.1, 0.39, 0.41, 0.8] {
            let h = 1e-5;
            let e = b.eval(&pts(&c), t, 2);
            let ep = b.eval(&pts(&c), t + h, 1);
            let em = b.eval(&pts(&c), t - h, 1);
            assert!((e.d1.x - (ep.p.x - em.p.x) / (2.0 * h)).abs() < 1e-6);
            assert!((e.d2.y - (ep.d1.y - em.d1.y) / (2.0 * h)).abs() < 1e-5);
            // Dual numbers carry the parameter derivative too.
            let cd: Vec<V2<Dual>> = c
                .iter()
                .map(|p| V2::new(Dual::var(p[0], None), Dual::var(p[1], None)))
                .collect();
            let ed = b.eval(&cd, Dual::var(t, Some(0)), 0);
            assert!((ed.p.x.d[0] - e.d1.x).abs() < 1e-10);
        }
    }

    #[test]
    fn interpolation_passes_through_points() {
        let fit = [[0.0, 0.0], [10.0, 5.0], [20.0, 0.0], [30.0, 8.0]];
        let it = interpolate(&fit).expect("interpolates");
        let c = pts(&it.ctrl);
        for (j, &t) in it.params.iter().enumerate() {
            let e = it.basis.eval(&c, t, 2);
            assert!((e.p.x - fit[j][0]).abs() < 1e-9 && (e.p.y - fit[j][1]).abs() < 1e-9);
        }
        let e0 = it.basis.eval(&c, 0.0, 2);
        assert!(e0.d2.x.abs() < 1e-9 && e0.d2.y.abs() < 1e-9);
    }
}
