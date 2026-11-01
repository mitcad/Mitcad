// SPDX-License-Identifier: MIT
//! Reader for `.dwg` drawings, versions R13 to R2018 (`AC1012` to
//! `AC1032`): the 3D content a drawing holds, for the import (mitcad#453).
//! Read-only; no geometry (the import builds the bodies).
//!
//! Layers, bottom up:
//! - [`bits`]: the bit-coded values; [`crc`]: the checks.
//! - [`container`]: the file header and the sections (R13 to R2000 by
//!   their locators; R2004, R2010, R2013 and R2018 in compressed pages,
//!   [`lz`]; R2007 in its own pages, [`r2007`]).
//! - [`header`]: the header variables (units, extents); [`classes`]; the
//!   object map ([`objmap`]); the objects ([`objects`]); the data storage
//!   of R2013 on ([`datastore`]).
//! - [`solid`]: the B-rep record of solids, bodies, regions and surfaces,
//!   an ASM file in its text or binary form, parsed and converted by the
//!   `.f3d` reader's ASM code.
//! - [`scene`]: the drawing's 3D content as blocks of bodies and meshes
//!   with their inserts, and the plan of components the import builds.
//!
//! The writer ([`write`], mitcad#728): a 2D drawing (the drawing export's
//! sheets) as an R2000 file; [`flat`] reads such content back.
//!
//! The format knowledge comes from the Open Design Specification for .dwg
//! files (Open Design Alliance), a published description of the format.

pub mod bits;
pub mod classes;
pub mod container;
pub mod crc;
pub mod datastore;
pub mod header;
pub mod lz;
pub mod objects;
pub mod objmap;
pub mod r2007;
// The 3D content needs the `.f3d` reader's ASM code (feature `solids`,
// on by default; mitcad-model's drawing export goes without, mitcad#728).
#[cfg(feature = "solids")]
pub mod scene;
pub mod solid;
#[cfg(feature = "solids")]
#[doc(hidden)]
pub mod testfile;
// The 2D drawing writer (mitcad#728) and its read-back.
pub mod flat;
pub mod write;

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

pub use classes::Class;
pub use header::HeaderVars;
pub use objects::Object;
// Drawing units, shared with the DXF reader.
pub use mitcad_dxf::Units;

/// A drawing's format version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Version {
    R13,
    R14,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl Version {
    /// The version of the id the file starts with.
    pub fn from_id(id: &str) -> Option<Version> {
        Some(match id {
            "AC1012" | "AC1013" => Version::R13,
            "AC1014" => Version::R14,
            "AC1015" => Version::R2000,
            "AC1018" => Version::R2004,
            "AC1021" => Version::R2007,
            "AC1024" => Version::R2010,
            "AC1027" => Version::R2013,
            "AC1032" => Version::R2018,
            _ => return None,
        })
    }

    pub fn id(self) -> &'static str {
        match self {
            Version::R13 => "AC1012",
            Version::R14 => "AC1014",
            Version::R2000 => "AC1015",
            Version::R2004 => "AC1018",
            Version::R2007 => "AC1021",
            Version::R2010 => "AC1024",
            Version::R2013 => "AC1027",
            Version::R2018 => "AC1032",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Version::R13 => "R13",
            Version::R14 => "R14",
            Version::R2000 => "R2000",
            Version::R2004 => "R2004",
            Version::R2007 => "R2007",
            Version::R2010 => "R2010",
            Version::R2013 => "R2013",
            Version::R2018 => "R2018",
        }
    }

    pub const ALL: [Version; 8] = [
        Version::R13,
        Version::R14,
        Version::R2000,
        Version::R2004,
        Version::R2007,
        Version::R2010,
        Version::R2013,
        Version::R2018,
    ];
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name(), self.id())
    }
}

/// Why a drawing cannot be read.
#[derive(Clone, Debug, PartialEq)]
pub enum DwgError {
    /// Not a drawing at all.
    NotDwg(String),
    /// A drawing of a kind the reader does not read.
    Unsupported(String),
    /// Data that does not follow the format.
    Corrupt(String),
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DwgError::NotDwg(e) => write!(f, "not a .dwg drawing: {e}"),
            DwgError::Unsupported(e) => write!(f, "not supported: {e}"),
            DwgError::Corrupt(e) => write!(f, "damaged or unexpected data: {e}"),
        }
    }
}

impl std::error::Error for DwgError {}

/// A drawing read from its bytes.
#[derive(Clone, Debug)]
pub struct DwgFile {
    pub version: Version,
    pub maintenance: u8,
    pub codepage: u16,
    /// The sections the file lists with their sizes.
    pub sections: BTreeMap<String, u64>,
    /// None when the header variables could not be read (see `warnings`).
    pub header: Option<HeaderVars>,
    pub classes: Vec<Class>,
    /// The objects by handle.
    pub objects: BTreeMap<u64, Object>,
    /// The objects the map lists that could not be read: handle, type
    /// (when known) and why.
    pub failed: Vec<(u64, String, String)>,
    /// The B-rep records of the data storage (R2013 on) by handle.
    pub stored_breps: BTreeMap<u64, Vec<u8>>,
    pub warnings: Vec<String>,
}

impl DwgFile {
    pub fn open(path: &Path) -> Result<DwgFile, DwgError> {
        let bytes = std::fs::read(path)
            .map_err(|e| DwgError::NotDwg(format!("{}: {e}", path.display())))?;
        read(&bytes)
    }

    /// Objects of the type (by DXF name), in handle order.
    pub fn objects_of<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Object> + 'a {
        self.objects.values().filter(move |o| o.type_name == name)
    }

    /// Object counts by type name.
    pub fn type_counts(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for o in self.objects.values() {
            *m.entry(o.type_name.clone()).or_insert(0) += 1;
        }
        m
    }
}

/// Reads a drawing.
pub fn read(bytes: &[u8]) -> Result<DwgFile, DwgError> {
    let (version, sections) = container::read(bytes)?;
    let mut warnings = Vec::new();
    let header = match header::read(&sections.header, version, sections.maintenance) {
        Ok(h) => Some(h),
        Err(e) => {
            warnings.push(format!(
                "the header variables could not be read ({e}): units and extents unknown"
            ));
            None
        }
    };
    let classes = match classes::read(&sections.classes, version, sections.maintenance) {
        Ok(c) => c,
        Err(e) => {
            warnings.push(format!(
                "the classes could not be read ({e}): objects of class types are not recognized"
            ));
            Vec::new()
        }
    };
    let map = objmap::read(&sections.handles)?;
    let mut objects = BTreeMap::new();
    let mut failed = Vec::new();
    for (&handle, &offset) in &map {
        match objects::read(&sections.objects, offset as usize, version, &classes) {
            Ok(o) => {
                if o.handle != handle {
                    failed.push((
                        handle,
                        o.type_name.clone(),
                        format!("the map's object has the handle {:X}", o.handle),
                    ));
                    continue;
                }
                objects.insert(handle, o);
            }
            Err(e) => {
                // The type, when the object's start can be read.
                let kind = object_type_at(&sections.objects, offset as usize, version, &classes)
                    .unwrap_or_default();
                failed.push((handle, kind, e.to_string()));
            }
        }
    }
    let stored_breps = match &sections.data_storage {
        Some(data) => match datastore::brep_records(data) {
            Ok(m) => m,
            Err(e) => {
                warnings.push(format!(
                    "the data storage could not be read ({e}): solids stored there are missing"
                ));
                BTreeMap::new()
            }
        },
        None => BTreeMap::new(),
    };
    Ok(DwgFile {
        version,
        maintenance: sections.maintenance,
        codepage: sections.codepage,
        sections: sections.listed,
        header,
        classes,
        objects,
        failed,
        stored_breps,
        warnings,
    })
}

/// An object's bytes as stored (from its size on, with its CRC), and the
/// version: for diagnostics (`mitcad-dwg-inspect --dump`).
pub fn object_bytes(bytes: &[u8], handle: u64) -> Result<(Version, Vec<u8>), DwgError> {
    let (version, sections) = container::read(bytes)?;
    let map = objmap::read(&sections.handles)?;
    let offset = *map
        .get(&handle)
        .ok_or_else(|| DwgError::Corrupt(format!("no object {handle:X} in the map")))?
        as usize;
    let mut p = offset;
    let size = bits::modular_short(&sections.objects, &mut p)? as usize;
    if version >= Version::R2010 {
        bits::modular_char(&sections.objects, &mut p, false)?;
    }
    let end = (p + size + 2).min(sections.objects.len());
    Ok((version, sections.objects[offset..end].to_vec()))
}

/// The data storage section's bytes (R2013 on): for diagnostics.
pub fn data_storage_bytes(bytes: &[u8]) -> Result<Option<Vec<u8>>, DwgError> {
    Ok(container::read(bytes)?.1.data_storage)
}

/// The type name of the object at `offset`, if its start can be read.
fn object_type_at(objects: &[u8], offset: usize, v: Version, classes: &[Class]) -> Option<String> {
    let mut p = offset;
    let size = bits::modular_short(objects, &mut p).ok()? as usize;
    if v >= Version::R2010 {
        bits::modular_char(objects, &mut p, false).ok()?;
    }
    let mut r = bits::BitReader::at(objects, v, p * 8, (p + size) * 8);
    let code = r.ot().ok()?;
    Some(objects::type_info(code, classes).0)
}
