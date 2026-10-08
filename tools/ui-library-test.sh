#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Component libraries (mitcad#64) and the community library (mitcad#63)
# through the real UI, with local git repositories only (no network):
#   1. Publish to Library: the demo block added to a new library of one's
#      own (made, recorded as a version), pushed to a bare repository, and
#      its entry for a community index.
#   2. Libraries: the default sources removed (they are not fetched), a
#      fastener library made by the generator (v1.0.0, then v1.1.0 with
#      wider M5 heads) and a community index added by URL and fetched.
#   3. Insert from Library: a search for 4762, the older version v1.0.0,
#      the default size M5x16, linked: its volume; a nut copied.
#   4. Library Parts: the screw still at v1.0.0 with a newer version
#      fetched; v1.1.0 chosen, Show Changes names the wider head, Update:
#      the new volume; then another size (M6x20): renamed, its volume; the
#      parts list with the designations and licences.
#   5. Community Library: the index's library without a licence hidden
#      until asked for; Get Library fetches it after a question; its
#      component inserted with no licence named.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-library-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-library.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
# The versions the app records are by this author.
export GIT_AUTHOR_NAME="Library Tester" GIT_AUTHOR_EMAIL=tester@example.invalid
export GIT_COMMITTER_NAME="Library Tester" GIT_COMMITTER_EMAIL=tester@example.invalid
LIB=$WORK/fasteners
OTHER=$WORK/brackets
INDEX=$WORK/index
MINE=$WORK/mine
BARE=$WORK/mine-remote.git
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The fastener library: v1.0.0, then v1.1.0 with the M5 heads 8.7 mm wide.
python3 "$ROOT/tools/libraries/make-fastener-library.py" --cli "$CLI" --out "$LIB" --sizes M5,M6 \
  --standards iso4762,iso4032 --version 1.0.0 --commit > /dev/null || ui_fail "the generator failed"
sed -i 's/"dk": "8.5 mm"/"dk": "8.7 mm"/' "$LIB/screws/iso4762.mitcad"
g "$LIB" commit -q -am "Wider M5 heads"
g "$LIB" tag v1.1.0
# A library without a licence.
"$CLI" library init "$OTHER" --id test-brackets --name "Test brackets" > /dev/null || ui_fail "library init"
"$CLI" library add "$OTHER" "$ROOT/tools/cli/tests/v1_boss.mitcad" --id boss-bracket --name "Boss bracket" \
  --category brackets > /dev/null || ui_fail "library add"
g "$OTHER" add -A
g "$OTHER" commit -q -m "Brackets"
# The community index: the fastener library and the brackets.
mkdir -p "$INDEX/libraries"
printf '{"format": "mitcad-index", "version": 1, "name": "Test index"}\n' > "$INDEX/mitcad-index.json"
"$CLI" library index-entry "$LIB" --url "$LIB" > "$INDEX/libraries/mitcad-fasteners.json" || ui_fail "index entry"
printf '{"format": "mitcad-index-entry", "version": 1, "id": "test-brackets", "name": "Test brackets",
  "url": "%s", "tags": ["bracket"], "components": [{"id": "boss-bracket", "name": "Boss bracket"}]}\n' \
  "$OTHER" > "$INDEX/libraries/test-brackets.json"
g "$INDEX" init -q
g "$INDEX" add -A
g "$INDEX" commit -q -m "Index"
git init -q --bare "$BARE"

ui_start_display
ui_start_app --demo

echo "--- 1. Publish to Library"
ui_mark
ui_step "Publish to Library (command search)" ui_command "Publish to Library"
ui_focus_dialog '^Publish to Library$'
ui_step "the folder"                       eval 'ui_key alt+f; xdotool type --delay 10 "$MINE"'
ui_step "the library's id"                 eval 'ui_key alt+i; xdotool type --delay 20 "my-parts"'
ui_step "the library's name"               eval 'ui_key alt+n; xdotool type --delay 20 "My parts"'
ui_step "the author"                       eval 'ui_key alt+a; xdotool type --delay 20 "Library Tester"'
ui_step "the component's id"               eval 'ui_key alt+d; xdotool type --delay 20 "demo-block"'
ui_step "its name"                         eval 'ui_key alt+m; xdotool type --delay 20 "Demo block"'
ui_step "its category"                     eval 'ui_key alt+g; xdotool type --delay 20 "blocks"'
ui_step "its tags"                         eval 'ui_key alt+t; xdotool type --delay 20 "block, demo"'
ui_step "Add to Library (Alt+L)"           ui_key alt+l
ui_expect_new "Publish: Made the library my-parts in $MINE" "the library made"
ui_expect_new "Publish: Added demo-block to the library, recorded as version " "added and recorded"
ui_step "the remote"                       eval 'ui_key alt+r; xdotool type --delay 10 "$BARE"'
ui_step "Push (Alt+P)"                     ui_key alt+p
ui_expect_new "Publish: Pushed the library." "pushed" 30
ui_step "Index Entry (Alt+X)"              ui_key alt+x
ui_expect_new "Publish: The index entry libraries/my-parts.json" "the index entry"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
"$CLI" library check "$MINE" > /dev/null || ui_fail "the published library has errors"
[ -f "$MINE/previews/demo-block.png" ] || ui_fail "no preview image in the library"
git -C "$BARE" log --oneline main | grep -q "Add demo-block" || ui_fail "the version was not pushed"
echo "ok   the library checks, has a preview, and its version is in the remote"

echo "--- 2. Libraries"
ui_mark
ui_step "Libraries (command search)"       ui_command "Libraries..."
ui_focus_dialog '^Libraries$'
ui_expect_new "Library source https://github.com/mitcad/fasteners.git (on): not fetched" "the default sources listed"
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_expect_new "Library source removed: https://github.com/mitcad/community-index.git" "the defaults removed"
for url in "$LIB" "$INDEX"; do
  ui_step "Add URL (Alt+A)"                ui_key alt+a
  ui_focus_dialog '^Add Library$'
  ui_step "the URL"                        eval 'xdotool type --delay 10 "$url"; ui_key Return'
  ui_focus_dialog '^Libraries$'
done
ui_expect_new "Library source added: $INDEX" "the sources added"
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetched: library Mitcad metric fasteners (mitcad-fasteners) from $LIB; versions v1.1.0 (" \
  "the fastener library fetched" 30
ui_expect_new "Library fetched: index Test index () from $INDEX" "the index fetched" 30
ui_expect_new "Library source $LIB (on): Mitcad metric fasteners (mitcad-fasteners); versions v1.1.0" \
  "the source shows the library"
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main

echo "--- 3. Insert from Library"
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search 4762"                      xdotool type --delay 40 "4762"
ui_expect_new "Library search '4762': 1 components" "the search"
ui_expect_new "Library browser selected mitcad-fasteners/iso4762: version v1.1.0 (" "the newest version first"
ui_expect_new ", size M5x16, licence CC0-1.0" "the default size and the licence"
ui_step "the older version (Alt+V, Down)"  eval 'ui_key alt+v; ui_sync; ui_key Down'
ui_expect_new "Library browser selected mitcad-fasteners/iso4762: version v1.0.0 (" "v1.0.0 chosen"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component ISO 4762 M5x16 (linked) from mitcad-fasteners v1.0.0 (" "the screw inserted, linked"
ui_expect_volume "New body ISO 4762 M5x16 ([^)]*): volume \([0-9.]*\) mm3" 563.2433357349299 "the screw at v1.0.0"
ui_mark
ui_step "Insert from Library again"        ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search nut"                       xdotool type --delay 40 "hexagon nut"
ui_expect_new "Library browser selected mitcad-fasteners/iso4032: version v1.1.0 (" "the nut"
ui_step "a copy (Alt+P), Insert (Alt+I)"   eval 'ui_key alt+p; ui_sync; ui_key alt+i'
ui_focus_main
ui_expect_new "Inserted component ISO 4032 M5 (copy) from mitcad-fasteners v1.1.0 (" "the nut copied"

echo "--- 4. Library Parts"
ui_mark
ui_step "Library Parts (command search)"   ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_expect_new "Library part ISO 4762 M5x16: mitcad-fasteners v1.0.0 (" "the screw's recorded version"
ui_expect_new "size M5x16, a newer version is fetched" "a newer version is there, not taken"
ui_expect_new "Library part ISO 4032 M5: mitcad-fasteners v1.1.0 (" "the nut"
ui_expect_new "a copy: does not follow the library" "the copy does not follow"
ui_step "v1.1.0 (Alt+V, Up)"               eval 'ui_key alt+v; ui_sync; ui_key Up'
ui_expect_new "size M5x16, to update" "the change waits"
ui_step "Show Changes (Alt+S)"             ui_key alt+s
ui_expect_new "Library changes: ISO 4762 M5x16: version v1.0.0 -> v1.1.0" "what changes"
ui_expect_new "M5x16: dk 8.5 mm -> 8.7 mm" "the wider head"
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_focus_main
ui_expect_new "Updated library parts: ISO 4762 M5x16: version v1.0.0 (" "updated"
ui_expect_volume "Body ISO 4762 M5x16 ([^)]*): volume 563.243 -> \([0-9.]*\) mm3" 576.752184145366 "the screw at v1.1.0"
ui_mark
ui_step "Library Parts again"              ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_step "size M6 x 20"                     eval 'ui_key alt+v; ui_sync; ui_key Tab; xdotool type --delay 80 "M6";
  ui_sync; ui_key Tab; xdotool type --delay 80 "20"'
ui_expect_new "Library size M6x20" "the size chosen"
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_focus_main
ui_expect_new "Updated library parts: ISO 4762 M5x16: size M5x16 -> M6x20" "another size"
ui_expect_volume "ISO 4762 M6x20 ([^)]*): volume [0-9.]* -> \([0-9.]*\) mm3" 971.7736704007988 "the M6x20 screw"
ui_mark
ui_step "Library Parts: the parts list"    ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_expect_new "Parts list row: 1 x ISO 4762 M6x20 (mitcad-fasteners v1.1.0 (" "the parts list's screw"
ui_expect_new "Parts list row: 1 x ISO 4032 M5 (mitcad-fasteners v1.1.0 (" "the parts list's nut"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 5. Community Library"
ui_mark
ui_step "Community Library (command search)" ui_command "Community Library"
ui_focus_dialog '^Community Library$'
ui_expect_new "Library search '': 2 components, 1 libraries, 1 hidden by the licence" "the unlicensed library hidden"
ui_step "with items without a licence (Alt+W)" ui_key alt+w
ui_expect_new "Library search '': 2 components, 2 libraries, 0 hidden by the licence" "shown when asked"
ui_step "search bracket"                   eval 'ui_key alt+s; xdotool type --delay 40 "bracket"'
ui_expect_new "Library browser selected library test-brackets ($OTHER), licence none, not fetched" "the index's entry"
ui_step "Get Library (Alt+G)"              ui_key alt+g
ui_focus_dialog '^Get Library$'
ui_step "yes (Alt+Y)"                      ui_key alt+y
ui_focus_dialog '^Community Library$'
ui_expect_new "Library fetched: library Test brackets (test-brackets) from $OTHER" "the library fetched" 30
ui_step "its component (Alt+S, Down)"      eval 'ui_key alt+s; ui_sync; ui_key Down'
ui_expect_new "Library browser selected test-brackets/boss-bracket: version " "its component"
ui_expect_new "licence none" "no licence named"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component Boss bracket (linked) from test-brackets " "the community part inserted"
ui_expect_new "licence none" "no licence recorded"

ui_stop_app
ui_finish "component libraries and the community library"
