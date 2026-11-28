#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ShortcutDialog.cpp app/framework/CommandRegistry.cpp tools/cli
# Isolate and Unisolate by keys of the user's own (mitcad#2482, GitHub
# mitcad/Mitcad#613) through the real UI, on a design with a body of the
# root (Body1, F2.b0) and a component placed once (Component1:1, O1, its
# body F3.b0):
#   - Docked: Isolate and Unisolate have no key; Keyboard Shortcuts binds
#     F7 to Isolate and F8 to Unisolate.
#   - Body1's row selected, F7 isolates Body1 (the view shows F2.b0 alone),
#     F8 shows both again; Component1:1's row selected, F7 isolates the
#     component (F3.b0 alone), Undo takes it back; both rows selected, F7
#     isolates both; F8 with nothing isolated says so.
#   - Floating: the saved keys isolate Component1:1 and restore the view.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-isolate-key-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-isolate-key.XXXXXX")
FILE=$WORK/design.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1 (F2.b0) in the root.
# Extrude2 (F3) 4 down as a new component: Component1:1 (O1, F3.b0), moved
# 40 mm along X.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# Keyboard Shortcuts by a key of our own, as that window saves it.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[shortcuts]\ntools.shortcuts=F12\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

BOTH="F2.b0, F3.b0 in O1"

# expect_shown "bodies" description: the view's last "Bodies shown" line is
# exactly that ("Bodies shown: F2.b0" is also the start of both's line).
expect_shown() {
  local line i
  for i in $(seq 1 60); do
    ui_sync
    line=$(grep "^Bodies shown: " "$UI_LOG" | tail -1)
    [ "$line" = "Bodies shown: $1" ] && { echo "ok   $2 ($line)"; return; }
    ui_poll_pause "$i"
  done
  ui_fail "$2: '$line', not 'Bodies shown: $1'"
}

# select_row "path": that row alone selected; it must not be selected
# already (a click on a selected row logs no new selection).
select_row() {
  ui_mark
  ui_step "click $1's row"                 ui_click_logged "Browser $1"
  ui_expect_new "Browser selected: " "$1 selected"
}

# select_both: Body1's row, then Component1:1's row with Ctrl.
select_both() {
  local selected
  select_row "Root/Bodies/Body1"
  ui_mark
  ui_step "Ctrl+click Component1:1's row"  xdotool mousemove $(ui_logged_at "Browser Root/Component1:1") keydown ctrl click 1 keyup ctrl
  ui_expect_new "Browser selected: " "rows selected"
  ui_sync
  selected=$(grep "^Browser selected: " "$UI_LOG" | tail -1)
  [[ $selected == *"Root/Bodies/Body1"* && $selected == *"Root/Component1:1"* && $selected != *"Component1:1/"* ]] ||
    ui_fail "not Body1 and Component1:1 selected: $selected"
  echo "ok   $selected"
}

# bind "command" key: the command's row found in Keyboard Shortcuts (the
# search has the keyboard), the key recorded for it.
bind() {
  ui_step "find $1"                        xdotool type --delay 40 "$1"
  ui_step "to its row (Down)"              ui_key Down
  ui_expect_new "Keyboard Shortcuts row: $1 | 3D View | (none)" "$1's row: no key of its own"
  ui_step "record its key (Enter)"         ui_key Return
  ui_expect_new "Keyboard Shortcuts recording: $1 | 3D View" "recording the key"
  ui_step "$2"                             ui_key "$2"
  ui_expect_new "Keyboard Shortcuts key: $1 | 3D View = $2" "$2 for $1"
}

echo "=== Docked"
ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
expect_shown "$BOTH" "Body1 and Component1:1 shown"
ui_expect_log "Browser Root/Bodies/Body1 at" "the browser lists Body1"
ui_expect_log "Browser Root/Component1:1 at" "and Component1:1"

echo "--- Isolate and Unisolate bound in Keyboard Shortcuts"
ui_mark
ui_step "Keyboard Shortcuts (F12)"          ui_key F12
ui_expect_new "Keyboard Shortcuts opened" "the window opened"
ui_focus_dialog '^Keyboard Shortcuts$'
bind "Isolate" F7
ui_step "back to the search"               ui_key shift+Tab shift+Tab ctrl+a
bind "Unisolate" F8
ui_step "OK (Enter in the search)"         ui_key shift+Tab shift+Tab Return
ui_expect_new "Keyboard Shortcuts saved" "OK passed"
grep -qF "Conflicting saved shortcuts" "$UI_LOG" && ui_fail "the saved shortcuts conflict"
echo "ok   no shortcut in conflict"
ui_focus_main

echo "--- A body: F7 isolates it, F8 shows everything again"
select_row "Root/Bodies/Body1"
ui_mark
ui_step "F7"                               ui_key F7
ui_expect_new "Isolated body F2.b0" "F7 isolated Body1"
expect_shown "F2.b0" "only Body1 shown"
ui_mark
ui_step "F8"                               ui_key F8
ui_expect_new "Unisolated" "F8 unisolated"
expect_shown "$BOTH" "both shown again"

echo "--- A component: F7 isolates it, Undo takes it back"
select_row "Root/Component1:1"
ui_mark
ui_step "F7"                               ui_key F7
ui_expect_new "Isolated component " "F7 isolated Component1:1"
line=$(grep "^Isolated " "$UI_LOG" | tail -1)
[[ $line == *" in O1" && $line != *"F2.b0"* ]] || ui_fail "not Component1:1 alone isolated: $line"
echo "ok   $line"
expect_shown "F3.b0 in O1" "only Component1:1's body shown"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Isolate an item" "one undo step"
expect_shown "$BOTH" "both shown"

echo "--- Both selected: F7 isolates both, one undo step"
# Component1:1's row is still selected after the Undo: Body1's row first,
# as a click on a selected row selects nothing new (a slow one renames it).
select_row "Root/Bodies/Body1"
select_row "Root/Component1:1"
ui_mark
ui_step "F7"                               ui_key F7
ui_expect_new "Isolated component " "F7 isolated Component1:1 again"
expect_shown "F3.b0 in O1" "Component1:1 isolated again"
select_both
ui_mark
ui_step "F7"                               ui_key F7
ui_expect_new "Isolated body F2.b0, component " "F7 isolated Body1 and Component1:1"
expect_shown "$BOTH" "both shown, isolated"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Isolate 2 items" "the isolation of both, one undo step"
expect_shown "F3.b0 in O1" "Component1:1 alone isolated again"
ui_mark
ui_step "F8"                               ui_key F8
ui_expect_new "Unisolated" "F8 unisolated"
expect_shown "$BOTH" "both shown"
ui_mark
ui_step "F8 again"                         ui_key F8
ui_expect_new "Unisolate: nothing is isolated" "F8 with nothing isolated says so"
ui_expect_new ": Nothing is isolated." "in the status bar"
ui_stop_app

echo "=== Floating: the saved keys"
ui_start_app --open "$FILE" --chrome=floating
ui_step "fit (F6)"                         ui_key F6
expect_shown "$BOTH" "Floating: both shown"
ui_expect_log "Browser Root/Component1:1 at" "Floating: the browser lists Component1:1"
select_row "Root/Component1:1"
ui_mark
ui_step "F7"                               ui_key F7
ui_expect_new "Isolated component " "Floating: F7 isolated Component1:1"
expect_shown "F3.b0 in O1" "Floating: only Component1:1's body shown"
ui_mark
ui_step "F8"                               ui_key F8
ui_expect_new "Unisolated" "Floating: F8 unisolated"
expect_shown "$BOTH" "Floating: both shown again"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI Isolate key"
