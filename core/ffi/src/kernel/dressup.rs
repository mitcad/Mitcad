// SPDX-License-Identifier: MIT
//! Fillets and chamfers on named edges, in edge sets
//! (`geometry/include/mitcad/geometry/dressup.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FilletKind {
        Constant,
        ChordLength,
        Variable,
        Asymmetric,
    }

    /// One edge set: `value` is the radius, the chord length, the start
    /// radius or the first distance, `value2` the end radius or the second
    /// distance; `start_vertex` empty for none; `mid` the mid radii of a
    /// variable set as (position, radius) pairs one after the other;
    /// `reference_face` (empty for none) and `flip` place an asymmetric
    /// set's distances; `curvature` asks for G2 with the tangency `weight`.
    struct FilletSet {
        edges: Vec<String>,
        faces: Vec<String>,
        kind: FilletKind,
        value: f64,
        value2: f64,
        start_vertex: String,
        mid: Vec<f64>,
        reference_face: String,
        flip: bool,
        curvature: bool,
        weight: f64,
        tangent_chain: bool,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChamferKind {
        EqualDistance,
        TwoDistances,
        DistanceAngle,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ChamferCornerKind {
        Chamfer,
        Miter,
        Blend,
    }

    /// One edge set: `value2` is the second distance or the angle;
    /// `reference_face` empty for the first face of each edge's name.
    struct ChamferSet {
        edges: Vec<String>,
        faces: Vec<String>,
        kind: ChamferKind,
        distance: f64,
        value2: f64,
        reference_face: String,
        flip: bool,
        tangent_chain: bool,
    }

    unsafe extern "C++" {
        include!("bridge/dressup.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        fn fillet(
            feature: &str,
            body: &Shape,
            sets: &[FilletSet],
            rolling_ball_corners: bool,
        ) -> Result<SharedPtr<Shape>>;
        fn chamfer(
            feature: &str,
            body: &Shape,
            sets: &[ChamferSet],
            corner: ChamferCornerKind,
        ) -> Result<SharedPtr<Shape>>;

        /// What the operation that made a shape gave up (P9: a fillet or
        /// chamfer built 0.1 % smaller), for the feature's warnings.
        fn shape_notes(shape: &Shape) -> Vec<String>;
    }
}
