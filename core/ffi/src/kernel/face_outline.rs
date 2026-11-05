// SPDX-License-Identifier: MIT
//! Outlines of faces seen along a direction (mitcad#1265): Project of a
//! curved face, its boundary edges without the seams off the outline and
//! its silhouettes (`geometry/include/mitcad/geometry/face_outline.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// See `mitcad_model::Curve3`: `kind` 0 a point, 1 a line, 2 a conic,
    /// 3 a B-spline; only the fields of its kind are used (as
    /// `sketch_ref::ffi::ModelCurve`'s).
    struct OutlineCurve {
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

    unsafe extern "C++" {
        include!("bridge/face_outline.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The faces of the name seen along `direction`: their boundary
        /// edges without the seams off the outline, and their silhouettes
        /// (`geometry::face_outline`).
        fn face_outline(
            shape: &Shape,
            face: &str,
            direction: [f64; 3],
        ) -> Result<Vec<OutlineCurve>>;
    }
}

use mitcad_model::Curve3;

pub fn curve(c: &ffi::OutlineCurve) -> Curve3 {
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
