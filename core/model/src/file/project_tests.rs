// SPDX-License-Identifier: MIT
//! Project files of version 3 and the B-rep store (P12a), with the mock
//! kernel.

use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::*;
use crate::FeatureStatus;
use crate::document_tests::{block, def};
use crate::features::Brep;
use crate::ids::FeatureUid;
use crate::testing::MockKernel;

fn brep(data: &str) -> Value {
    serde_json::to_value(Brep::new(data.as_bytes().to_vec())).unwrap()
}

/// The block with two base features: Base1 with two bodies of the same
/// data, Base2 with another.
fn with_bases() -> Document<MockKernel> {
    let mut b = block();
    for definition in [
        json!({"type": "base", "bodies": [
            {"name": "Left", "brep": brep("6 bolt")},
            {"name": "Right", "brep": brep("6 bolt")}]}),
        json!({"type": "base", "bodies": [{"brep": brep("4 wedge")}], "source": "wedge.step"}),
    ] {
        b.doc.add_feature(&def(definition), None).unwrap();
    }
    b.doc
}

/// The bodies with what made them, after a recompute.
fn bodies(doc: &mut Document<MockKernel>) -> Vec<(String, String, String)> {
    doc.recompute();
    doc.bodies()
        .into_iter()
        .map(|b| (b.uid.to_string(), b.name, b.shape.history.clone()))
        .collect()
}

fn value(json: &str) -> Value {
    serde_json::from_str(json).unwrap()
}

/// A fresh folder in the temporary directory.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mitcad-p12a-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The files under `dir`, relative and sorted.
fn files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap().to_string_lossy();
                out.push(relative.replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn version_3_refers_to_the_store_and_round_trips() {
    let mut doc = with_bases();
    let expected = bodies(&mut doc);
    let single = doc.to_json();
    assert_eq!(value(&single)["version"], 2);

    let store = MemoryStore::new();
    let v3 = doc.to_project_json(&store).unwrap();
    let file = value(&v3);
    assert_eq!(file["version"], 3);
    // The same data is stored once.
    assert_eq!(store.len(), 2);
    let bolt = Sha256::of(b"6 bolt");
    let left = &file["features"][2]["bodies"][0]["brep"];
    assert_eq!(
        *left,
        json!({"format": "occt", "compression": "zlib", "size": 6, "sha256": bolt.to_string()})
    );
    assert_eq!(file["features"][2]["bodies"][1]["brep"], *left);
    assert!(!v3.contains("\"data\""));
    // Apart from the B-rep data and the version, the files are the same.
    let mut without = value(&single);
    for (i, j) in [(2, 0), (2, 1), (3, 0)] {
        without["features"][i]["bodies"][j]["brep"] =
            file["features"][i]["bodies"][j]["brep"].clone();
    }
    without["version"] = json!(3);
    assert_eq!(without, file);

    // Saving again writes no B-rep file.
    assert_eq!(doc.to_project_json(&store).unwrap(), v3);
    assert_eq!(store.writes(), 2);

    // Read back: the same bodies, and the single file is the same text.
    let mut back = Document::from_json_in(&v3, &store, MockKernel::default()).unwrap();
    assert!(
        back.load_warnings().is_empty(),
        "{:?}",
        back.load_warnings()
    );
    assert_eq!(bodies(&mut back), expected);
    assert_eq!(back.to_json(), single);
    // Bodies with the same data share it.
    let FeatureDef::Base(base) = &back.state().features[2].def else {
        panic!("a base feature");
    };
    assert!(std::ptr::eq(
        base.bodies[0].brep.data().unwrap(),
        base.bodies[1].brep.data().unwrap()
    ));
}

#[test]
fn documents_without_base_features_are_the_same_in_both_versions() {
    let b = block();
    let store = MemoryStore::new();
    assert_eq!(b.doc.to_project_json(&store).unwrap(), b.doc.to_json());
    assert!(store.is_empty());
    assert_eq!(value(&b.doc.to_json())["version"], 2);
}

#[test]
fn missing_and_damaged_data_fail_the_base_feature() {
    let doc = with_bases();
    let store = MemoryStore::new();
    let v3 = doc.to_project_json(&store).unwrap();
    let bolt = Sha256::of(b"6 bolt");
    let wedge = Sha256::of(b"4 wedge");
    store.remove(&bolt);
    store.replace(wedge, Brep::new(b"4 wedgf".to_vec()).stored().unwrap());

    let mut back = Document::from_json_in(&v3, &store, MockKernel::default()).unwrap();
    assert_eq!(
        back.load_warnings(),
        [
            format!(
                "features[2] (Base1): bodies[0]: its B-rep data {bolt} is missing from the \
                 project store (.mitcad/brep)"
            ),
            format!(
                "features[3] (Base2): bodies[0]: the file {} is damaged: its data has the \
                 SHA-256 {}",
                brep_path(&wedge),
                Sha256::of(b"4 wedgf")
            ),
        ]
    );
    back.recompute();
    assert_eq!(
        back.status(FeatureUid(3)),
        Some(&FeatureStatus::Failed(format!(
            "body 0: its B-rep data {bolt} is missing from the project store (.mitcad/brep)"
        )))
    );
    assert!(matches!(
        back.status(FeatureUid(4)),
        Some(FeatureStatus::Failed(_))
    ));
    // The block before them is fine.
    assert_eq!(back.status(FeatureUid(2)), Some(&FeatureStatus::Ok));

    // Saved again, a missing reference stays one, also in a single file,
    // which is then version 3.
    let single = back.to_json();
    assert_eq!(value(&single)["version"], 3);
    assert_eq!(
        value(&single)["features"][2]["bodies"][0]["brep"]["sha256"],
        bolt.to_string()
    );
    // Without a store at all, the references stay too.
    let plain = Document::from_json(&v3, MockKernel::default()).unwrap();
    assert!(plain.load_warnings().is_empty());
    assert_eq!(plain.to_json(), v3);
}

#[test]
fn newer_versions_are_refused() {
    let doc = with_bases();
    let v3 = doc.to_project_json(&MemoryStore::new()).unwrap();
    let v4 = v3.replace("\"version\": 3", "\"version\": 4");
    let error = Document::from_json(&v4, MockKernel::default())
        .err()
        .expect("refused")
        .to_string();
    assert!(
        error.contains("version 4, newer than this Mitcad can read (version 3)"),
        "{error}"
    );
}

#[test]
fn saving_in_and_outside_a_project() {
    let dir = scratch("save");
    let doc = with_bases();
    let expected = bodies(&mut with_bases());

    // Outside a project: one file, version 2, nothing else written.
    let outside = dir.join("outside.mitcad");
    let text = doc.save_file(&outside, FileFormat::Auto).unwrap();
    assert_eq!(text, doc.to_json());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), text);
    assert_eq!(files(&dir), ["outside.mitcad"]);
    let error = doc
        .save_file(&outside, FileFormat::Project)
        .unwrap_err()
        .to_string();
    assert!(error.contains("is not in a Mitcad project"), "{error}");

    // In a project (a sub-folder of it): version 3 and the store.
    let root = dir.join("project");
    let (project, written) = Project::init(&root).unwrap();
    assert_eq!(
        written,
        [".mitcad/project.json", ".gitattributes", ".gitignore"]
    );
    let path = root.join("parts").join("bracket.mitcad");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    assert_eq!(Project::find(&path), Some(project.clone()));
    let text = doc.save_file(&path, FileFormat::Auto).unwrap();
    assert_eq!(value(&text)["version"], 3);
    let bolt = brep_path(&Sha256::of(b"6 bolt"));
    let wedge = brep_path(&Sha256::of(b"4 wedge"));
    let mut expected_files = vec![
        ".gitattributes".to_owned(),
        ".gitignore".to_owned(),
        ".mitcad/project.json".to_owned(),
        bolt.clone(),
        wedge.clone(),
        "parts/bracket.mitcad".to_owned(),
    ];
    expected_files.sort();
    assert_eq!(files(&root), expected_files);
    assert!(bolt.starts_with(".mitcad/brep/") && bolt.ends_with(".brep.zlib"));
    // A single copy in the project: version 2.
    let copy = root.join("copy.mitcad");
    assert_eq!(
        doc.save_file(&copy, FileFormat::Single).unwrap(),
        doc.to_json()
    );

    // Opened, from the project's store.
    let mut back = Document::load_project(&path, MockKernel::default()).unwrap();
    assert!(
        back.load_warnings().is_empty(),
        "{:?}",
        back.load_warnings()
    );
    assert_eq!(bodies(&mut back), expected);
    // Moved out of the project, the data is not found.
    let moved = dir.join("moved.mitcad");
    std::fs::copy(&path, &moved).unwrap();
    let lost = Document::load_project(&moved, MockKernel::default()).unwrap();
    assert!(
        lost.load_warnings()[0].contains("is not in a Mitcad project"),
        "{:?}",
        lost.load_warnings()
    );
    // Converted back to a single file, it opens anywhere.
    back.save_file(&moved, FileFormat::Single).unwrap();
    let mut single = Document::load_project(&moved, MockKernel::default()).unwrap();
    assert_eq!(bodies(&mut single), expected);
    assert_eq!(single.to_json(), doc.to_json());
    // No temporary files are left.
    assert!(!files(&dir).iter().any(|f| f.ends_with(".tmp")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn init_keeps_what_is_there() {
    let dir = scratch("init");
    std::fs::write(dir.join(".gitignore"), "build/\n.mitcad/cache/").unwrap();
    let (project, written) = Project::init(&dir).unwrap();
    assert_eq!(
        written,
        [".mitcad/project.json", ".gitattributes", ".gitignore"]
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
        "build/\n.mitcad/cache/\n.mitcad/local/\n.*.tmp\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitattributes")).unwrap(),
        GITATTRIBUTES
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(PROJECT_MARKER)).unwrap(),
        "{\"format\": \"mitcad-project\", \"version\": 1}\n"
    );
    // Again: nothing to do.
    let (again, written) = Project::init(&dir).unwrap();
    assert_eq!(again, project);
    assert!(written.is_empty());
    assert_eq!(Project::open(&dir), Some(project.clone()));
    assert_eq!(
        project.store().dir(),
        project.root().join(".mitcad").join("brep")
    );
    assert_eq!(Project::open(&dir.join(".mitcad")), None);
    // Paths with `..` are folded before walking up.
    let dotted = dir.join("sub").join("..").join("a.mitcad");
    assert_eq!(Project::find(&dotted), Some(project.clone()));
    assert_eq!(
        project.local_state_path(&dotted),
        project.local_state_path(&dir.join("a.mitcad"))
    );
    assert!(Project::find(&dir.join("..").join("a.mitcad")).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A project made before project files were merged as whole files (P12
/// remote) gets `merge=binary` in place of its older line; the user's own
/// lines stay.
#[test]
fn init_brings_older_attributes_up_to_date() {
    let dir = scratch("attributes");
    let older = "\
# Mitcad project files are text with LF line ends on every platform.
*.mitcad   text eol=lf
*.png binary
# B-rep data of base features: content-addressed, never changed.
.mitcad/brep/** binary
";
    std::fs::write(dir.join(".gitattributes"), older).unwrap();
    std::fs::create_dir_all(dir.join(".mitcad")).unwrap();
    std::fs::write(dir.join(PROJECT_MARKER), "{}").unwrap();
    std::fs::write(dir.join(".gitignore"), GITIGNORE).unwrap();
    let (_, written) = Project::init(&dir).unwrap();
    assert_eq!(written, [".gitattributes"]);
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitattributes")).unwrap(),
        older.replace(
            "*.mitcad   text eol=lf",
            "*.mitcad text eol=lf merge=binary"
        )
    );
    let (_, written) = Project::init(&dir).unwrap();
    assert!(written.is_empty());
    // A line of the user's for the same pattern with other attributes stays.
    std::fs::write(dir.join(".gitattributes"), "*.mitcad diff=mitcad\n").unwrap();
    Project::init(&dir).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitattributes")).unwrap(),
        "*.mitcad diff=mitcad\n*.mitcad text eol=lf merge=binary\n.mitcad/brep/** binary\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_folder_store_writes_a_file_once() {
    let dir = scratch("store");
    let store = FsStore::new(dir.join("brep"));
    let sha = Sha256::of(b"data");
    assert_eq!(store.get(&sha).unwrap(), None);
    assert!(!store.contains(&sha).unwrap());
    store.put(&sha, b"first").unwrap();
    store.put(&sha, b"second").unwrap();
    assert_eq!(store.get(&sha).unwrap().as_deref(), Some(&b"first"[..]));
    assert!(store.contains(&sha).unwrap());
    let name = sha.to_string();
    assert_eq!(
        store.path(&sha),
        dir.join("brep")
            .join(&name[..2])
            .join(format!("{name}.brep.zlib"))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn linked_components_read_their_data_from_their_project() {
    let dir = scratch("link");
    let root = dir.join("project");
    Project::init(&root).unwrap();
    let part = with_bases();
    part.save_file(&root.join("part.mitcad"), FileFormat::Auto)
        .unwrap();
    let mut assembly = Document::new(MockKernel::default());
    let options = crate::document::InsertOptions {
        link: true,
        base: Some(root.clone()),
        ..Default::default()
    };
    assembly.insert_component("part.mitcad", &options).unwrap();
    let names: Vec<String> = bodies(&mut assembly).into_iter().map(|b| b.1).collect();
    // The part's block and its three base feature bodies.
    assert_eq!(names.len(), 4, "{names:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_display_state_is_kept_apart_in_a_project() {
    let dir = scratch("display");
    let root = dir.join("project");
    let (project, _) = Project::init(&root).unwrap();
    let mut doc = with_bases();
    let path = root.join("bracket.mitcad");
    let plain = doc.save_file(&path, FileFormat::Auto).unwrap();
    doc.set_origin_visible(true).unwrap();
    doc.set_isolation(vec![crate::document::Isolated {
        body: Some("F2.b0".parse().unwrap()),
        occurrence: String::new(),
    }])
    .unwrap();
    // The versioned file does not change with the display state.
    let text = doc.save_file(&path, FileFormat::Auto).unwrap();
    assert_eq!(text, plain);
    assert!(value(&text).get("display").is_none());
    let local = root
        .join(".mitcad")
        .join("local")
        .join("display")
        .join("bracket.mitcad.json");
    assert_eq!(project.local_state_path(&path), Some(local.clone()));
    let kept: Value = serde_json::from_str(&std::fs::read_to_string(&local).unwrap()).unwrap();
    assert_eq!(
        kept,
        json!({"format": "mitcad-local", "version": 1,
               "display": {"origin": true, "isolated": [{"body": "F2.b0"}]}})
    );
    // A single file keeps it inside, as version 2 does.
    assert_eq!(value(&doc.to_json())["display"]["origin"], true);

    // Opened, the display state comes back.
    let back = Document::load_project(&path, MockKernel::default()).unwrap();
    assert_eq!(back.display(), doc.display());
    // A damaged one is left out with a warning.
    std::fs::write(&local, "{").unwrap();
    let back = Document::load_project(&path, MockKernel::default()).unwrap();
    assert!(back.display().is_default());
    assert!(
        back.load_warnings()[0].contains("the display state in"),
        "{:?}",
        back.load_warnings()
    );
    // Back to the defaults, the file goes.
    doc.set_origin_visible(false).unwrap();
    doc.set_isolation(Vec::new()).unwrap();
    doc.save_file(&path, FileFormat::Auto).unwrap();
    assert!(!local.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_display_state_follows_a_renamed_file() {
    let dir = scratch("display-rename");
    let root = dir.join("project");
    let (project, _) = Project::init(&root).unwrap();
    let mut doc = with_bases();
    doc.set_origin_visible(true).unwrap();
    let old = root.join("bracket.mitcad");
    doc.save_file(&old, FileFormat::Auto).unwrap();
    let new = root.join("parts").join("plate.mitcad");
    std::fs::create_dir_all(new.parent().unwrap()).unwrap();
    std::fs::rename(&old, &new).unwrap();
    // Before it follows, the renamed file opens without it.
    assert!(
        Document::load_project(&new, MockKernel::default())
            .unwrap()
            .display()
            .is_default()
    );
    assert!(project.move_local_state(&old, &new).unwrap());
    assert!(!project.local_state_path(&old).unwrap().exists());
    let back = Document::load_project(&new, MockKernel::default()).unwrap();
    assert!(back.display().origin);
    // Nothing to move the second time; a file's own state stays.
    assert!(!project.move_local_state(&old, &new).unwrap());
    doc.save_file(&old, FileFormat::Auto).unwrap();
    assert!(!project.move_local_state(&old, &new).unwrap());
    assert!(!project.local_state_path(&old).unwrap().exists());
    assert!(project.local_state_path(&new).unwrap().exists());
    // A file that is gone takes its state along.
    assert!(project.remove_local_state(&new).unwrap());
    assert!(!project.remove_local_state(&new).unwrap());
    // Outside the project there is no state to move.
    assert!(
        !project
            .move_local_state(&dir.join("x.mitcad"), &new)
            .unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rename_is_tried_again_while_another_program_holds_the_file() {
    use std::cell::Cell;
    use std::time::Duration;
    let locked = || io::Error::from_raw_os_error(32);
    let transient = |e: &io::Error| e.raw_os_error() == Some(32);
    // Free after three tries.
    let tries = Cell::new(0);
    let result = super::project::retrying(
        || {
            tries.set(tries.get() + 1);
            if tries.get() < 4 {
                Err(locked())
            } else {
                Ok(())
            }
        },
        transient,
        Duration::from_secs(2),
    );
    assert!(result.is_ok());
    assert_eq!(tries.get(), 4);
    // Held for good: it gives up after its patience.
    let tries = Cell::new(0);
    let start = std::time::Instant::now();
    let result = super::project::retrying(
        || {
            tries.set(tries.get() + 1);
            Err(locked())
        },
        transient,
        Duration::from_millis(100),
    );
    assert_eq!(result.unwrap_err().raw_os_error(), Some(32));
    assert!(tries.get() > 1 && start.elapsed() < Duration::from_secs(1));
    // Another error is not tried again.
    let tries = Cell::new(0);
    let result = super::project::retrying(
        || {
            tries.set(tries.get() + 1);
            Err(io::Error::from(io::ErrorKind::NotFound))
        },
        transient,
        Duration::from_secs(2),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_eq!(tries.get(), 1);
}

#[test]
fn references_are_read_without_loading() {
    let doc = with_bases();
    let store = MemoryStore::new();
    let v3 = doc.to_project_json(&store).unwrap();
    let expected: BTreeSet<Sha256> = [Sha256::of(b"6 bolt"), Sha256::of(b"4 wedge")].into();
    assert_eq!(brep_references(&v3).unwrap(), expected);
    // Data inside the file is no reference.
    assert!(brep_references(&doc.to_json()).unwrap().is_empty());
    assert!(matches!(brep_references("{}"), Err(FileError::NotAProject)));
    assert!(matches!(brep_references("[1"), Err(FileError::Syntax(_))));
    let newer = v3.replacen("\"version\": 3", "\"version\": 9", 1);
    assert!(matches!(
        brep_references(&newer),
        Err(FileError::Version(_))
    ));
    let damaged = v3.replacen("\"sha256\": \"", "\"sha256\": \"x", 1);
    assert!(matches!(
        brep_references(&damaged),
        Err(FileError::Invalid(_))
    ));
}
