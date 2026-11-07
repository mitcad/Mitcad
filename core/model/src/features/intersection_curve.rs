// SPDX-License-Identifier: MIT
//! Intersection Curve (mitcad#1405): the 3D curve whose views along two
//! sketches' normals are curves of those sketches. Each input is a chain
//! of one sketch's curves ([`SketchCurves`], as a sweep's path takes them);
//! the geometry kernel sweeps each chain along its own sketch's normal,
//! through the other's extent, and crosses the two surfaces
//! ([`Kernel::intersection_curves`]). The pieces it gives are joined here
//! into one curve that does not branch; a result that branches or comes in
//! stretches apart is refused, saying where. The curve follows both
//! sketches: it is made again whenever either changes.
//!
//! The curve is the feature's output ([`FeatureOutput::curve`]): later
//! features take it as a path, `{"feature": "F5"}`
//! ([`PathRef::FeatureCurve`](super::PathRef::FeatureCurve)); its pieces
//! are named `F5:curve1`, `F5:curve2`, ... in order along it. It runs from
//! the end nearer to where the first chain starts. [`CurveChain`] measures
//! it along its length for the paths that place things along one (a path
//! pattern, a construction plane or point along a path): along its exact
//! pieces, their speed integrated (mitcad#1417), not along chords. A rib or
//! a web grows from it in the plane it lies in ([`curve_plane`]).

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::mapping::map_params;
use super::path::{SketchCurves, chain};
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::{PathParameter, PathPoint, Vec3};
use crate::kernel::{Curve3, Kernel};
use crate::parameters::ParamId;
use crate::sweeps::PathCurve;
use mitcad_model_base::curves::{
    bspline_point, conic_point, curve_ends, helix_point, reversed_path, tolerance,
};
use mitcad_vec3::{add, cross, distance, norm, scale, sub, unit};

/// Normals whose cross product is shorter than this are parallel.
const PARALLEL: f64 = 1e-9;

/// About how many samples each curve of a [`CurveChain`] keeps its length
/// at (each span's length integrated exactly, mitcad#1417).
const SAMPLES: usize = 2048;

/// Has no values; `P` only matches the other feature types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntersectionCurveDef<P = ParamId> {
    /// A chain of curves of one sketch, swept along its normal.
    pub first: SketchCurves,
    /// A chain of curves of another sketch, on a plane at an angle to the
    /// first's, swept along its normal.
    pub second: SketchCurves,
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> IntersectionCurveDef<P> {
    pub const TYPE: &'static str = "intersection_curve";
    pub const BASE_NAME: &'static str = "IntersectionCurve";

    pub fn new(first: SketchCurves, second: SketchCurves) -> Self {
        Self {
            first,
            second,
            values: PhantomData,
        }
    }

    map_params! {
        IntersectionCurveDef {
            first: clone,
            second: clone,
            values: phantom,
        }
    }
}

impl FeatureInfo for IntersectionCurveDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.first.add_references(&mut references);
        self.second.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.first
            .check(ctx)
            .map_err(|e| format!("the first curves: {e}"))?;
        self.second
            .check(ctx)
            .map_err(|e| format!("the second curves: {e}"))?;
        if self.first.sketch == self.second.sketch {
            let (entry, _) = ctx.sketch(self.first.sketch)?;
            return Err(format!(
                "both chains are curves of {}: take the second from another sketch, \
                 on a plane at an angle to the first",
                entry.name
            ));
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for IntersectionCurveDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (first_frame, first) = self.first.curves(ctx)?;
        let (second_frame, second) = self.second.curves(ctx)?;
        let first = chain(first).map_err(|e| format!("the first curves: {e}"))?;
        let second = chain(second).map_err(|e| format!("the second curves: {e}"))?;
        let (n1, n2) = (first_frame.normal(), second_frame.normal());
        if norm(cross(n1, n2)) <= PARALLEL {
            return Err(format!(
                "{} and {} lie in parallel planes: curves swept along the same normal \
                 cross in no curve; take sketches on planes at an angle",
                ctx.feature_name(self.first.sketch),
                ctx.feature_name(self.second.sketch)
            ));
        }
        let crossing = ctx
            .kernel
            .intersection_curves(&first, n1, &second, n2)
            .map_err(|e| format!("intersection curve: {e}"))?;
        let pieces = crossing
            .into_iter()
            .map(|curve| PathCurve {
                name: String::new(),
                curve,
            })
            .collect();
        let start = first
            .first()
            .and_then(|p| start_of(&p.curve))
            .unwrap_or_default();
        let uid = ctx.uid;
        let curve = joined_curve(pieces, start)?
            .into_iter()
            .enumerate()
            .map(|(i, piece)| PathCurve {
                name: format!("{uid}:curve{}", i + 1),
                curve: piece.curve,
            })
            .collect();
        Ok(FeatureOutput {
            curve: Some(curve),
            ..FeatureOutput::default()
        })
    }
}

/// The crossing's pieces as one curve: in order along it, each running
/// along it, from the end nearer to `start` (a closed curve as it comes).
/// Points (where the surfaces only touch) are left out. The error says why
/// they are not one curve: there are none, they branch (where), or they
/// come in stretches apart (each from where to where).
pub(crate) fn joined_curve(pieces: Vec<PathCurve>, start: Vec3) -> Result<Vec<PathCurve>, String> {
    let pieces: Vec<PathCurve> = pieces
        .into_iter()
        .filter(|p| !matches!(p.curve, Curve3::Point(_)))
        .collect();
    if pieces.is_empty() {
        return Err(
            "the curves do not cross: swept along their sketches' normals, their surfaces \
             have no curve in common"
                .to_owned(),
        );
    }
    if pieces.len() == 1 {
        return Ok(oriented(pieces, start));
    }
    let tolerance = tolerance(&pieces);
    let mut nodes: Vec<Vec3> = Vec::new();
    let mut node_of = |p: Vec3| -> usize {
        if let Some(i) = nodes.iter().position(|q| distance(*q, p) <= tolerance) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    // Each piece's end nodes; None for a closed piece.
    let links: Vec<Option<(usize, usize)>> = pieces
        .iter()
        .map(|p| curve_ends(&p.curve).map(|(a, b)| (node_of(a), node_of(b))))
        .collect();
    let mut degree = vec![0usize; nodes.len()];
    for (a, b) in links.iter().flatten() {
        degree[*a] += 1;
        degree[*b] += 1;
    }
    let forks: Vec<Vec3> = nodes
        .iter()
        .zip(&degree)
        .filter(|(_, d)| **d > 2)
        .map(|(p, _)| *p)
        .collect();
    if !forks.is_empty() {
        return Err(format!(
            "the curve branches at {}: it must be one curve without forks",
            points_text(&forks)
        ));
    }
    // The stretches: the pieces joined through their nodes; a closed piece
    // is one of its own.
    let mut group: Vec<usize> = (0..nodes.len()).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while group[i] != i {
            group[i] = group[group[i]];
            i = group[i];
        }
        i
    }
    for (a, b) in links.iter().flatten() {
        let (ra, rb) = (root(&mut group, *a), root(&mut group, *b));
        group[ra] = rb;
    }
    let mut stretches: Vec<String> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    for (piece, link) in pieces.iter().zip(&links) {
        let Some((a, _)) = link else {
            let at = start_of(&piece.curve).unwrap_or_default();
            stretches.push(format!("a closed loop through {}", point_text(at)));
            continue;
        };
        let r = root(&mut group, *a);
        if seen.contains(&r) {
            continue;
        }
        seen.push(r);
        let ends: Vec<Vec3> = (0..nodes.len())
            .filter(|n| degree[*n] == 1 && root(&mut group, *n) == r)
            .map(|n| nodes[n])
            .collect();
        stretches.push(match ends.as_slice() {
            [from, to] => format!("from {} to {}", point_text(*from), point_text(*to)),
            _ => format!("a closed loop through {}", point_text(nodes[*a])),
        });
    }
    if stretches.len() > 1 {
        return Err(format!(
            "the curves cross in {} separate stretches ({}): it must be one curve",
            stretches.len(),
            stretches.join("; ")
        ));
    }
    Ok(oriented(chain(pieces)?, start))
}

/// The curve run from its end nearer to `start`.
fn oriented(curve: Vec<PathCurve>, start: Vec3) -> Vec<PathCurve> {
    let first = curve.first().and_then(|p| curve_ends(&p.curve));
    let last = curve.last().and_then(|p| curve_ends(&p.curve));
    match (first, last) {
        (Some((a, _)), Some((_, b))) if distance(b, start) < distance(a, start) - 1e-9 => {
            reversed_path(curve)
        }
        _ => curve,
    }
}

/// `(x, y, z)` to the micrometre.
fn point_text(p: Vec3) -> String {
    format!("({:.3}, {:.3}, {:.3})", p[0] + 0.0, p[1] + 0.0, p[2] + 0.0)
}

fn points_text(points: &[Vec3]) -> String {
    points
        .iter()
        .map(|p| point_text(*p))
        .collect::<Vec<_>>()
        .join(" and ")
}

/// A curve's parameter range; None for a point.
fn range(curve: &Curve3) -> Option<(f64, f64)> {
    match curve {
        Curve3::Point(_) => None,
        Curve3::Line { .. } => Some((0.0, 1.0)),
        Curve3::Conic { start, end, .. } | Curve3::Helix { start, end, .. } => Some((*start, *end)),
        Curve3::BSpline {
            degree,
            poles,
            knots,
            ..
        } => {
            let p = *degree as usize;
            (p > 0 && poles.len() > p && knots.len() == poles.len() + p + 1)
                .then(|| (knots[p], knots[poles.len()]))
        }
    }
}

/// A curve's point at parameter `t` of its [`range`].
fn point(curve: &Curve3, t: f64) -> Vec3 {
    match curve {
        Curve3::Point(p) => *p,
        Curve3::Line { start, end } => add(*start, scale(sub(*end, *start), t)),
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            ..
        } => conic_point(*center, *normal, *x_axis, (*major, *minor), t),
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            ..
        } => helix_point(*center, *axis, *x_axis, (*radius, *pitch), t),
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => bspline_point(*degree as usize, poles, weights, knots, t),
    }
}

/// Points a drawn piece other than a line is sampled at.
const DRAWN: usize = 64;

/// The curve as one polyline, for drawing it (the `feature_curves` query):
/// a line's ends, other pieces at [`DRAWN`] points along their parameter,
/// the point where two pieces meet once.
pub fn curve_polyline(curve: &[PathCurve]) -> Vec<Vec3> {
    let mut points: Vec<Vec3> = Vec::new();
    for piece in curve {
        let Some((t0, t1)) = range(&piece.curve) else {
            continue;
        };
        let count = if matches!(piece.curve, Curve3::Line { .. }) {
            1
        } else {
            DRAWN
        };
        for i in 0..=count {
            let p = point(&piece.curve, t0 + (t1 - t0) * i as f64 / count as f64);
            if points.last().is_none_or(|q| distance(*q, p) > 1e-9) {
                points.push(p);
            }
        }
    }
    points
}

/// Where a curve starts.
fn start_of(curve: &Curve3) -> Option<Vec3> {
    range(curve).map(|(t0, _)| point(curve, t0))
}

/// A curve's derivative at parameter `t` of its [`range`] (mitcad#1417:
/// its speed, to measure it along itself rather than along chords).
fn derivative(curve: &Curve3, t: f64) -> Vec3 {
    match curve {
        Curve3::Point(_) => [0.0; 3],
        Curve3::Line { start, end } => sub(*end, *start),
        Curve3::Conic {
            normal,
            x_axis,
            major,
            minor,
            ..
        } => {
            let y_axis = cross(*normal, *x_axis);
            add(
                scale(*x_axis, -major * t.sin()),
                scale(y_axis, minor * t.cos()),
            )
        }
        Curve3::Helix {
            axis,
            x_axis,
            radius,
            pitch,
            ..
        } => {
            let y_axis = cross(*axis, *x_axis);
            add(
                add(
                    scale(*x_axis, -radius * t.sin()),
                    scale(y_axis, radius * t.cos()),
                ),
                scale(*axis, pitch / std::f64::consts::TAU),
            )
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => bspline_derivative(*degree as usize, poles, weights, knots, t),
    }
}

/// A point of a B-spline in homogeneous coordinates (de Boor), as
/// [`bspline_point`] finds it; `knots` the full knot vector.
fn de_boor(degree: usize, points: &[[f64; 4]], knots: &[f64], t: f64) -> [f64; 4] {
    let n = points.len();
    let p = degree;
    let mut k = p;
    while k + 1 < n && knots[k + 1] <= t {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=p).map(|j| points[k + j - p]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k + j - p;
            let span = knots[i + p + 1 - r] - knots[i];
            let alpha = if span == 0.0 {
                0.0
            } else {
                (t - knots[i]) / span
            };
            d[j] = std::array::from_fn(|c| (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c]);
        }
    }
    d[p]
}

/// The derivative of a (rational) B-spline at `t`: the homogeneous curve's
/// derivative, a B-spline of one degree less on the knots without the
/// first and the last, and the quotient rule.
fn bspline_derivative(
    degree: usize,
    poles: &[[f64; 3]],
    weights: &[f64],
    knots: &[f64],
    t: f64,
) -> Vec3 {
    let n = poles.len();
    let p = degree;
    if p == 0 || n <= p || knots.len() != n + p + 1 {
        return [0.0; 3];
    }
    let homogeneous: Vec<[f64; 4]> = poles
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let w = weights.get(i).copied().unwrap_or(1.0);
            [q[0] * w, q[1] * w, q[2] * w, w]
        })
        .collect();
    let a = de_boor(p, &homogeneous, knots, t);
    let differences: Vec<[f64; 4]> = (0..n - 1)
        .map(|i| {
            let span = knots[i + p + 1] - knots[i + 1];
            let f = if span == 0.0 { 0.0 } else { p as f64 / span };
            std::array::from_fn(|c| f * (homogeneous[i + 1][c] - homogeneous[i][c]))
        })
        .collect();
    let da = de_boor(p - 1, &differences, &knots[1..knots.len() - 1], t);
    let w = a[3];
    if w == 0.0 {
        return [0.0; 3];
    }
    std::array::from_fn(|c| (da[c] * w - a[c] * da[3]) / (w * w))
}

/// The curve's speed (the length of its derivative) at `t`.
fn speed(curve: &Curve3, t: f64) -> f64 {
    norm(derivative(curve, t))
}

/// Gauss-Legendre nodes on [-1, 1] and their weights, 8 points.
#[allow(clippy::excessive_precision)] // the published values, digit for digit
const GAUSS: [(f64, f64); 8] = [
    (-0.960_289_856_497_536_2, 0.101_228_536_290_376_3),
    (-0.796_666_477_413_626_7, 0.222_381_034_453_374_5),
    (-0.525_532_409_916_329_0, 0.313_706_645_877_887_3),
    (-0.183_434_642_495_649_8, 0.362_683_783_378_362_0),
    (0.183_434_642_495_649_8, 0.362_683_783_378_362_0),
    (0.525_532_409_916_329_0, 0.313_706_645_877_887_3),
    (0.796_666_477_413_626_7, 0.222_381_034_453_374_5),
    (0.960_289_856_497_536_2, 0.101_228_536_290_376_3),
];

/// The curve's length from parameter `a` to `b`: its speed integrated
/// (Gauss-Legendre, exact to rounding on the short spans it is used on,
/// which lie within one knot span of a B-spline).
fn length_between(curve: &Curve3, a: f64, b: f64) -> f64 {
    if b == a {
        return 0.0;
    }
    let (middle, half) = ((a + b) / 2.0, (b - a) / 2.0);
    half * GAUSS
        .iter()
        .map(|(x, w)| w * speed(curve, middle + half * x))
        .sum::<f64>()
}

/// Where a curve's parameter range is split before it is measured: its
/// ends, and a B-spline's knots between them (its speed may turn sharply
/// there).
fn breaks(curve: &Curve3, t0: f64, t1: f64) -> Vec<f64> {
    let mut at = vec![t0];
    if let Curve3::BSpline {
        degree,
        poles,
        knots,
        ..
    } = curve
    {
        let p = *degree as usize;
        if p > 0 && poles.len() > p && knots.len() == poles.len() + p + 1 {
            for k in &knots[p + 1..poles.len()] {
                if *k > at[at.len() - 1] && *k < t1 {
                    at.push(*k);
                }
            }
        }
    }
    at.push(t1);
    at
}

/// A curve measured along its length (mitcad#1417: along the curve itself,
/// its exact piece, not along chords): its parameters at about [`SAMPLES`]
/// points (a B-spline's knots among them) and the arc length from the
/// chain's start to each, each span's length its speed integrated
/// ([`length_between`]). Between the samples a parameter is found by
/// Newton's method on the same integral, so a point at a length lies on the
/// curve and at that length to rounding.
struct Measured {
    curve: Curve3,
    params: Vec<f64>,
    lengths: Vec<f64>,
}

impl Measured {
    fn new(curve: Curve3, before: f64) -> Option<Self> {
        let (t0, t1) = range(&curve)?;
        if t1 <= t0 {
            return None;
        }
        let at = breaks(&curve, t0, t1);
        let mut params = vec![t0];
        for pair in at.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let count = ((SAMPLES as f64) * (b - a) / (t1 - t0)).ceil().max(1.0) as usize;
            for i in 1..=count {
                params.push(if i == count {
                    b
                } else {
                    a + (b - a) * i as f64 / count as f64
                });
            }
        }
        let mut lengths = Vec::with_capacity(params.len());
        let mut s = before;
        lengths.push(s);
        for pair in params.windows(2) {
            s += length_between(&curve, pair[0], pair[1]);
            lengths.push(s);
        }
        Some(Self {
            curve,
            params,
            lengths,
        })
    }

    fn last(&self) -> usize {
        self.params.len() - 1
    }

    /// The parameter where the curve starts.
    fn first_param(&self) -> f64 {
        self.params[0]
    }

    /// The parameter where the curve ends.
    fn last_param(&self) -> f64 {
        self.params[self.last()]
    }

    fn end(&self) -> f64 {
        self.lengths[self.last()]
    }

    /// The parameter at arc length `s` (within the curve).
    fn param(&self, s: f64) -> f64 {
        let i = self
            .lengths
            .partition_point(|l| *l < s)
            .clamp(1, self.last());
        let (a, b) = (self.params[i - 1], self.params[i]);
        let (l0, l1) = (self.lengths[i - 1], self.lengths[i]);
        let wanted = s - l0;
        // From where an even speed would put it, Newton's steps on the
        // length from the sample.
        let mut t = if l1 > l0 {
            a + (b - a) * (wanted / (l1 - l0)).clamp(0.0, 1.0)
        } else {
            a
        };
        for _ in 0..12 {
            let v = speed(&self.curve, t);
            if v <= 0.0 || !v.is_finite() {
                break;
            }
            let step = (length_between(&self.curve, a, t) - wanted) / v;
            let next = (t - step).clamp(a, b);
            let moved = (next - t).abs();
            t = next;
            if moved <= 1e-15 * (b - a).abs() {
                break;
            }
        }
        t
    }

    /// The arc length at parameter `t` (within the curve).
    fn length_at(&self, t: f64) -> f64 {
        let i = self
            .params
            .partition_point(|p| *p <= t)
            .clamp(1, self.last());
        let (a, b) = (self.params[i - 1], self.params[i]);
        self.lengths[i - 1] + length_between(&self.curve, a, t.clamp(a, b))
    }

    /// The unit tangent at parameter `t`, along the curve's direction.
    fn tangent(&self, t: f64) -> Vec3 {
        if let Some(along) = unit(derivative(&self.curve, t)) {
            return along;
        }
        // A point where the derivative vanishes: the chord through it.
        let (t0, t1) = (self.first_param(), self.last_param());
        let h = (t1 - t0) * 1e-6;
        let (a, b) = ((t - h).max(t0), (t + h).min(t1));
        let d = sub(point(&self.curve, b), point(&self.curve, a));
        unit(d).unwrap_or([1.0, 0.0, 0.0])
    }

    /// The parameter of the curve's point nearest to `p`.
    fn nearest(&self, p: Vec3) -> f64 {
        let at = |t: f64| distance(point(&self.curve, t), p);
        let last = self.last();
        let i = (0..=last)
            .min_by(|a, b| at(self.params[*a]).total_cmp(&at(self.params[*b])))
            .unwrap_or(0);
        // Golden section between the samples beside the nearest.
        let (mut a, mut b) = (
            self.params[i.saturating_sub(1)],
            self.params[(i + 1).min(last)],
        );
        let ratio = (5.0_f64.sqrt() - 1.0) / 2.0;
        for _ in 0..80 {
            let c = b - ratio * (b - a);
            let d = a + ratio * (b - a);
            if at(c) < at(d) {
                b = d;
            } else {
                a = c;
            }
        }
        (a + b) / 2.0
    }
}

/// A chain of curves in order, each running along it (an Intersection
/// Curve's), measured along its length: what a path pattern and a
/// construction plane or point along a path read of it.
pub(crate) struct CurveChain {
    pieces: Vec<Measured>,
    closed: bool,
}

impl CurveChain {
    pub(crate) fn new(curve: &[PathCurve]) -> Result<Self, String> {
        let mut pieces = Vec::new();
        let mut before = 0.0;
        for piece in curve {
            if let Some(measured) = Measured::new(piece.curve.clone(), before) {
                before = measured.end();
                pieces.push(measured);
            }
        }
        if pieces.is_empty() {
            return Err("the path has no curves".to_owned());
        }
        let closed = match (curve.first(), curve.last()) {
            (Some(first), _) if curve_ends(&first.curve).is_none() => true,
            (Some(first), Some(last)) => {
                match (curve_ends(&first.curve), curve_ends(&last.curve)) {
                    (Some((a, _)), Some((_, b))) => distance(a, b) <= tolerance(curve),
                    _ => false,
                }
            }
            _ => false,
        };
        Ok(Self { pieces, closed })
    }

    pub(crate) fn length(&self) -> f64 {
        self.pieces.last().map_or(0.0, Measured::end)
    }

    /// The point and the unit tangent at arc length `s`; before the start
    /// and past the end the chain goes on straight along its tangent there.
    /// Along a closed chain `s` goes round it again.
    pub(crate) fn at(&self, s: f64) -> (Vec3, Vec3) {
        let length = self.length();
        let s = if self.closed && length > 0.0 {
            s.rem_euclid(length)
        } else {
            s
        };
        let first = &self.pieces[0];
        let last = &self.pieces[self.pieces.len() - 1];
        if s < 0.0 {
            let t = first.first_param();
            let tangent = first.tangent(t);
            return (add(point(&first.curve, t), scale(tangent, s)), tangent);
        }
        if s > length {
            let t = last.last_param();
            let tangent = last.tangent(t);
            return (
                add(point(&last.curve, t), scale(tangent, s - length)),
                tangent,
            );
        }
        let piece = self.pieces.iter().find(|p| s <= p.end()).unwrap_or(last);
        let t = piece.param(s);
        (point(&piece.curve, t), piece.tangent(t))
    }

    /// The arc length of the chain's point nearest to `p`.
    pub(crate) fn nearest(&self, p: Vec3) -> f64 {
        let mut best = (f64::INFINITY, 0.0);
        for piece in &self.pieces {
            let t = piece.nearest(p);
            let d = distance(point(&piece.curve, t), p);
            if d < best.0 - 1e-12 {
                best = (d, piece.length_at(t));
            }
        }
        best.1
    }

    /// A point of the chain as [`PathParameter`] gives it.
    pub(crate) fn path_point(&self, at: PathParameter) -> PathPoint {
        let s = match at {
            PathParameter::Fraction(f) => f * self.length(),
            PathParameter::Length(l) => l,
            PathParameter::Near(p) => self.nearest(p),
        };
        let (point, tangent) = self.at(s);
        PathPoint { point, tangent }
    }
}

/// The plane an Intersection Curve lies in, for a rib or a web grown from
/// it (mitcad#1417), as a frame through its start, its x axis towards the
/// curve's point farthest from there. Its normal points the way of the
/// one of `normals` (the curve's sketches') it is most nearly parallel to,
/// so that a curve in a plane parallel to a sketch's grows as a rib drawn
/// in that sketch would. An error when the curve is straight, or a point
/// of it is farther off the plane than the joining tolerance.
pub(crate) fn curve_plane(
    curve: &[PathCurve],
    normals: &[Vec3],
) -> Result<crate::profile::SketchFrame, String> {
    let points = curve_polyline(curve);
    let Some(&start) = points.first() else {
        return Err("the curve has no points".to_owned());
    };
    let tolerance = tolerance(curve);
    let farthest = |from: &dyn Fn(Vec3) -> f64| {
        points
            .iter()
            .copied()
            .max_by(|a, b| from(*a).total_cmp(&from(*b)))
            .unwrap_or(start)
    };
    let far = farthest(&|q| distance(q, start));
    let Some(along) = unit(sub(far, start)) else {
        return Err("the curve has no length".to_owned());
    };
    let off = |q: Vec3| norm(cross(along, sub(q, start)));
    let aside = farthest(&off);
    if off(aside) <= tolerance {
        return Err(
            "the curve is straight: it lies in no one plane for a rib or a web to grow in"
                .to_owned(),
        );
    }
    let Some(mut normal) = unit(cross(along, sub(aside, start))) else {
        return Err("the curve is straight".to_owned());
    };
    if let Some(worst) = points
        .iter()
        .copied()
        .max_by(|a, b| {
            mitcad_vec3::dot(sub(*a, start), normal)
                .abs()
                .total_cmp(&mitcad_vec3::dot(sub(*b, start), normal).abs())
        })
        .filter(|q| mitcad_vec3::dot(sub(*q, start), normal).abs() > tolerance)
    {
        return Err(format!(
            "the curve does not lie in one plane ({} is {:.6} mm off the plane through {} \
             and {}): a rib or a web grows in its curve's plane",
            point_text(worst),
            mitcad_vec3::dot(sub(worst, start), normal).abs(),
            point_text(start),
            point_text(far)
        ));
    }
    if let Some(sketch) = normals.iter().copied().max_by(|a, b| {
        mitcad_vec3::dot(*a, normal)
            .abs()
            .total_cmp(&mitcad_vec3::dot(*b, normal).abs())
    }) && mitcad_vec3::dot(sketch, normal) < 0.0
    {
        normal = scale(normal, -1.0);
    }
    Ok(crate::profile::SketchFrame {
        origin: start,
        x_axis: along,
        y_axis: cross(normal, along),
    })
}

/// The rotation about `pivot` that turns the unit tangent `from` onto `to`
/// (a path pattern's copies turned along an Intersection Curve).
pub(crate) fn turned(pivot: Vec3, from: Vec3, to: Vec3) -> crate::transform::Transform {
    let fallback = mitcad_model_base::internal::transform::perpendicular(from);
    mitcad_model_base::internal::transform::rotation_between(pivot, from, to, fallback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: Vec3, b: Vec3) -> PathCurve {
        PathCurve {
            name: String::new(),
            curve: Curve3::Line { start: a, end: b },
        }
    }

    #[test]
    fn pieces_in_any_order_join_into_one_curve_from_the_start() {
        let pieces = vec![
            line([10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
            line([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
            line([10.0, 10.0, 5.0], [10.0, 10.0, 0.0]),
        ];
        let curve = joined_curve(pieces, [10.0, 10.0, 6.0]).unwrap();
        assert_eq!(curve.len(), 3);
        assert_eq!(
            curve_ends(&curve[0].curve).unwrap().0,
            [10.0, 10.0, 5.0],
            "starts at the end nearer to the start given"
        );
        assert_eq!(curve_ends(&curve[2].curve).unwrap().1, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_branching_result_says_where() {
        let pieces = vec![
            line([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
            line([10.0, 0.0, 0.0], [20.0, 0.0, 0.0]),
            line([10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
        ];
        let error = joined_curve(pieces, [0.0; 3]).unwrap_err();
        assert!(
            error.contains("branches at (10.000, 0.000, 0.000)"),
            "{error}"
        );
    }

    #[test]
    fn stretches_apart_are_listed() {
        let pieces = vec![
            line([0.0, 0.0, 0.0], [10.0, 0.0, 0.0]),
            line([0.0, 5.0, 0.0], [10.0, 5.0, 0.0]),
        ];
        let error = joined_curve(pieces, [0.0; 3]).unwrap_err();
        assert!(error.contains("2 separate stretches"), "{error}");
        assert!(
            error.contains("from (0.000, 0.000, 0.000) to (10.000, 0.000, 0.000)"),
            "{error}"
        );
        assert!(
            error.contains("from (0.000, 5.000, 0.000) to (10.000, 5.000, 0.000)"),
            "{error}"
        );
    }

    #[test]
    fn no_pieces_is_no_crossing() {
        let error = joined_curve(Vec::new(), [0.0; 3]).unwrap_err();
        assert!(error.contains("do not cross"), "{error}");
    }

    #[test]
    fn a_chain_is_measured_along_its_length() {
        // A quarter circle of radius 10 about the origin in XY, then a line.
        let arc = PathCurve {
            name: String::new(),
            curve: Curve3::Conic {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 10.0,
                minor: 10.0,
                start: 0.0,
                end: std::f64::consts::FRAC_PI_2,
                closed: false,
            },
        };
        let chain = CurveChain::new(&[arc, line([0.0, 10.0, 0.0], [-5.0, 10.0, 0.0])]).unwrap();
        let quarter = 5.0 * std::f64::consts::PI;
        assert!((chain.length() - quarter - 5.0).abs() < 1e-4);
        let (p, t) = chain.at(quarter / 2.0);
        let r = 10.0 / 2.0_f64.sqrt();
        assert!(distance(p, [r, r, 0.0]) < 1e-4, "{p:?}");
        assert!(
            distance(t, [-0.5_f64.sqrt(), 0.5_f64.sqrt(), 0.0]) < 1e-4,
            "{t:?}"
        );
        let (p, _) = chain.at(quarter + 2.0);
        assert!(distance(p, [-2.0, 10.0, 0.0]) < 1e-4, "{p:?}");
        assert!((chain.nearest([20.0, 0.1, 0.0]) - 0.05).abs() < 1e-3);
        let end = chain.path_point(PathParameter::Fraction(1.0));
        assert!(distance(end.point, [-5.0, 10.0, 0.0]) < 1e-6);
    }

    /// A line 300 long up Z from (20, 0, 0), then a quarter circle of
    /// length 700 turning from +Z to +X: 1 m (mitcad#1417).
    fn metre() -> (Vec<PathCurve>, f64) {
        let r = 1400.0 / std::f64::consts::PI;
        let arc = PathCurve {
            name: String::new(),
            curve: Curve3::Conic {
                center: [20.0 + r, 0.0, 300.0],
                normal: [0.0, 1.0, 0.0],
                x_axis: [-1.0, 0.0, 0.0],
                major: r,
                minor: r,
                start: 0.0,
                end: std::f64::consts::FRAC_PI_2,
                closed: false,
            },
        };
        (vec![line([20.0, 0.0, 0.0], [20.0, 0.0, 300.0]), arc], r)
    }

    /// The point of [`metre`] at length `s`, and its tangent.
    fn metre_at(s: f64, r: f64) -> (Vec3, Vec3) {
        if s <= 300.0 {
            return ([20.0, 0.0, s], [0.0, 0.0, 1.0]);
        }
        let angle = (s - 300.0) / r;
        (
            [20.0 + r - r * angle.cos(), 0.0, 300.0 + r * angle.sin()],
            [angle.sin(), 0.0, angle.cos()],
        )
    }

    #[test]
    fn a_metre_long_chain_is_measured_along_itself_within_a_nanometre() {
        let (curve, r) = metre();
        let chain = CurveChain::new(&curve).unwrap();
        assert!((chain.length() - 1000.0).abs() < 1e-9, "{}", chain.length());
        // Chords of 2048 samples would put the arc's points about 2e-8 of
        // the length short: 17 nm at 700 mm.
        for s in [0.0, 150.0, 300.0, 300.5, 450.0, 650.0, 777.7, 999.0, 1000.0] {
            let (want, tangent) = metre_at(s, r);
            let got = chain.path_point(PathParameter::Length(s));
            assert!(
                distance(got.point, want) < 1e-6,
                "at {s}: {:?}, not {want:?}",
                got.point
            );
            assert!(distance(got.tangent, tangent) < 1e-9, "at {s}");
            let fraction = chain.path_point(PathParameter::Fraction(s / 1000.0));
            assert!(distance(fraction.point, want) < 1e-6, "at {s} / 1000");
            // And back: the point's length along the chain.
            assert!((chain.nearest(want) - s).abs() < 1e-6, "nearest at {s}");
        }
    }

    #[test]
    fn a_rational_spline_is_measured_along_itself() {
        // A quarter circle of radius 500 as a rational quadratic B-spline,
        // its speed uneven along its parameter.
        let r = 500.0;
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let spline = PathCurve {
            name: String::new(),
            curve: Curve3::BSpline {
                degree: 2,
                poles: vec![[r, 0.0, 0.0], [r, r, 0.0], [0.0, r, 0.0]],
                weights: vec![1.0, w, 1.0],
                knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            },
        };
        let chain = CurveChain::new(&[spline]).unwrap();
        let quarter = r * std::f64::consts::FRAC_PI_2;
        assert!(
            (chain.length() - quarter).abs() < 1e-9,
            "{}",
            chain.length()
        );
        for f in [0.1, 0.25, 0.5, 0.8] {
            let angle = f * std::f64::consts::FRAC_PI_2;
            let want = [r * angle.cos(), r * angle.sin(), 0.0];
            let got = chain.path_point(PathParameter::Fraction(f)).point;
            assert!(distance(got, want) < 1e-6, "at {f}: {got:?}, not {want:?}");
        }
    }

    #[test]
    fn the_plane_of_a_curve_faces_its_sketch() {
        let (curve, _) = metre();
        // In XZ; the sketch normals -Y and +Z: the normal is -Y.
        let frame = curve_plane(&curve, &[[0.0, -1.0, 0.0], [0.0, 0.0, 1.0]]).unwrap();
        assert!(distance(frame.normal(), [0.0, -1.0, 0.0]) < 1e-12);
        assert!(distance(frame.origin, [20.0, 0.0, 0.0]) < 1e-12);
        let frame = curve_plane(&curve, &[[0.0, 1.0, 0.0]]).unwrap();
        assert!(distance(frame.normal(), [0.0, 1.0, 0.0]) < 1e-12);
        // Straight: no plane.
        let error = curve_plane(&[line([0.0; 3], [10.0, 0.0, 0.0])], &[]).unwrap_err();
        assert!(error.contains("straight"), "{error}");
        // Off its plane: refused, saying where.
        let bent = vec![
            line([0.0; 3], [10.0, 0.0, 0.0]),
            line([10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
            line([10.0, 10.0, 0.0], [10.0, 10.0, 5.0]),
        ];
        let error = curve_plane(&bent, &[]).unwrap_err();
        assert!(error.contains("does not lie in one plane"), "{error}");
    }
}
