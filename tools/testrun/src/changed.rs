// SPDX-License-Identifier: MIT
//! The tests related to the files a change touches (mitcad#790, roadmap
//! step 4). `--changed FILE` lists the changed paths, a line each, as
//! `git diff --name-only <base>` prints them; the tests whose sources cover
//! one of them join the priority lane with a failure's raised place
//! (`CHANGE_SCORE` added to their failure score for this run).
//!
//! A test's sources, found from its ctest registration and the tree:
//!
//! - the files and folders of the source tree that its command and its
//!   environment name (its script, a cli test's JSON, a `cmake -P` script,
//!   a folder of tests), not those in the build folder;
//! - the areas each named file declares in `# check-all sources: <path>...`
//!   lines (the UI tests' scripts: `.sh`, `.py`, `.ps1`, `.mitcad-ui`; the
//!   lines `tools/check-all.sh` reads for its UI tests);
//! - for a crate's tests (`cargo-tests.cmake -- run|parts|build <package>`,
//!   `cargo test --doc -p <package>`) the folder of each package, from the
//!   workspace's members (`core/Cargo.toml`);
//! - for a test program that CMake builds (the command's program) the
//!   sources of its own target, from CMake's file API reply in the build
//!   folder where there is one (`tools/check-all.sh` asks for it).
//!
//! A source covers a changed path that is that file or lies in that
//! folder. Only these direct sources count, not what they depend on in
//! turn, so that a change to a crate every test uses does not put every
//! test in the lane.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::ctest::TestDef;

/// What a related test gets added to its failure score for the run: a
/// failure's (profile.rs), so that it goes into the lane as a test that
/// failed once would.
pub const CHANGE_SCORE: f64 = 10.0;

/// The workspace manifest whose members are the crates, in the source
/// tree.
pub const WORKSPACE: &str = "core/Cargo.toml";

/// The line in which a script declares the areas it depends on.
const DECLARED: &str = "# check-all sources:";

/// The largest named file read for its declared sources.
const MAX_SCRIPT: u64 = 1 << 20;

/// What the mapping knows of the tree.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    /// The source tree, absolute (`norm`).
    pub source: String,
    /// The build tree, absolute (`norm`).
    pub build: String,
    /// Each workspace package's folder in the source tree.
    pub crates: HashMap<String, String>,
    /// Each artifact CMake builds (by `key` of its absolute path) with its
    /// own target's sources in the source tree.
    pub targets: HashMap<String, Vec<String>>,
}

/// A path as it is compared: on Windows without case.
pub fn key(path: &str) -> String {
    if cfg!(windows) {
        path.to_ascii_lowercase()
    } else {
        path.to_owned()
    }
}

/// An absolute path: `/...`, `\...` or `C:/...`.
pub fn is_absolute(path: &str) -> bool {
    let b = path.as_bytes();
    let drive = b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':';
    path.starts_with(['/', '\\']) || (drive && (b[2] == b'/' || b[2] == b'\\'))
}

/// The path with forward slashes, without `.`, `..` (where it can go back),
/// doubled or trailing slashes; `.` for an empty relative path.
pub fn norm(path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                let back = parts
                    .last()
                    .is_some_and(|p| *p != ".." && !p.ends_with(':'));
                if back {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            _ => parts.push(part),
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_owned()
    } else {
        joined
    }
}

/// `path` relative to `root` (both absolute), or None outside it or for
/// the root itself.
pub fn relative(root: &str, path: &str) -> Option<String> {
    let (root, path) = (norm(root), norm(path));
    let (root_key, path_key) = (key(&root), key(&path));
    let rest = path_key.strip_prefix(&root_key)?.strip_prefix('/')?;
    // The keys differ only in case, so the lengths are the path's.
    Some(path[path.len() - rest.len()..].to_owned())
}

/// `path` on `base` unless it is absolute.
pub fn join(base: &str, path: &str) -> String {
    if is_absolute(path) {
        norm(path)
    } else {
        norm(&format!("{base}/{path}"))
    }
}

/// The source (a file or folder of the source tree) covers the changed
/// path: it is that path, or the path lies in it.
pub fn covers(source: &str, changed: &str) -> bool {
    let (source, changed) = (key(source), key(changed));
    let inside = |rest: &str| rest.starts_with('/');
    source != "." && (changed == source || changed.strip_prefix(&source).is_some_and(inside))
}

/// The value of a `-DNAME=value` or `-DNAME:TYPE=value` word, else the
/// word.
pub fn path_value(word: &str) -> &str {
    if let Some(rest) = word.strip_prefix("-D")
        && let Some((name, value)) = rest.split_once('=')
    {
        let name = name.split(':').next().unwrap_or(name);
        let plain = |c: char| c.is_ascii_alphanumeric() || c == '_';
        if !name.is_empty() && name.chars().all(plain) {
            return value;
        }
    }
    word
}

/// The areas a script declares in its `# check-all sources: <path>...`
/// lines (several add up).
pub fn declared_sources(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim_end().strip_prefix(DECLARED))
        .flat_map(str::split_whitespace)
        .map(norm)
        .filter(|path| path != "." && !path.starts_with("../"))
        .collect()
}

/// The file name of a path.
fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// The packages a crate's test runs: `cargo-tests.cmake -- run <package>`,
/// `-- parts <package>`, `-- build [--release] <package>...`, or `cargo
/// test --doc -p <package>...`; none for another command.
pub fn cargo_packages(command: &[String]) -> Vec<String> {
    if command.iter().any(|w| base_name(w) == "cargo-tests.cmake")
        && let Some(at) = command.iter().position(|w| w == "--")
    {
        let rest = &command[at + 1..];
        return match rest.first().map(String::as_str) {
            Some("run" | "parts") => rest.get(1).cloned().into_iter().collect(),
            Some("build") => rest[1..]
                .iter()
                .filter(|w| !w.starts_with('-'))
                .cloned()
                .collect(),
            _ => Vec::new(),
        };
    }
    let program = command.first().map_or("", |w| base_name(w));
    let cargo = program == "cargo" || program == "cargo.exe";
    if cargo && command.iter().any(|w| w == "--doc") {
        return command
            .windows(2)
            .filter(|pair| pair[0] == "-p" || pair[0] == "--package")
            .map(|pair| pair[1].clone())
            .collect();
    }
    Vec::new()
}

/// The `members` of a workspace manifest (paths relative to it; a list
/// over several lines and comments allowed, no globs).
pub fn workspace_members(manifest: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut rest = line;
        if !inside {
            let start = line.trim_start();
            let named = start.starts_with("members") && start.contains('=');
            let Some(open) = line.find('[').filter(|_| named) else {
                continue;
            };
            inside = true;
            rest = &line[open + 1..];
        }
        let (items, done) = match rest.find(']') {
            Some(close) => (&rest[..close], true),
            None => (rest, false),
        };
        let names = items.split(',').map(|m| m.trim().trim_matches('"'));
        members.extend(names.filter(|m| !m.is_empty()).map(str::to_owned));
        if done {
            break;
        }
    }
    members
}

/// The package's name in its manifest (`[package]`'s `name`).
pub fn package_name(manifest: &str) -> Option<String> {
    let mut package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            package = line == "[package]";
            continue;
        }
        if package
            && let Some((name, value)) = line.split_once('=')
            && name.trim() == "name"
        {
            return Some(value.trim().trim_matches('"').to_owned());
        }
    }
    None
}

/// A value of CMakeCache.txt (`NAME:TYPE=value`).
pub fn cache_value(cache: &str, name: &str) -> Option<String> {
    cache.lines().find_map(|line| {
        let (head, value) = line.split_once('=')?;
        (head.split(':').next() == Some(name)).then(|| value.trim().to_owned())
    })
}

/// The codemodel's file named in a file API reply index.
pub fn reply_codemodel(index: &str) -> Option<String> {
    let root: Value = serde_json::from_str(index).ok()?;
    let objects = root.get("objects")?.as_array()?;
    let kind = |o: &&Value| o.get("kind").and_then(Value::as_str) == Some("codemodel");
    let codemodel = objects.iter().find(kind)?;
    codemodel.get("jsonFile")?.as_str().map(str::to_owned)
}

/// The targets' files of a codemodel, of every configuration.
pub fn codemodel_targets(codemodel: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<Value>(codemodel) else {
        return Vec::new();
    };
    let configurations = root.get("configurations").and_then(Value::as_array);
    let mut files = Vec::new();
    for configuration in configurations.into_iter().flatten() {
        let targets = configuration.get("targets").and_then(Value::as_array);
        for target in targets.into_iter().flatten() {
            if let Some(file) = target.get("jsonFile").and_then(Value::as_str) {
                files.push(file.to_owned());
            }
        }
    }
    files
}

/// A target's artifacts (relative to the build tree unless absolute) and
/// its sources that are not generated (relative to the source tree unless
/// absolute), as CMake's file API writes them.
pub fn target_files(target: &str) -> (Vec<String>, Vec<String>) {
    let Ok(root) = serde_json::from_str::<Value>(target) else {
        return (Vec::new(), Vec::new());
    };
    let items = |name: &str| {
        let list = root.get(name).and_then(Value::as_array);
        list.into_iter().flatten().collect::<Vec<&Value>>()
    };
    let path = |item: &Value| item.get("path").and_then(Value::as_str).map(str::to_owned);
    let generated = |item: &&Value| {
        let flag = item.get("isGenerated").and_then(Value::as_bool);
        flag.unwrap_or(false)
    };
    let artifacts = items("artifacts").into_iter().filter_map(path).collect();
    let sources = items("sources")
        .into_iter()
        .filter(|item| !generated(item))
        .filter_map(path)
        .collect();
    (artifacts, sources)
}

/// The changed paths of the list (`--changed`): a path a line, relative to
/// the source tree or absolute in it; empty lines and lines starting with
/// `#` are left out, as are paths outside the tree.
pub fn changed_paths(list: &str, source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for line in list.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = if is_absolute(line) {
            relative(source, line)
        } else {
            Some(norm(line))
        };
        if let Some(path) = path.filter(|p| p != "." && p != ".." && !p.starts_with("../")) {
            paths.push(path);
        }
    }
    paths
}

/// The test's sources in the source tree (the module's comment). `read`
/// gives the text of a file of the source tree (None for a folder or a
/// file it does not read).
pub fn dependencies(
    def: &TestDef,
    tree: &Tree,
    read: &mut dyn FnMut(&str) -> Option<String>,
) -> Vec<String> {
    let mut sources: Vec<String> = Vec::new();
    // The test program's own target.
    let program = def.command.first().filter(|p| is_absolute(p));
    if let Some(own) = program.and_then(|p| tree.targets.get(&key(&norm(p)))) {
        sources.extend(own.iter().cloned());
    }
    let environment = def.environment.iter().filter_map(|e| e.split_once('='));
    let words = def.command.iter().map(String::as_str);
    for word in words.chain(environment.map(|(_, value)| value)) {
        let value = path_value(word);
        if !is_absolute(value) {
            continue;
        }
        let path = norm(value);
        if key(&path) == key(&tree.build) || relative(&tree.build, &path).is_some() {
            continue;
        }
        let Some(named) = relative(&tree.source, &path) else {
            continue;
        };
        if let Some(text) = read(&named) {
            sources.extend(declared_sources(&text));
        }
        sources.push(named);
    }
    for package in cargo_packages(&def.command) {
        if let Some(folder) = tree.crates.get(&package) {
            sources.push(folder.clone());
        }
    }
    sources.sort();
    sources.dedup();
    sources
}

/// The indices of the tests whose sources cover a changed path.
pub fn related(
    defs: &[&TestDef],
    changed: &[String],
    tree: &Tree,
    read: &mut dyn FnMut(&str) -> Option<String>,
) -> Vec<usize> {
    let mut found = Vec::new();
    if changed.is_empty() {
        return found;
    }
    for (index, def) in defs.iter().enumerate() {
        let sources = dependencies(def, tree, read);
        let hit = |c: &String| sources.iter().any(|s| covers(s, c));
        if changed.iter().any(hit) {
            found.push(index);
        }
    }
    found
}

/// The tests of a run related to its changed files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Related {
    /// The changed paths in the source tree.
    pub files: usize,
    /// The related tests' indices.
    pub tests: Vec<usize>,
    pub names: Vec<String>,
}

impl Related {
    /// A line for the run's output.
    pub fn summary(&self) -> String {
        let files = self.files;
        if self.names.is_empty() {
            return format!("testrun: {files} changed files, no related test");
        }
        let count = self.names.len();
        let names = self.names.join(", ");
        format!("testrun: {files} changed files, {count} related tests: {names}")
    }
}

/// A file of the source tree as text, when it is a file of at most
/// `MAX_SCRIPT` bytes.
fn read_text(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_SCRIPT {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The workspace's packages with their folders.
fn workspace_crates(source: &str) -> HashMap<String, String> {
    let root = Path::new(source);
    let manifest = fs::read_to_string(root.join(WORKSPACE)).unwrap_or_default();
    let base = WORKSPACE.rsplit_once('/').map_or("", |(folder, _)| folder);
    let mut crates = HashMap::new();
    for member in workspace_members(&manifest) {
        let folder = norm(&format!("{base}/{member}"));
        let text = fs::read_to_string(root.join(&folder).join("Cargo.toml"));
        if let Some(name) = text.ok().as_deref().and_then(package_name) {
            crates.insert(name, folder);
        }
    }
    crates
}

/// The artifacts CMake builds with their own targets' sources, from the
/// newest file API reply of the build folder (none without one).
fn cmake_targets(build: &Path, tree: &Tree) -> HashMap<String, Vec<String>> {
    let mut targets = HashMap::new();
    let reply = build.join(".cmake").join("api").join("v1").join("reply");
    let Ok(entries) = fs::read_dir(&reply) else {
        return targets;
    };
    let names = entries.filter_map(Result::ok);
    let names = names.map(|e| e.file_name().to_string_lossy().into_owned());
    let index = names
        .filter(|n| n.starts_with("index-") && n.ends_with(".json"))
        .max();
    let read = |name: &str| fs::read_to_string(reply.join(name)).ok();
    let codemodel = index
        .and_then(|name| read(&name))
        .and_then(|text| reply_codemodel(&text))
        .and_then(|name| read(&name));
    let Some(codemodel) = codemodel else {
        return targets;
    };
    for file in codemodel_targets(&codemodel) {
        let Some(text) = read(&file) else {
            continue;
        };
        let (artifacts, sources) = target_files(&text);
        let in_tree = |s: &String| relative(&tree.source, &join(&tree.source, s));
        let sources: Vec<String> = sources.iter().filter_map(in_tree).collect();
        if sources.is_empty() {
            continue;
        }
        for artifact in artifacts {
            targets.insert(key(&join(&tree.build, &artifact)), sources.clone());
        }
    }
    targets
}

/// The tree of a build folder: its source and build trees from
/// CMakeCache.txt (else the working folder and the build folder), the
/// workspace's crates and CMake's targets.
pub fn load_tree(build: &Path) -> Tree {
    let cache = fs::read_to_string(build.join("CMakeCache.txt")).unwrap_or_default();
    let text = |path: PathBuf| path.to_string_lossy().into_owned();
    let absolute = std::path::absolute(build).unwrap_or_else(|_| build.to_path_buf());
    let build_dir = cache_value(&cache, "CMAKE_CACHEFILE_DIR").unwrap_or_else(|| text(absolute));
    let source = cache_value(&cache, "CMAKE_HOME_DIRECTORY")
        .or_else(|| std::env::current_dir().ok().map(text))
        .unwrap_or_default();
    let mut tree = Tree {
        source: norm(&source),
        build: norm(&build_dir),
        ..Tree::default()
    };
    tree.crates = workspace_crates(&tree.source);
    tree.targets = cmake_targets(build, &tree);
    tree
}

/// Reads the list of changed files (`--changed`) and the build's tree and
/// finds the related tests among `defs`.
pub fn related_tests(list: &Path, build: &Path, defs: &[&TestDef]) -> Result<Related, String> {
    let text =
        fs::read_to_string(list).map_err(|e| format!("--changed {}: {e}", list.display()))?;
    let tree = load_tree(build);
    let changed = changed_paths(&text, &tree.source);
    let source = PathBuf::from(&tree.source);
    let mut cache: HashMap<String, Option<String>> = HashMap::new();
    let mut read = |rel: &str| {
        let text = cache.entry(rel.to_owned());
        text.or_insert_with(|| read_text(&source.join(rel))).clone()
    };
    let tests = related(defs, &changed, &tree, &mut read);
    let names = tests.iter().map(|&i| defs[i].name.clone()).collect();
    Ok(Related {
        files: changed.len(),
        tests,
        names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn paths_are_normalised() {
        assert_eq!(norm("core/../tools/testrun"), "tools/testrun");
        assert_eq!(norm("a\\b\\"), "a/b");
        assert_eq!(norm("/src/./a//b/"), "/src/a/b");
        assert_eq!(norm("./"), ".");
        assert_eq!(norm("../x"), "../x");
        assert_eq!(norm("C:\\m\\tools\\..\\app"), "C:/m/app");
        assert!(is_absolute("/src") && is_absolute("C:/m") && is_absolute("c:\\m"));
        assert!(!is_absolute("app/x.cpp") && !is_absolute("C:") && !is_absolute(""));
        assert_eq!(join("/build", "app/test_app"), "/build/app/test_app");
        assert_eq!(join("/build", "/elsewhere/x"), "/elsewhere/x");
    }

    #[test]
    fn paths_relative_to_a_root() {
        let rel = relative;
        assert_eq!(rel("/src", "/src/app/x.cpp"), Some("app/x.cpp".to_owned()));
        assert_eq!(rel("/src/", "/src/a/../b"), Some("b".to_owned()));
        assert_eq!(rel("/src", "/srcx/a"), None);
        assert_eq!(rel("/src", "/src"), None);
        assert_eq!(rel("/src", "/other/a"), None);
        let windows = rel("C:/m", "C:\\m\\tools\\x.ps1");
        assert_eq!(windows, Some("tools/x.ps1".to_owned()));
    }

    #[test]
    fn sources_cover_their_files_and_folders() {
        assert!(covers("app/commands", "app/commands/Extrude.cpp"));
        assert!(covers("app/commands", "app/commands"));
        assert!(covers("app/MainWindow.cpp", "app/MainWindow.cpp"));
        assert!(!covers("app/command", "app/commands/Extrude.cpp"));
        assert!(!covers("core/model", "core/model-tests/api/src/lib.rs"));
        assert!(!covers("app/commands/Extrude.cpp", "app/commands"));
        // The whole tree is no source.
        assert!(!covers(".", "app/x.cpp"));
    }

    #[test]
    fn define_values_are_paths() {
        assert_eq!(path_value("-DCLI=/b/mitcad-cli"), "/b/mitcad-cli");
        assert_eq!(path_value("-DLIST:FILEPATH=/b/t.tsv"), "/b/t.tsv");
        assert_eq!(path_value("/src/tools/x.sh"), "/src/tools/x.sh");
        assert_eq!(path_value("-D"), "-D");
        assert_eq!(path_value("-Da b=c"), "-Da b=c");
        assert_eq!(path_value("--doc"), "--doc");
    }

    #[test]
    fn declared_sources_are_read() {
        let script = "#!/usr/bin/env bash\r\n# SPDX-License-Identifier: MIT\r\n\
            # check-all sources: app/commands app/framework  tools/cli\r\n\
            # check-all sources: app/MainWindow.cpp\r\n\
            # The test (its check-all sources: are above).\n\
            echo '# check-all sources: not/a/header'\n";
        let expected = [
            "app/commands",
            "app/framework",
            "tools/cli",
            "app/MainWindow.cpp",
        ];
        assert_eq!(declared_sources(script), expected);
        assert!(declared_sources("# SPDX-License-Identifier: MIT\n").is_empty());
        // The whole tree or paths outside it are not areas.
        assert!(declared_sources("# check-all sources: . ../x\n").is_empty());
    }

    #[test]
    fn crate_tests_name_their_packages() {
        let script = "-P /s/core/cargo-tests.cmake --";
        let run = words(&format!(
            "/usr/bin/cmake -DLIST=/b/t.tsv {script} run mitcad-model"
        ));
        assert_eq!(cargo_packages(&run), ["mitcad-model"]);
        let part = words("cmake -P C:\\s\\core\\cargo-tests.cmake -- run mitcad-vcs tests::sync::");
        assert_eq!(cargo_packages(&part), ["mitcad-vcs"]);
        let parts = words(&format!("cmake {script} parts mitcad-vcs a:: b::"));
        assert_eq!(cargo_packages(&parts), ["mitcad-vcs"]);
        let build = words(&format!(
            "cmake {script} build --release mitcad-f3d mitcad-ipt"
        ));
        assert_eq!(cargo_packages(&build), ["mitcad-f3d", "mitcad-ipt"]);
        let doc = "/r/bin/cargo test --doc --manifest-path /s/core/Cargo.toml";
        let doc = words(&format!("{doc} -p mitcad-model --package mitcad-solver"));
        assert_eq!(cargo_packages(&doc), ["mitcad-model", "mitcad-solver"]);
        assert!(cargo_packages(&words("/b/app/test_app")).is_empty());
        assert!(cargo_packages(&words("cargo test -p mitcad-model")).is_empty());
        assert!(cargo_packages(&[]).is_empty());
    }

    #[test]
    fn workspace_members_and_names_are_read() {
        let manifest = "[workspace]\nresolver = \"3\"\n# members are listed below\n\
            default-members = [\"model\"]\n\
            members = [\"model\", \"model-tests/api\", # the API's tests\n\
              \"../tools/testrun\",\n]\n\
            [workspace.package]\nversion = \"0.18.0\"\n";
        let members = workspace_members(manifest);
        assert_eq!(members, ["model", "model-tests/api", "../tools/testrun"]);
        let one_line = "[workspace]\nmembers = [\"a\", \"b\"]\n";
        assert_eq!(workspace_members(one_line), ["a", "b"]);
        assert!(workspace_members("[package]\nname = \"x\"\n").is_empty());
        let package = "# SPDX\n[package]\ndescription = \"a name = here\"\n\
            name = \"mitcad-testrun\"\n";
        assert_eq!(package_name(package), Some("mitcad-testrun".to_owned()));
        let dependency = "[dependencies]\nname = \"not-the-package\"\n[package]\nversion = \"1\"\n";
        assert_eq!(package_name(dependency), None);
    }

    #[test]
    fn cache_values_are_read() {
        let cache = "# This is the CMakeCache file.\n\
            CMAKE_CACHEFILE_DIR:INTERNAL=C:/m/build/dev\n\
            CMAKE_HOME_DIRECTORY:INTERNAL=C:/m\n";
        let home = cache_value(cache, "CMAKE_HOME_DIRECTORY");
        assert_eq!(home, Some("C:/m".to_owned()));
        let build = cache_value(cache, "CMAKE_CACHEFILE_DIR");
        assert_eq!(build, Some("C:/m/build/dev".to_owned()));
        assert_eq!(cache_value(cache, "CMAKE_HOME"), None);
    }

    #[test]
    fn the_file_api_reply_is_read() {
        let index = r#"{ "cmake": {}, "objects": [
            { "kind": "toolchains", "jsonFile": "toolchains-v1-1.json" },
            { "kind": "codemodel", "jsonFile": "codemodel-v2-2.json" } ] }"#;
        assert_eq!(
            reply_codemodel(index),
            Some("codemodel-v2-2.json".to_owned())
        );
        assert_eq!(reply_codemodel("{}"), None);
        let codemodel = r#"{ "configurations": [
            { "name": "Debug", "targets": [
              { "name": "test_sheet", "jsonFile": "target-test_sheet-Debug.json" },
              { "name": "geometry", "jsonFile": "target-geometry-Debug.json" } ] } ] }"#;
        let files = codemodel_targets(codemodel);
        assert_eq!(
            files,
            ["target-test_sheet-Debug.json", "target-geometry-Debug.json"]
        );
        assert!(codemodel_targets("not json").is_empty());
        let target = r#"{ "name": "test_sheet",
            "artifacts": [ { "path": "geometry/test_sheet" } ],
            "sources": [
              { "path": "geometry/tests/test_sheet.cpp" },
              { "path": "build/dev/geometry/test_sheet_autogen/mocs.cpp", "isGenerated": true },
              { "path": "/elsewhere/shared.cpp" } ] }"#;
        let (artifacts, sources) = target_files(target);
        assert_eq!(artifacts, ["geometry/test_sheet"]);
        assert_eq!(
            sources,
            ["geometry/tests/test_sheet.cpp", "/elsewhere/shared.cpp"]
        );
    }

    #[test]
    fn changed_lists_are_read() {
        let list = "app/MainWindow.cpp\n\n# a comment\n./tools/ui-x-test.sh\r\n\
            /src/core/model/src/lib.rs\n/other/file\n../outside\ndocs\\naming.md\n";
        let paths = changed_paths(list, "/src");
        let expected = [
            "app/MainWindow.cpp",
            "tools/ui-x-test.sh",
            "core/model/src/lib.rs",
            "docs/naming.md",
        ];
        assert_eq!(paths, expected);
        assert!(changed_paths("", "/src").is_empty());
    }

    fn def(name: &str, command: &str) -> TestDef {
        TestDef {
            name: name.to_owned(),
            available: true,
            command: words(command),
            ..TestDef::default()
        }
    }

    /// A tree as a Linux checkout with the build folder inside it.
    fn tree() -> Tree {
        let mut tree = Tree {
            source: "/src".to_owned(),
            build: "/src/build/dev".to_owned(),
            ..Tree::default()
        };
        tree.crates
            .insert("mitcad-model".into(), "core/model".into());
        tree.crates.insert(
            "mitcad-model-tests-api".into(),
            "core/model-tests/api".into(),
        );
        tree.crates
            .insert("mitcad-testrun".into(), "tools/testrun".into());
        let sheet = strings(&["geometry/tests/test_sheet.cpp"]);
        tree.targets
            .insert(key("/src/build/dev/geometry/test_sheet"), sheet);
        let cli = strings(&["tools/cli/main.cpp"]);
        tree.targets
            .insert(key("/src/build/dev/tools/cli/mitcad-cli"), cli);
        tree
    }

    /// The tests of the run: a UI test, a cli test, two crates' tests, a
    /// C++ test program, a Python suite on a folder, a test with a source
    /// folder in its environment.
    fn defs() -> Vec<TestDef> {
        let mut fixtures = def("app.fixtures", "/src/build/dev/app/test_app --fixtures");
        fixtures.environment = strings(&["MITCAD_FIXTURES=/src/tests/fixtures", "LANG=C"]);
        vec![
            def(
                "app.ui-linux-selection-owners",
                "/usr/bin/python3 /src/tools/ui-selection-owners-test.py /src/build/dev/app/mitcad \
                 /src/build/dev/tools/cli/mitcad-cli /src/build/dev/ui-linux-selection-owners",
            ),
            def(
                "cli.f1_hole",
                "/src/build/dev/tools/cli/mitcad-cli run /src/tools/cli/tests/f1_hole.json",
            ),
            def(
                "core.model",
                "/usr/bin/cmake -DLIST=/src/build/dev/core/cargo-test/tests-debug.tsv \
                 -P /src/core/cargo-tests.cmake -- run mitcad-model",
            ),
            def(
                "core.model.api",
                "/usr/bin/cmake -DLIST=/src/build/dev/core/cargo-test/tests-debug.tsv \
                 -P /src/core/cargo-tests.cmake -- run mitcad-model-tests-api",
            ),
            def("geometry.sheet", "/src/build/dev/geometry/test_sheet"),
            def(
                "tools.f3d_learn",
                "/usr/bin/python3 -m unittest discover -s /src/tools/f3d-learn -p test_*.py",
            ),
            fixtures,
        ]
    }

    /// The related tests' names for the changed paths.
    fn related_names(changed: &[&str]) -> Vec<String> {
        let defs = defs();
        let refs: Vec<&TestDef> = defs.iter().collect();
        let mut read = |rel: &str| match rel {
            "tools/ui-selection-owners-test.py" => Some(
                "# SPDX-License-Identifier: MIT\n\
                 # check-all sources: app/framework core/model-api/src tools/cli\n"
                    .to_owned(),
            ),
            "tools/cli/tests/f1_hole.json" => Some("{ \"commands\": [] }".to_owned()),
            _ => None,
        };
        let changed = strings(changed);
        let found = related(&refs, &changed, &tree(), &mut read);
        found.iter().map(|&i| defs[i].name.clone()).collect()
    }

    #[test]
    fn ui_tests_follow_their_declared_sources() {
        let ui = ["app.ui-linux-selection-owners"];
        assert_eq!(related_names(&["app/framework/TestDriver.cpp"]), ui);
        assert_eq!(related_names(&["core/model-api/src/lib.rs"]), ui);
        // Its script itself.
        assert_eq!(related_names(&["tools/ui-selection-owners-test.py"]), ui);
        // The application it starts is a build product, not a source.
        assert!(related_names(&["app/main.cpp"]).is_empty());
    }

    #[test]
    fn named_files_and_programs_relate() {
        // The cli test's JSON; tools/cli covers the UI test that declares it
        // and the cli program's own source the cli test.
        let both = ["app.ui-linux-selection-owners", "cli.f1_hole"];
        assert_eq!(related_names(&["tools/cli/tests/f1_hole.json"]), both);
        assert_eq!(related_names(&["tools/cli/main.cpp"]), both);
        assert_eq!(
            related_names(&["geometry/tests/test_sheet.cpp"]),
            ["geometry.sheet"]
        );
        // Only the program's own target, not the libraries it links.
        assert!(related_names(&["geometry/src/sheet.cpp"]).is_empty());
        // A folder the command names, and one its environment names.
        assert_eq!(
            related_names(&["tools/f3d-learn/test_reader.py"]),
            ["tools.f3d_learn"]
        );
        assert_eq!(
            related_names(&["tests/fixtures/block.json"]),
            ["app.fixtures"]
        );
        // The crate tests' runner script relates all of them.
        assert_eq!(
            related_names(&["core/cargo-tests.cmake"]),
            ["core.model", "core.model.api"]
        );
        assert!(related_names(&["docs/development.md"]).is_empty());
        assert!(related_names(&[]).is_empty());
    }

    #[test]
    fn crate_tests_follow_their_crates() {
        assert_eq!(
            related_names(&["core/model/src/document.rs"]),
            ["core.model"]
        );
        assert_eq!(
            related_names(&["core/model-tests/api/src/lib.rs"]),
            ["core.model.api"]
        );
        let both = ["core/model/Cargo.toml", "core/model-tests/api/tests/x.rs"];
        assert_eq!(related_names(&both), ["core.model", "core.model.api"]);
        // A crate the run has no test of.
        assert!(related_names(&["tools/testrun/src/changed.rs"]).is_empty());
    }

    #[test]
    fn build_products_are_not_read() {
        let defs = defs();
        let refs: Vec<&TestDef> = defs.iter().collect();
        let mut asked: Vec<String> = Vec::new();
        let mut read = |rel: &str| -> Option<String> {
            asked.push(rel.to_owned());
            None
        };
        let changed = strings(&["build/dev/app/mitcad"]);
        assert!(related(&refs, &changed, &tree(), &mut read).is_empty());
        assert!(
            asked.iter().all(|rel| !rel.starts_with("build/")),
            "{asked:?}"
        );
        assert!(asked.contains(&"tools/ui-selection-owners-test.py".to_owned()));
    }

    #[test]
    fn ctest_commands_and_environments_are_kept() {
        let json = r#"{ "tests": [ { "name": "t",
            "command": ["/usr/bin/bash", "/src/tools/ui-x-test.sh"],
            "properties": [ { "name": "ENVIRONMENT", "value": ["A=/src/x", "B=1"] } ] } ] }"#;
        let tests = crate::ctest::parse_tests(json).unwrap();
        assert_eq!(
            tests[0].command,
            ["/usr/bin/bash", "/src/tools/ui-x-test.sh"]
        );
        assert_eq!(tests[0].environment, ["A=/src/x", "B=1"]);
    }

    #[test]
    fn the_option_names_the_list() {
        let line = words("--build b --schedule --changed changed.txt");
        let o = crate::options::parse(&line).unwrap();
        assert_eq!(o.changed, Some(PathBuf::from("changed.txt")));
        let o = crate::options::parse(&words("--build b")).unwrap();
        assert_eq!(o.changed, None);
        assert!(crate::options::parse(&words("--build b --changed")).is_err());
    }

    #[test]
    fn the_summary_names_the_tests() {
        let related = Related {
            files: 3,
            tests: vec![0, 4],
            names: strings(&["core.model", "geometry.sheet"]),
        };
        let line = "testrun: 3 changed files, 2 related tests: core.model, geometry.sheet";
        assert_eq!(related.summary(), line);
        let none = Related {
            files: 1,
            ..Related::default()
        };
        assert_eq!(none.summary(), "testrun: 1 changed files, no related test");
    }
}
