// SPDX-License-Identifier: MIT
//! Minimum Radius (mitcad#2518): each face's largest principal curvature
//! and where it is, evaluated on the exact surfaces inside the faces
//! (`core/cpp/bridge/min_radius.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::FaceCurvature`].
    struct MinRadiusFace {
        face: usize,
        curvature: f64,
        at: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/min_radius.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// Each face's largest principal curvature (1/mm, 0 for a face that
        /// does not bend) and where it is: every face when none is named,
        /// else every piece of each name, each face once, by its index in
        /// the shape's face order.
        fn face_curvatures(shape: &Shape, faces: &[String]) -> Result<Vec<MinRadiusFace>>;
    }
}
