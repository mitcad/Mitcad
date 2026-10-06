// SPDX-License-Identifier: MIT
//! Reading trees: entries by path, the project files and B-rep files of a
//! version, and the paths that differ between two trees.

use std::collections::BTreeMap;

use gix::ObjectId;
use gix::bstr::{BString, ByteSlice};
use gix::objs::tree::EntryMode;
use mitcad_model::Sha256;
use mitcad_model::file::BREP_DIR;

use crate::{ProjectRepo, VcsError, is_project_file};

/// A path that differs between two trees, with its blobs before and after
/// (None where it is not a file) and the mode of the one after.
pub(crate) struct Difference {
    pub path: String,
    pub old: Option<ObjectId>,
    pub new: Option<ObjectId>,
    pub mode: Option<EntryMode>,
}

/// The SHA-256 a path of the B-rep store is named by
/// (`.mitcad/brep/<aa>/<sha256>.brep.zlib`), or None for another path.
pub(crate) fn brep_sha256(path: &str) -> Option<Sha256> {
    let rest = path.strip_prefix(BREP_DIR)?.strip_prefix('/')?;
    let (folder, name) = rest.split_once('/')?;
    let sha256: Sha256 = name.strip_suffix(".brep.zlib")?.parse().ok()?;
    (folder.len() == 2 && sha256.to_string().starts_with(folder)).then_some(sha256)
}

impl ProjectRepo {
    /// The entries of a tree by name; none for the empty tree.
    fn entries(
        &self,
        tree: ObjectId,
    ) -> Result<BTreeMap<BString, (EntryMode, ObjectId)>, VcsError> {
        let mut out = BTreeMap::new();
        if tree.is_empty_tree() {
            return Ok(out);
        }
        let tree = self.repo.find_tree(tree)?;
        for entry in tree.decode()?.entries {
            out.insert(
                entry.filename.to_owned(),
                (entry.mode, entry.oid.to_owned()),
            );
        }
        Ok(out)
    }

    /// The object at `path` (`a/b/c`) in a tree, if there is one.
    pub(crate) fn tree_entry(
        &self,
        tree: ObjectId,
        path: &str,
    ) -> Result<Option<ObjectId>, VcsError> {
        let mut current = tree;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let entries = self.entries(current)?;
            match entries.get(part.as_bytes().as_bstr()) {
                Some((mode, id)) if parts.peek().is_none() || mode.is_tree() => current = *id,
                _ => return Ok(None),
            }
        }
        Ok(Some(current))
    }

    /// The file at `path` in a commit's tree: its blob, if there is one.
    pub(crate) fn entry_at(
        &self,
        commit: ObjectId,
        path: &str,
    ) -> Result<Option<ObjectId>, VcsError> {
        let tree = self.repo.find_commit(commit)?.tree_id()?.detach();
        self.blob_entry(tree, path)
    }

    /// The blob at `path` in a tree (None for a folder or nothing).
    pub(crate) fn blob_entry(
        &self,
        tree: ObjectId,
        path: &str,
    ) -> Result<Option<ObjectId>, VcsError> {
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        let folder = if folder.is_empty() {
            Some(tree)
        } else {
            self.tree_entry(tree, folder)?
        };
        let Some(folder) = folder else {
            return Ok(None);
        };
        Ok(match self.entries(folder)?.get(name.as_bytes().as_bstr()) {
            Some((mode, id)) if !mode.is_tree() => Some(*id),
            _ => None,
        })
    }

    /// The project files (`*.mitcad`) of a tree with their blobs, leaving
    /// out the `.mitcad` folder.
    pub(crate) fn project_files(
        &self,
        tree: ObjectId,
    ) -> Result<Vec<(String, ObjectId)>, VcsError> {
        let mut out = Vec::new();
        let mut folders = vec![(String::new(), tree)];
        while let Some((prefix, folder)) = folders.pop() {
            for (name, (mode, id)) in self.entries(folder)? {
                let path = format!("{prefix}{}", name.to_str_lossy());
                if mode.is_tree() {
                    if path != ".mitcad" && path != ".git" {
                        folders.push((format!("{path}/"), id));
                    }
                } else if mode.is_blob() && is_project_file(&path) {
                    out.push((path, id));
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// The files of the B-rep store (`.mitcad/brep/<aa>/…`) in a tree.
    pub(crate) fn brep_files(&self, tree: ObjectId) -> Result<Vec<String>, VcsError> {
        let mut out = Vec::new();
        let Some(store) = self.tree_entry(tree, BREP_DIR)? else {
            return Ok(out);
        };
        for (folder, (mode, id)) in self.entries(store)? {
            if !mode.is_tree() {
                continue;
            }
            for (name, (mode, _)) in self.entries(id)? {
                if mode.is_blob() {
                    out.push(format!("{BREP_DIR}/{folder}/{name}"));
                }
            }
        }
        Ok(out)
    }

    /// The files that differ between two trees (None: the empty tree),
    /// sorted by path; a folder that became a file or the other way round
    /// gives the files on both sides.
    pub(crate) fn differences(
        &self,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
    ) -> Result<Vec<Difference>, VcsError> {
        let mut out = Vec::new();
        self.differences_in(old, new, "", &mut out)?;
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn differences_in(
        &self,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        prefix: &str,
        out: &mut Vec<Difference>,
    ) -> Result<(), VcsError> {
        let empty = BTreeMap::new();
        let old_entries = match old {
            Some(id) => self.entries(id)?,
            None => empty.clone(),
        };
        let new_entries = match new {
            Some(id) => self.entries(id)?,
            None => empty,
        };
        let names: std::collections::BTreeSet<&BString> =
            old_entries.keys().chain(new_entries.keys()).collect();
        for name in names {
            let path = format!("{prefix}{}", name.to_str_lossy());
            let before = old_entries.get(name);
            let after = new_entries.get(name);
            if before == after {
                continue;
            }
            let tree = |entry: Option<&(EntryMode, ObjectId)>| {
                entry.filter(|(mode, _)| mode.is_tree()).map(|(_, id)| *id)
            };
            let file = |entry: Option<&(EntryMode, ObjectId)>| {
                entry.filter(|(mode, _)| !mode.is_tree()).map(|(_, id)| *id)
            };
            let (old_tree, new_tree) = (tree(before), tree(after));
            if old_tree.is_some() || new_tree.is_some() {
                self.differences_in(old_tree, new_tree, &format!("{path}/"), out)?;
            }
            let (old_file, new_file) = (file(before), file(after));
            if old_file.is_some() || new_file.is_some() {
                out.push(Difference {
                    path,
                    old: old_file,
                    new: new_file,
                    mode: after
                        .filter(|(mode, _)| !mode.is_tree())
                        .map(|(mode, _)| *mode),
                });
            }
        }
        Ok(())
    }
}
