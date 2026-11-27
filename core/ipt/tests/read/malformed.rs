// SPDX-License-Identifier: MIT
//! Damaged `.ipt` and `.iam` files (mitcad#939), made from the synthetic
//! parts and assemblies of `testdata` and `testassembly`: cut short, with a
//! wrong signature, or with a property set declaring more than its stream
//! holds. Each is read as far as it goes or refused, never a panic.

use mitcad_ipt::cfb::CfbError;
use mitcad_ipt::{IptError, IptFile, props, testassembly, testdata};

/// Reads what the import worker reads before it builds any geometry.
fn read_all(data: Vec<u8>) {
    let Ok(file) = IptFile::parse(data) else {
        return;
    };
    let _ = file.document();
    let _ = file.definitions();
    if let Ok(records) = file.brep_records() {
        let _ = mitcad_ipt::read_bodies(&records);
    }
    let _ = mitcad_ipt::design::read(&file, "cut.ipt");
    let _ = file.meshes();
    if !file.is_part() {
        let _ = mitcad_ipt::assembly::read(&file);
    }
}

#[test]
fn every_cut_of_a_part_and_an_assembly_is_read_as_far_as_it_goes_or_refused() {
    let files = [
        testdata::test_part_with_design(),
        testdata::test_mesh_part(),
        testassembly::assembly_file(&testassembly::top_assembly()),
    ];
    for full in files {
        // A prime step reaches every part of the sectors.
        for cut in (0..full.len()).step_by(31) {
            read_all(full[..cut].to_vec());
        }
        read_all(full);
    }
}

#[test]
fn a_wrong_signature_is_no_compound_file() {
    let mut data = testdata::test_part();
    data[0] ^= 0xFF;
    assert_eq!(
        IptFile::parse(data).err(),
        Some(IptError::Container(CfbError::NotCompound))
    );
}

#[test]
fn property_sets_declaring_more_than_their_stream_are_refused() {
    let stream = props::write([7; 16], &[]);
    assert!(props::parse(&stream).is_ok());
    // The set's property count (after its size), at the offset the
    // stream's set list gives (after the 28-byte header and the FMTID).
    let base = u32::from_le_bytes(stream[44..48].try_into().unwrap()) as usize;
    let mut damaged = stream.clone();
    damaged[base + 4..base + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(props::parse(&damaged).is_err());
    // More property sets than a stream has.
    let mut damaged = stream.clone();
    damaged[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(props::parse(&damaged).is_err());
    // No byte order mark.
    let mut damaged = stream.clone();
    damaged[0] = 0;
    assert!(props::parse(&damaged).is_err());
    for cut in 0..stream.len() {
        let _ = props::parse(&stream[..cut]);
    }
}
