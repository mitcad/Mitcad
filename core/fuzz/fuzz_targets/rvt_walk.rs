// SPDX-License-Identifier: MIT
//! An element record walked by a file's schema (`mitcad_rvt::walk`): the
//! schemas are those of the small synthetic project and family files of
//! `mitcad_rvt::testfile`, the input's first byte picks one and the rest
//! is the record (from its class tag). The walk, the record's JSON, its
//! element header and the geometry read from it finish within the budget
//! and without a panic.
#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_rvt::schema::Schema;
use mitcad_rvt::{RvtFile, testfile};

/// The schemas of the synthetic files, in the order the seeds number
/// them (`examples/write_seeds.rs`).
fn schemas() -> &'static [Schema] {
    static SCHEMAS: OnceLock<Vec<Schema>> = OnceLock::new();
    SCHEMAS.get_or_init(|| {
        [
            testfile::project(),
            testfile::family(),
            testfile::building::house(),
        ]
        .into_iter()
        .map(|bytes| {
            RvtFile::parse(bytes)
                .and_then(|file| file.schema())
                .expect("the synthetic files have schemas")
        })
        .collect()
    })
}

fuzz_target!(|data: &[u8]| {
    let Some((&choice, record)) = data.split_first() else {
        return;
    };
    let schemas = schemas();
    let schema = &schemas[usize::from(choice) % schemas.len()];
    within_budget("rvt_walk", record, || {
        let (walked, _end, _error) = mitcad_rvt::walk::walk_partial(schema, record);
        if let Some(walked) = walked {
            let _ = walked.to_json(schema);
            let _ = mitcad_rvt::elements::Header::read(schema, &walked);
            let _ = mitcad_rvt::brep::read(schema, &walked);
        }
        let _ = mitcad_rvt::walk::walk(schema, record);
    });
});
