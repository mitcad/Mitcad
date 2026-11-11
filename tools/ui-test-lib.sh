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
# No desktop of the caller's: run from a GNOME or KDE session, Qt would take
# that desktop's platform theme (on GNOME, Return presses the focused check
# box instead of the dialog's default button), and through the session bus
# its portal's colour scheme and file dialogs instead of the test's.
unset XDG_CURRENT_DESKTOP GNOME_DESKTOP_SESSION_ID DESKTOP_SESSION KDE_FULL_SESSION WAYLAND_DISPLAY
export DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent/mitcad-ui-test-bus
# No printers of the machine's either (mitcad#1591): Qt's QPrinter takes the
# print system's default printer, or its first one, and CUPS lists the
# network printers it finds on the local network. For one it has not used
# lately it sets up a queue first and waits for that printer to answer, up
# to a minute when it sleeps, so File › Print waited on the network. With
# no print server the tests' printer is a PDF one at once
# (MITCAD_TEST_PRINT_PDF names its file).
export CUPS_SERVER=/nonexistent/mitcad-ui-test-cups.sock
# Where faces, edges and vertices can be clicked, logged when a command's
# input takes them ("Pick face F2.b0/F2:end(...) at x,y"; ui_click_pick).
export MITCAD_LOG_PICKS=1
# The bodies' volumes before and after each change ("Body Body1 (F2.b0):
# volume a -> b mm3"; ui_expect_volume).
export MITCAD_LOG_VOLUMES=1
# The sync key (ui_sync): Pause answered with "Sync <n>" (app/framework/TestSync.hpp).
export MITCAD_TEST_SYNC=1
# What typed-in fields hold: "File dialog <title>: <text>", "Dialog field
# <title>: <text>", "Panel field <input>: <text>"
# (app/framework/TypedTextLog.hpp; ui_type_field, ui_type_in).
export MITCAD_LOG_TYPED_TEXT=1
# The app's scale (QT_SCALE_FACTOR, a whole number): with UI_SCALE=2 it runs
# at device pixel ratio 2 on a screen twice as large. The app logs places in
# logical pixels; the helpers below click at UI_SCALE times them.
UI_SCALE=${UI_SCALE:-1}
if [ "$UI_SCALE" != 1 ]; then
  export QT_SCALE_FACTOR=$UI_SCALE
fi
# The test screen's logical height (UI_SCREEN_HEIGHT, default 1000): a test
# of a tall dialog that must not scroll sets a larger one before
# ui_start_display.
# No update checks (mitcad#9): the tests make no network requests;
# tools/ui-update-test.sh checks against a local server of its own.
export MITCAD_NO_UPDATE_CHECK=1
# The first dimension of a drawn sketch scales nothing (mitcad#860): the
# tests' sketches keep the sizes they are drawn at, as their clicks expect.
export MITCAD_SKETCH_FIRST_DIMENSION_SCALE=0
# Create Sketch and the Construct commands on another component's face ask
# nothing (mitcad#960): the tests' picks stay as they were. A test of the
# question sets it to 1 after sourcing this file.
export MITCAD_ASK_ACTIVATE_COMPONENT=0
# Command panels start from their defaults, not from the values given on
# an earlier OK (mitcad#1028): a step's fillet never changes where a later
# step's Fillet starts. A test of the remembered values sets it to 1 before
# ui_start_app.
export MITCAD_REMEMBER_PANEL_VALUES=0
# MitCad starts on an empty design, not on the Start tab (mitcad#1258), as
# the tests were written for. A test of the Start page sets it to 1 before
# ui_start_app (the page then shows by its Show start page at launch).
export MITCAD_START_PAGE=0
# The interface in English whatever the machine's language (mitcad#1460):
# the tests read English texts. A test of the language passes --language,
# which comes first.
export MITCAD_LANGUAGE=en
# No browser either: an address the app would open (a report's issue form,
# mitcad#61) is logged instead ("Open URL (test, not opened): <url>").
export MITCAD_TEST_LOG_URLS=1
unset MITCAD_TEST_CRASH MITCAD_CRASH_DIR MITCAD_CRASH_PARENT
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
  # The other instances of a test that runs several (ui_instance).
  local instance runner
  for instance in $UI_INSTANCES; do
    [ "$instance" = "$UI_CURRENT" ] && continue
    runner="UI_INSTANCE_${instance}_RUNNER"
    UI_RUNNER=${!runner:-}
    ui_stop_app
  done
  [ -n "$UI_XVFB_PID" ] && kill "$UI_XVFB_PID" 2> /dev/null
  rm -rf "$UI_CONFIG" "$UI_DATA" "$UI_CACHE"
}
trap ui_cleanup EXIT

# Several instances of Mitcad at once (tools/ui-locks-test.sh): the helpers
# work on the current one, its process, log, window and mark (UI_RUNNER,
# UI_LOG, UI_WINDOW, X, Y, WIDTH, HEIGHT, UI_MARK). ui_instance NAME keeps
# the current instance's state and makes NAME the current one: an instance
# seen before gets its state back and its window raised and given the
# keyboard; a new one starts with a log of its own and no process, for
# ui_start_app. They share the display, the settings and the data folders.
UI_INSTANCES=""
UI_CURRENT=""
# ui_app_pid: the current instance's process (gdb's child under gdb).
ui_app_pid() {
  local child
  child=$(pgrep -P "$UI_RUNNER" 2> /dev/null | head -1)
  echo "${child:-$UI_RUNNER}"
}
ui_instance() {
  local var
  if [ -n "$UI_CURRENT" ]; then
    for var in RUNNER LOG WINDOW MARK; do
      local value="UI_$var"
      printf -v "UI_INSTANCE_${UI_CURRENT}_$var" '%s' "${!value:-}"
    done
    printf -v "UI_INSTANCE_${UI_CURRENT}_GEOMETRY" '%s %s %s %s' "${X:-0}" "${Y:-0}" "${WIDTH:-0}" "${HEIGHT:-0}"
  fi
  case " $UI_INSTANCES " in
  *" $1 "*)
    for var in RUNNER LOG WINDOW MARK; do
      local saved="UI_INSTANCE_${1}_$var"
      printf -v "UI_$var" '%s' "${!saved:-}"
    done
    local geometry="UI_INSTANCE_${1}_GEOMETRY"
    read -r X Y WIDTH HEIGHT <<< "${!geometry}"
    ;;
  *)
    UI_INSTANCES="$UI_INSTANCES $1"
    if [ -n "$UI_CURRENT" ]; then
      UI_LOG=$(mktemp /tmp/mitcad-ui.XXXXXX.log)
    fi
    UI_RUNNER=""
    UI_WINDOW=""
    UI_MARK=0
    ;;
  esac
  UI_CURRENT=$1
  if [ -n "$UI_WINDOW" ] && [ -n "$UI_RUNNER" ]; then
    xdotool windowraise "$UI_WINDOW" 2> /dev/null
    xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
    ui_sync
  fi
}

# ui_lock_owner n: the PID in display n's lock file, which an X server
# creates atomically (link()) for its display number before it starts.
ui_lock_owner() { tr -dc 0-9 2> /dev/null < "/tmp/.X$1-lock"; }

# ui_plain_function_keys: makes F1 to F12 plain keys of one level on the
# test display. In newer xkeyboard-config data (2.46, for one) their key
# type maps Alt and Control to the first level explicitly, and xdotool
# (3.20160805) then presses Alt with them: "xdotool key F6" sends Alt+F6,
# which no shortcut takes. The server's keymap is read, the function keys'
# type replaced (their other levels only switch consoles) and loaded
# back; the server keeps it (-noreset). Older keymaps change nothing that
# a test notices.
ui_plain_function_keys() {
  local keymap
  keymap=$(xkbcomp -xkb "$DISPLAY" - 2> /dev/null) || return 0
  sed -E '/^ *key <FK[0-9]+> \{/,/\};/{s/type= "[^"]*"/type= "ONE_LEVEL"/;s/(symbols\[Group1\]= \[ *[A-Za-z0-9_]+),[^]]*\]/\1 ]/}' \
    <<< "$keymap" | xkbcomp -w 0 - "$DISPLAY" > /dev/null 2>&1 ||
    echo "note: the function keys' keymap was not changed"
}

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
    Xvfb ":$display" -screen 0 $((1600 * UI_SCALE))x$((${UI_SCREEN_HEIGHT:-1000} * UI_SCALE))x24 -nolisten tcp -noreset \
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
        ui_plain_function_keys
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

# ui_chooses_chrome [app arguments...]: whether the arguments choose the
# window's layout (--chrome=…, --chrome …).
ui_chooses_chrome() {
  local argument
  for argument in "$@"; do
    case "$argument" in
    --chrome | --chrome=*) return 0 ;;
    esac
  done
  return 1
}

# ui_start_app [app arguments...]: starts Mitcad, without gdb: gdb loads the
# debug build's symbols at every start, and the app's crash handler logs a
# crash's stack itself. UI_GDB=1 runs it under gdb (when available) for
# gdb's backtrace; tools/check-all.sh reruns a crashed test so.
# UI_WINDOW_WAIT is how long the window may take to appear (seconds, default 20; a large design opens first). The app
# does not offer to recover work at start-up (--no-recovery: a test that
# killed it and starts it again would get the question) unless
# UI_RECOVERY=1. The window is Docked (--chrome=docked), which the tests
# were written for, unless the arguments choose a layout (--chrome…) or
# UI_DEFAULT_CHROME=1 leaves it to the preference and the default
# (Floating since mitcad#429).
# ui_open_in_current_window: New and Open replace the window's design, as
# they did before several designs could be open at once (Preferences >
# General > Open in the current window, mitcad#1172). For tests of that
# replacement and its questions (save, discard, locks); call it before
# ui_start_app.
ui_open_in_current_window() {
  mkdir -p "$UI_CONFIG/Mitcad"
  printf '[documents]\nopenInCurrentWindow=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
}

ui_start_app() {
  local options=(--no-recovery) before window
  # Never on the caller's desktop: a test that forgot ui_start_display
  # would show the window there and type its keys into whatever has focus.
  if [ -z "$UI_XVFB_PID" ] && [ "${MITCAD_UI_VISIBLE:-0}" != 1 ]; then
    echo "FAIL: ui_start_app without ui_start_display (the test's own Xvfb display)"
    exit 1
  fi
  [ "${UI_RECOVERY:-0}" = 1 ] && options=()
  if [ "${UI_DEFAULT_CHROME:-0}" != 1 ] && ! ui_chooses_chrome "$@"; then
    options+=(--chrome=docked)
  fi
  # The windows of other instances (ui_instance) are not this one's.
  before=" $(xdotool search --onlyvisible --name ' - MitCad [^ ]+$' 2> /dev/null | tr '\n' ' ')"
  if [ "${UI_GDB:-0}" = 1 ] && command -v gdb > /dev/null; then
    # gdb's status is the app's (mitcad#1665): the backtrace only while the
    # app is stopped (on a signal), since a bt after a normal exit fails and
    # a batch gdb then exits non-zero although the test passed; a crash
    # still gives its backtrace and a non-zero status.
    gdb -q -batch -return-child-result -ex "set debuginfod enabled off" -ex run \
      -ex "python (gdb.execute('bt') if gdb.selected_inferior().pid else None)" \
      --args "$UI_APP" "${options[@]}" "$@" > "$UI_LOG" 2>&1 &
  else
    "$UI_APP" "${options[@]}" "$@" > "$UI_LOG" 2>&1 &
  fi
  UI_RUNNER=$!
  # The main window is titled "<document> - MitCad <version>", "*" marking
  # changes (mitcad#1209: the version; ui_window_title leaves it out).
  UI_WINDOW=""
  for _ in $(seq 1 $((${UI_WINDOW_WAIT:-20} * 20))); do
    for window in $(xdotool search --onlyvisible --name ' - MitCad [^ ]+$' 2> /dev/null); do
      case "$before " in
      *" $window "*) ;;
      *)
        UI_WINDOW=$window
        break
        ;;
      esac
    done
    [ -n "$UI_WINDOW" ] && break
    sleep 0.05
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
  ui_keep_log
  exit 1
}

# ui_keep_log [log [instance]]: with MITCAD_UI_LOG_DIR set (tools/check-all.sh
# sets it), a failed test keeps the application's whole log there, as
# ui-<name>.app.log (ui-<name>.<instance>.app.log): the one in /tmp is
# not kept for long.
ui_keep_log() {
  [ -n "${MITCAD_UI_LOG_DIR:-}" ] || return 0
  local name
  name=$(basename "$0" .sh)
  name=$MITCAD_UI_LOG_DIR/${name%-test}${2:+.$2}.app.log
  cp -f "${1:-$UI_LOG}" "$name" 2> /dev/null && echo "the application's log: $name"
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

# ui_poll_pause i: the pause after the i-th look at the log in a wait for a
# line: 0.05 s during the first second, where most lines come, then 0.2 s
# (mitcad#833). A wait of T seconds looks 5 * T + 15 times.
ui_poll_pause() {
  if [ "$1" -le 20 ]; then sleep 0.05; else sleep 0.2; fi
}

# ui_busy: the app computes now: a job logged "Computing <label> started"
# and not yet its end (ui_wait_idle's test).
ui_busy() {
  local started ended
  started=$(grep -c 'Computing .* started$' "$UI_LOG")
  ended=$(grep -cE 'Computing .* (done in|cancelled after) [0-9]+ ms' "$UI_LOG")
  [ "$started" -gt "$ended" ]
}

# ui_wait_busy pauses: while the app computes, one pause of 0.2 s that does
# not count towards a wait's time (mitcad#872): a wait for a line that
# follows a computation (a preview, an OK that adds a feature) waits for
# the computation first, up to 60 s in all (`pauses`, the pauses so far, as
# ui_wait_idle), then its own time. Under a full check's load a debug
# build's preview took 6 s and the line came just after a fixed 6 s wait
# (ui-pick's chamfer preview, ui-sweep's coil), and each such failure
# lowered the machine's cap of UI tests at once. Status 1 when the app is
# idle or the 60 s are spent.
ui_wait_busy() {
  [ "$1" -lt 300 ] && ui_busy || return 1
  sleep 0.2
}

# ui_expect_log "text" "description": waits for a line in the application log
# (a single ellipsis and three dots match each other, mitcad#645), up to 6 s
# while the app is idle (the time it computes is not counted, ui_wait_busy).
ui_expect_log() {
  local i=0 busy=0
  while [ "$i" -lt 45 ]; do
    # (No pipeline into grep -q under pipefail: ui_expect_new.)
    grep -qF -- "${1//…/...}" < <(sed "s/…/.../g" "$UI_LOG") && { echo "ok   $2"; return; }
    if ui_wait_busy "$busy"; then
      busy=$((busy + 1))
      continue
    fi
    i=$((i + 1))
    ui_poll_pause "$i"
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

# ui_type_ahead "text": types an item view's type-ahead search (a list's or
# a tree's row by its first letters). Qt joins the keys typed within its
# keyboard input interval (400 ms) into one search, also across steps: a
# search typed right after another ("W", then "Slot") would look for
# "WSlot" and find nothing. It waits that long first.
ui_type_ahead() {
  sleep 0.5
  xdotool type --delay 40 "$1"
}

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

# ui_place_valid "text": whether the last line about "<text>" gives its place
# ("<text> at x,y") and not that the thing left it ("<text> out of sight":
# a row of the browser scrolled away, folded, or gone).
ui_place_valid() {
  local last
  # (A single ellipsis and three dots match each other, mitcad#645.)
  last=$(sed "s/…/.../g" "$UI_LOG" | grep -F -e "${1//…/...} at " -e "${1//…/...} out of sight" | tail -1)
  [ -n "$last" ] && [[ $last != *" out of sight" ]]
}

# ui_browser_wheel down|up: four notches of the mouse wheel over the browser's
# rows (over the last row place the app logged), then the sync. Nothing when
# no row is in sight.
ui_browser_wheel() {
  local where px py
  where=$(grep -E '^Browser (eye |radio )?Root.* at [0-9]+,[0-9]+$' "$UI_LOG" | tail -1 |
    sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  [ -n "$where" ] || return 0
  read -r px py <<< "$where"
  xdotool mousemove $((X + px * UI_SCALE)) $((Y + py * UI_SCALE)) click --repeat 4 --delay 30 \
    "$([ "$1" = up ] && echo 4 || echo 5)"
  ui_sync
}

# ui_browser_reveal "Browser <path>" (or "Browser eye|radio <path>"): scrolls
# the browser's rows with the wheel until that row is in sight, that is,
# until the app logs its place (it logs "... out of sight" for a row that
# scrolls away: BrowserPanel::logLayout). Down first, then up: the row may be
# on either side, and one never logged is below.
ui_browser_reveal() {
  local direction round
  ui_place_valid "$1" && return 0
  for direction in down up; do
    for round in 1 2 3 4 5 6 7 8; do
      ui_browser_wheel "$direction"
      ui_place_valid "$1" && return 0
    done
  done
  return 0
}

# ui_panel_reveal "Panel <command> input <id>": after a sync, a long panel
# scrolled, by wheel notches over its scroll bar, until the input's logged
# place is in its view (the Linux counterpart of Ui-PanelReveal in
# ui-windows-lib.ps1).
# A panel can be higher than its page (Timber's frame with its Corner and
# its fasteners), and a click on a place scrolled away hits whatever is
# there instead. The panel logs its view with its places ("Panel <command>
# view y A to B, scrolled S of M, bar at x,y"; the bar only while it
# scrolls) and logs them again when it scrolls (MainWindow). Nothing for a
# panel that does not scroll or a hidden input.
ui_panel_reveal() {
  local panel
  # Also a row of an inspection's result ("Panel <command> result <name>",
  # mitcad#1010) and the places of the Measure panel's own section
  # ("Measure history"), logged with the panel's; not its
  # buttons, which stay in view under the page.
  # A row's note action and value mark too ("Panel <command> action
  # purge.0", "Panel <command> mark <id>", mitcad#1440): they move with
  # their row when a row above it is laid out again.
  if [[ $1 =~ ^Panel\ (.+)\ (input|action|mark)\ [^\ ]+$ ]] || [[ $1 =~ ^Panel\ (.+)\ result\  ]]; then
    panel=${BASH_REMATCH[1]}
  elif [[ $1 == "Measure "* ]]; then
    panel=Measure
  else
    return 0
  fi
  local view="Panel $panel view y " line place top bottom bx by y count margin=12 i j
  # The places and the view of one settled layout, logged whole: the panel
  # logs its places again 50 ms after a row of it moved (CommandPanel's
  # moved check, which the sync waits for), and a place read before that
  # is where the input was (mitcad#1440: the pencil of a row under a note
  # that wrapped again, clicked 34 px too low on a loaded machine). Also
  # for a panel that does not scroll.
  ui_sync >&2
  for i in $(seq 1 30); do
    line=$(grep -F -- "$view" "$UI_LOG" | tail -1)
    [[ $line =~ \ view\ y\ (-?[0-9]+)\ to\ (-?[0-9]+),\ scrolled\ [0-9]+\ of\ [0-9]+,\ bar\ at\ ([0-9]+),([0-9]+)$ ]] ||
      return 0
    top=${BASH_REMATCH[1]} bottom=${BASH_REMATCH[2]} bx=${BASH_REMATCH[3]} by=${BASH_REMATCH[4]}
    place=$(grep -F -- "$1 at " "$UI_LOG" | tail -1)
    [[ $place =~ \ at\ ([0-9]+),([0-9]+) ]] && [[ $place != *"(hidden)" ]] || return 0
    y=${BASH_REMATCH[2]}
    [ "$y" -ge $((top + margin)) ] && [ "$y" -le $((bottom - margin)) ] && return 0
    count=$(wc -l < "$UI_LOG")
    # Button 5 scrolls the panel down (a place below the view), 4 up.
    xdotool mousemove $((X + bx * UI_SCALE)) $((Y + by * UI_SCALE)) sleep 0.08 \
      click "$([ "$y" -gt $((bottom - margin)) ] && echo 5 || echo 4)"
    for j in $(seq 1 30); do
      grep -qF -- "$view" < <(tail -n +$((count + 1)) "$UI_LOG") && break
      sleep 0.1
    done
    ui_sync >&2
  done
  ui_fail "the panel did not scroll '$1' into its view"
}

# ui_sketch3d_reveal "3D sketch item k5 field": a row or control of the 3D
# Sketch Palette that the card's scrolling put out of sight ("… out of
# sight") is wheeled into it, over a place of the palette in sight: down
# first, up when the card goes no further. The palette logs its places
# again after each scroll that moved them.
ui_sketch3d_reveal() {
  local i j last anchor count button=5 moved
  local places='^3D sketch (item|reference|edit control) .* at [0-9]+,[0-9]+$'
  ui_sync >&2
  for i in $(seq 1 80); do
    last=$(grep -F -e "$1 at " -e "$1 out of sight" "$UI_LOG" | tail -1)
    [[ $last == *" out of sight" ]] || return 0
    anchor=$(grep -E "$places" "$UI_LOG" | tail -1)
    [[ $anchor =~ \ at\ ([0-9]+),([0-9]+)$ ]] || return 0
    count=$(wc -l < "$UI_LOG")
    xdotool mousemove $((X + BASH_REMATCH[1] * UI_SCALE)) $((Y + BASH_REMATCH[2] * UI_SCALE)) sleep 0.08 \
      click "$button"
    moved=0
    for j in $(seq 1 20); do
      tail -n +$((count + 1)) "$UI_LOG" | grep -qE '^3D sketch (item|reference|edit control) ' && { moved=1; break; }
      sleep 0.1
    done
    ui_sync >&2
    if [ "$moved" = 0 ]; then
      [ "$button" = 5 ] || ui_fail "the 3D Sketch Palette did not scroll '$1' into sight"
      button=4
    fi
  done
  ui_fail "the 3D Sketch Palette did not scroll '$1' into sight"
}

# ui_logged_at "text": screen position of the last "<text> at x,y" line. A
# row of the browser that is not in sight is scrolled into sight first
# (ui_browser_reveal), and so is an input, a note action or a mark of a
# long panel scrolled away, after a sync (ui_panel_reveal); the place of anything else that "<text> out of sight"
# says has gone is not given: the test fails instead of clicking where it
# was.
ui_logged_at() {
  local last at px py
  case $1 in
    "Browser "*) ui_browser_reveal "$1" ;;
    "Panel "*" input "*|"Panel "*" action "*|"Panel "*" mark "*|"Panel "*" result "*|"Measure "*)
      ui_panel_reveal "$1" ;;
    "3D sketch item "*|"3D sketch reference "*|"3D sketch edit control "*) ui_sketch3d_reveal "$1" ;;
  esac
  # (A single ellipsis and three dots match each other, mitcad#645.)
  last=$(sed "s/…/.../g" "$UI_LOG" | grep -F -e "${1//…/...} at " -e "${1//…/...} out of sight" | tail -1)
  [[ $last != *" out of sight" ]] || ui_fail "'$1' is out of sight (scrolled away, folded or gone)"
  at=$(sed -n 's/.* at \([0-9]*\),\([0-9]*\).*/\1 \2/p' <<< "$last")
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
  # The sketches started on this plane before (counting every plane's would
  # wait for one too many after a sketch on another plane).
  before=$(grep -c "Sketch started on $plane" "$UI_LOG")
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

# ui_show_origin: the Origin folder's light bulb on, unless it is already
# (mitcad#1030). A command's input shows the origin's point, axes and
# planes in the view only as the light bulb does (Create Sketch's plane
# pick shows the planes whatever it says), so a test that clicks them
# there ("Datum xy", "Datum z") switches it on first. Works during a
# command too (mitcad#295); the caller's mark is kept.
ui_show_origin() {
  local mark=${UI_MARK:-0}
  if ! grep -o "Design origin .*" "$UI_LOG" | tail -1 | grep -q "light bulb on$"; then
    ui_mark
    ui_step "show the origin (light bulb)" ui_click_logged "Browser eye Root/Origin"
    ui_expect_new "Visibility Root/Origin: shown" "the origin shown by its light bulb"
    ui_expect_new "Datum xy at" "the origin in the view"
  fi
  UI_MARK=$mark
}

# ui_pick_origin_row XY: a click on the origin's row (O, X, Y, Z, XY, XZ or
# YZ) in the browser's Origin folder, which a command's active input takes
# as it takes a click in the view, also while the origin is hidden
# (mitcad#1030); the folder is opened first when its rows are folded. The
# view stays as it is (no light bulb switched). The caller's mark is kept.
ui_pick_origin_row() {
  local row="Browser Root/Origin/$1" last mark=${UI_MARK:-0}
  last=$(grep -F -e "$row at " -e "$row out of sight" "$UI_LOG" | tail -1)
  if [[ $last != *" at "* ]]; then
    ui_mark
    ui_step "open the Origin folder"       ui_click_logged "Browser arrow Root/Origin"
    ui_expect_new "$row at " "the origin's rows in the browser"
  fi
  UI_MARK=$mark
  ui_step "pick $1 in the browser"         ui_click_logged "$row"
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

# ui_menu_choose "entry" ["entry of its submenu" ...]: picks an entry of
# the context menu just opened, with the arrow keys, from the logged
# "Context menu: a | b | c"; with more entries, the first opens a submenu
# (Return opens it, its first entry active) whose entries are logged as
# "Context menu <entry>: ...", and so on ("Solid" "More" "Draft": the log's
# "Context menu Solid More: ..."; mitcad#494).
ui_menu_choose() {
  local entries index found path="" level=0 item
  ui_sync
  for wanted in "$@"; do
    entries=$(grep -F "Context menu${path}: " "$UI_LOG" | tail -1 | sed "s/.*Context menu${path}: //")
    IFS='|' read -ra items <<< "$entries"
    index=0
    found=""
    for item in "${items[@]}"; do
      index=$((index + 1))
      item=${item# }
      item=${item% }
      # The labels' single ellipsis (mitcad#645) matches three dots too.
      if [ "${item//…/...}" = "${wanted//…/...}" ]; then
        found=$index
        break
      fi
    done
    [ -n "$found" ] || ui_fail "'$wanted' is not in the context menu${path}: $entries"
    # The menu opens with nothing active; a submenu with its first entry.
    [ $level = 0 ] || found=$((found - 1))
    [ "$found" -le 0 ] || xdotool key --delay 80 --repeat "$found" Down
    ui_key Return
    path="$path $wanted"
    level=$((level + 1))
    [ $level = $# ] || ui_sync
  done
}

# ui_double_click_logged "text": a double-click at the last "<text> at x,y".
ui_double_click_logged() { xdotool mousemove $(ui_logged_at "$1") click --repeat 2 --delay 80 1; }

# ui_double_click_expect "text" "logged line" description: a double-click at
# the last "<text> at x,y" that the app answers with the line; when it does
# not, once more, a moment later. Under load xdotool can send the second
# click too late for a double-click (the app sees two clicks), or the first
# click's pick has not reached the selection when the double-click arrives
# (the view picks with its next frame): then the double-click opens nothing,
# and the selection is there for the second one (mitcad#488).
ui_double_click_expect() {
  local attempt
  for attempt in 1 2; do
    ui_mark
    ui_wait_idle
    ui_double_click_logged "$1"
    ui_sync
    ui_crashed && ui_fail "crashed during the double-click on '$1'"
    for _ in $(seq 1 15); do
      grep -qF -- "$2" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && { echo "ok   $3"; return; }
      sleep 0.2
    done
    echo "note: no '$2' after the double-click on '$1' (attempt $attempt)"
    sleep 0.45 # not a double-click with the clicks before
  done
  ui_fail "$3: '$2' not in the log after two double-clicks"
}

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
UI_DIALOG_TITLE=""
ui_focus_dialog() {
  local dialog=""
  for _ in $(seq 1 50); do
    if [ -n "$UI_INSTANCES" ] && [ -n "$UI_RUNNER" ]; then
      # Several instances (ui_instance): the current one's dialog.
      dialog=$(xdotool search --all --pid "$(ui_app_pid)" --onlyvisible --name "$1" 2> /dev/null | head -1)
    else
      dialog=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
    fi
    [ -n "$dialog" ] && break
    sleep 0.2
  done
  [ -n "$dialog" ] || ui_fail "no dialog matching '$1'"
  # Its title, for ui_type_text.
  UI_DIALOG_TITLE=$(xdotool getwindowname "$dialog" 2> /dev/null)
  ui_sync "$dialog"
  xdotool windowfocus --sync "$dialog" 2> /dev/null
  ui_sync "$dialog"
}
# ui_focus_replace "file dialog title": Qt's own file dialog (Linux; a
# native one asks in its own words) asks before saving over an existing
# file in a message box with the file dialog's own title ("<name> already
# exists. Do you want to replace it?"). Waits for that box, the second
# visible window of that title (the newer, so the higher window id), and
# gives it the keyboard: Alt+Y replaces the file, Escape returns to the
# file dialog.
ui_focus_replace() {
  local title windows box=""
  title=$(ui_title_regex "$1")
  for _ in $(seq 1 50); do
    if [ -n "$UI_INSTANCES" ] && [ -n "$UI_RUNNER" ]; then
      windows=$(xdotool search --all --pid "$(ui_app_pid)" --onlyvisible --name "$title" 2> /dev/null)
    else
      windows=$(xdotool search --onlyvisible --name "$title" 2> /dev/null)
    fi
    [ "$(printf '%s\n' "$windows" | grep -c .)" -ge 2 ] &&
      box=$(printf '%s\n' "$windows" | sort -n | tail -1) && break
    sleep 0.2
  done
  [ -n "$box" ] || ui_fail "no question in '$1' before replacing an existing file"
  UI_DIALOG_TITLE=$1
  ui_sync "$box"
  xdotool windowfocus --sync "$box" 2> /dev/null
  ui_sync "$box"
}
ui_focus_main() {
  ui_sync
  ui_wait_idle
  xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
  ui_sync
}

# ui_title_regex "title": a regex that matches exactly that window title.
ui_title_regex() { printf '^%s$' "$(printf '%s' "$1" | sed 's/[][\.*^$+?(){}|]/\\&/g')"; }

# ui_window_title [window]: the main window's title (UI_WINDOW's) without
# the version after "MitCad" (mitcad#1209): "block.mitcad* - MitCad".
ui_window_title() {
  xdotool getwindowname "${1:-$UI_WINDOW}" 2> /dev/null | sed -E 's/ - MitCad [^ ]+$/ - MitCad/'
}

# ui_typed_since line "prefix"...: the text logged last after that line of
# the log for a typed-in field, in lines "<prefix><text>"
# (MITCAD_LOG_TYPED_TEXT, app/framework/TypedTextLog.hpp), "(nothing
# logged)" if none.
ui_typed_since() {
  local from=$1
  shift
  tail -n +$((from + 1)) "$UI_LOG" | awk -v prefixes="$(printf '%s\n' "$@")" '
    BEGIN { n = split(prefixes, prefix, "\n") }
    { for (i = 1; i <= n; i++) if (prefix[i] != "" && index($0, prefix[i]) == 1) { last = substr($0, length(prefix[i]) + 1); seen = 1 } }
    END { print seen ? last : "(nothing logged)" }'
}

# ui_type_field "dialog title" text: replaces the text of the dialog's field
# that has the keyboard (select all, type) and waits until the app has
# logged exactly that text for it ("File dialog <title>: <text>", "Dialog
# field <title>: <text>"); otherwise (the dialog had not taken the
# keyboard, or a character was lost under load) gives the dialog the
# keyboard again and types once more. No fixed wait (mitcad#102).
ui_type_field() {
  local attempt from typed=""
  for attempt in 1 2; do
    from=$(wc -l < "$UI_LOG")
    ui_key ctrl+a
    xdotool type --delay 20 -- "$2"
    ui_sync
    typed=$(ui_typed_since "$from" "File dialog $1: " "Dialog field $1: ")
    [ "$typed" = "$2" ] && return 0
    echo "note: '$1' has '$typed' after typing '$2' (attempt $attempt)"
    [ "$attempt" = 1 ] && ui_focus_dialog "$(ui_title_regex "$1")"
  done
  ui_fail "typing into '$1': its field has '$typed', not '$2'"
}

# ui_type_text text: ui_type_field in the dialog ui_focus_dialog gave the
# keyboard last.
ui_type_text() { ui_type_field "$UI_DIALOG_TITLE" "$1"; }

# ui_type_path "dialog title" path: a path typed into a file dialog (Qt's
# own, --no-native-dialogs) or another dialog's focused path field, checked
# as ui_type_field does, then Enter. The caller gives the keyboard back
# (ui_focus_main) or goes on in the next dialog.
ui_type_path() {
  ui_focus_dialog "$(ui_title_regex "$1")"
  ui_type_field "$1" "$2"
  ui_key Return
}

# ui_mark; ...; ui_expect_new "text" "description" [seconds]: like
# ui_expect_log, but only lines logged after the mark count; waits up to
# 6 s, or as long as given (slow geometry such as modelled threads). A single
# ellipsis and three dots match each other (mitcad#645).
ui_mark() { UI_MARK=$(wc -l < "$UI_LOG"); }
ui_expect_new() {
  local i=0 busy=0 looks=$((${3:-6} * 5 + 15))
  while [ "$i" -lt "$looks" ]; do
    # Not a pipeline into grep -q: grep stops at the first match, the
    # writer before it then dies of SIGPIPE when more of the log follows,
    # and pipefail (the tests' set -o) made a line found one not found.
    grep -qF -- "${1//…/...}" < <(tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | sed "s/…/.../g") &&
      { echo "ok   $2"; return; }
    if ui_wait_busy "$busy"; then
      busy=$((busy + 1))
      continue
    fi
    i=$((i + 1))
    ui_poll_pause "$i"
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
# text (a click in it, select all, type) and waits until the app has logged
# exactly that text for it ("Panel field <id>: <text>"; for a dialog's
# field logged as "<name> at x,y", "Dialog field <title>: <text>");
# otherwise clicks and types once more (mitcad#102).
ui_type_in() {
  local attempt from typed=""
  for attempt in 1 2; do
    from=$(wc -l < "$UI_LOG")
    ui_click_logged "$1"
    ui_sync
    ui_key ctrl+a
    xdotool type --delay 40 -- "$2"
    ui_sync
    if [[ $1 == "Panel "*" input "* ]]; then
      typed=$(ui_typed_since "$from" "Panel field ${1##* input }: ")
    else
      # A field of a dialog: "Dialog field <title>: <text>".
      typed=$(ui_typed_since "$from" "Dialog field ")
      typed=${typed#*: }
    fi
    [ "$typed" = "$2" ] && return 0
    echo "note: '$1' has '$typed' after typing '$2' (attempt $attempt)"
  done
  ui_fail "typing into '$1': it has '$typed', not '$2'"
}

# ui_choose "Panel <command> input <id>" downs: picks the entry that many
# below the current one in a panel's drop-down (negative: above it).
ui_choose() {
  ui_click_logged "$1"
  ui_sync
  if [ "$2" -gt 0 ]; then
    xdotool key --delay 80 --repeat "$2" Down
  elif [ "$2" -lt 0 ]; then
    xdotool key --delay 80 --repeat $((-$2)) Up
  fi
  ui_key Return
}

# ui_choose_text "Panel <command> input <id>" "text": opens a panel's
# drop-down and chooses the entry whose label starts with the text by the
# list's type-ahead, then Enter.
ui_choose_text() {
  ui_click_logged "$1"
  ui_sync
  ui_type_ahead "$2"
  ui_sync
  ui_key Return
}

# ui_picker_choose "Panel <command> input <id>" "Material picker" "name": a
# click on the picker button of the Material or Appearance tool
# (mitcad#1059) opens its picker (a window titled "Material Picker"); the
# name typed in its search, Enter chooses the first entry that contains
# it, which must be that name; the main window has the keyboard again.
ui_picker_choose() {
  local from line title
  from=$(wc -l < "$UI_LOG")
  ui_click_logged "$1"
  for _ in $(seq 1 40); do
    grep -qF "$2 opened: " < <(tail -n +$((from + 1)) "$UI_LOG") && break
    sleep 0.2
  done
  grep -qF "$2 opened: " < <(tail -n +$((from + 1)) "$UI_LOG") || ui_fail "the $2 did not open"
  title="${2% picker} Picker"
  ui_focus_dialog "^$title\$"
  xdotool type --delay 40 -- "$3"
  for _ in $(seq 1 40); do
    line=$(tail -n +$((from + 1)) "$UI_LOG" | grep -F "$2 search: $3, first " | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ "$line" = "$2 search: $3, first $3" ] || ui_fail "the $2 finds '${line##*first }' first for '$3'"
  ui_key Return
  for _ in $(seq 1 40); do
    if grep -qxF "$2 chose: $3" < <(tail -n +$((from + 1)) "$UI_LOG"); then
      ui_focus_main
      echo "ok   $3 chosen in the $2"
      return
    fi
    sleep 0.2
  done
  ui_fail "the $2 did not choose $3"
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

# ui_update_test_hooks file: ON when the build that made the file (an
# executable in a build folder) reads the update tests' environment
# variables (the CMake option MITCAD_UPDATE_TEST_HOOKS, mitcad#921), OFF
# when it does not; empty when unknown. UI_UPDATE_TEST_HOOKS (a ctest's
# setting) decides first, then the first CMakeCache.txt above the file.
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

ui_finish() {
  # The warning itself, at the start of its line: a test driver's logged
  # step (expect-never 'OCCT view event failed|…') only names it.
  if grep -q "^OCCT view event failed:" "$UI_LOG"; then
    ui_fail "OCCT reported a failed view operation"
  fi
  echo "PASS: $1"
}

# ui_place_on_xy Command: places a part that waits for a pick (mitcad#656:
# Pulley, Gear, Bearing) on the XY plane at the origin: a click on the
# plane while Place On takes picks, then both distances 0 in the panel.
ui_place_on_xy() {
  ui_show_origin
  ui_mark
  ui_step "Place On: the XY plane"         ui_click_logged "Datum xy"
  ui_expect_new "Place $1: face plane xy, " "placed on the XY plane" 30
  ui_step "Distance 1: 0"                  ui_type_in "Panel $1 input at_offset1" "0"
  ui_step "Distance 2: 0"                  ui_type_in "Panel $1 input at_offset2" "0"
  ui_expect_new "$1 Distance 1: 0 = 0 mm" "Distance 1 typed" 30
  ui_expect_new "$1 Distance 2: 0 = 0 mm" "Distance 2 typed" 30
  ui_expect_new "Preview $1: ok" "previewed at the origin" 30
}

# ui_save_state enabled|disabled description: Save's state as the
# quick-access group logged it last ("Quick access file.save enabled",
# "Quick access file.save disabled: no unsaved changes"; mitcad#1090: Save
# is disabled while an unchanged design has nothing to save), waited for.
ui_save_state() {
  local last=""
  for _ in $(seq 1 30); do
    last=$(grep -F "Quick access file.save " "$UI_LOG" | grep -v -e " at " -e " out of sight" | tail -1)
    case "$1:$last" in
      enabled:*" enabled")
        echo "ok   $2"
        return
        ;;
      disabled:*" disabled: "*)
        echo "ok   $2 (${last#*disabled: })"
        return
        ;;
    esac
    sleep 0.2
  done
  ui_fail "$2: Save is not $1 (last: '$last')"
}

# ui_save_if_changed file: Ctrl+S and "Saved <file>" while Save is enabled;
# while the design has nothing to save (Save disabled, mitcad#1090: an
# undo back to the saved state, a step that changed nothing) the file
# already holds it, and nothing is pressed. For tests that save to read the
# file; ui_save_state checks Save's state itself.
ui_save_if_changed() {
  local last
  ui_sync
  last=$(grep -F "Quick access file.save " "$UI_LOG" | grep -v -e " at " -e " out of sight" | tail -1)
  if [[ $last == *" disabled: "* ]]; then
    echo "ok   nothing to save: $1 holds the design"
    return
  fi
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $1" "saved"
}
