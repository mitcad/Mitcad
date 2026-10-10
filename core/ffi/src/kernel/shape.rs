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

// SAFETY (the .f3d import's definitions evaluated in parallel, mitcad#95):
// a `mitcad::geometry::Shape` is immutable once made; what it remembers of
// itself (its measured properties and box) is kept behind a lock
// (`LazyValue`, geometry/include/mitcad/geometry/shape.hpp), and its
// `shared_ptr` and the OCCT handles under it count references atomically.
// The geometry's operations read their input shapes and never modify them
// (T0e, `MITCAD_CHECK_INPUTS`): those whose OCCT algorithms write into
// their inputs' sub-shapes (booleans, fillets, chamfers, shells, healing)
// work on copies (`InputCopy`, geometry/src/history.hpp), so threads that
// evaluate features on the same input shapes at once share only what they
// read. What is left are OCCT's bookkeeping flags of sub-shapes (free,
// modified, checked), which builders set on the shapes they put into a new
// one and nothing here reads. A shape must still not be triangulated
// while another thread computes with it (the view does that on the
// application's thread, between jobs).
unsafe impl Send for ffi::Shape {}
unsafe impl Sync for ffi::Shape {}
