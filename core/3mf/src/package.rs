// SPDX-License-Identifier: MIT
//! A 3MF package's settings without its geometry (mitcad#640): the model
//! part's metadata, each object's own metadata (a slicer's per-object
//! settings) and the package's other files (a slicer's settings files),
//! for reading a slicer's print settings. Nothing is checked of the
//! geometry, and files of other models, images and G-code are left out.
//!
//! A package's entries are read whole into memory, so their declared sizes
//! are checked against a budget before anything is read (mitcad#927): a
//! few-MB package must not be able to make the reader allocate gigabytes.

use mitcad_zip::{Archive, DEFAULT_ENTRY_LIMIT, Entry};
use roxmltree::{Document, Node};

use crate::{Error, MODEL_RELATIONSHIP};

/// The largest file read into [`Package::files`] (larger ones are left
/// out), and the largest relationships part.
const MAX_FILE: u64 = 64 << 20;

/// The largest model part read. It is read only for its metadata, but a
/// slicer's project may keep the meshes in it (hundreds of MB for a large
/// mesh), so it takes the zip reader's default entry limit rather than
/// [`MAX_FILE`]; not the large limit a mesh import opts in to, since the
/// part is parsed whole for a few lines of metadata.
const MAX_MODEL_PART: u64 = DEFAULT_ENTRY_LIMIT;

/// The most bytes of all files read into [`Package::files`] together.
const MAX_FILES_TOTAL: u64 = 256 << 20;

/// The most entries a package may have: a slicer's project holds a few
/// files per plate and object, far fewer.
const MAX_ENTRIES: usize = 4096;

/// The usual model part, when the relationships name none.
const MODEL_PART: &str = "3D/3dmodel.model";

/// What a package holds besides the geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Package {
    /// The model part's metadata (name, text), in order.
    pub metadata: Vec<(String, String)>,
    /// The model part's objects with their own metadata.
    pub objects: Vec<PackageObject>,
    /// The package's other files by path, in the archive's order.
    pub files: Vec<(String, Vec<u8>)>,
}

/// An object of the model part.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PackageObject {
    pub id: u32,
    pub name: String,
    /// Its `metadatagroup`'s metadata (name, text).
    pub metadata: Vec<(String, String)>,
}

impl Package {
    /// A file of the package by path (ignoring case).
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(path))
            .map(|(_, data)| data.as_slice())
    }

    /// A metadata text of the model part.
    pub fn metadata(&self, name: &str) -> Option<&str> {
        self.metadata
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

fn element(node: &Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

/// Refuses, before it is read, an entry that declares more than `limit`.
fn check_size(entry: &Entry, limit: u64) -> Result<(), Error> {
    if entry.size > limit {
        return Err(Error(format!(
            "{} declares {} bytes, more than the package's limit of {limit}",
            entry.name, entry.size
        )));
    }
    Ok(())
}

/// Reads a package's metadata, objects' metadata and other files.
pub fn read_package(data: &[u8]) -> Result<Package, Error> {
    let archive =
        Archive::new(data.to_vec()).map_err(|e| Error(format!("not a zip package: {e}")))?;
    if archive.entries().len() > MAX_ENTRIES {
        return Err(Error(format!(
            "the package has {} entries, more than the limit of {MAX_ENTRIES}",
            archive.entries().len()
        )));
    }
    let relationships = match archive.find("_rels/.rels") {
        Some(entry) => {
            check_size(entry, MAX_FILE)?;
            archive.read_with_limit(entry, MAX_FILE).ok()
        }
        None => None,
    };
    let model_part = relationships
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| {
            let document = Document::parse(&text).ok()?;
            document
                .root_element()
                .children()
                .filter(|n| element(n, "Relationship"))
                .find(|n| n.attribute("Type") == Some(MODEL_RELATIONSHIP))
                .and_then(|n| n.attribute("Target"))
                .map(|t| t.trim_start_matches('/').to_owned())
        })
        .unwrap_or_else(|| MODEL_PART.to_owned());
    // The other files: all but the model part, other models, images and
    // G-code, each up to MAX_FILE; their sum is checked before any is read.
    let files: Vec<&Entry> = archive
        .entries()
        .iter()
        .filter(|entry| {
            let lower = entry.name.to_ascii_lowercase();
            let geometry_or_image = [".model", ".png", ".jpg", ".jpeg", ".gcode", ".stl"]
                .iter()
                .any(|e| lower.ends_with(e));
            let left_out = entry.is_dir() || entry.name == model_part || entry.size > MAX_FILE;
            !(left_out || geometry_or_image)
        })
        .collect();
    let total: u64 = files.iter().map(|entry| entry.size).sum();
    if total > MAX_FILES_TOTAL {
        return Err(Error(format!(
            "the package's files declare {total} bytes, more than the limit of \
             {MAX_FILES_TOTAL} in all"
        )));
    }
    let mut package = Package::default();
    let model = match archive.find(&model_part) {
        Some(entry) => {
            check_size(entry, MAX_MODEL_PART)?;
            archive.read_with_limit(entry, MAX_MODEL_PART).ok()
        }
        None => None,
    };
    if let Some(bytes) = model {
        let text =
            String::from_utf8(bytes).map_err(|_| Error(format!("{model_part} is not UTF-8")))?;
        let document = Document::parse(&text).map_err(|e| Error(format!("{model_part}: {e}")))?;
        for child in document.root_element().children().filter(Node::is_element) {
            if element(&child, "metadata") {
                if let Some(name) = child.attribute("name") {
                    package.metadata.push((
                        name.to_owned(),
                        child.text().unwrap_or("").trim().to_owned(),
                    ));
                }
            } else if element(&child, "resources") {
                for object in child.children().filter(|n| element(n, "object")) {
                    let metadata = object
                        .children()
                        .filter(|n| element(n, "metadatagroup"))
                        .flat_map(|group| group.children())
                        .filter(|n| element(n, "metadata"))
                        .filter_map(|m| {
                            Some((
                                m.attribute("name")?.to_owned(),
                                m.text().unwrap_or("").trim().to_owned(),
                            ))
                        })
                        .collect();
                    package.objects.push(PackageObject {
                        id: object
                            .attribute("id")
                            .and_then(|i| i.trim().parse().ok())
                            .unwrap_or(0),
                        name: object.attribute("name").unwrap_or("").to_owned(),
                        metadata,
                    });
                }
            }
        }
    }
    for entry in files {
        let bytes = archive
            .read_with_limit(entry, MAX_FILE)
            .map_err(|e| Error(format!("{}: {e}", entry.name)))?;
        package.files.push((entry.name.clone(), bytes));
    }
    Ok(package)
}
