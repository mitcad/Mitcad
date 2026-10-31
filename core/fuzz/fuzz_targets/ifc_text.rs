// SPDX-License-Identifier: MIT
//! IFC text (ISO 10303-21) as the import reads it before any geometry is
//! built: the records and entities, the model (units, spatial structure,
//! placements, products) and the representation items of the entities
//! decoded into curves, profiles and solids. Each is read or refused
//! within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_ifc::geometry::Decoder;
use mitcad_ifc::model::Model;
use mitcad_ifc::{IfcFile, ReadOptions};

/// The most entities whose items are decoded per input: each one may
/// reach many others, and the decoder's own limits bound each.
const ITEMS: usize = 512;

fuzz_target!(|data: &[u8]| {
    within_budget("ifc_text", data, || {
        let Ok(file) = IfcFile::read(data, &ReadOptions::all()) else {
            return;
        };
        let model = Model::read(&file);
        let decoder = Decoder::new(&file, &model.units);
        let mut ids: Vec<u64> = file.entities.keys().copied().collect();
        ids.sort_unstable();
        for id in ids.into_iter().take(ITEMS) {
            let _ = decoder.item(id);
        }
    });
});
