// SPDX-License-Identifier: MIT
//! Edit locks (mitcad#89) against repositories in temporary folders: a bare
//! repository (a folder remote) and two projects A and B connected to it,
//! each with a lock clock of its own (milliseconds, advanced by the tests).
//! Then the readers of `lock.json` and `request.json` with checked, cleaned
//! and clamped fields, malformed and oversized files dropped and counted,
//! and a seeded random run of mutated files through them. Without the
//! system's git the tests with repositories are skipped.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::remote::{AUTHOR, bare, connected, error_class, new_version, run, slashes, system_git};
use super::*;
use crate::remote::locks::{
    self, Answer, AnswerKind, Lock, LockRefName, LockState, MAX_FILE_SIZE, Person, Previous,
    Request, TakenBecause, Unreadable, clean_limited, clean_text, command_line_session,
    format_time, is_project_path, is_session, lock_id, parse_time, read_lock, read_request,
};
use crate::remote::{GitCli, clone_project};

const SA: &str = "aaaaaaaa-0000-4000-8000-000000000001";
const SA2: &str = "aaaaaaaa-0000-4000-8000-000000000003";
const SB: &str = "bbbbbbbb-0000-4000-8000-000000000002";
const OTHER: &str = "Other Tester <other@example.invalid>";

/// Project A (the author of [`AUTHOR`]) connected to an empty remote with
/// its first version, B (another author) opened from it, and their lock
/// clocks.
struct Pair {
    a: ProjectRepo,
    b: ProjectRepo,
    remote: PathBuf,
    cli: GitCli,
    clock_a: Arc<AtomicU64>,
    clock_b: Arc<AtomicU64>,
    // Removed last.
    scratch: Scratch,
}

fn pair(name: &str) -> Option<Pair> {
    pair_with(name, |remote| remote.to_string_lossy().into_owned())
}

fn pair_with(name: &str, url: impl Fn(&Path) -> String) -> Option<Pair> {
    let cli = system_git()?;
    let scratch = Scratch::new(name);
    let remote = bare(&scratch, "remote.git");
    let url = url(&remote);
    let (a, _) = connected(&scratch, &cli, &url);
    let mut log = Vec::new();
    clone_project(&url, &scratch.join("b"), Some(&cli), None, &mut log).unwrap();
    let b = ProjectRepo::open(&scratch.join("b")).unwrap();
    b.set_git(cli.clone());
    let (clock_a, clock_b) = (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)));
    a.set_lock_clock(clock_a.clone());
    b.set_lock_clock(clock_b.clone());
    Some(Pair {
        a,
        b,
        remote,
        cli,
        clock_a,
        clock_b,
        scratch,
    })
}

/// Advances a lock clock by `seconds`.
fn advance(clock: &AtomicU64, seconds: u64) {
    clock.fetch_add(seconds * 1000, Ordering::SeqCst);
}

/// The design's path in a project.
fn part(repo: &ProjectRepo) -> String {
    repo.root()
        .join("part.mitcad")
        .to_string_lossy()
        .into_owned()
}

/// Runs a lock command that must answer without an error.
fn ok(repo: &ProjectRepo, command: Value) -> Value {
    let answer = run(repo, command);
    assert_eq!(answer["error"], Value::Null, "{answer}");
    answer
}

/// Takes the design's lock with short times: idle 1 minute, polls 5 s.
fn take(repo: &ProjectRepo, session: &str, author: &str) -> Value {
    ok(
        repo,
        json!({"cmd": "lock_take", "path": part(repo), "session": session, "author": author,
               "idle_minutes": 1, "poll_seconds": 5}),
    )
}

fn status(repo: &ProjectRepo, session: &str) -> Value {
    ok(
        repo,
        json!({"cmd": "lock_status", "session": session, "path": part(repo)}),
    )
}

fn poll(repo: &ProjectRepo, session: &str) -> Value {
    ok(repo, json!({"cmd": "lock_poll", "session": session}))
}

/// The lock refs and request refs on a repository.
fn lock_refs(repository: &Path) -> Vec<String> {
    git(
        repository,
        &["for-each-ref", "--format=%(refname)", "refs/mitcad/"],
    )
    .unwrap()
    .lines()
    .filter(|name| !name.starts_with("refs/mitcad/sync-backup/"))
    .map(str::to_owned)
    .collect()
}

fn fsck(repository: &Path) {
    git(repository, &["fsck", "--strict", "--no-progress"]).unwrap();
}

#[test]
fn take_refresh_release_and_held() {
    let Some(p) = pair("lock-basic") else { return };
    let id = lock_id("part.mitcad");
    let taken = take(&p.a, SA, AUTHOR);
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["lock"]["mine"], true);
    assert_eq!(taken["lock"]["path"], "part.mitcad");
    assert_eq!(taken["lock"]["idle_minutes"], 1);
    assert_eq!(taken["lock"]["state"], "active");
    assert_eq!(lock_refs(&p.remote), [format!("refs/mitcad/locks/{id}")]);
    // The lock commit: no parents, authored by the holder, not in the
    // history.
    let commit = taken["lock"]["commit"].as_str().unwrap().to_owned();
    let parents = git(&p.remote, &["log", "-1", "--format=%P|%an", &commit]).unwrap();
    assert_eq!(parents.trim(), "|Mitcad Test");
    let history = git(&p.remote, &["rev-list", "main"]).unwrap();
    assert!(!history.contains(&commit));

    // B sees A's lock and cannot take it.
    let seen = status(&p.b, SB);
    let lock = &seen["lock"];
    assert_eq!(lock["owner"]["name"], "Mitcad Test", "{seen}");
    assert_eq!(lock["mine"], false);
    assert_eq!(lock["same_owner"], false);
    assert_eq!(lock["stale"], Value::Null);
    assert_eq!(lock["stale_in_seconds"], 70);
    let held = take(&p.b, SB, OTHER);
    assert_eq!(held["outcome"], "held", "{held}");
    let text =
        p.b.command_text(
            &json!({"cmd": "lock_take", "path": part(&p.b), "session": SB, "author": OTHER})
                .to_string(),
        )
        .unwrap();
    assert!(
        text.contains("part.mitcad is being edited by Mitcad Test <test@example.invalid>"),
        "{text}"
    );

    // A marks it idle; B sees the state.
    let refreshed = ok(
        &p.a,
        json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA, "state": "idle"}),
    );
    assert_eq!(refreshed["outcome"], "refreshed", "{refreshed}");
    assert_ne!(refreshed["lock"]["commit"], commit);
    let seen = status(&p.b, SB);
    assert_eq!(seen["lock"]["state"], "idle", "{seen}");
    assert!(seen["lock"]["idle_since"].is_string());

    // B cannot release it without --force; A can.
    let release = ok(
        &p.b,
        json!({"cmd": "lock_release", "path": part(&p.b), "session": SB}),
    );
    assert_eq!(release["outcome"], "not_held", "{release}");
    let release = ok(
        &p.a,
        json!({"cmd": "lock_release", "path": part(&p.a), "session": SA}),
    );
    assert_eq!(release["outcome"], "released", "{release}");
    assert!(lock_refs(&p.remote).is_empty());
    let taken = take(&p.b, SB, OTHER);
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["lock"]["owner"]["email"], "other@example.invalid");
    let text =
        p.a.command_text(&json!({"cmd": "lock_status", "session": SA}).to_string())
            .unwrap();
    assert!(
        text.contains("part.mitcad: edited by Other Tester <other@example.invalid> since"),
        "{text}"
    );
    for repository in [&p.remote, p.a.root(), p.b.root()] {
        fsck(repository);
    }
}

#[test]
fn of_two_takers_at_once_one_wins() {
    let Some(p) = pair("lock-race") else { return };
    let roots = [p.a.root().to_path_buf(), p.b.root().to_path_buf()];
    let outcomes: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = roots
            .iter()
            .zip([(SA, AUTHOR), (SB, OTHER)])
            .map(|(root, (session, author))| {
                let cli = p.cli.clone();
                scope.spawn(move || {
                    let repo = ProjectRepo::open(root).unwrap();
                    repo.set_git(cli);
                    take(&repo, session, author)["outcome"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let won = outcomes.iter().filter(|o| *o == "taken").count();
    assert_eq!(won, 1, "{outcomes:?}");
    assert!(
        outcomes
            .iter()
            .all(|o| ["taken", "held", "changed"].contains(&o.as_str())),
        "{outcomes:?}"
    );
    assert_eq!(lock_refs(&p.remote).len(), 1);

    // Deterministically: A takes between B's listing and B's push.
    let free = ok(
        &p.a,
        json!({"cmd": "lock_release", "path": part(&p.a), "session": SA, "force": true}),
    );
    assert_eq!(free["outcome"], "released", "{free}");
    let (root_a, cli) = (p.a.root().to_path_buf(), p.cli.clone());
    let mut once = true;
    p.b.set_lock_write_hook(Some(Box::new(move || {
        if std::mem::take(&mut once) {
            let a = ProjectRepo::open(&root_a).unwrap();
            a.set_git(cli.clone());
            assert_eq!(take(&a, SA, AUTHOR)["outcome"], "taken");
        }
    })));
    let lost = take(&p.b, SB, OTHER);
    p.b.set_lock_write_hook(None);
    assert_eq!(lost["outcome"], "changed", "{lost}");
    assert_eq!(lost["lock"]["owner"]["name"], "Mitcad Test", "{lost}");
}

#[test]
fn a_stale_lock_is_taken_and_its_holder_finds_it_lost() {
    let Some(p) = pair("lock-stale") else { return };
    take(&p.a, SA, AUTHOR);
    let seen = status(&p.b, SB);
    assert_eq!(seen["lock"]["stale"], Value::Null);
    // Unchanged for the idle time (1 min) and two polls (2 × 5 s) on B's
    // clock.
    advance(&p.clock_b, 69);
    assert_eq!(status(&p.b, SB)["lock"]["stale_in_seconds"], 1);
    advance(&p.clock_b, 1);
    let seen = status(&p.b, SB);
    assert_eq!(seen["lock"]["stale"], "unchanged", "{seen}");
    // A's clock is never compared: it has not moved.
    let taken = take(&p.b, SB, OTHER);
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["taken_from"]["name"], "Mitcad Test");
    assert_eq!(taken["taken_from"]["reason"], "unchanged");
    assert_eq!(taken["lock"]["taken_from"]["session"], SA);
    assert_eq!(taken["previous"]["owner"]["name"], "Mitcad Test");
    assert!(taken["previous"]["active_at"].is_string());

    let polled = poll(&p.a, SA);
    let lost = polled["lost"].as_array().unwrap();
    assert_eq!(lost.len(), 1, "{polled}");
    assert_eq!(lost[0]["path"], "part.mitcad");
    assert_eq!(lost[0]["lock"]["owner"]["name"], "Other Tester");
    assert_eq!(lost[0]["lock"]["taken_from"]["reason"], "unchanged");
    let refreshed = ok(
        &p.a,
        json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA}),
    );
    assert_eq!(refreshed["outcome"], "lost", "{refreshed}");
    // Nothing more to lose.
    assert_eq!(poll(&p.a, SA)["lost"], json!([]));
}

#[test]
fn a_takeover_fails_when_the_holder_refreshed_meanwhile() {
    let Some(p) = pair("lock-refreshed") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    status(&p.b, SB);
    advance(&p.clock_b, 70);
    assert_eq!(status(&p.b, SB)["lock"]["stale"], "unchanged");
    // Between B's listing and B's push, A refreshes its lock.
    let (root_a, cli) = (p.a.root().to_path_buf(), p.cli.clone());
    let mut once = true;
    p.b.set_lock_write_hook(Some(Box::new(move || {
        if std::mem::take(&mut once) {
            let a = ProjectRepo::open(&root_a).unwrap();
            a.set_git(cli.clone());
            let refreshed = ok(
                &a,
                json!({"cmd": "lock_refresh", "path": part(&a), "session": SA,
                       "active_at": "2026-10-08T13:40:00Z"}),
            );
            assert_eq!(refreshed["outcome"], "refreshed");
        }
    })));
    let taken = take(&p.b, SB, OTHER);
    p.b.set_lock_write_hook(None);
    assert_eq!(taken["outcome"], "changed", "{taken}");
    assert_eq!(taken["lock"]["owner"]["name"], "Mitcad Test");
    // The refreshed lock is not stale: B saw it change.
    assert_eq!(taken["lock"]["stale"], Value::Null, "{taken}");
    assert_eq!(poll(&p.a, SA)["lost"], json!([]));
    assert_eq!(status(&p.a, SA)["lock"]["mine"], true);
}

#[test]
fn requests_receipts_answers_and_hand_over() {
    let Some(p) = pair("lock-requests") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    // B asks for the lock.
    let asked = ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER,
               "message": "May I?\u{202E}\u{200B}  Please"}),
    );
    assert_eq!(asked["outcome"], "requested", "{asked}");
    assert_eq!(asked["my_request"]["state"], "waiting");
    // The receipt is due within two of the holder's polls.
    assert_eq!(asked["receipt_due_seconds"], 10);
    let id = lock_id("part.mitcad");
    assert!(lock_refs(&p.remote).contains(&format!("refs/mitcad/lock-requests/{id}/{SB}")));

    // A's poll writes the receipt.
    let polled = poll(&p.a, SA);
    assert_eq!(polled["receipts"], json!(["part.mitcad"]), "{polled}");
    let requests = polled["locks"][0]["requests"].as_array().unwrap().clone();
    assert_eq!(requests.len(), 1, "{polled}");
    assert_eq!(requests[0]["requester"]["name"], "Other Tester");
    assert_eq!(requests[0]["message"], "May I? Please");
    assert_eq!(requests[0]["seen"], true);
    assert_eq!(requests[0]["order"], 1);
    let request = requests[0]["id"].as_str().unwrap().to_owned();
    // No second receipt for the same request.
    assert_eq!(poll(&p.a, SA)["receipts"], json!([]));
    let seen = poll(&p.b, SB);
    assert_eq!(seen["my_requests"][0]["state"], "seen", "{seen}");

    // Keep 15 more minutes.
    let kept = ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": request,
               "answer": "keep", "minutes": 15}),
    );
    assert_eq!(kept["outcome"], "answered", "{kept}");
    let seen = poll(&p.b, SB);
    let mine = &seen["my_requests"][0];
    assert_eq!(mine["state"], "kept", "{seen}");
    assert!(mine["answer"]["until"].is_string());
    // Kept: while A refreshes its lock, it is not stale before the 15
    // minutes, the idle time and two polls (on B's clock, from when B saw
    // the answer); then it is.
    for step in 0..19 {
        advance(&p.clock_b, 50);
        // A new activity time: a refresh in the same second as the last
        // one would write the same commit.
        ok(
            &p.a,
            json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA,
                   "active_at": format_time(1_791_466_800 + step)}),
        );
        let seen = poll(&p.b, SB);
        assert_eq!(seen["my_requests"][0]["stale"], Value::Null, "{seen}");
        assert_eq!(seen["my_requests"][0]["state"], "kept", "{seen}");
    }
    advance(&p.clock_b, 50);
    let seen = poll(&p.b, SB);
    assert_eq!(seen["my_requests"][0]["stale"], "unanswered", "{seen}");
    // The holder's dialog came back: kept again, with a new end.
    ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": request,
               "answer": "keep", "minutes": 20}),
    );
    let seen = poll(&p.b, SB);
    assert_eq!(seen["my_requests"][0]["state"], "kept", "{seen}");
    assert_eq!(seen["my_requests"][0]["stale"], Value::Null, "{seen}");

    // Declined with a message.
    ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": request,
               "answer": "declined", "message": "Busy until noon"}),
    );
    let seen = poll(&p.b, SB);
    let mine = &seen["my_requests"][0];
    assert_eq!(mine["state"], "declined", "{seen}");
    assert_eq!(mine["answer"]["message"], "Busy until noon");
    let text =
        p.a.command_text(&json!({"cmd": "lock_status", "session": SA}).to_string())
            .unwrap();
    assert!(text.contains("requested by Other Tester"), "{text}");

    // B asks again: a new request, which A hands the lock over to.
    let again = ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    assert_ne!(again["request"].as_str().unwrap(), request);
    let polled = poll(&p.a, SA);
    assert_eq!(polled["receipts"], json!(["part.mitcad"]));
    let lock = &polled["locks"][0];
    assert_eq!(lock["requests"][0]["answer"], Value::Null, "{lock}");
    let handed = ok(
        &p.a,
        json!({"cmd": "lock_hand_over", "path": part(&p.a), "session": SA}),
    );
    assert_eq!(handed["outcome"], "handed_over", "{handed}");
    assert_eq!(handed["to"]["session"], SB);
    assert_eq!(handed["lock"]["mine"], false);

    let seen = poll(&p.b, SB);
    let lock = &seen["locks"][0];
    assert_eq!(lock["mine"], true, "{seen}");
    assert_eq!(lock["owner"]["name"], "Other Tester");
    assert_eq!(lock["handed_over_from"]["session"], SA);
    assert_eq!(seen["my_requests"][0]["state"], "granted");
    // B takes it as its own: its values, and its request withdrawn.
    let taken = take(&p.b, SB, OTHER);
    assert_eq!(taken["outcome"], "refreshed", "{taken}");
    assert_eq!(taken["lock"]["handed_over_from"]["name"], "Mitcad Test");
    assert_eq!(lock_refs(&p.remote), [format!("refs/mitcad/locks/{id}")]);
    // A no longer holds it, and knows.
    assert_eq!(poll(&p.a, SA)["lost"], json!([]), "handed over, not lost");
    assert_eq!(status(&p.a, SA)["lock"]["mine"], false);
    for repository in [&p.remote, p.a.root(), p.b.root()] {
        fsck(repository);
    }
}

#[test]
fn a_request_without_a_receipt_makes_the_lock_stale() {
    let Some(p) = pair("lock-receipt") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    // A's application does not poll: no receipt within 2 × 5 s.
    advance(&p.clock_b, 9);
    assert_eq!(poll(&p.b, SB)["locks"][0]["stale"], Value::Null);
    advance(&p.clock_b, 1);
    let seen = poll(&p.b, SB);
    assert_eq!(seen["locks"][0]["stale"], "no_receipt", "{seen}");
    assert_eq!(seen["my_requests"][0]["stale"], "no_receipt");
    let taken = take(&p.b, SB, OTHER);
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["taken_from"]["reason"], "no_receipt");
    // The request is withdrawn with the take.
    assert_eq!(lock_refs(&p.remote).len(), 1);
}

#[test]
fn a_request_without_an_answer_makes_the_lock_stale() {
    let Some(p) = pair("lock-answer") else { return };
    take(&p.a, SA, AUTHOR);
    ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    poll(&p.a, SA);
    assert_eq!(poll(&p.b, SB)["my_requests"][0]["state"], "seen");
    // A keeps refreshing (so the lock is not unchanged) but does not
    // answer: after the idle time and two polls from the receipt it is.
    advance(&p.clock_b, 40);
    ok(
        &p.a,
        json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA,
               "active_at": "2026-10-08T13:40:00Z"}),
    );
    assert_eq!(poll(&p.b, SB)["locks"][0]["stale"], Value::Null);
    advance(&p.clock_b, 30);
    let seen = poll(&p.b, SB);
    assert_eq!(seen["locks"][0]["stale"], "unanswered", "{seen}");
    let taken = take(&p.b, SB, OTHER);
    assert_eq!(taken["taken_from"]["reason"], "unanswered", "{taken}");
}

/// The request refs of the design on a repository.
fn request_refs(repository: &Path) -> Vec<String> {
    lock_refs(repository)
        .into_iter()
        .filter(|name| name.starts_with("refs/mitcad/lock-requests/"))
        .collect()
}

/// The contents of a ref's file on a repository, as JSON.
fn ref_file(repository: &Path, name: &str, file: &str) -> Value {
    let text = git(repository, &["show", &format!("{name}:{file}")]).unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn a_killed_requesters_request_goes_stale_and_is_removed() {
    let Some(p) = pair("lock-request-stale") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    let asked = ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER,
               "poll_seconds": 5}),
    );
    assert_eq!(asked["outcome"], "requested", "{asked}");
    let refs = request_refs(&p.remote);
    assert_eq!(refs.len(), 1, "{refs:?}");
    assert_eq!(
        ref_file(&p.remote, &refs[0], "request.json")["poll_seconds"],
        5
    );
    // A's poll writes the receipt; the request is stale for A after the
    // project's idle time (10 minutes) and two of B's polls (2 × 5 s)
    // unchanged.
    let polled = poll(&p.a, SA);
    assert_eq!(polled["receipts"], json!(["part.mitcad"]), "{polled}");
    let request = &polled["locks"][0]["requests"][0];
    assert_eq!(request["stale"], Value::Null, "{polled}");
    assert_eq!(request["stale_in_seconds"], 610);
    // B's Mitcad is killed: it polls no more, so it never refreshes.
    advance(&p.clock_a, 609);
    let polled = poll(&p.a, SA);
    assert_eq!(polled["removed_requests"], json!([]), "{polled}");
    assert_eq!(polled["locks"][0]["requests"][0]["stale_in_seconds"], 1);
    advance(&p.clock_a, 1);
    // Seen without writing: stale, and no longer given a receipt.
    let seen = status(&p.a, SA);
    assert_eq!(seen["lock"]["requests"][0]["stale"], "unchanged", "{seen}");
    let refreshed = ok(
        &p.a,
        json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA,
               "active_at": "2026-10-08T13:41:00Z"}),
    );
    assert_eq!(
        refreshed["lock"]["requests"][0]["seen"], false,
        "{refreshed}"
    );
    let text =
        p.a.command_text(&json!({"cmd": "lock_status", "session": SA}).to_string())
            .unwrap();
    assert!(
        text.contains("(stale: its Mitcad no longer refreshes it)"),
        "{text}"
    );
    // The holder's poll removes it (anyone's would).
    let polled = poll(&p.a, SA);
    let removed = polled["removed_requests"].as_array().unwrap();
    assert_eq!(removed.len(), 1, "{polled}");
    assert_eq!(removed[0]["session"], SB);
    assert_eq!(removed[0]["path"], "part.mitcad");
    assert_eq!(removed[0]["requester"]["name"], "Other Tester");
    assert_eq!(polled["locks"][0]["requests"], json!([]), "{polled}");
    assert!(request_refs(&p.remote).is_empty());
    let text =
        p.a.command_text(&json!({"cmd": "lock_poll", "session": SA}).to_string())
            .unwrap();
    assert!(!text.contains("Removed a stale request"), "once: {text}");
    // B's Mitcad, started again in the same session, finds its request gone.
    let seen = poll(&p.b, SB);
    assert_eq!(seen["my_requests"], json!([]), "{seen}");
    for repository in [&p.remote, p.a.root(), p.b.root()] {
        fsck(repository);
    }
}

#[test]
fn a_waiting_requester_refreshes_its_request_which_keeps_its_id() {
    let Some(p) = pair("lock-request-refresh") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    let asked = ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER,
               "message": "May I?"}),
    );
    let request = asked["request"].as_str().unwrap().to_owned();
    poll(&p.a, SA);
    ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": request,
               "answer": "keep", "minutes": 30}),
    );
    // Not due before half the project's idle time (5 minutes) on B's clock.
    advance(&p.clock_b, 299);
    assert_eq!(poll(&p.b, SB)["refreshed_requests"], json!([]));
    let refs = request_refs(&p.remote);
    let first = git(&p.remote, &["rev-parse", &refs[0]]).unwrap();
    assert_eq!(first.trim(), request);
    advance(&p.clock_b, 1);
    let polled = ok(
        &p.b,
        json!({"cmd": "lock_poll", "session": SB, "poll_seconds": 120}),
    );
    assert_eq!(
        polled["refreshed_requests"],
        json!(["part.mitcad"]),
        "{polled}"
    );
    let mine = &polled["my_requests"][0];
    assert_eq!(mine["id"], request, "the same request");
    assert_ne!(mine["commit"], request, "a new commit");
    assert_eq!(mine["state"], "kept", "{polled}");
    assert_eq!(mine["tracked"], true);
    // On the remote: the refresh names the request, with B's poll interval
    // now; asked_at and the message stay.
    let written = ref_file(&p.remote, &refs[0], "request.json");
    assert_eq!(written["refresh_of"], request, "{written}");
    assert_eq!(written["poll_seconds"], 120);
    assert_eq!(written["message"], "May I?");
    assert!(written["refreshed_at"].is_string());
    let commit = git(&p.remote, &["log", "-1", "--format=%P|%an", &refs[0]]).unwrap();
    assert_eq!(
        commit.trim(),
        "|Other Tester",
        "parentless, by the requester"
    );
    // A sees the same request (its place, receipt and answer kept), and
    // its lock keeps the answer when refreshed.
    let polled = poll(&p.a, SA);
    assert_eq!(polled["receipts"], json!([]), "no new receipt: {polled}");
    let seen = &polled["locks"][0]["requests"][0];
    assert_eq!(seen["id"], request, "{polled}");
    assert_eq!(seen["order"], 1);
    assert_eq!(seen["seen"], true);
    assert_eq!(seen["answer"]["answer"], "keep");
    assert_eq!(seen["message"], "May I?");
    assert!(seen["refreshed_at"].is_string());
    // Refreshed in time, it never goes stale for A: unchanged at most the
    // half idle time and one of B's polls.
    for _ in 0..4 {
        advance(&p.clock_a, 300);
        advance(&p.clock_b, 300);
        let polled = poll(&p.b, SB);
        assert_eq!(
            polled["refreshed_requests"],
            json!(["part.mitcad"]),
            "{polled}"
        );
        assert_eq!(polled["my_requests"][0]["state"], "kept", "{polled}");
        let polled = poll(&p.a, SA);
        assert_eq!(polled["removed_requests"], json!([]), "{polled}");
        assert_eq!(polled["locks"][0]["requests"][0]["stale"], Value::Null);
        ok(
            &p.a,
            json!({"cmd": "lock_refresh", "path": part(&p.a), "session": SA,
                   "active_at": "now"}),
        );
    }
    let polled = poll(&p.a, SA);
    assert_eq!(
        polled["locks"][0]["requests"][0]["answer"]["answer"], "keep",
        "{polled}"
    );

    // Declined: B's Mitcad stops refreshing it, and it goes stale.
    ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": request,
               "answer": "declined"}),
    );
    advance(&p.clock_b, 300);
    let polled = poll(&p.b, SB);
    assert_eq!(polled["my_requests"][0]["state"], "declined", "{polled}");
    assert_eq!(polled["refreshed_requests"], json!([]), "{polled}");
    // A saw it last changed at the last refresh: the idle time and two of
    // B's polls (2 × 120 s) later it is removed.
    advance(&p.clock_a, 839);
    assert_eq!(poll(&p.a, SA)["removed_requests"], json!([]));
    advance(&p.clock_a, 1);
    let polled = poll(&p.a, SA);
    let removed = polled["removed_requests"].as_array().unwrap();
    assert_eq!(removed.len(), 1, "{polled}");
    assert_eq!(removed[0]["id"], request);
    assert!(request_refs(&p.remote).is_empty());
    // A hand-over then has nobody to go to.
    let handed = ok(
        &p.a,
        json!({"cmd": "lock_hand_over", "path": part(&p.a), "session": SA}),
    );
    assert_eq!(handed["outcome"], "no_request", "{handed}");
    for repository in [&p.remote, p.a.root(), p.b.root()] {
        fsck(repository);
    }
}

#[test]
fn a_hand_over_passes_declined_and_stale_requests_by() {
    let Some(p) = pair("lock-hand-over-order") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    let first = ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    let first = first["request"].as_str().unwrap().to_owned();
    poll(&p.a, SA);
    ok(
        &p.a,
        json!({"cmd": "lock_answer", "path": part(&p.a), "session": SA, "request": first,
               "answer": "declined"}),
    );
    // Another session of B's author asks after it (a project of its own
    // folder: B's clone, another process's session).
    let b2 = ProjectRepo::open(p.b.root()).unwrap();
    b2.set_git(p.cli.clone());
    let sb2 = "bbbbbbbb-0000-4000-8000-000000000004";
    ok(
        &b2,
        json!({"cmd": "lock_request", "path": part(&b2), "session": sb2, "author": OTHER}),
    );
    let polled = poll(&p.a, SA);
    let requests = polled["locks"][0]["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 2, "{polled}");
    assert_eq!(requests[0]["session"], SB, "served in order: {polled}");
    let handed = ok(
        &p.a,
        json!({"cmd": "lock_hand_over", "path": part(&p.a), "session": SA}),
    );
    assert_eq!(handed["outcome"], "handed_over", "{handed}");
    assert_eq!(
        handed["to"]["session"], sb2,
        "the declined request passed by"
    );
}

#[test]
fn take_over_from_another_session() {
    let Some(p) = pair("lock-take-over") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    // The same author in another session (window or computer).
    let held = take(&p.a, SA2, AUTHOR);
    assert_eq!(held["outcome"], "held", "{held}");
    assert_eq!(held["lock"]["same_owner"], true);
    let commit = held["lock"]["commit"].as_str().unwrap().to_owned();
    // A take-over names the lock as it was shown: another commit is not it.
    let wrong = ok(
        &p.a,
        json!({"cmd": "lock_take", "path": part(&p.a), "session": SA2, "author": AUTHOR,
               "take_over": "0123456789012345678901234567890123456789"}),
    );
    assert_eq!(wrong["outcome"], "held");
    let taken = ok(
        &p.a,
        json!({"cmd": "lock_take", "path": part(&p.a), "session": SA2, "author": AUTHOR,
               "take_over": commit}),
    );
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["taken_from"]["reason"], "take_over");
    assert_eq!(taken["lock"]["session"], SA2);
    // The first session finds it lost.
    let polled = poll(&p.a, SA);
    assert_eq!(polled["lost"].as_array().unwrap().len(), 1, "{polled}");
    assert_eq!(polled["lost"][0]["lock"]["session"], SA2);
}

#[test]
fn sync_and_fetch_never_carry_lock_refs() {
    let Some(p) = pair("lock-sync") else { return };
    take(&p.a, SA, AUTHOR);
    ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    // B has local copies of the lock refs.
    assert!(
        lock_refs(p.b.root())
            .iter()
            .any(|name| name.starts_with(locks::LOCAL_PREFIX))
    );
    let on_remote = lock_refs(&p.remote);
    assert_eq!(on_remote.len(), 2, "{on_remote:?}");

    // B records a version and syncs; A fetches and syncs.
    let part_b = p.b.root().join("part.mitcad");
    new_version(&p.b, &part_b, 31.0);
    let synced = run(&p.b, json!({"cmd": "sync", "author": OTHER}));
    assert_eq!(synced["error"], Value::Null, "{synced}");
    assert_eq!(synced["pushed"], true);
    let fetched = run(&p.a, json!({"cmd": "fetch"}));
    assert_eq!(fetched["error"], Value::Null, "{fetched}");
    let synced = run(&p.a, json!({"cmd": "sync", "author": AUTHOR}));
    assert_eq!(synced["error"], Value::Null, "{synced}");
    // Nothing of the local copies went to the remote, and A's fetch took
    // no lock refs (only its own lock's copy, from its take).
    assert_eq!(lock_refs(&p.remote), on_remote);
    let a_refs = lock_refs(p.a.root());
    assert!(
        a_refs
            .iter()
            .all(|name| !name.starts_with("refs/mitcad/remote-locks/lock-requests/")),
        "{a_refs:?}"
    );
    // A clone has none of them; the lock commits are in no history.
    let mut log = Vec::new();
    let url = p.remote.to_string_lossy().into_owned();
    clone_project(&url, &p.scratch.join("c"), Some(&p.cli), None, &mut log).unwrap();
    assert!(lock_refs(&p.scratch.join("c")).is_empty());
    for name in &on_remote {
        let commit = git(&p.remote, &["rev-parse", name]).unwrap();
        let parents = git(&p.remote, &["log", "-1", "--format=%P", commit.trim()]).unwrap();
        assert_eq!(parents.trim(), "", "{name}");
        let contains = git(&p.remote, &["branch", "--contains", commit.trim()]).unwrap();
        assert_eq!(contains.trim(), "", "{name}");
    }
    for repository in [&p.remote, p.a.root(), p.b.root()] {
        fsck(repository);
    }
}

#[test]
fn sync_plan_names_files_locked_by_others() {
    let Some(p) = pair("lock-plan") else { return };
    take(&p.a, SA, AUTHOR);
    let part_b = p.b.root().join("part.mitcad");
    new_version(&p.b, &part_b, 32.0);
    let plan = run(&p.b, json!({"cmd": "sync_plan", "session": SB}));
    assert_eq!(plan["error"], Value::Null, "{plan}");
    assert_eq!(plan["case"], "push");
    let locked = plan["locked"].as_array().unwrap();
    assert_eq!(locked.len(), 1, "{plan}");
    assert_eq!(locked[0]["path"], "part.mitcad");
    assert_eq!(locked[0]["lock"]["owner"]["name"], "Mitcad Test");
    // As the holder's session it is not someone else's; from the last
    // poll's state without the network too.
    let plan = run(
        &p.b,
        json!({"cmd": "sync_plan", "session": SA, "locks": "last"}),
    );
    assert_eq!(plan["locked"], json!([]), "{plan}");
    let plan = run(
        &p.b,
        json!({"cmd": "sync_plan", "session": SB, "locks": "last"}),
    );
    assert_eq!(plan["locked"].as_array().unwrap().len(), 1, "{plan}");
    // Nothing to send: nothing locked to warn about.
    let plan = run(&p.a, json!({"cmd": "sync_plan", "session": SB}));
    assert_eq!(plan["case"], "up_to_date", "{plan}");
    assert_eq!(plan["locked"], json!([]));
}

#[test]
fn release_force_and_all() {
    let Some(p) = pair("lock-release") else {
        return;
    };
    take(&p.a, SA, AUTHOR);
    let other = p.a.root().join("other.mitcad");
    ok(
        &p.a,
        json!({"cmd": "lock_take", "path": other, "session": SA, "author": AUTHOR}),
    );
    ok(
        &p.b,
        json!({"cmd": "lock_request", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    assert_eq!(lock_refs(&p.remote).len(), 3);
    // --force removes the lock and the file's requests.
    let forced = ok(
        &p.b,
        json!({"cmd": "lock_release", "path": part(&p.b), "force": true, "author": OTHER}),
    );
    assert_eq!(forced["outcome"], "released", "{forced}");
    assert_eq!(forced["withdrawn"], json!(["part.mitcad"]));
    assert_eq!(lock_refs(&p.remote).len(), 1);
    let polled = poll(&p.a, SA);
    assert_eq!(polled["lost"].as_array().unwrap().len(), 1, "{polled}");
    // All of a session's locks and requests.
    ok(
        &p.b,
        json!({"cmd": "lock_take", "path": part(&p.b), "session": SB, "author": OTHER}),
    );
    ok(
        &p.b,
        json!({"cmd": "lock_request", "path": p.b.root().join("other.mitcad"), "session": SB,
               "author": OTHER}),
    );
    let all = ok(
        &p.b,
        json!({"cmd": "lock_release", "all": true, "session": SB}),
    );
    assert_eq!(all["released"], json!(["part.mitcad"]), "{all}");
    assert_eq!(all["withdrawn"], json!(["other.mitcad"]));
    assert_eq!(
        lock_refs(&p.remote),
        [format!("refs/mitcad/locks/{}", lock_id("other.mitcad"))]
    );
    // Nothing of B's left to release.
    let none = ok(
        &p.b,
        json!({"cmd": "lock_release", "path": part(&p.b), "session": SB}),
    );
    assert_eq!(none["outcome"], "free", "{none}");
}

#[test]
fn the_command_line_session_comes_from_the_author() {
    let Some(p) = pair("lock-cli") else { return };
    let taken = ok(
        &p.a,
        json!({"cmd": "lock_take", "path": part(&p.a), "author": AUTHOR}),
    );
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(
        taken["session"],
        command_line_session("test@example.invalid").as_str()
    );
    // Another run (another ProjectRepo) releases it.
    let again = ProjectRepo::open(p.a.root()).unwrap();
    again.set_git(p.cli.clone());
    let text = again
        .command_text(
            &json!({"cmd": "lock_release", "path": part(&again), "author": AUTHOR}).to_string(),
        )
        .unwrap();
    assert!(
        text.contains("Released the edit lock of part.mitcad"),
        "{text}"
    );
}

#[test]
fn locks_through_a_file_url() {
    let Some(p) = pair_with("lock-file-url", |remote| {
        format!(
            "file://{}",
            slashes(remote).trim_start_matches('/').replace(' ', "%20")
        )
        .replacen("file://", "file:///", 1)
    }) else {
        return;
    };
    assert_eq!(take(&p.a, SA, AUTHOR)["outcome"], "taken");
    assert_eq!(take(&p.b, SB, OTHER)["outcome"], "held");
    let probe = ok(&p.a, json!({"cmd": "lock_probe", "session": SA}));
    assert_eq!(probe["accepted"], true, "{probe}");
    assert_eq!(lock_refs(&p.remote).len(), 1, "the probe is gone");
}

#[test]
fn a_remote_that_refuses_lock_refs() {
    let Some(p) = pair("lock-refused") else {
        return;
    };
    let probe = ok(&p.a, json!({"cmd": "lock_probe", "session": SA}));
    assert_eq!(probe["accepted"], true, "{probe}");
    assert!(lock_refs(&p.remote).is_empty());
    if !cfg!(unix) {
        return;
    }
    // A server hook that refuses every ref outside the branches.
    let hook = p.remote.join("hooks").join("pre-receive");
    fs::write(
        &hook,
        "#!/bin/sh\nwhile read old new name; do\n  case \"$name\" in\n    refs/heads/*) ;;\n    \
         *) echo \"refs outside the branches are not allowed\" >&2; exit 1 ;;\n  esac\ndone\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let probe = ok(&p.a, json!({"cmd": "lock_probe", "session": SA}));
    assert_eq!(probe["accepted"], false, "{probe}");
    assert_eq!(
        probe["message"],
        "This remote does not accept Mitcad's lock references"
    );
    let taken = run(
        &p.a,
        json!({"cmd": "lock_take", "path": part(&p.a), "session": SA, "author": AUTHOR}),
    );
    assert_eq!(error_class(&taken), "unsupported", "{taken}");
    assert!(
        taken["error"]["message"]
            .as_str()
            .unwrap()
            .contains("does not accept Mitcad's lock references"),
        "{taken}"
    );
}

/// Writes a parentless (or `parent`'s child) commit with `file` holding
/// `data` in `repo` and pushes it to `name` on `remote` by the system's
/// git, as anyone with push access could.
fn push_raw(
    repo: &ProjectRepo,
    remote: &Path,
    name: &str,
    file: &str,
    data: &[u8],
    parent: Option<ObjectId>,
) {
    push_raw_with(repo, remote, name, &[(file, data)], "raw", parent);
}

/// [`push_raw`] with several files and a message.
fn push_raw_with(
    repo: &ProjectRepo,
    remote: &Path,
    name: &str,
    files: &[(&str, &[u8])],
    message: &str,
    parent: Option<ObjectId>,
) {
    let r = repo.fresh().unwrap();
    let mut entries: Vec<gix::objs::tree::Entry> = files
        .iter()
        .map(|(file, data)| gix::objs::tree::Entry {
            mode: gix::objs::tree::EntryKind::Blob.into(),
            filename: (*file).into(),
            oid: r.write_blob(data).unwrap().detach(),
        })
        .collect();
    entries.sort_by(|a, b| a.filename.cmp(&b.filename));
    let tree = gix::objs::Tree { entries };
    let tree = r.write_object(&tree).unwrap().detach();
    let signature = gix::actor::Signature {
        name: "Someone".into(),
        email: "someone@example.invalid".into(),
        time: gix::date::Time::now_utc(),
    };
    let commit = gix::objs::Commit {
        tree,
        parents: parent.into_iter().collect(),
        author: signature.clone(),
        committer: signature,
        encoding: None,
        message: format!("{message}\n").into(),
        extra_headers: Vec::new(),
    };
    let id = r.write_object(&commit).unwrap().detach();
    git(
        repo.root(),
        &["push", "-q", &slashes(remote), &format!("{id}:{name}")],
    )
    .unwrap();
}

/// A valid lock of `path` as Mitcad writes it.
fn sample_lock(path: &str) -> Lock {
    let now = parse_time("2026-10-08T13:40:00Z").unwrap();
    Lock {
        path: path.to_owned(),
        owner: Person::new("Alex Example", "alex@example.invalid"),
        session: SA.to_owned(),
        application_version: "0.2.0".to_owned(),
        taken_at: now,
        refreshed_at: now + 60,
        active_at: now + 30,
        idle_minutes: 10,
        poll_seconds: 10,
        mqtt: true,
        base: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
        state: LockState::Idle,
        idle_since: Some(now + 50),
        requests_seen: vec!["89abcdef0123456789abcdef0123456789abcdef".to_owned()],
        answers: [(
            "89abcdef0123456789abcdef0123456789abcdef".to_owned(),
            Answer {
                answer: AnswerKind::Keep,
                until: Some(now + 900),
                message: Some("A moment".to_owned()),
            },
        )]
        .into_iter()
        .collect(),
        handed_over_from: Some(Previous {
            person: Person::new("Sam Example", "sam@example.invalid"),
            session: SB.to_owned(),
            because: None,
        }),
        taken_from: Some(Previous {
            person: Person::new("Kim Example", "kim@example.invalid"),
            session: SA2.to_owned(),
            because: Some(TakenBecause::NoReceipt),
        }),
    }
}

fn sample_request(path: &str) -> Request {
    Request {
        path: path.to_owned(),
        requester: Person::new("Sam Example", "sam@example.invalid"),
        session: SB.to_owned(),
        asked_at: parse_time("2026-10-08T14:00:00Z").unwrap(),
        message: Some("Could I edit it?".to_owned()),
        refresh_of: None,
        refreshed_at: None,
        poll_seconds: Some(10),
    }
}

/// A lock's JSON with `change` applied.
fn edited(change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&sample_lock("part.mitcad").to_json()).unwrap();
    change(&mut value);
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn malformed_and_oversized_lock_refs_are_dropped_and_counted() {
    let Some(p) = pair("lock-malformed") else {
        return;
    };
    let head = p.b.head_commit().unwrap().unwrap();
    let raw = |path: &str, file: &str, data: &[u8], parent: Option<ObjectId>| {
        push_raw(
            &p.b,
            &p.remote,
            &locks::lock_ref(&lock_id(path)),
            file,
            data,
            parent,
        );
    };
    raw("garbage.mitcad", "lock.json", b"{not json", None);
    let mut large = sample_lock("large.mitcad").to_json();
    large.truncate(large.len() - 2);
    large.extend(vec![b' '; MAX_FILE_SIZE]);
    large.extend(b"}\n");
    raw("large.mitcad", "lock.json", &large, None);
    raw(
        "elsewhere.mitcad",
        "lock.json",
        &sample_lock("part.mitcad").to_json(),
        None,
    );
    raw(
        "parent.mitcad",
        "lock.json",
        &sample_lock("parent.mitcad").to_json(),
        Some(head),
    );
    let newer = edited(|lock| {
        lock["version"] = json!(2);
        lock["path"] = json!("newer.mitcad");
    });
    raw("newer.mitcad", "lock.json", &newer, None);
    raw(
        "nolock.mitcad",
        "other.txt",
        &sample_lock("nolock.mitcad").to_json(),
        None,
    );
    // A commit over 64 KiB, and a tree over 4 KiB.
    let huge = sample_lock("huge.mitcad").to_json();
    push_raw_with(
        &p.b,
        &p.remote,
        &locks::lock_ref(&lock_id("huge.mitcad")),
        &[("lock.json", &huge)],
        &"m".repeat(70 * 1024),
        None,
    );
    let wide = sample_lock("wide.mitcad").to_json();
    let names: Vec<String> = (0..200).map(|i| format!("file-{i:03}.txt")).collect();
    let mut files: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
    files.push(("lock.json", &wide));
    push_raw_with(
        &p.b,
        &p.remote,
        &locks::lock_ref(&lock_id("wide.mitcad")),
        &files,
        "raw",
        None,
    );
    // A request whose session is not its ref's, and refs of other forms.
    push_raw(
        &p.b,
        &p.remote,
        &locks::request_ref(&lock_id("part.mitcad"), SA),
        "request.json",
        &sample_request("part.mitcad").to_json(),
        None,
    );
    push_raw(
        &p.b,
        &p.remote,
        "refs/mitcad/locks/not-an-id",
        "lock.json",
        b"{}",
        None,
    );
    push_raw(
        &p.b,
        &p.remote,
        &format!("refs/mitcad/lock-requests/{}/not-a-session", lock_id("x")),
        "request.json",
        b"{}",
        None,
    );

    let seen = ok(&p.a, json!({"cmd": "lock_status", "session": SA}));
    let locks = seen["locks"].as_array().unwrap();
    assert_eq!(locks.len(), 8, "{seen}");
    assert!(locks.iter().all(|lock| lock["readable"] == false), "{seen}");
    // Garbage, too large, another file's path, a parent, no lock.json, a
    // large commit, a large tree and the request: eight; the newer version
    // is not counted.
    assert_eq!(seen["dropped"], 8, "{seen}");
    let problems: Vec<&str> = locks
        .iter()
        .map(|lock| lock["problem"].as_str().unwrap())
        .collect();
    for expected in [
        "more than 16384",
        "version 2",
        "parents",
        "not a small commit",
        "not a small tree",
        "no lock.json",
        "not the file of its ref",
    ] {
        assert!(
            problems.iter().any(|p| p.contains(expected)),
            "{expected}: {problems:?}"
        );
    }
    let request = &seen["requests_without_lock"][0];
    assert_eq!(request["readable"], false, "{seen}");
    // Counted once, however often they are seen.
    assert_eq!(
        ok(&p.a, json!({"cmd": "lock_status", "session": SA}))["dropped"],
        8
    );
    let text =
        p.a.command_text(&json!({"cmd": "lock_status", "session": SA}).to_string())
            .unwrap();
    assert!(text.contains("a lock that cannot be read"), "{text}");
    assert!(
        text.contains("Malformed lock or request refs dropped: 8"),
        "{text}"
    );

    // An unreadable lock holds its file for the project's idle time and two
    // polls (10 min, 2 × 10 s), then it is taken.
    let garbage = p.a.root().join("garbage.mitcad");
    let take_garbage = || {
        ok(
            &p.a,
            json!({"cmd": "lock_take", "path": garbage, "session": SA, "author": AUTHOR}),
        )
    };
    assert_eq!(take_garbage()["outcome"], "held");
    advance(&p.clock_a, 620);
    let taken = take_garbage();
    assert_eq!(taken["outcome"], "taken", "{taken}");
    assert_eq!(taken["taken_from"], Value::Null);
    assert_eq!(taken["lock"]["readable"], true);
}

// The readers of lock.json and request.json.

#[test]
fn locks_and_requests_round_trip() {
    let lock = sample_lock("parts/bracket.mitcad");
    assert_eq!(read_lock(&lock.to_json()).unwrap(), lock);
    let request = sample_request("parts/bracket.mitcad");
    assert_eq!(read_request(&request.to_json()).unwrap(), request);
    let text = String::from_utf8(lock.to_json()).unwrap();
    assert!(text.contains("\"format\": \"mitcad-lock\""), "{text}");
    assert!(
        text.contains("\"taken_at\": \"2026-10-08T13:40:00Z\""),
        "{text}"
    );
    assert!(text.contains("\"reason\": \"no_receipt\""), "{text}");
}

#[test]
fn lock_fields_are_checked_cleaned_and_clamped() {
    let malformed = |bytes: Vec<u8>| match read_lock(&bytes) {
        Err(Unreadable::Malformed(_)) => {}
        other => panic!("not malformed: {other:?}"),
    };
    // Numbers clamped to the settings' bounds.
    let lock = read_lock(&edited(|l| {
        l["idle_minutes"] = json!(100_000);
        l["poll_seconds"] = json!(1);
    }))
    .unwrap();
    assert_eq!((lock.idle_minutes, lock.poll_seconds), (120, 5));
    // Names cleaned: invisible characters gone, whitespace collapsed.
    let lock = read_lock(&edited(|l| {
        l["owner"]["name"] = json!("  Al\u{202E}ex\u{200B}\n\tExample\u{0007} ");
    }))
    .unwrap();
    assert_eq!(lock.owner.name, "Alex Example");
    // Markup stays text.
    let lock = read_lock(&edited(|l| l["owner"]["name"] = json!("<img src=x>"))).unwrap();
    assert_eq!(lock.owner.name, "<img src=x>");
    malformed(edited(|l| l["owner"]["name"] = json!("x".repeat(101))));
    malformed(edited(|l| l["owner"]["name"] = json!("\u{200B}\u{202E}")));
    malformed(edited(|l| {
        l["answers"]["89abcdef0123456789abcdef0123456789abcdef"]["message"] =
            json!("m".repeat(201));
    }));
    malformed(edited(|l| l["session"] = json!("not-a-uuid")));
    malformed(edited(|l| l["session"] = json!(SA.to_uppercase())));
    malformed(edited(|l| l["base"] = json!("abc")));
    malformed(edited(|l| l["path"] = json!("../outside.mitcad")));
    malformed(edited(|l| l["path"] = json!("/root.mitcad")));
    malformed(edited(|l| l["path"] = json!("C:/part.mitcad")));
    malformed(edited(|l| l["path"] = json!("a\\b.mitcad")));
    malformed(edited(|l| l["taken_at"] = json!("1999-12-31T23:59:59Z")));
    malformed(edited(|l| l["taken_at"] = json!("2026-02-30T10:00:00Z")));
    malformed(edited(|l| l["taken_at"] = json!(1_700_000_000)));
    malformed(edited(|l| l["state"] = json!("away")));
    malformed(edited(|l| l["requests_seen"] = json!(["xyz"])));
    malformed(edited(|l| {
        l["requests_seen"] = json!(vec!["0123456789abcdef0123456789abcdef01234567"; 65]);
    }));
    malformed(edited(|l| l["format"] = json!("mitcad-lock-request")));
    malformed(edited(|l| l["idle_minutes"] = json!(-1)));
    malformed(edited(|l| l["idle_minutes"] = json!(1.5)));
    malformed(b"[]".to_vec());
    malformed(b"".to_vec());
    malformed(vec![0xff, 0xfe]);
    // Unknown fields of version 1 are ignored; another version is not read.
    assert!(read_lock(&edited(|l| l["later"] = json!({"a": 1}))).is_ok());
    assert_eq!(
        read_lock(&edited(|l| l["version"] = json!(2))),
        Err(Unreadable::UnknownVersion(2))
    );
    // The size is checked before anything is parsed.
    let mut large = sample_lock("part.mitcad").to_json();
    large.resize(MAX_FILE_SIZE + 1, b' ');
    assert_eq!(
        read_lock(&large),
        Err(Unreadable::TooLarge((MAX_FILE_SIZE + 1) as u64))
    );
    // Requests: the same checks.
    let request = |change: &dyn Fn(&mut Value)| {
        let mut value: Value =
            serde_json::from_slice(&sample_request("part.mitcad").to_json()).unwrap();
        change(&mut value);
        read_request(&serde_json::to_vec(&value).unwrap())
    };
    assert!(request(&|_| {}).is_ok());
    assert!(request(&|r| r["message"] = json!("m".repeat(201))).is_err());
    assert_eq!(
        request(&|r| r["message"] = json!("  ")).unwrap().message,
        None
    );
    assert!(request(&|r| r["asked_at"] = json!("yesterday")).is_err());
    assert!(request(&|r| r["requester"] = json!({"name": "Sam"})).is_err());
    assert_eq!(
        request(&|r| r["version"] = json!(7)),
        Err(Unreadable::UnknownVersion(7))
    );
    // A refresh (mitcad#89): the request's id as a commit id, its time and
    // the requester's poll interval clamped; a request without them (as
    // the first Mitcad with locks wrote) is read as before.
    let refresh = request(&|r| {
        r["refresh_of"] = json!("0123456789abcdef0123456789abcdef01234567");
        r["refreshed_at"] = json!("2026-10-08T14:05:00Z");
        r["poll_seconds"] = json!(100_000);
    })
    .unwrap();
    assert_eq!(
        refresh.refresh_of.as_deref(),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
    assert_eq!(
        refresh.refreshed_at,
        Some(parse_time("2026-10-08T14:05:00Z").unwrap())
    );
    assert_eq!(refresh.poll_seconds, Some(600));
    assert_eq!(read_request(&refresh.to_json()).unwrap(), refresh);
    let plain = request(&|r| {
        let object = r.as_object_mut().unwrap();
        for key in ["refresh_of", "refreshed_at", "poll_seconds"] {
            object.remove(key);
        }
    })
    .unwrap();
    assert_eq!(
        (plain.refresh_of, plain.refreshed_at, plain.poll_seconds),
        (None, None, None)
    );
    assert!(request(&|r| r["refresh_of"] = json!("HEAD")).is_err());
    assert!(request(&|r| r["refresh_of"] = json!(42)).is_err());
    assert!(request(&|r| r["refreshed_at"] = json!("soon")).is_err());
    assert!(request(&|r| r["poll_seconds"] = json!(-5)).is_err());
}

#[test]
fn texts_times_ids_and_paths() {
    assert_eq!(clean_text(" a\u{202A}b\u{2066}c \u{FEFF} d\r\n"), "abc d");
    assert_eq!(clean_text("\u{E0041}tag\u{00AD}s"), "tags");
    assert_eq!(clean_text("tab\tand\u{2028}line"), "tab and line");
    assert_eq!(clean_limited("abcdef", 3), "abc");
    assert_eq!(clean_limited("ab  cdef", 3), "ab");
    assert_eq!(clean_limited("äöü€x", 4), "äöü€");

    for (text, seconds) in [
        ("2026-10-08T13:40:00Z", 1_791_466_800),
        ("2026-10-08T15:40:00+02:00", 1_791_466_800),
        ("2026-10-08t13:40:00.123456z", 1_791_466_800),
        ("2026-10-08 08:40:00-05:00", 1_791_466_800),
        ("2000-01-01T00:00:00Z", 946_684_800),
        ("2024-02-29T12:00:00Z", 1_709_208_000),
        ("2016-12-31T23:59:60Z", 1_483_228_799),
    ] {
        assert_eq!(parse_time(text), Some(seconds), "{text}");
    }
    assert_eq!(format_time(1_791_466_800), "2026-10-08T13:40:00Z");
    assert_eq!(format_time(1_709_208_000), "2024-02-29T12:00:00Z");
    for seconds in (946_684_800..7_258_118_400).step_by(86_400 * 37 + 3_607) {
        assert_eq!(parse_time(&format_time(seconds)), Some(seconds));
    }
    for bad in [
        "",
        "2026-10-08",
        "2026-10-08T13:40:00",
        "2026-13-08T13:40:00Z",
        "2023-02-29T12:00:00Z",
        "2026-10-08T24:00:00Z",
        "2026-10-08T13:60:00Z",
        "2026-10-08T13:40:00+24:00",
        "2026-10-08T13:40:00.Z",
        "2026-10-08T13:40:00.1234567890Z",
        "2201-01-01T00:00:00Z",
        "1999-12-31T23:59:59Z",
        "+026-10-08T13:40:00Z",
        "2026-10-08T13:40:00Zjunk",
        "２０２６-10-08T13:40:00Z",
    ] {
        assert_eq!(parse_time(bad), None, "{bad}");
    }

    assert!(is_session(SA));
    assert!(!is_session(&SA.to_uppercase()));
    assert!(!is_session("aaaaaaaa-0000-4000-8000-00000000000"));
    assert!(!is_session("aaaaaaaa00000-4000-8000-000000000001"));
    assert_eq!(
        locks::session_of(&format!(" {} ", SA.to_uppercase())).as_deref(),
        Some(SA)
    );
    let session = command_line_session("Test@Example.invalid ");
    assert!(is_session(&session), "{session}");
    assert_eq!(session, command_line_session("test@example.invalid"));

    assert_eq!(
        lock_id("part.mitcad"),
        mitcad_model::Sha256::of(b"part.mitcad").to_string()
    );
    assert_eq!(
        LockRefName::parse(&locks::lock_ref(&lock_id("a"))),
        Some(LockRefName::Lock(lock_id("a")))
    );
    assert_eq!(
        LockRefName::parse(&locks::request_ref(&lock_id("a"), SB)),
        Some(LockRefName::Request(lock_id("a"), SB.to_owned()))
    );
    for other in [
        "refs/mitcad/locks/probe-0123456789abcdef",
        "refs/mitcad/locks/",
        "refs/heads/main",
        &format!("refs/mitcad/lock-requests/{}", lock_id("a")),
        &format!("refs/mitcad/lock-requests/{}/{SB}/x", lock_id("a")),
    ] {
        assert_eq!(LockRefName::parse(other), None, "{other}");
    }
    assert_eq!(
        locks::local_ref("refs/mitcad/locks/x"),
        "refs/mitcad/remote-locks/locks/x"
    );

    for good in ["part.mitcad", "a/b/c.mitcad", "a b/ä.mitcad", "x:y.mitcad"] {
        assert!(is_project_path(good), "{good}");
    }
    for bad in [
        "",
        "/a",
        "a//b",
        "a/./b",
        "a/../b",
        "..",
        "a\\b",
        "C:/a",
        "c:",
        "a\nb",
        &"a".repeat(1025),
    ] {
        assert!(!is_project_path(bad), "{bad}");
    }
}

#[test]
fn listings_and_push_lines_are_read_strictly() {
    let (a, b) = (lock_id("a.mitcad"), lock_id("b.mitcad"));
    let oid = "0123456789abcdef0123456789abcdef01234567";
    let listing = locks::parse_listing(
        &format!(
            "{oid}\trefs/heads/main\r\n\
             {oid}\trefs/mitcad/locks/{a}\n\
             {oid}\trefs/mitcad/lock-requests/{b}/{SB}\n\
             {oid}\trefs/mitcad/locks/probe-0123456789abcdef\n\
             {oid}\trefs/heads/other\n\
             nonsense\trefs/mitcad/locks/{b}\n\
             {oid} refs/mitcad/locks/{b}\n\
             \n"
        ),
        "refs/heads/main",
    );
    assert_eq!(listing.head.map(|id| id.to_string()).as_deref(), Some(oid));
    let names: Vec<LockRefName> = listing.refs.keys().cloned().collect();
    assert_eq!(
        names,
        [
            LockRefName::Lock(a.clone()),
            LockRefName::Request(b.clone(), SB.to_owned())
        ]
    );

    let line = locks::porcelain_line;
    assert_eq!(
        line(&format!("*\t{oid}:refs/mitcad/locks/{a}\t[new reference]")),
        Some((
            '*',
            format!("refs/mitcad/locks/{a}").as_str(),
            "[new reference]"
        ))
    );
    assert_eq!(
        line(&format!(
            "!\t{oid}:refs/mitcad/locks/{a}\t[rejected] (stale info)"
        )),
        Some((
            '!',
            format!("refs/mitcad/locks/{a}").as_str(),
            "[rejected] (stale info)"
        ))
    );
    assert_eq!(
        line(&format!("-\t:refs/mitcad/locks/{a}\t[deleted]")),
        Some(('-', format!("refs/mitcad/locks/{a}").as_str(), "[deleted]"))
    );
    for other in [
        "To /srv/remote.git",
        "Done",
        "",
        "!!\ta:b\tx",
        "*\tnocolon\tx",
    ] {
        assert_eq!(line(other), None, "{other}");
    }
}

/// A small generator of pseudo-random numbers (xorshift64*), seeded, so
/// that a failure can be repeated.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// `base` mutated a few times: bytes flipped, replaced, inserted or
/// removed, a piece repeated or cut off, a token replaced by another JSON
/// value.
fn mutated(random: &mut Random, base: &[u8]) -> Vec<u8> {
    const TOKENS: [&[u8]; 14] = [
        b"null",
        b"true",
        b"-1",
        b"1e999",
        b"18446744073709551616",
        b"\"\"",
        b"\"\\u202e\"",
        b"\"\\ud800\"",
        b"[[[[[[[[[[[[[[[[",
        b"{}",
        b"\"2026-10-08T13:40:00Z\"",
        b"\"aaaaaaaa-0000-4000-8000-000000000001\"",
        b"\"..\"",
        b"0.5",
    ];
    let mut data = base.to_vec();
    for _ in 0..1 + random.below(4) {
        let at = random.below(data.len() + 1);
        match random.below(8) {
            0 if at < data.len() => data[at] ^= 1 << random.below(8),
            1 if at < data.len() => data[at] = random.next() as u8,
            2 => data.insert(at, random.next() as u8),
            3 if at < data.len() => {
                data.remove(at);
            }
            4 => data.truncate(at),
            5 => {
                let end = (at + random.below(64)).min(data.len());
                let piece = data[at..end].to_vec();
                data.splice(at..at, piece);
            }
            6 => {
                let end = (at + random.below(12)).min(data.len());
                let token = TOKENS[random.below(TOKENS.len())];
                data.splice(at..end, token.iter().copied());
            }
            _ => {
                let token = TOKENS[random.below(TOKENS.len())];
                let repeat = 1 + random.below(2000);
                let filler: Vec<u8> = token
                    .iter()
                    .copied()
                    .cycle()
                    .take(token.len() * repeat)
                    .collect();
                data.splice(at..at, filler);
            }
        }
    }
    data
}

#[test]
fn mutated_and_truncated_files_are_read_or_dropped_never_a_panic() {
    let mut random = Random(0x6d69_7463_6164_2389);
    let lock = sample_lock("parts/bracket.mitcad").to_json();
    let request = sample_request("parts/bracket.mitcad").to_json();
    let (mut read, mut dropped) = (0, 0);
    for round in 0..20_000 {
        let lock_data = mutated(&mut random, &lock);
        let request_data = mutated(&mut random, &request);
        for result in [
            read_lock(&lock_data).map(|_| ()),
            read_request(&request_data).map(|_| ()),
        ] {
            match result {
                Ok(()) => read += 1,
                Err(_) => dropped += 1,
            }
        }
        // Every truncation of the originals is dropped (none is whole).
        if round < lock.len() - 2 {
            assert!(read_lock(&lock[..round]).is_err(), "{round}");
        }
        if round < request.len() - 2 {
            assert!(read_request(&request[..round]).is_err(), "{round}");
        }
    }
    assert!(
        read > 100 && dropped > 10_000,
        "read {read}, dropped {dropped}"
    );
    // What is read is clean and within the limits.
    for _ in 0..5_000 {
        if let Ok(lock) = read_lock(&mutated(&mut random, &lock)) {
            assert!(is_project_path(&lock.path));
            assert!(lock.owner.name.chars().count() <= locks::MAX_NAME);
            assert_eq!(clean_text(&lock.owner.name), lock.owner.name);
            assert!((1..=120).contains(&lock.idle_minutes));
            assert!((5..=600).contains(&lock.poll_seconds));
            // Written back by Mitcad, it is read again.
            let again = read_lock(&lock.to_json()).unwrap();
            assert_eq!((again.path, again.owner), (lock.path, lock.owner));
        }
    }
}

#[test]
fn a_lock_mitcad_writes_always_fits() {
    let mut lock = sample_lock("part.mitcad");
    lock.owner = Person::new(&"\u{1F600}".repeat(150), &"e".repeat(300));
    assert_eq!(lock.owner.name.chars().count(), locks::MAX_NAME);
    assert_eq!(lock.owner.email.chars().count(), locks::MAX_EMAIL);
    let ids: Vec<String> = (0..80).map(|i| format!("{i:040x}")).collect();
    lock.requests_seen = ids.clone();
    lock.answers = ids
        .iter()
        .map(|id| {
            let answer = Answer {
                answer: AnswerKind::Declined,
                until: None,
                message: Some("\u{1F600}".repeat(locks::MAX_MESSAGE)),
            };
            (id.clone(), answer)
        })
        .collect();
    let written = lock.to_json();
    assert!(written.len() <= MAX_FILE_SIZE, "{}", written.len());
    let again = read_lock(&written).unwrap();
    assert_eq!(again.requests_seen.len(), locks::MAX_REQUEST_IDS);
    assert_eq!(again.requests_seen, ids[16..]);
    assert!(!again.answers.is_empty() && again.answers.len() < 20);
    // The newest answers stay.
    assert!(again.answers.contains_key(&ids[79]));
}
