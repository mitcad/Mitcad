// SPDX-License-Identifier: MIT
//! The version history in temporary projects, with the mock kernel; the
//! system's git checks the repositories where it is installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use gix::bstr::ByteSlice;
use mitcad_model::features::Brep;
use mitcad_model::testing::MockKernel;
use mitcad_model::{Document, FileFormat};

use super::*;

// Remote repositories (P12 remote).
mod remote;
// Sync and conflicts.
mod sync;
// Component libraries and the community library (mitcad#64, mitcad#63).
mod library;

/// A fresh folder in the temporary directory, removed afterwards.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mitcad-vcs-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn join(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn author() -> Identity {
    Identity::new("Mitcad Test", "test@example.invalid").unwrap()
}

fn brep(data: &str) -> String {
    serde_json::to_string(&Brep::new(data.as_bytes().to_vec())).unwrap()
}

/// A block (Sketch1, Extrude1 with `d3`) and two base features: Base1
/// with a bolt, Base2 with a wedge.
fn design() -> Document<MockKernel> {
    let mut doc = Document::new(MockKernel::default());
    let commands = [
        r#"{"cmd": "sketch.create"}"#.to_owned(),
        r#"{"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40}"#
            .to_owned(),
        r#"{"cmd": "add_feature", "def": {"type": "extrude",
            "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
            "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}"#
            .to_owned(),
        format!(
            r#"{{"cmd": "add_feature", "def": {{"type": "base", "bodies": [{{"name": "Bolt", "brep": {}}}]}}}}"#,
            brep("6 bolt")
        ),
        format!(
            r#"{{"cmd": "add_feature", "def": {{"type": "base", "bodies": [{{"name": "Wedge", "brep": {}}}]}}}}"#,
            brep("4 wedge")
        ),
    ];
    for command in commands {
        doc.command(&command).unwrap();
    }
    doc
}

/// The bodies with what made them, after a recompute.
fn bodies(doc: &mut Document<MockKernel>) -> Vec<(String, String, String)> {
    doc.recompute();
    doc.bodies()
        .into_iter()
        .map(|b| (b.uid.to_string(), b.name, b.shape.history.clone()))
        .collect()
}

/// A project with version history and the design saved in `part.mitcad`
/// (not recorded yet).
fn project(scratch: &Scratch) -> (ProjectRepo, Document<MockKernel>, PathBuf) {
    let (repo, _) = ProjectRepo::init(&scratch.join("project"), Some(&author())).unwrap();
    let doc = design();
    let part = repo.root().join("part.mitcad");
    doc.save_file(&part, FileFormat::Auto).unwrap();
    (repo, doc, part)
}

/// The paths of HEAD's tree.
fn tree_files(repo: &ProjectRepo) -> Vec<String> {
    let head = repo.head_commit().unwrap().unwrap();
    let tree = repo
        .repo
        .find_commit(head)
        .unwrap()
        .tree_id()
        .unwrap()
        .detach();
    let mut files: Vec<String> = repo
        .differences(None, Some(tree))
        .unwrap()
        .into_iter()
        .map(|d| d.path)
        .collect();
    files.sort();
    files
}

/// The B-rep path of mock data.
fn brep_file(data: &str) -> String {
    mitcad_model::file::brep_path(&Sha256::of(data.as_bytes()))
}

fn set_parameter(doc: &mut Document<MockKernel>, name: &str, value: f64) {
    doc.command(&format!(
        r#"{{"cmd": "set_parameter", "name": "{name}", "value": {value}}}"#
    ))
    .unwrap();
}

/// The system's git, if it is installed.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Git User",
            "-c",
            "user.email=git@example.invalid",
        ])
        .args(args)
        .output()
        .ok()?;
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn has_git() -> bool {
    let found = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !found {
        eprintln!("git is not installed: the checks with the system's git are skipped");
    }
    found
}

#[test]
fn identities_are_name_and_email() {
    let identity = Identity::parse(" Ada Lovelace <ada@example.invalid> ").unwrap();
    assert_eq!(
        identity,
        Identity::new("Ada Lovelace", "ada@example.invalid").unwrap()
    );
    assert_eq!(identity.to_string(), "Ada Lovelace <ada@example.invalid>");
    for bad in [
        "Ada",
        "<ada@example.invalid>",
        "Ada <>",
        "Ada <a<b>",
        "A\nda <a@b>",
    ] {
        assert!(Identity::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn init_makes_a_repository_with_the_project_files() {
    let scratch = Scratch::new("init");
    let (repo, outcome) = ProjectRepo::init(&scratch.join("bracket"), Some(&author())).unwrap();
    assert!(outcome.commit.is_some(), "{outcome:?}");
    assert_eq!(
        outcome.written,
        [".mitcad/project.json", ".gitattributes", ".gitignore"]
    );
    assert_eq!(repo.branch().unwrap().as_deref(), Some("main"));
    assert_eq!(
        tree_files(&repo),
        [".gitattributes", ".gitignore", ".mitcad/project.json"]
    );
    let first = repo.history(&repo.root().join(".gitignore")).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].summary, "Create project bracket");
    assert_eq!(first[0].author, author());

    // Again: nothing new to record.
    let (again, outcome) = ProjectRepo::init(&scratch.join("bracket"), Some(&author())).unwrap();
    assert_eq!(outcome, CommitOutcome::default());
    assert_eq!(again.head_commit().unwrap(), repo.head_commit().unwrap());
    // Found from the folder and from a file in it, existing or not.
    for path in [
        repo.root().to_path_buf(),
        repo.root().join("parts/new.mitcad"),
    ] {
        assert_eq!(ProjectRepo::open(&path).unwrap().root(), repo.root());
    }
}

#[test]
fn only_projects_at_a_repository_root_have_history() {
    let scratch = Scratch::new("open");
    let error = |path: &Path| match ProjectRepo::open(path) {
        Err(VcsError::NotVersioned(message)) => message,
        other => panic!("{other:?}"),
    };
    // Not a project.
    fs::create_dir_all(scratch.join("plain")).unwrap();
    assert!(error(&scratch.join("plain/a.mitcad")).contains("is not in a Mitcad project"));
    // A project without a repository.
    Project::init(&scratch.join("loose")).unwrap();
    assert!(error(&scratch.join("loose/a.mitcad")).contains("has no version history"));
    // A project inside another repository, below its root.
    gix::init(scratch.join("outer")).unwrap();
    Project::init(&scratch.join("outer/inner")).unwrap();
    assert!(
        error(&scratch.join("outer/inner/a.mitcad")).contains("inside the git repository"),
        "{}",
        error(&scratch.join("outer/inner/a.mitcad"))
    );
    // A repository without the marker at its root.
    gix::init(scratch.join("code")).unwrap();
    assert!(error(&scratch.join("code/a.mitcad")).contains("is not in a Mitcad project"));
}

#[test]
fn a_version_has_the_saved_paths_and_their_brep_files() {
    let scratch = Scratch::new("commit");
    let (repo, mut doc, part) = project(&scratch);
    fs::write(repo.root().join("notes.txt"), "not saved by Mitcad\n").unwrap();
    let outcome = repo
        .commit(std::slice::from_ref(&part), "First version", &author())
        .unwrap();
    let mut written = outcome.written.clone();
    written.sort();
    let mut expected = vec![
        brep_file("4 wedge"),
        brep_file("6 bolt"),
        "part.mitcad".to_owned(),
    ];
    expected.sort();
    assert_eq!(written, expected);
    assert!(
        outcome.removed.is_empty() && outcome.warnings.is_empty(),
        "{outcome:?}"
    );
    // The folder's other files are not part of it.
    assert!(!tree_files(&repo).contains(&"notes.txt".to_owned()));

    // Saving again without a change records nothing.
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let unchanged = repo
        .commit(std::slice::from_ref(&part), "Again", &author())
        .unwrap();
    assert_eq!(unchanged, CommitOutcome::default());

    set_parameter(&mut doc, "d3", 25.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let second = repo
        .commit(std::slice::from_ref(&part), "", &author())
        .unwrap();
    assert_eq!(second.written, ["part.mitcad"]);
    let history = repo.history(&part).unwrap();
    let summaries: Vec<&str> = history.iter().map(|v| v.summary.as_str()).collect();
    assert_eq!(summaries, ["Save part.mitcad", "First version"]);
    assert_eq!(history[0].id, second.commit.unwrap());
    assert_eq!(history[0].message, "Save part.mitcad");
    assert_eq!(history[1].author, author());
    // The trailers are in the commit.
    let raw = repo
        .repo
        .find_commit(repo.head_commit().unwrap().unwrap())
        .unwrap();
    let raw = raw.message_raw().unwrap().to_str_lossy().into_owned();
    assert!(raw.ends_with("Mitcad-Format: 3\n"), "{raw}");
    assert!(raw.contains(&format!("Mitcad-Version: {MITCAD_VERSION}\n")));
}

#[test]
fn older_versions_open_with_their_bodies() {
    let scratch = Scratch::new("older");
    let (repo, mut doc, part) = project(&scratch);
    let first_text = doc.to_json();
    let first_bodies = bodies(&mut doc);
    let first = repo
        .commit(std::slice::from_ref(&part), "First", &author())
        .unwrap()
        .commit
        .unwrap();
    set_parameter(&mut doc, "d3", 25.0);
    doc.command(r#"{"cmd": "delete_feature", "uid": "F3"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();

    for rev in [first.as_str(), &first[..8], "HEAD~1", "head^"] {
        let mut old = repo
            .load_version(rev, &part, MockKernel::default())
            .unwrap();
        assert!(old.load_warnings().is_empty(), "{:?}", old.load_warnings());
        // The single file of the version is the one saved then: the same
        // parameters, features and B-rep data.
        assert_eq!(old.to_json(), first_text);
        assert_eq!(bodies(&mut old), first_bodies);
    }
    let latest = repo
        .load_version("HEAD", &part, MockKernel::default())
        .unwrap();
    assert_eq!(latest.to_json(), doc.to_json());
    // The store of a version reads from its tree and cannot be written.
    let store = repo.store_at(&first).unwrap();
    use mitcad_model::BlobStore;
    assert!(store.contains(&Sha256::of(b"6 bolt")).unwrap());
    assert!(!store.contains(&Sha256::of(b"7 other")).unwrap());
    assert!(store.put(&Sha256::of(b"x"), b"x").is_err());
    // Versions that are not there.
    assert!(matches!(repo.resolve("HEAD~5"), Err(VcsError::NotFound(_))));
    assert!(matches!(repo.resolve("abc"), Err(VcsError::NotFound(_))));
    assert!(matches!(
        repo.resolve("0000000"),
        Err(VcsError::NotFound(_))
    ));
    assert!(matches!(
        repo.read(&first, &repo.root().join("nothing.mitcad")),
        Err(VcsError::NotFound(_))
    ));
}

#[test]
fn brep_files_nothing_refers_to_are_removed() {
    let scratch = Scratch::new("cleanup");
    let (repo, mut doc, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let bolt = brep_file("6 bolt");
    let wedge = brep_file("4 wedge");

    // Another project file, not recorded, refers to both bodies' data.
    let other = repo.root().join("drafts/other.mitcad");
    doc.save_file(&other, FileFormat::Auto).unwrap();
    doc.command(r#"{"cmd": "delete_feature", "uid": "F4"}"#)
        .unwrap();
    doc.command(r#"{"cmd": "delete_feature", "uid": "F3"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let outcome = repo
        .commit(std::slice::from_ref(&part), "Without bases", &author())
        .unwrap();
    let mut removed = outcome.removed.clone();
    removed.sort();
    let mut expected = vec![bolt.clone(), wedge.clone()];
    expected.sort();
    assert_eq!(removed, expected, "{outcome:?}");
    assert!(
        !tree_files(&repo)
            .iter()
            .any(|f| f.starts_with(".mitcad/brep"))
    );
    // The folder keeps what the other file refers to (both), so nothing
    // is deleted.
    assert!(outcome.deleted.is_empty(), "{outcome:?}");
    assert!(repo.root().join(&bolt).is_file());

    // Without it the files go, and their folders.
    fs::remove_file(&other).unwrap();
    let first = repo.resolve("HEAD~1").unwrap();
    let back = repo
        .load_version(&first, &part, MockKernel::default())
        .unwrap();
    back.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Bases back", &author())
        .unwrap();
    assert!(tree_files(&repo).contains(&bolt));
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let outcome = repo
        .commit(
            std::slice::from_ref(&part),
            "Without bases again",
            &author(),
        )
        .unwrap();
    let mut deleted = outcome.deleted.clone();
    deleted.sort();
    assert_eq!(deleted, expected);
    assert!(!repo.root().join(&bolt).exists());
    assert!(!repo.root().join(&bolt).parent().unwrap().exists());
    // Older versions still have them.
    let mut old = repo
        .load_version(&first, &part, MockKernel::default())
        .unwrap();
    assert!(old.load_warnings().is_empty(), "{:?}", old.load_warnings());
    assert_eq!(bodies(&mut old).len(), 3);
}

#[test]
fn changes_and_renames_between_versions() {
    let scratch = Scratch::new("changes");
    let (repo, mut doc, part) = project(&scratch);
    let first = repo
        .commit(std::slice::from_ref(&part), "First", &author())
        .unwrap()
        .commit
        .unwrap();
    assert!(repo.changes(&first, None).unwrap().is_empty());
    doc.command(r#"{"cmd": "delete_feature", "uid": "F3"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Delete Base1", &author())
        .unwrap();
    let change = |kind, path: &str, from: Option<&str>| Change {
        kind,
        path: path.to_owned(),
        from: from.map(str::to_owned),
    };
    let mut expected = vec![
        change(ChangeKind::Deleted, &brep_file("6 bolt"), None),
        change(ChangeKind::Modified, "part.mitcad", None),
    ];
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(repo.changes(&first, None).unwrap(), expected);

    // A rename: the old path removed, the new one added.
    let renamed = repo.root().join("parts/bracket.mitcad");
    fs::create_dir_all(renamed.parent().unwrap()).unwrap();
    fs::rename(&part, &renamed).unwrap();
    let outcome = repo
        .commit(&[part.clone(), renamed.clone()], "Rename", &author())
        .unwrap();
    assert_eq!(outcome.written, ["parts/bracket.mitcad"]);
    assert_eq!(outcome.removed, ["part.mitcad"]);
    assert_eq!(
        repo.changes("HEAD~1", None).unwrap(),
        [change(
            ChangeKind::Renamed,
            "parts/bracket.mitcad",
            Some("part.mitcad")
        )]
    );
    let history = repo.history(&renamed).unwrap();
    let summary: Vec<(&str, &str)> = history
        .iter()
        .map(|v| (v.summary.as_str(), v.path.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("Rename", "parts/bracket.mitcad"),
            ("Delete Base1", "part.mitcad"),
            ("First", "part.mitcad")
        ]
    );
    assert_eq!(history[0].renamed_from.as_deref(), Some("part.mitcad"));
    // The old path's history ends with its deletion.
    let old = repo.history(&part).unwrap();
    assert_eq!(old.len(), 3);
    assert_eq!(old[0].blob, None);
}

/// Comparison of versions (P12c): two versions, and a version against the
/// file as saved, in the model's terms.
#[test]
fn versions_compare_their_designs() {
    let scratch = Scratch::new("diff");
    let (repo, mut doc, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    set_parameter(&mut doc, "d3", 25.0);
    doc.command(r#"{"cmd": "delete_feature", "uid": "F3"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    let diff = repo.diff(&part, "HEAD~1", Some("HEAD")).unwrap();
    assert_eq!(
        diff.summary,
        "d3 20 mm -> 25 mm, -1 feature, 1 feature modified, -1 body"
    );
    let features: Vec<&str> = diff.features.iter().map(|f| f.text.as_str()).collect();
    assert_eq!(
        features,
        [
            "Extrude1 (F2): distance 20 mm -> 25 mm",
            "Base1 (F3, base): deleted"
        ]
    );
    assert!(repo.diff(&part, "HEAD", None).unwrap().identical);
    // Saved, not recorded: the file against the latest version.
    doc.command(r#"{"cmd": "rename_feature", "uid": "F2", "name": "Plate"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let diff = repo.diff(&part, "HEAD", None).unwrap();
    assert_eq!(diff.features[0].text, "Plate (F2): renamed from Extrude1");
    // As a command.
    let path = serde_json::to_string(&part.to_string_lossy()).unwrap();
    let command = format!(r#"{{"cmd": "diff", "path": {path}, "from": "HEAD~1", "to": "HEAD"}}"#);
    let answer: serde_json::Value = serde_json::from_str(&repo.command(&command).unwrap()).unwrap();
    assert_eq!(answer["path"], "part.mitcad");
    assert_eq!(answer["from"], repo.resolve("HEAD~1").unwrap().as_str());
    assert_eq!(answer["to"], repo.resolve("HEAD").unwrap().as_str());
    assert_eq!(answer["features"][1]["kind"], "deleted");
    assert_eq!(answer["text"], diff_text(&repo, &part));
    let text = repo.command_text(&command).unwrap();
    let short = |rev: &str| repo.resolve(rev).unwrap()[..7].to_owned();
    assert!(
        text.starts_with(&format!(
            "Changes in part.mitcad from {} to {}:\nSummary: d3 20 mm -> 25 mm",
            short("HEAD~1"),
            short("HEAD")
        )),
        "{text}"
    );
    let saved = repo
        .command_text(&format!(
            r#"{{"cmd": "diff", "path": {path}, "from": "HEAD"}}"#
        ))
        .unwrap();
    assert!(
        saved.starts_with(&format!(
            "Changes in part.mitcad from {} to the saved file:\nSummary: 1 feature modified\n",
            short("HEAD")
        )),
        "{saved}"
    );
    // Through a rename: the older versions under the old path.
    repo.commit(std::slice::from_ref(&part), "Plate", &author())
        .unwrap();
    let renamed = repo.root().join("parts/bracket.mitcad");
    fs::create_dir_all(renamed.parent().unwrap()).unwrap();
    fs::rename(&part, &renamed).unwrap();
    repo.commit(&[part.clone(), renamed.clone()], "Rename", &author())
        .unwrap();
    let diff = repo.diff(&renamed, "HEAD~3", Some("HEAD")).unwrap();
    assert_eq!(
        diff.features[0].text,
        "Plate (F2): renamed from Extrude1; distance 20 mm -> 25 mm"
    );
    // A file that is not in the version.
    assert!(matches!(
        repo.diff(&repo.root().join("other.mitcad"), "HEAD", None),
        Err(VcsError::NotFound(_))
    ));
}

/// The text of the comparison of the last two versions of `part`.
fn diff_text(repo: &ProjectRepo, part: &Path) -> String {
    repo.diff(part, "HEAD~1", Some("HEAD")).unwrap().to_text()
}

#[test]
fn staged_changes_of_the_user_stay() {
    let scratch = Scratch::new("staged");
    let (repo, mut doc, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    // The user stages a file of their own.
    let notes = repo.root().join("notes.txt");
    fs::write(&notes, "staged by the user\n").unwrap();
    let staged = repo
        .repo
        .write_blob(b"staged by the user\n")
        .unwrap()
        .detach();
    {
        let mut index = repo.repo.open_index().unwrap();
        let metadata = gix::index::fs::Metadata::from_path_no_follow(&notes).unwrap();
        index.dangerously_push_entry(
            gix::index::entry::Stat::from_fs(&metadata).unwrap(),
            staged,
            gix::index::entry::Flags::empty(),
            gix::index::entry::Mode::FILE,
            b"notes.txt".as_bstr(),
        );
        index.sort_entries();
        index.write(Default::default()).unwrap();
    }
    set_parameter(&mut doc, "d3", 30.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let outcome = repo
        .commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    assert!(outcome.commit.is_some());
    // Still staged, and not committed.
    let index = repo.repo.open_index().unwrap();
    let entry = index
        .entry_by_path(b"notes.txt".as_bstr())
        .expect("still staged");
    assert_eq!(entry.id, staged);
    assert!(!tree_files(&repo).contains(&"notes.txt".to_owned()));
    // The committed file's entry is the new version's.
    let committed = index.entry_by_path(b"part.mitcad".as_bstr()).unwrap();
    assert_eq!(
        Some(committed.id),
        repo.entry_at(repo.head_commit().unwrap().unwrap(), "part.mitcad")
            .unwrap()
    );
    if has_git() {
        assert_eq!(
            git(repo.root(), &["status", "--porcelain"]).unwrap(),
            "A  notes.txt\n"
        );
    }
}

#[test]
fn another_commit_first_is_built_upon() {
    let scratch = Scratch::new("race");
    let (repo, mut doc, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    // Between building the tree and moving the branch, another program
    // commits a file of its own.
    let root = repo.root().to_path_buf();
    *repo.race.borrow_mut() = Some(Box::new(move || {
        let other = gix::open(&root).unwrap();
        let head = other.head_id().unwrap().detach();
        let base = other.find_commit(head).unwrap().tree_id().unwrap().detach();
        let blob = other.write_blob(b"from elsewhere\n").unwrap().detach();
        let mut editor = other.edit_tree(base).unwrap();
        editor
            .upsert("elsewhere.txt", gix::objs::tree::EntryKind::Blob, blob)
            .unwrap();
        let tree = editor.write().unwrap().detach();
        let signature = gix::actor::Signature {
            name: "Elsewhere".into(),
            email: "elsewhere@example.invalid".into(),
            time: gix::date::Time::now_local_or_utc(),
        };
        let (mut a, mut c) = Default::default();
        other
            .commit_as(
                signature.to_ref(&mut c),
                signature.to_ref(&mut a),
                "HEAD",
                "Elsewhere",
                tree,
                [head],
            )
            .unwrap();
    }));
    set_parameter(&mut doc, "d3", 35.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let outcome = repo
        .commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    let ours: ObjectId = outcome.commit.unwrap().parse().unwrap();
    let commit = repo.repo.find_commit(ours).unwrap();
    let parent = commit.parent_ids().next().unwrap().detach();
    assert_eq!(
        repo.repo
            .find_commit(parent)
            .unwrap()
            .message_raw()
            .unwrap()
            .to_str_lossy(),
        "Elsewhere"
    );
    assert_eq!(repo.head_commit().unwrap(), Some(ours));
    let files = tree_files(&repo);
    assert!(files.contains(&"elsewhere.txt".to_owned()), "{files:?}");
    assert_eq!(repo.read("HEAD", &part).unwrap(), fs::read(&part).unwrap());
}

#[test]
fn the_author_comes_from_git_or_from_the_settings() {
    let scratch = Scratch::new("author");
    let (repo, _, _) = project(&scratch);
    let isolated = || ProjectRepo::open_with(repo.root(), gix::open::Options::isolated()).unwrap();
    let settings = Identity::new("From Settings", "settings@example.invalid").unwrap();
    assert_eq!(isolated().identity(None), Err(VcsError::NoIdentity));
    assert_eq!(isolated().identity(Some(&settings)).unwrap(), settings);
    let config = repo.repo.git_dir().join("config");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str("[user]\n\tname = From Git\n\temail = git@example.invalid\n");
    fs::write(&config, text).unwrap();
    let configured = Identity::new("From Git", "git@example.invalid").unwrap();
    assert_eq!(isolated().identity(Some(&settings)).unwrap(), configured);
    // The JSON commands take an author, else git's, else the fallback.
    let answer: serde_json::Value = serde_json::from_str(
        &isolated()
            .command(r#"{"cmd": "identity", "fallback_author": "From Settings <settings@example.invalid>"}"#)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        answer,
        serde_json::json!({"name": "From Git", "email": "git@example.invalid"})
    );
}

#[test]
fn no_version_on_a_detached_head() {
    let scratch = Scratch::new("detached");
    let (repo, mut doc, part) = project(&scratch);
    let first = repo
        .commit(std::slice::from_ref(&part), "First", &author())
        .unwrap()
        .commit
        .unwrap();
    fs::write(repo.repo.git_dir().join("HEAD"), format!("{first}\n")).unwrap();
    set_parameter(&mut doc, "d3", 40.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let outcome = repo
        .commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    assert!(outcome.commit.is_none());
    assert!(outcome.skipped.unwrap().contains("detached"));
    assert_eq!(repo.branch().unwrap(), None);
}

#[test]
fn restore_records_an_older_version_as_a_new_one() {
    let scratch = Scratch::new("restore");
    let (repo, mut doc, part) = project(&scratch);
    let first_text = fs::read(&part).unwrap();
    let first = repo
        .commit(std::slice::from_ref(&part), "First", &author())
        .unwrap()
        .commit
        .unwrap();
    doc.command(r#"{"cmd": "delete_feature", "uid": "F3"}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Delete Base1", &author())
        .unwrap();
    assert!(!repo.root().join(brep_file("6 bolt")).exists());

    let outcome = repo.restore(&part, &first, None, &author()).unwrap();
    assert!(outcome.commit.is_some());
    assert_eq!(fs::read(&part).unwrap(), first_text);
    // The B-rep file came back from the version.
    assert!(repo.root().join(brep_file("6 bolt")).is_file());
    assert!(tree_files(&repo).contains(&brep_file("6 bolt")));
    let history = repo.history(&part).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0].summary,
        format!("Restore part.mitcad from {}", &first[..7])
    );
    assert_eq!(history[0].blob, history[2].blob);
    let mut back = Document::load_project(&part, MockKernel::default()).unwrap();
    assert!(
        back.load_warnings().is_empty(),
        "{:?}",
        back.load_warnings()
    );
    assert_eq!(bodies(&mut back).len(), 3);
}

/// The Version History window (P12e): what each version changed against
/// the one before, through a rename and around a deletion; opening and
/// restoring an older version follow the rename.
#[test]
fn versions_summarise_their_changes_and_follow_renames() {
    let scratch = Scratch::new("summaries");
    let (repo, mut doc, part) = project(&scratch);
    let first_text = fs::read(&part).unwrap();
    let first = repo
        .commit(std::slice::from_ref(&part), "First", &author())
        .unwrap()
        .commit
        .unwrap();
    set_parameter(&mut doc, "d3", 25.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    let renamed = repo.root().join("plate.mitcad");
    fs::rename(&part, &renamed).unwrap();
    repo.commit(&[part.clone(), renamed.clone()], "Rename", &author())
        .unwrap();
    set_parameter(&mut doc, "d3", 30.0);
    doc.save_file(&renamed, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&renamed), "Fourth", &author())
        .unwrap();

    let history = repo.history(&renamed).unwrap();
    let summaries: Vec<Option<String>> = repo
        .summaries(&history)
        .into_iter()
        .map(|s| s.map(Result::unwrap))
        .collect();
    let diff = |from: &str, to: &str| repo.diff(&renamed, from, Some(to)).unwrap().summary;
    assert_eq!(
        summaries,
        [
            Some(diff("HEAD~1", "HEAD")),
            Some("no changes".to_owned()), // the rename changed no design
            Some(diff("HEAD~3", "HEAD~2")),
            None, // the first version
        ]
    );
    assert!(
        summaries[0]
            .as_deref()
            .unwrap()
            .starts_with("d3 25 mm -> 30 mm"),
        "{summaries:?}"
    );
    // The old path's history ends with its deletion: nothing to compare.
    let old = repo.history(&part).unwrap();
    let around: Vec<bool> = repo.summaries(&old).iter().map(Option::is_some).collect();
    assert_eq!(around, [false, true, false]);
    // As a command: `changes` with each version, null for the first.
    let path = serde_json::to_string(&renamed.to_string_lossy()).unwrap();
    let answer: serde_json::Value = serde_json::from_str(
        &repo
            .command(&format!(
                r#"{{"cmd": "history", "path": {path}, "summaries": true}}"#
            ))
            .unwrap(),
    )
    .unwrap();
    let changes: Vec<serde_json::Value> = answer["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["changes"].clone())
        .collect();
    assert_eq!(changes[0], serde_json::json!(summaries[0]));
    assert_eq!(changes[3], serde_json::Value::Null);

    // The first version, under the path it had then.
    let mut opened = repo
        .load_version(&first, &renamed, MockKernel::default())
        .unwrap();
    assert_eq!(bodies(&mut opened).len(), 3);
    let outcome = repo.restore(&renamed, &first, None, &author()).unwrap();
    assert!(outcome.commit.is_some());
    assert_eq!(fs::read(&renamed).unwrap(), first_text);
    assert!(!part.exists());
    let history = repo.history(&renamed).unwrap();
    assert_eq!(
        history[0].summary,
        format!("Restore plate.mitcad from {}", &first[..7])
    );
    assert_eq!(history[0].blob, history[4].blob);
}

#[test]
fn a_project_is_created_before_its_first_version() {
    let scratch = Scratch::new("create");
    let dir = scratch.join("bracket");
    assert_eq!(repository_root(&dir), None);
    let repo = ProjectRepo::create(&dir).unwrap();
    assert_eq!(repo.head_commit().unwrap(), None);
    assert!(dir.join(PROJECT_MARKER).is_file());
    // Opened again, it has history (but no version), and a folder below
    // it is in its repository.
    let opened = ProjectRepo::open(&dir.join("part.mitcad")).unwrap();
    assert_eq!(opened.branch().unwrap().as_deref(), Some("main"));
    assert!(same_folder(
        &repository_root(&dir.join("sub/folder")).unwrap(),
        &dir
    ));
    // init records the first version in it.
    let (repo, outcome) = ProjectRepo::init(&dir, Some(&author())).unwrap();
    assert!(outcome.commit.is_some());
    assert_eq!(
        tree_files(&repo),
        [".gitattributes", ".gitignore", ".mitcad/project.json"]
    );
}

#[test]
fn a_file_renamed_outside_mitcad_takes_its_display_state_along() {
    let scratch = Scratch::new("rename");
    let (repo, mut doc, part) = project(&scratch);
    doc.command(r#"{"cmd": "set_origin_visible", "visible": true}"#)
        .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let local = |file: &Path| {
        Project::open(repo.root())
            .unwrap()
            .local_state_path(file)
            .unwrap()
    };
    assert!(local(&part).is_file());
    // Renamed in a file manager.
    let plate = repo.root().join("parts/plate.mitcad");
    fs::create_dir_all(plate.parent().unwrap()).unwrap();
    fs::rename(&part, &plate).unwrap();
    let status = repo.status(&plate).unwrap();
    assert_eq!(status.head, None);
    assert_eq!(status.renamed_from.as_deref(), Some("part.mitcad"));
    let answer: serde_json::Value = serde_json::from_str(
        &repo
            .command(&format!(
                r#"{{"cmd": "follow_rename", "path": {}}}"#,
                serde_json::to_string(&plate.to_string_lossy()).unwrap()
            ))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        answer,
        serde_json::json!({"renamed_from": "part.mitcad", "display_moved": true})
    );
    assert!(!local(&part).exists() && local(&plate).is_file());
    let opened = Document::load_project(&plate, MockKernel::default()).unwrap();
    assert!(opened.display().origin);
    // A version records the rename: the old path with the new one, the
    // content as it was (the history follows such renames); then the
    // change.
    let outcome = repo
        .commit(&[plate.clone(), part.clone()], "Rename", &author())
        .unwrap();
    assert_eq!(outcome.written, ["parts/plate.mitcad"]);
    assert_eq!(outcome.removed, ["part.mitcad"]);
    assert_eq!(repo.status(&plate).unwrap().renamed_from, None);
    set_parameter(&mut doc, "d3", 25.0);
    doc.save_file(&plate, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&plate), "Change d3", &author())
        .unwrap();
    let history = repo.history(&plate).unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[1].renamed_from.as_deref(), Some("part.mitcad"));
    // A copy is no rename (the old path is still there); a file deleted in
    // a version leaves no display state behind.
    fs::copy(&plate, repo.root().join("copy.mitcad")).unwrap();
    assert_eq!(
        repo.status(&repo.root().join("copy.mitcad"))
            .unwrap()
            .renamed_from,
        None
    );
    fs::remove_file(&plate).unwrap();
    assert!(local(&plate).is_file());
    repo.commit(std::slice::from_ref(&plate), "Delete", &author())
        .unwrap();
    assert!(!local(&plate).exists());
}

#[test]
fn json_commands() {
    let scratch = Scratch::new("api");
    let dir = scratch.join("project");
    let (repo, answer) = api::init(&dir, "Mitcad Test <test@example.invalid>").unwrap();
    let answer: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(answer["branch"], "main");
    assert!(answer["commit"].is_string());
    let part = repo.root().join("part.mitcad");
    design().save_file(&part, FileFormat::Auto).unwrap();
    let path = serde_json::to_string(&part.to_string_lossy()).unwrap();
    let run = |command: String| -> serde_json::Value {
        serde_json::from_str(&repo.command(&command).unwrap()).unwrap()
    };
    let status = run(format!(r#"{{"cmd": "status", "path": {path}}}"#));
    assert_eq!(status["head"], serde_json::Value::Null);
    assert_eq!(status["modified"], true);
    let commit = run(format!(
        r#"{{"cmd": "commit", "paths": [{path}], "message": "First", "author": "Mitcad Test <test@example.invalid>"}}"#
    ));
    let id = commit["commit"].as_str().unwrap().to_owned();
    assert_eq!(commit["files"], "part.mitcad");
    let status = run(format!(r#"{{"cmd": "status", "path": {path}}}"#));
    assert_eq!(status["modified"], false);
    let history = run(format!(r#"{{"cmd": "history", "path": {path}}}"#));
    assert_eq!(history["versions"][0]["id"], id.as_str());
    assert_eq!(history["versions"][0]["author"]["name"], "Mitcad Test");
    let read = run(format!(
        r#"{{"cmd": "read_version", "path": {path}, "rev": "HEAD"}}"#
    ));
    assert_eq!(
        read["text"].as_str().unwrap().as_bytes(),
        fs::read(&part).unwrap()
    );
    let changes = run(r#"{"cmd": "changes", "from": "HEAD~1"}"#.to_owned());
    assert_eq!(changes["to"], id.as_str());
    assert_eq!(changes["changes"].as_array().unwrap().len(), 3);
    let text = repo
        .command_text(&format!(r#"{{"cmd": "history", "path": {path}}}"#))
        .unwrap();
    assert!(
        text.starts_with("Versions of part.mitcad (1), newest first:\n  v1   "),
        "{text}"
    );
    assert!(text.trim_end().ends_with("Mitcad Test  First"), "{text}");
    let text = repo
        .command_text(r#"{"cmd": "changes", "from": "HEAD~1"}"#)
        .unwrap();
    assert!(text.contains("A part.mitcad\n"), "{text}");
    let again = repo
        .command_text(&format!(
            r#"{{"cmd": "commit", "paths": [{path}], "author": "A <a@b.c>"}}"#
        ))
        .unwrap();
    assert_eq!(
        again,
        "No changes since the latest version of part.mitcad\n"
    );
    for bad in [
        r#"{"cmd": "frobnicate"}"#,
        r#"{"cmd": "history"}"#,
        "not json",
    ] {
        assert!(
            matches!(repo.command(bad), Err(VcsError::Command(_))),
            "{bad}"
        );
    }
}

/// The repository as the system's git sees it: consistent, clean, the same
/// history, and an index that git's own commits build on correctly.
#[test]
fn the_system_git_agrees() {
    if !has_git() {
        return;
    }
    let scratch = Scratch::new("git");
    let (repo, mut doc, part) = project(&scratch);
    let root = repo.root().to_path_buf();
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    // git commits a file of its own (and writes its tree cache).
    fs::write(root.join("readme.txt"), "a project\n").unwrap();
    git(&root, &["add", "readme.txt"]).unwrap();
    git(&root, &["commit", "-q", "-m", "Readme"]).unwrap();
    for (d3, delete) in [(25.0, None), (30.0, Some("F3"))] {
        set_parameter(&mut doc, "d3", d3);
        if let Some(uid) = delete {
            doc.command(&format!(r#"{{"cmd": "delete_feature", "uid": "{uid}"}}"#))
                .unwrap();
        }
        doc.save_file(&part, FileFormat::Auto).unwrap();
        repo.commit(
            std::slice::from_ref(&part),
            &format!("d3 = {d3} mm"),
            &author(),
        )
        .unwrap();
    }
    git(&root, &["fsck", "--strict", "--no-progress"]).unwrap();
    assert_eq!(git(&root, &["status", "--porcelain"]).unwrap(), "");
    let ids: Vec<String> = repo
        .history(&part)
        .unwrap()
        .into_iter()
        .map(|v| v.id)
        .collect();
    let logged = git(
        &root,
        &["log", "--first-parent", "--format=%H", "--", "part.mitcad"],
    )
    .unwrap();
    assert_eq!(logged.lines().collect::<Vec<_>>(), ids);
    let shown = git(&root, &["show", &format!("{}:part.mitcad", ids[1])]).unwrap();
    assert_eq!(shown.as_bytes(), repo.read(&ids[1], &part).unwrap());
    let files = git(&root, &["ls-files", ".mitcad/brep"]).unwrap();
    assert_eq!(files.lines().collect::<Vec<_>>(), [brep_file("4 wedge")]);
    // git's next commit takes the latest version, not a stale tree.
    fs::write(root.join("todo.txt"), "more\n").unwrap();
    git(&root, &["add", "todo.txt"]).unwrap();
    git(&root, &["commit", "-q", "-m", "Todo"]).unwrap();
    assert_eq!(
        git(&root, &["diff", "--name-only", "HEAD~1", "HEAD"]).unwrap(),
        "todo.txt\n"
    );
    assert_eq!(
        git(&root, &["show", "HEAD:part.mitcad"])
            .unwrap()
            .as_bytes(),
        fs::read(&part).unwrap()
    );
    let author_line = git(&root, &["log", "-1", "--format=%an <%ae>", "HEAD~1"]).unwrap();
    assert_eq!(author_line.trim(), author().to_string());
}

/// Times of commits and of the history (run in release builds:
/// `cargo test -p mitcad-vcs --release -- --ignored --nocapture`).
#[test]
#[ignore]
fn measure_commits_and_history() {
    let scratch = Scratch::new("measure");
    let (repo, mut doc, part) = project(&scratch);
    // A large B-rep file (about 1.2 MB compressed), as big imported bodies
    // give.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let big: String = (0..2_000_000)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            char::from(b'a' + ((state >> 33) % 26) as u8)
        })
        .collect();
    doc.command(&format!(
        r#"{{"cmd": "add_feature", "def": {{"type": "base", "bodies": [{{"name": "Big", "brep": {}}}]}}}}"#,
        brep(&format!("8 {big}"))
    ))
    .unwrap();
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let start = Instant::now();
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    println!("first commit (B-rep files written): {:?}", start.elapsed());
    let commits = 1000;
    let mut all = std::time::Duration::ZERO;
    let mut slowest = std::time::Duration::ZERO;
    for i in 0..commits {
        set_parameter(&mut doc, "d3", 21.0 + f64::from(i));
        doc.save_file(&part, FileFormat::Auto).unwrap();
        let one = Instant::now();
        assert!(
            repo.commit(std::slice::from_ref(&part), "", &author())
                .unwrap()
                .commit
                .is_some()
        );
        all += one.elapsed();
        slowest = slowest.max(one.elapsed());
    }
    println!(
        "{commits} commits: {:?} each, slowest {slowest:?}",
        all / commits
    );
    let start = Instant::now();
    let history = repo.history(&part).unwrap();
    println!(
        "history of {} versions: {:?}",
        history.len(),
        start.elapsed()
    );
    assert_eq!(history.len(), commits as usize + 1);
    let start = Instant::now();
    let mut old = repo
        .load_version("HEAD~500", &part, MockKernel::default())
        .unwrap();
    println!("version 500 back read: {:?}", start.elapsed());
    assert!(old.load_warnings().is_empty());
    bodies(&mut old);
}

/// Times of commits of a large project file of version 3 that
/// `MITCAD_VCS_LARGE` names, with its B-rep files, copied into a project
/// with history (release builds: `MITCAD_VCS_LARGE=<file> cargo test -p
/// mitcad-vcs --release -- --ignored --nocapture large`).
#[test]
#[ignore]
fn measure_a_large_project() {
    let Some(source) = std::env::var_os("MITCAD_VCS_LARGE").map(PathBuf::from) else {
        eprintln!("MITCAD_VCS_LARGE is not set");
        return;
    };
    let scratch = Scratch::new("large");
    let (repo, _) = ProjectRepo::init(&scratch.join("project"), Some(&author())).unwrap();
    let text = fs::read_to_string(&source).unwrap();
    let references = mitcad_model::file::brep_references(&text).unwrap();
    let from = Project::find(&source).unwrap().store();
    let to = Project::open(repo.root()).unwrap().store();
    let mut bytes = 0;
    for sha256 in &references {
        use mitcad_model::BlobStore;
        let content = from.get(sha256).unwrap().unwrap();
        bytes += content.len();
        to.put(sha256, &content).unwrap();
    }
    let part = repo.root().join("large.mitcad");
    fs::write(&part, &text).unwrap();
    println!(
        "{} bytes of JSON, {} B-rep files of {bytes} bytes",
        text.len(),
        references.len()
    );
    let start = Instant::now();
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    println!("first commit (all B-rep files): {:?}", start.elapsed());
    // Saves that change the project file only, as a parameter change does.
    let saves = 50;
    let mut all = std::time::Duration::ZERO;
    for i in 1..=saves {
        fs::write(&part, format!("{text}{}", "\n".repeat(i))).unwrap();
        let one = Instant::now();
        let outcome = repo
            .commit(std::slice::from_ref(&part), "", &author())
            .unwrap();
        all += one.elapsed();
        assert_eq!(outcome.written, ["large.mitcad"]);
    }
    println!(
        "{saves} commits of the project file: {:?} each",
        all / saves as u32
    );
    let start = Instant::now();
    assert_eq!(repo.history(&part).unwrap().len(), saves + 1);
    println!("history of {} versions: {:?}", saves + 1, start.elapsed());
    let start = Instant::now();
    let old = repo
        .load_version("HEAD~25", &part, MockKernel::default())
        .unwrap();
    assert!(old.load_warnings().is_empty(), "{:?}", old.load_warnings());
    println!(
        "an older version read with its B-rep data: {:?}",
        start.elapsed()
    );
}
