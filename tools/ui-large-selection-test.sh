#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/browser
# check-all sources: app/framework/Selection.cpp app/framework/Selection.hpp tools/cli
# A whole large design selected and cleared (mitcad#1631, GitHub #235),
# through the real UI, on a design mitcad-cli makes: a 10 mm cube and a
# rectangular pattern of it, 16 x 16 (256 bodies, 3072 edges):
#   - A window over the whole view selects every edge; Esc clears it. The
#     view draws the edges of each body as one object, not one object each,
#     and a selected body from the view's own presentation of it, so both
#     finish at once. The timings are printed (the selection's parts, the
#     window's pick, the highlights' slices), none asserted.
#   - A click on the pattern's timeline cell selects its 255 bodies (drawn
#     as shadows with their dashed copies, sliced); Esc clears them.
#   - Drawn slowly on purpose (MITCAD_TEST_HIGHLIGHT_STEP_MS: one body's
#     highlights at a time, 40 ms apart), the window's highlights are cut
#     short: by Esc, by another window and by a click, each of which drops
#     what is left to draw ("View highlights dropped") and selects anew.
#   - With a large design of the private test files (MITCAD_UI_LARGE_DESIGN,
#     else the reported design of GitHub #133 under MITCAD_MITCAD_REPORTED;
#     never committed): a window over all of it and Esc, timed.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-large-selection-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-large-selection.XXXXXX")
FILE=$WORK/pattern.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "rectangular_pattern",
    "objects": {"type": "bodies", "bodies": ["F2.b0"]},
    "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 16, "distance": 30},
    "direction2": {"axis": {"type": "origin", "axis": "y"}, "quantity": 16, "distance": 30},
    "distance_type": "spacing"}},
  {"expect": {"bodies": 256}}
]
JSON
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_new_match regex description [seconds]: a line matching the
# extended regex in the log since the mark, within the time (default 30 s).
expect_new_match() {
  local i
  for i in $(seq 1 $((${3:-30} * 5))); do
    # (No pipeline into grep -q under pipefail: ui_expect_new.)
    grep -qE -- "$1" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: /$1/ not in the log since the mark"
}

# last_new regex: the last line since the mark that matches, cut to 160
# characters ("none" when there is none).
last_new() {
  local line
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E -- "$1" | tail -1 | cut -c1-160)
  echo "${line:-none}"
}

# timings: the selection's timings logged since the mark.
timings() {
  echo "     $(last_new '^Selected: ')"
  echo "     $(last_new '^Timing window pick: ')"
  echo "     $(last_new '^Selection timing: ')"
  echo "     $(last_new '^View highlights drawn: ')"
}

# select_all_and_clear what: a window over the whole view, then Esc; the
# timings of both printed.
select_all_and_clear() {
  ui_mark
  ui_step "window over $1"                 ui_view_drag 1 3 3 97 97
  expect_new_match "^Selected: [0-9]+ " "the window selected $1" 120
  ui_wait_idle
  ui_sync
  timings
  ui_mark
  ui_step "clear the selection (Esc)"      ui_key Escape
  ui_expect_new "Selected: nothing" "Esc cleared the selection" 60
  ui_sync
  timings
}

ui_start_display

echo "--- Every edge of 256 bodies selected and cleared"
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "the pattern opened"
ui_step "fit (F6)"                       ui_key F6
ui_wait_idle
ui_sync
select_all_and_clear "every edge"
grep -E '^Selected: [0-9]+ edges' "$UI_LOG" > /dev/null || ui_fail "the window selected no edges"
echo "ok   $(grep -E '^Selected: [0-9]+ edges' "$UI_LOG" | tail -1 | cut -c1-40)"

echo "--- The pattern's 255 bodies selected and cleared"
ui_expect_log "Timeline RectangularPattern1 at" "the pattern's cell"
ui_mark
ui_step "click RectangularPattern1"      ui_click_logged "Timeline RectangularPattern1"
ui_expect_new "Selected: 255 bodies" "the pattern's bodies selected" 60
ui_sync
timings
ui_mark
ui_step "clear the selection (Esc)"      ui_key Escape
ui_expect_new "Selected: nothing" "Esc cleared the bodies" 60
ui_expect_new "View body highlights: none" "and their highlights"
ui_sync
timings
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "--- Highlights drawn slowly, cut short (Esc, another window, a click)"
export MITCAD_TEST_HIGHLIGHT_STEP_MS=40
ui_start_app --open "$FILE"
unset MITCAD_TEST_HIGHLIGHT_STEP_MS
ui_expect_log "Opened $FILE" "the pattern opened again"
ui_step "fit (F6)"                       ui_key F6
ui_wait_idle
ui_sync
# No ui_step around the windows and keys below: its sync would wait for
# every slice of the highlights.
ui_mark
ui_view_drag 1 3 3 97 97
expect_new_match "^Selected: [0-9]+ edges" "the window selected every edge" 60
ui_key Escape
ui_expect_new "View highlights dropped: " "Esc dropped the highlights not drawn yet" 30
ui_expect_new "Selected: nothing" "and cleared the selection" 30
ui_sync
ui_crashed && ui_fail "crashed after Esc"

ui_mark
ui_view_drag 1 3 3 97 97
expect_new_match "^Selected: [0-9]+ edges" "the window selected every edge again" 60
UI_MARK=$(wc -l < "$UI_LOG")
sleep 0.5 # not one gesture with the window before
ui_view_drag 1 3 3 50 97
ui_expect_new "View highlights dropped: " "another window dropped the first one's highlights" 30
expect_new_match "^Selected: [0-9]+ edges" "and selected the edges it holds" 60
UI_MARK=$(wc -l < "$UI_LOG")
sleep 0.5
ui_view_click 50 50
ui_expect_new "View highlights dropped: " "a click dropped them too" 30
expect_new_match "^Selected: " "and selected what it hit, or nothing" 30
ui_sync
ui_mark
ui_step "clear the selection (Esc)"      ui_key Escape
ui_sync
ui_crashed && ui_fail "crashed while highlights were drawn"
ui_stop_app

# A large design of the private test files (never committed): the given
# one, else the reported design of GitHub #133.
LARGE=${MITCAD_UI_LARGE_DESIGN:-}
if [ -z "$LARGE" ] && [ -n "${MITCAD_MITCAD_REPORTED:-}" ]; then
  LARGE=$(find "$MITCAD_MITCAD_REPORTED" -maxdepth 1 -name 'github-133-*.mitcad' 2> /dev/null | sort | head -1)
fi
if [ -n "$LARGE" ] && [ -f "$LARGE" ]; then
  echo "--- A large design of the private test files: all of it selected and cleared"
  UI_WINDOW_WAIT=180 ui_start_app --open "$LARGE"
  for _ in $(seq 1 900); do
    grep -qF -- "Opened $LARGE" "$UI_LOG" && break
    sleep 0.2
  done
  ui_expect_log "Opened $LARGE" "the large design opened"
  ui_step "fit (F6)"                     ui_key F6
  ui_wait_idle
  ui_sync
  select_all_and_clear "all of the large design"
  ui_stop_app
else
  echo "skip the large design of the private test files (MITCAD_UI_LARGE_DESIGN or MITCAD_MITCAD_REPORTED)"
fi

ui_finish "UI large selection test"
