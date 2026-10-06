// SPDX-License-Identifier: MIT
//! Intersections of solved sketch curves: closed forms for lines, circles
//! and arcs, and for every other pair candidates from sampled polylines
//! refined by damped Gauss-Newton on `A(t) - B(s)`, which also finds
//! touching (tangent) curves. Points where a curve ends on another curve
//! (shared points, T-junctions) are found from the end points.

use std::f64::consts::TAU;

use super::geometry::{Curve2, P2, angle_after, cross, dist, dot, lerp, norm, sub};

/// A point common to two curves, with its parameter on each.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub ta: f64,
    pub tb: f64,
    pub point: P2,
}

/// The intersections of two curves, within `tol` (millimetres), including
/// where an end of one lies on the other.
pub fn intersections(a: &Curve2, b: &Curve2, tol: f64) -> Vec<Hit> {
    let mut hits = match (a, b) {
        (Curve2::Line { .. }, Curve2::Line { .. }) => line_line(a, b, tol),
        (Curve2::Line { .. }, Curve2::Circle { .. } | Curve2::Arc { .. }) => line_round(a, b, tol),
        (Curve2::Circle { .. } | Curve2::Arc { .. }, Curve2::Line { .. }) => {
            line_round(b, a, tol).into_iter().map(swap).collect()
        }
        (
            Curve2::Circle { .. } | Curve2::Arc { .. },
            Curve2::Circle { .. } | Curve2::Arc { .. },
        ) => round_round(a, b, tol),
        _ => numeric(a, b, tol),
    };
    hits.extend(end_contacts(a, b, tol));
    hits.extend(end_contacts(b, a, tol).into_iter().map(swap));
    dedupe(hits, tol)
}

fn swap(h: Hit) -> Hit {
    Hit {
        ta: h.tb,
        tb: h.ta,
        point: h.point,
    }
}

/// Ends of `a` that lie on `b`.
fn end_contacts(a: &Curve2, b: &Curve2, tol: f64) -> Vec<Hit> {
    let Some((start, end)) = a.ends() else {
        return Vec::new();
    };
    let (lo, hi) = a.domain();
    let mut hits = Vec::new();
    for (ta, p) in [(lo, start), (hi, end)] {
        let (tb, d) = b.closest(p);
        if d <= tol {
            hits.push(Hit { ta, tb, point: p });
        }
    }
    hits
}

/// Hits closer than a few tolerances are one; an end point wins over a
/// computed point.
fn dedupe(hits: Vec<Hit>, tol: f64) -> Vec<Hit> {
    let mut kept: Vec<Hit> = Vec::new();
    for hit in hits {
        if !kept.iter().any(|k| dist(k.point, hit.point) <= 10.0 * tol) {
            kept.push(hit);
        }
    }
    kept
}

/// The parameter of `angle` on a round curve, if it lies on it (within
/// `eps` radians of an arc's range).
fn round_param(curve: &Curve2, angle: f64, eps: f64) -> Option<f64> {
    match curve {
        Curve2::Circle { .. } => Some(angle.rem_euclid(TAU)),
        Curve2::Arc { start, end, .. } => {
            let t = angle_after(start - eps, angle);
            (t <= end + eps).then(|| t.clamp(*start, *end))
        }
        _ => None,
    }
}

fn round_of(curve: &Curve2) -> (P2, f64) {
    match curve {
        Curve2::Circle { center, radius } | Curve2::Arc { center, radius, .. } => {
            (*center, *radius)
        }
        _ => unreachable!("a circle or an arc"),
    }
}

fn line_of(curve: &Curve2) -> (P2, P2) {
    match curve {
        Curve2::Line { a, b } => (*a, *b),
        _ => unreachable!("a line"),
    }
}

fn line_param(a: P2, b: P2, t: f64, tol: f64) -> Option<f64> {
    let eps = tol / dist(a, b).max(1e-300);
    (-eps..=1.0 + eps).contains(&t).then(|| t.clamp(0.0, 1.0))
}

fn line_line(a: &Curve2, b: &Curve2, tol: f64) -> Vec<Hit> {
    let (p, p2) = line_of(a);
    let (q, q2) = line_of(b);
    let r = sub(p2, p);
    let s = sub(q2, q);
    let denom = cross(r, s);
    // Parallel lines meet only at ends (found from the end points).
    if denom.abs() <= 1e-14 * norm(r) * norm(s) {
        return Vec::new();
    }
    let qp = sub(q, p);
    let t = cross(qp, s) / denom;
    let u = cross(qp, r) / denom;
    match (line_param(p, p2, t, tol), line_param(q, q2, u, tol)) {
        (Some(t), Some(u)) => vec![Hit {
            ta: t,
            tb: u,
            point: lerp(p, p2, t),
        }],
        _ => Vec::new(),
    }
}

fn line_round(line: &Curve2, round: &Curve2, tol: f64) -> Vec<Hit> {
    let (a, b) = line_of(line);
    let (c, r) = round_of(round);
    let d = sub(b, a);
    let len = norm(d);
    if len == 0.0 {
        return Vec::new();
    }
    let u = [d[0] / len, d[1] / len];
    // Foot of the perpendicular from the centre, and its distance.
    let along = dot(sub(c, a), u);
    let foot = [a[0] + u[0] * along, a[1] + u[1] * along];
    let h = dist(c, foot);
    let mut points = Vec::new();
    if (h - r).abs() <= tol {
        points.push(foot);
    } else if h < r {
        let half = (r * r - h * h).sqrt();
        points.push([foot[0] - u[0] * half, foot[1] - u[1] * half]);
        points.push([foot[0] + u[0] * half, foot[1] + u[1] * half]);
    }
    let eps = tol / r.max(tol);
    points
        .into_iter()
        .filter_map(|p| {
            let t = line_param(a, b, dot(sub(p, a), u) / len, tol)?;
            let v = sub(p, c);
            let s = round_param(round, v[1].atan2(v[0]), eps)?;
            Some(Hit {
                ta: t,
                tb: s,
                point: p,
            })
        })
        .collect()
}

fn round_round(a: &Curve2, b: &Curve2, tol: f64) -> Vec<Hit> {
    let (c1, r1) = round_of(a);
    let (c2, r2) = round_of(b);
    let d = dist(c1, c2);
    if d <= tol {
        // Concentric: equal circles overlap (ends handle arcs), others miss.
        return Vec::new();
    }
    let mut points = Vec::new();
    let u = [(c2[0] - c1[0]) / d, (c2[1] - c1[1]) / d];
    if (d - (r1 + r2)).abs() <= tol || (d - (r1 - r2).abs()).abs() <= tol {
        // Touching: the point on the line of centres.
        let along = if (d - (r1 + r2)).abs() <= tol || r1 >= r2 {
            r1
        } else {
            -r1
        };
        points.push([c1[0] + u[0] * along, c1[1] + u[1] * along]);
    } else if d < r1 + r2 && d > (r1 - r2).abs() {
        let along = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
        let half = (r1 * r1 - along * along).max(0.0).sqrt();
        let m = [c1[0] + u[0] * along, c1[1] + u[1] * along];
        points.push([m[0] - u[1] * half, m[1] + u[0] * half]);
        points.push([m[0] + u[1] * half, m[1] - u[0] * half]);
    }
    points
        .into_iter()
        .filter_map(|p| {
            let va = sub(p, c1);
            let vb = sub(p, c2);
            let ta = round_param(a, va[1].atan2(va[0]), tol / r1.max(tol))?;
            let tb = round_param(b, vb[1].atan2(vb[0]), tol / r2.max(tol))?;
            Some(Hit { ta, tb, point: p })
        })
        .collect()
}

/// Closest points of the segments `a0 a1` and `b0 b1`: parameters along
/// each (0 to 1) and the distance.
fn segment_closest(a0: P2, a1: P2, b0: P2, b1: P2) -> (f64, f64, f64) {
    let d1 = sub(a1, a0);
    let d2 = sub(b1, b0);
    let r = sub(a0, b0);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);
    let (mut s, mut t);
    if a <= 1e-300 && e <= 1e-300 {
        return (0.0, 0.0, dist(a0, b0));
    }
    if a <= 1e-300 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= 1e-300 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denom = a * e - b * b;
            s = if denom > 1e-300 {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    (s, t, dist(lerp(a0, a1, s), lerp(b0, b1, t)))
}

/// A sampled curve: parameters, points and how far each piece may stray
/// from its chord.
struct Polyline {
    params: Vec<f64>,
    points: Vec<P2>,
    bulge: Vec<f64>,
}

fn polyline(curve: &Curve2) -> Polyline {
    let (lo, hi) = curve.domain();
    let params = curve.sample_params(lo, hi);
    let points: Vec<P2> = params.iter().map(|t| curve.point(*t)).collect();
    let bulge = params
        .windows(2)
        .zip(points.windows(2))
        .map(|(t, p)| {
            let mid = curve.point((t[0] + t[1]) / 2.0);
            2.0 * dist(mid, lerp(p[0], p[1], 0.5))
        })
        .collect();
    Polyline {
        params,
        points,
        bulge,
    }
}

/// Refines a candidate `(t, s)` toward `A(t) = B(s)` by damped
/// Gauss-Newton, keeping the parameters in range. Returns the parameters
/// and the remaining distance.
fn refine(a: &Curve2, b: &Curve2, t: f64, s: f64) -> (f64, f64, f64) {
    let (alo, ahi) = a.domain();
    let (blo, bhi) = b.domain();
    let keep = |c: &Curve2, x: f64, lo: f64, hi: f64| {
        if c.is_periodic() {
            x.rem_euclid(TAU)
        } else {
            x.clamp(lo, hi)
        }
    };
    let (mut t, mut s) = (t, s);
    let residual = |t: f64, s: f64| sub(a.point(t), b.point(s));
    let mut f = residual(t, s);
    let mut mu = 1e-3;
    for _ in 0..100 {
        let fa = a.eval(t)[1];
        let fb = b.eval(s)[1];
        // J = [A'(t), -B'(s)]; solve (J^T J + mu diag) step = -J^T f.
        let j11 = dot(fa, fa);
        let j22 = dot(fb, fb);
        let j12 = -dot(fa, fb);
        let g1 = dot(fa, f);
        let g2 = -dot(fb, f);
        let (m11, m22) = (j11 * (1.0 + mu), j22 * (1.0 + mu));
        let det = m11 * m22 - j12 * j12;
        if det.abs() < 1e-300 {
            break;
        }
        let dt = -(m22 * g1 - j12 * g2) / det;
        let ds = -(m11 * g2 - j12 * g1) / det;
        let (nt, ns) = (keep(a, t + dt, alo, ahi), keep(b, s + ds, blo, bhi));
        let nf = residual(nt, ns);
        if norm(nf) < norm(f) {
            let small = (nt - t).abs() <= 1e-15 * (1.0 + t.abs())
                && (ns - s).abs() <= 1e-15 * (1.0 + s.abs());
            t = nt;
            s = ns;
            f = nf;
            mu = (mu * 0.3).max(1e-12);
            if small || norm(f) == 0.0 {
                break;
            }
        } else {
            mu *= 10.0;
            if mu > 1e12 {
                break;
            }
        }
    }
    (t, s, norm(f))
}

fn numeric(a: &Curve2, b: &Curve2, tol: f64) -> Vec<Hit> {
    let pa = polyline(a);
    let pb = polyline(b);
    let mut hits = Vec::new();
    for i in 0..pa.points.len() - 1 {
        let (a0, a1) = (pa.points[i], pa.points[i + 1]);
        for j in 0..pb.points.len() - 1 {
            let (b0, b1) = (pb.points[j], pb.points[j + 1]);
            let reach = pa.bulge[i] + pb.bulge[j] + 10.0 * tol;
            if a0[0].min(a1[0]) > b0[0].max(b1[0]) + reach
                || b0[0].min(b1[0]) > a0[0].max(a1[0]) + reach
                || a0[1].min(a1[1]) > b0[1].max(b1[1]) + reach
                || b0[1].min(b1[1]) > a0[1].max(a1[1]) + reach
            {
                continue;
            }
            let (u, v, d) = segment_closest(a0, a1, b0, b1);
            if d > reach {
                continue;
            }
            let t0 = pa.params[i] + (pa.params[i + 1] - pa.params[i]) * u;
            let s0 = pb.params[j] + (pb.params[j + 1] - pb.params[j]) * v;
            let (t, s, d) = refine(a, b, t0, s0);
            if d <= tol {
                hits.push(Hit {
                    ta: t,
                    tb: s,
                    point: a.point(t),
                });
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sketch::geometry::{Nurbs, clamped_uniform_knots};

    const TOL: f64 = 1e-9;

    fn line(a: P2, b: P2) -> Curve2 {
        Curve2::Line { a, b }
    }

    fn circle(center: P2, radius: f64) -> Curve2 {
        Curve2::Circle { center, radius }
    }

    #[test]
    fn lines_and_circles() {
        let h = intersections(
            &line([0.0, 0.0], [10.0, 0.0]),
            &line([5.0, -5.0], [5.0, 5.0]),
            TOL,
        );
        assert_eq!(h.len(), 1);
        assert!((h[0].ta - 0.5).abs() < 1e-12 && (h[0].tb - 0.5).abs() < 1e-12);
        // A line through a circle, and one touching it.
        let c = circle([0.0, 0.0], 5.0);
        let h = intersections(&line([-10.0, 3.0], [10.0, 3.0]), &c, TOL);
        assert_eq!(h.len(), 2);
        for hit in &h {
            assert!((hit.point[0].abs() - 4.0).abs() < 1e-12);
        }
        let h = intersections(&line([-10.0, 5.0], [10.0, 5.0]), &c, TOL);
        assert_eq!(h.len(), 1);
        assert!(dist(h[0].point, [0.0, 5.0]) < 1e-12);
        // Touching circles, outside and inside.
        let h = intersections(&c, &circle([8.0, 0.0], 3.0), TOL);
        assert_eq!(h.len(), 1);
        assert!(dist(h[0].point, [5.0, 0.0]) < 1e-12);
        let h = intersections(&c, &circle([2.0, 0.0], 3.0), TOL);
        assert_eq!(h.len(), 1);
        assert!(dist(h[0].point, [5.0, 0.0]) < 1e-12);
        // A T-junction and lines that share an end.
        let h = intersections(
            &line([5.0, 0.0], [5.0, 4.0]),
            &line([0.0, 0.0], [10.0, 0.0]),
            TOL,
        );
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].ta, h[0].tb), (0.0, 0.5));
        let h = intersections(
            &line([0.0, 0.0], [10.0, 0.0]),
            &line([10.0, 0.0], [20.0, 0.0]),
            TOL,
        );
        assert_eq!(h.len(), 1);
        // An arc that misses the line.
        let arc = Curve2::Arc {
            center: [0.0, 0.0],
            radius: 5.0,
            start: 0.0,
            end: 1.0,
        };
        assert!(intersections(&line([-10.0, -3.0], [10.0, -3.0]), &arc, TOL).is_empty());
    }

    #[test]
    fn ellipses_and_splines_numerically() {
        let ellipse = Curve2::Ellipse {
            center: [0.0, 0.0],
            major: 6.0,
            minor: 3.0,
            rotation: 0.0,
        };
        let h = intersections(&line([-10.0, 0.0], [10.0, 0.0]), &ellipse, TOL);
        assert_eq!(h.len(), 2, "{h:?}");
        // Tangent to the ellipse at its top.
        let h = intersections(&line([-10.0, 3.0], [10.0, 3.0]), &ellipse, TOL);
        assert_eq!(h.len(), 1, "{h:?}");
        assert!(dist(h[0].point, [0.0, 3.0]) < 1e-6);
        // A wavy spline crossing a line several times.
        let spline = Curve2::Nurbs(Nurbs {
            degree: 3,
            control: (0..8)
                .map(|i| [i as f64 * 2.0, if i % 2 == 0 { -3.0 } else { 3.0 }])
                .collect(),
            weights: Vec::new(),
            knots: clamped_uniform_knots(8, 3),
        });
        let h = intersections(&spline, &line([-1.0, 0.0], [15.0, 0.0]), TOL);
        // The control polygon crosses 7 times; the curve at most as often.
        assert!((5..=7).contains(&h.len()), "{h:?}");
        for hit in h {
            assert!(hit.point[1].abs() < 1e-9);
            assert!(dist(spline.point(hit.ta), hit.point) < 1e-9);
        }
    }
}
