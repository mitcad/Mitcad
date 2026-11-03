// SPDX-License-Identifier: MIT
//! The machine's test profiles: the last runs of each test (wall time, CPU
//! time, peak memory, the result), its failure score and whether it is
//! timing-sensitive, kept in one JSON file between runs; and the prediction
//! the scheduler takes from them.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::concurrency::{self, UiCap};
use crate::corpus::{self, FileRun};

/// The runs kept per test.
pub const KEPT_RUNS: usize = 5;
/// The format of the profile file.
pub const FORMAT: u32 = 1;
/// Passes under load after which a timing-sensitive test is normal again.
pub const SENSITIVE_CLEAR_PASSES: u32 = 5;

/// How a test's run ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    Passed,
    /// ctest's skip (a skip return code or regular expression).
    Skipped,
    Failed,
    Timeout,
    /// Over its memory limit (Job Object limit, cgroup memory.max).
    KilledForMemory,
    /// Not the test's fault: the disk below its floor, ctest not starting.
    Environment,
    /// Never started: its fixture's setup failed, or the run stopped.
    #[default]
    NotRun,
}

impl ResultKind {
    pub fn ok(self) -> bool {
        matches!(self, ResultKind::Passed | ResultKind::Skipped)
    }

    /// A failure of the environment rather than of the code: these add
    /// less to the failure score.
    pub fn environmental(self) -> bool {
        matches!(
            self,
            ResultKind::Timeout | ResultKind::KilledForMemory | ResultKind::Environment
        )
    }

    /// The run started, so its times and memory tell something.
    pub fn measured(self) -> bool {
        self != ResultKind::NotRun
    }

    pub fn label(self) -> &'static str {
        match self {
            ResultKind::Passed => "passed",
            ResultKind::Skipped => "skipped",
            ResultKind::Failed => "failed",
            ResultKind::Timeout => "timeout",
            ResultKind::KilledForMemory => "killed for memory",
            ResultKind::Environment => "environment",
            ResultKind::NotRun => "not run",
        }
    }
}

/// One measured run of a test.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// Seconds since the Unix epoch at the start.
    pub when: u64,
    pub result: ResultKind,
    /// Wall time, seconds.
    pub wall: f64,
    /// CPU time of the process tree (user and system), seconds.
    pub cpu: f64,
    /// Peak memory of the process tree, bytes.
    pub peak: u64,
    /// The system's mean CPU load during the run, 0 to 1.
    #[serde(default)]
    pub load: f64,
    /// The hard memory limit the run had, bytes.
    #[serde(default)]
    pub limit: Option<u64>,
    /// A UI entry's run: the most other UI entries that ran beside it
    /// (`concurrency`); None for the other tests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beside: Option<u32>,
}

/// What the file keeps of one test.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TestProfile {
    /// The newest last.
    #[serde(default)]
    pub runs: Vec<Run>,
    /// +10 for each failure, +2 for an environment failure, -1 for each
    /// pass, never below 0.
    #[serde(default)]
    pub failure_score: f64,
    /// Failed under load: starts only when the machine is calmer.
    #[serde(default)]
    pub sensitive: bool,
    /// Passes under load since it was marked sensitive.
    #[serde(default)]
    pub loaded_passes: u32,
    /// Times the test went over its memory limit.
    #[serde(default)]
    pub memory_kills: u32,
    /// A UI entry that failed beside other UI entries and passed with
    /// fewer: the other UI entries it may have beside it
    /// (`concurrency::update_room`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_room: Option<u32>,
    /// Passes with its room full since the room last changed.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub room_passes: u32,
    /// A corpus test's files (`corpus`): each file's last time and peak in
    /// a child process, by the key the test gives it (its path, a label).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub files: BTreeMap<String, FileRun>,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The machine the profiles were measured on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    #[serde(default)]
    pub cores: usize,
    #[serde(default)]
    pub memory: u64,
}

/// The whole file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profiles {
    pub format: u32,
    #[serde(default)]
    pub machine: Machine,
    #[serde(default)]
    pub tests: BTreeMap<String, TestProfile>,
    /// The UI entries at once each platform's runs on this machine have
    /// learnt (`concurrency`), by `std::env::consts::OS`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ui_caps: BTreeMap<String, UiCap>,
}

impl Default for Profiles {
    fn default() -> Self {
        Profiles {
            format: FORMAT,
            machine: Machine::default(),
            tests: BTreeMap::new(),
            ui_caps: BTreeMap::new(),
        }
    }
}

/// What the scheduler expects of a test.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prediction {
    /// Wall time, seconds.
    pub wall: f64,
    /// Mean busy cores (CPU time / wall time).
    pub cores: f64,
    /// Peak memory with the margin, bytes.
    pub memory: u64,
    /// Peak memory without the margin (the p90), bytes.
    pub memory_raw: u64,
    /// From history (false: the cautious defaults).
    pub known: bool,
}

/// The defaults for a test with no history, and the margin.
#[derive(Clone, Copy, Debug)]
pub struct Defaults {
    pub wall: f64,
    pub cores: f64,
    pub memory: u64,
    /// Multiplies the p90 of the peak memory.
    pub margin: f64,
}

/// What a test with no history is taken to be, by its name and labels.
/// Cargo test builds share their compiler lock; the native macOS runner
/// also reserves a conservative compiler floor (`macos_policy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `app.ui-*`, or labelled `ui`: 1 GiB, 1 core.
    Ui,
    /// A name with `corpus`, or labelled `corpus`: 2 GiB, 2 cores.
    Corpus,
    /// The rest: the given defaults (512 MiB, 1 core).
    Other,
}

pub fn kind_of(name: &str, labels: &[String]) -> Kind {
    let labelled = |word: &str| labels.iter().any(|l| l.eq_ignore_ascii_case(word));
    if name.starts_with("app.ui-") || labelled("ui") {
        Kind::Ui
    } else if name.contains("corpus") || labelled("corpus") {
        Kind::Corpus
    } else {
        Kind::Other
    }
}

/// The defaults of a kind of test.
pub fn kind_defaults(kind: Kind, base: &Defaults) -> Defaults {
    match kind {
        Kind::Ui => Defaults {
            memory: 1 << 30,
            cores: 1.0,
            ..*base
        },
        Kind::Corpus => Defaults {
            memory: 2 << 30,
            cores: 2.0,
            ..*base
        },
        Kind::Other => *base,
    }
}

/// The largest peak of the started runs over the smallest; 1 without
/// runs.
pub fn peak_spread(runs: &[Run]) -> f64 {
    let peaks = runs.iter().filter(|r| r.result.measured());
    let peaks = peaks.map(|r| r.peak.max(1) as f64);
    let (low, high) = peaks.fold((f64::INFINITY, 0.0_f64), |(low, high), p| {
        (low.min(p), high.max(p))
    });
    if high > 0.0 { high / low } else { 1.0 }
}

/// The median of the values; None for none.
pub fn median(values: &[f64]) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        Some(sorted[middle])
    } else {
        Some((sorted[middle - 1] + sorted[middle]) / 2.0)
    }
}

/// A test with no history borrows the median prediction of its siblings
/// that have one (the tests sharing one of its fixtures or labels); it
/// stays unknown, so that its own first run replaces the guess. A test
/// with a history, or without known siblings, keeps its own.
pub fn borrow(own: Prediction, siblings: &[Prediction]) -> Prediction {
    let known: Vec<&Prediction> = siblings.iter().filter(|p| p.known).collect();
    if own.known || known.is_empty() {
        return own;
    }
    let of = |f: fn(&Prediction) -> f64| {
        let values: Vec<f64> = known.iter().map(|p| f(p)).collect();
        median(&values).unwrap_or(0.0)
    };
    Prediction {
        wall: of(|p| p.wall),
        cores: of(|p| p.cores),
        memory: of(|p| p.memory as f64) as u64,
        memory_raw: of(|p| p.memory_raw as f64) as u64,
        known: false,
    }
}

/// The 90th percentile of the values, interpolated linearly between the
/// nearest ranks; None for none.
pub fn p90(values: &[f64]) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.total_cmp(b));
    let position = 0.9 * (sorted.len() - 1) as f64;
    let low = position.floor() as usize;
    let high = position.ceil() as usize;
    let fraction = position - low as f64;
    Some(sorted[low] + (sorted[high] - sorted[low]) * fraction)
}

/// The prediction from a test's runs: the p90 of the last runs' wall
/// time, cores and peak memory (times the margin), or the defaults when
/// there are none. `processors` is ctest's PROCESSORS property, a floor
/// for the cores.
pub fn predict(runs: &[Run], processors: f64, defaults: &Defaults) -> Prediction {
    let measured: Vec<&Run> = runs.iter().filter(|r| r.result.measured()).collect();
    let floor = processors.max(0.0);
    if measured.is_empty() {
        let memory = defaults.memory;
        return Prediction {
            wall: defaults.wall,
            cores: defaults.cores.max(floor),
            memory,
            memory_raw: memory,
            known: false,
        };
    }
    let walls: Vec<f64> = measured.iter().map(|r| r.wall).collect();
    let cores: Vec<f64> = measured.iter().map(|r| r.cpu / r.wall.max(0.01)).collect();
    let peaks: Vec<f64> = measured.iter().map(|r| r.peak as f64).collect();
    let wall = p90(&walls).unwrap_or(defaults.wall).max(0.01);
    let busy = p90(&cores).unwrap_or(defaults.cores).max(0.05).max(floor);
    let raw = p90(&peaks).unwrap_or(defaults.memory as f64).max(0.0);
    let memory = (raw * defaults.margin).max(64.0 * MIB) as u64;
    Prediction {
        wall,
        cores: busy,
        memory,
        memory_raw: raw as u64,
        known: true,
    }
}

pub const MIB: f64 = 1024.0 * 1024.0;
pub const GIB: f64 = 1024.0 * MIB;

/// The failure score after a run: +10 for a failure, +2 for an
/// environment failure, -1 for a pass, floored at 0; skipped and not run
/// leave it.
pub fn update_score(score: f64, result: ResultKind) -> f64 {
    match result {
        ResultKind::Passed => (score - 1.0).max(0.0),
        ResultKind::Failed => score + 10.0,
        ResultKind::Timeout | ResultKind::KilledForMemory | ResultKind::Environment => score + 2.0,
        ResultKind::Skipped | ResultKind::NotRun => score,
    }
}

/// Whether a test is timing-sensitive after a run, and its passes under
/// load since it was marked: a failure under load (the run's mean load at
/// least `threshold`) of a test that has passed before marks it; then
/// `SENSITIVE_CLEAR_PASSES` passes under load clear the mark.
pub fn update_sensitive(
    sensitive: bool,
    loaded_passes: u32,
    result: ResultKind,
    load: f64,
    passed_before: bool,
    threshold: f64,
) -> (bool, u32) {
    let loaded = load >= threshold;
    match result {
        ResultKind::Failed if loaded && passed_before => (true, 0),
        ResultKind::Passed if sensitive && loaded => {
            let passes = loaded_passes + 1;
            if passes >= SENSITIVE_CLEAR_PASSES {
                (false, 0)
            } else {
                (true, passes)
            }
        }
        _ => (sensitive, loaded_passes),
    }
}

/// Adds a run to a test's profile: the run (keeping the last
/// `KEPT_RUNS`), the score, the sensitivity, the memory kills and a UI
/// entry's room. A UI entry's failure beside others that its run passes
/// with fewer beside it (a contention failure) counts +2 as other
/// environment failures, not +10.
pub fn record(profile: &mut TestProfile, run: Run, sensitive_load: f64) {
    if run.result == ResultKind::NotRun {
        return;
    }
    let passed_before = profile.runs.iter().any(|r| r.result == ResultKind::Passed);
    let previous = profile.runs.last();
    let contended = previous.and_then(|p| concurrency::contention(p, &run));
    let (room, room_passes) =
        concurrency::update_room(profile.ui_room, profile.room_passes, previous, &run);
    profile.ui_room = room;
    profile.room_passes = room_passes;
    profile.failure_score = update_score(profile.failure_score, run.result);
    if contended.is_some() {
        let refunded = profile.failure_score - concurrency::CONTENTION_REFUND;
        profile.failure_score = refunded.max(0.0);
    }
    let (sensitive, passes) = update_sensitive(
        profile.sensitive,
        profile.loaded_passes,
        run.result,
        run.load,
        passed_before,
        sensitive_load,
    );
    profile.sensitive = sensitive;
    profile.loaded_passes = passes;
    if run.result == ResultKind::KilledForMemory {
        profile.memory_kills += 1;
    }
    profile.runs.push(run);
    if profile.runs.len() > KEPT_RUNS {
        let extra = profile.runs.len() - KEPT_RUNS;
        profile.runs.drain(..extra);
    }
}

/// A change of a test's profile worth reporting: the wall time or the
/// peak memory halved or doubled against the prediction, by at least 10 s
/// or 256 MiB.
pub fn big_change(before: &Prediction, wall: f64, peak: u64) -> Option<String> {
    if !before.known {
        return None;
    }
    let mut changes = Vec::new();
    let wall_ratio = wall / before.wall.max(0.01);
    if (wall_ratio >= 2.0 || wall_ratio <= 0.5) && (wall - before.wall).abs() >= 10.0 {
        changes.push(format!("wall {:.0} s -> {wall:.0} s", before.wall));
    }
    let old = before.memory_raw as f64;
    let new = peak as f64;
    let peak_ratio = new / old.max(1.0);
    if (peak_ratio >= 2.0 || peak_ratio <= 0.5) && (new - old).abs() >= 256.0 * MIB {
        changes.push(format!("peak {:.0} MiB -> {:.0} MiB", old / MIB, new / MIB));
    }
    if changes.is_empty() {
        None
    } else {
        Some(changes.join(", "))
    }
}

/// The default profile file: `MITCAD_TEST_PROFILES`, else
/// `~/.mitcad/test-profiles.json` (per machine, outside every build tree
/// and mirror, so that fresh build folders and removed mirrors keep it).
pub fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("MITCAD_TEST_PROFILES") {
        return Some(PathBuf::from(path));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(
        PathBuf::from(home)
            .join(".mitcad")
            .join("test-profiles.json"),
    )
}

/// Reads the file; a missing file is empty profiles.
pub fn load(path: &Path) -> Result<Profiles, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("cannot read the profiles {}: {e}", path.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Profiles::default()),
        Err(e) => Err(format!("cannot read the profiles {}: {e}", path.display())),
    }
}

/// A lock file beside the profiles, so that runs of several checkouts on
/// the machine add their runs one at a time. Removed on drop.
pub struct FileLock {
    path: PathBuf,
}

impl FileLock {
    /// Waits up to 30 s for the lock; a lock older than two minutes is
    /// left from a run that stopped and is taken over.
    pub fn acquire(target: &Path) -> Option<FileLock> {
        let mut name = target.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        for _ in 0..300 {
            let created = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path);
            match created {
                Ok(_) => return Some(FileLock { path }),
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    let stale = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| SystemTime::now().duration_since(t).ok())
                        .is_some_and(|age| age > Duration::from_secs(120));
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Adds the runs to the file under its lock (`update`), recording each
/// run.
pub fn save_runs(
    path: &Path,
    runs: &[(String, Run)],
    machine: &Machine,
    sensitive_load: f64,
) -> Result<(), String> {
    update(path, |profiles| {
        profiles.machine = machine.clone();
        for (name, run) in runs {
            let profile = profiles.tests.entry(name.clone()).or_default();
            record(profile, run.clone(), sensitive_load);
        }
    })
}

/// Adds the corpus tests' file runs to the file under its lock (`update`):
/// each test's rows into its files (`corpus::merge`).
pub fn save_files(
    path: &Path,
    rows: &[(String, Vec<(String, FileRun)>)],
    now: u64,
) -> Result<(), String> {
    update(path, |profiles| {
        for (name, files) in rows {
            let profile = profiles.tests.entry(name.clone()).or_default();
            corpus::merge(&mut profile.files, files, now);
        }
    })
}

/// Changes the file under its lock: reads the file again (another run may
/// have written it since), applies `change`, writes a temporary file and
/// renames it over the old one; what `change` returns.
pub fn update<T>(path: &Path, change: impl FnOnce(&mut Profiles) -> T) -> Result<T, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let lock = FileLock::acquire(path);
    if lock.is_none() {
        eprintln!("testrun: no lock on {}; writing anyway", path.display());
    }
    let mut profiles = load(path)?;
    let changed = change(&mut profiles);
    let text = serde_json::to_string_pretty(&profiles)
        .map_err(|e| format!("cannot write the profiles: {e}"))?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.tmp", std::process::id()));
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text)
        .map_err(|e| format!("cannot write {}: {e}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    drop(lock);
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(result: ResultKind, wall: f64, cpu: f64, peak_mib: f64) -> Run {
        Run {
            when: 0,
            result,
            wall,
            cpu,
            peak: (peak_mib * MIB) as u64,
            load: 0.0,
            limit: None,
            beside: None,
        }
    }

    const DEFAULTS: Defaults = Defaults {
        wall: 60.0,
        cores: 2.0,
        memory: 4 * 1024 * 1024 * 1024,
        margin: 1.25,
    };

    #[test]
    fn p90_interpolates_between_ranks() {
        assert_eq!(p90(&[]), None);
        assert_eq!(p90(&[3.0]), Some(3.0));
        // 0.9 * 4 = 3.6: between the 4th (40) and the 5th (50).
        let value = p90(&[50.0, 10.0, 30.0, 20.0, 40.0]).unwrap();
        assert!((value - 46.0).abs() < 1e-9);
    }

    #[test]
    fn unknown_tests_get_the_defaults() {
        let p = predict(&[], 0.0, &DEFAULTS);
        assert!(!p.known);
        assert_eq!(p.memory, DEFAULTS.memory);
        assert_eq!(p.cores, 2.0);
        // PROCESSORS raises the cores.
        assert_eq!(predict(&[], 4.0, &DEFAULTS).cores, 4.0);
    }

    #[test]
    fn unknown_tests_are_guessed_by_kind() {
        let labels = |l: &[&str]| -> Vec<String> { l.iter().map(|s| s.to_string()).collect() };
        assert_eq!(kind_of("app.ui-windows-sketch-cube", &[]), Kind::Ui);
        assert_eq!(kind_of("viewer", &labels(&["UI"])), Kind::Ui);
        assert_eq!(kind_of("f3d.corpus_occt", &[]), Kind::Corpus);
        assert_eq!(kind_of("rvt", &labels(&["corpus"])), Kind::Corpus);
        assert_eq!(kind_of("core.model", &labels(&["rust"])), Kind::Other);
        let base = Defaults {
            wall: 60.0,
            cores: 1.0,
            memory: 512 << 20,
            margin: 1.25,
        };
        let ui = predict(&[], 0.0, &kind_defaults(Kind::Ui, &base));
        assert_eq!((ui.memory, ui.cores), (1 << 30, 1.0));
        let corpus = predict(&[], 0.0, &kind_defaults(Kind::Corpus, &base));
        assert_eq!((corpus.memory, corpus.cores), (2 << 30, 2.0));
        let other = predict(&[], 0.0, &kind_defaults(Kind::Other, &base));
        assert_eq!(
            (other.memory, other.cores, other.wall),
            (512 << 20, 1.0, 60.0)
        );
        assert!(!ui.known && !corpus.known && !other.known);
    }

    #[test]
    fn spreads_compare_the_largest_and_smallest_peak() {
        assert_eq!(peak_spread(&[]), 1.0);
        let warm = run(ResultKind::Passed, 1.0, 1.0, 50.0);
        let cold = run(ResultKind::Passed, 1.0, 1.0, 5000.0);
        assert!((peak_spread(&[warm.clone(), cold]) - 100.0).abs() < 1e-9);
        assert_eq!(peak_spread(&[warm.clone(), warm]), 1.0);
    }

    #[test]
    fn unknown_tests_borrow_their_siblings_median() {
        let guess = predict(&[], 0.0, &DEFAULTS);
        let sibling = |wall: f64, memory_mib: f64| {
            let history = [run(ResultKind::Passed, wall, wall, memory_mib)];
            predict(&history, 0.0, &DEFAULTS)
        };
        let siblings = [
            sibling(10.0, 100.0),
            sibling(30.0, 300.0),
            sibling(20.0, 200.0),
        ];
        let borrowed = borrow(guess, &siblings);
        assert!(!borrowed.known);
        assert!((borrowed.wall - 20.0).abs() < 1e-9);
        assert!((borrowed.cores - 1.0).abs() < 1e-9);
        assert_eq!(borrowed.memory, siblings[2].memory);
        // An even count takes the mean of the middle two.
        assert_eq!(median(&[1.0, 4.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        // Unknown siblings lend nothing; a known test keeps its own.
        assert_eq!(borrow(guess, &[guess]), guess);
        assert_eq!(borrow(guess, &[]), guess);
        let own = sibling(99.0, 50.0);
        assert_eq!(borrow(own, &siblings), own);
    }

    #[test]
    fn prediction_is_the_p90_with_the_margin() {
        let runs: Vec<Run> = (1..=5)
            .map(|i| {
                let f = f64::from(i);
                run(ResultKind::Passed, 10.0 * f, 20.0 * f, 100.0 * f)
            })
            .collect();
        let p = predict(&runs, 0.0, &DEFAULTS);
        assert!(p.known);
        assert!((p.wall - 46.0).abs() < 1e-9);
        assert!((p.cores - 2.0).abs() < 1e-9);
        assert_eq!(p.memory_raw, (460.0 * MIB) as u64);
        assert_eq!(p.memory, (460.0 * MIB * 1.25) as u64);
    }

    #[test]
    fn small_tests_get_a_memory_floor() {
        let p = predict(&[run(ResultKind::Passed, 1.0, 0.1, 1.0)], 0.0, &DEFAULTS);
        assert_eq!(p.memory, (64.0 * MIB) as u64);
    }

    #[test]
    fn scores_rise_on_failures_and_fall_on_passes() {
        assert_eq!(update_score(0.0, ResultKind::Failed), 10.0);
        assert_eq!(update_score(10.0, ResultKind::Passed), 9.0);
        assert_eq!(update_score(0.0, ResultKind::Passed), 0.0);
        assert_eq!(update_score(0.0, ResultKind::KilledForMemory), 2.0);
        assert_eq!(update_score(0.0, ResultKind::Timeout), 2.0);
        assert_eq!(update_score(0.0, ResultKind::Environment), 2.0);
        assert_eq!(update_score(3.0, ResultKind::Skipped), 3.0);
        // About ten passes bring a failure back to normal.
        let mut score = update_score(0.0, ResultKind::Failed);
        for _ in 0..10 {
            score = update_score(score, ResultKind::Passed);
        }
        assert_eq!(score, 0.0);
    }

    #[test]
    fn failures_under_load_mark_a_test_sensitive() {
        // A failure under load of a test that passed before.
        let failed =
            |load, passed| update_sensitive(false, 0, ResultKind::Failed, load, passed, 0.7);
        assert_eq!(failed(0.9, true), (true, 0));
        // Not under load, or never passed: a plain failure.
        assert_eq!(failed(0.5, true), (false, 0));
        assert_eq!(failed(0.9, false), (false, 0));
        // Passes under load clear the mark.
        let passed = |state: (bool, u32), load| {
            update_sensitive(state.0, state.1, ResultKind::Passed, load, true, 0.7)
        };
        let mut state = (true, 0);
        for _ in 0..SENSITIVE_CLEAR_PASSES {
            state = passed(state, 0.8);
        }
        assert_eq!(state, (false, 0));
        // Calm passes do not.
        assert_eq!(passed((true, 2), 0.2), (true, 2));
    }

    #[test]
    fn records_keep_the_last_runs() {
        let mut profile = TestProfile::default();
        for i in 0..8 {
            let passed = run(ResultKind::Passed, f64::from(i), 0.0, 1.0);
            record(&mut profile, passed, 0.7);
        }
        assert_eq!(profile.runs.len(), KEPT_RUNS);
        assert_eq!(profile.runs[0].wall, 3.0);
        let killed = run(ResultKind::KilledForMemory, 1.0, 0.0, 1.0);
        record(&mut profile, killed, 0.7);
        assert_eq!(profile.memory_kills, 1);
        assert_eq!(profile.failure_score, 2.0);
        // A test that never started leaves no run.
        record(&mut profile, run(ResultKind::NotRun, 0.0, 0.0, 0.0), 0.7);
        assert_eq!(profile.runs.len(), KEPT_RUNS);
    }

    #[test]
    fn big_changes_are_named() {
        let history = [run(ResultKind::Passed, 100.0, 100.0, 1000.0)];
        let before = predict(&history, 0.0, &DEFAULTS);
        assert_eq!(big_change(&before, 120.0, (1100.0 * MIB) as u64), None);
        let change = big_change(&before, 250.0, (3000.0 * MIB) as u64).unwrap();
        assert!(change.contains("wall 100 s -> 250 s"), "{change}");
        assert!(change.contains("peak 1000 MiB -> 3000 MiB"), "{change}");
        // Small tests do not count.
        let small = predict(&[run(ResultKind::Passed, 2.0, 1.0, 10.0)], 0.0, &DEFAULTS);
        assert_eq!(big_change(&small, 6.0, (30.0 * MIB) as u64), None);
    }

    #[test]
    fn the_file_round_trips() {
        let mut profiles = Profiles::default();
        let mut profile = TestProfile::default();
        record(&mut profile, run(ResultKind::Failed, 1.0, 2.0, 3.0), 0.7);
        profiles.tests.insert("core.model".into(), profile);
        let text = serde_json::to_string(&profiles).unwrap();
        assert!(text.contains("\"failed\""));
        let back: Profiles = serde_json::from_str(&text).unwrap();
        assert_eq!(back, profiles);
        // Older or partial files read with defaults.
        let partial: Profiles = serde_json::from_str(r#"{"format":1}"#).unwrap();
        assert!(partial.tests.is_empty());
        assert!(partial.ui_caps.is_empty());
        // The UI fields are left out where unset, kept where set.
        assert!(
            !text.contains("beside") && !text.contains("ui_room"),
            "{text}"
        );
        let mut learnt = Profiles::default();
        let cap = UiCap {
            cap: 3,
            clean_runs: 1,
        };
        learnt.ui_caps.insert("linux".into(), cap);
        let mut ui = TestProfile::default();
        let mut failed = run(ResultKind::Failed, 1.0, 1.0, 1.0);
        failed.beside = Some(4);
        record(&mut ui, failed, 0.7);
        learnt.tests.insert("app.ui-x".into(), ui);
        let text = serde_json::to_string(&learnt).unwrap();
        assert!(text.contains("\"beside\":4"), "{text}");
        let back: Profiles = serde_json::from_str(&text).unwrap();
        assert_eq!(back, learnt);
    }

    #[test]
    fn a_failure_beside_other_ui_entries_counts_as_the_environment() {
        let mut profile = TestProfile::default();
        let mut failed = run(ResultKind::Failed, 30.0, 10.0, 300.0);
        failed.beside = Some(11);
        record(&mut profile, failed, 0.7);
        assert_eq!(profile.failure_score, 10.0);
        // Passed alone: +2 for the failure, -1 for the pass; room 5.
        let mut alone = run(ResultKind::Passed, 25.0, 10.0, 300.0);
        alone.beside = Some(0);
        record(&mut profile, alone, 0.7);
        assert_eq!(profile.failure_score, 1.0);
        assert_eq!((profile.ui_room, profile.room_passes), (Some(5), 0));
        // A failure that fails alone too stays a failure.
        let mut real = TestProfile::default();
        let mut first = run(ResultKind::Failed, 30.0, 10.0, 300.0);
        first.beside = Some(3);
        record(&mut real, first, 0.7);
        let mut again = run(ResultKind::Failed, 30.0, 10.0, 300.0);
        again.beside = Some(0);
        record(&mut real, again, 0.7);
        assert_eq!(real.failure_score, 20.0);
        assert_eq!(real.ui_room, None);
    }

    #[test]
    fn a_corpus_tests_files_round_trip_and_older_files_read() {
        // A file written before the files were kept reads with none.
        let old = r#"{"format":1,"tests":{"f3d.corpus_designs":{"runs":[]}}}"#;
        let mut profiles: Profiles = serde_json::from_str(old).unwrap();
        assert!(profiles.tests["f3d.corpus_designs"].files.is_empty());
        assert!(!serde_json::to_string(&profiles).unwrap().contains("files"));
        let rows = vec![(
            "db73863f".to_owned(),
            FileRun {
                wall: 120.5,
                peak: 2 << 30,
                when: 100,
                ..FileRun::default()
            },
        )];
        let test = profiles.tests.get_mut("f3d.corpus_designs").unwrap();
        corpus::merge(&mut test.files, &rows, 100);
        let text = serde_json::to_string(&profiles).unwrap();
        assert!(text.contains("\"files\":{\"db73863f\""), "{text}");
        let back: Profiles = serde_json::from_str(&text).unwrap();
        assert_eq!(back, profiles);
    }
}
