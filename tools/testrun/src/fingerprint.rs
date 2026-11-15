// SPDX-License-Identifier: MIT
//! Release fingerprints (mitcad#1822): in a release gate's full test run a
//! `geometry*` ctest is skipped when its fingerprint is one with which it
//! passed in an earlier release gate of the same platform.
//!
//! The fingerprint is the logic of `tools/ctest-fingerprints.py` (which
//! decides the Linux development checks of `tools/check-all.sh`) in Rust,
//! so that it is computed the same way on Linux, Windows and macOS: a
//! digest (SHA-256) of the contents of what the test depends on, found from
//! the build itself rather than from a list kept by hand:
//!
//! - its own registration (command, properties), the checkout's and the
//!   build folder's paths left out;
//! - the files and folders of the source tree its command and environment
//!   name;
//! - for each program its command names (the test executable), the CMake
//!   targets it is built from, through their dependencies (CMake's file
//!   API, codemodel-v2): each target's folder (less the folders of other
//!   CMake directories and Rust crates, and less the files tests name), its
//!   listed sources, and for a crate Corrosion builds its Rust closure;
//!   with them the build files (`BUILD_FILES`: the top-level CMake files,
//!   `cmake/`, `triplets/`, `third_party/` with the OCCT port and its
//!   patches, `vcpkg.json`, `rust-toolchain.toml`), the compiler and its
//!   version (the file API's toolchains-v1) and the build's options;
//! - for a crate's tests the Rust closure of each package (`cargo
//!   metadata`): the workspace crates' folders, the files they include, the
//!   external crates with their versions and features;
//! - for a script an interpreter runs the files beside it; for cargo-deny
//!   the manifests, the lock and `deny.toml`;
//! - the fingerprints of the fixtures the test requires.
//!
//! A test whose command is none of these depends on the whole tree, as
//! does every test when the file API's reply is missing. Markdown documents
//! count for no test. Unlike the Python script, the release's version is
//! left out of the files whose version every release sets (`versionless`),
//! or no fingerprint would ever repeat from one release to the next. The
//! digests differ from the Python script's (MD5 there): the two keep their
//! passes apart.
//!
//! The passes are kept per platform (`std::env::consts::OS`) in a folder
//! that outlives the build folders (`--release-passed DIR`):
//! `DIR/<platform>/<test>`, a line per fingerprint, `<fingerprint>\t<release>`
//! (the release and RC of the gate in which it last passed), the last
//! `KEEP` fingerprints of each test.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::changed::{
    cache_value, cargo_packages, is_absolute, join, key, norm, path_value, relative,
};
use crate::ctest::TestDef;

/// The tests a release may skip: those whose names start with it.
pub const GROUP: &str = "geometry";
/// The table of the skipped tests the run writes into its output folder
/// (name, fingerprint, the release it last passed in).
pub const SKIPPED_TABLE: &str = "release-skipped.tsv";
/// The fingerprints kept per test.
pub const KEEP: usize = 20;
/// Tests that check the whole tree (every source file's licence header).
const WHOLE_TREE: [&str; 1] = ["repo.spdx"];
/// What every C++ build depends on beyond its targets' folders.
pub const BUILD_FILES: [&str; 7] = [
    "CMakeLists.txt",
    "CMakePresets.json",
    "cmake",
    "triplets",
    "vcpkg.json",
    "third_party",
    "rust-toolchain.toml",
];
/// What every Rust build depends on beyond its crates.
const RUST_FILES: [&str; 3] = ["rust-toolchain.toml", "core/Cargo.toml", "core/.cargo"];

/// The SHA-256 of the data, in hex.
pub fn digest(data: &[u8]) -> String {
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(data) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// The file name of a path.
fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The program's name without `.exe`.
fn program(path: &str) -> &str {
    let name = base_name(path);
    name.strip_suffix(".exe").unwrap_or(name)
}

/// An interpreter of scripts: bash, sh, python (with a version).
fn interpreter(path: &str) -> bool {
    let name = program(path);
    let python = name
        .strip_prefix("python")
        .is_some_and(|v| v.chars().all(|c| c.is_ascii_digit() || c == '.'));
    name == "bash" || name == "sh" || python
}

/// A test in the group a release may skip (and not a fixture's setup or
/// cleanup, which other tests need).
pub fn candidate(def: &TestDef) -> bool {
    def.name.starts_with(GROUP) && def.fixtures_setup.is_empty() && def.fixtures_cleanup.is_empty()
}

/// The string literals `include_str!` and `include_bytes!` name.
pub fn included_files(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("include_") {
        rest = &rest[at + "include_".len()..];
        let Some(body) = rest
            .strip_prefix("str!")
            .or_else(|| rest.strip_prefix("bytes!"))
        else {
            continue;
        };
        let Some(body) = body.trim_start().strip_prefix('(') else {
            continue;
        };
        let Some(body) = body.trim_start().strip_prefix('"') else {
            continue;
        };
        if let Some(end) = body.find('"') {
            found.push(body[..end].to_owned());
        }
    }
    found
}

/// The line is `<prefix><a version><suffix>` (a version: digits and dots).
fn version_line(line: &str, prefix: &str, suffix: &str) -> bool {
    let line = line.trim_end_matches(['\r', '\n']);
    let version = line
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix));
    version.is_some_and(|v| !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

/// The line with its version replaced by a placeholder.
fn placeholder(line: &str, prefix: &str, suffix: &str) -> String {
    let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
    format!("{prefix}<version>{suffix}{ending}")
}

/// A block of Cargo.lock into `out`, a workspace package's (no source
/// line) without its version.
fn lock_block(block: &mut Vec<&str>, out: &mut String) {
    let own = !block.iter().any(|l| l.starts_with("source = "));
    for line in block.drain(..) {
        if own && version_line(line, "version = \"", "\"") {
            out.push_str(&placeholder(line, "version = \"", "\""));
        } else {
            out.push_str(line);
        }
    }
}

/// The text a file's digest is taken of when it is one of the files whose
/// version `release.sh version` sets for every release (the top-level
/// `CMakeLists.txt`'s `  VERSION`, `vcpkg.json`'s `  "version"`, the
/// workspace's version in `core/Cargo.toml` and its packages' in
/// `core/Cargo.lock`): the version left out, so that a release's version
/// alone changes no fingerprint. None for the other files.
pub fn versionless(rel: &str, data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let mut out = String::with_capacity(text.len());
    match rel {
        "CMakeLists.txt" | "vcpkg.json" => {
            let (prefix, suffix) = if rel == "vcpkg.json" {
                ("  \"version\": \"", "\",")
            } else {
                ("  VERSION ", "")
            };
            for line in text.split_inclusive('\n') {
                if version_line(line, prefix, suffix) {
                    out.push_str(&placeholder(line, prefix, suffix));
                } else {
                    out.push_str(line);
                }
            }
        }
        "core/Cargo.toml" => {
            let mut section = "";
            for line in text.split_inclusive('\n') {
                if line.starts_with('[') {
                    section = line.trim();
                }
                let workspace = section == "[workspace.package]";
                if workspace && version_line(line, "version = \"", "\"") {
                    out.push_str(&placeholder(line, "version = \"", "\""));
                } else {
                    out.push_str(line);
                }
            }
        }
        "core/Cargo.lock" => {
            let mut block: Vec<&str> = Vec::new();
            for line in text.split_inclusive('\n') {
                if line.trim_end() == "[[package]]" {
                    lock_block(&mut block, &mut out);
                }
                block.push(line);
            }
            lock_block(&mut block, &mut out);
        }
        _ => return None,
    }
    Some(out)
}

/// The build's options that every C++ target depends on: `MITCAD_*` switches
/// and the build type, as CMakeCache.txt has them.
pub fn cache_options(cache: &str) -> Vec<String> {
    let option = |line: &str| {
        if line.starts_with("CMAKE_BUILD_TYPE:STRING=") {
            return true;
        }
        let name = line
            .strip_prefix("MITCAD_")
            .and_then(|rest| rest.split_once(":BOOL="))
            .map(|(name, _)| name);
        let plain = |c: char| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_';
        name.is_some_and(|n| !n.is_empty() && n.chars().all(plain))
    };
    let mut options: Vec<String> = cache
        .lines()
        .filter(|line| option(line))
        .map(|line| format!("option {}", line.trim()))
        .collect();
    options.sort();
    options
}

/// A property's value as a list of strings.
fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        Value::String(s) if !s.is_empty() => s.split(';').map(str::to_owned).collect(),
        _ => Vec::new(),
    }
}

/// How a folder is walked.
#[derive(Clone, Copy)]
struct Walk {
    /// Into the folders of other CMake directories and of crates.
    nested: bool,
    /// Only the folder's own files.
    flat: bool,
    /// Markdown documents too.
    docs: bool,
    /// The files tests name too.
    test_files: bool,
}

const ALL: Walk = Walk {
    nested: true,
    flat: false,
    docs: false,
    test_files: true,
};

/// The source tree's files and their digests, by path relative to it
/// (forward slashes, `.` for the tree itself).
struct Tree {
    source: String,
    build: String,
    root: PathBuf,
    /// The build folder relative to the source tree, when it is inside.
    build_rel: Option<String>,
    digests: HashMap<String, String>,
    /// Folders not walked into: other CMake directories (`key`).
    cmake_dirs: HashSet<String>,
    /// Files tests name (`key`), which count only for those tests.
    test_files: HashSet<String>,
}

impl Tree {
    fn new(source: &str, build: &str) -> Tree {
        let source = norm(source);
        let build = norm(build);
        Tree {
            root: PathBuf::from(&source),
            build_rel: relative(&source, &build),
            source,
            build,
            digests: HashMap::new(),
            cmake_dirs: HashSet::new(),
            test_files: HashSet::new(),
        }
    }

    /// The path relative to the source tree; None outside it or in the
    /// build folder.
    fn rel(&self, path: &str) -> Option<String> {
        let path = norm(path);
        if key(&path) == key(&self.build) || relative(&self.build, &path).is_some() {
            return None;
        }
        if key(&path) == key(&self.source) {
            return Some(".".to_owned());
        }
        relative(&self.source, &path)
    }

    fn abs(&self, rel: &str) -> PathBuf {
        if rel == "." || rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(rel)
        }
    }

    fn digest(&mut self, rel: &str) -> String {
        if let Some(d) = self.digests.get(rel) {
            return d.clone();
        }
        let d = match fs::read(self.abs(rel)) {
            Ok(data) => match versionless(rel, &data) {
                Some(text) => digest(text.as_bytes()),
                None => digest(&data),
            },
            Err(_) => "missing".to_owned(),
        };
        self.digests.insert(rel.to_owned(), d.clone());
        d
    }

    fn skipped_dir(&self, rel: &str, top: &str) -> bool {
        let name = base_name(rel);
        if (name.starts_with('.') && rel != top) || name == "__pycache__" {
            return true;
        }
        let build = self
            .build_rel
            .as_deref()
            .is_some_and(|b| key(b) == key(rel));
        if build || rel == "build" {
            return true;
        }
        if rel == top {
            return false;
        }
        let parent = rel.rsplit_once('/').map_or(".", |(p, _)| p);
        name == "target" && self.abs(parent).join("Cargo.toml").exists()
    }

    /// The files under `rel` (a file: itself), sorted.
    fn files(&self, rel: &str, walk: Walk) -> Vec<String> {
        let rel = if rel.is_empty() { "." } else { rel };
        let path = self.abs(rel);
        let Ok(meta) = fs::metadata(&path) else {
            return Vec::new();
        };
        if meta.is_file() {
            return vec![rel.to_owned()];
        }
        if !meta.is_dir() {
            return Vec::new();
        }
        let mut found = Vec::new();
        let mut stack = vec![rel.to_owned()];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(self.abs(&current)) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let child = if current == "." {
                    name.clone()
                } else {
                    format!("{current}/{name}")
                };
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if walk.flat || self.skipped_dir(&child, rel) {
                        continue;
                    }
                    let other = self.cmake_dirs.contains(&key(&child))
                        || self.abs(&child).join("Cargo.toml").exists();
                    if !walk.nested && other {
                        continue;
                    }
                    stack.push(child);
                    continue;
                }
                // A link to a folder is not followed; a link to a file counts.
                let file = kind.is_file() || fs::metadata(entry.path()).is_ok_and(|m| m.is_file());
                if !file
                    || (!walk.docs && name.ends_with(".md"))
                    || (!walk.test_files && self.test_files.contains(&key(&child)))
                {
                    continue;
                }
                found.push(child);
            }
        }
        found.sort();
        found
    }

    fn lines(&mut self, rels: &[String]) -> Vec<String> {
        rels.iter()
            .map(|r| format!("{r} {}", self.digest(r)))
            .collect()
    }

    /// The test's registration with the build's and the checkout's paths
    /// replaced, so that checkouts share fingerprints.
    fn strip_roots(&self, text: &str) -> String {
        text.replace(&self.build, "<build>")
            .replace(&self.source, "<source>")
    }
}

/// A workspace package or a resolved dependency (`cargo metadata`).
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    /// The manifest's folder.
    dir: String,
}

/// A node of cargo's resolution: its features and dependencies, each with
/// its kinds (None: normal).
struct Node {
    features: Vec<String>,
    deps: Vec<(String, Vec<Option<String>>)>,
}

/// The workspace's crates and their resolved dependencies.
#[derive(Default)]
struct Cargo {
    ok: bool,
    packages: HashMap<String, Package>,
    nodes: HashMap<String, Node>,
    by_name: HashMap<String, String>,
    by_lib: HashMap<String, String>,
    memo: HashMap<(String, bool), Vec<String>>,
}

impl Cargo {
    fn from_metadata(meta: Option<&Value>) -> Cargo {
        let Some(meta) = meta else {
            return Cargo::default();
        };
        let mut cargo = Cargo::default();
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        for p in meta
            .get("packages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(name)) = (text(p, "id"), text(p, "name")) else {
                continue;
            };
            let manifest = text(p, "manifest_path").unwrap_or_default();
            let dir = norm(&manifest)
                .rsplit_once('/')
                .map_or_else(String::new, |(d, _)| d.to_owned());
            let package = Package {
                name,
                version: text(p, "version").unwrap_or_default(),
                source: text(p, "source"),
                dir,
            };
            cargo.packages.insert(id, package);
        }
        let resolve = meta.get("resolve").and_then(|r| r.get("nodes"));
        for n in resolve.and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = text(n, "id") else {
                continue;
            };
            let features = n.get("features").map(strings).unwrap_or_default();
            let mut deps = Vec::new();
            for d in n
                .get("deps")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(pkg) = text(d, "pkg") else {
                    continue;
                };
                let kinds = d.get("dep_kinds").and_then(Value::as_array);
                let kinds = kinds
                    .into_iter()
                    .flatten()
                    .map(|k| text(k, "kind"))
                    .collect();
                deps.push((pkg, kinds));
            }
            cargo.nodes.insert(id, Node { features, deps });
        }
        let members = meta.get("workspace_members").and_then(Value::as_array);
        for pid in members.into_iter().flatten().filter_map(Value::as_str) {
            let Some(p) = cargo.packages.get(pid) else {
                continue;
            };
            cargo.by_name.insert(p.name.clone(), pid.to_owned());
            // Corrosion names its targets after the crate's library or
            // program (cargo-build_mitcad_ffi, cargo-build_mitcad-release).
            let package = meta
                .get("packages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|q| q.get("id").and_then(Value::as_str) == Some(pid));
            let targets = package
                .and_then(|q| q.get("targets"))
                .and_then(Value::as_array);
            for t in targets.into_iter().flatten() {
                let kinds = t.get("kind").map(strings).unwrap_or_default();
                let built = ["lib", "staticlib", "cdylib", "rlib", "bin"];
                if kinds.iter().any(|k| built.contains(&k.as_str()))
                    && let Some(name) = text(t, "name")
                {
                    cargo.by_lib.insert(name.replace('-', "_"), pid.to_owned());
                }
            }
        }
        cargo.ok = true;
        cargo
    }

    /// `cargo metadata` of the workspace, None when cargo cannot give it.
    fn metadata(source: &str) -> Option<Value> {
        let manifest = Path::new(source).join("core").join("Cargo.toml");
        let output = Command::new("cargo")
            .args(["metadata", "--offline", "--locked", "--format-version", "1"])
            .arg("--manifest-path")
            .arg(manifest)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        serde_json::from_slice(&output.stdout).ok()
    }

    /// A workspace crate's files and what its sources include.
    fn crate_lines(tree: &mut Tree, rel: &str) -> Vec<String> {
        let walk = Walk {
            nested: false,
            ..ALL
        };
        let mut lines = Vec::new();
        for f in tree.files(rel, walk) {
            lines.push(format!("{f} {}", tree.digest(&f)));
            if !f.ends_with(".rs") {
                continue;
            }
            let Ok(bytes) = fs::read(tree.abs(&f)) else {
                continue;
            };
            let folder = f.rsplit_once('/').map_or(".", |(d, _)| d);
            let base = format!("{}/{folder}", tree.source);
            for included in included_files(&String::from_utf8_lossy(&bytes)) {
                if let Some(target) = tree.rel(&join(&base, &included)) {
                    lines.push(format!("{f} includes {target} {}", tree.digest(&target)));
                }
            }
        }
        lines
    }

    /// The lines of a package's Rust closure (dev: with its
    /// dev-dependencies), None for an unknown package.
    fn closure(&mut self, tree: &mut Tree, name: &str, dev: bool) -> Option<Vec<String>> {
        let memo = (name.to_owned(), dev);
        if let Some(lines) = self.memo.get(&memo) {
            return Some(lines.clone());
        }
        if !self.ok {
            // Without cargo's resolution: the whole workspace and its lock.
            let mut files = tree.files("core", ALL);
            files.push("core/Cargo.lock".to_owned());
            let lines = tree.lines(&files);
            self.memo.insert(memo, lines.clone());
            return Some(lines);
        }
        let pid = self
            .by_name
            .get(name)
            .or_else(|| self.by_lib.get(&name.replace('-', "_")))?
            .clone();
        let mut lines = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![(pid, true)];
        while let Some((current, top)) = stack.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            let Some(p) = self.packages.get(&current) else {
                continue;
            };
            let node = self.nodes.get(&current);
            let mut features = node.map(|n| n.features.clone()).unwrap_or_default();
            features.sort();
            let features = features.join(",");
            match &p.source {
                None => {
                    lines.push(format!("crate {} {features}", p.name));
                    if let Some(rel) = tree.rel(&p.dir) {
                        lines.extend(Cargo::crate_lines(tree, &rel));
                    }
                }
                Some(source) => {
                    let (n, v) = (&p.name, &p.version);
                    lines.push(format!("external {n} {v} {source} {features}"));
                }
            }
            for (dep, kinds) in node.map_or(&[][..], |n| &n.deps[..]) {
                let normal = kinds
                    .iter()
                    .any(|k| k.is_none() || k.as_deref() == Some("build"));
                if (top && dev) || normal {
                    stack.push((dep.clone(), false));
                }
            }
        }
        for f in RUST_FILES {
            let files = tree.files(f, ALL);
            lines.extend(tree.lines(&files));
        }
        lines.sort();
        self.memo.insert(memo, lines.clone());
        Some(lines)
    }
}

/// A CMake target of the file API's codemodel.
#[derive(Clone)]
struct Target {
    name: String,
    kind: String,
    /// Its CMake directory, relative to the source tree.
    dir: String,
    deps: Vec<String>,
    /// Its sources that are not generated, as the file API names them.
    sources: Vec<String>,
}

/// The build's targets (CMake's file API) and options.
#[derive(Default)]
struct CMake {
    ok: bool,
    targets: HashMap<String, Target>,
    /// Each artifact's path (`key`) and its target's id.
    artifacts: HashMap<String, String>,
    toolchain: Vec<String>,
    options: Vec<String>,
}

impl CMake {
    /// The newest reply of the build folder's file API.
    fn load(build: &Path, build_dir: &str, tree: &mut Tree) -> CMake {
        let reply = build.join(".cmake").join("api").join("v1").join("reply");
        let read = |name: &str| -> Option<Value> {
            let text = fs::read_to_string(reply.join(name)).ok()?;
            serde_json::from_str(&text).ok()
        };
        let index = fs::read_dir(&reply).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("index-") && n.ends_with(".json"))
                .max()
        });
        let Some(index) = index.and_then(|name| read(&name)) else {
            return CMake::default();
        };
        let mut codemodel = None;
        let mut toolchains = None;
        for object in index
            .get("objects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let file = object.get("jsonFile").and_then(Value::as_str);
            match object.get("kind").and_then(Value::as_str) {
                Some("codemodel") => codemodel = file.and_then(read),
                Some("toolchains") => toolchains = file.and_then(read),
                _ => {}
            }
        }
        let config = codemodel
            .as_ref()
            .and_then(|c| c.get("configurations")?.as_array()?.first());
        let Some(config) = config else {
            return CMake::default();
        };
        let dirs: Vec<String> = config
            .get("directories")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|d| norm(d.get("source").and_then(Value::as_str).unwrap_or(".")))
            .collect();
        for d in &dirs {
            tree.cmake_dirs.insert(key(d));
        }
        let mut cmake = CMake::default();
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
        let list = |v: &Value, k: &str| -> Vec<Value> {
            v.get(k)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        for reference in list(config, "targets") {
            let Some(t) = reference
                .get("jsonFile")
                .and_then(Value::as_str)
                .and_then(read)
            else {
                continue;
            };
            let at = reference.get("directoryIndex").and_then(Value::as_u64);
            let dir = at
                .and_then(|i| dirs.get(usize::try_from(i).ok()?))
                .cloned()
                .unwrap_or_else(|| ".".to_owned());
            let id = text(&t, "id");
            for artifact in list(&t, "artifacts") {
                let path = text(&artifact, "path");
                cmake
                    .artifacts
                    .insert(key(&join(build_dir, &path)), id.clone());
            }
            let generated = |s: &Value| s.get("isGenerated").and_then(Value::as_bool) == Some(true);
            let target = Target {
                name: text(&t, "name"),
                kind: text(&t, "type"),
                dir,
                deps: list(&t, "dependencies")
                    .iter()
                    .map(|d| text(d, "id"))
                    .collect(),
                sources: list(&t, "sources")
                    .iter()
                    .filter(|s| !generated(s))
                    .map(|s| text(s, "path"))
                    .collect(),
            };
            cmake.targets.insert(id, target);
        }
        let toolchains = toolchains
            .as_ref()
            .map(|t| list(t, "toolchains"))
            .unwrap_or_default();
        for tc in toolchains {
            let compiler = tc.get("compiler").cloned().unwrap_or(Value::Null);
            let field = |v: &Value, k: &str| {
                v.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or("None")
                    .to_owned()
            };
            cmake.toolchain.push(format!(
                "toolchain {} {} {}",
                field(&tc, "language"),
                field(&compiler, "id"),
                field(&compiler, "version")
            ));
        }
        let cache = fs::read_to_string(build.join("CMakeCache.txt")).unwrap_or_default();
        cmake.options = cache_options(&cache);
        cmake.ok = true;
        cmake
    }

    /// The target and every target it depends on, in the order found.
    fn closure(&self, id: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut stack = vec![id.to_owned()];
        while let Some(current) = stack.pop() {
            if seen.contains(&current) {
                continue;
            }
            let Some(target) = self.targets.get(&current) else {
                continue;
            };
            stack.extend(target.deps.iter().cloned());
            seen.push(current);
        }
        seen
    }
}

/// A test of ctest's listing.
struct Registration {
    name: String,
    command: Vec<String>,
    /// The command's words and the environment's values.
    words: Vec<String>,
    setups: Vec<String>,
    required: Vec<String>,
    /// Its command and properties as JSON.
    text: String,
}

fn registrations(listing: &str) -> Result<Vec<Registration>, String> {
    let root: Value =
        serde_json::from_str(listing).map_err(|e| format!("ctest's JSON does not parse: {e}"))?;
    let tests = root
        .get("tests")
        .and_then(Value::as_array)
        .ok_or("ctest's JSON has no tests")?;
    let mut out = Vec::new();
    for test in tests {
        let Some(name) = test.get("name").and_then(Value::as_str) else {
            continue;
        };
        let command = test.get("command").map(strings).unwrap_or_default();
        let properties = test
            .get("properties")
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));
        let property = |wanted: &str| -> Vec<String> {
            let list = properties.as_array().into_iter().flatten();
            list.filter(|p| p.get("name").and_then(Value::as_str) == Some(wanted))
                .filter_map(|p| p.get("value").map(strings))
                .flatten()
                .collect()
        };
        let mut words = command.clone();
        for variable in property("ENVIRONMENT") {
            if let Some((_, value)) = variable.split_once('=') {
                words.push(value.to_owned());
            }
        }
        let setups = property("FIXTURES_SETUP");
        let required = property("FIXTURES_REQUIRED");
        let registration = serde_json::json!({
            "command": test.get("command").cloned().unwrap_or(Value::Array(Vec::new())),
            "properties": properties,
        });
        out.push(Registration {
            name: name.to_owned(),
            words,
            setups,
            required,
            command,
            text: registration.to_string(),
        });
    }
    Ok(out)
}

/// A cargo-tests.cmake `build` of the crates' test binaries: a fixture
/// whose fingerprint does not count for the tests requiring it.
fn crate_build(command: &[String]) -> bool {
    let named = command.iter().any(|w| base_name(w) == "cargo-tests.cmake");
    let at = command.iter().position(|w| w == "--");
    named && at.is_some_and(|at| command.get(at + 1).map(String::as_str) == Some("build"))
}

/// The fingerprints of a build's tests.
pub struct Fingerprints {
    tree: Tree,
    cmake: CMake,
    /// Read on first use (`cargo metadata`).
    cargo: Option<Cargo>,
    tests: Vec<Registration>,
    index: HashMap<String, usize>,
    /// Each fixture's setup tests.
    setups: HashMap<String, Vec<String>>,
    own: HashMap<String, Vec<String>>,
    targets: HashMap<String, Vec<String>>,
    dirs: HashMap<String, Vec<String>>,
    build_lines: Option<Vec<String>>,
    whole: Option<Vec<String>>,
}

impl Fingerprints {
    /// The fingerprints of the tests of `listing` (ctest's
    /// `--show-only=json-v1` of the whole build) in the build folder: its
    /// source and build trees from CMakeCache.txt, CMake's file API reply.
    pub fn load(build: &Path, listing: &str) -> Result<Fingerprints, String> {
        let cache = fs::read_to_string(build.join("CMakeCache.txt"))
            .map_err(|e| format!("no CMakeCache.txt in {}: {e}", build.display()))?;
        let source = cache_value(&cache, "CMAKE_HOME_DIRECTORY")
            .ok_or("CMakeCache.txt names no CMAKE_HOME_DIRECTORY")?;
        let absolute = std::path::absolute(build).unwrap_or_else(|_| build.to_path_buf());
        let build_dir = cache_value(&cache, "CMAKE_CACHEFILE_DIR")
            .unwrap_or_else(|| absolute.to_string_lossy().into_owned());
        let mut tree = Tree::new(&source, &build_dir);
        let cmake = CMake::load(build, &tree.build.clone(), &mut tree);
        let tests = registrations(listing)?;
        // The files tests name.
        for test in &tests {
            for word in &test.words {
                let value = path_value(word);
                if is_absolute(value)
                    && Path::new(value).is_file()
                    && let Some(rel) = tree.rel(value)
                {
                    tree.test_files.insert(key(&rel));
                }
            }
        }
        let index = tests
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.clone(), i))
            .collect();
        // The fixtures' setups, not those that build the crates' test
        // binaries (a crate's test depends on its own closure).
        let mut setups: HashMap<String, Vec<String>> = HashMap::new();
        for test in tests.iter().filter(|t| !crate_build(&t.command)) {
            for fixture in &test.setups {
                setups
                    .entry(fixture.clone())
                    .or_default()
                    .push(test.name.clone());
            }
        }
        Ok(Fingerprints {
            tree,
            cmake,
            cargo: None,
            tests,
            index,
            setups,
            own: HashMap::new(),
            targets: HashMap::new(),
            dirs: HashMap::new(),
            build_lines: None,
            whole: None,
        })
    }

    /// Uses this `cargo metadata` output (None: none) instead of running
    /// cargo.
    pub fn with_cargo_metadata(mut self, metadata: Option<&Value>) -> Fingerprints {
        self.cargo = Some(Cargo::from_metadata(metadata));
        self
    }

    /// Why the fingerprints cover more than the tests' own sources.
    pub fn note(&self) -> Option<String> {
        (!self.cmake.ok).then(|| {
            "no CMake file API reply in the build folder: every fingerprint covers the whole tree"
                .to_owned()
        })
    }

    fn crate_closure(&mut self, name: &str, dev: bool) -> Option<Vec<String>> {
        if self.cargo.is_none() {
            self.cargo = Some(Cargo::from_metadata(
                Cargo::metadata(&self.tree.source).as_ref(),
            ));
        }
        let cargo = self.cargo.as_mut()?;
        cargo.closure(&mut self.tree, name, dev)
    }

    fn build_lines(&mut self) -> Vec<String> {
        if let Some(lines) = &self.build_lines {
            return lines.clone();
        }
        let mut lines = Vec::new();
        for f in BUILD_FILES {
            let files = self.tree.files(f, ALL);
            lines.extend(self.tree.lines(&files));
        }
        self.build_lines = Some(lines.clone());
        lines
    }

    fn dir_lines(&mut self, rel: &str) -> Vec<String> {
        if let Some(lines) = self.dirs.get(rel) {
            return lines.clone();
        }
        let lines = if rel == "." {
            self.build_lines()
        } else {
            let walk = Walk {
                nested: false,
                test_files: false,
                ..ALL
            };
            let files = self.tree.files(rel, walk);
            self.tree.lines(&files)
        };
        self.dirs.insert(rel.to_owned(), lines.clone());
        lines
    }

    /// The lines of what a target is built from, through its dependencies.
    fn target_lines(&mut self, id: &str) -> Vec<String> {
        if let Some(lines) = self.targets.get(id) {
            return lines.clone();
        }
        let mut lines: BTreeSet<String> = self.cmake.toolchain.iter().cloned().collect();
        lines.extend(self.cmake.options.iter().cloned());
        for current in self.cmake.closure(id) {
            let Some(t) = self.cmake.targets.get(&current).cloned() else {
                continue;
            };
            lines.insert(format!("target {} {} {}", t.name, t.kind, t.dir));
            lines.extend(self.dir_lines(&t.dir));
            for s in &t.sources {
                let path = join(&self.tree.source, s);
                if let Some(rel) = self.tree.rel(&path) {
                    let d = self.tree.digest(&rel);
                    lines.insert(format!("{rel} {d}"));
                }
            }
            let name = t.name.strip_prefix('_').unwrap_or(&t.name);
            if let Some(crate_name) = name.strip_prefix("cargo-build_") {
                match self.crate_closure(crate_name, false) {
                    Some(crate_lines) => lines.extend(crate_lines),
                    None => {
                        lines.insert(format!("unknown crate {crate_name}"));
                    }
                }
            }
        }
        lines.extend(self.build_lines());
        let lines: Vec<String> = lines.into_iter().collect();
        self.targets.insert(id.to_owned(), lines.clone());
        lines
    }

    fn whole_tree(&mut self) -> Vec<String> {
        if let Some(lines) = &self.whole {
            return lines.clone();
        }
        let files = self.tree.files(".", ALL);
        let lines = self.tree.lines(&files);
        self.whole = Some(lines.clone());
        lines
    }

    /// The lines of a test itself, without its fixtures.
    fn own_lines(&mut self, at: usize) -> Vec<String> {
        let name = self.tests[at].name.clone();
        if let Some(lines) = self.own.get(&name) {
            return lines.clone();
        }
        let command = self.tests[at].command.clone();
        let words = self.tests[at].words.clone();
        let mut lines = vec![format!(
            "test {}",
            self.tree.strip_roots(&self.tests[at].text)
        )];
        let first = command.first().map_or("", String::as_str);
        let mut known = false;
        let packages = cargo_packages(&command);
        if !packages.is_empty() {
            known = true;
            for package in &packages {
                match self.crate_closure(package, true) {
                    Some(crate_lines) => lines.extend(crate_lines),
                    None => lines.push(format!("unknown crate {package}")),
                }
            }
        }
        if WHOLE_TREE.contains(&name.as_str()) {
            known = false;
        } else if !first.is_empty() && program(first) == "cargo-deny" {
            // The dependency policy: the manifests, the lock and the policy.
            known = true;
            let policy = ["Cargo.toml", "Cargo.lock", "deny.toml"];
            let files: Vec<String> = self
                .tree
                .files("core", ALL)
                .into_iter()
                .filter(|f| policy.contains(&base_name(f)))
                .collect();
            lines.extend(self.tree.lines(&files));
        } else if !first.is_empty()
            && (self.cmake.artifacts.contains_key(&key(&norm(first)))
                || ["cmake", "ctest", "cargo"].contains(&program(first)))
        {
            known = true;
        }
        let script = !first.is_empty() && interpreter(first);
        if script && !WHOLE_TREE.contains(&name.as_str()) {
            known = true;
        }
        for word in &words {
            let value = path_value(word);
            if !is_absolute(value) {
                continue;
            }
            let path = norm(value);
            if let Some(id) = self.cmake.artifacts.get(&key(&path)).cloned() {
                lines.extend(self.target_lines(&id));
                continue;
            }
            let Some(rel) = self.tree.rel(&path).filter(|r| r != ".") else {
                continue;
            };
            let files = self.tree.files(&rel, ALL);
            lines.extend(self.tree.lines(&files));
            if script && (path.ends_with(".py") || path.ends_with(".sh")) {
                let folder = rel.rsplit_once('/').map_or(".", |(d, _)| d);
                let walk = Walk { flat: true, ..ALL };
                let beside = self.tree.files(folder, walk);
                lines.extend(self.tree.lines(&beside));
            }
        }
        if !known || !self.cmake.ok {
            lines.extend(self.whole_tree());
        }
        self.own.insert(name, lines.clone());
        lines
    }

    /// The test's lines with the fingerprints of the fixtures it requires.
    fn with_fixtures(&mut self, at: usize, seen: &mut HashSet<String>) -> Vec<String> {
        let mut lines = self.own_lines(at);
        let name = self.tests[at].name.clone();
        for fixture in self.tests[at].required.clone() {
            for setup in self.setups.get(&fixture).cloned().unwrap_or_default() {
                if setup == name || seen.contains(&setup) {
                    continue;
                }
                seen.insert(setup.clone());
                let Some(&other) = self.index.get(&setup) else {
                    continue;
                };
                let theirs = self.with_fixtures(other, seen).join("\n");
                lines.push(format!(
                    "fixture {fixture} {setup} {}",
                    digest(theirs.as_bytes())
                ));
            }
        }
        let lines: BTreeSet<String> = lines.into_iter().collect();
        lines.into_iter().collect()
    }

    /// What the test's fingerprint is made of, a line each.
    pub fn explain(&mut self, name: &str) -> Option<Vec<String>> {
        let at = *self.index.get(name)?;
        let mut seen = HashSet::from([name.to_owned()]);
        Some(self.with_fixtures(at, &mut seen))
    }

    /// The test's fingerprint; None for a test the listing does not have.
    pub fn fingerprint(&mut self, name: &str) -> Option<String> {
        let lines = self.explain(name)?;
        Some(digest(lines.join("\n").as_bytes()))
    }
}

/// A file name for a test.
fn file_name(name: &str) -> String {
    let keep = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    name.chars()
        .map(|c| if keep(c) { c } else { '_' })
        .collect()
}

/// The records of a test's file: (fingerprint, release), oldest first.
pub fn parse_records(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (fingerprint, release) = line.trim_end().split_once('\t')?;
            (!fingerprint.is_empty()).then(|| (fingerprint.to_owned(), release.to_owned()))
        })
        .collect()
}

/// The records with a pass of `fingerprint` in `release`: its earlier line
/// goes, the new one is the last, and only the last `KEEP` stay.
pub fn updated(text: &str, fingerprint: &str, release: &str) -> String {
    let mut records: Vec<(String, String)> = parse_records(text)
        .into_iter()
        .filter(|(f, _)| f != fingerprint)
        .collect();
    records.push((fingerprint.to_owned(), release.to_owned()));
    let from = records.len().saturating_sub(KEEP);
    records[from..]
        .iter()
        .map(|(f, r)| format!("{f}\t{r}\n"))
        .collect()
}

/// The passes of one platform's release gates.
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The platform's folder in the folder of the passes.
    pub fn new(base: &Path, platform: &str) -> Store {
        Store {
            dir: base.join(platform),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, test: &str) -> PathBuf {
        self.dir.join(file_name(test))
    }

    /// Any test has a pass recorded.
    pub fn has_records(&self) -> bool {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return false;
        };
        entries
            .filter_map(Result::ok)
            .any(|e| e.file_type().is_ok_and(|t| t.is_file()))
    }

    /// The release in which the test last passed with this fingerprint.
    pub fn last_pass(&self, test: &str, fingerprint: &str) -> Option<String> {
        let text = fs::read_to_string(self.path(test)).ok()?;
        parse_records(&text)
            .into_iter()
            .rev()
            .find(|(f, _)| f == fingerprint)
            .map(|(_, release)| release)
    }

    /// Keeps a pass of the test with this fingerprint in `release`.
    pub fn record(&self, test: &str, fingerprint: &str, release: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let path = self.path(test);
        let text = fs::read_to_string(&path).unwrap_or_default();
        let new = self.dir.join(format!("{}.new", file_name(test)));
        fs::write(&new, updated(&text, fingerprint, release))?;
        fs::rename(&new, &path)
    }
}

/// A test the release skips.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skip {
    pub name: String,
    pub fingerprint: String,
    /// The release and RC in which it last passed with that fingerprint.
    pub passed_in: String,
}

/// What a release gate's run does with the group's tests.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    pub platform: String,
    /// The release and RC of this gate.
    pub release: String,
    /// The group's tests in the run's selection.
    pub candidates: usize,
    pub skipped: Vec<Skip>,
    /// The tests that run with their fingerprints, kept when they pass.
    pub run: Vec<(String, String)>,
    /// No pass recorded on the platform yet.
    pub first: bool,
    /// `--all-geometry`: every test of the group runs.
    pub forced: bool,
    pub note: Option<String>,
    /// After the run: how many passes were kept.
    pub recorded: Option<String>,
}

impl Plan {
    pub fn skips(&self, name: &str) -> bool {
        self.skipped.iter().any(|s| s.name == name)
    }

    /// The decision in a line.
    pub fn summary(&self) -> String {
        let n = self.candidates;
        let mut line = if self.forced {
            format!("all {n} {GROUP} tests run (--all-geometry)")
        } else if self.first {
            format!(
                "all {n} {GROUP} tests run: first release with fingerprints on {}",
                self.platform
            )
        } else {
            let skipped = self.skipped.len();
            format!(
                "{skipped} of {n} {GROUP} tests skipped (passed with the same fingerprint in an \
                 earlier release gate on {}), {} run",
                self.platform,
                n - skipped
            )
        };
        if let Some(note) = &self.note {
            line.push_str("; ");
            line.push_str(note);
        }
        line
    }

    /// The report's lines: the summary, each skipped test with the release
    /// it last passed in, and after the run the passes kept.
    pub fn text(&self) -> String {
        let mut out = format!("Release fingerprints: {}\n", self.summary());
        for s in &self.skipped {
            let _ = writeln!(out, "  Skipped {} (last passed in {})", s.name, s.passed_in);
        }
        if let Some(recorded) = &self.recorded {
            let _ = writeln!(out, "Release fingerprints kept: {recorded}");
        }
        out
    }

    /// The table of the skipped tests (`SKIPPED_TABLE`).
    pub fn table(&self) -> String {
        let mut out = String::from("name\tfingerprint\tpassed_in\n");
        for s in &self.skipped {
            let _ = writeln!(out, "{}\t{}\t{}", s.name, s.fingerprint, s.passed_in);
        }
        out
    }
}

/// Which of the group's tests run: `fingerprints` gives each candidate's
/// (None: none could be computed, the test runs and nothing is kept); a
/// test is skipped when it passed with its fingerprint in an earlier
/// release gate of the store's platform, unless `forced`.
pub fn decide(
    store: &Store,
    platform: &str,
    release: &str,
    fingerprints: &[(String, Option<String>)],
    forced: bool,
    note: Option<String>,
) -> Plan {
    let first = !store.has_records();
    let mut plan = Plan {
        platform: platform.to_owned(),
        release: release.to_owned(),
        candidates: fingerprints.len(),
        first,
        forced,
        note,
        ..Plan::default()
    };
    for (name, fingerprint) in fingerprints {
        let Some(fingerprint) = fingerprint else {
            continue;
        };
        let last = if forced || first {
            None
        } else {
            store.last_pass(name, fingerprint)
        };
        match last {
            Some(passed_in) => plan.skipped.push(Skip {
                name: name.clone(),
                fingerprint: fingerprint.clone(),
                passed_in,
            }),
            None => plan.run.push((name.clone(), fingerprint.clone())),
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder of the test's own.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mitcad-testrun-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn text(path: &Path) -> String {
        norm(&path.to_string_lossy())
    }

    /// A checkout and its build folder: a geometry library and its test
    /// program, the OCCT port's patch, a document, another folder, and the
    /// file API's reply for them.
    fn checkout(root: &Path, compiler: &str) -> (PathBuf, String) {
        let source = root.join("src");
        let build = source.join("build").join("dev");
        let (s, b) = (text(&source), text(&build));
        write(
            &source.join("CMakeLists.txt"),
            "project(x\n  VERSION 0.62.0\n)\n",
        );
        write(
            &source.join("vcpkg.json"),
            "{\n  \"name\": \"x\",\n  \"version\": \"0.62.0\",\n}\n",
        );
        write(
            &source.join("third_party/vcpkg-ports/opencascade/0001-fix.patch"),
            "--- a\n+++ b\n",
        );
        write(&source.join("geometry/CMakeLists.txt"), "add_library(g)\n");
        write(
            &source.join("geometry/src/fillet.cpp"),
            "int fillet() { return 1; }\n",
        );
        write(
            &source.join("geometry/tests/test_sheet.cpp"),
            "int main() {}\n",
        );
        write(&source.join("geometry/NOTES.md"), "notes\n");
        write(&source.join("app/main.cpp"), "int main() {}\n");
        write(
            &build.join("CMakeCache.txt"),
            &format!(
                "CMAKE_HOME_DIRECTORY:INTERNAL={s}\nCMAKE_CACHEFILE_DIR:INTERNAL={b}\n\
                 CMAKE_BUILD_TYPE:STRING=RelWithDebInfo\nMITCAD_RENDER:BOOL=ON\nOTHER:PATH={s}\n"
            ),
        );
        let reply = build.join(".cmake/api/v1/reply");
        write(
            &reply.join("index-1.json"),
            r#"{ "objects": [
                { "kind": "codemodel", "jsonFile": "codemodel-v2-1.json" },
                { "kind": "toolchains", "jsonFile": "toolchains-v1-1.json" } ] }"#,
        );
        write(
            &reply.join("codemodel-v2-1.json"),
            r#"{ "configurations": [ {
                "directories": [ { "source": "." }, { "source": "geometry" }, { "source": "app" } ],
                "targets": [
                  { "jsonFile": "target-geometry.json", "directoryIndex": 1 },
                  { "jsonFile": "target-test.json", "directoryIndex": 1 },
                  { "jsonFile": "target-app.json", "directoryIndex": 2 } ] } ] }"#,
        );
        write(
            &reply.join("target-geometry.json"),
            r#"{ "id": "mitcad_geometry::@1", "name": "mitcad_geometry", "type": "STATIC_LIBRARY",
                 "sources": [ { "path": "geometry/src/fillet.cpp" },
                              { "path": "build/dev/geometry/gen.cpp", "isGenerated": true } ],
                 "artifacts": [ { "path": "geometry/libmitcad_geometry.a" } ] }"#,
        );
        write(
            &reply.join("target-test.json"),
            r#"{ "id": "test_sheet::@1", "name": "test_sheet", "type": "EXECUTABLE",
                 "dependencies": [ { "id": "mitcad_geometry::@1" } ],
                 "sources": [ { "path": "geometry/tests/test_sheet.cpp" } ],
                 "artifacts": [ { "path": "geometry/test_sheet" } ] }"#,
        );
        write(
            &reply.join("target-app.json"),
            r#"{ "id": "mitcad::@2", "name": "mitcad", "type": "EXECUTABLE",
                 "sources": [ { "path": "app/main.cpp" } ],
                 "artifacts": [ { "path": "app/mitcad" } ] }"#,
        );
        write(
            &reply.join("toolchains-v1-1.json"),
            &format!(
                r#"{{ "toolchains": [ {{ "language": "CXX",
                     "compiler": {{ "id": "GNU", "version": "{compiler}" }} }} ] }}"#
            ),
        );
        let listing = format!(
            r#"{{ "tests": [
              {{ "name": "geometry.sheet", "command": [ "{b}/geometry/test_sheet" ],
                 "properties": [ {{ "name": "WORKING_DIRECTORY", "value": "{b}/geometry" }},
                                 {{ "name": "LABELS", "value": [ "geometry" ] }} ] }},
              {{ "name": "app.smoke", "command": [ "{b}/app/mitcad", "--smoke" ] }} ] }}"#
        );
        (build, listing)
    }

    fn sheet(build: &Path, listing: &str) -> String {
        let fingerprints = Fingerprints::load(build, listing).unwrap();
        let mut fingerprints = fingerprints.with_cargo_metadata(None);
        fingerprints.fingerprint("geometry.sheet").unwrap()
    }

    #[test]
    fn digests_and_includes() {
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let rust = "const A: &str = include_str!(\"../commands.md\");\n\
                    static B: &[u8] = include_bytes! ( \"data/x.bin\" );\nfn include_x() {}";
        assert_eq!(included_files(rust), ["../commands.md", "data/x.bin"]);
        let cache = "MITCAD_RENDER:BOOL=ON\nMITCAD_DIR:PATH=/x\nCMAKE_BUILD_TYPE:STRING=Debug\n\
                     mitcad_x:BOOL=ON\nMITCAD_:BOOL=OFF\n";
        assert_eq!(
            cache_options(cache),
            [
                "option CMAKE_BUILD_TYPE:STRING=Debug",
                "option MITCAD_RENDER:BOOL=ON"
            ]
        );
        assert!(interpreter("/usr/bin/python3.12") && interpreter("C:/Git/bin/bash.exe"));
        assert!(!interpreter("/usr/bin/pythonista") && !interpreter("test_sheet"));
    }

    #[test]
    fn the_release_version_is_left_out() {
        let same = |rel: &str, a: &str, b: &str| {
            versionless(rel, a.as_bytes()).unwrap() == versionless(rel, b.as_bytes()).unwrap()
        };
        let cmake = |v: &str| format!("project(mitcad\r\n  VERSION {v}\r\n  LANGUAGES CXX)\r\n");
        assert!(same("CMakeLists.txt", &cmake("0.62.0"), &cmake("0.63.0")));
        assert!(!same(
            "CMakeLists.txt",
            &cmake("0.62.0"),
            "project(mitcad)\r\n"
        ));
        let cargo = |v: &str, other: &str| {
            format!(
                "[workspace.package]\nversion = \"{v}\"\n[dependencies]\nversion = \"{other}\"\n"
            )
        };
        assert!(same(
            "core/Cargo.toml",
            &cargo("1.0.0", "2"),
            &cargo("1.1.0", "2")
        ));
        assert!(!same(
            "core/Cargo.toml",
            &cargo("1.0.0", "2"),
            &cargo("1.0.0", "3")
        ));
        let lock = |own: &str, external: &str| {
            format!(
                "[[package]]\nname = \"mitcad-model\"\nversion = \"{own}\"\n\n\
                 [[package]]\nname = \"serde\"\nversion = \"{external}\"\n\
                 source = \"registry+https://github.com/rust-lang/crates.io-index\"\n"
            )
        };
        assert!(same(
            "core/Cargo.lock",
            &lock("0.62.0", "1.0.1"),
            &lock("0.63.0", "1.0.1")
        ));
        assert!(!same(
            "core/Cargo.lock",
            &lock("0.62.0", "1.0.1"),
            &lock("0.62.0", "1.0.2")
        ));
        let text = versionless("core/Cargo.lock", lock("0.62.0", "1.0.1").as_bytes()).unwrap();
        assert!(text.contains("version = \"<version>\"\n") && text.contains("1.0.1"));
        // Other files keep their versions.
        assert!(versionless("geometry/CMakeLists.txt", b"  VERSION 1\n").is_none());
    }

    #[test]
    fn an_unchanged_input_keeps_the_fingerprint() {
        let root = scratch("fp-same");
        let (build, listing) = checkout(&root, "13.2.0");
        let first = sheet(&build, &listing);
        assert_eq!(first.len(), 64);
        assert_eq!(sheet(&build, &listing), first);
        // Neither a document nor a folder the test is not built from counts.
        write(&root.join("src/geometry/NOTES.md"), "other notes\n");
        write(&root.join("src/app/main.cpp"), "int main() { return 2; }\n");
        assert_eq!(sheet(&build, &listing), first);
        // Nor does the release's version (release.sh version).
        write(
            &root.join("src/CMakeLists.txt"),
            "project(x\n  VERSION 0.63.0\n)\n",
        );
        write(
            &root.join("src/vcpkg.json"),
            "{\n  \"name\": \"x\",\n  \"version\": \"0.63.0\",\n}\n",
        );
        assert_eq!(sheet(&build, &listing), first);
        // But another change of a build file does.
        write(
            &root.join("src/CMakeLists.txt"),
            "project(x\n  VERSION 0.63.0\n)\nadd_subdirectory(y)\n",
        );
        assert_ne!(sheet(&build, &listing), first);
        write(
            &root.join("src/CMakeLists.txt"),
            "project(x\n  VERSION 0.63.0\n)\n",
        );
        // Another checkout of the same tree has the same fingerprints.
        let other = scratch("fp-same-elsewhere");
        let (other_build, other_listing) = checkout(&other, "13.2.0");
        write(&other.join("src/geometry/NOTES.md"), "other notes\n");
        write(
            &other.join("src/app/main.cpp"),
            "int main() { return 2; }\n",
        );
        assert_eq!(sheet(&other_build, &other_listing), first);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn a_changed_input_changes_the_fingerprint() {
        let root = scratch("fp-changed");
        let (build, listing) = checkout(&root, "13.2.0");
        let first = sheet(&build, &listing);
        // A geometry source file.
        write(
            &root.join("src/geometry/src/fillet.cpp"),
            "int fillet() { return 2; }\n",
        );
        let second = sheet(&build, &listing);
        assert_ne!(second, first);
        // An OCCT patch.
        write(
            &root.join("src/third_party/vcpkg-ports/opencascade/0001-fix.patch"),
            "--- a\n+++ b\n@@ fixed @@\n",
        );
        let third = sheet(&build, &listing);
        assert_ne!(third, second);
        // The compiler's version.
        let (build, listing) = checkout(&root, "13.3.0");
        write(
            &root.join("src/geometry/src/fillet.cpp"),
            "int fillet() { return 2; }\n",
        );
        write(
            &root.join("src/third_party/vcpkg-ports/opencascade/0001-fix.patch"),
            "--- a\n+++ b\n@@ fixed @@\n",
        );
        let fourth = sheet(&build, &listing);
        assert_ne!(fourth, third);
        // An option of the build.
        let cache = build.join("CMakeCache.txt");
        let options = fs::read_to_string(&cache)
            .unwrap()
            .replace(":BOOL=ON", ":BOOL=OFF");
        write(&cache, &options);
        assert_ne!(sheet(&build, &listing), fourth);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn without_the_reply_the_whole_tree_counts() {
        let root = scratch("fp-whole");
        let (build, listing) = checkout(&root, "13.2.0");
        fs::remove_dir_all(build.join(".cmake")).unwrap();
        let fingerprints = Fingerprints::load(&build, &listing).unwrap();
        assert!(fingerprints.note().is_some());
        let first = sheet(&build, &listing);
        write(&root.join("src/app/main.cpp"), "int main() { return 3; }\n");
        assert_ne!(sheet(&build, &listing), first);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fixtures_count_for_the_tests_requiring_them() {
        let root = scratch("fp-fixture");
        let (build, _) = checkout(&root, "13.2.0");
        let b = text(&build);
        let listing = |setup_arg: &str| {
            format!(
                r#"{{ "tests": [
                  {{ "name": "geometry.sheet", "command": [ "{b}/geometry/test_sheet" ],
                     "properties": [ {{ "name": "FIXTURES_REQUIRED", "value": [ "sheets" ] }} ] }},
                  {{ "name": "geometry.make_sheets", "command": [ "{b}/app/mitcad", "{setup_arg}" ],
                     "properties": [ {{ "name": "FIXTURES_SETUP", "value": [ "sheets" ] }} ] }} ] }}"#
            )
        };
        let first = sheet(&build, &listing("--a"));
        assert_ne!(sheet(&build, &listing("--b")), first);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn records_keep_the_last_release_of_each_fingerprint() {
        let text = updated("", "f1", "0.60.0-rc1");
        let text = updated(&text, "f2", "0.61.0-rc2");
        let text = updated(&text, "f1", "0.62.0-rc1");
        assert_eq!(
            parse_records(&text),
            [
                ("f2".to_owned(), "0.61.0-rc2".to_owned()),
                ("f1".to_owned(), "0.62.0-rc1".to_owned())
            ]
        );
        let mut many = String::new();
        for i in 0..30 {
            many = updated(&many, &format!("f{i}"), "r");
        }
        let kept = parse_records(&many);
        assert_eq!(kept.len(), KEEP);
        assert_eq!(kept[0].0, "f10");
        assert_eq!(file_name("geometry.suite.fillet"), "geometry.suite.fillet");
        assert_eq!(file_name("a/b c"), "a_b_c");
    }

    #[test]
    fn an_equal_fingerprint_is_skipped_and_a_changed_one_runs() {
        let base = scratch("fp-store");
        let store = Store::new(&base, "linux");
        let fps = |a: &str, b: &str| {
            vec![
                ("geometry.sheet".to_owned(), Some(a.to_owned())),
                ("geometry.suite.fillet".to_owned(), Some(b.to_owned())),
                ("geometry.unknown".to_owned(), None),
            ]
        };
        // The first release with fingerprints runs them all.
        let plan = decide(&store, "linux", "0.63.0-rc1", &fps("a1", "b1"), false, None);
        assert!(plan.first && plan.skipped.is_empty());
        assert_eq!(plan.run.len(), 2);
        assert_eq!(plan.candidates, 3);
        assert!(plan.summary().contains("first release with fingerprints"));
        for (name, fingerprint) in &plan.run {
            store.record(name, fingerprint, "0.63.0-rc1").unwrap();
        }
        // The next: equal fingerprints are skipped, a changed one runs.
        let plan = decide(&store, "linux", "0.64.0-rc1", &fps("a1", "b2"), false, None);
        assert!(!plan.first);
        assert_eq!(
            plan.skipped,
            [Skip {
                name: "geometry.sheet".to_owned(),
                fingerprint: "a1".to_owned(),
                passed_in: "0.63.0-rc1".to_owned()
            }]
        );
        assert!(plan.skips("geometry.sheet") && !plan.skips("geometry.suite.fillet"));
        assert_eq!(
            plan.run,
            [("geometry.suite.fillet".to_owned(), "b2".to_owned())]
        );
        assert!(plan.summary().starts_with("1 of 3 geometry tests skipped"));
        assert!(
            plan.text()
                .contains("  Skipped geometry.sheet (last passed in 0.63.0-rc1)")
        );
        assert_eq!(
            plan.table(),
            "name\tfingerprint\tpassed_in\ngeometry.sheet\ta1\t0.63.0-rc1\n"
        );
        // --all-geometry runs them all.
        let plan = decide(&store, "linux", "0.64.0-rc1", &fps("a1", "b2"), true, None);
        assert!(plan.skipped.is_empty() && plan.run.len() == 2);
        assert!(plan.summary().contains("--all-geometry"));
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn the_platforms_records_do_not_mix() {
        let base = scratch("fp-platforms");
        let linux = Store::new(&base, "linux");
        let windows = Store::new(&base, "windows");
        linux.record("geometry.sheet", "a1", "0.63.0-rc1").unwrap();
        assert!(linux.has_records() && !windows.has_records());
        assert_eq!(
            linux.last_pass("geometry.sheet", "a1").as_deref(),
            Some("0.63.0-rc1")
        );
        assert_eq!(windows.last_pass("geometry.sheet", "a1"), None);
        let fps = [("geometry.sheet".to_owned(), Some("a1".to_owned()))];
        let plan = decide(&windows, "windows", "0.64.0-rc1", &fps, false, None);
        assert!(plan.first && plan.skipped.is_empty());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn only_the_group_without_fixture_roles_is_a_candidate() {
        let def = |name: &str| TestDef {
            name: name.to_owned(),
            ..TestDef::default()
        };
        assert!(candidate(&def("geometry.sheet")));
        assert!(candidate(&def("geometry.suite.fillet")));
        assert!(!candidate(&def("core.model")));
        assert!(!candidate(&def("app.ui-windows-geometry")));
        let mut setup = def("geometry.prepare");
        setup.fixtures_setup = vec!["g".to_owned()];
        assert!(!candidate(&setup));
    }
}
