// SPDX-License-Identifier: MIT
//! Model geometry for sketches: the curves of named edges, faces and
//! vertices (`geometry/include/mitcad/geometry/sketch_ref.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ModelCurveKind {
        Point,
        Line,
        Conic,
        BSpline,
    }

    /// See `mitcad_model::Curve3`; only the fields of its kind are used.
    struct ModelCurve {
        kind: ModelCurveKind,
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

    /// FreeCAD import: a face, edge or vertex by its index.
    struct IndexedElement {
        found: bool,
        /// Empty when its faces have no names.
        name: String,
        curves: Vec<ModelCurve>,
    }

    unsafe extern "C++" {
        include!("bridge/sketch_ref.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The curves of a named edge, face (its boundary) or vertex.
        fn model_curves(shape: &Shape, name: &str) -> Result<Vec<ModelCurve>>;

        /// The face (`b'F'`), edge (`b'E'`) or vertex (`b'V'`) `index` in
        /// OCCT's order.
        fn indexed_element(shape: &Shape, kind: u8, index: usize) -> Result<IndexedElement>;
    }
}

use mitcad_model::Curve3;

pub fn curve(c: &ffi::ModelCurve) -> Curve3 {
    match c.kind {
        ffi::ModelCurveKind::Point => Curve3::Point(c.start),
        ffi::ModelCurveKind::Line => Curve3::Line {
            start: c.start,
            end: c.end,
        },
        ffi::ModelCurveKind::Conic => Curve3::Conic {
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
