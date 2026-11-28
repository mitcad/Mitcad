// SPDX-License-Identifier: MIT
//! The end report: totals, resource use, the critical path, the longest
//! tests, the failures by kind and the profiles that changed much; and the
//! results as a table.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use crate::profile::{GIB, MIB, ResultKind};
use crate::schedule::{Link, Span, critical_path};

/// How one test of the run ended.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub name: String,
    pub kind: ResultKind,
    /// The kind of the first attempt, for a test rerun alone after its
    /// memory limit or (a UI entry) after failing beside other UI entries.
    pub first: Option<ResultKind>,
    /// Seconds from the run's start.
    pub start: f64,
    pub end: f64,
    pub wall: f64,
    pub cpu: f64,
    pub peak: u64,
    pub limit: Option<u64>,
    /// Predicted wall time and memory.
    pub predicted_wall: f64,
    pub predicted_memory: u64,
    /// Ran in the priority lane (scheduling mode, a failure score of at
    /// least the lane's threshold: `schedule::in_lane`).
    pub lane: bool,
    pub log: Option<PathBuf>,
    pub note: String,
    /// The tests it waited for (indices into the outcomes).
    pub before: Vec<usize>,
    /// Ran alone (RUN_SERIAL, or a rerun after its memory limit).
    pub alone: bool,
    /// Its resource locks.
    pub locks: Vec<String>,
    /// A test that did not pass: the lines of its log that name its
    /// failures (`ctest::failure_lines`, mitcad#2614).
    pub quotes: Vec<String>,
}

/// The run as a whole.
#[derive(Clone, Debug, Default)]
pub struct Summary<'a> {
    pub mode: &'a str,
    pub method: &'a str,
    pub elapsed: f64,
    pub cores: usize,
    pub memory: u64,
    pub budget: u64,
    /// The system's CPU load over the run (time-weighted), 0 to 1.
    pub mean_load: f64,
    /// The memory in use over the run (time-weighted), a share of `memory`
    /// (the machine's, or on Linux the lowest cgroup limit the run is
    /// under).
    pub mean_memory_used: Option<f64>,
    pub outcomes: &'a [Outcome],
    /// Tests whose profile changed much, and how.
    pub changes: &'a [(String, String)],
    /// Why the run stopped starting tests early.
    pub stop: Option<&'a str>,
    pub profiles: Option<&'a Path>,
    /// What the run taught of the UI entries at once (`concurrency`).
    pub ui: Option<&'a str>,
    /// The free budget over the run (`--memory-budget free`, mitcad#872).
    pub free: Option<&'a str>,
    /// A release gate's geometry tests skipped by their fingerprints and
    /// the passes kept (`fingerprint::Plan::text`, mitcad#1822).
    pub release: Option<&'a str>,
    /// The first claims on memory that held other tests back
    /// (`claims_line`, mitcad#2177).
    pub claims: Option<&'a str>,
}

/// The report's line of the first claims on memory (`schedule::claimant`,
/// mitcad#2177): each test whose claim held tests ranked below it back,
/// since when, when it started and its prediction. `claims`: the index
/// into `outcomes` and the seconds from the run's start. None without one.
pub fn claims_line(outcomes: &[Outcome], claims: &[(usize, f64)]) -> Option<String> {
    let parts: Vec<String> = claims
        .iter()
        .filter_map(|&(i, at)| {
            let o = outcomes.get(i)?;
            Some(format!(
                "{} from {}, started {} ({} predicted)",
                o.name,
                clock(at),
                clock(o.start),
                size(o.predicted_memory)
            ))
        })
        .collect();
    (!parts.is_empty()).then(|| {
        format!(
            "First claim on memory (the tests ranked below left it room): {}",
            parts.join("; ")
        )
    })
}

/// A duration as "41 min 12 s".
pub fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours} h {minutes} min")
    } else if minutes > 0 {
        format!("{minutes} min {seconds} s")
    } else {
        format!("{seconds} s")
    }
}

/// A clock time from the run's start, "m:ss" or "h:mm:ss".
pub fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// A size as "1.2 GiB" or "350 MiB".
pub fn size(bytes: u64) -> String {
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else {
        format!("{:.0} MiB", bytes / MIB)
    }
}

const KINDS: [ResultKind; 7] = [
    ResultKind::Passed,
    ResultKind::Skipped,
    ResultKind::Failed,
    ResultKind::Timeout,
    ResultKind::KilledForMemory,
    ResultKind::Environment,
    ResultKind::NotRun,
];

/// "passed 330, failed 3, ..." (kinds that occur).
pub fn counts(outcomes: &[Outcome]) -> String {
    let mut parts = Vec::new();
    for kind in KINDS {
        let n = outcomes.iter().filter(|o| o.kind == kind).count();
        if n > 0 {
            parts.push(format!("{} {n}", kind.label()));
        }
    }
    if parts.is_empty() {
        "no tests".to_owned()
    } else {
        parts.join(", ")
    }
}

/// The critical path's outcomes, first to last, with why each started
/// when it did.
pub fn critical(outcomes: &[Outcome]) -> Vec<Link> {
    let spans: Vec<Span> = outcomes
        .iter()
        .map(|o| Span {
            start: o.start,
            // Tests that never started end nothing.
            end: if o.kind.measured() { o.end } else { 0.0 },
            before: o.before.clone(),
            alone: o.alone,
            locks: o.locks.clone(),
        })
        .collect();
    critical_path(&spans)
}

/// The tests' use of the machine over the run: time integrals over the
/// run's wall time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Use {
    /// Tests running at once (the sum of their wall times / the run's).
    pub running: f64,
    /// Busy cores of the tests (their CPU times / the run's wall time).
    pub cores: f64,
    /// The running tests' predicted memory, bytes.
    pub predicted: f64,
}

pub fn use_of(outcomes: &[Outcome], elapsed: f64) -> Use {
    if elapsed <= 0.0 {
        return Use::default();
    }
    let mut used = Use::default();
    for o in outcomes.iter().filter(|o| o.kind.measured()) {
        used.running += o.wall / elapsed;
        used.cores += o.cpu / elapsed;
        used.predicted += o.predicted_memory as f64 * o.wall / elapsed;
    }
    used
}

/// The end report.
pub fn render(s: &Summary) -> String {
    let mut out = String::new();
    let outcomes = s.outcomes;
    let _ = writeln!(
        out,
        "mitcad-testrun: {} tests in {} ({}; {} cores, budget {} of {})",
        outcomes.len(),
        duration(s.elapsed),
        s.mode,
        s.cores,
        size(s.budget),
        size(s.memory)
    );
    let _ = writeln!(out, "Measured with {}.", s.method);
    if let Some(free) = s.free {
        let _ = writeln!(out, "{free}");
    }
    if let Some(claims) = s.claims {
        let _ = writeln!(out, "{claims}");
    }
    let _ = writeln!(out, "Results: {}", counts(outcomes));
    if let Some(stop) = s.stop {
        let _ = writeln!(out, "Stopped early: {stop}");
    }
    if let Some(ui) = s.ui {
        let _ = writeln!(out, "{ui}");
    }
    if let Some(release) = s.release {
        let _ = writeln!(out, "{}", release.trim_end());
    }
    let reruns = outcomes.iter().filter(|o| o.first.is_some());
    for o in reruns {
        if let Some(first) = o.first.filter(|&k| k != ResultKind::KilledForMemory) {
            let _ = writeln!(
                out,
                "Rerun alone ({} beside other UI entries): {} {}",
                first.label(),
                o.name,
                o.kind.label()
            );
            continue;
        }
        let limit = o.limit.map(size).unwrap_or_default();
        let _ = writeln!(
            out,
            "Rerun alone after its memory limit: {} {} (limit {limit}, peak {})",
            o.name,
            o.kind.label(),
            size(o.peak)
        );
    }
    let memory = match s.mean_memory_used {
        Some(share) => format!("{:.0} % of the {} in use", share * 100.0, size(s.memory)),
        None => "memory in use unknown".to_owned(),
    };
    let used = use_of(outcomes, s.elapsed);
    let tests_cpu = used.cores / s.cores.max(1) as f64;
    let _ = writeln!(
        out,
        "Use (means over the run): system CPU load {:.0} %, the tests' CPU {:.1} cores ({:.0} %), {memory}",
        s.mean_load * 100.0,
        used.cores,
        tests_cpu * 100.0
    );
    let _ = writeln!(
        out,
        "  {:.1} tests at once, predicted memory {} of the {} budget",
        used.running,
        size(used.predicted as u64),
        size(s.budget)
    );
    let path = critical(outcomes);
    if !path.is_empty() {
        let _ = writeln!(out, "Critical path (the chain that ended the run):");
        let mut previous_end: Option<f64> = None;
        for link in &path {
            let o = &outcomes[link.index];
            let (from, to) = (clock(o.start), clock(o.end));
            let _ = write!(out, "  {from} - {to}  {} ({:.0} s", o.name, o.wall);
            if let Some(end) = previous_end {
                let waited = (o.start - end).max(0.0);
                let _ = write!(out, "; {}", link.why.label());
                if waited >= 1.0 {
                    let _ = write!(out, ", {} more after it ended", duration(waited));
                }
            } else if o.start >= 1.0 {
                let _ = write!(out, "; started at {}", clock(o.start));
            }
            out.push_str(")\n");
            previous_end = Some(o.end);
        }
    }
    let mut longest: Vec<&Outcome> = outcomes.iter().filter(|o| o.kind.measured()).collect();
    longest.sort_by(|a, b| b.wall.total_cmp(&a.wall));
    if !longest.is_empty() {
        let _ = writeln!(out, "Longest tests:");
        for o in longest.iter().take(10) {
            let cores = o.cpu / o.wall.max(0.01);
            let peak = size(o.peak);
            let _ = writeln!(
                out,
                "  {:>6.0} s  {cores:>4.1} cores  {peak:>9}  {}",
                o.wall, o.name
            );
        }
    }
    let failures: Vec<&Outcome> = outcomes.iter().filter(|o| !o.kind.ok()).collect();
    if !failures.is_empty() {
        let _ = writeln!(out, "Failures:");
        for kind in KINDS {
            for o in failures.iter().filter(|o| o.kind == kind) {
                let lane = if o.lane { " [priority lane]" } else { "" };
                let _ = write!(out, "  {:<17} {}{lane}", kind.label(), o.name);
                if !o.note.is_empty() {
                    let _ = write!(out, ": {}", o.note);
                }
                if let Some(log) = &o.log {
                    let _ = write!(out, " (log {})", log.display());
                }
                out.push('\n');
                for quote in &o.quotes {
                    let _ = writeln!(out, "      | {quote}");
                }
            }
        }
    }
    if !s.changes.is_empty() {
        let _ = writeln!(out, "Profiles that changed much:");
        for (name, change) in s.changes {
            let _ = writeln!(out, "  {name}: {change}");
        }
    }
    if let Some(profiles) = s.profiles {
        let _ = writeln!(out, "Profiles: {}", profiles.display());
    }
    out
}

/// The results as tab-separated rows with a header.
pub fn results_tsv(outcomes: &[Outcome]) -> String {
    let mut out = String::from(
        "name\tresult\tstart_s\twall_s\tcpu_s\tpeak_mib\tlimit_mib\tpredicted_s\tpredicted_mib\tlane\n",
    );
    let mib = |bytes: u64| format!("{:.0}", bytes as f64 / MIB);
    for o in outcomes {
        let limit = o.limit.map(mib).unwrap_or_default();
        let _ = writeln!(
            out,
            "{}\t{}\t{:.1}\t{:.1}\t{:.1}\t{}\t{limit}\t{:.1}\t{}\t{}",
            o.name,
            o.kind.label(),
            o.start,
            o.wall,
            o.cpu,
            mib(o.peak),
            o.predicted_wall,
            mib(o.predicted_memory),
            u8::from(o.lane)
        );
    }
    out
}

/// The failure lines of the tests that did not pass (`failures.tsv`,
/// mitcad#2614), tab-separated with a header: the test, its result, a line
/// of its log that names a failure; a row per line, a test without such
/// lines left out. `tools/check-all.sh` quotes them in its summary.
pub fn failures_tsv(outcomes: &[Outcome]) -> String {
    let mut out = String::from("name\tresult\tline\n");
    for o in outcomes.iter().filter(|o| !o.kind.ok()) {
        for quote in &o.quotes {
            let _ = writeln!(out, "{}\t{}\t{quote}", o.name, o.kind.label());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(name: &str, kind: ResultKind, start: f64, end: f64) -> Outcome {
        Outcome {
            name: name.to_owned(),
            kind,
            start,
            end,
            wall: end - start,
            cpu: end - start,
            peak: 1 << 30,
            ..Outcome::default()
        }
    }

    #[test]
    fn durations_and_sizes() {
        assert_eq!(duration(2472.0), "41 min 12 s");
        assert_eq!(duration(7300.0), "2 h 1 min");
        assert_eq!(duration(5.4), "5 s");
        assert_eq!(clock(252.0), "4:12");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(size(3 << 29), "1.5 GiB");
        assert_eq!(size(350 << 20), "350 MiB");
    }

    #[test]
    fn the_report_names_the_path_and_the_failures() {
        let mut outcomes = [
            outcome("core.cargo_build", ResultKind::Passed, 0.0, 252.0),
            outcome("core.model", ResultKind::Passed, 252.5, 570.0),
            outcome("app.ui-x", ResultKind::Failed, 0.0, 100.0),
            outcome("never", ResultKind::NotRun, 600.0, 600.0),
        ];
        outcomes[1].before = vec![0];
        outcomes[2].lane = true;
        outcomes[2].note = "exit 1".into();
        outcomes[2].quotes = vec!["x.cpp:3: check failed: a == b".into()];
        let change = ("core.model".to_owned(), "wall 100 s -> 318 s".to_owned());
        let changes = [change];
        let summary = Summary {
            mode: "schedule",
            method: "a test",
            elapsed: 600.0,
            cores: 8,
            memory: 32 << 30,
            budget: 24 << 30,
            outcomes: &outcomes,
            changes: &changes,
            ..Summary::default()
        };
        let text = render(&summary);
        // The failure with the line of its log that names it (mitcad#2614).
        let quoted = "app.ui-x [priority lane]: exit 1\n      | x.cpp:3: check failed: a == b\n";
        assert!(text.contains(quoted), "{text}");
        assert!(text.contains("4 tests in 10 min 0 s"), "{text}");
        assert!(text.contains("passed 2, failed 1, not run 1"), "{text}");
        assert!(text.contains("0:00 - 4:12  core.cargo_build"), "{text}");
        let model = "4:13 - 9:30  core.model (318 s; waited for its dependency)";
        assert!(text.contains(model), "{text}");
        assert!(text.contains("app.ui-x [priority lane]: exit 1"), "{text}");
        assert!(text.contains("core.model: wall 100 s -> 318 s"), "{text}");
        let path: Vec<usize> = critical(&outcomes).iter().map(|l| l.index).collect();
        assert_eq!(path, [0, 1]);
        let table = results_tsv(&outcomes);
        assert_eq!(table.lines().count(), 5);
        assert!(table.contains("app.ui-x\tfailed\t0.0\t100.0"), "{table}");
        // The lane column: 1 for the test that ran in the lane only.
        let lanes: Vec<&str> = table
            .lines()
            .skip(1)
            .filter_map(|l| l.rsplit('\t').next())
            .collect();
        assert_eq!(lanes, ["0", "0", "1", "0"]);
        // The failures' lines, a row each; a failure without lines (the
        // test that never ran) has none.
        let failures = failures_tsv(&outcomes);
        assert_eq!(
            failures,
            "name\tresult\tline\napp.ui-x\tfailed\tx.cpp:3: check failed: a == b\n"
        );
    }

    #[test]
    fn use_is_integrated_over_the_run() {
        // Three tests overlapping in a 10 s run: two the whole run, one
        // half of it.
        let mut outcomes = [
            outcome("a", ResultKind::Passed, 0.0, 10.0),
            outcome("b", ResultKind::Passed, 0.0, 10.0),
            outcome("c", ResultKind::Failed, 5.0, 10.0),
            outcome("never", ResultKind::NotRun, 10.0, 10.0),
        ];
        for o in &mut outcomes {
            o.predicted_memory = 1 << 30;
        }
        let used = use_of(&outcomes, 10.0);
        assert!((used.running - 2.5).abs() < 1e-9, "{used:?}");
        assert!((used.cores - 2.5).abs() < 1e-9, "{used:?}");
        assert!((used.predicted - 2.5 * GIB).abs() < 1.0, "{used:?}");
        assert_eq!(use_of(&outcomes, 0.0), Use::default());
    }

    #[test]
    fn ui_reruns_and_the_cap_are_reported() {
        let mut outcomes = [
            outcome("app.ui-windows-pick", ResultKind::Passed, 0.0, 30.0),
            outcome("core.big", ResultKind::Passed, 0.0, 30.0),
        ];
        outcomes[0].first = Some(ResultKind::Failed);
        outcomes[1].first = Some(ResultKind::KilledForMemory);
        let line = "UI entries at once: cap 3; this run at most 3";
        let summary = Summary {
            elapsed: 30.0,
            outcomes: &outcomes,
            ui: Some(line),
            ..Summary::default()
        };
        let text = render(&summary);
        assert!(text.contains(line), "{text}");
        let ui = "Rerun alone (failed beside other UI entries): app.ui-windows-pick passed";
        assert!(text.contains(ui), "{text}");
        assert!(
            text.contains("Rerun alone after its memory limit: core.big"),
            "{text}"
        );
    }

    #[test]
    fn the_release_skips_are_reported() {
        let outcomes = [outcome("geometry.sheet", ResultKind::Passed, 0.0, 3.0)];
        let release = "Release fingerprints: 1 of 2 geometry tests skipped, 1 run\n  \
                       Skipped geometry.suite.fillet (last passed in 0.62.0-rc2)\n";
        let summary = Summary {
            elapsed: 3.0,
            outcomes: &outcomes,
            release: Some(release),
            ..Summary::default()
        };
        let text = render(&summary);
        assert!(text.contains(release), "{text}");
    }

    #[test]
    fn the_first_claims_on_memory_are_reported() {
        let mut outcomes = [
            outcome("geometry.small", ResultKind::Passed, 0.0, 30.0),
            outcome("core.cargo_build", ResultKind::Passed, 41.0, 900.0),
        ];
        outcomes[1].predicted_memory = 25 << 30;
        assert_eq!(claims_line(&outcomes, &[]), None);
        let line = claims_line(&outcomes, &[(1, 12.0)]).unwrap();
        let expected = "First claim on memory (the tests ranked below left it room): \
                        core.cargo_build from 0:12, started 0:41 (25.0 GiB predicted)";
        assert_eq!(line, expected);
        let summary = Summary {
            elapsed: 900.0,
            outcomes: &outcomes,
            claims: Some(&line),
            ..Summary::default()
        };
        assert!(render(&summary).contains(expected));
    }

    #[test]
    fn the_path_shows_the_wait_behind_a_test_that_runs_alone() {
        let mut outcomes = [
            outcome("core.cargo_build_release", ResultKind::Passed, 0.0, 188.8),
            outcome(
                "app.ui-windows-sketch-cube",
                ResultKind::Passed,
                189.6,
                251.4,
            ),
        ];
        outcomes[0].alone = true;
        let summary = Summary {
            elapsed: 251.4,
            outcomes: &outcomes,
            ..Summary::default()
        };
        let text = render(&summary);
        assert!(!text.contains("UI entries"), "{text}");
        let release = "0:00 - 3:09  core.cargo_build_release (189 s)";
        assert!(text.contains(release), "{text}");
        let ui =
            "3:10 - 4:11  app.ui-windows-sketch-cube (62 s; waited for a test that runs alone)";
        assert!(text.contains(ui), "{text}");
    }
}
