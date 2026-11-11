// SPDX-License-Identifier: MIT
//! Constant reference curves of the spatial system (edges of earlier
//! bodies, other sketches' curves) and the geometric helpers of its curves:
//! the circle through three points, the nearest parameter of a curve, the
//! length of a curve.

use std::f64::consts::TAU;

use crate::scalar::{Scalar, V3};
use crate::spline::SplineBasis;

/// A curve that does not move while the spatial system solves: an edge of
/// an earlier body, another sketch's curve or an intersection curve, mapped
/// into the sketch's local frame by the caller (`R^T (C(t) - O)`). A point
/// on it slides along its parameter (one unknown, kept in the curve's
/// range), and its midpoint is a constant target;
/// [`super::SpatialSystem::set_reference_curve`] replaces it when its
/// source moves.
#[derive(Clone, Debug, PartialEq)]
pub enum SpatialReferenceCurve {
    /// `start + t (end - start)`, `t` from 0 to 1.
    Line { start: [f64; 3], end: [f64; 3] },
    /// A circle (`major == minor`) or an ellipse: `center + major cos t
    /// x_axis + minor sin t (normal x x_axis)` for `t` from `start` to
    /// `end` (the whole curve, without a range, when `closed`). `normal`
    /// and `x_axis` are unit vectors at right angles.
    Conic {
        center: [f64; 3],
        normal: [f64; 3],
        x_axis: [f64; 3],
        major: f64,
        minor: f64,
        start: f64,
        end: f64,
        closed: bool,
    },
    /// A B-spline with its full knot vector (`poles.len() + degree + 1`
    /// knots); empty `weights` for a non-rational one.
    BSpline {
        degree: usize,
        poles: Vec<[f64; 3]>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    },
    /// A helix: `center + radius (cos t x_axis + sin t (axis x x_axis)) +
    /// pitch t / 2 pi axis` for `t` from `start` to `end` (unit `axis`,
    /// `x_axis` at right angles to it).
    Helix {
        center: [f64; 3],
        axis: [f64; 3],
        x_axis: [f64; 3],
        radius: f64,
        pitch: f64,
        start: f64,
        end: f64,
    },
}

impl SpatialReferenceCurve {
    /// Its start, its end and its middle (halfway along it by length, the
    /// constant target of a Midpoint relation with it): the points a
    /// source curve gives a point attached to it (mitcad#1543). `None` for
    /// a closed conic, which has no ends; an error for invalid geometry.
    pub fn ends_and_middle(&self) -> Result<Option<[[f64; 3]; 3]>, &'static str> {
        let curve = RefCurve::new(self.clone())?;
        let ((lo, hi), bounded) = curve.domain();
        if !bounded {
            return Ok(None);
        }
        let middle = curve.midpoint().ok_or("the curve has no middle")?;
        Ok(Some([curve.at(lo), curve.at(hi), middle]))
    }
}

/// A reference curve ready for evaluation (a spline's basis built once).
#[derive(Clone, Debug)]
pub(crate) struct RefCurve {
    pub curve: SpatialReferenceCurve,
    basis: Option<SplineBasis>,
}

fn finite(v: &[f64]) -> bool {
    v.iter().all(|x| x.is_finite())
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn unit_ok(v: [f64; 3]) -> bool {
    (norm(v) - 1.0).abs() < 1e-9
}

impl RefCurve {
    /// Checks the curve and builds its evaluation.
    pub fn new(curve: SpatialReferenceCurve) -> Result<RefCurve, &'static str> {
        use SpatialReferenceCurve::*;
        let basis = match &curve {
            Line { start, end } => {
                if !finite(start) || !finite(end) {
                    return Err("a reference line needs finite end points");
                }
                None
            }
            Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                ..
            } => {
                let ok = finite(center)
                    && unit_ok(*normal)
                    && unit_ok(*x_axis)
                    && dot(*normal, *x_axis).abs() < 1e-9
                    && major.is_finite()
                    && minor.is_finite()
                    && *major > 0.0
                    && *minor > 0.0
                    && start.is_finite()
                    && end.is_finite()
                    && start < end;
                if !ok {
                    return Err(
                        "a reference conic needs a centre, unit normal and x axis at right \
                         angles, positive radii and a range",
                    );
                }
                None
            }
            BSpline {
                degree,
                poles,
                weights,
                knots,
            } => {
                let n = poles.len();
                let weights = if weights.is_empty() {
                    vec![1.0; n]
                } else {
                    weights.clone()
                };
                let ok = *degree >= 1
                    && n > *degree
                    && poles.iter().all(|p| finite(&p[..]))
                    && weights.len() == n
                    && weights.iter().all(|w| *w > 0.0 && w.is_finite())
                    && knots.len() == n + degree + 1
                    && finite(knots)
                    && knots.windows(2).all(|w| w[0] <= w[1])
                    && knots[*degree] < knots[n];
                if !ok {
                    return Err(
                        "a reference spline needs degree + 1 poles, one positive weight \
                         per pole and poles + degree + 1 non-decreasing knots",
                    );
                }
                Some(SplineBasis {
                    degree: *degree,
                    knots: knots.clone(),
                    weights,
                })
            }
            Helix {
                center,
                axis,
                x_axis,
                radius,
                pitch,
                start,
                end,
            } => {
                let ok = finite(center)
                    && unit_ok(*axis)
                    && unit_ok(*x_axis)
                    && dot(*axis, *x_axis).abs() < 1e-9
                    && radius.is_finite()
                    && *radius > 0.0
                    && pitch.is_finite()
                    && start.is_finite()
                    && end.is_finite()
                    && start < end;
                if !ok {
                    return Err(
                        "a reference helix needs a centre, unit axis and x axis at right \
                         angles, a positive radius and a range",
                    );
                }
                None
            }
        };
        Ok(RefCurve { curve, basis })
    }

    /// The parameter range, and whether a parameter is kept inside it (not
    /// on a closed conic).
    pub fn domain(&self) -> ((f64, f64), bool) {
        use SpatialReferenceCurve::*;
        match &self.curve {
            Line { .. } => ((0.0, 1.0), true),
            Conic {
                start, end, closed, ..
            } => {
                if *closed {
                    ((*start, *start + TAU), false)
                } else {
                    ((*start, *end), true)
                }
            }
            BSpline { .. } => (self.basis.as_ref().expect("spline basis").domain(), true),
            Helix { start, end, .. } => ((*start, *end), true),
        }
    }

    /// The point at `t`.
    pub fn eval<T: Scalar>(&self, t: T) -> V3<T> {
        use SpatialReferenceCurve::*;
        match &self.curve {
            Line { start, end } => V3::cst(*start).add(V3::cst(sub(*end, *start)).mul(t)),
            Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                ..
            } => {
                let y = cross(*normal, *x_axis);
                V3::cst(*center)
                    .add(V3::cst(*x_axis).mul(t.cos().mulf(*major)))
                    .add(V3::cst(y).mul(t.sin().mulf(*minor)))
            }
            BSpline { poles, .. } => {
                let basis = self.basis.as_ref().expect("spline basis");
                let cp: Vec<V3<T>> = poles.iter().map(|p| V3::cst(*p)).collect();
                basis.eval(&cp, t, 0).p
            }
            Helix {
                center,
                axis,
                x_axis,
                radius,
                pitch,
                ..
            } => {
                let y = cross(*axis, *x_axis);
                V3::cst(*center)
                    .add(V3::cst(*x_axis).mul(t.cos().mulf(*radius)))
                    .add(V3::cst(y).mul(t.sin().mulf(*radius)))
                    .add(V3::cst(*axis).mul(t.mulf(pitch / TAU)))
            }
        }
    }

    fn at(&self, t: f64) -> [f64; 3] {
        self.eval(t).val()
    }

    /// The length of the curve (of its range).
    pub fn length(&self) -> f64 {
        let ((lo, hi), _) = self.domain();
        length(|t| self.at(t), lo, hi)
    }

    /// The point halfway along the curve (by length), `None` on a closed
    /// conic (it has no ends).
    pub fn midpoint(&self) -> Option<[f64; 3]> {
        use SpatialReferenceCurve::*;
        let ((lo, hi), bounded) = self.domain();
        if !bounded {
            return None;
        }
        Some(match &self.curve {
            Line { start, end } => [0, 1, 2].map(|k| 0.5 * (start[k] + end[k])),
            // A circle's and a helix's length grows evenly with t.
            Conic { major, minor, .. } if major == minor => self.at(0.5 * (lo + hi)),
            Helix { .. } => self.at(0.5 * (lo + hi)),
            _ => self.at(at_length(|t| self.at(t), lo, hi, 0.5)),
        })
    }

    /// The parameter of the point nearest to `p`.
    pub fn closest(&self, p: [f64; 3]) -> f64 {
        let ((lo, hi), _) = self.domain();
        closest(|t| self.at(t), lo, hi, p)
    }
}

/// Samples of a curve for lengths and nearest points.
const SAMPLES: usize = 256;

/// The length of `f` over `[lo, hi]` (a fine polyline).
pub(super) fn length(f: impl Fn(f64) -> [f64; 3], lo: f64, hi: f64) -> f64 {
    let n = 4 * SAMPLES;
    let mut prev = f(lo);
    let mut len = 0.0;
    for i in 1..=n {
        let q = f(lo + (hi - lo) * i as f64 / n as f64);
        len += norm(sub(q, prev));
        prev = q;
    }
    len
}

/// The parameter at which `fraction` of `f`'s length over `[lo, hi]` is
/// behind.
fn at_length(f: impl Fn(f64) -> [f64; 3], lo: f64, hi: f64, fraction: f64) -> f64 {
    let n = 4 * SAMPLES;
    let ts: Vec<f64> = (0..=n)
        .map(|i| lo + (hi - lo) * i as f64 / n as f64)
        .collect();
    let pts: Vec<[f64; 3]> = ts.iter().map(|&t| f(t)).collect();
    let segs: Vec<f64> = pts.windows(2).map(|w| norm(sub(w[1], w[0]))).collect();
    let goal = fraction * segs.iter().sum::<f64>();
    let mut acc = 0.0;
    for (i, &s) in segs.iter().enumerate() {
        if acc + s >= goal && s > 0.0 {
            return ts[i] + (ts[i + 1] - ts[i]) * (goal - acc) / s;
        }
        acc += s;
    }
    hi
}

/// The parameter in `[lo, hi]` of the point of `f` nearest to `p`: the
/// best of a sampling, refined by golden sections between its neighbours.
pub(super) fn closest(f: impl Fn(f64) -> [f64; 3], lo: f64, hi: f64, p: [f64; 3]) -> f64 {
    let d = |t: f64| {
        let q = f(t);
        dot(sub(q, p), sub(q, p))
    };
    let step = (hi - lo) / SAMPLES as f64;
    let best = (0..=SAMPLES)
        .map(|i| lo + step * i as f64)
        .min_by(|a, b| d(*a).total_cmp(&d(*b)))
        .unwrap_or(lo);
    let (mut a, mut b) = ((best - step).max(lo), (best + step).min(hi));
    let g = 0.5 * (5f64.sqrt() - 1.0);
    for _ in 0..60 {
        let (c, e) = (b - g * (b - a), a + g * (b - a));
        if d(c) < d(e) {
            b = e;
        } else {
            a = c;
        }
    }
    let t = 0.5 * (a + b);
    if d(t) <= d(best) { t } else { best }
}

/// A circle through three points ([`circle_through`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpatialCircle {
    pub center: [f64; 3],
    /// Unit normal: the points run counter-clockwise about it in their
    /// order.
    pub normal: [f64; 3],
    pub radius: f64,
}

/// The circle through three points, for the Three-Point Arc of 3D sketches
/// (from `a` through `b` to `c`, counter-clockwise about the normal).
/// `None` for collinear input: two points at most
/// [`super::SPATIAL_MIN_SEGMENT`] apart, or the middle point at most that
/// far from the line through the other two (or not finite).
pub fn circle_through(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<SpatialCircle> {
    if !finite(&a) || !finite(&b) || !finite(&c) {
        return None;
    }
    let (u, v) = (sub(b, a), sub(c, a));
    let w = cross(u, v);
    let (lu, lv, lw) = (norm(u), norm(v), norm(w));
    let min = super::SPATIAL_MIN_SEGMENT;
    if lu <= min || lv <= min || norm(sub(c, b)) <= min || lw / lv <= min {
        return None;
    }
    // centre = a + (|u|^2 (v x w) + |v|^2 (w x u)) / (2 |w|^2)
    let (vw, wu) = (cross(v, w), cross(w, u));
    let k = 1.0 / (2.0 * lw * lw);
    let center = [0, 1, 2].map(|i| a[i] + k * (lu * lu * vw[i] + lv * lv * wu[i]));
    Some(SpatialCircle {
        center,
        normal: w.map(|x| x / lw),
        radius: norm(sub(a, center)),
    })
}
