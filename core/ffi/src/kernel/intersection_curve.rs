// SPDX-License-Identifier: MIT
//! Intersection curves (mitcad#1405): two chains of curves, each swept
//! along its sketch's normal, and where the swept surfaces cross
//! (`geometry/include/mitcad/geometry/intersection_curve.hpp`); since
//! mitcad#1777 also the curve tools' growing helices and sections of faces
//! and bodies.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// See `mitcad_model::Curve3`: `kind` 0 a point, 1 a line, 2 a conic,
    /// 3 a B-spline; only the fields of its kind are used (as
    /// `face_outline::ffi::OutlineCurve`'s).
    struct CrossingCurve {
        kind: u8,
        start: [f64; 3],
        end: [f64; 3],
        center: [f64; 3],
        normal: [f64; 3],
        x_axis: [f64; 3],
        major: f64,
        minor: f64,
        first: f64,
        last: f64,
        closed: bool,
        degree: u32,
        /// x0, y0, z0, x1, ...
        poles: Vec<f64>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    }

    /// See `mitcad_model::kernel::HelixCurveSpec` (mitcad#1777).
    struct HelixCurveInput {
        origin: [f64; 3],
        direction: [f64; 3],
        x_axis: [f64; 3],
        radius: f64,
        pitch: f64,
        revolutions: f64,
        left_handed: bool,
        growth: f64,
    }

    unsafe extern "C++" {
        include!("bridge/intersection_curve.hpp");
        include!("bridge/trycatch.hpp");

        type SweepCurve = crate::kernel::sweeps::ffi::SweepCurve;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// A helix that grows (mitcad#1777, `geometry::helix_curve`): one
        /// curve on the cone its turns lie on.
        fn helix_curve(input: &HelixCurveInput) -> Result<Vec<CrossingCurve>>;
        /// Where the shapes of `first` meet those of `second` (mitcad#1777,
        /// `geometry::shape_section`): shape `i` of a side with the next
        /// `first_counts[i]` (`second_counts[i]`) of its face names, none
        /// for the whole shape.
        fn shape_section(
            first: &ShapeList,
            first_faces: &[String],
            first_counts: &[usize],
            second: &ShapeList,
            second_faces: &[String],
            second_counts: &[usize],
        ) -> Result<Vec<CrossingCurve>>;

        /// Where the chain `first` swept along `first_direction` crosses
        /// the chain `second` swept along `second_direction`
        /// (`geometry::intersection_curves`).
        fn intersection_curves(
            first: &[SweepCurve],
            first_direction: [f64; 3],
            second: &[SweepCurve],
            second_direction: [f64; 3],
        ) -> Result<Vec<CrossingCurve>>;
    }
}

use mitcad_model::Curve3;

pub fn curve(c: &ffi::CrossingCurve) -> Curve3 {
    match c.kind {
        0 => Curve3::Point(c.start),
        1 => Curve3::Line {
            start: c.start,
            end: c.end,
        },
        2 => Curve3::Conic {
            center: c.center,
            normal: c.normal,
            x_axis: c.x_axis,
            major: c.major,
            minor: c.minor,
            start: c.first,
            end: c.last,
            closed: c.closed,
        },
        _ => Curve3::BSpline {
            degree: c.degree,
            poles: c.poles.chunks(3).map(|p| [p[0], p[1], p[2]]).collect(),
            weights: c.weights.clone(),
            knots: c.knots.clone(),
        },
    }
}
