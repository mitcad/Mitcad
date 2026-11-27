#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The checks before a commit, in the Linux build environment (run from any
# directory; it works in the checkout it belongs to): configure and build
# with the dev preset, cargo fmt and clippy, the dependency check, the
# user guide's table of contents (tools/check-guide-contents.py), on a
# host newer than the Linux release's build root the changed C++ files
# compiled with the root's older GCC (tools/check-root-compile.py), ctest,
# the ctests, the corpus tests and the UI tests (tools/ui-*-test.sh) whose
# sources changed, several at once.
#
# Two depths. By default (development, before a merge to main) the check
# is scoped to the change: a test runs only when what it depends on
# differs from every run in which it passed on this machine, a few long
# tests that seldom catch anything (FULL_ONLY in tools/ctest-fingerprints.py)
# are left out, and a UI test depends only on the areas it declares (the
# smoke set, UI_SMOKE, on everything the application is built from), so
# that now and then a fault slips through to the full check. --full runs
# everything: every ctest, the corpus and every UI test. Use it before a
# release build.
#
# Every step's time and result go to build/dev/check-timings.tsv (step,
# seconds, result; also appended to check-timings-history.tsv with the
# run's start, the rows of single ctests and corpus tests too) and are printed as a table at the end, so that each run
# measures itself; the steps' output is in build/dev/check-logs/. The exit
# status is 0 only if every step passed.
#
# Usage: tools/check-all.sh [--full] [--ui-jobs N] [--ui-retry-jobs N] [--ui auto|all]
#          [--ctest auto|all] [--ctest-jobs N] [--testrun auto|off]
#          [--corpus auto|always|never] [--corpus-tests N] [--corpus-jobs N]
#          [--memory SIZE|auto] [--overlap auto|off] [--fresh] [--only STEP,...]
#          [--release-passed DIR --release-tag X.Y.Z-rcN [--all-geometry]]
#   --full          the full check, before a release build: --ctest all,
#                   --corpus always, --ui all (--release is the same), and
#                   at the end each fuzz target of core/fuzz for 60 s
#                   ($MITCAD_FUZZ_SECONDS) when cargo-fuzz and a nightly
#                   toolchain are installed (skipped with a note
#                   otherwise; mitcad#939).
#   --ui-jobs N     UI tests at once (default $MITCAD_UI_JOBS, else 4). The
#                   longest ones start first, by their last times in the
#                   history. The app runs without gdb (UI_GDB=0, its own
#                   crash handler logs the stack); a test that crashed runs
#                   once more under gdb (UI_GDB=1) for gdb's backtrace, as
#                   ui-<name>.gdb.log, and stays failed whatever that run
#                   gives.
#   --ui-retry-jobs N
#                   the UI tests that failed among the others run again
#                   after them (and after ui-compute), N at once (default
#                   $MITCAD_UI_RETRY_JOBS, else 3; 1: one at a time),
#                   longest first, each with fewer other UI tests beside
#                   it than its failure had (none when that had none) and
#                   within its room (mitcad#2177).
#   --ui MODE       "all", or "auto" (default): a UI test runs only when the
#                   files it depends on differ from every run in which it
#                   passed on this machine (kept in
#                   ~/.cache/mitcad/ui-passed/<name>): the areas it
#                   declares in header lines "# check-all sources:
#                   <path>..." (several such lines add up: the parts of
#                   app/ and core/ it exercises, tools/cli when it makes
#                   designs with mitcad-cli, ...; a declared tools/cli
#                   counts only the program's own files and build lines
#                   and the fixtures of tools/cli/tests/ that UI material
#                   names, not the ctests' scripts and fixtures; a declared
#                   app/framework/TestDriver.cpp counts TestDriver.cpp and
#                   the driver's area files whose steps the test's scripts
#                   use, mitcad#2173), the build
#                   files and options (UI_BASE, without the Windows and
#                   macOS UI tests' registrations in cmake/ui-tests/ and
#                   the OCCT port's patches that change only its modelling,
#                   which count for the smoke set and the tests that
#                   declare geometry, mitcad#2136), its script and the test library
#                   (UI_LIBRARY: the top-level code of tools/ui-test-lib.sh
#                   and the functions the test calls, tools/ui-test-deps.py,
#                   mitcad#2173), and the main window's layout as the
#                   built application lays it out (tools/ui-layout.sh:
#                   the window's size, where its bars, docks and view
#                   are), so that a change of the window reruns every UI
#                   test; when the layout cannot be read, none is
#                   skipped. The smoke set (UI_SMOKE) depends on
#                   everything the application is built from (UI_COMMON)
#                   besides, so a change anywhere in app/, core/ or
#                   geometry/ runs at least it. A test named in --only
#                   always runs.
#   --ctest MODE    "all", or "auto" (default): a ctest runs only when its
#                   fingerprint differs from every run in which it passed
#                   on this machine (~/.cache/mitcad/ctest-passed/<name>):
#                   what its command runs and names, the targets and crates
#                   they are built from, the build files, the compiler and
#                   the options, found from the build itself
#                   (tools/ctest-fingerprints.py says how). Its FULL_ONLY
#                   tests run only with "all". --only ctest:<name> runs that
#                   test whatever changed (with "ctest" also the others
#                   that changed).
#   --ctest-jobs N  the most ctests at once (default: the test runner
#                   decides by memory and CPU; ctest -j without it: the
#                   cores / 4).
#   --testrun MODE  "auto" (default, $MITCAD_TESTRUN): the ctests and the
#                   corpus group run through the test runner
#                   build/dev/core/mitcad-testrun --schedule (tools/testrun,
#                   mitcad#790) when it is built: each test in a systemd
#                   scope of its own, as many at once as the memory budget
#                   (85 % of the check's limit, or of what a slice above it
#                   allows) and a CPU gate allow, the longest first,
#                   chronic failures early, the test at the head of the
#                   longest chain (the cargo test build) with the first
#                   claim on the memory that comes free (mitcad#2177);
#                   its report, results and each
#                   test's log in build/dev/testrun/<step>/, its profiles in
#                   ~/.mitcad/test-profiles.json. "off", or when it is not
#                   built or does not finish: ctest -j in a test slot.
#   --corpus MODE   the corpus group: f3d.corpus*, f3d.models_loft (the
#                   reference models replayed), freecad.corpus*,
#                   freecad.reported, ipt.corpus, iam.corpus*, ifc.corpus and
#                   solidcraft.corpus, after
#                   ctest's other tests, several at once, each importing
#                   several files at once in child processes (mitcad#70).
#                   "always", "never",
#                   or "auto" (default): only when the files they depend on
#                   (CORPUS_SOURCES below: the import, the readers, the
#                   geometry, the bridge's kernel and import files and how
#                   they are built, and the resolved dependencies of the
#                   import's crates, CORPUS_CRATES) or the corpora differ
#                   from every corpus run that passed on this machine,
#                   which are kept in ~/.cache/mitcad/corpus-passed.
#                   Markdown documents count for neither fingerprint.
#   --corpus-tests N, --corpus-jobs N
#                   corpus tests at once (ctest -j) and the files each
#                   imports at once (MITCAD_CORPUS_JOBS). By default, with
#                   the test runner, the runner gives each corpus test
#                   (also those of the ctest step, rvt.corpus and
#                   dwg.corpus, mitcad#875) its
#                   files at once from its profile (mitcad-testrun
#                   --corpus-children, mitcad#832): every run records each
#                   file's time and peak in its child process, and the
#                   next run starts the longest files first and shares
#                   the machine's cores and the run's memory budget among
#                   the corpus tests by their work, a long test more
#                   children, a short one fewer, so that they end at about
#                   the same time; a child's memory limit is twice its
#                   test's largest peak of a file, never below
#                   CORPUS_MEMORY. A test without that history yet (and
#                   every test with --corpus-jobs, or without the runner)
#                   takes the fixed plan: all the children together as
#                   many as the memory limit allows, CORPUS_MEMORY (1536M)
#                   each (MITCAD_CORPUS_MEMORY: the memory a child may
#                   commit), with 1G left for the rest, and at most the
#                   cores / the test slots, about the square root of that
#                   many tests at once (with the runner the tests at once
#                   only with --corpus-tests). A child runs at most
#                   CORPUS_TIMEOUT (300 s, MITCAD_CORPUS_TIMEOUT).
#   --memory SIZE   the memory the whole run may use (default
#                   $MITCAD_CHECK_MEMORY, else "auto"; "none": no limit), in
#                   a systemd slice of its own (MemoryMax, no swap) beneath
#                   the caller's slice, the test runner's per-test scopes
#                   in it too: what needs more is killed, not the
#                   machine's other work (without busctl: a scope).
#                   "auto" (mitcad#872): on bare metal a budget from the
#                   free memory: the test runner (--memory-budget free) and
#                   the corpus tests' children (MITCAD_CORPUS_RESERVE) start
#                   a test or a child only while the memory available
#                   (MemAvailable, or less where a slice above is nearer
#                   its limit, the jobs slice among them) exceeds a reserve
#                   for the desktop and the other jobs by what it is
#                   predicted to take, read again before each start, so
#                   that the check backs off while another job (an agent's
#                   build) runs and takes the room when it ends; the slice's
#                   limit is then the machine's memory less the reserve.
#                   The reserve: $MITCAD_CHECK_RESERVE, else the larger of
#                   4G and a tenth of the memory. In a virtual machine
#                   (systemd-detect-virt --vm, WSL, the hypervisor flag in
#                   /proc/cpuinfo), whose free memory can read low (dynamic
#                   memory, ballooning), "auto" is the fixed 12G as before.
#                   "dynamic": the budget from the free memory as on bare
#                   metal, also in a virtual machine whose memory is fixed
#                   (no ballooning, no dynamic memory).
#                   The run's first lines say which applied.
#   --overlap MODE  "auto" (default, $MITCAD_CHECK_OVERLAP) or "off": with
#                   "auto" the lint steps (fmt, clippy, deps, guide,
#                   root-compile) run
#                   beside the ctest step and the corpus step beside the UI
#                   tests (the UI tests run alone again only after the corpus
#                   step),
#                   each group in the background with its output printed
#                   when it ends (check-logs/<group>.out), when the test
#                   runner runs the tests; "off": one step after another.
#   --fresh         configure from scratch (cmake --fresh: a new CMake
#                   cache; the objects, cargo's builds and vcpkg_installed
#                   stay), when a cached value has to be found again: an
#                   option's or cache variable's default, a find_* result,
#                   the compiler or toolchain. Not for a changed vcpkg port
#                   (OCCT's patches): every configure runs vcpkg's install,
#                   which rebuilds a changed port, and a change in a port's
#                   folder makes the build configure again (mitcad#2134).
#   --only STEPS    only these steps: configure, build, fmt, clippy, deps,
#                   guide, root-compile, ctest, corpus, ui, fuzz (with
#                   --full), or single UI
#                   tests (ui-sketch) or ctests (ctest:core.model).
#   --release-passed DIR, --release-tag X.Y.Z-rcN, --all-geometry
#                   with --full, the Linux release gate's (release/
#                   build-linux.sh, mitcad#1822): the ctest step's test
#                   runner skips a geometry* ctest whose fingerprint passed
#                   in an earlier release gate on this platform (kept in
#                   DIR/linux/<test>), keeps this run's passes with the
#                   release's name and lists the skipped tests in
#                   build/dev/testrun/ctest/release-skipped.tsv (each a
#                   skipped ctest:<name> row); --all-geometry runs them
#                   all (tools/testrun/README.md, "Release fingerprints").
#                   Development checks never give them.
#   --dry-run       builds and runs nothing: lists the ctests, the corpus
#                   and the UI tests that would run for the working tree as
#                   it is (against the last configure's targets), and why.
#
# A UI test that fails keeps the application's log as ui-<name>.app.log
# beside its own; when it runs again, both are renamed *.failed.log and
# *.failed.app.log first (run_ui).
#
# The passes of the ctests, the corpus and the UI tests are kept only from
# runs that built (the build step ran), so that a test of an older build
# does not count for newer sources. The tests left out appear as "skipped"
# rows of the table (a ctest's indented, as ctest:<name>).
#
# The UI tests, and the ctests and the corpus group when they run without
# the test runner (ctest -j), hold one of two test slots, files that
# flock(1) locks ($MITCAD_TEST_LOCK, default ~/.mitcad-test.lock, and
# $MITCAD_TEST_LOCK.2; $MITCAD_TEST_SLOTS of them), so that at most two
# checks on one machine test at the same time, each within its memory;
# "flock ~/.mitcad-test.lock <command>" takes the first slot. The runner
# needs no slot: it fits its tests into the memory that is left. Building and
# lint take none; unless set, they use half the cores
# (CMAKE_BUILD_PARALLEL_LEVEL, CARGO_BUILD_JOBS). tools/ui-compute-test.sh
# runs alone after the other UI tests: it fails intermittently under load
# (mitcad#8). A UI test that fails among the others without crashing runs
# once more after them, with fewer UI tests beside it (--ui-retry-jobs); a
# pass then shows as "ok-again" and counts (mitcad#657: tests that count
# events in a wall-clock window miss under load), a second failure fails
# the check.
set -uo pipefail
# Never a test on the desktop: since mitcad#2031 the app picks Wayland when
# WAYLAND_DISPLAY is set, ahead of the Xvfb display a test gives it.
unset WAYLAND_DISPLAY

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
SELF=$ROOT/tools/check-all.sh
cd "$ROOT" || exit 2
BUILD=$ROOT/build/dev
LOGS=$BUILD/check-logs
TIMINGS=$BUILD/check-timings.tsv
HISTORY=$BUILD/check-timings-history.tsv
LOCK=${MITCAD_TEST_LOCK:-$HOME/.mitcad-test.lock}
SLOTS=${MITCAD_TEST_SLOTS:-2}
UI_TIMEOUT=${UI_TIMEOUT:-1800}
# The corpus group's tests, and what they depend on besides the corpora:
# the readers and the import, the bridge's kernel and import files (not
# other families'), the geometry, mitcad-cli's imports (import.cpp, not
# main.cpp), how they are built, and the import's crates' resolved
# dependencies (cargo tree; not Cargo.toml and Cargo.lock, which change for
# crates of other parts too).
# The OCCT port counts, its docs (third_party/vcpkg-ports/README.md, docs/)
# do not (mitcad#794).
CORPUS_TESTS='^(f3d\.corpus|f3d\.models_loft$|freecad\.corpus|freecad\.reported$|ipt\.corpus$|iam\.corpus|ifc\.corpus$|solidcraft\.corpus$)'
CORPUS_SOURCES=(core/f3d core/import core/freecad core/zip core/ipt core/tests
  core/ffi/src/kernel core/ffi/src/*_import.rs core/ffi/src/ipt_history.rs core/ffi/src/exchange.rs
  core/ffi/src/memory.rs core/cpp core/CMakeLists.txt geometry tools/cli/import.cpp tools/cli/import.hpp
  tools/cli/CMakeLists.txt tools/cli/fcstd-corpus.cmake tools/cli/ipt-corpus.cmake tools/cli/iam-corpus.cmake
  CMakeLists.txt rust-toolchain.toml vcpkg.json third_party/vcpkg-ports/opencascade)
# The IFC import (mitcad#273).
CORPUS_SOURCES+=(core/ifc tools/cli/ifc-corpus.cmake)
# The vector helpers the readers and the import use (mitcad#582).
CORPUS_SOURCES+=(core/vec3)
# Single .f3d designs checked item by item (mitcad#311).
CORPUS_SOURCES+=(tools/cli/f3d-designs.cmake)
# The .solidcraft import (mitcad#450).
CORPUS_SOURCES+=(core/solidcraft third_party/solidcraft tools/cli/solidcraft-corpus.cmake)
# The crates split off the .f3d reader (mitcad#776).
CORPUS_SOURCES+=(core/dump core/asm core/f3d-names core/f3d-file core/f3d-design)
CORPUS_CRATES=(mitcad-import mitcad-f3d mitcad-freecad mitcad-ipt mitcad-zip mitcad-ifc mitcad-solidcraft)
CORPUS_CRATES+=(mitcad-dump mitcad-asm mitcad-f3d-names mitcad-f3d-file mitcad-f3d-design)
# The design segment's decoding split off mitcad-f3d-design (mitcad#779).
CORPUS_SOURCES+=(core/f3d-decode)
CORPUS_CRATES+=(mitcad-f3d-decode)
# The imports' crates split from mitcad-import (mitcad#775).
CORPUS_SOURCES+=(core/import-base core/import-ifc core/import-rvt core/import-fcstd)
CORPUS_CRATES+=(mitcad-import-base mitcad-import-ifc mitcad-import-rvt mitcad-import-fcstd)
# The crates split from the dump replay (mitcad#785).
CORPUS_SOURCES+=(core/import-solidcraft core/import-replay core/import-joints core/import-sheet
  core/import-primitives)
CORPUS_CRATES+=(mitcad-import-solidcraft mitcad-import-replay mitcad-import-joints mitcad-import-sheet
  mitcad-import-primitives)
# The appearances family above the replay's core (mitcad#791).
CORPUS_SOURCES+=(core/import-appearances)
# FreeCAD documents reported with import faults (mitcad#998).
CORPUS_SOURCES+=(tools/cli/fcstd-reported.cmake)
CORPUS_CRATES+=(mitcad-import-appearances)
CORPUS_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/corpus-passed
# The build files every UI test depends on, what the application is built
# from (which the smoke set depends on), the smoke set, and the test
# library every UI test uses.
UI_BASE=(third_party CMakeLists.txt CMakePresets.json cmake triplets rust-toolchain.toml vcpkg.json)
UI_COMMON=(app geometry core)
UI_SMOKE=(smoke workflow command file)
UI_LIBRARY=(tools/ui-test-lib.sh tools/ui-image-stats.py tools/xwd2png.py tools/ui-visible-text-allow.txt)
UI_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/ui-passed
# Narrower UI fingerprints (mitcad#2136). What under UI_BASE no Linux UI
# test depends on: the Windows and macOS UI tests' registrations, which
# cmake/MesaRuntime.cmake includes only on Windows and cmake/MacUiTests.cmake
# only on macOS. The OCCT port, whose patches that change only OCCT's
# modelling (ui_occt_modelling) count like geometry/ rather than as the base.
# A declared tools/cli is narrowed to what the UI tests use of it
# (ui_cli_fingerprint).
UI_BASE_OTHER_PLATFORMS=cmake/ui-tests
UI_OCCT_PORT=third_party/vcpkg-ports/opencascade
# What a test uses of the test library and of the UI test driver
# (mitcad#2173, tools/ui-test-deps.py): the library's top-level code and the
# functions the test calls, and for a declared UI_DRIVER the driver's core
# and the area files whose steps its scripts use. When the helper fails, the
# whole library and every file of the driver count (UI_DRIVER_ALL).
UI_DRIVER=app/framework/TestDriver.cpp
UI_DRIVER_ALL=(app/framework/TestDriver.cpp app/framework/TestDriver.hpp app/framework/TestDriverSupport.hpp
  app/framework/TestDriver?*.cpp)
CTEST_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/ctest-passed
# The memory a corpus test's child may commit, and its time.
CORPUS_MEMORY=${MITCAD_CORPUS_MEMORY:-1536M}
CORPUS_TIMEOUT=${MITCAD_CORPUS_TIMEOUT:-300}

UI_JOBS=${MITCAD_UI_JOBS:-4}
UI_JOBS_GIVEN=0
# The UI tests run again after failing among the others, at once
# (mitcad#2177).
UI_RETRY_JOBS=${MITCAD_UI_RETRY_JOBS:-3}
UI=auto
CTEST=auto
CTEST_JOBS=$(($(nproc) / 4))
[ "$CTEST_JOBS" -ge 1 ] || CTEST_JOBS=1
# The test runner (tools/testrun, mitcad#790), built with the project; its
# most tests at once only when --ctest-jobs is given.
TESTRUN=${MITCAD_TESTRUN:-auto}
TESTRUN_BIN=$BUILD/core/mitcad-testrun
TESTRUN_JOBS=""
CORPUS=auto
CORPUS_TESTS_AT_ONCE=""
CORPUS_JOBS=""
MEMORY=${MITCAD_CHECK_MEMORY:-auto}
OVERLAP=${MITCAD_CHECK_OVERLAP:-auto}
FULL=0
DRY=0
FRESH=()
ONLY=""
# A release gate's passes of the geometry ctests (mitcad#1822).
RELEASE_PASSED=""
RELEASE_TAG=""
ALL_GEOMETRY=0
ARGS=("$@")
while [ $# -gt 0 ]; do
  case $1 in
    --ui-jobs) UI_JOBS=$2; UI_JOBS_GIVEN=1; shift 2 ;;
    --ui-retry-jobs) UI_RETRY_JOBS=$2; shift 2 ;;
    --ui) UI=$2; shift 2 ;;
    --full | --release) FULL=1; shift ;;
    --ctest) CTEST=$2; shift 2 ;;
    --ctest-jobs) CTEST_JOBS=$2; TESTRUN_JOBS=$2; shift 2 ;;
    --testrun) TESTRUN=$2; shift 2 ;;
    --corpus) CORPUS=$2; shift 2 ;;
    --corpus-tests) CORPUS_TESTS_AT_ONCE=$2; shift 2 ;;
    --corpus-jobs) CORPUS_JOBS=$2; shift 2 ;;
    --memory) MEMORY=$2; shift 2 ;;
    --overlap) OVERLAP=$2; shift 2 ;;
    --fresh) FRESH=(--fresh); shift ;;
    --only) ONLY=$2; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    --release-passed) RELEASE_PASSED=$2; shift 2 ;;
    --release-tag) RELEASE_TAG=$2; shift 2 ;;
    --all-geometry) ALL_GEOMETRY=1; shift ;;
    -h | --help) sed -n '3,/^set -uo/{/^set -uo/d;s/^# \{0,1\}//;p}' "$SELF"; exit 0 ;;
    *) echo "unknown option $1 (--help)" >&2; exit 2 ;;
  esac
done
case $CORPUS in auto | always | never) ;; *) echo "--corpus auto, always or never" >&2; exit 2 ;; esac
case $UI in auto | all) ;; *) echo "--ui auto or all" >&2; exit 2 ;; esac
case $CTEST in auto | all) ;; *) echo "--ctest auto or all" >&2; exit 2 ;; esac
case $TESTRUN in auto | off) ;; *) echo "--testrun auto or off" >&2; exit 2 ;; esac
case $OVERLAP in auto | off) ;; *) echo "--overlap auto or off" >&2; exit 2 ;; esac
[[ $UI_RETRY_JOBS =~ ^[1-9][0-9]*$ ]] || { echo "--ui-retry-jobs a number of at least 1" >&2; exit 2; }
if [ "$FULL" = 1 ]; then
  CTEST=all
  CORPUS=always
  UI=all
fi
# The test runner's arguments of a release gate's ctest step.
RELEASE_ARGS=()
if [ -n "$RELEASE_PASSED$RELEASE_TAG" ] || [ "$ALL_GEOMETRY" = 1 ]; then
  [ "$FULL" = 1 ] || { echo "--release-passed, --release-tag and --all-geometry are for --full" >&2; exit 2; }
  [ -n "$RELEASE_PASSED" ] && [ -n "$RELEASE_TAG" ] ||
    { echo "--release-passed DIR and --release-tag X.Y.Z-rcN go together" >&2; exit 2; }
  RELEASE_ARGS=(--release-passed "$RELEASE_PASSED" --release-tag "$RELEASE_TAG")
  [ "$ALL_GEOMETRY" = 0 ] || RELEASE_ARGS+=(--all-geometry)
fi
# A dry run keeps no history, holds no memory scope and takes no slot
# (it still says which memory mode a run would take).
MEMORY_ASKED=$MEMORY
if [ "$DRY" = 1 ]; then
  TIMINGS=$LOGS/dry-run.tsv
  HISTORY=/dev/null
  MEMORY=none
fi

# mib size: a size such as 12G, 1536M or 800K (MemoryMax's, bytes without
# a unit) in MiB.
mib() {
  local n=${1%[KkMmGgTt]}
  case $1 in
    *[Kk]) echo $((n / 1024)) ;;
    *[Mm]) echo "$n" ;;
    *[Gg]) echo $((n * 1024)) ;;
    *[Tt]) echo $((n * 1024 * 1024)) ;;
    *) echo $((n / 1024 / 1024)) ;;
  esac
}

# caller_slice: the slice of the user's systemd this script runs in (the
# last *.slice below user@<uid>.service in its cgroup, e.g. a job slice
# with a memory ceiling of its own), empty outside the user's systemd.
caller_slice() {
  local path part slice="" parts=()
  path=$(sed -n 's/^0:://p' /proc/self/cgroup 2> /dev/null)
  [[ $path == */user@*.service/* ]] || return 0
  IFS=/ read -ra parts <<< "${path#*/user@*.service/}"
  for part in "${parts[@]}"; do
    [[ $part == *.slice ]] && slice=$part
  done
  echo "$slice"
}

# check_slice name bytes: a transient slice of the user's systemd with the
# memory limit (and no swap), through systemd's D-Bus API.
check_slice() {
  busctl --user call org.freedesktop.systemd1 /org/freedesktop/systemd1 org.freedesktop.systemd1.Manager \
    StartTransientUnit 'ssa(sv)a(sa(sv))' "$1" fail 3 MemoryMax t "$2" MemorySwapMax t 0 \
    Description s "tools/check-all.sh in $ROOT" 0 > /dev/null 2>&1
}

# virtual_machine: why this machine is a virtual one, nothing on bare
# metal: systemd-detect-virt --vm, WSL (a container to systemd, a Hyper-V
# guest underneath), or the hypervisor flag in /proc/cpuinfo (seen in a
# chroot or build root too, where systemd-detect-virt may not answer).
virtual_machine() {
  local kind
  kind=$(systemd-detect-virt --vm 2> /dev/null) && [ -n "$kind" ] && [ "$kind" != none ] &&
    { echo "a virtual machine ($kind, systemd-detect-virt)"; return; }
  grep -qiE 'microsoft|wsl' /proc/sys/kernel/osrelease 2> /dev/null &&
    { echo "WSL (/proc/sys/kernel/osrelease)"; return; }
  grep -qw hypervisor /proc/cpuinfo 2> /dev/null &&
    { echo "a virtual machine (the hypervisor flag in /proc/cpuinfo)"; return; }
  return 0
}

# meminfo_mib key: a /proc/meminfo value in MiB (0 when unknown).
meminfo_mib() {
  local kib
  kib=$(sed -n "s/^$1: *\([0-9]*\) kB/\1/p" /proc/meminfo 2> /dev/null)
  echo $((${kib:-0} / 1024))
}

# The memory mode (mitcad#872): a fixed limit (--memory SIZE,
# MITCAD_CHECK_MEMORY, or "auto" in a virtual machine: 12G), or on bare
# metal with "auto" the budget from the free memory (DYNAMIC): the slice's
# limit is the machine's memory less the reserve, and the test runner and
# the corpus tests' children start only while the memory available
# exceeds the reserve by what they are predicted to take. MEMORY_MODE is
# the line the run's first lines print.
DYNAMIC=""
RESERVE_MIB=""
MEMORY_MODE=""
if [ "$MEMORY_ASKED" = auto ] || [ "$MEMORY_ASKED" = dynamic ]; then
  vm=""
  [ "$MEMORY_ASKED" = dynamic ] || vm=$(virtual_machine)
  if [ -n "$vm" ]; then
    [ "$DRY" = 1 ] || MEMORY=12G
    MEMORY_MODE="memory: a fixed 12G, the machine is $vm (the budget from the free memory is for bare metal; --memory or MITCAD_CHECK_MEMORY sets the size)"
  else
    total=$(meminfo_mib MemTotal)
    RESERVE_MIB=$((total / 10))
    [ "$RESERVE_MIB" -ge 4096 ] || RESERVE_MIB=4096
    [ -z "${MITCAD_CHECK_RESERVE:-}" ] || RESERVE_MIB=$(mib "$MITCAD_CHECK_RESERVE")
    DYNAMIC=1
    hard=$((total - RESERVE_MIB))
    [ "$hard" -ge 4096 ] || hard=4096
    [ "$DRY" = 1 ] || MEMORY=${hard}M
    how="bare metal"
    [ "$MEMORY_ASKED" = auto ] || how="dynamic: --memory or MITCAD_CHECK_MEMORY"
    MEMORY_MODE="memory: the budget from the free memory ($how): tests and corpus children start while the memory available exceeds a reserve of ${RESERVE_MIB}M by what they take, read again before each start (available now $(meminfo_mib MemAvailable)M of ${total}M); the slice's limit ${hard}M"
  fi
elif [ "$MEMORY_ASKED" = none ]; then
  MEMORY_MODE="memory: no limit (--memory none)"
else
  MEMORY_MODE="memory: a fixed $MEMORY_ASKED (--memory or MITCAD_CHECK_MEMORY)"
fi
# The test runner's and the corpus children's side of the budget.
RUNNER_MEMORY=()
if [ -n "$DYNAMIC" ]; then
  RUNNER_MEMORY=(--memory-budget free --memory-reserve "${RESERVE_MIB}M")
  export MITCAD_CORPUS_RESERVE=${RESERVE_MIB}M
fi

# The whole run under the memory limit (the script again, inside it), when
# the user's systemd can do it: in a slice of its own with the limit,
# beneath the slice the caller runs in (so that a job slice's ceiling holds
# it too), where the test runner (mitcad-testrun) starts each test's scope
# beside the check, so that the limit covers the tests as well. The slice
# goes when the run ends (or this script is stopped). Without busctl, in a
# scope with the limit in the caller's slice.
if [ "$MEMORY" != none ] && [ -z "${MITCAD_CHECK_SCOPE:-}" ]; then
  if command -v systemd-run > /dev/null && systemd-run --user --scope --quiet true 2> /dev/null; then
    parent=$(caller_slice)
    slice=${parent:-app.slice}
    slice=${slice%.slice}-check$$.slice
    if command -v busctl > /dev/null && check_slice "$slice" $(($(mib "$MEMORY") * 1024 * 1024)); then
      trap 'systemctl --user stop "$slice" 2> /dev/null' EXIT
      trap 'exit 129' HUP
      trap 'exit 130' INT
      trap 'exit 143' TERM
      # In the background, so that a signal reaches the traps at once.
      MITCAD_CHECK_SCOPE=$MEMORY MITCAD_CHECK_SLICE=$slice systemd-run --user --scope --quiet \
        --slice="$slice" -- "$SELF" "${ARGS[@]}" &
      wait $!
      exit
    fi
    MITCAD_CHECK_SCOPE=$MEMORY exec systemd-run --user --scope --quiet ${parent:+--slice="$parent"} \
      -p MemoryMax="$MEMORY" -p MemorySwapMax=0 -- "$SELF" "${ARGS[@]}"
  fi
  echo "note: no systemd scope for the memory limit; running without it"
fi
if [ -n "${MITCAD_CHECK_SLICE:-}" ]; then
  echo "== memory limit $MITCAD_CHECK_SCOPE (systemd slice $MITCAD_CHECK_SLICE, the tests' scopes in it too)"
elif [ -n "${MITCAD_CHECK_SCOPE:-}" ]; then
  echo "== memory limit $MITCAD_CHECK_SCOPE (systemd scope)"
fi
echo "== $MEMORY_MODE"
export CMAKE_BUILD_PARALLEL_LEVEL=${CMAKE_BUILD_PARALLEL_LEVEL:-$(($(nproc) / 2))}
# At most 8 cargo jobs: ctest runs the debug and the release cargo builds
# at once, and at 16 jobs each a fresh build peaked at 12.7G, past the
# check's memory limit (17.10.2026); at 8 each it peaks at 10.7G.
jobs=$(($(nproc) / 2))
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-$((jobs > 8 ? 8 : jobs))}

# The UI tests show no windows here, whatever the caller's environment says.
unset MITCAD_UI_VISIBLE

wanted() {
  if [ "$DRY" = 1 ]; then
    case $1 in configure | build | fmt | clippy | deps | guide | root-compile) return 1 ;; esac
  fi
  [ -z "$ONLY" ] || [[ ",$ONLY," == *",$1,"* ]]
}
# ctest_named: the ctests named in --only (ctest:<name>), one a line.
ctest_named() { tr ',' '\n' <<< "$ONLY" | sed -n 's/^ctest://p'; }
ui_wanted() {
  [ -z "$ONLY" ] || [[ ",$ONLY," == *",ui,"* ]] || [[ ",$ONLY," == *",ui-$1,"* ]]
}
# ui_named name: the UI test is named in --only, so it runs whatever changed.
ui_named() { [[ ",$ONLY," == *",ui-$1,"* ]]; }

now_us() { echo "${EPOCHREALTIME/[.,]/}"; }
# seconds_since start_us: the seconds since then, to a tenth.
seconds_since() {
  local tenths=$((($(now_us) - $1) / 100000))
  # The clock can step back (time synchronisation).
  [ "$tenths" -ge 0 ] || tenths=0
  echo "$((tenths / 10)).$((tenths % 10))"
}

STARTED=$(date '+%Y-%m-%d %H:%M')
RUN_START=$(now_us)
FAILED=0
# Whether this run built: only then are the passes of the corpus and of the
# UI tests kept for their sources.
BUILT=0
mkdir -p "$LOGS"
# Each step's time in the last run that ran it (not skipped), which orders
# the UI tests.
declare -A LAST
if [ -f "$HISTORY" ]; then
  while IFS=$'\t' read -r _ name seconds result; do
    [ "$result" = skipped ] || LAST[$name]=${seconds%.*}
  done < "$HISTORY"
fi
printf 'step\tseconds\tresult\n' > "$TIMINGS"

# record step seconds result: a row of the table; "ok", "ok-again" (a UI
# test that passed when run again alone, mitcad#657) and "skipped" pass.
record() {
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$TIMINGS"
  printf '%s\t%s\t%s\t%s\n' "$STARTED" "$1" "$2" "$3" >> "$HISTORY"
  case $3 in ok | ok-again | skipped | -) ;; *) FAILED=1 ;; esac
}

# step name command...: runs a step, its output in check-logs/<name>.log.
# The commands do not inherit the test lock (descriptor 9): a process left
# behind would hold it after the check.
step() {
  local name=$1 start result
  shift
  start=$(now_us)
  echo "== $name"
  if "$@" > "$LOGS/$name.log" 2>&1 9>&-; then
    result=ok
  else
    result=FAIL
    tail -n 40 "$LOGS/$name.log"
  fi
  record "$name" "$(seconds_since "$start")" "$result"
  [ "$result" = ok ]
}

# ctest_rows log [seconds]: the tests of a ctest log that took 10 s (or
# the seconds given) or more, as rows "ctest:<name>" (not counted in the
# result: the step's row is).
ctest_rows() {
  local least=${2:-10}
  sed -nE 's/^ *[0-9]+\/[0-9]+ +Test +#[0-9]+: +([^ ]+) [ .]*(\**[A-Za-z ]+[a-z]) +([0-9.]+) sec$/\1\t\3\t\2/p' "$1" |
    while IFS=$'\t' read -r name seconds result; do
      if [ "${seconds%.*}" -ge "$least" ]; then
        case $result in Passed) result=ok ;; *Skipped*) result=skipped ;; *) result=${result//\*/} ;; esac
        printf '  ctest:%s\t%s\t%s\n' "$name" "$seconds" "$result" >> "$TIMINGS"
        # In the history too, so that a test's time can be compared between
        # runs; the step's own row says whether it failed.
        printf '%s\tctest:%s\t%s\t%s\n' "$STARTED" "$name" "$seconds" "$result" >> "$HISTORY"
      fi
    done
}

# A test slot, held on descriptor 9 from the first test step to the end:
# the first of the slot files that is free, tried again every 2 s.
LOCKED=0
take_lock() {
  [ "$LOCKED" = 1 ] && return
  local start slot file waiting=0
  start=$(now_us)
  while [ "$LOCKED" = 0 ]; do
    for slot in $(seq 1 "$SLOTS"); do
      file=$LOCK
      [ "$slot" = 1 ] || file=$LOCK.$slot
      exec 9>> "$file"
      if flock -n 9; then
        LOCKED=1
        echo "== test slot $slot ($file)"
        break
      fi
      exec 9>&-
    done
    if [ "$LOCKED" = 0 ]; then
      [ "$waiting" = 1 ] || echo "== waiting for a test slot ($LOCK and $((SLOTS - 1)) more)"
      waiting=1
      sleep 2
    fi
  done
  record "wait for a test slot" "$(seconds_since "$start")" -
}

# corpus_plan: CORPUS_TESTS_AT_ONCE and CORPUS_JOBS unless given. All the
# children together within the run's memory (its scope's limit, else what
# the machine has available) less 1G for ctest, the tests themselves and
# the binaries, and within the cores of a test slot.
corpus_plan() {
  local budget children cores
  if [ -n "$DYNAMIC" ]; then
    # The budget from the free memory: what is available now less the
    # reserve (the children wait for memory as they start, too).
    budget=$(($(meminfo_mib MemAvailable) - RESERVE_MIB))
  elif [ -n "${MITCAD_CHECK_SCOPE:-}" ]; then
    budget=$(mib "$MITCAD_CHECK_SCOPE")
  else
    budget=$(($(sed -n 's/^MemAvailable: *\([0-9]*\) kB/\1/p' /proc/meminfo) / 1024))
  fi
  children=$(((budget - 1024) / $(mib "$CORPUS_MEMORY")))
  cores=$(($(nproc) / SLOTS))
  [ "$children" -le "$cores" ] || children=$cores
  [ "$children" -ge 1 ] || children=1
  if [ -z "$CORPUS_TESTS_AT_ONCE" ]; then
    CORPUS_TESTS_AT_ONCE=1
    while [ $(((CORPUS_TESTS_AT_ONCE + 1) * (CORPUS_TESTS_AT_ONCE + 1))) -le "$children" ]; do
      CORPUS_TESTS_AT_ONCE=$((CORPUS_TESTS_AT_ONCE + 1))
    done
  fi
  if [ -z "$CORPUS_JOBS" ]; then
    CORPUS_JOBS=$((children / CORPUS_TESTS_AT_ONCE))
    [ "$CORPUS_JOBS" -ge 1 ] || CORPUS_JOBS=1
  fi
}

# not_docs: from NUL-separated paths, those that are not Markdown documents:
# neither the corpus nor a UI test reads them, so editing a README or
# commands.md reruns neither. (The MCP server embeds commands.md: the
# ctests' fingerprints count the files a crate includes, core.mcp's that.)
not_docs() { grep -zvE '\.md$'; }

# files_digest path...: an md5sum line for each file under the paths, by
# path (Python's caches and Markdown documents left out).
files_digest() {
  find "$@" -type f -not -path '*/__pycache__/*' -print0 2> /dev/null | not_docs | sort -z | xargs -0 -r md5sum
}

# corpus_dependencies: the resolved dependencies of the import's crates,
# with their versions and features (cargo tree; the checkout's paths left
# out, so that checkouts share the fingerprints), or, when cargo cannot
# tell them offline, Cargo.toml's and Cargo.lock's digests.
corpus_dependencies() {
  local crate selected=()
  for crate in "${CORPUS_CRATES[@]}"; do selected+=(-p "$crate"); done
  cargo tree --offline --locked --manifest-path core/Cargo.toml -e normal,build --prefix none -f '{p} {f}' \
    "${selected[@]}" 2> /dev/null | sed -E 's# \(/[^)]*\)##; s# \(\*\)$##' | sort -u ||
    md5sum core/Cargo.toml core/Cargo.lock
}

corpus_fingerprint() {
  local dir dirs=()
  IFS=: read -ra dirs <<< "${MITCAD_FCSTD_CORPUS:-}:${MITCAD_IPT_CORPUS:-}:${MITCAD_IFC_CORPUS:-}"
  dirs+=("${MITCAD_F3D_CORPUS:-$HOME/f3d-corpus}" "${MITCAD_F3D_MODELS:-$HOME/f3d-models}")
  {
    files_digest "${CORPUS_SOURCES[@]}"
    corpus_dependencies
    for dir in "${dirs[@]}"; do
      [ -d "$dir" ] && find "$dir" -type f -printf '%p %s %T@\n' | sort
    done
  } | md5sum | cut -d' ' -f1
}

# ui_declared name: the areas tools/ui-<name>-test.sh declares it depends
# on ("# check-all sources: <path>..." lines).
ui_declared() { sed -n 's/^# check-all sources: *//p' "tools/ui-$1-test.sh" | tr '\n' ' '; }

# ui_occt_modelling: the OCCT port's patches whose every changed file lies
# in OCCT's modelling toolkits, one path a line: src/ModelingData/ and
# src/ModelingAlgorithms/ but for the meshing (TKMesh) and the hidden lines
# (TKHLR), which the view and the drawings show (mitcad#2136). Such a patch
# changes what the geometry computes, as a change in geometry/ does, so it
# counts like geometry/ (for the smoke set and the tests that declare
# geometry) rather than as the base of every UI test. A patch with any
# other file (the OpenGL driver, TKernel's memory and signals, the data
# exchange) or with none found stays in the base, as do the port's other
# files and the rest of its portfile.
ui_occt_modelling() {
  local patches=("$UI_OCCT_PORT"/*.patch)
  [ -f "${patches[0]}" ] || return 0
  awk '
    function verdict() { if (file != "" && seen && !other) print file }
    FNR == 1 { verdict(); file = FILENAME; seen = 0; other = 0 }
    /^(\+\+\+ b|--- a)\// {
      path = substr($2, 3)
      seen = 1
      if (path !~ /^src\/(ModelingAlgorithms|ModelingData)\// || path ~ /^src\/ModelingAlgorithms\/(TKMesh|TKHLR)\//)
        other = 1
    }
    END { verdict() }' "${patches[@]}"
}

# ui_occt_portfile in|out: the port's portfile, only the lines that name
# a modelling patch (in) or all the others (out); UI_OCCT_MODELLING holds
# the patches.
ui_occt_portfile() {
  [ -f "$UI_OCCT_PORT/portfile.cmake" ] || return 0
  awk -v want="$1" '
    NR == FNR { sub(/.*\//, ""); if ($0 != "") modelling[$0] = 1; next }
    { line = $0; gsub(/^[ \t]+|[ \t]+$/, "", line) }
    (line in modelling) == (want == "in")' <(printf '%s\n' "$UI_OCCT_MODELLING") "$UI_OCCT_PORT/portfile.cmake"
}

# ui_occt_fingerprint: the digest of the modelling patches and of their
# lines in the portfile (their order).
ui_occt_fingerprint() {
  {
    printf '%s\n' "$UI_OCCT_MODELLING" | sed '/^$/d' | xargs -r -d '\n' md5sum
    ui_occt_portfile in
  } | md5sum | cut -d' ' -f1
}

# ui_base_files: from md5sum lines of UI_BASE's files, those of the base:
# not the other platforms' UI test registrations, the modelling patches or
# the portfile (whose other lines ui_base_fingerprint takes).
ui_base_files() {
  awk -v other="$UI_BASE_OTHER_PLATFORMS/" -v portfile="$UI_OCCT_PORT/portfile.cmake" '
    NR == FNR { if ($0 != "") modelling[$0] = 1; next }
    { path = substr($0, 35) }
    index(path, other) == 1 || path == portfile || (path in modelling) { next }
    { print }' <(printf '%s\n' "$UI_OCCT_MODELLING") -
}

# ui_base_fingerprint: the digest of the build files every UI test depends
# on (UI_BASE but for what ui_base_files leaves out, the portfile without
# the modelling patches' lines) and of the build's options (MITCAD_*
# switches, build type).
ui_base_fingerprint() {
  {
    files_digest "${UI_BASE[@]}" | ui_base_files
    ui_occt_portfile out | md5sum
    grep -sE '^(MITCAD_[A-Z0-9_]+:BOOL|CMAKE_BUILD_TYPE:STRING)=' "$BUILD/CMakeCache.txt" | sort
  } | md5sum | cut -d' ' -f1
}

# ui_cli_build: tools/cli/CMakeLists.txt without its test registrations
# (the if(BUILD_TESTING ...) blocks) and its comments: the lines that make
# mitcad-cli.
ui_cli_build() {
  awk '
    skip == 0 && /^if\(BUILD_TESTING/ { skip = 1; next }
    skip > 0 && /^[ \t]*if\(/ { skip++; next }
    skip > 0 && /^[ \t]*endif\(/ { skip--; next }
    skip == 0 && !/^[ \t]*(#|$)/' tools/cli/CMakeLists.txt
}

# ui_cli_fingerprint: the digest of mitcad-cli as the UI tests use it, which
# a declared tools/cli stands for (mitcad#2136): the program's own files
# (tools/cli/ but for the ctests' scripts, *.cmake and *.py, which its
# CMakeLists.txt runs with -P or Python and never builds; all of them when
# the build lines include a file), the build lines (ui_cli_build), and the
# fixtures of tools/cli/tests/ that UI material names: "cli/tests/<name>"
# in tools/ui-* (the tests, their helpers and tools/ui-scripts/), a
# $variable in the name standing for any text and every name taken as a
# prefix. The other fixtures are the ctests' (cli.*), which a new CLI test
# adds; a reference to the folder itself counts all of it.
ui_cli_fingerprint() {
  local files=() refs=() ref path
  mapfile -t files < <(find tools/cli -maxdepth 1 -type f -not -name '*.cmake' -not -name '*.py' | sort)
  if ui_cli_build | grep -q 'include('; then
    files+=(tools/cli/*.cmake)
  fi
  if grep -rqsE 'cli/tests/?([^/A-Za-z0-9_.${}-]|$)' tools/ui-*; then
    files+=(tools/cli/tests)
  else
    mapfile -t refs < <(grep -rhosE 'cli/tests/[A-Za-z0-9_.${}/-]+' tools/ui-* |
      sed -E 's#^cli/tests/##; s/\.+$//; s/\$\{?[A-Za-z0-9_]+\}?/*/g' | sort -u)
    for ref in ${refs[@]+"${refs[@]}"}; do
      for path in tools/cli/tests/$ref*; do
        [ -e "$path" ] && files+=("$path")
      done
    done
  fi
  {
    files_digest ${files[@]+"${files[@]}"}
    ui_cli_build
  } | md5sum | cut -d' ' -f1
}

# ui_declares_geometry name: the test declares geometry/ or a part of it.
ui_declares_geometry() {
  local source sources=()
  read -ra sources <<< "$(ui_declared "$1")"
  for source in ${sources[@]+"${sources[@]}"}; do
    case $source in geometry | geometry/*) return 0 ;; esac
  done
  return 1
}

# ui_layout_fingerprint: the digest of the main window's layout as the
# built application lays it out (tools/ui-layout.sh: the window's size and
# where its bars, docks and view are), which every UI test's clicks and
# screenshots depend on, whatever areas it declares (mitcad#412: a row
# added to the sketch palette made the window taller, and ui-report, which
# declares only app/report, was skipped). Fails when the application could
# not be asked; the line itself is kept as check-logs/ui-layout.txt.
ui_layout_fingerprint() {
  local layout
  layout=$(timeout -k 10 120 bash tools/ui-layout.sh 2> "$LOGS/ui-layout.log" | grep '^Window layout: ') ||
    return 1
  echo "$layout" > "$LOGS/ui-layout.txt"
  md5sum <<< "$layout" | cut -d' ' -f1
}

# ui_smoke name: the UI test is in the smoke set.
ui_smoke() { [[ " ${UI_SMOKE[*]} " == *" $1 "* ]]; }

# ui_test_deps: what each UI test uses of the test library and of the UI test
# driver (mitcad#2173), once, from tools/ui-test-deps.py into
# UI_LIBRARY_DIGEST (the digest of the library's top-level code, the
# functions the test calls and the library files they name) and
# UI_DRIVER_FILES (the driver's core and the area files whose steps the
# test's scripts use); its table is kept as check-logs/ui-test-deps.tsv.
# When the helper fails, both stay empty and the whole library and driver
# count.
declare -A UI_LIBRARY_DIGEST UI_DRIVER_FILES
ui_test_deps() {
  local name digest files
  if python3 tools/ui-test-deps.py plan --root . > "$LOGS/ui-test-deps.tsv" 2> "$LOGS/ui-test-deps.log"; then
    while IFS=$'\t' read -r name digest files; do
      UI_LIBRARY_DIGEST[$name]=$digest
      UI_DRIVER_FILES[$name]=$files
    done < "$LOGS/ui-test-deps.tsv"
  else
    echo "== tools/ui-test-deps.py failed (check-logs/ui-test-deps.log): the whole test library and UI test driver count"
  fi
}

# ui_fingerprint name base common layout: the digest of what the UI test
# depends on: the base's digest, the window's layout, the areas it
# declares (a declared tools/cli as UI_CLI_FINGERPRINT, a declared
# UI_DRIVER as the driver files it uses), its script and what it uses of
# the test library, for the smoke set the digest of what the application is
# built from (common), and for the smoke set and the tests that declare
# geometry the OCCT port's modelling patches (UI_OCCT_FINGERPRINT).
ui_fingerprint() {
  local source sources=() declared=() driver=() cli=0
  read -ra declared <<< "$(ui_declared "$1")"
  for source in ${declared[@]+"${declared[@]}"}; do
    if [ "$source" = tools/cli ]; then
      cli=1
    elif [ "$source" = "$UI_DRIVER" ]; then
      if [ -n "${UI_DRIVER_FILES[$1]:-}" ]; then
        read -ra driver <<< "${UI_DRIVER_FILES[$1]}"
      else
        driver=("${UI_DRIVER_ALL[@]}")
      fi
      sources+=("${driver[@]}")
    else
      sources+=("$source")
    fi
  done
  {
    echo "$2"
    echo "layout $4"
    if ui_smoke "$1"; then echo "smoke $3"; fi
    if ui_smoke "$1" || ui_declares_geometry "$1"; then echo "occt $UI_OCCT_FINGERPRINT"; fi
    if [ "$cli" = 1 ]; then echo "cli $UI_CLI_FINGERPRINT"; fi
    if [ -n "${UI_LIBRARY_DIGEST[$1]:-}" ]; then
      echo "library ${UI_LIBRARY_DIGEST[$1]}"
      files_digest "tools/ui-$1-test.sh" ${sources[@]+"${sources[@]}"}
    else
      files_digest "tools/ui-$1-test.sh" "${UI_LIBRARY[@]}" ${sources[@]+"${sources[@]}"}
    fi
  } | md5sum | cut -d' ' -f1
}

# ctest_plan: which ctests run (tools/ctest-fingerprints.py) into
# check-logs/ctest-plan.tsv, a line per test: name, run or skip,
# fingerprint, reason. Without a plan (the helper failed) every test runs.
ctest_plan() {
  local name args=(--source "$ROOT" --build "$BUILD" --passed "$CTEST_PASSED" --mode "$CTEST"
    --exclude "$CORPUS_TESTS")
  while read -r name; do
    [ -n "$name" ] && args+=(--force "$name")
  done < <(ctest_named)
  wanted ctest || args+=(--forced-only)
  python3 tools/ctest-fingerprints.py plan "${args[@]}" > "$LOGS/ctest-plan.tsv" 2> "$LOGS/ctest-plan.log"
}

# ctest_names file: the tests of the plan that run, one name a line, into
# the file (for mitcad-testrun --tests-from and ctest --tests-from-file):
# one -R expression of every name grows past what ctest's regular
# expressions take (about 1400 names; mitcad#2579). Fails when the plan
# runs no test.
ctest_names() {
  awk -F'\t' '$2 == "run" { print $1 }' "$LOGS/ctest-plan.tsv" > "$1"
  [ -s "$1" ]
}

# testrun_lines results.tsv: mitcad-testrun's results as ctest prints them,
# so that what reads ctest's output reads the runner's too (ctest_rows, the
# passes that tools/ctest-fingerprints.py keeps, the release scripts): the
# output of each test that did not pass (marked "| ", from its log), a
# result line per test, ctest's summary and its list of failed tests.
testrun_lines() {
  local results=$1 logs=$2 name kind log
  awk -F'\t' 'NR > 1 && $2 != "passed" && $2 != "skipped" { print $1 }' "$results" | while read -r name; do
    log=$logs/$(tr -c 'A-Za-z0-9._\n-' '_' <<< "$name")
    # A test rerun after its memory limit ended in its rerun.
    if [ -f "$log.rerun.log" ]; then log=$log.rerun.log; else log=$log.log; fi
    echo "---- $name: output ($log)"
    [ -f "$log" ] && sed 's/^/| /' "$log"
  done
  awk -F'\t' '
    function verdict(kind) {
      if (kind == "passed") return "   Passed"
      if (kind == "skipped") return "***Skipped"
      if (kind == "failed") return "***Failed"
      if (kind == "timeout") return "***Timeout"
      if (kind == "not run") return "***Not Run"
      if (kind == "killed for memory") return "***Killed for memory"
      return "***Environment"
    }
    function status(kind) {
      if (kind == "not run") return "Not Run"
      if (kind == "killed for memory") return "Killed for memory"
      return toupper(substr(kind, 1, 1)) substr(kind, 2)
    }
    NR == 1 { next }
    {
      n++; name[n] = $1; kind[n] = $2
      end = $3 + $4; if (end > total) total = end
      dots = 48 - length($1); if (dots < 3) dots = 3
      printf "%3d/%d Test #%d: %s %s %s %7.2f sec\n", n, rows, n, $1, substr("................................................", 1, dots), verdict($2), $4
      if ($2 != "passed" && $2 != "skipped") failed++
    }
    END {
      if (n == 0) exit
      printf "\n%d%% tests passed, %d tests failed out of %d\n\n", int(100 * (n - failed) / n), failed, n
      printf "Total Test time (real) = %.2f sec\n", total
      if (failed == 0) exit
      print "\nThe following tests FAILED:"
      for (i = 1; i <= n; i++) if (kind[i] != "passed" && kind[i] != "skipped") printf "\t%3d - %s (%s)\n", i, name[i], status(kind[i])
    }' rows="$(($(wc -l < "$results") - 1))" "$results"
}

# changed_files: the paths this checkout changes against origin/main (since
# their merge base, committed or not), one per line; nothing when there is
# no origin/main or no merge base.
changed_files() {
  local base
  base=$(git -C "$ROOT" merge-base origin/main HEAD 2> /dev/null) || return 0
  { git -C "$ROOT" diff --name-only "$base" HEAD; git -C "$ROOT" diff --name-only HEAD; } |
    sort -u
}

# test_step name fallback-jobs ctest-arguments...: the step's ctests
# (ctest's -R/-E selection, and with TESTRUN_NAMES only the tests that
# file names, one a line) through the test runner, mitcad-testrun
# --schedule (tools/testrun: each test in a systemd scope of its own, by
# a memory budget from the check's limit and a CPU gate, with the
# machine's profiles in ~/.mitcad/test-profiles.json), its output in
# check-logs/<name>.log and the runner's report, results and each test's log
# in build/dev/testrun/<name>/; without the runner (not built, --testrun
# off), or when it does not finish (it cannot list the tests, it stops),
# ctest -j <fallback-jobs> in a test slot as before. A runner that did not
# finish fails in a row of its own ("<name>: mitcad-testrun") beside the
# fallback's, so that a step never passes on a runner that failed; with
# TESTRUN_EXPECT (the step has tests to run, mitcad#2579) a run of no test
# fails too (the runner's results empty, ctest's --no-tests=error).
test_step() {
  local name=$1 jobs=$2 out=$BUILD/testrun/$1 start status=0 result changed=() release=()
  local names=() fallback=() rows
  shift 2
  # A release gate's ctest step (TESTRUN_RELEASE): the geometry tests by
  # their release fingerprints (RELEASE_ARGS, mitcad#1822).
  [ -z "${TESTRUN_RELEASE:-}" ] || release=(${RELEASE_ARGS[@]+"${RELEASE_ARGS[@]}"})
  if [ -n "${TESTRUN_NAMES:-}" ]; then
    names=(--tests-from "$TESTRUN_NAMES")
    fallback=(--tests-from-file "$TESTRUN_NAMES")
  fi
  [ -z "${TESTRUN_EXPECT:-}" ] || fallback+=(--no-tests=error)
  if [ "$TESTRUN" != off ] && [ -x "$TESTRUN_BIN" ]; then
    start=$(now_us)
    echo "== $name (mitcad-testrun --schedule; build/dev/testrun/$name)"
    mkdir -p "$out"
    : > "$out/status.txt"
    rm -f "$out/release-skipped.tsv"
    # The change's files (since the merge base with origin/main, and what is
    # not committed yet): their tests go into the runner's priority lane
    # (mitcad#790 step 4), so their failures show first.
    if changed_files > "$out/changed.txt" && [ -s "$out/changed.txt" ]; then
      changed=(--changed "$out/changed.txt")
    fi
    "$TESTRUN_BIN" --build "$BUILD" --schedule --out "$out" ${TESTRUN_JOBS:+-j "$TESTRUN_JOBS"} \
      ${TESTRUN_CORPUS:+--corpus-children} ${RUNNER_MEMORY[@]+"${RUNNER_MEMORY[@]}"} "${changed[@]}" \
      ${release[@]+"${release[@]}"} ${names[@]+"${names[@]}"} "$@" > "$LOGS/$name.log" 2>&1 9>&- || status=$?
    if grep -q '^finished' "$out/status.txt" && [ -f "$out/results.tsv" ]; then
      testrun_lines "$out/results.tsv" "$out/logs" >> "$LOGS/$name.log"
      result=ok
      # A release gate's run: the geometry tests skipped and the passes kept.
      if [ ${#release[@]} -gt 0 ]; then
        grep '^Release fingerprints' "$out/report.txt" 2> /dev/null | sed 's/^/   /'
      fi
      if [ "$status" != 0 ]; then
        result=FAIL
        sed -n 's/^Results: /   mitcad-testrun: /p' "$out/report.txt"
        sed -n '/The following tests FAILED:/,$p' "$LOGS/$name.log"
      fi
      # Tests to run and none ran (those a release gate skipped by their
      # fingerprints count as run).
      rows=$(($(wc -l < "$out/results.tsv") - 1))
      if [ -f "$out/release-skipped.tsv" ]; then
        rows=$((rows + $(awk -F'\t' 'NR > 1 && $1 != ""' "$out/release-skipped.tsv" | wc -l)))
      fi
      if [ -n "${TESTRUN_EXPECT:-}" ] && [ "$rows" -le 0 ]; then
        result="FAIL(no tests ran)"
        echo "   mitcad-testrun ran no test, but the step has tests to run (check-logs/$name.log)"
      fi
      record "$name" "$(seconds_since "$start")" "$result"
      [ "$result" = ok ]
      return
    fi
    cp "$LOGS/$name.log" "$LOGS/$name.testrun.log"
    echo "   mitcad-testrun did not finish (exit $status, check-logs/$name.testrun.log): ctest -j $jobs instead"
    tail -n 5 "$LOGS/$name.testrun.log"
    # The fallback's results show beside it, but the runner's failure is
    # one too: its own row fails the check.
    record "$name: mitcad-testrun" "$(seconds_since "$start")" "FAIL(exit $status)"
    result=FAIL
  fi
  take_lock
  step "$name" ctest --preset dev -j "$jobs" ${fallback[@]+"${fallback[@]}"} "$@" && [ "${result:-ok}" = ok ]
}

# Groups of steps in the background beside the next ones (--overlap auto,
# mitcad#872): the lint steps beside the ctest step, the corpus step
# beside the UI tests, so that the machine is loaded from the start rather
# than one step at a time. A group's table rows and history go into files
# of its own and its output into check-logs/<group>.out, its status (0
# when every step passed) into check-logs/<group>.status; bg_join waits
# for it and adds them to the run's. Only with the test runner, which fits
# its tests into the memory what runs beside leaves (without it the
# corpus holds a test slot of its own).
declare -A BG_PIDS
overlapping() { [ "$OVERLAP" = auto ] && [ "$DRY" = 0 ] && [ "$TESTRUN" != off ] && [ -x "$TESTRUN_BIN" ]; }
# bg_start group function: runs the function in the background.
bg_start() {
  local group=$1
  shift
  : > "$LOGS/$group.rows"
  : > "$LOGS/$group.history"
  : > "$LOGS/$group.status"
  echo "== $group: in the background beside the next steps (check-logs/$group.out)"
  (
    TIMINGS=$LOGS/$group.rows
    HISTORY=$LOGS/$group.history
    FAILED=0
    LOCKED=0
    "$@"
    echo "$FAILED" > "$LOGS/$group.status"
  ) > "$LOGS/$group.out" 2>&1 9>&- &
  BG_PIDS[$group]=$!
}
# bg_join group: waits for the group (if it runs) and adds its output,
# rows and result. Its exit status is in the status file: a `wait -n` of
# the UI step may have reaped the process already.
bg_join() {
  local group=$1 status start pid
  [ -n "${BG_PIDS[$group]:-}" ] || return 0
  start=$(now_us)
  pid=${BG_PIDS[$group]}
  # A `wait -n` of the UI step may have reaped it: then its status file is
  # written and it is gone.
  while kill -0 "$pid" 2> /dev/null; do
    wait "$pid" 2> /dev/null || true
  done
  unset "BG_PIDS[$group]"
  echo "== $group (ran in the background):"
  cat "$LOGS/$group.out"
  cat "$LOGS/$group.rows" >> "$TIMINGS"
  [ "$HISTORY" = /dev/null ] || cat "$LOGS/$group.history" >> "$HISTORY"
  # The time the run waited for it after its own steps (not a step).
  record "wait for $group" "$(seconds_since "$start")" -
  status=$(cat "$LOGS/$group.status" 2> /dev/null)
  [ "$status" = 0 ] || FAILED=1
}

# --- Build and lint
# CMake's file API: the targets the ctests' fingerprints follow.
mkdir -p "$BUILD/.cmake/api/v1/query"
touch "$BUILD/.cmake/api/v1/query/codemodel-v2" "$BUILD/.cmake/api/v1/query/toolchains-v1"
if wanted configure; then
  step configure cmake --preset dev "${FRESH[@]}" || { echo "configure failed"; exit 1; }
fi
if wanted build; then
  step build cmake --build --preset dev || { echo "build failed"; exit 1; }
  BUILT=1
  # The build is to be warning-free (what it compiled, that is). vcpkg's
  # note that a port has many files (the OCCT port's patches) is not
  # about the code; it shows whenever vcpkg hashes the port again.
  warnings=$(grep -E '(^| )warning(\[[^]]*\])?: ' "$LOGS/build.log" |
    grep -vc 'files. Hashing these contents may take a long time')
  if [ "$warnings" -gt 0 ]; then
    grep -E '(^| )warning(\[[^]]*\])?: ' "$LOGS/build.log" |
      grep -v 'files. Hashing these contents may take a long time' | head -20
    record "build warnings" - "FAIL($warnings)"
  fi
fi
# clippy's own target directory, kept with the build (the sync scripts
# remove anything else that is not in the working tree); where the build
# keeps cargo's builds in a persistent folder (MITCAD_CARGO_TARGET_DIR:
# build/dev/cargo links into it, as in the release builds, whose build
# folder starts fresh), beside them there, so that clippy does not check
# every crate from nothing in each release (52 s in 0.33.0's).
CLIPPY_DIR=$BUILD/clippy
if [ -L "$BUILD/cargo" ] && cargo_link=$(readlink -f "$BUILD/cargo"); then
  CLIPPY_DIR=$(dirname "$cargo_link")/clippy
fi
lint() {
  if wanted fmt; then
    step fmt cargo fmt --manifest-path core/Cargo.toml --all --check
  fi
  if wanted clippy; then
    step clippy env CARGO_TARGET_DIR="$CLIPPY_DIR" \
      cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
  fi
  if wanted deps; then
    step deps tools/check-dependencies.sh
  fi
  # The user guide's table of contents against its headings (mitcad#1032):
  # the script's self-test, then the guide.
  if wanted guide; then
    step guide guide_contents
  fi
  if wanted root-compile; then
    root_compile
  fi
}
guide_contents() { python3 tools/check-guide-contents.py --self-test && python3 tools/check-guide-contents.py; }
# root_compile: the C++ files the change touches (with --full every source
# file of geometry/, core/cpp/ and app/) compiled with the Linux release's
# build root's older GCC, -fsyntax-only (tools/check-root-compile.py,
# mitcad#1551): code the host's newer compiler accepts and the release's
# refuses fails here, not in the release. A skipped row, with the reason
# printed, where it cannot tell (no MITCAD_LINUX_BUILD_ROOT, a build
# compiler that is not GCC or not newer than the root's, as inside the
# root itself).
root_compile() {
  local start status result all=()
  start=$(now_us)
  echo "== root-compile"
  [ "$FULL" = 0 ] || all=(--all)
  python3 tools/check-root-compile.py --build "$BUILD" ${all[@]+"${all[@]}"} \
    > "$LOGS/root-compile.log" 2>&1 9>&-
  status=$?
  case $status in
    0) result=ok ;;
    77)
      result=skipped
      echo "   $(tail -n 1 "$LOGS/root-compile.log")"
      ;;
    *)
      result=FAIL
      tail -n 60 "$LOGS/root-compile.log"
      ;;
  esac
  record root-compile "$(seconds_since "$start")" "$result"
}
# Beside the ctest step when it runs (--overlap auto), else before it.
if overlapping && { wanted ctest || [ -n "$(ctest_named)" ]; } &&
  { wanted fmt || wanted clippy || wanted deps || wanted guide || wanted root-compile; }; then
  bg_start lint lint
else
  lint
fi

# --- Tests, under the test lock
if wanted ctest || [ -n "$(ctest_named)" ]; then
  # The tests to run: the plan's, by name through a file (never one -R
  # expression, mitcad#2579); every test without a plan; none.
  ctest_list=$LOGS/ctest-run.txt
  if ctest_plan; then
    ctest_tests=list
    ctest_names "$ctest_list" || ctest_tests=""
    echo "== ctest: $(grep -c $'\trun\t' "$LOGS/ctest-plan.tsv") of $(wc -l < "$LOGS/ctest-plan.tsv") tests" \
      "run, the others are skipped (check-logs/ctest-plan.tsv)"
  else
    echo "== ctest: no plan (check-logs/ctest-plan.log); every test runs"
    : > "$LOGS/ctest-plan.tsv"
    ctest_tests=all
  fi
  if [ "$DRY" = 1 ]; then
    awk -F'\t' '$2 == "run" { print "   would run ctest:" $1 " (" $4 ")" }' "$LOGS/ctest-plan.tsv"
  elif [ -z "$ctest_tests" ]; then
    record ctest 0 skipped
  else
    # The corpus tests outside the corpus group (rvt.corpus, dwg.corpus)
    # import their files in children too: the runner gives them theirs
    # from the files' history (mitcad#875), unless --corpus-jobs fixes them.
    ctest_children=1
    [ -z "$CORPUS_JOBS" ] || ctest_children=""
    ctest_from=""
    [ "$ctest_tests" = all ] || ctest_from=$ctest_list
    TESTRUN_RELEASE=$RELEASE_PASSED TESTRUN_CORPUS=$ctest_children TESTRUN_NAMES=$ctest_from TESTRUN_EXPECT=1 \
      test_step ctest "$CTEST_JOBS" -E "$CORPUS_TESTS"
    ctest_rows "$LOGS/ctest.log"
    # A release gate's geometry tests that passed with the same fingerprint
    # in an earlier release gate (mitcad#1822): skipped rows.
    if [ -f "$BUILD/testrun/ctest/release-skipped.tsv" ]; then
      awk -F'\t' 'NR > 1 && $1 != "" { print $1 }' "$BUILD/testrun/ctest/release-skipped.tsv" |
        while read -r name; do
          printf '  ctest:%s\t0\tskipped\n' "$name" >> "$TIMINGS"
        done
    fi
    if [ "$BUILT" = 1 ]; then
      python3 tools/ctest-fingerprints.py record --plan "$LOGS/ctest-plan.tsv" --log "$LOGS/ctest.log" \
        --passed "$CTEST_PASSED" > /dev/null
    fi
  fi
  # The tests left out (not those a fixture brought in after all).
  awk -F'\t' '$2 == "skip" { print $1 }' "$LOGS/ctest-plan.tsv" | while read -r name; do
    if [ -z "$ctest_tests" ] || ! grep -qE "Test +#[0-9]+: +${name//./\\.} " "$LOGS/ctest.log"; then
      printf '  ctest:%s\t0\tskipped\n' "$name" >> "$TIMINGS"
    fi
  done
fi

# corpus_run: the corpus group's tests (the fingerprint of their sources
# in `fingerprint`, kept when they pass in a run that built).
corpus_run() {
  local tests_given jobs_given corpus_runner corpus_at_once
  [ -n "$fingerprint" ] || fingerprint=$(corpus_fingerprint)
  tests_given=$CORPUS_TESTS_AT_ONCE
  jobs_given=$CORPUS_JOBS
  corpus_plan
  # With the test runner and no --corpus-jobs, the runner plans each
  # corpus test's children from its files' history (mitcad#832,
  # tools/testrun/README.md "Corpus tests"): within the machine's cores
  # and the run's memory budget, a long test more, a short one fewer,
  # each child's memory limit from its test's largest file. The fixed
  # plan above is for the tests without that history, and for ctest -j.
  corpus_runner=""
  corpus_at_once=$CORPUS_TESTS_AT_ONCE
  if [ "$TESTRUN" != off ] && [ -x "$TESTRUN_BIN" ] && [ -z "$jobs_given" ]; then
    corpus_runner=1
    corpus_at_once=$tests_given
    echo "== corpus: children per test from the files' history (mitcad-testrun --corpus-children);" \
      "without one $CORPUS_JOBS files each at once, $CORPUS_MEMORY each${tests_given:+, $tests_given tests at once}"
  else
    echo "== corpus: $CORPUS_TESTS_AT_ONCE tests at once, $CORPUS_JOBS files each at once, $CORPUS_MEMORY each"
  fi
  # The runner takes the tests at once as its most when they are fixed
  # (a test's children are within its own scope's limit).
  if MITCAD_CORPUS_JOBS=$CORPUS_JOBS MITCAD_CORPUS_MEMORY=$CORPUS_MEMORY MITCAD_CORPUS_TIMEOUT=$CORPUS_TIMEOUT \
    TESTRUN_JOBS=$corpus_at_once TESTRUN_CORPUS=$corpus_runner \
    test_step corpus "$CORPUS_TESTS_AT_ONCE" -R "$CORPUS_TESTS" &&
    [ "$BUILT" = 1 ]; then
    mkdir -p "$(dirname "$CORPUS_PASSED")"
    echo "$fingerprint" >> "$CORPUS_PASSED"
  fi
  ctest_rows "$LOGS/corpus.log" 0
}

if wanted corpus; then
  fingerprint=""
  if [ "$CORPUS" = never ]; then
    record corpus 0 skipped
  elif [ "$CORPUS" = auto ] && fingerprint=$(corpus_fingerprint) &&
    grep -qxF "$fingerprint" "$CORPUS_PASSED" 2> /dev/null; then
    echo "== corpus: skipped, its sources and corpora are those of a run that passed"
    record corpus 0 skipped
  elif [ "$DRY" = 1 ]; then
    echo "== corpus: would run"
  elif overlapping; then
    # Beside the UI tests (--overlap auto); joined before they run alone.
    bg_start corpus corpus_run
  else
    corpus_run
  fi
fi

# lint's group ends before the corpus and the UI tests: its output and
# rows follow the ctest step's.
bg_join lint

# run_ui name: one UI test, with its exit status and time in <log>.status.
# The logs of a run that failed (the test's, and the application's that a
# failed test keeps, ui_keep_log) are kept as ui-<name>.failed.log and
# ui-<name>[.<instance>].failed.app.log when the test runs again (mitcad#102).
run_ui() {
  local start status kept
  if [ -f "$LOGS/ui-$1.status" ] && [ "$(cut -d' ' -f1 "$LOGS/ui-$1.status")" != 0 ]; then
    mv -f "$LOGS/ui-$1.log" "$LOGS/ui-$1.failed.log" 2> /dev/null
    for kept in "$LOGS/ui-$1".app.log "$LOGS/ui-$1".*.app.log; do
      case $kept in *.failed.app.log) continue ;; esac
      [ -f "$kept" ] && mv -f "$kept" "${kept%.app.log}.failed.app.log"
    done
  fi
  start=$(now_us)
  MITCAD_UI_LOG_DIR=$LOGS timeout -k 30 "$UI_TIMEOUT" bash "tools/ui-$1-test.sh" > "$LOGS/ui-$1.log" 2>&1 9>&-
  status=$?
  echo "$status $(seconds_since "$start")" > "$LOGS/ui-$1.status"
}

declare -A UI_PIDS
# Each UI test's fingerprint, kept with its passes.
declare -A UI_FINGERPRINTS
UI_RUNNING=0
# The UI tests that failed without crashing among the others, run again
# after them (mitcad#657): tests that count events in a wall-clock
# window (ui-drag-proxy, ui-pick, ui-render, ui-sweep) miss under load.
# They run UI_RETRY_JOBS at once (mitcad#2177), each with fewer other UI
# tests beside it than its failure had (UI_FIRST_BESIDE).
UI_AGAIN=()
declare -A UI_FIRST_BESIDE
UI_RETRYING=0
# How many other UI tests ran beside each one (its most), handed to the test
# runner afterwards so that it learns each test's room (mitcad#790 step 5,
# mitcad#872): a test that fails beside others and passes alone lowers its
# own room, never the machine's UI cap (mitcad#2124).
declare -A UI_BESIDE
# ui_started name: the new test and every running one count each other.
ui_started() {
  local other
  UI_BESIDE[$1]=$((UI_RUNNING - 1))
  for other in "${UI_PIDS[@]}"; do
    [ "${UI_BESIDE[$other]:-0}" -ge $((UI_RUNNING - 1)) ] || UI_BESIDE[$other]=$((UI_RUNNING - 1))
  done
}
# Each UI test's room, from the test runner's profiles (--ui-rooms,
# mitcad#872): the most other UI tests it may have beside it.
declare -A UI_ROOM
# ui_admits name: neither the test nor a running one would have more UI
# tests beside it than its room (the runner's concurrency::admits).
ui_admits() {
  local room other
  room=${UI_ROOM[$1]:-}
  [ -z "$room" ] || [ "$UI_RUNNING" -le "$room" ] || return 1
  for other in ${UI_PIDS[@]+"${UI_PIDS[@]}"}; do
    room=${UI_ROOM[$other]:-}
    [ -z "$room" ] || [ "$UI_RUNNING" -le "$room" ] || return 1
  done
  return 0
}
# ui_run_line name status seconds: one run for the test runner's --record-ui.
ui_run_line() {
  printf 'ui-%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "${UI_BESIDE[$1]:-0}" >> "$LOGS/ui-runs.tsv"
}
# reap_ui: waits for a UI test to end and records it; a pass in a run that
# built is kept for the test's fingerprint. A failure among the others
# that is not a crash is not recorded yet: the test runs again after them
# with fewer beside it (UI_AGAIN), and its second result counts,
# "ok-again" for a pass.
reap_ui() {
  local pid="" name status seconds result=ok crashed=0
  wait -n -p pid
  [ -n "$pid" ] || return
  # Another background job of this script (not a UI test) may end first.
  name=${UI_PIDS[$pid]:-}
  [ -n "$name" ] || return
  unset "UI_PIDS[$pid]"
  UI_RUNNING=$((UI_RUNNING - 1))
  read -r status seconds < "$LOGS/ui-$name.status"
  case $status in
    0) ;;
    124 | 137) result=TIMEOUT ;;
    *) result=FAIL ;;
  esac
  if [ "$result" = FAIL ] && grep -qsE "crashed during|SIGSEGV|SIGABRT|terminate called|Segmentation fault" \
    "$LOGS/ui-$name.log" "$LOGS/ui-$name".*app.log "$LOGS/ui-$name".app.log; then
    crashed=1
  fi
  ui_run_line "$name" "$status" "$seconds"
  if [ "$result" != ok ] && [ "$crashed" = 0 ] && [ "$UI_RETRYING" = 0 ]; then
    echo "   $result ui-$name ($seconds s, ${UI_BESIDE[$name]:-0} beside it): again after the others," \
      "with fewer beside it"
    tail -n 40 "$LOGS/ui-$name.log"
    UI_AGAIN+=("$name")
    UI_FIRST_BESIDE[$name]=${UI_BESIDE[$name]:-0}
    return
  fi
  [ "$result" = ok ] && [ "$UI_RETRYING" = 1 ] && result=ok-again
  echo "   $result ui-$name ($seconds s)"
  [ "$result" = ok ] || [ "$result" = ok-again ] || tail -n 40 "$LOGS/ui-$name.log"
  record "ui-$name" "$seconds" "$result"
  # A crash (the app's crash handler logged its stack) runs the test once
  # more under gdb, for gdb's backtrace with lines; the test stays failed.
  if [ "$crashed" = 1 ]; then
    echo "   ui-$name crashed: once more under gdb (ui-$name.gdb.log)"
    mkdir -p "$LOGS/ui-$name.gdb"
    UI_GDB=1 MITCAD_UI_LOG_DIR=$LOGS/ui-$name.gdb timeout -k 30 "$UI_TIMEOUT" \
      bash "tools/ui-$name-test.sh" > "$LOGS/ui-$name.gdb.log" 2>&1 9>&-
  fi
  if [ "${result%-again}" = ok ] && [ "$BUILT" = 1 ] && [ "$UI_LAYOUT_FINGERPRINT" != unknown ]; then
    mkdir -p "$UI_PASSED"
    echo "${UI_FINGERPRINTS[$name]}" >> "$UI_PASSED/$name"
  fi
}

# ui_round cap name...: runs the UI tests in the order given, at most cap
# at once: the first waiting test that the cap and the rooms (ui_admits)
# admit starts; while none does, the next to end makes room. Returns when
# all have ended.
ui_round() {
  local cap=$1 picked i name
  shift
  local pending=("$@")
  while [ ${#pending[@]} -gt 0 ]; do
    picked=""
    if [ "$UI_RUNNING" -lt "$cap" ]; then
      for i in "${!pending[@]}"; do
        if ui_admits "${pending[$i]}"; then
          picked=$i
          break
        fi
      done
    fi
    if [ -z "$picked" ]; then
      reap_ui
      continue
    fi
    name=${pending[$picked]}
    unset "pending[$picked]"
    pending=(${pending[@]+"${pending[@]}"})
    run_ui "$name" &
    UI_PIDS[$!]=$name
    UI_RUNNING=$((UI_RUNNING + 1))
    ui_started "$name"
  done
  while [ "$UI_RUNNING" -gt 0 ]; do reap_ui; done
}

UI_TESTS=()
UI_SKIPPED=()
UI_BASE_FINGERPRINT=""
UI_COMMON_FINGERPRINT=""
UI_LAYOUT_FINGERPRINT=""
# The OCCT port's modelling patches and the CLI as the UI tests use it, once
# (mitcad#2136); UI_OCCT_MODELLING is read by the base's fingerprint too.
UI_OCCT_MODELLING=""
UI_OCCT_FINGERPRINT=""
UI_CLI_FINGERPRINT=""
for test in tools/ui-*-test.sh; do
  name=${test#tools/ui-}
  name=${name%-test.sh}
  # The macOS UI tests are ctests in the macOS VM (cmake/MacUiTests.cmake).
  case $name in macos*) continue ;; esac
  ui_wanted "$name" || continue
  # The paths a test declares must be there: a declaration left behind by a
  # move would leave the moved files to the common sources unnoticed.
  read -ra sources <<< "$(ui_declared "$name")"
  for source in ${sources[@]+"${sources[@]}"}; do
    if [ ! -e "$source" ]; then
      echo "ui-$name declares $source, which is not there"
      record "ui-$name sources" - "FAIL($source)"
    fi
  done
  if [ -z "$UI_BASE_FINGERPRINT" ]; then
    ui_test_deps
    UI_OCCT_MODELLING=$(ui_occt_modelling)
    UI_OCCT_FINGERPRINT=$(ui_occt_fingerprint)
    UI_CLI_FINGERPRINT=$(ui_cli_fingerprint)
    UI_BASE_FINGERPRINT=$(ui_base_fingerprint)
  fi
  if ui_smoke "$name" && [ -z "$UI_COMMON_FINGERPRINT" ]; then
    UI_COMMON_FINGERPRINT=$(files_digest "${UI_COMMON[@]}" | md5sum | cut -d' ' -f1)
  fi
  # The window's layout, once (an application start); without it no UI
  # test is skipped and no pass is kept.
  if [ -z "$UI_LAYOUT_FINGERPRINT" ]; then
    if UI_LAYOUT_FINGERPRINT=$(ui_layout_fingerprint); then
      echo "== $(cat "$LOGS/ui-layout.txt")"
    else
      UI_LAYOUT_FINGERPRINT=unknown
      echo "== window layout unknown (check-logs/ui-layout.log): no UI test is skipped"
    fi
  fi
  UI_FINGERPRINTS[$name]=$(ui_fingerprint "$name" "$UI_BASE_FINGERPRINT" "$UI_COMMON_FINGERPRINT" \
    "$UI_LAYOUT_FINGERPRINT")
  if [ "$UI" = auto ] && ! ui_named "$name" && [ "$UI_LAYOUT_FINGERPRINT" != unknown ] &&
    grep -qxF "${UI_FINGERPRINTS[$name]}" "$UI_PASSED/$name" 2> /dev/null; then
    UI_SKIPPED+=("$name")
    record "ui-$name" 0 skipped
  else
    UI_TESTS+=("$name")
  fi
done
if [ ${#UI_SKIPPED[@]} -gt 0 ]; then
  echo "== UI tests skipped, their sources are those of a run in which they passed: ${UI_SKIPPED[*]}"
fi
if [ "$DRY" = 1 ]; then
  echo "== UI tests that would run: ${UI_TESTS[*]}"
elif [ ${#UI_TESTS[@]} -gt 0 ]; then
  take_lock
  # The machine's UI cap from the test runner, unless --ui-jobs set the
  # number: the bound UI_JOBS itself (MITCAD_UI_JOBS, else 4), kept for the
  # memory budget only; contention failures lower only the tests' rooms
  # below, never this cap (mitcad#2124).
  rooms=""
  if [ "$UI_JOBS_GIVEN" = 0 ] && [ "$TESTRUN" != off ] && [ -x "$TESTRUN_BIN" ]; then
    UI_JOBS=$("$TESTRUN_BIN" --ui-cap --ui-max "$UI_JOBS" 2> /dev/null || echo "$UI_JOBS")
    # And each test's room, as the runner's scheduler keeps them
    # (mitcad#872): a test that failed beside others waits until few
    # enough run, instead of the machine's cap dropping for all of them.
    while IFS=$'\t' read -r test room; do
      if [[ $test == ui-* ]] && [[ $room =~ ^[0-9]+$ ]]; then
        UI_ROOM[${test#ui-}]=$room
        rooms+=" ${test#ui-} $room,"
      fi
    done < <("$TESTRUN_BIN" --ui-rooms 2> /dev/null)
  fi
  : > "$LOGS/ui-runs.tsv"
  echo "== UI tests, $UI_JOBS at once${rooms:+; rooms (the most other UI tests beside):${rooms%,}}"
  start=$(now_us)
  # Longest first (unknown ones count as long); the compute test at the end.
  mapfile -t PENDING < <(for name in "${UI_TESTS[@]}"; do
    [ "$name" = compute ] || echo "${LAST[ui-$name]:-9999} $name"
  done | sort -rn | cut -d' ' -f2)
  ui_round "$UI_JOBS" ${PENDING[@]+"${PENDING[@]}"}
  # Alone means alone: the corpus group ends first.
  bg_join corpus
  if [[ " ${UI_TESTS[*]} " == *" compute "* ]]; then
    echo "== ui-compute alone (mitcad#8)"
    ui_round 1 compute
  fi
  if [ ${#UI_AGAIN[@]} -gt 0 ]; then
    # Again after the others, UI_RETRY_JOBS at once (mitcad#2177; 1 is the
    # earlier one at a time), each with fewer UI tests beside it than its
    # failure had (0 when that had none): its room for this round, so that
    # a pass still tells the test runner that the failure came from the
    # load (mitcad#657).
    for name in "${UI_AGAIN[@]}"; do
      first=${UI_FIRST_BESIDE[$name]:-0}
      limit=$((first > 0 ? first - 1 : 0))
      room=${UI_ROOM[$name]:-}
      if [ -z "$room" ] || [ "$room" -gt "$limit" ]; then
        UI_ROOM[$name]=$limit
      fi
    done
    mapfile -t again < <(for name in "${UI_AGAIN[@]}"; do
      echo "${LAST[ui-$name]:-9999} $name"
    done | sort -rn | cut -d' ' -f2)
    echo "== UI tests failed among the others, again, $UI_RETRY_JOBS at once, each with fewer beside it:" \
      "${again[*]} (mitcad#657, mitcad#2177)"
    UI_RETRYING=1
    ui_round "$UI_RETRY_JOBS" "${again[@]}"
  fi
  record "ui (all)" "$(seconds_since "$start")" -
  if [ "$TESTRUN" != off ] && [ -x "$TESTRUN_BIN" ] && [ -s "$LOGS/ui-runs.tsv" ]; then
    "$TESTRUN_BIN" --record-ui "$LOGS/ui-runs.tsv" --ui-max "${MITCAD_UI_JOBS:-$UI_JOBS}" \
      > "$LOGS/ui-runs.log" 2>&1 || echo "   the test runner did not take the UI runs (check-logs/ui-runs.log)"
  fi
fi

# The corpus group when no UI test ran beside it.
bg_join corpus

# --- The fuzz targets of the file readers (mitcad#939, core/fuzz), with
# --full only: the synthetic seeds written, then each target for
# FUZZ_SECONDS ($MITCAD_FUZZ_SECONDS, else 60) on them, one after another.
# They need cargo-fuzz and a nightly toolchain; without them the step is
# skipped with a note. A finding fails the target's row
# (check-logs/fuzz:<target>.log); its input is in
# core/fuzz/artifacts/<target>/.
FUZZ_SECONDS=${MITCAD_FUZZ_SECONDS:-60}
fuzz_list() { (cd core && cargo +nightly fuzz list --fuzz-dir fuzz); }
fuzz_build() { (cd core && cargo +nightly fuzz build --fuzz-dir fuzz -O); }
# fuzz_run target: one target for FUZZ_SECONDS.
fuzz_run() {
  (cd core && cargo +nightly fuzz run --fuzz-dir fuzz -O "$1" -- \
    -max_total_time="$FUZZ_SECONDS" -timeout=20 -rss_limit_mb=2048)
}
if [ "$FULL" = 1 ] && wanted fuzz; then
  if [ "$DRY" = 1 ]; then
    echo "   would run every fuzz target of core/fuzz for $FUZZ_SECONDS s"
  elif ! cargo +nightly --version > /dev/null 2>&1 || ! cargo +nightly fuzz --version > /dev/null 2>&1; then
    echo "== fuzz: skipped: cargo-fuzz or the nightly toolchain is not installed" \
      "(docs/development.md, \"Fuzz targets of the file readers\")"
    record fuzz 0 skipped
  elif step fuzz-seeds cargo run -q --manifest-path core/fuzz/Cargo.toml --example write_seeds &&
    step fuzz-build fuzz_build; then
    mapfile -t FUZZ_TARGETS < <(fuzz_list 2> /dev/null)
    echo "== fuzz: ${#FUZZ_TARGETS[@]} targets, $FUZZ_SECONDS s each"
    if [ ${#FUZZ_TARGETS[@]} -eq 0 ]; then
      record "fuzz list" 0 FAIL
    fi
    for target in "${FUZZ_TARGETS[@]}"; do
      step "fuzz:$target" fuzz_run "$target"
    done
  fi
fi

record total "$(seconds_since "$RUN_START")" -
echo
column -t -s $'\t' "$TIMINGS" 2> /dev/null || cat "$TIMINGS"
if [ "$FAILED" = 0 ]; then
  echo "All checks passed."
else
  echo "FAILED: see the table and build/dev/check-logs/."
fi
exit "$FAILED"
