// SPDX-License-Identifier: MIT
//! Residual equations. Each equation is one scalar residual over a small set
//! of unknowns (its local variables), written generically over [`Scalar`] so
//! that the Jacobian comes from forward-mode differentiation.
//!
//! Residuals are lengths in millimetres. Direction-only residuals (unit
//! vectors, angles) are multiplied by the sketch size `scale` so that one
//! tolerance fits all.

use std::sync::Arc;

use crate::scalar::{Scalar, V2, wrap_angle};
use crate::spline::SplineBasis;

/// Local indices of a point's x and y.
pub(crate) type P = [usize; 2];

/// A radius: a scalar unknown or the distance between two points.
#[derive(Clone, Debug)]
pub(crate) enum Rad {
    Param(usize),
    Dist(P, P),
}

/// Spline parameter: fixed or a local unknown.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TParam {
    Const(f64),
    Var(usize),
}

#[derive(Clone, Debug)]
pub(crate) enum SplineEq {
    /// `(p - C(t))[axis]`
    Coord { p: P, axis: usize },
    /// `C''(t)[axis]`
    Second { axis: usize },
    /// `C(t)` on the line `a b`.
    OnLine { a: P, b: P },
    /// `C'(t)` parallel to the line `a b`.
    TanLine { a: P, b: P },
    /// `C(t)` on the circle.
    OnCircle { c: P, r: Rad },
    /// `C'(t)` perpendicular to the radius at `C(t)`.
    TanCircle { c: P },
}

/// Signed curvature of a curve at one of its end points, for the
/// parametrization that starts there and runs into the curve.
#[derive(Clone, Debug)]
pub(crate) enum CurvTerm {
    /// A line.
    Zero,
    /// An arc with centre `c` and radius `|s - c|`; `sign` is +1 at the start
    /// (counter-clockwise into the arc) and -1 at the end.
    Arc { c: P, s: P, sign: f64 },
    /// A spline at parameter `t` (an end of its range); `sign` is +1 at the
    /// start and -1 at the end.
    Spline {
        basis: Arc<SplineBasis>,
        ctrl: Vec<P>,
        t: f64,
        sign: f64,
    },
}

impl CurvTerm {
    fn eval<T: Scalar>(&self, x: &[T]) -> T {
        match self {
            CurvTerm::Zero => T::cst(0.0),
            CurvTerm::Arc { c, s, sign } => T::cst(*sign) / pt(x, *s).sub(pt(x, *c)).norm(),
            CurvTerm::Spline {
                basis,
                ctrl,
                t,
                sign,
            } => {
                let cp: Vec<V2<T>> = ctrl.iter().map(|p| pt(x, *p)).collect();
                let e = basis.eval(&cp, T::cst(*t), 2);
                let n = e.d1.norm();
                e.d1.cross(e.d2).mulf(*sign) / (n * n * n)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum EqKind {
    /// `x[a] - x[b] - side * value`
    Diff { a: usize, b: usize, side: f64 },
    /// `x[a] - value`
    Fixed { a: usize },
    /// `2 x[m] - x[a] - x[b]`
    Mid { m: usize, a: usize, b: usize },
    /// `|q - p| - value`
    Distance { p: P, q: P },
    /// Signed distance of `p` from the line `a b` (positive on the left)
    /// minus `side * value`.
    PointLine { p: P, a: P, b: P, side: f64 },
    /// Signed distance of the midpoint of `b1 b2` from the line `a1 a2`
    /// minus `side * value`.
    LineLine {
        a1: P,
        a2: P,
        b1: P,
        b2: P,
        side: f64,
    },
    /// `unit(a2 - a1) x (b2 - b1)`
    Parallel { a1: P, a2: P, b1: P, b2: P },
    /// `unit(a2 - a1) . (b2 - b1)`
    Perpendicular { a1: P, a2: P, b1: P, b2: P },
    /// `unit(a2 - a1) x unit(b2 - b1) * scale`
    DirParallel { a1: P, a2: P, b1: P, b2: P },
    /// `unit(a2 - a1) . unit(b2 - b1) * scale`
    DirPerp { a1: P, a2: P, b1: P, b2: P },
    /// `|a2 - a1| - |b2 - b1|`
    EqualLength { a1: P, a2: P, b1: P, b2: P },
    /// `r_a - r_b`
    RadDiff { a: Rad, b: Rad },
    /// `factor * r - value`
    Radius { r: Rad, factor: f64 },
    /// `|p - c| - r`
    OnCircle { p: P, c: P, r: Rad },
    /// Oriented angle from `a2 - a1` to `flip (b2 - b1)` minus
    /// `sigma * value`, wrapped, times `scale`.
    Angle {
        a1: P,
        a2: P,
        b1: P,
        b2: P,
        flip: f64,
        sigma: f64,
    },
    /// Point `m` at the midpoint of the arc `c s e` (counter-clockwise).
    ArcMid { m: P, c: P, s: P, e: P, axis: usize },
    /// Arc length minus `value`.
    ArcLen { c: P, s: P, e: P },
    /// Midpoint of `p q` on the line `a b`.
    SymMid { p: P, q: P, a: P, b: P },
    /// `p q` perpendicular to the line `a b`.
    SymPerp { p: P, q: P, a: P, b: P },
    /// The direction from `c` to `e` is the mirror image (about the line
    /// `a b`) of the direction from `rc` to `rp`.
    MirrorDir {
        c: P,
        e: P,
        rc: P,
        rp: P,
        a: P,
        b: P,
    },
    /// Signed distance of the centre from the line minus `side * r`.
    TanLineCircle { a: P, b: P, c: P, r: Rad, side: f64 },
    /// Line `a b` perpendicular to the radius at the contact point `e`.
    TanLineAt { a: P, b: P, e: P, c: P },
    /// External (`internal == 0`): `|c2 - c1| - (r1 + r2)`; internal:
    /// `|c2 - c1| - internal * (r1 - r2)`.
    TanCircles {
        c1: P,
        r1: Rad,
        c2: P,
        r2: Rad,
        internal: f64,
    },
    /// Centres collinear with the contact point `e`.
    TanCirclesAt { e: P, c1: P, c2: P },
    /// `(p - E(t))[axis]` for the ellipse with centre `c`, major axis end
    /// `a` and minor radius `b`.
    Ellipse {
        p: P,
        c: P,
        a: P,
        b: usize,
        t: usize,
        axis: usize,
    },
    /// Curvature continuity at a join: the curvatures of both curves (each
    /// parametrized from the join into the curve) cancel. Times `scale^2`.
    Curvature { a: CurvTerm, b: CurvTerm },
    /// Equations through a spline. `ctrl[i]` is control point `first + i`.
    Spline {
        basis: Arc<SplineBasis>,
        first: usize,
        ctrl: Vec<P>,
        t: TParam,
        what: SplineEq,
    },
    /// `x[b] - x[a] - by` (a translated copy, per axis).
    Shift { a: usize, b: usize, by: f64 },
    /// `(q - c - R (p - c))[axis]` with `R` the rotation by the angle of
    /// `(cos, sin)` (a turned copy).
    Turned {
        c: P,
        p: P,
        q: P,
        cos: f64,
        sin: f64,
        axis: usize,
    },
    /// `unit(b - cb) x R (a - ca)`: the direction from `cb` to `b` is that
    /// from `ca` to `a` turned by the angle of `(cos, sin)`.
    TurnedDir {
        ca: P,
        a: P,
        cb: P,
        b: P,
        cos: f64,
        sin: f64,
    },
}

fn pt<T: Scalar>(x: &[T], p: P) -> V2<T> {
    V2::new(x[p[0]], x[p[1]])
}

fn rad<T: Scalar>(x: &[T], r: &Rad) -> T {
    match r {
        Rad::Param(i) => x[*i],
        Rad::Dist(c, s) => pt(x, *s).sub(pt(x, *c)).norm(),
    }
}

/// Signed distance of `p` from the line through `a` and `b`.
fn sdist<T: Scalar>(p: V2<T>, a: V2<T>, b: V2<T>) -> T {
    b.sub(a).unit().cross(p.sub(a))
}

/// Adds the constant `shift` to a scalar without changing its derivatives.
fn shifted<T: Scalar>(v: T, target: f64) -> T {
    v - T::cst(v.val()) + T::cst(target)
}

impl EqKind {
    /// Residual for local variables `x`, target `value` and sketch `scale`.
    pub fn residual<T: Scalar>(&self, x: &[T], value: f64, scale: f64) -> T {
        use EqKind::*;
        match self {
            Diff { a, b, side } => x[*a] - x[*b] - T::cst(side * value),
            Fixed { a } => x[*a] - T::cst(value),
            Mid { m, a, b } => x[*m].mulf(2.0) - x[*a] - x[*b],
            Distance { p, q } => pt(x, *q).sub(pt(x, *p)).norm() - T::cst(value),
            PointLine { p, a, b, side } => {
                sdist(pt(x, *p), pt(x, *a), pt(x, *b)) - T::cst(side * value)
            }
            LineLine {
                a1,
                a2,
                b1,
                b2,
                side,
            } => {
                let mid = pt(x, *b1).add(pt(x, *b2)).mulf(0.5);
                sdist(mid, pt(x, *a1), pt(x, *a2)) - T::cst(side * value)
            }
            Parallel { a1, a2, b1, b2 } => {
                let u = pt(x, *a2).sub(pt(x, *a1)).unit();
                u.cross(pt(x, *b2).sub(pt(x, *b1)))
            }
            Perpendicular { a1, a2, b1, b2 } => {
                let u = pt(x, *a2).sub(pt(x, *a1)).unit();
                u.dot(pt(x, *b2).sub(pt(x, *b1)))
            }
            DirParallel { a1, a2, b1, b2 } => {
                let u = pt(x, *a2).sub(pt(x, *a1)).unit();
                let v = pt(x, *b2).sub(pt(x, *b1)).unit();
                u.cross(v).mulf(scale)
            }
            DirPerp { a1, a2, b1, b2 } => {
                let u = pt(x, *a2).sub(pt(x, *a1)).unit();
                let v = pt(x, *b2).sub(pt(x, *b1)).unit();
                u.dot(v).mulf(scale)
            }
            EqualLength { a1, a2, b1, b2 } => {
                pt(x, *a2).sub(pt(x, *a1)).norm() - pt(x, *b2).sub(pt(x, *b1)).norm()
            }
            RadDiff { a, b } => rad(x, a) - rad(x, b),
            Radius { r, factor } => rad(x, r).mulf(*factor) - T::cst(value),
            OnCircle { p, c, r } => pt(x, *p).sub(pt(x, *c)).norm() - rad(x, r),
            Angle {
                a1,
                a2,
                b1,
                b2,
                flip,
                sigma,
            } => {
                let da = pt(x, *a2).sub(pt(x, *a1));
                let db = pt(x, *b2).sub(pt(x, *b1)).mulf(*flip);
                let theta = da.cross(db).atan2(da.dot(db));
                let target = wrap_angle(theta.val() - sigma * value);
                shifted(theta, target).mulf(scale)
            }
            ArcMid { m, c, s, e, axis } => {
                let (cv, sv) = (pt(x, *c), pt(x, *s));
                let ch = pt(x, *e).sub(sv);
                let dir = V2::new(ch.y, -ch.x).unit();
                let r = sv.sub(cv).norm();
                pt(x, *m).sub(cv.add(dir.mul(r))).get(*axis)
            }
            ArcLen { c, s, e } => {
                let cv = pt(x, *c);
                let (vs, ve) = (pt(x, *s).sub(cv), pt(x, *e).sub(cv));
                let theta = vs.cross(ve).atan2(vs.dot(ve));
                let sweep = if theta.val() <= 0.0 {
                    theta + T::cst(std::f64::consts::TAU)
                } else {
                    theta
                };
                vs.norm() * sweep - T::cst(value)
            }
            SymMid { p, q, a, b } => {
                let mid = pt(x, *p).add(pt(x, *q)).mulf(0.5);
                sdist(mid, pt(x, *a), pt(x, *b))
            }
            SymPerp { p, q, a, b } => {
                let u = pt(x, *b).sub(pt(x, *a)).unit();
                u.dot(pt(x, *q).sub(pt(x, *p)))
            }
            MirrorDir { c, e, rc, rp, a, b } => {
                let u = pt(x, *b).sub(pt(x, *a)).unit();
                let r = pt(x, *rp).sub(pt(x, *rc));
                let m = u.mul(r.dot(u)).mulf(2.0).sub(r);
                pt(x, *e).sub(pt(x, *c)).unit().cross(m)
            }
            TanLineCircle { a, b, c, r, side } => {
                sdist(pt(x, *c), pt(x, *a), pt(x, *b)) - rad(x, r).mulf(*side)
            }
            TanLineAt { a, b, e, c } => {
                let u = pt(x, *b).sub(pt(x, *a)).unit();
                u.dot(pt(x, *e).sub(pt(x, *c)))
            }
            TanCircles {
                c1,
                r1,
                c2,
                r2,
                internal,
            } => {
                let d = pt(x, *c2).sub(pt(x, *c1)).norm();
                if *internal == 0.0 {
                    d - (rad(x, r1) + rad(x, r2))
                } else {
                    d - (rad(x, r1) - rad(x, r2)).mulf(*internal)
                }
            }
            TanCirclesAt { e, c1, c2 } => {
                let ev = pt(x, *e);
                ev.sub(pt(x, *c1)).unit().cross(ev.sub(pt(x, *c2)))
            }
            Curvature { a, b } => (a.eval(x) + b.eval(x)).mulf(scale * scale),
            Ellipse {
                p,
                c,
                a,
                b,
                t,
                axis,
            } => {
                let cv = pt(x, *c);
                let av = pt(x, *a).sub(cv);
                let ratio = x[*b] / av.norm();
                let tv = x[*t];
                let ep = cv
                    .add(av.mul(tv.cos()))
                    .add(av.perp().mul(ratio * tv.sin()));
                pt(x, *p).sub(ep).get(*axis)
            }
            Spline {
                basis,
                first,
                ctrl,
                t,
                what,
            } => {
                let zero = T::cst(0.0);
                let mut cp = vec![V2::new(zero, zero); basis.n_ctrl()];
                for (i, p) in ctrl.iter().enumerate() {
                    cp[first + i] = pt(x, *p);
                }
                let tv = match *t {
                    TParam::Const(v) => T::cst(v),
                    TParam::Var(i) => x[i],
                };
                let ders = match what {
                    SplineEq::Coord { .. }
                    | SplineEq::OnLine { .. }
                    | SplineEq::OnCircle { .. } => 0,
                    SplineEq::TanLine { .. } | SplineEq::TanCircle { .. } => 1,
                    SplineEq::Second { .. } => 2,
                };
                let ev = basis.eval(&cp, tv, ders);
                match what {
                    SplineEq::Coord { p, axis } => pt(x, *p).sub(ev.p).get(*axis),
                    SplineEq::Second { axis } => ev.d2.get(*axis),
                    SplineEq::OnLine { a, b } => sdist(ev.p, pt(x, *a), pt(x, *b)),
                    SplineEq::TanLine { a, b } => {
                        let u = pt(x, *b).sub(pt(x, *a)).unit();
                        u.cross(ev.d1.unit()).mulf(scale)
                    }
                    SplineEq::OnCircle { c, r } => ev.p.sub(pt(x, *c)).norm() - rad(x, r),
                    SplineEq::TanCircle { c } => {
                        ev.p.sub(pt(x, *c)).unit().dot(ev.d1.unit()).mulf(scale)
                    }
                }
            }
            Shift { a, b, by } => x[*b] - x[*a] - T::cst(*by),
            Turned {
                c,
                p,
                q,
                cos,
                sin,
                axis,
            } => {
                let cv = pt(x, *c);
                let r = pt(x, *p).sub(cv);
                let turned = V2::new(
                    r.x.mulf(*cos) - r.y.mulf(*sin),
                    r.x.mulf(*sin) + r.y.mulf(*cos),
                );
                pt(x, *q).sub(cv).sub(turned).get(*axis)
            }
            TurnedDir {
                ca,
                a,
                cb,
                b,
                cos,
                sin,
            } => {
                let r = pt(x, *a).sub(pt(x, *ca));
                let turned = V2::new(
                    r.x.mulf(*cos) - r.y.mulf(*sin),
                    r.x.mulf(*sin) + r.y.mulf(*cos),
                );
                pt(x, *b).sub(pt(x, *cb)).unit().cross(turned)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scalar::{Dual, LANES};
    use crate::spline::clamped_uniform_knots;

    /// Every residual's dual-number derivatives match central differences.
    #[test]
    fn jacobians_match_finite_differences() {
        let basis = Arc::new(SplineBasis {
            degree: 3,
            knots: clamped_uniform_knots(5, 3),
            weights: vec![1.0, 1.2, 0.8, 1.0, 1.1],
        });
        let x0: Vec<f64> = vec![
            0.3, 0.1, 2.0, 0.4, 1.1, 3.0, -0.5, 2.2, 0.9, 1.7, 2.5, -1.0, 0.45, 1.3,
        ];
        let kinds = vec![
            EqKind::Distance {
                p: [0, 1],
                q: [2, 3],
            },
            EqKind::PointLine {
                p: [0, 1],
                a: [2, 3],
                b: [4, 5],
                side: 1.0,
            },
            EqKind::LineLine {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
                side: -1.0,
            },
            EqKind::Parallel {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
            },
            EqKind::Perpendicular {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
            },
            EqKind::DirParallel {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
            },
            EqKind::DirPerp {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
            },
            EqKind::EqualLength {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
            },
            EqKind::RadDiff {
                a: Rad::Param(8),
                b: Rad::Dist([0, 1], [2, 3]),
            },
            EqKind::OnCircle {
                p: [4, 5],
                c: [0, 1],
                r: Rad::Param(9),
            },
            EqKind::Angle {
                a1: [0, 1],
                a2: [2, 3],
                b1: [4, 5],
                b2: [6, 7],
                flip: -1.0,
                sigma: 1.0,
            },
            EqKind::ArcMid {
                m: [6, 7],
                c: [0, 1],
                s: [2, 3],
                e: [4, 5],
                axis: 0,
            },
            EqKind::ArcMid {
                m: [6, 7],
                c: [0, 1],
                s: [2, 3],
                e: [4, 5],
                axis: 1,
            },
            EqKind::ArcLen {
                c: [0, 1],
                s: [2, 3],
                e: [4, 5],
            },
            EqKind::SymMid {
                p: [0, 1],
                q: [2, 3],
                a: [4, 5],
                b: [6, 7],
            },
            EqKind::SymPerp {
                p: [0, 1],
                q: [2, 3],
                a: [4, 5],
                b: [6, 7],
            },
            EqKind::MirrorDir {
                c: [0, 1],
                e: [2, 3],
                rc: [4, 5],
                rp: [6, 7],
                a: [8, 9],
                b: [10, 11],
            },
            EqKind::TanLineCircle {
                a: [0, 1],
                b: [2, 3],
                c: [4, 5],
                r: Rad::Param(8),
                side: -1.0,
            },
            EqKind::TanLineAt {
                a: [0, 1],
                b: [2, 3],
                e: [4, 5],
                c: [6, 7],
            },
            EqKind::TanCircles {
                c1: [0, 1],
                r1: Rad::Param(8),
                c2: [2, 3],
                r2: Rad::Param(9),
                internal: 1.0,
            },
            EqKind::TanCirclesAt {
                e: [0, 1],
                c1: [2, 3],
                c2: [4, 5],
            },
            EqKind::Ellipse {
                p: [0, 1],
                c: [2, 3],
                a: [4, 5],
                b: 8,
                t: 9,
                axis: 1,
            },
            EqKind::Spline {
                basis: basis.clone(),
                first: 0,
                ctrl: vec![[0, 1], [2, 3], [4, 5], [6, 7], [10, 11]],
                t: TParam::Var(12),
                what: SplineEq::TanLine {
                    a: [8, 9],
                    b: [2, 3],
                },
            },
            EqKind::Spline {
                basis: basis.clone(),
                first: 0,
                ctrl: vec![[0, 1], [2, 3], [4, 5], [6, 7], [10, 11]],
                t: TParam::Var(12),
                what: SplineEq::TanCircle { c: [8, 9] },
            },
            EqKind::Spline {
                basis: basis.clone(),
                first: 0,
                ctrl: vec![[0, 1], [2, 3], [4, 5], [6, 7], [10, 11]],
                t: TParam::Const(0.3),
                what: SplineEq::Second { axis: 0 },
            },
            EqKind::Curvature {
                a: CurvTerm::Arc {
                    c: [8, 9],
                    s: [12, 13],
                    sign: -1.0,
                },
                b: CurvTerm::Spline {
                    basis,
                    ctrl: vec![[0, 1], [2, 3], [4, 5], [6, 7], [10, 11]],
                    t: 1.0,
                    sign: -1.0,
                },
            },
            EqKind::Shift {
                a: 2,
                b: 5,
                by: 1.5,
            },
            EqKind::Turned {
                c: [0, 1],
                p: [2, 3],
                q: [4, 5],
                cos: 0.6,
                sin: 0.8,
                axis: 1,
            },
            EqKind::TurnedDir {
                ca: [0, 1],
                a: [2, 3],
                cb: [4, 5],
                b: [6, 7],
                cos: 0.6,
                sin: -0.8,
            },
        ];
        let n = x0.len();
        for kind in &kinds {
            let mut grad = vec![0.0; n];
            for start in (0..n).step_by(LANES) {
                let xd: Vec<Dual> = (0..n)
                    .map(|i| {
                        let lane = (i >= start && i < start + LANES).then(|| i - start);
                        Dual::var(x0[i], lane)
                    })
                    .collect();
                let r = kind.residual(&xd, 0.7, 3.0);
                let end = (start + LANES).min(n);
                grad[start..end].copy_from_slice(&r.d[..end - start]);
            }
            for i in 0..n {
                let h = 1e-6;
                let mut xp = x0.clone();
                let mut xm = x0.clone();
                xp[i] += h;
                xm[i] -= h;
                let fd = (kind.residual(&xp, 0.7, 3.0) - kind.residual(&xm, 0.7, 3.0)) / (2.0 * h);
                assert!(
                    (fd - grad[i]).abs() < 1e-6 * (1.0 + fd.abs()),
                    "{kind:?}: d/dx{i} {} vs {fd}",
                    grad[i]
                );
            }
        }
    }
}
