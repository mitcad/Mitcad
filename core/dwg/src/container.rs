// SPDX-License-Identifier: MIT
//! The file's container: where its sections are (the specification's
//! chapters 3 to 8).
//!
//! - R13 to R2000: the section locator records after the version, with the
//!   header variables, the classes and the object map at absolute
//!   addresses; objects are read at the absolute addresses the map gives.
//! - R2004 and its successors (R2010, R2013, R2018): a masked file header,
//!   the section page map and the section map (system pages), and the data
//!   sections in pages, each compressed (`crate::lz`).
//! - R2007: its own pages, Reed-Solomon coded and compressed otherwise
//!   (`crate::r2007`).

use std::collections::BTreeMap;

use crate::{DwgError, Version, lz};

/// The sentinel before the header variables.
pub const HEADER_SENTINEL: [u8; 16] = [
    0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F,
];
/// The sentinel after the header variables.
pub const HEADER_END_SENTINEL: [u8; 16] = [
    0x30, 0x84, 0xE0, 0xDC, 0x02, 0x21, 0xC7, 0x56, 0xA0, 0x83, 0x97, 0x47, 0xB1, 0x92, 0xCC, 0xA0,
];
/// The sentinel before the classes.
pub const CLASSES_SENTINEL: [u8; 16] = [
    0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A,
];
/// The sentinel after the classes.
pub const CLASSES_END_SENTINEL: [u8; 16] = [
    0x72, 0x5E, 0x3B, 0x47, 0x3B, 0x56, 0x07, 0x3A, 0x3F, 0x23, 0x0B, 0xA0, 0x18, 0x30, 0x49, 0x75,
];
/// The sentinel after the R13-R15 file header.
pub const FILE_HEADER_END_SENTINEL: [u8; 16] = [
    0x95, 0xA0, 0x4E, 0x28, 0x99, 0x82, 0x1A, 0xE5, 0x5E, 0x41, 0xE0, 0x5F, 0x9D, 0x3A, 0x4D, 0x00,
];

/// The section page types of the R2004 format.
pub const PAGE_MAP_TYPE: u32 = 0x4163_0e3b;
pub const SECTION_MAP_TYPE: u32 = 0x4163_003b;
pub const DATA_PAGE_TYPE: u32 = 0x4163_043b;

/// The names of the data sections the reader takes.
pub const HEADER: &str = "AcDb:Header";
pub const CLASSES: &str = "AcDb:Classes";
pub const HANDLES: &str = "AcDb:Handles";
pub const OBJECTS: &str = "AcDb:AcDbObjects";
pub const DATA_STORAGE: &str = "AcDb:AcDsPrototype_1b";

/// The sections of a file, as bytes.
#[derive(Clone, Debug, Default)]
pub struct Sections {
    /// The maintenance release of the saving version.
    pub maintenance: u8,
    pub codepage: u16,
    /// From the header variables' sentinel on.
    pub header: Vec<u8>,
    /// From the classes' sentinel on.
    pub classes: Vec<u8>,
    /// The object map.
    pub handles: Vec<u8>,
    /// The objects: the whole file for R13 to R2000 (the map gives
    /// absolute addresses), else the objects section.
    pub objects: Vec<u8>,
    /// The data storage section (R2013 on): the bodies' B-rep records.
    pub data_storage: Option<Vec<u8>>,
    /// Every section the file lists, with its size.
    pub listed: BTreeMap<String, u64>,
}

fn corrupt(msg: impl Into<String>) -> DwgError {
    DwgError::Corrupt(msg.into())
}

/// The `len` bytes from `at`, None when they are not all in `b` (also
/// when `at + len` overflows: both are often fields of the file).
pub(crate) fn span(b: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    b.get(at..at.checked_add(len)?)
}

/// The most bytes a section or a page of a file may claim: 64 times the
/// file, at least 1 MiB. A size beyond it is a damaged field, refused
/// before anything is allocated for it.
pub(crate) fn size_limit(file: &[u8]) -> usize {
    file.len().saturating_mul(64).max(1 << 20)
}

/// The largest page of a data section the format writes (decompressed).
pub const MAX_PAGE_SIZE: usize = 0x7400;

fn le32(b: &[u8], at: usize) -> Result<u32, DwgError> {
    span(b, at, 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| {
            corrupt(format!(
                "the file ends before byte {}",
                at.saturating_add(4)
            ))
        })
}

fn le64(b: &[u8], at: usize) -> Result<u64, DwgError> {
    span(b, at, 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| {
            corrupt(format!(
                "the file ends before byte {}",
                at.saturating_add(8)
            ))
        })
}

/// The version of a file by its first six bytes.
pub fn version_of(file: &[u8]) -> Result<Version, DwgError> {
    let id = file
        .get(..6)
        .ok_or_else(|| DwgError::NotDwg("too short".into()))?;
    let id = std::str::from_utf8(id).map_err(|_| DwgError::NotDwg("no version id".into()))?;
    Version::from_id(id).ok_or_else(|| match id {
        "AC1.40" | "AC1.50" | "AC2.10" | "AC1001" | "AC1002" | "AC1003" | "AC1004" | "AC1006"
        | "AC1009" => DwgError::Unsupported(format!(
            "a drawing of a release before R13 ({id}); open it in a newer release and save it again"
        )),
        _ if id.starts_with("AC") => DwgError::Unsupported(format!("drawing version {id}")),
        _ => DwgError::NotDwg(format!(
            "the file starts with {id:?}, not a drawing version"
        )),
    })
}

/// Reads the sections of a file.
pub fn read(file: &[u8]) -> Result<(Version, Sections), DwgError> {
    let version = version_of(file)?;
    let sections = match version {
        Version::R13 | Version::R14 | Version::R2000 => read_r13(file)?,
        Version::R2007 => crate::r2007::read(file)?,
        _ => read_r2004(file)?,
    };
    Ok((version, sections))
}

/// R13 to R2000: the section locators.
fn read_r13(file: &[u8]) -> Result<Sections, DwgError> {
    let mut s = Sections {
        maintenance: *file.get(11).unwrap_or(&0),
        codepage: u16::from_le_bytes([
            *file.get(0x13).unwrap_or(&0),
            *file.get(0x14).unwrap_or(&0),
        ]),
        ..Default::default()
    };
    let count = le32(file, 0x15)? as usize;
    if count > 16 {
        return Err(corrupt(format!("{count} section locators")));
    }
    for i in 0..count {
        let at = 0x19 + i * 9;
        let number = *file
            .get(at)
            .ok_or_else(|| corrupt("the locators end early"))?;
        let seeker = le32(file, at + 1)? as usize;
        let size = le32(file, at + 5)? as usize;
        let bytes = || {
            file.get(seeker..seeker + size)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| corrupt(format!("section {number} lies past the end of the file")))
        };
        let name = match number {
            0 => {
                s.header = bytes()?;
                HEADER
            }
            1 => {
                s.classes = bytes()?;
                CLASSES
            }
            2 => {
                s.handles = bytes()?;
                HANDLES
            }
            3 => "unknown section",
            4 => "AcDb:Template",
            _ => "unnamed section",
        };
        s.listed.insert(format!("{name} ({number})"), size as u64);
    }
    if s.header.is_empty() || s.handles.is_empty() {
        return Err(corrupt("no header variables or object map"));
    }
    s.objects = file.to_vec();
    Ok(s)
}

/// The sequence the R2004 file header and page fill are masked with.
pub fn magic_sequence(n: usize) -> Vec<u8> {
    let mut seed: u32 = 1;
    (0..n)
        .map(|_| {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            (seed >> 16) as u8
        })
        .collect()
}

/// The decoded R2004 file header data (at 0x80, 0x6c bytes).
#[derive(Clone, Debug, Default)]
pub struct R2004Header {
    pub page_map_id: u32,
    pub page_map_address: u64,
    pub section_map_id: u32,
    pub page_count: u32,
}

/// Reads the masked file header data.
pub fn r2004_header(file: &[u8]) -> Result<R2004Header, DwgError> {
    let raw = file
        .get(0x80..0x80 + 0x6c)
        .ok_or_else(|| corrupt("no file header data"))?;
    let mask = magic_sequence(0x6c);
    let h: Vec<u8> = raw.iter().zip(&mask).map(|(a, b)| a ^ b).collect();
    if &h[..11] != b"AcFssFcAJMB" {
        return Err(corrupt("the file header data has no file id"));
    }
    Ok(R2004Header {
        page_map_id: le32(&h, 0x50)?,
        page_map_address: le64(&h, 0x54)?.saturating_add(0x100),
        section_map_id: le32(&h, 0x5c)?,
        page_count: le32(&h, 0x40)?,
    })
}

/// A system page (the page map, the section map): its 0x14-byte header,
/// then its compressed data.
fn system_page(file: &[u8], address: usize, kind: u32) -> Result<Vec<u8>, DwgError> {
    let found = le32(file, address)?;
    if found != kind {
        return Err(corrupt(format!(
            "the system page at {address:#x} is of type {found:#x}, not {kind:#x}"
        )));
    }
    // `le32` above found four bytes at `address`: the additions below do
    // not overflow.
    let size = le32(file, address + 4)? as usize;
    let compressed = le32(file, address + 8)? as usize;
    let method = le32(file, address + 12)?;
    if size > size_limit(file) {
        return Err(corrupt(format!(
            "the system page at {address:#x} claims {size} bytes from a file of {}",
            file.len()
        )));
    }
    let data = span(file, address + 0x14, compressed)
        .ok_or_else(|| corrupt(format!("the system page at {address:#x} runs past the end")))?;
    if method == 2 {
        let mut out = lz::decompress(data, size)?;
        out.resize(size, 0);
        Ok(out)
    } else {
        Ok(data.to_vec())
    }
}

/// A data section of the section map.
#[derive(Clone, Debug)]
pub struct SectionInfo {
    pub name: String,
    pub size: u64,
    pub page_size: u64,
    pub compressed: bool,
    pub encrypted: u32,
    /// Page numbers with their stored data size and start offset.
    pub pages: Vec<(u32, u32, u64)>,
}

/// The section map's entries.
fn section_map(data: &[u8]) -> Result<Vec<SectionInfo>, DwgError> {
    let count = le32(data, 0)? as usize;
    let mut at = 0x14;
    let mut out = Vec::new();
    for _ in 0..count {
        let size = le64(data, at)?;
        let page_count = le32(data, at + 8)? as usize;
        let page_size = u64::from(le32(data, at + 12)?);
        let compressed = le32(data, at + 0x14)? == 2;
        let encrypted = le32(data, at + 0x1c)?;
        let name_bytes = data
            .get(at + 0x20..at + 0x60)
            .ok_or_else(|| corrupt("the section map ends early"))?;
        let name = crate::bits::latin1(name_bytes);
        at += 0x60;
        // Each page takes 16 bytes of the map: no more than it holds.
        let mut pages = Vec::with_capacity(page_count.min(data.len() / 16));
        for _ in 0..page_count {
            pages.push((le32(data, at)?, le32(data, at + 4)?, le64(data, at + 8)?));
            at += 16;
        }
        out.push(SectionInfo {
            name,
            size,
            page_size,
            compressed,
            encrypted,
            pages,
        });
    }
    Ok(out)
}

/// R2004 and its successors.
fn read_r2004(file: &[u8]) -> Result<Sections, DwgError> {
    let security = le32(file, 0x18)?;
    if security & 1 != 0 {
        return Err(DwgError::Unsupported(
            "a drawing saved with a password (its data is encrypted)".into(),
        ));
    }
    let header = r2004_header(file)?;
    // The page map: page numbers and sizes; pages follow each other from
    // 0x100.
    let map = system_page(file, header.page_map_address as usize, PAGE_MAP_TYPE)?;
    let mut addresses: BTreeMap<u32, (u64, u32)> = BTreeMap::new();
    let mut address = 0x100u64;
    let mut at = 0;
    while at + 8 <= map.len() {
        let number = le32(&map, at)? as i32;
        let size = le32(&map, at + 4)?;
        at += 8;
        if number < 0 {
            // A gap: parent, left, right and a zero follow.
            at += 16;
        } else if number > 0 {
            addresses.insert(number as u32, (address, size));
        }
        address = address.saturating_add(u64::from(size));
    }
    let (map_address, _) = addresses
        .get(&header.section_map_id)
        .copied()
        .ok_or_else(|| corrupt("the page map has no section map page"))?;
    let infos = section_map(&system_page(file, map_address as usize, SECTION_MAP_TYPE)?)?;

    let mut s = Sections {
        maintenance: *file.get(11).unwrap_or(&0),
        codepage: u16::from_le_bytes([
            *file.get(0x13).unwrap_or(&0),
            *file.get(0x14).unwrap_or(&0),
        ]),
        ..Default::default()
    };
    for info in &infos {
        s.listed.insert(info.name.clone(), info.size);
    }
    let get = |name: &str| -> Result<Option<Vec<u8>>, DwgError> {
        let Some(info) = infos.iter().find(|i| i.name == name) else {
            return Ok(None);
        };
        if info.encrypted == 1 {
            return Err(DwgError::Unsupported(format!(
                "the encrypted section {name}"
            )));
        }
        Ok(Some(section_data(file, info, &addresses)?))
    };
    s.header = get(HEADER)?.ok_or_else(|| corrupt("no header section"))?;
    s.classes = get(CLASSES)?.unwrap_or_default();
    s.handles = get(HANDLES)?.ok_or_else(|| corrupt("no object map section"))?;
    s.objects = get(OBJECTS)?.ok_or_else(|| corrupt("no objects section"))?;
    s.data_storage = get(DATA_STORAGE)?;
    Ok(s)
}

/// The bytes of a data section: its pages decompressed and put at their
/// start offsets (pages of zeros are not stored).
fn section_data(
    file: &[u8],
    info: &SectionInfo,
    addresses: &BTreeMap<u32, (u64, u32)>,
) -> Result<Vec<u8>, DwgError> {
    let size = usize::try_from(info.size).map_err(|_| corrupt("a section too large"))?;
    if size > size_limit(file) {
        return Err(corrupt(format!(
            "the section {} claims {size} bytes from a file of {}",
            info.name,
            file.len()
        )));
    }
    let mut out = vec![0u8; size];
    let page_size = info.page_size as usize;
    for &(number, _, start) in &info.pages {
        let (address, _) = *addresses.get(&number).ok_or_else(|| {
            corrupt(format!(
                "{}: page {number} is not in the page map",
                info.name
            ))
        })?;
        let address = usize::try_from(address).unwrap_or(usize::MAX);
        let raw = span(file, address, 32)
            .ok_or_else(|| corrupt(format!("{}: page {number} lies past the end", info.name)))?;
        let mask = 0x4164_536b_u32 ^ address as u32;
        let mut head = [0u32; 8];
        for (i, h) in head.iter_mut().enumerate() {
            *h = u32::from_le_bytes([raw[4 * i], raw[4 * i + 1], raw[4 * i + 2], raw[4 * i + 3]])
                ^ mask;
        }
        if head[0] != DATA_PAGE_TYPE {
            return Err(corrupt(format!(
                "{}: page {number} is of type {:#x}",
                info.name, head[0]
            )));
        }
        let data_size = head[2] as usize;
        let data = span(file, address + 32, data_size)
            .ok_or_else(|| corrupt(format!("{}: page {number} runs past the end", info.name)))?;
        let start = usize::try_from(start).map_err(|_| corrupt("a page offset too large"))?;
        if start >= size {
            continue;
        }
        let want = page_size.min(size - start);
        let page = if info.compressed {
            // The page size the map gives is a field of the file: the
            // output may reach the format's page size or what the section
            // still takes, no more (mitcad#931).
            lz::decompress(data, page_size.min(MAX_PAGE_SIZE).max(want))?
        } else {
            data.to_vec()
        };
        let n = want.min(page.len());
        out[start..start + n].copy_from_slice(&page[..n]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_sequence_of_the_specification() {
        let m = magic_sequence(0x6c);
        assert_eq!(&m[..8], &[0x29, 0x23, 0xBE, 0x84, 0xE1, 0x6C, 0xD6, 0xAE]);
        // The specification lists 92 bytes; its last four.
        assert_eq!(&m[88..92], &[0x6B, 0xC4, 0x30, 0xB7]);
    }

    #[test]
    fn spans_past_the_end_or_overflowing_are_none() {
        let b = [1u8, 2, 3, 4];
        assert_eq!(span(&b, 1, 2), Some(&b[1..3]));
        assert_eq!(span(&b, 3, 2), None);
        assert_eq!(span(&b, usize::MAX, 2), None);
        assert!(le32(&b, usize::MAX - 1).is_err());
    }

    #[test]
    fn versions() {
        assert_eq!(version_of(b"AC1032\0\0").unwrap(), Version::R2018);
        assert!(matches!(
            version_of(b"AC1009"),
            Err(DwgError::Unsupported(_))
        ));
        assert!(matches!(
            version_of(b"PK\x03\x04xx"),
            Err(DwgError::NotDwg(_))
        ));
    }
}
