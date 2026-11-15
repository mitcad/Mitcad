// SPDX-License-Identifier: MIT
//! Shaded meshes of a body's faces for USDZ export (mitcad#123;
//! `geometry/io/include/mitcad/io/face_mesh.hpp`, `face_meshes`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A body's faces as triangles: `vertices` and `normals` x, y, z after
    /// each other (millimetres; unit normals out of the body), `triangles`
    /// three vertex indices each (counter-clockwise seen from outside,
    /// face by face), `face_triangles` each face's number of triangles in
    /// the shape's face order; `uvs` a mesh body's texture coordinates
    /// (mitcad#1874), u, v per vertex after each other, or empty when no
    /// face has them.
    struct FaceMeshes {
        vertices: Vec<f64>,
        normals: Vec<f64>,
        triangles: Vec<u32>,
        face_triangles: Vec<u32>,
        uvs: Vec<f64>,
    }

    unsafe extern "C++" {
        include!("bridge/face_mesh.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The body's faces triangulated within the surface deviation (mm)
        /// and the angle between neighbouring facets (radians), on a copy.
        /// At most about `face_triangles` triangles a doubly curved face (0:
        /// no limit; mitcad#398).
        fn face_meshes(
            shape: &Shape,
            deviation: f64,
            angle: f64,
            face_triangles: u64,
        ) -> Result<FaceMeshes>;
    }
}
