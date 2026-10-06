// SPDX-License-Identifier: MIT
//! Evaluated sketch geometry handed to the kernel: profile regions bounded
//! by loops of named curve segments, placed in 3D by a sketch frame.
//! Millimetres and radians, sketch coordinates in the frame.

use crate::topo::{RegionKey, SegmentKey};

/// Places sketch coordinates in 3D: p = origin + u x_axis + v y_axis. The
/// axes are unit vectors at right angles; the normal is x_axis × y_axis.
#[derive(Debug, Clone, Copy, PartialEq)]
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

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The curve of a segment. Arcs run counter-clockwise from the start to the
/// end angle; ellipse angles are parameter angles from the major axis.
#[derive(Debug, Clone, PartialEq)]
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
            Self::Circle { center, radius } => angles(0.0, std::f64::consts::TAU)
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
}
