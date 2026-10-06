// SPDX-License-Identifier: MIT
//! The FreeCAD version that saved a document, and the value lists of
//! enumeration properties per version.
//!
//! Documents store an enumeration as an index into its type's list of
//! values, and the lists change between versions. `data/enums.json` holds
//! them per major.minor version, read from FreeCAD itself
//! (`tools/freecad-export/enums.py`); a document is read with the table of
//! its version, else the nearest older one (the oldest for older files).
//! Lists that depend on another property's value are listed per value: a
//! hole's thread sizes for ISO metric threads are the property
//! `ThreadSize[ISOMetricProfile]` of `PartDesign::Hole`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// `ProgramVersion`: `0.21R33771 (Git)`, `1.0.2R39319 (Git)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
}

impl Version {
    /// The version in a `ProgramVersion` text; 0.0 when there is none.
    pub fn parse(text: &str) -> Version {
        let text = text.trim();
        let (numbers, rest) = match text.find(['R', ' ']) {
            Some(at) => (&text[..at], &text[at..]),
            None => (text, ""),
        };
        let mut parts = numbers.split('.').map(|p| p.trim().parse::<u32>().ok());
        let major = parts.next().flatten().unwrap_or(0);
        let minor = parts.next().flatten().unwrap_or(0);
        let patch = parts.next().flatten();
        let revision = rest.strip_prefix('R').and_then(|r| {
            let digits: String = r.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        });
        Version {
            major,
            minor,
            patch,
            revision,
        }
    }

    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)?;
        if let Some(patch) = self.patch {
            write!(f, ".{patch}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct Tables {
    versions: BTreeMap<String, VersionTable>,
}

#[derive(Debug, Deserialize)]
struct VersionTable {
    /// The release the table was read from.
    freecad: String,
    #[serde(default)]
    document: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    types: BTreeMap<String, BTreeMap<String, Vec<String>>>,
}

fn tables() -> &'static [((u32, u32), VersionTable)] {
    static TABLES: OnceLock<Vec<((u32, u32), VersionTable)>> = OnceLock::new();
    TABLES.get_or_init(|| {
        let tables: Tables = serde_json::from_str(include_str!("../data/enums.json"))
            .expect("data/enums.json is valid");
        tables
            .versions
            .into_iter()
            .map(|(key, table)| {
                let v = Version::parse(&key);
                ((v.major, v.minor), table)
            })
            .collect()
    })
}

fn table_for(version: &Version) -> Option<&'static VersionTable> {
    let tables = tables();
    let wanted = (version.major, version.minor);
    tables
        .iter()
        .rev()
        .find(|(v, _)| *v <= wanted)
        .or_else(|| tables.first())
        .map(|(_, t)| t)
}

/// The values of an object type's enumeration property in a version.
pub fn enum_values(
    version: &Version,
    type_name: &str,
    property: &str,
) -> Option<&'static [String]> {
    table_for(version)?
        .types
        .get(type_name)?
        .get(property)
        .map(Vec::as_slice)
}

/// The values of a document property's enumeration (`UnitSystem`).
pub fn document_enum_values(version: &Version, property: &str) -> Option<&'static [String]> {
    table_for(version)?
        .document
        .get(property)
        .map(Vec::as_slice)
}

/// The FreeCAD release whose tables a version is read with.
pub fn table_release(version: &Version) -> Option<&'static str> {
    table_for(version).map(|t| t.freecad.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_program_versions() {
        let v = Version::parse("1.0.2R39319 (Git)");
        assert_eq!(
            (v.major, v.minor, v.patch, v.revision),
            (1, 0, Some(2), Some(39319))
        );
        let v = Version::parse("0.21R33771 (Git)");
        assert_eq!(
            (v.major, v.minor, v.patch, v.revision),
            (0, 21, None, Some(33771))
        );
        assert_eq!(v.to_string(), "0.21");
        assert!(v.at_least(0, 19) && !v.at_least(1, 0));
        assert_eq!(Version::parse("").major, 0);
        assert_eq!(
            Version::parse("1.1.4R20260928 (Git shallow)").patch,
            Some(4)
        );
    }

    #[test]
    fn enumerations_come_from_the_nearest_older_table() {
        // A pad's type list exists in every table; the first value is the
        // same throughout.
        for text in ["0.19R24291", "0.21R33771", "1.0R39319", "1.1.4R1", "2.0R1"] {
            let values = enum_values(&Version::parse(text), "PartDesign::Pad", "Type")
                .unwrap_or_else(|| panic!("{text}: no table"));
            assert_eq!(values[0], "Length", "{text}");
        }
        assert!(table_release(&Version::parse("0.18R1")).is_some());
        assert!(enum_values(&Version::parse("1.0"), "No::Such", "Type").is_none());
        // A hole's M6: its size's name changed in 1.1, not its place.
        let size = |v: &str| {
            enum_values(
                &Version::parse(v),
                "PartDesign::Hole",
                "ThreadSize[ISOMetricProfile]",
            )
            .map(|sizes| sizes[14].clone())
        };
        assert_eq!(size("0.21R1").as_deref(), Some("M6"));
        assert_eq!(size("1.1.4R1").as_deref(), Some("M6x1.0"));
    }
}
