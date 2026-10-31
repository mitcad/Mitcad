// SPDX-License-Identifier: MIT
//! Dogbones: the relief cylinders at a body's inside corners
//! (`geometry/include/mitcad/geometry/dogbone.hpp`, mitcad#1011).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::DogboneSpec`] without its body.
    struct Dogbone {
        feature: String,
        edges: Vec<String>,
        faces: Vec<String>,
        axis: [f64; 3],
        diameter: f64,
        extra: f64,
        along_side: bool,
    }

    unsafe extern "C++" {
        include!("bridge/dogbone.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// geometry::dogbone_tool on the body.
        fn dogbone_tool(body: &Shape, spec: &Dogbone) -> Result<SharedPtr<Shape>>;
    }
}
