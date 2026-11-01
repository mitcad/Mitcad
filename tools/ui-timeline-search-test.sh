#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp tools/cli
# The timeline's search (mitcad#179) through the real UI, on a design of
# two components whose timeline is longer than the window: Part and Cover
# each have a sketch, an extrude, a fillet with the same name but for a
# number ("Rounded Edge", "Rounded Edge 2") and 24 planes; Cover's extrude
# has an import note (mitcad#170), one of Part's planes is suppressed.
#   - Ctrl+F in the timeline opens the field; a name lists both fillets
#     with their components, outlined in the strip; Enter, Down and Up
#     step through them, each selected (its face highlighted) and scrolled
#     into sight; a click on a listed match goes to it.
#   - Esc closes the search: the selection and the scroll position from
#     before it come back.
#   - Ctrl+F in the browser opens it too; the filters: is:note finds the
#     import-noted extrude (information, mitcad#1040),
#     "plane in:cover" Cover's planes only,
#     is:suppressed the suppressed plane; an unknown status is reported.
#     Emptying the field without a step changes nothing.
#   - The floating layout: the same search, steps and restore.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-search-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-timeline-search.XXXXXX)
FILE=$WORK/search.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# component name first-uid: a sketch 30 x 20, its extrude 10 up, a fillet on
# a vertical edge and 24 planes; the uids follow on from first-uid.
component() {
  local sketch=F$2 extrude=F$(($2 + 1)) i
  printf '  {"cmd": "activate_component", "component": "Root"},\n'
  printf '  {"cmd": "create_component", "name": "%s"},\n' "$1"
  printf '  {"cmd": "sketch.create"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "%s", "corner": [0, 0], "width": 30, "height": 20},\n' "$sketch"
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "%s",' "$sketch"
  printf ' "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],'
  printf ' "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},\n'
  printf '  {"cmd": "add_feature", "def": {"type": "fillet", "body": "%s.b0",' "$extrude"
  printf ' "edges": ["E{%s:side(c1[c4,c2])|%s:side(c2[c1,c3])}"], "radius": 1}},\n' "$extrude" "$extrude"
  for i in $(seq 1 24); do
    printf '  {"cmd": "add_feature", "def": {"type": "construction_plane",'
    printf ' "definition": {"type": "offset", "plane": "xy", "distance": %d}}},\n' "$((10 + i))"
  done
}
{
  echo "["
  component Part 1
  component Cover 28
  printf '  {"cmd": "rename_feature", "uid": "F3", "name": "Rounded Edge"},\n'
  printf '  {"cmd": "rename_feature", "uid": "F30", "name": "Rounded Edge 2"},\n'
  printf '  {"cmd": "suppress_feature", "uid": "F6"}\n'
  echo "]"
} > "$WORK/model.json"
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# An import note on Cover's extrude (as an import leaves one, mitcad#170).
python3 - "$FILE" << 'EOF' || ui_fail "cannot add the import note"
import json, sys
path = sys.argv[1]
doc = json.load(open(path))
names = {f["uid"]: f["name"] for f in doc["features"]}
assert names["F2"] == "Extrude1" and names["F29"] == "Extrude2", names
assert names["F3"] == "Rounded Edge" and names["F30"] == "Rounded Edge 2", names
assert names["F6"] == "Plane3" and names["F31"] == "Plane25", names
for feature in doc["features"]:
    if feature["uid"] == "F29":
        feature["import_note"] = "Partly imported: left out: 2 constraints"
json.dump(doc, open(path, "w"), indent=1)
EOF

# x_of "text": the x of the last "<text> at x,y" line, in window pixels.
x_of() { grep -F -- "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*\),.*/\1/p'; }

# step_through layout: the search for "rounded edge" from the field, its
# steps, a click on a listed match, and Esc; Plane1 was selected before.
step_through() {
  ui_mark
  ui_step "type rounded edge"              xdotool type --delay 40 "rounded edge"
  ui_expect_new 'Timeline search "rounded edge": 2 matches: Rounded Edge (Part), Rounded Edge 2 (Cover)' \
    "both fillets found, with their components"
  ui_expect_new "Timeline search result Rounded Edge (Part) at" "Part's listed"
  ui_expect_new "Timeline search result Rounded Edge 2 (Cover) at" "Cover's listed"
  [ -n "$SHOT" ] && ui_capture "${SHOT%.png}-$1-found.png"
  local before
  before=$(x_of "Timeline Rounded Edge 2")
  [ "$before" -gt "$WIDTH" ] || ui_fail "Rounded Edge 2 is in sight before the search scrolled (x $before)"
  echo "ok   Cover's fillet is out of sight (x $before)"

  ui_mark
  ui_step "Enter"                          ui_key Return
  ui_expect_new "Timeline search match 1 of 2: Rounded Edge (Part)" "Enter: the first match"
  ui_expect_new "Feature faces highlighted: F3 1" "selected: its rounded face"
  ui_mark
  ui_step "Down"                           ui_key Down
  ui_expect_new "Timeline search match 2 of 2: Rounded Edge 2 (Cover)" "Down: the next match"
  ui_expect_new "Feature faces highlighted: F30 1" "selected: Cover's rounded face"
  ui_expect_new "Timeline Rounded Edge 2 at" "the timeline scrolled"
  local after
  after=$(x_of "Timeline Rounded Edge 2")
  [ "$after" -gt 0 ] && [ "$after" -lt "$WIDTH" ] || ui_fail "Rounded Edge 2 not scrolled into sight (x $after)"
  echo "ok   Cover's fillet scrolled into sight (x $after)"
  ui_mark
  ui_step "Down (wraps)"                   ui_key Down
  ui_expect_new "Timeline search match 1 of 2: Rounded Edge (Part)" "past the last: the first again"
  ui_mark
  ui_step "Up (wraps)"                     ui_key Up
  ui_expect_new "Timeline search match 2 of 2: Rounded Edge 2 (Cover)" "Up from the first: the last"
  ui_mark
  ui_step "Shift+Enter"                    ui_key shift+Return
  ui_expect_new "Timeline search match 1 of 2: Rounded Edge (Part)" "Shift+Enter: the previous"
  ui_mark
  ui_step "click Cover's listed match"     ui_click_logged "Timeline search result Rounded Edge 2 (Cover)"
  ui_expect_new "Timeline search match 2 of 2: Rounded Edge 2 (Cover)" "a click on the list goes to it"
  [ -n "$SHOT" ] && ui_capture "${SHOT%.png}-$1.png"

  ui_mark
  ui_step "Esc"                            ui_key Escape
  ui_expect_new "Timeline search cleared: selected Plane1 again, scrolled to 0" \
    "cleared: Plane1 selected and the timeline at its start again"
  ui_expect_new "Timeline search closed" "the field closed"
  ui_expect_new "Selection restored after the timeline search: 1 item(s)" "the window's selection too"
  ui_expect_new "Selected: 1 plane" "Plane1 selected in the window"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Command Cancel\|Selected: nothing" &&
    ui_fail "Esc went to the window"
  echo "ok   Esc stayed in the field"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline Rounded Edge 2 at" "the timeline is logged"
ui_expect_log "Timeline cell Extrude2: info, badge" "the import note is information: its badge (mitcad#1040)"

echo "--- Ctrl+F in the timeline; steps through the matches"
ui_mark
ui_step "click Plane1"                     ui_click_logged "Timeline Plane1"
ui_expect_new "Selected: 1 plane" "Plane1 selected"
ui_mark
ui_step "Ctrl+F"                           ui_key ctrl+f
ui_expect_new "Timeline search opened" "the search field opened"
ui_expect_new "Timeline search field at" "and is laid out"
step_through docked

echo "--- Ctrl+F in the browser; filters"
ui_mark
ui_step "click Part:1 in the browser"      ui_click_logged "Browser Root/Part:1"
ui_step "Ctrl+F"                           ui_key ctrl+f
ui_expect_new "Timeline search opened" "Ctrl+F in the browser opens it"
ui_mark
ui_step "type is:note"                     xdotool type --delay 40 "is:note"
ui_expect_new 'Timeline search "is:note": 1 match: Extrude2 (Cover)' "is:note: the import-noted extrude"
ui_mark
ui_step "select all, type plane in:cover"  ui_key ctrl+a
ui_step "type"                             xdotool type --delay 40 "plane in:cover"
ui_expect_new 'Timeline search "plane in:cover": 24 matches: Plane25 (Cover), Plane26 (Cover)' \
  "only Cover's planes"
ui_mark
ui_step "select all, type is:suppressed"   ui_key ctrl+a
ui_step "type"                             xdotool type --delay 40 "is:suppressed"
ui_expect_new 'Timeline search "is:suppressed": 1 match: Plane3 (Part)' "the suppressed plane"
ui_mark
ui_step "select all, type is:broken"       ui_key ctrl+a
ui_step "type"                             xdotool type --delay 40 "is:broken"
ui_expect_new 'Timeline search "is:broken": unknown filter is:broken' "an unknown status is reported"
ui_mark
ui_step "empty the field"                  ui_key ctrl+a BackSpace
ui_expect_new "Timeline search cleared" "emptied"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Timeline search cleared: selected" &&
  ui_fail "a search without steps restored a selection"
echo "ok   nothing to restore without a step"
ui_mark
ui_step "Esc"                              ui_key Escape
ui_expect_new "Timeline search closed" "closed"

echo "--- The floating layout"
ui_stop_app
ui_start_app --open "$FILE" --chrome=floating
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline Rounded Edge 2 at" "the timeline is logged"
ui_mark
ui_step "click Plane1"                     ui_click_logged "Timeline Plane1"
ui_expect_new "Selected: 1 plane" "Plane1 selected"
ui_mark
ui_step "Ctrl+F"                           ui_key ctrl+f
ui_expect_new "Timeline search opened" "the search field opened in the card"
step_through floating

ui_finish "UI timeline search test"
