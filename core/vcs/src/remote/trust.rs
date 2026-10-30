// SPDX-License-Identifier: MIT
//! A repository's own git configuration as untrusted input (mitcad#920).
//!
//! A project folder can come from anywhere (a zip, a shared drive) with its
//! `.git` folder, and git reads that folder's configuration on every run.
//! Some keys name a program git starts as the user (`core.sshCommand`,
//! `credential.helper`, `remote.<name>.uploadpack` for a remote in a
//! folder), send the sign-in or the traffic elsewhere (`url.<base>.insteadOf`,
//! `http.proxy`) or move the work tree; and MitCad runs git without a
//! question when a design opens (the edit lock taken, the quiet fetch). So
//! every git run of the remote code ([`check_run`], called by
//! [`super::GitCli`] before git starts):
//!
//! - refuses to run in a repository whose own configuration (`config` of its
//!   git folder, `config.worktree`) sets such a key ([`untrusted`]): the error
//!   class `untrusted_config`, which names the keys and what to do, and git
//!   does not start. These keys are refused rather than overridden because
//!   their safe value is the user's own configuration (an SSH command or a
//!   credential helper that signing in needs), which an override on the
//!   command line would replace;
//! - overrides on the command line what has one safe value whatever the
//!   configuration says ([`NEUTRAL`]): no hooks run (also those in
//!   `.git/hooks`, which no key names), no file system monitor, no
//!   submodules fetched or pushed. These need no refusal, so a project
//!   whose team set `core.hooksPath` still syncs;
//! - checks the address of a remote that the repository configures with
//!   [`check_url`] before git fetches from, pushes to or lists it by its
//!   name: a remote helper (`ext::`), another scheme or credentials in it
//!   are refused as they are when typed into MitCad;
//! - for a command that reaches a remote in a folder (a path or `file://`,
//!   mitcad#940), where git starts that repository's own git on this
//!   computer (`receive-pack` for a push, `upload-pack` for a fetch, a
//!   listing or a clone): runs that git with its hooks and file system
//!   monitor off ([`RECEIVE_PACK`], [`UPLOAD_PACK`]; the `-c` overrides of
//!   [`NEUTRAL`] do not reach it, as git clears them for a local remote),
//!   and refuses (`untrusted_config`) when that repository's own
//!   configuration sets a refused key (`core.alternateRefsCommand`, a
//!   filter or `core.worktree` for a push that updates its work tree). A
//!   project folder that arrives with such a remote beside it runs nothing
//!   of the remote's when its design opens. Remotes on a server are not
//!   affected: their hooks are the server's.
//!
//! Projects that MitCad created or cloned set none of the refused keys, so
//! they are never refused. The files are read here, without git and
//! without following includes (an `include` is refused too).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gix::bstr::ByteSlice;

use super::errors::{ErrorClass, RemoteError, check_url, is_scp_like};

/// The configuration every git run has on its command line, after
/// `core.autocrlf` and `core.eol` (`cli.rs`): no hooks (a folder that does
/// not exist; also on Windows, where Git for Windows reads `/dev/null` as a
/// path), no file system monitor (an empty value is off for every git
/// since 2.34), and no submodules fetched or pushed along.
pub(crate) const NEUTRAL: [&str; 10] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=",
    "-c",
    "fetch.recurseSubmodules=false",
    "-c",
    "push.recurseSubmodules=no",
    "-c",
    "submodule.recurse=false",
];

/// The git commands that reach a remote by its name.
const NETWORK: [&str; 4] = ["fetch", "push", "ls-remote", "pull"];

/// The receiving git of a push to a remote in a folder, which git starts
/// through the shell in that repository (mitcad#940): with no hooks
/// (`pre-receive`, `update`, `post-receive`, `reference-transaction`, …) and
/// no file system monitor, whatever that repository's configuration says.
/// `git` is found first in the folder of the git that pushes (git puts it
/// at the front of `PATH`).
pub(crate) const RECEIVE_PACK: &str =
    "--receive-pack=git -c core.hooksPath=/dev/null -c core.fsmonitor= receive-pack";

/// The sending git of a fetch, a listing or a clone from a remote in a
/// folder, as [`RECEIVE_PACK`].
pub(crate) const UPLOAD_PACK: &str =
    "--upload-pack=git -c core.hooksPath=/dev/null -c core.fsmonitor= upload-pack";

/// What [`check_run`] adds to a git run: for a command that reaches a
/// remote in a folder, [`RECEIVE_PACK`] or [`UPLOAD_PACK`] right after the
/// command.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Checked {
    option: Option<(usize, &'static str)>,
}

impl Checked {
    /// `args` with the option in its place.
    pub(crate) fn arguments<'a>(&self, args: &[&'a str]) -> Vec<&'a str> {
        let mut out = args.to_vec();
        if let Some((at, option)) = self.option {
            out.insert(at.min(out.len()), option);
        }
        out
    }
}

/// Whether git reaches `url` as a repository on this computer (a folder, a
/// `file://` URL) and so starts that repository's git itself: not an
/// address with another scheme, not git's short SSH form `host:path`.
pub(crate) fn is_folder(url: &str) -> bool {
    let url = url.trim();
    match url.find("://") {
        Some(at) => url[..at].eq_ignore_ascii_case("file"),
        None => !url.is_empty() && !is_scp_like(url),
    }
}

/// The path of a folder address: the address itself, or the path of a
/// `file://` URL (percent-decoded, the host left out as git leaves it out;
/// `file:///C:/x` is `C:/x` on Windows).
fn folder_path(url: &str) -> Option<PathBuf> {
    let url = url.trim();
    let Some(at) = url.find("://") else {
        return Some(PathBuf::from(url));
    };
    let rest = &url[at + 3..];
    let path = percent_decoded(&rest[rest.find('/')?..])?;
    let bytes = path.as_bytes();
    let drive = bytes.len() >= 3 && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    if cfg!(windows) && drive {
        return Some(PathBuf::from(&path[1..]));
    }
    Some(PathBuf::from(path))
}

/// `text` with its `%XX` escapes decoded; None when that is not UTF-8.
fn percent_decoded(text: &str) -> Option<String> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'%'
            && let Some(hex) = tail.get(..2)
            && hex.iter().all(u8::is_ascii_hexdigit)
            && let Ok(hex) = std::str::from_utf8(hex)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            out.push(value);
            rest = &tail[2..];
            continue;
        }
        out.push(byte);
        rest = tail;
    }
    String::from_utf8(out).ok()
}

/// The git folder of the repository at the folder address `url` (relative
/// to `base`, the folder git runs in), as git looks for it: `<path>/.git`,
/// `<path>`, `<path>.git/.git`, `<path>.git`.
pub(crate) fn folder_repository(base: Option<&Path>, url: &str) -> Option<PathBuf> {
    let path = folder_path(url)?;
    let path = match base {
        Some(base) => base.join(path),
        None => path,
    };
    let with_suffix = |suffix: &str| {
        let mut text = OsString::from(path.as_os_str());
        text.push(suffix);
        PathBuf::from(text)
    };
    for candidate in [
        path.join(".git"),
        path.clone(),
        with_suffix(".git").join(".git"),
        with_suffix(".git"),
    ] {
        let dot_git = candidate.file_name().is_some_and(|name| name == ".git");
        if dot_git && candidate.is_file() {
            if let Some(target) = linked_folder(&candidate).filter(|target| git_folder(target)) {
                return Some(target);
            }
        } else if git_folder(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Whether a key of a repository's own configuration is refused: `section`
/// and `name` in lower case, `subsection` as written.
pub(crate) fn untrusted(section: &str, subsection: Option<&str>, name: &str) -> bool {
    match section {
        // Programs git starts, a proxy program, and where the work tree is.
        "core" => matches!(
            name,
            "sshcommand" | "gitproxy" | "askpass" | "worktree" | "alternaterefscommand"
        ),
        // Programs run for a remote (locally for a remote in a folder), a
        // remote helper, a proxy.
        "remote" => matches!(name, "uploadpack" | "receivepack" | "vcs" | "proxy"),
        "credential" => name == "helper",
        "url" => matches!(name, "insteadof" | "pushinsteadof"),
        // Proxies, certificates, headers and cookies; the transfer's sizes
        // and speeds are harmless.
        "http" => !matches!(
            name,
            "postbuffer" | "lowspeedlimit" | "lowspeedtime" | "version"
        ),
        "diff" => matches!(name, "external" | "command" | "textconv"),
        "merge" => name == "driver",
        // A signed push starts the signing program.
        "push" => name == "gpgsign",
        // A clone's template folder (hooks, configuration).
        "init" => name == "templatedir",
        // `git lfs version` runs an alias of that name when git-lfs is
        // missing.
        "alias" => name == "lfs",
        // Programs git-lfs starts.
        "lfs" => {
            name == "standalonetransferagent"
                || subsection.is_some_and(|subsection| {
                    let subsection = subsection.to_ascii_lowercase();
                    subsection.starts_with("customtransfer.")
                        || subsection.starts_with("extension.")
                })
        }
        "protocol" | "filter" | "gpg" | "include" | "includeif" => true,
        _ => false,
    }
}

/// Checks a git run with `args` in `dir` (None: the current folder) before
/// it starts: the repository git would use there (or the one `--git-dir`
/// names) must not set a refused key, and a remote that a network command
/// names must have an address [`check_url`] accepts. A command that
/// reaches a remote in a folder gets the option that runs that
/// repository's git without hooks ([`Checked`]), and that repository's own
/// configuration must not set a refused key either.
pub(crate) fn check_run(dir: Option<&Path>, args: &[&str]) -> Result<Checked, RemoteError> {
    let folder = match dir {
        Some(dir) => Some(std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())),
        None => std::env::current_dir().ok(),
    };
    let parsed = Parsed::of(args);
    let git_dir = match (&folder, parsed.git_dir) {
        (Some(folder), Some(path)) => {
            let path = folder.join(path);
            path.is_dir().then_some(path)
        }
        (Some(folder), None) => repository_of(folder),
        (None, _) => None,
    };
    let config = match git_dir {
        Some(git_dir) => {
            let config = RepositoryConfig::read(&git_dir)?;
            config.refuse_untrusted()?;
            Some(config)
        }
        None => None,
    };
    let Some((at, command)) = parsed.command else {
        return Ok(Checked::default());
    };
    if let Some(config) = &config
        && NETWORK.contains(&command)
    {
        for operand in &parsed.operands {
            config.check_remote(operand)?;
        }
    }
    let option = match command {
        "push" => RECEIVE_PACK,
        "fetch" | "pull" | "ls-remote" | "clone" => UPLOAD_PACK,
        _ => return Ok(Checked::default()),
    };
    // The addresses git reaches: a remote's of the repository (for a push
    // its push addresses), else the operand itself; without one, every
    // remote's (MitCad always names one).
    let push = command == "push";
    let addresses = match (parsed.repository(), &config) {
        (Some(repository), Some(config)) if command != "clone" => {
            let addresses = config.remote_addresses(repository, push);
            if addresses.is_empty() {
                vec![repository.to_owned()]
            } else {
                addresses
            }
        }
        (Some(repository), _) => vec![repository.to_owned()],
        (None, Some(config)) => config
            .remote_names()
            .iter()
            .flat_map(|name| config.remote_addresses(name, push))
            .collect(),
        (None, None) => Vec::new(),
    };
    let mut checked = Checked::default();
    for address in addresses.iter().filter(|address| is_folder(address)) {
        checked.option = Some((at + 1, option));
        if let Some(remote) = folder_repository(folder.as_deref(), address) {
            RepositoryConfig::read(&remote)?.refuse_untrusted_remote(address)?;
        }
    }
    Ok(checked)
}

/// What a git command line says: `--git-dir`, the command (and where it
/// is) and its operands (what follows it that is neither an option nor an
/// option's value).
struct Parsed<'a> {
    git_dir: Option<&'a str>,
    command: Option<(usize, &'a str)>,
    operands: Vec<&'a str>,
    /// `push --repo`.
    repo: Option<&'a str>,
}

impl<'a> Parsed<'a> {
    fn of(args: &[&'a str]) -> Self {
        let mut parsed = Parsed {
            git_dir: None,
            command: None,
            operands: Vec::new(),
            repo: None,
        };
        let mut options = true;
        let mut rest = args.iter().copied().enumerate();
        while let Some((at, arg)) = rest.next() {
            if let Some((_, command)) = parsed.command {
                if !options || arg == "-" || !arg.starts_with('-') {
                    parsed.operands.push(arg);
                } else if arg == "--" {
                    options = false;
                } else if let Some(repo) = arg.strip_prefix("--repo=") {
                    parsed.repo = Some(repo);
                } else if takes_value(command, arg) {
                    let value = rest.next().map(|(_, value)| value);
                    if arg == "--repo" {
                        parsed.repo = value;
                    }
                }
                continue;
            }
            match arg {
                "-c" | "-C" => {
                    rest.next();
                }
                "--git-dir" => parsed.git_dir = rest.next().map(|(_, value)| value),
                _ => {
                    if let Some(path) = arg.strip_prefix("--git-dir=") {
                        parsed.git_dir = Some(path);
                    } else if !arg.starts_with('-') {
                        parsed.command = Some((at, arg));
                    }
                }
            }
        }
        parsed
    }

    /// The repository a network command reaches: its first operand, else
    /// `push --repo`.
    fn repository(&self) -> Option<&'a str> {
        self.operands.first().copied().or(self.repo)
    }
}

/// Whether `option` of a git `command` takes the next argument as its
/// value (MitCad writes `--option=value` mostly, but `clone --branch main`
/// too), so that it is not read as an operand.
fn takes_value(command: &str, option: &str) -> bool {
    const VALUE_OPTIONS: &[&str] = &[
        "-o",
        "--upload-pack",
        "--receive-pack",
        "--exec",
        "--repo",
        "--push-option",
        "--server-option",
        "--depth",
        "--deepen",
        "--shallow-since",
        "--shallow-exclude",
        "--filter",
        "-j",
        "--jobs",
        "--negotiation-tip",
        "--refmap",
        "--recurse-submodules-default",
        "--submodule-prefix",
        "--sort",
        "--origin",
        "--branch",
        "--config",
        "--template",
        "--reference",
        "--reference-if-able",
        "--separate-git-dir",
        "--bundle-uri",
        "--ref-format",
        "--strategy",
        "--strategy-option",
    ];
    match option {
        "-u" | "-b" | "-c" => command == "clone",
        "-s" | "-X" => command == "pull",
        _ => VALUE_OPTIONS.contains(&option),
    }
}

/// Whether `dir` looks like a git folder: a `HEAD`, and objects of its own
/// or of the repository it is a work tree of.
fn git_folder(dir: &Path) -> bool {
    dir.join("HEAD").is_file() && (dir.join("objects").is_dir() || dir.join("commondir").is_file())
}

/// The git folder that a `.git` file (`gitdir: <path>`) names.
fn linked_folder(file: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    Some(file.parent()?.join(target))
}

/// The git folder git finds from `folder`, as git looks for it: in each
/// folder upwards, its `.git` (a folder, or a file naming one), then the
/// folder itself as a bare repository.
pub(crate) fn repository_of(folder: &Path) -> Option<PathBuf> {
    for level in folder.ancestors() {
        let dot_git = level.join(".git");
        if dot_git.is_file() {
            if let Some(target) = linked_folder(&dot_git).filter(|target| git_folder(target)) {
                return Some(target);
            }
        } else if git_folder(&dot_git) {
            return Some(dot_git);
        }
        if git_folder(level) {
            return Some(level.to_path_buf());
        }
    }
    None
}

/// A repository's own configuration files, as git reads them for it.
pub(crate) struct RepositoryConfig {
    files: Vec<(PathBuf, gix::config::File)>,
    /// The common git folder (`remotes/`, `branches/`).
    common: PathBuf,
}

impl RepositoryConfig {
    /// `config` of the repository's common git folder and the work tree's
    /// `config.worktree`. A file that cannot be read or parsed is refused:
    /// what git would make of it is not known.
    pub(crate) fn read(git_dir: &Path) -> Result<Self, RemoteError> {
        let common = std::fs::read_to_string(git_dir.join("commondir"))
            .ok()
            .map(|text| git_dir.join(text.trim()))
            .unwrap_or_else(|| git_dir.to_path_buf());
        let mut files = Vec::new();
        for (path, source) in [
            (common.join("config"), gix::config::Source::Local),
            (
                git_dir.join("config.worktree"),
                gix::config::Source::Worktree,
            ),
        ] {
            if !path.exists() {
                continue;
            }
            match gix::config::File::from_path_no_includes(path.clone(), source) {
                Ok(file) => files.push((path, file)),
                Err(error) => {
                    return Err(RemoteError::new(
                        ErrorClass::UntrustedConfig,
                        format!(
                            "MitCad does not run git in this project: the repository's \
                             configuration {} cannot be read ({error})",
                            path.display()
                        ),
                    ));
                }
            }
        }
        Ok(Self { files, common })
    }

    /// The refused keys the files set, as `section.subsection.name`, and
    /// the files that set them.
    pub(crate) fn untrusted_keys(&self) -> (Vec<String>, Vec<&Path>) {
        let mut keys: Vec<String> = Vec::new();
        let mut paths: Vec<&Path> = Vec::new();
        for (path, file) in &self.files {
            for section in file.sections() {
                let header = section.header();
                let name = header.name().to_str_lossy().to_ascii_lowercase();
                let subsection = header
                    .subsection_name()
                    .map(|subsection| subsection.to_str_lossy().into_owned());
                for value in section.body().value_names() {
                    let value = value.to_ascii_lowercase();
                    if !untrusted(&name, subsection.as_deref(), &value) {
                        continue;
                    }
                    let key = match &subsection {
                        Some(subsection) => format!("{name}.{subsection}.{value}"),
                        None => format!("{name}.{value}"),
                    };
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                    if !paths.contains(&path.as_path()) {
                        paths.push(path);
                    }
                }
            }
        }
        (keys, paths)
    }

    /// The error `untrusted_config` when the files set a refused key.
    pub(crate) fn refuse_untrusted(&self) -> Result<(), RemoteError> {
        let (keys, paths) = self.untrusted_keys();
        if keys.is_empty() {
            return Ok(());
        }
        let files: Vec<String> = paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        Err(RemoteError::new(
            ErrorClass::UntrustedConfig,
            format!(
                "MitCad does not run git in this project: its repository's own configuration \
                 ({}) sets {}, which could start programs or send your sign-in elsewhere. If \
                 you set {} yourself, remove {} from the project (git config --local --unset \
                 <key>) and keep what you need in your own configuration (git config --global). \
                 The versions on this computer are not affected.",
                files.join(", "),
                keys.join(", "),
                if keys.len() == 1 { "it" } else { "them" },
                if keys.len() == 1 { "it" } else { "them" },
            ),
        ))
    }

    /// The error `untrusted_config` when the files, those of the repository
    /// of a remote in a folder at `address`, set a refused key: git starts
    /// that repository's git on this computer for a push or a fetch, and it
    /// reads them (mitcad#940).
    pub(crate) fn refuse_untrusted_remote(&self, address: &str) -> Result<(), RemoteError> {
        let (keys, paths) = self.untrusted_keys();
        if keys.is_empty() {
            return Ok(());
        }
        let files: Vec<String> = paths
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        Err(RemoteError::new(
            ErrorClass::UntrustedConfig,
            format!(
                "MitCad does not use the remote {address}: the configuration of the repository \
                 in that folder ({}) sets {}, which could start programs on this computer when \
                 git sends to it or reads from it. If you trust that repository, remove {} from \
                 its configuration (git config --file <its config> --unset <key>). The versions \
                 on this computer are not affected.",
                files.join(", "),
                keys.join(", "),
                if keys.len() == 1 { "it" } else { "them" },
            ),
        ))
    }

    /// The names of the remotes these files configure.
    pub(crate) fn remote_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for (_, file) in &self.files {
            for section in file.sections() {
                let header = section.header();
                if !header.name().to_str_lossy().eq_ignore_ascii_case("remote") {
                    continue;
                }
                if let Some(name) = header.subsection_name() {
                    let name = name.to_str_lossy().into_owned();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        names
    }

    /// The values of `key` of the remote `name` in these files.
    fn remote_values(&self, name: &str, key: &str) -> Vec<String> {
        let mut values = Vec::new();
        for (_, file) in &self.files {
            for value in file.strings_by("remote", name, key).unwrap_or_default() {
                values.push(value.to_str_lossy().into_owned());
            }
        }
        values
    }

    /// The addresses of the remote `name` in the files git still reads
    /// beside its configuration when that does not configure it:
    /// `remotes/<name>` (its `URL:` lines), else `branches/<name>` (the
    /// address, then `#branch`).
    fn legacy_addresses(&self, name: &str) -> Vec<String> {
        if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
            return Vec::new();
        }
        let remotes =
            std::fs::read_to_string(self.common.join("remotes").join(name)).unwrap_or_default();
        let urls: Vec<String> = remotes
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("URL:"))
            .map(|url| url.trim().to_owned())
            .filter(|url| !url.is_empty())
            .collect();
        if !urls.is_empty() {
            return urls;
        }
        let branches =
            std::fs::read_to_string(self.common.join("branches").join(name)).unwrap_or_default();
        branches
            .lines()
            .next()
            .and_then(|line| line.split('#').next())
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(|url| vec![url.to_owned()])
            .unwrap_or_default()
    }

    /// The addresses git reaches for the remote `name` (empty when it is
    /// not a remote of this repository): for a push its `pushurl`s when it
    /// has any, else its `url`s.
    pub(crate) fn remote_addresses(&self, name: &str, push: bool) -> Vec<String> {
        if push {
            let push_urls = self.remote_values(name, "pushurl");
            if !push_urls.is_empty() {
                return push_urls;
            }
        }
        let urls = self.remote_values(name, "url");
        if urls.is_empty() {
            return self.legacy_addresses(name);
        }
        urls
    }

    /// Every address (`url`, `pushurl`) of the remote `name`, when this
    /// repository has one.
    fn remote_urls(&self, name: &str) -> Vec<String> {
        let mut urls = self.remote_values(name, "url");
        urls.extend(self.remote_values(name, "pushurl"));
        if urls.is_empty() {
            urls = self.legacy_addresses(name);
        }
        urls
    }

    /// [`check_url`] on every address of the remote `name` (nothing when
    /// it is not a remote of these files).
    pub(crate) fn check_remote(&self, name: &str) -> Result<(), RemoteError> {
        for url in self.remote_urls(name) {
            check_url(&url).map_err(|error| {
                RemoteError::new(
                    ErrorClass::InvalidUrl,
                    format!("The remote {name} is not used: {}", error.message),
                )
            })?;
        }
        Ok(())
    }
}
