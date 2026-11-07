// SPDX-License-Identifier: MIT
//! Neutral B-rep model (topology and geometry, millimetres), independent of
//! the ASM format and of OCCT, with consistency checks.
//!
//! Conventions:
//! - Every edge runs along increasing parameter of its curve, from
//!   `vertices[0]` at `t[0]` to `vertices[1]` at `t[1]` (`t[0] < t[1]`).
//! - A coedge uses its edge forward (`forward`) or reversed.
//! - Loops are oriented so that the face lies on the left of the coedges
//!   when viewed against the face's material normal. The material normal is
//!   the surface's natural normal (see [`Surface`]), negated when
//!   `Face::reversed` is set.

use mitcad_vec3::unit_above;

pub use mitcad_vec3::Vec3 as P3;

// The vector helpers (shared with the model and the other readers); the
// names are this module's public ones.
pub use mitcad_vec3::{add, cross, distance as dist, dot, norm, scale, sub};

/// The unit vector along `a`; `a` itself when it has no length.
pub fn normalize(a: P3) -> P3 {
    unit_above(a, 0.0).unwrap_or(a)
}

/// Non-rational or rational B-spline curve in OCCT form: distinct knots with
/// multiplicities, `sum(mults) == poles.len() + degree + 1`.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineCurve {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub poles: Vec<P3>,
    pub weights: Option<Vec<f64>>,
    /// Closed curve whose parameter wraps around (edges may cross the
    /// seam); the poles are still those of a clamped curve.
    pub periodic: bool,
}

/// B-spline curve in a surface's parameter space.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineCurve2d {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub poles: Vec<[f64; 2]>,
    pub weights: Option<Vec<f64>>,
}

/// Tensor-product B-spline surface; `poles[iu * nv + iv]`.
#[derive(Clone, Debug, PartialEq)]
pub struct BSplineSurface {
    pub u_degree: usize,
    pub v_degree: usize,
    pub u_knots: Vec<f64>,
    pub u_mults: Vec<usize>,
    pub v_knots: Vec<f64>,
    pub v_mults: Vec<usize>,
    pub nu: usize,
    pub nv: usize,
    pub poles: Vec<P3>,
    pub weights: Option<Vec<f64>>,
    /// Periodic in u or v (stored clamped and closed): faces may cross
    /// the seam, as edges may on periodic curves.
    pub u_periodic: bool,
    pub v_periodic: bool,
    /// Closed in u or v without being periodic (the ends meet, the
    /// derivatives need not): faces may cross the seam too, the files'
    /// parameter curves run past it (mitcad#499). Built so the parameter
    /// wraps, as on a periodic surface, where the geometry is closed.
    pub u_closed: bool,
    pub v_closed: bool,
}

fn full_knots(knots: &[f64], mults: &[usize]) -> Vec<f64> {
    let mut out = Vec::new();
    for (k, m) in knots.iter().zip(mults) {
        for _ in 0..*m {
            out.push(*k);
        }
    }
    out
}

/// De Boor evaluation of homogeneous points `cw` (x, y, z, w).
fn de_boor(degree: usize, flat: &[f64], cw: &[[f64; 4]], t: f64) -> [f64; 4] {
    let n = cw.len();
    if n == 0 {
        return [0.0; 4];
    }
    let p = degree.min(n - 1);
    // Span index k: flat[k] <= t < flat[k+1], limited to valid spans.
    let lo = p;
    let hi = n - 1;
    let mut k = lo;
    while k < hi && flat[k + 1] <= t {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=p).map(|j| cw[k + j - p]).collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k + j - p;
            let denom = flat[i + p + 1 - r] - flat[i];
            let a = if denom.abs() < 1e-300 {
                0.0
            } else {
                (t - flat[i]) / denom
            };
            let prev = d[j - 1];
            for (x, p) in d[j].iter_mut().zip(prev) {
                *x = (1.0 - a) * p + a * *x;
            }
        }
    }
    d[p]
}

impl BSplineCurve {
    pub fn is_valid(&self) -> bool {
        self.knots.len() == self.mults.len()
            && self.knots.len() >= 2
            && self.mults.iter().sum::<usize>() == self.poles.len() + self.degree + 1
            && self.knots.windows(2).all(|w| w[0] < w[1])
            && self
                .weights
                .as_ref()
                .is_none_or(|w| w.len() == self.poles.len())
    }

    /// Parameter range where the curve is defined.
    pub fn range(&self) -> (f64, f64) {
        let flat = full_knots(&self.knots, &self.mults);
        (flat[self.degree], flat[flat.len() - 1 - self.degree])
    }

    pub fn eval(&self, t: f64) -> P3 {
        let flat = full_knots(&self.knots, &self.mults);
        let (a, b) = (flat[self.degree], flat[flat.len() - 1 - self.degree]);
        let t = if self.periodic && b > a && !(a..=b).contains(&t) {
            a + (t - a).rem_euclid(b - a)
        } else {
            t
        };
        let cw: Vec<[f64; 4]> = self
            .poles
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let w = self.weights.as_ref().map_or(1.0, |w| w[i]);
                [p[0] * w, p[1] * w, p[2] * w, w]
            })
            .collect();
        let h = de_boor(self.degree, &flat, &cw, t);
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    }
}

impl BSplineCurve2d {
    pub fn is_valid(&self) -> bool {
        self.knots.len() == self.mults.len()
            && self.knots.len() >= 2
            && self.mults.iter().sum::<usize>() == self.poles.len() + self.degree + 1
            && self.knots.windows(2).all(|w| w[0] < w[1])
            && self
                .weights
                .as_ref()
                .is_none_or(|w| w.len() == self.poles.len())
    }

    /// Parameter range where the curve is defined.
    pub fn range(&self) -> (f64, f64) {
        let flat = full_knots(&self.knots, &self.mults);
        (flat[self.degree], flat[flat.len() - 1 - self.degree])
    }
}

impl BSplineSurface {
    pub fn is_valid(&self) -> bool {
        self.u_knots.len() == self.u_mults.len()
            && self.v_knots.len() == self.v_mults.len()
            && self.u_knots.len() >= 2
            && self.v_knots.len() >= 2
            && self.u_mults.iter().sum::<usize>() == self.nu + self.u_degree + 1
            && self.v_mults.iter().sum::<usize>() == self.nv + self.v_degree + 1
            && self.poles.len() == self.nu * self.nv
            && self.u_knots.windows(2).all(|w| w[0] < w[1])
            && self.v_knots.windows(2).all(|w| w[0] < w[1])
            && self
                .weights
                .as_ref()
                .is_none_or(|w| w.len() == self.poles.len())
    }

    pub fn u_range(&self) -> (f64, f64) {
        let f = full_knots(&self.u_knots, &self.u_mults);
        (f[self.u_degree], f[f.len() - 1 - self.u_degree])
    }

    pub fn v_range(&self) -> (f64, f64) {
        let f = full_knots(&self.v_knots, &self.v_mults);
        (f[self.v_degree], f[f.len() - 1 - self.v_degree])
    }

    pub fn eval(&self, u: f64, v: f64) -> P3 {
        let fu = full_knots(&self.u_knots, &self.u_mults);
        let fv = full_knots(&self.v_knots, &self.v_mults);
        let mut col = Vec::with_capacity(self.nu);
        for iu in 0..self.nu {
            let row: Vec<[f64; 4]> = (0..self.nv)
                .map(|iv| {
                    let i = iu * self.nv + iv;
                    let p = self.poles[i];
                    let w = self.weights.as_ref().map_or(1.0, |w| w[i]);
                    [p[0] * w, p[1] * w, p[2] * w, w]
                })
                .collect();
            col.push(de_boor(self.v_degree, &fv, &row, v));
        }
        let h = de_boor(self.u_degree, &fu, &col, u);
        [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
    }

    /// Normal du x dv by central differences.
    pub fn normal(&self, u: f64, v: f64) -> P3 {
        let (u0, u1) = self.u_range();
        let (v0, v1) = self.v_range();
        let hu = (u1 - u0) * 1e-5;
        let hv = (v1 - v0) * 1e-5;
        let du = sub(
            self.eval((u + hu).min(u1), v),
            self.eval((u - hu).max(u0), v),
        );
        let dv = sub(
            self.eval(u, (v + hv).min(v1)),
            self.eval(u, (v - hv).max(v0)),
        );
        normalize(cross(du, dv))
    }
}

/// A helix (conical where it grows), exactly: at angle `t` the point
/// `center + (scale + growth t / 2 pi) (major cos t + minor sin t) + pitch t
/// / 2 pi` over `range` (mitcad_private#198: before, it was interpolated
/// through 48 samples a turn). `scale` is 1 for a helix as the file stores
/// it; the helices that bound a thread's surfaces are scaled towards or
/// from the axis. The builder gets it as the B-spline of [`Helix::bspline`]
/// with `spans` knot spans, within a few nanometres of it; the helices of
/// one surface share their range and spans, so their B-splines share their
/// knots.
#[derive(Clone, Debug, PartialEq)]
pub struct Helix {
    pub center: P3,
    pub major: P3,
    pub minor: P3,
    /// Advance per turn.
    pub pitch: P3,
    pub scale: f64,
    /// Radial growth per turn, as a fraction of `major` and `minor`.
    pub growth: f64,
    /// The angles it spans (increasing).
    pub range: [f64; 2],
    /// The knot spans of its B-spline ([`helix_spans`]).
    pub spans: usize,
}

/// Knot spans a turn of a helix's B-spline: degree 7 Hermite pieces of a
/// twelfth of a turn are within 6e-10 of the radius of the helix (the
/// remainder of the interpolation, `|f^(8)| h^8 / (8! 2^8)`, with `h` the
/// piece's angle), 1e-7 mm on a helix of 100 mm radius.
pub const HELIX_SPANS_PER_TURN: f64 = 12.0;

/// The fewest spans a turn of a long helix's B-spline: an eighth of a turn
/// is within 1.4e-8 of the radius.
pub const MIN_HELIX_SPANS_PER_TURN: f64 = 8.0;

/// The knot spans of the B-spline of a helix of `turns` turns: twelve a
/// turn, within `max` while eight a turn are; `None` when even those are
/// more than `max`.
pub fn helix_spans(turns: f64, max: usize) -> Option<usize> {
    let turns = turns.abs();
    let n = ((turns * HELIX_SPANS_PER_TURN).ceil() as usize).max(1);
    if n <= max {
        Some(n)
    } else if turns * MIN_HELIX_SPANS_PER_TURN <= max as f64 {
        Some(max)
    } else {
        None
    }
}

/// The degree of a helix's B-spline.
const HELIX_DEGREE: usize = 7;

impl Helix {
    /// The point and its first three derivatives at `t`.
    fn jet(&self, t: f64) -> [P3; 4] {
        let tau = std::f64::consts::TAU;
        let (s, c) = t.sin_cos();
        let r = add(scale(self.major, c), scale(self.minor, s));
        let dr = add(scale(self.major, -s), scale(self.minor, c));
        let g = self.scale + self.growth * t / tau;
        let dg = self.growth / tau;
        // r'' = -r, r''' = -r'; g'' = 0.
        [
            add(add(self.center, scale(r, g)), scale(self.pitch, t / tau)),
            add(
                add(scale(r, dg), scale(dr, g)),
                scale(self.pitch, 1.0 / tau),
            ),
            sub(scale(dr, 2.0 * dg), scale(r, g)),
            sub(scale(r, -3.0 * dg), scale(dr, g)),
        ]
    }

    pub fn eval(&self, t: f64) -> P3 {
        self.jet(t)[0]
    }

    /// The derivative at `t`.
    pub fn derivative(&self, t: f64) -> P3 {
        self.jet(t)[1]
    }

    /// The turns it spans.
    pub fn turns(&self) -> f64 {
        (self.range[1] - self.range[0]).abs() / std::f64::consts::TAU
    }

    /// The length from `a` to `b` (Gauss-Legendre, five points in each
    /// 24th of a turn).
    pub fn length(&self, a: f64, b: f64) -> f64 {
        const X: [f64; 5] = [
            0.0,
            -0.538_469_310_105_683,
            0.538_469_310_105_683,
            -0.906_179_845_938_664,
            0.906_179_845_938_664,
        ];
        const W: [f64; 5] = [
            0.568_888_888_888_889,
            0.478_628_670_499_366,
            0.478_628_670_499_366,
            0.236_926_885_056_189,
            0.236_926_885_056_189,
        ];
        let (lo, hi) = (a.min(b), a.max(b));
        let pieces =
            (((hi - lo) / std::f64::consts::TAU * 24.0).ceil() as usize).clamp(1, 1_000_000);
        let h = (hi - lo) / pieces as f64;
        let mut sum = 0.0;
        for k in 0..pieces {
            let mid = lo + h * (k as f64 + 0.5);
            for (x, w) in X.iter().zip(W) {
                sum += w * norm(self.derivative(mid + 0.5 * h * x));
            }
        }
        0.5 * h * sum
    }

    /// The B-spline of degree 7 through it in `spans` pieces of equal
    /// angle, each matching the helix's point and first three derivatives
    /// at its ends (Hermite), joined C3: interior knots of multiplicity 4.
    /// Each pole is the blossom of a piece at its seven knots (the pieces
    /// meeting at a knot agree there to the third derivative, so any piece
    /// under the pole's knots gives it).
    pub fn bspline(&self, spans: usize) -> BSplineCurve {
        let n = spans.max(1);
        let (a, b) = (self.range[0], self.range[1]);
        let mut tau: Vec<f64> = (0..=n).map(|j| a + (b - a) * j as f64 / n as f64).collect();
        // (Its ends exactly the helix's.)
        tau[n] = b;
        // The Bezier points of each piece from the derivatives at its ends:
        // n(n-1)..(n-k+1) times the k-th forward difference is the k-th
        // derivative (in the piece's parameter, `h^k` times the helix's).
        let d = HELIX_DEGREE as f64;
        let (d1, d2, d3) = (d, d * (d - 1.0), d * (d - 1.0) * (d - 2.0));
        let pieces: Vec<[P3; HELIX_DEGREE + 1]> = tau
            .windows(2)
            .map(|w| {
                let h = w[1] - w[0];
                let s = self.jet(w[0]);
                let e = self.jet(w[1]);
                let mut p = [[0.0; 3]; HELIX_DEGREE + 1];
                p[0] = s[0];
                p[1] = add(p[0], scale(s[1], h / d1));
                p[2] = add(sub(scale(p[1], 2.0), p[0]), scale(s[2], h * h / d2));
                p[3] = add(
                    add(sub(scale(p[2], 3.0), scale(p[1], 3.0)), p[0]),
                    scale(s[3], h * h * h / d3),
                );
                p[7] = e[0];
                p[6] = sub(p[7], scale(e[1], h / d1));
                p[5] = add(sub(scale(p[6], 2.0), p[7]), scale(e[2], h * h / d2));
                p[4] = sub(
                    add(sub(scale(p[5], 3.0), scale(p[6], 3.0)), p[7]),
                    scale(e[3], h * h * h / d3),
                );
                p
            })
            .collect();
        let mut mults = vec![4; n + 1];
        mults[0] = HELIX_DEGREE + 1;
        mults[n] = HELIX_DEGREE + 1;
        let flat = full_knots(&tau, &mults);
        let count = flat.len() - HELIX_DEGREE - 1;
        let poles = (0..count)
            .map(|i| {
                // The piece whose span (flat knots 4j + 7, 4j + 8) lies
                // under the pole's knots i + 1 ..= i + 7.
                let j = i.saturating_sub(HELIX_DEGREE).div_ceil(4).min(n - 1);
                blossom(
                    &pieces[j],
                    tau[j],
                    tau[j + 1],
                    &flat[i + 1..=i + HELIX_DEGREE],
                )
            })
            .collect();
        BSplineCurve {
            degree: HELIX_DEGREE,
            knots: tau,
            mults,
            poles,
            weights: None,
            periodic: false,
        }
    }
}

/// The blossom of the Bezier curve `points` over `[a, b]` at `args` (one
/// argument per degree): de Casteljau's steps, each at its own argument.
fn blossom(points: &[P3], a: f64, b: f64, args: &[f64]) -> P3 {
    let mut p = points.to_vec();
    for &u in args {
        let s = (u - a) / (b - a);
        p = p
            .windows(2)
            .map(|w| add(scale(w[0], 1.0 - s), scale(w[1], s)))
            .collect();
    }
    p[0]
}

/// 3D curve. Parameterisation as in OCCT: lines by arc length, ellipses by
/// angle from the major axis towards `normal x major`.
#[derive(Clone, Debug, PartialEq)]
pub enum Curve {
    Line {
        origin: P3,
        dir: P3,
    },
    /// Circle when `minor == major`.
    Ellipse {
        center: P3,
        normal: P3,
        major_dir: P3,
        major: f64,
        minor: f64,
    },
    BSpline(BSplineCurve),
    /// Curve known by points at parameters; the builder interpolates it
    /// (used for curves on surfaces stored without their approximation).
    Interpolated {
        params: Vec<f64>,
        points: Vec<P3>,
    },
    /// A surface along a curve in its parameter space, at the parameter
    /// curve's parameter: for curves on surfaces only the builder makes (a
    /// rolling-ball blend's contact tracks, mitcad#621). Not evaluated
    /// here: [`Curve::eval`] gives NaN.
    OnSurface {
        surface: Box<Surface>,
        pcurve: BSplineCurve2d,
    },
    /// A helix, exactly ([`Helix`]).
    Helix(Helix),
    /// The curve of an edge the reader could not decode (why): the edge
    /// is kept so that its loops stay closed, the builder leaves it out
    /// (mitcad_private#198). Not evaluated: NaN.
    Unknown(String),
}

impl Curve {
    pub fn eval(&self, t: f64) -> P3 {
        match self {
            Curve::Line { origin, dir } => add(*origin, scale(*dir, t)),
            Curve::Ellipse {
                center,
                normal,
                major_dir,
                major,
                minor,
            } => {
                let y = cross(*normal, *major_dir);
                add(
                    *center,
                    add(
                        scale(*major_dir, major * t.cos()),
                        scale(y, minor * t.sin()),
                    ),
                )
            }
            Curve::BSpline(b) => b.eval(t),
            Curve::Interpolated { params, points } => {
                // Cubic through the four samples around `t` (Lagrange):
                // piecewise linear between a helix's samples was 0.04 mm
                // off a coil's edge end of 33 mm radius, which the checks
                // took for a vertex off its edge (mitcad#235).
                let n = params.len();
                let i = params.partition_point(|&p| p <= t).clamp(1, n - 1);
                if n < 4 {
                    let (a, b) = (params[i - 1], params[i]);
                    let s = if b > a { (t - a) / (b - a) } else { 0.0 };
                    return add(points[i - 1], scale(sub(points[i], points[i - 1]), s));
                }
                let first = i.saturating_sub(2).min(n - 4);
                let near = &params[first..first + 4];
                let mut p = [0.0; 3];
                for (j, (&pj, &point)) in near.iter().zip(&points[first..first + 4]).enumerate() {
                    let mut w = 1.0;
                    for (m, &pm) in near.iter().enumerate() {
                        if m != j && pj != pm {
                            w *= (t - pm) / (pj - pm);
                        }
                    }
                    p = add(p, scale(point, w));
                }
                p
            }
            Curve::OnSurface { .. } | Curve::Unknown(_) => [f64::NAN; 3],
            Curve::Helix(h) => h.eval(t),
        }
    }

    /// Parameter range of bounded curves (B-splines, interpolated curves,
    /// helices).
    pub fn range(&self) -> Option<(f64, f64)> {
        match self {
            Curve::BSpline(b) => Some(b.range()),
            Curve::Interpolated { params, .. } => Some((*params.first()?, *params.last()?)),
            Curve::OnSurface { pcurve, .. } => Some(pcurve.range()),
            Curve::Helix(h) => Some((h.range[0], h.range[1])),
            _ => None,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Curve::Line { .. } => "line",
            Curve::Ellipse { major, minor, .. } if (major - minor).abs() <= 1e-12 * major.abs() => {
                "circle"
            }
            Curve::Ellipse { .. } => "ellipse",
            Curve::BSpline(_) => "bspline",
            Curve::Interpolated { .. } => "interpolated",
            Curve::OnSurface { .. } => "on surface",
            Curve::Helix(_) => "helix",
            Curve::Unknown(_) => "unknown",
        }
    }
}

/// Surface. The natural normal of each kind:
/// - `Plane`: `normal`.
/// - `Cone`: away from the axis (OCCT `Geom_ConicalSurface` with a direct
///   frame); `half_angle` in (-pi/2, pi/2), radius grows along `axis` when
///   it is positive. Cylinder when `half_angle == 0`.
/// - `Sphere`, `Torus`: outward (OCCT convention, direct frame).
/// - `BSpline`: du x dv.
/// - `Extrusion`: dcurve/du x dir.
/// - `Revolution`: OCCT `Geom_SurfaceOfRevolution` (du x dv, u around the
///   axis, v along the curve).
/// - `Ruled`: du x dv of `S(u, v) = (1 - u) from(v) + u to(v)`, `u` in
///   [0, 1].
/// - `ArcSweep`: du x dv, `u` in [0, 1] along the arcs.
/// - `RollingBall`: du x dv, `u` across the section from the first
///   support's contact, `v` along the spine.
/// - `Offset`: the basis's.
/// - `Unknown`: none (the face's geometry is unknown).
#[derive(Clone, Debug, PartialEq)]
pub enum Surface {
    Plane {
        origin: P3,
        normal: P3,
        u_dir: P3,
    },
    Cone {
        origin: P3,
        axis: P3,
        ref_dir: P3,
        radius: f64,
        half_angle: f64,
    },
    Sphere {
        center: P3,
        axis: P3,
        ref_dir: P3,
        radius: f64,
    },
    Torus {
        center: P3,
        axis: P3,
        ref_dir: P3,
        major: f64,
        minor: f64,
    },
    BSpline(BSplineSurface),
    Extrusion {
        curve: Curve,
        dir: P3,
    },
    Revolution {
        curve: Curve,
        origin: P3,
        axis: P3,
    },
    /// Straight lines between corresponding points of two curves with the
    /// same parameter range (used for thread flanks, between two helices
    /// sampled at the same parameters).
    Ruled {
        from: Curve,
        to: Curve,
    },
    /// Circular arcs through corresponding points of curves with the same
    /// parameter range: for each v, the rational quadratic B-spline (in u,
    /// knots spaced evenly in [0, 1], one arc per two spans) whose control
    /// points are the curves' points at v and whose weights are `weights`
    /// (used for the rounded roots of threads, between helices sampled at
    /// the same parameters).
    ArcSweep {
        sections: Vec<Curve>,
        weights: Vec<f64>,
    },
    /// The surface a ball of `radius` sweeps while its centre runs along
    /// `spine` over `range` touching both supports (a rolling-ball blend
    /// stored without its approximation, mitcad#621, or with one too far
    /// off, mitcad#622). The builder makes it with OCCT's blend walking:
    /// `u` in [0, 1] across the section from the first support's contact
    /// to the second's, `v` the spine's parameter. On a closed spine a
    /// `range` of one whole period makes the surface closed in `v`.
    RollingBall {
        supports: Box<[BlendSupport; 2]>,
        spine: Curve,
        range: [f64; 2],
        /// The part of `range` the surface must cover (a face's, a curve's):
        /// the ball may leave a support in the rest.
        needed: [f64; 2],
        radius: f64,
        /// The approximation the file stores, the surface where OCCT
        /// cannot walk the ball (as before mitcad#622).
        stored: Option<Box<Surface>>,
    },
    /// `basis` moved by `distance` along its natural normal (OCCT
    /// `Geom_OffsetSurface`; the basis's parameters): an offset surface
    /// stored without its approximation (mitcad_private#198).
    Offset {
        basis: Box<Surface>,
        distance: f64,
    },
    /// The surface of a face the reader could not decode (why): the face
    /// is kept with its loops so that the body's topology stays closed,
    /// the builder leaves it out (mitcad_private#198).
    Unknown(String),
}

/// What a rolling ball touches on one side.
#[derive(Clone, Debug, PartialEq)]
pub enum BlendSupport {
    Surface(Surface),
    Curve(Curve),
}

impl Surface {
    pub fn kind(&self) -> &'static str {
        match self {
            Surface::RollingBall { .. } => "rolling-ball blend",
            Surface::Ruled { .. } => "ruled",
            Surface::ArcSweep { .. } => "arc sweep",
            Surface::Plane { .. } => "plane",
            Surface::Cone { half_angle, .. } if *half_angle == 0.0 => "cylinder",
            Surface::Cone { .. } => "cone",
            Surface::Sphere { .. } => "sphere",
            Surface::Torus { .. } => "torus",
            Surface::BSpline(_) => "bspline",
            Surface::Extrusion { .. } => "extrusion",
            Surface::Revolution { .. } => "revolution",
            Surface::Offset { .. } => "offset",
            Surface::Unknown(_) => "unknown",
        }
    }

    /// Distance from a point to the (untrimmed) surface, when cheap to
    /// compute; `None` for free-form surfaces.
    pub fn distance(&self, p: P3) -> Option<f64> {
        match self {
            Surface::Plane { origin, normal, .. } => Some(dot(sub(p, *origin), *normal).abs()),
            Surface::Cone {
                origin,
                axis,
                radius,
                half_angle,
                ..
            } => {
                let d = sub(p, *origin);
                let h = dot(d, *axis);
                let rho = norm(sub(d, scale(*axis, h)));
                // Distance in the meridian half-plane to the line
                // r = radius + h tan(a).
                let (s, c) = half_angle.sin_cos();
                Some(((rho - radius) * c - h * s).abs())
            }
            Surface::Sphere { center, radius, .. } => Some((dist(p, *center) - radius).abs()),
            Surface::Torus {
                center,
                axis,
                major,
                minor,
                ..
            } => {
                let d = sub(p, *center);
                let h = dot(d, *axis);
                let rho = norm(sub(d, scale(*axis, h)));
                Some(((rho - major).hypot(h) - minor).abs())
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Vertex {
    pub point: P3,
    pub tolerance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub curve: usize,
    pub t: [f64; 2],
    pub vertices: [usize; 2],
    pub tolerance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coedge {
    pub edge: usize,
    pub forward: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub surface: usize,
    pub reversed: bool,
    pub double_sided: bool,
    pub loops: Vec<Vec<Coedge>>,
    /// Loops that are a single point: vertices where the face closes up
    /// at a singular point of its surface (a cone apex, a sphere pole).
    pub point_loops: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shell {
    pub faces: Vec<usize>,
    /// Edges of the shell that bound no face (wire parts).
    pub wire_edges: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Lump {
    pub shells: Vec<Shell>,
}

/// Affine map: `p' = m * p + t` (rows of `m`), already in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub m: [[f64; 3]; 3],
    pub t: P3,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Body {
    pub lumps: Vec<Lump>,
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub vertices: Vec<Vertex>,
    pub curves: Vec<Curve>,
    pub surfaces: Vec<Surface>,
    pub transform: Option<Transform>,
}

/// Result of [`Body::check`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CheckReport {
    /// Edges used by one coedge only: the boundary of a sheet.
    pub free_edges: usize,
    /// Edges used more often in one direction than in the other (outside
    /// double-sided faces): inconsistent orientation.
    pub unpaired_coedges: usize,
    /// Loops whose coedges do not connect end to start.
    pub open_loops: usize,
    /// Edge ends farther than the tolerance from their vertex.
    pub vertex_mismatches: usize,
    /// Largest vertex-to-curve-end distance (mm).
    pub max_vertex_gap: f64,
    /// Vertices farther than the tolerance from an analytic face surface.
    pub off_surface_vertices: usize,
    pub max_surface_gap: f64,
    pub messages: Vec<String>,
}

impl CheckReport {
    pub fn is_clean(&self) -> bool {
        self.unpaired_coedges == 0 && self.open_loops == 0 && self.vertex_mismatches == 0
    }
}

impl Coedge {
    fn start_end(&self, body: &Body) -> (usize, usize) {
        let e = &body.edges[self.edge];
        if self.forward {
            (e.vertices[0], e.vertices[1])
        } else {
            (e.vertices[1], e.vertices[0])
        }
    }
}

impl Body {
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Per edge of the shell: (forward uses, reversed uses).
    fn edge_uses(&self, shell: &Shell) -> std::collections::HashMap<usize, (usize, usize)> {
        let mut uses: std::collections::HashMap<usize, (usize, usize)> =
            std::collections::HashMap::new();
        for &fi in &shell.faces {
            for c in self.faces[fi].loops.iter().flatten() {
                let u = uses.entry(c.edge).or_insert((0, 0));
                if c.forward {
                    u.0 += 1;
                } else {
                    u.1 += 1;
                }
            }
        }
        uses
    }

    /// The shell bounds a volume: it has faces, every edge is used as often
    /// in one direction as in the other, and no face is double-sided. An
    /// edge is mostly used once each way; ASM also puts the seam of a full
    /// cylinder on the line where it touches planar faces (a hole tangent to
    /// a wall), an edge used twice each way.
    pub fn shell_is_closed(&self, shell: &Shell) -> bool {
        !shell.faces.is_empty()
            && shell.faces.iter().all(|&f| !self.faces[f].double_sided)
            && self
                .edge_uses(shell)
                .values()
                .all(|&(a, b)| a == b && a >= 1)
    }

    /// Splits shells whose faces fall into groups that share no edge (ASM
    /// allows that in sheet bodies, OCCT does not). The groups of a lump's
    /// only shell are lumps of their own: pieces side by side, not one
    /// inside another (a lump's other shells are its voids, which ASM keeps
    /// as shells of their own).
    pub fn split_disconnected_shells(&mut self) {
        let mut pieces = Vec::new();
        for li in 0..self.lumps.len() {
            let single = self.lumps[li].shells.len() == 1;
            let mut out = Vec::new();
            for shell in std::mem::take(&mut self.lumps[li].shells) {
                let n = shell.faces.len();
                let mut parent: Vec<usize> = (0..n).collect();
                fn find(p: &mut [usize], mut i: usize) -> usize {
                    while p[i] != i {
                        p[i] = p[p[i]];
                        i = p[i];
                    }
                    i
                }
                let mut by_edge: std::collections::HashMap<usize, usize> =
                    std::collections::HashMap::new();
                for (k, &fi) in shell.faces.iter().enumerate() {
                    for c in self.faces[fi].loops.iter().flatten() {
                        if let Some(&other) = by_edge.get(&c.edge) {
                            let (a, b) = (find(&mut parent, k), find(&mut parent, other));
                            parent[a] = b;
                        } else {
                            by_edge.insert(c.edge, k);
                        }
                    }
                }
                let mut groups: Vec<(usize, Shell)> = Vec::new();
                for (k, &fi) in shell.faces.iter().enumerate() {
                    let root = find(&mut parent, k);
                    match groups.iter_mut().find(|(r, _)| *r == root) {
                        Some((_, s)) => s.faces.push(fi),
                        None => groups.push((
                            root,
                            Shell {
                                faces: vec![fi],
                                wire_edges: Vec::new(),
                            },
                        )),
                    }
                }
                if groups.len() <= 1 {
                    out.push(shell);
                } else if single {
                    let mut groups = groups.into_iter().map(|(_, s)| s);
                    out.extend(groups.next());
                    pieces.extend(groups.map(|s| Lump { shells: vec![s] }));
                } else {
                    out.extend(groups.into_iter().map(|(_, s)| s));
                }
            }
            self.lumps[li].shells = out;
        }
        self.lumps.extend(pieces);
    }

    /// The shell is closed ([`Body::shell_is_closed`]) and the builder makes
    /// each of its faces: none is on an unknown surface (mitcad_private#198:
    /// such a face keeps the topology closed, but has no geometry).
    pub fn shell_is_closed_solid(&self, shell: &Shell) -> bool {
        self.shell_is_closed(shell)
            && shell
                .faces
                .iter()
                .all(|&f| !matches!(self.surfaces[self.faces[f].surface], Surface::Unknown(_)))
    }

    /// Every shell of every lump is closed and has no face on an unknown
    /// surface: the body is a solid.
    pub fn is_solid(&self) -> bool {
        !self.lumps.is_empty()
            && self
                .lumps
                .iter()
                .all(|l| l.shells.iter().all(|s| self.shell_is_closed_solid(s)))
    }

    /// Checks topology and geometry consistency. `tolerance` (mm) is the
    /// allowed vertex gap in addition to the vertex and edge tolerances.
    pub fn check(&self, tolerance: f64) -> CheckReport {
        let mut r = CheckReport::default();
        let note = |r: &mut CheckReport, m: String| {
            if r.messages.len() < 20 {
                r.messages.push(m);
            }
        };
        for (fi, f) in self.faces.iter().enumerate() {
            for (li, lp) in f.loops.iter().enumerate() {
                let n = lp.len();
                let mut open = false;
                for i in 0..n {
                    let (_, end) = lp[i].start_end(self);
                    let (start, _) = lp[(i + 1) % n].start_end(self);
                    if end != start {
                        let gap = dist(self.vertices[end].point, self.vertices[start].point);
                        if gap > tolerance {
                            open = true;
                        }
                    }
                }
                if open {
                    r.open_loops += 1;
                    note(&mut r, format!("face {fi} loop {li} is open"));
                }
            }
        }
        // An inner edge of a shell is used as often in each direction (once,
        // or twice at a cylinder's seam on planar faces it touches).
        for lump in &self.lumps {
            for shell in &lump.shells {
                let uses = self.edge_uses(shell);
                let sheet = shell.faces.iter().any(|&f| self.faces[f].double_sided);
                r.free_edges += uses.values().filter(|(a, b)| a + b == 1).count();
                if !sheet {
                    let mut bad: Vec<(usize, (usize, usize))> = uses
                        .iter()
                        .filter(|(_, (a, b))| a + b >= 2 && a != b)
                        .map(|(e, u)| (*e, *u))
                        .collect();
                    bad.sort_unstable();
                    r.unpaired_coedges += bad.len();
                    if !bad.is_empty() {
                        let first: Vec<String> = bad
                            .iter()
                            .take(4)
                            .map(|(e, (a, b))| format!("edge {e} ({a} forward, {b} reversed)"))
                            .collect();
                        // The faces at the first such edge.
                        let at: Vec<String> = shell
                            .faces
                            .iter()
                            .flat_map(|&fi| {
                                self.faces[fi]
                                    .loops
                                    .iter()
                                    .flatten()
                                    .filter(|c| c.edge == bad[0].0)
                                    .map(move |c| (fi, c.forward))
                            })
                            .map(|(fi, forward)| {
                                format!(
                                    "face {fi} ({}{}{})",
                                    self.surfaces[self.faces[fi].surface].kind(),
                                    if self.faces[fi].reversed {
                                        ", reversed"
                                    } else {
                                        ""
                                    },
                                    if forward { "" } else { ", coedge reversed" }
                                )
                            })
                            .collect();
                        note(
                            &mut r,
                            format!(
                                "{} edges used inconsistently: {}; at edge {}: {}",
                                bad.len(),
                                first.join(", "),
                                bad[0].0,
                                at.join(", ")
                            ),
                        );
                    }
                }
            }
        }
        for (ei, e) in self.edges.iter().enumerate() {
            let c = &self.curves[e.curve];
            for k in 0..2 {
                let v = &self.vertices[e.vertices[k]];
                let gap = dist(c.eval(e.t[k]), v.point);
                r.max_vertex_gap = r.max_vertex_gap.max(gap);
                if gap > tolerance + v.tolerance + e.tolerance {
                    r.vertex_mismatches += 1;
                    note(
                        &mut r,
                        format!(
                            "edge {ei} ({}) end {k} is {gap:.3e} mm from its vertex",
                            c.kind()
                        ),
                    );
                }
            }
        }
        for (fi, f) in self.faces.iter().enumerate() {
            let s = &self.surfaces[f.surface];
            for c in f.loops.iter().flatten() {
                for &vi in &self.edges[c.edge].vertices {
                    let v = &self.vertices[vi];
                    if let Some(d) = s.distance(v.point) {
                        r.max_surface_gap = r.max_surface_gap.max(d);
                        if d > tolerance + v.tolerance {
                            r.off_surface_vertices += 1;
                            note(
                                &mut r,
                                format!("face {fi} ({}) vertex {vi} is {d:.3e} mm off", s.kind()),
                            );
                        }
                    }
                }
            }
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A helix sampled 64 times a turn (radius 33 mm) evaluates between its
    /// samples within a micrometre (mitcad#235: piecewise linear it was
    /// 0.04 mm off, and a coil's state failed its check).
    #[test]
    fn interpolated_curves_evaluate_between_samples() {
        let r = 33.0;
        let helix = |t: f64| [r * t.cos(), r * t.sin(), 2.1 * t / std::f64::consts::TAU];
        let n = 64;
        let params: Vec<f64> = (0..=n)
            .map(|i| std::f64::consts::TAU * f64::from(i) / f64::from(n))
            .collect();
        let points = params.iter().map(|&t| helix(t)).collect();
        let curve = Curve::Interpolated { params, points };
        for k in 0..=200 {
            let t = std::f64::consts::TAU * f64::from(k) / 200.0;
            assert!(dist(curve.eval(t), helix(t)) < 1e-3, "t {t}");
        }
    }

    /// mitcad_private#198: a tapered helix of 33 mm radius and 2.1 mm pitch
    /// over 3.4 turns. Its B-spline (degree 7, twelve spans a turn, interior
    /// knots of multiplicity 4) is within 1e-7 mm of it, its ends on the
    /// helix's; its length is the integral's.
    #[test]
    fn helices_are_exact_b_splines() {
        let tau = std::f64::consts::TAU;
        let helix = Helix {
            center: [1.0, 2.0, 3.0],
            major: [33.0, 0.0, 0.0],
            minor: [0.0, 33.0, 0.0],
            pitch: [0.0, 0.0, 2.1],
            scale: 1.0,
            growth: 0.05,
            range: [0.3, 0.3 + 3.4 * tau],
            spans: 0,
        };
        let spans = helix_spans(helix.turns(), 2160).unwrap();
        assert_eq!(spans, 41);
        let b = helix.bspline(spans);
        assert!(b.is_valid());
        assert_eq!(b.degree, 7);
        assert_eq!(b.poles.len(), 4 * spans + 4);
        assert_eq!(b.mults[1], 4);
        assert_eq!(b.range(), (helix.range[0], helix.range[1]));
        for k in 0..=2000 {
            let t = helix.range[0] + (helix.range[1] - helix.range[0]) * f64::from(k) / 2000.0;
            let d = dist(b.eval(t), helix.eval(t));
            assert!(d < 1e-7, "t {t}: {d:e} mm");
        }
        // A helix without taper: the length of the unrolled line.
        let plain = Helix {
            growth: 0.0,
            ..helix.clone()
        };
        let want = 3.4 * (tau * 33.0).hypot(2.1);
        let got = plain.length(plain.range[0], plain.range[1]);
        assert!((got - want).abs() < 1e-9 * want, "{got} {want}");
        // The derivative is the curve's.
        let t = 2.0;
        let h = 1e-6;
        let fd = scale(sub(helix.eval(t + h), helix.eval(t - h)), 0.5 / h);
        assert!(dist(fd, helix.derivative(t)) < 1e-5);
        // Long helices take fewer spans a turn, up to the most.
        assert_eq!(helix_spans(200.0, 2160), Some(2160));
        assert_eq!(helix_spans(300.0, 2160), None);
        assert_eq!(helix_spans(0.01, 2160), Some(1));
    }

    /// One closed edge: two faces use it once each way, a third twice (its
    /// seam), as ASM stores a full cylinder touching a wall.
    fn seam_on_a_wall(third: [bool; 2]) -> Body {
        let face = |loops: Vec<Vec<Coedge>>| Face {
            surface: 0,
            reversed: false,
            double_sided: false,
            loops,
            point_loops: Vec::new(),
        };
        let use_ = |forward: bool| Coedge { edge: 0, forward };
        Body {
            lumps: vec![Lump {
                shells: vec![Shell {
                    faces: vec![0, 1, 2],
                    wire_edges: Vec::new(),
                }],
            }],
            faces: vec![
                face(vec![vec![use_(true)]]),
                face(vec![vec![use_(false)]]),
                face(vec![vec![use_(third[0])], vec![use_(third[1])]]),
            ],
            edges: vec![Edge {
                curve: 0,
                t: [0.0, 0.0],
                vertices: [0, 0],
                tolerance: 0.0,
            }],
            vertices: vec![Vertex {
                point: [0.0; 3],
                tolerance: 0.0,
            }],
            curves: vec![Curve::Line {
                origin: [0.0; 3],
                dir: [1.0, 0.0, 0.0],
            }],
            surfaces: vec![Surface::Plane {
                origin: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                u_dir: [1.0, 0.0, 0.0],
            }],
            transform: None,
        }
    }

    #[test]
    fn edges_used_as_often_each_way_close_a_shell() {
        let balanced = seam_on_a_wall([true, false]);
        assert!(balanced.is_solid());
        assert!(balanced.check(1e-3).is_clean());
        let unbalanced = seam_on_a_wall([true, true]);
        assert!(!unbalanced.is_solid());
        let report = unbalanced.check(1e-3);
        assert_eq!(report.unpaired_coedges, 1);
        assert!(report.messages[0].contains("edge 0 (3 forward, 1 reversed)"));
    }

    #[test]
    fn evaluates_clamped_cubic_bspline() {
        let b = BSplineCurve {
            degree: 3,
            knots: vec![0.0, 1.0],
            mults: vec![4, 4],
            poles: vec![
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [2.0, 1.0, 0.0],
                [3.0, 0.0, 0.0],
            ],
            weights: None,
            periodic: false,
        };
        assert!(b.is_valid());
        assert_eq!(b.eval(0.0), [0.0, 0.0, 0.0]);
        assert_eq!(b.eval(1.0), [3.0, 0.0, 0.0]);
        let m = b.eval(0.5);
        assert!((m[0] - 1.5).abs() < 1e-12 && (m[1] - 0.75).abs() < 1e-12);
    }

    #[test]
    fn evaluates_rational_quarter_circle() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let b = BSplineCurve {
            degree: 2,
            knots: vec![0.0, 1.0],
            mults: vec![3, 3],
            poles: vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            weights: Some(vec![1.0, w, 1.0]),
            periodic: false,
        };
        for i in 0..=10 {
            let p = b.eval(i as f64 / 10.0);
            assert!((norm(p) - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn surface_distances() {
        let cyl = Surface::Cone {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 2.0,
            half_angle: 0.0,
        };
        assert!((cyl.distance([3.0, 0.0, 5.0]).unwrap() - 1.0).abs() < 1e-12);
        let cone = Surface::Cone {
            origin: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 1.0,
            half_angle: std::f64::consts::FRAC_PI_4,
        };
        assert!(cone.distance([2.0, 0.0, 1.0]).unwrap() < 1e-12);
        let torus = Surface::Torus {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            major: 5.0,
            minor: 1.0,
        };
        assert!(torus.distance([6.0, 0.0, 0.0]).unwrap() < 1e-12);
    }
}
