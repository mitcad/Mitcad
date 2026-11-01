# mitcad-testrun

The test runner over ctest (mitcad#790). It runs a build tree's ctests,
each as its own `ctest --test-dir <build> -R ^name$ -FA .*`, so that the
tests' commands, environments, timeouts and labels stay as ctest defines
them, and:

- measures every test of every run: wall time, CPU time (user and system)
  of the whole process tree, hence the mean busy cores, peak memory, and
  the kind of result (passed, skipped, failed, timeout, killed for memory,
  environment, not run);
- keeps the machine's profiles between runs: the last 5 runs of each test,
  its failure score (+10 for a failure, +2 for an environment failure, -1
  for a pass, never below 0) and whether it is timing-sensitive (it failed
  under a load of 70 % or more after passing before; 5 passes under load
  clear the mark);
- learns from the same runs how many UI entries the machine runs at once
  without one disturbing another ([below](#ui-entries-at-once));
- either runs the tests at a fixed parallelism and only records
  (`--measure`, the default), or schedules them (`--schedule`).

It is built with the project, beside the build's other programs:
`build\dev\mitcad-testrun.exe` on Windows, `build/dev/core/mitcad-testrun`
on Linux and macOS (a member of the core workspace). Its scheduling,
profiles and parsers are tested by the ctest `tools.testrun`.

## Use

On Linux and macOS:

```sh
# Measuring at the parallelism of today's runs (records only):
build/dev/core/mitcad-testrun --build build/dev --measure -j 3

# Scheduling by memory and CPU:
build/dev/core/mitcad-testrun --build build/dev --schedule

# On bare metal, by the memory the machine has free (less a reserve):
build/dev/core/mitcad-testrun --build build/dev --schedule --memory-budget free --memory-reserve 6G

# What would run, in which order, with what predictions (runs nothing):
build/dev/core/mitcad-testrun --build build/dev --schedule --dry-run

# A selection, as ctest takes it:
build/dev/core/mitcad-testrun --build build/dev --measure -R '^core\.' -E corpus
```

On Windows (the VM), from a shell where `ctest` is on `PATH` (or with
`--ctest <path to ctest.exe>`); the UI ctests need the build configured
with `MITCAD_MESA_DIR` as before:

```bat
tools\dev-env\msvc.cmd build\dev\mitcad-testrun.exe --build build\dev --schedule
```

`--help` lists every option. The run writes into `<build>/testrun`
(`--out`): `logs/<test>.log` (ctest's output of each test),
`status.txt` (a line per failure as it happens: kind, test, `priority
lane`, log, note; then `finished`), `report.txt` (the end report) and
`results.tsv` (a row per test: result, start, wall, CPU, peak, limit,
prediction, lane). The exit status is 0 when every test passed or was
skipped, 1 otherwise, 2 for a usage or listing error.

## Measuring per platform

- **Windows:** each ctest starts suspended, goes into a Job Object of its
  own and then runs; every process it starts stays in the job (ctest's own
  job for its test nests in it). The job gives the tree's total CPU time
  and peak committed memory (`PeakJobMemoryUsed`), holds the hard memory
  limit (`JOB_OBJECT_LIMIT_JOB_MEMORY`, reported through a completion
  port), and kills what is left of the tree when the test ends.
- **Linux:** each ctest runs in a systemd scope of its own (`systemd-run
  --user --scope`, cgroup v2) when the user's systemd is there
  (`--no-cgroup` turns it off): the scope's working set
  (`memory.current` less the page cache, `memory.stat`'s `active_file`
  and `inactive_file`) is sampled while the test runs and its largest is
  the peak, its `memory.events` OOM kills mark a test killed for memory,
  and `MemoryMax` is the hard limit. Not `memory.peak`: it counts the
  cache of every file the test wrote or read, which the kernel reclaims
  under the limit (a fresh cargo test build showed a 16 GiB peak). The CPU time comes from `wait4`'s rusage of ctest and the
  children it waited for. Without a scope the peak is the largest single
  process's (`ru_maxrss`).
  The scopes go into the slice of the user's systemd that the runner
  itself runs in (`--slice=`, the last `*.slice` below
  `user@<uid>.service` in `/proc/self/cgroup`), not into the user's
  default slice: a runner started in a job slice with a memory ceiling, or
  in the slice of its own that `tools/check-all.sh` makes for each run,
  keeps its tests under that ceiling. The memory the budget is a share of
  is the lowest limit (`memory.max`, or `memory.high` where lower) on the
  runner's cgroup path, from its own scope up, or the machine's memory;
  the memory available to a test is the machine's `MemAvailable`, or less
  where a cgroup on that path is nearer its limit (its limit less its
  working set: `memory.current` less the page cache of `memory.stat`), so
  that other jobs in a shared slice count too.
- **macOS:** `wait4`'s rusage (CPU time; the largest process's peak); the
  system load from the host's CPU ticks. No hard memory limits.

The system load for the CPU gate: `GetSystemTimes` (Windows), `/proc/stat`
(Linux), `host_statistics` (macOS), sampled every 0.5 s.

## The profile file

Per machine (cores and memory differ), outside every build tree and
mirror so that fresh build folders and removed mirrors keep it:
`MITCAD_TEST_PROFILES`, else `~/.mitcad/test-profiles.json`
(`%USERPROFILE%\.mitcad\test-profiles.json` on Windows), or `--profiles`.
Runs of several checkouts on the machine add to it one at a time (a lock
file beside it); `--no-record` leaves it alone.

```json
{
  "format": 1,
  "machine": { "cores": 16, "memory": 34359738368 },
  "tests": {
    "core.model": {
      "runs": [
        { "when": 1793000000, "result": "passed", "wall": 41.2, "cpu": 160.3,
          "peak": 2147483648, "load": 0.62, "limit": 8589934592 }
      ],
      "failure_score": 0.0,
      "sensitive": false,
      "loaded_passes": 0,
      "memory_kills": 0
    },
    "app.ui-windows-pick": {
      "runs": [
        { "when": 1793000100, "result": "failed", "wall": 31.0, "cpu": 20.1,
          "peak": 402653184, "load": 0.55, "limit": 4294967296, "beside": 3 },
        { "when": 1793000900, "result": "passed", "wall": 28.4, "cpu": 19.0,
          "peak": 398458880, "load": 0.21, "limit": 4294967296, "beside": 0 }
      ],
      "failure_score": 1.0,
      "sensitive": false,
      "loaded_passes": 0,
      "memory_kills": 0,
      "ui_room": 1
    }
  },
  "ui_caps": { "windows": { "cap": 3, "clean_runs": 1 } }
}
```

`beside` (a UI entry's runs only), `ui_room`, `room_passes` and
`ui_caps` are left out where they are not set
([UI entries at once](#ui-entries-at-once)). A corpus test keeps its
files' last runs too, `"files": { "<key>": { "wall": 41.3, "peak":
805306368, "when": 1793000000 } }` ([below](#corpus-tests)), left out
for the other tests.

The prediction of a test is the 90th percentile of its last runs' wall
time, busy cores and peak memory, the memory times `--margin` (1.25). A
test with no history:

- borrows the median prediction of its siblings that have one: the tests
  that require one of its fixtures or carry one of its labels (the crates'
  tests share `cargo_tests_debug`, for example);
- else is guessed by its kind: UI tests (`app.ui-*`, or labelled `ui`)
  1 GiB and 1 core, corpus tests (`corpus` in the name or a label) 2 GiB
  and 2 cores, the others 512 MiB and 1 core (`--default-memory`,
  `--default-cores`), all 60 s (`--default-wall`); ctest's `PROCESSORS`
  raises the cores. The cargo test builds run one at a time (RESOURCE_LOCK
  `cargo_build` on Linux and Windows, RUN_SERIAL on macOS).

It stays a test with no history until its own first run, which replaces
the guess. While fewer than half of the tests have a history, at most half
the cores' worth of tests run at once (as `ctest -j <cores / 2>`), so a
first run is not slower than ctest at that; with most tests known the cap
is the cores. `-j` sets the cap.

## Scheduling (`--schedule`)

Whenever a test ends, and every 0.5 s:

1. **Memory is the hard limit:** the running tests' predicted peaks plus
   the candidate's stay within the budget, and the memory available to
   the tests (read again at each decision; on Linux the cgroup headroom
   above, see "Measuring per platform") stays above `--memory-floor`
   (1.5 GiB). With nothing running any test starts, so that one larger
   than the budget still runs, alone. The budget (`--memory-budget`) is
   85 % of a basis: `MITCAD_TEST_MEMORY` when set, else the larger of the
   physical memory (on Linux the lowest cgroup limit above the runner
   where lower) and, on Windows, the commit limit (physical memory plus
   the paging file); the basis is read again during the run and only
   grows. An absolute `--memory-budget 24G` is used as given.
   **A VM with Hyper-V dynamic memory** reports only its current
   assignment as physical memory (6 or 18 GiB on different runs of the
   same VM): give it `--memory-budget` as a size (e.g. `24G`) or set
   `MITCAD_TEST_MEMORY` to its maximum memory.
   **On bare metal**, `--memory-budget free` (mitcad#872;
   `tools/check-all.sh` gives it there, not in a VM) is a budget from the
   free memory: at each decision it is the running tests' predicted
   memory plus the memory available less a reserve (`--memory-reserve`,
   default the larger of 4G and a tenth of the basis) less what the
   running tests are still predicted to grow into (each one's prediction
   less its scope's working set now: a test just started has not yet
   taken its memory, which the available memory does not show), at most
   85 % of the basis (`schedule::free_budget`). So a candidate starts only
   while the memory available exceeds the reserve by its prediction and
   that growth: when another job (an agent's build) takes memory, fewer
   tests start, and more when it ends. The corpus tests get the reserve
   as `MITCAD_CORPUS_RESERVE` for their children ([below](#corpus-tests)).
   The run's first lines and the end report give it (`Memory budget from
   the free memory: ...`, at the start and its range over the run); the
   report's `budget` is its cap.
2. **CPU is the soft limit:** no test starts at or above `--cpu-gate`
   (0.9) of the load; the load is the measured one or the running tests'
   predicted cores where higher (a test just started has not yet loaded
   the processors). `-j` caps the tests at once (default: the cores, or
   half of them while fewer than half of the tests have a history).
3. **Longest first:** each test's rank is its predicted wall time plus the
   longest chain of tests waiting for it (DEPENDS, fixtures).
4. **Complementary pairing:** among the tests ranked within 60 % of the
   longest, the one whose memory and cores leave the remaining memory and
   CPU in the most even shares (a memory-heavy CPU-light test when much
   memory and little CPU is left, and the other way round).
5. **The priority lane:** tests with a failure score of at least
   `--lane-score` (1) start first, highest score first; while lane tests
   wait, the others may use only 80 % of the budget (`--lane-share`) and
   one slot less, so the lane runs beside the long tests.
   With `--changed FILE` the tests related to the change join the lane
   too ([below](#tests-related-to-a-change)).
6. **Timing-sensitive tests** start only below `--sensitive-load` (0.7),
   at most `--sensitive-max` (2) at once; `--sensitive 'app.ui-*'` marks
   tests sensitive from the start. With a bound for the UI entries
   (`--ui-max` or `MITCAD_UI_JOBS`), **UI entries** start only while fewer
   than the machine's learnt cap run, and only where neither they nor a
   running UI entry would have more UI entries beside them than their room
   ([below](#ui-entries-at-once)); a UI entry that failed beside others
   runs again alone, after the others. Without a bound they start as other
   tests do.
7. **Safety:** each test's hard memory limit is the largest of 4 GiB,
   `--limit-factor` (2) times the p90 of its history's peaks and that
   factor times its kind's guess, at most 95 % of the memory. RUN_SERIAL
   tests (they run alone) and tests whose history's largest peak is over
   4 times its smallest (a cold build after warm ones) get no limit. A
   test killed for memory counts as an environment failure (+2), its
   profile keeps the run with the larger of its peak and the limit, and it
   reruns once alone without a limit (the report names the rerun); no test starts
   while the build folder's disk has less than `--disk-floor` (10 GiB)
   free, and the tests left are reported as not run for that reason.

Both modes keep exactly the properties ctest reports, nothing more:
DEPENDS and a fixture's setup tests order only the tests that name them,
a test whose fixture setup failed is not run, cleanups run after every
test of their fixture, a RESOURCE_LOCK keeps apart only the tests with the
same lock, and only a test with RUN_SERIAL set runs alone (as with ctest
-j, nothing else runs beside it). In this tree the cargo test builds,
`core.cargo_build` and `core.cargo_build_release`, share the lock
`cargo_build` on Linux and Windows and have RUN_SERIAL on macOS
(`core/CMakeLists.txt`); the Windows UI tests that use the system
clipboard, which all tests of one ssh logon share, hold the lock
`system_clipboard` (mitcad#870). macOS Aqua UI and render-surface tests
share `macos_desktop`, so they exclude each other while independent model,
geometry and offscreen unit tests may run beside them. The shell UI
harness also holds a per-user file lock across separate runs and checkouts.
RUN_SERIAL remains for the timed `cli.perf_*`, the macOS Cargo builds and
Metal scene acceptance, and the Metal surface tests whose parallel
execution has not been validated. A macOS Cargo build needs this isolation because
its memory profile records the largest child process rather than the
whole compiler process tree, and no hard memory limit is available.
`--dry-run` shows
each test's RUN_SERIAL and RESOURCE_LOCK. A test still running two minutes after its ctest TIMEOUT
(1500 s by default) is stopped and counted as a timeout.

Every failure is printed as it happens (marked `priority lane` for the
lane's tests) and appended to `status.txt`, so that a fixer can start
while the run goes on. The end report gives the total time; the means over
the run of the system's CPU load and memory in use (sampled every 0.5 s,
weighted by time), of the tests' busy cores, of the tests running at once
and of their predicted memory (the sums of each test's CPU time, wall
time and predicted memory times wall time, over the run's wall time); the
critical path, the chain of tests that ended the run, each with what held
it back (its dependency, a test that runs alone, a resource lock, or memory
and CPU that came free) and how long it waited; the longest tests, the
failures by kind and the tests whose profile changed much (wall time or
peak halved or doubled). In `results.tsv` the `lane` column is 1 for a
test that ran in the priority lane.

## UI entries at once

How many UI entries a machine runs at once without one disturbing another
(roadmap step 5: the Windows VM's session 0, Linux's Xvfb displays,
macOS) is learnt from the ordinary runs, not from a measuring run of its
own (`src/concurrency.rs`). A UI entry is a test of the UI kind
(`app.ui-*`, or labelled `ui`), or a UI test `tools/check-all.sh` runs
itself and hands over (`--record-ui`, below).

- **What each run records:** a UI entry's run keeps `beside`, the most
  other UI entries that ran beside it. In `--schedule` with a bound (see
  below), a UI entry that failed or timed out with others beside it runs
  once more alone, after the others (as check-all's own UI step does,
  mitcad#657); its result counts, the first attempt is named in the
  report (`Rerun alone (failed beside other UI entries)`), in
  `status.txt` (`ui`) and in its note.
- **A contention failure:** a UI entry failed beside `b` others and then
  passed with fewer beside it (alone). It adds +2 to the failure score,
  as other failures of the environment, not +10.
- **A test's room** (`ui_room` in its profile): the other UI entries it
  may have beside it. A contention failure beside `b` halves it to `b / 2`
  (0: no other UI entry beside it); 3 passes with the room full (at least
  that many beside it) grow it by one. `--dry-run` shows it (`UI room N`).
- **The machine's cap** (`ui_caps`, per platform, `std::env::consts::OS`):
  UI entries at once. Bounded by `--ui-max`, else `MITCAD_UI_JOBS` (4 for
  the learning when neither is given), it starts at the bound (Linux
  with `MITCAD_UI_JOBS=12`: 12, as before), drops
  by one (at least 1) after a run with contention failures of two or more
  UI entries (`CAP_DROP_TESTS`, mitcad#872: one test's contention failure
  lowers only its room, which the scheduler and check-all keep; ui-pick
  and ui-sweep alone held Linux's 196 UI tests at 7 or 8 at once of the
  bound 12, since check-all did not keep the rooms), and grows by
  one after 2 clean runs that reached it (no UI entry failed beside
  others), never above the bound. A run with fewer UI entries at once, or
  with a UI failure that failed alone too, leaves it. The end report gives
  the run's line (`UI entries at once: cap 6 -> 7; this run at most 6,
  ...`). Measuring mode records and learns too, but neither reruns nor
  holds UI entries to the cap.

**A machine opts in by giving the bound** (`--ui-max N` or
`MITCAD_UI_JOBS`). Only then does the scheduler start a UI entry only
while the UI entries running are fewer than the cap and its room and every
running UI entry's room allow one more beside them (non-UI tests are not
counted), and rerun a UI entry alone after a failure beside others.
Without a bound the runs still record `beside` and learn the cap and the
rooms (within 4), but UI entries are scheduled as before: no cap, no
rooms, no rerun. With a bound the fixed `MITCAD_UI_JOBS` is no longer the
number at once but its ceiling: the runner starts there, goes below it
only after contention failures and back up as clean runs show, never
above it. `--dry-run` and the run's first
lines say which applies.

`tools/check-all.sh` runs the Linux UI tests (`tools/ui-*-test.sh`) itself,
not as ctests. For such a caller:

```sh
# The cap to run at (within the bound; MITCAD_UI_JOBS when --ui-max is not given):
build/dev/core/mitcad-testrun --ui-cap --ui-max "$MITCAD_UI_JOBS"
# Each UI entry's learnt room, a line each: name, the most other UI entries
# beside it (check-all starts such a test only while few enough run, and
# no other while it runs with its room full, as the scheduler does):
build/dev/core/mitcad-testrun --ui-rooms
# Its UI runs, a line each in the order they ended (both attempts of a
# test rerun alone): name, exit status, seconds, other UI entries beside it.
printf 'ui-pick\t1\t40\t11\nui-pick\t0\t38\t0\n' > ui-runs.tsv
build/dev/core/mitcad-testrun --record-ui ui-runs.tsv --ui-max "$MITCAD_UI_JOBS"
```

`--record-ui` adds the runs to the profiles (exit status 0 passed, 124 and
137 timed out, 77 skipped, others failed), learns the cap from them and
prints what it learnt; neither needs `--build`.

## Corpus tests

A corpus test (`corpus` in its name or a `corpus` label) imports its
files in child processes, several at once (`core/tests/parallel_runs.hpp`:
`test_f3d_import`, `test_brep_import`, `test_exchange`, and
`mitcad_run_parallel` for the scripts' corpora, `f3d.corpus_designs`
among them, its imports with a debug build's own-start time, mitcad#847),
mitcad#832. `rvt.corpus` (its project and its link check, keyed by the
content ids they import) and `dwg.corpus` (a drawing each) do so too
since mitcad#875; they run in `tools/check-all.sh`'s ctest step, which
gives them `--corpus-children` as the corpus step does its tests:

- **Each file's time and peak.** For every corpus test the runner names
  a table in `MITCAD_CORPUS_RECORD` (`<out>/corpus/<test>.runs.tsv`), to
  which the test appends a line per child: the file's key (its path, or
  the label of a `mitcad_run_parallel` list), seconds, peak bytes and
  `ok` or `failed`. The profiles keep each file's last run under the
  test's `files`; a file not run for 60 days goes. `--no-record` leaves
  them alone.
- **The longest files first.** A test with files in its profile gets
  their times in the table `MITCAD_CORPUS_FILES` names
  (`<out>/corpus/<test>.files.tsv`: key, seconds, peak bytes). Its
  children start by them, the longest first; a file without a time by
  its size at the known files' median seconds per byte, or among the
  first when it has no size.
- **Children from the profile** (`--corpus-children`, which
  `tools/check-all.sh` gives its corpus step unless `--corpus-jobs` is
  given). Each corpus test with files in its profile gets its children at
  once in `MITCAD_CORPUS_JOBS`: every such test starts with two (one with
  a single file), then a child at a time goes to the test that would end
  last (its work, the sum of its files' times, over its children, never
  below its longest file), as long as one more shortens it, all the
  children together within the machine's cores and their predicted
  memory (256 MiB per test and each child its test's largest file's peak
  times `--margin`) within the budget. So a long test gets more children
  and a short one fewer, and they end at about the same time. A child's
  memory limit, `MITCAD_CORPUS_MEMORY`, is `--limit-factor` times its
  test's largest peak of a file (a limit on committed memory, which
  exceeds the resident peak), never below the caller's
  `MITCAD_CORPUS_MEMORY` (1536M when unset). The test's own prediction
  and hard memory limit follow its children. The corpus tests without
  files in their profile (their first run, or a test that imports in one
  process) keep the caller's variables, and their predictions are kept
  out of the room the others share. The run's first lines name each
  planned test's children; `--dry-run` shows the variables.
- **A test that no longer imports in children** (mitcad#872): a corpus
  test that passes while its children recorded nothing, although its
  profile has files, imports in one process now, and its files go from
  the profile (`corpus::one_process`), so that the next run predicts it
  by its own history. `f3d.corpus_designs` checked its designs one after
  another from 0.32.0 until mitcad#874 put them back on children;
  planned by its files it was predicted at its longest design's 314 s
  with two children's 7 GiB held for it, and took 498 s.
- **Children wait for memory** (mitcad#872): with the budget from the
  free memory the runner gives each corpus test `MITCAD_CORPUS_RESERVE`
  (the reserve, unless the caller set it); a child beside others then
  starts only while the memory available (`MemAvailable`, or less where
  a cgroup above is nearer its limit, its working set without the page
  cache) exceeds the reserve by the child's expected peak (its file's
  last, from `MITCAD_CORPUS_FILES`, else the limit per child), checked
  again every 0.5 s (`parallel_runs.hpp`, `may_start`).

## Tests related to a change

`--changed FILE` (roadmap step 4) names a list of the changed files, a
path a line, relative to the source tree as `git diff --name-only <base>`
prints them (absolute paths in the tree are taken too; empty lines, lines
starting with `#` and paths outside the tree are left out). Each test
related to one of them gets a failure's raised place for this run: 10 is
added to its failure score (to order the lane; the profile keeps its own
score) and, in scheduling mode, it joins the priority lane whatever
`--lane-score` says, so that a change's own tests run and report first.
The run prints the changed files' count and the related tests.

A test's sources, from its ctest registration and the tree
(`src/changed.rs`):

- the files and folders of the source tree its command and its
  `ENVIRONMENT` name (its script, a cli test's JSON, a `cmake -P` script,
  a folder of tests), not those in the build folder;
- the areas each of those files declares in `# check-all sources:
  <path>...` lines, the header lines `tools/check-all.sh` reads for its UI
  tests (the UI ctests' scripts: `tools/ui-*-test.py`,
  `ui-windows-*-test.ps1`, `ui-macos-*-test.sh`, `ui-scripts/*.mitcad-ui`);
- for a crate's tests (`cargo-tests.cmake -- run|parts|build <package>`,
  `cargo test --doc -p <package>`) each package's folder, from the
  workspace's members (`core/Cargo.toml`; no globs);
- for a test program CMake builds (the command's program, e.g.
  `test_sheet`, `mitcad-cli`) its own target's sources, from CMake's file
  API reply in the build folder (`.cmake/api/v1/reply`, which
  `tools/check-all.sh` asks for; without it this part is left out).

The source and build trees come from the build's `CMakeCache.txt`. A
source covers a changed path that is that file or lies in that folder.
Only these direct sources count, not the crates and libraries they use in
turn, so that a change to a crate or library every test uses does not put
every test in the lane (the fingerprints of `tools/ctest-fingerprints.py`
decide which tests run at all; this only orders them).

## Known limits

- Each test is its own ctest process; parallel ctests of one build tree
  share its `Testing/Temporary` files (`LastTest.log`,
  `CTestCostData.txt`). The results come from each ctest's own output, but
  a platform that refuses the concurrent writes would show it as
  environment failures in the first runs.
- Linux: the peak is the largest working set sampled every 0.1 s, so a
  spike shorter than that is seen only through the largest process's
  `ru_maxrss`; the last 0.1 s before the scope goes is not sampled.
- Windows: the peak is committed memory, which can exceed the working set.

## Left for later (mitcad#790's roadmap)

- Step 1 and 2 are here: measuring as today, and scheduling, first for the
  Windows VM.
- Step 3: Linux `tools/check-all.sh` runs its ctest and corpus steps through
  `--schedule` (ctest -j in a test slot when the runner is not built or
  does not finish), and so does the Linux release build, whose tests are
  `check-all.sh --full`; the check's slice holds the tests' scopes
  ([the check script](../../docs/development.md#the-check-script)).
  macOS uses `tools/test-macos.sh` for merge and release checks (one Aqua
  UI test at a time; initial take-up in serial measure mode).
- Step 4: the tests related to the files a change touches join the
  priority lane (`--changed`, [above](#tests-related-to-a-change));
  `tools/check-all.sh` hands it the files of its diff.
- Step 5: how many UI entries run at once without disturbing each other
  (Windows session 0, Xvfb displays, macOS), learnt from the runs: a
  machine cap per platform and each test's room
  ([above](#ui-entries-at-once)), applied where a machine gives the bound
  (`--ui-max` or `MITCAD_UI_JOBS`); check-all's own Linux UI step can take
  `--ui-cap` and hand its runs over with `--record-ui`. macOS's Aqua UI
  tests hold `macos_desktop`, so they stay one at a time while non-UI
  tests can use the remaining resources.
