// SPDX-License-Identifier: MIT
//! Silhouettes of curved faces on drawings' views, and extreme points
//! (mitcad#1909, GitHub #363): a view draws the outline of a curved face
//! where it turns away from the eye (a pipe bend's outer curve seen along
//! its axis), which is no edge of the model. Dimension picks such an
//! outline as it picks edges, and offers the extreme points of curves
//! along the view's axes (and along the line from a dimension's first
//! pick), so that an overall size to the top of a curved outline can be
//! placed (`drawing_annotations.rs` finds and resolves the picks).
//!
//! The kernel gives a face's silhouettes seen along a direction
//! (`Kernel::face_silhouettes`), kept per body version and direction in
//! the drawing's projection cache. Each is seen here in the view's plane:
//! a line, or a circle or an arc seen end on where it is one (a torus's
//! outline seen along its axis and a sphere's are their circles, taken
//! from the surface, as the kernel's outline curves approximate them),
//! else a curve known by its points; its extreme points are found on the
//! curve itself, not on its points.

use std::f64::consts::{PI, TAU};
use std::sync::Arc;

use mitcad_model::Document;
use mitcad_model::datum::{SurfaceGeometry, Vec3};
use mitcad_model::ids::BodyUid;
use mitcad_model::internal::fingerprint::Fingerprint;
use mitcad_model::kernel::{Curve3, Kernel};
use mitcad_model::topo::FaceName;
use mitcad_model::transform::Transform;
use mitcad_model_drawing::annotate::{BodyGeometry, CurvedFace, Seen, segment_distance};
use mitcad_model_drawing::bspline::Spline2;
use mitcad_model_drawing::internal::Silhouettes;
use mitcad_model_drawing::{DrawingCache, P2, ViewSpec, ViewSpline};
use mitcad_vec3::vec2::{dot as dot2, norm as len2, sub as sub2};

/// How far (view mm) a silhouette may lie from an edge's line and be that
/// edge: the kernel's outline curves approximate the surface.
pub(super) const ON_EDGE: f64 = 1e-4;

/// The silhouettes of a body's curved faces seen along `toward` (a unit
/// vector in the body's coordinates: toward the view's eye), from the
/// projection cache or the kernel. A torus whose axis lies square to the
/// direction has none here: its outline seen from the side is its tube's
/// arcs (`seen_torus`, mitcad#1694). A face the kernel gives none of, or
/// cannot work out, is left out.
pub(super) fn body_silhouettes<K: Kernel>(
    doc: &Document<K>,
    body: BodyUid,
    geometry: &BodyGeometry,
    toward: Vec3,
) -> Arc<Silhouettes> {
    let Some(shape) = doc.body_shape(body) else {
        return Arc::default();
    };
    let mut fp = Fingerprint::new("drawing-silhouettes-v1");
    fp.str(&body.to_string());
    fp.u128(doc.body_version(body).unwrap_or(0));
    for v in toward {
        fp.f64(v);
    }
    let key = fp.finish();
    if let Some(found) = doc.drawing_cache().silhouettes(key) {
        return found;
    }
    let mut out: Silhouettes = Vec::new();
    for face in &geometry.curved {
        if let SurfaceGeometry::Torus { axis, .. } = face.surface {
            let axis = mitcad_vec3::unit(axis).unwrap_or(axis);
            if mitcad_vec3::dot(axis, toward).abs() <= 1e-6 {
                continue;
            }
        }
        if let Ok(curves) = doc.kernel().face_silhouettes(shape, &face.face, toward)
            && !curves.is_empty()
        {
            out.push((face.face.clone(), curves));
        }
    }
    let out = Arc::new(out);
    doc.drawing_cache().keep_silhouettes(key, Arc::clone(&out));
    out
}

/// The curved face of this name among a body's, if it has one.
pub(super) fn curved_face<'g>(
    geometry: &'g BodyGeometry,
    face: &FaceName,
) -> Option<&'g CurvedFace> {
    geometry
        .curved
        .iter()
        .find(|c| c.face == *face || c.names.contains(face))
}

/// A model point (the drawing's coordinates) of a view point at a depth
/// along the view's direction to its eye.
pub(super) fn lift(spec: &ViewSpec, p: P2, depth: f64) -> Vec3 {
    let (x, y, z) = (spec.x_axis, spec.y_axis(), spec.toward_eye);
    [0, 1, 2].map(|k| x[k] * p[0] + y[k] * p[1] + z[k] * depth)
}

/// A direction of the view's plane in the drawing's coordinates.
pub(super) fn in_plane(spec: &ViewSpec, d: P2) -> Vec3 {
    lift(spec, d, 0.0)
}

/// How the view sees a direction of the drawing's coordinates: a unit
/// vector of its plane; None for its own direction.
pub(super) fn view_direction(spec: &ViewSpec, w: Vec3) -> Option<P2> {
    let d = spec.project(w);
    let length = len2(d);
    (length > 1e-9).then(|| [d[0] / length, d[1] / length])
}

/// A curve of the view's plane with its depth toward the eye along it.
#[derive(Debug, Clone)]
enum Curve2 {
    /// From `a` to `b` for parameters 0 to 1.
    Line { a: P2, b: P2, depths: [f64; 2] },
    /// `center + x cos t + y sin t` for t from `from` to `to`; its depth
    /// likewise from `depths` (the centre's, x's and y's).
    Conic {
        center: P2,
        x: P2,
        y: P2,
        depths: [f64; 3],
        from: f64,
        to: f64,
    },
    /// The curve's points and its depth (its points' x) on the same
    /// parameters.
    Spline { points: Spline2, depth: Spline2 },
}

impl Curve2 {
    fn domain(&self) -> (f64, f64) {
        match self {
            Self::Line { .. } => (0.0, 1.0),
            Self::Conic { from, to, .. } => (from.min(*to), from.max(*to)),
            Self::Spline { points, .. } => points.domain(),
        }
    }

    /// Its point and depth at a parameter.
    fn at(&self, u: f64) -> (P2, f64) {
        match self {
            Self::Line { a, b, depths } => (
                [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u],
                depths[0] + (depths[1] - depths[0]) * u,
            ),
            Self::Conic {
                center,
                x,
                y,
                depths,
                ..
            } => {
                let (s, c) = u.sin_cos();
                let p = [
                    center[0] + x[0] * c + y[0] * s,
                    center[1] + x[1] * c + y[1] * s,
                ];
                (p, depths[0] + depths[1] * c + depths[2] * s)
            }
            Self::Spline { points, depth } => (points.point(u), depth.point(u)[0]),
        }
    }
}

/// A silhouette as a view sees it (view mm).
#[derive(Debug, Clone)]
pub(super) struct SeenOutline {
    /// How a pick sees it whole: a line, or a circle or an arc seen end
    /// on; a curve of another kind as its chord.
    pub(super) seen: Seen,
    /// A line or a circle, picked whole as an edge is; a curve of another
    /// kind is picked by its extreme points only.
    pub(super) whole: bool,
    curve: Curve2,
    /// Its parameters, points and depths along it, from its start to its
    /// end, within the view's deflection.
    samples: Vec<(f64, P2, f64)>,
}

impl SeenOutline {
    /// How a view sees a silhouette given in its body's coordinates, the
    /// body placed by `transform` and the face's surface `surface`; None
    /// for a point or a curve seen as one.
    pub(super) fn of(
        curve: &Curve3,
        transform: &Transform,
        spec: &ViewSpec,
        surface: &SurfaceGeometry,
    ) -> Option<Self> {
        let eye = spec.toward_eye;
        let depth_of = |p: Vec3| mitcad_vec3::dot(p, eye);
        let mut circle: Option<(P2, f64)> = None;
        let curve2 = match curve {
            Curve3::Point(_) | Curve3::Helix { .. } => return None,
            Curve3::Line { start, end } => {
                let (s, e) = (transform.apply_point(*start), transform.apply_point(*end));
                Curve2::Line {
                    a: spec.project(s),
                    b: spec.project(e),
                    depths: [depth_of(s), depth_of(e)],
                }
            }
            Curve3::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                start,
                end,
                closed,
            } => {
                let c = transform.apply_point(*center);
                let x = transform.apply_vector(mitcad_vec3::scale(*x_axis, *major));
                let side = mitcad_vec3::cross(*normal, *x_axis);
                let y = transform.apply_vector(mitcad_vec3::scale(side, *minor));
                let facing = transform.apply_vector(*normal);
                let facing = mitcad_vec3::unit(facing).unwrap_or(eye);
                // A circle seen end on is itself.
                if (major - minor).abs() <= 1e-9 * major.max(1.0)
                    && mitcad_vec3::dot(facing, eye).abs() > 1.0 - 1e-6
                {
                    circle = Some((spec.project(c), *major));
                }
                Curve2::Conic {
                    center: spec.project(c),
                    x: spec.project(x),
                    y: spec.project(y),
                    depths: [depth_of(c), depth_of(x), depth_of(y)],
                    from: *start,
                    to: if *closed { *start + TAU } else { *end },
                }
            }
            Curve3::BSpline {
                degree,
                poles,
                weights,
                knots,
            } => {
                let placed: Vec<Vec3> = poles.iter().map(|p| transform.apply_point(*p)).collect();
                let spline = |poles: Vec<P2>| {
                    Spline2::of(&ViewSpline {
                        degree: *degree,
                        knots: knots.clone(),
                        poles,
                        weights: weights.clone(),
                        points: Vec::new(),
                    })
                };
                let seen: Vec<P2> = placed.iter().map(|p| spec.project(*p)).collect();
                let depths: Vec<P2> = placed.iter().map(|p| [depth_of(*p), 0.0]).collect();
                Curve2::Spline {
                    points: spline(seen)?,
                    depth: spline(depths)?,
                }
            }
        };
        let samples = sampled(&curve2, spec.deflection.clamp(1e-4, 0.05));
        let (first, last) = (samples.first()?.1, samples.last()?.1);
        if let Curve2::Line { a, b, .. } = curve2 {
            if len2(sub2(b, a)) < 1e-9 {
                return None;
            }
            return Some(Self {
                seen: Seen::Line(a, b),
                whole: true,
                curve: curve2,
                samples,
            });
        }
        // A torus's outline seen along its axis, a sphere's: their circles
        // (the kernel's curves approximate them).
        let circle = circle.or_else(|| {
            surface_circles(surface, transform, spec)
                .into_iter()
                .find(|(c, r)| on_circle(&samples, *c, *r))
        });
        let seen = match circle {
            Some((center, radius)) => Seen::Circle {
                center,
                radius,
                end_on: true,
                across: [1.0, 0.0],
                arc: arc_of(&samples, center, radius),
            },
            None if len2(sub2(last, first)) > 1e-9 => Seen::Line(first, last),
            None => Seen::Point(first),
        };
        Some(Self {
            seen,
            whole: circle.is_some(),
            curve: curve2,
            samples,
        })
    }

    /// Its middle point and depth: the sample halfway along it.
    pub(super) fn middle(&self) -> (P2, f64) {
        let (_, p, depth) = self.samples[self.samples.len() / 2];
        (p, depth)
    }

    /// How far a view point is from it, view mm.
    pub(super) fn distance(&self, q: P2) -> f64 {
        if self.whole {
            return self.seen.distance(q);
        }
        self.samples
            .windows(2)
            .map(|w| segment_distance(q, w[0].1, w[1].1))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether it lies on what a pick sees (an edge's line) within
    /// `tolerance` all along, view mm.
    pub(super) fn lies_on(&self, seen: &Seen, tolerance: f64) -> bool {
        let step = (self.samples.len() / 32).max(1);
        self.samples
            .iter()
            .step_by(step)
            .chain(self.samples.last())
            .all(|(_, p, _)| seen.distance(*p) <= tolerance)
    }

    /// Which way it runs at its point nearest a view point (unit): a
    /// line's or a circle's as a pick sees it, another curve's between the
    /// samples either side of its nearest.
    pub(super) fn along(&self, q: P2) -> Option<P2> {
        if self.whole {
            return self.seen.along(q);
        }
        let last = self.samples.len().checked_sub(1)?;
        let nearest = (0..=last).min_by(|i, k| {
            let (a, b) = (self.samples[*i].1, self.samples[*k].1);
            len2(sub2(a, q)).total_cmp(&len2(sub2(b, q)))
        })?;
        let (a, b) = (
            self.samples[nearest.saturating_sub(1)].1,
            self.samples[(nearest + 1).min(last)].1,
        );
        let d = sub2(b, a);
        let length = len2(d);
        (length > 1e-12).then(|| [d[0] / length, d[1] / length])
    }

    /// Its sample farthest from a view point and its nearest.
    pub(super) fn farthest_and_nearest(&self, f: P2) -> (P2, P2) {
        let distance = |p: P2| len2(sub2(p, f));
        let mut far = self.samples[0].1;
        let mut near = far;
        for (_, p, _) in &self.samples {
            if distance(*p) > distance(far) {
                far = *p;
            }
            if distance(*p) < distance(near) {
                near = *p;
            }
        }
        (far, near)
    }

    /// Its point furthest along a direction of the view (a unit vector),
    /// with its depth: a line's or a circle's as a pick sees it, another
    /// curve's found on the curve about its farthest sample.
    pub(super) fn extreme(&self, d: P2) -> (P2, f64) {
        let along = |p: P2| dot2(p, d);
        let depth_near = |q: P2| {
            self.samples
                .iter()
                .min_by(|a, b| len2(sub2(a.1, q)).total_cmp(&len2(sub2(b.1, q))))
                .map_or(0.0, |s| s.2)
        };
        if self.whole {
            let p = self.seen.extreme(d);
            return match self.curve {
                Curve2::Line { a, b, depths } => {
                    let ab = sub2(b, a);
                    let t = (dot2(sub2(p, a), ab) / dot2(ab, ab).max(1e-24)).clamp(0.0, 1.0);
                    (p, depths[0] + (depths[1] - depths[0]) * t)
                }
                _ => (p, depth_near(p)),
            };
        }
        let key = |i: &usize| along(self.samples[*i].1);
        let best = (0..self.samples.len())
            .max_by(|i, k| key(i).total_cmp(&key(k)))
            .unwrap_or(0);
        let lo = self.samples[best.saturating_sub(1)].0;
        let hi = self.samples[(best + 1).min(self.samples.len() - 1)].0;
        let u = most_along(&self.curve, d, lo.min(hi), lo.max(hi));
        let found = self.curve.at(u);
        let sample = (self.samples[best].1, self.samples[best].2);
        if along(found.0) >= along(sample.0) {
            found
        } else {
            sample
        }
    }
}

/// Points along a curve within `deflection` of it, with their parameters
/// and depths, from its start to its end.
fn sampled(curve: &Curve2, deflection: f64) -> Vec<(f64, P2, f64)> {
    let (lo, hi) = curve.domain();
    let parameters: Vec<f64> = match curve {
        Curve2::Line { .. } => vec![0.0, 1.0],
        // Every half degree.
        Curve2::Conic { .. } => {
            let n = (((hi - lo) / (TAU / 720.0)).ceil() as usize).max(8);
            let step = (hi - lo) / n as f64;
            (0..=n).map(|i| lo + step * i as f64).collect()
        }
        Curve2::Spline { points, .. } => {
            let found = points.samples(lo, hi, deflection);
            found.into_iter().map(|(u, _)| u).collect()
        }
    };
    parameters
        .into_iter()
        .map(|u| {
            let (p, depth) = curve.at(u);
            (u, p, depth)
        })
        .collect()
}

/// The parameter between `lo` and `hi` where a curve lies furthest along
/// a direction (golden section search: the samples either side of the
/// farthest one bound a single top).
fn most_along(curve: &Curve2, d: P2, lo: f64, hi: f64) -> f64 {
    let f = |u: f64| dot2(curve.at(u).0, d);
    let g = (5.0_f64.sqrt() - 1.0) / 2.0;
    let (mut a, mut b) = (lo, hi);
    let mut c = b - g * (b - a);
    let mut e = a + g * (b - a);
    let (mut fc, mut fe) = (f(c), f(e));
    for _ in 0..90 {
        if b - a <= 1e-15 * (a.abs() + b.abs()).max(1.0) {
            break;
        }
        if fc > fe {
            b = e;
            e = c;
            fe = fc;
            c = b - g * (b - a);
            fc = f(c);
        } else {
            a = c;
            c = e;
            fc = fe;
            e = a + g * (b - a);
            fe = f(e);
        }
    }
    (a + b) / 2.0
}

/// The circles a surface's silhouettes are, as a view sees them: a
/// torus's two seen along its axis (its tube's outer and inner sides), a
/// sphere's one; centre and radius, view mm.
fn surface_circles(
    surface: &SurfaceGeometry,
    transform: &Transform,
    spec: &ViewSpec,
) -> Vec<(P2, f64)> {
    match *surface {
        SurfaceGeometry::Torus {
            center,
            axis,
            major_radius,
            minor_radius,
        } => {
            let axis = transform.apply_vector(axis);
            let axis = mitcad_vec3::unit(axis).unwrap_or(axis);
            if mitcad_vec3::dot(axis, spec.toward_eye).abs() <= 1.0 - 1e-6 {
                return Vec::new();
            }
            let c = spec.project(transform.apply_point(center));
            let mut circles = vec![(c, major_radius + minor_radius)];
            if major_radius > minor_radius {
                circles.push((c, major_radius - minor_radius));
            }
            circles
        }
        SurfaceGeometry::Sphere { center, radius } => {
            vec![(spec.project(transform.apply_point(center)), radius)]
        }
        _ => Vec::new(),
    }
}

/// Whether points lie on a circle, within the kernel's approximation of an
/// outline.
fn on_circle(samples: &[(f64, P2, f64)], center: P2, radius: f64) -> bool {
    let tolerance = (1e-3_f64).max(1e-4 * radius);
    let off = |p: P2| (len2(sub2(p, center)) - radius).abs();
    !samples.is_empty() && samples.iter().all(|(_, p, _)| off(*p) <= tolerance)
}

/// The arc of a circle that points along it cover, counter-clockwise from
/// its first angle to its second (view radians); None for the whole
/// circle.
fn arc_of(samples: &[(f64, P2, f64)], center: P2, radius: f64) -> Option<(f64, f64)> {
    let angle = |p: P2| (p[1] - center[1]).atan2(p[0] - center[0]);
    let (first, last) = (samples.first()?.1, samples.last()?.1);
    let mut turned = 0.0;
    for w in samples.windows(2) {
        let mut step = angle(w[1].1) - angle(w[0].1);
        if step > PI {
            step -= TAU;
        } else if step < -PI {
            step += TAU;
        }
        turned += step;
    }
    let closed = len2(sub2(last, first)) <= (1e-3_f64).max(1e-4 * radius) && samples.len() > 2;
    if closed || turned.abs() >= TAU - 1e-9 {
        return None;
    }
    Some(if turned >= 0.0 {
        (angle(first), angle(first) + turned)
    } else {
        (angle(last), angle(last) - turned)
    })
}
