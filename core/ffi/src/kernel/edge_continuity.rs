// SPDX-License-Identifier: MIT
//! Continuity Analysis (mitcad#2521): the gap, the angle between the
//! normals and the difference of the normal curvatures across a body's
//! edges, sampled along each edge between its faces
//! (`geometry/include/mitcad/geometry/edge_continuity.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// `mitcad_model::kernel::ContinuitySpec`: the samples along an edge
    /// and the classes' tolerances (mm, radians, a share).
    struct ContinuityParams {
        samples: u32,
        gap: f64,
        angle: f64,
        curvature: f64,
    }

    /// One edge measured (`mitcad_model::kernel::EdgeContinuity`):
    /// `sides` 0 two faces, 1 a seam, 2 a free edge; `reached` (its class)
    /// 0 none, 1 G0, 2 G1, 3 G2, 4 not measured; `points` x, y, z triples
    /// along it.
    struct EdgeContinuityInfo {
        name: String,
        sides: u8,
        reached: u8,
        samples: u32,
        gap: f64,
        angle: f64,
        curvature: f64,
        curvature_share: f64,
        gap_at: [f64; 3],
        angle_at: [f64; 3],
        curvature_at: [f64; 3],
        points: Vec<f64>,
    }

    unsafe extern "C++" {
        include!("bridge/edge_continuity.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// geometry::edge_continuity of the edges named and every edge of
        /// the faces named (every edge of the shape when neither names
        /// any). A name that finds nothing on the shape is an error.
        fn edge_continuity(
            shape: &Shape,
            edges: &[String],
            faces: &[String],
            params: &ContinuityParams,
        ) -> Result<Vec<EdgeContinuityInfo>>;
    }
}
