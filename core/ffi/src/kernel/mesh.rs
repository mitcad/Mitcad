// SPDX-License-Identifier: MIT
//! Triangle meshes of bodies for 3D printing (3MF export, mitcad#13;
//! `geometry/io/include/mitcad/io/mesh.hpp`, `indexed_mesh`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A body's closed triangle mesh: `vertices` x, y, z after each other
    /// (millimetres), `triangles` three vertex indices each, counter-clockwise
    /// seen from outside.
    struct TriangleMesh {
        vertices: Vec<f64>,
        triangles: Vec<u32>,
    }

    unsafe extern "C++" {
        include!("bridge/mesh.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The body triangulated within the surface deviation (mm) and the
        /// angle between neighbouring facets (radians), on a copy; a mesh
        /// body's own triangles.
        fn triangle_mesh(shape: &Shape, deviation: f64, angle: f64) -> Result<TriangleMesh>;
    }
}
