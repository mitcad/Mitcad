// SPDX-License-Identifier: MIT
//! Feature inputs drawn where they were (mitcad#957): a Fillet's or a
//! Chamfer's picked edges as polylines
//! (`geometry/include/mitcad/geometry/input_edges.hpp`), which the model
//! keeps with the feature's result.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// One edge's points, x0, y0, z0, x1, ...
    struct EdgePolyline {
        points: Vec<f64>,
    }

    unsafe extern "C++" {
        include!("bridge/input_edges.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The edges the names resolve to and those bounding the faces the
        /// names resolve to, each once, as polylines within the deflection
        /// (mm) and the angle (radians); an error for a name the shape
        /// lacks.
        fn edge_polylines(
            shape: &Shape,
            edges: &[String],
            faces: &[String],
            deflection: f64,
            angle: f64,
        ) -> Result<Vec<EdgePolyline>>;
    }
}
