#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/library app/MainWindow.cpp tools/cli
# Browse Components (mitcad#1283, GitHub #136) through the real UI, with
# local folder sources only (no network): a library of a block (a source)
# and a community index that names a library of a plate (not a source).
#   1. Tools › Libraries lists Browse Components… once, as its first entry;
#      it opens the one window, titled Browse Components, on Installed.
#   2. The browser's Library Sources… opens the Library Sources window (the
#      menu's): the default sources removed, the block's library and the
#      index added as folders; back in the browser Update fetches both.
#   3. Installed lists the block, and the plate's library to get. Community
#      keeps the block selected; the search "plate" finds the index's
#      library and its component Boss plate, whose library is not fetched:
#      the primary button is Get Library. Get Library fetches it after its
#      question; Boss plate stays selected, the search kept, the button
#      Insert, and Insert places it.
#   4. The browser opened again from the menu: Installed → Community →
#      Favorites (no favorites yet) → Installed, the block starred, then
#      Favorites lists it with the selection kept. Insert from Library and
#      Community Library open the same window on Installed and on
#      Community.
#   5. The Floating layout (--chrome=floating): Tools › Libraries lists
#      Browse Components… once and opens the browser.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browse-components-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-browse-components.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
BLOCKS=$WORK/blocks
PLATES=$WORK/plates
INDEX=$WORK/index
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# logged_after "text" sed-expression: the last line since the mark with
# the text, through the expression.
logged_after() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1 | sed "$2"
}

# libraries_menu: Tools › Libraries opened (Alt+T, L); its entries checked:
# Browse Components… once, first; Insert from Library… and Community
# Library… no longer in it.
libraries_menu() {
  local menu count
  ui_mark
  ui_step "the Tools menu (Alt+T)"         ui_key alt+t
  ui_expect_new "Tools menu: " "the Tools menu opened"
  ui_step "Libraries (L)"                  ui_key l
  ui_expect_new "Libraries menu: " "the Libraries menu opened"
  menu=$(logged_after "Libraries menu: " 's/.*Libraries menu: //')
  count=$(grep -o "Browse Components…" <<< "$menu" | wc -l)
  [ "$count" -eq 1 ] || ui_fail "Browse Components… is $count times in the Libraries menu: '$menu'"
  [[ $menu == "Browse Components… | - | "* ]] || ui_fail "Browse Components… is not the menu's first entry: '$menu'"
  [[ $menu != *"Insert from Library"* && $menu != *"Community Library"* ]] ||
    ui_fail "the Libraries menu still has Insert from Library or Community Library: '$menu'"
  echo "ok   Browse Components… once, first: $menu"
}

# make_library folder id name component title category: a library of one
# component (the boss test design), recorded with git.
make_library() {
  "$CLI" library init "$1" --id "$2" --name "$3" --license CC0-1.0 > /dev/null || ui_fail "library init $2"
  "$CLI" library add "$1" "$ROOT/tools/cli/tests/v1_boss.mitcad" --id "$4" --name "$5" --category "$6" \
    --license CC0-1.0 > /dev/null || ui_fail "library add $4"
  "$CLI" library check "$1" > /dev/null || ui_fail "the library $2 has errors"
  g "$1" add -A
  g "$1" commit -q -m "Parts"
}

# The block's library (a source) and the plate's (named by the index only).
make_library "$BLOCKS" test-blocks "Test blocks" block "Test block" blocks
make_library "$PLATES" test-plates "Test plates" boss-plate "Boss plate" plates
# The community index: the plates' library with its component, not fetched.
mkdir -p "$INDEX/libraries"
printf '{"format": "mitcad-index", "version": 1, "name": "Test index"}\n' > "$INDEX/mitcad-index.json"
printf '{"format": "mitcad-index-entry", "version": 1, "id": "test-plates", "name": "Test plates",
  "url": "%s", "license": "CC0-1.0", "tags": ["plate"],
  "components": [{"id": "boss-plate", "name": "Boss plate", "tags": ["plate"]}]}\n' \
  "$PLATES" > "$INDEX/libraries/test-plates.json"
g "$INDEX" init -q
g "$INDEX" add -A
g "$INDEX" commit -q -m "Index"

ui_start_display
ui_start_app --demo

echo "--- 1. Tools › Libraries: Browse Components… once"
libraries_menu
ui_mark
ui_step "Browse Components… (B)"          ui_key b
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library browser view: Installed" "the browser opened on Installed"
ui_expect_new "Part picker opened: 0 parts for Browse Components" "one window, titled Browse Components"

echo "--- 2. Library Sources… from the browser"
ui_mark
ui_step "Library Sources… (Alt+B)"        ui_key alt+b
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library browser: Library Sources opened" "the browser's button opened it"
ui_expect_new "Library source https://github.com/mitcad/parts.git (on): not fetched" \
  "the window of Tools › Libraries › Library Sources…: the default sources listed"
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_expect_new "Library source removed: https://github.com/mitcad/community-index.git" "the defaults removed"
for url in "$BLOCKS" "$INDEX"; do
  ui_step "Add URL (Alt+A)"                ui_key alt+a
  ui_focus_dialog '^Add Library$'
  ui_step "the folder"                     eval 'ui_type_text "$url"; ui_key Return'
  ui_focus_dialog '^Library Sources$'
done
ui_expect_new "Library source added: $INDEX" "the block's library and the index added"
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_dialog '^Browse Components$'
ui_mark
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_expect_new "Library fetched: library Test blocks (test-blocks) from $BLOCKS" "the block's library fetched" 30
ui_expect_new "Library fetched: index Test index () from $INDEX" "the index fetched" 30
ui_expect_new "Library fetch of 2 source(s): 0 failed" "no failures" 30

echo "--- 3. Installed, Community, Get Library in place"
ui_expect_new "Library search '': 1 components, 1 libraries" "Installed: the block and the plates' library to get"
ui_expect_new "Library browser selected test-blocks/block: version " "the block selected"
ui_expect_new "Library browser button: Insert" "Insert for a component whose library is here"
ui_mark
ui_step "Community (Alt+M)"                ui_key alt+m
ui_expect_new "Library browser view: Community" "the Community view"
ui_expect_new "Library browser: 1 components of libraries to get" "the index's component of a library to get"
ui_expect_new "Library browser kept the selection component|" "the block kept selected"
ui_mark
ui_step "search plate"                     eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "plate"'
ui_expect_new "Library search 'plate': 0 components, 1 libraries" "the search finds the index's library"
ui_expect_new "Library browser selected library test-plates ($PLATES), licence CC0-1.0, not fetched" \
  "the plates' library first"
ui_expect_new "Library browser button: Get Library" "Get Library for a library that is not here"
ui_mark
ui_step "its component (Down)"             ui_key Down
ui_expect_new "Library browser selected test-plates/boss-plate of a library to get ($PLATES), licence CC0-1.0: Get Library" \
  "a component of a library not fetched: the primary button is Get Library"
ui_mark
ui_step "Get Library (Alt+G)"              ui_key alt+g
ui_focus_dialog '^Get Library$'
ui_step "yes (Alt+Y)"                      ui_key alt+y
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library fetched: library Test plates (test-plates) from $PLATES" "the library fetched" 30
ui_expect_new "Library search 'plate': 1 components, 1 libraries" "the search kept, the fetched component listed"
ui_expect_new "Library browser fetched test-plates: selected test-plates/boss-plate" "the same component kept selected"
ui_expect_new "Library browser selected test-plates/boss-plate: version " "Boss plate at its version"
ui_expect_new "Library browser button: Insert" "now Insert"
ui_mark
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Part picker chose test-plates/boss-plate" "chosen in the browser"
ui_expect_new "Inserted component Boss plate (linked) from test-plates " "Boss plate inserted"

echo "--- 4. The views: Installed → Community → Favorites, and the old commands"
libraries_menu
ui_mark
ui_step "Browse Components… (B)"          ui_key b
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library browser view: Installed" "Installed first"
ui_mark
ui_step "Community (Alt+M)"                ui_key alt+m
ui_expect_new "Library browser view: Community" "Community"
ui_mark
ui_step "Favorites (Alt+F)"                ui_key alt+f
ui_expect_new "Library browser view: Favorites" "Favorites"
ui_expect_new "Library browser favorites: 0 of 2 components starred" "nothing starred yet"
ui_expect_new "Library browser empty: no favorites" "its empty list says how to star"
ui_mark
ui_step "Installed (Alt+N)"                ui_key alt+n
ui_expect_new "Library browser view: Installed" "back to Installed"
ui_mark
ui_step "search block"                     eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "block"'
ui_expect_new "Library browser selected test-blocks/block: version " "the block selected"
ui_mark
ui_step "star it (Ctrl+D)"                 ui_key ctrl+d
ui_expect_new "Part picker star block on" "the block starred"
ui_mark
ui_step "Favorites (Alt+F)"                ui_key alt+f
ui_expect_new "Library browser view: Favorites" "Favorites again"
ui_expect_new "Library browser favorites: 1 of 1 components starred" "the starred block under the search"
ui_expect_new "Library browser kept the selection component|" "the block kept selected"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library browser view: Installed" "Insert from Library opens it on Installed"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "Community Library (command search)" ui_command "Community Library"
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library browser view: Community" "Community Library opens it on Community"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 5. The Floating layout"
ui_stop_app
ui_start_app --demo --chrome=floating
libraries_menu
ui_mark
ui_step "Browse Components… (B)"          ui_key b
ui_focus_dialog '^Browse Components$'
ui_expect_new "Library browser view: Installed" "the browser opened on Installed in the Floating layout"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

ui_stop_app
ui_finish "Browse Components"
