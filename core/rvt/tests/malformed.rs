// SPDX-License-Identifier: MIT
//! Damaged `.rvt`/`.rfa` files (mitcad#939), made from the small
//! synthetic files of `testfile`: cut short, with a wrong signature, or
//! with a metadata string declaring more than the stream holds. Each is
//! read as far as it goes or refused, never a panic. (The schema's and the
//! records' own damage is tested beside their readers: `schema.rs`,
//! `walk.rs`, `gzip.rs`.)

use mitcad_rvt::cfb::CfbError;
use mitcad_rvt::elements::Model;
use mitcad_rvt::info::BasicFileInfo;
use mitcad_rvt::{RvtError, RvtFile, testfile};

/// Reads what the import reads, as far as it goes.
fn read_all(data: Vec<u8>) {
    let Ok(file) = RvtFile::parse(data) else {
        return;
    };
    let _ = file.part_atom();
    let _ = file.external_files();
    let _ = file.preview();
    if let Ok(schema) = file.schema() {
        let _ = Model::read(&file, &schema);
    }
}

#[test]
fn every_cut_of_a_project_and_a_family_is_read_as_far_as_it_goes_or_refused() {
    for full in [testfile::project(), testfile::family()] {
        // A prime step reaches every part of the sectors.
        for cut in (0..full.len()).step_by(97) {
            read_all(full[..cut].to_vec());
        }
        // The last sector, byte by byte.
        for cut in full.len().saturating_sub(600)..full.len() {
            read_all(full[..cut].to_vec());
        }
        read_all(full);
    }
}

#[test]
fn a_wrong_signature_is_no_compound_file() {
    let mut data = testfile::project();
    data[0] ^= 0xFF;
    assert!(matches!(
        RvtFile::parse(data),
        Err(RvtError::Container(CfbError::NotCompound))
    ));
}

#[test]
fn basic_file_info_strings_declaring_more_than_the_stream_are_left_out() {
    let mut data = testfile::basic_file_info(2024, "20240101_1515(x64)");
    // The release string's count of UTF-16 units, at offset 14.
    data[14..18].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(BasicFileInfo::parse(&data).is_ok());
    for cut in 0..data.len() {
        let _ = BasicFileInfo::parse(&data[..cut]);
    }
    assert!(BasicFileInfo::parse(&[]).is_err());
}
