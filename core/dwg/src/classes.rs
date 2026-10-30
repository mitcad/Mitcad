// SPDX-License-Identifier: MIT
//! The classes (the specification's chapter 10 and 5.8): the object types
//! from 500 on, each named by its DXF name (`MESH`, `PLANESURFACE`, ...).
//!
//! The layout around the records differs between the versions and the
//! specification's chapters do not agree on every width; the reader tries
//! the layouts it describes and takes the one whose records number the
//! classes from 500 on, each producing entities or objects.

use crate::bits::BitReader;
use crate::container::CLASSES_SENTINEL;
use crate::{DwgError, Version};

/// A class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Class {
    /// The object type of its objects (500 on).
    pub number: u16,
    pub app_name: String,
    pub cpp_name: String,
    pub dxf_name: String,
    /// It produces entities (0x1F2), not objects (0x1F3).
    pub entity: bool,
}

/// One layout of the records.
#[derive(Clone, Copy, Debug)]
struct Layout {
    /// The extra fields of R2004 on are BL, BS, BS, BL, BL; else
    /// BL, BL, BL, BL, BL.
    short_versions: bool,
}

fn record(
    r: &mut BitReader,
    strings: &mut Option<BitReader>,
    v: Version,
    layout: Layout,
) -> Result<Class, DwgError> {
    let number = r.bs()? as u16;
    r.bs()?;
    let mut text = |r: &mut BitReader| -> Result<String, DwgError> {
        match strings {
            Some(s) => s.text16(),
            None => r.text(),
        }
    };
    let app_name = text(r)?;
    let cpp_name = text(r)?;
    let dxf_name = text(r)?;
    r.bit()?;
    let item = r.bs()? as u16;
    if v >= Version::R2004 {
        r.bl()?;
        if layout.short_versions {
            r.bs()?;
            r.bs()?;
        } else {
            r.bl()?;
            r.bl()?;
        }
        r.bl()?;
        r.bl()?;
    }
    if item != 0x1f2 && item != 0x1f3 {
        return Err(DwgError::Corrupt(format!(
            "class {number} ({dxf_name}) has the item class id {item:#x}"
        )));
    }
    Ok(Class {
        number,
        app_name,
        cpp_name,
        dxf_name,
        entity: item == 0x1f2,
    })
}

fn plausible(classes: &[Class]) -> bool {
    classes.iter().enumerate().all(|(i, c)| {
        usize::from(c.number) == 500 + i
            && !c.dxf_name.is_empty()
            && c.dxf_name.chars().all(|ch| ch.is_ascii_graphic())
    })
}

/// Reads the classes from the classes section (from its sentinel on).
pub fn read(section: &[u8], version: Version, maintenance: u8) -> Result<Vec<Class>, DwgError> {
    if section.is_empty() {
        return Ok(Vec::new());
    }
    if section.get(..16) != Some(&CLASSES_SENTINEL[..]) {
        return Err(DwgError::Corrupt("the classes have no sentinel".into()));
    }
    let size = section
        .get(16..20)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize)
        .ok_or_else(|| DwgError::Corrupt("the classes end after their sentinel".into()))?;
    let mut start = 20;
    if (matches!(version, Version::R2010 | Version::R2013) && maintenance > 3)
        || version >= Version::R2018
    {
        start += 4;
    }
    let data_end = (start + size).min(section.len());
    let mut last = None;
    for short_versions in [true, false] {
        let layout = Layout { short_versions };
        for (bit_size, long_max) in candidates(version) {
            match read_with(
                section, version, start, data_end, layout, bit_size, long_max,
            ) {
                Ok(classes) if plausible(&classes) => return Ok(classes),
                Ok(_) => {}
                Err(e) => last = Some(e),
            }
        }
        if version < Version::R2004 {
            break;
        }
    }
    Err(last.unwrap_or_else(|| DwgError::Corrupt("the classes do not number from 500".into())))
}

/// The layouts of the section's start: a bit size (R2007 on), the
/// maximum class number as BL instead of BS.
fn candidates(v: Version) -> Vec<(bool, bool)> {
    if v < Version::R2004 {
        return vec![(false, false)];
    }
    if v < Version::R2007 {
        return vec![(false, false), (false, true)];
    }
    vec![(true, false), (true, true), (false, false), (false, true)]
}

fn read_with(
    section: &[u8],
    v: Version,
    start: usize,
    data_end: usize,
    layout: Layout,
    bit_size: bool,
    long_max: bool,
) -> Result<Vec<Class>, DwgError> {
    let mut r = BitReader::at(section, v, start * 8, data_end * 8);
    let mut end = data_end * 8;
    let mut strings = None;
    if bit_size {
        let bits = r.rl()? as usize;
        // The bit size counts from the section's data, before or after
        // itself; either way the string stream ends just before it.
        for base in [start * 8, (start + 4) * 8] {
            let e = base + bits;
            if e <= data_end * 8
                && let Ok(Some(s)) = r.string_stream(e)
            {
                end = e;
                strings = Some(s);
                break;
            }
        }
        if strings.is_none() {
            return Err(DwgError::Corrupt("no string stream in the classes".into()));
        }
    }
    let mut out = Vec::new();
    if v >= Version::R2004 {
        if long_max {
            r.bl()?;
        } else {
            r.bs()?;
        }
        if !bit_size || !long_max {
            r.rc()?;
            r.rc()?;
        }
        r.bit()?;
    }
    let stop = match &strings {
        Some(s) => s.position().min(end),
        None => end,
    };
    // Records until the data runs out (a record takes at least 30 bits).
    while r.position() + 30 <= stop {
        out.push(record(&mut r, &mut strings, v, layout)?);
    }
    Ok(out)
}

/// Writes the classes (the test files) as `read` reads them: R2004 on
/// with BS versions; R2007 on with a bit size counted from the data and
/// the names in a string stream.
pub fn write(classes: &[Class], v: Version) -> Vec<u8> {
    use crate::bits::BitWriter;
    let mut w = BitWriter::new(v);
    let mut s = BitWriter::new(v);
    if v >= Version::R2004 {
        w.bs(classes.len() as i16 + 499).rc(0).rc(0).bit(true);
    }
    for c in classes {
        w.bs(c.number as i16).bs(0);
        for name in [&c.app_name, &c.cpp_name, &c.dxf_name] {
            if v >= Version::R2007 {
                s.text16(name);
            } else {
                w.text(name);
            }
        }
        w.bit(false).bs(if c.entity { 0x1f2 } else { 0x1f3 });
        if v >= Version::R2004 {
            w.bl(0).bs(0).bs(0).bl(0).bl(0);
        }
    }
    let mut body = BitWriter::new(v);
    if v >= Version::R2007 {
        let mut data = w.clone();
        data.append(&s);
        data.rs(s.len as u16).bit(true);
        // The bit size counts from the start of the section's data: the
        // 32 bits of the size field and the data.
        body.rl((data.len + 32) as u32);
        body.append(&data);
    } else {
        body = w;
    }
    body.align();
    let mut out = CLASSES_SENTINEL.to_vec();
    out.extend_from_slice(&(body.data.len() as u32).to_le_bytes());
    if (matches!(v, Version::R2010 | Version::R2013) && crate::header::MAINTENANCE > 3)
        || v >= Version::R2018
    {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(&body.data);
    let crc = crate::crc::crc16(0xC0C1, &out[16..]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&crate::container::CLASSES_END_SENTINEL);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_section_of_the_sentinel_only_is_corrupt() {
        // The size after the sentinel was indexed directly (mitcad#931):
        // a panic.
        assert!(matches!(
            read(&CLASSES_SENTINEL, Version::R2000, 0),
            Err(DwgError::Corrupt(_))
        ));
        let mut short = CLASSES_SENTINEL.to_vec();
        short.extend([1, 2, 3]);
        for v in Version::ALL {
            assert!(read(&short, v, 0).is_err(), "{v}");
        }
    }
}
