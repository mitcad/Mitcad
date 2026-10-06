// SPDX-License-Identifier: MIT
//! Shapes with their names as bytes for the result store (P7d,
//! `geometry/include/mitcad/geometry/persist.hpp`), and what the caches'
//! diagnostics need: a shape's memory and the process's.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// The process's memory in bytes; 0 where the system does not tell.
    struct ProcessMemory {
        resident: u64,
        peak_resident: u64,
        /// Allocated by malloc (OCCT allocates there too).
        heap: u64,
    }

    unsafe extern "C++" {
        include!("bridge/persist.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The shape with its face names, notes and measured properties
        /// (a string, so that megabytes are not pushed byte by byte).
        fn shape_bytes(shape: &Shape) -> Result<UniquePtr<CxxString>>;
        /// A shape from shape_bytes; damaged data is an error.
        fn shape_from_bytes(data: &[u8]) -> Result<SharedPtr<Shape>>;
        /// The memory the shape takes, estimated in bytes.
        fn shape_memory(shape: &Shape) -> u64;
        fn process_memory() -> ProcessMemory;
    }
}
