// SPDX-License-Identifier: MIT
//! The command line.

use std::path::PathBuf;
use std::str::FromStr;

pub const USAGE: &str = "\
Usage: mitcad-testrun --build DIR [--measure | --schedule] [options]
       mitcad-testrun --ui-cap [--ui-max N] [--profiles FILE]
       mitcad-testrun --ui-rooms [--profiles FILE]
       mitcad-testrun --record-ui FILE [--ui-max N] [--profiles FILE]

Runs the build tree's ctests, each as its own `ctest -R ^name$`, measures
each (wall and CPU time, peak memory, the kind of result) and keeps the
machine's profiles and failure scores between runs.

Modes:
  --measure             a fixed number of tests at once (-j), records only
                        (the default)
  --schedule            by a memory budget and a CPU gate: longest first
                        with complementary pairing, a priority lane for the
                        tests with the highest failure scores, hard memory
                        limits, a rerun alone after a memory limit
  --dry-run             lists the tests with their predictions; runs nothing
  --ui-cap              prints the UI entries this machine runs at once: the
                        bound (--ui-max, else MITCAD_UI_JOBS), else 'no cap
                        (memory budget)'; a test that fails
                        beside others and passes alone lowers its own room
                        (--ui-rooms), never this cap; runs nothing
  --ui-rooms            prints each UI entry's learnt room (name, the other
                        UI entries it may have beside it; tab-separated, a
                        line each); runs nothing
  --record-ui FILE      adds UI runs that tools/check-all.sh ran itself (a
                        line each: name, exit status, seconds, other UI
                        entries beside it, tab-separated) to the profiles
                        and learns their rooms from them

Selection (passed to ctest): -R REGEX, -E REGEX, -L REGEX, -LE REGEX
  --tests-from FILE     only the tests the file names, one exact name a line,
                        with the setup and cleanup tests of their fixtures
                        (within the selection above; no expression to grow
                        too big); a list that names none of the build
                        tree's tests is an error, never an empty run
  -C CONFIG             ctest's configuration (multi-configuration builds)
  --ctest PROGRAM       the ctest to run (default: ctest)

Options:
  -j, --jobs N          measure: tests at once (default: the cores / 2);
                        schedule: the most tests at once (default: the cores
                        once most tests have a history, else the cores / 2)
  --profiles FILE       the profile file (default: MITCAD_TEST_PROFILES, else
                        ~/.mitcad/test-profiles.json)
  --out DIR             logs, status file and report (default: BUILD/testrun)
  --no-record           leaves the profile file as it is
  --memory-budget X     a share of the memory (0.85, 85%) or a size (24G,
                        used as given); default 0.85 of MITCAD_TEST_MEMORY,
                        else of the larger of the physical memory and (on
                        Windows) the commit limit. A VM with dynamic memory
                        should give a size here or in MITCAD_TEST_MEMORY.
                        free (bare metal): what the machine has available
                        less --memory-reserve, read again before each test
                        starts, at most 0.85 of the memory as above
  --memory-reserve SIZE with --memory-budget free: the memory left to the
                        desktop and other jobs (default the larger of 4G
                        and a tenth of the machine's memory)
  --memory-floor SIZE   schedule: no test starts while the machine has less
                        memory available (default 1.5G)
  --cpu-gate F          no test starts above this load (default 0.9)
  --sensitive-load F    timing-sensitive tests start below it (default 0.7)
  --sensitive-max N     timing-sensitive tests at once (default 2)
  --sensitive PATTERN   tests timing-sensitive from the start (a * matches
                        anything; repeatable), e.g. 'app.ui-*'
  --ui-max N            the most UI entries at once (default:
                        MITCAD_UI_JOBS): opts in to that cap, to each UI
                        entry's learnt room and to the rerun alone of a UI
                        entry that failed beside others; without either no
                        UI cap or room applies, only the memory budget (the
                        rooms are still learnt)
  --lane-share F        the budget's share kept for the lane (default 0.2)
  --lane-score N        the failure score that puts a test in the lane
                        (default 1)
  --changed FILE        the changed files, a path a line (git diff
                        --name-only): their tests join the lane
  --corpus-children     each corpus test with a history of its files gets
                        its children at once (MITCAD_CORPUS_JOBS) and their
                        memory limit (MITCAD_CORPUS_MEMORY) from it, all
                        of them within the cores and the budget; the others
                        keep the caller's (the files' times are recorded
                        and handed back either way)
  --margin F            the predicted peak memory's margin (default 1.25)
  --limit-factor F      schedule: each test's hard memory limit as a multiple
                        of its history's p90 peak or its kind's guess, at
                        least 4G; none for RUN_SERIAL tests, a history
                        spread over 4x, or a rerun (default 2; 0: no limits)
  --default-memory SIZE a test with no history and no siblings with one,
                        of no known kind (default 512M; UI tests 1G, corpus
                        tests 2G)
  --default-cores N     (default 1; corpus tests 2)
  --default-wall SECS   (default 60)
  --disk-floor SIZE     no test starts below this free disk (default 10G)
  --no-cgroup           Linux: no systemd scopes (rusage only)

A release gate's full run (mitcad#1822):
  --release-passed DIR  the folder of the release gates' passes, outside
                        every build folder: a geometry* test whose
                        fingerprint (what it is built from and runs) passed
                        in an earlier release gate of this platform
                        (DIR/<platform>/<test>) is skipped; the report and
                        OUT/release-skipped.tsv name each with the release it
                        last passed in, and the passes of this run are kept
  --release-tag TAG     this gate's release and RC (0.63.0-rc1), kept with
                        its passes (needed with --release-passed)
  --all-geometry        runs every geometry* test (their passes are kept)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Measure,
    Schedule,
}

/// The memory budget: a share of the machine's memory, or bytes, or what
/// the machine has free (mitcad#872).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Budget {
    Share(f64),
    Bytes(u64),
    /// Bare metal: the memory available now less a reserve, read again
    /// before each test starts (`schedule::free_budget`); `bytes` is its
    /// cap, `FREE_CAP` of the memory.
    Free,
}

/// The share of the memory the free budget never goes above.
pub const FREE_CAP: f64 = 0.85;

impl Budget {
    pub fn bytes(self, memory: u64) -> u64 {
        match self {
            Budget::Share(share) => (memory as f64 * share) as u64,
            Budget::Bytes(bytes) => bytes,
            Budget::Free => (memory as f64 * FREE_CAP) as u64,
        }
    }

    /// The share of the memory it is, where it follows the memory (a
    /// share, or the free budget's cap).
    pub fn share(self) -> Option<f64> {
        match self {
            Budget::Share(share) => Some(share),
            Budget::Free => Some(FREE_CAP),
            Budget::Bytes(_) => None,
        }
    }
}

/// The reserve the free budget leaves when none is given: the larger of
/// 4 GiB and a tenth of the machine's memory.
pub fn default_reserve(machine: u64) -> u64 {
    (machine / 10).max(4 << 30)
}

/// The memory a share budget is taken of: `MITCAD_TEST_MEMORY` when set,
/// else the larger of the physical memory and the commit limit (a VM
/// with dynamic memory has only its current assignment as physical
/// memory), else 16 GiB.
pub fn budget_basis(variable: Option<u64>, physical: Option<u64>, commit: Option<u64>) -> u64 {
    if let Some(memory) = variable {
        return memory;
    }
    match (physical, commit) {
        (Some(p), Some(c)) => p.max(c),
        (Some(m), None) | (None, Some(m)) => m,
        (None, None) => 16 << 30,
    }
}

/// `MITCAD_TEST_MEMORY` as a size; a value that is not one is ignored
/// with a warning.
pub fn memory_variable() -> Option<u64> {
    let text = std::env::var("MITCAD_TEST_MEMORY").ok()?;
    match parse_size(&text) {
        Ok(memory) if memory > 0 => Some(memory),
        _ => {
            eprintln!("testrun: MITCAD_TEST_MEMORY={text} is not a size; ignored");
            None
        }
    }
}

/// The tests at once when -j is not given: measuring, half the cores
/// (never slower than ctest -j at that); scheduling, all the cores once
/// at least half the tests have a history, else half of them, so that a
/// first run with cautious predictions still runs as many as ctest -j.
pub fn default_jobs(cores: usize, scheduling: bool, known: usize, total: usize) -> usize {
    let half = (cores / 2).max(1);
    if scheduling && known * 2 >= total && total > 0 {
        cores.max(1)
    } else {
        half
    }
}

#[derive(Clone, Debug)]
pub struct Options {
    pub build: PathBuf,
    pub mode: Mode,
    pub dry_run: bool,
    pub jobs: Option<usize>,
    /// ctest's selection arguments, as given.
    pub select: Vec<String>,
    /// A file of test names, one a line (`ctest::select_named`).
    pub tests_from: Option<PathBuf>,
    pub config: Option<String>,
    pub ctest: String,
    pub profiles: Option<PathBuf>,
    pub out: Option<PathBuf>,
    pub record: bool,
    pub memory_budget: Budget,
    /// With `Budget::Free`: the memory left to the rest of the machine
    /// (None: `default_reserve`).
    pub memory_reserve: Option<u64>,
    pub memory_floor: u64,
    pub cpu_gate: f64,
    pub sensitive_load: f64,
    pub sensitive_max: usize,
    pub sensitive: Vec<String>,
    pub lane_share: f64,
    pub lane_score: f64,
    pub changed: Option<PathBuf>,
    /// Plans the corpus tests' children from their files' history
    /// (`corpus`).
    pub corpus_children: bool,
    /// The bound of the UI entries at once (`concurrency::bound`).
    pub ui_max: Option<usize>,
    /// Prints the machine's UI entries at once and runs nothing.
    pub ui_cap: bool,
    /// Prints the UI entries' learnt rooms and runs nothing.
    pub ui_rooms: bool,
    /// UI runs of check-all to record and learn from; runs nothing.
    pub record_ui: Option<PathBuf>,
    pub margin: f64,
    pub limit_factor: f64,
    pub default_memory: u64,
    pub default_cores: f64,
    pub default_wall: f64,
    pub disk_floor: u64,
    pub cgroups: bool,
    /// A release gate's passes (`fingerprint`): the folder, this gate's
    /// release and RC, and whether every geometry test runs anyway.
    pub release_passed: Option<PathBuf>,
    pub release_tag: Option<String>,
    pub all_geometry: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            build: PathBuf::new(),
            mode: Mode::Measure,
            dry_run: false,
            jobs: None,
            select: Vec::new(),
            tests_from: None,
            config: None,
            ctest: "ctest".to_owned(),
            profiles: None,
            out: None,
            record: true,
            memory_budget: Budget::Share(0.85),
            memory_reserve: None,
            memory_floor: 3 << 29,
            cpu_gate: 0.9,
            sensitive_load: 0.7,
            sensitive_max: 2,
            sensitive: Vec::new(),
            lane_share: 0.2,
            lane_score: 1.0,
            changed: None,
            corpus_children: false,
            ui_max: None,
            ui_cap: false,
            ui_rooms: false,
            record_ui: None,
            margin: 1.25,
            limit_factor: 2.0,
            default_memory: 512 << 20,
            default_cores: 1.0,
            default_wall: 60.0,
            disk_floor: 10 << 30,
            cgroups: true,
            release_passed: None,
            release_tag: None,
            all_geometry: false,
        }
    }
}

/// A size: bytes, or with K, M, G or T (powers of 1024; a trailing B or iB
/// allowed).
pub fn parse_size(text: &str) -> Result<u64, String> {
    let upper = text.trim().to_ascii_uppercase();
    let upper = upper.trim_end_matches("IB").trim_end_matches('B');
    let (digits, shift) = match upper.chars().next_back() {
        Some('K') => (&upper[..upper.len() - 1], 10),
        Some('M') => (&upper[..upper.len() - 1], 20),
        Some('G') => (&upper[..upper.len() - 1], 30),
        Some('T') => (&upper[..upper.len() - 1], 40),
        _ => (upper, 0),
    };
    let value: f64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("not a size: {text}"))?;
    if !value.is_finite() || value < 0.0 {
        return Err(format!("not a size: {text}"));
    }
    Ok((value * (1u64 << shift) as f64) as u64)
}

/// The budget: a share (0.85 or 85%), a size (24G) or "free".
pub fn parse_budget(text: &str) -> Result<Budget, String> {
    let text = text.trim();
    if text.eq_ignore_ascii_case("free") {
        return Ok(Budget::Free);
    }
    if let Some(percent) = text.strip_suffix('%') {
        let share: f64 = percent
            .parse()
            .map_err(|_| format!("not a share: {text}"))?;
        return Ok(Budget::Share(share / 100.0));
    }
    let share = text.parse::<f64>().ok().filter(|s| *s <= 1.0);
    if let Some(share) = share {
        return Ok(Budget::Share(share));
    }
    parse_size(text).map(Budget::Bytes)
}

fn number<T: FromStr>(text: &str, name: &str) -> Result<T, String> {
    text.parse()
        .map_err(|_| format!("{name}: not a number: {text}"))
}

/// The options of the command line (without the program's name).
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut build = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let name = arg.as_str();
        let mut value = || {
            it.next()
                .map(String::as_str)
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match name {
            "--build" => build = Some(PathBuf::from(value()?)),
            "--measure" => o.mode = Mode::Measure,
            "--schedule" => o.mode = Mode::Schedule,
            "--dry-run" => o.dry_run = true,
            "-j" | "--jobs" => o.jobs = Some(number(value()?, name)?),
            "-R" | "-E" | "-L" | "-LE" => {
                let pattern = value()?;
                o.select.push(name.to_owned());
                o.select.push(pattern.to_owned());
            }
            "--tests-from" => o.tests_from = Some(PathBuf::from(value()?)),
            "-C" => o.config = Some(value()?.to_owned()),
            "--ctest" => o.ctest = value()?.to_owned(),
            "--profiles" => o.profiles = Some(PathBuf::from(value()?)),
            "--out" => o.out = Some(PathBuf::from(value()?)),
            "--no-record" => o.record = false,
            "--memory-budget" => o.memory_budget = parse_budget(value()?)?,
            "--memory-reserve" => o.memory_reserve = Some(parse_size(value()?)?),
            "--memory-floor" => o.memory_floor = parse_size(value()?)?,
            "--cpu-gate" => o.cpu_gate = number(value()?, name)?,
            "--sensitive-load" => o.sensitive_load = number(value()?, name)?,
            "--sensitive-max" => o.sensitive_max = number(value()?, name)?,
            "--sensitive" => o.sensitive.push(value()?.to_owned()),
            "--lane-share" => o.lane_share = number(value()?, name)?,
            "--lane-score" => o.lane_score = number(value()?, name)?,
            "--changed" => o.changed = Some(PathBuf::from(value()?)),
            "--corpus-children" => o.corpus_children = true,
            "--ui-max" => o.ui_max = Some(number(value()?, name)?),
            "--ui-cap" => o.ui_cap = true,
            "--ui-rooms" => o.ui_rooms = true,
            "--record-ui" => o.record_ui = Some(PathBuf::from(value()?)),
            "--margin" => o.margin = number(value()?, name)?,
            "--limit-factor" => o.limit_factor = number(value()?, name)?,
            "--default-memory" => o.default_memory = parse_size(value()?)?,
            "--default-cores" => o.default_cores = number(value()?, name)?,
            "--default-wall" => o.default_wall = number(value()?, name)?,
            "--disk-floor" => o.disk_floor = parse_size(value()?)?,
            "--no-cgroup" => o.cgroups = false,
            "--release-passed" => o.release_passed = Some(PathBuf::from(value()?)),
            "--release-tag" => o.release_tag = Some(value()?.to_owned()),
            "--all-geometry" => o.all_geometry = true,
            _ => return Err(format!("unknown option {name}")),
        }
    }
    // --ui-cap and --record-ui read and write only the profiles.
    let profiles_only = o.ui_cap || o.ui_rooms || o.record_ui.is_some();
    o.build = match build {
        Some(build) => build,
        None if profiles_only => PathBuf::new(),
        None => return Err("--build DIR is needed".into()),
    };
    if o.jobs == Some(0) {
        return Err("-j must be at least 1".into());
    }
    if o.ui_max == Some(0) {
        return Err("--ui-max must be at least 1".into());
    }
    let tag_ok = |tag: &String| !tag.is_empty() && !tag.contains(['\t', '\n', '\r']);
    match (&o.release_passed, &o.release_tag) {
        (Some(_), Some(tag)) if !tag_ok(tag) => return Err(format!("not a release: {tag}")),
        (Some(_), None) => return Err("--release-passed needs --release-tag".into()),
        (None, Some(_)) => return Err("--release-tag needs --release-passed".into()),
        _ => {}
    }
    if o.all_geometry && o.release_passed.is_none() {
        return Err("--all-geometry needs --release-passed".into());
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn sizes_parse() {
        assert_eq!(parse_size("12G"), Ok(12 << 30));
        assert_eq!(parse_size("1536M"), Ok(1536 << 20));
        assert_eq!(parse_size("1.5GiB"), Ok(3 << 29));
        assert_eq!(parse_size("800k"), Ok(800 << 10));
        assert_eq!(parse_size("4096"), Ok(4096));
        assert!(parse_size("lots").is_err());
        assert!(parse_size("-1G").is_err());
    }

    #[test]
    fn budgets_parse() {
        assert_eq!(parse_budget("0.85"), Ok(Budget::Share(0.85)));
        assert_eq!(parse_budget("85%"), Ok(Budget::Share(0.85)));
        assert_eq!(parse_budget("24G"), Ok(Budget::Bytes(24 << 30)));
        assert_eq!(Budget::Share(0.5).bytes(1000), 500);
        assert_eq!(Budget::Bytes(7).bytes(1000), 7);
        assert_eq!(parse_budget("free"), Ok(Budget::Free));
        assert_eq!(parse_budget(" Free "), Ok(Budget::Free));
        // The free budget's cap is a share of the memory.
        assert_eq!(Budget::Free.bytes(1000), 850);
        assert_eq!(Budget::Free.share(), Some(FREE_CAP));
        assert_eq!(Budget::Bytes(7).share(), None);
    }

    #[test]
    fn the_free_budget_keeps_a_reserve() {
        let gib = |n: u64| n << 30;
        // At least 4 GiB, a tenth of a large machine.
        assert_eq!(default_reserve(gib(16)), gib(4));
        assert_eq!(default_reserve(gib(60)), gib(6));
        let o = parse(&args(
            "--build b --schedule --memory-budget free --memory-reserve 8G",
        ))
        .unwrap();
        assert_eq!(o.memory_budget, Budget::Free);
        assert_eq!(o.memory_reserve, Some(gib(8)));
        assert_eq!(parse(&args("--build b")).unwrap().memory_reserve, None);
    }

    #[test]
    fn the_budget_is_taken_of_the_largest_memory_known() {
        let gib = |n: u64| n << 30;
        // A VM with dynamic memory: 6.2 GiB assigned, a 24 GiB commit limit.
        let assigned = Some(gib(6));
        assert_eq!(budget_basis(None, assigned, Some(gib(24))), gib(24));
        assert_eq!(budget_basis(None, Some(gib(32)), Some(gib(20))), gib(32));
        assert_eq!(budget_basis(None, Some(gib(8)), None), gib(8));
        assert_eq!(budget_basis(None, None, None), gib(16));
        // MITCAD_TEST_MEMORY wins over what the system says.
        assert_eq!(
            budget_basis(Some(gib(28)), assigned, Some(gib(40))),
            gib(28)
        );
        // An absolute budget ignores the basis altogether.
        let basis = budget_basis(None, assigned, None);
        assert_eq!(Budget::Bytes(gib(24)).bytes(basis), gib(24));
        assert_eq!(Budget::Share(0.5).bytes(basis), gib(3));
    }

    #[test]
    fn a_first_run_runs_as_many_as_ctest_j() {
        // Measuring: half the cores.
        assert_eq!(default_jobs(16, false, 0, 500), 8);
        assert_eq!(default_jobs(16, false, 500, 500), 8);
        assert_eq!(default_jobs(1, false, 0, 10), 1);
        // Scheduling with little history: half the cores; with most: all.
        assert_eq!(default_jobs(16, true, 10, 500), 8);
        assert_eq!(default_jobs(16, true, 250, 500), 16);
        assert_eq!(default_jobs(16, true, 0, 0), 8);
    }

    #[test]
    fn command_lines_parse() {
        let o = parse(&args("--build b --schedule -j 6 -R ^core -E corpus -L ui")).unwrap();
        assert_eq!(o.build, PathBuf::from("b"));
        assert_eq!(o.mode, Mode::Schedule);
        assert_eq!(o.jobs, Some(6));
        assert_eq!(o.select, args("-R ^core -E corpus -L ui"));
        assert_eq!(o.tests_from, None);
        let o = parse(&args(
            "--build b --schedule --tests-from plan.txt -E corpus",
        ))
        .unwrap();
        assert_eq!(o.tests_from, Some(PathBuf::from("plan.txt")));
        assert_eq!(o.select, args("-E corpus"));
        assert!(parse(&args("--build b --tests-from")).is_err());
        let o = parse(&args("--build b --memory-budget 20G --sensitive app.ui-*")).unwrap();
        assert_eq!(o.memory_budget, Budget::Bytes(20 << 30));
        assert_eq!(o.memory_floor, 3 << 29);
        assert_eq!(o.sensitive, ["app.ui-*"]);
        let o = parse(&args("--build b --memory-floor 2G")).unwrap();
        assert_eq!(o.memory_floor, 2 << 30);
        assert!(!o.corpus_children);
        assert_eq!(o.mode, Mode::Measure);
        let o = parse(&args("--build b --schedule --corpus-children")).unwrap();
        assert!(o.corpus_children);
        assert!(parse(&args("--schedule")).is_err());
        assert!(parse(&args("--build b --frobnicate")).is_err());
        assert!(parse(&args("--build b -j")).is_err());
        assert!(parse(&args("--build b -j 0")).is_err());
    }

    #[test]
    fn the_release_options_parse() {
        let o = parse(&args(
            "--build b --schedule --release-passed /r --release-tag 0.63.0-rc1 --all-geometry",
        ))
        .unwrap();
        assert_eq!(o.release_passed, Some(PathBuf::from("/r")));
        assert_eq!(o.release_tag.as_deref(), Some("0.63.0-rc1"));
        assert!(o.all_geometry);
        let o = parse(&args("--build b")).unwrap();
        assert!(o.release_passed.is_none() && !o.all_geometry);
        assert!(parse(&args("--build b --release-passed /r")).is_err());
        assert!(parse(&args("--build b --release-tag 0.63.0-rc1")).is_err());
        assert!(parse(&args("--build b --all-geometry")).is_err());
    }

    #[test]
    fn the_ui_options_parse() {
        let o = parse(&args("--build b --schedule --ui-max 12")).unwrap();
        assert_eq!(o.ui_max, Some(12));
        assert!(!o.ui_cap && o.record_ui.is_none());
        // The profile commands need no build tree.
        let o = parse(&args("--ui-rooms")).unwrap();
        assert!(o.ui_rooms && !o.ui_cap);
        let o = parse(&args("--ui-cap --ui-max 6")).unwrap();
        assert!(o.ui_cap);
        assert_eq!(o.build, PathBuf::new());
        let o = parse(&args("--record-ui runs.tsv --profiles p.json")).unwrap();
        assert_eq!(o.record_ui, Some(PathBuf::from("runs.tsv")));
        assert_eq!(o.profiles, Some(PathBuf::from("p.json")));
        assert!(parse(&args("--build b --ui-max 0")).is_err());
        assert!(parse(&args("--ui-max 3")).is_err());
    }
}
