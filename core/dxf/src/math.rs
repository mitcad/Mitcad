// SPDX-License-Identifier: MIT
//! Plane geometry: affine transforms of entities and polyline bulges.

use std::f64::consts::{FRAC_PI_2, TAU};

use crate::types::{Geometry, Point2, Text, normalize_range};

/// A 2D affine transform: `x' = a x + b y + tx`, `y' = c x + d y + ty`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn translation(x: f64, y: f64) -> Self {
        Self {
            tx: x,
            ty: y,
            ..Self::IDENTITY
        }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Rotation by `angle` radians counter-clockwise about the origin.
    pub fn rotation(angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            a: c,
            b: -s,
            c: s,
            d: c,
            ..Self::IDENTITY
        }
    }

    /// `self` applied after `first`.
    pub fn then_after(&self, first: &Self) -> Self {
        Self {
            a: self.a * first.a + self.b * first.c,
            b: self.a * first.b + self.b * first.d,
            c: self.c * first.a + self.d * first.c,
            d: self.c * first.b + self.d * first.d,
            tx: self.a * first.tx + self.b * first.ty + self.tx,
            ty: self.c * first.tx + self.d * first.ty + self.ty,
        }
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn apply(&self, p: Point2) -> Point2 {
        Point2::new(
            self.a * p.x + self.b * p.y + self.tx,
            self.c * p.x + self.d * p.y + self.ty,
        )
    }

    /// The linear part applied to a vector.
    pub fn apply_vector(&self, v: Point2) -> Point2 {
        Point2::new(self.a * v.x + self.b * v.y, self.c * v.x + self.d * v.y)
    }

    /// Uniform scale factor when the transform is a similarity (rotation,
    /// uniform scale, mirror, translation).
    fn similarity_scale(&self) -> Option<f64> {
        let u = self.apply_vector(Point2::new(1.0, 0.0));
        let v = self.apply_vector(Point2::new(0.0, 1.0));
        let (lu, lv) = (u.length(), v.length());
        let tolerance = 1e-12 * lu.max(lv).max(1.0);
        ((lu - lv).abs() <= tolerance && u.dot(v).abs() <= tolerance * lu.max(1.0)).then_some(lu)
    }

    pub(crate) fn apply_geometry(&self, geometry: &Geometry) -> Geometry {
        match geometry {
            Geometry::Point(p) => Geometry::Point(self.apply(*p)),
            Geometry::Line { start, end } => Geometry::Line {
                start: self.apply(*start),
                end: self.apply(*end),
            },
            Geometry::Circle { center, radius } => match self.similarity_scale() {
                Some(scale) => Geometry::Circle {
                    center: self.apply(*center),
                    radius: radius * scale,
                },
                None => ellipse_from_conjugate(
                    self.apply(*center),
                    self.apply_vector(Point2::new(*radius, 0.0)),
                    self.apply_vector(Point2::new(0.0, *radius)),
                    0.0,
                    TAU,
                ),
            },
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => match self.similarity_scale() {
                Some(scale) => {
                    let sweep = end_angle - start_angle;
                    let new_center = self.apply(*center);
                    // A mirror reverses the direction: the old end becomes the start.
                    let from = if self.determinant() < 0.0 {
                        *end_angle
                    } else {
                        *start_angle
                    };
                    let start =
                        (self.apply(*center + Point2::polar(from) * *radius) - new_center).angle();
                    Geometry::arc(new_center, radius * scale, start, start + sweep)
                }
                None => ellipse_from_conjugate(
                    self.apply(*center),
                    self.apply_vector(Point2::new(*radius, 0.0)),
                    self.apply_vector(Point2::new(0.0, *radius)),
                    *start_angle,
                    *end_angle,
                ),
            },
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => ellipse_from_conjugate(
                self.apply(*center),
                self.apply_vector(*major_axis),
                self.apply_vector(major_axis.perp() * *ratio),
                *start_param,
                *end_param,
            ),
            Geometry::Spline(spline) => {
                let mut spline = spline.clone();
                for p in spline
                    .control_points
                    .iter_mut()
                    .chain(spline.fit_points.iter_mut())
                {
                    *p = self.apply(*p);
                }
                Geometry::Spline(spline)
            }
            Geometry::Text(text) => {
                let direction = self.apply_vector(Point2::polar(text.rotation));
                let up = self.apply_vector(Point2::polar(text.rotation + FRAC_PI_2));
                // Height along the text's up direction, perpendicular to the baseline.
                let height = if direction.length() > 0.0 {
                    text.height * direction.cross(up).abs() / direction.length()
                } else {
                    text.height * up.length()
                };
                Geometry::Text(Text {
                    position: self.apply(text.position),
                    height,
                    rotation: direction.angle().rem_euclid(TAU),
                    content: text.content.clone(),
                })
            }
        }
    }
}

/// The ellipse `center + a cos(t) + b sin(t)` for t from `t1` to `t2`, with
/// any conjugate semi-diameters `a` and `b`, in the normal form of
/// [`Geometry::Ellipse`] (perpendicular axes, counter-clockwise parameter).
/// Equal axes give a circle or an arc.
pub(crate) fn ellipse_from_conjugate(
    center: Point2,
    a: Point2,
    b: Point2,
    t1: f64,
    t2: f64,
) -> Geometry {
    // Shift the parameter by t0 so that the semi-diameters are perpendicular:
    // P = C + u cos(t - t0) + v sin(t - t0).
    let t0 = 0.5 * (2.0 * a.dot(b)).atan2(a.dot(a) - b.dot(b));
    let (s0, c0) = t0.sin_cos();
    let mut u = a * c0 + b * s0;
    let mut v = b * c0 - a * s0;
    let mut shift = t0;
    if v.length() > u.length() {
        // Make u the major axis: t - t0 = s + pi/2.
        let (old_u, old_v) = (u, v);
        u = old_v;
        v = -old_u;
        shift += FRAC_PI_2;
    }
    let full = (t2 - t1 - TAU).abs() < 1e-12;
    // A clockwise parametrisation (v is the clockwise normal of u) is
    // reversed: s = -(t - shift).
    let (start, end) = if u.cross(v) >= 0.0 {
        (t1 - shift, t2 - shift)
    } else {
        v = -v;
        (shift - t2, shift - t1)
    };
    let ratio = if u.length() > 0.0 {
        v.length() / u.length()
    } else {
        1.0
    };
    if (ratio - 1.0).abs() < 1e-12 {
        let radius = u.length();
        if full {
            return Geometry::Circle { center, radius };
        }
        let angle = u.angle();
        return Geometry::arc(center, radius, start + angle, end + angle);
    }
    let (start_param, end_param) = if full {
        (0.0, TAU)
    } else {
        normalize_range(start, end)
    };
    Geometry::Ellipse {
        center,
        major_axis: u,
        ratio,
        start_param,
        end_param,
    }
}

/// The arc of a polyline segment from `start` to `end` with bulge `bulge`
/// (tan of a quarter of the included angle; positive is counter-clockwise),
/// or a line for a zero bulge.
pub fn bulge_arc(start: Point2, end: Point2, bulge: f64) -> Geometry {
    let chord = end - start;
    let length = chord.length();
    if bulge.abs() < 1e-12 || length == 0.0 {
        return Geometry::Line { start, end };
    }
    // Signed distance from the chord midpoint to the centre, to the left of
    // the chord: (c / 2) / tan(theta / 2) with tan(theta / 2) = 2b / (1 - b^2).
    let offset = length * (1.0 - bulge * bulge) / (4.0 * bulge);
    let center = (start + end) * 0.5 + chord.perp() * (offset / length);
    let radius = length * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    let from = if bulge > 0.0 { start } else { end };
    let start_angle = (from - center).angle();
    let sweep = 4.0 * bulge.abs().atan();
    Geometry::arc(center, radius, start_angle, start_angle + sweep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_4, PI};

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near_point(a: Point2, b: Point2) -> bool {
        near(a.x, b.x) && near(a.y, b.y)
    }

    fn arc_parts(g: &Geometry) -> (Point2, f64, f64, f64) {
        match *g {
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => (center, radius, start_angle, end_angle),
            ref other => panic!("not an arc: {other:?}"),
        }
    }

    #[test]
    fn quarter_circle_bulge() {
        let b = (PI / 8.0).tan();
        let (c, r, s, e) = arc_parts(&bulge_arc(Point2::new(1.0, 0.0), Point2::new(0.0, 1.0), b));
        assert!(near_point(c, Point2::new(0.0, 0.0)));
        assert!(near(r, 1.0));
        assert!(near(s, 0.0));
        assert!(near(e, FRAC_PI_2));
    }

    #[test]
    fn negative_bulge_is_clockwise() {
        // Clockwise from (1, 0) to (0, 1) around (1, 1): counter-clockwise
        // from (0, 1) to (1, 0), angles pi to 3pi/2.
        let b = -(PI / 8.0).tan();
        let (c, r, s, e) = arc_parts(&bulge_arc(Point2::new(1.0, 0.0), Point2::new(0.0, 1.0), b));
        assert!(near_point(c, Point2::new(1.0, 1.0)));
        assert!(near(r, 1.0));
        assert!(near(s, PI));
        assert!(near(e, 1.5 * PI));
    }

    #[test]
    fn semicircle_and_large_bulges() {
        // Bulge 1: half circle below the chord for a counter-clockwise turn.
        let (c, r, s, e) = arc_parts(&bulge_arc(
            Point2::new(0.0, 0.0),
            Point2::new(2.0, 0.0),
            1.0,
        ));
        assert!(near_point(c, Point2::new(1.0, 0.0)));
        assert!(near(r, 1.0));
        assert!(near(s, PI));
        assert!(near(e, TAU));
        // Bulge tan(3pi/8): 270 degree arc, centre on the other side.
        let b = (3.0 * PI / 8.0).tan();
        let (c, r, s, e) = arc_parts(&bulge_arc(Point2::new(1.0, 0.0), Point2::new(0.0, -1.0), b));
        assert!(near_point(c, Point2::new(0.0, 0.0)));
        assert!(near(r, 1.0));
        assert!(near(s, 0.0));
        assert!(near(e, 1.5 * PI));
    }

    #[test]
    fn zero_bulge_is_a_line() {
        let g = bulge_arc(Point2::new(0.0, 0.0), Point2::new(3.0, 4.0), 0.0);
        assert_eq!(
            g,
            Geometry::Line {
                start: Point2::new(0.0, 0.0),
                end: Point2::new(3.0, 4.0)
            }
        );
    }

    #[test]
    fn composition_order() {
        let move_then_rotate =
            Affine::rotation(FRAC_PI_2).then_after(&Affine::translation(1.0, 0.0));
        assert!(near_point(
            move_then_rotate.apply(Point2::new(0.0, 0.0)),
            Point2::new(0.0, 1.0)
        ));
    }

    #[test]
    fn mirrored_arc_stays_counter_clockwise() {
        let arc = Geometry::arc(Point2::new(0.0, 0.0), 2.0, 0.0, FRAC_PI_2);
        let mirrored = arc.transformed(&Affine::scale(-1.0, 1.0));
        let (c, r, s, e) = arc_parts(&mirrored);
        assert!(near_point(c, Point2::new(0.0, 0.0)));
        assert!(near(r, 2.0));
        assert!(near(s, FRAC_PI_2));
        assert!(near(e, PI));
    }

    #[test]
    fn non_uniform_scale_makes_an_ellipse() {
        let circle = Geometry::Circle {
            center: Point2::new(1.0, 1.0),
            radius: 1.0,
        };
        match circle.transformed(&Affine::scale(1.0, 3.0)) {
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => {
                assert!(near_point(center, Point2::new(1.0, 3.0)));
                assert!(near(major_axis.length(), 3.0));
                assert!(near(major_axis.x, 0.0));
                assert!(near(ratio, 1.0 / 3.0));
                assert!(near(start_param, 0.0));
                assert!(near(end_param, TAU));
            }
            other => panic!("not an ellipse: {other:?}"),
        }
    }

    #[test]
    fn transformed_ellipse_arc_keeps_its_points() {
        let ellipse = Geometry::Ellipse {
            center: Point2::new(2.0, -1.0),
            major_axis: Point2::new(3.0, 1.0),
            ratio: 0.4,
            start_param: 0.3,
            end_param: 2.0,
        };
        let transforms = [
            Affine::rotation(0.7).then_after(&Affine::scale(2.0, -0.5)),
            Affine::scale(-1.0, 1.0),
            Affine {
                a: 1.0,
                b: 0.8,
                c: 0.1,
                d: 1.3,
                tx: 4.0,
                ty: 2.0,
            },
        ];
        for transform in transforms {
            let mapped = ellipse.transformed(&transform);
            let original_points = sample(&ellipse);
            let mapped_points = sample(&mapped);
            // The same arc: end points match (as a set) and each sample of the
            // mapped curve lies on the image of the original.
            let ends = [
                transform.apply(original_points[0]),
                transform.apply(*original_points.last().unwrap()),
            ];
            let mapped_ends = [mapped_points[0], *mapped_points.last().unwrap()];
            assert!(
                (near_point(ends[0], mapped_ends[0]) && near_point(ends[1], mapped_ends[1]))
                    || (near_point(ends[0], mapped_ends[1]) && near_point(ends[1], mapped_ends[0])),
                "{transform:?}: {ends:?} vs {mapped_ends:?}"
            );
            let dense: Vec<Point2> = sample_n(&ellipse, 2000)
                .into_iter()
                .map(|p| transform.apply(p))
                .collect();
            for p in mapped_points {
                let best = dense
                    .iter()
                    .map(|q| q.distance(p))
                    .fold(f64::INFINITY, f64::min);
                assert!(best < 1e-2, "{transform:?}: {p:?} off the curve by {best}");
            }
        }
    }

    fn sample(g: &Geometry) -> Vec<Point2> {
        sample_n(g, 16)
    }

    fn sample_n(g: &Geometry, n: usize) -> Vec<Point2> {
        match *g {
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => (0..=n)
                .map(|i| {
                    let t = start_param + (end_param - start_param) * i as f64 / n as f64;
                    center + major_axis * t.cos() + major_axis.perp() * (ratio * t.sin())
                })
                .collect(),
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => (0..=n)
                .map(|i| {
                    let t = start_angle + (end_angle - start_angle) * i as f64 / n as f64;
                    center + Point2::polar(t) * radius
                })
                .collect(),
            ref other => panic!("cannot sample {other:?}"),
        }
    }

    #[test]
    fn rotated_text() {
        let text = Geometry::Text(Text {
            position: Point2::new(1.0, 0.0),
            height: 2.5,
            rotation: 0.0,
            content: "A".into(),
        });
        match text.transformed(&Affine::rotation(FRAC_PI_4).then_after(&Affine::scale(2.0, 2.0))) {
            Geometry::Text(t) => {
                assert!(near(t.height, 5.0));
                assert!(near(t.rotation, FRAC_PI_4));
                assert!(near_point(
                    t.position,
                    Point2::new(2.0_f64.sqrt(), 2.0_f64.sqrt())
                ));
            }
            other => panic!("{other:?}"),
        }
    }
}
