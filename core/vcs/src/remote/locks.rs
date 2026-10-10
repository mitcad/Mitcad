// SPDX-License-Identifier: MIT
//! Edit locks (mitcad#89): in a Cloud project a design has one editor at a
//! time. The lock is kept on the remote as a git ref outside the branches,
//! so it works with any git host that accepts such refs; Mitcad honours it,
//! other git tools do not see it.
//!
//! - **Where**: `refs/mitcad/locks/<id>` per locked file, `<id>` the SHA-256
//!   of its path in the project, pointing to a parentless commit whose tree
//!   holds `lock.json` (authored by the holder). Requests are refs of their
//!   own, `refs/mitcad/lock-requests/<id>/<session>`, holding `request.json`,
//!   so a requester never races the holder's updates. The contents and
//!   their checks are in `format.rs`.
//! - **Writes** are compare-and-swap pushes (`git push
//!   --force-with-lease=<ref>:<expected>`, empty: the ref must not exist):
//!   of two who take a free lock at once one succeeds, and a takeover of a
//!   stale lock fails when its holder refreshed it meanwhile. A release is
//!   a deletion with the same lease. A push the remote refuses is told apart
//!   from a lost race by listing the ref again.
//! - **Reading**: one `git ls-remote <remote> refs/mitcad/locks/*
//!   refs/mitcad/lock-requests/* refs/heads/<branch>` per poll and project;
//!   when a ref changed to an object the project lacks, the lock refs are
//!   fetched (`--prune`) into `refs/mitcad/remote-locks/…` and read with gix.
//!   The branch head of the same listing tells of newer versions at once.
//!   Plain fetches, pushes and Sync name only the branch, so they never
//!   fetch or push lock refs, and lock commits are in no version's history.
//! - **Stale locks** are judged on this process's monotonic clock, never by
//!   comparing another computer's clock: a lock ref seen unchanged for its
//!   `idle_minutes` + 2 × `poll_seconds`, a request of this session without
//!   a receipt within 2 × the holder's `poll_seconds` (the project's poll
//!   interval when both listen to the broker), or with a receipt but no
//!   answer within the idle time. What each poll saw is kept per project
//!   in the process, shared by every `ProjectRepo` of the same folder (the
//!   application opens one per task).
//! - **Stale requests** (a requester's Mitcad killed): a waiting request is
//!   written again by its requester's polls every half idle time, as a
//!   refresh of itself (`refresh_of`: its id stays, and with it the
//!   receipt and the answer); another session's request seen unchanged for
//!   the idle time and two of the requester's polls is stale, gets no
//!   receipt and is removed by whoever's poll sees it
//!   (`ProjectRepo::tidy_requests`).
//!
//! The JSON commands are in `commands.rs` (`core/model/src/api/commands.md`,
//! "Edit locks").

pub(crate) mod commands;
mod format;

pub use format::{
    Answer, AnswerKind, FORMAT_VERSION, LOCAL_PREFIX, LOCK_FILE, LOCK_FORMAT, LOCKS_PREFIX, Lock,
    LockRefName, LockState, MAX_EMAIL, MAX_FILE_SIZE, MAX_KEEP_MINUTES, MAX_MESSAGE, MAX_NAME,
    MAX_REQUEST_IDS, Person, Previous, REQUEST_FILE, REQUEST_FORMAT, REQUESTS_PREFIX, Request,
    TakenBecause, Unreadable, clean_limited, clean_text, command_line_session, format_time,
    is_commit_id, is_lock_id, is_project_path, is_session, local_ref, lock_id, lock_ref,
    parse_time, read_lock, read_request, request_ref, session_of,
};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;
use gix::refs::Target;
use gix::refs::transaction::{Change, LogChange, PreviousValue, RefEdit, RefLog};
use mitcad_model::file::settings::{EditLocks, ProjectSettings};
use serde_json::{Value, json};

use super::{Control, ErrorClass, RemoteError, failure, log_line, reference_id};
use crate::{ProjectRepo, VcsError};

/// A lock or request commit larger than this is not read.
const MAX_COMMIT_SIZE: u64 = 64 * 1024;
/// A lock or request commit's tree larger than this is not read.
const MAX_TREE_SIZE: u64 = 4 * 1024;
/// How often a write is tried again when another `ProjectRepo` of this
/// session wrote the same lock meanwhile.
const WRITE_ATTEMPTS: usize = 3;
/// The variable that speeds up the lock clock for the application's tests
/// (a factor, 1 when unset).
pub const TIME_SCALE_VARIABLE: &str = "MITCAD_LOCK_TIME_SCALE";

// The clock.

/// The monotonic clock of the lock observations: the time since the first
/// reading, times `MITCAD_LOCK_TIME_SCALE`.
fn monotonic() -> Duration {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    static SCALE: OnceLock<f64> = OnceLock::new();
    let scale = *SCALE.get_or_init(|| {
        std::env::var(TIME_SCALE_VARIABLE)
            .ok()
            .and_then(|text| text.trim().parse::<f64>().ok())
            .filter(|scale| scale.is_finite() && *scale >= 1.0 && *scale <= 10_000.0)
            .unwrap_or(1.0)
    });
    EPOCH.get_or_init(Instant::now).elapsed().mul_f64(scale)
}

/// Seconds since 1970 (UTC) by this computer's clock: what is written in
/// locks and shown, never compared with another computer's.
fn now_utc() -> i64 {
    gix::date::Time::now_utc().seconds
}

// What this process saw of each project's lock refs.

/// A lock or request as read: its contents, or why they could not be.
#[derive(Debug, Clone)]
enum Contents<T> {
    Read(T),
    Unreadable(Unreadable),
}

impl<T> Contents<T> {
    fn get(&self) -> Option<&T> {
        match self {
            Self::Read(value) => Some(value),
            Self::Unreadable(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
struct SeenLock {
    oid: ObjectId,
    /// When this process first saw the ref at `oid`.
    since: Duration,
    contents: Contents<Lock>,
    /// The holder's session, and when this process first saw that session
    /// hold the lock.
    holder: Option<String>,
    holder_since: Duration,
}

#[derive(Debug, Clone)]
struct SeenRequest {
    /// The ref's commit, and when this process first saw it there.
    oid: ObjectId,
    since: Duration,
    /// The request's id: the commit it was first written as (a refresh
    /// names it in `refresh_of`), which receipts and answers name.
    id: ObjectId,
    /// The order in which this process first saw the requests: the order
    /// they are served in (a refresh keeps its place), and when.
    order: u64,
    first_seen: Duration,
    contents: Contents<Request>,
}

impl SeenRequest {
    /// The ref's commit `oid` with `contents`, seen at `now`: its place
    /// that of `old` when it is a refresh of the same request, else the
    /// next one.
    fn new(
        oid: ObjectId,
        contents: Contents<Request>,
        old: Option<&SeenRequest>,
        next_order: &mut u64,
        now: Duration,
    ) -> Self {
        let id = contents
            .get()
            .and_then(|request| request.refresh_of.as_deref())
            .and_then(|id| ObjectId::from_hex(id.as_bytes()).ok())
            .unwrap_or(oid);
        let (order, first_seen) = match old {
            Some(old) if old.id == id => (old.order, old.first_seen),
            _ => {
                *next_order += 1;
                (*next_order, now)
            }
        };
        Self {
            oid,
            since: now,
            id,
            order,
            first_seen,
            contents,
        }
    }

    /// When the request is stale for others (mitcad#89), as a lock is:
    /// its ref seen unchanged for the project's idle time and two of the
    /// requester's polls (the project's poll interval when it names none),
    /// the requester refreshing it every half idle time while it waits.
    fn stale_at(&self, project: &EditLocks) -> Duration {
        let poll = self
            .contents
            .get()
            .and_then(|request| request.poll_seconds)
            .unwrap_or(project.poll_seconds);
        self.since + Duration::from_secs(u64::from(project.idle_minutes) * 60 + 2 * u64::from(poll))
    }

    /// Whether this request is stale for session `me` (never its own).
    fn stale_for(&self, session: &str, me: &Me, project: &EditLocks, now: Duration) -> bool {
        session != me.session && now >= self.stale_at(project)
    }
}

/// How often the requester refreshes its waiting request: every half of the
/// project's idle time, as a holder refreshes its lock.
fn request_refresh_interval(project: &EditLocks) -> Duration {
    Duration::from_secs(u64::from(project.idle_minutes) * 30)
}

/// A request of a session of this process, as its requester follows it.
#[derive(Debug, Clone)]
struct Asked {
    /// The request's commit (its id).
    request: ObjectId,
    at: Duration,
    /// The holder whose receipt and answer are below.
    holder: Option<String>,
    receipt_at: Option<Duration>,
    /// When the holder's answer was first seen, and for a "keep" how long
    /// it keeps the lock (`until` minus the same lock's `refreshed_at`).
    answer_at: Option<Duration>,
    answer: Option<AnswerKind>,
    /// The answer's `until` (the holder's clock: only compared with itself).
    until: Option<i64>,
    keep: Duration,
}

/// A test's hook, called before each write.
#[cfg(test)]
pub(crate) type WriteHook = Box<dyn FnMut() + Send>;

/// What this process knows of a project's lock refs.
#[derive(Default)]
struct Tracked {
    /// By lock id.
    locks: BTreeMap<String, SeenLock>,
    /// By (lock id, session).
    requests: BTreeMap<(String, String), SeenRequest>,
    next_order: u64,
    /// The requests of this process's sessions, by (lock id, session).
    asked: BTreeMap<(String, String), Asked>,
    /// The locks this process's sessions hold, by (session, lock id): the
    /// commit last written and the file's path.
    held: BTreeMap<(String, String), (ObjectId, String)>,
    /// Malformed lock and request commits seen (each counted once).
    malformed: BTreeSet<ObjectId>,
    /// The remote branch's commit in the last listing.
    head: Option<ObjectId>,
    polled: bool,
    #[cfg(test)]
    clock: Option<std::sync::Arc<std::sync::atomic::AtomicU64>>,
    #[cfg(test)]
    before_write: Option<WriteHook>,
}

impl Tracked {
    fn now(&self) -> Duration {
        #[cfg(test)]
        if let Some(clock) = &self.clock {
            return Duration::from_millis(clock.load(std::sync::atomic::Ordering::SeqCst));
        }
        monotonic()
    }

    /// The requests for lock `id`, in the order they are served.
    fn requests_of(&self, id: &str) -> Vec<(&String, &SeenRequest)> {
        let mut list: Vec<(&String, &SeenRequest)> = self
            .requests
            .iter()
            .filter(|((lock, _), _)| lock == id)
            .map(|((_, session), seen)| (session, seen))
            .collect();
        list.sort_by_key(|(session, seen)| {
            (
                seen.order,
                seen.contents.get().map_or(i64::MAX, |r| r.asked_at),
                (*session).clone(),
            )
        });
        list
    }

    /// The ids of the requests for lock `id` that its holder (session
    /// `holder`) gives receipts for: readable, of other sessions, not stale.
    fn receipts_for(&self, id: &str, holder: &str, project: &EditLocks) -> Vec<String> {
        let now = self.now();
        self.requests_of(id)
            .into_iter()
            .filter(|(session, seen)| {
                session.as_str() != holder
                    && seen.contents.get().is_some()
                    && now < seen.stale_at(project)
            })
            .map(|(_, seen)| seen.id.to_string())
            .collect()
    }
}

/// Every project's [`Tracked`], by its folder.
static TRACKED: Mutex<BTreeMap<PathBuf, Tracked>> = Mutex::new(BTreeMap::new());

// The remote's listing.

/// The remote and branch the locks go with.
#[derive(Debug, Clone)]
pub(crate) struct LockRemote {
    pub(crate) name: String,
    /// The branch on the remote, and its remote-tracking reference.
    branch: String,
    tracking: String,
}

/// One `git ls-remote` of the lock refs and the branch.
#[derive(Debug, Default)]
pub(crate) struct Listing {
    pub(crate) refs: BTreeMap<LockRefName, ObjectId>,
    pub(crate) head: Option<ObjectId>,
}

/// Reads `git ls-remote`'s lines (`<id>\t<ref>`): the lock and request refs
/// of the known forms, and the branch `branch_ref`; anything else is left
/// out.
pub(crate) fn parse_listing(stdout: &str, branch_ref: &str) -> Listing {
    let mut listing = Listing::default();
    for line in stdout.lines() {
        let Some((oid, name)) = line.split_once('\t') else {
            continue;
        };
        let Ok(oid) = ObjectId::from_hex(oid.trim().as_bytes()) else {
            continue;
        };
        let name = name.trim();
        if name == branch_ref {
            listing.head = Some(oid);
        } else if let Some(parsed) = LockRefName::parse(name) {
            listing.refs.insert(parsed, oid);
        }
    }
    listing
}

/// A ref to change with a compare-and-swap push.
#[derive(Debug, Clone)]
struct Update {
    remote_ref: String,
    /// What the ref must point to now (None: it must not exist).
    expected: Option<ObjectId>,
    /// What it is to point to (None: deleted).
    new: Option<ObjectId>,
}

/// How a compare-and-swap push of a ref ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Written {
    Done,
    /// The ref was not what the lease expected (someone else wrote it):
    /// what it is now.
    Changed(Option<ObjectId>),
    /// The remote refused it (git's reason): it does not accept the ref.
    Refused(String),
}

/// The flag and the destination of a line of `git push --porcelain`, with
/// its summary.
pub(crate) fn porcelain_line(line: &str) -> Option<(char, &str, &str)> {
    let mut parts = line.split('\t');
    let flag = parts.next()?;
    let refs = parts.next()?;
    let summary = parts.next().unwrap_or("");
    let mut chars = flag.chars();
    let flag = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let (_, destination) = refs.rsplit_once(':')?;
    Some((flag, destination, summary))
}

/// A signature for a lock commit: the person's name and email without what
/// git's format cannot hold, at `time` (UTC).
fn signature(person: &Person, time: i64) -> gix::actor::Signature {
    let tidy = |text: &str| -> String {
        text.chars()
            .filter(|c| !matches!(c, '<' | '>' | '\n' | '\r' | '\0'))
            .collect::<String>()
            .trim()
            .to_owned()
    };
    let name = tidy(&person.name);
    let email = tidy(&person.email);
    gix::actor::Signature {
        name: if name.is_empty() {
            "?".into()
        } else {
            name.into()
        },
        email: email.into(),
        time: gix::date::Time::new(time, 0),
    }
}

/// The lock ref of the remote and its id, for a reason of a failure.
fn refused(reason: &str) -> VcsError {
    failure(
        ErrorClass::Unsupported,
        format!(
            "This remote does not accept Mitcad's lock references ({}): edit locks cannot \
             be used with it",
            reason.trim()
        ),
    )
}

/// What a session is: its id and, when known, its person (the project's
/// author).
#[derive(Debug, Clone)]
pub(crate) struct Me {
    pub(crate) session: String,
    pub(crate) person: Option<Person>,
}

impl Me {
    fn same_person(&self, other: &Person) -> bool {
        self.person
            .as_ref()
            .is_some_and(|me| me.email.eq_ignore_ascii_case(&other.email))
    }
}

/// What a holder's refresh changes (None: as it was).
#[derive(Debug, Clone, Default)]
pub(crate) struct Refresh {
    pub(crate) active_at: Option<i64>,
    pub(crate) state: Option<LockState>,
    pub(crate) idle_minutes: Option<u32>,
    pub(crate) poll_seconds: Option<u32>,
    pub(crate) mqtt: Option<bool>,
    pub(crate) base: Option<String>,
    /// An answer to a request to add.
    pub(crate) answer: Option<(String, Answer)>,
}

impl ProjectRepo {
    /// The key of this project's [`Tracked`].
    fn tracked_key(&self) -> PathBuf {
        std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone())
    }

    /// Runs `f` with this project's [`Tracked`] (never across a git run).
    fn tracked<R>(&self, f: impl FnOnce(&mut Tracked) -> R) -> R {
        let key = self.tracked_key();
        let mut all = TRACKED.lock().unwrap_or_else(PoisonError::into_inner);
        f(all.entry(key).or_default())
    }

    /// Tests: the lock clock of this project (milliseconds) instead of the
    /// monotonic one.
    #[cfg(test)]
    pub(crate) fn set_lock_clock(&self, clock: std::sync::Arc<std::sync::atomic::AtomicU64>) {
        self.tracked(|t| t.clock = Some(clock));
    }

    /// Tests: a hook called before each lock write of this project.
    #[cfg(test)]
    pub(crate) fn set_lock_write_hook(&self, hook: Option<WriteHook>) {
        self.tracked(|t| t.before_write = hook);
    }

    #[cfg(test)]
    fn before_write(&self) {
        let hook = self.tracked(|t| t.before_write.take());
        if let Some(mut hook) = hook {
            hook();
            self.tracked(|t| {
                if t.before_write.is_none() {
                    t.before_write = Some(hook);
                }
            });
        }
    }

    #[cfg(not(test))]
    fn before_write(&self) {}

    /// The project's edit-lock settings (the defaults when unreadable).
    pub(crate) fn lock_settings(&self) -> EditLocks {
        ProjectSettings::read(&self.root).0.shared.edit_locks
    }

    /// The remote the locks are on: the one the branch follows.
    pub(crate) fn lock_remote(&self) -> Result<LockRemote, VcsError> {
        let repo = self.fresh()?;
        let (_, upstream) = self.current_upstream(&repo)?;
        Ok(LockRemote {
            tracking: upstream.tracking(),
            name: upstream.remote,
            branch: upstream.branch,
        })
    }

    /// The lock's path of `file` (absolute, or relative to the current
    /// folder): relative to the project.
    pub(crate) fn lock_path(&self, file: &std::path::Path) -> Result<String, VcsError> {
        let path = self.relative(file)?;
        if !is_project_path(&path) {
            return Err(VcsError::InvalidPath(format!(
                "{path} cannot have an edit lock"
            )));
        }
        Ok(path)
    }

    /// Lists the remote's lock refs and branch, brings the local copies
    /// along and records what was seen.
    fn look(&self, remote: &LockRemote, control: Option<&Control>) -> Result<(), VcsError> {
        let branch_ref = format!("refs/heads/{}", remote.branch);
        let listed = self.git_ok(
            &format!("Reading the edit locks of {}", remote.name),
            &[
                "ls-remote",
                &remote.name,
                "refs/mitcad/locks/*",
                "refs/mitcad/lock-requests/*",
                &branch_ref,
            ],
            control,
        )?;
        let listing = parse_listing(&listed.stdout, &branch_ref);
        let refs = self.mirror(&remote.name, &listing.refs, control)?;
        self.observe(&refs, listing.head)
    }

    /// The local copies of the lock refs, by the remote's ref names.
    fn local_copies(repo: &gix::Repository) -> Result<BTreeMap<String, ObjectId>, VcsError> {
        let mut out = BTreeMap::new();
        for reference in repo.references()?.all()? {
            let reference = reference?;
            let name = reference.name().as_bstr().to_str_lossy().into_owned();
            if let Some(rest) = name.strip_prefix(LOCAL_PREFIX)
                && let Some(id) = reference.try_id()
            {
                out.insert(format!("refs/mitcad/{rest}"), id.detach());
            }
        }
        Ok(out)
    }

    /// Makes the local copies of the lock refs those of the listing:
    /// without the network when the objects are here (written by this
    /// project, or fetched before), else with one fetch of the lock refs
    /// (`--prune`). Returns the refs as the copies have them.
    fn mirror(
        &self,
        remote: &str,
        listed: &BTreeMap<LockRefName, ObjectId>,
        control: Option<&Control>,
    ) -> Result<BTreeMap<LockRefName, ObjectId>, VcsError> {
        let repo = self.fresh()?;
        let local = Self::local_copies(&repo)?;
        let wanted: BTreeMap<String, ObjectId> = listed
            .iter()
            .map(|(name, oid)| (name.remote(), *oid))
            .collect();
        let fetch = wanted
            .iter()
            .any(|(name, oid)| local.get(name) != Some(oid) && !repo.has_object(oid));
        if fetch {
            self.git_ok(
                &format!("Fetching the edit locks of {remote}"),
                &[
                    "fetch",
                    "--no-tags",
                    "--no-write-fetch-head",
                    "--prune",
                    "--quiet",
                    remote,
                    "+refs/mitcad/locks/*:refs/mitcad/remote-locks/locks/*",
                    "+refs/mitcad/lock-requests/*:refs/mitcad/remote-locks/lock-requests/*",
                ],
                control,
            )?;
            let copies = Self::local_copies(&self.fresh()?)?;
            return Ok(copies
                .into_iter()
                .filter_map(|(name, oid)| LockRefName::parse(&name).map(|parsed| (parsed, oid)))
                .collect());
        }
        let mut edits = Vec::new();
        for (name, oid) in &wanted {
            if local.get(name) != Some(oid) {
                edits.push(local_edit(name, Some(*oid))?);
            }
        }
        for name in local.keys() {
            if !wanted.contains_key(name) {
                edits.push(local_edit(name, None)?);
            }
        }
        self.edit_local(edits)?;
        Ok(listed.clone())
    }

    /// Edits local copies of lock refs.
    fn edit_local(&self, edits: Vec<RefEdit>) -> Result<(), VcsError> {
        if edits.is_empty() {
            return Ok(());
        }
        let who = signature(
            &Person::new("Mitcad edit locks", "locks@mitcad.invalid"),
            now_utc(),
        );
        let mut time = Default::default();
        self.fresh()?
            .edit_references_as(edits, Some(who.to_ref(&mut time)))?;
        Ok(())
    }

    /// The file `name` of a lock or request commit, read with the limits:
    /// a parentless commit and a small tree, the file a blob of at most
    /// [`MAX_FILE_SIZE`] bytes.
    fn read_ref_file(
        repo: &gix::Repository,
        oid: ObjectId,
        name: &str,
    ) -> Result<Vec<u8>, Unreadable> {
        let malformed = |why: &str| Unreadable::Malformed(why.to_owned());
        let header = |oid: ObjectId| -> Result<(gix::object::Kind, u64), Unreadable> {
            match repo.try_find_header(oid) {
                Ok(Some(header)) => Ok((header.kind(), header.size())),
                _ => Err(malformed("its object is missing")),
            }
        };
        let (kind, size) = header(oid)?;
        if kind != gix::object::Kind::Commit || size > MAX_COMMIT_SIZE {
            return Err(malformed("it is not a small commit"));
        }
        let commit = repo
            .find_commit(oid)
            .map_err(|_| malformed("its commit cannot be read"))?;
        if commit.parent_ids().next().is_some() {
            return Err(malformed("its commit has parents"));
        }
        let tree = commit
            .tree_id()
            .map_err(|_| malformed("its commit cannot be read"))?
            .detach();
        let (kind, size) = header(tree)?;
        if kind != gix::object::Kind::Tree || size > MAX_TREE_SIZE {
            return Err(malformed("its tree is not a small tree"));
        }
        let tree = repo
            .find_tree(tree)
            .map_err(|_| malformed("its tree cannot be read"))?;
        let decoded = tree
            .decode()
            .map_err(|_| malformed("its tree cannot be read"))?;
        let entry = decoded
            .entries
            .iter()
            .find(|entry| entry.filename == name)
            .ok_or_else(|| Unreadable::Malformed(format!("it has no {name}")))?;
        if !entry.mode.is_blob() {
            return Err(Unreadable::Malformed(format!("its {name} is not a file")));
        }
        let blob = entry.oid.to_owned();
        let (kind, size) = header(blob)?;
        if kind != gix::object::Kind::Blob {
            return Err(Unreadable::Malformed(format!("its {name} is not a file")));
        }
        if size > MAX_FILE_SIZE as u64 {
            return Err(Unreadable::TooLarge(size));
        }
        repo.find_blob(blob)
            .map(|blob| blob.data.clone())
            .map_err(|_| Unreadable::Malformed(format!("its {name} cannot be read")))
    }

    /// Reads a lock commit, checked against the ref it is under.
    fn read_lock_commit(repo: &gix::Repository, oid: ObjectId, id: &str) -> Contents<Lock> {
        let read = || -> Result<Lock, Unreadable> {
            let data = Self::read_ref_file(repo, oid, LOCK_FILE)?;
            let lock = read_lock(&data)?;
            if lock_id(&lock.path) != id {
                return Err(Unreadable::Malformed(
                    "its path is not the file of its ref".to_owned(),
                ));
            }
            Ok(lock)
        };
        match read() {
            Ok(lock) => Contents::Read(lock),
            Err(why) => Contents::Unreadable(why),
        }
    }

    /// Reads a request commit, checked against the ref it is under.
    fn read_request_commit(
        repo: &gix::Repository,
        oid: ObjectId,
        id: &str,
        session: &str,
    ) -> Contents<Request> {
        let read = || -> Result<Request, Unreadable> {
            let data = Self::read_ref_file(repo, oid, REQUEST_FILE)?;
            let request = read_request(&data)?;
            if lock_id(&request.path) != id || request.session != session {
                return Err(Unreadable::Malformed(
                    "its path or session is not that of its ref".to_owned(),
                ));
            }
            Ok(request)
        };
        match read() {
            Ok(request) => Contents::Read(request),
            Err(why) => Contents::Unreadable(why),
        }
    }

    /// Records the refs of a listing: what changed is read, what is gone is
    /// forgotten, and the requests of this process's sessions follow their
    /// receipts and answers.
    fn observe(
        &self,
        refs: &BTreeMap<LockRefName, ObjectId>,
        head: Option<ObjectId>,
    ) -> Result<(), VcsError> {
        let repo = self.fresh()?;
        // Read outside the lock of the tracked state: only what changed.
        let known: BTreeMap<LockRefName, ObjectId> = self.tracked(|t| {
            let locks = t
                .locks
                .iter()
                .map(|(id, seen)| (LockRefName::Lock(id.clone()), seen.oid));
            let requests = t.requests.iter().map(|((id, session), seen)| {
                (LockRefName::Request(id.clone(), session.clone()), seen.oid)
            });
            locks.chain(requests).collect()
        });
        let mut read_locks = BTreeMap::new();
        let mut read_requests = BTreeMap::new();
        for (name, oid) in refs {
            if known.get(name) == Some(oid) {
                continue;
            }
            match name {
                LockRefName::Lock(id) => {
                    read_locks.insert(id.clone(), Self::read_lock_commit(&repo, *oid, id));
                }
                LockRefName::Request(id, session) => {
                    read_requests.insert(
                        (id.clone(), session.clone()),
                        Self::read_request_commit(&repo, *oid, id, session),
                    );
                }
            }
        }
        self.tracked(|t| {
            let now = t.now();
            t.polled = true;
            t.head = head;
            let mut locks = BTreeMap::new();
            let mut requests = BTreeMap::new();
            for (name, oid) in refs {
                match name {
                    LockRefName::Lock(id) => {
                        let old = t.locks.get(id);
                        // Seen before (also by another `ProjectRepo` meanwhile):
                        // the observation goes on.
                        let seen = match (old, read_locks.remove(id)) {
                            (Some(old), _) if old.oid == *oid => old.clone(),
                            (old, contents) => {
                                let contents = contents
                                    .unwrap_or_else(|| Self::read_lock_commit(&repo, *oid, id));
                                if let Contents::Unreadable(why) = &contents
                                    && why.is_malformed()
                                {
                                    t.malformed.insert(*oid);
                                }
                                let holder = contents.get().map(|lock| lock.session.clone());
                                let holder_since = match old {
                                    Some(old) if old.holder == holder => old.holder_since,
                                    _ => now,
                                };
                                SeenLock {
                                    oid: *oid,
                                    since: now,
                                    contents,
                                    holder,
                                    holder_since,
                                }
                            }
                        };
                        locks.insert(id.clone(), seen);
                    }
                    LockRefName::Request(id, session) => {
                        let key = (id.clone(), session.clone());
                        let old = t.requests.get(&key);
                        let seen = match (old, read_requests.remove(&key)) {
                            (Some(old), _) if old.oid == *oid => old.clone(),
                            (old, contents) => {
                                let contents = contents.unwrap_or_else(|| {
                                    Self::read_request_commit(&repo, *oid, id, session)
                                });
                                if let Contents::Unreadable(why) = &contents
                                    && why.is_malformed()
                                {
                                    t.malformed.insert(*oid);
                                }
                                // A refresh of the same request keeps its place.
                                SeenRequest::new(*oid, contents, old, &mut t.next_order, now)
                            }
                        };
                        requests.insert(key, seen);
                    }
                }
            }
            t.locks = locks;
            t.requests = requests;
            follow_requests(t, now);
        });
        Ok(())
    }

    /// Writes a parentless commit whose tree holds `file` with `data`:
    /// authored by `author`, committed by `committer`, at the time `time`
    /// (UTC).
    fn write_ref_commit(
        &self,
        file: &str,
        data: &[u8],
        author: &Person,
        committer: &Person,
        message: &str,
        time: i64,
    ) -> Result<ObjectId, VcsError> {
        let repo = self.fresh()?;
        let blob = repo.write_blob(data)?.detach();
        let tree = gix::objs::Tree {
            entries: vec![gix::objs::tree::Entry {
                mode: EntryKind::Blob.into(),
                filename: file.into(),
                oid: blob,
            }],
        };
        let tree = repo.write_object(&tree)?.detach();
        let commit = gix::objs::Commit {
            tree,
            parents: Default::default(),
            author: signature(author, time),
            committer: signature(committer, time),
            encoding: None,
            message: format!("{message}\n").into(),
            extra_headers: Vec::new(),
        };
        Ok(repo.write_object(&commit)?.detach())
    }

    /// Pushes `updates` with leases (one `git push`). A ref the remote
    /// rejects is listed again: changed (someone else wrote it) or refused
    /// (the remote does not accept it).
    fn push_updates(
        &self,
        remote: &str,
        updates: &[Update],
        control: Option<&Control>,
    ) -> Result<Vec<Written>, VcsError> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        self.before_write();
        // No hooks, signing or submodules: a lock is not a version.
        let mut args: Vec<String> = [
            "push",
            "--porcelain",
            "--no-verify",
            "--no-signed",
            "--no-recurse-submodules",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        for update in updates {
            args.push(format!(
                "--force-with-lease={}:{}",
                update.remote_ref,
                update.expected.map(|id| id.to_string()).unwrap_or_default()
            ));
        }
        args.push(remote.to_owned());
        for update in updates {
            args.push(match update.new {
                Some(new) => format!("{new}:{}", update.remote_ref),
                None => format!(":{}", update.remote_ref),
            });
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let git = self.git_cli()?;
        let result = git.run(Some(&self.root), &args, control);
        self.remote_log.borrow_mut().push(log_line(&args, &result));
        let output = result?;
        let mut outcomes: Vec<Option<Written>> = vec![None; updates.len()];
        let mut rejected = Vec::new();
        for line in output.stdout.lines() {
            let Some((flag, destination, summary)) = porcelain_line(line) else {
                continue;
            };
            let Some(index) = updates.iter().position(|u| u.remote_ref == destination) else {
                continue;
            };
            outcomes[index] = Some(match flag {
                '!' => {
                    rejected.push(index);
                    Written::Refused(summary.to_owned())
                }
                _ => Written::Done,
            });
        }
        // git leaves out refs that were up to date (the same commit).
        if output.success {
            for outcome in &mut outcomes {
                outcome.get_or_insert(Written::Done);
            }
        }
        if outcomes.iter().any(Option::is_none) {
            return Err(RemoteError::from_git(
                &format!("Writing the edit locks to {remote}"),
                &format!("{}\n{}", output.stderr, output.stdout),
            )
            .into());
        }
        if !rejected.is_empty() {
            let mut list_args = vec!["ls-remote", remote];
            for &index in &rejected {
                list_args.push(&updates[index].remote_ref);
            }
            let listed = self.git_ok(
                &format!("Reading the edit locks of {remote}"),
                &list_args,
                control,
            )?;
            for &index in &rejected {
                let update = &updates[index];
                let now = listed.stdout.lines().find_map(|line| {
                    let (oid, name) = line.split_once('\t')?;
                    (name.trim() == update.remote_ref)
                        .then(|| ObjectId::from_hex(oid.trim().as_bytes()).ok())
                        .flatten()
                });
                if now != update.expected {
                    outcomes[index] = Some(Written::Changed(now));
                }
            }
        }
        Ok(outcomes.into_iter().map(|o| o.expect("set")).collect())
    }

    /// After a successful write of `name` to `new` (None: deleted): the
    /// local copy and what this process knows follow.
    fn written(&self, name: &LockRefName, new: Option<ObjectId>) -> Result<(), VcsError> {
        self.edit_local(vec![local_edit(&name.remote(), new)?])?;
        let repo = self.fresh()?;
        let contents = match (name, new) {
            (LockRefName::Lock(id), Some(oid)) => Some(Ok(Self::read_lock_commit(&repo, oid, id))),
            (LockRefName::Request(id, session), Some(oid)) => {
                Some(Err(Self::read_request_commit(&repo, oid, id, session)))
            }
            _ => None,
        };
        self.tracked(|t| {
            let now = t.now();
            match (name, new, contents) {
                // The same commit again (written in the same second): the
                // observation goes on.
                (LockRefName::Lock(id), Some(oid), _)
                    if t.locks.get(id).is_some_and(|old| old.oid == oid) => {}
                (LockRefName::Request(id, session), Some(oid), _)
                    if t.requests
                        .get(&(id.clone(), session.clone()))
                        .is_some_and(|old| old.oid == oid) => {}
                (LockRefName::Lock(id), Some(oid), Some(Ok(contents))) => {
                    let holder = contents.get().map(|lock| lock.session.clone());
                    let holder_since = match t.locks.get(id) {
                        Some(old) if old.holder == holder => old.holder_since,
                        _ => now,
                    };
                    t.locks.insert(
                        id.clone(),
                        SeenLock {
                            oid,
                            since: now,
                            contents,
                            holder,
                            holder_since,
                        },
                    );
                }
                (LockRefName::Request(id, session), Some(oid), Some(Err(contents))) => {
                    let key = (id.clone(), session.clone());
                    let seen = SeenRequest::new(
                        oid,
                        contents,
                        t.requests.get(&key),
                        &mut t.next_order,
                        now,
                    );
                    t.requests.insert(key, seen);
                }
                (LockRefName::Lock(id), None, _) => {
                    t.locks.remove(id);
                }
                (LockRefName::Request(id, session), None, _) => {
                    t.requests.remove(&(id.clone(), session.clone()));
                }
                _ => {}
            }
            follow_requests(t, now);
        });
        Ok(())
    }

    /// The lock of `id` as this process last saw it.
    fn seen_lock(&self, id: &str) -> Option<SeenLock> {
        self.tracked(|t| t.locks.get(id).cloned())
    }

    /// Writes `lock` for `id` with a lease on `expected`; on success the
    /// local copy and the tracked state follow, and with `hold` the lock's
    /// session is recorded as holding it (to tell when it is lost).
    #[allow(clippy::too_many_arguments)]
    fn write_lock(
        &self,
        remote: &LockRemote,
        id: &str,
        lock: &Lock,
        committer: &Person,
        expected: Option<ObjectId>,
        hold: bool,
        control: Option<&Control>,
    ) -> Result<Written, VcsError> {
        let time = lock.refreshed_at;
        let commit = self.write_ref_commit(
            LOCK_FILE,
            &lock.to_json(),
            &lock.owner,
            committer,
            "Mitcad edit lock",
            time,
        )?;
        let update = Update {
            remote_ref: lock_ref(id),
            expected,
            new: Some(commit),
        };
        let written = self
            .push_updates(&remote.name, &[update], control)?
            .remove(0);
        match &written {
            Written::Done => {
                self.written(&LockRefName::Lock(id.to_owned()), Some(commit))?;
                if hold {
                    self.tracked(|t| {
                        t.held.insert(
                            (lock.session.clone(), id.to_owned()),
                            (commit, lock.path.clone()),
                        );
                    });
                }
            }
            Written::Refused(reason) => return Err(refused(reason)),
            Written::Changed(_) => {}
        }
        Ok(written)
    }

    /// A lock of this session's, copied with a refresh's changes: the
    /// receipts of the requests seen, the answers pruned to them.
    fn refreshed_lock(&self, id: &str, old: &Lock, refresh: &Refresh) -> Lock {
        let now = now_utc();
        let mut lock = old.clone();
        lock.refreshed_at = now;
        if let Some(active_at) = refresh.active_at {
            lock.active_at = active_at;
        }
        if let Some(state) = refresh.state {
            if state == LockState::Idle && lock.state != LockState::Idle {
                lock.idle_since = Some(now);
            }
            if state == LockState::Active {
                lock.idle_since = None;
            }
            lock.state = state;
        }
        if let Some(minutes) = refresh.idle_minutes {
            lock.idle_minutes = minutes;
        }
        if let Some(seconds) = refresh.poll_seconds {
            lock.poll_seconds = seconds;
        }
        if let Some(mqtt) = refresh.mqtt {
            lock.mqtt = mqtt;
        }
        if let Some(base) = &refresh.base {
            lock.base = Some(base.clone());
        }
        let project = self.lock_settings();
        let requests = self.tracked(|t| t.receipts_for(id, &old.session, &project));
        lock.requests_seen = requests.clone();
        lock.answers.retain(|request, _| requests.contains(request));
        if let Some((request, answer)) = &refresh.answer {
            lock.answers.insert(request.clone(), answer.clone());
        }
        lock
    }

    /// Refreshes this session's lock of `id` (the receipts of the requests
    /// seen, and `refresh`'s changes). Ok(None): the lock is not this
    /// session's (lost, or released elsewhere); else the lock written.
    pub(crate) fn refresh_lock(
        &self,
        remote: &LockRemote,
        me: &Me,
        id: &str,
        refresh: &Refresh,
        control: Option<&Control>,
    ) -> Result<Option<Lock>, VcsError> {
        let mut looked = false;
        for _ in 0..WRITE_ATTEMPTS {
            let current = self.seen_lock(id);
            let mine = current.as_ref().and_then(|seen| match &seen.contents {
                Contents::Read(lock) if lock.session == me.session => Some((seen.oid, lock)),
                _ => None,
            });
            let Some((expected, old)) = mine else {
                if looked {
                    return Ok(None);
                }
                self.look(remote, control)?;
                looked = true;
                continue;
            };
            let lock = self.refreshed_lock(id, old, refresh);
            let committer = me.person.clone().unwrap_or_else(|| old.owner.clone());
            match self.write_lock(remote, id, &lock, &committer, Some(expected), true, control)? {
                Written::Done => return Ok(Some(lock)),
                _ => {
                    self.look(remote, control)?;
                    looked = true;
                }
            }
        }
        Ok(None)
    }

    /// Requests kept alive and stale ones removed (mitcad#89), after a
    /// listing: `me`'s waiting requests (asked in this process; not
    /// declined, not granted) whose refs have not changed for half the
    /// project's idle time are written again as refreshes of themselves
    /// (`refresh_of`, with `poll_seconds` when given), and other sessions'
    /// requests that are stale are removed, all in one compare-and-swap
    /// push: a requester that refreshed meanwhile keeps its request.
    /// Returns the paths refreshed and the requests removed.
    pub(crate) fn tidy_requests(
        &self,
        remote: &LockRemote,
        me: &Me,
        poll_seconds: Option<u32>,
        control: Option<&Control>,
    ) -> Result<(Vec<String>, Vec<Value>), VcsError> {
        let project = self.lock_settings();
        // (ref, lease, the request rewritten or None: removed).
        let planned: Vec<(LockRefName, ObjectId, Option<Request>, Value)> = self.tracked(|t| {
            let now = t.now();
            let mut planned = Vec::new();
            for ((id, session), seen) in &t.requests {
                let name = LockRefName::Request(id.clone(), session.clone());
                if seen.stale_for(session, me, &project, now) {
                    let request = seen.contents.get();
                    planned.push((
                        name,
                        seen.oid,
                        None,
                        json!({
                            "id": seen.id.to_string(),
                            "file_id": id,
                            "session": session,
                            "path": request.map(|r| r.path.clone()),
                            "requester": request.map(|r| person_json(&r.requester)),
                        }),
                    ));
                    continue;
                }
                let waiting = *session == me.session
                    && t.asked
                        .get(&(id.clone(), session.clone()))
                        .is_some_and(|asked| asked.request == seen.id)
                    && t.locks
                        .get(id)
                        .and_then(|l| l.contents.get())
                        .is_some_and(|lock| {
                            lock.session != me.session
                                && lock.answers.get(&seen.id.to_string()).map(|a| a.answer)
                                    != Some(AnswerKind::Declined)
                        });
                let due = now >= seen.since + request_refresh_interval(&project);
                if let (true, true, Contents::Read(request)) = (waiting, due, &seen.contents) {
                    let mut refreshed = request.clone();
                    refreshed.refresh_of = Some(seen.id.to_string());
                    // Never the commit there already (only shown, never
                    // compared): others must see the ref change.
                    let last = request.refreshed_at.unwrap_or(request.asked_at);
                    refreshed.refreshed_at = Some(now_utc().max(last + 1));
                    if poll_seconds.is_some() {
                        refreshed.poll_seconds = poll_seconds;
                    }
                    planned.push((name, seen.oid, Some(refreshed), Value::Null));
                }
            }
            planned
        });
        if planned.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut updates = Vec::new();
        let mut commits = Vec::new();
        for (name, expected, request, _) in &planned {
            let commit = match request {
                Some(request) => Some(self.write_ref_commit(
                    REQUEST_FILE,
                    &request.to_json(),
                    &request.requester,
                    me.person.as_ref().unwrap_or(&request.requester),
                    "Mitcad edit lock request",
                    request.refreshed_at.unwrap_or_else(now_utc),
                )?),
                None => None,
            };
            commits.push(commit);
            updates.push(Update {
                remote_ref: name.remote(),
                expected: Some(*expected),
                new: commit,
            });
        }
        let results = self.push_updates(&remote.name, &updates, control)?;
        let (mut refreshed, mut removed) = (Vec::new(), Vec::new());
        for (((name, _, request, shown), commit), written) in
            planned.iter().zip(commits).zip(results)
        {
            match (written, request) {
                (Written::Done, Some(request)) => {
                    self.written(name, commit)?;
                    refreshed.push(request.path.clone());
                }
                (Written::Done | Written::Changed(None), None) => {
                    self.written(name, None)?;
                    removed.push(shown.clone());
                }
                // Gone meanwhile (withdrawn elsewhere, or removed as stale).
                (Written::Changed(None), Some(_)) => self.written(name, None)?,
                // Written meanwhile (a refresh, a new request): the next
                // listing reads it. A remote that refuses lock refs took
                // this one once; a refusal now only stays in the log.
                (Written::Changed(Some(_)) | Written::Refused(_), _) => {}
            }
        }
        Ok((refreshed, removed))
    }

    /// The locks this process's session `me` held that are no longer its
    /// own (taken, or removed elsewhere): their ids, paths and the locks
    /// now.
    fn lost_locks(&self, me: &Me) -> Vec<(String, String, Option<SeenLock>)> {
        self.tracked(|t| {
            let mut lost = Vec::new();
            let held: Vec<(String, String)> = t
                .held
                .iter()
                .filter(|((session, _), _)| *session == me.session)
                .map(|((_, id), (_, path))| (id.clone(), path.clone()))
                .collect();
            for (id, path) in held {
                let still = t.locks.get(&id).is_some_and(|seen| {
                    seen.contents
                        .get()
                        .is_some_and(|lock| lock.session == me.session)
                });
                if !still {
                    t.held.remove(&(me.session.clone(), id.clone()));
                    let now = t.locks.get(&id).cloned();
                    lost.push((id, path, now));
                }
            }
            lost
        })
    }
}

/// An edit of a local copy of a lock ref (`remote_ref`'s), to `new` or
/// deleted.
fn local_edit(remote_ref: &str, new: Option<ObjectId>) -> Result<RefEdit, VcsError> {
    let name = local_ref(remote_ref);
    let change = match new {
        Some(oid) => Change::Update {
            log: LogChange {
                mode: RefLog::AndReference,
                force_create_reflog: false,
                message: "edit locks".into(),
            },
            expected: PreviousValue::Any,
            new: Target::Object(oid),
        },
        None => Change::Delete {
            expected: PreviousValue::Any,
            log: RefLog::AndReference,
        },
    };
    Ok(RefEdit {
        change,
        name: name
            .as_str()
            .try_into()
            .map_err(|e| VcsError::Git(format!("{name}: {e}")))?,
        deref: false,
    })
}

/// The requests of this process's sessions follow the locks: a receipt or
/// an answer seen for the first time is noted with the time, a new holder
/// starts them again, and requests that are gone are forgotten.
fn follow_requests(t: &mut Tracked, now: Duration) {
    let requests = &t.requests;
    // A refresh is the same request (its id).
    t.asked.retain(|key, asked| {
        requests
            .get(key)
            .is_some_and(|seen| seen.id == asked.request)
    });
    for ((id, _), asked) in t.asked.iter_mut() {
        let Some(lock) = t.locks.get(id).and_then(|seen| seen.contents.get()) else {
            asked.holder = None;
            asked.receipt_at = None;
            asked.answer_at = None;
            asked.answer = None;
            asked.until = None;
            continue;
        };
        if asked.holder.as_deref() != Some(lock.session.as_str()) {
            asked.holder = Some(lock.session.clone());
            asked.receipt_at = None;
            asked.answer_at = None;
            asked.answer = None;
            asked.until = None;
        }
        let request = asked.request.to_string();
        if asked.receipt_at.is_none() && lock.requests_seen.contains(&request) {
            asked.receipt_at = Some(now);
        }
        match lock.answers.get(&request) {
            // A new answer, or a "keep" again with a new end.
            Some(answer) if asked.answer != Some(answer.answer) || asked.until != answer.until => {
                // An answer without a receipt counts as one too.
                asked.receipt_at.get_or_insert(now);
                asked.answer_at = Some(now);
                asked.answer = Some(answer.answer);
                asked.until = answer.until;
                let keep = answer
                    .until
                    .map_or(0, |until| until - lock.refreshed_at)
                    .clamp(0, i64::from(MAX_KEEP_MINUTES) * 60);
                asked.keep = Duration::from_secs(u64::try_from(keep).unwrap_or(0));
            }
            Some(_) => {}
            None => {
                asked.answer_at = None;
                asked.answer = None;
                asked.until = None;
            }
        }
    }
}

/// Seconds of a duration, rounded down.
fn seconds(duration: Duration) -> u64 {
    duration.as_secs()
}

/// Whether a lock held by another session is stale for `me`, and why; or
/// how long until it is by the earliest rule. `project`: the project's
/// settings (for a lock that cannot be read, and the poll interval when
/// both sides listen to the broker); `mqtt`: this session listens.
fn staleness(
    t: &Tracked,
    id: &str,
    seen: &SeenLock,
    me: &Me,
    project: &EditLocks,
    mqtt: bool,
    now: Duration,
) -> (Option<TakenBecause>, Option<Duration>) {
    let lock = seen.contents.get();
    let idle = lock.map_or(project.idle_minutes, |l| l.idle_minutes);
    let poll = lock.map_or(project.poll_seconds, |l| l.poll_seconds);
    let idle_time = Duration::from_secs(u64::from(idle) * 60);
    let two_polls = Duration::from_secs(2 * u64::from(poll));
    let mut deadlines = vec![(TakenBecause::Unchanged, seen.since + idle_time + two_polls)];
    if let (Some(lock), Some(asked)) = (lock, t.asked.get(&(id.to_owned(), me.session.clone()))) {
        let receipt_poll = if mqtt && lock.mqtt {
            project.poll_seconds
        } else {
            poll
        };
        let receipt_due = Duration::from_secs(2 * u64::from(receipt_poll));
        match (asked.receipt_at, asked.answer, asked.answer_at) {
            (None, _, _) => deadlines.push((
                TakenBecause::NoReceipt,
                asked.at.max(seen.holder_since) + receipt_due,
            )),
            (Some(receipt), None, _) => {
                deadlines.push((TakenBecause::Unanswered, receipt + idle_time + two_polls))
            }
            (Some(_), Some(AnswerKind::Keep), Some(at)) => deadlines.push((
                TakenBecause::Unanswered,
                at + asked.keep + idle_time + two_polls,
            )),
            _ => {}
        }
    }
    let (because, deadline) = deadlines
        .into_iter()
        .min_by_key(|(_, deadline)| *deadline)
        .expect("one rule");
    if now >= deadline {
        (Some(because), None)
    } else {
        (None, Some(deadline - now))
    }
}

// The answers' parts.

fn person_json(person: &Person) -> Value {
    json!({"name": person.name, "email": person.email})
}

fn previous_json(previous: &Option<Previous>) -> Value {
    match previous {
        None => Value::Null,
        Some(previous) => json!({
            "name": previous.person.name,
            "email": previous.person.email,
            "session": previous.session,
            "reason": previous.because.map(TakenBecause::as_str),
        }),
    }
}

fn answer_json(answer: Option<&Answer>) -> Value {
    match answer {
        None => Value::Null,
        Some(answer) => json!({
            "answer": match answer.answer {
                AnswerKind::Declined => "declined",
                AnswerKind::Keep => "keep",
            },
            "until": answer.until.map(format_time),
            "message": answer.message,
        }),
    }
}

/// A request as the answers show it.
#[allow(clippy::too_many_arguments)]
fn request_json(
    id: &str,
    session: &str,
    seen: &SeenRequest,
    lock: Option<&Lock>,
    me: &Me,
    position: usize,
    project: &EditLocks,
    now: Duration,
) -> Value {
    let request = seen.id.to_string();
    let mine = session == me.session;
    // Another session's request not refreshed in time (mitcad#89).
    let (stale, stale_in) = if mine {
        (None, None)
    } else if seen.stale_for(session, me, project, now) {
        (Some("unchanged"), None)
    } else {
        (None, Some(seconds(seen.stale_at(project) - now)))
    };
    let mut value = json!({
        "id": request,
        "commit": seen.oid.to_string(),
        "file_id": id,
        "session": session,
        "mine": mine,
        "order": position,
        "waiting_seconds": seconds(now.saturating_sub(seen.first_seen)),
        "unchanged_seconds": seconds(now.saturating_sub(seen.since)),
        "stale": stale,
        "stale_in_seconds": stale_in,
        "seen": lock.is_some_and(|lock| lock.requests_seen.contains(&request)),
        "answer": answer_json(lock.and_then(|lock| lock.answers.get(&request))),
    });
    match &seen.contents {
        Contents::Read(r) => {
            value["readable"] = json!(true);
            value["problem"] = Value::Null;
            value["path"] = json!(r.path);
            value["requester"] = person_json(&r.requester);
            value["asked_at"] = json!(format_time(r.asked_at));
            value["refreshed_at"] = json!(r.refreshed_at.map(format_time));
            value["message"] = json!(r.message);
        }
        Contents::Unreadable(why) => {
            value["readable"] = json!(false);
            value["problem"] = json!(why.to_string());
            value["path"] = Value::Null;
            value["requester"] = Value::Null;
            value["asked_at"] = Value::Null;
            value["refreshed_at"] = Value::Null;
            value["message"] = Value::Null;
        }
    }
    value
}

/// The state of a request of `me` for lock `id`: `waiting` (no receipt
/// yet), `seen`, `kept` (the holder keeps the lock until `until`),
/// `declined`, `granted` (the lock is this session's) or `free` (no lock).
fn my_request_json(t: &Tracked, id: &str, me: &Me, project: &EditLocks, mqtt: bool) -> Value {
    let key = (id.to_owned(), me.session.clone());
    let Some(seen) = t.requests.get(&key) else {
        return Value::Null;
    };
    let now = t.now();
    let asked = t.asked.get(&key);
    let lock = t.locks.get(id);
    let contents = lock.and_then(|seen| seen.contents.get());
    let request = seen.id.to_string();
    let answer = contents.and_then(|lock| lock.answers.get(&request));
    let state = match (lock, contents) {
        (None, _) => "free",
        (Some(_), Some(lock)) if lock.session == me.session => "granted",
        _ => match answer.map(|a| a.answer) {
            Some(AnswerKind::Declined) => "declined",
            Some(AnswerKind::Keep) => "kept",
            None if contents.is_some_and(|lock| lock.requests_seen.contains(&request)) => "seen",
            None => "waiting",
        },
    };
    let (stale, stale_in) = match (lock, state) {
        (Some(lock), "waiting" | "seen" | "kept") => staleness(t, id, lock, me, project, mqtt, now),
        _ => (None, None),
    };
    json!({
        "id": request,
        "commit": seen.oid.to_string(),
        "file_id": id,
        "path": seen.contents.get().map(|r| r.path.clone()),
        "state": state,
        "tracked": asked.is_some(),
        "answer": answer_json(answer),
        "stale": stale.map(TakenBecause::as_str),
        "stale_in_seconds": stale_in.map(seconds),
    })
}

/// A lock as the answers show it, for session `me`.
fn lock_json(
    t: &Tracked,
    id: &str,
    seen: &SeenLock,
    me: &Me,
    project: &EditLocks,
    mqtt: bool,
) -> Value {
    let now = t.now();
    let lock = seen.contents.get();
    let mine = lock.is_some_and(|lock| lock.session == me.session);
    let (stale, stale_in) = if mine {
        (None, None)
    } else {
        staleness(t, id, seen, me, project, mqtt, now)
    };
    let requests: Vec<Value> = t
        .requests_of(id)
        .into_iter()
        .enumerate()
        .map(|(i, (session, request))| {
            request_json(id, session, request, lock, me, i + 1, project, now)
        })
        .collect();
    let mut value = json!({
        "id": id,
        "commit": seen.oid.to_string(),
        "mine": mine,
        "same_owner": !mine && lock.is_some_and(|lock| me.same_person(&lock.owner)),
        "unchanged_seconds": seconds(now.saturating_sub(seen.since)),
        "stale": stale.map(TakenBecause::as_str),
        "stale_in_seconds": stale_in.map(seconds),
        "requests": requests,
        "my_request": my_request_json(t, id, me, project, mqtt),
    });
    match &seen.contents {
        Contents::Read(lock) => {
            let fields = json!({
                "readable": true,
                "problem": null,
                "path": lock.path,
                "owner": person_json(&lock.owner),
                "session": lock.session,
                "application_version": lock.application_version,
                "taken_at": format_time(lock.taken_at),
                "refreshed_at": format_time(lock.refreshed_at),
                "active_at": format_time(lock.active_at),
                "idle_minutes": lock.idle_minutes,
                "poll_seconds": lock.poll_seconds,
                "mqtt": lock.mqtt,
                "base": lock.base,
                "state": lock.state,
                "idle_since": lock.idle_since.map(format_time),
                "handed_over_from": previous_json(&lock.handed_over_from),
                "taken_from": previous_json(&lock.taken_from),
            });
            for (key, field) in fields.as_object().expect("an object") {
                value[key] = field.clone();
            }
        }
        Contents::Unreadable(why) => {
            value["readable"] = json!(false);
            value["problem"] = json!(why.to_string());
            for key in [
                "path",
                "owner",
                "session",
                "application_version",
                "taken_at",
                "refreshed_at",
                "active_at",
                "idle_minutes",
                "poll_seconds",
                "mqtt",
                "base",
                "state",
                "idle_since",
                "handed_over_from",
                "taken_from",
            ] {
                value[key] = Value::Null;
            }
        }
    }
    value
}
