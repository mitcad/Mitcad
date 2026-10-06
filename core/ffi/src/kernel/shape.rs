// SPDX-License-Identifier: MIT
//! The shape handle shared by all kernel bridges, and lists of shapes to
//! pass several at once.

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/shape.hpp");

        /// `mitcad::geometry::Shape`: an immutable B-rep with face names.
        #[namespace = "mitcad::geometry"]
        type Shape;

        #[namespace = "mitcad::bridge"]
        type ShapeList;

        #[namespace = "mitcad::bridge"]
        fn new_shape_list() -> UniquePtr<ShapeList>;
        fn push(self: Pin<&mut ShapeList>, shape: SharedPtr<Shape>);
        fn size(self: &ShapeList) -> usize;
        fn at(self: &ShapeList, index: usize) -> SharedPtr<Shape>;
    }
}
