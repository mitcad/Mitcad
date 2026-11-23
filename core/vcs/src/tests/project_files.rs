// SPDX-License-Identifier: MIT
//! Other files of a project (mitcad#2204) in temporary projects: listed in
//! `project_tree`'s `files` with their size and state (dot-names,
//! `.mitcad/`, the operating system's files and what the project's
//! `.gitignore` leaves out are not), added (`add_files`), renamed, moved,
//! deleted and their changes recorded (`record`), each one version; every
//! refusal changing nothing; files over 100 MiB refused without
//! `allow_large`; and a Cloud project's push of an added file (a bare
//! repository stands in for the cloud). Without the system's git the
//! checks that need it are skipped.

use serde_json::{Value, json};

use super::remote::{AUTHOR, bare, connected, error_class, run, system_git, tip};
use super::*;
use crate::projects::LARGE_FILE;

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

/// The files under a folder (relative, `/`) with their contents, leaving
/// out `.git`.
fn files_of(root: &Path) -> Vec<(String, Vec<u8>)> {
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
                // Sparse large files are compared by size only.
                let data = if entry.metadata().unwrap().len() > 1024 * 1024 {
                    entry.metadata().unwrap().len().to_le_bytes().to_vec()
                } else {
                    fs::read(entry.path()).unwrap()
                };
                out.push((path, data));
            }
        }
    }
    out.sort();
    out
}

fn tree(repo: &ProjectRepo) -> Value {
    run(repo, json!({"cmd": "project_tree", "author": AUTHOR}))
}

/// A file of `project_tree`'s answer, or None when it is not listed.
fn file_in<'a>(tree: &'a Value, path: &str) -> Option<&'a Value> {
    tree["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == path)
}

fn listed_files(tree: &Value) -> Vec<String> {
    tree["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_owned())
        .collect()
}

/// The newest version's summary.
fn last_message(repo: &ProjectRepo) -> String {
    let head = repo.head_commit().unwrap().unwrap();
    let commit = repo.repo.find_commit(head).unwrap();
    let message = commit.message_raw().unwrap().to_str_lossy().into_owned();
    crate::commit::user_message(&message).to_owned()
}

/// How many versions HEAD's history has.
fn versions(repo: &ProjectRepo) -> usize {
    let head = repo.head_commit().unwrap().unwrap();
    repo.repo.rev_walk([head]).all().unwrap().count()
}

/// Whether the latest version holds `path`.
fn in_version(repo: &ProjectRepo, path: &str) -> bool {
    let head = repo.head_commit().unwrap().unwrap();
    let tree = repo
        .repo
        .find_commit(head)
        .unwrap()
        .tree_id()
        .unwrap()
        .detach();
    repo.blob_entry(tree, path).unwrap().is_some()
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

/// Files outside the project to add: a PNG and a PDF stand-in.
fn sources(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let outside = scratch.join("outside");
    fs::create_dir_all(&outside).unwrap();
    let png = outside.join("photo.png");
    fs::write(&png, b"\x89PNG\r\n\x1a\nnot really a picture").unwrap();
    let pdf = outside.join("datasheet.pdf");
    fs::write(&pdf, b"%PDF-1.4\nnot really a datasheet\n").unwrap();
    (png, pdf)
}

#[test]
fn other_files_are_listed_beside_the_designs() {
    let scratch = Scratch::new("files-listed");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let root = repo.root().to_path_buf();
    fs::create_dir_all(root.join("docs/build")).unwrap();
    fs::create_dir_all(root.join(".hidden")).unwrap();
    fs::write(root.join("photo.png"), b"picture").unwrap();
    fs::write(root.join("docs/datasheet.pdf"), b"%PDF").unwrap();
    // Left out: hidden names, MitCad's own folder, the system's files and
    // what the project's .gitignore leaves out (a pattern and a folder).
    fs::write(root.join(".notes.txt"), b"hidden").unwrap();
    fs::write(root.join(".hidden/a.txt"), b"hidden").unwrap();
    fs::write(root.join(".mitcad/extra.txt"), b"MitCad's").unwrap();
    fs::write(root.join("Thumbs.db"), b"system").unwrap();
    fs::write(root.join("docs/desktop.ini"), b"system").unwrap();
    fs::write(root.join("docs/old.bak"), b"ignored").unwrap();
    fs::write(root.join("docs/build/out.bin"), b"ignored").unwrap();
    let mut ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    ignore.push_str("*.bak\nbuild/\n");
    fs::write(root.join(".gitignore"), ignore).unwrap();

    let listed = tree(&repo);
    assert_eq!(
        listed_files(&listed),
        ["docs/datasheet.pdf", "photo.png"],
        "{listed}"
    );
    // The design stays a design; the ignored folder is still a folder.
    assert_eq!(listed["designs"][0]["path"], "part.mitcad", "{listed}");
    assert!(
        listed["folders"]
            .as_array()
            .unwrap()
            .contains(&json!("docs/build")),
        "{listed}"
    );
    let photo = file_in(&listed, "photo.png").unwrap();
    assert_eq!(photo["size"], 7, "{photo}");
    assert_eq!(photo["recorded"], false, "{photo}");
    assert_eq!(photo["changed"], true, "{photo}");
    assert!(
        photo["modified"].as_str().unwrap().ends_with('Z'),
        "{photo}"
    );
    assert!(photo.get("version").is_none(), "{photo}");
    assert!(photo.get("lock").is_none(), "{photo}");

    // Recorded: the state follows.
    done(&repo, json!({"cmd": "record", "path": "photo.png"}));
    let listed = tree(&repo);
    let photo = file_in(&listed, "photo.png").unwrap();
    assert_eq!(photo["recorded"], true, "{photo}");
    assert_eq!(photo["changed"], false, "{photo}");
    // Changed in its own program: the same size too, so the content tells.
    fs::write(root.join("photo.png"), b"PICTURE").unwrap();
    let photo = file_in(&tree(&repo), "photo.png").unwrap().clone();
    assert_eq!(photo["changed"], true, "{photo}");
    fs::write(root.join("photo.png"), b"a larger picture").unwrap();
    let photo = file_in(&tree(&repo), "photo.png").unwrap().clone();
    assert_eq!(photo["changed"], true, "{photo}");
    assert_eq!(photo["size"], 16, "{photo}");

    // In text: the size and the state.
    let text = repo
        .command_text(&json!({"cmd": "project_tree"}).to_string())
        .unwrap();
    assert!(text.contains("  photo.png  16 bytes, changed\n"), "{text}");
    assert!(
        text.contains("  docs/datasheet.pdf  4 bytes, not recorded\n"),
        "{text}"
    );
    assert!(!text.contains("old.bak"), "{text}");
}

#[test]
fn files_are_added_as_one_version() {
    let scratch = Scratch::new("files-added");
    let repo = empty_project(&scratch);
    let (png, pdf) = sources(&scratch);
    let before = versions(&repo);
    let added = done(
        &repo,
        json!({"cmd": "add_files", "files": [pdf.to_string_lossy()]}),
    );
    assert_eq!(added["added"], json!(["datasheet.pdf"]), "{added}");
    assert_eq!(added["folder"], "", "{added}");
    assert_eq!(last_message(&repo), "Add datasheet.pdf");
    assert_eq!(versions(&repo), before + 1);
    assert_eq!(
        fs::read(repo.root().join("datasheet.pdf")).unwrap(),
        fs::read(&pdf).unwrap()
    );
    // The source stays where it was.
    assert!(pdf.is_file());

    // Into a folder, several at once: one version.
    done(&repo, json!({"cmd": "create_folder", "path": "docs"}));
    let before = versions(&repo);
    let copy = scratch.join("outside/notes.txt");
    fs::write(&copy, b"notes").unwrap();
    let added = done(
        &repo,
        json!({"cmd": "add_files", "folder": "docs",
               "files": [png.to_string_lossy(), copy.to_string_lossy()]}),
    );
    assert_eq!(
        added["added"],
        json!(["docs/photo.png", "docs/notes.txt"]),
        "{added}"
    );
    assert_eq!(versions(&repo), before + 1);
    assert_eq!(last_message(&repo), "Add docs/photo.png and docs/notes.txt");
    let listed = tree(&repo);
    for path in ["datasheet.pdf", "docs/notes.txt", "docs/photo.png"] {
        let file = file_in(&listed, path).unwrap_or_else(|| panic!("{path}: {listed}"));
        assert_eq!(file["recorded"], true, "{file}");
        assert_eq!(file["changed"], false, "{file}");
    }
    // No temporary copies are left.
    assert!(
        !files_of(repo.root())
            .iter()
            .any(|(path, _)| path.ends_with(".tmp")),
        "{:?}",
        files_of(repo.root())
    );
    let text = repo
        .command_text(
            &json!({"cmd": "add_files", "folder": "docs", "author": AUTHOR,
                    "files": [scratch.join("outside/datasheet.pdf").to_string_lossy()]})
            .to_string(),
        )
        .unwrap();
    assert!(
        text.starts_with("Added docs/datasheet.pdf\nRecorded version "),
        "{text}"
    );
    clean(repo.root());
}

#[test]
fn refused_additions_change_nothing() {
    let scratch = Scratch::new("files-refused");
    let repo = empty_project(&scratch);
    let (png, pdf) = sources(&scratch);
    done(
        &repo,
        json!({"cmd": "add_files", "files": [png.to_string_lossy()]}),
    );
    // A name that is there already: nothing replaced, not even the files
    // before it in the list.
    let answer = refused(
        &repo,
        json!({"cmd": "add_files",
               "files": [pdf.to_string_lossy(), png.to_string_lossy()]}),
        "exists",
    );
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("photo.png is there already"),
        "{answer}"
    );
    // The same name twice in one add.
    refused(
        &repo,
        json!({"cmd": "add_files",
               "files": [pdf.to_string_lossy(), pdf.to_string_lossy()]}),
        "exists",
    );
    // A folder, a design, a file that is not there, a folder of the
    // project that is not there, a hidden folder, an ignored name.
    let design = scratch.join("outside/part.mitcad");
    fs::write(&design, mitcad_model::file::empty_design()).unwrap();
    let backup = scratch.join("outside/old.bak");
    fs::write(&backup, b"old").unwrap();
    let mut ignore = fs::read_to_string(repo.root().join(".gitignore")).unwrap();
    ignore.push_str("*.bak\n");
    fs::write(repo.root().join(".gitignore"), ignore).unwrap();
    for (command, class) in [
        (
            json!({"cmd": "add_files", "files": [scratch.join("outside").to_string_lossy()]}),
            "invalid_path",
        ),
        (
            json!({"cmd": "add_files", "files": [design.to_string_lossy()]}),
            "invalid_path",
        ),
        (
            json!({"cmd": "add_files", "files": [scratch.join("missing.txt").to_string_lossy()]}),
            "not_found",
        ),
        (
            json!({"cmd": "add_files", "folder": "nowhere", "files": [pdf.to_string_lossy()]}),
            "not_found",
        ),
        (
            json!({"cmd": "add_files", "folder": ".mitcad", "files": [pdf.to_string_lossy()]}),
            "invalid_path",
        ),
        (
            json!({"cmd": "add_files", "files": [backup.to_string_lossy()]}),
            "invalid_path",
        ),
    ] {
        refused(&repo, command, class);
    }
    // Without files: a failed command.
    assert!(
        repo.command(&json!({"cmd": "add_files", "files": [], "author": AUTHOR}).to_string())
            .is_err()
    );
}

#[test]
fn files_over_100_mib_are_recorded_only_when_allowed() {
    let scratch = Scratch::new("files-large");
    let repo = empty_project(&scratch);
    // A sparse file: large without writing its bytes.
    let large = scratch.join("outside/scan.zip");
    fs::create_dir_all(large.parent().unwrap()).unwrap();
    fs::File::create(&large)
        .unwrap()
        .set_len(LARGE_FILE + 1)
        .unwrap();
    let answer = refused(
        &repo,
        json!({"cmd": "add_files", "files": [large.to_string_lossy()]}),
        "too_large",
    );
    let message = answer["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("scan.zip is larger than 100 MiB"),
        "{message}"
    );
    assert!(message.contains("GitHub"), "{message}");
    assert!(message.contains("every copy"), "{message}");
    // Put there by another program, then recorded: the same question.
    fs::File::create(repo.root().join("scan.zip"))
        .unwrap()
        .set_len(LARGE_FILE + 1)
        .unwrap();
    refused(
        &repo,
        json!({"cmd": "record", "path": "scan.zip"}),
        "too_large",
    );
    refused(&repo, json!({"cmd": "record"}), "too_large");
}

#[test]
fn files_are_renamed_moved_and_deleted_as_versions() {
    let scratch = Scratch::new("files-moved");
    let repo = empty_project(&scratch);
    let (png, pdf) = sources(&scratch);
    done(
        &repo,
        json!({"cmd": "add_files", "files": [png.to_string_lossy(), pdf.to_string_lossy()]}),
    );
    done(&repo, json!({"cmd": "create_folder", "path": "docs"}));

    // Renamed: one version, the old name gone from it.
    let before = versions(&repo);
    let renamed = done(
        &repo,
        json!({"cmd": "move", "from": "photo.png", "to": "front.png"}),
    );
    assert_eq!(renamed["moved"], json!([]), "{renamed}");
    assert_eq!(versions(&repo), before + 1);
    assert!(in_version(&repo, "front.png") && !in_version(&repo, "photo.png"));
    assert!(repo.root().join("front.png").is_file());
    // Moved into a folder.
    done(
        &repo,
        json!({"cmd": "move", "from": "front.png", "to": "docs/front.png"}),
    );
    assert!(in_version(&repo, "docs/front.png") && !in_version(&repo, "front.png"));
    assert_eq!(last_message(&repo), "Move front.png to docs/front.png");
    // A file that no version holds yet is recorded at its new path.
    fs::write(repo.root().join("loose.txt"), b"loose").unwrap();
    done(
        &repo,
        json!({"cmd": "move", "from": "loose.txt", "to": "docs/loose.txt"}),
    );
    assert!(in_version(&repo, "docs/loose.txt"));
    // Refused: a file named as a design, a name that is there.
    refused(
        &repo,
        json!({"cmd": "move", "from": "datasheet.pdf", "to": "datasheet.mitcad"}),
        "invalid_path",
    );
    refused(
        &repo,
        json!({"cmd": "move", "from": "datasheet.pdf", "to": "docs/front.png"}),
        "exists",
    );

    // Deleted: one version; a changed file only with force.
    let before = versions(&repo);
    let deleted = done(&repo, json!({"cmd": "delete", "path": "docs/front.png"}));
    assert_eq!(deleted["deleted_designs"], json!([]), "{deleted}");
    assert_eq!(versions(&repo), before + 1);
    assert!(!repo.root().join("docs/front.png").exists());
    assert!(!in_version(&repo, "docs/front.png"));
    fs::write(repo.root().join("datasheet.pdf"), b"changed elsewhere").unwrap();
    refused(
        &repo,
        json!({"cmd": "delete", "path": "datasheet.pdf"}),
        "local_changes",
    );
    done(
        &repo,
        json!({"cmd": "delete", "path": "datasheet.pdf", "force": true}),
    );
    assert!(!in_version(&repo, "datasheet.pdf"));
    // The folder keeps its placeholder once its files are gone.
    done(&repo, json!({"cmd": "delete", "path": "docs/loose.txt"}));
    assert!(in_version(
        &repo,
        &format!("docs/{}", crate::projects::FOLDER_PLACEHOLDER)
    ));
    clean(repo.root());
}

#[test]
fn changes_made_elsewhere_are_recorded() {
    let scratch = Scratch::new("files-record");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let root = repo.root().to_path_buf();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/a.txt"), b"a").unwrap();
    fs::write(root.join("docs/b.txt"), b"b").unwrap();
    fs::write(root.join("top.txt"), b"top").unwrap();

    // One file.
    let before = versions(&repo);
    let recorded = done(&repo, json!({"cmd": "record", "path": "docs/a.txt"}));
    assert_eq!(recorded["recorded"], json!(["docs/a.txt"]), "{recorded}");
    assert_eq!(last_message(&repo), "Record docs/a.txt");
    assert_eq!(versions(&repo), before + 1);
    assert!(!in_version(&repo, "docs/b.txt"));
    // A folder: everything changed in it, nothing outside.
    fs::write(root.join("docs/a.txt"), b"a, changed").unwrap();
    let recorded = done(&repo, json!({"cmd": "record", "path": "docs"}));
    assert_eq!(
        recorded["recorded"],
        json!(["docs/a.txt", "docs/b.txt"]),
        "{recorded}"
    );
    assert_eq!(last_message(&repo), "Record changes in docs");
    assert!(!in_version(&repo, "top.txt"));
    // The whole project; a file deleted elsewhere is recorded as gone; a
    // changed design is Save's and stays as it is.
    fs::remove_file(root.join("docs/b.txt")).unwrap();
    fs::write(&part, fs::read_to_string(&part).unwrap() + "\n").unwrap();
    let recorded = done(&repo, json!({"cmd": "record"}));
    assert_eq!(
        recorded["recorded"],
        json!(["docs/b.txt", "top.txt"]),
        "{recorded}"
    );
    assert_eq!(last_message(&repo), "Record changes");
    assert!(!in_version(&repo, "docs/b.txt") && in_version(&repo, "top.txt"));
    let listed = tree(&repo);
    assert_eq!(listed["designs"][0]["changed"], true, "{listed}");
    for file in listed["files"].as_array().unwrap() {
        assert_eq!(file["changed"], false, "{file}");
    }

    // Nothing changed: no version, no error.
    let head = repo.head_commit().unwrap();
    let mut command = json!({"cmd": "record", "author": AUTHOR});
    let answer = run(&repo, command.clone());
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["commit"], Value::Null, "{answer}");
    assert_eq!(answer["recorded"], json!([]), "{answer}");
    assert_eq!(repo.head_commit().unwrap(), head);
    command["path"] = json!("docs");
    let text = repo.command_text(&command.to_string()).unwrap();
    assert_eq!(text, "No changes to record\n");

    // Refused: a design, a path that is nowhere.
    refused(
        &repo,
        json!({"cmd": "record", "path": "part.mitcad"}),
        "invalid_path",
    );
    refused(
        &repo,
        json!({"cmd": "record", "path": "missing.txt"}),
        "not_found",
    );
}

#[test]
fn a_cloud_projects_added_file_is_pushed() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("files-cloud");
    let remote = bare(&scratch, "remote.git");
    let (repo, _) = connected(&scratch, &cli, &remote.to_string_lossy());
    let (png, _) = sources(&scratch);
    let added = done(
        &repo,
        json!({"cmd": "add_files", "files": [png.to_string_lossy()]}),
    );
    assert_eq!(added["push"]["pushed"], true, "{added}");
    assert_eq!(tip(&remote, "main"), added["commit"].as_str().unwrap());
    // A recorded change goes out too, unless the command says no.
    fs::write(repo.root().join("photo.png"), b"another picture").unwrap();
    let kept = done(
        &repo,
        json!({"cmd": "record", "path": "photo.png", "push": false}),
    );
    assert_eq!(kept["push"], Value::Null, "{kept}");
    assert_ne!(tip(&remote, "main"), kept["commit"].as_str().unwrap());
    let listed = tree(&repo);
    assert_eq!(listed["ahead"], 1, "{listed}");
    clean(repo.root());
}
