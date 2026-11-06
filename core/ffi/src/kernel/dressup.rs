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
        Spline,
    }

    /// One edge set: `value` is the radius, the chord length, the start
    /// radius or the first distance, `value2` the end radius or the second
    /// distance; `start_vertex` empty for none; `mid` the mid radii of a
    /// variable set as (position, radius) pairs one after the other;
    /// `reference_face` (empty for none) and `flip` place an asymmetric
    /// set's distances; `curvature` asks for G2 with the tangency `weight`;
    /// `knots` and `radii` a spline radius's knot vector and control radii
    /// (mitcad#464).
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
        knots: Vec<f64>,
        radii: Vec<f64>,
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
        include!("bridge/trycatch.hpp");

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

        /// What is wrong with a shape the operation kept (P9: a fillet's
        /// corner that folds back on itself), for the feature's warnings.
        fn shape_notes(shape: &Shape) -> Vec<String>;
        /// What is worth knowing of how it was made (mitcad#1106: a fillet
        /// or chamfer built 0.1 % smaller), for the feature's information.
        fn shape_infos(shape: &Shape) -> Vec<String>;
        /// The places of each of its notes, in their order (mitcad#1327):
        /// a JSON array of places as the model's notices give them
        /// (`{"kind": "face", "name": …}`, `{"kind": "point", "at": […]}`).
        fn shape_note_places(shape: &Shape) -> Vec<String>;
    }
}

/// The places of a shape's notes (mitcad#1327), from their JSON; a note
/// whose places cannot be read has none.
pub(crate) fn note_places(
    shape: &crate::kernel::shape::ffi::Shape,
) -> Vec<Vec<mitcad_model::notice::NoticePlace>> {
    ffi::shape_note_places(shape)
        .iter()
        .map(|json| serde_json::from_str(json).unwrap_or_default())
        .collect()
}
