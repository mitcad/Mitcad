// SPDX-License-Identifier: MIT
//! The manifests of libraries and indexes (mitcad#64, mitcad#63): what a
//! library repository's `mitcad-library.json` and a community index's
//! `mitcad-index.json` and `libraries/<id>.json` say, and what is wrong
//! with them. The format is documented in `docs/libraries.md`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// The manifest at a library repository's root.
pub const LIBRARY_MANIFEST: &str = "mitcad-library.json";
/// The manifest at a community index's root.
pub const INDEX_MANIFEST: &str = "mitcad-index.json";
/// The folder of an index's entries, one file per library.
pub const INDEX_ENTRIES: &str = "libraries";
/// The largest manifest or index entry read.
pub const MAX_MANIFEST: usize = 4 << 20;
/// The largest preview image.
pub const MAX_PREVIEW: usize = 512 << 10;
/// The largest component design read from a library.
pub const MAX_COMPONENT: usize = 64 << 20;

/// Licences a community index accepts, with what a user of such a part
/// should know (not legal advice).
pub const ACCEPTED_LICENSES: [(&str, &str); 10] = [
    ("CC0-1.0", "No conditions."),
    (
        "CC-BY-4.0",
        "Credit the authors when you share designs that contain it.",
    ),
    (
        "CC-BY-SA-4.0",
        "Credit the authors; designs you share that contain it may need the same licence.",
    ),
    (
        "MIT",
        "Keep the copyright and licence notice when you share it.",
    ),
    (
        "Apache-2.0",
        "Keep the copyright, licence and notice files when you share it.",
    ),
    (
        "BSD-2-Clause",
        "Keep the copyright and licence notice when you share it.",
    ),
    (
        "BSD-3-Clause",
        "Keep the copyright and licence notice when you share it.",
    ),
    (
        "CERN-OHL-P-2.0",
        "A permissive hardware licence: keep the notices.",
    ),
    (
        "CERN-OHL-W-2.0",
        "A weakly reciprocal hardware licence: changes to it are shared under the same licence.",
    ),
    (
        "CERN-OHL-S-2.0",
        "A strongly reciprocal hardware licence: designs you share that contain it may need the \
         same licence.",
    ),
];

/// What a user should know about a licence, when it is an accepted one.
pub fn license_note(license: &str) -> Option<&'static str> {
    ACCEPTED_LICENSES
        .iter()
        .find(|(id, _)| id.eq_ignore_ascii_case(license.trim()))
        .map(|(_, note)| *note)
}

/// A category of a library's components.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    #[serde(default)]
    pub name: String,
}

/// A component of a library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestComponent {
    pub id: String,
    /// The design in the repository (`bolts/iso4762.mitcad`).
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub standard: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Search words and tags (`socket head`, `DIN 912`).
    #[serde(default, alias = "tags", skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// A PNG image of the default configuration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preview: String,
    /// The component's own licence; the library's when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// How a bill of materials names a part (`ISO 4762 {config}`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designation: String,
}

/// `mitcad-library.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The library's own version (`1.2.0`), matching its tag `v1.2.0`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub library_version: String,
    /// An SPDX licence expression for every component without its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    /// Where the data came from (standards' tables, other sources) and
    /// their licences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub min_mitcad: String,
    #[serde(default = "millimetres")]
    pub units: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<Category>,
    #[serde(default)]
    pub components: Vec<ManifestComponent>,
}

fn millimetres() -> String {
    "mm".to_owned()
}

/// `mitcad-index.json` at an index's root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexManifest {
    pub format: String,
    pub version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A component an index entry lists, so that it can be found before its
/// library is fetched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexComponent {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub standard: String,
    #[serde(default, alias = "keywords", skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A version of a library an index's maintainers have looked at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reviewed {
    pub rev: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub date: String,
}

/// `libraries/<id>.json` in an index: one library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<IndexComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviewed: Vec<Reviewed>,
}

pub const LIBRARY_FORMAT: &str = "mitcad-library";
pub const INDEX_FORMAT: &str = "mitcad-index";
pub const ENTRY_FORMAT: &str = "mitcad-index-entry";

/// What is wrong with a manifest: errors make it unusable, warnings are
/// told.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Problems {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Problems {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, text: impl Into<String>) {
        self.errors.push(text.into());
    }

    fn warn(&mut self, text: impl Into<String>) {
        self.warnings.push(text.into());
    }
}

/// Whether `id` is a library or component id: 2 to 64 of `a-z`, `0-9`
/// and `-`, starting with a letter or digit.
pub fn valid_id(id: &str) -> bool {
    (2..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

/// Whether `path` is a path inside a repository a library may name:
/// relative, with `/`, without `.`, `..`, `.git` or empty parts.
pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.eq_ignore_ascii_case(".git")
        })
}

/// Reads JSON of at most [`MAX_MANIFEST`] bytes.
pub fn read_json<T: for<'de> Deserialize<'de>>(data: &[u8], what: &str) -> Result<T, String> {
    if data.len() > MAX_MANIFEST {
        return Err(format!(
            "{what} is {} bytes, more than the {} a manifest may have",
            data.len(),
            MAX_MANIFEST
        ));
    }
    let text = std::str::from_utf8(data).map_err(|_| format!("{what} is not UTF-8 text"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    serde_json::from_str(text).map_err(|e| format!("{what}: {e}"))
}

impl Manifest {
    /// The component of `id`.
    pub fn component(&self, id: &str) -> Option<&ManifestComponent> {
        self.components.iter().find(|c| c.id == id)
    }

    /// A component's licence: its own, else the library's.
    pub fn license_of(&self, component: &ManifestComponent) -> Option<String> {
        component
            .license
            .clone()
            .or_else(|| self.license.clone())
            .filter(|l| !l.trim().is_empty())
    }

    /// What is wrong with the manifest itself (files are checked where
    /// they are read).
    pub fn problems(&self) -> Problems {
        let mut out = Problems::default();
        if self.format != LIBRARY_FORMAT {
            out.error(format!(
                "the format is '{}', not '{LIBRARY_FORMAT}'",
                self.format
            ));
        }
        if self.version != 1 {
            out.error(format!(
                "version {} of the library format is not one this Mitcad reads (1)",
                self.version
            ));
        }
        if !valid_id(&self.id) {
            out.error(format!(
                "'{}' is no library id (2 to 64 of a-z, 0-9 and -)",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            out.error("the library has no name");
        }
        match self.license.as_deref().map(str::trim) {
            None | Some("") => {
                if self.components.iter().any(|c| c.license.is_none()) {
                    out.warn(
                        "the library names no licence; components without one are not shown \
                         unless asked for",
                    );
                }
            }
            Some(license) if license_note(license).is_none() => out.warn(format!(
                "the licence {license} is not one a community index accepts"
            )),
            Some(_) => {}
        }
        if self.units != "mm" {
            out.warn(format!(
                "the library's units are '{}'; Mitcad's libraries are in mm",
                self.units
            ));
        }
        let categories: BTreeSet<&str> = self.categories.iter().map(|c| c.id.as_str()).collect();
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for (i, c) in self.components.iter().enumerate() {
            let label = if c.id.is_empty() {
                format!("components[{i}]")
            } else {
                c.id.clone()
            };
            if !valid_id(&c.id) {
                out.error(format!("{label}: '{}' is no component id", c.id));
            } else if !ids.insert(c.id.as_str()) {
                out.error(format!("{label}: the id is used twice"));
            }
            if !valid_path(&c.path) || !c.path.ends_with(".mitcad") {
                out.error(format!(
                    "{label}: '{}' is no path of a design in the library (relative, with /, \
                     ending in .mitcad)",
                    c.path
                ));
            } else if !paths.insert(c.path.as_str()) {
                out.warn(format!(
                    "{label}: another component has the design {}",
                    c.path
                ));
            }
            if c.name.trim().is_empty() {
                out.error(format!("{label}: no name"));
            }
            if !c.category.is_empty() && !categories.contains(c.category.as_str()) {
                out.warn(format!(
                    "{label}: the category {} is not in the library's categories",
                    c.category
                ));
            }
            if !c.preview.is_empty() && (!valid_path(&c.preview) || !c.preview.ends_with(".png")) {
                out.error(format!(
                    "{label}: '{}' is no path of a PNG image in the library",
                    c.preview
                ));
            }
            if let Some(license) = c.license.as_deref()
                && license_note(license).is_none()
            {
                out.warn(format!(
                    "{label}: the licence {license} is not one a community index accepts"
                ));
            }
        }
        out
    }
}

impl IndexEntry {
    pub fn problems(&self) -> Problems {
        let mut out = Problems::default();
        if self.format != ENTRY_FORMAT || self.version != 1 {
            out.error(format!(
                "not an index entry of a format this Mitcad reads ({ENTRY_FORMAT}, version 1)"
            ));
        }
        if !valid_id(&self.id) {
            out.error(format!("'{}' is no library id", self.id));
        }
        if self.url.trim().is_empty() {
            out.error("the entry has no URL");
        } else if let Err(e) = crate::remote::check_url(&self.url) {
            out.error(e.message);
        }
        match self.license.as_deref() {
            None => out.warn("the entry names no licence"),
            Some(license) if license_note(license).is_none() => out.warn(format!(
                "the licence {license} is not one the index accepts"
            )),
            Some(_) => {}
        }
        for reviewed in &self.reviewed {
            if reviewed.rev.len() != 40 || !reviewed.rev.bytes().all(|b| b.is_ascii_hexdigit()) {
                out.error(format!(
                    "reviewed version '{}' is no commit id (40 hex digits)",
                    reviewed.rev
                ));
            }
        }
        out
    }
}

/// Words of a search: lowercase, split at spaces.
pub fn search_words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_lowercase).collect()
}

/// Whether every word is in one of the texts (case-insensitive).
pub fn matches(words: &[String], texts: &[&str]) -> bool {
    let haystack: String = texts
        .iter()
        .map(|t| t.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");
    words.iter().all(|w| haystack.contains(w.as_str()))
}
