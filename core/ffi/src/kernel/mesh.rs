// SPDX-License-Identifier: MIT
//! Triangle meshes of bodies for 3D printing (3MF export, mitcad#13;
//! `geometry/io/include/mitcad/io/mesh.hpp`, `indexed_mesh`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A body's closed triangle mesh: `vertices` x, y, z after each other
    /// (millimetres), `triangles` three vertex indices each, counter-clockwise
    /// seen from outside; `faces` each triangle's face, its index in the
    /// shape's face order (mitcad#2385).
    struct TriangleMesh {
        vertices: Vec<f64>,
        triangles: Vec<u32>,
        faces: Vec<u32>,
    }

    unsafe extern "C++" {
        include!("bridge/mesh.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The body triangulated within the surface deviation (mm) and the
        /// angle between neighbouring facets (radians), on a copy; a mesh
        /// body's own triangles. At most about `face_triangles`
        /// triangles a doubly curved face (0: no limit; mitcad#398).
        fn triangle_mesh(
            shape: &Shape,
            deviation: f64,
            angle: f64,
            face_triangles: u64,
        ) -> Result<TriangleMesh>;

        /// A mesh body (as an STL file gives) of triangles: `vertices` x,
        /// y, z after each other (millimetres), `triangles` three vertex
        /// indices each. Errors without triangles or with an index out of
        /// range.
        fn mesh_from_triangles(vertices: &[f64], triangles: &[u32]) -> Result<SharedPtr<Shape>>;

        /// A mesh body as `mesh_from_triangles` makes one, each vertex with
        /// its texture coordinates (`uvs` u, v after each other, one pair
        /// per vertex; mitcad#1862), kept as its triangulation's UV nodes.
        /// Errors as `mesh_from_triangles`, and when `uvs` is not one pair
        /// per vertex or holds a number that is not finite.
        fn mesh_from_textured_triangles(
            vertices: &[f64],
            triangles: &[u32],
            uvs: &[f64],
        ) -> Result<SharedPtr<Shape>>;
    }
}
