// SPDX-License-Identifier: MIT
//! Profile regions for the geometry library (`geometry/include/mitcad/
//! geometry/profile.hpp`), shared by the profile and extrude bridges.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// Places sketch coordinates in 3D; the normal is x_axis × y_axis.
    #[derive(Clone, Copy, Debug)]
    struct Frame {
        origin: [f64; 3],
        x_axis: [f64; 3],
        y_axis: [f64; 3],
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SegmentKind {
        Line,
        Arc,
        Circle,
        Ellipse,
        EllipseArc,
        BSpline,
    }

    /// A curve piece of a loop, in sketch coordinates; only the fields of
    /// its kind are used (see `mitcad_model::SegmentGeometry`).
    #[derive(Clone, Debug)]
    struct Segment {
        name: String,
        kind: SegmentKind,
        start: [f64; 2],
        end: [f64; 2],
        center: [f64; 2],
        radius: f64,
        minor_radius: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
        degree: u32,
        /// x0, y0, x1, y1, ...
        poles: Vec<f64>,
        weights: Vec<f64>,
        knots: Vec<f64>,
        multiplicities: Vec<u32>,
        periodic: bool,
    }

    #[derive(Clone, Debug)]
    struct Loop {
        segments: Vec<Segment>,
    }

    /// The first loop is the outer boundary.
    #[derive(Clone, Debug)]
    struct Region {
        name: String,
        loops: Vec<Loop>,
    }

    unsafe extern "C++" {
        include!("bridge/profile.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The faces of the regions, for display.
        fn profile_shape(frame: &Frame, regions: &[Region]) -> Result<SharedPtr<Shape>>;
    }
}

use mitcad_model::{ProfileRegion, SegmentGeometry, SketchFrame};

pub fn frame(frame: &SketchFrame) -> ffi::Frame {
    ffi::Frame {
        origin: frame.origin,
        x_axis: frame.x_axis,
        y_axis: frame.y_axis,
    }
}

fn segment(name: String, geometry: &SegmentGeometry) -> ffi::Segment {
    let mut segment = ffi::Segment {
        name,
        kind: ffi::SegmentKind::Line,
        start: [0.0; 2],
        end: [0.0; 2],
        center: [0.0; 2],
        radius: 0.0,
        minor_radius: 0.0,
        rotation: 0.0,
        start_angle: 0.0,
        end_angle: 0.0,
        degree: 0,
        poles: Vec::new(),
        weights: Vec::new(),
        knots: Vec::new(),
        multiplicities: Vec::new(),
        periodic: false,
    };
    match geometry {
        SegmentGeometry::Line { start, end } => {
            segment.start = *start;
            segment.end = *end;
        }
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            segment.kind = ffi::SegmentKind::Arc;
            segment.center = *center;
            segment.radius = *radius;
            segment.start_angle = *start_angle;
            segment.end_angle = *end_angle;
        }
        SegmentGeometry::Circle { center, radius } => {
            segment.kind = ffi::SegmentKind::Circle;
            segment.center = *center;
            segment.radius = *radius;
        }
        SegmentGeometry::Ellipse {
            center,
            major_radius,
            minor_radius,
            rotation,
        } => {
            segment.kind = ffi::SegmentKind::Ellipse;
            segment.center = *center;
            segment.radius = *major_radius;
            segment.minor_radius = *minor_radius;
            segment.rotation = *rotation;
        }
        SegmentGeometry::EllipseArc {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        } => {
            segment.kind = ffi::SegmentKind::EllipseArc;
            segment.center = *center;
            segment.radius = *major_radius;
            segment.minor_radius = *minor_radius;
            segment.rotation = *rotation;
            segment.start_angle = *start_angle;
            segment.end_angle = *end_angle;
        }
        SegmentGeometry::BSpline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
            periodic,
        } => {
            segment.kind = ffi::SegmentKind::BSpline;
            segment.degree = *degree;
            segment.poles = poles.iter().flatten().copied().collect();
            segment.weights = weights.clone();
            segment.knots = knots.clone();
            segment.multiplicities = multiplicities.clone();
            segment.periodic = *periodic;
        }
    }
    segment
}

pub fn region(region: &ProfileRegion) -> ffi::Region {
    ffi::Region {
        name: region.key.to_string(),
        loops: region
            .loops
            .iter()
            .map(|l| ffi::Loop {
                segments: l
                    .segments
                    .iter()
                    .map(|s| segment(s.key.to_string(), &s.geometry))
                    .collect(),
            })
            .collect(),
    }
}
