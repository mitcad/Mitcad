// SPDX-License-Identifier: MIT
//! How many UI entries run at once without disturbing each other
//! (mitcad#790 roadmap step 5, mitcad#872, mitcad#2124), learnt from the
//! ordinary runs, never from a measuring run of its own.
//!
//! A UI entry is a test of the UI kind (`profile::Kind::Ui`: `app.ui-*`,
//! labelled `ui`), or a UI test that `tools/check-all.sh` runs itself and
//! hands over with `--record-ui`. Each of its runs records how many other UI
//! entries ran beside it at most (`Run::beside`). A UI entry that failed
//! (or timed out) with others beside it and then passed with fewer beside
//! it (the runner reruns it alone) failed for the company, not for the
//! code: a **contention failure**.
//!
//! - **The machine's cap**: UI entries at once. It is the bound
//!   (`--ui-max`, else `MITCAD_UI_JOBS`), one value for the scheduler,
//!   `--ui-cap` and the run's report (`machine_cap`); without either there
//!   is no cap. The scheduler admits a UI entry, as any test, only while
//!   its predicted memory fits the budget beside the tests running
//!   (`schedule::admit`). Contention
//!   failures never lower it (mitcad#2124: a 12-core 30 GB VM with
//!   `MITCAD_UI_JOBS=6` fell to 2 by a few load-sensitive tests and never
//!   grew back). A cap that an older runner learnt lower into the profile
//!   file (`ui_caps`) is not read: the file's next save drops it.
//! - **A test's room** (in its profile): the other UI entries it may have
//!   beside it. A contention failure beside `b` others halves it to `b / 2`;
//!   `ROOM_GROW_PASSES` passes with a full room beside it grow it by one. So
//!   a test sensitive to load waits until few enough run, while the others
//!   go on at the cap.
//!
//! The scheduler (`schedule::admit`) starts a UI entry only while the UI
//! entries running are fewer than the cap and neither it nor any running UI
//! entry would have more beside it than its room (`admits`). A machine
//! opts in by giving the bound: without `--ui-max` and `MITCAD_UI_JOBS` the
//! runs record and learn the rooms, but the scheduler holds UI entries to
//! no count cap and no room (only the memory budget and the CPU rules, as
//! every test) and reruns none alone (`scheduled_cap`, `scheduled_room`);
//! the report says so, "no cap (memory budget)" (mitcad#2260: the Windows
//! VM runs about 15 at once so).
//!
//! Owner 22.11.2026: do not change how many UI entries run at once (the
//! default without a bound, the bound's handling, MITCAD_UI_JOBS) without
//! the owner's explicit permission; 934331a20 capped the Windows VM at 4
//! unasked and slowed every Windows round several times over (mitcad#2260).

use std::collections::BTreeMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::options::Options;
use crate::profile::{self, ResultKind, Run};

/// Passes with its room full after which a test's room grows by one.
pub const ROOM_GROW_PASSES: u32 = 3;
/// What a contention failure takes back of the failure's +10: it counts
/// +2, as the other failures of the environment.
pub const CONTENTION_REFUND: f64 = 8.0;
/// The platform (`std::env::consts::OS`), for the release fingerprints.
pub const PLATFORM: &str = std::env::consts::OS;

/// The bound of the cap: `--ui-max`, else `MITCAD_UI_JOBS` (a number of at
/// least 1); None without either, and then there is no cap.
///
/// Owner 22.11.2026: do not change how many UI entries run at once (the
/// default without a bound, the bound's handling, MITCAD_UI_JOBS) without
/// the owner's explicit permission; 934331a20 capped the Windows VM at 4
/// unasked and slowed every Windows round several times over (mitcad#2260).
pub fn bound(given: Option<usize>, variable: Option<&str>) -> Option<usize> {
    let from_variable = variable.and_then(|v| v.trim().parse::<usize>().ok());
    given.or(from_variable).filter(|&n| n > 0)
}

/// The machine's cap: the bound, at least 1; None without a bound, and
/// then the scheduler holds UI entries to no count, only to the memory
/// budget and the CPU rules, as every test (mitcad#2260). The one value
/// the scheduler, `--ui-cap` and the run's report use. It follows the
/// bound only; contention failures lower the tests' rooms, never the cap
/// (mitcad#2124).
pub fn machine_cap(bound: Option<usize>) -> Option<usize> {
    bound.map(|b| b.max(1))
}

/// The UI entries at once the scheduler holds to (`Limits::ui_max`): the
/// cap, or no limit (`usize::MAX`) without one.
pub fn scheduled_cap(cap: Option<usize>) -> usize {
    cap.unwrap_or(usize::MAX)
}

/// The cap as `--ui-cap` prints it: the number, or that there is none.
pub fn cap_text(cap: Option<usize>) -> String {
    cap.map_or_else(|| "no cap (memory budget)".to_owned(), |c| c.to_string())
}

/// The room the scheduler holds a test to: its learnt room, or none
/// without a bound.
pub fn scheduled_room(room: Option<u32>, bound: Option<usize>) -> Option<usize> {
    bound.and(room).map(|r| r as usize)
}

/// Whether a UI entry may start: the UI entries running (`running`) are
/// fewer than the cap, and neither it (`own_room`) nor any of them (the
/// smallest room among them, `running_room`) would have more UI entries
/// beside it than its room.
pub fn admits(
    running: usize,
    running_room: Option<usize>,
    own_room: Option<usize>,
    cap: usize,
) -> bool {
    let fits = |room: Option<usize>| room.is_none_or(|r| running <= r);
    running < cap && fits(own_room) && fits(running_room)
}

fn failed(result: ResultKind) -> bool {
    matches!(result, ResultKind::Failed | ResultKind::Timeout)
}

/// A contention failure: `previous` failed (or timed out) with `b` > 0
/// other UI entries beside it and `run` passed with fewer beside it. The
/// `b`, or None.
pub fn contention(previous: &Run, run: &Run) -> Option<u32> {
    let beside = previous.beside.filter(|&b| b > 0)?;
    let fewer = run.beside.is_some_and(|n| n < beside);
    let passed = run.result == ResultKind::Passed;
    (failed(previous.result) && passed && fewer).then_some(beside)
}

/// A test's room and its passes with the room full, after `run` (its run
/// before: `previous`): a contention failure beside `b` others halves the
/// room to `b / 2` (0: no other UI entry beside it); a pass with at least
/// the room beside it counts, and `ROOM_GROW_PASSES` of them grow the room
/// by one.
pub fn update_room(
    room: Option<u32>,
    passes: u32,
    previous: Option<&Run>,
    run: &Run,
) -> (Option<u32>, u32) {
    if let Some(beside) = previous.and_then(|p| contention(p, run)) {
        let halved = beside / 2;
        return (Some(room.map_or(halved, |r| r.min(halved))), 0);
    }
    match (room, run.beside) {
        (Some(r), Some(beside)) if run.result == ResultKind::Passed && beside >= r => {
            if passes + 1 >= ROOM_GROW_PASSES {
                (Some(r + 1), 0)
            } else {
                (Some(r), passes + 1)
            }
        }
        _ => (room, passes),
    }
}

/// What one run tells of its UI entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunFacts {
    /// The most UI entries at once.
    pub at_once: usize,
    /// UI entries that failed beside others and then passed with fewer.
    pub contended: usize,
    /// UI entries that failed beside others and did not pass with fewer
    /// afterwards (a real failure, or one that fails alone too).
    pub unresolved: usize,
}

/// The facts of a run from its records in the order they ended (a test's
/// rerun after its first attempt); the runs without `beside` are not UI
/// entries.
pub fn run_facts(records: &[(String, Run)]) -> RunFacts {
    let mut by_test: BTreeMap<&str, Vec<&Run>> = BTreeMap::new();
    for (name, run) in records {
        if run.beside.is_some() {
            by_test.entry(name.as_str()).or_default().push(run);
        }
    }
    let mut facts = RunFacts::default();
    for runs in by_test.values() {
        for (k, run) in runs.iter().enumerate() {
            let beside = run.beside.unwrap_or(0) as usize;
            facts.at_once = facts.at_once.max(beside + 1);
            if !failed(run.result) || beside == 0 {
                continue;
            }
            let next = runs.get(k + 1);
            if next.is_some_and(|n| contention(run, n).is_some()) {
                facts.contended += 1;
            } else {
                facts.unresolved += 1;
            }
        }
    }
    facts
}

/// One line on what a run taught: the cap it ran at, the most UI entries
/// at once, and the contention failures (whose rooms were lowered).
pub fn summary(facts: &RunFacts, cap: Option<usize>) -> String {
    let cap = match cap {
        Some(c) => format!("cap {c}"),
        None => cap_text(None),
    };
    format!(
        "UI entries at once: {cap}; this run at most {}, {} failed beside others and passed with fewer beside them (their rooms lowered), {} failed beside others",
        facts.at_once, facts.contended, facts.unresolved
    )
}

/// The UI runs `tools/check-all.sh` hands over (`--record-ui FILE`): a line
/// per run in the order they ended, `name<TAB>exit status<TAB>seconds<TAB>
/// other UI entries beside it at most`; 0 passed, 124 and 137 (timeout's)
/// timed out, 77 skipped, others failed. Empty lines and lines starting
/// with `#` are left out.
pub fn parse_ui_runs(text: &str, when: u64) -> Result<Vec<(String, Run)>, String> {
    let mut runs = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let [name, status, seconds, beside] = fields[..] else {
            return Err(format!(
                "not a UI run (name, status, seconds, beside): {line}"
            ));
        };
        let bad = || format!("not a UI run: {line}");
        let status: i32 = status.trim().parse().map_err(|_| bad())?;
        let wall: f64 = seconds.trim().parse().map_err(|_| bad())?;
        let beside: u32 = beside.trim().parse().map_err(|_| bad())?;
        let result = match status {
            0 => ResultKind::Passed,
            77 => ResultKind::Skipped,
            124 | 137 => ResultKind::Timeout,
            _ => ResultKind::Failed,
        };
        let run = Run {
            when,
            result,
            wall,
            cpu: 0.0,
            peak: 0,
            load: 0.0,
            limit: None,
            beside: Some(beside),
        };
        runs.push((name.trim().to_owned(), run));
    }
    Ok(runs)
}

/// `--ui-rooms`: each UI entry with a learnt room, a line each, `name<TAB>
/// room` (the other UI entries it may have beside it), by name; for
/// tools/check-all.sh, which keeps them for its own UI tests as the
/// scheduler does (`admits`, mitcad#872).
pub fn rooms_table(profiles: &profile::Profiles) -> String {
    profiles
        .tests
        .iter()
        .filter_map(|(name, test)| test.ui_room.map(|room| format!("{name}\t{room}")))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `--ui-cap` (prints the machine's cap, `machine_cap`), `--ui-rooms`
/// (`rooms_table`) and `--record-ui FILE` (adds check-all's UI runs to the
/// profiles, which lowers or grows their rooms, and prints what the run
/// taught): the output.
pub fn command(o: &Options) -> Result<String, String> {
    let variable = std::env::var("MITCAD_UI_JOBS").ok();
    let cap = machine_cap(bound(o.ui_max, variable.as_deref()));
    let path = o.profiles.clone().or_else(profile::default_path);
    let Some(file) = &o.record_ui else {
        if !o.ui_rooms {
            return Ok(cap_text(cap));
        }
        let profiles = match &path {
            Some(path) => profile::load(path)?,
            None => profile::Profiles::default(),
        };
        return Ok(rooms_table(&profiles));
    };
    let text =
        fs::read_to_string(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    let runs = parse_ui_runs(&text, since.map_or(0, |d| d.as_secs()))?;
    let facts = run_facts(&runs);
    if let Some(path) = path.filter(|_| o.record) {
        let load = o.sensitive_load;
        profile::update(&path, |profiles| {
            for (name, run) in &runs {
                let test = profiles.tests.entry(name.clone()).or_default();
                profile::record(test, run.clone(), load);
            }
        })?;
    }
    Ok(summary(&facts, cap))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui(result: ResultKind, beside: u32) -> Run {
        Run {
            when: 0,
            result,
            wall: 10.0,
            cpu: 5.0,
            peak: 1 << 28,
            load: 0.5,
            limit: None,
            beside: Some(beside),
        }
    }

    fn records(runs: &[(&str, Run)]) -> Vec<(String, Run)> {
        runs.iter()
            .map(|(name, run)| ((*name).to_owned(), run.clone()))
            .collect()
    }

    const PASSED: ResultKind = ResultKind::Passed;
    const FAILED: ResultKind = ResultKind::Failed;

    #[test]
    fn the_bound_comes_from_the_option_or_the_variable() {
        assert_eq!(bound(Some(6), Some("12")), Some(6));
        assert_eq!(bound(None, Some("12")), Some(12));
        assert_eq!(bound(None, Some(" 3 ")), Some(3));
        assert_eq!(bound(None, Some("many")), None);
        assert_eq!(bound(None, Some("0")), None);
        assert_eq!(bound(None, None), None);
        // The machine's cap is the bound; without one, none.
        assert_eq!(machine_cap(None), None);
        assert_eq!(machine_cap(Some(12)), Some(12));
        assert_eq!(machine_cap(Some(1)), Some(1));
        assert_eq!(machine_cap(Some(0)), Some(1));
    }

    #[test]
    fn without_a_bound_no_cap_and_no_room_applies() {
        // No --ui-max, no MITCAD_UI_JOBS: no cap, no rooms (mitcad#2260).
        let cap = scheduled_cap(machine_cap(None));
        assert_eq!(cap, usize::MAX);
        assert_eq!(scheduled_room(Some(1), None), None);
        assert!(admits(40, None, scheduled_room(Some(0), None), cap));
        assert_eq!(cap_text(machine_cap(None)), "no cap (memory budget)");
        // With a bound: the bound itself, the rooms as learnt.
        assert_eq!(scheduled_cap(machine_cap(Some(12))), 12);
        assert_eq!(scheduled_cap(machine_cap(Some(6))), 6);
        assert_eq!(cap_text(machine_cap(Some(6))), "6");
        assert_eq!(scheduled_room(Some(1), Some(12)), Some(1));
        assert_eq!(scheduled_room(None, Some(12)), None);
    }

    #[test]
    fn rooms_and_the_cap_admit_ui_entries() {
        // The cap counts the UI entries running.
        assert!(admits(1, None, None, 2));
        assert!(!admits(2, None, None, 2));
        // Its own room: with room 1 it may join one UI entry, not two.
        assert!(admits(1, None, Some(1), 4));
        assert!(!admits(2, None, Some(1), 4));
        // Room 0: only with no other UI entry running.
        assert!(admits(0, None, Some(0), 4));
        assert!(!admits(1, None, Some(0), 4));
        // A running entry's room holds back the others too.
        assert!(!admits(1, Some(0), None, 4));
        assert!(admits(1, Some(1), None, 4));
        assert!(!admits(2, Some(1), None, 4));
    }

    #[test]
    fn a_failure_beside_others_then_a_pass_alone_is_contention() {
        assert_eq!(contention(&ui(FAILED, 5), &ui(PASSED, 0)), Some(5));
        let timeout = ui(ResultKind::Timeout, 3);
        assert_eq!(contention(&timeout, &ui(PASSED, 1)), Some(3));
        // Failing alone, failing again alone, passing with as many beside:
        // not the company's fault.
        assert_eq!(contention(&ui(FAILED, 0), &ui(PASSED, 0)), None);
        assert_eq!(contention(&ui(FAILED, 5), &ui(FAILED, 0)), None);
        assert_eq!(contention(&ui(FAILED, 5), &ui(PASSED, 5)), None);
        // Not UI entries (no `beside`).
        let mut plain = ui(FAILED, 0);
        plain.beside = None;
        assert_eq!(contention(&plain, &ui(PASSED, 0)), None);
    }

    #[test]
    fn contention_halves_a_room_and_full_passes_grow_it() {
        let failed = ui(FAILED, 11);
        let alone = ui(PASSED, 0);
        // ui-pick failed beside 11 others and passed alone: room 5.
        assert_eq!(update_room(None, 0, Some(&failed), &alone), (Some(5), 0));
        // Again beside 5: room 2; a smaller room stays.
        let five = ui(FAILED, 5);
        assert_eq!(update_room(Some(5), 2, Some(&five), &alone), (Some(2), 0));
        assert_eq!(update_room(Some(1), 0, Some(&five), &alone), (Some(1), 0));
        // Beside 1: room 0, alone among the UI entries.
        let one = ui(FAILED, 1);
        assert_eq!(update_room(Some(2), 0, Some(&one), &alone), (Some(0), 0));
        // Passes with the room full count; three grow the room by one.
        let mut state = (Some(2), 0);
        for _ in 0..ROOM_GROW_PASSES {
            state = update_room(state.0, state.1, None, &ui(PASSED, 2));
        }
        assert_eq!(state, (Some(3), 0));
        // Passes with fewer beside, and other results, do not.
        assert_eq!(update_room(Some(3), 1, None, &ui(PASSED, 1)), (Some(3), 1));
        assert_eq!(update_room(Some(3), 1, None, &ui(FAILED, 3)), (Some(3), 1));
        // A room 0 grows after passes alone, so that it is tried again.
        let mut zero = (Some(0), 0);
        for _ in 0..ROOM_GROW_PASSES {
            zero = update_room(zero.0, zero.1, None, &ui(PASSED, 0));
        }
        assert_eq!(zero, (Some(1), 0));
        // Without a mark nothing changes.
        assert_eq!(update_room(None, 0, None, &ui(PASSED, 8)), (None, 0));
    }

    #[test]
    fn a_run_tells_its_most_at_once_and_its_failures() {
        // Today's Linux check: twelve at once, ui-pick and ui-render failed
        // and passed alone, ui-x failed beside others and alone.
        let runs = records(&[
            ("ui-a", ui(PASSED, 11)),
            ("ui-pick", ui(FAILED, 11)),
            ("ui-render", ui(FAILED, 9)),
            ("ui-x", ui(FAILED, 4)),
            ("ui-pick", ui(PASSED, 0)),
            ("ui-render", ui(PASSED, 0)),
            ("ui-x", ui(FAILED, 0)),
        ]);
        let facts = run_facts(&runs);
        assert_eq!(
            facts,
            RunFacts {
                at_once: 12,
                contended: 2,
                unresolved: 1
            }
        );
        // Tests that are not UI entries tell nothing.
        let mut plain = ui(FAILED, 0);
        plain.beside = None;
        let facts = run_facts(&records(&[("core.model", plain)]));
        assert_eq!(facts, RunFacts::default());
        // A failure alone is no failure beside others.
        let facts = run_facts(&records(&[("ui-a", ui(FAILED, 0))]));
        assert_eq!((facts.at_once, facts.unresolved), (1, 0));
    }

    #[test]
    fn check_all_gets_the_rooms() {
        let mut profiles = profile::Profiles::default();
        assert_eq!(rooms_table(&profiles), "");
        for (name, room) in [("ui-sweep", Some(4)), ("ui-pick", Some(0)), ("ui-a", None)] {
            let test = profiles.tests.entry(name.to_owned()).or_default();
            test.ui_room = room;
        }
        assert_eq!(rooms_table(&profiles), "ui-pick\t0\nui-sweep\t4");
    }

    /// A fresh folder of the test's own.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mitcad-testrun-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The options of a command line; a path may hold spaces.
    fn options(args: &[&str]) -> Options {
        let args: Vec<String> = args.iter().map(|&a| a.to_owned()).collect();
        crate::options::parse(&args).unwrap()
    }

    /// A profile file an older runner wrote: a cap learnt down to 2 by
    /// contention failures (mitcad#2124's VM, `MITCAD_UI_JOBS=6`).
    const OLD_PROFILES: &str = r#"{
  "format": 1,
  "tests": {},
  "ui_caps": { "linux": { "cap": 2, "clean_runs": 0 }, "windows": { "cap": 3 } }
}"#;

    #[test]
    fn an_old_learnt_cap_reads_as_the_bound() {
        let dir = scratch("old-cap");
        let path = dir.join("profiles.json");
        fs::write(&path, OLD_PROFILES).unwrap();
        // The old file still reads (its caps are left out) ...
        assert!(profile::load(&path).unwrap().tests.is_empty());
        // ... and --ui-cap prints the bound, not the old cap 2.
        let p = path.to_str().unwrap();
        let cap = command(&options(&["--ui-cap", "--ui-max", "6", "--profiles", p]));
        assert_eq!(cap.as_deref(), Ok("6"));
        let cap = command(&options(&["--ui-cap", "--ui-max", "12", "--profiles", p]));
        assert_eq!(cap.as_deref(), Ok("12"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn contention_failures_lower_their_rooms_and_leave_the_cap_at_the_bound() {
        let dir = scratch("contention");
        let path = dir.join("profiles.json");
        fs::write(&path, OLD_PROFILES).unwrap();
        // Six at once; three tests failed beside others and passed alone,
        // which the old rule took for a reason to drop the machine's cap.
        let runs = dir.join("ui-runs.tsv");
        let text = "ui-a\t0\t30\t5\n\
                    ui-pick\t1\t40\t5\n\
                    ui-render\t1\t40\t4\n\
                    ui-sweep\t124\t300\t3\n\
                    ui-pick\t0\t38\t0\n\
                    ui-render\t0\t35\t0\n\
                    ui-sweep\t0\t60\t0\n";
        fs::write(&runs, text).unwrap();
        let facts = run_facts(&parse_ui_runs(text, 0).unwrap());
        assert_eq!(
            (facts.at_once, facts.contended, facts.unresolved),
            (6, 3, 0)
        );
        let (r, p) = (runs.to_str().unwrap(), path.to_str().unwrap());
        let record = ["--record-ui", r, "--ui-max", "6", "--profiles", p];
        let line = command(&options(&record)).unwrap();
        assert!(line.starts_with("UI entries at once: cap 6;"), "{line}");
        assert!(!line.contains("->"), "{line}");
        // Each contended test's room is halved from what it had beside it;
        // the test that passed keeps none.
        let profiles = profile::load(&path).unwrap();
        let room = |name: &str| profiles.tests[name].ui_room;
        assert_eq!(room("ui-pick"), Some(2));
        assert_eq!(room("ui-render"), Some(2));
        assert_eq!(room("ui-sweep"), Some(1));
        assert_eq!(room("ui-a"), None);
        assert_eq!(
            rooms_table(&profiles),
            "ui-pick\t2\nui-render\t2\nui-sweep\t1"
        );
        // The machine's cap stays at the bound, and the old learnt cap is
        // gone from the file after the save.
        let cap = command(&options(&["--ui-cap", "--ui-max", "6", "--profiles", p]));
        assert_eq!(cap.as_deref(), Ok("6"));
        let saved = fs::read_to_string(&path).unwrap();
        assert!(!saved.contains("ui_caps"), "{saved}");
        // The scheduler holds UI entries to the bound and the rooms.
        let six = scheduled_cap(machine_cap(Some(6)));
        assert_eq!(six, 6);
        assert!(admits(5, None, None, six));
        assert!(!admits(
            3,
            None,
            scheduled_room(room("ui-pick"), Some(6)),
            6
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn check_all_hands_over_its_ui_runs() {
        let text = "# name\tstatus\tseconds\tbeside\n\
                    ui-pick\t1\t40.5\t11\n\
                    \n\
                    ui-render\t124\t300\t9\r\n\
                    ui-pick\t0\t38\t0\n\
                    ui-skip\t77\t1\t2\n";
        let runs = parse_ui_runs(text, 7).unwrap();
        let kinds: Vec<(&str, ResultKind, Option<u32>)> = runs
            .iter()
            .map(|(n, r)| (n.as_str(), r.result, r.beside))
            .collect();
        assert_eq!(
            kinds,
            [
                ("ui-pick", FAILED, Some(11)),
                ("ui-render", ResultKind::Timeout, Some(9)),
                ("ui-pick", PASSED, Some(0)),
                ("ui-skip", ResultKind::Skipped, Some(2)),
            ]
        );
        assert_eq!((runs[0].1.when, runs[0].1.wall), (7, 40.5));
        assert_eq!(run_facts(&runs).contended, 1);
        assert!(parse_ui_runs("ui-pick\t0\t1\n", 0).is_err());
        assert!(parse_ui_runs("ui-pick\tok\t1\t0\n", 0).is_err());
    }
}
