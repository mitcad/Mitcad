// SPDX-License-Identifier: MIT
//! Damaged `.dwg` drawings (mitcad#939), made from the synthetic test
//! drawing of `testfile`: cut short in each container layout, with a
//! version id that is no drawing's, or with a section declaring more than
//! the file holds. Each is read as far as it goes or refused, never a
//! panic. (Damaged pages of the R2007 layout are tested in `r2007.rs`.)

use mitcad_dwg::scene::Scene;
use mitcad_dwg::{DwgError, Version, testfile};

/// Reads what the import worker reads before it builds any geometry.
fn read_all(data: &[u8]) {
    let Ok(file) = mitcad_dwg::read(data) else {
        return;
    };
    let _ = mitcad_dwg::flat::drawing(&file);
    let _ = Scene::of(&file).plan();
}

#[test]
fn every_cut_of_a_drawing_is_read_as_far_as_it_goes_or_refused() {
    let drawing = testfile::test_drawing();
    for version in [
        Version::R2000,
        Version::R2004,
        Version::R2007,
        Version::R2018,
    ] {
        let full = testfile::write(&drawing, version);
        // A prime step reaches every part of the sections.
        for cut in (0..full.len()).step_by(13) {
            read_all(&full[..cut]);
        }
        read_all(&full);
    }
}

#[test]
fn version_ids_of_no_drawing_are_refused() {
    let mut data = testfile::write(&testfile::test_drawing(), Version::R2000);
    data[..6].copy_from_slice(b"PK\x03\x04\x14\0");
    assert!(matches!(mitcad_dwg::read(&data), Err(DwgError::NotDwg(_))));
    data[..6].copy_from_slice(b"AC1009");
    assert!(matches!(
        mitcad_dwg::read(&data),
        Err(DwgError::Unsupported(_))
    ));
    assert!(matches!(
        mitcad_dwg::read(b"AC10"),
        Err(DwgError::NotDwg(_))
    ));
}

#[test]
fn a_section_declaring_more_than_the_file_holds_is_corrupt() {
    let mut data = testfile::write(&testfile::test_drawing(), Version::R2000);
    // The first section locator's size (the R13 to R2000 file header: the
    // locator count at 0x15, then 9-byte locators from 0x19).
    data[0x19 + 5..0x19 + 9].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(mitcad_dwg::read(&data), Err(DwgError::Corrupt(_))));
    // More locators than the header has room for.
    let mut data = testfile::write(&testfile::test_drawing(), Version::R2000);
    data[0x15..0x19].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(mitcad_dwg::read(&data), Err(DwgError::Corrupt(_))));
}

/// The data storage of R2013 and later cut anywhere, its data segment's
/// declared end past the bytes left (mitcad#939: the `dwg_file` fuzz
/// target's finding, a record sliced past the section's end): an error,
/// never a panic.
#[test]
fn every_cut_of_a_data_storage_is_refused_or_read() {
    use mitcad_dwg::datastore::{brep_records, write};
    let asm = mitcad_asm::testdata::cube_blob();
    let records = vec![(0x2a, asm.clone()), (0x2b, b"not a body".to_vec())];
    let full = write(&records);
    for cut in 0..full.len() {
        if let Ok(read) = brep_records(&full[..cut]) {
            for bytes in read.values() {
                assert_eq!(bytes, &asm, "cut at {cut}");
            }
        }
    }
    assert_eq!(brep_records(&full).unwrap()[&0x2a], asm);
}
