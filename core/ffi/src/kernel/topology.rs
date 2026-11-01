// SPDX-License-Identifier: MIT
//! Topology counts for Physical Properties (mitcad#1095): how many
//! distinct faces, edges and vertices a shape has, as the geometry
//! library's `Shape` indexes them.

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::TopologyCounts`].
    struct TopologyCounts {
        faces: usize,
        edges: usize,
        vertices: usize,
    }

    unsafe extern "C++" {
        include!("bridge/topology.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The shape's distinct faces, edges and vertices.
        fn topology_counts(shape: &Shape) -> TopologyCounts;
    }
}
