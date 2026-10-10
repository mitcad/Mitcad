// SPDX-License-Identifier: MIT
//! What went wrong with a remote repository, told from git's messages; the
//! URLs Mitcad passes to git; and credentials hidden in every text Mitcad
//! shows or logs.
//!
//! git runs with `LC_ALL=C` (`cli.rs`), so its messages are English and the
//! classes below can be told from them. Mitcad never asks for, keeps or
//! passes passwords or tokens: a URL with credentials is refused, and what
//! git prints (a URL configured outside Mitcad may hold them) goes through
//! [`redact`] before anyone sees it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The kinds of failure the application tells apart (its status bar and
/// hints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorClass {
    /// No git program, or `MITCAD_GIT` names none.
    GitMissing,
    /// A URL Mitcad does not give to git: with credentials, a remote helper
    /// (`ext::`), or a scheme other than ssh, https, http and file.
    InvalidUrl,
    /// The project has no remote repository.
    NoRemote,
    /// A state the operation does not handle: HEAD detached, no version
    /// yet, histories that went apart.
    Unsupported,
    /// The server refused the sign-in (SSH key, credential helper).
    AuthFailed,
    /// The server's SSH host key is not known yet, or changed.
    HostKeyUnknown,
    /// The server cannot be reached (offline, a wrong host name).
    Network,
    /// No repository there, or no access to it.
    NotFound,
    /// The remote has versions this project lacks (not a fast-forward).
    Rejected,
    /// The server refused a file that is too large (GitHub: 100 MiB).
    TooLarge,
    /// The repository uses Git LFS and git-lfs is not installed.
    LfsMissing,
    /// A cloned repository holds no Mitcad project.
    NotAProject,
    /// The remote holds another history than the project's.
    Unrelated,
    /// git gave no sign of life for too long and was stopped.
    TimedOut,
    Cancelled,
    /// Files changed both in the project and on the remote need a choice
    /// (keep mine, take theirs, save mine as a copy) before a sync.
    Conflict,
    /// Files a sync would change have changes no version holds.
    LocalChanges,
    Other,
    // Local and Cloud projects (mitcad#89).
    /// The remote holds a Mitcad project (where a new one was to go).
    HasProject,
    /// A folder that must be empty (or missing) is not.
    NotEmpty,
    /// A folder that is a project, or inside one, where a new project was
    /// to be made.
    InsideProject,
}

/// A failure of a remote operation: its class, a message for people and
/// git's own words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteError {
    pub class: ErrorClass,
    /// What failed and what to do, credentials hidden.
    pub message: String,
    /// git's error output, credentials hidden; empty when git did not run.
    pub detail: String,
}

impl RemoteError {
    pub fn new(class: ErrorClass, message: impl Into<String>) -> Self {
        Self {
            class,
            message: redact(&message.into()),
            detail: String::new(),
        }
    }

    /// The failure of a git command that printed `stderr`: `what` failed
    /// (`Fetching from origin`), classified, with a hint.
    pub fn from_git(what: &str, stderr: &str) -> Self {
        let class = classify(stderr);
        let detail = redact(stderr.trim());
        let message = match hint(class) {
            Some(hint) => format!("{what} failed: {hint}"),
            None => match first_error_line(&detail) {
                Some(line) => format!("{what} failed: {line}"),
                None => format!("{what} failed"),
            },
        };
        Self {
            class,
            message: redact(&message),
            detail,
        }
    }
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RemoteError {}

/// What to do about a class of failure, if anything general can be said.
fn hint(class: ErrorClass) -> Option<&'static str> {
    Some(match class {
        ErrorClass::AuthFailed => {
            "the server refused the sign-in. For SSH, add your key to the SSH agent \
             (ssh-add) and to your account on the server; for HTTPS, set up a credential \
             helper such as Git Credential Manager, or use SSH."
        }
        ErrorClass::HostKeyUnknown => {
            "the server's SSH host key is not known yet, or it changed. Compare its \
             fingerprint with the one the service publishes, then trust it, which adds it \
             to known_hosts (Trust This Server, or mitcad-cli host-keys <host> --trust)."
        }
        ErrorClass::Network => {
            "the server cannot be reached. Check the network connection and the host name."
        }
        ErrorClass::NotFound => {
            "there is no repository at that address, or your account has no access to it."
        }
        ErrorClass::Rejected => {
            "the remote has versions this project does not have yet. Sync first; Mitcad \
             never overwrites the remote's history."
        }
        ErrorClass::TooLarge => {
            "the server refused a file that is too large (GitHub allows 100 MiB per \
             file). B-rep data that large needs Git LFS."
        }
        ErrorClass::LfsMissing => {
            "the repository stores data with Git LFS, but git-lfs is not installed. \
             Install it (https://git-lfs.com) and try again."
        }
        ErrorClass::InvalidUrl => {
            "git does not allow that kind of address here. Mitcad uses ssh://, https://, \
             http:// and folders."
        }
        _ => return None,
    })
}

/// The class of a git failure from its error output.
pub fn classify(stderr: &str) -> ErrorClass {
    let text = stderr.to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if any(&[
        "host key verification failed",
        "remote host identification has changed",
        "no matching host key",
    ]) {
        ErrorClass::HostKeyUnknown
    } else if any(&[
        "permission denied (publickey",
        "permission denied, please try again",
        "authentication failed",
        "could not read username",
        "could not read password",
        "terminal prompts disabled",
        "invalid username or password",
        "http basic: access denied",
        "returned error: 401",
        "returned error: 403",
    ]) {
        ErrorClass::AuthFailed
    } else if any(&[
        "gh001",
        "exceeds github's file size limit",
        "large files detected",
        "file is too large",
        "request entity too large",
        "returned error: 413",
    ]) {
        ErrorClass::TooLarge
    } else if any(&[
        "git-lfs: command not found",
        "git-lfs was not found",
        "'git-lfs' was not found",
        "git: 'lfs' is not a git command",
    ]) {
        ErrorClass::LfsMissing
    } else if any(&[
        "repository not found",
        "does not appear to be a git repository",
        "returned error: 404",
        "' not found",
        "project you were looking for could not be found",
    ]) {
        ErrorClass::NotFound
    } else if any(&[
        "[rejected]",
        "[remote rejected]",
        "non-fast-forward",
        "updates were rejected",
        "fetch first",
    ]) {
        ErrorClass::Rejected
    } else if (text.contains("transport '") && text.contains("not allowed"))
        || text.contains("unsupported protocol")
    {
        ErrorClass::InvalidUrl
    } else if any(&[
        "could not resolve host",
        "could not resolve hostname",
        "name or service not known",
        "temporary failure in name resolution",
        "connection timed out",
        "operation timed out",
        "connection refused",
        "network is unreachable",
        "no route to host",
        "failed to connect",
        "couldn't connect to server",
        "unable to access",
        "connection reset",
        "connection closed by",
        "ssl certificate",
        "ssl connect error",
        "tls connection",
        "gnutls",
    ]) {
        ErrorClass::Network
    } else {
        ErrorClass::Other
    }
}

/// The line of git's output that says what went wrong: the first `fatal:`
/// or `error:` line without that word, else the first line.
fn first_error_line(stderr: &str) -> Option<String> {
    let lines = || {
        stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .find_map(|line| {
            line.strip_prefix("fatal: ")
                .or_else(|| line.strip_prefix("error: "))
        })
        .or_else(|| lines().next())
        .map(str::to_owned)
}

/// The prefixes of the access tokens of GitHub and GitLab.
const TOKEN_PREFIXES: [&str; 7] = [
    "github_pat_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "glpat-",
];

/// A character of a token.
fn token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Whether a URL's user name looks like a credential: a known token, or a
/// long run of token characters (a password-like string).
fn looks_like_token(user: &str) -> bool {
    TOKEN_PREFIXES.iter().any(|prefix| user.starts_with(prefix))
        || (user.len() >= 32 && user.chars().all(token_char))
}

/// Whether a scheme is SSH (whose user names, such as `git`, are no secret).
fn is_ssh(scheme: &str) -> bool {
    matches!(
        scheme.to_ascii_lowercase().as_str(),
        "ssh" | "git+ssh" | "ssh+git"
    )
}

/// `text` with credentials hidden: the user information of URLs
/// (`https://user:token@host` → `https://***@host`; SSH URLs keep their
/// user name, `ssh://git:***@host`) and access tokens (`ghp_…`,
/// `github_pat_…`, `glpat-…` → `ghp_***`).
pub fn redact(text: &str) -> String {
    redact_tokens(&redact_userinfo(text))
}

fn redact_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let before = &head[..head.len() - 3];
        let scheme_start = before
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || "+-.".contains(c)))
            .map_or(0, |i| i + 1);
        let scheme = &before[scheme_start..];
        let end = tail
            .find(|c: char| c == '/' || c.is_whitespace() || "'\"<>()`".contains(c))
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) => {
                let user = &authority[..at];
                match user.split_once(':') {
                    Some((name, _)) if is_ssh(scheme) => {
                        out.push_str(name);
                        out.push_str(":***");
                    }
                    None if is_ssh(scheme) => out.push_str(user),
                    _ => out.push_str("***"),
                }
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn redact_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous: Option<char> = None;
    let mut rest = text;
    'scan: while let Some(c) = rest.chars().next() {
        if !previous.is_some_and(|p| p.is_ascii_alphanumeric() || p == '_') {
            for prefix in TOKEN_PREFIXES {
                if let Some(after) = rest.strip_prefix(prefix) {
                    let length = after.find(|c| !token_char(c)).unwrap_or(after.len());
                    if length >= 8 {
                        out.push_str(prefix);
                        out.push_str("***");
                        rest = &after[length..];
                        previous = Some('*');
                        continue 'scan;
                    }
                }
            }
        }
        out.push(c);
        previous = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Whether `url` is git's short SSH form `[user@]host:path`: a colon
/// before any slash, and no `://` (on Windows, `C:\…` is a folder).
pub(crate) fn is_scp_like(url: &str) -> bool {
    if url.contains("://") {
        return false;
    }
    let Some(colon) = url.find(':') else {
        return false;
    };
    if url.find(['/', '\\']).is_some_and(|slash| slash < colon) {
        return false;
    }
    let drive = colon == 1 && url.as_bytes()[0].is_ascii_alphabetic();
    !(cfg!(windows) && drive)
}

/// Checks a remote's URL before git sees it, and returns the one to use: a
/// relative folder made absolute. Refused: an empty one, one starting with
/// `-` (git would read an option), remote helpers (`ext::`, `fd::`), a
/// scheme other than ssh, https, http and file (`git://` has neither
/// authentication nor encryption), and credentials in the URL (a password,
/// or a token as the user name): Mitcad never keeps them, git's SSH keys and
/// credential helpers do.
pub fn check_url(url: &str) -> Result<String, RemoteError> {
    let url = url.trim();
    let invalid = |why: &str| {
        RemoteError::new(
            ErrorClass::InvalidUrl,
            format!("'{url}' cannot be used as a remote: {why}"),
        )
    };
    if url.is_empty() {
        return Err(RemoteError::new(
            ErrorClass::InvalidUrl,
            "no address of a remote repository was given",
        ));
    }
    if url.chars().any(char::is_control) {
        return Err(invalid("it holds control characters"));
    }
    if url.starts_with('-') {
        return Err(invalid("it starts with '-'"));
    }
    if let Some(at) = url.find("::")
        && at > 0
        && url[..at]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return Err(invalid(
            "remote helpers such as ext:: run programs and are not used",
        ));
    }
    if let Some(at) = url.find("://") {
        let scheme = url[..at].to_ascii_lowercase();
        match scheme.as_str() {
            "ssh" | "git+ssh" | "ssh+git" | "https" | "http" | "file" => {}
            "git" => {
                return Err(invalid(
                    "git:// has neither authentication nor encryption; use https:// or ssh://",
                ));
            }
            _ => {
                return Err(invalid(&format!(
                    "{scheme}:// is not supported; use https://, ssh:// or a folder"
                )));
            }
        }
        let rest = &url[at + 3..];
        let authority = &rest[..rest.find('/').unwrap_or(rest.len())];
        if let Some((user, _)) = authority.rsplit_once('@') {
            let password = user.contains(':') || user.to_ascii_lowercase().contains("%3a");
            if password || (!is_ssh(&scheme) && looks_like_token(user)) {
                return Err(invalid(
                    "it holds credentials. Mitcad never keeps passwords or tokens: use an \
                     SSH key or a credential helper (Git Credential Manager), and give the \
                     address without them",
                ));
            }
        }
        return Ok(url.to_owned());
    }
    if is_scp_like(url) {
        return Ok(url.to_owned());
    }
    // A folder (a repository on this computer or a network drive).
    Ok(std::path::absolute(url)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| url.to_owned()))
}
