// SPDX-License-Identifier: MIT
//! Join, Cut and Intersect with participant bodies
//! (`geometry/include/mitcad/geometry/boolean.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/boolean.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// The pieces of a boolean and the targets they come from.
        type BooleanResult;

        fn boolean_join(targets: &ShapeList, tool: &Shape) -> Result<UniquePtr<BooleanResult>>;
        fn boolean_cut(targets: &ShapeList, tool: &Shape) -> Result<UniquePtr<BooleanResult>>;
        fn boolean_intersect(targets: &ShapeList, tool: &Shape)
        -> Result<UniquePtr<BooleanResult>>;

        fn piece_count(self: &BooleanResult) -> usize;
        fn piece(self: &BooleanResult, index: usize) -> SharedPtr<Shape>;
        fn piece_sources(self: &BooleanResult, index: usize) -> Vec<usize>;
        fn touched(self: &BooleanResult, target: usize) -> bool;
    }
}
