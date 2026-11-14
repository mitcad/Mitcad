// SPDX-License-Identifier: MIT
//! The design packed into a zip for a report (mitcad#1795, GitHub
//! mitcad/Mitcad#328): Help › Send Feedback's Include the design.
//!
//! [`Document::write_design_package`] writes one zip that opens as a
//! project again: the open design as it is now (a single file, its B-rep
//! data inside), the designs it links and the designs they link, the image
//! files of its appearances' textures and the B-rep files its linked
//! designs refer to in the project's store, each at its path in the project
//! (or, for a design in no project, in the design's folder), the project's
//! marker `.mitcad/project.json`, and the files the caller adds (the
//! report's Markdown, the screenshot, the log) in a folder of their own.
//!
//! Only what the design refers to goes in, and only from inside its
//! project: a link the design does not follow (mitcad#929) is not
//! followed here either, a texture file outside the project is left out,
//! and so is a file whose real path leaves the project (a symbolic link),
//! so that a design file never chooses which other files of the computer
//! a report carries. The project's per-user state (`.mitcad/local/`) and
//! its caches never go in.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::components::{followed_link, link_root, normalized, relative_path, resolve_path};
use super::{Document, ModelError, invalid};
use crate::file::{BlobStore, PROJECT_MARKER, Project, brep_path};
use crate::kernel::Kernel;

/// The name of the open design in the zip when it was never saved.
pub const UNSAVED_DESIGN_NAME: &str = "design.mitcad";

/// The image files a texture is read from that a package takes, by their
/// extensions (lower case).
const IMAGE_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The images of a texture besides its base colour (`appearance::MAPS`).
const TEXTURE_MAPS: [&str; 4] = ["normal", "roughness", "metalness", "occlusion"];

/// A file the caller adds to a package: its name in the zip (relative,
/// `/` between folders) and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageExtra {
    pub name: String,
    pub data: Vec<u8>,
}

/// What [`Document::write_design_package`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignPackage {
    /// The zip, absolute.
    pub zip: PathBuf,
    /// Its size in bytes.
    pub size: u64,
    /// The open design's name in the zip.
    pub design: String,
    /// Every file in the zip, in the order written: the open design first,
    /// then the files it refers to, the project's marker, the caller's.
    pub files: Vec<String>,
    /// Files the design refers to that are not there (a linked design, a
    /// texture or B-rep data that is missing or cannot be read), by their
    /// names in the project.
    pub missing: Vec<String>,
    /// Files the design refers to that a package does not take: outside the
    /// project, or not an image, by their file names only.
    pub left_out: Vec<String>,
}

/// The files of a package as they are gathered.
struct Packer {
    /// The project's folder (or the design's, in no project), absolute and
    /// normalized; None for a design never saved.
    root: Option<PathBuf>,
    /// The root's real path, to tell a file whose real path leaves it.
    real_root: Option<PathBuf>,
    entries: Vec<(String, Vec<u8>)>,
    names: BTreeSet<String>,
    /// The design files read, so that a design linked twice, or a loop of
    /// links, is read once.
    read: BTreeSet<PathBuf>,
    missing: Vec<String>,
    left_out: Vec<String>,
}

impl Packer {
    fn add(&mut self, name: String, data: Vec<u8>) {
        if self.names.insert(name.clone()) {
            self.entries.push((name, data));
        }
    }

    /// `file`'s name in the zip: its path in the root, when it is inside.
    fn name_of(&self, file: &Path) -> Option<String> {
        let root = self.root.as_ref()?;
        if file == root || !file.starts_with(root) {
            return None;
        }
        relative_path(file, root)
    }

    /// Whether the real path of `file` (an existing file) is in the root's.
    fn really_inside(&self, file: &Path) -> bool {
        let Some(real_root) = &self.real_root else {
            return false;
        };
        std::fs::canonicalize(file)
            .is_ok_and(|real| real != *real_root && real.starts_with(real_root))
    }

    fn label(file: &Path) -> String {
        file.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_owned()
    }

    /// The files the design text `text`, whose file is `file`, refers to:
    /// its linked designs (and theirs, read from their files), its
    /// textures' images and the B-rep data in its project's store.
    fn refers(&mut self, text: &str, file: Option<&Path>) {
        let folder = file.and_then(Path::parent).map(Path::to_path_buf);
        self.textures(text, folder.as_deref());
        if let Some(file) = file {
            self.brep_data(text, file);
        }
        for link in crate::file::linked_paths(text) {
            // A link the design does not follow brings nothing in: its
            // component keeps the bodies saved with the design.
            let Ok(target) = followed_link(&link, folder.as_deref()) else {
                continue;
            };
            if !self.read.insert(target.clone()) {
                continue;
            }
            let Some(name) = self.name_of(&target) else {
                continue;
            };
            if !target.is_file() {
                self.missing.push(name);
                continue;
            }
            if !self.really_inside(&target) {
                self.left_out.push(Self::label(&target));
                continue;
            }
            match std::fs::read_to_string(&target) {
                Ok(linked) => {
                    // Listed before the files it refers to.
                    self.add(name, linked.clone().into_bytes());
                    self.refers(&linked, Some(&target));
                }
                Err(_) => self.missing.push(name),
            }
        }
    }

    /// The B-rep data a design file refers to in its project's store
    /// (version 3), at its path in the project.
    fn brep_data(&mut self, text: &str, file: &Path) {
        let Ok(references) = crate::file::brep_references(text) else {
            return;
        };
        if references.is_empty() {
            return;
        }
        let Some(project) = Project::find(file) else {
            return;
        };
        let store = project.store();
        let project_root = normalized(project.root());
        for sha256 in references {
            let Some(name) = self.name_of(&project_root.join(brep_path(&sha256))) else {
                continue;
            };
            if self.names.contains(&name) {
                continue;
            }
            // The store refuses symbolic links in it.
            match store.get(&sha256) {
                Ok(Some(data)) => self.add(name, data),
                _ => self.missing.push(name),
            }
        }
    }

    /// The image files of the appearances' textures that are not embedded
    /// in the design, read from the design's folder.
    fn textures(&mut self, text: &str, folder: Option<&Path>) {
        let Ok(value) =
            serde_json::from_str::<Value>(text.strip_prefix('\u{feff}').unwrap_or(text))
        else {
            return;
        };
        let appearances = value.get("appearances").and_then(Value::as_array);
        let mut paths = Vec::new();
        for texture in appearances
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("texture"))
            .filter(|t| t.is_object())
        {
            let slots =
                std::iter::once(texture).chain(TEXTURE_MAPS.iter().filter_map(|m| texture.get(*m)));
            for slot in slots {
                let embedded = ["data", "image"]
                    .iter()
                    .any(|key| slot.get(*key).is_some_and(|v| !v.is_null()));
                match slot.get("path").and_then(Value::as_str) {
                    Some(path) if !path.is_empty() && !embedded => paths.push(path.to_owned()),
                    _ => {}
                }
            }
        }
        for path in paths {
            let Ok(file) = std::path::absolute(resolve_path(&path, folder)) else {
                continue;
            };
            let file = normalized(&file);
            let image = file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
            let Some(name) = self.name_of(&file) else {
                self.left_out.push(Self::label(&file));
                continue;
            };
            if self.names.contains(&name) {
                continue;
            }
            if !file.is_file() {
                self.missing.push(name);
                continue;
            }
            if !image || !self.really_inside(&file) {
                self.left_out.push(Self::label(&file));
                continue;
            }
            match std::fs::read(&file) {
                Ok(data) => self.add(name, data),
                Err(_) => self.missing.push(name),
            }
        }
    }
}

/// Whether `name` is a relative name in a zip: names separated by `/`,
/// none empty, `.` or `..`, no `\` or `:`.
fn valid_entry_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':'])
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Already compressed data is stored as it is; the rest is deflated.
fn method_for(name: &str) -> mitcad_zip::Method {
    let lower = name.to_ascii_lowercase();
    if [".zlib", ".png", ".jpg", ".jpeg", ".zip"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        mitcad_zip::Method::Stored
    } else {
        mitcad_zip::Method::Deflate
    }
}

impl<K: Kernel> Document<K> {
    /// Writes the design and the files it refers to into the zip `zip`
    /// (made or written over, whole or not at all), with `extra` added
    /// (see the module's documentation). `file` is the open design's file
    /// (absolute), None for a design never saved: it is then
    /// [`UNSAVED_DESIGN_NAME`] in the zip and follows no link. The open
    /// design goes in as it is now, saved or not, as a single file.
    pub fn write_design_package(
        &self,
        file: Option<&Path>,
        zip: &Path,
        extra: &[PackageExtra],
    ) -> Result<DesignPackage, ModelError> {
        let file = match file {
            Some(file) => {
                Some(normalized(&std::path::absolute(file).map_err(|e| {
                    invalid(format!("cannot resolve the design's file: {e}"))
                })?))
            }
            None => None,
        };
        let root = file.as_deref().and_then(Path::parent).and_then(link_root);
        let real_root = root.as_deref().and_then(|r| std::fs::canonicalize(r).ok());
        let mut packer = Packer {
            root,
            real_root,
            entries: Vec::new(),
            names: BTreeSet::new(),
            read: BTreeSet::new(),
            missing: Vec::new(),
            left_out: Vec::new(),
        };
        let design = file
            .as_deref()
            .and_then(|f| {
                packer
                    .name_of(f)
                    .or_else(|| f.file_name()?.to_str().map(str::to_owned))
            })
            .unwrap_or_else(|| UNSAVED_DESIGN_NAME.to_owned());
        // The open design first, so that a design that links itself is not
        // read again from its file.
        let text = self.to_json();
        packer.names.insert(design.clone());
        packer.entries.push((design.clone(), Vec::new()));
        if let Some(file) = &file {
            packer.read.insert(file.clone());
        }
        packer.refers(&text, file.as_deref());
        packer.entries[0].1 = text.into_bytes();
        // The project's marker, so that the zip opens as a project.
        if let Some(project) = file.as_deref().and_then(Project::find) {
            let marker = normalized(project.root()).join(PROJECT_MARKER);
            if let Some(name) = packer.name_of(&marker)
                && let Ok(data) = std::fs::read(&marker)
            {
                packer.add(name, data);
            }
        }
        for added in extra {
            if !valid_entry_name(&added.name) {
                return Err(invalid(format!(
                    "{} is not a file name in a zip: names separated by /, without . or ..",
                    added.name
                )));
            }
            if packer.names.contains(&added.name) {
                return Err(invalid(format!(
                    "{} is in the package already: a file the design refers to has that name",
                    added.name
                )));
            }
            packer.add(added.name.clone(), added.data.clone());
        }
        let mut writer = mitcad_zip::Writer::new();
        for (name, data) in &packer.entries {
            writer
                .add(name, data, method_for(name))
                .map_err(|e| invalid(format!("cannot pack the design: {e}")))?;
        }
        let bytes = writer
            .finish()
            .map_err(|e| invalid(format!("cannot pack the design: {e}")))?;
        let zip = std::path::absolute(zip)
            .map_err(|e| invalid(format!("cannot resolve the zip's path: {e}")))?;
        if let Some(folder) = zip.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|e| invalid(format!("cannot make the zip's folder: {e}")))?;
        }
        crate::file::write_atomically(&zip, &bytes)
            .map_err(|e| invalid(format!("cannot write the zip: {e}")))?;
        Ok(DesignPackage {
            zip,
            size: bytes.len() as u64,
            design,
            files: packer.entries.into_iter().map(|(name, _)| name).collect(),
            missing: packer.missing,
            left_out: packer.left_out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_are_relative_without_dots() {
        assert!(valid_entry_name("mitcad-report/report.md"));
        assert!(valid_entry_name("a.b/c"));
        for bad in ["", "/a", "a/", "a//b", "../a", "a/./b", "a\\b", "C:/a"] {
            assert!(!valid_entry_name(bad), "{bad}");
        }
    }

    #[test]
    fn compressed_data_is_stored() {
        assert_eq!(
            method_for(".mitcad/brep/ab/x.brep.zlib"),
            mitcad_zip::Method::Stored
        );
        assert_eq!(method_for("shot.PNG"), mitcad_zip::Method::Stored);
        assert_eq!(method_for("designs/a.mitcad"), mitcad_zip::Method::Deflate);
        assert_eq!(method_for("report.md"), mitcad_zip::Method::Deflate);
    }
}
