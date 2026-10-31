// SPDX-License-Identifier: MIT
//! An `.rvt`/`.rfa` file's own serialisation schema (`Formats/Latest`,
//! inflated): parsed, or refused, within the budget and without a panic;
//! a parsed schema answers its classes' lineages and encodings.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_rvt::schema::Schema;

fuzz_target!(|data: &[u8]| {
    within_budget("rvt_schema", data, || {
        let Ok(schema) = Schema::parse(data) else {
            return;
        };
        let _ = schema.unknown_encodings();
        for class in &schema.classes {
            let _ = schema.lineage(class.tag);
            let _ = schema.class_name(class.tag);
        }
    });
});
