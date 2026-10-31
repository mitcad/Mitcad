// SPDX-License-Identifier: MIT
//! A design file of any version (`Document::from_json`, with the mock
//! kernel: nothing is evaluated): loaded with its B-rep data inside the
//! file's budget, or refused, within the time budget and without a panic;
//! a loaded design saves again.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_model::Document;
use mitcad_model::testing::MockKernel;

fuzz_target!(|data: &[u8]| {
    within_budget("design_file", data, || {
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        if let Ok(document) = Document::from_json(text, MockKernel::default()) {
            let _ = document.to_json();
        }
    });
});
