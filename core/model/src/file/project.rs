// SPDX-License-Identifier: MIT
//! Projects and their B-rep store (P12a).
//!
//! A project is a folder with the marker file `.mitcad/project.json`; its
//! project files (`.mitcad`, in the folder or below it) are written in
//! version 3, which keeps the B-rep data of base features out of the JSON
//! in files of the project's store, `.mitcad/brep/<aa>/<sha256>.brep.zlib`:
//! the data zlib-compressed, named by the SHA-256 of the uncompressed data
//! (`aa` its first two digits). A file is written once and never changed, so
//! a body that does not change is stored once however often the project is
//! saved, and bodies with the same data share a file. The project files stay
//! small and diff line by line, which is what the local version history
//! (P12, git) needs. Outside projects a project file is one file (version
//! 2, the data inside).
//!
//! The display state (the Origin folder shown, Isolate) is per user and not
//! versioned: in a project it goes to `.mitcad/local/display/<path in the
//! project>.json` instead of the project file (`.gitignore` leaves out
//! `.mitcad/local/`).
//!
//! The store is a trait ([`BlobStore`]) so that an older version can be read
//! from elsewhere, e.g. a commit's tree (P12b). Stores keep and return the
//! files' content as it is; compressing and checking the data is the
//! model's (`crate::features::Brep`).

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::sha256::Sha256;

/// The marker file of a project, relative to its folder.
pub const PROJECT_MARKER: &str = ".mitcad/project.json";
/// The folder of a project's B-rep files, relative to its folder.
pub const BREP_DIR: &str = ".mitcad/brep";
/// The folder of a project's per-user state, which is not versioned (the
/// recommended `.gitignore` lists it), relative to its folder.
pub const LOCAL_DIR: &str = ".mitcad/local";

/// What [`Project::init`] writes into the marker file.
const MARKER_TEXT: &str = "{\"format\": \"mitcad-project\", \"version\": 1}\n";

/// The recommended `.gitattributes` of a project: project files are text
/// with LF line ends on every platform (Mitcad writes LF, and a checkout on
/// Windows with `core.autocrlf` must not change them) that git merges only
/// as whole files (`merge=binary`: a line-by-line merge could make valid
/// JSON of a broken design; Mitcad's sync lets the user choose a file's
/// version instead), B-rep files are binary.
pub const GITATTRIBUTES: &str = "\
# Mitcad project files are text with LF line ends on every platform; git
# merges them only as whole files.
*.mitcad text eol=lf merge=binary
# B-rep data of base features: content-addressed, never changed.
.mitcad/brep/** binary
";

/// The recommended `.gitignore` of a project: what is not part of a
/// version (caches, per-user state, temporary files of an interrupted
/// save).
pub const GITIGNORE: &str = "\
# Mitcad: caches, per-user state and temporary files of interrupted saves.
.mitcad/cache/
.mitcad/local/
.*.tmp
";

/// Where the B-rep files of version 3 project files are kept, by the
/// SHA-256 of their uncompressed data. A store keeps a file's content as it
/// is given (zlib-compressed data); compressing, decompressing and checking
/// the data is the model's.
pub trait BlobStore {
    /// The content of the file of `sha256`, or None when the store does not
    /// have it.
    fn get(&self, sha256: &Sha256) -> io::Result<Option<Vec<u8>>>;
    /// Whether the store has the file of `sha256`, so saving does not
    /// compress data again that is stored already.
    fn contains(&self, sha256: &Sha256) -> io::Result<bool>;
    /// Stores the content of the file of `sha256`, whole or not at all. A
    /// file that exists is left as it is (same name, same data).
    fn put(&self, sha256: &Sha256, content: &[u8]) -> io::Result<()>;
}

/// The path of the B-rep file of `sha256` relative to a project's folder:
/// `.mitcad/brep/<aa>/<sha256>.brep.zlib`.
pub fn brep_path(sha256: &Sha256) -> String {
    let name = sha256.to_string();
    format!("{BREP_DIR}/{}/{name}.brep.zlib", &name[..2])
}

/// A store in a folder: a project's `.mitcad/brep`.
#[derive(Debug, Clone)]
pub struct FsStore {
    dir: PathBuf,
}

impl FsStore {
    /// The store in `dir` (created when the first file is written).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file of `sha256`.
    pub fn path(&self, sha256: &Sha256) -> PathBuf {
        let name = sha256.to_string();
        self.dir.join(&name[..2]).join(format!("{name}.brep.zlib"))
    }
}

impl BlobStore for FsStore {
    fn get(&self, sha256: &Sha256) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.path(sha256)) {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn contains(&self, sha256: &Sha256) -> io::Result<bool> {
        Ok(self.path(sha256).is_file())
    }

    fn put(&self, sha256: &Sha256, content: &[u8]) -> io::Result<()> {
        let path = self.path(sha256);
        if path.is_file() {
            return Ok(());
        }
        match write_atomically(&path, content) {
            // Another process may have written it in the meantime.
            Err(_) if path.is_file() => Ok(()),
            result => result,
        }
    }
}

/// A store in memory: tests, and B-rep data read from elsewhere.
#[derive(Debug, Default)]
pub struct MemoryStore {
    files: Mutex<HashMap<Sha256, Vec<u8>>>,
    puts: AtomicU64,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of files.
    pub fn len(&self) -> usize {
        self.files.lock().expect("not poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many files [`BlobStore::put`] has written (not counting those
    /// that were there).
    pub fn writes(&self) -> u64 {
        self.puts.load(Ordering::Relaxed)
    }

    /// Replaces a file's content, e.g. to damage it in a test.
    pub fn replace(&self, sha256: Sha256, content: Vec<u8>) {
        self.files
            .lock()
            .expect("not poisoned")
            .insert(sha256, content);
    }

    /// Removes a file.
    pub fn remove(&self, sha256: &Sha256) {
        self.files.lock().expect("not poisoned").remove(sha256);
    }
}

impl BlobStore for MemoryStore {
    fn get(&self, sha256: &Sha256) -> io::Result<Option<Vec<u8>>> {
        Ok(self
            .files
            .lock()
            .expect("not poisoned")
            .get(sha256)
            .cloned())
    }

    fn contains(&self, sha256: &Sha256) -> io::Result<bool> {
        Ok(self
            .files
            .lock()
            .expect("not poisoned")
            .contains_key(sha256))
    }

    fn put(&self, sha256: &Sha256, content: &[u8]) -> io::Result<()> {
        let mut files = self.files.lock().expect("not poisoned");
        if !files.contains_key(sha256) {
            files.insert(*sha256, content.to_vec());
            self.puts.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// A project: a folder with the marker file `.mitcad/project.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    root: PathBuf,
}

impl Project {
    /// The project a file is in: the nearest folder, from the file's own
    /// upwards, that has the marker. The file need not exist.
    pub fn find(file: &Path) -> Option<Project> {
        let file = absolute(file).ok()?;
        file.ancestors()
            .skip(1)
            .find(|dir| dir.join(PROJECT_MARKER).is_file())
            .map(|dir| Project {
                root: dir.to_path_buf(),
            })
    }

    /// The project in folder `dir`, when it has the marker.
    pub fn open(dir: &Path) -> Option<Project> {
        let root = absolute(dir).ok()?;
        root.join(PROJECT_MARKER)
            .is_file()
            .then_some(Project { root })
    }

    /// Makes folder `dir` (created if needed) a project: writes the marker,
    /// and the recommended `.gitattributes` and `.gitignore` lines that are
    /// missing (a file there keeps its own lines; an attributes line of an
    /// older recommendation is brought up to date in place). Returns the
    /// project and the files written or extended, relative to `dir`. A
    /// folder that is a project already stays one.
    pub fn init(dir: &Path) -> io::Result<(Project, Vec<String>)> {
        let root = absolute(dir)?;
        fs::create_dir_all(&root)?;
        let mut written = Vec::new();
        let marker = root.join(PROJECT_MARKER);
        if !marker.is_file() {
            write_atomically(&marker, MARKER_TEXT.as_bytes())?;
            written.push(PROJECT_MARKER.to_owned());
        }
        if add_attribute_lines(&root.join(".gitattributes"), GITATTRIBUTES)? {
            written.push(".gitattributes".to_owned());
        }
        if add_missing_lines(&root.join(".gitignore"), GITIGNORE)? {
            written.push(".gitignore".to_owned());
        }
        Ok((Project { root }, written))
    }

    /// The project's folder (absolute).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project's B-rep store.
    pub fn store(&self) -> FsStore {
        FsStore::new(self.root.join(BREP_DIR))
    }

    /// The file of a project file's per-user state, which the project does
    /// not version: `.mitcad/local/display/<path in the project>.json`.
    /// None for a file outside the project.
    pub fn local_state_path(&self, file: &Path) -> Option<PathBuf> {
        let file = absolute(file).ok()?;
        let relative = file.strip_prefix(&self.root).ok()?;
        let mut path = self.root.join(LOCAL_DIR).join("display").join(relative);
        let mut name = path.file_name()?.to_os_string();
        name.push(".json");
        path.set_file_name(name);
        Some(path)
    }

    /// A project file of this project was renamed or moved from `from` to
    /// `to` (both in the project): its per-user state follows, unless `to`
    /// has one of its own already (then `from`'s goes). True when a state
    /// was moved.
    pub fn move_local_state(&self, from: &Path, to: &Path) -> io::Result<bool> {
        let (Some(old), Some(new)) = (self.local_state_path(from), self.local_state_path(to))
        else {
            return Ok(false);
        };
        if old == new || !old.is_file() {
            return Ok(false);
        }
        if new.is_file() {
            fs::remove_file(&old)?;
            return Ok(false);
        }
        if let Some(dir) = new.parent() {
            fs::create_dir_all(dir)?;
        }
        rename_retrying(&old, &new)?;
        Ok(true)
    }

    /// A project file of this project is gone: its per-user state goes too.
    /// True when there was one.
    pub fn remove_local_state(&self, file: &Path) -> io::Result<bool> {
        let Some(local) = self.local_state_path(file) else {
            return Ok(false);
        };
        match fs::remove_file(local) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

/// The absolute form of a path with `.` and `..` folded away lexically, so
/// that walking up from `../parts/a.mitcad` visits the folders that really
/// hold it.
fn absolute(path: &Path) -> io::Result<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in std::path::absolute(path)?.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Ok(out)
}

/// The pattern and the attributes of a `.gitattributes` line (empty for a
/// comment or a blank line).
fn attribute_words(line: &str) -> Vec<&str> {
    if line.trim_start().starts_with('#') {
        return Vec::new();
    }
    line.split_whitespace().collect()
}

/// Adds the attribute lines of `text` that `path` lacks (comments only to a
/// new file): a line for the same pattern whose attributes the new line all
/// has (an older recommendation, `*.mitcad text eol=lf`) is replaced in
/// place, others are appended; the file's own lines stay. True when the
/// file was written.
fn add_attribute_lines(path: &Path, text: &str) -> io::Result<bool> {
    let existing = match fs::read_to_string(path) {
        Ok(existing) => existing,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            write_atomically(path, text.as_bytes())?;
            return Ok(true);
        }
        Err(e) => return Err(e),
    };
    let mut lines: Vec<String> = existing.lines().map(str::to_owned).collect();
    let mut changed = false;
    for line in text.lines() {
        let words = attribute_words(line);
        let Some((pattern, attributes)) = words.split_first() else {
            continue;
        };
        if lines.iter().any(|have| attribute_words(have) == words) {
            continue;
        }
        let older = lines.iter_mut().find(|have| {
            let have = attribute_words(have);
            have.first() == Some(pattern) && have[1..].iter().all(|a| attributes.contains(a))
        });
        match older {
            Some(older) => *older = line.to_owned(),
            None => lines.push(line.to_owned()),
        }
        changed = true;
    }
    if !changed {
        return Ok(false);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    write_atomically(path, out.as_bytes())?;
    Ok(true)
}

/// Appends the lines of `text` that `path` lacks (comments only to a new
/// file); true when the file was written.
fn add_missing_lines(path: &Path, text: &str) -> io::Result<bool> {
    let existing = match fs::read_to_string(path) {
        Ok(existing) => existing,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            write_atomically(path, text.as_bytes())?;
            return Ok(true);
        }
        Err(e) => return Err(e),
    };
    let have: Vec<&str> = existing.lines().map(str::trim).collect();
    let missing: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !have.contains(&line.trim()))
        .collect();
    if missing.is_empty() {
        return Ok(false);
    }
    let mut out = existing.clone();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for line in missing {
        out.push_str(line);
        out.push('\n');
    }
    write_atomically(path, out.as_bytes())?;
    Ok(true)
}

/// Writes a file whole or not at all: a temporary file next to it (named
/// `.<name>.<process>.<n>.tmp`, which a project's `.gitignore` leaves out),
/// flushed to disk, then renamed over it. The folder is created if needed.
pub fn write_atomically(path: &Path, data: &[u8]) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?
        .to_string_lossy();
    let temporary = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        use std::io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        rename_retrying(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// How long [`rename_retrying`] keeps trying, in all.
const RENAME_PATIENCE: std::time::Duration = std::time::Duration::from_secs(2);

/// Renames `from` over `to`, trying again for a while when another program
/// holds `to` (or `from`) open: on Windows a virus scanner, an indexer or a
/// sync client that just opened the file makes the rename fail with "access
/// denied" or a sharing violation for a moment.
pub fn rename_retrying(from: &Path, to: &Path) -> io::Result<()> {
    retrying(|| fs::rename(from, to), is_transient_lock, RENAME_PATIENCE)
}

/// An error another program's open handle causes for a moment (Windows:
/// access denied, sharing and lock violations). Never on other systems,
/// where a rename does not fail for an open file.
fn is_transient_lock(error: &io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Runs `operation` until it succeeds, fails with an error `transient` does
/// not accept, or `patience` has passed, waiting longer each time (10 ms,
/// 20 ms, ... up to 200 ms).
pub(crate) fn retrying(
    mut operation: impl FnMut() -> io::Result<()>,
    transient: impl Fn(&io::Error) -> bool,
    patience: std::time::Duration,
) -> io::Result<()> {
    let start = std::time::Instant::now();
    let mut wait = std::time::Duration::from_millis(10);
    loop {
        match operation() {
            Err(error) if transient(&error) && start.elapsed() + wait <= patience => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(std::time::Duration::from_millis(200));
            }
            result => return result,
        }
    }
}
