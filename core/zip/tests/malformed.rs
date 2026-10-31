// SPDX-License-Identifier: MIT
//! Damaged archives (mitcad#939): cut short, with a wrong signature, or
//! declaring more than they hold. Each is an error, never a panic, and
//! what still reads is the entry as it was written.

use mitcad_zip::{Archive, Method, Writer, ZipError, stored_zip};

const TEXT: &[u8] = b"<Document><Object name=\"Box\"/></Document>";

fn archive() -> Vec<u8> {
    let mut writer = Writer::new();
    writer
        .add("Document.xml", &TEXT.repeat(20), Method::Deflate)
        .unwrap();
    writer
        .add("dir/data.bin", &[1, 2, 3, 4, 5], Method::Stored)
        .unwrap();
    writer.finish().unwrap()
}

/// Where the end of central directory record starts (no comment: the
/// last 22 bytes).
fn eocd(zip: &[u8]) -> usize {
    zip.len() - 22
}

#[test]
fn every_cut_of_an_archive_is_refused_or_reads_what_was_written() {
    let full = archive();
    let written = |name: &str| match name {
        "Document.xml" => Some(TEXT.repeat(20)),
        "dir/data.bin" => Some(vec![1, 2, 3, 4, 5]),
        _ => None,
    };
    for cut in 0..full.len() {
        let Ok(archive) = Archive::new(full[..cut].to_vec()) else {
            continue;
        };
        for entry in archive.entries() {
            if let (Ok(data), Some(expected)) = (archive.read(entry), written(&entry.name)) {
                assert_eq!(data, expected, "cut at {cut}");
            }
        }
    }
    let archive = Archive::new(full).unwrap();
    assert_eq!(
        archive.read_by_name("Document.xml").unwrap(),
        TEXT.repeat(20)
    );
}

#[test]
fn a_wrong_end_record_signature_is_no_zip() {
    let mut zip = archive();
    let at = eocd(&zip);
    zip[at] ^= 0xFF;
    assert!(matches!(Archive::new(zip), Err(ZipError::NotZip)));
}

#[test]
fn a_wrong_local_header_signature_is_corrupt() {
    let mut zip = stored_zip(&[("a.txt", b"hello")]);
    zip[0] ^= 0xFF;
    let archive = Archive::new(zip).unwrap();
    assert!(matches!(
        archive.read_by_name("a.txt"),
        Err(ZipError::Corrupt(_))
    ));
}

#[test]
fn a_central_directory_declared_past_the_end_is_corrupt() {
    let mut zip = archive();
    let at = eocd(&zip);
    let size = u32::try_from(zip.len()).unwrap();
    zip[at + 12..at + 16].copy_from_slice(&size.to_le_bytes());
    assert!(matches!(Archive::new(zip), Err(ZipError::Corrupt(_))));
}

#[test]
fn more_entries_declared_than_the_directory_holds_are_corrupt() {
    let mut zip = archive();
    let at = eocd(&zip);
    // 0xFFFE: not the ZIP64 marker 0xFFFF.
    zip[at + 10..at + 12].copy_from_slice(&0xFFFEu16.to_le_bytes());
    assert!(matches!(Archive::new(zip), Err(ZipError::Corrupt(_))));
}

#[test]
fn an_entry_declaring_more_than_it_holds_is_refused() {
    let mut zip = stored_zip(&[("a.txt", b"hello")]);
    // The central directory entry's uncompressed size (offset 24 of the
    // entry, after the 30-byte local header, the name and the data).
    let central = 30 + 5 + 5;
    zip[central + 24..central + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    let archive = Archive::new(zip).unwrap();
    assert!(archive.read_by_name("a.txt").is_err());
}

#[test]
fn a_zip64_locator_pointing_past_the_address_space_is_corrupt() {
    let zip = stored_zip(&[("a.txt", b"hello")]);
    let at = eocd(&zip);
    let mut damaged = zip[..at].to_vec();
    // A ZIP64 end of central directory locator whose record offset is the
    // largest 64-bit number.
    damaged.extend_from_slice(&0x0706_4b50u32.to_le_bytes());
    damaged.extend_from_slice(&0u32.to_le_bytes());
    damaged.extend_from_slice(&u64::MAX.to_le_bytes());
    damaged.extend_from_slice(&1u32.to_le_bytes());
    let mut end = zip[at..].to_vec();
    // An entry count of 0xFFFF sends the reader to the locator.
    end[10..12].copy_from_slice(&0xFFFFu16.to_le_bytes());
    damaged.extend_from_slice(&end);
    assert!(matches!(Archive::new(damaged), Err(ZipError::Corrupt(_))));
}
