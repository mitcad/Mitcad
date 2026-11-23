// SPDX-License-Identifier: MIT
//! Point Cloud Deviation (mitcad#2240): each point's exact distance from a
//! body's faces and the nearest point of them, solved on the faces, their
//! edges and vertices (`core/cpp/bridge/point_clouds.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/point_clouds.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// Per point (x, y, z triples) its distance from the faces named
        /// (every face when none is named) and the nearest point: four
        /// values each, the distance then x, y, z.
        fn boundary_nearest(shape: &Shape, faces: &[String], points: &[f64]) -> Result<Vec<f64>>;
    }
}
