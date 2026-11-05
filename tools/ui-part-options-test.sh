#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/library app/files app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp
# Options of configurable library parts in the part pickers (mitcad#810;
# docs/configurations.md, "Part pickers and BUILD") through the real UI,
# on synthetic parts mitcad-cli makes in a local git library (no network):
#   1. Insert from Library, an open table: a shelf board with the options
#      Color (Black, White, Red) and Length (300 mm … 2000 mm, step 10 mm),
#      the rule "Black is short" (Length at most 1000 mm when Color is
#      Black) and the row Standard (White, 1200 mm). Black chosen: the rule
#      moves Length from 1200 mm to 1000 mm and narrows its range, and the
#      notice names it; Length 800 mm; inserted with those options and no
#      row: 800 x 200 x 20.
#   2. Edit Options… of the inserted board: Length bound to the design's
#      parameter Shelf_L (700 mm): the board follows (700 x 200 x 20);
#      Shelf_L set to 900 mm in Edit Parameters: it follows again.
#   3. A closed table (a spacer, its rows Short and Long): chosen by its
#      rows as before, Long inserted: 20 x 20 x 30.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-part-options-test.sh
# check-all sources: core/vcs/src/library core/model/src/part_options.rs core/model-api/src/configurations.rs
# check-all sources: core/model-base/src/configurations.rs core/model-base/src/configurations tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-part-options.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
export GIT_AUTHOR_NAME="Options Tester" GIT_AUTHOR_EMAIL=tester@example.invalid
export GIT_COMMITTER_NAME="Options Tester" GIT_COMMITTER_EMAIL=tester@example.invalid
LIB=$WORK/options
SCENE=$WORK/scene.mitcad
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# design name 'commands...': a design from mitcad-cli's commands.
design() {
  printf '[%s]\n' "$2" > "$WORK/$1.json"
  "$CLI" run "$WORK/$1.json" --save "$WORK/$1.mitcad" > "$WORK/$1.log" 2>&1 ||
    { cat "$WORK/$1.log"; ui_fail "mitcad-cli: $1"; }
}

# The shelf board: an open table, its Length a range, its Color a list, one
# rule and one row.
design board '{"cmd": "add_parameter", "name": "Color", "expression": "'"'White'"'",
    "values": ["'"'Black'"'", "'"'White'"'", "'"'Red'"'"]},
  {"cmd": "add_parameter", "name": "Length", "expression": "1200 mm", "min": "300 mm", "max": "2000 mm",
    "step": "10 mm"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": "Length", "width": 200,
    "height": 20, "operation": "new_body"}},
  {"cmd": "set_configurations", "configurations": {"options": ["Color", "Length"], "closed": false,
    "default": "Standard",
    "rows": [{"name": "Standard", "values": {"Color": "'"'White'"'", "Length": "1200 mm"}}],
    "rules": [{"name": "Black is short", "when": "Color == '"'Black'"'",
               "then": [{"option": "Length", "max": "1000 mm"}]}]}}'
# The spacer: a closed table, its rows setting Size and H.
design spacer '{"cmd": "add_parameter", "name": "Size", "expression": "'"'S'"'"},
  {"cmd": "add_parameter", "name": "H", "expression": "10 mm"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
    "height": "H", "operation": "new_body"}},
  {"cmd": "set_configurations", "configurations": {"options": ["Size"], "closed": true, "parameters": ["H"],
    "default": "Short",
    "rows": [{"name": "Short", "values": {"Size": "'"'S'"'", "H": "10 mm"}},
             {"name": "Long", "values": {"Size": "'"'L'"'", "H": "30 mm"}}]}}'
# The design they go into: its own parameter Shelf_L.
design scene '{"cmd": "add_parameter", "name": "Shelf_L", "expression": "700 mm"}'

"$CLI" library init "$LIB" --id test-options --name "Test options" --license CC0-1.0 --author "Options Tester" \
  > /dev/null || ui_fail "library init"
"$CLI" library add "$LIB" "$WORK/board.mitcad" --id shelf-board --name "Shelf board" --designation "Shelf board" \
  --category boards > /dev/null || ui_fail "library add shelf-board"
"$CLI" library add "$LIB" "$WORK/spacer.mitcad" --id spacer --name "Spacer" --designation "Spacer" \
  --category spacers > /dev/null || ui_fail "library add spacer"
"$CLI" library check "$LIB" > /dev/null || ui_fail "the library has errors"
g "$LIB" add -A
g "$LIB" commit -q -m "Configurable parts"
g "$LIB" tag v1.0.0
echo "ok   the library of the configurable parts"

# The bodies' volumes: the last one logged since the mark.
expect_volume_new() { # sed pattern with one group, expected, description
  local volume=""
  for _ in $(seq 1 150); do
    volume=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n "s/.*$1.*/\\1/p" | tail -1)
    [ -n "$volume" ] && break
    sleep 0.2
  done
  [ -n "$volume" ] || ui_fail "$3: no volume logged"
  awk -v v="$volume" -v e="$2" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$3: volume $volume mm3, expected $2"
  echo "ok   $3: $volume mm3"
}

ui_start_display
ui_start_app --open "$SCENE"
ui_step "fit (F6)"                         ui_key F6

echo "--- 0. Library Sources: the configurable parts' library only"
ui_mark
ui_step "Library Sources (command search)" ui_command "Library Sources"
ui_focus_dialog '^Library Sources$'
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "the folder"                       eval 'ui_type_text "$LIB"; ui_key Return'
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library source added: $LIB" "the source added"
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetched: library Test options (test-options) from $LIB" "fetched" 30
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main

echo "--- 1. Insert from Library: an open table's options, filtered by the rule"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search Shelf board"               xdotool type --delay 40 "Shelf board"
ui_expect_new "Library search 'Shelf board': 1 components" "the search finds it"
ui_expect_new "Library browser selected test-options/shelf-board: version v1.0.0 (" "the board selected"
ui_expect_new "Library browser options of test-options/shelf-board: Color = White, Length = 1200" \
  "the options start with the default row's values"
ui_expect_new "(row Standard)" "the combination is the row Standard"
ui_expect_new "Library option Color allowed Black, White, Red" "Color lists its values"
ui_expect_new "Library option Length allowed 300 mm … 2000 mm, step 10 mm" "Length within its range"
ui_mark
ui_step "Color Black (Alt+V, Tab, B)"      eval 'ui_key alt+v; ui_sync; ui_key Tab; ui_sync; xdotool type --delay 80 "B"'
ui_expect_new "Library option Color chosen: Black" "Black chosen"
ui_expect_new "Library option moves: Length 1200" "the rule moves Length"
ui_expect_new ": Black is short" "the move names the rule"
ui_expect_new "Library option Length allowed 300 mm … 1000 mm, step 10 mm (Black is short)" \
  "the rule narrows Length's range"
ui_expect_new "Library options: Color = Black, Length = 1000" "Length at the rule's maximum"
ui_mark
ui_step "Length 800 (Tab, type, Tab)"      eval 'ui_key Tab; ui_sync; ui_key ctrl+a; xdotool type --delay 60 "800"; ui_key Tab'
ui_expect_new "Library option Length chosen: 800 mm" "800 mm chosen"
ui_expect_new "Library options: Color = Black, Length = 800" "Black and 800 mm"
ui_expect_new "(no row)" "a combination without a row"
ui_mark
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component " "the board inserted"
ui_expect_new "with options Color = 'Black', Length = 800 mm (no row)" "with the options chosen"
NAME=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/.*Inserted component \(.*\) (linked) from test-options .*/\1/p' |
  tail -1)
[ -n "$NAME" ] || ui_fail "the inserted component's name is not logged"
expect_volume_new "New body [^(]* ([^)]*): volume \([0-9.]*\) mm3" 3200000 "the board 800 x 200 x 20"

echo "--- 2. Edit Options: Length bound to the design's Shelf_L"
ui_mark
ui_step "right-click $NAME:1"              ui_click_logged "Browser Root/$NAME:1" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Edit Options..."           ui_menu_choose "Edit Options…"
ui_focus_dialog "^Edit Options - $NAME\$"
ui_expect_new "Edit Options opened for $NAME" "Edit Options opened"
ui_expect_new "Edit Options options: Color = Black, Length = 800" "the component's own options"
ui_expect_log "Edit Options Length expression at" "the expression fields are logged"
ui_mark
ui_step "Length = Shelf_L"                 ui_type_in "Edit Options Length expression" "Shelf_L"
# OK by a click: Return in the field may take the open suggestion instead.
ui_step "OK (click)"                       ui_click_logged "Edit Options OK"
ui_focus_main
ui_expect_new "Edit Options bound Length = Shelf_L" "Length bound to Shelf_L"
ui_expect_new "Edit Options accepted: Color = 'Black', Length = Shelf_L" "the expressions kept"
ui_expect_new "Edit Options of $NAME: Color = 'Black', Length = Shelf_L" "one command"
expect_volume_new "Body [^(]* ([^)]*): volume [0-9.]* -> \([0-9.]*\) mm3" 2800000 "the board follows Shelf_L, 700 mm"

ui_mark
ui_step "Edit Parameters"                ui_command "Edit Parameters"
ui_focus_dialog '^Parameters$'
ui_expect_log "Parameters Shelf_L expression at" "Shelf_L's cell is logged"
ui_mark
ui_step "Shelf_L = 900 mm"                 eval 'ui_double_click_logged "Parameters Shelf_L expression"; ui_sync;
  ui_key ctrl+a; xdotool type --delay 40 "900 mm"; ui_key Return'
ui_expect_new "Parameter Shelf_L expression: 900 mm" "Shelf_L changed"
expect_volume_new "Body [^(]* ([^)]*): volume [0-9.]* -> \([0-9.]*\) mm3" 3600000 "the board follows Shelf_L, 900 mm"
ui_step "OK (the table)"                   ui_click_logged "Parameters OK"
ui_focus_main

echo "--- 3. A closed table: chosen by its rows as before"
ui_mark
ui_step "Insert from Library"              ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search Spacer"                    xdotool type --delay 40 "Spacer"
ui_expect_new "Library browser selected test-options/spacer: version v1.0.0 (" "the spacer selected"
ui_expect_new ", size Short, licence CC0-1.0" "its default row"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Library browser options of test-options/spacer"; then
  ui_fail "a closed table was offered options"
fi
echo "ok   a closed table has no option choosers"
ui_mark
ui_step "Size L (Alt+V, Tab, L)"           eval 'ui_key alt+v; ui_sync; ui_key Tab; ui_sync; xdotool type --delay 80 "L"'
ui_expect_new "Library size Long" "the row Long chosen by its Size"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component " "the spacer inserted"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "with options" && ui_fail "a closed table's part was inserted with options"
expect_volume_new "New body [^(]* ([^)]*): volume \([0-9.]*\) mm3" 12000 "the spacer Long, 20 x 20 x 30"

ui_finish "UI part options test"
