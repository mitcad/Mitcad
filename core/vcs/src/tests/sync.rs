// SPDX-License-Identifier: MIT
//! Sync and conflicts against repositories in temporary folders: a bare
//! repository as the remote and two projects A and B connected to it (B
//! opened from it), changing the same or other files. After each sync the
//! repositories are checked as git sees them: `git fsck`, a clean `git
//! status`, and `git log --first-parent` of each file agreeing with its
//! history. Without the system's git these tests are skipped.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

use super::remote::{
    AUTHOR, bare, connected, error_class, fake_ssh, new_version, run, system_git, tip,
};
use super::*;
use crate::remote::sync::{copy_path, stamp};
use crate::remote::{
    BACKUP_PREFIX, ConflictCopy, ConflictKind, Control, ErrorClass, GitCli, Resolution, SyncCase,
    SyncOptions, SyncOutcome, clone_project,
};

/// The author of B's versions.
fn other() -> Identity {
    Identity::new("Other Tester", "other@example.invalid").unwrap()
}

/// Project A connected to an empty remote with its first version, and B
/// opened from it.
struct Pair {
    a: ProjectRepo,
    b: ProjectRepo,
    part_a: PathBuf,
    part_b: PathBuf,
    remote: PathBuf,
    cli: GitCli,
    // Removed last.
    scratch: Scratch,
}

fn pair(name: &str) -> Option<Pair> {
    pair_with(
        name,
        |_, cli| cli.clone(),
        |remote| remote.to_string_lossy().into_owned(),
    )
}

/// A pair whose git is `git` and whose remote's URL is `url`.
fn pair_with(
    name: &str,
    git: impl Fn(&Scratch, &GitCli) -> GitCli,
    url: impl Fn(&Path) -> String,
) -> Option<Pair> {
    let cli = system_git()?;
    let scratch = Scratch::new(name);
    let remote = bare(&scratch, "remote.git");
    let cli = git(&scratch, &cli);
    let url = url(&remote);
    let (a, part_a) = connected(&scratch, &cli, &url);
    let b = opened(&scratch, &cli, &url, "b");
    let part_b = b.root().join("part.mitcad");
    Some(Pair {
        a,
        b,
        part_a,
        part_b,
        remote,
        cli,
        scratch,
    })
}

/// The project opened from `url` into the scratch folder `name`.
fn opened(scratch: &Scratch, cli: &GitCli, url: &str, name: &str) -> ProjectRepo {
    let mut log = Vec::new();
    clone_project(url, &scratch.join(name), Some(cli), None, &mut log).unwrap();
    let repo = ProjectRepo::open(&scratch.join(name)).unwrap();
    repo.set_git(cli.clone());
    repo
}

/// `d3` of the project file `part` changed and recorded by `who`.
fn version_by(repo: &ProjectRepo, part: &Path, d3: f64, who: &Identity) -> String {
    let mut doc = Document::load_project(part, MockKernel::default()).unwrap();
    set_parameter(&mut doc, "d3", d3);
    doc.save_file(part, FileFormat::Auto).unwrap();
    repo.commit(&[part.to_path_buf()], &format!("d3 = {d3} mm"), who)
        .unwrap()
        .commit
        .unwrap()
}

/// A file written and recorded by `who`.
fn record_file(repo: &ProjectRepo, path: &str, text: &str, who: &Identity) -> String {
    let file = repo.root().join(path);
    fs::write(&file, text).unwrap();
    repo.commit(&[file], &format!("Write {path}"), who)
        .unwrap()
        .commit
        .unwrap()
}

/// A base feature with a body of mock data added to a design.
fn add_body(doc: &mut Document<MockKernel>, name: &str, data: &str) {
    doc.command(&format!(
        r#"{{"cmd": "add_feature", "def": {{"type": "base", "bodies": [{{"name": "{name}", "brep": {}}}]}}}}"#,
        brep(data)
    ))
    .unwrap();
}

fn sync_as(repo: &ProjectRepo, options: &SyncOptions, who: &Identity) -> SyncOutcome {
    repo.sync(options, Some(who), None).unwrap()
}

fn sync(repo: &ProjectRepo, options: &SyncOptions) -> SyncOutcome {
    sync_as(repo, options, &author())
}

fn choosing(choices: &[(&str, Resolution)]) -> SyncOptions {
    SyncOptions {
        resolutions: choices
            .iter()
            .map(|(path, choice)| ((*path).to_owned(), *choice))
            .collect(),
        ..SyncOptions::default()
    }
}

/// The backup references of a project.
fn backups(repo: &ProjectRepo) -> Vec<String> {
    git(
        repo.root(),
        &["for-each-ref", "--format=%(refname)", BACKUP_PREFIX],
    )
    .unwrap()
    .lines()
    .map(str::to_owned)
    .collect()
}

fn ids(repo: &ProjectRepo, path: &str) -> Vec<String> {
    repo.history(&repo.root().join(path))
        .unwrap()
        .into_iter()
        .map(|version| version.id)
        .collect()
}

fn head(repo: &ProjectRepo) -> String {
    repo.resolve("HEAD").unwrap()
}

/// The repository as git sees it (10.1, 12): sound, nothing changed in the
/// folder, and each file's first-parent log its history.
fn check(repo: &ProjectRepo, files: &[&str]) {
    git(
        repo.root(),
        &["fsck", "--strict", "--no-progress", "--no-dangling"],
    )
    .unwrap();
    assert_eq!(
        git(repo.root(), &["status", "--porcelain"]).unwrap(),
        "",
        "{}",
        repo.root().display()
    );
    for file in files {
        let logged: Vec<String> = git(
            repo.root(),
            &["log", "--first-parent", "--format=%H", "--", file],
        )
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
        assert_eq!(logged, ids(repo, file), "{file}");
    }
}

#[test]
fn copies_are_named_after_their_author_and_time() {
    let none = |_: &str| false;
    assert_eq!(
        copy_path("part.mitcad", "Ada Lovelace", "2026-10-05 14.03", none),
        "part (conflict copy Ada Lovelace 2026-10-05 14.03).mitcad"
    );
    assert_eq!(
        copy_path("parts/a.b.mitcad", "A<b>:c", "2026-10-05 14.03", |p| {
            p == "parts/a.b (conflict copy A_b__c 2026-10-05 14.03).mitcad"
        }),
        "parts/a.b (conflict copy A_b__c 2026-10-05 14.03 2).mitcad"
    );
    assert_eq!(
        copy_path(".gitignore", "Ada", "2026-10-05 14.03", none),
        ".gitignore (conflict copy Ada 2026-10-05 14.03)"
    );
    assert_eq!(
        copy_path("README", "Ada", "2026-10-05 14.03", none),
        "README (conflict copy Ada 2026-10-05 14.03)"
    );
    assert_eq!(stamp(0), "19700101-000000");
    assert_eq!(stamp(1_788_609_792), "20260905-120312");
    assert_eq!(Resolution::parse("copy"), Some(Resolution::Copy));
    assert_eq!(Resolution::parse("newest"), None);
}

/// Versions on both sides in other files: the project's are replayed after
/// the remote's without a question, the history is one line with the
/// original authors, times and messages, and the other side takes it with a
/// fast-forward (10.1, 4).
#[test]
fn versions_on_both_sides_are_replayed_in_one_line() {
    let Some(p) = pair("replay") else {
        return;
    };
    // B adds a design of its own and pushes it.
    let other_b = p.b.root().join("other.mitcad");
    let mut doc = design();
    set_parameter(&mut doc, "d3", 50.0);
    doc.save_file(&other_b, FileFormat::Auto).unwrap();
    let from_b =
        p.b.commit(std::slice::from_ref(&other_b), "Other part", &other())
            .unwrap()
            .commit
            .unwrap();
    assert!(p.b.push(None).unwrap().pushed);
    // A records a version without knowing of it.
    let from_a = new_version(&p.a, &p.part_a, 30.0);
    let mine = fs::read(&p.part_a).unwrap();
    let before = head(&p.a);

    // Without a fetch the project only knows its own version.
    let plan = p.a.sync_plan(false, None).unwrap();
    assert_eq!((plan.case, plan.ahead, plan.behind), (SyncCase::Push, 1, 0));
    let plan = p.a.sync_plan(true, None).unwrap();
    assert_eq!(plan.case, SyncCase::Replay);
    assert_eq!((plan.ahead, plan.behind), (1, 1));
    assert_eq!(
        (plan.local[0].id.as_str(), plan.remote[0].id.as_str()),
        (from_a.as_str(), from_b.as_str())
    );
    assert_eq!(plan.remote[0].author, other());
    assert_eq!(plan.remote[0].summary, "Other part");
    assert!(
        plan.conflicts.is_empty() && plan.uncommitted.is_empty(),
        "{plan:?}"
    );
    assert_eq!(head(&p.a), before);
    let text =
        p.a.command_text(r#"{"cmd": "sync_plan", "fetch": false}"#)
            .unwrap();
    assert!(
        text.contains(
            "Sync will replay 1 version of this project after 1 version newer on origin/main"
        ),
        "{text}"
    );

    let outcome = sync(&p.a, &SyncOptions::default());
    assert_eq!(outcome.error, None, "{outcome:?}");
    assert_eq!(outcome.case, SyncCase::Replay);
    assert_eq!(outcome.replayed.len(), 1);
    assert_eq!(outcome.replayed[0].from, from_a);
    let replayed = outcome.replayed[0].to.clone().unwrap();
    assert_ne!(replayed, from_a);
    assert_eq!(outcome.head.as_deref(), Some(replayed.as_str()));
    assert!(outcome.pushed && outcome.retries == 0, "{outcome:?}");
    assert_eq!((outcome.ahead, outcome.behind), (Some(0), Some(0)));
    assert!(outcome.changed_paths.contains(&"other.mitcad".to_owned()));
    assert!(outcome.conflicts.is_empty() && outcome.copies.is_empty());
    // One line: B's version, then A's with its author, time and message,
    // recorded by the one who synced.
    let new =
        p.a.repo
            .find_commit(replayed.parse::<ObjectId>().unwrap())
            .unwrap();
    let old =
        p.a.repo
            .find_commit(from_a.parse::<ObjectId>().unwrap())
            .unwrap();
    assert_eq!(
        new.parent_ids()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&from_b)
    );
    assert_eq!(new.message_raw().unwrap(), old.message_raw().unwrap());
    let (new_author, old_author) = (new.author().unwrap(), old.author().unwrap());
    assert_eq!(new_author.name, old_author.name);
    assert_eq!(new_author.email, old_author.email);
    assert_eq!(new_author.time().unwrap(), old_author.time().unwrap());
    assert_eq!(new.committer().unwrap().name, "Mitcad Test");
    // The folder has both.
    assert_eq!(fs::read(&p.part_a).unwrap(), mine);
    assert_eq!(
        fs::read(p.a.root().join("other.mitcad")).unwrap(),
        fs::read(&other_b).unwrap()
    );
    // The old line is kept.
    let backup = outcome.backup.clone().unwrap();
    assert!(backup.starts_with(BACKUP_PREFIX), "{backup}");
    assert_eq!(tip(p.a.root(), &backup), before);
    assert_eq!(backups(&p.a), [backup]);
    assert_eq!(ids(&p.a, "part.mitcad")[0], replayed);
    assert_eq!(ids(&p.a, "part.mitcad").len(), 2);
    assert_eq!(ids(&p.a, "other.mitcad"), [from_b]);
    assert_eq!(tip(&p.remote, "refs/heads/main"), replayed);
    let info = p.a.remote_info(false).unwrap();
    assert!(info.last_sync.unwrap().error.is_none());
    check(&p.a, &["part.mitcad", "other.mitcad"]);

    // B takes it.
    let taken = sync_as(&p.b, &SyncOptions::default(), &other());
    assert_eq!(taken.error, None, "{taken:?}");
    assert_eq!(taken.case, SyncCase::FastForward);
    assert_eq!(taken.changed_paths, ["part.mitcad"]);
    assert_eq!(taken.backup, None);
    assert_eq!(head(&p.b), replayed);
    assert_eq!(fs::read(&p.part_b).unwrap(), mine);
    check(&p.b, &["part.mitcad", "other.mitcad"]);
    // Nothing more to do.
    let again = sync(&p.a, &SyncOptions::default());
    assert_eq!((again.case, again.pushed), (SyncCase::UpToDate, false));
    let text = p.a.command_text(r#"{"cmd": "sync"}"#).unwrap();
    assert!(
        text.ends_with("Up to date with origin/main\n")
            || text.contains("Branch main follows origin/main: up to date\n"),
        "{text}"
    );
}

/// The same file changed on both sides: the sync stops before changing
/// anything until there is a choice, then keeps mine, takes theirs or saves
/// mine as a copy, with the old history in the backup reference (10.1, 5).
#[test]
fn a_file_changed_on_both_sides_is_kept_taken_or_copied() {
    for choice in [Resolution::Mine, Resolution::Theirs, Resolution::Copy] {
        let Some(p) = pair(&format!("conflict-{choice:?}")) else {
            return;
        };
        let theirs = version_by(&p.b, &p.part_b, 25.0, &other());
        assert!(p.b.push(None).unwrap().pushed);
        let theirs_text = fs::read(&p.part_b).unwrap();
        let mine = new_version(&p.a, &p.part_a, 30.0);
        let mine_text = fs::read(&p.part_a).unwrap();
        // The design's display state, per user.
        let project = Project::open(p.a.root()).unwrap();
        let display = project.local_state_path(&p.part_a).unwrap();
        fs::create_dir_all(display.parent().unwrap()).unwrap();
        fs::write(&display, "{\"view\": \"mine\"}\n").unwrap();
        let before = head(&p.a);

        // No choice: nothing changes.
        let stopped = sync(&p.a, &SyncOptions::default());
        assert_eq!(
            stopped.error.as_ref().map(|e| e.class),
            Some(ErrorClass::Conflict),
            "{stopped:?}"
        );
        assert!(
            stopped
                .error
                .as_ref()
                .unwrap()
                .message
                .contains("part.mitcad"),
            "{stopped:?}"
        );
        assert_eq!(stopped.conflicts.len(), 1);
        let conflict = &stopped.conflicts[0];
        assert_eq!(conflict.path, "part.mitcad");
        assert_eq!(conflict.kind, ConflictKind::Modified);
        let mine_version = conflict.mine.version.as_ref().unwrap();
        let theirs_version = conflict.theirs.version.as_ref().unwrap();
        assert_eq!(
            (mine_version.id.as_str(), theirs_version.id.as_str()),
            (mine.as_str(), theirs.as_str())
        );
        assert_eq!(theirs_version.author, other());
        assert_eq!(theirs_version.summary, "d3 = 25 mm");
        assert_eq!((conflict.mine.versions, conflict.theirs.versions), (1, 1));
        assert_eq!(
            conflict.choices,
            [Resolution::Mine, Resolution::Theirs, Resolution::Copy]
        );
        let copy = conflict.copy.clone().unwrap();
        assert!(
            copy.starts_with("part (conflict copy Mitcad Test ") && copy.ends_with(").mitcad"),
            "{copy}"
        );
        assert_eq!(head(&p.a), before);
        assert_eq!(fs::read(&p.part_a).unwrap(), mine_text);
        assert!(backups(&p.a).is_empty());
        assert_eq!(tip(&p.remote, "refs/heads/main"), theirs);
        let plan = p.a.sync_plan(false, None).unwrap();
        assert_eq!(plan.case, SyncCase::Replay);
        assert_eq!(plan.conflicts, stopped.conflicts);
        // As JSON and as text.
        let answer = run(&p.a, json!({"cmd": "sync", "author": AUTHOR}));
        assert_eq!(error_class(&answer), "conflict", "{answer}");
        assert_eq!(answer["conflicts"][0]["kind"], "modified");
        assert_eq!(answer["conflicts"][0]["copy"], copy.as_str());
        let text = api::describe_answer("sync", &answer.to_string()).unwrap();
        assert!(
            text.contains("Conflict: part.mitcad: changed on both sides\n")
                && text.contains("  theirs: ")
                && text.contains("Other Tester: d3 = 25 mm\n")
                && text.contains(&format!("  copy:   {copy}\n"))
                && text.contains("Choose for each file: --resolve <path>=mine|theirs|copy"),
            "{text}"
        );
        let error =
            p.a.command_text(r#"{"cmd": "sync"}"#)
                .unwrap_err()
                .to_string();
        assert!(
            error.contains("changed both here and on the remote: part.mitcad"),
            "{error}"
        );
        assert_eq!(head(&p.a), before);

        // The choice, given by the file's path.
        let done = sync(&p.a, &choosing(&[(p.part_a.to_str().unwrap(), choice)]));
        assert_eq!(done.error, None, "{done:?}");
        assert_eq!(
            done.resolved,
            BTreeMap::from([("part.mitcad".to_owned(), choice)])
        );
        let backup = done.backup.clone().unwrap();
        assert_eq!(tip(p.a.root(), &backup), before);
        let history = ids(&p.a, "part.mitcad");
        let copy_file = p.a.root().join(&copy);
        match choice {
            Resolution::Mine => {
                assert_eq!(fs::read(&p.part_a).unwrap(), mine_text);
                assert_eq!(history.len(), 3);
                assert_eq!(history[0], head(&p.a));
                assert_eq!(history[1], theirs);
                assert!(done.pushed);
                assert!(display.is_file());
            }
            Resolution::Theirs => {
                assert_eq!(fs::read(&p.part_a).unwrap(), theirs_text);
                assert_eq!(head(&p.a), theirs);
                assert_eq!(done.replayed[0].to, None);
                assert!(!done.pushed);
                assert!(
                    done.warnings.iter().any(|w| w.contains(&backup)),
                    "{:?}",
                    done.warnings
                );
                assert_eq!(history[0], theirs);
            }
            Resolution::Copy => {
                assert_eq!(fs::read(&p.part_a).unwrap(), theirs_text);
                assert_eq!(fs::read(&copy_file).unwrap(), mine_text);
                assert_eq!(history[0], theirs);
                assert_eq!(ids(&p.a, &copy), [head(&p.a)]);
                assert_eq!(
                    done.copies,
                    [ConflictCopy {
                        path: "part.mitcad".to_owned(),
                        copy: copy.clone()
                    }]
                );
                // The display state went with the design.
                assert!(!display.exists());
                assert_eq!(
                    fs::read_to_string(project.local_state_path(&copy_file).unwrap()).unwrap(),
                    "{\"view\": \"mine\"}\n"
                );
                assert!(done.pushed);
                let text =
                    api::describe_answer("sync", &serde_json::to_value(&done).unwrap().to_string())
                        .unwrap();
                assert!(
                    text.contains(&format!(
                        "Saved mine as a copy: {copy} (theirs: part.mitcad)\n"
                    )),
                    "{text}"
                );
            }
        }
        assert_eq!(tip(&p.remote, "refs/heads/main"), head(&p.a));
        check(&p.a, &["part.mitcad"]);
        // B takes the outcome.
        let taken = sync_as(&p.b, &SyncOptions::default(), &other());
        assert_eq!(taken.error, None, "{taken:?}");
        assert_eq!(head(&p.b), head(&p.a));
        assert_eq!(fs::read(&p.part_b).unwrap(), fs::read(&p.part_a).unwrap());
        assert_eq!(p.b.root().join(&copy).is_file(), choice == Resolution::Copy);
        check(&p.b, &["part.mitcad"]);
    }
}

/// A file deleted on one side and changed on the other, both ways, and a
/// file added on both sides (10.1, 6).
#[test]
fn deletions_against_changes_and_files_added_on_both_sides() {
    let Some(p) = pair("deleted") else {
        return;
    };
    record_file(&p.a, "readme.txt", "first\n", &author());
    assert!(p.a.push(None).unwrap().pushed);
    assert_eq!(sync_as(&p.b, &SyncOptions::default(), &other()).error, None);
    // B deletes the readme, changes the design and adds a plan.
    let readme_b = p.b.root().join("readme.txt");
    fs::remove_file(&readme_b).unwrap();
    p.b.commit(&[readme_b], "Delete readme.txt", &other())
        .unwrap();
    version_by(&p.b, &p.part_b, 25.0, &other());
    let theirs_text = fs::read(&p.part_b).unwrap();
    record_file(&p.b, "plan.txt", "B's plan\n", &other());
    assert!(p.b.push(None).unwrap().pushed);
    // A changes the readme, deletes the design and adds a plan of its own.
    record_file(&p.a, "readme.txt", "changed by A\n", &author());
    fs::remove_file(&p.part_a).unwrap();
    p.a.commit(
        std::slice::from_ref(&p.part_a),
        "Delete part.mitcad",
        &author(),
    )
    .unwrap();
    assert!(!p.a.root().join(brep_file("6 bolt")).exists());
    record_file(&p.a, "plan.txt", "A's plan\n", &author());

    let plan = p.a.sync_plan(true, None).unwrap();
    let kinds: Vec<(&str, ConflictKind)> = plan
        .conflicts
        .iter()
        .map(|c| (c.path.as_str(), c.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("part.mitcad", ConflictKind::DeletedMine),
            ("plan.txt", ConflictKind::AddedBoth),
            ("readme.txt", ConflictKind::DeletedTheirs),
        ]
    );
    assert_eq!(plan.conflicts[0].mine.blob, None);
    assert_eq!(plan.conflicts[2].theirs.blob, None);
    let copy = plan.conflicts[1].copy.clone().unwrap();
    assert!(copy.starts_with("plan (conflict copy Mitcad Test ") && copy.ends_with(").txt"));

    let done = sync(
        &p.a,
        &choosing(&[
            ("readme.txt", Resolution::Mine),
            ("part.mitcad", Resolution::Theirs),
            ("plan.txt", Resolution::Copy),
        ]),
    );
    assert_eq!(done.error, None, "{done:?}");
    assert_eq!(
        fs::read_to_string(p.a.root().join("readme.txt")).unwrap(),
        "changed by A\n"
    );
    // The design is the remote's again, with its B-rep data.
    assert_eq!(fs::read(&p.part_a).unwrap(), theirs_text);
    assert!(p.a.root().join(brep_file("6 bolt")).is_file());
    assert!(tree_files(&p.a).contains(&brep_file("6 bolt")));
    assert_eq!(
        fs::read_to_string(p.a.root().join("plan.txt")).unwrap(),
        "B's plan\n"
    );
    assert_eq!(
        fs::read_to_string(p.a.root().join(&copy)).unwrap(),
        "A's plan\n"
    );
    // The deletion was all theirs, so that version is left out.
    let left_out: Vec<&str> = done
        .replayed
        .iter()
        .filter(|r| r.to.is_none())
        .map(|r| r.summary.as_str())
        .collect();
    assert_eq!(left_out, ["Delete part.mitcad"]);
    assert_eq!(done.replayed.len(), 3);
    check(&p.a, &["readme.txt", "part.mitcad", "plan.txt", &copy]);
    assert_eq!(sync_as(&p.b, &SyncOptions::default(), &other()).error, None);
    assert!(p.b.root().join(&copy).is_file());
    check(&p.b, &["readme.txt", "part.mitcad", "plan.txt"]);
}

/// B-rep files are never conflicts: both sides' bodies are kept, the same
/// body added on both is one file, and each replayed version holds what its
/// designs refer to, so a body added and dropped again is gone at the end
/// (10.1, 7).
#[test]
fn brep_files_are_united_and_follow_the_final_designs() {
    let Some(p) = pair("brep") else {
        return;
    };
    // B: a design of its own with a cone and a body A adds too.
    let other_b = p.b.root().join("other.mitcad");
    let mut doc = design();
    add_body(&mut doc, "Cone", "9 cone");
    add_body(&mut doc, "Same", "7 same");
    doc.save_file(&other_b, FileFormat::Auto).unwrap();
    p.b.commit(std::slice::from_ref(&other_b), "Cone", &other())
        .unwrap();
    assert!(p.b.push(None).unwrap().pushed);
    // A: a cube, then the same body in place of the cube.
    let first = fs::read(&p.part_a).unwrap();
    let mut doc = Document::load_project(&p.part_a, MockKernel::default()).unwrap();
    add_body(&mut doc, "Cube", "8 cube");
    doc.save_file(&p.part_a, FileFormat::Auto).unwrap();
    p.a.commit(std::slice::from_ref(&p.part_a), "Cube", &author())
        .unwrap();
    fs::write(&p.part_a, &first).unwrap();
    let mut doc = Document::load_project(&p.part_a, MockKernel::default()).unwrap();
    add_body(&mut doc, "Same", "7 same");
    doc.save_file(&p.part_a, FileFormat::Auto).unwrap();
    p.a.commit(
        std::slice::from_ref(&p.part_a),
        "Same body instead of the cube",
        &author(),
    )
    .unwrap();
    assert!(!p.a.root().join(brep_file("8 cube")).exists());

    let done = sync(&p.a, &SyncOptions::default());
    assert_eq!(done.error, None, "{done:?}");
    assert!(done.conflicts.is_empty());
    let files = tree_files(&p.a);
    for body in ["9 cone", "7 same", "6 bolt", "4 wedge"] {
        assert!(files.contains(&brep_file(body)), "{body}: {files:?}");
        assert!(p.a.root().join(brep_file(body)).is_file(), "{body}");
    }
    assert!(!files.contains(&brep_file("8 cube")));
    assert!(!p.a.root().join(brep_file("8 cube")).exists());
    // The replayed version with the cube holds it.
    let with_cube: ObjectId = done.replayed[0].to.clone().unwrap().parse().unwrap();
    assert!(
        p.a.entry_at(with_cube, &brep_file("8 cube"))
            .unwrap()
            .is_some()
    );
    assert!(
        p.a.entry_at(with_cube, &brep_file("9 cone"))
            .unwrap()
            .is_some()
    );
    // Both designs open with their bodies: the block, the bolt and the
    // wedge, and the bodies added.
    for (file, count) in [("part.mitcad", 4), ("other.mitcad", 5)] {
        let mut doc =
            Document::load_project(&p.a.root().join(file), MockKernel::default()).unwrap();
        assert_eq!(bodies(&mut doc).len(), count, "{file}");
    }
    check(&p.a, &["part.mitcad", "other.mitcad"]);
}

/// Changes no version holds in a file the sync would change stop it, with
/// the branch and the folder as they were; other changes in the folder stay
/// (10.1, 8).
#[test]
fn changes_no_version_holds_stop_a_sync_that_would_overwrite_them() {
    let Some(p) = pair("local") else {
        return;
    };
    version_by(&p.b, &p.part_b, 25.0, &other());
    assert!(p.b.push(None).unwrap().pushed);
    // A saves a change without recording it.
    let mut doc = Document::load_project(&p.part_a, MockKernel::default()).unwrap();
    set_parameter(&mut doc, "d3", 40.0);
    doc.save_file(&p.part_a, FileFormat::Auto).unwrap();
    let unsaved = fs::read(&p.part_a).unwrap();
    let before = head(&p.a);
    let plan = p.a.sync_plan(true, None).unwrap();
    assert_eq!(plan.case, SyncCase::FastForward);
    assert_eq!(plan.uncommitted, ["part.mitcad"]);
    let stopped = sync(&p.a, &SyncOptions::default());
    let error = stopped.error.clone().unwrap();
    assert_eq!(error.class, ErrorClass::LocalChanges, "{stopped:?}");
    assert!(error.message.contains("part.mitcad"), "{}", error.message);
    assert_eq!(head(&p.a), before);
    assert_eq!(fs::read(&p.part_a).unwrap(), unsaved);
    assert_eq!(stopped.uncommitted, ["part.mitcad"]);
    // With a version of A's own the replay stops the same way.
    record_file(&p.a, "notes.txt", "A's notes\n", &author());
    let before = head(&p.a);
    let stopped = sync(&p.a, &SyncOptions::default());
    assert_eq!(stopped.case, SyncCase::Replay);
    assert_eq!(
        stopped.error.as_ref().map(|e| e.class),
        Some(ErrorClass::LocalChanges),
        "{stopped:?}"
    );
    assert_eq!(head(&p.a), before);
    assert_eq!(fs::read(&p.part_a).unwrap(), unsaved);
    assert!(backups(&p.a).is_empty());
    // A new file the sync does not touch neither stops it nor goes.
    fs::write(&p.part_a, p.a.read("HEAD", &p.part_a).unwrap()).unwrap();
    fs::write(p.a.root().join("scratch.txt"), "mine\n").unwrap();
    let done = sync(&p.a, &SyncOptions::default());
    assert_eq!(done.error, None, "{done:?}");
    assert_eq!(done.uncommitted, ["scratch.txt"]);
    assert_eq!(
        fs::read_to_string(p.a.root().join("scratch.txt")).unwrap(),
        "mine\n"
    );
    assert_eq!(fs::read(&p.part_a).unwrap(), fs::read(&p.part_b).unwrap());
    fs::remove_file(p.a.root().join("scratch.txt")).unwrap();
    check(&p.a, &["part.mitcad", "notes.txt"]);
}

/// Another push between the sync's fetch and its push: the remote refuses,
/// and the sync fetches and replays again (10.1, 9).
#[test]
fn a_push_refused_meanwhile_starts_the_round_again() {
    let Some(p) = pair("again") else {
        return;
    };
    new_version(&p.a, &p.part_a, 30.0);
    let (b_root, cli) = (p.b.root().to_path_buf(), p.cli.clone());
    let mut first = true;
    *p.a.sync_hook.borrow_mut() = Some(Box::new(move |stage: &str| {
        if stage == "pushing" && first {
            first = false;
            let b = ProjectRepo::open(&b_root).unwrap();
            b.set_git(cli.clone());
            record_file(&b, "notes.txt", "B's notes\n", &other());
            assert!(b.push(None).unwrap().pushed);
        }
    }));
    let done = sync(&p.a, &SyncOptions::default());
    assert_eq!(done.error, None, "{done:?}");
    assert_eq!(done.case, SyncCase::Push);
    assert_eq!(done.retries, 1);
    assert!(done.pushed);
    assert_eq!(done.replayed.len(), 1);
    assert!(done.backup.is_some());
    assert_eq!(tip(&p.remote, "refs/heads/main"), head(&p.a));
    assert_eq!(ids(&p.a, "notes.txt").len(), 1);
    assert!(p.a.root().join("notes.txt").is_file());
    check(&p.a, &["part.mitcad", "notes.txt"]);
}

/// A merge made with git in the versions the remote lacks, or a remote
/// branch of another history, is not replayed; nothing changes (10.1, 10).
#[test]
fn a_merge_in_the_unpublished_versions_is_not_replayed() {
    let Some(p) = pair("merge") else {
        return;
    };
    version_by(&p.b, &p.part_b, 25.0, &other());
    assert!(p.b.push(None).unwrap().pushed);
    let root = p.a.root();
    // A remote branch with a history of its own.
    let empty_tree = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    let foreign = git(root, &["commit-tree", empty_tree, "-m", "Foreign"]).unwrap();
    git(
        root,
        &["update-ref", "refs/remotes/origin/main", foreign.trim()],
    )
    .unwrap();
    let plan = p.a.sync_plan(false, None).unwrap();
    assert_eq!(plan.case, SyncCase::Unsupported);
    assert!(
        plan.reason.as_deref().unwrap().contains("another history"),
        "{plan:?}"
    );
    // A merge (the fetch below puts the remote's branch back).
    git(root, &["checkout", "-q", "-b", "side"]).unwrap();
    fs::write(root.join("side.txt"), "side\n").unwrap();
    git(root, &["add", "side.txt"]).unwrap();
    git(root, &["commit", "-q", "-m", "Side"]).unwrap();
    git(root, &["checkout", "-q", "main"]).unwrap();
    record_file(&p.a, "main.txt", "main\n", &author());
    git(
        root,
        &["merge", "-q", "--no-ff", "-m", "Merge side", "side"],
    )
    .unwrap();
    let before = head(&p.a);
    let plan = p.a.sync_plan(true, None).unwrap();
    assert_eq!(plan.case, SyncCase::Unsupported);
    assert!(
        plan.reason.as_deref().unwrap().contains("merge"),
        "{plan:?}"
    );
    let stopped = sync(&p.a, &SyncOptions::default());
    assert_eq!(
        stopped.error.as_ref().map(|e| e.class),
        Some(ErrorClass::Unsupported),
        "{stopped:?}"
    );
    assert_eq!(head(&p.a), before);
    assert!(backups(&p.a).is_empty());
}

/// A sync cancelled while fetching or while replaying changes nothing
/// (10.1, 11).
#[test]
fn a_cancelled_sync_changes_nothing() {
    let Some(p) = pair_with(
        "cancelled",
        |scratch, cli| fake_ssh(scratch, cli, ""),
        |_| "ssh://test/remote.git".to_owned(),
    ) else {
        return;
    };
    version_by(&p.b, &p.part_b, 25.0, &other());
    assert!(p.b.push(None).unwrap().pushed);
    record_file(&p.a, "notes.txt", "A's notes\n", &author());
    let before = head(&p.a);
    let remote_before = tip(&p.remote, "refs/heads/main");
    let unchanged = |p: &Pair| {
        assert_eq!(head(&p.a), before);
        assert_eq!(tip(&p.remote, "refs/heads/main"), remote_before);
        assert!(backups(&p.a).is_empty());
        assert_eq!(git(p.a.root(), &["status", "--porcelain"]).unwrap(), "");
    };
    // While fetching: git is stopped.
    p.a.set_git(fake_ssh(&p.scratch, &system_git().unwrap(), "hang"));
    let control = Control::new();
    let stopped = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(300));
            control.cancel();
        });
        p.a.sync(&SyncOptions::default(), Some(&author()), Some(&control))
            .unwrap()
    });
    assert_eq!(
        stopped.error.as_ref().map(|e| e.class),
        Some(ErrorClass::Cancelled),
        "{stopped:?}"
    );
    unchanged(&p);
    // While replaying, before anything changes.
    p.a.set_git(p.cli.clone());
    let control = Arc::new(Control::new());
    let cancel = Arc::clone(&control);
    *p.a.sync_hook.borrow_mut() = Some(Box::new(move |stage: &str| {
        if stage == "replayed" {
            cancel.cancel();
        }
    }));
    let stopped =
        p.a.sync(&SyncOptions::default(), Some(&author()), Some(&control))
            .unwrap();
    assert_eq!(stopped.case, SyncCase::Replay);
    assert_eq!(
        stopped.error.as_ref().map(|e| e.class),
        Some(ErrorClass::Cancelled),
        "{stopped:?}"
    );
    unchanged(&p);
    // Again without a cancel: done.
    *p.a.sync_hook.borrow_mut() = None;
    let done = sync(&p.a, &SyncOptions::default());
    assert_eq!(done.error, None, "{done:?}");
    assert_eq!(tip(&p.remote, "refs/heads/main"), head(&p.a));
    check(&p.a, &["part.mitcad", "notes.txt"]);
}

/// git configured to convert line endings on checkout (Git for Windows sets
/// `core.autocrlf=true` in its system configuration): the files a sync, a
/// fast-forward and a clone write are still the bytes Mitcad recorded, and
/// the folders stay clean.
#[test]
fn line_endings_stay_as_recorded_when_git_converts_them() {
    let Some(p) = pair_with(
        "autocrlf",
        |scratch, cli| {
            let config = scratch.join("autocrlf.gitconfig");
            fs::write(&config, "[core]\n\tautocrlf = true\n").unwrap();
            cli.clone()
                .with_env("GIT_CONFIG_GLOBAL", &config)
                .with_env("GIT_CONFIG_SYSTEM", &config)
        },
        |remote| remote.to_string_lossy().into_owned(),
    ) else {
        return;
    };
    let plan = "B's plan\nline two\n";
    record_file(&p.b, "plan.txt", plan, &other());
    version_by(&p.b, &p.part_b, 25.0, &other());
    assert!(p.b.push(None).unwrap().pushed);
    // A has a version of its own: the sync replays it and checks B's out.
    record_file(&p.a, "notes.txt", "A's notes\n", &author());
    let done = sync(&p.a, &SyncOptions::default());
    assert_eq!(done.error, None, "{done:?}");
    assert_eq!(done.case, SyncCase::Replay);
    assert_eq!(
        fs::read_to_string(p.a.root().join("plan.txt")).unwrap(),
        plan
    );
    assert_eq!(fs::read(&p.part_a).unwrap(), fs::read(&p.part_b).unwrap());
    check(&p.a, &["plan.txt", "notes.txt", "part.mitcad"]);
    // B takes A's with a fast-forward.
    let taken = sync_as(&p.b, &SyncOptions::default(), &other());
    assert_eq!(taken.error, None, "{taken:?}");
    assert_eq!(taken.case, SyncCase::FastForward);
    assert_eq!(
        fs::read_to_string(p.b.root().join("notes.txt")).unwrap(),
        "A's notes\n"
    );
    check(&p.b, &["plan.txt", "notes.txt", "part.mitcad"]);
    // A project opened from the remote.
    let url = p.remote.to_string_lossy().into_owned();
    let c = opened(&p.scratch, &p.cli, &url, "c");
    for file in ["plan.txt", "notes.txt", "part.mitcad", ".gitattributes"] {
        assert_eq!(
            fs::read(c.root().join(file)).unwrap(),
            fs::read(p.a.root().join(file)).unwrap(),
            "{file}"
        );
    }
    assert!(
        !fs::read(c.root().join("plan.txt"))
            .unwrap()
            .contains(&b'\r')
    );
    check(&c, &["plan.txt", "notes.txt", "part.mitcad"]);
}

/// A file too large for a git host holds a push back; a large one is a
/// warning.
#[test]
fn files_too_large_for_a_git_host_are_held_back() {
    let Some(p) = pair("large") else {
        return;
    };
    record_file(&p.a, "big.bin", &"x".repeat(3000), &author());
    let remote_before = tip(&p.remote, "refs/heads/main");
    let error = match p.a.push_within(None, (2000, 1000)) {
        Err(VcsError::Remote(error)) => error,
        other => panic!("{other:?}"),
    };
    assert_eq!(error.class, ErrorClass::TooLarge);
    assert!(error.message.contains("big.bin"), "{}", error.message);
    assert_eq!(tip(&p.remote, "refs/heads/main"), remote_before);
    let info = run(&p.a, json!({"cmd": "remote_info"}));
    assert_eq!(info["last_push"]["error"]["class"], "too_large", "{info}");
    let pushed = p.a.push_within(None, (10_000, 1000)).unwrap();
    assert!(pushed.pushed);
    assert!(
        pushed.warnings[0].contains("big.bin"),
        "{:?}",
        pushed.warnings
    );
    assert_eq!(tip(&p.remote, "refs/heads/main"), head(&p.a));
}

/// Backups older than 30 days go at a sync, but the newest 5 always stay.
#[test]
fn old_backups_go_but_the_newest_five_stay() {
    if !has_git() {
        return;
    }
    let scratch = Scratch::new("backups");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    let head = head(&repo);
    let names = [
        "20200101-000000",
        "20200102-000000",
        "20200103-000000",
        "20200104-000000",
        "20200105-000000",
        "20200106-000000",
        "20260930-120000",
        "20261001-120000",
    ];
    for name in names {
        git(
            repo.root(),
            &["update-ref", &format!("{BACKUP_PREFIX}{name}"), &head],
        )
        .unwrap();
    }
    repo.prune_backups("20200101-000000", &author()).unwrap();
    assert_eq!(backups(&repo).len(), names.len());
    repo.prune_backups("20260905-120312", &author()).unwrap();
    let kept: Vec<String> = names[3..]
        .iter()
        .map(|name| format!("{BACKUP_PREFIX}{name}"))
        .collect();
    assert_eq!(backups(&repo), kept);
    git(repo.root(), &["fsck", "--strict", "--no-progress"]).unwrap();
}

/// The sync commands' JSON: choices by path and for all, and the options.
#[test]
fn the_sync_commands_take_their_options() {
    let Some(p) = pair("json") else {
        return;
    };
    version_by(&p.b, &p.part_b, 25.0, &other());
    assert!(p.b.push(None).unwrap().pushed);
    new_version(&p.a, &p.part_a, 30.0);
    let refused =
        p.a.command(r#"{"cmd": "sync", "resolutions": {"part.mitcad": "newest"}}"#)
            .unwrap_err();
    assert!(
        refused.to_string().contains("mine, theirs or copy"),
        "{refused}"
    );
    let plan = run(&p.a, json!({"cmd": "sync_plan"}));
    assert_eq!(plan["error"], Value::Null, "{plan}");
    assert_eq!(plan["case"], "replay");
    assert_eq!(
        plan["conflicts"][0]["choices"],
        json!(["mine", "theirs", "copy"])
    );
    // A choice for all, without a push.
    let done = run(
        &p.a,
        json!({"cmd": "sync", "resolve_all": "mine", "push": false, "author": AUTHOR,
               "resolutions": {"other.txt": "copy"}}),
    );
    assert_eq!(done["error"], Value::Null, "{done}");
    assert_eq!(done["resolved"], json!({"part.mitcad": "mine"}));
    assert_eq!(done["pushed"], false);
    assert_eq!((&done["ahead"], &done["behind"]), (&json!(1), &json!(0)));
    assert!(
        done["warnings"][0]
            .as_str()
            .unwrap()
            .contains("other.txt is not a file changed both here and on the remote"),
        "{done}"
    );
    let log: Vec<&str> = done["log"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        log.iter().any(|line| line.starts_with("git reset --keep")),
        "{log:?}"
    );
    // Then pushed by a sync of its own.
    let pushed = run(&p.a, json!({"cmd": "sync"}));
    assert_eq!(
        (&pushed["case"], &pushed["pushed"]),
        (&json!("push"), &json!(true))
    );
    check(&p.a, &["part.mitcad"]);
}

/// The remote's newer versions as the last fetch left them, by file (the
/// application's notice of a newer version of the open design), and a
/// remote's version compared by its full id, which HEAD does not reach (a
/// conflict's Compare).
#[test]
fn newer_versions_on_the_remote_are_told_by_file() {
    let Some(p) = pair("incoming") else {
        return;
    };
    let none = p.b.incoming(Some(&p.part_b)).unwrap();
    assert_eq!((none.ahead, none.behind, none.differs), (0, 0, false));
    assert!(none.file_versions.is_empty() && !none.changed_here);
    // A records a version of the design and one of another file.
    let from_a = new_version(&p.a, &p.part_a, 30.0);
    let notes = record_file(&p.a, "notes.txt", "notes\n", &author());
    assert!(p.a.push(None).unwrap().pushed);
    // Nothing is known of them before a fetch.
    assert_eq!(p.b.incoming(None).unwrap().behind, 0);
    p.b.fetch(None).unwrap();
    let all = p.b.incoming(None).unwrap();
    assert_eq!((all.upstream.as_str(), all.behind), ("origin/main", 2));
    let listed: Vec<&str> = all.versions.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(listed, [notes.as_str(), from_a.as_str()]);
    assert!(all.path.is_none() && all.file_versions.is_empty());
    let part = p.b.incoming(Some(&p.part_b)).unwrap();
    assert_eq!(part.path.as_deref(), Some("part.mitcad"));
    assert!(part.differs && !part.changed_here, "{part:?}");
    assert_eq!(part.file_versions.len(), 1);
    assert_eq!(part.file_versions[0].id, from_a);
    assert_eq!(part.file_versions[0].author, author());
    let text =
        p.b.command_text(&json!({"cmd": "incoming", "path": p.part_b}).to_string())
            .unwrap();
    assert!(
        text.contains("part.mitcad: a newer version is on the remote"),
        "{text}"
    );
    // B changes the design too: a sync will ask what to keep.
    version_by(&p.b, &p.part_b, 40.0, &other());
    let both = run(&p.b, json!({"cmd": "incoming", "path": p.part_b}));
    assert_eq!(both["error"], Value::Null, "{both}");
    assert_eq!((&both["ahead"], &both["behind"]), (&json!(1), &json!(2)));
    assert_eq!(
        (&both["differs"], &both["changed_here"]),
        (&json!(true), &json!(true))
    );
    // The remote's version of the file against the project's, by the full
    // id; a prefix names only versions HEAD reaches.
    let compared = run(
        &p.b,
        json!({"cmd": "diff", "path": p.part_b, "from": from_a, "to": "HEAD"}),
    );
    let compared = compared["text"].as_str().unwrap_or_default();
    assert!(compared.contains("30 mm -> 40 mm"), "{compared}");
    assert!(p.b.resolve(&from_a[..7]).is_err());
    assert_eq!(p.b.resolve(&from_a).unwrap(), from_a);
    // Nothing changed.
    check(&p.b, &["part.mitcad"]);
}
