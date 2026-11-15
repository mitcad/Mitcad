// SPDX-License-Identifier: MIT
//! Fill Cavity: the empty spaces of a body below a cap plane
//! (`geometry/include/mitcad/geometry/fill_cavity.hpp`, mitcad#1818).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::CavitySpec`] without its body.
    struct Cavity {
        feature: String,
        top: String,
        origin: [f64; 3],
        x_axis: [f64; 3],
        normal: [f64; 3],
        min: [f64; 3],
        max: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/fill_cavity.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// geometry::cavity_pieces of the body, in their order.
        fn cavity_pieces(body: &Shape, spec: &Cavity) -> Result<UniquePtr<ShapeList>>;
    }
}
