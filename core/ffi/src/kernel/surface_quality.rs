// SPDX-License-Identifier: MIT
//! Surface quality analyses (mitcad#2517): a body's faces triangulated with
//! each vertex's outward normal and principal curvatures, and the faces' u
//! and v iso curves trimmed to them with the normal curvature along each
//! (`core/cpp/bridge/surface_quality.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// Faces triangulated: `vertices` and `normals` x, y, z after each
    /// other (millimetres; unit normals out of the body), `curvatures` the
    /// larger then the smaller principal curvature per vertex (1/mm,
    /// positive where the face bulges outwards; NaN where there is none),
    /// `triangles` three vertex indices each (counter-clockwise seen from
    /// outside, face by face), `face_triangles` each face's number of
    /// triangles in the order the faces were asked.
    struct SurfaceSampleMesh {
        vertices: Vec<f64>,
        normals: Vec<f64>,
        curvatures: Vec<f64>,
        triangles: Vec<u32>,
        face_triangles: Vec<u32>,
    }

    /// Iso curves, one entry each in `faces` (the face's index among those
    /// asked), `along_u` (1 along u, 0 along v), `at` (the held parameter
    /// as a fraction of the face's range) and `counts` (its points); its
    /// points' `points` and `normals` x, y, z after each other and their
    /// `curvatures` (the normal curvature along the curve, 1/mm, NaN where
    /// there is none), curve after curve.
    struct IsoCurveSet {
        faces: Vec<u32>,
        along_u: Vec<u8>,
        at: Vec<f64>,
        counts: Vec<u32>,
        points: Vec<f64>,
        normals: Vec<f64>,
        curvatures: Vec<f64>,
    }

    unsafe extern "C++" {
        include!("bridge/surface_quality.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The faces named (every face when none is named) triangulated
        /// within the surface deviation (mm) and the angle between
        /// neighbouring facets (radians), on a copy, with each vertex's
        /// normal and principal curvatures. At most about `face_triangles`
        /// triangles a doubly curved face (0: no limit).
        fn surface_samples(
            shape: &Shape,
            faces: &[String],
            deviation: f64,
            angle: f64,
            face_triangles: u64,
        ) -> Result<SurfaceSampleMesh>;

        /// `count` u and `count` v iso curves of each face named (every
        /// face when none is named), trimmed to it, within `deviation` mm.
        fn face_isocurves(
            shape: &Shape,
            faces: &[String],
            count: u32,
            deviation: f64,
        ) -> Result<IsoCurveSet>;
    }
}
