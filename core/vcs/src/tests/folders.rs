// SPDX-License-Identifier: MIT
//! A project's folders and designs (mitcad#144) in temporary projects:
//! `project_tree` with nested folders, versions, changes and locks;
//! `create_folder`, `new_design`, `move` and `delete`, each one version
//! with Save's rules; a moved folder whose designs refer to B-rep files
//! (the store stays consistent), a rename that keeps the display state and
//! the design opened last, every refusal, the dot-folder rule, and a Cloud
//! project's push (a bare repository stands in for the cloud). Without the
//! system's git the checks that need it are skipped.

use serde_json::{Value, json};

use super::remote::{AUTHOR, bare, connected, error_class, run, system_git, tip};
use super::*;
use crate::projects::{FOLDER_PLACEHOLDER, inspect_folder};
use crate::remote::clone_project;

const SA: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const SB: &str = "bbbbbbbb-0000-4000-8000-000000000002";
const OTHER: &str = "Other Tester <other@example.invalid>";

/// A project with version history and nothing else.
fn empty_project(scratch: &Scratch) -> ProjectRepo {
    ProjectRepo::init(&scratch.join("project"), Some(&author()))
        .unwrap()
        .0
}

/// Runs a write that must succeed: no error, a version recorded.
fn done(repo: &ProjectRepo, mut command: Value) -> Value {
    command["author"] = json!(AUTHOR);
    let answer = run(repo, command);
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert!(answer["commit"].is_string(), "{answer}");
    answer
}

/// Runs a write that must be refused with `class`, changing nothing:
/// HEAD and the folder's files stay.
fn refused(repo: &ProjectRepo, mut command: Value, class: &str) -> Value {
    command["author"] = json!(AUTHOR);
    let head = repo.head_commit().unwrap();
    let before = files_of(repo.root());
    let answer = run(repo, command);
    assert_eq!(error_class(&answer), class, "{answer}");
    assert_eq!(answer["commit"], Value::Null, "{answer}");
    assert!(answer["error"]["message"].is_string(), "{answer}");
    assert_eq!(repo.head_commit().unwrap(), head, "{answer}");
    assert_eq!(files_of(repo.root()), before, "{answer}");
    answer
}

/// The files under a folder (relative, `/`), leaving out `.git`.
fn files_of(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut folders = vec![(root.to_path_buf(), String::new())];
    while let Some((folder, prefix)) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".git" {
                continue;
            }
            let path = format!("{prefix}{name}");
            if entry.file_type().unwrap().is_dir() {
                folders.push((entry.path(), format!("{path}/")));
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn tree(repo: &ProjectRepo) -> Value {
    run(
        repo,
        json!({"cmd": "project_tree", "session": SA, "author": AUTHOR}),
    )
}

/// A design of `project_tree`'s answer.
fn design_in<'a>(tree: &'a Value, path: &str) -> &'a Value {
    tree["designs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|design| design["path"] == path)
        .unwrap_or_else(|| panic!("{path} is not listed: {tree}"))
}

fn paths(tree: &Value, key: &str) -> Vec<String> {
    tree[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            item.as_str()
                .map_or_else(|| item["path"].as_str().unwrap().to_owned(), str::to_owned)
        })
        .collect()
}

/// The newest version's summary.
fn last_message(repo: &ProjectRepo) -> String {
    let head = repo.head_commit().unwrap().unwrap();
    let commit = repo.repo.find_commit(head).unwrap();
    let message = commit.message_raw().unwrap().to_str_lossy().into_owned();
    crate::commit::user_message(&message).to_owned()
}

/// As git sees it: no damage, nothing changed in the folder.
fn clean(root: &Path) {
    if !has_git() {
        return;
    }
    git(root, &["fsck", "--strict", "--no-progress"]).unwrap();
    assert_eq!(
        git(root, &["status", "--porcelain"]).unwrap(),
        "",
        "{root:?}"
    );
}

#[test]
fn an_empty_design_is_a_new_documents_file() {
    assert_eq!(
        mitcad_model::file::empty_design(),
        Document::new_design(MockKernel::default()).to_json()
    );
}

#[test]
fn a_new_design_takes_the_units_given() {
    // mitcad#1036: the application's default units for new designs.
    assert_eq!(
        mitcad_model::file::empty_design_in(mitcad_model::expr::LengthUnit::Inch),
        Document::new_design_in(MockKernel::default(), mitcad_model::expr::LengthUnit::Inch)
            .to_json()
    );
    let scratch = Scratch::new("folders-units");
    let repo = empty_project(&scratch);
    done(
        &repo,
        json!({"cmd": "new_design", "path": "inch.mitcad", "units": "in"}),
    );
    let text = fs::read_to_string(repo.root().join("inch.mitcad")).unwrap();
    let file: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(file["units"]["length"], "in", "{text}");
    let opened = Document::from_json(&text, MockKernel::default()).unwrap();
    assert_eq!(
        opened.parameters().context().default_length_unit,
        mitcad_model::expr::LengthUnit::Inch
    );
    // Without `units` (and with null) millimetres, as before.
    done(
        &repo,
        json!({"cmd": "new_design", "path": "metric.mitcad", "units": null}),
    );
    let text = fs::read_to_string(repo.root().join("metric.mitcad")).unwrap();
    assert_eq!(text, mitcad_model::file::empty_design());
    // Not a length unit, not a name, or with a design's own text: refused,
    // nothing written.
    for command in [
        json!({"cmd": "new_design", "path": "a.mitcad", "units": "deg", "author": AUTHOR}),
        json!({"cmd": "new_design", "path": "b.mitcad", "units": "furlong", "author": AUTHOR}),
        json!({"cmd": "new_design", "path": "c.mitcad", "units": 25.4, "author": AUTHOR}),
        json!({"cmd": "new_design", "path": "d.mitcad", "units": "in", "author": AUTHOR,
               "text": mitcad_model::file::empty_design()}),
    ] {
        assert!(repo.command(&command.to_string()).is_err(), "{command}");
    }
    for name in ["a", "b", "c", "d"] {
        assert!(
            !repo.root().join(format!("{name}.mitcad")).exists(),
            "{name}"
        );
    }
}

#[test]
fn nested_folders_and_new_designs_are_versions() {
    let scratch = Scratch::new("folders-nested");
    let repo = empty_project(&scratch);
    let made = done(&repo, json!({"cmd": "create_folder", "path": "parts"}));
    assert_eq!(made["path"], "parts");
    assert_eq!(last_message(&repo), "Add folder parts");
    // Folders above a new one are made with it.
    done(
        &repo,
        json!({"cmd": "create_folder", "path": "parts/sub/deep"}),
    );
    assert_eq!(last_message(&repo), "Add folder parts/sub/deep");
    let added = done(
        &repo,
        json!({"cmd": "new_design", "path": "parts/sub/bracket.mitcad"}),
    );
    assert_eq!(last_message(&repo), "Add parts/sub/bracket.mitcad");
    assert_eq!(added["written"], json!(["parts/sub/bracket.mitcad"]));
    // An empty design that opens.
    let text = fs::read_to_string(repo.root().join("parts/sub/bracket.mitcad")).unwrap();
    assert_eq!(text, mitcad_model::file::empty_design());
    Document::from_json(&text, MockKernel::default()).unwrap();
    // An absolute path inside the project does too, and a message of its own.
    let absolute = repo.root().join("top.mitcad");
    done(
        &repo,
        json!({"cmd": "new_design", "path": absolute, "message": "A top design"}),
    );
    assert_eq!(last_message(&repo), "A top design");

    let files = tree_files(&repo);
    for path in [
        format!("parts/{FOLDER_PLACEHOLDER}"),
        format!("parts/sub/deep/{FOLDER_PLACEHOLDER}"),
        "parts/sub/bracket.mitcad".to_owned(),
        "top.mitcad".to_owned(),
    ] {
        assert!(files.contains(&path), "{path}: {files:?}");
    }

    let listed = tree(&repo);
    assert_eq!(listed["kind"], "local", "{listed}");
    assert_eq!(listed["remote"], Value::Null);
    assert_eq!(listed["ahead"], Value::Null);
    assert_eq!(listed["truncated"], false);
    assert_eq!(listed["error"], Value::Null);
    assert_eq!(
        paths(&listed, "folders"),
        ["parts", "parts/sub", "parts/sub/deep"]
    );
    // The placeholder is never listed.
    assert_eq!(
        paths(&listed, "designs"),
        ["parts/sub/bracket.mitcad", "top.mitcad"]
    );
    let bracket = design_in(&listed, "parts/sub/bracket.mitcad");
    assert_eq!(bracket["recorded"], true);
    assert_eq!(bracket["changed"], false);
    assert_eq!(bracket["lock"], Value::Null);
    assert_eq!(bracket["version"]["number"], 1, "{bracket}");
    assert_eq!(bracket["version"]["author"]["name"], "Mitcad Test");
    assert!(bracket["version"]["date"].as_str().unwrap().ends_with('Z'));
    let text = repo
        .command_text(&json!({"cmd": "project_tree"}).to_string())
        .unwrap();
    assert!(text.contains("Local project"), "{text}");
    assert!(text.contains("  parts/sub/\n"), "{text}");
    assert!(text.contains("parts/sub/bracket.mitcad  v1 "), "{text}");
    clean(repo.root());
}

#[test]
fn versions_changes_and_the_dot_folder_rule_in_the_tree() {
    let scratch = Scratch::new("folders-tree");
    let (repo, mut doc, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    set_parameter(&mut doc, "d3", 25.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    repo.commit(std::slice::from_ref(&part), "Second", &author())
        .unwrap();
    let listed = tree(&repo);
    let design = design_in(&listed, "part.mitcad");
    assert_eq!(design["version"]["number"], 2, "{design}");
    assert_eq!(design["changed"], false);

    // Changed since the latest version; a design never recorded.
    set_parameter(&mut doc, "d3", 30.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    fs::create_dir_all(repo.root().join("drafts")).unwrap();
    doc.save_file(&repo.root().join("drafts/new.mitcad"), FileFormat::Auto)
        .unwrap();
    // A dot-folder is left out of the tree, the listing and the commit's
    // walk alike.
    fs::create_dir_all(repo.root().join(".hidden")).unwrap();
    fs::write(repo.root().join(".hidden/secret.mitcad"), "{}").unwrap();
    let listed = tree(&repo);
    let design = design_in(&listed, "part.mitcad");
    assert_eq!(design["changed"], true, "{design}");
    assert_eq!(design["version"]["number"], 2);
    let draft = design_in(&listed, "drafts/new.mitcad");
    assert_eq!(draft["recorded"], false);
    assert_eq!(draft["changed"], true);
    assert_eq!(draft["version"], Value::Null);
    assert_eq!(paths(&listed, "folders"), ["drafts"]);
    assert_eq!(
        paths(&listed, "designs"),
        ["drafts/new.mitcad", "part.mitcad"]
    );
    let inspected = inspect_folder(repo.root()).unwrap();
    let mut listed_designs: Vec<String> =
        inspected.designs.iter().map(|d| d.path.clone()).collect();
    listed_designs.sort();
    assert_eq!(listed_designs, ["drafts/new.mitcad", "part.mitcad"]);
    let in_walk = crate::commit::design_files_in(repo.root());
    assert!(
        in_walk
            .iter()
            .all(|file| !file.to_string_lossy().contains(".hidden")),
        "{in_walk:?}"
    );
}

#[test]
fn a_moved_folder_keeps_its_designs_b_rep_files() {
    let scratch = Scratch::new("folders-move-brep");
    let (repo, mut doc, part) = project(&scratch);
    let before = bodies(&mut doc);
    let design = repo.root().join("parts/a.mitcad");
    fs::create_dir_all(design.parent().unwrap()).unwrap();
    doc.save_file(&design, FileFormat::Auto).unwrap();
    repo.commit(&[part.clone(), design.clone()], "Designs", &author())
        .unwrap();
    // The folder also holds a file the versions lack: it moves along and
    // stays unrecorded.
    fs::write(repo.root().join("parts/notes.txt"), "notes").unwrap();

    let moved = done(
        &repo,
        json!({"cmd": "move", "from": "parts", "to": "old/parts"}),
    );
    assert_eq!(last_message(&repo), "Move parts to old/parts");
    assert_eq!(
        moved["moved"],
        json!([{"from": "parts/a.mitcad", "to": "old/parts/a.mitcad"}])
    );
    let files = tree_files(&repo);
    assert!(
        files.contains(&"old/parts/a.mitcad".to_owned()),
        "{files:?}"
    );
    assert!(!files.contains(&"parts/a.mitcad".to_owned()), "{files:?}");
    assert!(!files.iter().any(|f| f.ends_with("notes.txt")), "{files:?}");
    assert!(repo.root().join("old/parts/notes.txt").is_file());
    assert!(!repo.root().join("parts").exists());
    // The B-rep files both designs refer to stay, in the version and in
    // the folder.
    for data in ["6 bolt", "4 wedge"] {
        assert!(files.contains(&brep_file(data)), "{data}: {files:?}");
        assert!(repo.root().join(brep_file(data)).is_file(), "{data}");
    }
    assert!(moved["deleted"].as_array().unwrap().is_empty(), "{moved}");
    // The moved design opens with the same bodies from the version, and
    // its history follows the move.
    let mut opened = repo
        .load_version(
            "HEAD",
            &repo.root().join("old/parts/a.mitcad"),
            MockKernel::default(),
        )
        .unwrap();
    assert_eq!(bodies(&mut opened), before);
    let history = repo
        .history(&repo.root().join("old/parts/a.mitcad"))
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].renamed_from.as_deref(), Some("parts/a.mitcad"));
    let listed = tree(&repo);
    let moved_design = design_in(&listed, "old/parts/a.mitcad");
    assert_eq!(moved_design["version"]["number"], 2, "{moved_design}");
    assert_eq!(moved_design["changed"], false);
    assert_eq!(paths(&listed, "folders"), ["old", "old/parts"]);

    // Deleting the design that stays leaves the other's B-rep files.
    done(&repo, json!({"cmd": "delete", "path": "part.mitcad"}));
    let files = tree_files(&repo);
    for data in ["6 bolt", "4 wedge"] {
        assert!(files.contains(&brep_file(data)), "{data}: {files:?}");
    }
    if has_git() {
        // Only the unrecorded notes differ from the version.
        assert_eq!(
            git(repo.root(), &["status", "--porcelain"]).unwrap().trim(),
            "?? old/parts/notes.txt"
        );
        git(repo.root(), &["fsck", "--strict", "--no-progress"]).unwrap();
    }
}

#[test]
fn a_rename_keeps_the_display_state_and_the_design_opened_last() {
    let scratch = Scratch::new("folders-rename");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let project = mitcad_model::Project::open(repo.root()).unwrap();
    let state = project.local_state_path(&part).unwrap();
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    fs::write(&state, "{\"format\": \"mitcad-local\", \"version\": 1}\n").unwrap();
    run(
        &repo,
        json!({"cmd": "remember_design", "path": part.to_string_lossy()}),
    );

    let moved = done(
        &repo,
        json!({"cmd": "move", "from": "part.mitcad", "to": "parts/renamed.mitcad"}),
    );
    assert_eq!(
        last_message(&repo),
        "Move part.mitcad to parts/renamed.mitcad"
    );
    assert_eq!(moved["display_moved"], 1, "{moved}");
    let renamed = repo.root().join("parts/renamed.mitcad");
    assert!(renamed.is_file());
    assert!(!part.exists());
    assert!(!state.exists());
    assert!(project.local_state_path(&renamed).unwrap().is_file());
    let inspected = inspect_folder(repo.root()).unwrap();
    assert_eq!(
        inspected.last_design.as_deref(),
        Some("parts/renamed.mitcad")
    );
    let files = tree_files(&repo);
    assert!(files.contains(&"parts/renamed.mitcad".to_owned()));
    assert!(!files.contains(&"part.mitcad".to_owned()));
    let text = repo
        .command_text(
            &json!({"cmd": "move", "from": "parts/renamed.mitcad", "to": "parts/again.mitcad",
                    "author": AUTHOR})
            .to_string(),
        )
        .unwrap();
    assert!(
        text.contains("Moved parts/renamed.mitcad to parts/again.mitcad"),
        "{text}"
    );
    assert!(text.contains("Recorded version "), "{text}");
    clean(repo.root());
}

#[test]
fn a_deleted_design_or_folder_stays_in_the_history() {
    let scratch = Scratch::new("folders-delete");
    let (repo, _, part) = project(&scratch);
    let nested = repo.root().join("parts/a.mitcad");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::copy(&part, &nested).unwrap();
    let other = repo.root().join("box/b.mitcad");
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    fs::copy(&part, &other).unwrap();
    repo.commit(
        &[part.clone(), nested.clone(), other.clone()],
        "Designs",
        &author(),
    )
    .unwrap();
    let recorded = repo.resolve("HEAD").unwrap();

    // The only design of a folder: the folder stays, kept by its
    // placeholder.
    let deleted = done(&repo, json!({"cmd": "delete", "path": "parts/a.mitcad"}));
    assert_eq!(last_message(&repo), "Delete parts/a.mitcad");
    assert_eq!(deleted["deleted_designs"], json!(["parts/a.mitcad"]));
    assert!(!nested.exists());
    assert!(repo.root().join("parts").is_dir());
    let files = tree_files(&repo);
    assert!(!files.contains(&"parts/a.mitcad".to_owned()));
    assert!(
        files.contains(&format!("parts/{FOLDER_PLACEHOLDER}")),
        "{files:?}"
    );
    assert!(!repo.read(&recorded, &nested).unwrap().is_empty());
    assert_eq!(paths(&tree(&repo), "folders"), ["box", "parts"]);

    // A folder with its designs.
    let deleted = done(&repo, json!({"cmd": "delete", "path": "box"}));
    assert_eq!(deleted["deleted_designs"], json!(["box/b.mitcad"]));
    assert!(!repo.root().join("box").exists());
    assert!(!tree_files(&repo).iter().any(|f| f.starts_with("box/")));
    assert!(!repo.read(&recorded, &other).unwrap().is_empty());
    // The B-rep files the remaining design refers to stay.
    assert!(tree_files(&repo).contains(&brep_file("6 bolt")));
    clean(repo.root());
}

#[test]
fn every_refusal_changes_nothing() {
    let scratch = Scratch::new("folders-refusals");
    let (repo, mut doc, part) = project(&scratch);
    let nested = repo.root().join("parts/sub/a.mitcad");
    fs::create_dir_all(nested.parent().unwrap()).unwrap();
    fs::copy(&part, &nested).unwrap();
    repo.commit(&[part.clone(), nested.clone()], "Designs", &author())
        .unwrap();

    // The target is there.
    refused(
        &repo,
        json!({"cmd": "create_folder", "path": "parts"}),
        "exists",
    );
    refused(
        &repo,
        json!({"cmd": "new_design", "path": "part.mitcad"}),
        "exists",
    );
    refused(
        &repo,
        json!({"cmd": "move", "from": "part.mitcad", "to": "parts/sub/a.mitcad"}),
        "exists",
    );
    // Outside the project, hidden, not a design's name, not portable, into
    // itself.
    let outside = scratch.join("outside.mitcad");
    for (command, what) in [
        (
            json!({"cmd": "new_design", "path": "../outside.mitcad"}),
            "parent",
        ),
        (json!({"cmd": "new_design", "path": outside}), "absolute"),
        (
            json!({"cmd": "move", "from": "part.mitcad", "to": "../part.mitcad"}),
            "move out",
        ),
        (json!({"cmd": "create_folder", "path": ".hidden"}), "hidden"),
        (
            json!({"cmd": "new_design", "path": ".mitcad/x.mitcad"}),
            "marker folder",
        ),
        (
            json!({"cmd": "new_design", "path": "notes.txt"}),
            "not a design",
        ),
        (json!({"cmd": "create_folder", "path": "a:b"}), "colon"),
        (
            json!({"cmd": "create_folder", "path": "trailing."}),
            "trailing dot",
        ),
        (
            json!({"cmd": "create_folder", "path": "trailing "}),
            "trailing space",
        ),
        (
            json!({"cmd": "create_folder", "path": "inner./x"}),
            "trailing dot of an inner folder",
        ),
        (
            json!({"cmd": "create_folder", "path": "nul"}),
            "device name",
        ),
        (
            json!({"cmd": "move", "from": "parts", "to": "parts/sub/parts"}),
            "into itself",
        ),
        (
            json!({"cmd": "move", "from": "part.mitcad", "to": "part.txt"}),
            "design to other",
        ),
        (
            json!({"cmd": "delete", "path": ".mitcad/project.json"}),
            "the marker",
        ),
    ] {
        let answer = refused(&repo, command, "invalid_path");
        assert!(answer["error"]["message"].is_string(), "{what}: {answer}");
    }
    assert!(!outside.exists());
    // Not there.
    refused(
        &repo,
        json!({"cmd": "move", "from": "missing.mitcad", "to": "b.mitcad"}),
        "not_found",
    );
    refused(
        &repo,
        json!({"cmd": "delete", "path": "missing"}),
        "not_found",
    );
    // A delete that would lose what no version holds, unless forced.
    set_parameter(&mut doc, "d3", 33.0);
    doc.save_file(&part, FileFormat::Auto).unwrap();
    let answer = refused(
        &repo,
        json!({"cmd": "delete", "path": "part.mitcad"}),
        "local_changes",
    );
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("part.mitcad"),
        "{answer}"
    );
    fs::write(repo.root().join("parts/sub/notes.txt"), "notes").unwrap();
    refused(
        &repo,
        json!({"cmd": "delete", "path": "parts"}),
        "local_changes",
    );
    let forced = done(
        &repo,
        json!({"cmd": "delete", "path": "parts", "force": true}),
    );
    assert_eq!(forced["deleted_designs"], json!(["parts/sub/a.mitcad"]));
    assert!(!repo.root().join("parts").exists());
    // The text answer of a refusal is an error.
    let error = repo
        .command_text(
            &json!({"cmd": "create_folder", "path": ".hidden", "author": AUTHOR}).to_string(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("hidden"), "{error}");
    // A malformed command is a failed command.
    assert!(
        repo.command(&json!({"cmd": "move", "from": "part.mitcad"}).to_string())
            .is_err()
    );
}

#[test]
fn nothing_changes_while_head_is_detached() {
    if !has_git() {
        return;
    }
    let scratch = Scratch::new("folders-detached");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    git(repo.root(), &["checkout", "-q", "--detach"]).unwrap();
    refused(
        &repo,
        json!({"cmd": "create_folder", "path": "parts"}),
        "unsupported",
    );
    refused(
        &repo,
        json!({"cmd": "move", "from": "part.mitcad", "to": "b.mitcad"}),
        "unsupported",
    );
    assert!(part.is_file());
    git(repo.root(), &["checkout", "-q", "main"]).unwrap();
    done(&repo, json!({"cmd": "create_folder", "path": "parts"}));
}

#[test]
fn a_cloud_projects_operations_are_pushed() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("folders-cloud");
    let remote = bare(&scratch, "remote.git");
    let (repo, _) = connected(&scratch, &cli, &remote.to_string_lossy());
    let made = done(&repo, json!({"cmd": "create_folder", "path": "parts"}));
    assert_eq!(made["push"]["pushed"], true, "{made}");
    assert_eq!(
        tip(&remote, "main"),
        made["commit"].as_str().unwrap(),
        "{made}"
    );
    let moved = done(
        &repo,
        json!({"cmd": "move", "from": "part.mitcad", "to": "parts/part.mitcad"}),
    );
    assert_eq!(moved["push"]["pushed"], true, "{moved}");
    assert_eq!(tip(&remote, "main"), moved["commit"].as_str().unwrap());
    // Without a push the version waits.
    let kept = done(
        &repo,
        json!({"cmd": "new_design", "path": "parts/b.mitcad", "push": false}),
    );
    assert_eq!(kept["push"], Value::Null, "{kept}");
    assert_ne!(tip(&remote, "main"), kept["commit"].as_str().unwrap());
    let listed = tree(&repo);
    assert_eq!(listed["kind"], "cloud", "{listed}");
    assert_eq!(listed["ahead"], 1, "{listed}");
    assert_eq!(listed["behind"], 0);
    assert_eq!(listed["remote"]["name"], "origin");
    // The project's setting decides when the command does not.
    run(
        &repo,
        json!({"cmd": "set_project_settings", "local": {"sync": {"send_at_once": false}}}),
    );
    let waiting = done(&repo, json!({"cmd": "delete", "path": "parts/b.mitcad"}));
    assert_eq!(waiting["push"], Value::Null, "{waiting}");
    let sent = done(
        &repo,
        json!({"cmd": "create_folder", "path": "more", "push": true}),
    );
    assert_eq!(sent["push"]["pushed"], true, "{sent}");
    assert_eq!(tip(&remote, "main"), sent["commit"].as_str().unwrap());
    // A push the remote refuses: the version is recorded, the error says so.
    let other = scratch.join("other");
    let mut log = Vec::new();
    clone_project(
        &remote.to_string_lossy(),
        &other,
        Some(&cli),
        None,
        &mut log,
    )
    .unwrap();
    let other = ProjectRepo::open(&other).unwrap();
    other.set_git(cli.clone());
    done(&other, json!({"cmd": "create_folder", "path": "theirs"}));
    let mut command = json!({"cmd": "create_folder", "path": "mine", "author": AUTHOR});
    command["push"] = json!(true);
    let answer = run(&repo, command);
    assert!(answer["commit"].is_string(), "{answer}");
    assert_eq!(error_class(&answer), "rejected", "{answer}");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("The version is recorded but was not sent"),
        "{answer}"
    );
    clean(repo.root());
}

#[test]
fn designs_locked_by_someone_else_are_not_moved_or_deleted() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("folders-locks");
    let remote = bare(&scratch, "remote.git");
    let url = remote.to_string_lossy().into_owned();
    let (a, part) = connected(&scratch, &cli, &url);
    let mut log = Vec::new();
    clone_project(&url, &scratch.join("b"), Some(&cli), None, &mut log).unwrap();
    let b = ProjectRepo::open(&scratch.join("b")).unwrap();
    b.set_git(cli.clone());
    let b_part = b.root().join("part.mitcad");
    let taken = run(
        &b,
        json!({"cmd": "lock_take", "path": b_part, "session": SB, "author": OTHER,
               "idle_minutes": 1, "poll_seconds": 5}),
    );
    assert_eq!(taken["outcome"], "taken", "{taken}");
    // A sees the lock after a poll; the tree reads it without the network.
    let seen = run(
        &a,
        json!({"cmd": "lock_status", "session": SA, "path": part}),
    );
    assert_eq!(seen["error"], Value::Null, "{seen}");
    let listed = tree(&a);
    let lock = &design_in(&listed, "part.mitcad")["lock"];
    assert_eq!(lock["owner"]["email"], "other@example.invalid", "{listed}");
    assert_eq!(lock["mine"], false);
    assert_eq!(lock["same_owner"], false);
    assert_eq!(lock["state"], "active");

    let answer = refused(
        &a,
        json!({"cmd": "move", "from": "part.mitcad", "to": "moved.mitcad", "session": SA}),
        "locked",
    );
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Other Tester"),
        "{answer}"
    );
    refused(
        &a,
        json!({"cmd": "delete", "path": "part.mitcad", "session": SA}),
        "locked",
    );
    // B moves its own locked design.
    done(
        &b,
        json!({"cmd": "create_folder", "path": "parts", "push": false}),
    );
    let mut own = json!({"cmd": "move", "from": "part.mitcad", "to": "parts/part.mitcad",
                         "session": SB, "push": false});
    own["author"] = json!(OTHER);
    let moved = run(&b, own);
    assert_eq!(moved["error"], Value::Null, "{moved}");
    assert!(moved["commit"].is_string(), "{moved}");

    // Released and polled again: A may move it.
    let released = run(
        &b,
        json!({"cmd": "lock_release", "path": b.root().join("part.mitcad"), "session": SB,
               "author": OTHER}),
    );
    assert_eq!(released["error"], Value::Null, "{released}");
    run(
        &a,
        json!({"cmd": "lock_status", "session": SA, "path": part}),
    );
    assert_eq!(design_in(&tree(&a), "part.mitcad")["lock"], Value::Null);
    done(
        &a,
        json!({"cmd": "move", "from": "part.mitcad", "to": "moved.mitcad", "session": SA}),
    );
}
