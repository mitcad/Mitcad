// SPDX-License-Identifier: MIT
//! Plastic part features: the bands of lips and grooves along rim faces
//! (`geometry/include/mitcad/geometry/plastic.hpp`, mitcad#459).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::RimBandSpec`] without its body.
    struct RimBand {
        feature: String,
        face: String,
        outside: bool,
        width: f64,
        free: f64,
        start: f64,
        end: f64,
        /// The side along the loop follows the faces beside it (mitcad#2511).
        tangent: bool,
        /// The other side's lean (radians, mitcad#2511).
        side_taper: f64,
    }

    unsafe extern "C++" {
        include!("bridge/plastic.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// geometry::rim_band on the first shape of the list.
        fn rim_band(body: &ShapeList, spec: &RimBand) -> Result<SharedPtr<Shape>>;
    }
}
