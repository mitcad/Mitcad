// SPDX-License-Identifier: MIT
//! Form bodies (mitcad#1757): the bicubic patches of a control cage's limit
//! surface, which the model works out (`core/subdiv`), made faces and sewn
//! into a solid or a sheet (`core/cpp/bridge/form.cpp`,
//! `geometry/src/form.cpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::kernel::SurfacePatch`], its poles as x, y, z after
    /// one another.
    struct FormPatch {
        face: String,
        u_degree: u32,
        v_degree: u32,
        nu: usize,
        nv: usize,
        poles: Vec<f64>,
        u_knots: Vec<f64>,
        u_mults: Vec<u32>,
        v_knots: Vec<f64>,
        v_mults: Vec<u32>,
    }

    unsafe extern "C++" {
        include!("bridge/form.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        /// The patches as faces sewn within `tolerance`; with `closed`, one
        /// solid.
        fn patch_body(
            patches: &[FormPatch],
            closed: bool,
            tolerance: f64,
        ) -> Result<SharedPtr<Shape>>;
    }
}

use mitcad_model::kernel::SurfacePatch;

/// A patch in the bridge's form.
pub fn patch(patch: &SurfacePatch) -> ffi::FormPatch {
    ffi::FormPatch {
        face: patch.face.to_string(),
        u_degree: patch.u_degree,
        v_degree: patch.v_degree,
        nu: patch.nu,
        nv: patch.nv,
        poles: patch.poles.iter().flatten().copied().collect(),
        u_knots: patch.u_knots.clone(),
        u_mults: patch.u_mults.clone(),
        v_knots: patch.v_knots.clone(),
        v_mults: patch.v_mults.clone(),
    }
}
