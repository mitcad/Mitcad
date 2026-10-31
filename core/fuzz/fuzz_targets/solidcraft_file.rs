// SPDX-License-Identifier: MIT
//! A `.solidcraft` design, plain JSON or its zip container with blobs: the
//! format upgraded step by step, the design read, each feature's inputs
//! walked, or refused, within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_solidcraft::SolidcraftFile;

fuzz_target!(|data: &[u8]| {
    within_budget("solidcraft_file", data, || {
        let _ = mitcad_solidcraft::looks_like_design(data);
        let Ok(mut file) = SolidcraftFile::from_bytes(data) else {
            return;
        };
        file.design.add_missing_occurrences();
        let _ = file.design.active_len();
        for feature in &file.design.features {
            let _ = feature.kind_title();
            let _ = mitcad_solidcraft::inputs::inputs(&feature.def);
        }
    });
});
