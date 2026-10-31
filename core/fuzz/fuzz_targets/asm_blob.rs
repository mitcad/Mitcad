// SPDX-License-Identifier: MIT
//! An ASM body blob (the B-rep data of `.f3d` files, of the B-rep records
//! of `.ipt` parts and of `.dwg` solids): tokenised, its history read and
//! its bodies converted, or refused, within the budget and without a
//! panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_asm::asm::history::History;
use mitcad_asm::asm::{AsmFile, Coverage};
use mitcad_asm::convert;
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("asm_blob", data, || {
        let Ok(file) = AsmFile::parse(data) else {
            return;
        };
        let _ = Coverage::of(&file);
        let _ = convert::body_records(&file);
        let _ = convert::convert_file(&file, &convert::Options::default());
        if let Ok(Some(history)) = History::parse(&file) {
            let _ = history.states.len();
        }
    });
});
