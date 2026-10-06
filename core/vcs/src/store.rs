// SPDX-License-Identifier: MIT
//! The B-rep store of an older version, read from its commit's tree, so
//! that the version opens without a checkout.

use std::io;

use gix::ObjectId;
use mitcad_model::file::BREP_DIR;
use mitcad_model::{BlobStore, Sha256};

use crate::VcsError;

/// The `.mitcad/brep` files of a version, as a [`BlobStore`] for
/// `Document::from_json_in`. It cannot be written.
pub struct GitTreeStore<'repo> {
    repo: &'repo gix::Repository,
    /// The version's `.mitcad/brep` tree; None when it has none.
    store: Option<ObjectId>,
}

impl<'repo> GitTreeStore<'repo> {
    pub(crate) fn new(repo: &'repo gix::Repository, commit: ObjectId) -> Result<Self, VcsError> {
        let tree = repo.find_commit(commit)?.tree()?;
        let store = tree
            .lookup_entry(BREP_DIR.split('/'))?
            .filter(|entry| entry.mode().is_tree())
            .map(|entry| entry.object_id());
        Ok(Self { repo, store })
    }

    /// The blob of the file of `sha256`.
    fn find(&self, sha256: &Sha256) -> io::Result<Option<ObjectId>> {
        let Some(store) = self.store else {
            return Ok(None);
        };
        let name = sha256.to_string();
        let file = format!("{name}.brep.zlib");
        let tree = self.repo.find_tree(store).map_err(io::Error::other)?;
        let entry = tree
            .lookup_entry([&name[..2], file.as_str()])
            .map_err(io::Error::other)?;
        Ok(entry
            .filter(|entry| !entry.mode().is_tree())
            .map(|entry| entry.object_id()))
    }
}

impl BlobStore for GitTreeStore<'_> {
    fn get(&self, sha256: &Sha256) -> io::Result<Option<Vec<u8>>> {
        match self.find(sha256)? {
            Some(id) => Ok(Some(
                self.repo
                    .find_blob(id)
                    .map_err(io::Error::other)?
                    .take_data(),
            )),
            None => Ok(None),
        }
    }

    fn contains(&self, sha256: &Sha256) -> io::Result<bool> {
        Ok(self.find(sha256)?.is_some())
    }

    fn put(&self, _sha256: &Sha256, _content: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "the B-rep store of an older version cannot be written",
        ))
    }
}
