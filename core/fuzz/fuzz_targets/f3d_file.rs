// SPDX-License-Identifier: MIT
//! The container of an `.f3d` file or `.f3z` package: its entries and
//! their kinds, the properties, the design segments, the body blobs (read
//! and their ASM data parsed and converted) and where the segments name
//! them, and the nested documents one level down. Each is read or refused
//! within the budget and without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_asm::convert;
use mitcad_f3d_file::F3dFile;
use mitcad_fuzz::within_budget;

/// Reads what the container lists; `nested` opens the documents of an
/// `.f3z` package (one level: a package inside a package is not followed).
fn read(file: &F3dFile, nested: bool) {
    let _ = file.entries();
    let _ = file.properties();
    let _ = file.root_document();
    let _ = file.segments();
    let _ = file.blob_references();
    let options = convert::Options::default();
    for blob in file.body_blobs() {
        if let Ok(bytes) = file.read_large(&blob.entry) {
            let _ = mitcad_asm::read_blob(&blob.entry, &bytes, &options);
        }
    }
    if nested {
        for name in file.documents() {
            if let Ok(inner) = file.open_document(&name) {
                read(&inner, false);
            }
        }
    }
}

fuzz_target!(|data: &[u8]| {
    within_budget("f3d_file", data, || {
        if let Ok(file) = F3dFile::from_bytes(data.to_vec()) {
            read(&file, true);
        }
    });
});
