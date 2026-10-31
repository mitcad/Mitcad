// SPDX-License-Identifier: MIT
//! Damaged `.FCStd` files (mitcad#939), made from the small hand-made
//! documents of `tests/data`: cut short anywhere, or declaring more than
//! they hold (a `Document.xml` entry over the XML limit, list files whose
//! counts exceed their bytes). Each is read as far as it goes or refused,
//! never a panic. (What is no FreeCAD document at all and hostile nesting
//! are tested in `parse.rs` and `xml.rs`.)

use mitcad_freecad::binary::{float_list, placement_list, vector_list};
use mitcad_freecad::{Document, FcstdError, FcstdFile, GuiDocument, Sketch};
use mitcad_zip::{Method, Writer, stored_zip};

const DOCUMENT_0_21: &str = include_str!("data/document_0_21.xml");
const GUI: &str = include_str!("data/gui_document.xml");

fn archive() -> Vec<u8> {
    let mut writer = Writer::new();
    writer
        .add("Document.xml", DOCUMENT_0_21.as_bytes(), Method::Deflate)
        .unwrap();
    writer
        .add("GuiDocument.xml", GUI.as_bytes(), Method::Deflate)
        .unwrap();
    writer.finish().unwrap()
}

#[test]
fn every_cut_of_a_file_is_refused_or_read() {
    let full = archive();
    for cut in 0..full.len() {
        if let Ok(file) = FcstdFile::from_bytes(full[..cut].to_vec()) {
            for object in &file.document.objects {
                let _ = file.shape_data(&object.name);
            }
        }
    }
    assert!(FcstdFile::from_bytes(full).is_ok());
}

#[test]
fn every_cut_of_the_xml_is_read_as_far_as_it_goes_or_refused() {
    for cut in (0..DOCUMENT_0_21.len()).filter(|&i| DOCUMENT_0_21.is_char_boundary(i)) {
        if let Ok(document) = Document::parse(&DOCUMENT_0_21[..cut]) {
            for object in &document.objects {
                let _ = Sketch::of(object);
            }
        }
    }
    for cut in (0..GUI.len()).filter(|&i| GUI.is_char_boundary(i)) {
        let _ = GuiDocument::parse(&GUI[..cut]);
    }
}

#[test]
fn a_document_entry_declaring_more_than_the_xml_limit_is_refused() {
    let xml = DOCUMENT_0_21.as_bytes();
    let mut zip = stored_zip(&[("Document.xml", xml)]);
    // The central directory entry's uncompressed size (offset 24), after
    // the 30-byte local header, the name and the stored data.
    let central = 30 + "Document.xml".len() + xml.len();
    zip[central + 24..central + 28].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
    assert!(matches!(
        FcstdFile::from_bytes(zip),
        Err(FcstdError::Xml { .. })
    ));
}

#[test]
fn list_files_counting_more_than_they_hold_are_errors() {
    let mut data = u32::MAX.to_le_bytes().to_vec();
    data.extend_from_slice(&[0; 56]);
    assert!(placement_list(&data).is_err());
    assert!(vector_list(&data).is_err());
    assert!(float_list(&data).is_err());
    for cut in 0..data.len() {
        let _ = placement_list(&data[..cut]);
    }
}
