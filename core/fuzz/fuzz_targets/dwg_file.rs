// SPDX-License-Identifier: MIT
//! A `.dwg` drawing as the import worker reads it before any geometry is
//! built: the file header, the sections and their decompression, the
//! classes, header variables, object map and objects, the data storage;
//! then the 2D drawing, the scene of blocks and inserts with its plan, and
//! the ASM bodies of the solids' B-rep records. Each is read or refused
//! within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_asm::convert;
use mitcad_dwg::scene::{ItemKind, Scene};
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("dwg_file", data, || {
        let Ok(file) = mitcad_dwg::read(data) else {
            return;
        };
        let _ = file.type_counts();
        let _ = mitcad_dwg::flat::drawing(&file);
        let scene = Scene::of(&file);
        let _ = scene.plan();
        let options = convert::Options::default();
        for block in scene.blocks.values() {
            for item in &block.items {
                if let ItemKind::Brep { form, bytes, .. } = &item.kind
                    && let Ok(asm) = mitcad_dwg::solid::parse(*form, bytes)
                {
                    let _ = convert::convert_file(&asm, &options);
                }
            }
        }
    });
});
