# SPDX-License-Identifier: MIT
# Shared helpers for the UI tests. Source this file; it is not a script.
#
# The tests run Mitcad on a private Xvfb display, so no window appears on the
# desktop. OpenGL then uses Mesa's software renderer. Set MITCAD_UI_VISIBLE=1
# to use the current $DISPLAY instead (for example WSLg) and watch.

set -u

UI_APP=${UI_APP:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/build/dev/app/mitcad}
UI_LOG=$(mktemp /tmp/mitcad-ui.XXXXXX.log)
UI_XVFB_PID=""
UI_RUNNER=""
UI_WINDOW=""
# Settings of our own (shortcuts), so the user's do not change the tests.
UI_CONFIG=$(mktemp -d /tmp/mitcad-ui-config.XXXXXX)
export XDG_CONFIG_HOME=$UI_CONFIG
# Application data of our own: autosave's recovery folder
# ($XDG_DATA_HOME/Mitcad/Mitcad/autosave), which a test that kills the app
# leaves behind, and which neither the user's nor another test's may see.
UI_DATA=$(mktemp -d /tmp/mitcad-ui-data.XXXXXX)
export XDG_DATA_HOME=$UI_DATA
unset MITCAD_AUTOSAVE_DIR
# A cache of our own: the store of computed results
# ($XDG_CACHE_HOME/Mitcad/Mitcad/results, P7d), so that each test starts
# with an empty one. The store is off unless a test turns it on
# (MITCAD_RESULT_STORE=$UI_RESULTS): what a slow debug build stores would
# otherwise change how many features later steps compute.
UI_CACHE=$(mktemp -d /tmp/mitcad-ui-cache.XXXXXX)
export XDG_CACHE_HOME=$UI_CACHE
UI_RESULTS=$UI_CACHE/Mitcad/Mitcad/results
export MITCAD_RESULT_STORE=off
unset MITCAD_RESULT_STORE_MIN_MS
# Git's configuration of our own (empty), so that versions saved in a
# project (P12d) never take the user's name and email: a test that records
# versions gives its author itself (tools/ui-version-test.sh).
export GIT_CONFIG_GLOBAL=$UI_CONFIG/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL
# Where faces, edges and vertices can be clicked, logged when a command's
# input takes them ("Pick face F2.b0/F2:end(...) at x,y"; ui_click_pick).
export MITCAD_LOG_PICKS=1
# The bodies' volumes before and after each change ("Body Body1 (F2.b0):
# volume a -> b mm3"; ui_expect_volume).
export MITCAD_LOG_VOLUMES=1
# The sync key (ui_sync): Pause answered with "Sync <n>" (app/framework/TestSync.hpp).
export MITCAD_TEST_SYNC=1
# The app's scale (QT_SCALE_FACTOR, a whole number): with UI_SCALE=2 it runs
# at device pixel ratio 2 on a screen twice as large. The app logs places in
# logical pixels; the helpers below click at UI_SCALE times them.
UI_SCALE=${UI_SCALE:-1}
if [ "$UI_SCALE" != 1 ]; then
  export QT_SCALE_FACTOR=$UI_SCALE
fi
# No update checks (mitcad#9): the tests make no network requests;
# tools/ui-update-test.sh checks against a local server of its own.
export MITCAD_NO_UPDATE_CHECK=1
unset MITCAD_UPDATE_URL MITCAD_UPDATE_RELEASES_URL MITCAD_UPDATE_TEST_KEY MITCAD_UPDATE_TEST_CA MITCAD_TEST_VERSION

for tool in xdotool Xvfb; do
  if ! command -v "$tool" > /dev/null; then
    echo "$tool is required (apt-get install xdotool xvfb x11-apps)" >&2
    exit 2
  fi
done

# ui_stop_app: stops this test's Mitcad, and only it: other tests may be
# running the same binary at the same time. Under gdb the app is gdb's child.
ui_stop_app() {
  if [ -n "$UI_RUNNER" ]; then
    pkill -P "$UI_RUNNER" 2> /dev/null
    kill "$UI_RUNNER" 2> /dev/null
    # gdb can stop the app on the first signal and then neither end it nor
    # exit: kill both when gdb has not exited in a few seconds.
    for _ in $(seq 1 50); do
      grep -qs '^State:[[:space:]]*[^Z]' "/proc/$UI_RUNNER/status" || break
      sleep 0.1
    done
    pkill -KILL -P "$UI_RUNNER" 2> /dev/null
    kill -KILL "$UI_RUNNER" 2> /dev/null
    wait "$UI_RUNNER" 2> /dev/null
  fi
  UI_RUNNER=""
}

ui_cleanup() {
  ui_stop_app
  [ -n "$UI_XVFB_PID" ] && kill "$UI_XVFB_PID" 2> /dev/null
  rm -rf "$UI_CONFIG" "$UI_DATA" "$UI_CACHE"
}
trap ui_cleanup EXIT

# ui_lock_owner n: the PID in display n's lock file, which an X server
# creates atomically (link()) for its display number before it starts.
ui_lock_owner() { tr -dc 0-9 2> /dev/null < "/tmp/.X$1-lock"; }

ui_start_display() {
  if [ "${MITCAD_UI_VISIBLE:-0}" = 1 ]; then
    return
  fi
  # Several tests may start at once, also from different checkouts. Xvfb
  # gives up on a display number whose lock file another live X server owns,
  # so try numbers until our own server owns the lock and answers. (Xvfb
  # -displayfd does not work here: WSLg mounts /tmp/.X11-unix read-only, so
  # Xvfb serves only its abstract socket, and -displayfd then needs
  # -nolisten unix and takes :0 from WSLg.)
  # -noreset: the server stays up when its last client goes, so that a
  # test that ends the app and starts it again finds it at once.
  local display owner
  for display in $(seq 90 189); do
    owner=$(ui_lock_owner "$display")
    [ -n "$owner" ] && kill -0 "$owner" 2> /dev/null && continue
    Xvfb ":$display" -screen 0 $((1600 * UI_SCALE))x$((1000 * UI_SCALE))x24 -nolisten tcp -noreset \
      > /dev/null 2>&1 &
    UI_XVFB_PID=$!
    # Up to 30 s: competing servers retry their lock every 2 s.
    for _ in $(seq 1 300); do
      kill -0 "$UI_XVFB_PID" 2> /dev/null || break # the number was taken
      if [ "$(ui_lock_owner "$display")" = "$UI_XVFB_PID" ] &&
        DISPLAY=":$display" xdotool getdisplaygeometry > /dev/null 2>&1; then
        export DISPLAY=":$display"
        # The GPU passthrough settings only work with WSLg's own display.
        unset GALLIUM_DRIVER MESA_D3D12_DEFAULT_ADAPTER_NAME
        return
      fi
      sleep 0.1
    done
    kill "$UI_XVFB_PID" 2> /dev/null
    wait "$UI_XVFB_PID" 2> /dev/null
    UI_XVFB_PID=""
  done
  echo "FAIL: Xvfb did not start" >&2
  exit 1
}

# ui_start_app [app arguments...]: starts Mitcad (under gdb when available).
# UI_GDB=0 runs it without gdb; UI_WINDOW_WAIT is how long the window may
# take to appear (seconds, default 20; a large design opens first). The app
# does not offer to recover work at start-up (--no-recovery: a test that
# killed it and starts it again would get the question) unless
# UI_RECOVERY=1.
ui_start_app() {
  local options=(--no-recovery)
  [ "${UI_RECOVERY:-0}" = 1 ] && options=()
  if [ "${UI_GDB:-1}" = 1 ] && command -v gdb > /dev/null; then
    gdb -q -batch -ex "set debuginfod enabled off" -ex run -ex bt --args "$UI_APP" "${options[@]}" "$@" \
      > "$UI_LOG" 2>&1 &
  else
    "$UI_APP" "${options[@]}" "$@" > "$UI_LOG" 2>&1 &
  fi
  UI_RUNNER=$!
  # The main window is titled "<document> - Mitcad", "*" marking changes.
  for _ in $(seq 1 $((${UI_WINDOW_WAIT:-20} * 5))); do
    UI_WINDOW=$(xdotool search --onlyvisible --name ' - Mitcad$' 2> /dev/null | head -1)
    [ -n "$UI_WINDOW" ] && break
    sleep 0.2
  done
  if [ -z "$UI_WINDOW" ]; then
    echo "FAIL: Mitcad window did not appear"; cat "$UI_LOG"; exit 1
  fi
  ui_sync
  # Not while a progress dialog blocks the window (--set computing): Qt
  # would pass the focus on to the dialog, and has none when it closes.
  ui_wait_idle
  xdotool windowmove "$UI_WINDOW" 0 0 2> /dev/null
  xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
}

# ui_sync [window]: waits until the app has handled the input sent so far
# and what it set going (a frame, a panel's layout logged, the camera at
# rest, a preview started): the Pause key, which the app answers with
# "Sync <n>" then (MITCAD_TEST_SYNC, app/framework/TestSync.hpp). The key
# goes through XTest like the input before it, so it comes after that;
# when no window of the app has the keyboard, it is not answered, and
# after a second it is sent once more straight to the main window (or the
# one given; XSendEvent, which would overtake input still queued). Up to
# UI_SYNC_WAIT seconds (default 20), noted when the app did not answer.
ui_sync() {
  local before i
  before=$(grep -c '^Sync [0-9]*$' "$UI_LOG")
  xdotool key Pause
  for i in $(seq 1 $((${UI_SYNC_WAIT:-20} * 50))); do
    [ "$(grep -c '^Sync [0-9]*$' "$UI_LOG")" -gt "$before" ] && return 0
    [ "$i" = 50 ] && xdotool key --window "${1:-$UI_WINDOW}" Pause 2> /dev/null
    # A crashed or ended app does not answer.
    [ $((i % 10)) = 0 ] && ui_crashed && return 0
    sleep 0.02
  done
  echo "note: no answer to the sync key in ${UI_SYNC_WAIT:-20} s"
}

# Screen position of a point given in percent of the window size.
ui_at() { echo $((X + WIDTH * $1 / 100)) $((Y + HEIGHT * $2 / 100)); }

ui_crashed() {
  grep -qE "SIGSEGV|SIGABRT|terminate called|Aborted|Segmentation fault" "$UI_LOG" ||
    ! kill -0 "$UI_RUNNER" 2> /dev/null
}

ui_fail() {
  echo "FAIL: $1"
  grep -vE "^\[(New|Thread|Detaching)|libthread_db|Using host|auto-load|debug_gdb_scripts" \
    "$UI_LOG" | tail -40
  exit 1
}

# ui_wait_idle: waits (up to 60 s) until the app computes nothing: every job
# that logged "Computing <label> started" (one that took over 50 ms; P7)
# has logged its end. While a job's progress dialog shows, the window takes
# no input, so UI steps wait first, as they waited for the model
# before it computed in the background; tests that cancel a job send its
# Esc without a step.
ui_wait_idle() {
  local started ended
  for _ in $(seq 1 300); do
    started=$(grep -c 'Computing .* started$' "$UI_LOG")
    ended=$(grep -cE 'Computing .* (done in|cancelled after) [0-9]+ ms' "$UI_LOG")
    [ "$started" -le "$ended" ] && return 0
    sleep 0.2
  done
  echo "note: the app was still computing after 60 s"
}

# ui_step "description" command...: runs a UI action (once the app computes
# nothing), waits until the app has handled it (ui_sync) and checks for
# crashes.
ui_step() {
  local name=$1; shift
  ui_wait_idle
  "$@"
  ui_sync
  ui_crashed && ui_fail "crashed during '$name'"
  echo "ok   $name"
}

# ui_expect_log "text" "description": waits for a line in the application log.
ui_expect_log() {
  for _ in $(seq 1 30); do
    grep -qF -- "$1" "$UI_LOG" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: '$1' not in the log"
}

ui_click() { xdotool mousemove $(ui_at "$1" "$2") click "${3:-1}"; }

# ui_apart x y: before a drag's press at (x, y). Where the pointer is now,
# it may have clicked a moment ago, and Qt makes a press within 400 ms
# and a few pixels of the last one a double-click: when the pointer is
# that near, the press waits that long.
ui_apart() {
  local X Y SCREEN WINDOW
  eval "$(xdotool getmouselocation --shell 2> /dev/null)"
  if [ $(((X - $1) * (X - $1) + (Y - $2) * (Y - $2))) -le 100 ]; then
    sleep 0.45
  fi
}

# No --clearmodifiers: it restores the modifier state it sampled, which can
# leave Ctrl pressed after a Ctrl+key combination and turn the next key into
# a Ctrl shortcut.
ui_key() { xdotool key --delay 50 "$@"; }

# ui_drag button from-x% from-y% to-x% to-y% [modifier]
ui_drag() {
  local button=$1 modifier=${6:-}
  [ -n "$modifier" ] && xdotool keydown "$modifier"
  ui_apart $(ui_at "$2" "$3")
  xdotool mousemove $(ui_at "$2" "$3") mousedown "$button"
  for i in 1 2 3 4 5; do
    xdotool mousemove $(ui_at $(($2 + ($4 - $2) * i / 5)) $(($3 + ($5 - $3) * i / 5)))
    sleep 0.05
  done
  xdotool mouseup "$button"
  [ -n "$modifier" ] && xdotool keyup "$modifier"
  return 0
}

# The app logs where things are, in window coordinates: "View area x y w h",
# "Panel <command> input <id> at x,y", "Ribbon SOLID/CREATE at x,y",
# "Datum xy at x,y". The helpers below click there, so the tests do not
# depend on the layout of the window.

# ui_view_at x% y%: screen position of a point given in percent of the 3D view.
ui_view_at() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  echo $((X + (vx + vw * $1 / 100) * UI_SCALE)) $((Y + (vy + vh * $2 / 100) * UI_SCALE))
}

ui_view_click() { xdotool mousemove $(ui_view_at "$1" "$2") click "${3:-1}"; }

# ui_view_drag button from-x% from-y% to-x% to-y%: a drag in the 3D view.
ui_view_drag() {
  local i
  ui_apart $(ui_view_at "$2" "$3")
  xdotool mousemove $(ui_view_at "$2" "$3") mousedown "$1"
  for i in 1 2 3 4 5; do
    xdotool mousemove $(ui_view_at $(($2 + ($4 - $2) * i / 5)) $(($3 + ($5 - $3) * i / 5)))
    sleep 0.05
  done
  xdotool mouseup "$1"
}

# ui_logged_at "text": screen position of the last "<text> at x,y" line.
ui_logged_at() {
  local at px py
  at=$(grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9]*\),\([0-9]*\).*/\1 \2/p')
  [ -n "$at" ] || ui_fail "no position logged for '$1'"
  read -r px py <<< "$at"
  echo $((X + px * UI_SCALE)) $((Y + py * UI_SCALE))
}

ui_click_logged() { xdotool mousemove $(ui_logged_at "$1") click "${2:-1}"; }

# ui_command "name": runs a command through the command search (S).
ui_command() {
  local before
  before=$(grep -c "Command search chose" "$UI_LOG")
  ui_key s
  ui_sync
  xdotool type --delay 40 "$1"
  ui_sync
  ui_key Return
  for _ in $(seq 1 25); do
    [ "$(grep -c "Command search chose" "$UI_LOG")" -gt "$before" ] && return 0
    sleep 0.2
  done
  ui_fail "the command search did not run '$1'"
}

# ui_create_sketch [plane]: Create Sketch through the search, then a click
# on the origin plane (default xy) it asks for.
ui_create_sketch() {
  local plane=${1:-xy} before
  before=$(grep -c "Sketch started on" "$UI_LOG")
  ui_command "Create Sketch"
  ui_expect_log "Create Sketch: select a plane" "Create Sketch asks for a plane" > /dev/null
  ui_sync
  ui_click_logged "Datum $plane"
  for _ in $(seq 1 25); do
    [ "$(grep -c "Sketch started on $plane" "$UI_LOG")" -gt "$before" ] && return 0
    sleep 0.2
  done
  ui_fail "no sketch started on $plane"
}

# The sketch view's placement, logged as "Sketch view x0,y0 x1,y1 x2,y2":
# where the sketch points (0, 0), (100, 0) and (0, 100) are in the window.
# ui_sketch_at x y: the screen position of a sketch point (mm).
ui_sketch_at() {
  local view
  view=$(grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the sketch view"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[3], o, ","); split(part[4], a, ","); split(part[5], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}

# ui_sketch_click x y [button]: a click at a sketch point (mm).
ui_sketch_click() { xdotool mousemove $(ui_sketch_at "$1" "$2") click "${3:-1}"; }

# ui_sketch_drag x1 y1 x2 y2: a left drag between sketch points (mm).
ui_sketch_drag() {
  local i fx fy tx ty
  read -r fx fy <<< "$(ui_sketch_at "$1" "$2")"
  read -r tx ty <<< "$(ui_sketch_at "$3" "$4")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
}

# ui_menu_choose "entry": picks an entry of the context menu just opened,
# with the arrow keys, from the logged "Context menu: a | b | c".
ui_menu_choose() {
  local entries index=0 found=""
  ui_sync
  entries=$(grep "Context menu: " "$UI_LOG" | tail -1 | sed 's/.*Context menu: //')
  IFS='|' read -ra items <<< "$entries"
  for item in "${items[@]}"; do
    index=$((index + 1))
    item=${item# }
    item=${item% }
    if [ "$item" = "$1" ]; then
      found=$index
      break
    fi
  done
  [ -n "$found" ] || ui_fail "'$1' is not in the context menu: $entries"
  xdotool key --delay 80 --repeat "$found" Down
  ui_key Return
}

# ui_double_click_logged "text": a double-click at the last "<text> at x,y".
ui_double_click_logged() { xdotool mousemove $(ui_logged_at "$1") click --repeat 2 --delay 80 1; }

# ui_drag_logged "from text" "to text" [dx]: a left drag between two logged
# places; dx moves the end that many pixels right (negative: left).
ui_drag_logged() {
  local fx fy tx ty i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  read -r tx ty <<< "$(ui_logged_at "$2")"
  tx=$((tx + ${3:-0}))
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
}

# ui_focus_dialog "title regex": waits for a dialog and gives it the keyboard
# (Xvfb has no window manager to do it); ui_focus_main gives it back.
ui_focus_dialog() {
  local dialog=""
  for _ in $(seq 1 50); do
    dialog=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
    [ -n "$dialog" ] && break
    sleep 0.2
  done
  [ -n "$dialog" ] || ui_fail "no dialog matching '$1'"
  ui_sync "$dialog"
  xdotool windowfocus --sync "$dialog" 2> /dev/null
  ui_sync "$dialog"
}
ui_focus_main() {
  ui_sync
  ui_wait_idle
  xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
  ui_sync
}

# ui_mark; ...; ui_expect_new "text" "description" [seconds]: like
# ui_expect_log, but only lines logged after the mark count; waits up to
# 6 s, or as long as given (slow geometry such as modelled threads).
ui_mark() { UI_MARK=$(wc -l < "$UI_LOG"); }
ui_expect_new() {
  for _ in $(seq 1 $((${3:-6} * 5))); do
    tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: '$1' not in the log since the mark"
}

# ui_pick_at "text": the screen position of a pick place ("Pick face
# F2.b0/F2:end(r{...}) at x,y") whose line contains the text, among those
# logged last (after the last "Pick places:"; older ones may be stale).
ui_pick_at() {
  local at px py start
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  at=$(tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick " | grep -F -- "$1" | tail -1 |
    sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  [ -n "$at" ] || ui_fail "no pick place logged for '$1'"
  read -r px py <<< "$at"
  echo $((X + px * UI_SCALE)) $((Y + py * UI_SCALE))
}

# ui_click_pick "text": clicks a face, edge or vertex where it was logged.
ui_click_pick() { xdotool mousemove $(ui_pick_at "$1") click 1; }

# ui_drag_from "text" dx dy: a left drag from the last "<text> at x,y" by
# dx, dy pixels (a manipulator's handle: "Manipulator Extrude distance").
ui_drag_from() {
  local fx fy i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + $2 * i / 8)) $((fy + $3 * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
}

# ui_type_in "Panel <command> input <id>" text: replaces a panel field's
# text (a click in it, select all, type).
ui_type_in() {
  ui_click_logged "$1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
}

# ui_choose "Panel <command> input <id>" downs: picks the entry that many
# below the current one in a panel's drop-down.
ui_choose() {
  ui_click_logged "$1"
  ui_sync
  if [ "$2" -gt 0 ]; then
    xdotool key --delay 80 --repeat "$2" Down
  fi
  ui_key Return
}

# ui_expect_volume "sed pattern with one \( \) group" expected description
# [tolerance]: the last logged number the pattern captures equals the
# expected volume (relative tolerance, default 1e-6).
ui_expect_volume() {
  local volume=""
  # Measuring bodies after a feature can take a while.
  for _ in $(seq 1 150); do
    volume=$(sed -n "s/.*$1.*/\\1/p" "$UI_LOG" | tail -1)
    [ -n "$volume" ] && break
    sleep 0.2
  done
  [ -n "$volume" ] || ui_fail "$3: no volume logged"
  awk -v v="$volume" -v e="$2" -v t="${4:-1e-6}" 'BEGIN { exit !((v - e) ^ 2 < (t * e) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $volume mm3, expected $2"
  echo "ok   $3: $volume mm3"
}

# ui_capture file.png: saves the screen of the test display.
ui_capture() {
  local dump
  dump=$(mktemp /tmp/mitcad-ui.XXXXXX.xwd)
  xwd -root -silent > "$dump" && python3 "$(dirname "${BASH_SOURCE[0]}")/xwd2png.py" "$dump" "$1"
  rm -f "$dump"
}

ui_finish() {
  if grep -q "OCCT view event failed" "$UI_LOG"; then
    ui_fail "OCCT reported a failed view operation"
  fi
  echo "PASS: $1"
}
