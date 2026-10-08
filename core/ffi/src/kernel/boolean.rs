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

        /// `before` minus `after` where they differ by more than `slack`
        /// (`geometry/include/mitcad/geometry/removed.hpp`).
        fn removed_material(
            before: &Shape,
            after: &Shape,
            slack: f64,
        ) -> Result<UniquePtr<BooleanResult>>;

        /// The join of `body` with a near copy of it where they differ by
        /// more than `slack`; the body not touched when `copy` is not a
        /// near copy (`geometry/include/mitcad/geometry/removed.hpp`,
        /// mitcad#88).
        fn join_near_copy(
            body: &Shape,
            copy: &Shape,
            slack: f64,
        ) -> Result<UniquePtr<BooleanResult>>;

        fn piece_count(self: &BooleanResult) -> usize;
        fn piece(self: &BooleanResult, index: usize) -> SharedPtr<Shape>;
        fn piece_sources(self: &BooleanResult, index: usize) -> Vec<usize>;
        fn touched(self: &BooleanResult, target: usize) -> bool;
    }
}
