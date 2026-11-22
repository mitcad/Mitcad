// SPDX-License-Identifier: MIT
//! Surface groups of faces for `model_render`'s edges (mitcad#2148): the
//! faces the 3D view takes as one smooth surface, joined across the edges
//! it leaves out (`geometry/include/mitcad/geometry/smooth_seams.hpp`,
//! `smooth_surface_groups`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/surface_groups.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// A surface number for each face, by face index
        /// (`geometry::smooth_surface_groups`).
        fn face_surface_groups(shape: &Shape) -> Result<Vec<u32>>;
    }
}
