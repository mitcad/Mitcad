// SPDX-License-Identifier: MIT
//! A planar face of a body extruded as a profile
//! (`geometry/include/mitcad/geometry/face_extrude.hpp`, mitcad#535).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// How far the face goes (see `FaceExtrudeSpec`): along its outward
    /// normal, against it with `reversed`, from `start` along the outward
    /// normal; `side2` against the sweep with `two_sides`; along
    /// `direction` instead of the normal with `has_direction`, each side
    /// tapered by its `taper` (radians; mitcad#541). A side up to a target
    /// (mitcad#2033, the function's `target1` and `target2`) has no
    /// distance: its `side1` or `side2` is not used.
    struct FaceExtrusion {
        reversed: bool,
        start: f64,
        side1: f64,
        two_sides: bool,
        side2: f64,
        has_direction: bool,
        direction: [f64; 3],
        taper1: f64,
        taper2: f64,
    }

    unsafe extern "C++" {
        include!("bridge/face_extrude.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type TargetInput = crate::kernel::profile_feature::ffi::TargetInput;

        /// geometry::extrude_face: the face `face` of the body swept, its
        /// faces named after `feature` and its caps keyed by `key`; each
        /// side up to its target unless that is unset (mitcad#2033), the
        /// targets' bodies indexing `bodies`.
        #[allow(clippy::too_many_arguments)]
        fn extrude_face(
            feature: &str,
            body: &Shape,
            face: &str,
            key: &str,
            extrusion: &FaceExtrusion,
            target1: &TargetInput,
            target2: &TargetInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
    }
}
