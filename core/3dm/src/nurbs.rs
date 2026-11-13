// SPDX-License-Identifier: MIT
//! B-spline helpers for the conversion: the format keeps knot vectors
//! without their two end knots and lets them be unclamped (periodic
//! curves); OCCT wants them clamped, as distinct knots with
//! multiplicities. A curve made of segments of different degrees (a
//! polycurve of lines and arcs) becomes one B-spline of the highest degree.

use crate::geom::{NurbsCurve, NurbsSurface, P3, Plane};
use mitcad_asm::brep::{BSplineCurve, BSplineSurface};

/// A B-spline with its full knot vector (`poles + degree + 1` knots) and
/// homogeneous control points (`x w, y w, z w, w`).
#[derive(Clone, Debug, PartialEq)]
pub struct Spline {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub cvs: Vec<[f64; 4]>,
}

fn lerp4(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// The full knot vector of the format's (its ends repeated once more).
pub fn full_knots(knots: &[f64]) -> Vec<f64> {
    let mut k = Vec::with_capacity(knots.len() + 2);
    k.push(knots[0]);
    k.extend_from_slice(knots);
    k.push(knots[knots.len() - 1]);
    k
}

/// Inserts `u` once into a B-spline of `degree` over `knots`, for each of
/// the rows of control points (a curve has one row; a surface's rows in
/// the direction of the knots).
fn insert_rows(degree: usize, knots: &mut Vec<f64>, rows: &mut [Vec<[f64; 4]>], u: f64) {
    let p = degree;
    let n = rows[0].len();
    let s = knots.iter().filter(|&&x| x == u).count();
    if s >= p {
        return;
    }
    // The span: the last k with knots[k] <= u (so knots[k] <= u <
    // knots[k + 1]; at a knot, its last copy), at least p.
    let mut k = p;
    for (i, &x) in knots.iter().enumerate().take(knots.len() - 1) {
        if x <= u {
            k = i.max(p);
        }
    }
    for row in rows.iter_mut() {
        let mut q = Vec::with_capacity(n + 1);
        for i in 0..=n {
            if i + p <= k {
                q.push(row[i]);
            } else if i > k - s {
                q.push(row[i - 1]);
            } else {
                let denom = knots[i + p] - knots[i];
                let a = if denom.abs() > 0.0 {
                    (u - knots[i]) / denom
                } else {
                    0.0
                };
                q.push(lerp4(row[i - 1], row[i], a));
            }
        }
        *row = q;
    }
    knots.insert(k + 1, u);
}

/// Clamps the rows' B-spline at the ends of its domain (`knots[p]` and
/// `knots[n]`): end knots of multiplicity `degree + 1`, end poles on the
/// curve.
fn clamp_rows(degree: usize, knots: &mut Vec<f64>, rows: &mut [Vec<[f64; 4]>]) {
    let p = degree;
    if p == 0 || rows.is_empty() {
        return;
    }
    let a = knots[p];
    let b = knots[rows[0].len()];
    for _ in 0..p {
        insert_rows(p, knots, rows, a);
    }
    for _ in 0..p {
        insert_rows(p, knots, rows, b);
    }
    // With a of multiplicity p or more, its last copy at k_a, the curve at
    // a is pole k_a - p; with b's first copy at f_b, the curve at b is pole
    // f_b - 1. The poles between them, with one more copy of each end
    // knot, are the clamped curve.
    let (Some(k_a), Some(f_b)) = (
        knots.iter().rposition(|&x| x == a),
        knots.iter().position(|&x| x == b),
    ) else {
        return;
    };
    if k_a < p || f_b == 0 || f_b - 1 < k_a - p {
        return;
    }
    let (start_pole, end_pole) = (k_a - p, f_b - 1);
    for row in rows.iter_mut() {
        *row = row[start_pole..=end_pole].to_vec();
    }
    let mut k = vec![a];
    k.extend_from_slice(&knots[start_pole + 1..=end_pole + p]);
    k.push(b);
    *knots = k;
}

impl Spline {
    /// A NURBS curve of the file, clamped (3D; a 2D curve's z is 0).
    pub fn from_nurbs(c: &NurbsCurve) -> Spline {
        let mut knots = full_knots(&c.knots);
        let mut rows = vec![c.cvs.clone()];
        clamp_rows(c.order - 1, &mut knots, &mut rows);
        Spline {
            degree: c.order - 1,
            knots,
            cvs: rows.pop().unwrap_or_default(),
        }
    }

    /// A line from `a` to `b` over `[t0, t1]`.
    pub fn line(a: P3, b: P3, t0: f64, t1: f64) -> Spline {
        Spline {
            degree: 1,
            knots: vec![t0, t0, t1, t1],
            cvs: vec![[a[0], a[1], a[2], 1.0], [b[0], b[1], b[2], 1.0]],
        }
    }

    /// A polyline through the points at the parameters.
    pub fn polyline(points: &[P3], params: &[f64]) -> Spline {
        let mut knots = vec![params[0]];
        knots.extend_from_slice(params);
        knots.push(params[params.len() - 1]);
        Spline {
            degree: 1,
            knots,
            cvs: points.iter().map(|p| [p[0], p[1], p[2], 1.0]).collect(),
        }
    }

    /// A circular arc as a rational quadratic B-spline over `domain`, in
    /// spans of at most a quarter turn. Its parameter is not the arc's
    /// angle; the shape is exact.
    pub fn arc(plane: &Plane, radius: f64, angles: [f64; 2], domain: [f64; 2]) -> Spline {
        let sweep = angles[1] - angles[0];
        let spans = ((sweep.abs() / std::f64::consts::FRAC_PI_2) - 1e-9)
            .ceil()
            .max(1.0) as usize;
        let d = sweep / spans as f64;
        let w = (d / 2.0).cos();
        let at = |angle: f64, r: f64| -> P3 {
            let (s, c) = angle.sin_cos();
            [
                plane.origin[0] + r * (c * plane.x[0] + s * plane.y[0]),
                plane.origin[1] + r * (c * plane.x[1] + s * plane.y[1]),
                plane.origin[2] + r * (c * plane.x[2] + s * plane.y[2]),
            ]
        };
        let mut cvs = Vec::with_capacity(2 * spans + 1);
        let mut knots = vec![domain[0]; 3];
        for i in 0..spans {
            let a0 = angles[0] + d * i as f64;
            let p = at(a0, radius);
            if i == 0 {
                cvs.push([p[0], p[1], p[2], 1.0]);
            }
            let m = at(a0 + d / 2.0, radius / w);
            cvs.push([m[0] * w, m[1] * w, m[2] * w, w]);
            let e = at(a0 + d, radius);
            cvs.push([e[0], e[1], e[2], 1.0]);
            if i + 1 < spans {
                let t = domain[0] + (domain[1] - domain[0]) * (i + 1) as f64 / spans as f64;
                knots.push(t);
                knots.push(t);
            }
        }
        knots.extend([domain[1]; 3]);
        Spline {
            degree: 2,
            knots,
            cvs,
        }
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.knots[self.degree], self.knots[self.cvs.len()])
    }

    /// The same curve over `[a, b]`.
    pub fn reparametrized(mut self, a: f64, b: f64) -> Spline {
        let (t0, t1) = self.domain();
        if t1 > t0 && (t0 != a || t1 != b) {
            for k in &mut self.knots {
                *k = a + (*k - t0) / (t1 - t0) * (b - a);
            }
        }
        self
    }

    /// The point at `t`.
    pub fn eval(&self, t: f64) -> P3 {
        let p = self.degree;
        let n = self.cvs.len();
        let mut k = p;
        while k + 1 < n && self.knots[k + 1] <= t {
            k += 1;
        }
        let mut d: Vec<[f64; 4]> = (0..=p).map(|j| self.cvs[k - p + j]).collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = k - p + j;
                let denom = self.knots[i + p + 1 - r] - self.knots[i];
                let a = if denom > 0.0 {
                    (t - self.knots[i]) / denom
                } else {
                    0.0
                };
                d[j] = lerp4(d[j - 1], d[j], a);
            }
        }
        let h = d[p];
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    }

    /// The same curve of a degree higher by `by`: split into Bézier
    /// segments, each raised, joined with C0 knots.
    pub fn elevated(&self, by: usize) -> Spline {
        if by == 0 {
            return self.clone();
        }
        let p = self.degree;
        let mut knots = self.knots.clone();
        let mut rows = vec![self.cvs.clone()];
        let (a, b) = self.domain();
        let interior: Vec<f64> = {
            let mut v: Vec<f64> = knots[p + 1..rows[0].len()].to_vec();
            v.dedup();
            v.retain(|&x| x > a && x < b);
            v
        };
        for &u in &interior {
            for _ in 0..p {
                insert_rows(p, &mut knots, &mut rows, u);
            }
        }
        let cvs = &rows[0];
        let mut breaks = vec![a];
        breaks.extend_from_slice(&interior);
        breaks.push(b);
        let q = p + by;
        let mut out_cvs: Vec<[f64; 4]> = Vec::new();
        let mut out_knots = vec![a; q + 1];
        // After the insertions segment s has the poles s p ..= s p + p.
        for s in 0..breaks.len() - 1 {
            let mut seg: Vec<[f64; 4]> = cvs[s * p..=s * p + p].to_vec();
            for e in 0..by {
                let deg = p + e;
                let mut up = Vec::with_capacity(deg + 2);
                up.push(seg[0]);
                for i in 1..=deg {
                    let t = i as f64 / (deg + 1) as f64;
                    // Q_i = t P_{i-1} + (1 - t) P_i.
                    up.push(lerp4(seg[i], seg[i - 1], t));
                }
                up.push(seg[deg]);
                seg = up;
            }
            if s == 0 {
                out_cvs.extend_from_slice(&seg);
            } else {
                out_cvs.extend_from_slice(&seg[1..]);
                out_knots.extend(std::iter::repeat_n(breaks[s], q));
            }
        }
        out_knots.extend(std::iter::repeat_n(b, q + 1));
        Spline {
            degree: q,
            knots: out_knots,
            cvs: out_cvs,
        }
    }

    /// Curves joined end to end (each after the previous one in
    /// parameter), as one B-spline of their highest degree.
    pub fn join(parts: Vec<Spline>) -> Option<Spline> {
        let degree = parts.iter().map(|s| s.degree).max()?;
        let mut out: Option<Spline> = None;
        for part in parts {
            let mut part = part.elevated(degree - part.degree);
            let Some(acc) = out.as_mut() else {
                out = Some(part);
                continue;
            };
            // Scale the part's weights so that its first matches the end
            // of the curve so far (the shape stays).
            let last = *acc.cvs.last()?;
            let first = part.cvs[0];
            if first[3] != 0.0 && last[3] != 0.0 {
                let f = last[3] / first[3];
                for cv in &mut part.cvs {
                    *cv = [cv[0] * f, cv[1] * f, cv[2] * f, cv[3] * f];
                }
            }
            // The joint: the mean of the two ends.
            let n = acc.cvs.len();
            acc.cvs[n - 1] = lerp4(last, part.cvs[0], 0.5);
            let (_, end) = acc.domain();
            let (p0, p1) = part.domain();
            let part = part.reparametrized(end, end + (p1 - p0));
            acc.knots.truncate(acc.knots.len() - 1);
            acc.knots.extend_from_slice(&part.knots[degree + 1..]);
            acc.cvs.extend_from_slice(&part.cvs[1..]);
        }
        out
    }

    /// The neutral B-spline, coordinates times `scale`.
    pub fn to_brep(&self, scale: f64) -> BSplineCurve {
        let (knots, mults) = distinct(&self.knots);
        let rational = self.cvs.iter().any(|c| (c[3] - 1.0).abs() > 1e-12);
        BSplineCurve {
            degree: self.degree,
            knots,
            mults,
            poles: self
                .cvs
                .iter()
                .map(|c| {
                    [
                        c[0] / c[3] * scale,
                        c[1] / c[3] * scale,
                        c[2] / c[3] * scale,
                    ]
                })
                .collect(),
            weights: rational.then(|| self.cvs.iter().map(|c| c[3]).collect()),
            periodic: false,
        }
    }
}

/// Distinct knots and their multiplicities.
pub fn distinct(full: &[f64]) -> (Vec<f64>, Vec<usize>) {
    let mut knots: Vec<f64> = Vec::new();
    let mut mults: Vec<usize> = Vec::new();
    for &k in full {
        match knots.last() {
            Some(&l) if l == k => *mults.last_mut().expect("a multiplicity per knot") += 1,
            _ => {
                knots.push(k);
                mults.push(1);
            }
        }
    }
    (knots, mults)
}

/// A NURBS surface, clamped both ways, as the neutral one (times `scale`).
pub fn surface_to_brep(s: &NurbsSurface, scale: f64) -> BSplineSurface {
    let [n0, n1] = s.count;
    let p = [s.order[0] - 1, s.order[1] - 1];
    // Rows along v (one per u index), clamp in v; then along u.
    let mut kv = full_knots(&s.knots[1]);
    let mut rows: Vec<Vec<[f64; 4]>> = (0..n0)
        .map(|i| s.cvs[i * n1..(i + 1) * n1].to_vec())
        .collect();
    clamp_rows(p[1], &mut kv, &mut rows);
    let nv = rows[0].len();
    let mut ku = full_knots(&s.knots[0]);
    let mut cols: Vec<Vec<[f64; 4]>> = (0..nv)
        .map(|j| rows.iter().map(|r| r[j]).collect())
        .collect();
    clamp_rows(p[0], &mut ku, &mut cols);
    let nu = cols[0].len();
    let mut cvs = Vec::with_capacity(nu * nv);
    for i in 0..nu {
        for col in &cols {
            cvs.push(col[i]);
        }
    }
    let rational = cvs.iter().any(|c| (c[3] - 1.0).abs() > 1e-12);
    let (u_knots, u_mults) = distinct(&ku);
    let (v_knots, v_mults) = distinct(&kv);
    BSplineSurface {
        u_degree: p[0],
        v_degree: p[1],
        u_knots,
        u_mults,
        v_knots,
        v_mults,
        nu,
        nv,
        poles: cvs
            .iter()
            .map(|c| {
                [
                    c[0] / c[3] * scale,
                    c[1] / c[3] * scale,
                    c[2] / c[3] * scale,
                ]
            })
            .collect(),
        weights: rational.then(|| cvs.iter().map(|c| c[3]).collect()),
        u_periodic: false,
        v_periodic: false,
        u_closed: false,
        v_closed: false,
    }
}

/// The surface `a(s) + b(t) + base` as a B-spline (exact: the sum of two
/// rational curves is the tensor product with the products of their
/// weights).
pub fn sum_surface(a: &Spline, b: &Spline, base: P3, scale: f64) -> BSplineSurface {
    let mut cvs = Vec::with_capacity(a.cvs.len() * b.cvs.len());
    for ca in &a.cvs {
        for cb in &b.cvs {
            let (wa, wb) = (ca[3], cb[3]);
            let w = wa * wb;
            let p = [
                ca[0] / wa + cb[0] / wb + base[0],
                ca[1] / wa + cb[1] / wb + base[1],
                ca[2] / wa + cb[2] / wb + base[2],
            ];
            cvs.push([p[0] * w, p[1] * w, p[2] * w, w]);
        }
    }
    let rational = cvs.iter().any(|c| (c[3] - 1.0).abs() > 1e-12);
    let (u_knots, u_mults) = distinct(&a.knots);
    let (v_knots, v_mults) = distinct(&b.knots);
    BSplineSurface {
        u_degree: a.degree,
        v_degree: b.degree,
        u_knots,
        u_mults,
        v_knots,
        v_mults,
        nu: a.cvs.len(),
        nv: b.cvs.len(),
        poles: cvs
            .iter()
            .map(|c| {
                [
                    c[0] / c[3] * scale,
                    c[1] / c[3] * scale,
                    c[2] / c[3] * scale,
                ]
            })
            .collect(),
        weights: rational.then(|| cvs.iter().map(|c| c[3]).collect()),
        u_periodic: false,
        v_periodic: false,
        u_closed: false,
        v_closed: false,
    }
}

/// The surface of `profile` turned about the axis through `origin` along
/// the unit vector `axis`, over `angles`, as a B-spline (exact: the tensor
/// product of the profile with a rational circle, the weights
/// multiplied). Its parameters run as OCCT's surface of revolution's: u
/// the angle (counterclockwise about the axis), v the profile's
/// parameter; so its normal is the same. Used for a face that covers its
/// whole surface of revolution (a sphere whose loop is only its seam and
/// its poles), which the builder then takes with its natural bounds.
pub fn revolution_surface(
    profile: &Spline,
    origin: P3,
    axis: P3,
    angles: [f64; 2],
    scale: f64,
) -> BSplineSurface {
    let unit = Plane {
        origin: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };
    let circle = Spline::arc(&unit, 1.0, angles, angles);
    let mut cvs = Vec::with_capacity(circle.cvs.len() * profile.cvs.len());
    for c in &circle.cvs {
        let (cx, cy, wc) = (c[0] / c[3], c[1] / c[3], c[3]);
        for p in &profile.cvs {
            let wp = p[3];
            let point = [p[0] / wp, p[1] / wp, p[2] / wp];
            let rel = [
                point[0] - origin[0],
                point[1] - origin[1],
                point[2] - origin[2],
            ];
            let along = rel[0] * axis[0] + rel[1] * axis[1] + rel[2] * axis[2];
            let center = [
                origin[0] + axis[0] * along,
                origin[1] + axis[1] * along,
                origin[2] + axis[2] * along,
            ];
            let r = [
                point[0] - center[0],
                point[1] - center[1],
                point[2] - center[2],
            ];
            // The direction the angle turns r: axis × r.
            let t = [
                axis[1] * r[2] - axis[2] * r[1],
                axis[2] * r[0] - axis[0] * r[2],
                axis[0] * r[1] - axis[1] * r[0],
            ];
            let w = wp * wc;
            let q = [
                center[0] + r[0] * cx + t[0] * cy,
                center[1] + r[1] * cx + t[1] * cy,
                center[2] + r[2] * cx + t[2] * cy,
            ];
            cvs.push([q[0] * w, q[1] * w, q[2] * w, w]);
        }
    }
    let rational = cvs.iter().any(|c| (c[3] - 1.0).abs() > 1e-12);
    let (u_knots, u_mults) = distinct(&circle.knots);
    let (v_knots, v_mults) = distinct(&profile.knots);
    BSplineSurface {
        u_degree: circle.degree,
        v_degree: profile.degree,
        u_knots,
        u_mults,
        v_knots,
        v_mults,
        nu: circle.cvs.len(),
        nv: profile.cvs.len(),
        poles: cvs
            .iter()
            .map(|c| {
                [
                    c[0] / c[3] * scale,
                    c[1] / c[3] * scale,
                    c[2] / c[3] * scale,
                ]
            })
            .collect(),
        weights: rational.then(|| cvs.iter().map(|c| c[3]).collect()),
        u_periodic: false,
        v_periodic: false,
        u_closed: false,
        v_closed: false,
    }
}
