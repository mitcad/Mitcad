#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/files/Print3d.cpp app/files/SlicerProcesses.cpp
# check-all sources: app/framework/CommandSession.cpp tools/cli
# Send to Slicer's Use the open slicer window (mitcad#1162) through the
# real UI, with fake slicers: a script that logs each start with the files
# it was given and stays running as a "window" until the test ends it;
# started with --single-instance while another of its windows runs, it
# hands the files over (logs "forwarded") and exits, as PrusaSlicer does.
#   1. The box on (the default): two sends of one design start one
#      window, with --single-instance, and the second hands it the file;
#      one window runs.
#   2. Another design (Save As) starts a window of its own, without the
#      switch (another window of the program runs).
#   3. The box off (clicked, remembered): every send starts a new window.
#   4. After a restart the box is still off; turned on again, a send of
#      a design whose window this session did not start starts one.
#   5. A program without a hand-over ("Fake slicer"): the second send
#      starts nothing, reuses the window (brought to the front or not:
#      Xvfb has no window manager) and says that the file was updated.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-slicer-window-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-slicer-window.XXXXXX")
FAKE=$WORK/fake
# The fake slicers' windows end when the stop file appears.
trap 'touch "$FAKE/stop"; ui_cleanup; sleep 0.5; rm -rf "$WORK"' EXIT
FILE=$WORK/plate.mitcad
OTHER=$WORK/other.mitcad
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR" "$FAKE/windows"
LOG=$FAKE/starts.log
touch "$LOG"

# The fake slicer: one log line per start, "<pid> window: <arguments>" or
# "<pid> forwarded to <pid>: <arguments>".
cat > "$WORK/fake-slicer.sh" << 'EOF'
#!/bin/sh
DIR=@FAKE@
case " $* " in
  *" --single-instance "*)
    for f in "$DIR"/windows/*; do
      [ -e "$f" ] || continue
      p=${f##*/}
      if [ "$p" != "$$" ] && kill -0 "$p" 2> /dev/null; then
        echo "$$ forwarded to $p: $*" >> "$DIR/starts.log"
        exit 0
      fi
    done
    ;;
esac
touch "$DIR/windows/$$"
echo "$$ window: $*" >> "$DIR/starts.log"
n=0
while [ ! -e "$DIR/stop" ] && [ $n -lt 3000 ]; do sleep 0.2; n=$((n + 1)); done
rm -f "$DIR/windows/$$"
EOF
sed -i "s|@FAKE@|$FAKE|" "$WORK/fake-slicer.sh"
chmod +x "$WORK/fake-slicer.sh"
cp "$WORK/fake-slicer.sh" "$WORK/other-slicer.sh"

# settings NAME PROGRAM [reuse]: the slicer in the settings.
settings() {
  mkdir -p "$XDG_CONFIG_HOME/Mitcad"
  printf '[print]\nslicerName=%s\nslicerProgram=%s\nformat=3mf\n%s' "$1" "$2" "${3:+reuseWindow=$3
}" > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
}
settings "PrusaSlicer (fake)" "$WORK/fake-slicer.sh"

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# windows: how many fake slicer windows run.
windows() {
  local n=0 f
  for f in "$FAKE"/windows/*; do
    [ -e "$f" ] && kill -0 "${f##*/}" 2> /dev/null && n=$((n + 1))
  done
  echo $n
}
expect_windows() {
  [ "$(windows)" = "$1" ] || ui_fail "$2: $(windows) slicer windows run, expected $1"
  echo "ok   $2"
}
# started_pid: the pid of the last "Send to Slicer: started …" line.
started_pid() {
  grep -F "Send to Slicer: started " "$UI_LOG" | tail -1 | sed -n 's/.*(pid \([0-9]*\)).*/\1/p'
}
# expect_start "pattern" description: waits for a line of the fake's log.
expect_start() {
  for _ in $(seq 1 50); do
    grep -qE -- "$1" "$LOG" && { echo "ok   $2"; return 0; }
    sleep 0.2
  done
  cat "$LOG"
  ui_fail "$2: no '$1' in the fake slicer's log"
}
# send: Send to Slicer through the search, then Send (Enter).
send() {
  ui_mark
  ui_command "Send to Slicer"
  ui_expect_new "Panel Send to Slicer input format at " "the panel"
  ui_key Return
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"

echo "--- 1. The box on: one window, the second file handed over"
ui_step "send"                             send
ui_expect_new "3D Print open slicer window: on" "the box is on by default"
ui_expect_new "Send to Slicer: started PrusaSlicer (fake) (pid " "a slicer window started"
FIRST=$(started_pid)
expect_start "^$FIRST window: --single-instance .*/plate\\.3mf\$" \
  "the first window took --single-instance and the file"
ui_step "send again"                       send
ui_expect_new "Send to Slicer: reused PrusaSlicer (fake) window of plate (pid $FIRST): the file handed over" \
  "the second send reused the window"
ui_expect_new "3D Print last line: Sent 1 body to the PrusaSlicer (fake) window of plate." "the last line says so"
expect_start "^[0-9]+ forwarded to $FIRST: --single-instance .*/plate\\.3mf\$" "the window was handed the file"
expect_windows 1 "one slicer window runs"

echo "--- 2. Another design starts its own window"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
ui_step "type the path, Enter"             ui_type_path "Save As" "$OTHER"
ui_expect_log "Saved $OTHER" "saved as another design"
ui_focus_main
ui_step "send"                             send
ui_expect_new "Send to Slicer: started PrusaSlicer (fake) (pid " "the other design's window started"
SECOND=$(started_pid)
[ "$SECOND" != "$FIRST" ] || ui_fail "the other design reused the first design's window"
expect_start "^$SECOND window: .*/other\\.3mf\$" "the other design's window took its file"
grep -qE "^$SECOND window: --single-instance" "$LOG" &&
  ui_fail "the other design's window took --single-instance while another window runs"
echo "ok   without --single-instance: another window of the program runs"
expect_windows 2 "two slicer windows run"

echo "--- 3. The box off: a new window every time"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer input format at " "the panel"
ui_step "Use the open slicer window"       ui_click_logged "Panel Send to Slicer input reuse"
ui_expect_new "3D Print open slicer window: off" "the box turned off"
ui_key Return
ui_expect_new "Send to Slicer: started PrusaSlicer (fake) (pid " "a new window although one is open"
THIRD=$(started_pid)
expect_start "^$THIRD window: .*/other\\.3mf\$" "the new window took the file"
ui_step "send again"                       send
ui_expect_new "3D Print open slicer window: off" "the box stays off"
ui_expect_new "Send to Slicer: started PrusaSlicer (fake) (pid " "another new window"
expect_windows 4 "four slicer windows run"
ui_stop_app

echo "--- 4. Remembered after a restart; a window of another session is not reused"
grep -q '^reuseWindow=false' "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf" || ui_fail "the box off is not remembered"
echo "ok   the box off in the settings"
ui_start_app --open "$OTHER"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer input format at " "the panel"
ui_expect_new "3D Print open slicer window: off" "the box still off"
ui_step "Use the open slicer window"       ui_click_logged "Panel Send to Slicer input reuse"
ui_expect_new "3D Print open slicer window: on" "the box turned on"
ui_key Return
ui_expect_new "Send to Slicer: started PrusaSlicer (fake) (pid " \
  "the windows the last session started are not reused"
expect_windows 5 "five slicer windows run"
ui_stop_app

echo "--- 5. A program without a hand-over: the window reused, the file updated"
touch "$FAKE/stop"
for _ in $(seq 1 50); do [ "$(windows)" = 0 ] && break; sleep 0.2; done
expect_windows 0 "the windows ended"
rm -f "$FAKE/stop"
settings "Fake slicer" "$WORK/other-slicer.sh" true
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"
ui_step "send"                             send
ui_expect_new "Send to Slicer: started Fake slicer (pid " "a window started"
FOURTH=$(started_pid)
expect_start "^$FOURTH window: .*/plate\\.3mf\$" "the window took the file, without --single-instance"
grep -qE "^$FOURTH window: --single-instance" "$LOG" && ui_fail "a program without a hand-over took --single-instance"
ui_step "send again"                       send
ui_expect_new "Send to Slicer: reused Fake slicer window of plate (pid $FOURTH): " "the window reused"
ui_expect_new "3D Print last line: Fake slicer is already open with plate: the file was updated" \
  "the last line says that the file was updated"
[ "$(grep -c " window: .*/plate\\.3mf\$" "$LOG")" = 2 ] || ui_fail "a send started another window: $(cat "$LOG")"
grep -q "forwarded to $FOURTH" "$LOG" && ui_fail "the program was started again to hand the file over"
echo "ok   nothing started"
expect_windows 1 "one slicer window runs"
[ -f "$TMPDIR/mitcad-print/plate/plate.3mf" ] || ui_fail "the file was not written again"
echo "ok   the file rewritten in place"

ui_finish "UI slicer window"
