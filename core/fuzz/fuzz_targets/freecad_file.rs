// SPDX-License-Identifier: MIT
//! A FreeCAD `.FCStd` file: the archive, `Document.xml`,
//! `GuiDocument.xml`, the lists kept in files of their own and each
//! object's stored shape data, read or refused within the budget and
//! without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_freecad::FcstdFile;
use mitcad_fuzz::within_budget;

fuzz_target!(|data: &[u8]| {
    within_budget("freecad_file", data, || {
        let Ok(file) = FcstdFile::from_bytes(data.to_vec()) else {
            return;
        };
        for object in &file.document.objects {
            let _ = file.visible(object);
            let _ = file.view(&object.name);
            let _ = file.shape_data(&object.name);
        }
    });
});
