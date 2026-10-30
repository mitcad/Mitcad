# SPDX-License-Identifier: MIT
# Shared helpers for the macOS UI tests (tools/ui-macos-*.sh). Source this
# file; it is not a script. Compatible with the bash 3.2 and the BSD
# userland that macOS ships: no associative arrays, no ${var,,}, no mapfile,
# no GNU timeout, no GNU-only flags.
#
# The tests run the application's own binary (mitcad.app/Contents/MacOS/
# mitcad) with --screenshot: the app renders into an offscreen framebuffer
# and saves it, so nothing is clicked and no screen recording permission is
# needed. The window itself still opens, so a window server must be there: a
# logged-in user's session (Terminal, or SSH into a Mac with a user logged in
# at the console); see ui_gui_unavailable.
#
# Everything the app writes of its own goes to a private temporary
# directory (UI_ROOT: settings, recovery folder, projects folder, thumbnails,
# result store off), removed on exit, so the user's ~/Library/Preferences and
# Application Support are never read or changed. AppKit window restoration is
# disabled for each test launch; a previous crash cannot show a restore alert.
#
# Usage: ui_init <app> <out directory>, then ui_run_app <name> <arguments>...
# runs the app with its log in <out>/<name>.log. Variables set in front of
# the call (MITCAD_RESULT_STORE=dir ui_run_app ...) reach the app.

set -u

UI_APP=""
UI_OUT=""
UI_ROOT=""
UI_PID=""
UI_EXIT=0
UI_SIGNAL=""
UI_TIMED_OUT=0
UI_ASSERTION_FAILED=0
UI_LOG_FILE=""
UI_FAILURES=0
# FD 9 is reserved for the per-user GUI lock. The shell and the actual
# application share its open file description until the app has exited.
UI_GUI_LOCK_FD_OPEN=0
UI_GUI_LOCK_HELD=0
UI_GUI_LOCK_WAITER=""
UI_STATS=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ui-image-stats.py
# Seconds the app may take for one run (a screenshot run exits after 2 s
# plus the start-up and the recompute; the first start can be slow).
UI_TIMEOUT=${UI_TIMEOUT:-120}

ui_pass() { echo "ok   $1"; }
ui_fail() { echo "FAIL: $1"; UI_FAILURES=$((UI_FAILURES + 1)); }

# ui_stop_app: ends the app of the current run, and only it. Its children
# (an import worker) go first.
ui_stop_app() {
  if [ -n "$UI_PID" ] && kill -0 "$UI_PID" 2> /dev/null; then
    pkill -TERM -P "$UI_PID" 2> /dev/null || true
    kill -TERM "$UI_PID" 2> /dev/null || true
    local i=0
    while [ "$i" -lt 30 ] && kill -0 "$UI_PID" 2> /dev/null; do
      sleep 0.1
      i=$((i + 1))
    done
    if kill -0 "$UI_PID" 2> /dev/null; then
      pkill -KILL -P "$UI_PID" 2> /dev/null || true
      kill -KILL "$UI_PID" 2> /dev/null || true
    fi
  fi
}

# Kernel flock ownership survives the short Python helper: its inherited
# descriptor and this shell's FD 9 refer to the same open file description.
# Never unlink the file or unlock in Python; either would let another
# checkout enter while the real application still owns the GUI.
ui_gui_lock() {
  if [ "$UI_GUI_LOCK_FD_OPEN" = 1 ]; then
    ui_fail "a macOS UI run already owns or awaits the GUI lock"
    return 1
  fi
  mkdir -p "$HOME/.mitcad" || return 1
  exec 9> "$HOME/.mitcad/macos-ui.lock" || return 1
  UI_GUI_LOCK_FD_OPEN=1
  echo "Waiting for the shared macOS UI lock"
  python3 -c 'import fcntl; fcntl.flock(9, fcntl.LOCK_EX)' 9>&9 &
  UI_GUI_LOCK_WAITER=$!
  local result=0
  # A background helper plus wait keeps INT/TERM traps responsive while
  # another checkout's app is running. No timeout belongs to this queue.
  wait "$UI_GUI_LOCK_WAITER" || result=$?
  UI_GUI_LOCK_WAITER=""
  if [ "$result" -ne 0 ]; then
    ui_gui_unlock
    ui_fail "could not acquire the shared macOS UI lock (exit $result)"
    return 1
  fi
  UI_GUI_LOCK_HELD=1
}

ui_gui_unlock() {
  if [ -n "$UI_GUI_LOCK_WAITER" ]; then
    kill -TERM "$UI_GUI_LOCK_WAITER" 2> /dev/null || true
    wait "$UI_GUI_LOCK_WAITER" 2> /dev/null || true
    UI_GUI_LOCK_WAITER=""
  fi
  if [ "$UI_GUI_LOCK_FD_OPEN" = 1 ]; then
    exec 9>&-
    UI_GUI_LOCK_FD_OPEN=0
  fi
  UI_GUI_LOCK_HELD=0
}

ui_cleanup() {
  ui_stop_app
  if [ -n "$UI_PID" ]; then
    wait "$UI_PID" 2> /dev/null || true
    UI_PID=""
  fi
  ui_gui_unlock
  [ -n "$UI_ROOT" ] && rm -rf "$UI_ROOT"
}

# ui_init app out: checks the app, makes the output directory and the
# private data root, and exports the environment of the tests.
ui_init() {
  if [ "$(uname -s)" != Darwin ]; then
    echo "the macOS UI tests run on macOS only" >&2
    exit 2
  fi
  if [ ! -x "$1" ]; then
    echo "FAIL: $1 is not an executable; pass .../mitcad.app/Contents/MacOS/mitcad" >&2
    exit 2
  fi
  if ! command -v python3 > /dev/null; then
    echo "FAIL: python3 is required (xcode-select --install, or brew install python)" >&2
    exit 2
  fi
  UI_APP=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
  mkdir -p "$2" || exit 2
  UI_OUT=$(cd "$2" && pwd)
  UI_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui.XXXXXX") || exit 2
  trap ui_cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  # The app's own files in the root, not the user's ~/Library: Qt would use
  # a plist of the user defaults for QSettings otherwise.
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings
  export MITCAD_AUTOSAVE_DIR=$UI_ROOT/autosave
  export MITCAD_PROJECTS_DIR=$UI_ROOT/projects
  export MITCAD_PRINT_LIBRARY_DIR=$UI_ROOT/print-library
  export MITCAD_THUMBNAIL_DIR=$UI_ROOT/thumbnails
  # Crash reports of the run's own (mitcad#62): none offered from the user's.
  export MITCAD_CRASH_DIR=$UI_ROOT/crashes
  # Local MCP discovery records and tokens stay with this run's data too.
  export MITCAD_MCP_DISCOVERY_DIR=$UI_ROOT/mcp
  mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_AUTOSAVE_DIR" "$MITCAD_PROJECTS_DIR" "$MITCAD_THUMBNAIL_DIR"
  # The store of computed results (P7d) is off unless a run turns it on
  # (MITCAD_RESULT_STORE=<directory> in front of ui_run_app).
  export MITCAD_RESULT_STORE=off
  unset MITCAD_RESULT_STORE_MIN_MS
  # Qt on macOS logs to the unified log, not stderr, unless told to.
  export QT_FORCE_STDERR_LOGGING=1
  # Where faces and edges can be clicked, and the bodies' volumes.
  export MITCAD_LOG_PICKS=1
  export MITCAD_LOG_VOLUMES=1
  # What typed-in text fields hold ("File dialog Save As: <path>").
  export MITCAD_LOG_TYPED_TEXT=1
  # An update banner can move the view between a logged pick and its click.
  export MITCAD_NO_UPDATE_CHECK=1
  # The first dimension of a drawn sketch scales nothing (mitcad#860), as in
  # ui-test-lib.sh.
  export MITCAD_SKETCH_FIRST_DIMENSION_SCALE=0
  # No question whether to activate a picked face's component (mitcad#960),
  # as in ui-test-lib.sh.
  export MITCAD_ASK_ACTIVATE_COMPONENT=0
  # Git's configuration of our own (empty), as in ui-test-lib.sh.
  : > "$UI_ROOT/gitconfig"
  export GIT_CONFIG_GLOBAL=$UI_ROOT/gitconfig
  export GIT_CONFIG_NOSYSTEM=1
  unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
}

# ui_gui_unavailable: the last run's log says that the app could not reach
# a window server or create an OpenGL context: a session without a
# desktop (a daemon, an SSH login without a user logged in at the
# console, a CI runner without a display). Not the app's fault.
ui_gui_unavailable() {
  [ "$UI_EXIT" -ne 0 ] || return 1
  grep -Eiq 'could not connect to (the )?(display|window ?server)|connection to the window server|WindowServer|CGSConnection|CGSInitialize|_RegisterApplication|no Qt platform plugin could be initialized|failed to create (an )?(open ?gl )?context|Failed to initialize QPA platform' \
    "$UI_LOG_FILE"
}

# ui_log_tail: the end of the last run's log, indented.
ui_log_tail() {
  echo "     --- tail of $UI_LOG_FILE"
  tail -n 60 "$UI_LOG_FILE" | sed 's/^/     | /'
}

# ui_crash_report: the run ended with a signal (a crash, or our own kill).
ui_crash_report() {
  local name
  name=$(kill -l "$UI_SIGNAL" 2> /dev/null)
  ui_fail "$1: Mitcad ended with signal $UI_SIGNAL (${name:-?})"
  ui_log_tail
  # macOS has no gdb; the crash report of the system has the stack. Set
  # MITCAD_UI_LLDB=1 to run the app under lldb --batch, which prints it.
  local dir=$HOME/Library/Logs/DiagnosticReports report
  report=$(ls -t "$dir"/mitcad* "$dir"/Mitcad* 2> /dev/null | head -n 1)
  echo "     crash reports: $dir${report:+ (newest: $report)}"
}

# ui_run_app name arguments...: runs the app until it exits, or ends it
# after UI_TIMEOUT seconds (macOS has no GNU timeout), with stdout and stderr in
# <out>/<name>.log (Qt messages go to stderr with QT_FORCE_STDERR_LOGGING).
# Sets UI_EXIT (the exit code; 128 + n after signal n), UI_SIGNAL,
# UI_TIMED_OUT, UI_ASSERTION_FAILED and UI_LOG_FILE. Records an assertion,
# timeout, crash or non-zero exit. A session without a window server ends the
# test with 77, ctest's SKIP (MITCAD_UI_REQUIRE_GUI=1 makes it a failure).
ui_run_app() {
  local name=$1
  shift
  UI_LOG_FILE=$UI_OUT/$name.log
  UI_TIMED_OUT=0
  UI_ASSERTION_FAILED=0
  UI_SIGNAL=""
  rm -f "$UI_LOG_FILE"
  # Cross-checkout exclusion starts before launching the app or counting
  # UI_TIMEOUT. UI_PID stays the actual app (or the existing lldb mode).
  if ! ui_gui_lock; then return 1; fi
  if [ "${MITCAD_UI_LLDB:-0}" = 1 ] && command -v lldb > /dev/null; then
    lldb --batch -o run -o 'bt all' -- "$UI_APP" -ApplePersistenceIgnoreState YES "$@" > "$UI_LOG_FILE" 2>&1 &
  else
    "$UI_APP" -ApplePersistenceIgnoreState YES "$@" > "$UI_LOG_FILE" 2>&1 &
  fi
  UI_PID=$!
  local ticks=0 limit=$((UI_TIMEOUT * 5)) assertion_tick=-1
  while kill -0 "$UI_PID" 2> /dev/null; do
    if [ "${MITCAD_UI_LLDB:-0}" != 1 ]; then
      # A native modal loop can outlive the driver's failed assertion and
      # Qt exit request. Match the actual diagnostic, never a traced script
      # line containing that text, and give normal shutdown one second.
      if [ "$assertion_tick" -lt 0 ] && [ -f "$UI_LOG_FILE" ] &&
          grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG_FILE"; then
        assertion_tick=$ticks
      fi
      if [ "$assertion_tick" -ge 0 ] && [ "$((ticks - assertion_tick))" -ge 5 ]; then
        UI_ASSERTION_FAILED=1
        ui_stop_app
        break
      fi
    fi
    if [ "$ticks" -ge "$limit" ]; then
      # Keep the outer deadline even when the assertion arrived just now;
      # the recorded assertion is still the reason this run failed.
      if [ "$assertion_tick" -ge 0 ]; then UI_ASSERTION_FAILED=1; else UI_TIMED_OUT=1; fi
      ui_stop_app
      break
    fi
    sleep 0.2
    ticks=$((ticks + 1))
  done
  # Preserve the application's failure for the report even when a caller
  # enables errexit; wait must not terminate the harness first.
  UI_EXIT=0
  wait "$UI_PID" 2> /dev/null || UI_EXIT=$?
  UI_PID=""
  ui_gui_unlock
  if [ "${MITCAD_UI_LLDB:-0}" = 1 ]; then
    # lldb's own exit code says nothing: the app's is in its message, and
    # no such message means it stopped on a signal.
    local code
    code=$(sed -n 's/.*Process [0-9]* exited with status = \([0-9]*\).*/\1/p' "$UI_LOG_FILE" | tail -n 1)
    if [ -n "$code" ]; then UI_EXIT=$code; elif [ "$UI_TIMED_OUT" = 0 ]; then UI_EXIT=134; fi
  fi
  if [ "$UI_EXIT" -gt 128 ] && [ "$UI_TIMED_OUT" = 0 ] && [ "$UI_ASSERTION_FAILED" = 0 ]; then
    UI_SIGNAL=$((UI_EXIT - 128))
  fi

  if [ "$UI_TIMED_OUT" = 0 ] && [ "$UI_ASSERTION_FAILED" = 0 ] && ui_gui_unavailable && [ "${MITCAD_UI_REQUIRE_GUI:-0}" != 1 ]; then
    echo "SKIP: $name: no window server or OpenGL context in this session (MITCAD_UI_REQUIRE_GUI=1 to fail instead)"
    ui_log_tail
    exit 77
  fi
  if [ "$UI_ASSERTION_FAILED" = 1 ]; then
    ui_fail "$name: TestDriver assertion failed; stopped its lingering app (exit code $UI_EXIT)"
    ui_log_tail
  elif [ "$UI_TIMED_OUT" = 1 ]; then
    ui_fail "$name: Mitcad did not exit within $UI_TIMEOUT s"
    ui_log_tail
  elif [ -n "$UI_SIGNAL" ]; then
    ui_crash_report "$name"
  elif [ "$UI_EXIT" -ne 0 ]; then
    ui_fail "$name: exit code $UI_EXIT"
    ui_log_tail
  fi
}

# ui_expect_log pattern description: the last run's log has a line matching
# the extended regular expression.
ui_expect_log() {
  if grep -Eq -- "$1" "$UI_LOG_FILE"; then ui_pass "$2"; else ui_fail "$2: '$1' not in the log"; fi
}

# ui_expect_no_log text description: the log does not contain the text.
ui_expect_no_log() {
  if grep -Fq -- "$1" "$UI_LOG_FILE"; then ui_fail "$2: '$1' in the log"; else ui_pass "$2"; fi
}

# ui_log_value pattern: what the first line matching the regular expression
# has after it (sed, a basic one with the value in \(\)).
ui_log_value() {
  sed -n "s/.*$1.*/\\1/p" "$UI_LOG_FILE" | head -n 1
}

# ui_gt a b: a > b for decimal numbers.
ui_gt() { awk -v a="$1" -v b="$2" 'BEGIN { exit !(a > b) }'; }
ui_lt() { awk -v a="$1" -v b="$2" 'BEGIN { exit !(a < b) }'; }

# ui_update_test_hooks file: ON when the build that made the file reads the
# update tests' environment variables (the CMake option
# MITCAD_UPDATE_TEST_HOOKS, mitcad#921), OFF when it does not; empty when
# unknown. UI_UPDATE_TEST_HOOKS (the ctest's setting) decides first, then
# the first CMakeCache.txt above the file (as in ui-test-lib.sh).
ui_update_test_hooks() {
  local dir
  if [ -n "${UI_UPDATE_TEST_HOOKS:-}" ]; then
    case $UI_UPDATE_TEST_HOOKS in
      ON | on | 1 | TRUE | true | YES | yes) echo ON ;;
      *) echo OFF ;;
    esac
    return 0
  fi
  dir=$(cd "$(dirname "$1")" 2> /dev/null && pwd) || return 0
  while [ -n "$dir" ] && [ "$dir" != / ]; do
    if [ -f "$dir/CMakeCache.txt" ]; then
      if grep -qiE '^MITCAD_UPDATE_TEST_HOOKS:BOOL=(ON|1|TRUE|YES|Y)$' "$dir/CMakeCache.txt"; then echo ON; else echo OFF; fi
      return 0
    fi
    dir=$(dirname "$dir")
  done
  return 0
}

# ui_check_shot name: checks the screenshot <out>/<name>.png as the Windows
# test does (tools/ui-windows-test.ps1, Check-Shot): the light background,
# the body filling 20 % of the image or more in two lit faces at least, in
# the bluish grey of the body, and not a blank image. Sets UI_SHOT_OK (1 or
# 0), UI_SHOT_SHADES (the number of face shades) and UI_SHOT_RATIO (side
# faces relative to the top face; "none" with one shade).
ui_check_shot() {
  local name=$1 path=$UI_OUT/$1.png json fields
  UI_SHOT_OK=0
  UI_SHOT_SHADES=0
  UI_SHOT_RATIO=none
  if [ ! -s "$path" ]; then
    ui_fail "$name: no screenshot"
    return
  fi
  if ! json=$(python3 "$UI_STATS" --shot "$path" 2> "$UI_OUT/$name.stats.err"); then
    ui_fail "$name: the screenshot could not be analysed: $(cat "$UI_OUT/$name.stats.err")"
    return
  fi
  fields=$(printf '%s' "$json" | python3 -c '
import json, sys
d = json.load(sys.stdin)
shades = ", ".join("%d rgb(%d,%d,%d) %.1f%%" % (s["luma"], s["r"], s["g"], s["b"], s["percent"])
                   for s in d["shades"])
ratio = "none" if d["side_ratio"] is None else "%.2f" % d["side_ratio"]
print("|".join([str(d["width"]), str(d["height"]), str(d["background"]),
                "%.1f" % d["face_percent"], str(len(d["shades"])),
                "1" if d["bluish"] else "0", ratio, str(d["colors"]), shades]))
')
  local width height background faces count bluish ratio colors shades
  IFS='|' read -r width height background faces count bluish ratio colors shades <<< "$fields"
  echo "     $name: ${width}x$height, background $background, faces $faces%: $shades"
  UI_SHOT_SHADES=$count
  UI_SHOT_RATIO=$ratio
  # The floating chrome fits the body into the free area between its cards,
  # so it covers a smaller part of the whole window than the docked one did.
  if [ "$width" -lt 300 ] || [ "$height" -lt 200 ]; then ui_fail "$name: image too small"
  elif [ "$colors" -lt 8 ]; then ui_fail "$name: the image is blank"
  elif [ "$background" -lt 200 ]; then ui_fail "$name: background is not the light gradient"
  elif ui_lt "$faces" 12; then ui_fail "$name: no body in the image"
  elif [ "$count" -lt 2 ]; then ui_fail "$name: fewer than two lit faces"
  elif [ "$bluish" != 1 ]; then ui_fail "$name: faces are not the bluish grey body colour"
  else
    ui_pass "$name: body drawn with $count face shades"
    UI_SHOT_OK=1
  fi
}
