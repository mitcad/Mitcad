// SPDX-License-Identifier: MIT
//! The manifests of libraries and indexes (mitcad#64, mitcad#63): what a
//! library repository's `mitcad-library.json` and a community index's
//! `mitcad-index.json` and `libraries/<id>.json` say, and what is wrong
//! with them. The format is documented in `docs/libraries.md`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub use mitcad_model::library::{LibraryRow, NON_STANDARD, SupplierPart};

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

/// A category of a library's components. Categories make a tree of groups
/// (mitcad#527): `parent` names another category of the library by its
/// id, and `name` may be a path of names (`Wood/Dimensioned timber`); a
/// category with neither is a group at the top. Libraries share a group
/// by its names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: String,
}

/// A component's `category`: one category's id or a list of them
/// (mitcad#527), the first its home (its folder, the group a one-group
/// view shows it in).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CategoryIds(pub Vec<String>);

impl CategoryIds {
    pub fn one(id: impl Into<String>) -> Self {
        let id = id.into();
        if id.is_empty() {
            Self::default()
        } else {
            Self(vec![id])
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|id| id.trim().is_empty())
    }
}

impl Serialize for CategoryIds {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0.as_slice() {
            [] => serializer.serialize_str(""),
            [one] => serializer.serialize_str(one),
            many => many.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for CategoryIds {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            One(String),
            Many(Vec<String>),
        }
        Ok(match Written::deserialize(deserializer)? {
            Written::One(id) => Self::one(id),
            Written::Many(ids) => Self(ids),
        })
    }
}

/// The group a category stands for (mitcad#527): its names from the top
/// group down and, for each name, the place in the manifest's list of the
/// category that names it (the groups' order; one the list does not have
/// comes after all).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GroupPath {
    pub category: String,
    #[serde(rename = "path")]
    pub names: Vec<String>,
    pub order: Vec<usize>,
}

/// A component of a library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestComponent {
    pub id: String,
    /// The design in the repository (`bolts/iso4762.mitcad`).
    pub path: String,
    /// Its category, or a list of them, the first its home (mitcad#527).
    #[serde(default, skip_serializing_if = "CategoryIds::is_empty")]
    pub category: CategoryIds,
    /// More categories it is listed in, after those of `category`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub categories: Vec<String>,
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
    /// PNG images of single configuration rows by the row's name
    /// (mitcad#484: a timber size seen from its end); a row without one
    /// shows `preview`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub row_previews: BTreeMap<String, String>,
    /// The component's own licence; the library's when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// How a bill of materials names a part (`ISO 4762 {config}`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub designation: String,
    /// Who sells the part (mitcad#375); shown where it is chosen and
    /// listed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub supplier: String,
    /// The supplier's article number; a configuration row's own
    /// `part_number` comes first.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub part_number: String,
    /// The supplier's product page (https).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub supplier_url: String,
    /// What the BUILD tab needs to place the part (`docs/libraries.md`,
    /// "BUILD parts"), kept as written so that newer fields survive; read
    /// with [`super::build::BuildPart`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<serde_json::Value>,
    /// The mount frames of a part that is no BUILD part (mitcad#525,
    /// `docs/libraries.md`, *Mount frames*), kept as written; read with
    /// [`super::mounts`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mounts: Option<serde_json::Value>,
    /// Its configuration rows' standards and suppliers' articles by the
    /// row's name (mitcad#949, `docs/libraries.md`, *Standards and
    /// suppliers of rows*).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rows: BTreeMap<String, LibraryRow>,
}

impl ManifestComponent {
    /// The ids of its categories, its home first, each once.
    pub fn category_ids(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::new();
        for id in self.category.0.iter().chain(&self.categories) {
            let id = id.trim();
            if !id.is_empty() && !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// Its home category's id (the first), empty for none.
    pub fn home_category(&self) -> &str {
        self.category_ids().first().copied().unwrap_or("")
    }

    /// Whether `id` is one of its categories.
    pub fn in_category(&self, id: &str) -> bool {
        self.category_ids().contains(&id.trim())
    }

    /// The standards its rows name (mitcad#949), each once: those its
    /// `standard` names in that order, then in the order the rows (`order`,
    /// the table's) first name them, `non-standard` last.
    pub fn row_standards(&self, order: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut odd = false;
        // The rows in the table's order, then any the order does not name.
        let ordered = order.iter().filter_map(|name| self.rows.get(name));
        for row in ordered.chain(self.rows.values()) {
            for standard in row.standards.iter().map(|s| s.trim()) {
                if standard.is_empty() {
                    continue;
                }
                if is_non_standard_tag(standard) {
                    odd = true;
                } else if !out.iter().any(|s| s.eq_ignore_ascii_case(standard)) {
                    out.push(standard.to_owned());
                }
            }
        }
        // The component's own order first (`ISO 4762 / DIN 912`: ISO
        // before DIN), then as the rows name them.
        let own = self.standard.to_ascii_lowercase();
        out.sort_by_key(|s| own.find(&s.to_ascii_lowercase()).unwrap_or(usize::MAX));
        if odd {
            out.push(NON_STANDARD.to_owned());
        }
        out
    }

    /// The ids of the suppliers with articles on its rows (mitcad#949),
    /// each once, in the order of the library's `suppliers`.
    pub fn row_suppliers<'a>(&self, suppliers: &'a [Supplier]) -> Vec<&'a Supplier> {
        suppliers
            .iter()
            .filter(|s| {
                self.rows
                    .values()
                    .any(|r| r.supplier_parts.iter().any(|p| p.supplier == s.id))
            })
            .collect()
    }
}

/// A supplier a library's rows refer to (mitcad#949, the manifest's
/// `suppliers`): a stable id, its name for people and its own site.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Supplier {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

/// Whether a text is a date `YYYY-MM-DD` (a supplier's article's
/// `checked`).
pub fn valid_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts.as_slice() else {
        return false;
    };
    let number = |s: &str, digits: usize| -> Option<u32> {
        if s.len() == digits && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse().ok()
        } else {
            None
        }
    };
    matches!(
        (number(year, 4), number(month, 2), number(day, 2)),
        (Some(_), Some(1..=12), Some(1..=31))
    )
}

/// The names of a category's own path (`Wood/Dimensioned timber`): its
/// parts between the slashes, trimmed, the empty ones left out.
fn path_names(text: &str) -> Vec<String> {
    text.split('/')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
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
    /// The library's materials, printers and process profiles of print
    /// settings (mitcad#640): one item file each, in `materials/`,
    /// `print/printers/` and `print/processes/` (`docs/libraries.md`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub materials: Vec<ManifestItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub printers: Vec<ManifestItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub processes: Vec<ManifestItem>,
    /// The suppliers the rows' articles name (mitcad#949).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppliers: Vec<Supplier>,
    /// The ids this library had before (mitcad#949: `mitcad-fasteners`,
    /// which became `mitcad-parts`): a library of such an id that an index
    /// offers, or a clone of it, is not listed beside this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
}

/// A material or print profile a manifest lists (mitcad#640): its id and
/// its item file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestItem {
    pub id: String,
    pub path: String,
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

    pub(crate) fn error(&mut self, text: impl Into<String>) {
        self.errors.push(text.into());
    }

    pub(crate) fn warn(&mut self, text: impl Into<String>) {
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

    /// The group of the category `id` (mitcad#527): its parents' names,
    /// then its own (its `name`, a path of names, else its id). A category
    /// the list does not have is a group of its id at the top; an unknown
    /// parent, or one that would close a cycle, ends the path there.
    pub fn group_path(&self, id: &str) -> GroupPath {
        let mut path = GroupPath {
            category: id.to_owned(),
            ..GroupPath::default()
        };
        let mut seen = Vec::new();
        self.add_group_names(id, &mut seen, &mut path);
        path
    }

    fn add_group_names<'a>(&'a self, id: &'a str, seen: &mut Vec<&'a str>, path: &mut GroupPath) {
        let Some((at, category)) = self.categories.iter().enumerate().find(|(_, c)| c.id == id)
        else {
            path.names.push(id.to_owned());
            path.order.push(self.categories.len());
            return;
        };
        seen.push(id);
        let parent = category.parent.trim();
        if !parent.is_empty()
            && !seen.contains(&parent)
            && self.categories.iter().any(|c| c.id == parent)
        {
            self.add_group_names(parent, seen, path);
        }
        let mut own = path_names(&category.name);
        if own.is_empty() {
            own.push(id.to_owned());
        }
        for name in own {
            path.names.push(name);
            path.order.push(at);
        }
    }

    /// The groups of a component: one per category, its home first.
    pub fn component_groups(&self, component: &ManifestComponent) -> Vec<GroupPath> {
        component
            .category_ids()
            .into_iter()
            .map(|id| self.group_path(id))
            .collect()
    }

    /// The categories' tree's problems (mitcad#527): a parent that is no
    /// category of the library, and categories that are each other's
    /// parents. Warnings: such a group is shown at the top, or where the
    /// cycle is cut.
    fn group_problems(&self, out: &mut Problems) {
        let by_id: BTreeMap<&str, &Category> =
            self.categories.iter().map(|c| (c.id.as_str(), c)).collect();
        let mut cycles: BTreeSet<Vec<&str>> = BTreeSet::new();
        for category in &self.categories {
            let parent = category.parent.trim();
            if parent.is_empty() {
                continue;
            }
            if !by_id.contains_key(parent) {
                out.warn(format!(
                    "the category {}: its parent {parent} is not one of the library's \
                     categories; the group is shown at the top",
                    category.id
                ));
                continue;
            }
            // Up its parents: back to itself is a cycle.
            let mut chain = vec![category.id.as_str()];
            let mut at = category;
            while let Some(&up) = by_id.get(at.parent.trim()) {
                if up.id == category.id {
                    let mut members = chain.clone();
                    members.sort_unstable();
                    if cycles.insert(members) {
                        out.warn(format!(
                            "the categories {} -> {} are each other's parents (a cycle); the \
                             groups are cut where the cycle closes",
                            chain.join(" -> "),
                            category.id
                        ));
                    }
                    break;
                }
                if chain.contains(&up.id.as_str()) {
                    // A cycle further up, not through this category.
                    break;
                }
                chain.push(up.id.as_str());
                at = up;
            }
        }
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
        self.group_problems(&mut out);
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
            if !valid_path(&c.path) || !crate::is_design_file(&c.path) {
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
            for category in c.category_ids() {
                if !categories.contains(category) {
                    out.warn(format!(
                        "{label}: the category {category} is not in the library's categories"
                    ));
                }
            }
            if !c.preview.is_empty() && (!valid_path(&c.preview) || !c.preview.ends_with(".png")) {
                out.error(format!(
                    "{label}: '{}' is no path of a PNG image in the library",
                    c.preview
                ));
            }
            for (row, path) in &c.row_previews {
                if !valid_path(path) || !path.ends_with(".png") {
                    out.error(format!(
                        "{label}: the preview of row {row}, '{path}', is no path of a PNG image in \
                         the library"
                    ));
                }
            }
            if let Some(license) = c.license.as_deref()
                && license_note(license).is_none()
            {
                out.warn(format!(
                    "{label}: the licence {license} is not one a community index accepts"
                ));
            }
            if !c.supplier_url.is_empty() && !c.supplier_url.starts_with("https://") {
                out.error(format!(
                    "{label}: the supplier_url '{}' is no https:// address",
                    c.supplier_url
                ));
            }
            if !c.part_number.trim().is_empty() && c.supplier.trim().is_empty() {
                out.warn(format!(
                    "{label}: a part number without a supplier says nothing about where to buy it"
                ));
            }
            super::build::check_component(self, c, &label, &mut out);
            // Mount frames of any part (mitcad#525).
            super::mounts::check_component(c, &label, &mut out);
            // The rows' standards and articles (mitcad#949).
            self.row_problems(c, &label, &mut out);
        }
        self.supplier_problems(&mut out);
        // Materials and print profiles (mitcad#640): files in their
        // folders, each id once per list.
        for (field, folder, items) in [
            ("materials", "materials/", &self.materials),
            ("printers", "print/printers/", &self.printers),
            ("processes", "print/processes/", &self.processes),
        ] {
            let mut ids = BTreeSet::new();
            for (i, item) in items.iter().enumerate() {
                if !valid_path(&item.path)
                    || !item.path.starts_with(folder)
                    || !item.path.ends_with(".json")
                {
                    out.error(format!(
                        "{field}[{i}]: '{}' is no path of a .json file in {folder}",
                        item.path
                    ));
                }
                if item.id.trim().is_empty() {
                    out.error(format!("{field}[{i}]: no id"));
                } else if !ids.insert(item.id.as_str()) {
                    out.error(format!("{field}[{i}]: the id {} is used twice", item.id));
                }
            }
        }
        out
    }

    /// The suppliers' problems (mitcad#949): each needs an id (as a
    /// component's) used once and a name; its site is an https address.
    fn supplier_problems(&self, out: &mut Problems) {
        let mut ids = BTreeSet::new();
        for (i, supplier) in self.suppliers.iter().enumerate() {
            if !valid_id(&supplier.id) {
                out.error(format!(
                    "suppliers[{i}]: '{}' is no supplier id (2 to 64 of a-z, 0-9 and -)",
                    supplier.id
                ));
            } else if !ids.insert(supplier.id.as_str()) {
                out.error(format!(
                    "suppliers[{i}]: the id {} is used twice",
                    supplier.id
                ));
            }
            if supplier.name.trim().is_empty() {
                out.error(format!("suppliers[{i}]: no name"));
            }
            if !supplier.url.is_empty() && !supplier.url.starts_with("https://") {
                out.error(format!(
                    "suppliers[{i}]: the url '{}' is no https:// address",
                    supplier.url
                ));
            }
        }
        for id in &self.replaces {
            if !valid_id(id) {
                out.error(format!("replaces: '{id}' is no library id"));
            } else if id == &self.id {
                out.error(format!("replaces: {id} is the library's own id"));
            }
        }
    }

    /// A component's rows' problems (mitcad#949): an article names one of
    /// the library's `suppliers`, has a part number and a `checked` date;
    /// a standard is no empty text.
    fn row_problems(&self, c: &ManifestComponent, label: &str, out: &mut Problems) {
        for (row, block) in &c.rows {
            if block.standards.iter().any(|s| s.trim().is_empty()) {
                out.error(format!("{label}: the row {row} names an empty standard"));
            }
            if block.standards.iter().any(|s| is_non_standard_tag(s)) && block.standards.len() > 1 {
                out.warn(format!(
                    "{label}: the row {row} is {NON_STANDARD} and names standards as well"
                ));
            }
            for (i, part) in block.supplier_parts.iter().enumerate() {
                let at = format!("{label}: the row {row}'s supplier_parts[{i}]");
                if !self.suppliers.iter().any(|s| s.id == part.supplier) {
                    out.error(format!(
                        "{at}: the supplier '{}' is not one of the library's suppliers",
                        part.supplier
                    ));
                }
                if part.part_number.trim().is_empty() {
                    out.error(format!("{at}: no part number"));
                }
                if !part.checked.is_empty() && !valid_date(&part.checked) {
                    out.error(format!(
                        "{at}: checked '{}' is no date (YYYY-MM-DD)",
                        part.checked
                    ));
                } else if part.checked.is_empty() {
                    out.warn(format!("{at}: no date it was checked"));
                }
            }
        }
    }

    /// The suppliers' articles of a component's row, each with its
    /// supplier's name (mitcad#949).
    pub fn named_supplier_parts(&self, c: &ManifestComponent, row: &str) -> Vec<SupplierPart> {
        c.rows
            .get(row)
            .map(|block| {
                block
                    .supplier_parts
                    .iter()
                    .map(|part| self.named_part(part))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// An article with its supplier's name from `suppliers`.
    pub fn named_part(&self, part: &SupplierPart) -> SupplierPart {
        let mut named = part.clone();
        if named.supplier_name.trim().is_empty()
            && let Some(supplier) = self.suppliers.iter().find(|s| s.id == part.supplier)
        {
            named.supplier_name = supplier.name.clone();
        }
        named
    }

    /// A component's rows' blocks with the suppliers' names (as
    /// [`mitcad_model::library::LibrarySource::rows`] has them).
    pub fn named_rows(&self, c: &ManifestComponent) -> BTreeMap<String, LibraryRow> {
        c.rows
            .iter()
            .map(|(name, block)| {
                let named = LibraryRow {
                    standards: block.standards.clone(),
                    supplier_parts: block
                        .supplier_parts
                        .iter()
                        .map(|part| self.named_part(part))
                        .collect(),
                };
                (name.clone(), named)
            })
            .collect()
    }

    /// The item files of materials and print profiles the manifest lists
    /// (mitcad#640), materials first.
    pub fn print_items(&self) -> impl Iterator<Item = &ManifestItem> {
        self.materials
            .iter()
            .chain(&self.printers)
            .chain(&self.processes)
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

fn is_non_standard_tag(standard: &str) -> bool {
    mitcad_model::library::is_non_standard(standard)
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
