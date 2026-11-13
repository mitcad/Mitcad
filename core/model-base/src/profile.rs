// SPDX-License-Identifier: MIT
//! Evaluated sketch geometry handed to the kernel: profile regions bounded
//! by loops of named curve segments, placed in 3D by a sketch frame.
//! Millimetres and radians, sketch coordinates in the frame.

use crate::topo::{RegionKey, SegmentKey};
use serde::{Deserialize, Serialize};

/// Places sketch coordinates in 3D: p = origin + u x_axis + v y_axis. The
/// axes are unit vectors at right angles; the normal is x_axis × y_axis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SketchFrame {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

impl SketchFrame {
    /// The XY origin plane.
    pub const XY: Self = Self {
        origin: [0.0, 0.0, 0.0],
        x_axis: [1.0, 0.0, 0.0],
        y_axis: [0.0, 1.0, 0.0],
    };

    pub fn normal(&self) -> [f64; 3] {
        cross(self.x_axis, self.y_axis)
    }

    /// The 3D point of sketch coordinates.
    pub fn point(&self, [u, v]: [f64; 2]) -> [f64; 3] {
        std::array::from_fn(|i| self.origin[i] + u * self.x_axis[i] + v * self.y_axis[i])
    }
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

// The vector helpers live in `mitcad-vec3`; the modules that take their cross
// and dot products from here keep this path.
#[doc(hidden)]
pub use mitcad_vec3::{cross, dot};

/// The curve of a segment. Arcs run counter-clockwise from the start to the
/// end angle; ellipse angles are parameter angles from the major axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SegmentGeometry {
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    Circle {
        center: [f64; 2],
        radius: f64,
        /// Where the circle starts, counter-clockwise from the sketch x
        /// axis (radians); absent (zero) for a circle that starts on it.
        #[serde(default, skip_serializing_if = "is_zero")]
        start: f64,
    },
    Ellipse {
        center: [f64; 2],
        major_radius: f64,
        minor_radius: f64,
        /// Angle of the major axis from the sketch x axis.
        rotation: f64,
    },
    EllipseArc {
        center: [f64; 2],
        major_radius: f64,
        minor_radius: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
    },
    BSpline {
        degree: u32,
        poles: Vec<[f64; 2]>,
        /// Empty for a non-rational curve.
        weights: Vec<f64>,
        /// Distinct knots with their multiplicities.
        knots: Vec<f64>,
        multiplicities: Vec<u32>,
        periodic: bool,
    },
}

impl SegmentGeometry {
    /// Points on the curve (ends and samples), for bounds.
    fn sample(&self) -> Vec<[f64; 2]> {
        let on_ellipse = |center: [f64; 2], a: f64, b: f64, rotation: f64, t: f64| {
            let (x, y) = (a * t.cos(), b * t.sin());
            [
                center[0] + x * rotation.cos() - y * rotation.sin(),
                center[1] + x * rotation.sin() + y * rotation.cos(),
            ]
        };
        let angles = |start: f64, end: f64| {
            let end = if end <= start {
                end + std::f64::consts::TAU
            } else {
                end
            };
            (0..=16).map(move |i| start + (end - start) * f64::from(i) / 16.0)
        };
        match self {
            Self::Line { start, end } => vec![*start, *end],
            Self::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => angles(*start_angle, *end_angle)
                .map(|t| on_ellipse(*center, *radius, *radius, 0.0, t))
                .collect(),
            Self::Circle { center, radius, .. } => angles(0.0, std::f64::consts::TAU)
                .map(|t| on_ellipse(*center, *radius, *radius, 0.0, t))
                .collect(),
            Self::Ellipse {
                center,
                major_radius,
                minor_radius,
                rotation,
            } => angles(0.0, std::f64::consts::TAU)
                .map(|t| on_ellipse(*center, *major_radius, *minor_radius, *rotation, t))
                .collect(),
            Self::EllipseArc {
                center,
                major_radius,
                minor_radius,
                rotation,
                start_angle,
                end_angle,
            } => angles(*start_angle, *end_angle)
                .map(|t| on_ellipse(*center, *major_radius, *minor_radius, *rotation, t))
                .collect(),
            // The control polygon contains the curve.
            Self::BSpline { poles, .. } => poles.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileSegment {
    pub key: SegmentKey,
    pub geometry: SegmentGeometry,
}

/// A closed boundary, segments in order around it.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileLoop {
    pub segments: Vec<ProfileSegment>,
}

/// A planar region of a sketch: the first loop is the outer boundary, the
/// others are holes.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRegion {
    pub key: RegionKey,
    pub loops: Vec<ProfileLoop>,
}

impl ProfileRegion {
    /// Whether an implicit supporting-face edge participates in a loop.
    pub fn face_bounded(&self) -> bool {
        self.loops
            .iter()
            .flat_map(|l| &l.segments)
            .any(|s| s.key.curve.is_face_edge())
    }

    /// Approximate bounds in sketch coordinates (minimum, maximum).
    pub fn bounds(&self) -> ([f64; 2], [f64; 2]) {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for segment in self.loops.iter().flat_map(|l| &l.segments) {
            for p in segment.geometry.sample() {
                for i in 0..2 {
                    min[i] = min[i].min(p[i]);
                    max[i] = max[i].max(p[i]);
                }
            }
        }
        (min, max)
    }

    /// Each loop as a closed polygon of sampled points in sketch
    /// coordinates (mitcad#1587): its segments joined end to end, each
    /// turned round where its own direction runs against the loop's.
    pub fn polygons(&self) -> Vec<Vec<[f64; 2]>> {
        let distance = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        self.loops
            .iter()
            .map(|l| {
                let mut points: Vec<[f64; 2]> = Vec::new();
                for (i, segment) in l.segments.iter().enumerate() {
                    let mut samples = segment.geometry.sample();
                    let (Some(&first), Some(&last)) = (samples.first(), samples.last()) else {
                        continue;
                    };
                    let reverse = match points.last() {
                        Some(&end) => distance(end, last) < distance(end, first),
                        // The first segment runs toward the second.
                        None => l.segments.get(i + 1).is_some_and(|next| {
                            let next = next.geometry.sample();
                            let near = |p: [f64; 2]| {
                                next.first()
                                    .zip(next.last())
                                    .map_or(f64::INFINITY, |(a, b)| {
                                        distance(p, *a).min(distance(p, *b))
                                    })
                            };
                            near(first) < near(last)
                        }),
                    };
                    if reverse {
                        samples.reverse();
                    }
                    points.extend(samples);
                }
                points
            })
            .collect()
    }

    /// The region's area (mm², the outer loop less the holes) and centre
    /// of area in sketch coordinates, from sampled loops (mitcad#1587).
    pub fn area_and_centre(&self) -> (f64, [f64; 2]) {
        let mut area = 0.0;
        let mut moment = [0.0; 2];
        for (i, polygon) in self.polygons().iter().enumerate() {
            let (mut a, mut mx, mut my) = (0.0, 0.0, 0.0);
            for (j, p) in polygon.iter().enumerate() {
                let q = polygon[(j + 1) % polygon.len()];
                let cross = p[0] * q[1] - q[0] * p[1];
                a += cross;
                mx += (p[0] + q[0]) * cross;
                my += (p[1] + q[1]) * cross;
            }
            // Outer loop counts positive, holes negative, whatever their
            // turning direction.
            let sign = if (i == 0) == (a >= 0.0) { 1.0 } else { -1.0 };
            area += sign * a / 2.0;
            moment[0] += sign * mx / 6.0;
            moment[1] += sign * my / 6.0;
        }
        if area.abs() < 1e-12 {
            return (0.0, [0.0; 2]);
        }
        (area, [moment[0] / area, moment[1] / area])
    }

    /// The length of all its loops (mm, the outer boundary and the holes;
    /// mitcad#1741: the selection card's perimeter): lines, circles and
    /// circular arcs exactly, ellipses and elliptical arcs to a few parts
    /// in a million; None when a loop has a spline.
    pub fn perimeter(&self) -> Option<f64> {
        let distance = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
        // Counter-clockwise from start to end, as the samples run.
        let sweep = |start: f64, end: f64| {
            if end <= start {
                end + std::f64::consts::TAU - start
            } else {
                end - start
            }
        };
        // Along the ellipse in many short chords.
        let elliptic = |a: f64, b: f64, start: f64, sweep: f64| {
            const STEPS: u32 = 1024;
            let at = |t: f64| [a * t.cos(), b * t.sin()];
            (0..STEPS)
                .map(|i| {
                    let t0 = start + sweep * f64::from(i) / f64::from(STEPS);
                    let t1 = start + sweep * f64::from(i + 1) / f64::from(STEPS);
                    distance(at(t0), at(t1))
                })
                .sum::<f64>()
        };
        let mut total = 0.0;
        for segment in self.loops.iter().flat_map(|l| &l.segments) {
            total += match &segment.geometry {
                SegmentGeometry::Line { start, end } => distance(*start, *end),
                SegmentGeometry::Circle { radius, .. } => std::f64::consts::TAU * radius.abs(),
                SegmentGeometry::Arc {
                    radius,
                    start_angle,
                    end_angle,
                    ..
                } => radius.abs() * sweep(*start_angle, *end_angle),
                SegmentGeometry::Ellipse {
                    major_radius,
                    minor_radius,
                    ..
                } => elliptic(*major_radius, *minor_radius, 0.0, std::f64::consts::TAU),
                SegmentGeometry::EllipseArc {
                    major_radius,
                    minor_radius,
                    start_angle,
                    end_angle,
                    ..
                } => elliptic(
                    *major_radius,
                    *minor_radius,
                    *start_angle,
                    sweep(*start_angle, *end_angle),
                ),
                SegmentGeometry::BSpline { .. } => return None,
            };
        }
        Some(total)
    }

    /// Whether a point in sketch coordinates lies inside the region (in
    /// its outer loop and outside its holes, by the sampled loops;
    /// mitcad#1587).
    pub fn contains(&self, point: [f64; 2]) -> bool {
        let mut inside = false;
        for polygon in self.polygons() {
            for (j, p) in polygon.iter().enumerate() {
                let q = polygon[(j + 1) % polygon.len()];
                if (p[1] > point[1]) != (q[1] > point[1]) {
                    let x = p[0] + (point[1] - p[1]) / (q[1] - p[1]) * (q[0] - p[0]);
                    if point[0] < x {
                        inside = !inside;
                    }
                }
            }
        }
        inside
    }

    /// The region turned by `angle` (radians, counter-clockwise) about
    /// `centre` in sketch coordinates (mitcad#1259: an emboss's Rotation);
    /// its keys unchanged.
    pub fn rotated(&self, centre: [f64; 2], angle: f64) -> ProfileRegion {
        ProfileRegion {
            key: self.key.clone(),
            loops: self
                .loops
                .iter()
                .map(|l| ProfileLoop {
                    segments: l
                        .segments
                        .iter()
                        .map(|s| ProfileSegment {
                            key: s.key.clone(),
                            geometry: s.geometry.rotated(centre, angle),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

impl SegmentGeometry {
    /// The curve turned by `angle` (radians, counter-clockwise) about
    /// `centre`: points rotated, angles from the sketch x axis grown by
    /// `angle`; ellipse parameter angles (from the major axis) and B-spline
    /// weights and knots stay.
    pub fn rotated(&self, centre: [f64; 2], angle: f64) -> SegmentGeometry {
        let (sin, cos) = angle.sin_cos();
        let turn = |p: [f64; 2]| {
            let (x, y) = (p[0] - centre[0], p[1] - centre[1]);
            [centre[0] + x * cos - y * sin, centre[1] + x * sin + y * cos]
        };
        match self {
            Self::Line { start, end } => Self::Line {
                start: turn(*start),
                end: turn(*end),
            },
            Self::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => Self::Arc {
                center: turn(*center),
                radius: *radius,
                start_angle: start_angle + angle,
                end_angle: end_angle + angle,
            },
            Self::Circle {
                center,
                radius,
                start,
            } => Self::Circle {
                center: turn(*center),
                radius: *radius,
                start: start + angle,
            },
            Self::Ellipse {
                center,
                major_radius,
                minor_radius,
                rotation,
            } => Self::Ellipse {
                center: turn(*center),
                major_radius: *major_radius,
                minor_radius: *minor_radius,
                rotation: rotation + angle,
            },
            Self::EllipseArc {
                center,
                major_radius,
                minor_radius,
                rotation,
                start_angle,
                end_angle,
            } => Self::EllipseArc {
                center: turn(*center),
                major_radius: *major_radius,
                minor_radius: *minor_radius,
                rotation: rotation + angle,
                start_angle: *start_angle,
                end_angle: *end_angle,
            },
            Self::BSpline {
                degree,
                poles,
                weights,
                knots,
                multiplicities,
                periodic,
            } => Self::BSpline {
                degree: *degree,
                poles: poles.iter().map(|p| turn(*p)).collect(),
                weights: weights.clone(),
                knots: knots.clone(),
                multiplicities: multiplicities.clone(),
                periodic: *periodic,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_place_sketch_points() {
        assert_eq!(SketchFrame::XY.normal(), [0.0, 0.0, 1.0]);
        let xz = SketchFrame {
            origin: [0.0, 5.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 0.0, 1.0],
        };
        assert_eq!(xz.normal(), [0.0, -1.0, 0.0]);
        assert_eq!(xz.point([2.0, 3.0]), [2.0, 5.0, 3.0]);
    }

    #[test]
    fn region_bounds_include_arcs() {
        let arc = SegmentGeometry::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            start_angle: std::f64::consts::FRAC_PI_2,
            end_angle: 3.0 * std::f64::consts::FRAC_PI_2,
        };
        let region = ProfileRegion {
            key: "r{c1[c2,c2],c2[c1,c1]}".parse().unwrap(),
            loops: vec![ProfileLoop {
                segments: vec![ProfileSegment {
                    key: "c2[c1,c1]".parse().unwrap(),
                    geometry: arc,
                }],
            }],
        };
        let (min, max) = region.bounds();
        assert!((min[0] + 10.0).abs() < 1e-9 && max[0].abs() < 1e-9);
        assert!((min[1] + 10.0).abs() < 1e-9 && (max[1] - 10.0).abs() < 1e-9);
    }

    /// mitcad#1741: the selection card's perimeter, a half disc's arc and
    /// diameter exactly, an ellipse to a few parts in a million, none with
    /// a spline.
    #[test]
    fn region_perimeters() {
        let half = std::f64::consts::FRAC_PI_2;
        let segment = |key: &str, geometry| ProfileSegment {
            key: key.parse().unwrap(),
            geometry,
        };
        let mut region = ProfileRegion {
            key: "r{c1[c2,c2],c2[c1,c1]}".parse().unwrap(),
            loops: vec![ProfileLoop {
                segments: vec![
                    segment(
                        "c2[c1,c1]",
                        SegmentGeometry::Arc {
                            center: [0.0, 0.0],
                            radius: 10.0,
                            start_angle: half,
                            end_angle: 3.0 * half,
                        },
                    ),
                    segment(
                        "c1[c2,c2]",
                        SegmentGeometry::Line {
                            start: [0.0, -10.0],
                            end: [0.0, 10.0],
                        },
                    ),
                ],
            }],
        };
        let expected = 10.0 * std::f64::consts::PI + 20.0;
        assert!((region.perimeter().unwrap() - expected).abs() < 1e-9);
        // An arc that runs past the x axis (its end angle below its start).
        region.loops[0].segments[0].geometry = SegmentGeometry::Arc {
            center: [0.0, 0.0],
            radius: 10.0,
            start_angle: 3.0 * half,
            end_angle: half,
        };
        assert!((region.perimeter().unwrap() - expected).abs() < 1e-9);
        // A circle of radius 2 as a hole: both loops count.
        region.loops.push(ProfileLoop {
            segments: vec![segment(
                "c3",
                SegmentGeometry::Circle {
                    center: [-4.0, 0.0],
                    radius: 2.0,
                    start: 0.0,
                },
            )],
        });
        let with_hole = expected + 4.0 * std::f64::consts::PI;
        assert!((region.perimeter().unwrap() - with_hole).abs() < 1e-9);
        // An ellipse with equal radii is a circle.
        region.loops[1].segments[0].geometry = SegmentGeometry::Ellipse {
            center: [-4.0, 0.0],
            major_radius: 2.0,
            minor_radius: 2.0,
            rotation: 0.3,
        };
        assert!((region.perimeter().unwrap() - with_hole).abs() < 1e-4);
        region.loops[1].segments[0].geometry = SegmentGeometry::BSpline {
            degree: 1,
            poles: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]],
            weights: Vec::new(),
            knots: Vec::new(),
            multiplicities: Vec::new(),
            periodic: false,
        };
        assert_eq!(region.perimeter(), None);
    }

    /// mitcad#1259: a region turned about a point keeps its keys and its
    /// shape; a quarter turn of the left half circle about (5, 0) puts it
    /// below that point.
    #[test]
    fn regions_turn_about_a_point() {
        let half = std::f64::consts::FRAC_PI_2;
        let region = ProfileRegion {
            key: "r{c1[c2,c2],c2[c1,c1]}".parse().unwrap(),
            loops: vec![ProfileLoop {
                segments: vec![
                    ProfileSegment {
                        key: "c2[c1,c1]".parse().unwrap(),
                        geometry: SegmentGeometry::Arc {
                            center: [0.0, 0.0],
                            radius: 10.0,
                            start_angle: half,
                            end_angle: 3.0 * half,
                        },
                    },
                    ProfileSegment {
                        key: "c1[c2,c2]".parse().unwrap(),
                        geometry: SegmentGeometry::Line {
                            start: [0.0, -10.0],
                            end: [0.0, 10.0],
                        },
                    },
                ],
            }],
        };
        let turned = region.rotated([5.0, 0.0], half);
        assert_eq!(turned.key, region.key);
        let (min, max) = turned.bounds();
        let near =
            |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        // The arc's centre (0, 0) goes to (5, -5), its span to y below it.
        assert!(near(min, [-5.0, -15.0]), "{min:?}");
        assert!(near(max, [15.0, -5.0]), "{max:?}");
        let SegmentGeometry::Arc {
            start_angle,
            end_angle,
            ..
        } = turned.loops[0].segments[0].geometry
        else {
            panic!("an arc");
        };
        assert!((start_angle - 2.0 * half).abs() < 1e-12 && (end_angle - 4.0 * half).abs() < 1e-12);
        // No turn: the same region.
        assert_eq!(region.rotated([5.0, 0.0], 0.0), region);
    }
}
