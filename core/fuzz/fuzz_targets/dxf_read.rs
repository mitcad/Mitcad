// SPDX-License-Identifier: MIT
//! A DXF file's bytes (`mitcad_dxf::read`): read with its blocks, inserts
//! and arrays expanded within the read budget, or refused, within the time
//! budget and without a panic; what is read converts to millimetres.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_dxf::Units;
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("dxf_read", data, || {
        if let Ok(drawing) = mitcad_dxf::read(data) {
            let _ = drawing.to_millimeters(Units::Millimeters);
        }
    });
});
