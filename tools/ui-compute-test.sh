#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJobs.cpp app/framework/ComputeProgress.cpp app/framework/ComputeProgress.hpp
# check-all sources: app/framework/WaitCursor.cpp app/framework/WaitCursor.hpp
# Background computation (P7) through the real UI. Every feature a job
# evaluates is made slower (MITCAD_TEST_RECOMPUTE_DELAY_MS: 400 ms, in
# some jobs more), so a six-feature design takes seconds to compute:
#   1. Open through the dialog: the design is computed on the worker thread,
#      the progress dialog shows after 400 ms with the feature being
#      computed, Esc cancels, and the document before stays.
#   2. Open again, to the end: the design is shown.
#   3. Edit Parameters: a change computed with the progress dialog over
#      the Parameters window, cancelled there (the parameter, the volume and
#      the undo steps stay), then done; undo.
#   4. Editing a feature: its preview computes without a progress dialog
#      and the window takes input meanwhile (mitcad#634); Esc cancels the
#      preview with the command (nothing shown, nothing edited); OK while
#      a preview computes waits for it and makes the edit; a newer choice
#      cancels the preview, which is made again for it; a number typed
#      while a preview computes replaces the pending value without a
#      cancel and is previewed after it.
#   5. --open at start-up: the dialog shows before the window; cancelled,
#      the window starts with an empty document.
#   6. Undo that computes (its results gone from the cache after eight
#      changes): cancelled, the document stays; undone the next time.
#   7. The result store (P7d): the design opened twice, the second time
#      the solid features come from the store (the sketches are evaluated),
#      and a change computes on them; Help > Diagnostics (Clear Memory,
#      Export Report); Preferences' Cache: Clear, and the store off.
#   8. A long kernel operation (P7e), without the test delay: the preview
#      of a modelled thread, cancelled with Esc while OCCT sweeps and cuts
#      the groove, stops within a second of the request; OK while its
#      preview computes makes the thread.
#   9. The wait cursor (mitcad#268): every wait's "Wait cursor on" has
#      its "off" (the progress dialog's or the end's), none is left on
#      once idle; a wait nested in a wait (a test hook) with its progress
#      dialog; a cursor left on (a test hook) is cleared once idle.
#  10. A preview interrupted with its panel open (the worker control of
#      the in-process test driver, tools/ui-scripts/
#      compute-interrupted-preview.mitcad-ui: the window has no control
#      that stops only a preview): it shows nothing; OK computes the edit
#      itself, in its progress dialog, cancelled there inside the kernel;
#      each cancel stops within a second; then OK computes the thread.
#
# The result store is off (ui-test-lib.sh) but in part 7.
#
# Runs headless on Xvfb (see ui-test-lib.sh). The app uses Qt's own file
# dialog (--no-native-dialogs), so a path can be typed into it.
# Usage: tools/ui-compute-test.sh
# check-all sources: tools/cli
# check-all sources: app/framework/CommandSession.cpp app/framework/DocumentSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/compute-interrupted-preview.mitcad-ui

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-compute.XXXXXX)
FILE=$WORK/boss.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# A change of d1 computes five features (3 s), a preview one (1.5 s).
export MITCAD_TEST_RECOMPUTE_DELAY_MS=400,set_parameter=600,preview=1500
# The wait cursor's test hooks (mitcad#268): Nested Wait (Test) and Leave
# Wait Cursor On (Test).
export MITCAD_TEST_WAIT_CURSOR=1

# Six features: a block, a boss joined onto it, a hole cut, a fillet.
cat > "$FILE" << 'EOF'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 },
    { "name": "d3", "value": 20.0 }, { "name": "d4", "value": 20.0 },
    { "name": "d5", "value": 10.0 }, { "name": "d6", "value": 10.0 },
    { "name": "d7", "value": 30.0 }, { "name": "d8", "value": 30.0 },
    { "name": "d9", "value": 2.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "sketch", "name": "Sketch2", "shapes": [
      { "type": "rectangle", "corner": [10.0, 10.0], "width": "d4", "height": "d5" },
      { "type": "circle", "center": [45.0, 20.0], "diameter": "d6" } ] },
    { "type": "extrude", "name": "Extrude2", "sketch": "Sketch2", "profile": 0,
      "distance": "d7", "operation": "join", "body": "Extrude1" },
    { "type": "extrude", "name": "Extrude3", "sketch": "Sketch2", "profile": 1,
      "distance": "d8", "operation": "cut", "body": "Extrude1" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude2", "role": "side", "index": 1 } ], "radius": "d9" }
  ]
}
EOF

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(ui_window_title)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

# open_through_dialog: File > Open (Ctrl+O), the path typed into Qt's dialog.
open_through_dialog() {
  ui_key ctrl+o
  ui_type_path "Open" "$FILE"
}

# edit_cell "name column" text: double-click a cell of Edit Parameters,
# replace its text, Enter.
edit_cell() {
  ui_focus_dialog '^Parameters$'
  ui_double_click_logged "Parameters $1"
  ui_sync
  ui_type_field "Parameters" "$2"
  ui_key Return
}

# cancel_job label: Esc in the progress dialog of the job named label.
cancel_job() {
  ui_expect_new "Progress dialog shown: $1" "the progress dialog of $1"
  ui_focus_dialog '^Computing$'
  ui_key Escape
  ui_expect_new "Computing $1 cancelled after" "$1 cancelled" 10
}

# wait_cursor_balanced description: in this run's log, every "Wait cursor
# on: <job> (depth N)" has its "Wait cursor off: <job> (depth N)" after it,
# no wait cursor was left on, and the idle check after them found none
# (mitcad#268).
wait_cursor_balanced() {
  local unbalanced
  grep -q '^Wait cursor on: ' "$UI_LOG" || ui_fail "$1: no wait cursor logged"
  unbalanced=$(awk '
    /^Wait cursor on: / { key = substr($0, 17); open[key]++ }
    /^Wait cursor off: / { key = substr($0, 18); if (--open[key] < 0) { print "off before on: " key; exit } }
    END { for (key in open) if (open[key] != 0) print open[key] " unbalanced: " key }' "$UI_LOG")
  [ -z "$unbalanced" ] || ui_fail "$1: $unbalanced"
  grep -q "Wait cursor left on after" "$UI_LOG" && ui_fail "$1: $(grep "Wait cursor left on after" "$UI_LOG" | head -1)"
  local idle=""
  for _ in $(seq 1 15); do
    idle=$(awk '/^Wait cursor off: / { seen = 0 } /^Wait cursor at idle: none set$/ { seen = 1 } END { print seen + 0 }' "$UI_LOG")
    [ "$idle" = 1 ] && break
    sleep 0.2
  done
  [ "$idle" = 1 ] || ui_fail "$1: no idle check found the cursor gone after the last wait"
  echo "ok   $1: $(grep -c '^Wait cursor on: ' "$UI_LOG") wait cursors, each restored once"
}

# no_new "text" description: the text was not logged since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# none_before "text" "later" description: since the mark, the text was not
# logged before the first line with "later" (which was logged).
none_before() {
  local lines
  lines=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -nF -m1 -- "$2" | cut -d: -f1)
  [ -n "$lines" ] || ui_fail "$3: '$2' not in the log since the mark"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | head -n "$lines" | grep -qF -- "$1" && ui_fail "$3"
  echo "ok   $3"
}

# stopped_soon label: the job named label stopped within a second of its
# cancel request (both logged since the mark: the progress dialog's Cancel
# or, for a job without one such as a preview, the cancel itself).
stopped_soon() {
  local requested stopped
  ui_expect_new "Computing $1 cancelled after" "$1 cancelled" 30
  requested=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n "s/.*Computing $1: cancel requested after \([0-9]*\) ms.*/\1/p" | tail -1)
  stopped=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n "s/.*Computing $1 cancelled after \([0-9]*\) ms.*/\1/p" | tail -1)
  [ -n "$requested" ] && [ -n "$stopped" ] || ui_fail "$1: no times of the cancel logged"
  [ $((stopped - requested)) -lt 1000 ] ||
    ui_fail "$1 stopped $((stopped - requested)) ms after the cancel request (at $requested ms)"
  echo "ok   $1 stopped $((stopped - requested)) ms after the cancel request (at $requested ms)"
}

# expect_volume_change mm3 description: Body1's volume changed by that much
# (the last change logged since the mark).
expect_volume_change() {
  local change=""
  for _ in $(seq 1 50); do
    change=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n 's/.*Body Body1 (F2.b0): volume \([0-9.]*\) -> \([0-9.]*\) mm3.*/\1 \2/p' | tail -1 |
      awk '{ printf "%.3f\n", $2 - $1 }')
    [ -n "$change" ] && break
    sleep 0.2
  done
  [ -n "$change" ] || ui_fail "$2: no volume change logged"
  awk -v c="$change" -v e="$1" 'BEGIN { exit !((c - e) ^ 2 < 1e-4) }' ||
    ui_fail "$2: the volume changed by $change mm3, expected $1"
  echo "ok   $2: $change mm3"
}

ui_start_display

echo "--- Open, cancelled in the progress dialog"
ui_start_app --no-native-dialogs
expect_title "Untitled - MitCad"
ui_mark
ui_step "open boss.mitcad (Ctrl+O)"      open_through_dialog
ui_expect_new "Computing boss.mitcad started" "opening is a job"
ui_expect_new "Progress dialog shown: boss.mitcad" "the progress dialog after 400 ms"
ui_expect_new "Computing boss.mitcad: 2 of 6, Extrude1" "the progress names the feature"
ui_focus_dialog '^Computing$'
ui_key Escape # not a step: it waits for the job
ui_expect_new "Computing boss.mitcad: cancel requested" "Esc asks the job to stop"
ui_expect_new "Computing boss.mitcad cancelled after" "the job stopped"
ui_expect_new "Opening boss.mitcad cancelled" "the open was cancelled"
ui_focus_main
expect_title "Untitled - MitCad"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Opened " && ui_fail "opened after the cancel"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Computing boss.mitcad done" &&
  ui_fail "the job ended after the cancel"
xdotool search --onlyvisible --name '^Computing$' > /dev/null 2>&1 && ui_fail "the progress dialog stayed"
echo "ok   the progress dialog went"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Bodies shown" && ui_fail "bodies shown after the cancel"
ui_step "fit (F6)"                       ui_key F6

echo "--- Open again, to the end"
ui_mark
ui_step "open boss.mitcad (Ctrl+O)"      open_through_dialog
ui_expect_new "Progress dialog shown: boss.mitcad" "the progress dialog again"
ui_expect_new "Computing boss.mitcad: 6 of 6, Fillet1" "up to the last feature" 10
ui_expect_new "Computing boss.mitcad done in" "the job ran to its end" 10
ui_expect_new "(6 evaluated)" "all six features evaluated"
ui_expect_new "Recomputed 6 feature(s)" "the recompute reported"
ui_expect_new "Opened $FILE" "opened"
ui_expect_new "Bodies shown: F2.b0" "the design is shown"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Recompute failed" && ui_fail "a feature failed"
ui_focus_main
expect_title "boss.mitcad - MitCad"
ui_step "fit (F6)"                       ui_key F6

echo "--- Edit Parameters: cancelled over the Parameters window, then done"
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters d1 expression at" "the Parameters window"
ui_mark
ui_step "d1 = 70"                        edit_cell "d1 expression" "70"
ui_expect_new "Computing set_parameter started" "the change is a job"
cancel_job set_parameter
ui_expect_new "Parameter d1 expression: 70 cancelled" "the change was cancelled"
sleep 1
no_new "Body Body1 (F2.b0): volume" "the volume stays"
no_new "Parameters: d1 = 70" "d1 stays 60"
no_new "Recompute failed" "no failure shown"
ui_mark
ui_step "d1 = 70 again"                  edit_cell "d1 expression" "70"
ui_expect_new "Progress dialog shown: set_parameter" "the progress dialog again"
ui_expect_new "Computing set_parameter done in" "computed to the end" 10
ui_expect_new "Parameter d1 expression: 70 = 70 mm" "d1 = 70"
expect_volume_change 8000 "the block 10 mm wider"
ui_focus_dialog '^Parameters$'
ui_step "OK"                             ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "the change undone"
ui_mark
ui_step "undo again (Ctrl+Z)"            ui_key ctrl+z
sleep 1
no_new "Undo: " "the cancelled change made no undo step"

# A feature's preview computes in the background without a progress dialog
# (mitcad#634): the steps below that give input while it computes are not
# ui_step, which would wait for it.
echo "--- Editing a feature: its preview computes without a dialog, Esc cancels it with the command"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing Extrude1"
ui_expect_new "Panel Extrude input distance at" "its panel"
ui_wait_idle # the panel's first preview (1.5 s here) ends before the mark
ui_mark
ui_step "distance 25"                    ui_type_in "Panel Extrude input distance" "25"
ui_expect_new "Computing preview started" "the preview is a job"
ui_key Escape # Esc while the preview computes
ui_expect_new "Computing preview: cancel requested after" "Esc asks the preview to stop"
ui_expect_new "Computing preview cancelled after" "the preview stopped" 10
ui_expect_new "Command Extrude cancelled" "the command cancelled with it"
no_new "Progress dialog shown: preview" "no progress dialog for the preview"
no_new "Preview Extrude: ok" "nothing previewed"
sleep 1
no_new "Edited F2" "nothing edited"
no_new "Body Body1 (F2.b0): volume" "the volume stays"

echo "--- OK while the preview computes: the edit waits for it"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing Extrude1 again"
ui_wait_idle # the panel's first preview ends before the mark
ui_mark
ui_step "distance 25"                    ui_type_in "Panel Extrude input distance" "25"
ui_expect_new "Computing preview started" "the preview computes"
ui_key Return # OK while the preview computes
ui_expect_new "Computing preview done in" "the preview ran to its end" 10
ui_expect_new "Edited F2" "Extrude1 edited" 10
ui_expect_new "Body Body1 (F2.b0): volume" "the volume changed"
no_new "Computing preview cancelled" "OK did not cancel the preview"
no_new "Progress dialog shown: preview" "no progress dialog for the preview"

echo "--- A preview cancelled by a newer choice, and made again for it"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing Extrude1 a third time"
ui_wait_idle # the panel's first preview ends before the mark
ui_mark
ui_step "distance 30"                    ui_type_in "Panel Extrude input distance" "30"
ui_expect_new "Computing preview started" "the preview computes"
ui_choose "Panel Extrude input direction" 2 # Symmetric, while the preview computes
ui_expect_new "Computing preview cancelled after" "the newer choice cancelled the preview" 10
ui_expect_new "Extrude: Direction = Symmetric" "Symmetric chosen"
ui_expect_new "Preview Extrude: ok" "previewed again" 10
none_before "Preview Extrude: ok" "Computing preview cancelled after" "the cancelled preview showed nothing"
none_before "Preview Extrude: intermediate" "Computing preview cancelled after" "nor its intermediate result"
no_new "Progress dialog shown: preview" "no progress dialog for the previews"

echo "--- A number typed while the preview computes: no cancel, previewed after it"
ui_wait_idle
ui_mark
ui_step "distance 31"                    ui_type_in "Panel Extrude input distance" "31"
ui_expect_new "Computing preview started" "the preview computes"
ui_type_in "Panel Extrude input distance" "32" # while the preview computes
ui_expect_new "Preview Extrude: ok" "the latest value previewed" 10
ui_wait_idle
[ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c 'Computing preview started$')" -ge 2 ] ||
  ui_fail "the number typed during the preview was not previewed after it"
echo "ok   previewed again after the first one"
no_new "Computing preview cancelled" "the number did not cancel the preview"
ui_focus_main
ui_mark
ui_step "cancel the command (Esc)"       ui_key Escape
ui_expect_new "Command Extrude cancelled" "the command cancelled"
sleep 1
no_new "Body Body1 (F2.b0): volume" "the volume stays"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "The document was read while a job computes it" &&
  ui_fail "the document was read during a job"
ui_crashed && ui_fail "crashed"
wait_cursor_balanced "the jobs with progress dialogs, cancels and previews"
ui_stop_app

echo "--- --open at start-up: the dialog before the window"
ui_start_app --open "$FILE"
ui_expect_log "Progress dialog shown: boss.mitcad" "the progress dialog at start-up"
ui_expect_log "Opened $FILE" "opened with --open"
expect_title "boss.mitcad - MitCad"
ui_stop_app

echo "--- --open at start-up, cancelled"
# Slower, so that the dialog is still there when Esc comes.
if [ "${UI_GDB:-0}" = 1 ] && command -v gdb > /dev/null; then
  MITCAD_TEST_RECOMPUTE_DELAY_MS=1500 gdb -q -batch -ex "set debuginfod enabled off" -ex run -ex bt \
    --args "$UI_APP" --no-recovery --chrome=docked --open "$FILE" > "$UI_LOG" 2>&1 &
else
  MITCAD_TEST_RECOMPUTE_DELAY_MS=1500 "$UI_APP" --no-recovery --chrome=docked --open "$FILE" > "$UI_LOG" 2>&1 &
fi
UI_RUNNER=$!
ui_focus_dialog '^Computing$'
ui_key Escape # not a step: it waits for the job
ui_expect_log "Opening boss.mitcad cancelled" "the open at start-up was cancelled"
for _ in $(seq 1 50); do
  UI_WINDOW=$(xdotool search --onlyvisible --name ' - MitCad [^ ]+$' 2> /dev/null | head -1)
  [ -n "$UI_WINDOW" ] && break
  sleep 0.2
done
[ -n "$UI_WINDOW" ] || ui_fail "the window did not appear after the cancel"
expect_title "Untitled - MitCad"
grep -q "Could not open" "$UI_LOG" && ui_fail "a cancel reported as an error"
ui_crashed && ui_fail "crashed after the cancel"
echo "ok   the window started with an empty document"
ui_stop_app

echo "--- Undo that computes: cancelled, then done"
# Eight changes of d1 at start-up push the results for d1 = 60 out of the
# model's cache (eight per feature), so undoing back to it computes five
# features again. Only undo is slowed down.
export MITCAD_TEST_RECOMPUTE_DELAY_MS=0,undo=800
changes=()
for value in 61 62 63 64 65 66 67 68; do
  changes+=(--set "d1=$value")
done
ui_start_app --open "$FILE" "${changes[@]}"
for _ in $(seq 1 50); do
  [ "$(grep -c 'Body Body1 (F2.b0): volume' "$UI_LOG")" -ge 8 ] && break
  sleep 0.2
done
[ "$(grep -c 'Body Body1 (F2.b0): volume' "$UI_LOG")" -ge 8 ] || ui_fail "the eight changes were not made"
echo "ok   d1 changed eight times"
ui_mark
for _ in 1 2 3 4 5 6 7; do
  ui_key ctrl+z
  sleep 0.4
done
for _ in $(seq 1 25); do
  [ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c 'Undo: ')" -ge 7 ] && break
  sleep 0.2
done
[ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c 'Undo: ')" -ge 7 ] || ui_fail "seven undos expected"
no_new "Computing undo started" "seven undos from the cache"
ui_mark
ui_step "undo to d1 = 60 (Ctrl+Z)"      ui_key ctrl+z
cancel_job undo
sleep 1
no_new "Undo: " "the undo was cancelled"
ui_focus_main
ui_mark
ui_step "undo again (Ctrl+Z)"            ui_key ctrl+z
ui_expect_new "Computing undo done in" "undone the next time" 10
ui_expect_new "Undo: " "undo"
grep -q "The document was read while a job computes it" "$UI_LOG" && ui_fail "the document was read during a job"
ui_crashed && ui_fail "crashed"
ui_stop_app

echo "--- The result store (P7d): opened again, the solid features come from it"
# Every result is stored, however quick.
export MITCAD_TEST_RECOMPUTE_DELAY_MS=0
export MITCAD_RESULT_STORE=$UI_RESULTS MITCAD_RESULT_STORE_MIN_MS=0
ui_start_app --open "$FILE"
ui_expect_log "Computing boss.mitcad done in" "opened"
ui_expect_log "(6 evaluated)" "all six features evaluated"
ui_expect_log "Result store: restored 0, evaluated 6; stored 4 " "the four solid features stored"
ui_stop_app
[ "$(find "$UI_RESULTS" -name '*.mrs' | wc -l)" = 4 ] || ui_fail "four result files expected in $UI_RESULTS"
echo "ok   four files in the store"
ui_start_app --open "$FILE"
ui_expect_log "Computing boss.mitcad done in" "opened again"
ui_expect_log "(2 evaluated)" "only the two sketches evaluated"
ui_expect_log "Result store: restored 4, evaluated 2; stored 0 " "the solid features from the store"
ui_expect_log "Bodies shown: F2.b0" "the design is shown"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a feature failed"
ui_mark
ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
ui_expect_new "Parameters d1 expression at" "the Parameters window"
ui_mark
ui_step "d1 = 61"                        edit_cell "d1 expression" "61"
expect_volume_change 800 "the block computed from the stored results 1 mm wider"
ui_focus_dialog '^Parameters$'
ui_step "OK"                             ui_click_logged "Parameters OK"
ui_focus_main
echo "--- Help > Diagnostics: the caches, Clear Memory, Export Report"
ui_mark
ui_step "diagnostics (search)"           ui_command "Diagnostics"
ui_expect_new "Diagnostics: memory " "the diagnostics dialog"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -m1 "Diagnostics: memory ")
echo "     $line"
case $line in
  *"(5 files)"*"last recompute 5 evaluated, 0 from the store, 1 cached"*) echo "ok   the store's files and the last change" ;;
  *) ui_fail "unexpected diagnostics: $line" ;;
esac
ui_focus_dialog '^Diagnostics$'
ui_step "clear memory (Alt+M)"           ui_key alt+m
ui_expect_new "Diagnostics: cleared memory: " "Clear Memory"
ui_step "export the report (Alt+E)"      ui_key alt+e
ui_type_path "Export Report" "$WORK/report.json"
ui_expect_new "Diagnostics report written to $WORK/report.json" "the report written"
grep -q '"last_recompute"' "$WORK/report.json" && grep -q '"by_type"' "$WORK/report.json" ||
  ui_fail "the report has no recompute or sizes"
echo "ok   the report has the recompute and the sizes"
ui_focus_dialog '^Diagnostics$'
ui_key Escape
ui_focus_main
echo "--- Preferences, Cache: Clear, and the store off"
ui_mark
ui_step "preferences, Cache (search)"    ui_command "Preferences: Cache"
ui_focus_dialog '^Preferences$'
ui_step "clear the results (Alt+L)"      ui_key alt+l
ui_expect_new "Result store cleared: 5 files" "Clear removed the five files"
[ -z "$(find "$UI_RESULTS" -name '*.mrs')" ] || ui_fail "result files are left"
ui_step "keep no results (Alt+K)"        ui_key alt+k
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: results on disk off, at most 5120 MB" "results on disk off"
grep -q '^disk=false' "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf" || ui_fail "the setting is not saved"
echo "ok   saved off"
ui_crashed && ui_fail "crashed"
ui_stop_app
ui_start_app --open "$FILE"
ui_expect_log "(6 evaluated)" "with the store off, all six evaluated"
grep -q "Result store:" "$UI_LOG" && ui_fail "the store was used while off"
echo "ok   no store"
unset MITCAD_RESULT_STORE_MIN_MS
export MITCAD_RESULT_STORE=off
ui_stop_app

echo "--- A long kernel operation (P7e): cancelled inside it"
# No test delay: the groove of a modelled thread on a 150 mm rod takes
# seconds (4.5 s with OCCT's release libraries, which the Linux debug
# build links), in OCCT's sweeps, the grooves' common and the boolean cut,
# which a cancel stops inside (the common could not be stopped before
# mitcad#102: a cancel a second and a half into the preview waited for it).
export MITCAD_TEST_RECOMPUTE_DELAY_MS=0
cat > "$WORK/rod.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 150}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
    "thread": {"designation": "M10x1.5", "class": "6g"}}}
]
EOF
"$CLI" run "$WORK/rod.json" --save "$WORK/rod.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli rod"; }

ui_start_app --open "$WORK/rod.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_step "edit Thread1 (double-click)"    ui_double_click_logged "Timeline Thread1"
ui_expect_log "Editing F3 with Thread" "editing Thread1"
ui_wait_idle
ui_mark
ui_step "Modeled"                        ui_click_logged "Panel Thread input modeled"
ui_expect_new "Thread: Modeled = on" "modelled"
ui_expect_new "Computing preview started" "the preview computes" 30
sleep 1
# Not ui_focus_main, which waits for the preview: the panel has the keyboard.
ui_key Escape # Esc a second into the preview
stopped_soon preview
ui_expect_new "Command Thread cancelled" "the command cancelled with it"
no_new "Progress dialog shown: preview" "no progress dialog for the preview"
no_new "Preview Thread: ok" "nothing previewed"
no_new "Edited F3" "Thread1 stays cosmetic"
ui_step "edit Thread1 (double-click)"    ui_double_click_logged "Timeline Thread1"
ui_expect_new "Editing F3 with Thread" "editing Thread1 again"
ui_wait_idle
ui_mark
ui_step "Modeled"                        ui_click_logged "Panel Thread input modeled"
ui_expect_new "Computing preview started" "the preview computes" 30
ui_key Return # OK while the preview computes
ui_expect_new "Computing preview done in" "the preview ran to its end" 120
ui_expect_new "Edited F3" "OK made the thread" 120
# What the cancel stopped short was not kept: the groove is cut in.
ui_expect_volume "Body Body1 (F2.b0): volume 11780.972 -> \([0-9.]*\) mm3" 9771.222684120039 \
  "the groove cut in" 1e-3
grep -q "The document was read while a job computes it" "$UI_LOG" && ui_fail "the document was read during a job"
ui_crashed && ui_fail "crashed"
# The previews take no wait cursor (no job here waits for the worker);
# the nested waits below check this log's wait cursors.

echo "--- The wait cursor (mitcad#268): a wait nested in a wait, with its progress dialog"
# Nested Wait (Test): a job of two seconds; 200 ms into its wait, while
# that holds the cursor, a second wait for it starts inside the first
# one's loop and shows the progress dialog after its own 400 ms.
ui_mark
ui_step "nested wait (search)"            ui_command "Nested Wait"
ui_expect_new "Wait cursor on: nested wait test (depth 1)" "the wait's cursor"
ui_expect_new "Wait cursor on: nested wait test (nested) (depth 2)" "the nested wait's cursor"
ui_expect_new "Progress dialog shown: nested wait test (nested)" "the nested wait's progress dialog"
ui_expect_new "Wait cursor off: nested wait test (nested) (depth 2)" "its cursor gone with the dialog"
ui_expect_new "Computing nested wait test done in" "the job done" 10
ui_expect_new "Wait cursor off: nested wait test (depth 1)" "the first wait's cursor gone at its end"
lines=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E '^(Wait cursor (on|off): nested|Progress dialog shown: nested)' |
  sed -E 's/^(Wait cursor|Progress dialog) //' | tr '\n' ';')
[ "$lines" = "on: nested wait test (depth 1);on: nested wait test (nested) (depth 2);off: nested wait test (nested) (depth 2);shown: nested wait test (nested);off: nested wait test (depth 1);" ] ||
  ui_fail "the nested waits in another order: $lines"
echo "ok   in order: $lines"
ui_focus_main
wait_cursor_balanced "the nested waits"

echo "--- The wait cursor (mitcad#268): one left on is cleared once idle"
ui_mark
ui_step "leave a wait cursor on (search)" ui_command "Leave Wait Cursor On"
ui_expect_new "Wait cursor left on for a test: test hook" "an override cursor set and left (the test hook)"
ui_expect_new "Wait cursor left on after test hook: cleared" "cleared once idle"
ui_expect_new "Wait cursor at idle: none set" "none left"
ui_crashed && ui_fail "crashed"
grep -q "OCCT view event failed" "$UI_LOG" && ui_fail "OCCT reported a failed view operation"
ui_stop_app

echo "--- A preview interrupted with its panel open: OK computes the edit itself"
# The window has no control that stops a preview and keeps its panel
# (mitcad#634: Esc and Cancel end the command, a newer input replaces the
# preview); the in-process test driver stops it through the worker, as a
# failed or interrupted computation would. The script: a second into the
# modelled thread's preview, it is stopped (nothing shown, OK enabled);
# OK computes the edit itself in its progress dialog, cancelled there a
# second in; OK again computes the thread.
MITCAD_TEST_INPUT=$(cd "$(dirname "$0")" && pwd)/ui-scripts/compute-interrupted-preview.mitcad-ui \
  ui_start_app --open "$WORK/rod.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the test script did not end in 300 s"
wait "$UI_RUNNER"
status=$?
UI_RUNNER=""
grep -q '^TestDriver: done' "$UI_LOG" && [ "$status" = 0 ] ||
  ui_fail "the test script failed (exit $status): $(grep -m1 'TestDriver: FAIL' "$UI_LOG")"
echo "ok   the script ran to its end"
UI_MARK=0
ui_expect_new "Preview Thread: cancelled" "the interrupted preview showed nothing"
stopped_soon preview
ui_expect_new "Progress dialog shown: edit_feature" "OK computed the edit itself, with its progress dialog"
stopped_soon edit_feature
ui_expect_new "Thread: OK cancelled" "OK was cancelled"
ui_expect_new "Computing edit_feature done in" "OK again computed the thread"
ui_expect_volume "Body Body1 (F2.b0): volume 11780.972 -> \([0-9.]*\) mm3" 9771.222684120039 \
  "the groove cut in" 1e-3
no_new "Progress dialog shown: preview" "no progress dialog for the previews"
grep -q "The document was read while a job computes it" "$UI_LOG" && ui_fail "the document was read during a job"
grep -qE "SIGSEGV|SIGABRT|terminate called|Aborted|Segmentation fault" "$UI_LOG" && ui_fail "crashed"

ui_finish "UI compute test"
