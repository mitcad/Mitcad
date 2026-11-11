// SPDX-License-Identifier: MIT
//! The rows of the spatial system: residuals over a few local unknowns,
//! written once over [`Scalar`] (values with `f64`, the Jacobian with
//! [`Dual`] in passes of [`LANES`] unknowns), and their evaluation as
//! [`Equations`] for the shared iterations and rank analysis.
//!
//! Every residual is a length in millimetres. Residuals of unit vectors
//! (a normal's unit row, normal alignment, tangency) are multiplied by the
//! row's characteristic length `l` (its entity's radius or length, at least
//! 1 mm, constant during one solve), so that the absolute acceptance
//! tolerance means the same for every row.

use std::sync::Arc;

use super::curve::RefCurve;
use super::surface::SpatialSurface;
use crate::equations::EqKind;
use crate::prepare::Component;
use crate::scalar::{Dual, LANES, Scalar, V3};
use crate::solve::Equations;
use crate::sparse::Rows;
use crate::spline::{CurveEval, SplineBasis};

/// Local indices of a point's (or a normal's) x, y and z.
pub(super) type P3 = [usize; 3];

/// Local numbering of one row's unknowns.
#[derive(Default)]
pub(super) struct Vars {
    pub list: Vec<usize>,
}

impl Vars {
    /// The local index of global column `g`.
    pub fn v(&mut self, g: usize) -> usize {
        match self.list.iter().position(|&x| x == g) {
            Some(i) => i,
            None => {
                self.list.push(g);
                self.list.len() - 1
            }
        }
    }

    /// The local indices of the three columns starting at `col`.
    pub fn p(&mut self, col: usize) -> P3 {
        [self.v(col), self.v(col + 1), self.v(col + 2)]
    }
}

fn pt<T: Scalar>(x: &[T], p: P3) -> V3<T> {
    V3::new(x[p[0]], x[p[1]], x[p[2]])
}

/// The normal of an arc or circle: its unknown unit vector, or a constant
/// axis of the frame (plane-bound).
#[derive(Clone, Debug)]
pub(super) enum Nrm {
    Var(P3),
    Const([f64; 3]),
}

impl Nrm {
    fn eval<T: Scalar>(&self, x: &[T]) -> V3<T> {
        match self {
            Nrm::Var(n) => pt(x, *n),
            Nrm::Const(v) => V3::cst(*v),
        }
    }
}

/// A length: the distance between two points or a scalar unknown (a
/// circle's radius).
#[derive(Clone, Debug)]
pub(super) enum Len {
    Dist(P3, P3),
    Param(usize),
}

impl Len {
    fn eval<T: Scalar>(&self, x: &[T]) -> T {
        match self {
            Len::Dist(a, b) => pt(x, *b).sub(pt(x, *a)).norm(),
            Len::Param(r) => x[*r],
        }
    }
}

/// A spline's basis and the control points a row uses: `ctrl[i]` is
/// control point `first + i` (the others do not influence the row).
#[derive(Clone, Debug)]
pub(super) struct SplineRef {
    pub basis: Arc<SplineBasis>,
    pub first: usize,
    pub ctrl: Vec<P3>,
}

impl SplineRef {
    fn eval<T: Scalar>(&self, x: &[T], t: T, ders: usize) -> CurveEval<V3<T>> {
        let mut cp = vec![V3::cst([0.0; 3]); self.basis.n_ctrl()];
        for (i, p) in self.ctrl.iter().enumerate() {
            cp[self.first + i] = pt(x, *p);
        }
        self.basis.eval(&cp, t, ders)
    }
}

/// A curve parameter: fixed (a fit point's) or a local unknown.
#[derive(Clone, Copy, Debug)]
pub(super) enum Tp {
    Const(f64),
    Var(usize),
}

/// The unit tangent of a curve at a point, as a function of the unknowns.
#[derive(Clone, Debug)]
pub(super) enum Tan {
    /// `unit(b - a)`.
    Line(P3, P3),
    /// `unit(n x (p - c))`: an arc or circle (counter-clockwise about `n`)
    /// at its point `p`.
    Round { n: Nrm, c: P3, p: P3 },
    /// `unit(C'(t))` at an end of a spline.
    Spline { s: SplineRef, t: f64 },
    /// `unit(n)`: an arc's or circle's normal as a direction (mitcad#1661:
    /// parallel, perpendicular and angle relations take it).
    Normal(Nrm),
}

impl Tan {
    pub fn eval<T: Scalar>(&self, x: &[T]) -> V3<T> {
        match self {
            Tan::Line(a, b) => pt(x, *b).sub(pt(x, *a)).unit(),
            Tan::Round { n, c, p } => n.eval(x).cross(pt(x, *p).sub(pt(x, *c))).unit(),
            Tan::Spline { s, t } => s.eval(x, T::cst(*t), 1).d1.unit(),
            Tan::Normal(n) => n.eval(x).unit(),
        }
    }
}

/// The curvature vector of a curve at a point (mitcad#1661: G2 continuity),
/// pointing toward the centre of curvature with the magnitude `1 / radius`.
#[derive(Clone, Debug)]
pub(super) enum Crv {
    /// A line: none.
    Line,
    /// An arc or circle at its point `p`: `(c - p) / abs(c - p)^2`.
    Round { c: P3, p: P3 },
    /// A spline at an end: `(C' x C'') x C' / abs(C')^4`.
    Spline { s: SplineRef, t: f64 },
}

impl Crv {
    pub fn eval<T: Scalar>(&self, x: &[T]) -> V3<T> {
        match self {
            Crv::Line => V3::cst([0.0; 3]),
            Crv::Round { c, p } => {
                let d = pt(x, *c).sub(pt(x, *p));
                let r2 = d.dot(d);
                d.mul(T::cst(1.0) / r2)
            }
            Crv::Spline { s, t } => {
                let e = s.eval(x, T::cst(*t), 2);
                let s2 = e.d1.dot(e.d1);
                e.d1.cross(e.d2).cross(e.d1).mul(T::cst(1.0) / (s2 * s2))
            }
        }
    }
}

/// The nodes and weights of the five-point Gauss-Legendre rule on
/// `[-1, 1]`.
const GAUSS5: [(f64, f64); 5] = [
    (0.0, 0.568_888_888_888_888_9),
    (-0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (0.538_469_310_105_683_1, 0.478_628_670_499_366_5),
    (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
    (0.906_179_845_938_664, 0.236_926_885_056_189_1),
];

/// Pieces of each knot span in [`spline_length`].
const LENGTH_PIECES: usize = 8;

/// The length of a spline over its domain (mitcad#1661): the five-point
/// Gauss-Legendre rule on eight pieces of every knot span, `d1` the
/// derivative `C'(t)`. One formula for a driving length's row and for
/// measuring, so a driving length measures what it drives.
pub(super) fn spline_length<T: Scalar>(basis: &SplineBasis, d1: impl Fn(f64) -> V3<T>) -> T {
    let (lo, hi) = basis.domain();
    let mut total = T::cst(0.0);
    for w in basis.knots.windows(2) {
        let (a, b) = (w[0].max(lo), w[1].min(hi));
        if b <= a {
            continue;
        }
        let h = (b - a) / LENGTH_PIECES as f64;
        for piece in 0..LENGTH_PIECES {
            let mid = a + h * (piece as f64 + 0.5);
            for (node, weight) in GAUSS5 {
                let speed = d1(mid + 0.5 * h * node).norm();
                total = total + speed.mulf(0.5 * h * weight);
            }
        }
    }
    total
}

#[derive(Clone, Debug)]
pub(super) enum RowKind {
    /// `x[a] - value`
    Target { a: usize },
    /// `x[b] - x[a] - value`
    Diff { a: usize, b: usize },
    /// `2 x[m] - x[a] - x[b]`
    Mid { m: usize, a: usize, b: usize },
    /// `k a - value` (a length, radius or diameter)
    Measure { a: Len, k: f64 },
    /// `a - b`
    LenDiff { a: Len, b: Len },
    /// `l (n.n - 1) / 2`
    Unit { n: P3 },
    /// `n.(b - a)`
    PlaneDot { n: Nrm, a: P3, b: P3 },
    /// `l n_k`
    NormalComp { n: P3, k: usize },
    /// `u.(p - c)`
    TanPoint { u: Tan, p: P3, c: P3 },
    /// `l u.n`
    TanNormal { u: Tan, n: Nrm },
    /// `l u.w`, `w` the first (`w1 = unit(v x e_axis)`) or second
    /// (`w2 = v x w1`) direction across `v`.
    TanAxis {
        u: Tan,
        v: Tan,
        axis: usize,
        second: bool,
    },
    /// `p_k - a_k - x[t] (b_k - a_k)`
    OnLine {
        p: P3,
        a: P3,
        b: P3,
        t: usize,
        k: usize,
    },
    /// `p_k - C_k(t)`
    OnSpline {
        p: P3,
        s: SplineRef,
        t: Tp,
        k: usize,
    },
    /// `C^(order)_k(t)` (a natural end: `order` 2).
    Deriv {
        s: SplineRef,
        t: f64,
        order: usize,
        k: usize,
    },
    /// `C^(order)_k(lo) - C^(order)_k(hi)`: a closed spline's seam.
    Periodic {
        s: SplineRef,
        order: usize,
        k: usize,
    },
    /// `p_k - C_k(x[t])` on a constant reference curve.
    OnReference {
        p: P3,
        curve: Arc<RefCurve>,
        t: usize,
        k: usize,
    },
    /// The signed distance from `p` to a constant surface.
    OnSurface { p: P3, surface: SpatialSurface },
    /// `|s - c| theta - value`, `theta` the sweep from `s` to `e`
    /// counter-clockwise about `n`, in `(0, 2 pi)`.
    ArcLength { c: P3, s: P3, e: P3, n: Nrm },
    /// A planar row of an in-plane relation, on the x and y columns of
    /// spatial points.
    Planar(EqKind),
    /// A point on a plane-bound ellipse (mitcad#1550), in its plane:
    /// `(sqrt((X b)^2 + (Y A)^2) - A b) / l` with `A = abs(a - c)`, `b` the
    /// minor radius, `X`, `Y` the point's coordinates along the major axis
    /// `u = unit(a - c)` and along `n x u` (`n` the plane's unit normal: a
    /// frame axis, or a rotated plane's, mitcad#1661).
    OnEllipse {
        p: P3,
        c: P3,
        a: P3,
        b: usize,
        n: [f64; 3],
    },
    /// `l u.v`: two directions at right angles (mitcad#1661).
    DirDot { u: Tan, v: Tan },
    /// `l (atan2(abs(u x v), u.v) - value)`: the angle between two
    /// directions, `value` in radians (mitcad#1661).
    DirAngle { u: Tan, v: Tan },
    /// `n.((a + b) / 2) - value`: the middle of two points on a constant
    /// plane (`value` its `n.origin`, mitcad#1661).
    MidPlane { a: P3, b: P3, n: [f64; 3] },
    /// `w.(b - a)`: two points level along a constant direction.
    Across { a: P3, b: P3, w: [f64; 3] },
    /// `l^2 (ka - kb).w`, `w` the first or second direction across `v`
    /// as in [`RowKind::TanAxis`]: equal curvature vectors across the
    /// tangent (G2, mitcad#1661).
    CurvAxis {
        ka: Crv,
        kb: Crv,
        v: Tan,
        axis: usize,
        second: bool,
    },
    /// `length(C) - value` of a whole spline (mitcad#1661).
    SplineLength { s: SplineRef },
    /// `abs(C'(t)) - value` at an end of a spline (mitcad#1661).
    TangentLength { s: SplineRef, t: f64 },
    /// `n.(b - a) - value` along a constant unit direction (mitcad#1661:
    /// the axes and normal of a rotated placement plane).
    DirDiff { n: [f64; 3], a: P3, b: P3 },
    /// `l n.w`: a free normal across the constant direction `w`
    /// (mitcad#1661: a circle held square to a rotated plane).
    NormalDot { n: P3, w: [f64; 3] },
    /// A planar row of an in-plane relation of a rotated plane
    /// (mitcad#1661): the planar solver's row over the planar unknowns
    /// `map` gives, each a spatial column or a point's coordinate along an
    /// in-plane axis of the plane's frame.
    FramePlanar { kind: EqKind, map: Vec<FrameVar> },
}

/// A planar unknown of an in-plane relation of a rotated plane
/// (mitcad#1661): a spatial column itself (a radius, a curve parameter),
/// or `e.p`, the coordinate of point `p` along the unit in-plane axis `e`.
#[derive(Clone, Debug)]
pub(super) enum FrameVar {
    Col(usize),
    Dot { p: P3, e: [f64; 3] },
}

/// The first (`w1 = unit(v x e_axis)`) or second (`w2 = v x w1`) direction
/// across `v`.
fn across<T: Scalar>(v: V3<T>, axis: usize, second: bool) -> V3<T> {
    let mut e = [0.0; 3];
    e[axis] = 1.0;
    let w1 = v.cross(V3::cst(e)).unit();
    if second { v.cross(w1) } else { w1 }
}

/// The sweep of an arc from `rs` to `re` (from its centre) counter-clockwise
/// about the unit normal `n`, in `(0, 2 pi]`.
pub(super) fn sweep<T: Scalar>(n: V3<T>, rs: V3<T>, re: V3<T>) -> T {
    let theta = n.dot(rs.cross(re)).atan2(rs.dot(re));
    if theta.val() <= 0.0 {
        theta + T::cst(std::f64::consts::TAU)
    } else {
        theta
    }
}

impl RowKind {
    /// Residual for local unknowns `x`, target `value`, characteristic
    /// length `l` and the sketch size `scale` (planar rows).
    pub fn residual<T: Scalar>(&self, x: &[T], value: f64, l: f64, scale: f64) -> T {
        use RowKind::*;
        match self {
            Target { a } => x[*a] - T::cst(value),
            Diff { a, b } => x[*b] - x[*a] - T::cst(value),
            Mid { m, a, b } => x[*m].mulf(2.0) - x[*a] - x[*b],
            Measure { a, k } => a.eval(x).mulf(*k) - T::cst(value),
            LenDiff { a, b } => a.eval(x) - b.eval(x),
            Unit { n } => {
                let n = pt(x, *n);
                (n.dot(n) - T::cst(1.0)).mulf(0.5 * l)
            }
            PlaneDot { n, a, b } => n.eval(x).dot(pt(x, *b).sub(pt(x, *a))),
            NormalComp { n, k } => x[n[*k]].mulf(l),
            TanPoint { u, p, c } => u.eval(x).dot(pt(x, *p).sub(pt(x, *c))),
            TanNormal { u, n } => u.eval(x).dot(n.eval(x)).mulf(l),
            TanAxis { u, v, axis, second } => {
                let w = across(v.eval(x), *axis, *second);
                u.eval(x).dot(w).mulf(l)
            }
            OnLine { p, a, b, t, k } => {
                let (pk, ak, bk) = (x[p[*k]], x[a[*k]], x[b[*k]]);
                pk - ak - x[*t] * (bk - ak)
            }
            OnSpline { p, s, t, k } => {
                let tv = match *t {
                    Tp::Const(v) => T::cst(v),
                    Tp::Var(i) => x[i],
                };
                pt(x, *p).sub(s.eval(x, tv, 0).p).get(*k)
            }
            Deriv { s, t, order, k } => {
                let e = s.eval(x, T::cst(*t), *order);
                if *order == 1 {
                    e.d1.get(*k)
                } else {
                    e.d2.get(*k)
                }
            }
            Periodic { s, order, k } => {
                let (lo, hi) = s.basis.domain();
                let (a, b) = (s.eval(x, T::cst(lo), *order), s.eval(x, T::cst(hi), *order));
                if *order == 1 {
                    a.d1.sub(b.d1).get(*k)
                } else {
                    a.d2.sub(b.d2).get(*k)
                }
            }
            OnReference { p, curve, t, k } => pt(x, *p).sub(curve.eval(x[*t])).get(*k),
            OnSurface { p, surface } => surface.distance(pt(x, *p)),
            ArcLength { c, s, e, n } => {
                let cv = pt(x, *c);
                let (rs, re) = (pt(x, *s).sub(cv), pt(x, *e).sub(cv));
                rs.norm() * sweep(n.eval(x), rs, re) - T::cst(value)
            }
            Planar(kind) => kind.residual(x, value, scale),
            OnEllipse { p, c, a, b, n } => {
                let cv = pt(x, *c);
                let u = pt(x, *a).sub(cv);
                let major = u.norm();
                let u = u.unit();
                let w = V3::cst(*n).cross(u);
                let d = pt(x, *p).sub(cv);
                let (px, py) = (d.dot(u), d.dot(w));
                let minor = x[*b];
                let (sx, sy) = (px * minor, py * major);
                ((sx * sx + sy * sy).sqrt() - major * minor).mulf(1.0 / l)
            }
            DirDot { u, v } => u.eval(x).dot(v.eval(x)).mulf(l),
            DirAngle { u, v } => {
                let (u, v) = (u.eval(x), v.eval(x));
                (u.cross(v).norm().atan2(u.dot(v)) - T::cst(value)).mulf(l)
            }
            MidPlane { a, b, n } => {
                let m = pt(x, *a).add(pt(x, *b)).mulf(0.5);
                V3::cst(*n).dot(m) - T::cst(value)
            }
            Across { a, b, w } => V3::cst(*w).dot(pt(x, *b).sub(pt(x, *a))),
            CurvAxis {
                ka,
                kb,
                v,
                axis,
                second,
            } => {
                let w = across(v.eval(x), *axis, *second);
                ka.eval(x).sub(kb.eval(x)).dot(w).mulf(l * l)
            }
            SplineLength { s } => {
                spline_length(&s.basis, |t| s.eval(x, T::cst(t), 1).d1) - T::cst(value)
            }
            TangentLength { s, t } => s.eval(x, T::cst(*t), 1).d1.norm() - T::cst(value),
            DirDiff { n, a, b } => V3::cst(*n).dot(pt(x, *b).sub(pt(x, *a))) - T::cst(value),
            NormalDot { n, w } => pt(x, *n).dot(V3::cst(*w)).mulf(l),
            FramePlanar { kind, map } => {
                let y: Vec<T> = map
                    .iter()
                    .map(|m| match m {
                        FrameVar::Col(i) => x[*i],
                        FrameVar::Dot { p, e } => V3::cst(*e).dot(pt(x, *p)),
                    })
                    .collect();
                kind.residual(&y, value, scale)
            }
        }
    }
}

/// Who a row belongs to: a curve (its internal rows) or a constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Owner {
    Curve(usize),
    Constraint(usize),
}

/// Where a row's value comes from: `factor` times value `k` of a
/// constraint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ValueRef {
    pub constraint: usize,
    pub k: usize,
    pub factor: f64,
}

/// An entity whose size scales a row ([`RowKind::residual`]'s `l`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Size {
    Line(usize),
    Curve(usize),
}

#[derive(Clone, Debug)]
pub(super) struct Row {
    pub owner: Owner,
    /// Global columns, in the kind's local order.
    pub vars: Vec<usize>,
    pub kind: RowKind,
    pub value: f64,
    pub src: Option<ValueRef>,
    /// A planar Fix: its value is the coordinate where a solve starts.
    pub fixed: bool,
    /// Characteristic length (refreshed when a solve starts from `size`).
    pub l: f64,
    pub size: Vec<Size>,
}

impl Row {
    pub fn new(owner: Owner, vars: Vars, kind: RowKind) -> Row {
        Row {
            owner,
            vars: vars.list,
            kind,
            value: 0.0,
            src: None,
            fixed: false,
            l: 1.0,
            size: Vec::new(),
        }
    }

    pub fn residual<T: Scalar>(&self, x: &[T], scale: f64) -> T {
        self.kind.residual(x, self.value, self.l, scale)
    }
}

pub(super) struct SpatialEquations<'a> {
    pub rows: &'a [Row],
    pub bounds: &'a [Option<(f64, f64)>],
}

impl Equations for SpatialEquations<'_> {
    fn evaluate(
        &self,
        comp: &Component,
        params: &[f64],
        scale: f64,
        f: &mut [f64],
        jac: Option<&mut Rows>,
    ) {
        match jac {
            None => {
                let mut x: Vec<f64> = Vec::with_capacity(9);
                for (r, &ri) in comp.eqs.iter().enumerate() {
                    let row = &self.rows[ri];
                    x.clear();
                    x.extend(row.vars.iter().map(|&g| params[g]));
                    f[r] = row.residual(&x, scale);
                }
            }
            Some(jac) => {
                // One pass of the dual numbers per LANES unknowns.
                let mut x: Vec<Dual> = Vec::with_capacity(9);
                for (r, &ri) in comp.eqs.iter().enumerate() {
                    let row = &self.rows[ri];
                    let n = row.vars.len();
                    let base = jac.ptr[r];
                    let mut start = 0;
                    loop {
                        x.clear();
                        x.extend(row.vars.iter().enumerate().map(|(i, &g)| {
                            let lane = (i >= start && i < start + LANES).then(|| i - start);
                            Dual::var(params[g], lane)
                        }));
                        let v = row.residual(&x, scale);
                        f[r] = v.v;
                        for i in start..(start + LANES).min(n) {
                            jac.val[base + i] = v.d[i - start];
                        }
                        start += LANES;
                        if start >= n {
                            break;
                        }
                    }
                }
            }
        }
    }

    fn bounds(&self, var: usize) -> Option<(f64, f64)> {
        self.bounds.get(var).copied().flatten()
    }
}
