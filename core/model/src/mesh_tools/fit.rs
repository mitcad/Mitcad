// SPDX-License-Identifier: MIT
//! Surfaces fitted to a region of a mesh (mitcad#1871): a plane, a
//! cylinder, a cone, a sphere or a B-spline surface, each bounded where
//! the region's points are, as the boundary of a surface body
//! ([`crate::boundary::BoundaryBody`]) that the kernel builds.
//!
//! - Plane: through the points' mean, square to the direction they spread
//!   least along; bounded by the rectangle of their extent in it.
//! - Cylinder: its axis first along the direction the triangles' normals
//!   are square to, then the axis, its place and the radius refined by
//!   least squares on the points' distances (Levenberg-Marquardt). Cone:
//!   likewise with a half angle. Sphere: the algebraic fit, refined. Each
//!   is bounded by the points' extent along the axis and around it; a gap
//!   around the axis of less than 45 degrees closes the face all the way
//!   round. Faces of revolution are split into pieces of at most half a
//!   turn that share their edges. A sphere is bounded at most 89.4
//!   degrees above and below its equator, which runs through the middle
//!   of the region.
//! - B-spline: a bicubic surface over the points' mean plane, fitted by
//!   least squares to their heights over it with a little smoothing; the
//!   region must face one side of that plane (no folds).
//!
//! The deviation is the largest distance of a point of the region from the
//! surface.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::{
    Point, TriangleMesh, area_normal, corners, perpendicular, scatter, solve, symmetric_eigen,
};
use crate::boundary::{BoundaryBody, BoundaryCurve, BoundaryEdge, BoundaryFace, BoundarySurface};

/// The kinds of surface a region is fitted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Plane,
    Cylinder,
    Cone,
    Sphere,
    BSpline,
}

impl SurfaceKind {
    /// The order Auto tries them in: the simplest first.
    pub const ALL: [SurfaceKind; 5] = [
        SurfaceKind::Plane,
        SurfaceKind::Cylinder,
        SurfaceKind::Sphere,
        SurfaceKind::Cone,
        SurfaceKind::BSpline,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
            Self::Sphere => "sphere",
            Self::BSpline => "B-spline surface",
        }
    }
}

/// A fitted surface with its bounds.
#[derive(Debug, Clone, PartialEq)]
pub enum Fitted {
    Plane {
        origin: Point,
        normal: Point,
        u_dir: Point,
        /// The rectangle [u0, u1] × [v0, v1] in the plane.
        u: [f64; 2],
        v: [f64; 2],
    },
    /// A cone, a cylinder when `half_angle` is 0: `radius` at `origin` on
    /// the axis; heights along the axis from `origin`.
    Cone {
        origin: Point,
        axis: Point,
        ref_dir: Point,
        radius: f64,
        half_angle: f64,
        angles: Angles,
        heights: [f64; 2],
    },
    Sphere {
        center: Point,
        axis: Point,
        ref_dir: Point,
        radius: f64,
        angles: Angles,
        /// Latitudes from the equator (radians).
        latitudes: [f64; 2],
    },
    /// Bicubic, the same clamped uniform knots both ways; poles in rows
    /// along u, each row along v.
    BSpline {
        knots: Vec<f64>,
        multiplicities: Vec<u32>,
        poles: Vec<Vec<Point>>,
    },
}

/// The part of a turn a face of revolution covers: from `start`
/// (radians from its reference direction) `span` on (at most a full turn).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Angles {
    pub start: f64,
    pub span: f64,
}

impl Angles {
    pub fn full(&self) -> bool {
        self.span >= TAU - 1e-12
    }
}

/// A surface fitted to a region.
#[derive(Debug, Clone, PartialEq)]
pub struct FittedSurface {
    pub kind: SurfaceKind,
    pub surface: Fitted,
    /// The largest distance of a point from the surface (mm).
    pub deviation: f64,
}

impl FittedSurface {
    /// The surface in words: its kind and its size.
    pub fn describe(&self) -> String {
        match &self.surface {
            Fitted::Cone {
                radius, half_angle, ..
            } if *half_angle == 0.0 => format!("a cylinder of radius {}", mm(*radius)),
            Fitted::Cone {
                radius, half_angle, ..
            } => format!(
                "a cone of radius {} at its narrow end and half angle {} degrees",
                mm(*radius),
                round(half_angle.to_degrees(), 3)
            ),
            Fitted::Sphere { radius, .. } => format!("a sphere of radius {}", mm(*radius)),
            Fitted::Plane { .. } => "a plane".to_owned(),
            Fitted::BSpline { poles, .. } => format!(
                "a B-spline surface of {} by {} control points",
                poles.len(),
                poles.first().map_or(0, Vec::len)
            ),
        }
    }
}

fn round(value: f64, decimals: i32) -> f64 {
    let f = 10f64.powi(decimals);
    (value * f).round() / f
}

fn mm(value: f64) -> String {
    format!("{} mm", round(value, 4))
}

/// The most points the least-squares refinement takes (an even sample of
/// the region's vertices); the deviation is measured on all of them.
const MAX_FIT_POINTS: usize = 6000;

/// The region's points, its triangles' unit normals and areas.
struct Region {
    points: Vec<Point>,
    sample: Vec<Point>,
    normals: Vec<(Point, f64)>,
}

impl Region {
    fn of(mesh: &TriangleMesh) -> Result<Self, String> {
        if mesh.triangles.is_empty() || mesh.vertices.len() < 3 {
            return Err("the region has no triangles".to_owned());
        }
        let points = mesh.vertices.clone();
        let stride = points.len().div_ceil(MAX_FIT_POINTS).max(1);
        let sample = points.iter().step_by(stride).copied().collect();
        let normals = (0..mesh.triangles.len())
            .filter_map(|t| {
                let n = area_normal(&corners(mesh, t));
                let a = norm(n);
                (a > 0.0).then(|| (scale(n, 1.0 / a), a / 2.0))
            })
            .collect();
        Ok(Self {
            points,
            sample,
            normals,
        })
    }

    /// The area-weighted mean of the normals.
    fn mean_normal(&self) -> Point {
        let mut sum = [0.0; 3];
        for (n, a) in &self.normals {
            sum = add(sum, scale(*n, *a));
        }
        mitcad_vec3::unit(sum).unwrap_or([0.0, 0.0, 1.0])
    }

    /// The direction the normals are most nearly square to (a cylinder's
    /// axis), or, `centred`, the one along which they vary least (a cone's).
    fn normal_axis(&self, centred: bool) -> Point {
        let total: f64 = self.normals.iter().map(|(_, a)| a).sum::<f64>().max(1e-300);
        let mean = if centred {
            let mut sum = [0.0; 3];
            for (n, a) in &self.normals {
                sum = add(sum, scale(*n, *a / total));
            }
            sum
        } else {
            [0.0; 3]
        };
        let mut m = [[0.0; 3]; 3];
        for (n, a) in &self.normals {
            let d = sub(*n, mean);
            for i in 0..3 {
                for j in 0..3 {
                    m[i][j] += a * d[i] * d[j];
                }
            }
        }
        symmetric_eigen(m).1[0]
    }
}

/// The surface of `kind` fitted to the region.
pub fn fit_surface(mesh: &TriangleMesh, kind: SurfaceKind) -> Result<FittedSurface, String> {
    let region = Region::of(mesh)?;
    let (surface, deviation) = match kind {
        SurfaceKind::Plane => fit_plane(&region),
        SurfaceKind::Cylinder => fit_cone(&region, false)?,
        SurfaceKind::Cone => fit_cone(&region, true)?,
        SurfaceKind::Sphere => fit_sphere(&region)?,
        SurfaceKind::BSpline => fit_bspline(&region)?,
    };
    if !deviation.is_finite() {
        return Err(format!("no {} fits the region", kind.name()));
    }
    Ok(FittedSurface {
        kind,
        surface,
        deviation,
    })
}

/// The simplest surface within `tolerance` of every point, trying them in
/// [`SurfaceKind::ALL`]'s order; an error naming the closest when none is.
pub fn fit_auto(mesh: &TriangleMesh, tolerance: f64) -> Result<FittedSurface, String> {
    let mut closest: Option<FittedSurface> = None;
    let mut last_error = None;
    for kind in SurfaceKind::ALL {
        match fit_surface(mesh, kind) {
            Ok(fitted) if fitted.deviation <= tolerance => return Ok(fitted),
            Ok(fitted) => {
                if closest
                    .as_ref()
                    .is_none_or(|c| fitted.deviation < c.deviation)
                {
                    closest = Some(fitted);
                }
            }
            Err(e) => last_error = Some(e),
        }
    }
    match closest {
        Some(c) => Err(format!(
            "no surface lies within the tolerance {} of the region; the closest, {}, lies up to {} \
             from it",
            mm(tolerance),
            c.kind.name(),
            mm(c.deviation)
        )),
        None => Err(last_error.unwrap_or_else(|| "no surface fits the region".to_owned())),
    }
}

// Planes.

fn fit_plane(region: &Region) -> (Fitted, f64) {
    let (mean, m) = scatter(&region.points);
    let (_, vectors) = symmetric_eigen(m);
    let mut normal = vectors[0];
    if dot(normal, region.mean_normal()) < 0.0 {
        normal = scale(normal, -1.0);
    }
    let u_dir = vectors[2];
    let v_dir = cross(normal, u_dir);
    let mut u = [f64::INFINITY, f64::NEG_INFINITY];
    let mut v = [f64::INFINITY, f64::NEG_INFINITY];
    let mut deviation: f64 = 0.0;
    for p in &region.points {
        let d = sub(*p, mean);
        let (pu, pv) = (dot(d, u_dir), dot(d, v_dir));
        u = [u[0].min(pu), u[1].max(pu)];
        v = [v[0].min(pv), v[1].max(pv)];
        deviation = deviation.max(dot(d, normal).abs());
    }
    (
        Fitted::Plane {
            origin: mean,
            normal,
            u_dir,
            u,
            v,
        },
        deviation,
    )
}

// Least squares.

/// Levenberg-Marquardt on `residuals` from `x`, with a numerical Jacobian;
/// the parameters that make the sum of squares least.
fn least_squares(
    mut x: Vec<f64>,
    residuals: &dyn Fn(&[f64]) -> Option<Vec<f64>>,
    iterations: usize,
) -> Option<Vec<f64>> {
    let cost = |r: &[f64]| r.iter().map(|v| v * v).sum::<f64>();
    let mut r = residuals(&x)?;
    let mut current = cost(&r);
    let mut lambda = 1e-3;
    let p = x.len();
    for _ in 0..iterations {
        // The Jacobian, column by column.
        let mut jac: Vec<Vec<f64>> = Vec::with_capacity(p);
        for i in 0..p {
            let h = 1e-7 * (1.0 + x[i].abs());
            let mut moved = x.clone();
            moved[i] += h;
            let rm = residuals(&moved)?;
            jac.push(rm.iter().zip(&r).map(|(a, b)| (a - b) / h).collect());
        }
        let mut jtj = vec![0.0; p * p];
        let mut jtr = vec![0.0; p];
        for i in 0..p {
            for j in 0..p {
                jtj[i * p + j] = jac[i].iter().zip(&jac[j]).map(|(a, b)| a * b).sum();
            }
            jtr[i] = -jac[i].iter().zip(&r).map(|(a, b)| a * b).sum::<f64>();
        }
        let mut improved = false;
        for _ in 0..10 {
            let mut damped = jtj.clone();
            for i in 0..p {
                damped[i * p + i] += lambda * jtj[i * p + i].max(1e-12);
            }
            let Some(step) = solve(damped, jtr.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let trial: Vec<f64> = x.iter().zip(&step).map(|(a, b)| a + b).collect();
            if let Some(rt) = residuals(&trial) {
                let c = cost(&rt);
                if c < current {
                    let small = step
                        .iter()
                        .zip(&x)
                        .all(|(s, v)| s.abs() <= 1e-12 * (1.0 + v.abs()));
                    x = trial;
                    r = rt;
                    current = c;
                    lambda = (lambda / 3.0).max(1e-12);
                    improved = !small;
                    break;
                }
            }
            lambda *= 4.0;
        }
        if !improved {
            break;
        }
    }
    Some(x)
}

// Cylinders and cones.

/// A frame square to `axis`: two unit vectors across it.
fn across(axis: Point) -> (Point, Point) {
    let e1 = perpendicular(axis);
    (e1, cross(axis, e1))
}

/// The axis tilted by (p0, p1) from `axis` across it.
fn tilted(axis: Point, e1: Point, e2: Point, p0: f64, p1: f64) -> Option<Point> {
    mitcad_vec3::unit(add(axis, add(scale(e1, p0), scale(e2, p1))))
}

/// The distance of `p` from the cone's surface, signed (outside positive):
/// the radial distance from the radius at its height, across the slope.
fn cone_residual(p: Point, origin: Point, axis: Point, radius: f64, half_angle: f64) -> f64 {
    let d = sub(p, origin);
    let h = dot(d, axis);
    let rho = norm(sub(d, scale(axis, h)));
    (rho - (radius + h * half_angle.tan())) * half_angle.cos()
}

fn fit_cone(region: &Region, cone: bool) -> Result<(Fitted, f64), String> {
    let what = if cone { "cone" } else { "cylinder" };
    // The axis: square to the normals, or (a cone) where they vary least.
    let mut axis0 = region.normal_axis(false);
    if cone {
        let centred = region.normal_axis(true);
        // A narrow band of normals leaves two directions alike; the
        // uncentred one then.
        let total: f64 = region
            .normals
            .iter()
            .map(|(_, a)| a)
            .sum::<f64>()
            .max(1e-300);
        let spread = region
            .normals
            .iter()
            .map(|(n, a)| a * dot(*n, centred).powi(2))
            .sum::<f64>()
            / total;
        if spread < 0.5 {
            axis0 = centred;
        }
    }
    let (e1, e2) = across(axis0);
    let (mean, _) = scatter(&region.points);
    // The axis's place: the circle of the points seen along it.
    let flat: Vec<[f64; 2]> = region
        .sample
        .iter()
        .map(|p| {
            let d = sub(*p, mean);
            [dot(d, e1), dot(d, e2)]
        })
        .collect();
    let (centre2, radius0) = super::section::fit_circle(&flat).ok_or_else(|| {
        format!("no {what} fits the region: its points lie in a line seen along any axis")
    })?;
    let origin0 = add(mean, add(scale(e1, centre2[0]), scale(e2, centre2[1])));
    // A cone's slope and radius at the origin: the radii against the
    // heights, by least squares.
    let (mut r0, mut slope) = (radius0, 0.0);
    if cone {
        let (mut sh, mut sr, mut shh, mut shr, n) =
            (0.0, 0.0, 0.0, 0.0, region.sample.len() as f64);
        for p in &region.sample {
            let d = sub(*p, origin0);
            let h = dot(d, axis0);
            let rho = norm(sub(d, scale(axis0, h)));
            sh += h;
            sr += rho;
            shh += h * h;
            shr += h * rho;
        }
        let det = n * shh - sh * sh;
        if det.abs() > 1e-12 * n * n {
            slope = (n * shr - sh * sr) / det;
            r0 = (sr - slope * sh) / n;
        }
    }
    let params = |x: &[f64]| -> Option<(Point, Point, f64, f64)> {
        let axis = tilted(axis0, e1, e2, x[0], x[1])?;
        let origin = add(origin0, add(scale(e1, x[2]), scale(e2, x[3])));
        let half_angle = if cone { x[5] } else { 0.0 };
        Some((origin, axis, x[4], half_angle))
    };
    let start = if cone {
        vec![0.0, 0.0, 0.0, 0.0, r0, slope.atan()]
    } else {
        vec![0.0, 0.0, 0.0, 0.0, radius0]
    };
    let sample = &region.sample;
    let residuals = |x: &[f64]| -> Option<Vec<f64>> {
        let (origin, axis, radius, half_angle) = params(x)?;
        if cone && half_angle.abs() >= FRAC_PI_2 - 1e-3 {
            return None;
        }
        Some(
            sample
                .iter()
                .map(|p| cone_residual(*p, origin, axis, radius, half_angle))
                .collect(),
        )
    };
    let x =
        least_squares(start, &residuals, 60).ok_or_else(|| format!("no {what} fits the region"))?;
    let (origin, axis, radius, half_angle) =
        params(&x).ok_or_else(|| format!("no {what} fits the region"))?;
    let (mut origin, mut axis, mut radius, mut half_angle) = (origin, axis, radius, half_angle);
    if cone && half_angle < 0.0 {
        // The same cone with its axis the other way: the radius grows along it.
        axis = scale(axis, -1.0);
        half_angle = -half_angle;
    }
    if !radius.is_finite() || (!cone && radius <= 0.0) {
        return Err(format!("no {what} fits the region"));
    }
    let ref_dir = perpendicular(axis);
    let third = cross(axis, ref_dir);
    let mut heights = [f64::INFINITY, f64::NEG_INFINITY];
    let mut angles = Vec::with_capacity(region.points.len());
    let mut deviation: f64 = 0.0;
    for p in &region.points {
        let d = sub(*p, origin);
        let h = dot(d, axis);
        heights = [heights[0].min(h), heights[1].max(h)];
        angles.push(dot(d, third).atan2(dot(d, ref_dir)));
        deviation = deviation.max(cone_residual(*p, origin, axis, radius, half_angle).abs());
    }
    // A cone's radius must stay above zero over the region: its apex
    // outside it. Measured from the lowest height on, the radius there.
    if cone {
        let tan = half_angle.tan();
        let low = radius + heights[0] * tan;
        if low <= 1e-9 * (1.0 + radius.abs()) {
            return Err(
                "the cone fitted to the region has its tip inside the region; fit it in parts"
                    .to_owned(),
            );
        }
        origin = add(origin, scale(axis, heights[0]));
        radius = low;
        heights = [0.0, heights[1] - heights[0]];
    }
    let angles = angle_extent(angles);
    Ok((
        Fitted::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
            angles,
            heights,
        },
        deviation,
    ))
}

/// The part of a turn the angles cover: all of it when no gap between
/// neighbouring angles reaches 45 degrees, else from the end of the
/// largest gap round to its start.
fn angle_extent(mut angles: Vec<f64>) -> Angles {
    for a in &mut angles {
        *a = a.rem_euclid(TAU);
    }
    angles.sort_by(f64::total_cmp);
    if angles.len() < 2 {
        return Angles {
            start: 0.0,
            span: TAU,
        };
    }
    let mut gap = angles[0] + TAU - angles[angles.len() - 1];
    let mut start = angles[0];
    for w in angles.windows(2) {
        if w[1] - w[0] > gap {
            gap = w[1] - w[0];
            start = w[1];
        }
    }
    if gap < PI / 4.0 {
        Angles {
            start: 0.0,
            span: TAU,
        }
    } else {
        Angles {
            start,
            span: TAU - gap,
        }
    }
}

// Spheres.

fn fit_sphere(region: &Region) -> Result<(Fitted, f64), String> {
    let (mean, _) = scatter(&region.points);
    // x² + y² + z² + D·p + G = 0 about the mean.
    let mut a = vec![0.0; 16];
    let mut b = vec![0.0; 4];
    for p in &region.sample {
        let d = sub(*p, mean);
        let row = [d[0], d[1], d[2], 1.0];
        let rhs = -dot(d, d);
        for i in 0..4 {
            for j in 0..4 {
                a[i * 4 + j] += row[i] * row[j];
            }
            b[i] += row[i] * rhs;
        }
    }
    let s = solve(a, b).ok_or("no sphere fits the region: its points lie in a plane")?;
    let c0 = [-s[0] / 2.0, -s[1] / 2.0, -s[2] / 2.0];
    let r2 = dot(c0, c0) - s[3];
    if r2 <= 0.0 {
        return Err("no sphere fits the region".to_owned());
    }
    let sample = &region.sample;
    let residuals = |x: &[f64]| -> Option<Vec<f64>> {
        let c = add(mean, [x[0], x[1], x[2]]);
        Some(sample.iter().map(|p| norm(sub(*p, c)) - x[3]).collect())
    };
    let x = least_squares(vec![c0[0], c0[1], c0[2], r2.sqrt()], &residuals, 40)
        .ok_or("no sphere fits the region")?;
    let center = add(mean, [x[0], x[1], x[2]]);
    let radius = x[3];
    if !(radius.is_finite() && radius > 0.0) {
        return Err("no sphere fits the region".to_owned());
    }
    // The equator through the middle of the region: the axis square to
    // the region's mean normal.
    let facing = region.mean_normal();
    let axis = perpendicular(facing);
    let ref_dir = facing;
    let third = cross(axis, ref_dir);
    let limit = FRAC_PI_2 - 0.01;
    let mut latitudes = [f64::INFINITY, f64::NEG_INFINITY];
    let mut angles = Vec::with_capacity(region.points.len());
    let mut deviation: f64 = 0.0;
    for p in &region.points {
        let d = sub(*p, center);
        let r = norm(d).max(1e-300);
        let lat = (dot(d, axis) / r).clamp(-1.0, 1.0).asin();
        latitudes = [latitudes[0].min(lat), latitudes[1].max(lat)];
        angles.push(dot(d, third).atan2(dot(d, ref_dir)));
        deviation = deviation.max((r - radius).abs());
    }
    let latitudes = [latitudes[0].max(-limit), latitudes[1].min(limit)];
    if latitudes[1] - latitudes[0] <= 1e-9 {
        return Err("the region is too thin across to bound a sphere's face".to_owned());
    }
    Ok((
        Fitted::Sphere {
            center,
            axis,
            ref_dir,
            radius,
            angles: angle_extent(angles),
            latitudes,
        },
        deviation,
    ))
}

// B-spline surfaces.

const DEGREE: usize = 3;

/// The full clamped knot vector of `poles` poles of degree 3, on [0, 1].
fn knot_vector(poles: usize) -> Vec<f64> {
    let spans = poles - DEGREE;
    let mut knots = vec![0.0; DEGREE];
    for i in 0..=spans {
        knots.push(i as f64 / spans as f64);
    }
    knots.extend(std::iter::repeat_n(1.0, DEGREE));
    knots
}

/// The basis functions' values at `t` (one per pole).
fn basis(knots: &[f64], poles: usize, t: f64) -> Vec<f64> {
    let t = t.clamp(0.0, 1.0);
    // The span: the last knot interval that starts at or before t.
    let mut span = DEGREE;
    while span + 1 < poles && knots[span + 1] <= t {
        span += 1;
    }
    let mut n = vec![0.0; poles];
    let mut local = [0.0; DEGREE + 1];
    local[0] = 1.0;
    let mut left = [0.0; DEGREE + 1];
    let mut right = [0.0; DEGREE + 1];
    for j in 1..=DEGREE {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator.abs() > 0.0 {
                local[r] / denominator
            } else {
                0.0
            };
            local[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        local[j] = saved;
    }
    for (k, value) in local.iter().enumerate() {
        n[span - DEGREE + k] = *value;
    }
    n
}

/// The Greville abscissa of each pole: where it sits along the surface.
fn greville(knots: &[f64], poles: usize) -> Vec<f64> {
    (0..poles)
        .map(|i| knots[i + 1..=i + DEGREE].iter().sum::<f64>() / DEGREE as f64)
        .collect()
}

fn fit_bspline(region: &Region) -> Result<(Fitted, f64), String> {
    let (mean, m) = scatter(&region.points);
    let (_, vectors) = symmetric_eigen(m);
    let mut normal = vectors[0];
    if dot(normal, region.mean_normal()) < 0.0 {
        normal = scale(normal, -1.0);
    }
    let u_dir = vectors[2];
    let v_dir = cross(normal, u_dir);
    // No folds: (nearly) every triangle faces the plane's side.
    let total: f64 = region
        .normals
        .iter()
        .map(|(_, a)| a)
        .sum::<f64>()
        .max(1e-300);
    let folded: f64 = region
        .normals
        .iter()
        .filter(|(n, _)| dot(*n, normal) <= 0.02)
        .map(|(_, a)| a)
        .sum();
    if folded > 0.01 * total {
        return Err(
            "the region folds over as seen square to its mean plane; a B-spline surface is fitted \
             to a region that faces one side (segment it into smaller regions first)"
                .to_owned(),
        );
    }
    let local: Vec<Point> = region
        .points
        .iter()
        .map(|p| {
            let d = sub(*p, mean);
            [dot(d, u_dir), dot(d, v_dir), dot(d, normal)]
        })
        .collect();
    let mut u = [f64::INFINITY, f64::NEG_INFINITY];
    let mut v = [f64::INFINITY, f64::NEG_INFINITY];
    for p in &local {
        u = [u[0].min(p[0]), u[1].max(p[0])];
        v = [v[0].min(p[1]), v[1].max(p[1])];
    }
    let (du, dv) = (u[1] - u[0], v[1] - v[0]);
    if du <= 1e-9 || dv <= 1e-9 {
        return Err("the region has no extent to fit a B-spline surface over".to_owned());
    }
    let count = ((local.len() as f64 / 6.0).sqrt().round() as usize).clamp(DEGREE + 1, 12);
    let knots = knot_vector(count);
    let unknowns = count * count;
    let mut a = vec![0.0; unknowns * unknowns];
    let mut b = vec![0.0; unknowns];
    let param = |p: &Point| ((p[0] - u[0]) / du, (p[1] - v[0]) / dv);
    for p in &local {
        let (s, t) = param(p);
        let bu = basis(&knots, count, s);
        let bv = basis(&knots, count, t);
        let terms: Vec<(usize, f64)> = (0..count)
            .flat_map(|i| (0..count).map(move |j| (i, j)))
            .filter_map(|(i, j)| {
                let w = bu[i] * bv[j];
                (w != 0.0).then_some((i * count + j, w))
            })
            .collect();
        for &(k, wk) in &terms {
            b[k] += wk * p[2];
            for &(l, wl) in &terms {
                a[k * unknowns + l] += wk * wl;
            }
        }
    }
    // A little smoothing: the second differences of the heights along both
    // ways, so that poles no point reaches stay in line.
    let lambda = 1e-3 * local.len() as f64 / unknowns as f64;
    let mut smooth = |k: [usize; 3]| {
        let w = [1.0, -2.0, 1.0];
        for x in 0..3 {
            for y in 0..3 {
                a[k[x] * unknowns + k[y]] += lambda * w[x] * w[y];
            }
        }
    };
    for i in 0..count {
        for j in 0..count {
            if i + 2 < count {
                smooth([i * count + j, (i + 1) * count + j, (i + 2) * count + j]);
            }
            if j + 2 < count {
                smooth([i * count + j, i * count + j + 1, i * count + j + 2]);
            }
        }
    }
    let heights = solve(a, b).ok_or("no B-spline surface fits the region")?;
    let mut deviation: f64 = 0.0;
    for p in &local {
        let (s, t) = param(p);
        let bu = basis(&knots, count, s);
        let bv = basis(&knots, count, t);
        let mut z = 0.0;
        for i in 0..count {
            if bu[i] == 0.0 {
                continue;
            }
            for j in 0..count {
                z += bu[i] * bv[j] * heights[i * count + j];
            }
        }
        deviation = deviation.max((z - p[2]).abs());
    }
    let g = greville(&knots, count);
    let poles: Vec<Vec<Point>> = (0..count)
        .map(|i| {
            (0..count)
                .map(|j| {
                    let (pu, pv) = (u[0] + g[i] * du, v[0] + g[j] * dv);
                    add(
                        mean,
                        add(
                            scale(u_dir, pu),
                            add(scale(v_dir, pv), scale(normal, heights[i * count + j])),
                        ),
                    )
                })
                .collect()
        })
        .collect();
    let mut distinct = vec![0.0];
    let mut multiplicities = vec![DEGREE as u32 + 1];
    for &k in &knots[DEGREE + 1..knots.len() - DEGREE - 1] {
        distinct.push(k);
        multiplicities.push(1);
    }
    distinct.push(1.0);
    multiplicities.push(DEGREE as u32 + 1);
    Ok((
        Fitted::BSpline {
            knots: distinct,
            multiplicities,
            poles,
        },
        deviation,
    ))
}

// The boundary of the surface body.

/// A builder of a boundary of one shell.
#[derive(Default)]
struct Builder {
    body: BoundaryBody,
}

impl Builder {
    fn vertex(&mut self, p: Point) -> usize {
        self.body.vertices.push(p);
        self.body.vertices.len() - 1
    }

    fn curve(&mut self, c: BoundaryCurve) -> usize {
        self.body.curves.push(c);
        self.body.curves.len() - 1
    }

    fn edge(&mut self, curve: usize, range: [f64; 2], vertices: [usize; 2]) -> usize {
        self.body.edges.push(BoundaryEdge {
            curve,
            range,
            vertices,
        });
        self.body.edges.len() - 1
    }

    /// A straight edge between two vertices.
    fn line(&mut self, a: usize, b: usize) -> usize {
        let (pa, pb) = (self.body.vertices[a], self.body.vertices[b]);
        let length = norm(sub(pb, pa));
        let dir = mitcad_vec3::unit(sub(pb, pa)).unwrap_or([1.0, 0.0, 0.0]);
        let curve = self.curve(BoundaryCurve::Line { origin: pa, dir });
        self.edge(curve, [0.0, length], [a, b])
    }

    fn circle(&mut self, center: Point, normal: Point, major_dir: Point, radius: f64) -> usize {
        self.curve(BoundaryCurve::Ellipse {
            center,
            normal,
            major_dir,
            major: radius,
            minor: radius,
        })
    }

    fn face(&mut self, surface: usize, lp: Vec<(usize, bool)>) {
        self.body.faces.push(BoundaryFace {
            surface,
            reversed: false,
            loops: if lp.is_empty() { Vec::new() } else { vec![lp] },
        });
    }

    fn finish(mut self) -> BoundaryBody {
        self.body.shells = vec![(0..self.body.faces.len()).collect()];
        self.body
    }
}

/// The angles splitting a turn's part into pieces of at most half a turn.
fn pieces(angles: &Angles) -> Vec<f64> {
    let k = (angles.span / PI - 1e-9).ceil().max(1.0) as usize;
    (0..=k)
        .map(|j| angles.start + angles.span * j as f64 / k as f64)
        .collect()
}

/// The surface body's boundary: one face, or a face of revolution's
/// pieces sharing their edges (see the module docs).
pub fn boundary(fitted: &FittedSurface) -> Result<BoundaryBody, String> {
    let mut b = Builder::default();
    match &fitted.surface {
        Fitted::Plane {
            origin,
            normal,
            u_dir,
            u,
            v,
        } => {
            if u[1] - u[0] <= 1e-9 || v[1] - v[0] <= 1e-9 {
                return Err("the region has no extent in its plane".to_owned());
            }
            let v_dir = cross(*normal, *u_dir);
            let at = |pu: f64, pv: f64| add(*origin, add(scale(*u_dir, pu), scale(v_dir, pv)));
            let surface = b.body.surfaces.len();
            b.body.surfaces.push(BoundarySurface::Plane {
                origin: *origin,
                normal: *normal,
                u_dir: *u_dir,
            });
            let corners = [
                b.vertex(at(u[0], v[0])),
                b.vertex(at(u[1], v[0])),
                b.vertex(at(u[1], v[1])),
                b.vertex(at(u[0], v[1])),
            ];
            let lp = (0..4)
                .map(|i| (b.line(corners[i], corners[(i + 1) % 4]), true))
                .collect();
            b.face(surface, lp);
        }
        Fitted::Cone {
            origin,
            axis,
            ref_dir,
            radius,
            half_angle,
            angles,
            heights,
        } => {
            if heights[1] - heights[0] <= 1e-9 {
                return Err("the region has no extent along the axis".to_owned());
            }
            let third = cross(*axis, *ref_dir);
            let dir = |a: f64| add(scale(*ref_dir, a.cos()), scale(third, a.sin()));
            let tan = half_angle.tan();
            let ring = |h: f64| (add(*origin, scale(*axis, h)), radius + h * tan);
            let at = |a: f64, h: f64| {
                let (c, r) = ring(h);
                add(c, scale(dir(a), r))
            };
            let surface = b.body.surfaces.len();
            b.body.surfaces.push(BoundarySurface::Cone {
                origin: *origin,
                axis: *axis,
                ref_dir: *ref_dir,
                radius: *radius,
                half_angle: *half_angle,
            });
            let cuts = pieces(angles);
            let full = angles.full();
            let k = cuts.len() - 1;
            let columns = if full { k } else { k + 1 };
            let bottom: Vec<usize> = (0..columns)
                .map(|j| b.vertex(at(cuts[j], heights[0])))
                .collect();
            let top: Vec<usize> = (0..columns)
                .map(|j| b.vertex(at(cuts[j], heights[1])))
                .collect();
            let (cb, rb) = ring(heights[0]);
            let (ct, rt) = ring(heights[1]);
            let low = b.circle(cb, *axis, *ref_dir, rb);
            let high = b.circle(ct, *axis, *ref_dir, rt);
            let sides: Vec<usize> = (0..columns).map(|j| b.line(bottom[j], top[j])).collect();
            for j in 0..k {
                let next = (j + 1) % columns;
                let e_low = b.edge(low, [cuts[j], cuts[j + 1]], [bottom[j], bottom[next]]);
                let e_high = b.edge(high, [cuts[j], cuts[j + 1]], [top[j], top[next]]);
                b.face(
                    surface,
                    vec![
                        (e_low, true),
                        (sides[next], true),
                        (e_high, false),
                        (sides[j], false),
                    ],
                );
            }
        }
        Fitted::Sphere {
            center,
            axis,
            ref_dir,
            radius,
            angles,
            latitudes,
        } => {
            let third = cross(*axis, *ref_dir);
            let dir = |a: f64| add(scale(*ref_dir, a.cos()), scale(third, a.sin()));
            let at = |a: f64, lat: f64| {
                add(
                    *center,
                    add(
                        scale(dir(a), radius * lat.cos()),
                        scale(*axis, radius * lat.sin()),
                    ),
                )
            };
            let surface = b.body.surfaces.len();
            b.body.surfaces.push(BoundarySurface::Sphere {
                center: *center,
                axis: *axis,
                ref_dir: *ref_dir,
                radius: *radius,
            });
            let cuts = pieces(angles);
            let full = angles.full();
            let k = cuts.len() - 1;
            let columns = if full { k } else { k + 1 };
            let [v0, v1] = *latitudes;
            let bottom: Vec<usize> = (0..columns).map(|j| b.vertex(at(cuts[j], v0))).collect();
            let top: Vec<usize> = (0..columns).map(|j| b.vertex(at(cuts[j], v1))).collect();
            let low = b.circle(
                add(*center, scale(*axis, radius * v0.sin())),
                *axis,
                *ref_dir,
                radius * v0.cos(),
            );
            let high = b.circle(
                add(*center, scale(*axis, radius * v1.sin())),
                *axis,
                *ref_dir,
                radius * v1.cos(),
            );
            let meridians: Vec<usize> = (0..columns)
                .map(|j| {
                    let d = dir(cuts[j]);
                    let curve = b.circle(*center, cross(d, *axis), d, *radius);
                    b.edge(curve, [v0, v1], [bottom[j], top[j]])
                })
                .collect();
            for j in 0..k {
                let next = (j + 1) % columns;
                let e_low = b.edge(low, [cuts[j], cuts[j + 1]], [bottom[j], bottom[next]]);
                let e_high = b.edge(high, [cuts[j], cuts[j + 1]], [top[j], top[next]]);
                b.face(
                    surface,
                    vec![
                        (e_low, true),
                        (meridians[next], true),
                        (e_high, false),
                        (meridians[j], false),
                    ],
                );
            }
        }
        Fitted::BSpline {
            knots,
            multiplicities,
            poles,
        } => {
            let surface = b.body.surfaces.len();
            b.body.surfaces.push(BoundarySurface::BSpline {
                u_degree: DEGREE as u32,
                v_degree: DEGREE as u32,
                u_knots: knots.clone(),
                u_multiplicities: multiplicities.clone(),
                v_knots: knots.clone(),
                v_multiplicities: multiplicities.clone(),
                poles: poles.clone(),
                weights: Vec::new(),
            });
            b.face(surface, Vec::new());
        }
    }
    let body = b.finish();
    body.check()?;
    Ok(body)
}
