// SPDX-License-Identifier: MIT
//! A whole `.rvt`/`.rfa` file: the compound file, the metadata streams,
//! the schema, the element table, the partitions and the geometry and
//! profiles of the elements (`Model::read`), or the error that refuses
//! it, within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_rvt::RvtFile;
use mitcad_rvt::elements::Model;

fuzz_target!(|data: &[u8]| {
    within_budget("rvt_file", data, || {
        let Ok(file) = RvtFile::parse(data.to_vec()) else {
            return;
        };
        let _ = file.streams();
        let _ = file.part_atom();
        let _ = file.external_files();
        let _ = file.episodes();
        let _ = file.preview();
        let _ = file.release_by_table();
        let Ok(schema) = file.schema() else {
            return;
        };
        for number in file.partition_numbers().into_iter().take(16) {
            let _ = file.documents(number, &schema, true);
        }
        let _ = Model::read(&file, &schema);
    });
});
