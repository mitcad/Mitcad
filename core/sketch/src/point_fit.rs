// SPDX-License-Identifier: MIT
//! A cubic B-spline fitted to a list of points by least squares
//! (mitcad#2232, Insert Points): chord-length parameters, the curve's ends
//! on the first and last point, and knots added one at a time where the
//! curve is furthest from the points until every point is within the
//! tolerance or the curve has its most control points.
//!
//! Why not a spline through every point: coordinates rounded to four to
//! six digits put small wiggles into the curvature of a cubic forced
//! through all of them, which show on a surface made from it, and leave
//! hundreds of points to drag. A least-squares fit with few knots, most of
//! them where the curve turns fastest (an airfoil's leading edge), is
//! smooth and keeps a few dozen control points to edit.
//!
//! Each fit's parameters are corrected afterwards (each point's parameter
//! moved to its nearest point on the curve, a few Newton steps), so the
//! deviation measured is the distance of a point from the curve, not from
//! the curve's point at a guessed parameter.
//!
//! Our own small routine: sketch curves are solved in Rust without the
//! kernel, and the `Kernel` trait has no point approximation.

use crate::bspline::{DEGREE, basis, knot_vector};
use crate::geometry::{Nurbs, P2};

/// The most control points a fitted spline gets.
pub const MAX_CONTROL: usize = 60;

/// How many times each fit's parameters are corrected and the curve
/// fitted again.
const PARAMETER_ROUNDS: usize = 3;

/// Newton steps of one parameter correction.
const NEWTON_STEPS: usize = 4;

/// How the fitted curve ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ends {
    /// On the first and the last point, each end on its own.
    Open,
    /// The first and the last point are the same (the caller repeats the
    /// first point at the end): the curve starts and ends there with the
    /// same tangent, a closed curve without a corner.
    Closed,
}

/// A fitted cubic spline.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveFit {
    /// Its control points: the first is the first point given, the last
    /// the last point given.
    pub control: Vec<P2>,
    /// Its full clamped knot vector on [0, 1] (`control.len() + 4`
    /// values).
    pub knots: Vec<f64>,
    /// The largest distance of a given point from the curve.
    pub deviation: f64,
    /// Every point is within the tolerance.
    pub within: bool,
}

impl CurveFit {
    /// The fit as a curve to evaluate.
    pub fn nurbs(&self) -> Nurbs {
        Nurbs {
            degree: DEGREE,
            control: self.control.clone(),
            weights: Vec::new(),
            knots: self.knots.clone(),
        }
    }
}

/// Fits a cubic spline to `points` (at least four, the curve's ends on the
/// first and the last) within `tolerance` (mm) with at most
/// `max_control` control points. When the cap (or a curve that cannot be
/// split further) stops it first, the fit it reached comes back with
/// `within` false.
pub fn fit(
    points: &[P2],
    tolerance: f64,
    max_control: usize,
    ends: Ends,
) -> Result<CurveFit, String> {
    if points.len() < 4 {
        return Err("a fitted spline needs at least four points".to_owned());
    }
    if !(tolerance.is_finite() && tolerance > 0.0) {
        return Err(format!(
            "the tolerance must be greater than zero, got {tolerance}"
        ));
    }
    let mut t = chord_parameters(points)
        .ok_or_else(|| "the points all lie at one place: no curve runs through them".to_owned())?;
    // A closed curve starts with a few spans, so that its tangent at the
    // join has room to turn.
    let mut interior: Vec<f64> = match ends {
        Ends::Open => Vec::new(),
        Ends::Closed => vec![0.25, 0.5, 0.75],
    };
    let max_control = max_control.max(interior.len() + DEGREE + 1);
    loop {
        let knots = full_knots(&interior);
        let poles = knots.len() - DEGREE - 1;
        let mut control = least_squares(points, &t, &knots, poles, ends)?;
        for _ in 0..PARAMETER_ROUNDS {
            correct_parameters(points, &mut t, &curve(&control, &knots));
            control = least_squares(points, &t, &knots, poles, ends)?;
        }
        let nurbs = curve(&control, &knots);
        correct_parameters(points, &mut t, &nurbs);
        let errors: Vec<f64> = points
            .iter()
            .zip(&t)
            .map(|(p, &ti)| distance(nurbs.point(ti), *p))
            .collect();
        let deviation = errors.iter().copied().fold(0.0, f64::max);
        let within = deviation <= tolerance;
        let fitted = CurveFit {
            control,
            knots,
            deviation,
            within,
        };
        if within || poles >= max_control {
            return Ok(fitted);
        }
        match split_knot(&interior, &t, &errors, tolerance) {
            Some(knot) => {
                let at = interior.partition_point(|k| *k < knot);
                interior.insert(at, knot);
            }
            None => return Ok(fitted),
        }
    }
}

/// Chord-length parameters on [0, 1]; None when the points have no length.
pub fn chord_parameters(points: &[P2]) -> Option<Vec<f64>> {
    let mut s = Vec::with_capacity(points.len());
    let mut total = 0.0;
    s.push(0.0);
    for w in points.windows(2) {
        total += distance(w[0], w[1]);
        s.push(total);
    }
    if !(total.is_finite() && total > 0.0) {
        return None;
    }
    let last = s.len() - 1;
    let mut t: Vec<f64> = s.into_iter().map(|v| v / total).collect();
    t[last] = 1.0;
    Some(t)
}

fn full_knots(interior: &[f64]) -> Vec<f64> {
    let mut knots = knot_vector(DEGREE + 1);
    knots.splice(DEGREE + 1..DEGREE + 1, interior.iter().copied());
    knots
}

fn curve(control: &[P2], knots: &[f64]) -> Nurbs {
    Nurbs {
        degree: DEGREE,
        control: control.to_vec(),
        weights: Vec::new(),
        knots: knots.to_vec(),
    }
}

fn distance(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The control points that fit the points at parameters `t` best, by
/// least squares, the first and last on the first and last point; with
/// `Ends::Closed` also the same tangent at both ends.
fn least_squares(
    points: &[P2],
    t: &[f64],
    knots: &[f64],
    poles: usize,
    ends: Ends,
) -> Result<Vec<P2>, String> {
    let first = points[0];
    let last = points[points.len() - 1];
    // The unknown poles; with a closed curve the second pole follows the
    // last but one: P1 = (1 + r) P0 - r P[n-2], the end tangents equal.
    let (free_from, ratio) = match ends {
        Ends::Open => (1, 0.0),
        Ends::Closed => {
            let a = knots[DEGREE + 1];
            let b = 1.0 - knots[poles - 1];
            (2, if b > 0.0 { a / b } else { 1.0 })
        }
    };
    let free_to = poles - 2; // inclusive
    if free_to < free_from {
        return Err("too few control points to fit".to_owned());
    }
    let unknowns = free_to - free_from + 1;
    let mut a = vec![0.0; unknowns * unknowns];
    let mut b = [vec![0.0; unknowns], vec![0.0; unknowns]];
    let mut row = vec![0.0; unknowns];
    for (p, &ti) in points.iter().zip(t) {
        let n = basis(knots, poles, ti);
        let mut rest = *p;
        // The fixed poles' part moves to the right-hand side.
        let fixed = |rest: &mut P2, weight: f64, at: P2| {
            rest[0] -= weight * at[0];
            rest[1] -= weight * at[1];
        };
        fixed(&mut rest, n[0], first);
        fixed(&mut rest, n[poles - 1], last);
        for (k, value) in row.iter_mut().enumerate() {
            *value = n[free_from + k];
        }
        if ends == Ends::Closed {
            fixed(&mut rest, n[1] * (1.0 + ratio), first);
            row[unknowns - 1] -= ratio * n[1];
        }
        for i in 0..unknowns {
            if row[i] == 0.0 {
                continue;
            }
            b[0][i] += row[i] * rest[0];
            b[1][i] += row[i] * rest[1];
            for j in 0..unknowns {
                a[i * unknowns + j] += row[i] * row[j];
            }
        }
    }
    // A trace's worth of nothing: keeps a span the points barely reach
    // from making the matrix singular, far below any length that shows.
    let trace: f64 = (0..unknowns).map(|i| a[i * unknowns + i]).sum();
    let ridge = 1e-12 * trace.max(1.0) / unknowns as f64;
    for i in 0..unknowns {
        a[i * unknowns + i] += ridge;
    }
    let solved = cholesky_solve(&mut a, unknowns, &mut b)
        .ok_or_else(|| "no spline fits the points: the fit's equations are singular".to_owned())?;
    let mut control = vec![first; poles];
    for k in 0..unknowns {
        control[free_from + k] = [solved[0][k], solved[1][k]];
    }
    control[poles - 1] = last;
    if ends == Ends::Closed {
        let before_last = control[poles - 2];
        control[1] = [
            (1.0 + ratio) * first[0] - ratio * before_last[0],
            (1.0 + ratio) * first[1] - ratio * before_last[1],
        ];
    }
    Ok(control)
}

/// Solves `a x = b` for both right-hand sides by Cholesky; `a` (n × n,
/// symmetric positive definite) is overwritten. None when it is not
/// positive definite.
fn cholesky_solve(a: &mut [f64], n: usize, b: &mut [Vec<f64>; 2]) -> Option<[Vec<f64>; 2]> {
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= a[j * n + k] * a[j * n + k];
        }
        if !(d.is_finite() && d > 0.0) {
            return None;
        }
        let d = d.sqrt();
        a[j * n + j] = d;
        for i in j + 1..n {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = s / d;
        }
    }
    let mut out = [vec![0.0; n], vec![0.0; n]];
    for (rhs, x) in b.iter().zip(out.iter_mut()) {
        // L y = b, then Lᵀ x = y.
        let mut y = rhs.clone();
        for i in 0..n {
            let mut s = y[i];
            for k in 0..i {
                s -= a[i * n + k] * y[k];
            }
            y[i] = s / a[i * n + i];
        }
        for i in (0..n).rev() {
            let mut s = y[i];
            for k in i + 1..n {
                s -= a[k * n + i] * x[k];
            }
            x[i] = s / a[i * n + i];
        }
    }
    Some(out)
}

/// Moves each point's parameter (not the ends') to where the curve comes
/// nearest it, by a few Newton steps from where it was.
fn correct_parameters(points: &[P2], t: &mut [f64], nurbs: &Nurbs) {
    let last = points.len() - 1;
    for i in 1..last {
        let p = points[i];
        let mut ti = t[i];
        for _ in 0..NEWTON_STEPS {
            let [c, d1, d2] = nurbs.eval(ti);
            let diff = [c[0] - p[0], c[1] - p[1]];
            let f = diff[0] * d1[0] + diff[1] * d1[1];
            let speed = d1[0] * d1[0] + d1[1] * d1[1];
            let mut slope = speed + diff[0] * d2[0] + diff[1] * d2[1];
            if !(slope.is_finite() && slope > 0.0) {
                slope = speed;
            }
            if !(slope.is_finite() && slope > 0.0) {
                break;
            }
            let next = (ti - f / slope).clamp(0.0, 1.0);
            if (next - ti).abs() < 1e-15 {
                ti = next;
                break;
            }
            ti = next;
        }
        // Only when the curve really comes nearer there.
        if distance(nurbs.point(ti), p) <= distance(nurbs.point(t[i]), p) {
            t[i] = ti;
        }
    }
}

/// The knot to add: in the span with the largest error above the
/// tolerance that holds at least two points' parameters, between the
/// middle two of them, so that both new spans keep points. None when no
/// such span is left.
fn split_knot(interior: &[f64], t: &[f64], errors: &[f64], tolerance: f64) -> Option<f64> {
    let spans = interior.len() + 1;
    let bound = |s: usize| -> (f64, f64) {
        let lo = if s == 0 { 0.0 } else { interior[s - 1] };
        let hi = if s == spans - 1 { 1.0 } else { interior[s] };
        (lo, hi)
    };
    let mut worst = vec![0.0f64; spans];
    let mut inside: Vec<Vec<f64>> = vec![Vec::new(); spans];
    for (&ti, &e) in t.iter().zip(errors) {
        let s = interior.partition_point(|k| *k <= ti).min(spans - 1);
        worst[s] = worst[s].max(e);
        let (lo, hi) = bound(s);
        if ti > lo && ti < hi {
            inside[s].push(ti);
        }
    }
    let mut order: Vec<usize> = (0..spans).filter(|s| worst[*s] > tolerance).collect();
    order.sort_by(|a, b| worst[*b].total_cmp(&worst[*a]));
    for s in order {
        let ts = &mut inside[s];
        if ts.len() < 2 {
            continue;
        }
        ts.sort_by(f64::total_cmp);
        let h = ts.len() / 2;
        let knot = 0.5 * (ts[h - 1] + ts[h]);
        let (lo, hi) = bound(s);
        let room = 1e-9;
        if knot > lo + room && knot < hi - room {
            return Some(knot);
        }
    }
    None
}
