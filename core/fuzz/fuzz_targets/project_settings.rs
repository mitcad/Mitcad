// SPDX-License-Identifier: MIT
//! A project's settings as someone else may have written them (the shared
//! settings of `project.json` and the per-user ones), strict and lenient:
//! read or refused within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_model::file::settings::{LocalSettings, SharedSettings};

fuzz_target!(|data: &[u8]| {
    within_budget("project_settings", data, || {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else {
            return;
        };
        for strict in [false, true] {
            let _ = SharedSettings::from_json(&value, strict);
            let _ = LocalSettings::from_json(&value, strict);
        }
    });
});
