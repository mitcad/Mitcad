// SPDX-License-Identifier: MIT
//! Extrusion (`geometry/include/mitcad/geometry/extrude.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// Along `direction` (a unit vector) from `start` to `end`, offsets
    /// from the sketch plane.
    struct Sweep {
        direction: [f64; 3],
        start: f64,
        end: f64,
    }

    unsafe extern "C++" {
        include!("bridge/extrude.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type Frame = crate::kernel::profile::ffi::Frame;
        type Region = crate::kernel::profile::ffi::Region;

        /// The regions swept, faces named after `feature`.
        fn extrude(
            feature: &str,
            frame: &Frame,
            regions: &[Region],
            sweep: &Sweep,
        ) -> Result<SharedPtr<Shape>>;
    }
}
