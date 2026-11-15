// SPDX-License-Identifier: MIT
//! Press Pull on a face continued Straight or Along the walls, tapered by a
//! draft or a scale (`geometry/include/mitcad/geometry/press_pull.hpp`,
//! mitcad#1850).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PressPullRun {
        Straight,
        Along,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum PressPullTaper {
        Untapered,
        Draft,
        Scale,
    }

    /// `mitcad_model::kernel::PressPullSpec`.
    struct PressPullParams {
        run: PressPullRun,
        distance: f64,
        taper: PressPullTaper,
        taper_angle: f64,
        end_scale: f64,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FaceAlong {
        Refused,
        Straight,
        Turn,
    }

    /// `mitcad_model::kernel::FaceRun`: the face's area centre and outward
    /// normal; how its walls continue (`direction` for Straight, the axis
    /// and `radius` for Turn), or why not (`refusal`, a placed error).
    struct FaceRunInfo {
        centre: [f64; 3],
        normal: [f64; 3],
        reach: f64,
        along: FaceAlong,
        direction: [f64; 3],
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        radius: f64,
        refusal: String,
    }

    unsafe extern "C++" {
        include!("bridge/press_pull.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// geometry::press_pull: the faces continued, the body joined with
        /// the new pieces (a positive distance) or cut by them.
        fn press_pull(
            feature: &str,
            body: &Shape,
            faces: &[String],
            params: &PressPullParams,
        ) -> Result<SharedPtr<Shape>>;

        /// geometry::face_run of one face.
        fn face_run(body: &Shape, face: &str) -> Result<FaceRunInfo>;
    }
}
