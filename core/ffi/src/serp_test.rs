// SPDX-License-Identifier: MIT
//! The `.serp` import's test file (mitcad#1870), written by
//! `mitcad-cli serp-test-file`: the reader's sample scene
//! (`mitcad_serp::testfile::sample`, in centimetres) around B-rep data that
//! OCCT wrote in its binary format, as the files' writer keeps its shapes:
//! a box of 2 x 1 x 0.5 cm, a B-spline surface of one face (a bicubic
//! patch of 3 x 3 cm, its middle raised), and a block of one box of
//! 3 x 1 x 1 cm placed twice, each placement a compound of the block's
//! shapes moved as the writer moves it: 5 cm along x, and turned a quarter
//! about z and 5 cm along y.

use mitcad_model::kernel::{Kernel, PatchBodySpec, SurfacePatch};
use mitcad_model::topo::{FaceName, RoleKey};
use mitcad_model::{FeatureUid, PrimitiveShape, PrimitiveSpec, SketchFrame, Transform};
use mitcad_model_api::ApiError;

use crate::kernel::OcctKernel;

fn failed(e: impl std::fmt::Display) -> ApiError {
    ApiError(format!("the .serp test file: {e}"))
}

/// A bicubic B-spline patch over `size` x `size` in the XY plane, its four
/// middle poles raised by `rise`.
fn bump(feature: FeatureUid, size: f64, rise: f64) -> SurfacePatch {
    let mut poles = Vec::with_capacity(16);
    for i in 0..4 {
        for j in 0..4 {
            let middle = (1..3).contains(&i) && (1..3).contains(&j);
            poles.push([
                size * f64::from(i) / 3.0,
                size * f64::from(j) / 3.0,
                if middle { rise } else { 0.0 },
            ]);
        }
    }
    SurfacePatch {
        face: FaceName::new(feature, "patch", Some(RoleKey::Index(0))),
        u_degree: 3,
        v_degree: 3,
        nu: 4,
        nv: 4,
        poles,
        u_knots: vec![0.0, 1.0],
        u_mults: vec![4, 4],
        v_knots: vec![0.0, 1.0],
        v_mults: vec![4, 4],
    }
}

/// Writes the test file; `test_case` `newer` writes it as a format version
/// newer than the reader knows (empty for the sample).
pub(crate) fn serp_write_test_file(path: &str, test_case: &str) -> Result<(), ApiError> {
    let kernel = OcctKernel;
    let feature = FeatureUid(1);
    let primitive = |shape: PrimitiveShape| {
        kernel
            .primitive(&PrimitiveSpec {
                feature,
                frame: SketchFrame::XY,
                shape,
            })
            .map_err(failed)
    };
    let data = |shape: &<OcctKernel as Kernel>::Shape| kernel.brep_data(shape).map_err(failed);
    let solid = primitive(PrimitiveShape::Box {
        length: 2.0,
        width: 1.0,
        height: 0.5,
    })?;
    let part = primitive(PrimitiveShape::Box {
        length: 3.0,
        width: 1.0,
        height: 1.0,
    })?;
    let patches = [bump(FeatureUid(2), 3.0, 1.0)];
    let face = kernel
        .patch_body(&PatchBodySpec {
            patches: &patches,
            closed: false,
            tolerance: 1e-6,
        })
        .map_err(failed)?;
    // A placed block is a compound of the block's shapes, moved.
    let placed = kernel
        .compound(std::slice::from_ref(&part))
        .map_err(failed)?;
    let moved = kernel
        .transform_shape(&placed, &Transform::translation([5.0, 0.0, 0.0]), None)
        .map_err(failed)?;
    let quarter = Transform {
        linear: [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        translation: [0.0, 5.0, 0.0],
    };
    let turned = kernel
        .transform_shape(&placed, &quarter, None)
        .map_err(failed)?;
    let mut scene = mitcad_serp::testfile::sample(
        &data(&solid)?,
        &data(&face)?,
        &data(&part)?,
        &data(&moved)?,
        &data(&turned)?,
    );
    match test_case {
        "" => {}
        "newer" => scene.version = mitcad_serp::NEWEST_VERSION + 1,
        other => return Err(ApiError(format!("unknown test case {other}"))),
    }
    std::fs::write(path, mitcad_serp::testfile::write(&scene))
        .map_err(|e| ApiError(format!("{path}: {e}")))
}
