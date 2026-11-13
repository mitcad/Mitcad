// SPDX-License-Identifier: MIT
//! The `.3dm` archive: a 32-byte header, then chunks. A chunk is a
//! typecode (4 bytes) and a value (4 bytes before version 5, 8 bytes
//! since): for a short chunk (`TCODE_SHORT`) the value is its content,
//! otherwise it is the length of the data that follows. A chunk with
//! `TCODE_CRC` ends in the CRC-32 of the rest of its data. Numbers are
//! little-endian; the data of objects is versioned, so newer fields at the
//! end of a chunk are skipped with it.
//!
//! The reader is fed untrusted files: every length, count and offset read
//! from the file is checked against the bytes of its chunk before it is
//! used, and chunks nest at most [`MAX_DEPTH`] deep, which also bounds the
//! recursion of the curves that hold curves (polycurves, the profiles of
//! surfaces).

use std::fmt;

/// The deepest chunk nesting read: real files nest about a dozen deep (a
/// polycurve segment in a B-rep's curve array); a file that nests deeper
/// is refused there instead of recursing without end.
pub const MAX_DEPTH: u32 = 64;

pub const TCODE_SHORT: u32 = 0x8000_0000;
pub const TCODE_CRC: u32 = 0x0000_8000;
pub const TCODE_ENDOFFILE: u32 = 0x0000_7FFF;
pub const TCODE_ENDOFTABLE: u32 = 0xFFFF_FFFF;
pub const TCODE_ANONYMOUS_CHUNK: u32 = 0x4000_8000;

pub const TCODE_MATERIAL_TABLE: u32 = 0x1000_0010;
pub const TCODE_LAYER_TABLE: u32 = 0x1000_0011;
pub const TCODE_OBJECT_TABLE: u32 = 0x1000_0013;
pub const TCODE_PROPERTIES_TABLE: u32 = 0x1000_0014;
pub const TCODE_SETTINGS_TABLE: u32 = 0x1000_0015;
pub const TCODE_INSTANCE_DEFINITION_TABLE: u32 = 0x1000_0021;

pub const TCODE_PROPERTIES_WRITER_VERSION: u32 = 0xA000_0026;
pub const TCODE_PROPERTIES_APPLICATION: u32 = 0x2000_8024;
pub const TCODE_SETTINGS_UNITSANDTOLS: u32 = 0x2000_8031;
pub const TCODE_LAYER_RECORD: u32 = 0x2000_8050;
pub const TCODE_OBJECT_RECORD: u32 = 0x2000_8070;
pub const TCODE_INSTANCE_DEFINITION_RECORD: u32 = 0x2000_8076;
pub const TCODE_OBJECT_RECORD_TYPE: u32 = 0x8200_0071;
pub const TCODE_OBJECT_RECORD_ATTRIBUTES: u32 = 0x0200_8072;
pub const TCODE_OBJECT_RECORD_END: u32 = 0x8200_007F;

pub const TCODE_CLASS: u32 = 0x0002_7FFA;
pub const TCODE_CLASS_UUID: u32 = 0x0002_FFFB;
pub const TCODE_CLASS_DATA: u32 = 0x0002_FFFC;

/// What went wrong reading a file: what was expected, in words a person
/// reads (`.0`, and the error's `Display`), and where, for agents and the
/// log (`.1`: offsets, byte counts, typecodes, class ids; empty when there
/// is nothing more to say). A message shown to a person never holds the
/// place (docs/ui-conventions.md, "What a message says").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String, pub String);

impl Error {
    /// An error in words alone.
    pub fn new(words: impl Into<String>) -> Error {
        Error(words.into(), String::new())
    }

    /// An error in words and its place.
    pub fn at(words: impl Into<String>, place: impl Into<String>) -> Error {
        Error(words.into(), place.into())
    }

    /// The words and the place, for agents and the log.
    pub fn detail(&self) -> String {
        if self.1.is_empty() {
            self.0.clone()
        } else {
            format!("{} ({})", self.0, self.1)
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(Error::new(message))
}

/// An error in words, with its place for agents and the log.
pub fn err_at<T>(message: impl Into<String>, place: impl Into<String>) -> Result<T> {
    Err(Error::at(message, place))
}

/// The CRC-32 of zlib (and of the format), seeded with `crc`.
pub fn crc32(mut crc: u32, data: &[u8]) -> u32 {
    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, entry) in t.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *entry = c;
        }
        t
    });
    crc = !crc;
    for &b in data {
        crc = table[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// A 128-bit id as the file stores it (Data1, Data2, Data3, Data4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Uuid(pub [u8; 16]);

impl Uuid {
    pub const NIL: Uuid = Uuid([0; 16]);

    /// From its text form (`5EAF1119-0B51-11D4-BFFE-0010830122F0`).
    pub const fn parse(s: &str) -> Uuid {
        let b = s.as_bytes();
        let mut out = [0u8; 16];
        let mut digits = [0u8; 32];
        let mut n = 0;
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            let v = match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            };
            if let Some(v) = v {
                digits[n] = v;
                n += 1;
            }
            i += 1;
        }
        assert!(n == 32, "a UUID has 32 hexadecimal digits");
        // The text is Data1 (big-endian digits), Data2, Data3, Data4; the
        // file has Data1..3 little-endian.
        let order = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
        let mut k = 0;
        while k < 16 {
            let d = order[k];
            out[k] = digits[2 * d] * 16 + digits[2 * d + 1];
            k += 1;
        }
        Uuid(out)
    }

    pub fn is_nil(&self) -> bool {
        self.0 == [0; 16]
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            b[3],
            b[2],
            b[1],
            b[0],
            b[5],
            b[4],
            b[7],
            b[6],
            b[8],
            b[9],
            b[10],
            b[11],
            b[12],
            b[13],
            b[14],
            b[15]
        )
    }
}

/// A cursor over part of the file: the whole archive, or a chunk's data.
#[derive(Clone)]
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    /// The archive's 3dm version (2, 3, 4, 50, 60, 70, 80): chunk values
    /// have 8 bytes from 50 on.
    pub version: u32,
    /// The version of the library that wrote the file (its properties),
    /// for fields added over the years; large when not known.
    pub writer: u64,
    /// How deep this reader's data is nested in chunks (0: the archive).
    depth: u32,
}

/// A chunk read from a [`Reader`]: its typecode, its value (the length of
/// its data or a short chunk's content) and a reader of its data (without
/// the CRC).
pub struct Chunk<'a> {
    pub typecode: u32,
    pub value: i64,
    pub data: Reader<'a>,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8], version: u32) -> Self {
        Reader {
            data,
            pos: 0,
            version,
            writer: u64::MAX,
            depth: 0,
        }
    }

    /// A reader of the data of a chunk nested in this reader's.
    fn sub(&self, data: &'a [u8]) -> Reader<'a> {
        Reader {
            data,
            pos: 0,
            version: self.version,
            writer: self.writer,
            depth: self.depth + 1,
        }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return err_at(
                "the data ends early",
                format!(
                    "{n} bytes needed at offset {}, {} left",
                    self.pos,
                    self.remaining()
                ),
            );
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.bytes(n).map(|_| ())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.bytes(N)?);
        Ok(a)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool> {
        Ok(self.u8()? != 0)
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.array()?))
    }

    pub fn point(&mut self) -> Result<[f64; 3]> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }

    pub fn interval(&mut self) -> Result<[f64; 2]> {
        Ok([self.f64()?, self.f64()?])
    }

    /// A count read as a 32-bit int, checked against the bytes left for
    /// items of `item_size` bytes.
    pub fn count(&mut self, item_size: usize) -> Result<usize> {
        let n = self.i32()?;
        let n = usize::try_from(n)
            .map_err(|_| Error::at("a negative count", format!("the count {n}")))?;
        if n.saturating_mul(item_size.max(1)) > self.remaining() {
            return err_at(
                "a count larger than its data",
                format!(
                    "a count of {n} items of {item_size} bytes, {} bytes left",
                    self.remaining()
                ),
            );
        }
        Ok(n)
    }

    pub fn f64s(&mut self, n: usize) -> Result<Vec<f64>> {
        (0..n).map(|_| self.f64()).collect()
    }

    /// An array of ints (count, then the ints).
    pub fn i32_array(&mut self) -> Result<Vec<i32>> {
        let n = self.count(4)?;
        (0..n).map(|_| self.i32()).collect()
    }

    /// An array of doubles (count, then the doubles).
    pub fn f64_array(&mut self) -> Result<Vec<f64>> {
        let n = self.count(8)?;
        self.f64s(n)
    }

    /// An array of 3D points (count, then the points).
    pub fn point_array(&mut self) -> Result<Vec<[f64; 3]>> {
        let n = self.count(24)?;
        (0..n).map(|_| self.point()).collect()
    }

    pub fn uuid(&mut self) -> Result<Uuid> {
        Ok(Uuid(self.array()?))
    }

    /// A colour as the file keeps it (`0xAABBGGRR`, alpha 0 opaque).
    pub fn color(&mut self) -> Result<u32> {
        self.u32()
    }

    /// A UTF-16 string: its count of 16-bit units with the terminator,
    /// then the units.
    pub fn wstring(&mut self) -> Result<String> {
        let n = self.u32()? as usize;
        if n == 0 {
            return Ok(String::new());
        }
        if n.saturating_mul(2) > self.remaining() {
            return err_at(
                "a text longer than its data",
                format!(
                    "a string of {n} characters, {} bytes left",
                    self.remaining()
                ),
            );
        }
        let units: Vec<u16> = (0..n).map(|_| self.u16()).collect::<Result<_>>()?;
        let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
        Ok(String::from_utf16_lossy(&units[..end]))
    }

    /// A chunk version byte: major in the high four bits, minor in the
    /// low ones.
    pub fn chunk_version(&mut self) -> Result<(u8, u8)> {
        let v = self.u8()?;
        Ok((v >> 4, v & 0x0F))
    }

    /// The next chunk's typecode without reading it (None at the end).
    pub fn peek_typecode(&self) -> Option<u32> {
        let b = self.data.get(self.pos..self.pos.saturating_add(4))?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// The next chunk. Its CRC, if it has one, is left off its data: it is
    /// the CRC of the chunk's own bytes without the chunks nested in it
    /// (the format's writer adds the bytes it writes to the innermost
    /// chunk only), which a reader that does not read every field cannot
    /// check; the lengths and counts are checked instead. A chunk nested
    /// deeper than [`MAX_DEPTH`] is refused.
    pub fn chunk(&mut self) -> Result<Chunk<'a>> {
        let at = self.pos;
        if self.depth >= MAX_DEPTH {
            return err_at(
                format!("chunks nested more than {MAX_DEPTH} deep"),
                format!("at offset {at}"),
            );
        }
        let typecode = self.u32()?;
        let value = if self.version >= 50 {
            self.i64()?
        } else {
            i64::from(self.i32()?)
        };
        if typecode & TCODE_SHORT != 0 {
            return Ok(Chunk {
                typecode,
                value,
                data: self.sub(&[]),
            });
        }
        let len = usize::try_from(value)
            .ok()
            .filter(|&l| l <= self.remaining())
            .ok_or_else(|| {
                Error::at(
                    "a part of the file longer than the file",
                    format!(
                        "the chunk {typecode:08X} at offset {at} has a length of {value}, {} bytes left",
                        self.remaining()
                    ),
                )
            })?;
        let body = self.bytes(len)?;
        let data = if typecode & TCODE_CRC != 0 && typecode != TCODE_ENDOFFILE && len >= 4 {
            &body[..len - 4]
        } else {
            body
        };
        Ok(Chunk {
            typecode,
            value,
            data: self.sub(data),
        })
    }

    /// The next chunk; one longer than what is left (a file cut short)
    /// with the bytes there are, and `true`.
    pub fn chunk_or_rest(&mut self) -> Result<(Chunk<'a>, bool)> {
        let saved = self.pos;
        match self.chunk() {
            Ok(c) => Ok((c, false)),
            Err(e) => {
                self.pos = saved;
                let typecode = self.u32().map_err(|_| e.clone())?;
                let value = if self.version >= 50 {
                    self.i64()
                } else {
                    self.i32().map(i64::from)
                }
                .map_err(|_| e.clone())?;
                if typecode & TCODE_SHORT != 0 || value < 0 {
                    return Err(e);
                }
                let rest = self.bytes(self.remaining())?;
                Ok((
                    Chunk {
                        typecode,
                        value,
                        data: self.sub(rest),
                    },
                    true,
                ))
            }
        }
    }

    /// The next chunk, which must have the typecode.
    pub fn expect_chunk(&mut self, typecode: u32, what: &str) -> Result<Chunk<'a>> {
        let c = self.chunk()?;
        if c.typecode != typecode {
            return err_at(
                format!("{what} is missing"),
                format!("a chunk {typecode:08X} expected, {:08X} found", c.typecode),
            );
        }
        Ok(c)
    }

    /// An anonymous chunk with a version (two ints) at its start, as the
    /// format writes versioned chunks; returns the reader after them.
    pub fn versioned_chunk(&mut self, what: &str) -> Result<(i32, i32, Reader<'a>)> {
        let mut c = self.expect_chunk(TCODE_ANONYMOUS_CHUNK, what)?;
        let major = c.data.i32()?;
        let minor = c.data.i32()?;
        Ok((major, minor, c.data))
    }

    /// An anonymous chunk's data, with the chunk version byte (major,
    /// minor) read: the arrays of B-reps and curves.
    pub fn anonymous(&mut self, what: &str) -> Result<(u8, u8, Reader<'a>)> {
        let mut c = self.expect_chunk(TCODE_ANONYMOUS_CHUNK, what)?;
        let (major, minor) = c.data.chunk_version()?;
        Ok((major, minor, c.data))
    }
}
