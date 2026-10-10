// SPDX-License-Identifier: MIT
//! Local and Cloud projects (mitcad#89) against folders and repositories in
//! temporary folders: what a folder is, projects made Local and Cloud (a
//! bare repository stands in for the cloud), a repository with files made
//! a project, a Local project shared onto a repository's files, the
//! project's settings and author, shared folders, and SSH host keys (with
//! a fake `ssh-keyscan` where a shell runs it). Nothing goes over the
//! network. After each change the repositories are checked as git sees
//! them (`git fsck`, a clean `git status`). Without the system's git the
//! checks that need it are skipped.

use serde_json::{Value, json};

use super::remote::{AUTHOR, bare, error_class, run, system_git, tip};
use super::*;
use crate::projects::{
    self, ADOPT_MESSAGE, Context, CreateOptions, FolderKind, Published, check_remote,
    clone_project, create_project, fingerprint, host_keys, init_bare, inspect_folder,
    parse_keyscan, ssh_public_key, trust_host_key,
};
use crate::remote::{Control, ErrorClass, GitCli};

/// GitHub's published Ed25519 host key (its documentation's known_hosts
/// line).
const GITHUB_ED25519: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl";
/// GitLab's.
const GITLAB_ED25519: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf";

fn context(cli: &GitCli) -> Context {
    Context {
        git: Some(cli.clone()),
        ..Context::default()
    }
}

fn options(url: Option<&str>) -> CreateOptions {
    CreateOptions {
        author: author(),
        design: Some(("design.mitcad".to_owned(), design().to_json())),
        include_designs: true,
        url: url.map(str::to_owned),
        push: true,
        shared: None,
    }
}

/// The class of a remote failure.
fn class_of<T: std::fmt::Debug>(result: Result<T, VcsError>) -> ErrorClass {
    match result {
        Err(VcsError::Remote(error)) => error.class,
        other => panic!("not a remote failure: {other:?}"),
    }
}

/// A bare remote `name` whose branch `branch` (its HEAD) holds one commit
/// of `files`.
fn remote_with(scratch: &Scratch, name: &str, branch: &str, files: &[(&str, &str)]) -> PathBuf {
    let remote = bare(scratch, name);
    let work_name = format!("{name}-work");
    git(
        &scratch.0,
        &[
            "-c",
            &format!("init.defaultBranch={branch}"),
            "init",
            "-q",
            &work_name,
        ],
    )
    .unwrap();
    let work = scratch.join(&work_name);
    for (path, text) in files {
        let file = work.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    git(&work, &["add", "-A"]).unwrap();
    git(&work, &["commit", "-q", "-m", "Initial files"]).unwrap();
    git(
        &work,
        &[
            "push",
            "-q",
            &remote.to_string_lossy(),
            &format!("{branch}:{branch}"),
        ],
    )
    .unwrap();
    git(
        &remote,
        &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
    )
    .unwrap();
    remote
}

/// The repository as git sees it: no damage, nothing changed in the folder.
fn clean(root: &Path) {
    git(
        root,
        &["fsck", "--strict", "--no-progress", "--no-dangling"],
    )
    .unwrap();
    assert_eq!(
        git(root, &["status", "--porcelain"]).unwrap(),
        "",
        "{root:?}"
    );
}

/// The summaries of the commits on HEAD's first-parent chain, newest first.
fn log_of(root: &Path) -> Vec<String> {
    git(root, &["log", "--first-parent", "--format=%s"])
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// Runs a projects command, which must not fail as a command.
fn projects_run(command: Value, context: &Context) -> Value {
    projects::run(&command.to_string(), context, None)
        .unwrap()
        .1
}

#[test]
fn a_folder_is_told_for_what_it_is() {
    let scratch = Scratch::new("inspect");
    let kind = |path: &Path| inspect_folder(path).unwrap().kind;
    assert_eq!(kind(&scratch.join("missing")), FolderKind::Missing);
    let empty = scratch.join("empty");
    fs::create_dir(&empty).unwrap();
    fs::write(empty.join(".DS_Store"), "x").unwrap();
    fs::write(empty.join("Thumbs.db"), "x").unwrap();
    assert_eq!(kind(&empty), FolderKind::Empty);
    let designs = scratch.join("designs");
    fs::create_dir_all(designs.join("parts")).unwrap();
    fs::write(designs.join("parts/bracket.mitcad"), design().to_json()).unwrap();
    fs::write(designs.join("notes.txt"), "notes").unwrap();
    let info = inspect_folder(&designs).unwrap();
    assert_eq!(info.kind, FolderKind::Designs);
    assert_eq!(info.designs.len(), 1);
    assert_eq!(info.designs[0].path, "parts/bracket.mitcad");
    assert!(info.designs[0].modified.as_ref().unwrap().ends_with('Z'));
    assert_eq!(info.settings, None);
    let other = scratch.join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("notes.txt"), "notes").unwrap();
    assert_eq!(kind(&other), FolderKind::Other);
    let file = other.join("notes.txt");
    assert!(matches!(
        inspect_folder(&file),
        Err(VcsError::InvalidPath(_))
    ));

    // A Local project and a folder inside it.
    let (repo, _) = ProjectRepo::init(&scratch.join("local"), Some(&author())).unwrap();
    let part = repo.root().join("part.mitcad");
    design().save_file(&part, FileFormat::Auto).unwrap();
    let info = inspect_folder(repo.root()).unwrap();
    assert_eq!(info.kind, FolderKind::Project);
    assert!(info.has_history && !info.cloud);
    assert_eq!(info.project_root.as_deref(), Some(info.dir.as_str()));
    assert_eq!(info.outer_repository, None);
    assert_eq!(info.remote, None);
    assert_eq!(info.designs[0].path, "part.mitcad");
    assert_eq!(info.last_design, None);
    assert_eq!(
        info.settings.as_ref().unwrap()["edit_locks"]["enabled"],
        true
    );
    let inner = repo.root().join("sub");
    fs::create_dir(&inner).unwrap();
    let info = inspect_folder(&inner).unwrap();
    assert_eq!(info.kind, FolderKind::InsideProject);
    assert_eq!(
        info.project_root.as_deref(),
        Some(repo.root().to_str().unwrap())
    );
    assert!(info.has_history);
    assert_eq!(info.designs[0].path, "part.mitcad", "the project's designs");
    // The design opened last, while it is there.
    let answer = run(&repo, json!({"cmd": "remember_design", "path": part}));
    assert_eq!(answer["path"], "part.mitcad");
    assert_eq!(
        inspect_folder(repo.root()).unwrap().last_design.as_deref(),
        Some("part.mitcad")
    );
    fs::rename(&part, repo.root().join("renamed.mitcad")).unwrap();
    assert_eq!(inspect_folder(repo.root()).unwrap().last_design, None);

    let Some(cli) = system_git() else {
        return;
    };
    repo.set_git(cli);
    // A repository that is not a project.
    git(&scratch.0, &["init", "-q", "plain"]).unwrap();
    // (gix may give a work tree in another spelling, as on Windows.)
    let same = |a: Option<&str>, b: &Path| {
        a.is_some_and(|a| fs::canonicalize(a).unwrap() == fs::canonicalize(b).unwrap())
    };
    let info = inspect_folder(&scratch.join("plain")).unwrap();
    assert_eq!(info.kind, FolderKind::Repository);
    assert!(
        same(info.git_root.as_deref(), Path::new(&info.dir)),
        "{info:?}"
    );
    assert!(!info.has_history && !info.cloud);
    // A project inside another repository: no versions.
    let nested = scratch.join("plain/nested");
    mitcad_model::Project::init(&nested).unwrap();
    let info = inspect_folder(&nested).unwrap();
    assert_eq!(info.kind, FolderKind::Project);
    assert!(!info.has_history);
    assert!(
        same(info.outer_repository.as_deref(), &scratch.join("plain")),
        "{info:?}"
    );
    assert!(
        info.history_blocked
            .unwrap()
            .contains("inside the git repository")
    );
    // A missing folder there: a new project would get a repository of its own.
    let info = inspect_folder(&scratch.join("plain/new")).unwrap();
    assert_eq!(info.kind, FolderKind::Missing);
    assert!(info.outer_repository.is_some() && info.history_blocked.is_none());

    // Remotes: the only one is followed, of several none is.
    let root = repo.root();
    git(
        root,
        &["remote", "add", "upstream", "https://example.invalid/a.git"],
    )
    .unwrap();
    let info = inspect_folder(root).unwrap();
    assert!(info.cloud);
    let remote = info.remote.unwrap();
    assert_eq!(
        (
            remote.name.as_str(),
            remote.upstream.as_deref(),
            remote.branch.as_deref()
        ),
        ("upstream", Some("upstream/main"), Some("main"))
    );
    git(
        root,
        &[
            "remote",
            "add",
            "backup",
            "https://u:s3cret@example.invalid/b.git",
        ],
    )
    .unwrap();
    let info = inspect_folder(root).unwrap();
    assert!(info.cloud);
    assert_eq!(info.remote, None);
    assert_eq!(info.remotes, ["backup", "upstream"]);
    // remote_info says so too, and which to choose from.
    let shown = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(shown["name"], Value::Null, "{shown}");
    assert_eq!(
        shown["remotes"],
        json!([{"name": "backup", "url": "https://***@example.invalid/b.git"},
               {"name": "upstream", "url": "https://example.invalid/a.git"}])
    );
    let text = repo
        .command_text(&json!({"cmd": "remote_info"}).to_string())
        .unwrap();
    assert!(
        text.contains(
            "Remotes backup (https://***@example.invalid/b.git), upstream \
             (https://example.invalid/a.git): none is followed"
        ),
        "{text}"
    );
    assert!(!text.contains("s3cret"), "{text}");
    // remote_follow (mitcad#89): a remote that is not there, a name that
    // is none, then the one chosen, its branch of the same name.
    let missing = run(&repo, json!({"cmd": "remote_follow", "name": "nowhere"}));
    assert_eq!(error_class(&missing), "no_remote", "{missing}");
    assert!(
        repo.command(&json!({"cmd": "remote_follow", "name": "-x"}).to_string())
            .is_err()
    );
    assert!(
        repo.command(
            &json!({"cmd": "remote_follow", "name": "backup", "branch": "a..b"}).to_string()
        )
        .is_err()
    );
    let followed = run(&repo, json!({"cmd": "remote_follow", "name": "upstream"}));
    assert_eq!(followed["error"], Value::Null, "{followed}");
    assert_eq!(followed["upstream"], "upstream/main");
    assert_eq!(followed["branch"], "main");
    assert_eq!(followed["changed"], true);
    assert_eq!(followed["ahead"], Value::Null, "not fetched yet");
    assert_eq!(
        git(root, &["config", "branch.main.remote"]).unwrap().trim(),
        "upstream"
    );
    let again = run(&repo, json!({"cmd": "remote_follow", "name": "upstream"}));
    assert_eq!(again["changed"], false, "{again}");
    assert_eq!(
        inspect_folder(root).unwrap().remote.unwrap().name,
        "upstream"
    );
    assert_eq!(
        run(&repo, json!({"cmd": "remote_info"}))["upstream"],
        "upstream/main"
    );
    // A branch with an upstream follows it (another remote's other branch
    // chosen); credentials are hidden.
    let followed = run(
        &repo,
        json!({"cmd": "remote_follow", "name": "backup", "branch": "trunk"}),
    );
    assert_eq!(followed["upstream"], "backup/trunk", "{followed}");
    assert_eq!(followed["url"], "https://***@example.invalid/b.git");
    let text = repo
        .command_text(
            &json!({"cmd": "remote_follow", "name": "backup", "branch": "trunk"}).to_string(),
        )
        .unwrap();
    assert!(text.contains("Branch main follows backup/trunk"), "{text}");
    assert_eq!(
        git(root, &["config", "branch.main.merge"]).unwrap().trim(),
        "refs/heads/trunk"
    );
    let remote = inspect_folder(root).unwrap().remote.unwrap();
    assert_eq!(remote.upstream.as_deref(), Some("backup/trunk"));
    assert_eq!(
        remote.url.as_deref(),
        Some("https://***@example.invalid/b.git")
    );
    // As JSON and as text.
    let answer = projects_run(
        json!({"cmd": "inspect_folder", "dir": root}),
        &Context::default(),
    );
    assert_eq!(answer["kind"], "project");
    assert_eq!(answer["error"], Value::Null);
    let text = crate::api::projects_command_text(
        &json!({"cmd": "inspect_folder", "dir": root}).to_string(),
        None,
    )
    .unwrap();
    assert!(text.contains(": a Cloud project\n"), "{text}");
    assert!(!text.contains("s3cret"), "{text}");
}

#[test]
fn a_local_project_is_made_with_its_first_version() {
    let scratch = Scratch::new("create-local");
    let dir = scratch.join("Robot arm");
    let mut log = Vec::new();
    let created =
        create_project(&dir, &options(None), &Context::default(), None, &mut log).unwrap();
    assert_eq!(created.branch.as_deref(), Some("main"));
    assert_eq!(
        created.files,
        [
            ".gitattributes",
            ".gitignore",
            ".mitcad/project.json",
            "design.mitcad"
        ]
    );
    assert!(created.commit.is_some() && created.error.is_none());
    assert_eq!(created.remote, None);
    let repo = ProjectRepo::open(&dir).unwrap();
    assert_eq!(
        repo.history(&dir.join("design.mitcad")).unwrap()[0].summary,
        "Create project Robot arm"
    );
    // The author is the repository's own.
    assert_eq!(repo.own_identity(), Some(author()));
    let info = inspect_folder(&dir).unwrap();
    assert!(info.has_history && !info.cloud);
    // Not again, and not inside it.
    let again = create_project(&dir, &options(None), &Context::default(), None, &mut log);
    assert_eq!(class_of(again), ErrorClass::InsideProject);
    let inside = create_project(
        &dir.join("sub"),
        &options(None),
        &Context::default(),
        None,
        &mut log,
    );
    assert_eq!(class_of(inside), ErrorClass::InsideProject);

    // A folder of designs and other files: the designs are in the first
    // version, the other files stay as they are.
    let folder = scratch.join("folder");
    fs::create_dir_all(folder.join("parts")).unwrap();
    fs::write(folder.join("parts/a.mitcad"), design().to_json()).unwrap();
    fs::write(folder.join("notes.txt"), "notes").unwrap();
    let created =
        create_project(&folder, &options(None), &Context::default(), None, &mut log).unwrap();
    assert!(
        created.files.contains(&"parts/a.mitcad".to_owned()),
        "{:?}",
        created.files
    );
    assert!(!created.files.contains(&"notes.txt".to_owned()));
    assert!(folder.join("notes.txt").is_file());
    // The shared settings in the first version (New Project's Live updates).
    let shared = json!({"edit_locks": {"idle_minutes": 20},
                        "live_updates": {"broker": "mqtts://broker.example.com"}});
    let answer = projects_run(
        json!({"cmd": "create_project", "dir": scratch.join("live"), "author": AUTHOR,
               "shared": shared}),
        &Context::default(),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    let live = ProjectRepo::open(&scratch.join("live")).unwrap();
    let settings = run(&live, json!({"cmd": "project_settings"}));
    assert_eq!(settings["shared"]["edit_locks"]["idle_minutes"], 20);
    assert_eq!(settings["shared"]["edit_locks"]["enabled"], true);
    assert_eq!(
        settings["shared"]["live_updates"]["broker"],
        "mqtts://broker.example.com:8883"
    );
    let marker = live.root().join(PROJECT_MARKER);
    assert_eq!(
        live.history(&marker).unwrap().len(),
        1,
        "in the first version"
    );
    let refused = projects::run(
        &json!({"cmd": "create_project", "dir": scratch.join("bad"), "author": AUTHOR,
                "shared": {"edit_locks": {"poll_seconds": 1}}})
        .to_string(),
        &Context::default(),
        None,
    );
    assert!(matches!(refused, Err(VcsError::Command(_))), "{refused:?}");
    assert!(!scratch.join("bad").exists());
    // A design that is there already is not overwritten.
    let taken = scratch.join("taken");
    fs::create_dir(&taken).unwrap();
    fs::write(taken.join("design.mitcad"), "mine").unwrap();
    let refused = create_project(&taken, &options(None), &Context::default(), None, &mut log);
    assert!(
        matches!(refused, Err(VcsError::InvalidPath(_))),
        "{refused:?}"
    );
    assert!(!taken.join(".mitcad").exists());

    // A cancel or a failure leaves the folder as it was.
    let control = Control::new();
    control.cancel();
    let missing = scratch.join("cancelled");
    let cancelled = create_project(
        &missing,
        &options(None),
        &Context::default(),
        Some(&control),
        &mut log,
    );
    assert_eq!(class_of(cancelled), ErrorClass::Cancelled);
    assert!(!missing.exists());
    let broken = scratch.join("broken");
    fs::create_dir_all(broken.join(".gitignore")).unwrap();
    fs::write(broken.join("notes.txt"), "notes").unwrap();
    let failed = create_project(&broken, &options(None), &Context::default(), None, &mut log);
    assert!(failed.is_err());
    let mut left: Vec<String> = fs::read_dir(&broken)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, [".gitignore", "notes.txt"]);

    if system_git().is_some() {
        clean(&dir);
        // The other files are not versioned: git sees them as new.
        assert_eq!(
            git(&folder, &["status", "--porcelain"]).unwrap(),
            "?? notes.txt\n"
        );
    }
}

#[test]
fn a_cloud_project_is_made_on_an_empty_remote() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("create-cloud");
    let remote = bare(&scratch, "remote.git");
    let url = remote.to_string_lossy().into_owned();
    let answer = projects_run(
        json!({"cmd": "create_project", "dir": scratch.join("a"), "author": AUTHOR,
               "url": url, "design": {"path": "a.mitcad", "text": design().to_json()}}),
        &context(&cli),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["adopted"], false);
    assert_eq!(answer["push"]["pushed"], true, "{answer}");
    assert_eq!(answer["remote"]["name"], "origin");
    assert_eq!(answer["remote"]["upstream"], "origin/main");
    assert_eq!(
        (&answer["ahead"], &answer["behind"]),
        (&json!(0), &json!(0))
    );
    assert!(answer["log"].as_array().unwrap().len() >= 2, "{answer}");
    let a = scratch.join("a");
    assert_eq!(tip(&remote, "refs/heads/main"), tip(&a, "HEAD"));
    assert!(inspect_folder(&a).unwrap().cloud);
    clean(&a);
    git(&remote, &["fsck", "--strict", "--no-progress"]).unwrap();

    // Without a push the versions wait.
    let other = bare(&scratch, "other.git");
    let mut log = Vec::new();
    let mut no_push = options(Some(&other.to_string_lossy()));
    no_push.push = false;
    let created =
        create_project(&scratch.join("b"), &no_push, &context(&cli), None, &mut log).unwrap();
    assert_eq!(created.push, None);
    assert_eq!(created.remote.unwrap().name, "origin");
    assert_eq!(created.ahead, None, "never pushed or fetched");

    // An unreachable remote changes nothing.
    let nowhere = scratch.join("nowhere.git").to_string_lossy().into_owned();
    let answer = projects_run(
        json!({"cmd": "create_project", "dir": scratch.join("c"), "author": AUTHOR,
               "url": nowhere}),
        &context(&cli),
    );
    assert_eq!(error_class(&answer), "not_found", "{answer}");
    assert!(!scratch.join("c").exists());
    // A remote with a project is not made a second one.
    let answer = projects_run(
        json!({"cmd": "create_project", "dir": scratch.join("d"), "author": AUTHOR,
               "url": url}),
        &context(&cli),
    );
    assert_eq!(error_class(&answer), "has_project", "{answer}");
    assert!(!scratch.join("d").exists());
}

#[test]
fn a_cloud_project_takes_a_remotes_files_in() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("create-files");
    let remote = remote_with(
        &scratch,
        "remote.git",
        "master",
        &[("README.md", "# Robot arm\n"), (".gitignore", "*.log\n")],
    );
    let url = remote.to_string_lossy().into_owned();
    // The check Open from Cloud and New Project show.
    let mut log = Vec::new();
    let check = check_remote(&url, &context(&cli), None, &mut log).unwrap();
    assert_eq!(check.has_project, Some(false));
    assert_eq!(check.default_branch.as_deref(), Some("master"));
    assert_eq!(check.files, [".gitignore", "README.md"]);
    assert_eq!(check.versions, Some(1));
    assert_eq!(check.latest.as_ref().unwrap().author, "Git User");
    assert!(
        log.iter().any(|line| line.starts_with("git clone --bare")),
        "{log:?}"
    );

    // A folder with files is not mixed with the remote's.
    let busy = scratch.join("busy");
    fs::create_dir(&busy).unwrap();
    fs::write(busy.join("notes.txt"), "notes").unwrap();
    let refused = create_project(&busy, &options(Some(&url)), &context(&cli), None, &mut log);
    assert_eq!(class_of(refused), ErrorClass::NotEmpty);
    assert!(!busy.join(".mitcad").exists() && !busy.join(".git").exists());

    let dir = scratch.join("a");
    let created =
        create_project(&dir, &options(Some(&url)), &context(&cli), None, &mut log).unwrap();
    assert!(created.error.is_none(), "{:?}", created.error);
    assert!(created.adopted);
    assert_eq!(created.branch.as_deref(), Some("master"));
    assert_eq!(
        created.remote.as_ref().unwrap().upstream.as_deref(),
        Some("origin/master")
    );
    assert!(created.push.as_ref().unwrap().pushed);
    assert_eq!(
        fs::read_to_string(dir.join("README.md")).unwrap(),
        "# Robot arm\n"
    );
    let ignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
    assert!(
        ignore.starts_with("*.log\n") && ignore.contains(".mitcad/local/"),
        "{ignore}"
    );
    assert_eq!(log_of(&dir), ["Create project a", "Initial files"]);
    assert_eq!(tip(&remote, "refs/heads/master"), tip(&dir, "HEAD"));
    clean(&dir);
    git(&remote, &["fsck", "--strict", "--no-progress"]).unwrap();
    // Now the remote holds a project.
    let check = check_remote(&url, &context(&cli), None, &mut log).unwrap();
    assert_eq!((check.has_project, check.versions), (Some(true), Some(2)));

    // A clone of the remote made a project in place: its history stays.
    let other = remote_with(&scratch, "other.git", "main", &[("README.md", "other\n")]);
    let other_url = other.to_string_lossy().into_owned();
    git(&scratch.0, &["clone", "-q", &other_url, "clone"]).unwrap();
    let clone = scratch.join("clone");
    let created = create_project(
        &clone,
        &options(Some(&other_url)),
        &context(&cli),
        None,
        &mut log,
    )
    .unwrap();
    assert!(created.error.is_none(), "{:?}", created.error);
    assert_eq!(log_of(&clone), ["Create project clone", "Initial files"]);
    assert_eq!(tip(&other, "refs/heads/main"), tip(&clone, "HEAD"));
    clean(&clone);
}

#[test]
fn a_repository_with_files_is_adopted_when_opened() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("adopt");
    let remote = remote_with(&scratch, "remote.git", "main", &[("README.md", "# Arm\n")]);
    let url = remote.to_string_lossy().into_owned();
    let mut log = Vec::new();
    // Without adopt it is not a project, and nothing stays.
    let refused = clone_project(
        &url,
        &scratch.join("x"),
        false,
        None,
        &context(&cli),
        None,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::NotAProject);
    assert!(!scratch.join("x").exists());
    // Not into a folder with files.
    fs::create_dir(scratch.join("busy")).unwrap();
    fs::write(scratch.join("busy/a.txt"), "a").unwrap();
    let refused = clone_project(
        &url,
        &scratch.join("busy"),
        true,
        None,
        &context(&cli),
        None,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::NotEmpty);

    let answer = projects_run(
        json!({"cmd": "clone_project", "url": url, "dir": scratch.join("a"), "adopt": true,
               "author": AUTHOR}),
        &context(&cli),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["adopted"], true);
    assert_eq!(answer["push"]["pushed"], true);
    assert_eq!(answer["files"], json!([]));
    assert_eq!(answer["branch"], "main");
    let a = scratch.join("a");
    assert_eq!(answer["commit"].as_str().unwrap(), tip(&a, "HEAD"));
    assert_eq!(log_of(&a), [ADOPT_MESSAGE, "Initial files"]);
    assert_eq!(tip(&remote, "refs/heads/main"), tip(&a, "HEAD"));
    assert_eq!(
        ProjectRepo::open(&a).unwrap().own_identity(),
        Some(author())
    );
    clean(&a);
    // Now a project: opened without adopting.
    let opened = clone_project(
        &url,
        &scratch.join("b"),
        false,
        None,
        &context(&cli),
        None,
        &mut log,
    )
    .unwrap();
    assert!(!opened.adopted && opened.commit.is_none());
    // An empty remote stays not a project.
    let empty = bare(&scratch, "empty.git");
    let refused = clone_project(
        &empty.to_string_lossy(),
        &scratch.join("c"),
        true,
        Some(&author()),
        &context(&cli),
        None,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::NotAProject);
    assert!(!scratch.join("c").exists());
}

#[test]
fn a_local_project_is_shared_onto_a_repositorys_files() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("onto");
    let make = |name: &str| {
        let dir = scratch.join(name);
        create_project(
            &dir,
            &options(None),
            &Context::default(),
            None,
            &mut Vec::new(),
        )
        .unwrap();
        let repo = ProjectRepo::open(&dir).unwrap();
        repo.set_git(cli.clone());
        repo
    };
    // A README only, on a branch of another name.
    let remote = remote_with(
        &scratch,
        "remote.git",
        "trunk",
        &[
            ("README.md", "# Arm\n"),
            (".gitignore", "*.log\n.mitcad/local/\n"),
        ],
    );
    let url = remote.to_string_lossy().into_owned();
    let a = make("a");
    let refused = run(&a, json!({"cmd": "connect", "url": url, "author": AUTHOR}));
    assert_eq!(error_class(&refused), "unrelated", "{refused}");
    let answer = run(
        &a,
        json!({"cmd": "connect", "url": url, "author": AUTHOR, "onto_files": true}),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["set"]["upstream"], "origin/trunk");
    assert_eq!(answer["sync"]["case"], "replay");
    assert_eq!(answer["push"]["pushed"], true);
    assert_eq!(answer["undone"], false);
    assert_eq!(
        (&answer["ahead"], &answer["behind"]),
        (&json!(0), &json!(0))
    );
    let root = a.root();
    assert_eq!(
        fs::read_to_string(root.join("README.md")).unwrap(),
        "# Arm\n"
    );
    // The remote's lines, then the project's missing ones.
    let ignore = fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.starts_with("*.log\n.mitcad/local/\n"), "{ignore}");
    assert_eq!(ignore.matches(".mitcad/local/").count(), 1, "{ignore}");
    assert!(ignore.contains(".mitcad/cache/"), "{ignore}");
    assert_eq!(log_of(root), ["Create project a", "Initial files"]);
    assert_eq!(tip(&remote, "refs/heads/trunk"), tip(root, "HEAD"));
    assert_eq!(
        a.remote_info(false).unwrap().upstream.as_deref(),
        Some("origin/trunk")
    );
    clean(root);
    git(&remote, &["fsck", "--strict", "--no-progress"]).unwrap();

    // A path on both sides is a conflict: nothing changes without a choice.
    let other = remote_with(&scratch, "other.git", "main", &[("notes.txt", "theirs\n")]);
    let other_url = other.to_string_lossy().into_owned();
    let b = make("b");
    let notes = b.root().join("notes.txt");
    fs::write(&notes, "mine\n").unwrap();
    b.commit(std::slice::from_ref(&notes), "Notes", &author())
        .unwrap();
    let head = tip(b.root(), "HEAD");
    let answer = run(
        &b,
        json!({"cmd": "connect", "url": other_url, "author": AUTHOR, "onto_files": true}),
    );
    assert_eq!(error_class(&answer), "conflict", "{answer}");
    assert_eq!(answer["undone"], true);
    assert_eq!(answer["sync"]["conflicts"][0]["path"], "notes.txt");
    assert_eq!(answer["sync"]["conflicts"][0]["kind"], "added_both");
    assert_eq!(tip(b.root(), "HEAD"), head);
    assert_eq!(
        b.remote_info(false).unwrap().name,
        None,
        "the remote is gone again"
    );
    let text = crate::remote::commands::describe_answer("connect", &answer);
    assert!(
        text.contains("Conflict: notes.txt: added on both sides"),
        "{text}"
    );
    // Kept mine.
    let answer = run(
        &b,
        json!({"cmd": "connect", "url": other_url, "author": AUTHOR, "onto_files": true,
               "resolutions": {"notes.txt": "mine"}}),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(fs::read_to_string(&notes).unwrap(), "mine\n");
    assert_eq!(
        log_of(b.root()),
        ["Notes", "Create project b", "Initial files"]
    );
    clean(b.root());

    // A remote with another project stays unrelated.
    let c = make("c");
    let answer = run(
        &c,
        json!({"cmd": "connect", "url": url, "author": AUTHOR, "onto_files": true}),
    );
    assert_eq!(error_class(&answer), "unrelated", "{answer}");
    assert_eq!(c.remote_info(false).unwrap().name, None);
}

#[test]
fn the_projects_author_is_its_repositorys() {
    let scratch = Scratch::new("identity");
    let (repo, _, _) = project(&scratch);
    let isolated = || ProjectRepo::open_with(repo.root(), gix::open::Options::isolated()).unwrap();
    let settings = Identity::new("From Settings", "settings@example.invalid").unwrap();
    assert_eq!(isolated().identity(Some(&settings)).unwrap(), settings);
    let answer = run(
        &repo,
        json!({"cmd": "set_identity", "name": "Ada Lovelace", "email": "ada@example.invalid"}),
    );
    assert_eq!(
        answer,
        json!({"name": "Ada Lovelace", "email": "ada@example.invalid"})
    );
    let ada = Identity::new("Ada Lovelace", "ada@example.invalid").unwrap();
    // Read at once by the same project, before git's configuration.
    assert_eq!(repo.identity(Some(&settings)).unwrap(), ada);
    assert_eq!(isolated().identity(None).unwrap(), ada);
    let identity = run(&repo, json!({"cmd": "identity"}));
    assert_eq!(identity["name"], "Ada Lovelace");
    if system_git().is_some() {
        assert_eq!(
            git(repo.root(), &["config", "--local", "user.email"])
                .unwrap()
                .trim(),
            "ada@example.invalid"
        );
    }
    // Bad ones change nothing.
    for (name, email) in [("Ada", ""), ("A<da", "a@b"), ("Ada", "a\u{7}@b")] {
        let result =
            repo.command(&json!({"cmd": "set_identity", "name": name, "email": email}).to_string());
        assert!(result.is_err(), "{name} {email}");
    }
    assert_eq!(repo.own_identity(), Some(ada));
    // Both empty: removed.
    run(
        &repo,
        json!({"cmd": "set_identity", "name": "", "email": ""}),
    );
    assert_eq!(repo.own_identity(), None);
    assert_eq!(isolated().identity(Some(&settings)).unwrap(), settings);
    let text = repo
        .command_text(r#"{"cmd": "set_identity", "name": "B", "email": "b@example.invalid"}"#)
        .unwrap();
    assert_eq!(text, "Author of this project: B <b@example.invalid>\n");
}

#[test]
fn shared_settings_are_recorded_as_a_version() {
    let scratch = Scratch::new("settings");
    let (repo, _) = ProjectRepo::init(&scratch.join("project"), Some(&author())).unwrap();
    let settings = run(&repo, json!({"cmd": "project_settings"}));
    assert_eq!(settings["kind"], "local");
    assert_eq!(settings["shared"]["edit_locks"]["enabled"], true);
    assert_eq!(settings["live_updates"], Value::Null);
    // Who made the versions sharing publishes.
    assert_eq!(
        settings["authors"],
        json!([{"name": "Mitcad Test", "email": "test@example.invalid", "versions": 1}])
    );
    let marker = repo.root().join(PROJECT_MARKER);
    let before = fs::read(&marker).unwrap();
    // Checked strictly: nothing is written.
    let refused = repo.command(
        &json!({"cmd": "set_project_settings", "shared": {"edit_locks": {"idle_minutes": 500}},
                "author": AUTHOR})
        .to_string(),
    );
    assert!(refused.is_err());
    assert_eq!(fs::read(&marker).unwrap(), before);
    // Locks off, the rest kept.
    let answer = run(
        &repo,
        json!({"cmd": "set_project_settings", "author": AUTHOR,
               "shared": {"edit_locks": {"enabled": false},
                          "live_updates": {"broker": "mqtts://broker.example.com"}}}),
    );
    assert!(answer["commit"].is_string(), "{answer}");
    assert_eq!(answer["written_local"], false);
    assert_eq!(answer["shared"]["edit_locks"]["idle_minutes"], 10);
    let history = repo.history(&marker).unwrap();
    assert_eq!(
        history[0].summary,
        "Change project settings: edit locks off, live updates through \
         mqtts://broker.example.com:8883"
    );
    let marker_json: Value = serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
    assert_eq!(marker_json["format"], "mitcad-project");
    assert_eq!(marker_json["edit_locks"]["enabled"], false);
    // This computer's only: no version.
    let answer = run(
        &repo,
        json!({"cmd": "set_project_settings",
               "local": {"live": {"mode": "off"}, "sync": {"check_minutes": 30}}}),
    );
    assert_eq!(answer["commit"], Value::Null);
    assert_eq!(answer["written_local"], true);
    let settings = run(&repo, json!({"cmd": "project_settings"}));
    assert_eq!(settings["local"]["sync"]["check_minutes"], 30);
    assert_eq!(settings["live_updates"], Value::Null, "off here");
    // The same again: nothing.
    let answer = run(
        &repo,
        json!({"cmd": "set_project_settings", "shared": {"edit_locks": {"enabled": false}}}),
    );
    assert_eq!(
        (&answer["commit"], &answer["written_local"]),
        (&Value::Null, &json!(false))
    );
    assert_eq!(repo.history(&marker).unwrap().len(), history.len());
    let text = repo.command_text(r#"{"cmd": "project_settings"}"#).unwrap();
    assert!(
        text.starts_with("Local project\nEdit locks: off\n"),
        "{text}"
    );
    if system_git().is_some() {
        clean(repo.root());
    }
}

#[test]
fn a_shared_folder_is_made_a_bare_repository() {
    let scratch = Scratch::new("bare");
    let dir = scratch.join("shared/robot-arm.git");
    let made = init_bare(&dir).unwrap();
    assert!(made.created);
    let repo = gix::open(&dir).unwrap();
    assert!(repo.is_bare());
    assert_eq!(
        fs::read_to_string(dir.join("HEAD")).unwrap().trim(),
        "ref: refs/heads/main"
    );
    assert!(!init_bare(&dir).unwrap().created, "used as it is");
    let busy = scratch.join("busy");
    fs::create_dir(&busy).unwrap();
    fs::write(busy.join("a.txt"), "a").unwrap();
    assert_eq!(class_of(init_bare(&busy)), ErrorClass::NotEmpty);
    let empty = scratch.join("empty");
    fs::create_dir(&empty).unwrap();
    fs::write(empty.join("desktop.ini"), "x").unwrap();
    assert!(init_bare(&empty).unwrap().created);
    let Some(cli) = system_git() else {
        return;
    };
    // A project's remote.
    let (work, _) = ProjectRepo::init(&scratch.join("work"), Some(&author())).unwrap();
    assert_eq!(class_of(init_bare(work.root())), ErrorClass::NotEmpty);
    let mut log = Vec::new();
    let created = create_project(
        &scratch.join("a"),
        &options(Some(&dir.to_string_lossy())),
        &context(&cli),
        None,
        &mut log,
    )
    .unwrap();
    assert!(created.push.unwrap().pushed);
    git(&dir, &["fsck", "--strict", "--no-progress"]).unwrap();
}

/// `ssh-keyscan`'s output with GitHub's and GitLab's keys and lines that
/// are not read.
fn keyscan_output(host: &str) -> String {
    format!(
        "# {host}:22 SSH-2.0-babeld\n\
         {host} ssh-ed25519 {GITHUB_ED25519}\n\
         {host} ssh-ed25519 {GITHUB_ED25519}\n\
         other.example.com ssh-ed25519 {GITLAB_ED25519}\n\
         {host} ssh-dss AAAAB3NzaC1kc3MAAACBAP\n\
         {host} ssh-rsa not*base64\n\
         {host} ssh-rsa {GITLAB_ED25519}\n\
         {host}\n"
    )
}

#[test]
fn host_keys_are_read_as_untrusted_input() {
    let (keys, dropped) =
        parse_keyscan(keyscan_output("github.com").as_bytes(), "github.com", 22).unwrap();
    assert_eq!(keys.len(), 1, "{keys:?}");
    assert_eq!(keys[0].kind, "ssh-ed25519");
    assert_eq!(keys[0].key, GITHUB_ED25519);
    // The fingerprint GitHub publishes.
    assert_eq!(
        keys[0].fingerprint,
        "SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU"
    );
    assert_eq!(
        dropped, 6,
        "a repeat, another host, a DSA key, bad base64, a type mismatch, a short line"
    );
    let (gitlab, _) = parse_keyscan(
        format!("gitlab.com ssh-ed25519 {GITLAB_ED25519}\n").as_bytes(),
        "gitlab.com",
        22,
    )
    .unwrap();
    assert_eq!(
        gitlab[0].fingerprint,
        "SHA256:eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8"
    );
    // Another port: [host]:port.
    let line = format!("[git.example.com]:2222 ssh-ed25519 {GITHUB_ED25519}\n");
    assert_eq!(
        parse_keyscan(line.as_bytes(), "git.example.com", 2222)
            .unwrap()
            .0
            .len(),
        1
    );
    assert_eq!(
        parse_keyscan(line.as_bytes(), "git.example.com", 22)
            .unwrap()
            .0
            .len(),
        0
    );
    // Too much is refused whole.
    let huge = "x".repeat(70 * 1024);
    assert!(parse_keyscan(huge.as_bytes(), "github.com", 22).is_err());
    // Garbage of every kind is dropped, never a panic.
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let sample = keyscan_output("github.com").into_bytes();
    for _ in 0..2000 {
        let mut bytes = sample.clone();
        for _ in 0..8 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let at = (seed as usize) % bytes.len();
            bytes[at] = (seed >> 32) as u8;
        }
        let cut = (seed as usize >> 8) % (bytes.len() + 1);
        let _ = parse_keyscan(&bytes[..cut], "github.com", 22);
    }
    assert_eq!(fingerprint(b"").len(), "SHA256:".len() + 43);
}

/// A fake `ssh-keyscan` `name` printing `output` (a shell script; None
/// where no shell runs it).
fn fake_keyscan(scratch: &Scratch, name: &str, output: &str) -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    let text = scratch.join(&format!("{name}.txt"));
    fs::write(&text, output).unwrap();
    let script = scratch.join(&format!("{name}-keyscan"));
    fs::write(&script, format!("#!/bin/sh\ncat '{}'\n", text.display())).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    Some(script)
}

#[test]
fn a_hosts_key_is_compared_and_trusted() {
    let scratch = Scratch::new("hostkeys");
    let Some(keyscan) = fake_keyscan(&scratch, "github", &keyscan_output("github.com")) else {
        return;
    };
    let home = scratch.join("home");
    let context = Context {
        keyscan: Some(keyscan),
        home: Some(home.clone()),
        ..Context::default()
    };
    let mut log = Vec::new();
    let keys = host_keys("GitHub.com", 22, &context, &mut log).unwrap();
    assert_eq!(keys.host, "github.com");
    assert_eq!(keys.service.as_deref(), Some("GitHub"));
    assert_eq!(keys.published, Some(Published::Verified));
    assert!(!keys.known && !keys.changed);
    assert!(log[0].starts_with("ssh-keyscan -T 10 -p 22"), "{log:?}");
    let answer = projects_run(
        json!({"cmd": "trust_host_key", "host": "github.com", "type": "ssh-ed25519",
               "key": GITHUB_ED25519}),
        &context,
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["written"], true);
    let known_hosts = home.join(".ssh/known_hosts");
    assert_eq!(
        fs::read_to_string(&known_hosts).unwrap(),
        format!("github.com ssh-ed25519 {GITHUB_ED25519}\n")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&known_hosts).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(home.join(".ssh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    let again = trust_host_key(
        "github.com",
        22,
        "ssh-ed25519",
        GITHUB_ED25519,
        &context,
        &mut log,
    )
    .unwrap();
    assert!(!again.written);
    assert!(
        host_keys("github.com", 22, &context, &mut log)
            .unwrap()
            .known
    );
    // Every key at once (mitcad-cli host-keys --trust).
    let all = projects_run(
        json!({"cmd": "trust_host_key", "host": "github.com"}),
        &context,
    );
    assert_eq!(
        (&all["error"], &all["written"]),
        (&Value::Null, &json!(false)),
        "{all}"
    );
    let other_home = Context {
        home: Some(scratch.join("other-home")),
        ..context.clone()
    };
    let all = projects_run(
        json!({"cmd": "trust_host_key", "host": "github.com"}),
        &other_home,
    );
    assert_eq!(all["written"], true, "{all}");
    // A key the server does not offer is not written.
    let refused = trust_host_key(
        "github.com",
        22,
        "ssh-ed25519",
        GITLAB_ED25519,
        &context,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::HostKeyUnknown);

    // A known_hosts with another key: changed. Hashed names are read.
    let hashed = crate::projects::hashed_host(&[7u8; 20], "github.com");
    fs::write(
        &known_hosts,
        format!("{hashed} ssh-ed25519 {GITLAB_ED25519}\n"),
    )
    .unwrap();
    let keys = host_keys("github.com", 22, &context, &mut log).unwrap();
    assert!(!keys.known && keys.changed);
    fs::write(
        &known_hosts,
        format!("{hashed} ssh-ed25519 {GITHUB_ED25519}\n"),
    )
    .unwrap();
    let keys = host_keys("github.com", 22, &context, &mut log).unwrap();
    assert!(keys.known && !keys.changed);
    let other = crate::projects::hashed_host(&[7u8; 20], "gitlab.com");
    fs::write(
        &known_hosts,
        format!("{other} ssh-ed25519 {GITHUB_ED25519}\n"),
    )
    .unwrap();
    assert!(
        !host_keys("github.com", 22, &context, &mut log)
            .unwrap()
            .known
    );

    // The service's name with a key it does not publish: never trusted.
    let keyscan = fake_keyscan(
        &scratch,
        "gitlab",
        &format!("gitlab.com ssh-ed25519 {GITHUB_ED25519}\n"),
    )
    .unwrap();
    let context = Context {
        keyscan: Some(keyscan),
        home: Some(home.clone()),
        ..Context::default()
    };
    let keys = host_keys("gitlab.com", 22, &context, &mut log).unwrap();
    assert_eq!(keys.published, Some(Published::Mismatch));
    let refused = trust_host_key(
        "gitlab.com",
        22,
        "ssh-ed25519",
        GITHUB_ED25519,
        &context,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::HostKeyUnknown);
    let text = crate::projects::describe("host_keys", &serde_json::to_value(&keys).unwrap());
    assert!(
        text.contains("warning: a key is not one GitLab publishes"),
        "{text}"
    );
    let all = projects_run(
        json!({"cmd": "trust_host_key", "host": "gitlab.com"}),
        &context,
    );
    assert_eq!(error_class(&all), "host_key_unknown", "{all}");
    // Another server: compared by the user.
    let keyscan = fake_keyscan(
        &scratch,
        "other",
        &format!("[git.example.com]:2222 ssh-ed25519 {GITHUB_ED25519}\n"),
    )
    .unwrap();
    let context = Context {
        keyscan: Some(keyscan),
        home: Some(home.clone()),
        ..Context::default()
    };
    let keys = host_keys("git.example.com", 2222, &context, &mut log).unwrap();
    assert_eq!((keys.service.clone(), keys.published), (None, None));
    let trusted = trust_host_key(
        "git.example.com",
        2222,
        "ssh-ed25519",
        GITHUB_ED25519,
        &context,
        &mut log,
    )
    .unwrap();
    assert!(trusted.written);
    assert!(
        fs::read_to_string(&known_hosts)
            .unwrap()
            .ends_with(&format!(
                "\n[git.example.com]:2222 ssh-ed25519 {GITHUB_ED25519}\n"
            ))
    );
    // A server that sends nothing.
    let keyscan = fake_keyscan(&scratch, "silent", "").unwrap();
    let context = Context {
        keyscan: Some(keyscan),
        home: Some(home.clone()),
        ..Context::default()
    };
    assert_eq!(
        class_of(host_keys("nowhere.example.com", 22, &context, &mut log)),
        ErrorClass::Network
    );
    // Not a host name.
    assert!(matches!(
        host_keys("-oProxyCommand=x", 22, &context, &mut log),
        Err(VcsError::Command(_))
    ));
}

#[test]
fn git_tells_its_author_and_credential_helper() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("gitconfig");
    // The user's configuration of the test's own (the system's may set a
    // helper, which the user's empty value resets).
    let cli = cli
        .with_env("HOME", &scratch.0)
        .with_env("XDG_CONFIG_HOME", scratch.join("xdg"))
        .with_env("GIT_CONFIG_NOSYSTEM", "1");
    fs::write(
        scratch.join(".gitconfig"),
        "[user]\n\tname = Ada Lovelace\n\temail = ada@example.invalid\n\
         [credential]\n\thelper = store\n\thelper =\n\thelper = cache --timeout 60\n",
    )
    .unwrap();
    let answer = projects_run(json!({"cmd": "git_info"}), &context(&cli));
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(
        answer["author"],
        json!({"name": "Ada Lovelace", "email": "ada@example.invalid"})
    );
    assert_eq!(
        answer["credential_helper"],
        json!({"configured": true, "name": "cache --timeout 60"})
    );
    let text = crate::projects::describe("git_info", &answer);
    assert!(
        text.contains("Author: Ada Lovelace <ada@example.invalid>\n"),
        "{text}"
    );
    assert!(
        text.contains("Credential helper: cache --timeout 60\n"),
        "{text}"
    );
}

#[test]
fn the_users_public_key_is_found() {
    let scratch = Scratch::new("publickey");
    let context = Context {
        home: Some(scratch.0.clone()),
        ..Context::default()
    };
    assert_eq!(ssh_public_key(&context), None);
    let ssh = scratch.join(".ssh");
    fs::create_dir(&ssh).unwrap();
    fs::write(ssh.join("id_rsa.pub"), "not a key\n").unwrap();
    assert_eq!(ssh_public_key(&context), None);
    fs::write(
        ssh.join("id_ecdsa.pub"),
        format!("ssh-ed25519 {GITHUB_ED25519} me@example\n"),
    )
    .unwrap();
    let key = ssh_public_key(&context).unwrap();
    assert!(key.path.ends_with("id_ecdsa.pub"));
    assert_eq!(key.text, format!("ssh-ed25519 {GITHUB_ED25519} me@example"));
    let answer = projects_run(json!({"cmd": "ssh_public_key"}), &context);
    assert_eq!(answer["text"], key.text.as_str());
}
