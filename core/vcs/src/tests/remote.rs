// SPDX-License-Identifier: MIT
//! Remote repositories (P12 remote) against repositories in temporary
//! folders: a bare repository (`git init --bare`) as the remote, projects
//! connected to it and opened from it, and an SSH path through a fake `ssh`
//! (a script that runs git's command on this computer). Nothing goes over
//! the network. Without the system's git these tests are skipped.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::*;
use crate::remote::cli::{locate, progress_of};
use crate::remote::{
    CONFIGURE_MESSAGE, Control, ErrorClass, GitCli, GitVersion, check_url, classify, clone_project,
    git_info, redact,
};

pub(super) const AUTHOR: &str = "Mitcad Test <test@example.invalid>";

/// The system's git for the remote work, or None (the test is skipped).
pub(super) fn system_git() -> Option<GitCli> {
    if !has_git() {
        return None;
    }
    // A proxy of the environment must not catch connections to this
    // computer.
    Some(
        GitCli::find()
            .ok()?
            .with_env("NO_PROXY", "127.0.0.1,localhost")
            .with_env("no_proxy", "127.0.0.1,localhost"),
    )
}

/// A bare repository `name` in the scratch folder: an empty remote.
pub(super) fn bare(scratch: &Scratch, name: &str) -> PathBuf {
    git(&scratch.0, &["init", "-q", "--bare", name]).unwrap();
    scratch.join(name)
}

/// A path with forward slashes, for the shell of git (also on Windows).
pub(super) fn slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs a JSON command, which must not fail as a command.
pub(super) fn run(repo: &ProjectRepo, command: Value) -> Value {
    serde_json::from_str(&repo.command(&command.to_string()).unwrap()).unwrap()
}

/// The class of a remote failure.
fn class_of<T: std::fmt::Debug>(result: Result<T, VcsError>) -> ErrorClass {
    match result {
        Err(VcsError::Remote(error)) => error.class,
        other => panic!("not a remote failure: {other:?}"),
    }
}

/// The class of a JSON answer's error.
pub(super) fn error_class(answer: &Value) -> &str {
    answer["error"]["class"].as_str().unwrap_or("none")
}

/// A project with its design recorded as a first version, connected to the
/// empty remote `url` and pushed.
pub(super) fn connected(scratch: &Scratch, cli: &GitCli, url: &str) -> (ProjectRepo, PathBuf) {
    let (repo, _, part) = project(scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    repo.set_git(cli.clone());
    let answer = run(
        &repo,
        json!({"cmd": "connect", "url": url, "author": AUTHOR}),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["push"]["pushed"], true, "{answer}");
    (repo, part)
}

/// Changes `d3` of the project file `part` and records it.
pub(super) fn new_version(repo: &ProjectRepo, part: &Path, d3: f64) -> String {
    let mut doc = Document::load_project(part, MockKernel::default()).unwrap();
    set_parameter(&mut doc, "d3", d3);
    doc.save_file(part, FileFormat::Auto).unwrap();
    repo.commit(&[part.to_path_buf()], &format!("d3 = {d3} mm"), &author())
        .unwrap()
        .commit
        .unwrap()
}

/// The commit of a branch of a repository, by the system's git.
pub(super) fn tip(repository: &Path, reference: &str) -> String {
    git(repository, &["rev-parse", reference])
        .unwrap()
        .trim()
        .to_owned()
}

/// A fake `ssh` for git (`GIT_SSH_COMMAND`): runs the command git gives it
/// (`git-upload-pack 'remote.git'`) in the scratch folder, or fails as a
/// server would (`denied`, `hostkey`) or hangs (`hang`); `notty` fails as
/// a refused sign-in when it has a terminal to ask on.
pub(super) fn fake_ssh(scratch: &Scratch, cli: &GitCli, mode: &str) -> GitCli {
    let script = scratch.join("fake-ssh.sh");
    fs::write(
        &script,
        "for last; do :; done\n\
         case \"$FAKE_SSH_MODE\" in\n\
         denied) echo 'git@test: Permission denied (publickey).' >&2; exit 255 ;;\n\
         hostkey) echo 'Host key verification failed.' >&2; exit 255 ;;\n\
         hang) sleep 10; exit 255 ;;\n\
         notty) if (: < /dev/tty) 2> /dev/null; then\n\
         echo 'git@test: Permission denied (a terminal to ask on).' >&2; exit 255; fi ;;\n\
         esac\n\
         cd \"$FAKE_SSH_ROOT\" || exit 1\n\
         command=$(printf '%s' \"$last\" | sed \"s#'/#'#\")\n\
         eval \"git ${command#git-}\"\n",
    )
    .unwrap();
    cli.clone()
        .with_env("GIT_SSH_COMMAND", format!("sh \"{}\"", slashes(&script)))
        .with_env("GIT_SSH_VARIANT", "ssh")
        .with_env("FAKE_SSH_ROOT", slashes(&scratch.0))
        .with_env("FAKE_SSH_MODE", mode)
}

#[test]
fn urls_with_credentials_or_programs_are_refused() {
    for bad in [
        "",
        "https://user:s3cret@example.com/team/part.git",
        "https://ghp_abcdefghijklmnopqrstuvwxyz0123456789@github.com/team/part.git",
        "http://user%3As3cret@example.com/part.git",
        "ssh://git:s3cret@example.com/part.git",
        "ext::sh -c touch% marker",
        "fd::3",
        "git://example.com/part.git",
        "ftp://example.com/part.git",
        "--upload-pack=touch marker",
        "https://example.com/part.git\nother",
    ] {
        match check_url(bad) {
            Err(error) => {
                assert_eq!(error.class, ErrorClass::InvalidUrl, "{bad}");
                assert!(!error.message.contains("s3cret"), "{}", error.message);
            }
            Ok(url) => panic!("{bad} was taken as {url}"),
        }
    }
    for good in [
        "https://github.com/team/part.git",
        "https://alice@example.com/team/part.git",
        "git@github.com:team/part.git",
        "ssh://git@example.com:2222/team/part.git",
        "file:///srv/git/part.git",
    ] {
        assert_eq!(check_url(good).unwrap(), good);
    }
    // A folder is made absolute (on Windows with the current drive).
    for folder in ["/srv/git/part.git", "remote.git"] {
        let absolute = check_url(folder).unwrap();
        assert!(Path::new(&absolute).is_absolute(), "{absolute}");
        assert!(absolute.ends_with("part.git") || absolute.ends_with("remote.git"));
    }
    if cfg!(windows) {
        assert_eq!(
            check_url("C:\\srv\\git\\part.git").unwrap(),
            "C:\\srv\\git\\part.git"
        );
    } else {
        assert_eq!(check_url("/srv/git/part.git").unwrap(), "/srv/git/part.git");
    }
}

#[test]
fn credentials_are_hidden_in_what_is_shown() {
    assert_eq!(
        redact("fatal: unable to access 'https://user:s3cret@example.com/a.git/': 403"),
        "fatal: unable to access 'https://***@example.com/a.git/': 403"
    );
    assert_eq!(
        redact("https://ghp_abcdefghijklmnopqrstuvwxyz0123@github.com/a/b"),
        "https://***@github.com/a/b"
    );
    assert_eq!(
        redact("ssh://git@example.com/a and ssh://git:pw@example.com/b"),
        "ssh://git@example.com/a and ssh://git:***@example.com/b"
    );
    assert_eq!(
        redact("token github_pat_11ABCDEFG0123456789_abcdef, glpat-abcdefghijk1234 and ghp_x"),
        "token github_pat_***, glpat-*** and ghp_x"
    );
    assert_eq!(
        redact("no secrets here: git@host:a/b.git"),
        "no secrets here: git@host:a/b.git"
    );
}

#[test]
fn failures_are_classified_from_gits_messages() {
    let cases = [
        (
            "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.",
            ErrorClass::AuthFailed,
        ),
        (
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
            ErrorClass::AuthFailed,
        ),
        (
            "fatal: unable to access 'https://x/': The requested URL returned error: 403",
            ErrorClass::AuthFailed,
        ),
        (
            "Host key verification failed.\nfatal: Could not read from remote repository.",
            ErrorClass::HostKeyUnknown,
        ),
        (
            "ssh: Could not resolve hostname nowhere: Name or service not known",
            ErrorClass::Network,
        ),
        (
            "fatal: unable to access 'https://x/': Failed to connect to x port 443: Connection refused",
            ErrorClass::Network,
        ),
        (
            "ERROR: Repository not found.\nfatal: Could not read from remote repository.",
            ErrorClass::NotFound,
        ),
        (
            "fatal: '/srv/none.git' does not appear to be a git repository",
            ErrorClass::NotFound,
        ),
        (
            "! [rejected]        main -> main (fetch first)\nerror: failed to push some refs",
            ErrorClass::Rejected,
        ),
        (
            "remote: error: GH001: Large files detected.\n ! [remote rejected] main -> main (pre-receive hook declined)",
            ErrorClass::TooLarge,
        ),
        (
            "git-lfs filter-process: git-lfs: command not found",
            ErrorClass::LfsMissing,
        ),
        ("fatal: transport 'git' not allowed", ErrorClass::InvalidUrl),
        ("fatal: something else", ErrorClass::Other),
    ];
    for (stderr, class) in cases {
        assert_eq!(classify(stderr), class, "{stderr}");
    }
    let error = crate::remote::RemoteError::from_git(
        "Fetching from origin",
        "fatal: unable to access 'https://u:s3cret@x/a.git/': The requested URL returned error: 401\n",
    );
    assert!(
        error
            .message
            .starts_with("Fetching from origin failed: the server refused"),
        "{}",
        error.message
    );
    assert!(error.message.contains("ssh-add"), "{}", error.message);
    assert!(!error.detail.contains("s3cret") && error.detail.contains("https://***@x/a.git/"));
    let other = crate::remote::RemoteError::from_git("Pushing", "hint: x\nfatal: odd failure\n");
    assert_eq!(other.message, "Pushing failed: odd failure");
}

#[test]
fn git_is_found_with_its_version_and_progress() {
    let version = GitVersion::parse("git version 2.45.1.windows.1\n").unwrap();
    assert_eq!(
        (version.text.as_str(), version.major, version.minor),
        ("2.45.1.windows.1", 2, 45)
    );
    assert!(version.supported());
    assert!(!GitVersion::parse("git version 2.30.2").unwrap().supported());
    assert_eq!(GitVersion::parse("hello"), None);
    for (line, expected) in [
        (
            "Receiving objects:  45% (9/20)",
            Some(("Receiving objects", Some(45))),
        ),
        (
            "remote: Counting objects: 100% (5/5), done.",
            Some(("Counting objects", Some(100))),
        ),
        (
            "remote: Enumerating objects: 5, done.",
            Some(("Enumerating objects", None)),
        ),
        (
            "Writing objects: 100% (3/3), 250 bytes | 250.00 KiB/s, done.",
            Some(("Writing objects", Some(100))),
        ),
        ("fatal: could not read from remote repository", None),
        (
            "warning: You appear to have cloned an empty repository.",
            None,
        ),
        ("Cloning into 'b'...", None),
    ] {
        let parsed = progress_of(line);
        assert_eq!(
            parsed
                .as_ref()
                .map(|(text, percent)| (text.as_str(), *percent)),
            expected,
            "{line}"
        );
    }
    // A program set for Mitcad that is not there.
    let missing = locate(Some("/no/such/folder/git".into()), None).unwrap_err();
    assert_eq!(missing.class, ErrorClass::GitMissing);
    if !cfg!(windows) {
        // Relative folders on PATH (the project's own) are not searched.
        let path = std::env::join_paths([Path::new("."), Path::new("relative")]).unwrap();
        assert!(locate(None, Some(path)).is_err());
    }
    let Some(cli) = system_git() else {
        return;
    };
    let info = git_info(Some(&cli));
    assert!(info.error.is_none() && info.supported, "{info:?}");
    assert!(info.version.is_some() && info.path.is_some());
    let text = api::git_info(true);
    assert!(text.starts_with("git "), "{text}");
}

/// Connecting to an empty remote pushes the history (10.1, 1): the remote
/// set, the branch following it, `.gitattributes` with `merge=binary`
/// recorded, nothing ahead or behind; later versions are pushed only when
/// the remote lacks them.
#[test]
fn a_project_connects_to_an_empty_remote() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("connect");
    let remote = bare(&scratch, "remote.git");
    let url = remote.to_string_lossy().into_owned();
    let (repo, _, part) = project(&scratch);
    // A project made before project files were merged as whole files.
    let attributes = repo.root().join(".gitattributes");
    fs::write(
        &attributes,
        "*.mitcad text eol=lf\n.mitcad/brep/** binary\n",
    )
    .unwrap();
    repo.commit(&[part.clone(), attributes.clone()], "First", &author())
        .unwrap();
    repo.set_git(cli.clone());

    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(info["name"], Value::Null, "{info}");
    assert_eq!(
        repo.command_text(r#"{"cmd": "remote_info"}"#).unwrap(),
        "No remote repository: connect one with mitcad-cli remote add <folder> <url>\n"
    );
    let check = run(&repo, json!({"cmd": "remote_check", "url": url}));
    assert_eq!(check["error"], Value::Null, "{check}");
    assert_eq!(
        (&check["reachable"], &check["empty"]),
        (&json!(true), &json!(true))
    );
    assert_eq!(check["related"], Value::Null);

    let answer = run(
        &repo,
        json!({"cmd": "connect", "url": url, "author": AUTHOR}),
    );
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["set"]["name"], "origin");
    assert_eq!(answer["set"]["upstream"], "origin/main");
    assert_eq!(answer["set"]["written"], json!([".gitattributes"]));
    assert!(answer["set"]["commit"].is_string(), "{answer}");
    assert_eq!(answer["push"]["pushed"], true);
    assert_eq!(
        (&answer["ahead"], &answer["behind"]),
        (&json!(0), &json!(0))
    );
    let log: Vec<&str> = answer["log"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        log.iter()
            .any(|line| line.starts_with("git push") && line.ends_with(": ok")),
        "{log:?}"
    );
    // Project files are merged as whole files, and that is a version.
    assert_eq!(
        fs::read_to_string(&attributes).unwrap(),
        "*.mitcad text eol=lf merge=binary\n.mitcad/brep/** binary\n"
    );
    assert_eq!(
        repo.history(&attributes).unwrap()[0].summary,
        CONFIGURE_MESSAGE
    );
    // The remote has the history.
    let head = repo.head_commit().unwrap().unwrap().to_string();
    assert_eq!(tip(&remote, "refs/heads/main"), head);
    let info = run(&repo, json!({"cmd": "remote_info", "links": true}));
    assert_eq!(info["url"], url.as_str());
    assert_eq!(info["upstream"], "origin/main");
    assert_eq!((&info["ahead"], &info["behind"]), (&json!(0), &json!(0)));
    assert_eq!(info["last_push"]["error"], Value::Null, "{info}");
    assert_eq!(info["external_links"], json!([]));
    let text = repo.command_text(r#"{"cmd": "remote_info"}"#).unwrap();
    assert!(
        text.contains("Branch main follows origin/main: up to date\n"),
        "{text}"
    );

    // A new version: one to push, then none.
    new_version(&repo, &part, 25.0);
    let info = repo.remote_info(false).unwrap();
    assert_eq!((info.ahead, info.behind), (Some(1), Some(0)));
    let pushed = run(&repo, json!({"cmd": "push"}));
    assert_eq!(pushed["error"], Value::Null, "{pushed}");
    assert_eq!(
        (&pushed["pushed"], &pushed["versions"]),
        (&json!(true), &json!(1))
    );
    assert_eq!(pushed["rejected"], false);
    assert_eq!(
        tip(&remote, "refs/heads/main"),
        repo.resolve("HEAD").unwrap()
    );
    assert_eq!(
        repo.command_text(r#"{"cmd": "push"}"#).unwrap(),
        "Nothing to push: origin/main has every version\n"
    );
    // The repositories as git sees them.
    assert_eq!(git(repo.root(), &["status", "--porcelain"]).unwrap(), "");
    git(&remote, &["fsck", "--strict", "--no-progress"]).unwrap();

    // Removed: no remote, no record of fetches and pushes.
    let removed = run(&repo, json!({"cmd": "remote_remove"}));
    assert_eq!(
        (&removed["name"], &removed["removed"]),
        (&json!("origin"), &json!(true))
    );
    assert!(!repo.root().join(".mitcad/local/remote.json").exists());
    assert_eq!(repo.remote_info(false).unwrap().name, None);
    assert_eq!(
        repo.command_text(r#"{"cmd": "remote_remove"}"#).unwrap(),
        "No remote origin\n"
    );
}

/// A remote that shares the project's history is connected and fetched
/// (its newer versions read first); another history is refused without
/// changing anything (10.1, 2).
#[test]
fn a_remote_with_another_history_is_refused() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("related");
    let remote = bare(&scratch, "remote.git");
    let url = remote.to_string_lossy().into_owned();
    let (a, part) = connected(&scratch, &cli, &url);

    // B has A's history without a remote, and A has a version more. (The
    // bare repository's HEAD may name another branch, as git's default.)
    git(&scratch.0, &["clone", "-q", "--branch", "main", &url, "b"]).unwrap();
    let b_root = scratch.join("b");
    git(&b_root, &["remote", "remove", "origin"]).unwrap();
    new_version(&a, &part, 30.0);
    run(&a, json!({"cmd": "push"}));
    let b = ProjectRepo::open(&b_root).unwrap();
    b.set_git(cli.clone());
    let check = b.remote_check(&url, None).unwrap();
    assert!(check.reachable && !check.empty);
    assert_eq!(check.default_branch.as_deref(), Some("main"));
    assert_eq!(
        check.head.as_deref(),
        Some(a.resolve("HEAD").unwrap().as_str())
    );
    assert_eq!((check.has_project, check.related), (Some(true), Some(true)));
    // Connected: fetched, one version behind, nothing pushed.
    let answer = run(&b, json!({"cmd": "connect", "url": url, "author": AUTHOR}));
    assert_eq!(answer["error"], Value::Null, "{answer}");
    assert_eq!(answer["push"], Value::Null);
    assert_eq!(
        answer["fetch"]["updated"].as_array().unwrap().len(),
        1,
        "{answer}"
    );
    assert_eq!(
        (&answer["ahead"], &answer["behind"]),
        (&json!(0), &json!(1))
    );
    let text = b.command_text(r#"{"cmd": "remote_info"}"#).unwrap();
    assert!(
        text.contains("follows origin/main: 1 version newer on the remote"),
        "{text}"
    );

    // C has a history of its own.
    let (c, _) = ProjectRepo::init(&scratch.join("c"), Some(&author())).unwrap();
    c.set_git(cli.clone());
    let config = fs::read_to_string(c.repo.git_dir().join("config")).unwrap();
    let head = c.head_commit().unwrap();
    let refused = run(&c, json!({"cmd": "connect", "url": url, "author": AUTHOR}));
    assert_eq!(error_class(&refused), "unrelated", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("another project's history")
    );
    assert_eq!(refused["set"], Value::Null);
    assert_eq!(
        fs::read_to_string(c.repo.git_dir().join("config")).unwrap(),
        config
    );
    assert_eq!(c.head_commit().unwrap(), head);
    assert_eq!(c.remote_info(false).unwrap().name, None);
    let error = c
        .command_text(&json!({"cmd": "connect", "url": url, "author": AUTHOR}).to_string())
        .unwrap_err();
    assert!(
        error.to_string().contains("another project's history"),
        "{error}"
    );

    // A repository that holds no project.
    let plain = scratch.join("plain");
    git(&scratch.0, &["init", "-q", "plain"]).unwrap();
    fs::write(plain.join("readme.txt"), "not a project\n").unwrap();
    git(&plain, &["add", "readme.txt"]).unwrap();
    git(&plain, &["commit", "-q", "-m", "Readme"]).unwrap();
    let check = c.remote_check(&plain.to_string_lossy(), None).unwrap();
    assert_eq!(
        (check.has_project, check.related),
        (Some(false), Some(false))
    );
    let refused = c.connect(&plain.to_string_lossy(), "origin", &author(), true, None);
    assert_eq!(class_of(refused), ErrorClass::Unrelated);
}

/// Versions go out with a push only when the remote lacks them, and come
/// in with a fast-forward that updates the folder (10.1, 3); when both
/// sides have new versions the remote refuses and nothing is forced.
#[test]
fn versions_go_out_with_a_push_and_come_in_with_a_fast_forward() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("fastforward");
    let remote = bare(&scratch, "remote.git");
    let url = remote.to_string_lossy().into_owned();
    let (a, part_a) = connected(&scratch, &cli, &url);

    // B opened from the remote.
    let mut log = Vec::new();
    let opened = clone_project(&url, &scratch.join("b"), Some(&cli), None, &mut log).unwrap();
    assert_eq!(opened.files, ["part.mitcad"]);
    assert_eq!(opened.branch.as_deref(), Some("main"));
    assert_eq!(
        opened.head.as_deref(),
        Some(a.resolve("HEAD").unwrap().as_str())
    );
    assert!(
        log.iter().any(|line| line.starts_with("git clone")),
        "{log:?}"
    );
    let b = ProjectRepo::open(&scratch.join("b")).unwrap();
    b.set_git(cli.clone());
    let part_b = b.root().join("part.mitcad");
    assert_eq!(fs::read(&part_b).unwrap(), fs::read(&part_a).unwrap());
    let info = b.remote_info(false).unwrap();
    assert_eq!(info.upstream.as_deref(), Some("origin/main"));
    assert_eq!((info.ahead, info.behind), (Some(0), Some(0)));

    // B records a version and pushes it.
    let from_b = new_version(&b, &part_b, 25.0);
    let pushed = b.push(None).unwrap();
    assert!(pushed.pushed);
    assert_eq!(pushed.versions, Some(1));
    // A knows nothing of it until it fetches.
    assert_eq!(a.remote_info(false).unwrap().behind, Some(0));
    let fetched = run(&a, json!({"cmd": "fetch"}));
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    assert_eq!(
        fetched["updated"],
        json!([{"name": "refs/remotes/origin/main", "old": a.resolve("HEAD").unwrap(),
                "new": from_b}])
    );
    assert_eq!(
        (&fetched["ahead"], &fetched["behind"]),
        (&json!(0), &json!(1))
    );
    let text = a.command_text(r#"{"cmd": "fetch"}"#).unwrap();
    assert_eq!(
        text,
        "Fetched from origin: nothing new; 1 version newer on the remote\n"
    );
    assert!(
        a.remote_info(false)
            .unwrap()
            .last_fetch
            .unwrap()
            .error
            .is_none()
    );
    // The fast-forward brings B's version, the file with it.
    let forward = a.fast_forward(None).unwrap().unwrap();
    assert_eq!(forward.to, from_b);
    assert_eq!(forward.changed_paths, ["part.mitcad"]);
    assert_eq!(fs::read(&part_a).unwrap(), fs::read(&part_b).unwrap());
    let history = a.history(&part_a).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, from_b);
    assert_eq!(git(a.root(), &["status", "--porcelain"]).unwrap(), "");
    assert_eq!(a.fast_forward(None).unwrap(), None);

    // Both record a version: A pushes first, B's push is refused.
    new_version(&a, &part_a, 30.0);
    assert!(a.push(None).unwrap().pushed);
    let remote_head = tip(&remote, "refs/heads/main");
    new_version(&b, &part_b, 35.0);
    let refused = run(&b, json!({"cmd": "push"}));
    assert_eq!(error_class(&refused), "rejected", "{refused}");
    assert_eq!(
        (&refused["pushed"], &refused["rejected"]),
        (&json!(false), &json!(true))
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Sync first")
    );
    assert_eq!(tip(&remote, "refs/heads/main"), remote_head);
    let info = b.remote_info(false).unwrap();
    assert_eq!(
        info.last_push.unwrap().error.unwrap().class,
        ErrorClass::Rejected
    );
    b.fetch(None).unwrap();
    let info = b.remote_info(false).unwrap();
    assert_eq!((info.ahead, info.behind), (Some(1), Some(1)));
    assert_eq!(class_of(b.fast_forward(None)), ErrorClass::Unsupported);
    git(&remote, &["fsck", "--strict", "--no-progress"]).unwrap();
}

/// The SSH path without a server: git runs a fake `ssh` that runs its
/// command here; the server's refusals are told apart (10.1, 13).
#[test]
fn the_ssh_path_and_its_failures() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("ssh");
    bare(&scratch, "remote.git");
    let ssh = fake_ssh(&scratch, &cli, "");
    let url = "ssh://test/remote.git";
    let (a, part) = connected(&scratch, &ssh, url);
    let control = Control::new();
    let mut log = Vec::new();
    let opened = clone_project(
        url,
        &scratch.join("b"),
        Some(&ssh),
        Some(&control),
        &mut log,
    )
    .unwrap();
    assert_eq!(opened.files, ["part.mitcad"]);
    assert_eq!(opened.url, url);
    assert!(!control.progress().text.is_empty(), "{log:?}");
    let b = ProjectRepo::open(&scratch.join("b")).unwrap();
    b.set_git(ssh.clone());
    new_version(&a, &part, 25.0);
    assert!(a.push(None).unwrap().pushed);
    assert_eq!(b.fetch(None).unwrap().behind, Some(1));

    // The server refuses the key.
    a.set_git(fake_ssh(&scratch, &cli, "denied"));
    let error = match a.fetch(None) {
        Err(VcsError::Remote(error)) => error,
        other => panic!("{other:?}"),
    };
    assert_eq!(error.class, ErrorClass::AuthFailed);
    assert!(error.message.contains("ssh-add"), "{}", error.message);
    assert!(error.detail.contains("Permission denied (publickey)"));
    let info = run(&a, json!({"cmd": "remote_info"}));
    assert_eq!(
        info["last_fetch"]["error"]["class"], "auth_failed",
        "{info}"
    );
    let check = run(&a, json!({"cmd": "remote_check", "url": url}));
    assert_eq!(
        (error_class(&check), &check["reachable"]),
        ("auth_failed", &json!(false))
    );
    // The host key is not known.
    a.set_git(fake_ssh(&scratch, &cli, "hostkey"));
    let check = run(&a, json!({"cmd": "remote_check", "url": url}));
    assert_eq!(error_class(&check), "host_key_unknown", "{check}");
    assert!(
        check["error"]["message"]
            .as_str()
            .unwrap()
            .contains("known_hosts")
    );
    let refused = clone_project(
        url,
        &scratch.join("c"),
        Some(&fake_ssh(&scratch, &cli, "denied")),
        None,
        &mut log,
    );
    assert_eq!(class_of(refused), ErrorClass::AuthFailed);
    assert!(!scratch.join("c").exists());
}

/// git runs in a session of its own, without the terminal the program was
/// started from: the ssh it starts cannot wait there for a passphrase or a
/// host key that nobody is asked for (where the tests have no terminal
/// either, this passes anyway).
#[cfg(unix)]
#[test]
fn git_runs_without_the_terminal() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("tty");
    bare(&scratch, "remote.git");
    let ssh = fake_ssh(&scratch, &cli, "notty");
    let (a, part) = connected(&scratch, &ssh, "ssh://test/remote.git");
    new_version(&a, &part, 25.0);
    assert!(a.push(None).unwrap().pushed);
    assert_eq!(a.fetch(None).unwrap().behind, Some(0));
}

/// git that is cancelled, or that shows no sign of life, is stopped and
/// nothing changes.
#[test]
fn a_cancelled_or_silent_git_is_stopped() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("cancel");
    bare(&scratch, "remote.git");
    let url = "ssh://test/remote.git";
    let (a, part) = connected(&scratch, &fake_ssh(&scratch, &cli, ""), url);
    let tracking = || {
        a.fresh()
            .unwrap()
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .id()
            .detach()
    };
    let before = tracking();
    let hang = fake_ssh(&scratch, &cli, "hang");
    a.set_git(hang.clone());
    let control = Control::new();
    let start = Instant::now();
    let result = thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(300));
            control.cancel();
        });
        a.fetch(Some(&control))
    });
    assert_eq!(class_of(result), ErrorClass::Cancelled);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    // A control that is cancelled already runs nothing.
    new_version(&a, &part, 25.0);
    assert_eq!(class_of(a.push(Some(&control))), ErrorClass::Cancelled);
    assert_eq!(a.remote_info(false).unwrap().ahead, Some(1));
    let answer: Value =
        serde_json::from_str(&a.command_with(r#"{"cmd": "fetch"}"#, &control).unwrap()).unwrap();
    assert_eq!(error_class(&answer), "cancelled");
    // Silence for longer than the limit.
    a.set_git(hang.with_idle_timeout(Duration::from_millis(500)));
    let start = Instant::now();
    assert_eq!(class_of(a.fetch(None)), ErrorClass::TimedOut);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(tracking(), before);
}

/// Only Mitcad projects are opened from a remote, into a new or empty
/// folder; a failure leaves no folder behind.
#[test]
fn only_projects_are_opened_from_a_remote() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("clone");
    let mut log = Vec::new();
    let empty = bare(&scratch, "empty.git");
    let error = clone_project(
        &empty.to_string_lossy(),
        &scratch.join("x"),
        Some(&cli),
        None,
        &mut log,
    );
    assert_eq!(class_of(error), ErrorClass::NotAProject);
    assert!(!scratch.join("x").exists());
    // A repository of other files.
    let plain = scratch.join("plain");
    git_run(&scratch.0, &["init", "-q", "plain"]);
    fs::write(plain.join("readme.txt"), "not a project\n").unwrap();
    git_run(&plain, &["add", "readme.txt"]);
    git_run(&plain, &["commit", "-q", "-m", "Readme"]);
    let url = plain.to_string_lossy().into_owned();
    let error = clone_project(&url, &scratch.join("x"), Some(&cli), None, &mut log);
    assert_eq!(class_of(error), ErrorClass::NotAProject);
    assert!(!scratch.join("x").exists());
    // An empty folder that was there stays, empty.
    fs::create_dir(scratch.join("kept")).unwrap();
    let error = clone_project(&url, &scratch.join("kept"), Some(&cli), None, &mut log);
    assert_eq!(class_of(error), ErrorClass::NotAProject);
    assert_eq!(fs::read_dir(scratch.join("kept")).unwrap().count(), 0);
    // A folder with files is not used.
    fs::write(scratch.join("kept/mine.txt"), "mine\n").unwrap();
    let error = clone_project(&url, &scratch.join("kept"), Some(&cli), None, &mut log);
    assert!(matches!(error, Err(VcsError::InvalidPath(_))), "{error:?}");
    assert!(scratch.join("kept/mine.txt").is_file());
    // As JSON: the failure is in the answer.
    let answer: Value = serde_json::from_str(
        &api::clone_project(
            "https://u:s3cret@example.com/a.git",
            &scratch.join("y"),
            None,
            false,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(error_class(&answer), "invalid_url");
    assert!(!answer.to_string().contains("s3cret"), "{answer}");
    assert!(!scratch.join("y").exists());
}

fn git_run(root: &Path, args: &[&str]) {
    git(root, args).unwrap();
}

/// URLs with credentials are refused before git runs, and those set
/// outside Mitcad are hidden in everything shown (10.1, 14).
#[test]
fn credentials_never_reach_what_is_shown() {
    let Some(cli) = system_git() else {
        return;
    };
    let scratch = Scratch::new("secret");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    repo.set_git(cli.clone());
    let config = fs::read_to_string(repo.repo.git_dir().join("config")).unwrap();
    let refused = run(
        &repo,
        json!({"cmd": "remote_set", "url": "https://user:s3cret@127.0.0.1:1/x.git", "author": AUTHOR}),
    );
    assert_eq!(error_class(&refused), "invalid_url");
    assert!(!refused.to_string().contains("s3cret"), "{refused}");
    assert_eq!(
        fs::read_to_string(repo.repo.git_dir().join("config")).unwrap(),
        config
    );
    // ext:: would run a program: it never reaches git.
    let marker = scratch.join("marker");
    let check = run(
        &repo,
        json!({"cmd": "remote_check", "url": format!("ext::sh -c touch% {}", slashes(&marker))}),
    );
    assert_eq!(error_class(&check), "invalid_url");
    assert!(!marker.exists());
    assert!(check["log"].as_array().unwrap().is_empty());

    // A URL with credentials set with git.
    git(
        repo.root(),
        &[
            "remote",
            "add",
            "origin",
            "http://user:s3cret@127.0.0.1:1/x.git",
        ],
    )
    .unwrap();
    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(info["url"], "http://***@127.0.0.1:1/x.git");
    let fetched = repo.command(r#"{"cmd": "fetch"}"#).unwrap();
    let answer: Value = serde_json::from_str(&fetched).unwrap();
    assert_eq!(error_class(&answer), "network", "{answer}");
    assert!(!fetched.contains("s3cret"), "{fetched}");
    let state = fs::read_to_string(repo.root().join(".mitcad/local/remote.json")).unwrap();
    assert!(!state.contains("s3cret"), "{state}");
    let error = repo
        .command_text(r#"{"cmd": "fetch"}"#)
        .unwrap_err()
        .to_string();
    assert!(
        error.starts_with("Fetching from origin failed: the server cannot be reached"),
        "{error}"
    );
}
