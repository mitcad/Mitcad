// SPDX-License-Identifier: MIT
//! Property set streams as Microsoft's published specification describes
//! them (MS-OLEPS): a stream holds one or two property sets, each a format
//! id and a table of typed values by property id. Property 0 is the
//! dictionary of property names, property 1 the code page of the set's
//! strings. `.ipt` files keep their document properties (part number,
//! material, the saving release, model units) in such streams at the root
//! of the compound file; their names start with the character 0x05.

use std::collections::BTreeMap;
use std::fmt;

/// A property's value; types the reader does not decode keep their
/// variant type.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Int(i64),
    Real(f64),
    Bool(bool),
    Text(String),
    /// 100-nanosecond intervals since 1601-01-01 (UTC).
    FileTime(u64),
    Clsid([u8; 16]),
    Other(u16),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Real(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Text(v) => write!(f, "{v:?}"),
            Value::FileTime(v) => write!(f, "filetime {v}"),
            Value::Clsid(v) => write!(f, "{{{}}}", crate::cfb::guid_text(v)),
            Value::Other(t) => write!(f, "(type {t:#x})"),
        }
    }
}

/// One property set.
#[derive(Clone, Debug, Default)]
pub struct PropertySet {
    pub fmtid: [u8; 16],
    pub code_page: Option<u16>,
    /// The dictionary: names by property id.
    pub names: BTreeMap<u32, String>,
    pub values: BTreeMap<u32, Value>,
}

impl PropertySet {
    pub fn text(&self, id: u32) -> Option<&str> {
        match self.values.get(&id) {
            Some(Value::Text(t)) => Some(t.as_str()),
            _ => None,
        }
    }

    pub fn int(&self, id: u32) -> Option<i64> {
        match self.values.get(&id) {
            Some(Value::Int(v)) => Some(*v),
            _ => None,
        }
    }

    pub fn real(&self, id: u32) -> Option<f64> {
        match self.values.get(&id) {
            Some(Value::Real(v)) => Some(*v),
            Some(Value::Int(v)) => Some(*v as f64),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyError(pub String);

impl fmt::Display for PropertyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "property set: {}", self.0)
    }
}

impl std::error::Error for PropertyError {}

struct Reader<'a> {
    data: &'a [u8],
}

impl Reader<'_> {
    fn bytes(&self, at: usize, n: usize) -> Result<&[u8], PropertyError> {
        at.checked_add(n)
            .and_then(|end| self.data.get(at..end))
            .ok_or_else(|| PropertyError(format!("{n} bytes at {at} past the end")))
    }

    fn u16(&self, at: usize) -> Result<u16, PropertyError> {
        let b = self.bytes(at, 2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&self, at: usize) -> Result<u32, PropertyError> {
        let b = self.bytes(at, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&self, at: usize) -> Result<u64, PropertyError> {
        let b = self.bytes(at, 8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    /// A UTF-16 string of `chars` code units, without its terminator.
    fn utf16(&self, at: usize, chars: usize) -> Result<String, PropertyError> {
        let b = self.bytes(
            at,
            chars
                .checked_mul(2)
                .ok_or_else(|| PropertyError("string size".into()))?,
        )?;
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        Ok(String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string())
    }

    /// A code page string of `size` bytes: UTF-16 for code page 1200,
    /// else bytes read as Latin-1 (UTF-8 for code page 65001).
    fn code_page_string(
        &self,
        at: usize,
        size: usize,
        code_page: Option<u16>,
    ) -> Result<String, PropertyError> {
        if code_page == Some(1200) {
            return self.utf16(at, size / 2);
        }
        let b = self.bytes(at, size)?;
        let text = if code_page == Some(65001) {
            String::from_utf8_lossy(b).into_owned()
        } else {
            b.iter().map(|&c| char::from(c)).collect()
        };
        Ok(text.trim_end_matches('\0').to_string())
    }
}

/// Size limit of a string or table, against damaged counts.
const LIMIT: usize = 1 << 24;

fn typed_value(r: &Reader, at: usize, code_page: Option<u16>) -> Result<Value, PropertyError> {
    let kind = r.u16(at)?;
    let v = at + 4;
    Ok(match kind {
        // VT_I2, VT_I4, VT_I1, VT_UI1, VT_UI2, VT_UI4, VT_I8, VT_UI8, VT_INT, VT_UINT
        0x02 => Value::Int(i64::from(r.u16(v)? as i16)),
        0x03 | 0x16 => Value::Int(i64::from(r.u32(v)? as i32)),
        0x10 => Value::Int(i64::from(r.bytes(v, 1)?[0] as i8)),
        0x11 => Value::Int(i64::from(r.bytes(v, 1)?[0])),
        0x12 => Value::Int(i64::from(r.u16(v)?)),
        0x13 | 0x17 => Value::Int(i64::from(r.u32(v)?)),
        0x14 => Value::Int(r.u64(v)? as i64),
        0x15 => Value::Int(i64::try_from(r.u64(v)?).unwrap_or(i64::MAX)),
        // VT_R4, VT_R8
        0x04 => Value::Real(f64::from(f32::from_bits(r.u32(v)?))),
        0x05 => Value::Real(f64::from_bits(r.u64(v)?)),
        // VT_BOOL: 0xFFFF true, 0 false
        0x0B => Value::Bool(r.u16(v)? != 0),
        // VT_LPSTR (and VT_BSTR): byte size, then the code page string
        0x1E | 0x08 => {
            let size = r.u32(v)? as usize;
            if size > LIMIT {
                return Err(PropertyError(format!("string of {size} bytes")));
            }
            Value::Text(r.code_page_string(v + 4, size, code_page)?)
        }
        // VT_LPWSTR: length in characters, then UTF-16
        0x1F => {
            let chars = r.u32(v)? as usize;
            if chars > LIMIT {
                return Err(PropertyError(format!("string of {chars} characters")));
            }
            Value::Text(r.utf16(v + 4, chars)?)
        }
        0x40 => Value::FileTime(r.u64(v)?),
        0x48 => {
            let mut id = [0u8; 16];
            id.copy_from_slice(r.bytes(v, 16)?);
            Value::Clsid(id)
        }
        other => Value::Other(other),
    })
}

fn dictionary(
    r: &Reader,
    at: usize,
    code_page: Option<u16>,
) -> Result<BTreeMap<u32, String>, PropertyError> {
    let count = r.u32(at)? as usize;
    if count > LIMIT / 8 {
        return Err(PropertyError(format!("dictionary of {count} entries")));
    }
    let mut names = BTreeMap::new();
    let mut p = at + 4;
    for _ in 0..count {
        let id = r.u32(p)?;
        let len = r.u32(p + 4)? as usize;
        if len > LIMIT {
            return Err(PropertyError(format!("name of {len} characters")));
        }
        p += 8;
        if code_page == Some(1200) {
            names.insert(id, r.utf16(p, len)?);
            // Unicode names are padded to a multiple of 4 bytes.
            p += (2 * len).div_ceil(4) * 4;
        } else {
            names.insert(id, r.code_page_string(p, len, code_page)?);
            p += len;
        }
    }
    Ok(names)
}

fn property_set(r: &Reader, base: usize, fmtid: [u8; 16]) -> Result<PropertySet, PropertyError> {
    let _size = r.u32(base)?;
    let count = r.u32(base + 4)? as usize;
    if count > LIMIT / 8 {
        return Err(PropertyError(format!("{count} properties")));
    }
    let mut offsets = Vec::with_capacity(count);
    for i in 0..count {
        let id = r.u32(base + 8 + 8 * i)?;
        let offset = r.u32(base + 12 + 8 * i)? as usize;
        offsets.push((id, base + offset));
    }
    let mut set = PropertySet {
        fmtid,
        ..PropertySet::default()
    };
    // The code page first: the strings depend on it.
    if let Some(&(_, at)) = offsets.iter().find(|(id, _)| *id == 1)
        && let Ok(Value::Int(cp)) = typed_value(r, at, None)
    {
        set.code_page = Some(cp as u16);
    }
    for (id, at) in offsets {
        match id {
            0 => set.names = dictionary(r, at, set.code_page)?,
            _ => {
                set.values.insert(id, typed_value(r, at, set.code_page)?);
            }
        }
    }
    Ok(set)
}

/// The property sets of a property set stream.
pub fn parse(data: &[u8]) -> Result<Vec<PropertySet>, PropertyError> {
    let r = Reader { data };
    if r.u16(0)? != 0xFFFE {
        return Err(PropertyError("no byte order mark".into()));
    }
    let count = r.u32(24)? as usize;
    if count > 2 {
        return Err(PropertyError(format!("{count} property sets")));
    }
    let mut sets = Vec::with_capacity(count);
    for i in 0..count {
        let mut fmtid = [0u8; 16];
        fmtid.copy_from_slice(r.bytes(28 + 20 * i, 16)?);
        let offset = r.u32(44 + 20 * i)? as usize;
        sets.push(property_set(&r, offset, fmtid)?);
    }
    Ok(sets)
}

/// Writes a property set stream with one set (for tests): the code page
/// is 1200 (UTF-16), so text values are written as `VT_LPWSTR`.
pub fn write(fmtid: [u8; 16], values: &[(u32, Value)]) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    let mut table: Vec<(u32, usize)> = Vec::new();
    let mut all: Vec<(u32, Value)> = vec![(1, Value::Int(1200))];
    all.extend(values.iter().cloned());
    let table_size = 8 + 8 * all.len();
    for (id, value) in &all {
        table.push((*id, table_size + body.len()));
        let mut v = Vec::new();
        match value {
            Value::Int(i) if *id == 1 => {
                v.extend_from_slice(&2u32.to_le_bytes());
                v.extend_from_slice(&(*i as i16).to_le_bytes());
            }
            Value::Int(i) => {
                v.extend_from_slice(&3u32.to_le_bytes());
                v.extend_from_slice(&(*i as i32).to_le_bytes());
            }
            Value::Real(r) => {
                v.extend_from_slice(&5u32.to_le_bytes());
                v.extend_from_slice(&r.to_le_bytes());
            }
            Value::Bool(b) => {
                v.extend_from_slice(&0x0Bu32.to_le_bytes());
                v.extend_from_slice(&(if *b { 0xFFFFu16 } else { 0 }).to_le_bytes());
            }
            Value::Text(t) => {
                let units: Vec<u16> = t.encode_utf16().chain(std::iter::once(0)).collect();
                v.extend_from_slice(&0x1Fu32.to_le_bytes());
                v.extend_from_slice(&(units.len() as u32).to_le_bytes());
                for u in units {
                    v.extend_from_slice(&u.to_le_bytes());
                }
            }
            Value::FileTime(t) => {
                v.extend_from_slice(&0x40u32.to_le_bytes());
                v.extend_from_slice(&t.to_le_bytes());
            }
            Value::Clsid(c) => {
                v.extend_from_slice(&0x48u32.to_le_bytes());
                v.extend_from_slice(c);
            }
            Value::Other(t) => v.extend_from_slice(&u32::from(*t).to_le_bytes()),
        }
        while v.len() % 4 != 0 {
            v.push(0);
        }
        body.extend_from_slice(&v);
    }
    let mut set = Vec::new();
    set.extend_from_slice(&((table_size + body.len()) as u32).to_le_bytes());
    set.extend_from_slice(&(all.len() as u32).to_le_bytes());
    for (id, offset) in table {
        set.extend_from_slice(&id.to_le_bytes());
        set.extend_from_slice(&(offset as u32).to_le_bytes());
    }
    set.extend_from_slice(&body);

    let mut out = Vec::new();
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0x0002_0006u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&fmtid);
    out.extend_from_slice(&48u32.to_le_bytes());
    out.extend_from_slice(&set);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        let fmtid = crate::cfb::guid_bytes("32853f0f-3444-11d1-9e93-0060b03c1ca6").unwrap();
        let data = write(
            fmtid,
            &[
                (5, Value::Text("P-100".into())),
                (8, Value::Int(11269)),
                (61, Value::Real(1.5)),
                (62, Value::Bool(true)),
            ],
        );
        let sets = parse(&data).unwrap();
        assert_eq!(sets.len(), 1);
        let s = &sets[0];
        assert_eq!(s.fmtid, fmtid);
        assert_eq!(s.code_page, Some(1200));
        assert_eq!(s.text(5), Some("P-100"));
        assert_eq!(s.int(8), Some(11269));
        assert_eq!(s.real(61), Some(1.5));
        assert_eq!(s.values.get(&62), Some(&Value::Bool(true)));
    }

    #[test]
    fn reads_a_dictionary_and_byte_strings() {
        // A set in code page 1252 with a dictionary and an LPSTR.
        let mut set = Vec::new();
        let entries: Vec<(u32, Vec<u8>)> = vec![
            (
                1,
                [
                    2u32.to_le_bytes().as_slice(),
                    &1252u16.to_le_bytes(),
                    &[0, 0],
                ]
                .concat(),
            ),
            (
                0,
                [
                    1u32.to_le_bytes().as_slice(),
                    &2u32.to_le_bytes(),
                    &5u32.to_le_bytes(),
                    b"Mass\0",
                    &[0, 0, 0],
                ]
                .concat(),
            ),
            (
                2,
                [
                    0x1Eu32.to_le_bytes().as_slice(),
                    &4u32.to_le_bytes(),
                    b"abc\0",
                ]
                .concat(),
            ),
        ];
        let table = 8 + 8 * entries.len();
        let mut body = Vec::new();
        let mut offsets = Vec::new();
        for (id, bytes) in &entries {
            offsets.push((*id, table + body.len()));
            body.extend_from_slice(bytes);
        }
        set.extend_from_slice(&((table + body.len()) as u32).to_le_bytes());
        set.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        for (id, o) in offsets {
            set.extend_from_slice(&id.to_le_bytes());
            set.extend_from_slice(&(o as u32).to_le_bytes());
        }
        set.extend_from_slice(&body);
        let mut stream = vec![0xFE, 0xFF, 0, 0, 6, 0, 2, 0];
        stream.extend_from_slice(&[0; 16]);
        stream.extend_from_slice(&1u32.to_le_bytes());
        stream.extend_from_slice(&[7; 16]);
        stream.extend_from_slice(&48u32.to_le_bytes());
        stream.extend_from_slice(&set);
        let sets = parse(&stream).unwrap();
        assert_eq!(sets[0].code_page, Some(1252));
        assert_eq!(sets[0].names.get(&2).map(String::as_str), Some("Mass"));
        assert_eq!(sets[0].text(2), Some("abc"));
        // Damaged: a property offset past the end.
        let mut damaged = stream.clone();
        let at = 48 + 8 + 4;
        damaged[at..at + 4].copy_from_slice(&10_000u32.to_le_bytes());
        assert!(parse(&damaged).is_err());
        assert!(parse(b"nope").is_err());
    }
}
