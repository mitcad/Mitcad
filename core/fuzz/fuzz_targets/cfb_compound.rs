// SPDX-License-Identifier: MIT
//! A compound file (the container of `.ipt`, `.iam`, `.rvt` and `.rfa`
//! files): its header, FAT, mini FAT and directory parsed and every
//! stream read, or refused, within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_cfb::{CompoundFile, EntryKind};
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("cfb_compound", data, || {
        let Ok(file) = CompoundFile::parse(data.to_vec()) else {
            return;
        };
        let _ = file.root();
        let _ = file.children("").count();
        let paths: Vec<String> = file.entries().map(|(path, _)| path.to_owned()).collect();
        for path in &paths {
            let Some(entry) = file.find(path) else {
                continue;
            };
            if entry.kind == EntryKind::Stream {
                let _ = file.read(entry);
            }
        }
    });
});
