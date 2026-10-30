// SPDX-License-Identifier: MIT
//! A repository's own git configuration as untrusted input (mitcad#920):
//! a project folder that came with a `.git/config` naming programs
//! (`remote.origin.uploadpack`, `core.sshCommand`, `credential.helper`)
//! gets no git run at all, the refusal named in `remote_info`; hooks never
//! run; a configured remote's address is checked like a typed one; and a
//! project MitCad connected itself fetches and pushes as before. The
//! programs are scripts that leave a marker file; plain git, run as it is,
//! shows that they would have run. Without the system's git the tests with
//! repositories are skipped.

use std::process::Stdio;

use serde_json::{Value, json};

use super::remote::{AUTHOR, bare, connected, error_class, new_version, run, slashes, system_git};
use super::*;
use crate::remote::trust::{
    RECEIVE_PACK, RepositoryConfig, UPLOAD_PACK, check_run, folder_repository, is_folder,
    repository_of, untrusted,
};
use crate::remote::{ErrorClass, GitCli};

const SESSION: &str = "aaaaaaaa-0000-4000-8000-000000000001";

/// A script that writes `marker` when it is started, then fails; the
/// command that starts it (for git's shell, also on Windows).
fn marker_program(scratch: &Scratch, marker: &Path) -> String {
    let script = scratch.join("program.sh");
    fs::write(
        &script,
        format!("echo ran > \"{}\"\nexit 1\n", slashes(marker)),
    )
    .unwrap();
    format!("sh \"{}\"", slashes(&script))
}

/// A hook at `path` that writes `marker` and refuses.
fn marker_hook(path: &Path, marker: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!("#!/bin/sh\necho ran > \"{}\"\nexit 1\n", slashes(marker)),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// git run as it is in `root`, as a user would (its outcome does not
/// matter, only what it started).
fn plain_git(root: &Path, args: &[&str]) {
    let _ = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_SSH_COMMAND")
        .env_remove("GIT_SSH")
        .env_remove("GIT_ASKPASS")
        .stdin(Stdio::null())
        .output();
}

/// A project with its first version and the remote `origin` at `url` (set
/// by MitCad, nothing sent).
fn project_with_remote(scratch: &Scratch, cli: &GitCli, url: &str) -> (ProjectRepo, PathBuf) {
    let (repo, _, part) = project(scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    repo.set_git(cli.clone());
    let set = run(
        &repo,
        json!({"cmd": "remote_set", "url": url, "author": AUTHOR}),
    );
    assert_eq!(set["error"], Value::Null, "{set}");
    (repo, part)
}

/// Takes the design's edit lock, as opening it does.
fn lock_take(repo: &ProjectRepo, part: &Path) -> Value {
    run(
        repo,
        json!({"cmd": "lock_take", "path": part.to_string_lossy(), "session": SESSION,
               "author": AUTHOR, "idle_minutes": 1, "poll_seconds": 5}),
    )
}

/// The class of a remote failure.
fn class_of<T: std::fmt::Debug>(result: Result<T, VcsError>) -> ErrorClass {
    match result {
        Err(VcsError::Remote(error)) => error.class,
        other => panic!("not a remote failure: {other:?}"),
    }
}

/// A received project whose remote is a folder and whose `.git/config`
/// names an upload-pack program: the lock take and the fetch on open, a
/// push and a sync start nothing, and `remote_info` says why.
#[test]
fn a_received_projects_upload_pack_never_runs() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("untrusted-uploadpack");
    let remote = bare(&scratch, "remote.git");
    let (repo, part) = connected(&scratch, &cli, &remote.to_string_lossy());
    // As MitCad connected it: fetched, locked, nothing refused.
    assert!(repo.fetch(None).is_ok());
    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(info["untrusted_config"], Value::Null, "{info}");

    let marker = scratch.join("marker");
    let program = marker_program(&scratch, &marker);
    git(
        repo.root(),
        &["config", "remote.origin.uploadpack", &program],
    )
    .unwrap();
    let taken = lock_take(&repo, &part);
    assert_eq!(error_class(&taken), "untrusted_config", "{taken}");
    let message = taken["error"]["message"].as_str().unwrap();
    assert!(message.contains("remote.origin.uploadpack"), "{message}");
    assert!(message.contains("git config --local --unset"), "{message}");
    let fetched = run(&repo, json!({"cmd": "fetch"}));
    assert_eq!(error_class(&fetched), "untrusted_config", "{fetched}");
    new_version(&repo, &part, 25.0);
    let pushed = run(&repo, json!({"cmd": "push"}));
    assert_eq!(error_class(&pushed), "untrusted_config", "{pushed}");
    let synced = run(&repo, json!({"cmd": "sync", "author": AUTHOR}));
    assert_eq!(error_class(&synced), "untrusted_config", "{synced}");
    assert!(!marker.exists(), "a program of the repository ran");
    // The project's state names the refusal (the project indicator shows
    // it), and the versions here are not affected.
    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(
        info["untrusted_config"]["class"], "untrusted_config",
        "{info}"
    );
    assert_eq!(info["last_fetch"]["error"]["class"], "untrusted_config");
    let text = repo
        .command_text(&json!({"cmd": "remote_info"}).to_string())
        .unwrap();
    assert!(
        text.starts_with("warning: MitCad does not run git in this project"),
        "{text}"
    );
    assert_eq!(repo.history(&part).unwrap().len(), 2);

    // git as it is would have started it.
    plain_git(repo.root(), &["fetch", "origin"]);
    assert!(marker.exists(), "plain git ran no upload-pack program");
}

/// `core.sshCommand` with an ssh:// remote and `credential.helper` with an
/// https:// remote: refused before git starts, also by the git program's
/// own runs (`ls-remote`) in the folder or below it.
#[test]
fn ssh_commands_and_credential_helpers_of_a_repository_are_refused() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("untrusted-ssh");
    let marker = scratch.join("marker");
    let program = marker_program(&scratch, &marker);
    let (repo, part) = project_with_remote(&scratch, &cli, "ssh://test/remote.git");
    git(repo.root(), &["config", "core.sshCommand", &program]).unwrap();
    assert_eq!(class_of(repo.fetch(None)), ErrorClass::UntrustedConfig);
    assert_eq!(class_of(repo.push(None)), ErrorClass::UntrustedConfig);
    let taken = lock_take(&repo, &part);
    assert_eq!(error_class(&taken), "untrusted_config", "{taken}");
    let message = taken["error"]["message"].as_str().unwrap();
    assert!(message.contains("core.sshcommand"), "{message}");
    let folder = repo.root().join("designs");
    fs::create_dir_all(&folder).unwrap();
    for dir in [repo.root(), folder.as_path()] {
        let error = cli
            .run(Some(dir), &["ls-remote", "origin"], None)
            .unwrap_err();
        assert_eq!(error.class, ErrorClass::UntrustedConfig, "{error:?}");
    }
    assert!(!marker.exists(), "the SSH command ran");
    plain_git(repo.root(), &["fetch", "origin"]);
    assert!(marker.exists(), "plain git ran no SSH command");

    let scratch = Scratch::new("untrusted-credential");
    let marker = scratch.join("marker");
    let program = marker_program(&scratch, &marker);
    let (repo, part) = project_with_remote(&scratch, &cli, "https://127.0.0.1:1/remote.git");
    git(
        repo.root(),
        &["config", "credential.helper", &format!("!{program}")],
    )
    .unwrap();
    git(
        repo.root(),
        &[
            "config",
            "credential.https://127.0.0.1.helper",
            &format!("!{program}"),
        ],
    )
    .unwrap();
    let fetched = run(&repo, json!({"cmd": "fetch"}));
    assert_eq!(error_class(&fetched), "untrusted_config", "{fetched}");
    let message = fetched["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("credential.helper")
            && message.contains("credential.https://127.0.0.1.helper"),
        "{message}"
    );
    let taken = lock_take(&repo, &part);
    assert_eq!(error_class(&taken), "untrusted_config", "{taken}");
    let error = cli
        .run(Some(repo.root()), &["ls-remote", "origin"], None)
        .unwrap_err();
    assert_eq!(error.class, ErrorClass::UntrustedConfig);
    assert!(!marker.exists(), "the credential helper ran");
}

/// Hooks never run, also those in `.git/hooks` that no key names, and a
/// team's `core.hooksPath` is no reason to refuse: such a project still
/// pushes and fetches.
#[test]
fn hooks_never_run_and_a_hooks_path_is_not_refused() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("untrusted-hooks");
    let remote = bare(&scratch, "remote.git");
    let (repo, part) = connected(&scratch, &cli, &remote.to_string_lossy());
    let marker = scratch.join("marker");
    let git_dir = repo.repo.git_dir().to_path_buf();
    for hook in [
        "pre-push",
        "reference-transaction",
        "post-merge",
        "pre-auto-gc",
    ] {
        marker_hook(&git_dir.join("hooks").join(hook), &marker);
    }
    new_version(&repo, &part, 25.0);
    assert!(repo.push(None).unwrap().pushed);
    assert_eq!(repo.fetch(None).unwrap().behind, Some(0));

    marker_hook(&repo.root().join(".githooks").join("pre-push"), &marker);
    git(repo.root(), &["config", "core.hooksPath", ".githooks"]).unwrap();
    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(info["untrusted_config"], Value::Null, "{info}");
    new_version(&repo, &part, 30.0);
    assert!(repo.push(None).unwrap().pushed);
    let taken = lock_take(&repo, &part);
    assert_eq!(taken["error"], Value::Null, "{taken}");
    assert!(!marker.exists(), "a hook ran");

    // git as it is runs the hook.
    new_version(&repo, &part, 35.0);
    plain_git(repo.root(), &["push", "origin", "main"]);
    assert!(marker.exists(), "plain git ran no hook");
}

/// The address of a configured remote is checked as a typed one is,
/// before git fetches from, pushes to or lists it.
#[test]
fn a_configured_remotes_address_is_checked() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("untrusted-url");
    let marker = scratch.join("marker");
    let (repo, _, part) = project(&scratch);
    repo.commit(std::slice::from_ref(&part), "First", &author())
        .unwrap();
    repo.set_git(cli.clone());
    let helper = format!("ext::sh -c touch% {}", slashes(&marker));
    git(repo.root(), &["remote", "add", "origin", &helper]).unwrap();
    let error = match repo.fetch(None) {
        Err(VcsError::Remote(error)) => error,
        other => panic!("{other:?}"),
    };
    assert_eq!(error.class, ErrorClass::InvalidUrl);
    assert!(
        error.message.starts_with("The remote origin is not used:"),
        "{}",
        error.message
    );
    assert_eq!(class_of(repo.push(None)), ErrorClass::InvalidUrl);
    let taken = lock_take(&repo, &part);
    assert_eq!(error_class(&taken), "invalid_url", "{taken}");
    // A push address of its own is checked too.
    git(
        repo.root(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://example.invalid/a.git",
        ],
    )
    .unwrap();
    git(
        repo.root(),
        &[
            "config",
            "remote.origin.pushurl",
            "https://user:s3cret@example.invalid/a.git",
        ],
    )
    .unwrap();
    let pushed = run(&repo, json!({"cmd": "push"}));
    assert_eq!(error_class(&pushed), "invalid_url", "{pushed}");
    assert!(!pushed.to_string().contains("s3cret"), "{pushed}");
    // Not a network command: Project Settings can still change the address.
    let set = run(
        &repo,
        json!({"cmd": "remote_set", "url": "https://example.invalid/b.git", "author": AUTHOR}),
    );
    assert_eq!(set["error"], Value::Null, "{set}");
    assert_eq!(
        git(repo.root(), &["config", "remote.origin.url"])
            .unwrap()
            .trim(),
        "https://example.invalid/b.git"
    );
    assert!(!marker.exists());
}

/// Which keys of a repository's own configuration are refused.
#[test]
fn the_refused_keys() {
    let refused = [
        ("core", None, "sshcommand"),
        ("core", None, "gitproxy"),
        ("core", None, "askpass"),
        ("core", None, "worktree"),
        ("remote", Some("origin"), "uploadpack"),
        ("remote", Some("origin"), "receivepack"),
        ("remote", Some("origin"), "vcs"),
        ("remote", Some("origin"), "proxy"),
        ("credential", None, "helper"),
        ("credential", Some("https://example.org"), "helper"),
        ("url", Some("ext::sh -c x"), "insteadof"),
        ("url", Some("/tmp/x"), "pushinsteadof"),
        ("http", None, "proxy"),
        ("http", Some("https://example.org"), "sslverify"),
        ("http", None, "extraheader"),
        ("protocol", None, "allow"),
        ("protocol", Some("ext"), "allow"),
        ("filter", Some("lfs"), "smudge"),
        ("filter", Some("x"), "process"),
        ("diff", None, "external"),
        ("diff", Some("x"), "textconv"),
        ("merge", Some("x"), "driver"),
        ("gpg", None, "program"),
        ("push", None, "gpgsign"),
        ("include", None, "path"),
        ("includeif", Some("gitdir:/x"), "path"),
        ("init", None, "templatedir"),
        ("alias", None, "lfs"),
        ("lfs", Some("customtransfer.x"), "path"),
        ("lfs", Some("Extension.x"), "smudge"),
        ("lfs", None, "standalonetransferagent"),
    ];
    for (section, subsection, name) in refused {
        assert!(
            untrusted(section, subsection, name),
            "{section}.{subsection:?}.{name}"
        );
    }
    let kept = [
        ("core", None, "repositoryformatversion"),
        ("core", None, "bare"),
        ("core", None, "symlinks"),
        ("core", None, "hookspath"),
        ("core", None, "fsmonitor"),
        ("remote", Some("origin"), "url"),
        ("remote", Some("origin"), "fetch"),
        ("branch", Some("main"), "remote"),
        ("branch", Some("main"), "merge"),
        ("user", None, "name"),
        ("user", None, "email"),
        ("credential", None, "usehttppath"),
        ("http", None, "postbuffer"),
        ("lfs", Some("https://example.org/"), "access"),
        ("alias", None, "st"),
    ];
    for (section, subsection, name) in kept {
        assert!(
            !untrusted(section, subsection, name),
            "{section}.{subsection:?}.{name}"
        );
    }
}

/// The files as git reads them: any case, the work tree's own file, a
/// linked work tree's common folder; a file that cannot be parsed is
/// refused; `--git-dir` names the repository; git's search upwards.
#[test]
fn the_configuration_files_are_read_as_git_reads_them() {
    let scratch = Scratch::new("untrusted-files");
    let git_dir = scratch.join("repo/.git");
    fs::create_dir_all(git_dir.join("objects")).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(
        git_dir.join("config"),
        "[core]\n\tbare = false\n[Core]\n\tSSHCommand = x\n[remote \"origin\"]\n\turl = /x\n\
         \tUploadPack = y\n[remote.second]\n\turl = /z\n",
    )
    .unwrap();
    fs::write(
        git_dir.join("config.worktree"),
        "[credential]\n\thelper = z\n",
    )
    .unwrap();
    let config = RepositoryConfig::read(&git_dir).unwrap();
    let (keys, files) = config.untrusted_keys();
    assert_eq!(
        keys,
        [
            "core.sshcommand",
            "remote.origin.uploadpack",
            "credential.helper"
        ]
    );
    assert_eq!(files.len(), 2);
    let error = config.refuse_untrusted().unwrap_err();
    assert_eq!(error.class, ErrorClass::UntrustedConfig);

    // git's search upwards, from a folder in the work tree.
    let below = scratch.join("repo/a/b");
    fs::create_dir_all(&below).unwrap();
    assert_eq!(repository_of(&below), Some(git_dir.clone()));
    let error = check_run(Some(&below), &["fetch", "origin"]).unwrap_err();
    assert_eq!(error.class, ErrorClass::UntrustedConfig);
    // A work tree whose .git file names a linked git folder: the common
    // folder's configuration.
    let linked = git_dir.join("worktrees/other");
    fs::create_dir_all(&linked).unwrap();
    fs::write(linked.join("HEAD"), "ref: refs/heads/other\n").unwrap();
    fs::write(linked.join("commondir"), "../..\n").unwrap();
    let other = scratch.join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(
        other.join(".git"),
        format!("gitdir: {}\n", linked.to_string_lossy()),
    )
    .unwrap();
    assert_eq!(repository_of(&other), Some(linked.clone()));
    let error = check_run(Some(&other), &["status"]).unwrap_err();
    assert_eq!(error.class, ErrorClass::UntrustedConfig);
    // --git-dir names the repository, wherever git runs.
    let outside = scratch.join("outside");
    fs::create_dir_all(&outside).unwrap();
    // (The temporary folder may itself be inside a repository.)
    if repository_of(&outside).is_none() {
        assert!(check_run(Some(&outside), &["ls-remote", "/x"]).is_ok());
    }
    let named = git_dir.to_string_lossy().into_owned();
    let flag = format!("--git-dir={named}");
    for args in [
        vec!["-c", "gc.auto=0", "--git-dir", named.as_str(), "fetch"],
        vec![flag.as_str(), "fetch"],
    ] {
        let error = check_run(Some(&outside), &args).unwrap_err();
        assert_eq!(error.class, ErrorClass::UntrustedConfig, "{args:?}");
    }

    // Only harmless keys: accepted; a remote's address is checked only
    // for a command that reaches it, by its name.
    fs::write(
        git_dir.join("config"),
        "[core]\n\tbare = false\n\thooksPath = .githooks\n[remote \"origin\"]\n\t\
         url = https://user:s3cret@example.invalid/x.git\n",
    )
    .unwrap();
    fs::write(git_dir.join("config.worktree"), "").unwrap();
    assert!(check_run(Some(&below), &["status"]).is_ok());
    assert!(check_run(Some(&below), &["config", "branch.main.remote", "origin"]).is_ok());
    for command in ["fetch", "push", "ls-remote"] {
        let error = check_run(Some(&below), &[command, "--prune", "origin"]).unwrap_err();
        assert_eq!(error.class, ErrorClass::InvalidUrl, "{command}");
        assert!(!error.message.contains("s3cret"), "{}", error.message);
    }

    // A file git might read otherwise than the check: refused.
    fs::write(git_dir.join("config"), "[core\n\tsshCommand = x\n").unwrap();
    let error = check_run(Some(&below), &["status"]).unwrap_err();
    assert_eq!(error.class, ErrorClass::UntrustedConfig);
}

/// A hook at `path` that adds its name to `marker` and lets git go on.
fn passing_marker_hook(path: &Path, marker: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    fs::write(
        path,
        format!(
            "#!/bin/sh\necho {name} >> \"{}\"\nexit 0\n",
            slashes(marker)
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A received project whose remote is a folder beside it (`../remote.git`,
/// then a `file://` URL) with server-side hooks in that folder's repository
/// (mitcad#940): the lock take on open, a push, a fetch and a sync answer
/// and none of the hooks runs; that repository's own configuration naming
/// a program refuses the remote; plain git, run as it is, runs the hooks.
#[test]
fn a_folder_remotes_hooks_never_run() {
    let Some(cli) = system_git() else { return };
    let scratch = Scratch::new("untrusted-folder-remote");
    let remote = bare(&scratch, "remote.git");
    let (repo, part) = connected(&scratch, &cli, &remote.to_string_lossy());
    // As it arrives: the remote named by a path relative to the project.
    git(
        repo.root(),
        &["config", "remote.origin.url", "../remote.git"],
    )
    .unwrap();
    let marker = scratch.join("marker");
    for hook in [
        "pre-receive",
        "update",
        "post-receive",
        "post-update",
        "reference-transaction",
        "push-to-checkout",
    ] {
        passing_marker_hook(&remote.join("hooks").join(hook), &marker);
    }
    let info = run(&repo, json!({"cmd": "remote_info"}));
    assert_eq!(info["untrusted_config"], Value::Null, "{info}");

    let taken = lock_take(&repo, &part);
    assert_eq!(taken["error"], Value::Null, "{taken}");
    assert_eq!(taken["outcome"], "taken", "{taken}");
    new_version(&repo, &part, 25.0);
    assert!(repo.push(None).unwrap().pushed);
    assert_eq!(repo.fetch(None).unwrap().behind, Some(0));
    new_version(&repo, &part, 30.0);
    let synced = run(&repo, json!({"cmd": "sync", "author": AUTHOR}));
    assert_eq!(synced["error"], Value::Null, "{synced}");
    let released = run(
        &repo,
        json!({"cmd": "lock_release", "path": part.to_string_lossy(), "session": SESSION,
               "author": AUTHOR}),
    );
    assert_eq!(released["error"], Value::Null, "{released}");
    assert!(!marker.exists(), "a hook of the remote's repository ran");

    // The same through a file:// URL.
    let url = format!(
        "file:///{}",
        slashes(&remote).trim_start_matches('/').replace(' ', "%20")
    );
    git(repo.root(), &["config", "remote.origin.url", &url]).unwrap();
    let taken = lock_take(&repo, &part);
    assert_eq!(taken["error"], Value::Null, "{taken}");
    new_version(&repo, &part, 35.0);
    assert!(repo.push(None).unwrap().pushed);
    assert!(!marker.exists(), "a hook of the remote's repository ran");

    // The remote's own configuration naming a program: refused, nothing
    // runs; the project's versions are not affected.
    let program_marker = scratch.join("program-marker");
    let program = marker_program(&scratch, &program_marker);
    git(&remote, &["config", "core.alternateRefsCommand", &program]).unwrap();
    let taken = lock_take(&repo, &part);
    assert_eq!(error_class(&taken), "untrusted_config", "{taken}");
    let message = taken["error"]["message"].as_str().unwrap();
    assert!(message.contains("does not use the remote"), "{message}");
    assert!(message.contains("core.alternaterefscommand"), "{message}");
    new_version(&repo, &part, 40.0);
    assert_eq!(class_of(repo.push(None)), ErrorClass::UntrustedConfig);
    assert_eq!(class_of(repo.fetch(None)), ErrorClass::UntrustedConfig);
    assert!(!program_marker.exists(), "a program of the remote ran");
    git(&remote, &["config", "--unset", "core.alternateRefsCommand"]).unwrap();
    assert!(repo.push(None).unwrap().pushed);
    assert!(!marker.exists(), "a hook of the remote's repository ran");

    // git as it is runs them.
    new_version(&repo, &part, 45.0);
    plain_git(repo.root(), &["push", "origin", "main"]);
    let ran = fs::read_to_string(&marker).unwrap_or_default();
    assert!(ran.contains("pre-receive"), "plain git ran no hook: {ran}");
}

/// Which runs get the option that keeps a folder remote's hooks off, and
/// where it goes: after the command, for a remote whose (push) address is
/// a folder, a `file://` URL or a legacy `remotes/` file's folder; not for
/// a server; option values are not taken for the repository.
#[test]
fn a_folder_remote_gets_hooks_off() {
    let scratch = Scratch::new("untrusted-folder-options");
    let git_dir = scratch.join("repo/.git");
    fs::create_dir_all(git_dir.join("objects")).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(
        git_dir.join("config"),
        "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = ../remote.git\n\
         [remote \"server\"]\n\turl = ssh://example.invalid/x.git\n\
         [remote \"mirror\"]\n\turl = ../remote.git\n\tpushurl = git@example.invalid:x.git\n",
    )
    .unwrap();
    fs::create_dir_all(git_dir.join("remotes")).unwrap();
    fs::write(
        git_dir.join("remotes/legacy"),
        "URL: ../remote.git\nPush: refs/heads/main\n",
    )
    .unwrap();
    let root = scratch.join("repo");
    let option = |args: &[&str]| -> Option<(usize, String)> {
        let checked = check_run(Some(&root), args).unwrap();
        let with = checked.arguments(args);
        assert!(with.len() <= args.len() + 1, "{with:?}");
        with.iter()
            .position(|arg| *arg == RECEIVE_PACK || *arg == UPLOAD_PACK)
            .map(|at| (at, with[at].to_owned()))
    };
    let receive = Some((1, RECEIVE_PACK.to_owned()));
    let upload = Some((1, UPLOAD_PACK.to_owned()));
    assert_eq!(option(&["push", "--porcelain", "origin", "a:b"]), receive);
    assert_eq!(option(&["fetch", "--prune", "origin"]), upload);
    assert_eq!(option(&["ls-remote", "origin", "refs/mitcad/*"]), upload);
    assert_eq!(
        option(&["-c", "gc.auto=0", "fetch", "origin"]),
        Some((3, UPLOAD_PACK.to_owned()))
    );
    assert_eq!(option(&["push", "legacy", "main"]), receive);
    assert_eq!(option(&["push", "--repo=origin"]), receive);
    // A server: nothing added, also for a refspec that looks like a path.
    assert_eq!(
        option(&["push", "server", "refs/heads/a:refs/heads/b"]),
        None
    );
    assert_eq!(option(&["fetch", "server"]), None);
    // A push address of its own decides a push.
    assert_eq!(option(&["push", "mirror", "main"]), None);
    assert_eq!(option(&["fetch", "mirror"]), upload);
    // Addresses as operands, and option values that are not.
    assert_eq!(option(&["ls-remote", "--symref", "/srv/x.git"]), upload);
    assert_eq!(
        option(&["ls-remote", "https://example.invalid/x.git"]),
        None
    );
    assert_eq!(
        option(&[
            "clone", "--origin", "origin", "--branch", "main", "--", "/x.git", "t"
        ]),
        upload
    );
    assert_eq!(
        option(&[
            "clone",
            "--branch",
            "main",
            "--",
            "git@example.invalid:x.git",
            "t"
        ]),
        None
    );
    // Not a network command.
    assert_eq!(option(&["status"]), None);
    assert_eq!(option(&["merge", "--ff-only", "origin/main"]), None);

    for folder in [
        "../x.git",
        "/srv/x.git",
        "x",
        "file:///srv/x.git",
        "FILE://h/x",
    ] {
        assert!(is_folder(folder), "{folder}");
    }
    for server in [
        "ssh://example.invalid/x.git",
        "https://example.invalid/x.git",
        "git@example.invalid:x.git",
        "example.invalid:x.git",
        "",
    ] {
        assert!(!is_folder(server), "{server}");
    }

    // The remote's repository as git finds it, also by a file:// URL with
    // escapes; its configuration naming a program refuses it.
    let remote = scratch.join("a b.git");
    fs::create_dir_all(remote.join("objects")).unwrap();
    fs::write(remote.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(remote.join("config"), "[core]\n\tbare = true\n").unwrap();
    assert_eq!(
        folder_repository(Some(&scratch.0), "a b.git"),
        Some(remote.clone())
    );
    assert_eq!(
        folder_repository(Some(&scratch.0), "a b"),
        Some(remote.clone())
    );
    let url = format!(
        "file:///{}",
        slashes(&remote).trim_start_matches('/').replace(' ', "%20")
    );
    assert_eq!(folder_repository(None, &url), Some(remote.clone()));
    assert_eq!(folder_repository(Some(&scratch.0), "missing.git"), None);
    let remote_text = remote.to_string_lossy().into_owned();
    assert!(check_run(Some(&root), &["ls-remote", &remote_text]).is_ok());
    fs::write(
        remote.join("config"),
        "[core]\n\tbare = true\n\talternateRefsCommand = x\n",
    )
    .unwrap();
    for args in [
        vec!["ls-remote", remote_text.as_str()],
        vec!["push", url.as_str(), "main"],
    ] {
        let error = check_run(Some(&root), &args).unwrap_err();
        assert_eq!(error.class, ErrorClass::UntrustedConfig, "{args:?}");
        assert!(
            error.message.contains("core.alternaterefscommand"),
            "{}",
            error.message
        );
    }
}
