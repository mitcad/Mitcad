#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The checks before a commit, in the Linux build environment (run from any
# directory; it works in the checkout it belongs to): configure and build
# with the dev preset, cargo fmt and clippy, the dependency check, ctest,
# the corpus tests when they are relevant, and every UI test
# (tools/ui-*-test.sh), several at once.
#
# Every step's time and result go to build/dev/check-timings.tsv (step,
# seconds, result; also appended to check-timings-history.tsv with the
# run's start) and are printed as a table at the end, so that each run
# measures itself; the steps' output is in build/dev/check-logs/. The exit
# status is 0 only if every step passed.
#
# Usage: tools/check-all.sh [--ui-jobs N] [--ctest-jobs N]
#          [--corpus auto|always|never] [--memory SIZE] [--fresh]
#          [--only STEP,...]
#   --ui-jobs N     UI tests at once (default 4). The longest ones start
#                   first, by their last times in the history.
#   --ctest-jobs N  ctest's parallel tests (default: the cores / 4).
#   --corpus MODE   the corpus group: f3d.corpus*, f3d.models_loft (the
#                   reference models replayed) and freecad.corpus, run one
#                   at a time after ctest's other tests. "always", "never",
#                   or "auto" (default): only when the files they depend on
#                   (CORPUS_SOURCES below: the import, the readers, the
#                   geometry, the bridge and how they are built) or the
#                   corpora differ from every corpus run that passed on
#                   this machine, which are kept in
#                   ~/.cache/mitcad/corpus-passed.
#   --memory SIZE   the memory the whole run may use (default
#                   $MITCAD_CHECK_MEMORY, else 12G; "none": no limit), in a
#                   systemd scope of its own (MemoryMax, no swap): what
#                   needs more is killed, not the machine's other work.
#   --fresh         configure from scratch (cmake --fresh), as after
#                   changes of CMake files.
#   --only STEPS    only these steps: configure, build, fmt, clippy, deps,
#                   ctest, corpus, ui, or single UI tests (ui-sketch).
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
# The corpus group's tests, and what they depend on besides the corpora.
CORPUS_TESTS='^(f3d\.corpus|f3d\.models_loft$|freecad\.corpus$)'
CORPUS_SOURCES=(core/f3d core/import core/freecad core/zip core/ffi core/cpp core/tests
  core/CMakeLists.txt core/Cargo.toml core/Cargo.lock geometry tools/cli/main.cpp
  tools/cli/CMakeLists.txt tools/cli/fcstd-corpus.cmake CMakeLists.txt rust-toolchain.toml vcpkg.json)
CORPUS_PASSED=${XDG_CACHE_HOME:-$HOME/.cache}/mitcad/corpus-passed

UI_JOBS=4
CTEST_JOBS=$(($(nproc) / 4))
[ "$CTEST_JOBS" -ge 1 ] || CTEST_JOBS=1
CORPUS=auto
MEMORY=${MITCAD_CHECK_MEMORY:-12G}
FRESH=()
ONLY=""
ARGS=("$@")
while [ $# -gt 0 ]; do
  case $1 in
    --ui-jobs) UI_JOBS=$2; shift 2 ;;
    --ctest-jobs) CTEST_JOBS=$2; shift 2 ;;
    --corpus) CORPUS=$2; shift 2 ;;
    --memory) MEMORY=$2; shift 2 ;;
    --fresh) FRESH=(--fresh); shift ;;
    --only) ONLY=$2; shift 2 ;;
    -h | --help) sed -n '3,/^set -uo/{/^set -uo/d;s/^# \{0,1\}//;p}' "$SELF"; exit 0 ;;
    *) echo "unknown option $1 (--help)" >&2; exit 2 ;;
  esac
done
case $CORPUS in auto | always | never) ;; *) echo "--corpus auto, always or never" >&2; exit 2 ;; esac

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

wanted() { [ -z "$ONLY" ] || [[ ",$ONLY," == *",$1,"* ]]; }
ui_wanted() {
  [ -z "$ONLY" ] || [[ ",$ONLY," == *",ui,"* ]] || [[ ",$ONLY," == *",ui-$1,"* ]]
}

now_us() { echo "${EPOCHREALTIME/[.,]/}"; }
# seconds_since start_us: the seconds since then, to a tenth.
seconds_since() {
  local tenths=$((($(now_us) - $1) / 100000))
  echo "$((tenths / 10)).$((tenths % 10))"
}

STARTED=$(date '+%Y-%m-%d %H:%M')
RUN_START=$(now_us)
FAILED=0
mkdir -p "$LOGS"
# Each step's time in the last run that had it, which orders the UI tests.
declare -A LAST
if [ -f "$HISTORY" ]; then
  while IFS=$'\t' read -r _ name seconds _; do LAST[$name]=${seconds%.*}; done < "$HISTORY"
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

# ctest_rows log: the tests of a ctest log that took 10 s or more, as rows
# "ctest:<name>" (not counted in the result: the step's row is).
ctest_rows() {
  sed -nE 's/^ *[0-9]+\/[0-9]+ +Test +#[0-9]+: +([^ ]+) [ .]*(\**[A-Za-z ]+[a-z]) +([0-9.]+) sec$/\1\t\3\t\2/p' "$1" |
    while IFS=$'\t' read -r name seconds result; do
      if [ "${seconds%.*}" -ge 10 ]; then
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

corpus_fingerprint() {
  local dir dirs=()
  IFS=: read -ra dirs <<< "${MITCAD_FCSTD_CORPUS:-}"
  dirs+=("${MITCAD_F3D_CORPUS:-$HOME/f3d-corpus}" "${MITCAD_F3D_MODELS:-$HOME/f3d-models}")
  {
    find "${CORPUS_SOURCES[@]}" -type f -print0 2> /dev/null | sort -z | xargs -0 md5sum
    for dir in "${dirs[@]}"; do
      [ -d "$dir" ] && find "$dir" -type f -printf '%p %s %T@\n' | sort
    done
  } | md5sum | cut -d' ' -f1
}

# --- Build and lint
if wanted configure; then
  step configure cmake --preset dev "${FRESH[@]}" || { echo "configure failed"; exit 1; }
fi
if wanted build; then
  step build cmake --build --preset dev || { echo "build failed"; exit 1; }
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
if wanted ctest; then
  take_lock
  step ctest ctest --preset dev -j "$CTEST_JOBS" -E "$CORPUS_TESTS"
  ctest_rows "$LOGS/ctest.log"
fi

if wanted corpus; then
  fingerprint=""
  if [ "$CORPUS" = never ]; then
    record corpus 0 skipped
  elif [ "$CORPUS" = auto ] && fingerprint=$(corpus_fingerprint) &&
    grep -qxF "$fingerprint" "$CORPUS_PASSED" 2> /dev/null; then
    echo "== corpus: skipped, its sources and corpora are those of a run that passed"
    record corpus 0 skipped
  else
    take_lock
    [ -n "$fingerprint" ] || fingerprint=$(corpus_fingerprint)
    if step corpus ctest --preset dev -j 1 -R "$CORPUS_TESTS"; then
      mkdir -p "$(dirname "$CORPUS_PASSED")"
      echo "$fingerprint" >> "$CORPUS_PASSED"
    fi
    ctest_rows "$LOGS/corpus.log"
  fi
fi

# run_ui name: one UI test, with its exit status and time in <log>.status.
run_ui() {
  local start status
  start=$(now_us)
  timeout -k 30 "$UI_TIMEOUT" bash "tools/ui-$1-test.sh" > "$LOGS/ui-$1.log" 2>&1 9>&-
  status=$?
  echo "$status $(seconds_since "$start")" > "$LOGS/ui-$1.status"
}

declare -A UI_PIDS
UI_RUNNING=0
# reap_ui: waits for a UI test to end and records it.
reap_ui() {
  local pid="" name status seconds result=ok
  wait -n -p pid
  [ -n "$pid" ] || return
  name=${UI_PIDS[$pid]}
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
}

UI_TESTS=()
for test in tools/ui-*-test.sh; do
  name=${test#tools/ui-}
  name=${name%-test.sh}
  # The macOS UI tests are ctests in the macOS VM (cmake/MacUiTests.cmake).
  case $name in macos*) continue ;; esac
  ui_wanted "$name" && UI_TESTS+=("$name")
done
if [ ${#UI_TESTS[@]} -gt 0 ]; then
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
