// SPDX-License-Identifier: MIT
//! A plane's section of a mesh (mitcad#1871): where the plane cuts the
//! triangles, as polylines in the plane's coordinates, and those polylines
//! fitted to lines, arcs and circles within a tolerance, as sketch curves.
//!
//! - The cut: each triangle the plane crosses gives a segment between the
//!   two edges it crosses; segments that share an edge join into
//!   polylines, closed where they come back. A vertex lying in the plane
//!   counts as just above it, so that a plane through vertices (a box's
//!   mid plane through its corners) still gives one point per crossing.
//! - The fit: a closed polyline whose points all lie within the tolerance
//!   of one circle is that circle. Otherwise the polyline is cut into runs
//!   from its sharpest corner on: each run as long as either a straight
//!   line between its ends or an arc through its ends (its centre on their
//!   bisector, fitted to the points between) stays within the tolerance of
//!   every point of it; an arc is taken where it runs half as far again
//!   as the line and bulges at least half the tolerance from its chord,
//!   else the line. Neighbouring curves share their end points, which are
//!   points of the polyline.

use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

use mitcad_vec3::{dot, sub};

use super::{Point, TriangleMesh, edge_key};
use crate::profile::SketchFrame;

/// A polyline in a plane's coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub points: Vec<[f64; 2]>,
    /// The last point joins the first.
    pub closed: bool,
}

/// A curve fitted to a section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SectionCurve {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    /// Counter-clockwise from `start` to `end` about `center`.
    Arc {
        center: [f64; 2],
        start: [f64; 2],
        end: [f64; 2],
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
}

/// A section's curves: each polyline's in its order, the curves of one
/// polyline end to end.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SectionCurves {
    /// The curves of each polyline.
    pub chains: Vec<Vec<SectionCurve>>,
    /// Whether each chain closes.
    pub closed: Vec<bool>,
}

impl SectionCurves {
    pub fn count(&self) -> usize {
        self.chains.iter().map(Vec::len).sum()
    }
}

/// The section of the mesh by the plane of `frame` moved `offset` along its
/// normal, in the frame's coordinates (x, y).
pub fn plane_section(mesh: &TriangleMesh, frame: &SketchFrame, offset: f64) -> Vec<Polyline> {
    let normal = frame.normal();
    let local = |p: Point| {
        let d = sub(p, frame.origin);
        [
            dot(d, frame.x_axis),
            dot(d, frame.y_axis),
            dot(d, normal) - offset,
        ]
    };
    let points: Vec<Point> = mesh.vertices.iter().map(|p| local(*p)).collect();
    // A vertex in the plane counts as above it.
    let above = |v: u32| points[v as usize][2] >= 0.0;
    let crossing = |a: u32, b: u32| -> [f64; 2] {
        let (pa, pb) = (points[a as usize], points[b as usize]);
        let t = pa[2] / (pa[2] - pb[2]);
        [pa[0] + t * (pb[0] - pa[0]), pa[1] + t * (pb[1] - pa[1])]
    };
    // Segments between the crossed edges, by edge.
    let mut segments: Vec<[(u32, u32); 2]> = Vec::new();
    let mut at: HashMap<(u32, u32), [f64; 2]> = HashMap::new();
    for tri in &mesh.triangles {
        let mut crossed: Vec<(u32, u32)> = Vec::with_capacity(2);
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if above(a) != above(b) {
                let key = edge_key(a, b);
                at.entry(key).or_insert_with(|| crossing(key.0, key.1));
                crossed.push(key);
            }
        }
        if crossed.len() == 2 {
            segments.push([crossed[0], crossed[1]]);
        }
    }
    // Join segments that share an edge.
    let mut by_edge: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (s, seg) in segments.iter().enumerate() {
        for key in seg {
            by_edge.entry(*key).or_default().push(s);
        }
    }
    let mut used = vec![false; segments.len()];
    let mut lines = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        // Walk both ways from the first segment.
        let walk = |from: (u32, u32), used: &mut Vec<bool>| -> (Vec<(u32, u32)>, bool) {
            let mut keys = vec![from];
            let mut edge = from;
            loop {
                let next = by_edge
                    .get(&edge)
                    .and_then(|on| on.iter().copied().find(|&s| !used[s]));
                let Some(s) = next else {
                    let closed = edge == segments[start][0];
                    return (keys, closed);
                };
                used[s] = true;
                let seg = segments[s];
                edge = if seg[0] == edge { seg[1] } else { seg[0] };
                keys.push(edge);
            }
        };
        let (forward, closed) = walk(segments[start][1], &mut used);
        let mut keys: Vec<(u32, u32)> = if closed {
            let mut keys = vec![segments[start][0]];
            keys.extend(forward);
            keys.pop();
            keys
        } else {
            let (mut backward, _) = walk(segments[start][0], &mut used);
            backward.reverse();
            backward.extend(forward);
            backward
        };
        keys.dedup();
        let mut pts: Vec<[f64; 2]> = Vec::with_capacity(keys.len());
        for key in &keys {
            let p = at[key];
            if pts
                .last()
                .is_none_or(|q: &[f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-9)
            {
                pts.push(p);
            }
        }
        if closed
            && pts.len() > 1
            && (pts[0][0] - pts[pts.len() - 1][0]).hypot(pts[0][1] - pts[pts.len() - 1][1]) <= 1e-9
        {
            pts.pop();
        }
        if pts.len() >= 2 && !(closed && pts.len() < 3) {
            lines.push(Polyline {
                points: pts,
                closed,
            });
        }
    }
    // Counter-clockwise loops, and an order that does not depend on the
    // triangles' order: by the lowest point.
    for line in &mut lines {
        if line.closed && signed_area(&line.points) < 0.0 {
            line.points.reverse();
        }
    }
    lines.sort_by(|a, b| {
        let low = |l: &Polyline| {
            l.points
                .iter()
                .copied()
                .min_by(|p, q| p[1].total_cmp(&q[1]).then(p[0].total_cmp(&q[0])))
                .unwrap_or([0.0; 2])
        };
        let (pa, pb) = (low(a), low(b));
        pa[1].total_cmp(&pb[1]).then(pa[0].total_cmp(&pb[0]))
    });
    lines
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// The polylines fitted to curves within `tolerance` (see the module docs).
pub fn fit_curves(lines: &[Polyline], tolerance: f64) -> SectionCurves {
    let mut out = SectionCurves::default();
    for line in lines {
        let curves = fit_polyline(line, tolerance);
        if !curves.is_empty() {
            out.chains.push(curves);
            out.closed.push(line.closed);
        }
    }
    out
}

/// The fewest points a fitted arc or circle passes near.
const MIN_ARC_POINTS: usize = 5;

/// One polyline fitted to curves.
pub fn fit_polyline(line: &Polyline, tolerance: f64) -> Vec<SectionCurve> {
    let mut pts = line.points.clone();
    if pts.len() < 2 {
        return Vec::new();
    }
    if line.closed {
        if pts.len() >= MIN_ARC_POINTS.max(8)
            && let Some((center, radius)) = fit_circle(&pts)
            && pts
                .iter()
                .all(|p| ((p[0] - center[0]).hypot(p[1] - center[1]) - radius).abs() <= tolerance)
        {
            return vec![SectionCurve::Circle { center, radius }];
        }
        // From the sharpest corner, so that no curve runs across it.
        let n = pts.len();
        let sharpest = (0..n)
            .max_by(|&i, &j| turn(&pts, i).total_cmp(&turn(&pts, j)).then(j.cmp(&i)))
            .unwrap_or(0);
        pts.rotate_left(sharpest);
        pts.push(pts[0]);
    }
    let mut curves = Vec::new();
    let mut i = 0;
    while i + 1 < pts.len() {
        let line_end =
            farthest(&pts, i, 1, |a, b| line_fits(&pts, a, b, tolerance)).unwrap_or(i + 1);
        let arc_end = farthest(&pts, i, MIN_ARC_POINTS - 1, |a, b| {
            arc_fits(&pts, a, b, tolerance).is_some()
        });
        // An arc only where it runs well past the line.
        if let Some(arc_end) =
            arc_end.filter(|&end| end > line_end && (end - i) * 2 > (line_end - i) * 3)
            && let Some(center) = arc_fits(&pts, i, arc_end, tolerance)
        {
            curves.push(arc_curve(center, pts[i], pts[arc_end], &pts[i..=arc_end]));
            i = arc_end;
        } else {
            curves.push(SectionCurve::Line {
                start: pts[i],
                end: pts[line_end],
            });
            i = line_end;
        }
    }
    if line.closed && curves.len() > 2 {
        merge_across_start(&mut curves, &pts, tolerance);
    }
    curves
}

/// A closed polyline's last and first curves made one where they are the
/// same line or arc (the start fell inside one).
fn merge_across_start(curves: &mut Vec<SectionCurve>, pts: &[[f64; 2]], tolerance: f64) {
    let (Some(last), Some(first)) = (curves.last().copied(), curves.first().copied()) else {
        return;
    };
    let merged = match (last, first) {
        (SectionCurve::Line { start, .. }, SectionCurve::Line { end, .. }) => {
            // Every point of both within the tolerance of one line.
            let run: Vec<[f64; 2]> = pts
                .iter()
                .copied()
                .filter(|p| on_segment(*p, start, pts[0]) || on_segment(*p, pts[0], end))
                .collect();
            let d = [end[0] - start[0], end[1] - start[1]];
            let len = d[0].hypot(d[1]);
            (len > 0.0
                && run.iter().all(|p| {
                    ((p[0] - start[0]) * d[1] - (p[1] - start[1]) * d[0]).abs() / len <= tolerance
                }))
            .then_some(SectionCurve::Line { start, end })
        }
        (
            SectionCurve::Arc {
                center: c1,
                start: s1,
                end: e1,
            },
            SectionCurve::Arc {
                center: c2,
                start: s2,
                end: e2,
            },
        ) => {
            let r1 = (s1[0] - c1[0]).hypot(s1[1] - c1[1]);
            let r2 = (s2[0] - c2[0]).hypot(s2[1] - c2[1]);
            let same =
                (c1[0] - c2[0]).hypot(c1[1] - c2[1]) <= tolerance && (r1 - r2).abs() <= tolerance;
            if !same {
                None
            } else if e1 == s2 {
                Some(SectionCurve::Arc {
                    center: c1,
                    start: s1,
                    end: e2,
                })
            } else if s1 == e2 {
                Some(SectionCurve::Arc {
                    center: c1,
                    start: s2,
                    end: e1,
                })
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(curve) = merged {
        curves.pop();
        curves[0] = curve;
    }
}

/// Whether `p` lies on the straight piece from `a` to `b` (within its box).
fn on_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> bool {
    let within = |v: f64, x: f64, y: f64| v >= x.min(y) - 1e-12 && v <= x.max(y) + 1e-12;
    within(p[0], a[0], b[0]) && within(p[1], a[1], b[1])
}

/// How sharply the polyline turns at point `i` of a closed one (radians).
fn turn(pts: &[[f64; 2]], i: usize) -> f64 {
    let n = pts.len();
    let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
    let u = [b[0] - a[0], b[1] - a[1]];
    let v = [c[0] - b[0], c[1] - b[1]];
    (u[0] * v[1] - u[1] * v[0])
        .atan2(u[0] * v[0] + u[1] * v[1])
        .abs()
}

/// The farthest end `j >= i + min` for which `fits(i, j)`, by doubling
/// and then halving the run; None when the shortest run does not fit.
fn farthest(
    pts: &[[f64; 2]],
    i: usize,
    min: usize,
    fits: impl Fn(usize, usize) -> bool,
) -> Option<usize> {
    let last = pts.len() - 1;
    let first = i + min;
    if first > last || !fits(i, first) {
        return None;
    }
    let mut good = first;
    let mut step = 1;
    let mut bad = None;
    while good < last {
        let next = (good + step).min(last);
        if fits(i, next) {
            good = next;
            step *= 2;
        } else {
            bad = Some(next);
            break;
        }
    }
    if let Some(mut bad) = bad {
        while bad - good > 1 {
            let mid = (good + bad) / 2;
            if fits(i, mid) {
                good = mid;
            } else {
                bad = mid;
            }
        }
    }
    Some(good)
}

/// Whether every point from `a` to `b` lies within `tolerance` of the
/// straight segment between them.
fn line_fits(pts: &[[f64; 2]], a: usize, b: usize, tolerance: f64) -> bool {
    let (p, q) = (pts[a], pts[b]);
    let d = [q[0] - p[0], q[1] - p[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 <= 1e-24 {
        return b == a + 1;
    }
    pts[a + 1..b].iter().all(|r| {
        let t = (((r[0] - p[0]) * d[0] + (r[1] - p[1]) * d[1]) / len2).clamp(0.0, 1.0);
        let c = [p[0] + t * d[0] - r[0], p[1] + t * d[1] - r[1]];
        c[0].hypot(c[1]) <= tolerance
    })
}

/// The centre of an arc from point `a` to point `b` through the points
/// between, within `tolerance` of every one, turning one way and less
/// than a full turn; None when there is none.
fn arc_fits(pts: &[[f64; 2]], a: usize, b: usize, tolerance: f64) -> Option<[f64; 2]> {
    if b < a + MIN_ARC_POINTS - 1 {
        return None;
    }
    let run = &pts[a..=b];
    let (p, q) = (run[0], run[run.len() - 1]);
    let chord = [q[0] - p[0], q[1] - p[1]];
    let length = chord[0].hypot(chord[1]);
    if length <= 1e-12 {
        return None;
    }
    let mid = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
    let normal = [-chord[1] / length, chord[0] / length];
    // The centre on the bisector: start from the least-squares circle's.
    let (guess, _) = fit_circle(run)?;
    let mut t = (guess[0] - mid[0]) * normal[0] + (guess[1] - mid[1]) * normal[1];
    let centre = |t: f64| [mid[0] + t * normal[0], mid[1] + t * normal[1]];
    for _ in 0..20 {
        let c = centre(t);
        let rp = (p[0] - c[0]).hypot(p[1] - c[1]);
        let dp = -((p[0] - c[0]) * normal[0] + (p[1] - c[1]) * normal[1]) / rp;
        let (mut num, mut den) = (0.0, 0.0);
        for r in &run[1..run.len() - 1] {
            let rk = (r[0] - c[0]).hypot(r[1] - c[1]);
            if rk <= 1e-300 {
                return None;
            }
            let dk = -((r[0] - c[0]) * normal[0] + (r[1] - c[1]) * normal[1]) / rk;
            let res = rk - rp;
            let der = dk - dp;
            num += res * der;
            den += der * der;
        }
        if den <= 1e-300 {
            break;
        }
        let step = num / den;
        t -= step;
        if step.abs() <= 1e-12 * (1.0 + t.abs()) {
            break;
        }
    }
    let c = centre(t);
    let radius = (p[0] - c[0]).hypot(p[1] - c[1]);
    if !radius.is_finite() || radius > 1e4 * length.max(tolerance) {
        return None;
    }
    // An arc that bulges less than half the tolerance from its chord is
    // nearly a line: the line is taken there.
    let bulge = radius - (radius * radius - length * length / 4.0).max(0.0).sqrt();
    if bulge <= tolerance / 2.0 && length < 2.0 * radius {
        return None;
    }
    if run
        .iter()
        .any(|r| ((r[0] - c[0]).hypot(r[1] - c[1]) - radius).abs() > tolerance)
    {
        return None;
    }
    // One way round, less than a full turn.
    let angle = |r: [f64; 2]| (r[1] - c[1]).atan2(r[0] - c[0]);
    let mut total = 0.0;
    let mut sign = 0.0;
    for w in run.windows(2) {
        let mut d = angle(w[1]) - angle(w[0]);
        if d > PI {
            d -= TAU;
        } else if d < -PI {
            d += TAU;
        }
        if d.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = d.signum();
        } else if d.signum() != sign {
            return None;
        }
        total += d;
    }
    (total.abs() < TAU - 1e-6 && total.abs() > 1e-6).then_some(c)
}

/// The arc about `center` from `start` to `end` the way the points run, as
/// a counter-clockwise arc.
fn arc_curve(center: [f64; 2], start: [f64; 2], end: [f64; 2], run: &[[f64; 2]]) -> SectionCurve {
    let angle = |r: [f64; 2]| (r[1] - center[1]).atan2(r[0] - center[0]);
    let mut total = 0.0;
    for w in run.windows(2) {
        let mut d = angle(w[1]) - angle(w[0]);
        if d > PI {
            d -= TAU;
        } else if d < -PI {
            d += TAU;
        }
        total += d;
    }
    if total >= 0.0 {
        SectionCurve::Arc { center, start, end }
    } else {
        SectionCurve::Arc {
            center,
            start: end,
            end: start,
        }
    }
}

/// The circle nearest the points: the algebraic fit, refined by
/// Gauss-Newton on the distances; None for points in a line.
pub(crate) fn fit_circle(points: &[[f64; 2]]) -> Option<([f64; 2], f64)> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    let mean = points
        .iter()
        .fold([0.0; 2], |s, p| [s[0] + p[0], s[1] + p[1]]);
    let mean = [mean[0] / n as f64, mean[1] / n as f64];
    // x² + y² + D x + E y + F = 0 about the mean.
    let mut a = vec![0.0; 9];
    let mut b = vec![0.0; 3];
    for p in points {
        let (x, y) = (p[0] - mean[0], p[1] - mean[1]);
        let row = [x, y, 1.0];
        let rhs = -(x * x + y * y);
        for i in 0..3 {
            for j in 0..3 {
                a[i * 3 + j] += row[i] * row[j];
            }
            b[i] += row[i] * rhs;
        }
    }
    let s = super::solve(a, b)?;
    let mut c = [-s[0] / 2.0, -s[1] / 2.0];
    let r2 = c[0] * c[0] + c[1] * c[1] - s[2];
    if r2 <= 0.0 {
        return None;
    }
    let mut r = r2.sqrt();
    for _ in 0..10 {
        let mut jtj = vec![0.0; 9];
        let mut jtr = vec![0.0; 3];
        for p in points {
            let (dx, dy) = (p[0] - mean[0] - c[0], p[1] - mean[1] - c[1]);
            let d = dx.hypot(dy);
            if d <= 1e-300 {
                return None;
            }
            let row = [-dx / d, -dy / d, -1.0];
            let res = d - r;
            for i in 0..3 {
                for j in 0..3 {
                    jtj[i * 3 + j] += row[i] * row[j];
                }
                jtr[i] += row[i] * res;
            }
        }
        let Some(step) = super::solve(jtj, jtr) else {
            break;
        };
        c = [c[0] - step[0], c[1] - step[1]];
        r -= step[2];
        if step.iter().all(|s| s.abs() <= 1e-12 * (1.0 + r.abs())) {
            break;
        }
    }
    (r.is_finite() && r > 0.0).then_some(([c[0] + mean[0], c[1] + mean[1]], r))
}
