// SPDX-License-Identifier: MIT
//! An `.ipt` part or `.iam` assembly as the import worker reads it before
//! any geometry is built: the compound file, the property sets, the
//! segment database, the B-rep records and their ASM bodies, the design
//! (sketches, features, parameters), the mesh features and an assembly's
//! occurrences, each read or refused within the budget and without a
//! panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_ipt::IptFile;

fuzz_target!(|data: &[u8]| {
    within_budget("ipt_file", data, || {
        let Ok(file) = IptFile::parse(data.to_vec()) else {
            return;
        };
        let _ = file.document();
        let _ = file.part_properties();
        for segment in &file.segments {
            let _ = file.segment_data(segment);
        }
        let _ = file.definitions();
        if let Ok(records) = file.brep_records() {
            for (record, bodies) in records.iter().zip(mitcad_ipt::read_bodies(&records)) {
                let _ = mitcad_ipt::history_states(&record.asm);
                let _ = bodies.map(|b| b.bodies.len());
            }
        }
        let _ = mitcad_ipt::design::read(&file, "fuzz.ipt");
        let _ = file.meshes();
        if !file.is_part() {
            let _ = mitcad_ipt::assembly::read(&file);
        }
    });
});
