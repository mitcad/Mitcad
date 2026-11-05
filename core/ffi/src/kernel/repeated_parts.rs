// SPDX-License-Identifier: MIT
//! Repeated parts (mitcad#1277): which bodies are copies of one another
//! moved by a rigid motion (`geometry/include/mitcad/geometry/repeated_parts.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::RigidCopy`]: the body copied, and the
    /// placement from it onto this one (row-major rotation, translation).
    struct RigidCopy {
        of: usize,
        linear: [f64; 9],
        translation: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/repeated_parts.hpp");
        include!("bridge/trycatch.hpp");

        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// geometry::rigid_copies of the bodies, one entry per body.
        fn rigid_copies(bodies: &ShapeList, tolerance: f64) -> Result<Vec<RigidCopy>>;
    }
}
