// SPDX-License-Identifier: MIT
//! The run: lists the tests through ctest, predicts each from the
//! profiles, starts them one ctest each in the order and at the times the
//! mode decides, measures and classifies each as it ends, reports failures
//! at once, adds the runs to the profiles and writes the end report.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::changed;
use crate::concurrency;
use crate::corpus::{self, Demand, FileRun, Room};
use crate::ctest::{self, TestDef};
use crate::options::{self, Budget, Mode, Options};
use crate::profile::{self, Defaults, Machine, Prediction, Profiles, ResultKind};
use crate::report::{self, Outcome, Summary};
use crate::schedule::{self, Candidate, LimitFacts, Limits, Now, Running, Seen};
use crate::sys;

/// How often running tests are polled.
const POLL: Duration = Duration::from_millis(100);
/// How often the scheduler looks again without a test ending.
const TICK: Duration = Duration::from_millis(500);
/// How often the profiles are written during the run.
const SAVE_EVERY: Duration = Duration::from_secs(60);
/// ctest's default timeout, for tests without TIMEOUT.
const DEFAULT_TIMEOUT: f64 = 1500.0;
/// How long after its timeout ctest gets before the runner stops it.
const GRACE: f64 = 120.0;

/// The tests each test waits for, and the fixture setups whose failure
/// keeps it from running: DEPENDS (those in the run), the setup tests of
/// the fixtures it requires, and for a cleanup test the setups and every
/// test requiring its fixtures.
pub fn link(defs: &[TestDef]) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let index: HashMap<&str, usize> = defs
        .iter()
        .enumerate()
        .map(|(i, d)| (d.name.as_str(), i))
        .collect();
    let mut setup: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut required: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, d) in defs.iter().enumerate() {
        for f in &d.fixtures_setup {
            setup.entry(f.as_str()).or_default().push(i);
        }
        for f in &d.fixtures_required {
            required.entry(f.as_str()).or_default().push(i);
        }
    }
    let mut before = vec![Vec::new(); defs.len()];
    let mut setups = vec![Vec::new(); defs.len()];
    for (i, d) in defs.iter().enumerate() {
        let mut waits: Vec<usize> = Vec::new();
        for name in &d.depends {
            waits.extend(index.get(name.as_str()));
        }
        for f in &d.fixtures_required {
            let of = setup.get(f.as_str()).map_or(&[][..], Vec::as_slice);
            waits.extend(of);
            setups[i].extend(of.iter().filter(|&&s| s != i));
        }
        for f in &d.fixtures_cleanup {
            for list in [setup.get(f.as_str()), required.get(f.as_str())] {
                waits.extend(list.map_or(&[][..], Vec::as_slice));
            }
        }
        waits.retain(|&w| w != i);
        waits.sort_unstable();
        waits.dedup();
        setups[i].sort_unstable();
        setups[i].dedup();
        before[i] = waits;
    }
    (before, setups)
}

/// Each test's siblings: the other tests that require one of its fixtures
/// or carry one of its labels (a test with no history borrows their
/// median prediction, `profile::borrow`).
pub fn siblings(defs: &[TestDef]) -> Vec<Vec<usize>> {
    let mut groups: HashMap<(bool, &str), Vec<usize>> = HashMap::new();
    for (i, d) in defs.iter().enumerate() {
        for f in &d.fixtures_required {
            groups.entry((true, f.as_str())).or_default().push(i);
        }
        for l in &d.labels {
            groups.entry((false, l.as_str())).or_default().push(i);
        }
    }
    let mut out = vec![Vec::new(); defs.len()];
    for members in groups.values() {
        for &i in members {
            out[i].extend(members.iter().filter(|&&j| j != i));
        }
    }
    for group in &mut out {
        group.sort_unstable();
        group.dedup();
    }
    out
}

/// A file name for a test's log.
fn file_name(name: &str) -> String {
    let keep = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    name.chars()
        .map(|c| if keep(c) { c } else { '_' })
        .collect()
}

fn unix_now() -> u64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH);
    since.map_or(0, |d| d.as_secs())
}

/// A running test.
struct Active {
    process: sys::Process,
    start: Instant,
    /// Seconds from the run's start.
    started_at: f64,
    limit: Option<u64>,
    log: PathBuf,
    load_sum: f64,
    samples: u32,
    deadline: Option<Instant>,
    stopped: bool,
    /// A UI entry: the most other UI entries that ran beside it.
    beside: u32,
}

enum State {
    Waiting,
    Running(Box<Active>),
    Done,
}

/// A test of the run.
struct Test {
    def: TestDef,
    /// From the profiles (raised after a memory limit).
    prediction: Prediction,
    /// The prediction the run started with, for the changes.
    predicted: Prediction,
    rank: f64,
    score: f64,
    lane: bool,
    sensitive: bool,
    /// A UI entry (`profile::Kind::Ui`), and its room (`concurrency`).
    ui: bool,
    ui_room: Option<usize>,
    /// Rerun alone after failing beside other UI entries (how many).
    again: Option<u32>,
    /// What its hard memory limit is decided from.
    limit_facts: LimitFacts,
    before: Vec<usize>,
    setups: Vec<usize>,
    state: State,
    /// Rerun alone after its memory limit (the limit it had).
    rerun: Option<u64>,
    first: Option<ResultKind>,
    outcome: Option<Outcome>,
    /// A corpus test (`profile::Kind::Corpus`): its files' times come from
    /// the profiles and go back to them (`corpus`).
    corpus: bool,
    /// Its files' last runs, from the profiles.
    files: BTreeMap<String, FileRun>,
    /// Variables for its ctest: a planned corpus test's children at once
    /// and their memory limit (`plan_corpus`).
    env: Vec<(String, String)>,
}

impl Test {
    fn waiting(&self) -> bool {
        matches!(self.state, State::Waiting)
    }

    fn running(&self) -> bool {
        matches!(self.state, State::Running(_))
    }

    fn done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    fn candidate(&self) -> Candidate<'_> {
        let mut prediction = self.prediction;
        if let State::Running(active) = &self.state {
            // A sampled live tree larger than its prediction already uses
            // that room. Do not admit another test on the old estimate.
            prediction.memory = prediction.memory.max(active.process.current().unwrap_or(0));
        }
        Candidate {
            prediction,
            rank: self.rank,
            score: self.score,
            lane: self.lane,
            sensitive: self.sensitive,
            ui: self.ui,
            ui_room: self.ui_room,
            alone: self.alone(),
            locks: &self.def.resource_locks,
        }
    }

    /// Runs alone: RUN_SERIAL as ctest reports it, or a rerun after its
    /// memory limit or after failing beside other UI entries.
    fn alone(&self) -> bool {
        self.def.run_serial || self.rerun.is_some() || self.again.is_some()
    }

    /// Its outcome with what every kind of end shares; `at`: the start
    /// and end, seconds from the run's start.
    fn base_outcome(&self, kind: ResultKind, at: f64) -> Outcome {
        Outcome {
            name: self.def.name.clone(),
            kind,
            first: self.first,
            start: at,
            end: at,
            predicted_wall: self.predicted.wall,
            predicted_memory: self.predicted.memory,
            lane: self.lane,
            before: self.before.clone(),
            alone: self.alone(),
            locks: self.def.resource_locks.clone(),
            ..Outcome::default()
        }
    }
}

/// Plans the corpus tests' children from their files' history
/// (`corpus::allocate`, mitcad#832): each corpus test with recorded files
/// gets its children at once and their memory limit as variables, and the
/// prediction and the memory limit of a test running that many. The corpus
/// tests without such a history keep their caller's settings and
/// predictions, which are kept out of the room the planned ones share.
/// `floor`: the caller's limit per child; `ceiling`: the most a limit is.
fn plan_corpus(tests: &mut [Test], room: Room, margin: f64, factor: f64, floor: u64, ceiling: u64) {
    let mut planned: Vec<(usize, Demand)> = Vec::new();
    let mut reserved_cores = 0.0_f64;
    let mut reserved_memory = 0_u64;
    for (i, t) in tests.iter().enumerate() {
        if !t.corpus {
            continue;
        }
        match Demand::of(&t.files) {
            Some(demand) => planned.push((i, demand)),
            None => {
                reserved_cores += t.prediction.cores;
                reserved_memory = reserved_memory.saturating_add(t.prediction.memory);
            }
        }
    }
    if planned.is_empty() {
        return;
    }
    let demands: Vec<Demand> = planned.iter().map(|(_, demand)| *demand).collect();
    let child_memory: Vec<u64> = demands.iter().map(|d| d.child_memory(margin)).collect();
    let shared = Room {
        cores: room.cores.saturating_sub(reserved_cores.ceil() as usize),
        memory: room.memory.saturating_sub(reserved_memory),
    };
    let children = corpus::allocate(&demands, &child_memory, shared);
    let rows = planned.iter().zip(&children).zip(&child_memory);
    for (((i, demand), count), memory) in rows {
        let count = *count;
        let limit = demand.child_limit(factor, floor, ceiling);
        let raw = corpus::PARENT_MEMORY + count as u64 * demand.peak;
        let prediction = Prediction {
            wall: demand.wall(count),
            cores: count as f64,
            memory: corpus::PARENT_MEMORY + count as u64 * *memory,
            memory_raw: raw,
            known: true,
        };
        let t = &mut tests[*i];
        t.prediction = prediction;
        t.predicted = prediction;
        // Its history ran other numbers of children: its limit follows the
        // children it has now.
        t.limit_facts.peak_p90 = raw;
        t.limit_facts.kind_memory = raw;
        t.limit_facts.spread = 1.0;
        t.env = vec![
            (corpus::JOBS.to_owned(), count.to_string()),
            (
                corpus::MEMORY.to_owned(),
                format!("{}M", limit.div_ceil(1 << 20)),
            ),
        ];
    }
}

/// The free budget (`--memory-budget free`, mitcad#872): its cap and
/// reserve, and what it was over the run, for the report.
#[derive(Clone, Copy, Debug)]
struct Free {
    /// The most it is: `options::FREE_CAP` of the memory (the lowest
    /// cgroup limit above the runner where lower).
    cap: u64,
    /// The memory left to the rest of the machine.
    reserve: u64,
    /// At the start, the lowest and the highest at a decision.
    first: u64,
    lowest: u64,
    highest: u64,
}

impl Free {
    fn note(&mut self, budget: u64) {
        self.lowest = self.lowest.min(budget);
        self.highest = self.highest.max(budget);
    }

    /// The line of the report and of the run's first lines.
    fn line(&self, mean: Option<u64>) -> String {
        let mut line = format!(
            "Memory budget from the free memory: the memory available less a reserve of {}, read again before each test starts, at most {}; {} at the start",
            report::size(self.reserve),
            report::size(self.cap),
            report::size(self.first)
        );
        if let Some(mean) = mean {
            line.push_str(&format!(
                ", {} - {} over the run (mean {})",
                report::size(self.lowest),
                report::size(self.highest),
                report::size(mean)
            ));
        }
        line
    }
}

/// The machine over the run, weighted by the time between samples.
#[derive(Default)]
struct Samples {
    last: Option<Instant>,
    time: f64,
    load: f64,
    memory_time: f64,
    memory_used: f64,
    /// The free budget times the time (`Free`).
    budget_time: f64,
    budget_sum: f64,
}

pub struct Runner<'o> {
    o: &'o Options,
    build: String,
    tests: Vec<Test>,
    limits: Limits,
    jobs: usize,
    /// The bound of the UI entries at once (`concurrency::bound`); None:
    /// the scheduler holds UI entries to no cap or room and reruns none.
    ui_bound: Option<usize>,
    /// The machine's learnt cap (within the learning bound), for the report.
    ui_learnt: usize,
    ceiling: u64,
    /// The budget's basis came from MITCAD_TEST_MEMORY (not re-read).
    fixed_basis: bool,
    /// The free budget, read again at each decision (`--memory-budget
    /// free`); None: `limits.budget` stays as given.
    free: Option<Free>,
    cgroup: bool,
    machine: Machine,
    out: PathBuf,
    logs: PathBuf,
    status: PathBuf,
    profiles: Option<PathBuf>,
    started: Instant,
    sampler: sys::CpuSampler,
    load: f64,
    seq: u64,
    finished: usize,
    records: Vec<(String, profile::Run)>,
    saved: usize,
    /// The corpus tests' file runs (`corpus`), and how many are saved.
    file_records: Vec<(String, Vec<(String, FileRun)>)>,
    saved_files: usize,
    changes: Vec<(String, String)>,
    samples: Samples,
    stop: Option<String>,
    skipped: Vec<String>,
}

/// The tests of the build tree as ctest selects them.
fn list_tests(o: &Options) -> Result<Vec<TestDef>, String> {
    let mut command = Command::new(&o.ctest);
    command
        .arg("--test-dir")
        .arg(&o.build)
        .arg("--show-only=json-v1")
        .args(&o.select);
    if let Some(config) = &o.config {
        command.arg("-C").arg(config);
    }
    let output = command
        .output()
        .map_err(|e| format!("cannot run {}: {e}", o.ctest))?;
    if !output.status.success() {
        let errors = String::from_utf8_lossy(&output.stderr);
        return Err(format!("{} --show-only failed: {}", o.ctest, errors.trim()));
    }
    ctest::parse_tests(&String::from_utf8_lossy(&output.stdout))
}

impl<'o> Runner<'o> {
    /// Lists and predicts the tests.
    pub fn prepare(o: &'o Options) -> Result<Runner<'o>, String> {
        let defs = list_tests(o)?;
        let mut skipped = Vec::new();
        let mut selected = Vec::new();
        for def in defs {
            if def.disabled || !def.available {
                skipped.push(def.name);
            } else {
                selected.push(def);
            }
        }
        let cores = thread::available_parallelism().map_or(1, |n| n.get());
        let variable = options::memory_variable();
        let fixed_basis = variable.is_some();
        let physical = sys::physical_memory();
        let memory = options::budget_basis(variable, physical, sys::commit_limit());
        let machine = Machine { cores, memory };
        let profiles_path = o.profiles.clone().or_else(profile::default_path);
        let profiles = match &profiles_path {
            Some(path) => profile::load(path).unwrap_or_else(|e| {
                eprintln!("testrun: {e}; starting without profiles");
                Profiles::default()
            }),
            None => Profiles::default(),
        };
        let defaults = Defaults {
            wall: o.default_wall,
            cores: o.default_cores,
            memory: o.default_memory,
            margin: o.margin,
        };
        let scheduling = o.mode == Mode::Schedule;
        // The bound of the UI entries at once; without one no cap or room
        // is applied (a machine opts in, `concurrency`).
        let ui_jobs = std::env::var("MITCAD_UI_JOBS").ok();
        let ui_bound = concurrency::bound(o.ui_max, ui_jobs.as_deref());
        let (before, setups) = link(&selected);
        // Each test's own prediction (a guess by its kind without history),
        // then the guesses replaced by the median of known siblings.
        let own: Vec<Prediction> = selected
            .iter()
            .map(|def| {
                let runs = profiles
                    .tests
                    .get(&def.name)
                    .map_or(&[][..], |p| &p.runs[..]);
                let kind = profile::kind_of(&def.name, &def.labels);
                let guess = profile::kind_defaults(kind, &defaults);
                profile::predict(runs, def.processors, &guess)
            })
            .collect();
        let predictions: Vec<Prediction> = siblings(&selected)
            .iter()
            .enumerate()
            .map(|(i, group)| {
                let theirs: Vec<Prediction> = group.iter().map(|&j| own[j]).collect();
                profile::borrow(own[i], &theirs)
            })
            .collect();
        let known = own.iter().filter(|p| p.known).count();
        let total = own.len();
        let mut tests = Vec::new();
        let rows = selected
            .into_iter()
            .zip(before)
            .zip(setups)
            .zip(predictions);
        for ((((def, before), setups), prediction), own) in rows.zip(own) {
            let prediction = crate::macos_policy::prediction(&def, prediction);
            let history = profiles.tests.get(&def.name).cloned().unwrap_or_default();
            let harness = crate::macos_policy::labelled(&def, crate::macos_policy::HARNESS_LABEL);
            let headless = crate::macos_policy::labelled(&def, crate::macos_policy::HEADLESS_LABEL);
            let seeded = !harness
                && !headless
                && o.sensitive.iter().any(|p| ctest::glob_match(p, &def.name));
            let score = history.failure_score;
            let kind = profile::kind_of(&def.name, &def.labels);
            let limit_facts = LimitFacts {
                peak_p90: if own.known { own.memory_raw } else { 0 },
                spread: profile::peak_spread(&history.runs),
                kind_memory: profile::kind_defaults(kind, &defaults).memory,
                serial: def.run_serial,
                rerun: false,
            };
            tests.push(Test {
                prediction,
                predicted: prediction,
                rank: prediction.wall,
                score,
                lane: schedule::in_lane(scheduling, score, o.lane_score),
                sensitive: (!harness && !headless && history.sensitive) || seeded,
                ui: kind == profile::Kind::Ui && !headless,
                ui_room: concurrency::scheduled_room(history.ui_room, ui_bound),
                again: None,
                limit_facts,
                before,
                setups,
                state: State::Waiting,
                rerun: None,
                first: None,
                outcome: None,
                corpus: kind == profile::Kind::Corpus,
                files: history.files.clone(),
                env: Vec::new(),
                def,
            });
        }
        // The tests related to the changed files join the lane (changed.rs).
        if let Some(list) = &o.changed {
            let defs: Vec<&TestDef> = tests.iter().map(|t| &t.def).collect();
            let related = changed::related_tests(list, &o.build, &defs)?;
            println!("{}", related.summary());
            for &i in &related.tests {
                tests[i].score += changed::CHANGE_SCORE;
                tests[i].lane = scheduling;
            }
        }
        let mut budget = o.memory_budget.bytes(memory);
        // The free budget starts at what the machine has free now; the
        // corpus tests' children are planned within it.
        let free = (o.memory_budget == Budget::Free).then(|| {
            let reserve = o
                .memory_reserve
                .unwrap_or_else(|| options::default_reserve(memory));
            let first = schedule::free_budget(budget, 0, 0, sys::available_memory(), reserve);
            Free {
                cap: budget,
                reserve,
                first,
                lowest: first,
                highest: first,
            }
        });
        if let Some(free) = &free {
            budget = free.first;
        }
        let ceiling = (memory as f64 * 0.95) as u64;
        // The corpus tests' children at once from their files' history.
        if o.corpus_children {
            let floor = std::env::var(corpus::MEMORY)
                .ok()
                .and_then(|text| options::parse_size(&text).ok())
                .filter(|&bytes| bytes > 0)
                .unwrap_or(corpus::DEFAULT_CHILD_LIMIT);
            let room = Room {
                cores,
                memory: budget,
            };
            plan_corpus(&mut tests, room, o.margin, o.limit_factor, floor, ceiling);
        }
        // Longest first counts the chains waiting behind a test.
        let walls: Vec<f64> = tests.iter().map(|t| t.prediction.wall).collect();
        let mut successors = vec![Vec::new(); tests.len()];
        for (i, t) in tests.iter().enumerate() {
            for &b in &t.before {
                successors[b].push(i);
            }
        }
        for (t, rank) in tests.iter_mut().zip(schedule::ranks(&walls, &successors)) {
            t.rank = rank;
        }
        let ui_cap = profiles.ui_caps.get(concurrency::PLATFORM);
        let ui_learnt = concurrency::effective_cap(ui_cap, concurrency::learning_bound(ui_bound));
        let limits = Limits {
            budget,
            cores: cores as f64,
            cpu_gate: o.cpu_gate,
            sensitive_load: o.sensitive_load,
            sensitive_max: o.sensitive_max,
            ui_max: concurrency::scheduled_cap(ui_cap, ui_bound),
            lane_share: o.lane_share,
            max_parallel: o
                .jobs
                .unwrap_or(options::default_jobs(cores, true, known, total)),
            pair_window: 0.6,
            pair_weight: 0.5,
            memory_floor: o.memory_floor,
        };
        let out = o.out.clone().unwrap_or_else(|| o.build.join("testrun"));
        let logs = out.join("logs");
        Ok(Runner {
            o,
            build: o.build.to_string_lossy().into_owned(),
            tests,
            limits,
            jobs: o
                .jobs
                .unwrap_or(options::default_jobs(cores, false, known, total)),
            ui_bound,
            ui_learnt,
            ceiling,
            fixed_basis,
            free,
            cgroup: o.cgroups && sys::cgroups_available(),
            machine,
            status: out.join("status.txt"),
            out,
            logs,
            profiles: profiles_path,
            started: Instant::now(),
            sampler: sys::CpuSampler::start(),
            load: 0.0,
            seq: 0,
            finished: 0,
            records: Vec::new(),
            saved: 0,
            file_records: Vec::new(),
            saved_files: 0,
            changes: Vec::new(),
            samples: Samples::default(),
            stop: None,
            skipped,
        })
    }

    fn mode_name(&self) -> &'static str {
        match self.o.mode {
            Mode::Measure => "measure",
            Mode::Schedule => "schedule",
        }
    }

    /// The tests in the order they would start on an empty machine, with
    /// their predictions.
    pub fn dry_run(&self) -> String {
        let mut order: Vec<&Test> = self.tests.iter().collect();
        if self.o.mode == Mode::Schedule {
            order.sort_by(|a, b| {
                let by_lane = b.lane.cmp(&a.lane);
                let by_score = b.score.total_cmp(&a.score);
                by_lane.then(by_score).then(b.rank.total_cmp(&a.rank))
            });
        }
        let mut out = format!(
            "{} tests ({} mode, {} cores, budget {}, {} at once in measure mode, {})\n",
            self.tests.len(),
            self.mode_name(),
            self.machine.cores,
            report::size(self.limits.budget),
            self.jobs,
            self.ui_line()
        );
        if let Some(free) = &self.free {
            out.push_str(&free.line(None));
            out.push('\n');
        }
        out.push_str("  rank_s  wall_s  cores    memory  score  name  [flags]\n");
        for t in order {
            let p = &t.prediction;
            let mut flags: Vec<String> = Vec::new();
            let marks = [
                (t.lane, "lane"),
                (t.sensitive, "timing-sensitive"),
                (!p.known, "no history"),
                (t.def.run_serial, "RUN_SERIAL"),
            ];
            for (on, flag) in marks {
                if on {
                    flags.push(flag.to_owned());
                }
            }
            if let Some(room) = t.ui_room {
                flags.push(format!("UI room {room}"));
            }
            for (name, value) in &t.env {
                flags.push(format!("{name}={value}"));
            }
            for lock in &t.def.resource_locks {
                flags.push(format!("RESOURCE_LOCK {lock}"));
            }
            let flags = if flags.is_empty() {
                String::new()
            } else {
                format!("  [{}]", flags.join(", "))
            };
            out.push_str(&format!(
                "  {:>6.0}  {:>6.0}  {:>5.1}  {:>8}  {:>5.0}  {}{flags}\n",
                t.rank,
                p.wall,
                p.cores,
                report::size(p.memory),
                t.score,
                t.def.name
            ));
        }
        if !self.skipped.is_empty() {
            let names = self.skipped.join(", ");
            out.push_str(&format!("Disabled or not available: {names}\n"));
        }
        out
    }

    fn running_now(&self) -> Running {
        let mut running = Running::default();
        for t in self.tests.iter().filter(|t| t.running()) {
            running.add(&t.candidate());
        }
        running
    }

    fn disk_free(&self) -> Option<u64> {
        sys::disk_free(&self.o.build)
    }

    fn disk_low(&self) -> bool {
        self.disk_free()
            .is_some_and(|free| free < self.o.disk_floor)
    }

    fn append_status(&self, line: &str) {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.status);
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{line}");
        }
    }

    /// Runs the tests; the process's exit status.
    pub fn run(&mut self) -> i32 {
        if let Err(e) = fs::create_dir_all(&self.logs) {
            eprintln!("testrun: cannot create {}: {e}", self.logs.display());
            return 2;
        }
        let _ = fs::write(&self.status, "");
        let total = self.tests.len();
        println!(
            "testrun: {total} tests, {} mode, {} cores, budget {}; measured with {}",
            self.mode_name(),
            self.machine.cores,
            report::size(self.limits.budget),
            sys::method(self.cgroup)
        );
        if let Some(free) = &self.free {
            println!("{}", free.line(None));
        }
        let lane: Vec<String> = self
            .tests
            .iter()
            .filter(|t| t.lane)
            .map(|t| format!("{} ({:.0})", t.def.name, t.score))
            .collect();
        if !lane.is_empty() {
            println!("Priority lane: {}", lane.join(", "));
        }
        let planned: Vec<String> = self
            .tests
            .iter()
            .filter(|t| !t.env.is_empty())
            .map(|t| {
                let value = |name: &str| {
                    t.env
                        .iter()
                        .find(|(k, _)| k == name)
                        .map(|(_, v)| v.as_str())
                };
                let jobs = value(corpus::JOBS).unwrap_or("?");
                let limit = value(corpus::MEMORY).unwrap_or("?");
                format!("{} {jobs} ({limit} each)", t.def.name)
            })
            .collect();
        if !planned.is_empty() {
            println!(
                "Corpus children from the files' history: {}",
                planned.join(", ")
            );
        }
        if self.o.mode == Mode::Schedule && self.tests.iter().any(|t| t.ui) {
            println!("{}", self.ui_line());
        }
        self.append_status(&format!("started\t{total} tests\t{}", self.mode_name()));
        self.started = Instant::now();
        let mut last_tick: Option<Instant> = None;
        let mut last_save = Instant::now();
        loop {
            let ended = self.poll();
            let due = last_tick.is_none_or(|t| t.elapsed() >= TICK);
            if due {
                self.tick();
                last_tick = Some(Instant::now());
            }
            if ended || due {
                self.start_ready();
            }
            if self.tests.iter().all(Test::done) {
                break;
            }
            if last_save.elapsed() >= SAVE_EVERY {
                self.save();
                last_save = Instant::now();
            }
            thread::sleep(POLL);
        }
        self.save();
        let ui = self.learn_ui_cap();
        self.finish_report(ui.as_deref())
    }

    /// Polls the running tests; whether any ended.
    fn poll(&mut self) -> bool {
        let mut ended = false;
        for i in 0..self.tests.len() {
            let State::Running(active) = &mut self.tests[i].state else {
                continue;
            };
            let Some(exit) = active.process.poll() else {
                let late = active.deadline.is_some_and(|d| Instant::now() >= d);
                if late && !active.stopped {
                    active.process.kill();
                    active.stopped = true;
                }
                continue;
            };
            let state = std::mem::replace(&mut self.tests[i].state, State::Done);
            if let State::Running(active) = state {
                self.finish(i, *active, exit);
                ended = true;
            }
        }
        ended
    }

    /// Samples the machine: the load since the last sample, weighted by
    /// that time over the whole run (the tests' own use is integrated
    /// from their outcomes, `report::use_of`).
    fn tick(&mut self) {
        let now = Instant::now();
        let s = &mut self.samples;
        let dt = s.last.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        s.last = Some(now);
        if let Some(load) = self.sampler.sample() {
            self.load = load;
            s.time += dt;
            s.load += load * dt;
        }
        let physical = sys::physical_memory();
        let available = sys::available_memory();
        // macOS headroom depends on memory pressure; its complement is
        // not a measurement of system memory in use. Keep that unknown.
        #[cfg(not(target_os = "macos"))]
        if let Some(available) = available {
            // Of the physical memory assigned now (the budget's basis may
            // be larger).
            let total = physical.unwrap_or(self.machine.memory).max(1) as f64;
            let used = (1.0 - available as f64 / total).clamp(0.0, 1.0);
            s.memory_time += dt;
            s.memory_used += used * dt;
        }
        if self.free.is_some() {
            let budget = self.free_budget_now(available);
            let s = &mut self.samples;
            s.budget_time += dt;
            s.budget_sum += budget as f64 * dt;
        }
        let load = self.load;
        for t in &mut self.tests {
            if let State::Running(active) = &mut t.state {
                active.load_sum += load;
                active.samples += 1;
            }
        }
        self.refresh_budget(physical);
    }

    /// A VM with dynamic memory gets more as it needs it: the basis of a
    /// share budget is read again and only ever grows. An absolute
    /// `--memory-budget` or MITCAD_TEST_MEMORY stays as given.
    fn refresh_budget(&mut self, physical: Option<u64>) {
        let Some(share) = self.o.memory_budget.share() else {
            return;
        };
        if self.fixed_basis {
            return;
        }
        let basis = options::budget_basis(None, physical, sys::commit_limit());
        if basis > self.machine.memory {
            self.machine.memory = basis;
            let budget = Budget::Share(share).bytes(basis);
            match &mut self.free {
                Some(free) => free.cap = budget,
                None => self.limits.budget = budget,
            }
            self.ceiling = (basis as f64 * 0.95) as u64;
        }
    }

    /// The free budget now (`schedule::free_budget`): the running tests'
    /// predictions, their growth still to come (their predictions less
    /// their working sets now) and the available memory read now.
    fn free_budget_now(&self, available: Option<u64>) -> u64 {
        let Some(free) = &self.free else {
            return self.limits.budget;
        };
        let mut memory = 0_u64;
        let mut pending = 0_u64;
        for t in &self.tests {
            if let State::Running(active) = &t.state {
                memory = memory.saturating_add(t.candidate().prediction.memory);
                let grows = schedule::pending(t.prediction.memory, active.process.current());
                pending = pending.saturating_add(grows);
            }
        }
        schedule::free_budget(free.cap, memory, pending, available, free.reserve)
    }

    /// Whether a waiting test's dependencies are done.
    fn ready(&self, i: usize) -> bool {
        let t = &self.tests[i];
        t.waiting() && t.before.iter().all(|&b| self.tests[b].done())
    }

    /// Marks the waiting tests whose fixture setup failed as not run.
    fn skip_blocked(&mut self) {
        loop {
            let mut changed = false;
            for i in 0..self.tests.len() {
                if !self.ready(i) {
                    continue;
                }
                let failed = self.tests[i].setups.iter().find(|&&s| {
                    let outcome = self.tests[s].outcome.as_ref();
                    outcome.is_some_and(|o| !o.kind.ok())
                });
                let Some(&setup) = failed else {
                    continue;
                };
                let note = format!("its fixture's setup {} failed", self.tests[setup].def.name);
                self.not_run(i, note);
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    /// Records a test that never starts.
    fn not_run(&mut self, i: usize, note: String) {
        let at = self.started.elapsed().as_secs_f64();
        let t = &mut self.tests[i];
        t.state = State::Done;
        t.outcome = Some(Outcome {
            note,
            ..t.base_outcome(ResultKind::NotRun, at)
        });
        self.finished += 1;
        self.report_finished(i);
    }

    fn lane_waiting(&self) -> bool {
        self.tests.iter().any(|t| t.lane && t.waiting())
    }

    /// Starts what the mode allows now.
    fn start_ready(&mut self) {
        self.skip_blocked();
        if self.disk_low() {
            let running = self.tests.iter().any(Test::running);
            if !running && self.stop.is_none() {
                let free = self.disk_free().map(report::size).unwrap_or_default();
                let floor = report::size(self.o.disk_floor);
                let reason = format!("free disk {free} below the floor {floor} (environment)");
                eprintln!("testrun: {reason}; no more tests start");
                self.append_status(&format!("stopped\t{reason}"));
                self.stop = Some(reason);
                for i in 0..self.tests.len() {
                    if self.tests[i].waiting() {
                        self.not_run(i, "not started: free disk below the floor".into());
                    }
                }
            }
            return;
        }
        loop {
            let harnesses = self
                .tests
                .iter()
                .filter(|t| {
                    t.running()
                        && crate::macos_policy::labelled(&t.def, crate::macos_policy::HARNESS_LABEL)
                })
                .count();
            let ready: Vec<usize> = (0..self.tests.len())
                .filter(|&i| {
                    self.ready(i)
                        && (harnesses < crate::macos_policy::HARNESS_MAX
                            || !crate::macos_policy::labelled(
                                &self.tests[i].def,
                                crate::macos_policy::HARNESS_LABEL,
                            ))
                })
                .collect();
            if ready.is_empty() {
                break;
            }
            let running = self.running_now();
            let chosen = match self.o.mode {
                Mode::Measure => ready.into_iter().find(|&i| {
                    let c = self.tests[i].candidate();
                    schedule::admit_fixed(&c, &running, self.jobs).is_ok()
                }),
                Mode::Schedule => {
                    let available = sys::available_memory();
                    if self.free.is_some() {
                        let budget = self.free_budget_now(available);
                        self.limits.budget = budget;
                        if let Some(free) = &mut self.free {
                            free.note(budget);
                        }
                    }
                    let now = Now {
                        load: self.load,
                        available,
                        lane_waiting: self.lane_waiting(),
                    };
                    let admissible: Vec<usize> = ready
                        .into_iter()
                        .filter(|&i| {
                            let c = self.tests[i].candidate();
                            schedule::admit(&c, &running, &now, &self.limits).is_ok()
                        })
                        .collect();
                    let candidates: Vec<Candidate> = admissible
                        .iter()
                        .map(|&i| self.tests[i].candidate())
                        .collect();
                    let picked = schedule::pick(&candidates, &running, &now, &self.limits);
                    picked.map(|k| admissible[k])
                }
            };
            let Some(i) = chosen else {
                break;
            };
            self.start(i);
        }
        // Nothing runs and nothing can start: the rest wait in a cycle.
        let idle = !self.tests.iter().any(Test::running);
        if idle && !(0..self.tests.len()).any(|i| self.ready(i)) {
            for i in 0..self.tests.len() {
                if self.tests[i].waiting() {
                    self.not_run(i, "waits in a cycle of dependencies".into());
                }
            }
        }
    }

    fn start(&mut self, i: usize) {
        self.seq += 1;
        let t = &self.tests[i];
        let limit = match self.o.mode {
            Mode::Schedule => {
                let facts = LimitFacts {
                    rerun: t.rerun.is_some(),
                    ..t.limit_facts
                };
                schedule::memory_limit(&facts, self.o.limit_factor, self.ceiling)
            }
            Mode::Measure => None,
        };
        let suffix = if t.rerun.is_some() { ".rerun" } else { "" };
        let log = self
            .logs
            .join(format!("{}{suffix}.log", file_name(&t.def.name)));
        let config = self.o.config.as_deref();
        let spec = sys::Spec {
            program: self.o.ctest.clone(),
            args: ctest::run_args(&self.build, config, &t.def.name),
            log: log.clone(),
            limit,
            seq: self.seq,
            cgroup: self.cgroup,
            env: self.test_env(i),
        };
        let timeout = t.def.timeout.unwrap_or(DEFAULT_TIMEOUT);
        match sys::spawn(&spec) {
            Ok(process) => {
                let start = Instant::now();
                let deadline = if timeout > 0.0 {
                    Some(start + Duration::from_secs_f64(timeout + GRACE))
                } else {
                    None
                };
                let active = Active {
                    process,
                    start,
                    started_at: self.started.elapsed().as_secs_f64(),
                    limit,
                    log,
                    load_sum: 0.0,
                    samples: 0,
                    deadline,
                    stopped: false,
                    beside: 0,
                };
                self.tests[i].state = State::Running(Box::new(active));
                if self.tests[i].ui {
                    self.count_ui_beside();
                }
            }
            Err(e) => {
                // ctest did not start: the environment's fault.
                let at = self.started.elapsed().as_secs_f64();
                let note = format!("cannot start {}: {e}", self.o.ctest);
                let t = &mut self.tests[i];
                t.state = State::Done;
                t.outcome = Some(Outcome {
                    note,
                    ..t.base_outcome(ResultKind::Environment, at)
                });
                self.finished += 1;
                self.report_finished(i);
            }
        }
    }

    /// The tables of a corpus test's files (`corpus`): the one it reads,
    /// its files' last times from the profiles, and the one it appends its
    /// children's runs to (only when the run records).
    fn corpus_tables(&self, i: usize) -> (PathBuf, PathBuf) {
        let dir = self.out.join("corpus");
        let name = file_name(&self.tests[i].def.name);
        (
            dir.join(format!("{name}.files.tsv")),
            dir.join(format!("{name}.runs.tsv")),
        )
    }

    fn records_files(&self) -> bool {
        self.o.record && self.profiles.is_some()
    }

    /// The variables of a test's ctest: a planned corpus test's children
    /// and their limit, and for every corpus test the tables of its files
    /// (written here, the record emptied).
    fn test_env(&self, i: usize) -> Vec<(String, String)> {
        let t = &self.tests[i];
        let mut env = t.env.clone();
        if crate::macos_policy::labelled(&t.def, crate::macos_policy::CARGO_LABEL) {
            let caller = std::env::var("CARGO_BUILD_JOBS").ok();
            let reserve = self.free.as_ref().map_or_else(
                || options::default_reserve(self.machine.memory),
                |free| free.reserve,
            );
            let room = sys::available_memory().unwrap_or(0).saturating_sub(reserve);
            env.push((
                "CARGO_BUILD_JOBS".into(),
                crate::macos_policy::cargo_jobs(caller.as_deref(), room).to_string(),
            ));
        }
        if !t.corpus {
            return env;
        }
        // The free budget's reserve holds the children too, each checked
        // against the available memory before it starts (unless the caller
        // set one).
        let caller_reserve = std::env::var_os(corpus::RESERVE).is_some();
        if let Some(free) = self.free.as_ref().filter(|_| !caller_reserve) {
            env.push((corpus::RESERVE.to_owned(), free.reserve.to_string()));
        }
        let (files, runs) = self.corpus_tables(i);
        if let Some(dir) = files.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let path = |p: &PathBuf| p.to_string_lossy().into_owned();
        if !t.files.is_empty() && fs::write(&files, corpus::table(&t.files)).is_ok() {
            env.push((corpus::FILES.to_owned(), path(&files)));
        }
        if self.records_files() && fs::write(&runs, "").is_ok() {
            env.push((corpus::RECORD.to_owned(), path(&runs)));
        }
        env
    }

    /// The runs a corpus test's children recorded, kept for the profiles;
    /// when it passed without any although it has files, it imports in one
    /// process now and its files go (`corpus::one_process`).
    fn take_file_runs(&mut self, i: usize, passed: bool) {
        if !self.tests[i].corpus || !self.records_files() {
            return;
        }
        let (_, runs) = self.corpus_tables(i);
        let text = fs::read_to_string(&runs).unwrap_or_default();
        let rows = corpus::parse_record(&text, unix_now());
        let known = self.tests[i].files.len();
        if !rows.is_empty() || corpus::one_process(passed, rows.len(), known) {
            let name = self.tests[i].def.name.clone();
            self.file_records.push((name, rows));
        }
    }

    /// Measures, classifies and records a test that ended.
    fn finish(&mut self, i: usize, mut active: Active, exit: sys::Exit) {
        let usage = active.process.usage();
        let wall = active.start.elapsed().as_secs_f64();
        let end = self.started.elapsed().as_secs_f64();
        let output = fs::read_to_string(&active.log).unwrap_or_default();
        let name = self.tests[i].def.name.clone();
        let seen = Seen {
            status: ctest::parse_status(&output, &name),
            exit_ok: exit.success,
            over_memory: usage.over_memory,
            stopped: active.stopped,
            disk_full: self.disk_low() || ctest::says_disk_full(&output),
        };
        let kind = schedule::classify(&seen);
        self.take_file_runs(i, kind == ResultKind::Passed);
        let load = if active.samples > 0 {
            active.load_sum / f64::from(active.samples)
        } else {
            self.load
        };
        let beside = active.beside;
        let ui = self.tests[i].ui;
        let run = profile::Run {
            when: unix_now().saturating_sub(wall as u64),
            result: kind,
            wall,
            cpu: usage.cpu,
            peak: schedule::recorded_peak(kind, usage.peak, active.limit),
            load,
            limit: active.limit,
            beside: ui.then_some(beside),
        };
        self.records.push((name.clone(), run));
        let scheduling = self.o.mode == Mode::Schedule;
        let t = &mut self.tests[i];
        let change = profile::big_change(&t.predicted, wall, usage.peak);
        if let Some(change) = change.filter(|_| kind.ok()) {
            self.changes.push((name.clone(), change));
        }
        if kind == ResultKind::KilledForMemory && scheduling && t.rerun.is_none() {
            let limit = active.limit.unwrap_or(t.prediction.memory);
            t.rerun = Some(limit);
            t.first = Some(kind);
            let peak = schedule::recorded_peak(kind, usage.peak, active.limit);
            t.prediction.memory = t.prediction.memory.max(peak);
            t.state = State::Waiting;
            let line = format!("{name}: over its memory limit {}", report::size(limit));
            println!("  {line}; rerunning alone without a limit");
            self.append_status(&format!("memory\t{line}"));
            return;
        }
        // A UI entry that failed beside others runs again alone, after the
        // others: a pass then is a contention failure (`concurrency`), which
        // the profiles learn from.
        let failed = matches!(kind, ResultKind::Failed | ResultKind::Timeout);
        let first_try = t.again.is_none() && t.rerun.is_none();
        let opted_in = self.ui_bound.is_some();
        if scheduling && opted_in && ui && failed && beside > 0 && first_try {
            t.again = Some(beside);
            t.first = Some(kind);
            t.state = State::Waiting;
            let line = format!("{name}: {} beside {beside} other UI entries", kind.label());
            println!("  {line}; rerunning alone");
            self.append_status(&format!("ui\t{line}"));
            return;
        }
        let mut note = String::new();
        if seen.status.is_none() && !kind.ok() {
            note = match exit.code {
                Some(code) => format!("no result line from ctest (exit {code})"),
                None => "no result line from ctest (killed)".to_owned(),
            };
        }
        if seen.disk_full && kind == ResultKind::Environment {
            note = "the disk was full".to_owned();
        }
        if seen.stopped {
            note = "stopped by the runner after its timeout".to_owned();
        }
        if let Some(others) = t.again {
            note = format!("rerun alone after failing beside {others} other UI entries");
        }
        t.outcome = Some(Outcome {
            start: active.started_at,
            end,
            wall,
            cpu: usage.cpu,
            peak: usage.peak,
            limit: active.limit,
            log: Some(active.log.clone()),
            note,
            ..t.base_outcome(kind, end)
        });
        self.finished += 1;
        self.report_finished(i);
    }

    /// The UI entries at once, for the dry run and the run's start.
    fn ui_line(&self) -> String {
        match self.ui_bound {
            Some(bound) => format!(
                "UI entries at most {} at once (learnt; bound {bound})",
                self.limits.ui_max
            ),
            None => format!(
                "UI entries not capped (no --ui-max or MITCAD_UI_JOBS; learnt cap {})",
                self.ui_learnt
            ),
        }
    }

    /// A UI entry started: each running UI entry has the others beside it.
    fn count_ui_beside(&mut self) {
        let running = self.tests.iter().filter(|t| t.ui && t.running()).count();
        let others = u32::try_from(running.saturating_sub(1)).unwrap_or(u32::MAX);
        for t in self.tests.iter_mut().filter(|t| t.ui) {
            if let State::Running(active) = &mut t.state {
                active.beside = active.beside.max(others);
            }
        }
    }

    /// Learns the machine's UI entries at once from this run's records
    /// (`concurrency::learn`) into the profile file; the report's line.
    fn learn_ui_cap(&self) -> Option<String> {
        let facts = concurrency::run_facts(&self.records);
        if facts.at_once == 0 {
            return None;
        }
        let cap = self.ui_learnt;
        let unchanged = concurrency::summary(&facts, cap, cap);
        let Some(path) = self.profiles.as_ref().filter(|_| self.o.record) else {
            return Some(unchanged);
        };
        let platform = concurrency::PLATFORM;
        let bound = concurrency::learning_bound(self.ui_bound);
        let learnt = profile::update(path, |profiles| {
            concurrency::learn_into(&mut profiles.ui_caps, platform, &facts, bound)
        });
        match learnt {
            Ok((before, after)) => Some(concurrency::summary(&facts, before, after)),
            Err(e) => {
                eprintln!("testrun: {e}");
                Some(unchanged)
            }
        }
    }

    /// One line per finished test; failures also to the status file.
    fn report_finished(&self, i: usize) {
        let t = &self.tests[i];
        let Some(o) = &t.outcome else {
            return;
        };
        let total = self.tests.len();
        let cores = o.cpu / o.wall.max(0.01);
        println!(
            "[{:>4}/{total}] {:<17} {:>7.1} s {cores:>5.1} cores {:>9}  {}",
            self.finished,
            o.kind.label(),
            o.wall,
            report::size(o.peak),
            o.name
        );
        if o.kind.ok() {
            return;
        }
        let lane = if t.lane { "priority lane" } else { "" };
        let log = o.log.as_ref().map(|p| p.display().to_string());
        let log = log.unwrap_or_default();
        if !o.note.is_empty() {
            println!("       {}: {}", o.name, o.note);
        }
        if !log.is_empty() {
            println!("       log: {log}");
        }
        let kind = o.kind.label();
        let line = format!("{kind}\t{}\t{lane}\t{log}\t{}", o.name, o.note);
        self.append_status(&line);
    }

    /// Adds the runs recorded since the last save to the profile file, and
    /// the corpus tests' file runs.
    fn save(&mut self) {
        if !self.o.record {
            return;
        }
        let Some(path) = &self.profiles else {
            return;
        };
        if self.saved < self.records.len() {
            let runs = &self.records[self.saved..];
            let load = self.o.sensitive_load;
            match profile::save_runs(path, runs, &self.machine, load) {
                Ok(()) => self.saved = self.records.len(),
                Err(e) => eprintln!("testrun: {e}"),
            }
        }
        if self.saved_files < self.file_records.len() {
            let rows = &self.file_records[self.saved_files..];
            match profile::save_files(path, rows, unix_now()) {
                Ok(()) => self.saved_files = self.file_records.len(),
                Err(e) => eprintln!("testrun: {e}"),
            }
        }
    }

    /// Writes the end report and the results; the exit status. `ui`: what
    /// the run taught of the UI entries at once.
    fn finish_report(&self, ui: Option<&str>) -> i32 {
        let outcomes: Vec<Outcome> = self
            .tests
            .iter()
            .map(|t| t.outcome.clone().unwrap_or_default())
            .collect();
        let s = &self.samples;
        let per = |sum: f64, time: f64| if time > 0.0 { sum / time } else { 0.0 };
        let used = per(s.memory_used, s.memory_time);
        let mean_memory_used = (s.memory_time > 0.0).then_some(used);
        // The free budget: the report's budget is its cap, its line tells
        // what it was over the run.
        let free = self.free.map(|free| {
            let mean = (s.budget_time > 0.0).then(|| per(s.budget_sum, s.budget_time) as u64);
            free.line(mean)
        });
        let budget = self.free.map_or(self.limits.budget, |free| free.cap);
        let summary = Summary {
            mode: self.mode_name(),
            method: sys::method(self.cgroup),
            elapsed: self.started.elapsed().as_secs_f64(),
            cores: self.machine.cores,
            memory: self.machine.memory,
            budget,
            mean_load: per(s.load, s.time),
            mean_memory_used,
            outcomes: &outcomes,
            changes: &self.changes,
            stop: self.stop.as_deref(),
            profiles: self.profiles.as_deref().filter(|_| self.o.record),
            ui,
            free: free.as_deref(),
        };
        let text = report::render(&summary);
        println!("\n{text}");
        let _ = fs::write(self.out.join("report.txt"), &text);
        let table = report::results_tsv(&outcomes);
        let _ = fs::write(self.out.join("results.tsv"), table);
        let ok = outcomes.iter().all(|o| o.kind.ok());
        self.append_status(&format!("finished\t{}", report::counts(&outcomes)));
        if ok { 0 } else { 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str) -> TestDef {
        TestDef {
            name: name.to_owned(),
            available: true,
            ..TestDef::default()
        }
    }

    #[test]
    fn dependencies_and_fixtures_link() {
        let mut build = def("build");
        build.fixtures_setup = vec!["cargo".into()];
        let mut a = def("a");
        a.fixtures_required = vec!["cargo".into()];
        let mut b = def("b");
        b.fixtures_required = vec!["cargo".into()];
        b.depends = vec!["a".into(), "elsewhere".into()];
        let mut clean = def("clean");
        clean.fixtures_cleanup = vec!["cargo".into()];
        let (before, setups) = link(&[build, a, b, clean]);
        assert!(before[0].is_empty());
        assert_eq!(before[1], [0]);
        assert_eq!(before[2], [0, 1]);
        assert_eq!(before[3], [0, 1, 2]);
        assert_eq!(setups[1], [0]);
        assert_eq!(setups[2], [0]);
        assert!(setups[3].is_empty());
    }

    #[test]
    fn siblings_share_a_fixture_or_a_label() {
        let mut a = def("core.model");
        a.fixtures_required = vec!["cargo".into()];
        let mut b = def("core.vcs.rest");
        b.fixtures_required = vec!["cargo".into()];
        b.labels = vec!["vcs".into()];
        let mut c = def("core.vcs.sync");
        c.labels = vec!["vcs".into()];
        let d = def("alone");
        let groups = siblings(&[a, b, c, d]);
        assert_eq!(groups[0], [1]);
        assert_eq!(groups[1], [0, 2]);
        assert_eq!(groups[2], [1]);
        assert!(groups[3].is_empty());
    }

    #[test]
    fn log_names_are_plain() {
        assert_eq!(
            file_name("core.model.sketch-refs"),
            "core.model.sketch-refs"
        );
        assert_eq!(file_name("a/b c:d"), "a_b_c_d");
    }
}
