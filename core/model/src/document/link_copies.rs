// SPDX-License-Identifier: MIT
//! Linked designs from outside the project (mitcad#1719).
//!
//! A link is followed only to a file in the design's project (mitcad#929;
//! for a design in no project, its folder). Before Insert Component links
//! a file, [`check_link`] says whether that link would be followed, and
//! [`copy_link_into_project`] copies a file that would not be into the
//! project's `parts/` folder, with the designs it links in turn that are
//! outside too, so that the copy is linked instead and followed when the
//! design opens again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::components::{followed_link, link_root, normalized, relative_path, resolve_path};
use super::{ModelError, invalid};

/// The folder of the project (or of the design's folder, without a
/// project) that copies of linked files go into.
pub const LINK_COPY_FOLDER: &str = "parts";

/// Whether a link to a file would be followed when the design opens
/// again ([`check_link`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCheck {
    /// The file, absolute and without `.` and `..`.
    pub file: PathBuf,
    /// The link would be followed: the file is in the design's project.
    pub followed: bool,
    /// The folder a copy would go into ([`copy_link_into_project`]); None
    /// for a design without a folder (never saved).
    pub folder: Option<PathBuf>,
}

/// What [`copy_link_into_project`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkCopy {
    /// The file to link: the copy of the file asked for (the file itself
    /// when it is in the project already).
    pub file: PathBuf,
    /// Every file copied, the one asked for first: (from, to).
    pub copied: Vec<(PathBuf, PathBuf)>,
}

/// Whether a link to `path` (absolute, or relative to `base`) from the
/// design whose folder is `base` would be followed when the design opens
/// again, and where a copy of the file would go.
pub fn check_link(path: &str, base: Option<&Path>) -> Result<LinkCheck, ModelError> {
    let file = absolute(path, base)?;
    let followed = followed_link(&file.to_string_lossy(), base).is_ok();
    let folder = base
        .and_then(link_root)
        .map(|root| root.join(LINK_COPY_FOLDER));
    Ok(LinkCheck {
        file,
        followed,
        folder,
    })
}

/// Copies the design file `path` (absolute, or relative to `base`) into
/// the `parts/` folder of the project of the design whose folder is
/// `base` (without a project: of that folder), so that a link to the copy
/// is followed. A name taken there gets a suffix (`plate_2.mitcad`). The
/// designs the file links that are outside the project are copied there
/// too, and so on; the copies' links name the copies, and their links to
/// files in the project name those files from the copy's folder. A copy
/// is a single file: B-rep data it refers to in its own project's store
/// goes inside. Nothing is written over. A file in the project is not
/// copied: it is linked as it is.
pub fn copy_link_into_project(path: &str, base: &Path) -> Result<LinkCopy, ModelError> {
    let root = link_root(base).ok_or_else(|| {
        invalid("the design's folder cannot be read, so there is no project to copy into")
    })?;
    let source = absolute(path, Some(base))?;
    let mut copies = Copies {
        folder: root.join(LINK_COPY_FOLDER),
        root,
        done: BTreeMap::new(),
        taken: BTreeSet::new(),
        copied: Vec::new(),
    };
    let file = copies.copy(&source)?;
    Ok(LinkCopy {
        file,
        copied: copies.copied,
    })
}

/// A path made absolute and without `.` and `..`.
fn absolute(path: &str, base: Option<&Path>) -> Result<PathBuf, ModelError> {
    std::path::absolute(resolve_path(path, base))
        .map(|p| normalized(&p))
        .map_err(|e| invalid(format!("cannot resolve {path}: {e}")))
}

struct Copies {
    /// The project (or the design's folder) that is followed.
    root: PathBuf,
    /// Where the copies go.
    folder: PathBuf,
    /// The copies made or being made, by their sources (a design linked
    /// twice is copied once, and a loop of links ends).
    done: BTreeMap<PathBuf, PathBuf>,
    /// The names given in this run, before their files are written.
    taken: BTreeSet<PathBuf>,
    copied: Vec<(PathBuf, PathBuf)>,
}

impl Copies {
    fn inside(&self, file: &Path) -> bool {
        file != self.root && file.starts_with(&self.root)
    }

    /// The copy of `source` (absolute, normalized), made with the copies of
    /// the designs it links that are outside too.
    fn copy(&mut self, source: &Path) -> Result<PathBuf, ModelError> {
        if self.inside(source) {
            return Ok(source.to_path_buf());
        }
        if let Some(done) = self.done.get(source) {
            return Ok(done.clone());
        }
        let label = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("design")
            .to_owned();
        let text = std::fs::read_to_string(source)
            .map_err(|e| invalid(format!("cannot read {label}: {e}")))?;
        std::fs::create_dir_all(&self.folder)
            .map_err(|e| invalid(format!("cannot make the folder for the copies: {e}")))?;
        let target = self.free_target(&label);
        self.done.insert(source.to_path_buf(), target.clone());
        self.taken.insert(target.clone());
        // Listed before the designs it links: the file asked for first.
        self.copied.push((source.to_path_buf(), target.clone()));
        // Its links, from the copy's folder.
        let from = source.parent().unwrap_or(Path::new(""));
        let mut links = BTreeMap::new();
        for link in crate::file::linked_paths(&text) {
            let linked = absolute(&link, Some(from))?;
            let named = if self.inside(&linked) {
                linked
            } else if linked.is_file() {
                self.copy(&linked)?
            } else {
                // A file that is not there stays as it was named: its
                // component keeps its bodies, as before.
                continue;
            };
            if let Some(new) = relative_path(&named, &normalized(&self.folder))
                && new != link
            {
                links.insert(link, new);
            }
        }
        let refers = crate::file::brep_references(&text).is_ok_and(|r| !r.is_empty());
        let written = if links.is_empty() && !refers {
            text
        } else {
            crate::file::relinked_text(&text, source, &links)
                .map_err(|e| invalid(format!("{label} cannot be copied: {e}")))?
        };
        std::fs::write(&target, written).map_err(|e| {
            invalid(format!(
                "cannot write the copy of {label} into the project: {e}"
            ))
        })?;
        Ok(target)
    }

    /// `name` in the copies' folder, else `stem_2.ext`, `stem_3.ext`, ...:
    /// the first that is neither a file there nor given in this run.
    fn free_target(&self, name: &str) -> PathBuf {
        let free = |candidate: &PathBuf| !candidate.exists() && !self.taken.contains(candidate);
        let first = self.folder.join(name);
        if free(&first) {
            return first;
        }
        let path = Path::new(name);
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("design");
        let extension = path.extension().and_then(|e| e.to_str());
        (2..)
            .map(|n| {
                self.folder.join(match extension {
                    Some(ext) => format!("{stem}_{n}.{ext}"),
                    None => format!("{stem}_{n}"),
                })
            })
            .find(free)
            .expect("a free name")
    }
}
