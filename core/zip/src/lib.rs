// SPDX-License-Identifier: MIT
//! Minimal zip reader and writer for the containers Mitcad reads:
//! `.f3d`/`.f3z` and FreeCAD's `.FCStd`.
//!
//! The reader supports what those programs write: the central directory
//! (also ZIP64), local headers, and the methods stored (0), deflate (8) and
//! Zstandard (93). Entries are verified with their CRC-32, and one entry
//! decompresses to at most 4 GiB. Encryption and multi-disk archives are not
//! supported. The [`Writer`] writes stored and deflated entries (test
//! files, and files Mitcad writes).

use std::fmt;

/// Compression method of a zip entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Stored,
    Deflate,
    Zstd,
    Other(u16),
}

impl Method {
    fn from_code(code: u16) -> Self {
        match code {
            0 => Method::Stored,
            8 => Method::Deflate,
            93 => Method::Zstd,
            other => Method::Other(other),
        }
    }

    pub fn code(self) -> u16 {
        match self {
            Method::Stored => 0,
            Method::Deflate => 8,
            Method::Zstd => 93,
            Method::Other(code) => code,
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Method::Stored => f.write_str("stored"),
            Method::Deflate => f.write_str("deflate"),
            Method::Zstd => f.write_str("zstd"),
            Method::Other(code) => write!(f, "method {code}"),
        }
    }
}

/// Error while reading the container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZipError {
    /// No end-of-central-directory record: not a zip file.
    NotZip,
    /// A record lies outside the file or has a wrong signature.
    Corrupt(String),
    /// The entry uses a feature this reader does not implement.
    Unsupported(String),
    /// Decompression failed.
    Decompress(String),
    /// The entry's CRC-32 does not match its data.
    Crc {
        name: String,
    },
    NotFound(String),
}

impl fmt::Display for ZipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ZipError::NotZip => f.write_str("not a zip file (no end of central directory)"),
            ZipError::Corrupt(what) => write!(f, "corrupt zip: {what}"),
            ZipError::Unsupported(what) => write!(f, "unsupported zip feature: {what}"),
            ZipError::Decompress(what) => write!(f, "decompression failed: {what}"),
            ZipError::Crc { name } => write!(f, "CRC-32 mismatch in {name}"),
            ZipError::NotFound(name) => write!(f, "no entry named {name}"),
        }
    }
}

impl std::error::Error for ZipError {}

/// One entry of the central directory.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub method: Method,
    pub compressed_size: u64,
    pub size: u64,
    pub crc32: u32,
    pub flags: u16,
    local_header_offset: u64,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// A zip archive held in memory.
pub struct Archive {
    data: Vec<u8>,
    entries: Vec<Entry>,
}

/// Upper bound for one decompressed entry, guarding against bogus sizes.
const MAX_ENTRY_SIZE: u64 = 4 << 30;

fn u16_at(data: &[u8], pos: usize) -> Option<u16> {
    data.get(pos..pos + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u64_at(data: &[u8], pos: usize) -> Option<u64> {
    data.get(pos..pos + 8).map(|b| {
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        u64::from_le_bytes(a)
    })
}

fn corrupt(what: &str) -> ZipError {
    ZipError::Corrupt(what.to_string())
}

const EOCD_SIG: u32 = 0x0605_4b50;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;

impl Archive {
    /// Reads the central directory of a zip file held in memory.
    pub fn new(data: Vec<u8>) -> Result<Self, ZipError> {
        let entries = read_central_directory(&data)?;
        Ok(Archive { data, entries })
    }

    pub fn open(path: &std::path::Path) -> Result<Self, ZipError> {
        let data =
            std::fs::read(path).map_err(|e| ZipError::Corrupt(format!("cannot read file: {e}")))?;
        Archive::new(data)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    pub fn read_by_name(&self, name: &str) -> Result<Vec<u8>, ZipError> {
        let entry = self
            .find(name)
            .ok_or_else(|| ZipError::NotFound(name.to_string()))?;
        self.read(entry)
    }

    /// The entry's stored (possibly compressed) bytes.
    pub fn raw(&self, entry: &Entry) -> Result<&[u8], ZipError> {
        let pos = usize::try_from(entry.local_header_offset)
            .map_err(|_| corrupt("local header offset"))?;
        if u32_at(&self.data, pos) != Some(LOCAL_SIG) {
            return Err(corrupt("local header signature"));
        }
        let name_len =
            u16_at(&self.data, pos + 26).ok_or_else(|| corrupt("local header"))? as usize;
        let extra_len =
            u16_at(&self.data, pos + 28).ok_or_else(|| corrupt("local header"))? as usize;
        let start = pos + 30 + name_len + extra_len;
        let len = usize::try_from(entry.compressed_size).map_err(|_| corrupt("entry size"))?;
        self.data
            .get(
                start
                    ..start
                        .checked_add(len)
                        .ok_or_else(|| corrupt("entry size"))?,
            )
            .ok_or_else(|| corrupt("entry data outside the file"))
    }

    /// Decompresses an entry and checks its CRC-32.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, ZipError> {
        if entry.flags & 1 != 0 {
            return Err(ZipError::Unsupported(format!(
                "encrypted entry {}",
                entry.name
            )));
        }
        if entry.size > MAX_ENTRY_SIZE {
            return Err(ZipError::Unsupported(format!(
                "entry {} is too large",
                entry.name
            )));
        }
        let raw = self.raw(entry)?;
        let data = match entry.method {
            Method::Stored => raw.to_vec(),
            Method::Deflate => {
                miniz_oxide::inflate::decompress_to_vec_with_limit(raw, entry.size as usize)
                    .map_err(|e| {
                        ZipError::Decompress(format!("{}: deflate {:?}", entry.name, e.status))
                    })?
            }
            Method::Zstd => decompress_zstd(raw, entry.size as usize)
                .map_err(|e| ZipError::Decompress(format!("{}: zstd {e}", entry.name)))?,
            Method::Other(code) => {
                return Err(ZipError::Unsupported(format!(
                    "compression method {code} in {}",
                    entry.name
                )));
            }
        };
        if data.len() as u64 != entry.size {
            return Err(ZipError::Decompress(format!(
                "{}: expected {} bytes, got {}",
                entry.name,
                entry.size,
                data.len()
            )));
        }
        if crc32(&data) != entry.crc32 {
            return Err(ZipError::Crc {
                name: entry.name.clone(),
            });
        }
        Ok(data)
    }
}

fn decompress_zstd(raw: &[u8], size: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out = Vec::with_capacity(size);
    let mut rest = raw;
    // An entry may hold several concatenated frames.
    while !rest.is_empty() {
        let mut decoder =
            ruzstd::decoding::StreamingDecoder::new(&mut rest).map_err(|e| e.to_string())?;
        decoder.read_to_end(&mut out).map_err(|e| e.to_string())?;
        if out.len() > size {
            return Err("more data than the entry size".to_string());
        }
    }
    Ok(out)
}

fn read_central_directory(data: &[u8]) -> Result<Vec<Entry>, ZipError> {
    // The end record is at most 22 + 65535 bytes from the end.
    let min = data.len().saturating_sub(22 + 0xffff);
    let eocd = (min..=data.len().saturating_sub(22))
        .rev()
        .find(|&pos| u32_at(data, pos) == Some(EOCD_SIG))
        .ok_or(ZipError::NotZip)?;
    let mut count = u64::from(u16_at(data, eocd + 10).ok_or(ZipError::NotZip)?);
    let mut cd_size = u64::from(u32_at(data, eocd + 12).ok_or(ZipError::NotZip)?);
    let mut cd_offset = u64::from(u32_at(data, eocd + 16).ok_or(ZipError::NotZip)?);
    if count == 0xffff || cd_size == 0xffff_ffff || cd_offset == 0xffff_ffff {
        // ZIP64: the locator precedes the end record.
        let loc = eocd
            .checked_sub(20)
            .ok_or_else(|| corrupt("ZIP64 locator"))?;
        if u32_at(data, loc) != Some(ZIP64_LOCATOR_SIG) {
            return Err(corrupt("ZIP64 locator signature"));
        }
        let rec = u64_at(data, loc + 8).ok_or_else(|| corrupt("ZIP64 locator"))? as usize;
        if u32_at(data, rec) != Some(ZIP64_EOCD_SIG) {
            return Err(corrupt("ZIP64 end record signature"));
        }
        count = u64_at(data, rec + 32).ok_or_else(|| corrupt("ZIP64 end record"))?;
        cd_size = u64_at(data, rec + 40).ok_or_else(|| corrupt("ZIP64 end record"))?;
        cd_offset = u64_at(data, rec + 48).ok_or_else(|| corrupt("ZIP64 end record"))?;
    }
    if cd_offset.saturating_add(cd_size) > data.len() as u64 {
        return Err(corrupt("central directory outside the file"));
    }
    let mut entries = Vec::new();
    let mut pos = cd_offset as usize;
    for _ in 0..count {
        if u32_at(data, pos) != Some(CENTRAL_SIG) {
            return Err(corrupt("central directory signature"));
        }
        let field16 =
            |off: usize| u16_at(data, pos + off).ok_or_else(|| corrupt("central directory"));
        let field32 =
            |off: usize| u32_at(data, pos + off).ok_or_else(|| corrupt("central directory"));
        let flags = field16(8)?;
        let method = Method::from_code(field16(10)?);
        let crc = field32(16)?;
        let mut compressed_size = u64::from(field32(20)?);
        let mut size = u64::from(field32(24)?);
        let name_len = field16(28)? as usize;
        let extra_len = field16(30)? as usize;
        let comment_len = field16(32)? as usize;
        let mut offset = u64::from(field32(42)?);
        let name_bytes = data
            .get(pos + 46..pos + 46 + name_len)
            .ok_or_else(|| corrupt("entry name"))?;
        let extra = data
            .get(pos + 46 + name_len..pos + 46 + name_len + extra_len)
            .ok_or_else(|| corrupt("extra field"))?;
        // ZIP64 extended information: the 64-bit values of the fields that
        // were saturated, in this order.
        let mut e = 0;
        while e + 4 <= extra.len() {
            let id = u16_at(extra, e).unwrap_or(0);
            let len = u16_at(extra, e + 2).unwrap_or(0) as usize;
            let body = extra.get(e + 4..e + 4 + len).unwrap_or(&[]);
            if id == 1 {
                let mut b = 0;
                let mut take = |v: &mut u64| {
                    if *v == 0xffff_ffff
                        && let Some(x) = u64_at(body, b)
                    {
                        *v = x;
                        b += 8;
                    }
                };
                take(&mut size);
                take(&mut compressed_size);
                take(&mut offset);
            }
            e += 4 + len;
        }
        let name = decode_name(name_bytes, flags & (1 << 11) != 0);
        entries.push(Entry {
            name,
            method,
            compressed_size,
            size,
            crc32: crc,
            flags,
            local_header_offset: offset,
        });
        pos += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// Names are UTF-8 when flag bit 11 is set, otherwise code page 437. Valid
/// UTF-8 is accepted in both cases (some writers omit the flag).
fn decode_name(bytes: &[u8], utf8_flag: bool) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    if utf8_flag {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    bytes
        .iter()
        .map(|&b| {
            if b < 0x80 {
                b as char
            } else {
                CP437_HIGH[(b - 0x80) as usize]
            }
        })
        .collect()
}

const CP437_HIGH: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å', 'É', 'æ', 'Æ',
    'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ', 'á', 'í', 'ó', 'ú', 'ñ', 'Ñ',
    'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»', '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕',
    '╣', '║', '╗', '╝', '╜', '╛', '┐', '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦',
    '╠', '═', '╬', '╧', '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐',
    '▀', 'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩', '≡', '±',
    '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// CRC-32 (IEEE 802.3, as used by zip).
pub fn crc32(data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xedb8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *slot = c;
        }
        t
    });
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
}

/// Writes a zip archive in memory: stored or deflated entries, without
/// ZIP64 (each entry and the archive below 4 GiB, at most 65535 entries).
#[derive(Debug, Default)]
pub struct Writer {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an entry, stored or compressed with deflate (`Method::Stored`
    /// or `Method::Deflate`).
    pub fn add(&mut self, name: &str, data: &[u8], method: Method) -> Result<(), ZipError> {
        let too_large = || ZipError::Unsupported(format!("{name}: too large without ZIP64"));
        let packed = match method {
            Method::Stored => data.to_vec(),
            Method::Deflate => miniz_oxide::deflate::compress_to_vec(data, 6),
            other => return Err(ZipError::Unsupported(format!("writing {other} entries"))),
        };
        let size = u32::try_from(data.len()).map_err(|_| too_large())?;
        let packed_size = u32::try_from(packed.len()).map_err(|_| too_large())?;
        let offset = u32::try_from(self.out.len()).map_err(|_| too_large())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_large())?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| ZipError::Unsupported("more than 65535 entries".to_owned()))?;
        // Bit 11: the name is UTF-8.
        let flags: u16 = if name.is_ascii() { 0 } else { 1 << 11 };
        let crc = crc32(data);
        let header = |buf: &mut Vec<u8>, central: bool| {
            buf.extend_from_slice(&(if central { CENTRAL_SIG } else { LOCAL_SIG }).to_le_bytes());
            if central {
                buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
            }
            buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
            buf.extend_from_slice(&flags.to_le_bytes());
            buf.extend_from_slice(&method.code().to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes()); // time
            buf.extend_from_slice(&0x21u16.to_le_bytes()); // date: 1980-01-01
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&packed_size.to_le_bytes());
            buf.extend_from_slice(&size.to_le_bytes());
            buf.extend_from_slice(&name_len.to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes()); // extra field length
            if central {
                buf.extend_from_slice(&[0u8; 6]); // comment length, disk, internal attributes
                buf.extend_from_slice(&0u32.to_le_bytes()); // external attributes
                buf.extend_from_slice(&offset.to_le_bytes());
            }
            buf.extend_from_slice(name.as_bytes());
        };
        header(&mut self.out, false);
        self.out.extend_from_slice(&packed);
        header(&mut self.central, true);
        Ok(())
    }

    /// The archive: the entries, the central directory and its end record.
    pub fn finish(self) -> Result<Vec<u8>, ZipError> {
        let Writer {
            mut out,
            central,
            count,
        } = self;
        let too_large = || ZipError::Unsupported("archive too large without ZIP64".to_owned());
        let offset = u32::try_from(out.len()).map_err(|_| too_large())?;
        let size = u32::try_from(central.len()).map_err(|_| too_large())?;
        out.extend_from_slice(&central);
        out.extend_from_slice(&EOCD_SIG.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]); // disk numbers
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // comment length
        Ok(out)
    }
}

/// A zip of stored entries (test files).
///
/// # Panics
/// When an entry or the archive does not fit without ZIP64.
pub fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = Writer::new();
    for (name, data) in files {
        writer
            .add(name, data, Method::Stored)
            .expect("a small test archive");
    }
    writer.finish().expect("a small test archive")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        stored_zip(files)
    }

    #[test]
    fn crc_matches_reference_value() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn reads_stored_entries() {
        let zip = build_stored_zip(&[("a.txt", b"hello"), ("dir/b.bin", &[1, 2, 3])]);
        let archive = Archive::new(zip).unwrap();
        assert_eq!(archive.entries().len(), 2);
        assert_eq!(archive.read_by_name("a.txt").unwrap(), b"hello");
        assert_eq!(archive.read_by_name("dir/b.bin").unwrap(), [1, 2, 3]);
        assert_eq!(archive.entries()[0].method, Method::Stored);
        assert!(matches!(
            archive.read_by_name("c"),
            Err(ZipError::NotFound(_))
        ));
    }

    #[test]
    fn rejects_non_zip() {
        assert!(matches!(
            Archive::new(vec![0u8; 100]),
            Err(ZipError::NotZip)
        ));
    }

    #[test]
    fn detects_crc_mismatch() {
        let mut zip = build_stored_zip(&[("a.txt", b"hello")]);
        zip[30 + 5] = b'j'; // first data byte after the 30-byte header and name
        let archive = Archive::new(zip).unwrap();
        assert!(matches!(
            archive.read_by_name("a.txt"),
            Err(ZipError::Crc { .. })
        ));
    }

    #[test]
    fn writes_deflated_entries_and_utf8_names() {
        let text = b"Document Document Document Document".repeat(20);
        let mut writer = Writer::new();
        writer.add("Document.xml", &text, Method::Deflate).unwrap();
        writer
            .add("caf\u{e9}.bin", &[7, 8], Method::Stored)
            .unwrap();
        let archive = Archive::new(writer.finish().unwrap()).unwrap();
        let entry = archive.find("Document.xml").unwrap();
        assert_eq!(entry.method, Method::Deflate);
        assert!(entry.compressed_size < entry.size);
        assert_eq!(archive.read(entry).unwrap(), text);
        assert_eq!(archive.read_by_name("caf\u{e9}.bin").unwrap(), [7, 8]);
        assert!(Writer::new().add("x", b"", Method::Zstd).is_err());
    }

    #[test]
    fn inflates_deflate_data() {
        // Round trip through miniz_oxide's own raw deflate compressor.
        let text = b"hello hello hello";
        let compressed = miniz_oxide::deflate::compress_to_vec(text, 6);
        let back =
            miniz_oxide::inflate::decompress_to_vec_with_limit(&compressed, text.len()).unwrap();
        assert_eq!(back, text);
    }
}
