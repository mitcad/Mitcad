// SPDX-License-Identifier: MIT
//! Recording a version: a commit of the saved paths built straight from
//! HEAD's tree, with the project's B-rep store kept to what the version's
//! design files refer to, and the index entries of those paths updated.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gix::ObjectId;
use gix::bstr::{BStr, ByteSlice};
use gix::objs::tree::EntryKind;
use mitcad_model::Sha256;
use mitcad_model::file::brep_path;
use serde::Serialize;

use crate::tree::brep_sha256;
use crate::{Identity, MITCAD_VERSION, ProjectRepo, VcsError, chain, io_error, is_design_file};

/// How often a commit is tried: another commit may move the branch
/// between building the tree and moving the branch, or hold its lock.
const ATTEMPTS: u32 = 5;
/// How long to wait for another program's lock of the index.
const INDEX_LOCK_WAIT: Duration = Duration::from_secs(10);
/// The design file version a project writes (a commit trailer).
const DESIGN_FORMAT: u32 = 3;

/// What recording a version did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CommitOutcome {
    /// The new version's commit id; None when nothing changed or the
    /// commit was skipped.
    pub commit: Option<String>,
    /// Why no version was recorded (HEAD detached, a merge in progress).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Paths the version adds or changes, relative to the project.
    pub written: Vec<String>,
    /// Paths the version no longer has: deleted files, and B-rep files no
    /// design file of the version refers to.
    pub removed: Vec<String>,
    /// Those B-rep files, deleted from the project's folder too.
    pub deleted: Vec<String>,
    pub warnings: Vec<String>,
}

/// A version's tree, built from HEAD's.
struct Prepared {
    /// HEAD's tree (the empty tree before the first version).
    base: ObjectId,
    tree: ObjectId,
    /// The paths whose index entries change, with their blobs in the new
    /// tree (None: removed).
    entries: Vec<(String, Option<ObjectId>)>,
    written: Vec<String>,
    removed: Vec<String>,
    /// B-rep files the version drops, to delete from the folder.
    unreferenced: Vec<String>,
    /// Paths whose content came from memory, not from the folder's file.
    from_memory: Vec<String>,
    warnings: Vec<String>,
}

impl ProjectRepo {
    /// Records the saved `files` (absolute, or relative to the current
    /// folder; a file that no longer exists is removed) as a new version
    /// with `message` (empty: `Save <paths>`) by `author`, on the branch
    /// HEAD is on. Only these paths change, with the B-rep files of the
    /// project's store: those the version's design files refer to are
    /// added where missing, and those none refers to are removed from the
    /// version and from the folder. The index entries of the changed paths
    /// follow; other staged changes stay. Nothing changed: no commit.
    pub fn commit(
        &self,
        files: &[PathBuf],
        message: &str,
        author: &Identity,
    ) -> Result<CommitOutcome, VcsError> {
        self.commit_with_renames(files, message, author, &[])
    }

    /// Records known moves as well as content changes. A moved design may
    /// have new bytes because its relative links were updated for its new
    /// location, so content alone cannot always identify its old path.
    pub(crate) fn commit_with_renames(
        &self,
        files: &[PathBuf],
        message: &str,
        author: &Identity,
        renames: &[(String, String)],
    ) -> Result<CommitOutcome, VcsError> {
        self.record_commit(files, &BTreeMap::new(), message, author, renames)
    }

    /// Records `files` as [`ProjectRepo::commit`] does, and the paths of
    /// `contents` (relative to the project, with `/`) with that content
    /// instead of the folder's file, which stays as it is (Remove
    /// Project's single-file form of a recorded design, mitcad#1607). The
    /// index entries of those paths take no file times, so git compares
    /// the folder's file with the version by content.
    pub(crate) fn commit_with_contents(
        &self,
        files: &[PathBuf],
        contents: &BTreeMap<String, Vec<u8>>,
        message: &str,
        author: &Identity,
    ) -> Result<CommitOutcome, VcsError> {
        self.record_commit(files, contents, message, author, &[])
    }

    fn record_commit(
        &self,
        files: &[PathBuf],
        contents: &BTreeMap<String, Vec<u8>>,
        message: &str,
        author: &Identity,
        renames: &[(String, String)],
    ) -> Result<CommitOutcome, VcsError> {
        let mut paths: Vec<String> = Vec::new();
        for file in files {
            let path = self.relative(file)?;
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        for path in contents.keys() {
            if !paths.contains(path) {
                paths.push(path.clone());
            }
        }
        if let Some(reason) = self.why_not_commit()? {
            return Ok(CommitOutcome {
                skipped: Some(reason),
                ..CommitOutcome::default()
            });
        }
        // The index stays locked for the whole round, so that git (or
        // another Mitcad) does not stage something in between that the
        // index written at the end would lose.
        let index_path = self.repo.index_path();
        let lock = gix::lock::File::acquire_to_update_resource(
            &index_path,
            gix::lock::acquire::Fail::AfterDurationWithBackoff(INDEX_LOCK_WAIT),
            None,
        )
        .map_err(|e| {
            VcsError::Git(format!(
                "cannot lock the index {} (another git program may be running): {e}",
                index_path.display()
            ))
        })?;
        let message = full_message(message, &paths, renames);
        let mut last_error = String::new();
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(20 << attempt));
            }
            let head = self.head_commit()?;
            let prepared = self.prepare(head, &paths, contents)?;
            if prepared.tree == prepared.base {
                return Ok(CommitOutcome {
                    warnings: prepared.warnings,
                    ..CommitOutcome::default()
                });
            }
            #[cfg(test)]
            {
                let race = self.race.borrow_mut().take();
                if let Some(race) = race {
                    race();
                }
            }
            let signature = gix::actor::Signature {
                name: author.name.as_str().into(),
                email: author.email.as_str().into(),
                time: gix::date::Time::now_local_or_utc(),
            };
            let (mut author_time, mut committer_time) = Default::default();
            let committed = self.repo.commit_as(
                signature.to_ref(&mut committer_time),
                signature.to_ref(&mut author_time),
                "HEAD",
                &message,
                prepared.tree,
                head,
            );
            match committed {
                Ok(id) => return Ok(self.finish(id.detach(), prepared, lock, renames)),
                // The branch moved (or its lock was taken): build again.
                Err(error) => last_error = chain(&error),
            }
        }
        Err(VcsError::Git(format!(
            "the version could not be recorded ({ATTEMPTS} attempts): {last_error}"
        )))
    }

    /// Why no version can be recorded now, if so.
    pub(crate) fn why_not_commit(&self) -> Result<Option<String>, VcsError> {
        if self.repo.head()?.is_detached() {
            return Ok(Some(
                "HEAD is detached (not on a branch), so the version is not recorded".to_owned(),
            ));
        }
        Ok(self.operation_in_progress().map(|operation| {
            format!(
                "a {operation} is in progress in the repository, so the version is not recorded"
            )
        }))
    }

    /// The git operation in progress in the repository (`merge`, `rebase`,
    /// …), if any.
    pub(crate) fn operation_in_progress(&self) -> Option<&'static str> {
        use gix::state::InProgress as P;
        Some(match self.repo.state()? {
            P::Merge => "merge",
            P::Rebase | P::RebaseInteractive | P::ApplyMailboxRebase => "rebase",
            P::CherryPick | P::CherryPickSequence => "cherry-pick",
            P::Revert | P::RevertSequence => "revert",
            P::Bisect => "bisect",
            P::ApplyMailbox => "git am",
        })
    }

    /// HEAD's tree with `paths` as they are in the folder (or in
    /// `contents`), and the B-rep files reconciled.
    fn prepare(
        &self,
        head: Option<ObjectId>,
        paths: &[String],
        contents: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Prepared, VcsError> {
        let base = match head {
            Some(id) => self.repo.find_commit(id)?.tree_id()?.detach(),
            None => ObjectId::empty_tree(self.repo.object_hash()),
        };
        let mut editor = self.repo.edit_tree(base)?;
        let mut prepared = Prepared {
            base,
            tree: base,
            entries: Vec::new(),
            written: Vec::new(),
            removed: Vec::new(),
            unreferenced: Vec::new(),
            from_memory: Vec::new(),
            warnings: Vec::new(),
        };
        // The references of the saved design files (None: unreadable).
        let mut saved = BTreeMap::new();
        for path in paths {
            let full = self.root.join(path);
            let old = self.blob_entry(base, path)?;
            let read = match contents.get(path) {
                Some(data) => {
                    prepared.from_memory.push(path.clone());
                    Ok(data.clone())
                }
                None => fs::read(&full),
            };
            match read {
                Ok(data) => {
                    let id = self.repo.write_blob(&data)?.detach();
                    if old != Some(id) {
                        editor.upsert(path.as_str(), EntryKind::Blob, id)?;
                        prepared.written.push(path.clone());
                    }
                    prepared.entries.push((path.clone(), Some(id)));
                    if is_design_file(path) {
                        saved.insert(path.clone(), self.references_with(id, || Ok(data)));
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if old.is_some() {
                        editor.remove(path.as_str())?;
                        prepared.removed.push(path.clone());
                    }
                    prepared.entries.push((path.clone(), None));
                    saved.insert(path.clone(), Some(BTreeSet::new()));
                }
                Err(e) => return Err(io_error("read", &full, e)),
            }
        }
        self.reconcile_store(&saved, &mut editor, &mut prepared)?;
        prepared.tree = editor.write()?.detach();
        Ok(prepared)
    }

    /// Adds the B-rep files the version's design files refer to that its
    /// tree lacks (from the project's store), and removes those none
    /// refers to, unless a design file could not be read.
    fn reconcile_store(
        &self,
        saved: &BTreeMap<String, Option<BTreeSet<Sha256>>>,
        editor: &mut gix::object::tree::Editor<'_>,
        prepared: &mut Prepared,
    ) -> Result<(), VcsError> {
        let mut referenced = BTreeSet::new();
        let mut unreadable = Vec::new();
        for (path, references) in saved {
            match references {
                Some(references) => referenced.extend(references.iter().copied()),
                None => unreadable.push(path.clone()),
            }
        }
        for (path, id) in self.design_files(prepared.base)? {
            if saved.contains_key(&path) {
                continue;
            }
            match self.references_with(id, || Ok(self.repo.find_blob(id)?.take_data())) {
                Some(references) => referenced.extend(references),
                None => unreadable.push(path),
            }
        }
        let present: BTreeSet<String> = self.brep_files(prepared.base)?.into_iter().collect();
        for sha256 in &referenced {
            let path = brep_path(sha256);
            let full = self.root.join(&path);
            crate::transaction::check_destination(&self.root, &full)?;
            if present.contains(&path) {
                continue;
            }
            match fs::read(&full) {
                Ok(data) => {
                    mitcad_model::features::Brep::check_stored(sha256, &data).map_err(|error| {
                        VcsError::File(format!("the B-rep file {path} is invalid: {error}"))
                    })?;
                    let id = self.repo.write_blob(&data)?.detach();
                    editor.upsert(path.as_str(), EntryKind::Blob, id)?;
                    prepared.written.push(path.clone());
                    prepared.entries.push((path, Some(id)));
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => prepared.warnings.push(format!(
                    "the B-rep file {path} is missing from the project's store, so the version \
                     lacks it"
                )),
                Err(e) => return Err(io_error("read", &full, e)),
            }
        }
        if !unreadable.is_empty() {
            prepared.warnings.push(format!(
                "{} could not be read as a design file, so no B-rep file is removed",
                unreadable.join(", ")
            ));
            return Ok(());
        }
        for path in present {
            match brep_sha256(&path) {
                Some(sha256) if !referenced.contains(&sha256) => {
                    editor.remove(path.as_str())?;
                    prepared.removed.push(path.clone());
                    prepared.entries.push((path.clone(), None));
                    prepared.unreferenced.push(path);
                }
                // Referred to, or not a file of the store.
                _ => {}
            }
        }
        Ok(())
    }

    /// The B-rep data a design file (blob `id`, its content from `read`)
    /// refers to; None when it is not a design file this build reads. Kept
    /// in memory and in `.mitcad/cache/refs/<id>`, as a blob never changes.
    pub(crate) fn references_with(
        &self,
        id: ObjectId,
        read: impl FnOnce() -> Result<Vec<u8>, VcsError>,
    ) -> Option<BTreeSet<Sha256>> {
        if let Some(known) = self.references.borrow().get(&id) {
            return known.clone();
        }
        let cache = self.root.join(".mitcad/cache/refs").join(id.to_string());
        let cached = fs::read_to_string(&cache).ok().and_then(|text| {
            text.lines()
                .map(|line| line.parse::<Sha256>().ok())
                .collect::<Option<BTreeSet<_>>>()
        });
        let references = cached.or_else(|| {
            let data = read().ok()?;
            let text = std::str::from_utf8(&data).ok()?;
            let references = mitcad_model::file::brep_references(text).ok()?;
            let lines: String = references.iter().map(|s| format!("{s}\n")).collect();
            let _ = mitcad_model::file::write_atomically(&cache, lines.as_bytes());
            Some(references)
        });
        self.references.borrow_mut().insert(id, references.clone());
        references
    }

    /// The commit is made: the index entries and the folder follow.
    fn finish(
        &self,
        commit: ObjectId,
        prepared: Prepared,
        lock: gix::lock::File,
        renames: &[(String, String)],
    ) -> CommitOutcome {
        let mut outcome = CommitOutcome {
            commit: Some(commit.to_string()),
            skipped: None,
            written: prepared.written,
            removed: prepared.removed,
            deleted: Vec::new(),
            warnings: prepared.warnings,
        };
        if let Err(e) = self.update_index(lock, &prepared.entries, &prepared.from_memory) {
            outcome.warnings.push(format!(
                "the version is recorded, but git's index could not be updated, so git status \
                 may show its files as changed: {e}"
            ));
        }
        outcome.deleted =
            self.delete_unreferenced(&prepared.unreferenced, prepared.tree, &mut outcome.warnings);
        // A removed design leaves no per-user display state behind.
        // An explicit move migrates that state after this commit succeeds.
        if let Some(project) = mitcad_model::Project::open(&self.root) {
            for path in outcome.removed.iter().filter(|p| is_design_file(p)) {
                if renames.iter().any(|(from, to)| {
                    from == path
                        && is_design_file(to)
                        && prepared
                            .entries
                            .iter()
                            .any(|(path, blob)| path == to && blob.is_some())
                }) {
                    continue;
                }
                let file = self.root.join(path);
                if !file.exists() {
                    let _ = project.remove_local_state(&file);
                }
            }
        }
        outcome
    }

    /// Sets the index entries of the committed paths to their blobs with
    /// the folder's file times and sizes (removing those of removed
    /// paths), so `git status` sees them clean; other entries stay. Those
    /// of `from_memory`, whose blobs are not the folder's files, take no
    /// file times, so that git compares them by content. Writes the index
    /// through the lock held since the round began.
    fn update_index(
        &self,
        mut lock: gix::lock::File,
        entries: &[(String, Option<ObjectId>)],
        from_memory: &[String],
    ) -> Result<(), VcsError> {
        let path = self.repo.index_path();
        // Read now, under the lock: not the repository's snapshot, which may
        // miss an index written a moment ago.
        let mut index = if path.is_file() {
            self.repo.open_index()?
        } else {
            gix::index::File::from_state(
                gix::index::State::new(self.repo.object_hash()),
                path.clone(),
            )
        };
        let keys: HashSet<&BStr> = entries
            .iter()
            .map(|(p, _)| p.as_bytes().as_bstr())
            .collect();
        // All stages: a conflict left on a committed path is resolved by it.
        index.remove_entries(|_, entry_path, _| keys.contains(entry_path));
        for (relative, id) in entries {
            let Some(id) = id else {
                continue;
            };
            let stat = if from_memory.contains(relative) {
                gix::index::entry::Stat::default()
            } else {
                gix::index::fs::Metadata::from_path_no_follow(&self.root.join(relative))
                    .ok()
                    .and_then(|metadata| gix::index::entry::Stat::from_fs(&metadata).ok())
                    .unwrap_or_default()
            };
            index.dangerously_push_entry(
                stat,
                *id,
                gix::index::entry::Flags::empty(),
                gix::index::entry::Mode::FILE,
                relative.as_bytes().as_bstr(),
            );
        }
        // gix writes the tree cache as it is, which would still name the
        // old trees of these paths' folders, and a later `git commit` would
        // take them.
        index.remove_tree();
        index.sort_entries();
        let mut data = Vec::new();
        index.write_to(&mut data, gix::index::write::Options::default())?;
        lock.write_all(&data)
            .map_err(|e| io_error("write", &path, e))?;
        lock.commit()
            .map_err(|e| io_error("write", &path, e.error))?;
        Ok(())
    }

    /// Deletes the B-rep files the version dropped from the folder, unless
    /// a design file there that differs from the version (not saved, or
    /// saved without a version) refers to them.
    fn delete_unreferenced(
        &self,
        unreferenced: &[String],
        tree: ObjectId,
        warnings: &mut Vec<String>,
    ) -> Vec<String> {
        if unreferenced.is_empty() {
            return Vec::new();
        }
        let inspected = (|| -> Result<BTreeSet<Sha256>, VcsError> {
            let (files, error) = scan_design_files(&self.root);
            if let Some(error) = error {
                return Err(error);
            }
            let mut kept = BTreeSet::new();
            for file in files {
                let relative = self.relative(&file)?;
                let data = fs::read(&file).map_err(|e| io_error("read", &file, e))?;
                let id =
                    gix::objs::compute_hash(self.repo.object_hash(), gix::objs::Kind::Blob, &data)
                        .map_err(|e| VcsError::Git(e.to_string()))?;
                if self.blob_entry(tree, &relative)? == Some(id) {
                    continue;
                }
                let references = self.references_with(id, || Ok(data)).ok_or_else(|| {
                    VcsError::File(format!("{relative} could not be read as a design file"))
                })?;
                kept.extend(references);
            }
            Ok(kept)
        })();
        let kept = match inspected {
            Ok(kept) => kept,
            Err(error) => {
                warnings.push(format!(
                    "{error}; the working designs could not all be inspected, so the B-rep \
                     files the version dropped stay in the folder"
                ));
                return Vec::new();
            }
        };
        let mut deleted = Vec::new();
        for path in unreferenced {
            if brep_sha256(path).is_some_and(|sha256| kept.contains(&sha256)) {
                continue;
            }
            let full = self.root.join(path);
            match fs::remove_file(&full) {
                Ok(()) => {
                    deleted.push(path.clone());
                    // The `<aa>` folder when it is empty now.
                    if let Some(folder) = full.parent() {
                        let _ = fs::remove_dir(folder);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => warnings.push(format!("cannot delete {}: {e}", full.display())),
            }
        }
        deleted
    }
}

/// The design files under `root`, leaving out folders whose names start
/// with a dot (`.git`, `.mitcad`) and links.
pub(crate) fn design_files_in(root: &Path) -> Vec<PathBuf> {
    scan_design_files(root).0
}

/// Discovery may use a partial list, but deleting geometry requires a
/// complete one. Keep the first inspection failure for that caller.
fn scan_design_files(root: &Path) -> (Vec<PathBuf>, Option<VcsError>) {
    let mut out = Vec::new();
    let mut error = None;
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) => {
                error.get_or_insert_with(|| io_error("read", &folder, e));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    error.get_or_insert_with(|| io_error("read", &folder, e));
                    continue;
                }
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                error.get_or_insert_with(|| {
                    VcsError::InvalidPath(format!(
                        "{} has a filename that is not UTF-8",
                        entry.path().display()
                    ))
                });
                continue;
            };
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && !name.starts_with('.') => folders.push(entry.path()),
                Ok(kind) if kind.is_file() && is_design_file(name) => out.push(entry.path()),
                Err(e) => {
                    error.get_or_insert_with(|| io_error("inspect", &entry.path(), e));
                }
                Ok(_) => {}
            }
        }
    }
    out.sort();
    (out, error)
}

/// The commit message: the given one (or `Save <paths>`) and trailers
/// with Mitcad's version, the design file version and any known moves.
fn full_message(message: &str, paths: &[String], renames: &[(String, String)]) -> String {
    let message = message.trim();
    let message = if message.is_empty() {
        format!("Save {}", paths.join(", "))
    } else {
        message.to_owned()
    };
    let mut message =
        format!("{message}\n\nMitcad-Version: {MITCAD_VERSION}\nMitcad-Format: {DESIGN_FORMAT}\n");
    for (from, to) in renames {
        // JSON keeps filenames containing newlines or quotes in one trailer.
        let pair = serde_json::to_string(&[from, to]).expect("two paths serialize");
        message.push_str(&format!("Mitcad-Rename: {pair}\n"));
    }
    message
}

/// Known moves from only the final trailer block generated by Mitcad.
/// Text in the user's message is never interpreted as a move.
pub(crate) fn rename_trailers(message: &str) -> Vec<(String, String)> {
    let Some((_, trailers)) = message.trim_end().rsplit_once("\n\n") else {
        return Vec::new();
    };
    let mut lines = trailers.lines();
    if !lines
        .next()
        .is_some_and(|line| line.starts_with("Mitcad-Version: "))
        || !lines
            .next()
            .is_some_and(|line| line.starts_with("Mitcad-Format: "))
        || !trailers.lines().all(|line| line.starts_with("Mitcad-"))
    {
        return Vec::new();
    }
    lines
        .filter_map(|line| line.strip_prefix("Mitcad-Rename: "))
        .filter_map(|pair| serde_json::from_str::<[String; 2]>(pair).ok())
        .map(|[from, to]| (from, to))
        .collect()
}

/// A commit message without the trailers [`full_message`] adds.
pub(crate) fn user_message(message: &str) -> &str {
    let message = message.trim_end();
    match message.rfind("\n\n") {
        Some(at)
            if message[at + 2..]
                .lines()
                .all(|line| line.starts_with("Mitcad-")) =>
        {
            message[..at].trim_end()
        }
        _ => message,
    }
}
