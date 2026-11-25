#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp app/MainWindowContextMenu.cpp
# check-all sources: app/framework/ShortcutDialog.cpp app/framework/CommandRegistry.cpp tools/cli
# Hide/Show of the selection by a key (mitcad#2356, GitHub mitcad/Mitcad#612)
# through the real UI, on a design with a body of the root (Body1, F2.b0)
# and a component placed once (Component1:1, O1, its body F3.b0):
#   - Body1's row and Component1:1's row selected in the Browser, V
#     (view.toggle_visibility) hides both in one undo step, V again shows
#     both; undo brings back what the last V changed.
#   - The same two rows selected, the row menu's Hide hides both (the same
#     command), one undo step.
#   - Hide/Show rebound to F8 in Keyboard Shortcuts: V no longer hides, F8
#     does, and F8 again shows.
#   - The command search finds Hide/Show and runs it on the selection.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hide-show-key-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-hide-show-key.XXXXXX")
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
printf '[shortcuts]\ntools.shortcuts=F9\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

SHOWN="Bodies shown: F2.b0, F3.b0 in O1"

# select_both: Body1's row, then Component1:1's row with Ctrl.
select_both() {
  local selected
  ui_mark
  ui_step "click Body1's row"              ui_click_logged "Browser Root/Bodies/Body1"
  ui_step "Ctrl+click Component1:1's row"  xdotool mousemove $(ui_logged_at "Browser Root/Component1:1") keydown ctrl click 1 keyup ctrl
  ui_expect_new "Browser selected: " "rows selected"
  ui_sync
  selected=$(grep "^Browser selected: " "$UI_LOG" | tail -1)
  [[ $selected == *"Root/Bodies/Body1"* && $selected == *"Root/Component1:1"* && $selected != *"Component1:1/"* ]] ||
    ui_fail "not Body1 and Component1:1 selected: $selected"
  echo "ok   $selected"
}

# expect_both hidden|shown description: Hide/Show logged both items so,
# in the selection's order.
expect_both() {
  local line
  ui_expect_new "Hide/Show $1: " "$2"
  line=$(grep "^Hide/Show $1: " "$UI_LOG" | tail -1)
  [[ $line == *"Body1"* && $line == *"Component1:1"* ]] || ui_fail "$2: $line"
  echo "ok   $line"
}

# expect_none_since "text" description: no line with the text since the mark.
expect_none_since() {
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | head -1)"
  echo "ok   $2"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "$SHOWN" "Body1 and Component1:1 shown"
ui_expect_log "Browser Root/Bodies/Body1 at" "the browser lists Body1"
ui_expect_log "Browser Root/Component1:1 at" "and Component1:1"

echo "--- V hides a body and a component, V again shows them"
select_both
ui_mark
ui_step "V"                                ui_key v
expect_both hidden "V hid both"
ui_expect_new "Bodies shown: none" "nothing shown"
ui_mark
ui_step "V again"                          ui_key v
expect_both shown "V again showed both: the selection stayed"
ui_expect_new "$SHOWN" "both shown again"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Show Selection" "one undo step for both"
ui_expect_new "Bodies shown: none" "both hidden again"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Hide Selection" "the first V's one undo step"
ui_expect_new "$SHOWN" "both shown"

echo "--- The row menu's Hide on the selected rows"
select_both
ui_mark
ui_step "right-click Body1's row"          ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu: " "the row's menu"
ui_step "choose Hide"                      ui_menu_choose "Hide"
expect_both hidden "the row's Hide hid both rows' items"
ui_expect_new "Bodies shown: none" "nothing shown"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Hide Selection" "one undo step"
ui_expect_new "$SHOWN" "both shown"

echo "--- Hide/Show rebound to F8 in Keyboard Shortcuts"
ui_mark
ui_step "Keyboard Shortcuts (F9)"          ui_key F9
ui_expect_new "Keyboard Shortcuts opened" "the window opened"
ui_focus_dialog '^Keyboard Shortcuts$'
ui_step "find Hide/Show"                   xdotool type --delay 40 "Hide/Show"
ui_step "to its row (Down)"                ui_key Down
ui_expect_new "Keyboard Shortcuts row: Hide/Show | 3D View | V" "its row: V in the 3D view"
ui_step "record its key (Enter)"           ui_key Return
ui_expect_new "Keyboard Shortcuts recording: Hide/Show | 3D View" "recording the key"
ui_step "F8"                               ui_key F8
ui_expect_new "Keyboard Shortcuts key: Hide/Show | 3D View = F8" "F8 for Hide/Show"
ui_step "OK (Enter in the search)"         ui_key shift+Tab shift+Tab Return
ui_expect_new "Keyboard Shortcuts saved" "OK passed"
grep -qF "Conflicting saved shortcuts" "$UI_LOG" && ui_fail "the saved shortcuts conflict"
echo "ok   no shortcut in conflict"
ui_focus_main

select_both
ui_mark
ui_step "V"                                ui_key v
expect_none_since "Hide/Show " "V no longer hides or shows"
ui_mark
ui_step "F8"                               ui_key F8
expect_both hidden "F8 hid both"
ui_expect_new "Bodies shown: none" "nothing shown"
ui_mark
ui_step "F8 again"                         ui_key F8
expect_both shown "F8 again showed both"
ui_expect_new "$SHOWN" "both shown again"

echo "--- Hide/Show from the command search"
ui_mark
ui_step "Hide/Show by the command search"  ui_command "Hide/Show"
expect_both hidden "the command search's Hide/Show hid both"
ui_expect_new "Bodies shown: none" "nothing shown"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI Hide/Show key"
