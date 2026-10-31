// SPDX-License-Identifier: MIT
//! Damaged compound files (mitcad#939): cut short, with a wrong
//! signature, or with a stream declaring more than the file holds. Each is
//! an error, never a panic, and a stream that still reads is the stream as
//! it was written.

use mitcad_cfb::{CfbError, CompoundFile, Writer};

fn big() -> Vec<u8> {
    (0..6000u32).map(|i| (i * 7 % 251) as u8).collect()
}

/// A file with a small stream (in the mini stream) and a large one (in
/// sectors of their own) inside a storage.
fn file(version: u16) -> Vec<u8> {
    let mut writer = Writer::new();
    writer
        .version(version)
        .stream("Top", b"small stream")
        .stream("Store/Big", &big());
    writer.finish()
}

fn u16_at(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([data[at], data[at + 1]])
}

fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]])
}

/// Where the directory entry named `name` starts, searched in the
/// directory's first sector.
fn entry_at(data: &[u8], name: &str) -> usize {
    let sector_size = 1usize << u16_at(data, 0x1E);
    let first = (u32_at(data, 0x30) as usize + 1) * sector_size;
    let units: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    (first..first + sector_size)
        .step_by(128)
        .find(|&at| data[at..].starts_with(&units) && data[at + units.len()] == 0)
        .expect("the entry is in the directory's first sector")
}

#[test]
fn every_cut_of_a_file_is_refused_or_reads_what_was_written() {
    for version in [3, 4] {
        let full = file(version);
        for cut in (0..full.len()).step_by(3) {
            let Ok(file) = CompoundFile::parse(full[..cut].to_vec()) else {
                continue;
            };
            if let Ok(data) = file.read_path("Top") {
                assert_eq!(data, b"small stream", "version {version}, cut at {cut}");
            }
            if let Ok(data) = file.read_path("Store/Big") {
                assert_eq!(data, big(), "version {version}, cut at {cut}");
            }
        }
        let whole = CompoundFile::parse(full).unwrap();
        assert_eq!(whole.read_path("Store/Big").unwrap(), big());
    }
}

#[test]
fn a_wrong_signature_is_no_compound_file() {
    for version in [3, 4] {
        let mut data = file(version);
        data[0] ^= 0xFF;
        assert!(matches!(
            CompoundFile::parse(data),
            Err(CfbError::NotCompound)
        ));
    }
}

#[test]
fn a_stream_declaring_more_than_the_file_holds_is_corrupt() {
    for version in [3, 4] {
        let mut data = file(version);
        let at = entry_at(&data, "Top");
        // The entry's stream size (a u64 at 0x78; version 3 reads the low
        // half).
        data[at + 0x78..at + 0x80].copy_from_slice(&0xFFFF_FFF0u64.to_le_bytes());
        // Refused as a whole, or the stream is.
        if let Ok(file) = CompoundFile::parse(data) {
            assert!(matches!(file.read_path("Top"), Err(CfbError::Corrupt(_))));
        }
    }
}

#[test]
fn a_fat_declared_larger_than_the_file_is_refused() {
    for version in [3, 4] {
        let mut data = file(version);
        data[0x2C..0x30].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            CompoundFile::parse(data),
            Err(CfbError::Header(_))
        ));
    }
}
