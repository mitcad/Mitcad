// SPDX-License-Identifier: MIT
//! A `.3dm` model as the import reads it before any geometry is built
//! (mitcad#1715): the chunks, the settings, layers, block definitions and
//! the object table; then every B-rep, extrusion and surface converted to
//! the neutral B-rep model, and the B-reps' render meshes joined. Each is
//! read or refused within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_3dm::geom::Geometry;
use mitcad_3dm::{File3dm, convert};
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("model_3dm", data, || {
        let Ok(file) = File3dm::read(data) else {
            return;
        };
        let _ = mitcad_3dm::summary(&file);
        let s = file.units.mm;
        for o in &file.objects {
            match &o.geometry {
                Geometry::Brep(b) => {
                    let _ = convert::brep_body(b, s);
                    let _ = convert::render_mesh(b).map(|m| m.triangles());
                }
                Geometry::Extrusion(e) => {
                    let _ = convert::extrusion_body(e, s);
                }
                Geometry::Surface(srf) => {
                    let _ = convert::surface_body(srf, s);
                }
                Geometry::Mesh(m) => {
                    let _ = m.triangles();
                }
                Geometry::Curve(c) => {
                    let _ = convert::to_spline(c).map(|spline| spline.to_brep(s));
                }
                Geometry::Instance(_) | Geometry::Other(_) => {}
            }
        }
    });
});
