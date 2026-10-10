#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The checks before a commit, in the Linux build environment (run from any
# directory; it works in the checkout it belongs to): configure and build
# with the dev preset, cargo fmt and clippy, the dependency check, ctest,
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
# run's start) and are printed as a table at the end, so that each run
# measures itself; the steps' output is in build/dev/check-logs/. The exit
# status is 0 only if every step passed.
#
# Usage: tools/check-all.sh [--full] [--ui-jobs N] [--ui auto|all]
#          [--ctest auto|all] [--ctest-jobs N]
#          [--corpus auto|always|never] [--corpus-tests N] [--corpus-jobs N]
#          [--memory SIZE] [--fresh] [--only STEP,...]
#   --full          the full check, before a release build: --ctest all,
#                   --corpus always, --ui all (--release is the same).
#   --ui-jobs N     UI tests at once (default 4). The longest ones start
#                   first, by their last times in the history.
#   --ui MODE       "all", or "auto" (default): a UI test runs only when the
#                   files it depends on differ from every run in which it
#                   passed on this machine (kept in
#                   ~/.cache/mitcad/ui-passed/<name>): the areas it
#                   declares in header lines "# check-all sources:
#                   <path>..." (several such lines add up: the parts of
#                   app/ and core/ it exercises, tools/cli when it makes
#                   designs with mitcad-cli, ...), the build files and
#                   options (UI_BASE), its script and the test library
#                   (UI_LIBRARY). The smoke set (UI_SMOKE) depends on
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
#   --ctest-jobs N  ctest's parallel tests (default: the cores / 4).
#   --corpus MODE   the corpus group: f3d.corpus*, f3d.models_loft (the
#                   reference models replayed), freecad.corpus,
#                   ipt.corpus and iam.corpus, after
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
#                   imports at once (MITCAD_CORPUS_JOBS). By default the
#                   children of all of them together are as many as the
#                   memory limit allows, CORPUS_MEMORY (1536M) each
#                   (MITCAD_CORPUS_MEMORY: the memory a child may commit),
#                   with 1G left for the rest, and at most the cores / the
#                   test slots; about the square root of that many tests
#                   run at once. A child runs at most CORPUS_TIMEOUT (300 s,
#                   MITCAD_CORPUS_TIMEOUT).
#   --memory SIZE   the memory the whole run may use (default
#                   $MITCAD_CHECK_MEMORY, else 12G; "none": no limit), in a
#                   systemd scope of its own (MemoryMax, no swap): what
#                   needs more is killed, not the machine's other work.
#   --fresh         configure from scratch (cmake --fresh), as after
#                   changes of CMake files.
#   --only STEPS    only these steps: configure, build, fmt, clippy, deps,
#                   ctest, corpus, ui, or single UI tests (ui-sketch) or
#                   ctests (ctest:core.model).
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
# The test steps (ctest, corpus, UI) hold one of two test slots, files that
# flock(1) locks ($MITCAD_TEST_LOCK, default ~/.mitcad-test.lock, and
# $MITCAD_TEST_LOCK.2; $MITCAD_TEST_SLOTS of them), so that at most two
# checks on one machine test at the same time, each within its memory;
# "flock ~/.mitcad-test.lock <command>" takes the first slot. Building and
# lint take none; unless set, they use half the cores
# (CMAKE_BUILD_PARALLEL_LEVEL, CARGO_BUILD_JOBS). tools/ui-compute-test.sh
# runs alone after the other UI tests: it fails intermittently under load
# (mitcad#8).
set -uo pipefail

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
CORPUS_TESTS='^(f3d\.corpus|f3d\.models_loft$|freecad\.corpus$|ipt\.corpus$|iam\.corpus$)'
CORPUS_SOURCES=(core/f3d core/import core/freecad core/zip core/ipt core/tests
  core/ffi/src/kernel core/ffi/src/*_import.rs core/ffi/src/ipt_history.rs core/ffi/src/exchange.rs
  core/ffi/src/memory.rs core/cpp core/CMakeLists.txt geometry tools/cli/import.cpp tools/cli/import.hpp
  tools/cli/CMakeLists.txt tools/cli/fcstd-corpus.cmake tools/cli/ipt-corpus.cmake tools/cli/iam-corpus.cmake
  CMakeLists.txt rust-toolchain.toml vcpkg.json third_party/vcpkg-ports)
CORPUS_CRATES=(mitcad-import mitcad-f3d mitcad-freecad mitcad-ipt mitcad-zip)
CORPUS_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/corpus-passed
# The build files every UI test depends on, what the application is built
# from (which the smoke set depends on), the smoke set, and the test
# library every UI test uses.
UI_BASE=(third_party CMakeLists.txt CMakePresets.json cmake triplets rust-toolchain.toml vcpkg.json)
UI_COMMON=(app geometry core)
UI_SMOKE=(smoke workflow command file)
UI_LIBRARY=(tools/ui-test-lib.sh tools/ui-image-stats.py tools/xwd2png.py)
UI_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/ui-passed
CTEST_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/ctest-passed
# The memory a corpus test's child may commit, and its time.
CORPUS_MEMORY=${MITCAD_CORPUS_MEMORY:-1536M}
CORPUS_TIMEOUT=${MITCAD_CORPUS_TIMEOUT:-300}

UI_JOBS=4
UI=auto
CTEST=auto
CTEST_JOBS=$(($(nproc) / 4))
[ "$CTEST_JOBS" -ge 1 ] || CTEST_JOBS=1
CORPUS=auto
CORPUS_TESTS_AT_ONCE=""
CORPUS_JOBS=""
MEMORY=${MITCAD_CHECK_MEMORY:-12G}
FULL=0
DRY=0
FRESH=()
ONLY=""
ARGS=("$@")
while [ $# -gt 0 ]; do
  case $1 in
    --ui-jobs) UI_JOBS=$2; shift 2 ;;
    --ui) UI=$2; shift 2 ;;
    --full | --release) FULL=1; shift ;;
    --ctest) CTEST=$2; shift 2 ;;
    --ctest-jobs) CTEST_JOBS=$2; shift 2 ;;
    --corpus) CORPUS=$2; shift 2 ;;
    --corpus-tests) CORPUS_TESTS_AT_ONCE=$2; shift 2 ;;
    --corpus-jobs) CORPUS_JOBS=$2; shift 2 ;;
    --memory) MEMORY=$2; shift 2 ;;
    --fresh) FRESH=(--fresh); shift ;;
    --only) ONLY=$2; shift 2 ;;
    --dry-run) DRY=1; shift ;;
    -h | --help) sed -n '3,/^set -uo/{/^set -uo/d;s/^# \{0,1\}//;p}' "$SELF"; exit 0 ;;
    *) echo "unknown option $1 (--help)" >&2; exit 2 ;;
  esac
done
case $CORPUS in auto | always | never) ;; *) echo "--corpus auto, always or never" >&2; exit 2 ;; esac
case $UI in auto | all) ;; *) echo "--ui auto or all" >&2; exit 2 ;; esac
case $CTEST in auto | all) ;; *) echo "--ctest auto or all" >&2; exit 2 ;; esac
if [ "$FULL" = 1 ]; then
  CTEST=all
  CORPUS=always
  UI=all
fi
# A dry run keeps no history, holds no memory scope and takes no slot.
if [ "$DRY" = 1 ]; then
  TIMINGS=$LOGS/dry-run.tsv
  HISTORY=/dev/null
  MEMORY=none
fi

# The whole run in a systemd scope with the memory limit (the script again,
# inside it), when the user's systemd can make one.
if [ "$MEMORY" != none ] && [ -z "${MITCAD_CHECK_SCOPE:-}" ]; then
  if command -v systemd-run > /dev/null && systemd-run --user --scope --quiet true 2> /dev/null; then
    MITCAD_CHECK_SCOPE=$MEMORY exec systemd-run --user --scope --quiet \
      -p MemoryMax="$MEMORY" -p MemorySwapMax=0 -- "$SELF" "${ARGS[@]}"
  fi
  echo "note: no systemd scope for the memory limit; running without it"
fi
[ -n "${MITCAD_CHECK_SCOPE:-}" ] && echo "== memory limit $MITCAD_CHECK_SCOPE (systemd scope)"
export CMAKE_BUILD_PARALLEL_LEVEL=${CMAKE_BUILD_PARALLEL_LEVEL:-$(($(nproc) / 2))}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-$(($(nproc) / 2))}

# The UI tests show no windows here, whatever the caller's environment says.
unset MITCAD_UI_VISIBLE

wanted() {
  if [ "$DRY" = 1 ]; then
    case $1 in configure | build | fmt | clippy | deps) return 1 ;; esac
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

# record step seconds result: a row of the table; "ok" and "skipped" pass.
record() {
  printf '%s\t%s\t%s\n' "$1" "$2" "$3" >> "$TIMINGS"
  printf '%s\t%s\t%s\t%s\n' "$STARTED" "$1" "$2" "$3" >> "$HISTORY"
  case $3 in ok | skipped | -) ;; *) FAILED=1 ;; esac
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

# corpus_plan: CORPUS_TESTS_AT_ONCE and CORPUS_JOBS unless given. All the
# children together within the run's memory (its scope's limit, else what
# the machine has available) less 1G for ctest, the tests themselves and
# the binaries, and within the cores of a test slot.
corpus_plan() {
  local budget children cores
  if [ -n "${MITCAD_CHECK_SCOPE:-}" ]; then
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
  IFS=: read -ra dirs <<< "${MITCAD_FCSTD_CORPUS:-}:${MITCAD_IPT_CORPUS:-}"
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

# ui_base_fingerprint: the digest of the build files every UI test depends
# on (UI_BASE) and of the build's options (MITCAD_RENDER, ...).
ui_base_fingerprint() {
  {
    files_digest "${UI_BASE[@]}"
    grep -sE '^(MITCAD_[A-Z0-9_]+:BOOL|CMAKE_BUILD_TYPE:STRING)=' "$BUILD/CMakeCache.txt" | sort
  } | md5sum | cut -d' ' -f1
}

# ui_smoke name: the UI test is in the smoke set.
ui_smoke() { [[ " ${UI_SMOKE[*]} " == *" $1 "* ]]; }

# ui_fingerprint name base common: the digest of what the UI test depends
# on: the base's digest, the areas it declares, its script and the test
# library, and for the smoke set the digest of what the application is
# built from (common).
ui_fingerprint() {
  local sources=()
  read -ra sources <<< "$(ui_declared "$1")"
  {
    echo "$2"
    if ui_smoke "$1"; then echo "smoke $3"; fi
    files_digest "tools/ui-$1-test.sh" "${UI_LIBRARY[@]}" ${sources[@]+"${sources[@]}"}
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

# ctest_regex: the tests of the plan that run, as a regular expression.
ctest_regex() {
  awk -F'\t' '$2 == "run" { print $1 }' "$LOGS/ctest-plan.tsv" | sed 's/[.[\*^$()+?{|]/\\&/g' |
    paste -sd '|' | sed 's/^\(..*\)$/^(\1)$/'
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
  # The build is to be warning-free (what it compiled, that is).
  warnings=$(grep -cE '(^| )warning(\[[^]]*\])?: ' "$LOGS/build.log")
  if [ "$warnings" -gt 0 ]; then
    grep -E '(^| )warning(\[[^]]*\])?: ' "$LOGS/build.log" | head -20
    record "build warnings" - "FAIL($warnings)"
  fi
fi
if wanted fmt; then
  step fmt cargo fmt --manifest-path core/Cargo.toml --all --check
fi
if wanted clippy; then
  # Its own target directory, kept with the build (the sync scripts remove
  # anything else that is not in the working tree).
  step clippy env CARGO_TARGET_DIR="$BUILD/clippy" \
    cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
fi
if wanted deps; then
  step deps tools/check-dependencies.sh
fi

# --- Tests, under the test lock
if wanted ctest || [ -n "$(ctest_named)" ]; then
  if ctest_plan; then
    regex=$(ctest_regex)
    echo "== ctest: $(grep -c $'\trun\t' "$LOGS/ctest-plan.tsv") of $(wc -l < "$LOGS/ctest-plan.tsv") tests" \
      "run, the others are skipped (check-logs/ctest-plan.tsv)"
  else
    echo "== ctest: no plan (check-logs/ctest-plan.log); every test runs"
    : > "$LOGS/ctest-plan.tsv"
    regex=.
  fi
  if [ "$DRY" = 1 ]; then
    awk -F'\t' '$2 == "run" { print "   would run ctest:" $1 " (" $4 ")" }' "$LOGS/ctest-plan.tsv"
  elif [ -z "$regex" ]; then
    record ctest 0 skipped
  else
    take_lock
    step ctest ctest --preset dev -j "$CTEST_JOBS" -R "$regex" -E "$CORPUS_TESTS"
    ctest_rows "$LOGS/ctest.log"
    if [ "$BUILT" = 1 ]; then
      python3 tools/ctest-fingerprints.py record --plan "$LOGS/ctest-plan.tsv" --log "$LOGS/ctest.log" \
        --passed "$CTEST_PASSED" > /dev/null
    fi
  fi
  # The tests left out (not those a fixture brought in after all).
  awk -F'\t' '$2 == "skip" { print $1 }' "$LOGS/ctest-plan.tsv" | while read -r name; do
    if [ -z "$regex" ] || ! grep -qE "Test +#[0-9]+: +${name//./\\.} " "$LOGS/ctest.log"; then
      printf '  ctest:%s\t0\tskipped\n' "$name" >> "$TIMINGS"
    fi
  done
fi

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
  else
    take_lock
    [ -n "$fingerprint" ] || fingerprint=$(corpus_fingerprint)
    corpus_plan
    echo "== corpus: $CORPUS_TESTS_AT_ONCE tests at once, $CORPUS_JOBS files each at once, $CORPUS_MEMORY each"
    if step corpus env MITCAD_CORPUS_JOBS="$CORPUS_JOBS" MITCAD_CORPUS_MEMORY="$CORPUS_MEMORY" \
      MITCAD_CORPUS_TIMEOUT="$CORPUS_TIMEOUT" ctest --preset dev -j "$CORPUS_TESTS_AT_ONCE" -R "$CORPUS_TESTS" &&
      [ "$BUILT" = 1 ]; then
      mkdir -p "$(dirname "$CORPUS_PASSED")"
      echo "$fingerprint" >> "$CORPUS_PASSED"
    fi
    ctest_rows "$LOGS/corpus.log" 0
  fi
fi

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
# reap_ui: waits for a UI test to end and records it; a pass in a run that
# built is kept for the test's fingerprint.
reap_ui() {
  local pid="" name status seconds result=ok
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
  echo "   $result ui-$name ($seconds s)"
  [ "$result" = ok ] || tail -n 40 "$LOGS/ui-$name.log"
  record "ui-$name" "$seconds" "$result"
  if [ "$result" = ok ] && [ "$BUILT" = 1 ]; then
    mkdir -p "$UI_PASSED"
    echo "${UI_FINGERPRINTS[$name]}" >> "$UI_PASSED/$name"
  fi
}

UI_TESTS=()
UI_SKIPPED=()
UI_BASE_FINGERPRINT=""
UI_COMMON_FINGERPRINT=""
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
  [ -n "$UI_BASE_FINGERPRINT" ] || UI_BASE_FINGERPRINT=$(ui_base_fingerprint)
  if ui_smoke "$name" && [ -z "$UI_COMMON_FINGERPRINT" ]; then
    UI_COMMON_FINGERPRINT=$(files_digest "${UI_COMMON[@]}" | md5sum | cut -d' ' -f1)
  fi
  UI_FINGERPRINTS[$name]=$(ui_fingerprint "$name" "$UI_BASE_FINGERPRINT" "$UI_COMMON_FINGERPRINT")
  if [ "$UI" = auto ] && ! ui_named "$name" &&
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
  echo "== UI tests, $UI_JOBS at once"
  start=$(now_us)
  # Longest first (unknown ones count as long); the compute test at the end.
  mapfile -t ORDER < <(for name in "${UI_TESTS[@]}"; do
    [ "$name" = compute ] || echo "${LAST[ui-$name]:-9999} $name"
  done | sort -rn | cut -d' ' -f2)
  for name in "${ORDER[@]}"; do
    while [ "$UI_RUNNING" -ge "$UI_JOBS" ]; do reap_ui; done
    run_ui "$name" &
    UI_PIDS[$!]=$name
    UI_RUNNING=$((UI_RUNNING + 1))
  done
  while [ "$UI_RUNNING" -gt 0 ]; do reap_ui; done
  if [[ " ${UI_TESTS[*]} " == *" compute "* ]]; then
    echo "== ui-compute alone (mitcad#8)"
    run_ui compute &
    UI_PIDS[$!]=compute
    UI_RUNNING=1
    reap_ui
  fi
  record "ui (all)" "$(seconds_since "$start")" -
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
