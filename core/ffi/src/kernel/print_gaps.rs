// SPDX-License-Identifier: MIT
//! Narrow gaps between bodies that touch (mitcad#1672): every pair of a
//! face of one body and a face of the other closer than a distance, with
//! the closest points and the faces' outward normals there
//! (`core/cpp/bridge/print_gaps.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::FaceSeparation`].
    struct PrintFaceGap {
        face_a: usize,
        face_b: usize,
        distance: f64,
        on_a: [f64; 3],
        on_b: [f64; 3],
        normal_a: [f64; 3],
        normal_b: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/print_gaps.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The pairs of a face of `a` and a face of `b` less than `within`
        /// mm apart, the faces by index in the shapes' face order.
        fn face_separations(a: &Shape, b: &Shape, within: f64) -> Result<Vec<PrintFaceGap>>;
    }
}
