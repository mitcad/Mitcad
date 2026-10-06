// SPDX-License-Identifier: MIT
//! Transforms, unions, tools of faces and primitives
//! (`geometry/include/mitcad/geometry/transform.hpp`, `pattern.hpp`,
//! `primitive.hpp`): the kernel family of moves, patterns, mirrors,
//! combine and primitives.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// `p' = linear · p + translation`, `linear` row-major.
    struct Affine {
        linear: [f64; 9],
        translation: [f64; 3],
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PrimitiveKind {
        Box,
        Cylinder,
        Sphere,
        Torus,
    }

    /// Sizes as in `mitcad::geometry::PrimitiveSpec` (a, b, c).
    struct PrimitiveInput {
        kind: PrimitiveKind,
        sizes: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/transform.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type Frame = crate::kernel::profile::ffi::Frame;

        /// The tool of faces and whether it is material of the body.
        type FaceTool;

        /// A mapped copy; `rename` (e.g. `F9:inst2`) wraps every face name.
        fn transform_shape(shape: &Shape, map: &Affine, rename: &str) -> Result<SharedPtr<Shape>>;
        fn unite(shapes: &ShapeList) -> Result<SharedPtr<Shape>>;
        fn face_tool(body: &Shape, faces: &[String]) -> Result<UniquePtr<FaceTool>>;
        fn tool(self: &FaceTool) -> SharedPtr<Shape>;
        fn material(self: &FaceTool) -> bool;
        fn primitive(
            feature: &str,
            frame: &Frame,
            input: &PrimitiveInput,
        ) -> Result<SharedPtr<Shape>>;
    }
}
