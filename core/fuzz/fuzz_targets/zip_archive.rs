// SPDX-License-Identifier: MIT
//! A zip archive (`.f3d`, `.FCStd`, `.3mf`, `.ifczip`): its central
//! directory read and every entry decompressed, or refused, within the
//! budget and without a panic; an entry that reads has its declared size
//! (at most the default limit) and its CRC-32.
#![no_main]

use libfuzzer_sys::fuzz_target;
use mitcad_fuzz::within_budget;
use mitcad_zip::{Archive, DEFAULT_ENTRY_LIMIT};

fuzz_target!(|data: &[u8]| {
    within_budget("zip_archive", data, || {
        let Ok(archive) = Archive::new(data.to_vec()) else {
            return;
        };
        for entry in archive.entries() {
            let _ = archive.find(&entry.name);
            let _ = archive.data_offset(entry);
            let _ = archive.raw(entry);
            if let Ok(bytes) = archive.read(entry) {
                assert!(entry.size <= DEFAULT_ENTRY_LIMIT);
                assert_eq!(bytes.len() as u64, entry.size);
                assert_eq!(mitcad_zip::crc32(&bytes), entry.crc32);
            }
        }
    });
});
