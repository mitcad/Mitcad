// SPDX-License-Identifier: MIT
//! Damaged IFC text (mitcad#939), made from the synthetic building of
//! `testdata`: cut short anywhere, not ISO 10303-21 text at all, or with
//! numbers and strings beyond what the reader keeps. Each is read as far
//! as it goes or refused, never a panic. (Deep nesting is tested beside
//! the reader, `lib.rs`.)

use mitcad_ifc::geometry::Decoder;
use mitcad_ifc::model::Model;
use mitcad_ifc::{IfcFile, ReadOptions, testdata};

/// Reads what the import reads before it builds any geometry: the
/// entities, the model and every entity's representation item.
fn read_all(text: &[u8]) -> Option<usize> {
    let file = IfcFile::read(text, &ReadOptions::all()).ok()?;
    let model = Model::read(&file);
    let decoder = Decoder::new(&file, &model.units);
    let mut ids: Vec<u64> = file.entities.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let _ = decoder.item(id);
    }
    Some(file.entities.len())
}

#[test]
fn every_cut_of_a_file_is_read_as_far_as_it_goes_or_refused() {
    let full = testdata::BUILDING.as_bytes();
    let whole = read_all(full).expect("the building reads");
    for cut in (0..full.len()).step_by(7) {
        if let Some(entities) = read_all(&full[..cut]) {
            assert!(entities <= whole, "cut at {cut}: {entities} of {whole}");
        }
    }
}

#[test]
fn other_files_are_refused() {
    for other in [
        &b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..],
        b"PK\x03\x04\x14\0\0\0\x08\0",
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1\0\0",
        b"\xff\xfe\0\0",
    ] {
        assert!(IfcFile::read(other, &ReadOptions::all()).is_err());
    }
}

#[test]
fn numbers_and_strings_beyond_the_reader_are_records_not_read() {
    let text = "ISO-10303-21;HEADER;ENDSEC;DATA;\
                #99999999999999999999999=IFCCARTESIANPOINT((0.,0.,0.));\
                #2=IFCCARTESIANPOINT((1e999999,-1e999999,0.));\
                #3=IFCDIRECTION((0.,0.,1.),#18446744073709551615);\
                #4=IFCLABEL('a string that never ends);\
                ENDSEC;";
    let _ = read_all(text.as_bytes());
    let _ = read_all(&text.as_bytes()[..text.len() / 2]);
}
