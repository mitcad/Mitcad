// SPDX-License-Identifier: MIT
//! Fetches over `https://` and `http://` without the git program
//! (mitcad#1224): gix's own client (reqwest with rustls, ring as its crypto
//! provider, the system's root certificates) brings the branches and tags
//! of a remote into a bare repository. Component libraries use it, so that
//! they work right after installing MitCad; Cloud projects keep using the
//! installed git ([`super::GitCli`]).
//!
//! It fetches public repositories only: a server that asks for a sign-in
//! gets no credentials (no prompt, no credential helper is run), and the
//! fetch fails as `auth_failed`, saying that the installed git program
//! signs in. The repository is opened with gix's isolated options (none of
//! the user's or the system's git configuration: no URL rewrites, no
//! helpers); proxies come from the environment (`HTTPS_PROXY`,
//! `NO_PROXY`), as reqwest reads them.
//!
//! The transfer runs on a thread of its own while the caller's thread
//! reports its progress to a [`Control`] and watches the cancel request and
//! the idle time; a cancel or a stall stops the pack being received.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use gix::bstr::ByteSlice;
use gix::progress::{Count, Id, MessageLevel, NestedProgress, Progress, Step, StepShared, Unit};
use gix::remote::Direction;

use super::cli::Control;
use super::errors::{ErrorClass, RemoteError, classify, redact};

/// How long a transfer may show no sign of life before it is stopped, as
/// for the git program.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
/// How often the progress, the cancel request and the idle time are read.
const POLL: Duration = Duration::from_millis(50);
/// How long a stopped transfer is waited for before it is left to end on
/// its own (a connection that does not answer ends at its time-out).
const DRAIN: Duration = Duration::from_secs(2);

/// Whether `url` (as [`super::check_url`] returned it) is fetched by the
/// built-in client: `https://` and `http://`.
pub fn supports(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// Whether `url` is an SSH address (`ssh://`, `git+ssh://`, `ssh+git://`,
/// `user@host:path`), which only the git program fetches.
pub fn is_ssh(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["ssh://", "git+ssh://", "ssh+git://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        || super::errors::is_scp_like(url.trim())
}

/// What a fetch brought besides the references.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Transferred {
    /// The branch the remote's HEAD names (`refs/heads/main`), when it
    /// tells.
    pub head: Option<String>,
}

/// Fetches `refspecs` (`+refs/heads/*:refs/incoming/heads/*`) from `url`
/// into the bare repository in `dir`, without tags of their own accord;
/// `what` names the operation in a failure (`Fetching <url>`). A failure is
/// classified as the git program's are (network, not found, sign-in).
pub fn fetch(
    dir: &Path,
    url: &str,
    refspecs: &[String],
    what: &str,
    control: Option<&Control>,
) -> Result<Transferred, RemoteError> {
    if control.is_some_and(Control::is_cancelled) {
        return Err(RemoteError::new(ErrorClass::Cancelled, "cancelled"));
    }
    // reqwest builds its TLS configuration from the process's default
    // provider (its own would be aws-lc-rs, which is not built); ring is
    // the one the live updates use. Installed once; a later call finds it.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let phase = Arc::new(Mutex::new(Phase::default()));
    let interrupt = Arc::new(AtomicBool::new(false));
    let (sender, finished) = mpsc::channel();
    {
        let dir = dir.to_path_buf();
        let url = url.to_owned();
        let refspecs = refspecs.to_vec();
        let reporter = Reporter::new(Arc::clone(&phase));
        let interrupt = Arc::clone(&interrupt);
        // Not joined when it is stopped: a connection that does not answer
        // ends on its own, and what it leaves (a pack being written, refs
        // under refs/incoming/) is cleaned up by the next fetch.
        let _worker = std::thread::Builder::new()
            .name("library fetch".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    transfer(&dir, &url, &refspecs, reporter, &interrupt)
                }))
                .unwrap_or_else(|_| Err("the built-in transfer stopped unexpectedly".to_owned()));
                let _ = sender.send(result);
            })
            .map_err(|e| {
                RemoteError::new(
                    ErrorClass::Other,
                    format!("{what} failed: cannot start the transfer: {e}"),
                )
            })?;
    }

    let mut last: (String, Step) = (String::new(), 0);
    let mut last_sign = Instant::now();
    let mut stopped: Option<(ErrorClass, Instant)> = None;
    let result = loop {
        match finished.recv_timeout(POLL) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("the built-in transfer stopped unexpectedly".to_owned());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let (name, max, step) = {
            let shown = phase.lock().unwrap_or_else(|e| e.into_inner());
            (
                shown.name.clone(),
                shown.max,
                shown.step.load(Ordering::Relaxed),
            )
        };
        if (name.as_str(), step) != (last.0.as_str(), last.1) {
            last_sign = Instant::now();
            if let Some(control) = control
                && !name.is_empty()
            {
                control.report(&phase_text(&name), percent(step, max));
            }
            last = (name, step);
        }
        match stopped {
            None => {
                if control.is_some_and(Control::is_cancelled) {
                    interrupt.store(true, Ordering::SeqCst);
                    stopped = Some((ErrorClass::Cancelled, Instant::now()));
                } else if last_sign.elapsed() > IDLE_TIMEOUT {
                    interrupt.store(true, Ordering::SeqCst);
                    stopped = Some((ErrorClass::TimedOut, Instant::now()));
                }
            }
            Some((_, at)) if at.elapsed() > DRAIN => break Err(String::new()),
            Some(_) => {}
        }
    };
    match (result, stopped) {
        (_, Some((ErrorClass::Cancelled, _))) => {
            Err(RemoteError::new(ErrorClass::Cancelled, "cancelled"))
        }
        (_, Some((class, _))) => Err(RemoteError::new(
            class,
            format!(
                "{what} showed no progress for {} s and was stopped",
                IDLE_TIMEOUT.as_secs()
            ),
        )),
        (Ok(transferred), None) => Ok(transferred),
        (Err(text), None) => Err(failure(what, &text)),
    }
}

/// The fetch itself, on its own thread: the error as text with its causes.
fn transfer(
    dir: &Path,
    url: &str,
    refspecs: &[String],
    progress: Reporter,
    interrupt: &AtomicBool,
) -> Result<Transferred, String> {
    receive(dir, url, refspecs, progress, interrupt).map_err(|e| crate::chain(&e))
}

/// Connects, lists the remote's references and receives the pack.
fn receive(
    dir: &Path,
    url: &str,
    refspecs: &[String],
    mut progress: Reporter,
    interrupt: &AtomicBool,
) -> Result<Transferred, gix::Error> {
    let repo = gix::open_opts(dir, gix::open::Options::isolated())?;
    let remote = repo
        .remote_at_without_url_rewrite(url)?
        // Tags only by the refspecs: tag following would write them to
        // refs/tags/ at once.
        .with_fetch_tags(gix::remote::fetch::Tags::None)
        .with_refspecs(refspecs.iter().map(String::as_str), Direction::Fetch)?;
    // No credentials: no prompt, no helper (they would start the git
    // program); a server that asks for a sign-in refuses the fetch.
    let connection = remote
        .connect(Direction::Fetch)?
        .with_credentials(|_| Ok(None));
    // Every reference, HEAD with its branch among them: the remote's
    // default branch becomes a new clone's HEAD.
    let options = gix::remote::ref_map::Options {
        prefix_from_spec_as_filter_on_remote: false,
        ..Default::default()
    };
    let prepared = connection.prepare_fetch(progress.add_child("Listing references"), options)?;
    let head = remote_head(&prepared.ref_map().remote_refs);
    prepared.receive(progress, interrupt)?;
    Ok(Transferred { head })
}

/// The branch the remote's HEAD names, if it names one.
fn remote_head(refs: &[gix::protocol::handshake::Ref]) -> Option<String> {
    use gix::protocol::handshake::Ref;
    refs.iter().find_map(|reference| match reference {
        Ref::Symbolic {
            full_ref_name,
            target,
            ..
        }
        | Ref::Unborn {
            full_ref_name,
            target,
            ..
        } if full_ref_name.to_str_lossy() == "HEAD" => Some(target.to_str_lossy().into_owned()),
        _ => None,
    })
}

/// A failure of the built-in transfer: classified from its text (gix's and
/// reqwest's messages), with what to do.
pub(crate) fn failure(what: &str, text: &str) -> RemoteError {
    let detail = redact(text.trim());
    let lower = detail.to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    let class = if any(&[
        "http status 401",
        "http status 403",
        "no credentials were returned",
        "were not accepted by the remote",
    ]) {
        ErrorClass::AuthFailed
    } else if any(&["http status 404", "http status 410"]) {
        ErrorClass::NotFound
    } else if any(&[
        "dns error",
        "error trying to connect",
        "tcp connect error",
        "error sending request",
        "connection refused",
        "connection reset",
        "timed out",
        "certificate",
        "tls handshake",
        "handshake failure",
    ]) {
        ErrorClass::Network
    } else {
        classify(&detail)
    };
    let why = match class {
        ErrorClass::AuthFailed => "the server asks for a sign-in. MitCad fetches public libraries \
             over https:// by itself; for a library that needs a sign-in, turn on Use the \
             installed git program (Tools > Libraries...): git signs in with its credential \
             helper."
            .to_owned(),
        ErrorClass::NotFound => {
            "there is no repository at that address, or it is not public.".to_owned()
        }
        ErrorClass::Network if lower.contains("certificate") => "the server's certificate \
             could not be verified with this computer's root certificates."
            .to_owned(),
        ErrorClass::Network => "the server cannot be reached. Check the network connection \
             and the address."
            .to_owned(),
        _ => detail
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| "the transfer failed.".to_owned(), str::to_owned),
    };
    RemoteError {
        class,
        message: redact(&format!("{what} failed: {why}")),
        detail,
    }
}

/// A phase's name for people: `Receiving pack` from gix's `receiving pack`.
fn phase_text(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A count's percentage of its maximum, when it has one.
fn percent(step: Step, max: Option<Step>) -> Option<u8> {
    let max = max.filter(|max| *max > 0)?;
    u8::try_from((step.min(max) * 100) / max).ok()
}

/// The phase a transfer is in: the last progress that was named or given
/// a maximum, with its counter.
#[derive(Default)]
struct Phase {
    name: String,
    max: Option<Step>,
    step: StepShared,
}

/// gix's progress for a transfer: each node has its own name, maximum and
/// counter and makes itself the current [`Phase`] when it starts one.
struct Reporter {
    phase: Arc<Mutex<Phase>>,
    name: String,
    max: Option<Step>,
    step: StepShared,
}

impl Reporter {
    fn new(phase: Arc<Mutex<Phase>>) -> Self {
        Self {
            phase,
            name: String::new(),
            max: None,
            step: StepShared::default(),
        }
    }

    fn child(&self, name: String) -> Self {
        let child = Self {
            phase: Arc::clone(&self.phase),
            name,
            max: None,
            step: StepShared::default(),
        };
        child.publish();
        child
    }

    fn publish(&self) {
        if self.name.is_empty() {
            return;
        }
        let mut phase = self.phase.lock().unwrap_or_else(|e| e.into_inner());
        phase.name.clone_from(&self.name);
        phase.max = self.max;
        phase.step = Arc::clone(&self.step);
    }
}

impl Count for Reporter {
    fn set(&self, step: Step) {
        self.step.store(step, Ordering::Relaxed);
    }

    fn step(&self) -> Step {
        self.step.load(Ordering::Relaxed)
    }

    fn inc_by(&self, step: Step) {
        self.step.fetch_add(step, Ordering::Relaxed);
    }

    fn counter(&self) -> StepShared {
        Arc::clone(&self.step)
    }
}

impl Progress for Reporter {
    fn init(&mut self, max: Option<Step>, _unit: Option<Unit>) {
        self.max = max;
        self.publish();
    }

    fn max(&self) -> Option<Step> {
        self.max
    }

    fn set_max(&mut self, max: Option<Step>) -> Option<Step> {
        let previous = self.max;
        self.max = max;
        self.publish();
        previous
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
        self.publish();
    }

    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn id(&self) -> Id {
        gix::progress::UNKNOWN
    }

    fn message(&self, _level: MessageLevel, _message: String) {}
}

impl NestedProgress for Reporter {
    type SubProgress = Reporter;

    fn add_child(&mut self, name: impl Into<String>) -> Self::SubProgress {
        self.child(name.into())
    }

    fn add_child_with_id(&mut self, name: impl Into<String>, _id: Id) -> Self::SubProgress {
        self.child(name.into())
    }
}

/// A new bare repository for a clone in `dir` (which must not exist or be
/// empty), made with gix's isolated options, with `remote.origin.url` set
/// to `url` as `git clone --bare` sets it.
pub fn init_bare(dir: &Path, url: &str) -> Result<(), RemoteError> {
    let made = gix::ThreadSafeRepository::init_opts(
        dir,
        gix::create::Kind::Bare,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    );
    if let Err(error) = made {
        return Err(RemoteError::new(
            ErrorClass::Other,
            format!(
                "cannot make a repository in {}: {}",
                dir.display(),
                crate::chain(&error)
            ),
        ));
    }
    let config: PathBuf = dir.join("config");
    let mut text = std::fs::read_to_string(&config).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str("[remote \"origin\"]\n\turl = ");
    text.push_str(&config_value(url));
    text.push('\n');
    std::fs::write(&config, text).map_err(|e| {
        RemoteError::new(
            ErrorClass::Other,
            format!("cannot write {}: {e}", config.display()),
        )
    })
}

/// A value for git's configuration file: as it is, or quoted with `"` and
/// `\` escaped when it has a character the file would read otherwise
/// (`#` and `;` begin a comment).
fn config_value(value: &str) -> String {
    let plain =
        !value.is_empty() && value.trim() == value && !value.contains(['"', '\\', '#', ';']);
    if plain {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_routed_by_their_scheme() {
        assert!(supports("https://github.com/mitcad/parts.git"));
        assert!(supports("HTTP://127.0.0.1:8080/pins.git"));
        assert!(!supports("ssh://git@example.org/pins.git"));
        assert!(!supports("/home/user/pins"));
        assert!(!supports("file:///home/user/pins"));
        assert!(is_ssh("ssh://git@example.org/pins.git"));
        assert!(is_ssh("git+ssh://example.org/pins.git"));
        assert!(is_ssh("git@example.org:mitcad/pins.git"));
        assert!(!is_ssh("https://example.org/pins.git"));
        assert!(!is_ssh("/home/user/pins"));
    }

    #[test]
    fn failures_are_told_apart_plainly() {
        let refused = failure(
            "Fetching http://127.0.0.1:9/x.git",
            "An IO error occurred when talking to the server: error sending request for url \
             (http://127.0.0.1:9/x.git/info/refs?service=git-upload-pack): client error \
             (Connect): tcp connect error: Connection refused (os error 111)",
        );
        assert_eq!(refused.class, ErrorClass::Network, "{refused:?}");
        assert!(refused.message.contains("cannot be reached"), "{refused:?}");
        let missing = failure("Fetching x", "Received HTTP status 404");
        assert_eq!(missing.class, ErrorClass::NotFound);
        let private = failure(
            "Fetching x",
            "No credentials were returned at all as if the credential helper isn't functioning",
        );
        assert_eq!(private.class, ErrorClass::AuthFailed);
        assert!(private.message.contains("Use the installed git program"));
        let secret = failure("Fetching x", "https://user:token@example.org failed");
        assert!(!secret.detail.contains("token"), "{secret:?}");
    }

    #[test]
    fn progress_is_reported_by_phase() {
        assert_eq!(phase_text("receiving pack"), "Receiving pack");
        assert_eq!(percent(5, Some(10)), Some(50));
        assert_eq!(percent(15, Some(10)), Some(100));
        assert_eq!(percent(5, None), None);
        assert_eq!(percent(5, Some(0)), None);
        let phase = Arc::new(Mutex::new(Phase::default()));
        let mut root = Reporter::new(Arc::clone(&phase));
        let mut child = root.add_child("indexing");
        child.init(Some(4), None);
        child.inc_by(3);
        let shown = phase.lock().unwrap();
        assert_eq!(shown.name, "indexing");
        assert_eq!(shown.max, Some(4));
        assert_eq!(shown.step.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn configuration_values_are_quoted() {
        assert_eq!(config_value("https://x/a.git"), "https://x/a.git");
        assert_eq!(config_value("https://x/a.git#v1"), "\"https://x/a.git#v1\"");
        assert_eq!(config_value("C:\\lib \"a\""), "\"C:\\\\lib \\\"a\\\"\"");
    }
}
