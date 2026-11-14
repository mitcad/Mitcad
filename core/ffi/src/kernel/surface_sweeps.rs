// SPDX-License-Identifier: MIT
//! Surface modelling, surface sweeps: surface bodies swept from chains of
//! sketch curves and a sweep's walls without its caps
//! (`geometry/include/mitcad/geometry/surface_sweeps.hpp`, mitcad#455).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    unsafe extern "C++" {
        include!("bridge/surface_sweeps.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type SweepCurve = crate::kernel::sweeps::ffi::SweepCurve;

        /// The shape's faces but the feature's faces in the roles.
        fn open_faces(shape: &Shape, feature: &str, roles: &[String]) -> Result<SharedPtr<Shape>>;
        /// The chains (`chains[i]` curves each, one after another) moved by
        /// `shift` (x, y, z) and swept from `-two` to `one` along
        /// `direction`.
        fn extrude_sheet(
            feature: &str,
            curves: &[SweepCurve],
            chains: &[usize],
            shift: &[f64],
            direction: &[f64],
            one: f64,
            two: f64,
        ) -> Result<SharedPtr<Shape>>;
        /// The chains turned about the axis through `origin` along
        /// `direction` by `angle1` and `angle2` the other way.
        fn revolve_sheet(
            feature: &str,
            curves: &[SweepCurve],
            chains: &[usize],
            origin: &[f64],
            direction: &[f64],
            angle1: f64,
            angle2: f64,
        ) -> Result<SharedPtr<Shape>>;
        /// The chains swept along the path from where it crosses the plane
        /// (origin and normal: x, y, z, nx, ny, nz), `extents` of the path
        /// after and before that point.
        fn sweep_sheet(
            feature: &str,
            curves: &[SweepCurve],
            chains: &[usize],
            path: &[SweepCurve],
            plane: &[f64],
            extents: &[f64],
            parallel: bool,
        ) -> Result<SharedPtr<Shape>>;
        /// A surface through the sections (one chain each).
        fn loft_sheet(
            feature: &str,
            curves: &[SweepCurve],
            chains: &[usize],
            ruled: bool,
            closed: bool,
        ) -> Result<SharedPtr<Shape>>;
        /// The chains moved by the screw motion about the axis (x, y, z,
        /// dx, dy, dz) that rises `pitch` per turn for `revolutions` turns
        /// (mitcad#1777).
        fn helix_sheet(
            feature: &str,
            curves: &[SweepCurve],
            chains: &[usize],
            axis: &[f64],
            pitch: f64,
            revolutions: f64,
            left_handed: bool,
        ) -> Result<SharedPtr<Shape>>;
    }
}

use mitcad_model::PathCurve;

use super::sweeps;

/// The chains' curves one after another and how many each has.
pub(crate) fn chains(chains: &[Vec<PathCurve>]) -> (Vec<sweeps::ffi::SweepCurve>, Vec<usize>) {
    (
        chains.iter().flatten().map(sweeps::curve).collect(),
        chains.iter().map(Vec::len).collect(),
    )
}
