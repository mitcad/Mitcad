// SPDX-License-Identifier: MIT
//! A 3MF package: its settings and files (`mitcad_3mf::read_package`) and
//! its model (`mitcad_3mf::read`) read, or refused, within the budget and
//! without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("package_3mf", data, || {
        if let Ok(package) = mitcad_3mf::read_package(data) {
            let _ = package.file("Metadata/project_settings.config");
            let _ = package.metadata("Title");
        }
        let _ = mitcad_3mf::read(data);
    });
});
