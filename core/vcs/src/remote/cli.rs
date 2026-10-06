// SPDX-License-Identifier: MIT
//! The system's git program, which does Mitcad's network work (fetch,
//! push, clone, `ls-remote`) and updates the work tree after one; objects,
//! trees and commits stay with gix.
//!
//! The program is `MITCAD_GIT` when set (the application sets it from
//! Preferences), else the first `git` on `PATH` (absolute folders only, so
//! a project's own folder never provides one), and on Windows also Git for
//! Windows where it installs itself. It runs without prompts: stdin is
//! closed, `GIT_TERMINAL_PROMPT=0`, on Windows without a console window,
//! and elsewhere in a session of its own, without the terminal Mitcad may
//! have been started from (ssh asks for a key's passphrase or a host key on
//! the terminal, not on stdin), so missing credentials fail at once instead
//! of waiting for an answer nobody sees. Its messages are English (`LC_ALL=C`) so that
//! failures can be classified (`errors.rs`). Only ssh, https, http and
//! local repositories are allowed (`GIT_ALLOW_PROTOCOL`). git converts no
//! line endings of its own accord (`core.autocrlf=false`, `core.eol=lf`,
//! whatever its configuration says: Git for Windows sets `autocrlf`), so
//! the files a clone, a fast-forward or a sync writes are the bytes Mitcad
//! recorded (gix runs no filters); the project's `.gitattributes` still
//! apply. A run can be cancelled and stops when git shows no sign of life
//! for a while; git's progress lines (`Receiving objects:  45% (9/20)`)
//! become the progress of a [`Control`].

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::errors::{ErrorClass, RemoteError, redact};

/// The oldest git that Mitcad's remote work is tested with; an older one is
/// reported as not supported.
pub const MIN_VERSION: (u32, u32) = (2, 34);
/// How long git may show no sign of life (no output, no progress) before
/// it is stopped: long enough for a sign-in window of a credential helper.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// How often a run looks at the process and at the cancel request.
const POLL: Duration = Duration::from_millis(20);
/// How long the output of a finished or stopped git is still read: a child
/// it started (ssh) may hold the pipes a little longer.
const DRAIN: Duration = Duration::from_secs(2);
/// The configuration every run has, before git's command: files are
/// checked out as recorded, without line endings converted unless the
/// project's `.gitattributes` asks for it (Mitcad records the bytes it
/// saves; a checkout in CRLF would differ from the version).
const CONFIGURATION: [&str; 4] = ["-c", "core.autocrlf=false", "-c", "core.eol=lf"];
/// Variables that point git at another repository than the project's (set
/// when Mitcad runs from a git hook, for example).
const REPOSITORY_VARIABLES: [&str; 8] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// The progress of a remote operation and its cancel request, shared by
/// the thread that runs it and the one that shows it.
#[derive(Debug, Default)]
pub struct Control {
    cancelled: AtomicBool,
    progress: Mutex<Progress>,
}

/// What a remote operation is doing: git's phase (`Receiving objects`)
/// and its percentage when git gives one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub text: String,
    pub percent: Option<u8>,
}

impl Control {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks the operation to stop: git is ended, and the operation fails
    /// as cancelled. A control stays cancelled.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn progress(&self) -> Progress {
        self.progress.lock().expect("not poisoned").clone()
    }

    pub(crate) fn report(&self, text: &str, percent: Option<u8>) {
        *self.progress.lock().expect("not poisoned") = Progress {
            text: text.to_owned(),
            percent,
        };
    }
}

/// git's version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitVersion {
    /// As git gives it, without `git version` (`2.45.1.windows.1`).
    pub text: String,
    pub major: u32,
    pub minor: u32,
}

impl GitVersion {
    /// Reads `git --version` (`git version 2.43.0`).
    pub fn parse(output: &str) -> Option<Self> {
        let text = output.trim().strip_prefix("git version ")?.trim();
        let mut numbers = text.split('.').map(|part| {
            let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<u32>().ok()
        });
        let major = numbers.next()??;
        let minor = numbers.next()??;
        Some(Self {
            text: text.to_owned(),
            major,
            minor,
        })
    }

    /// Whether it is [`MIN_VERSION`] or newer.
    pub fn supported(&self) -> bool {
        (self.major, self.minor) >= MIN_VERSION
    }
}

/// What a git run printed and how it ended.
#[derive(Debug, Clone)]
pub(crate) struct GitOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    /// The lines of the error output; progress updates (lines ended by a
    /// carriage return) are left out.
    pub stderr: String,
}

/// The git program and how it is run.
#[derive(Debug, Clone)]
pub struct GitCli {
    program: PathBuf,
    /// More environment for git (tests: a fake SSH command).
    env: Vec<(OsString, OsString)>,
    idle_timeout: Duration,
}

impl GitCli {
    /// The git program: `MITCAD_GIT`, else `PATH`, else (Windows) where Git
    /// for Windows installs itself.
    pub fn find() -> Result<Self, RemoteError> {
        locate(std::env::var_os("MITCAD_GIT"), std::env::var_os("PATH")).map(Self::at)
    }

    /// The git program at `program`.
    pub fn at(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            env: Vec::new(),
            idle_timeout: IDLE_TIMEOUT,
        }
    }

    pub fn program(&self) -> &Path {
        &self.program
    }

    /// git runs with this variable set too.
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// git is stopped after showing no sign of life for this long (default
    /// 5 minutes).
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = timeout;
        self
    }

    /// `git --version`.
    pub fn version(&self) -> Result<GitVersion, RemoteError> {
        let output = self.run(None, &["--version"], None)?;
        GitVersion::parse(&output.stdout).ok_or_else(|| {
            RemoteError::new(
                ErrorClass::GitMissing,
                format!(
                    "{} is not a git program: it answered '{}' to --version",
                    self.program.display(),
                    output.stdout.trim()
                ),
            )
        })
    }

    /// The version of git-lfs (`3.4.1`), or None without it.
    pub fn lfs_version(&self) -> Option<String> {
        let output = self.run(None, &["lfs", "version"], None).ok()?;
        let version = output.stdout.trim().strip_prefix("git-lfs/")?;
        let version = version.split_whitespace().next()?;
        output.success.then(|| version.to_owned())
    }

    /// Runs git with `args` in `dir` (None: the current folder). A failed
    /// command is an output that is not a success; an error is git that
    /// could not be started, was cancelled or timed out.
    pub(crate) fn run(
        &self,
        dir: Option<&Path>,
        args: &[&str],
        control: Option<&Control>,
    ) -> Result<GitOutput, RemoteError> {
        if control.is_some_and(Control::is_cancelled) {
            return Err(cancelled());
        }
        let mut command = Command::new(&self.program);
        command
            .args(CONFIGURATION)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("LANGUAGE", "")
            .env("GIT_ALLOW_PROTOCOL", "ssh:https:http:file");
        for variable in REPOSITORY_VARIABLES {
            command.env_remove(variable);
        }
        command.envs(self.env.iter().map(|(k, v)| (k, v)));
        if let Some(dir) = dir {
            command.current_dir(dir);
        }
        #[cfg(windows)]
        {
            // No console window flashes up for git (or the ssh it starts)
            // from the application.
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        {
            // A session of its own: no controlling terminal, so the ssh git
            // starts cannot ask on the terminal Mitcad was started from
            // (where nobody looks) and fails instead, or asks with the
            // desktop's askpass program; and its process group is git's,
            // which a cancel ends as a whole (stop).
            use std::os::unix::process::CommandExt;
            // SAFETY: the closure runs in the child between fork and exec
            // and only calls setsid(), which is async-signal-safe.
            unsafe {
                command.pre_exec(|| {
                    libc::setsid();
                    Ok(())
                });
            }
        }
        let mut child = command.spawn().map_err(|e| {
            RemoteError::new(
                ErrorClass::GitMissing,
                format!("cannot run git ({}): {e}", self.program.display()),
            )
        })?;
        let (sender, events) = mpsc::channel();
        read_in_background(child.stdout.take().expect("piped"), Stream::Out, &sender);
        read_in_background(child.stderr.take().expect("piped"), Stream::Err, &sender);
        drop(sender);

        let mut stdout = Vec::new();
        let mut stderr = ErrorOutput::default();
        let mut open = 2;
        let mut last_sign = Instant::now();
        let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
        loop {
            match events.recv_timeout(POLL) {
                Ok(Event::Data(Stream::Out, bytes)) => {
                    stdout.extend_from_slice(&bytes);
                    last_sign = Instant::now();
                }
                Ok(Event::Data(Stream::Err, bytes)) => {
                    stderr.feed(&bytes, control);
                    last_sign = Instant::now();
                }
                Ok(Event::Closed) => open -= 1,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => open = 0,
            }
            if exited.is_none() {
                let status = child.try_wait().map_err(|e| {
                    RemoteError::new(ErrorClass::Other, format!("cannot wait for git: {e}"))
                })?;
                exited = status.map(|status| (status, Instant::now()));
            }
            match exited {
                Some((_, at)) if open == 0 || at.elapsed() > DRAIN => break,
                Some(_) => {}
                None if control.is_some_and(Control::is_cancelled) => {
                    stop(&mut child);
                    return Err(cancelled());
                }
                None if last_sign.elapsed() > self.idle_timeout => {
                    stop(&mut child);
                    let mut error = RemoteError::new(
                        ErrorClass::TimedOut,
                        format!(
                            "git {} showed no progress for {} s and was stopped",
                            args.first().copied().unwrap_or_default(),
                            self.idle_timeout.as_secs()
                        ),
                    );
                    error.detail = redact(stderr.text.trim());
                    return Err(error);
                }
                None => {}
            }
        }
        let (status, _) = exited.expect("exited");
        stderr.finish();
        Ok(GitOutput {
            success: status.success(),
            code: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: stderr.text,
        })
    }
}

fn cancelled() -> RemoteError {
    RemoteError::new(ErrorClass::Cancelled, "cancelled")
}

/// Ends git. On Windows the `git.exe` of Git for Windows' `cmd` folder
/// starts the real git as a child, so the whole tree is ended; elsewhere
/// git leads a process group of its own (a session, `run`), which is ended
/// with the children git started (ssh, a remote helper).
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // SAFETY: kill() with the negative id of the group git leads.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let system = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from("C:\\Windows"), PathBuf::from);
        let _ = Command::new(system.join("System32").join("taskkill.exe"))
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Debug, Clone, Copy)]
enum Stream {
    Out,
    Err,
}

enum Event {
    Data(Stream, Vec<u8>),
    Closed,
}

/// Reads a pipe on a thread of its own until it closes. The thread is not
/// joined: a child of git that outlives it may keep the pipe open a while.
fn read_in_background(
    mut pipe: impl Read + Send + 'static,
    stream: Stream,
    sender: &mpsc::Sender<Event>,
) {
    let sender = sender.clone();
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if sender
                        .send(Event::Data(stream, buffer[..n].to_vec()))
                        .is_err()
                    {
                        return;
                    }
                }
            }
        }
        let _ = sender.send(Event::Closed);
    });
}

/// git's error output: its lines, and its progress updates reported to a
/// control.
#[derive(Default)]
struct ErrorOutput {
    text: String,
    pending: Vec<u8>,
}

impl ErrorOutput {
    fn feed(&mut self, bytes: &[u8], control: Option<&Control>) {
        for &byte in bytes {
            if byte == b'\n' || byte == b'\r' {
                let line = String::from_utf8_lossy(&self.pending).into_owned();
                self.pending.clear();
                if let Some(control) = control
                    && let Some((text, percent)) = progress_of(&line)
                {
                    control.report(&text, percent);
                }
                if byte == b'\n' {
                    self.text.push_str(&line);
                    self.text.push('\n');
                }
            } else {
                self.pending.push(byte);
            }
        }
    }

    fn finish(&mut self) {
        if !self.pending.is_empty() {
            self.text.push_str(&String::from_utf8_lossy(&self.pending));
            self.pending.clear();
        }
    }
}

/// The phase and percentage of a progress line of git
/// (`remote: Counting objects: 100% (5/5), done.`, `Receiving objects:  45%
/// (9/20)`), None for another line.
pub(crate) fn progress_of(line: &str) -> Option<(String, Option<u8>)> {
    let line = line.trim();
    let line = line.strip_prefix("remote:").map_or(line, str::trim);
    let (phase, rest) = line.split_once(':')?;
    let phase = phase.trim();
    if phase.is_empty()
        || !phase
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-')
    {
        return None;
    }
    let percent = rest.split_once('%').and_then(|(number, _)| {
        number
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|percent| *percent <= 100)
    });
    let counted = percent.is_some() || rest.trim_start().starts_with(|c: char| c.is_ascii_digit());
    counted.then(|| (phase.to_owned(), percent))
}

/// Finds the git program: `setting` (`MITCAD_GIT`) when given, else the
/// first `git` in the absolute folders of `path`, else (Windows) Git for
/// Windows in its usual places.
pub(crate) fn locate(
    setting: Option<OsString>,
    path: Option<OsString>,
) -> Result<PathBuf, RemoteError> {
    if let Some(setting) = setting.filter(|s| !s.is_empty()) {
        let program = PathBuf::from(&setting);
        return if program.is_file() {
            Ok(program)
        } else {
            Err(RemoteError::new(
                ErrorClass::GitMissing,
                format!(
                    "the git program set for Mitcad (MITCAD_GIT) is not there: {}",
                    program.display()
                ),
            ))
        };
    }
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let mut candidates: Vec<PathBuf> = path
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(windows) {
        for (variable, folder) in [
            ("ProgramFiles", "Git\\cmd"),
            ("ProgramW6432", "Git\\cmd"),
            ("LOCALAPPDATA", "Programs\\Git\\cmd"),
        ] {
            if let Some(base) = std::env::var_os(variable) {
                candidates.push(PathBuf::from(base).join(folder));
            }
        }
    }
    candidates
        .into_iter()
        .filter(|folder| folder.is_absolute())
        .map(|folder| folder.join(name))
        .find(|program| program.is_file())
        .ok_or_else(|| {
            RemoteError::new(
                ErrorClass::GitMissing,
                "git is not installed: remote repositories need the git program (on Windows, \
                 Git for Windows from https://git-scm.com). Set its path in MITCAD_GIT when it \
                 is not on PATH. The local version history works without it.",
            )
        })
}
