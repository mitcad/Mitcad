#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/library
# Component libraries (mitcad#64) and the community library (mitcad#63)
# through the real UI, with local git repositories only (no network):
#   1. Publish to Library: the demo block added to a new library of one's
#      own (made, recorded as a version), pushed to a bare repository, and
#      its entry for a community index.
#   2. Libraries: the default sources removed (they are not fetched), a
#      parts library's fasteners made by the generator (v1.0.0, then v1.1.0 with
#      wider M5 heads) and a community index added by URL, not fetched;
#      Auto-update is off and the start-up fetches nothing (mitcad#576).
#   2b. Insert from Library (mitcad#317): empty, its text says to press
#      Update; Update fetches both sources and lists the parts.
#   3. Insert from Library: a search for 4762, the default size M5x16's
#      suppliers' articles in their table (Supplier Articles, mitcad#979:
#      several articles, the columns they fill, Copy as TSV, Copy Part
#      Number, Open Link), the older version v1.0.0, linked: its volume;
#      a nut copied.
#   4. Library Parts: the screw still at v1.0.0 with a newer version
#      fetched, its articles' table (mitcad#979); v1.1.0 chosen, Show Changes names the wider head, Update:
#      the new volume; then another size (M6x20): renamed, its volume; the
#      parts list with the designations and licences.
#   5. Community Library: the index's library without a licence hidden
#      until asked for; Get Library fetches it after a question; its
#      component inserted with no licence named.
#   6. A source that is no library: Fetch All in Libraries... and Update in
#      the browser fetch the others and report it in one warning; Update
#      searches again and keeps the selected part.
#   7. The part picker's grid in Insert from Library (mitcad#418): In this
#      design lists the screw, a group (Screws), a search inside it, a star
#      (the nut first in All parts), Favourites, a part inserted from
#      there, and In this design after the placement.
#   8. The groups as a tree (mitcad#527): two more libraries, one's Wood
#      (a parent by id) and the other's Wood/Sheets one group listing both
#      libraries' parts, a screw in two groups (Slot profile/Screws and
#      General fasteners/Screws, beside the ISO screw) and once in
#      Favourites, Wood folded and still folded when the browser opens
#      again, then unfolded.
#   9. The Standard filter (mitcad#949): Standard = DIN 912 on the socket
#      head cap screw narrows its lengths (M6x8 offered, the non-standard
#      M6x6 not), and the part inserted is named DIN 912 M6x8.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-library-test.sh
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs
# check-all sources: tools/cli tools/libraries

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

# The parts library's fasteners: v1.0.0, then v1.1.0 with the M5 heads 8.7 mm wide.
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
# The community index: the parts library and the brackets.
mkdir -p "$INDEX/libraries"
printf '{"format": "mitcad-index", "version": 1, "name": "Test index"}\n' > "$INDEX/mitcad-index.json"
"$CLI" library index-entry "$LIB" --url "$LIB" > "$INDEX/libraries/mitcad-parts.json" || ui_fail "index entry"
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
ui_step "the folder"                       eval 'ui_key alt+f; ui_type_text "$MINE"'
ui_step "the library's id"                 eval 'ui_key alt+i; ui_type_text "my-parts"'
ui_step "the library's name"               eval 'ui_key alt+n; ui_type_text "My parts"'
ui_step "the author"                       eval 'ui_key alt+a; ui_type_text "Library Tester"'
ui_step "the component's id"               eval 'ui_key alt+d; ui_type_text "demo-block"'
ui_step "its name"                         eval 'ui_key alt+m; ui_type_text "Demo block"'
ui_step "its category"                     eval 'ui_key alt+g; ui_type_text "blocks"'
ui_step "its tags"                         eval 'ui_key alt+t; ui_type_text "block, demo"'
ui_step "Add to Library (Alt+L)"           ui_key alt+l
ui_expect_new "Publish: Made the library my-parts in $MINE" "the library made"
ui_expect_new "Publish: Added demo-block to the library, recorded as version " "added and recorded"
ui_step "the remote"                       eval 'ui_key alt+r; ui_type_text "$BARE"'
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
ui_expect_new "Library source https://github.com/mitcad/parts.git (on): not fetched" "the default sources listed"
ui_expect_new "Library source https://github.com/mitcad/parts.git (on): not fetched; versions ; auto-update off" \
  "Auto-update is off for the defaults (mitcad#576)"
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_expect_new "Library source removed: https://github.com/mitcad/community-index.git" "the defaults removed"
for url in "$LIB" "$INDEX"; do
  ui_step "Add URL (Alt+A)"                ui_key alt+a
  ui_focus_dialog '^Add Library$'
  ui_step "the URL"                        eval 'ui_type_text "$url"; ui_key Return'
  ui_focus_dialog '^Libraries$'
done
ui_expect_new "Library source added: $INDEX" "the sources added"
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
# No source has Auto-update: the start-up did no fetch (mitcad#576).
grep -qF "Library auto-update" "$UI_LOG" && ui_fail "a start-up fetch ran although no source has Auto-update"
echo "ok   no start-up fetch without Auto-update"

echo "--- 2b. Insert from Library: empty until Update (mitcad#317)"
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_expect_new "Library search '': 0 components, 0 libraries" "nothing listed before a fetch"
ui_expect_new "Library browser empty: nothing fetched yet, press Update" "the empty list says to press Update"
ui_mark
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_expect_new "Library browser update: 2 enabled source(s)" "Update fetches the enabled sources"
ui_expect_new "Library fetched: library Mitcad parts (mitcad-parts) from $LIB; versions v1.1.0 (" \
  "the parts library fetched" 30
ui_expect_new "Library fetched: index Test index () from $INDEX" "the index fetched" 30
ui_expect_new "Library fetch of 2 source(s): 0 failed" "no failures"
ui_expect_new "Library browser selected mitcad-parts/" "the results list the library's parts"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^Library search '': [1-9][0-9]* components" ||
  ui_fail "no components listed after Update"
echo "ok   the search ran again with the parts"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 3. Insert from Library"
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search 4762"                      xdotool type --delay 40 "4762"
ui_expect_new "Library search '4762': 1 components" "the search"
ui_expect_new "Library browser selected mitcad-parts/iso4762: version v1.1.0 (" "the newest version first"
ui_expect_new ", size M5x16, licence CC0-1.0" "the default size and the licence"
# The suppliers' articles of M5x16 (the generator's Würth and Etra
# articles) in their table, not as one line (mitcad#979).
ui_mark
ui_step "Supplier Articles (Alt+A)"        ui_key alt+a
ui_focus_dialog '^Supplier Articles - ISO 4762 M5x16$'
ui_expect_new "Supplier parts of M5x16: " "the table of M5x16's articles"
ARTICLES=$(grep "^Supplier parts of M5x16: " "$UI_LOG" | tail -1)
COUNT=$(sed -n 's/^Supplier parts of M5x16: \([0-9]*\) articles, .*$/\1/p' <<< "$ARTICLES")
[ "${COUNT:-0}" -ge 2 ] || ui_fail "M5x16 has not several articles: '$ARTICLES'"
[[ $ARTICLES == *", columns Supplier, Part number, Material, Finish, Checked" ]] ||
  ui_fail "not the columns the articles fill: '$ARTICLES'"
echo "ok   $ARTICLES"
ui_step "Copy as TSV (Alt+T)"              ui_key alt+t
ui_expect_new "Supplier articles copied: $COUNT rows" "every article copied"
ui_step "Copy Part Number (Alt+P)"         ui_key alt+p
ui_expect_new "Supplier part number copied: " "the first article's part number on its own"
ui_step "Open Link (Alt+L)"                ui_key alt+l
ui_expect_new "Supplier link opened: https://" "the supplier's site"
ui_expect_new "Open URL (test, not opened): https://" "opened in the browser (logged in the tests)"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_dialog '^Insert from Library$'
ui_step "the older version (Alt+V, Down)"  eval 'ui_key alt+v; ui_sync; ui_key Down'
ui_expect_new "Library browser selected mitcad-parts/iso4762: version v1.0.0 (" "v1.0.0 chosen"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component ISO 4762 M5x16 (linked) from mitcad-parts v1.0.0 (" "the screw inserted, linked"
ui_expect_volume "New body ISO 4762 M5x16 ([^)]*): volume \([0-9.]*\) mm3" 560.1463214427537 "the screw at v1.0.0"
ui_mark
ui_step "Insert from Library again"        ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search nut"                       xdotool type --delay 40 "hexagon nut"
ui_expect_new "Library browser selected mitcad-parts/iso4032: version v1.1.0 (" "the nut"
ui_step "a copy (Alt+P), Insert (Alt+I)"   eval 'ui_key alt+p; ui_sync; ui_key alt+i'
ui_focus_main
ui_expect_new "Inserted component ISO 4032 M5 (copy) from mitcad-parts v1.1.0 (" "the nut copied"

echo "--- 4. Library Parts"
ui_mark
ui_step "Library Parts (command search)"   ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_expect_new "Library part ISO 4762 M5x16: mitcad-parts v1.0.0 (" "the screw's recorded version"
ui_expect_new "size M5x16, a newer version is fetched" "a newer version is there, not taken"
ui_expect_new "Library part ISO 4032 M5: mitcad-parts v1.1.0 (" "the nut"
ui_expect_new "a copy: does not follow the library" "the copy does not follow"
# The screw's articles at its recorded version (mitcad#979).
ui_mark
ui_step "Supplier Articles (Alt+A)"        ui_key alt+a
ui_focus_dialog '^Supplier Articles - '
ui_expect_new "Supplier parts of M5x16: $COUNT articles, columns Supplier, Part number, Material, Finish, Checked" \
  "the screw's articles in Library Parts"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_dialog '^Library Parts$'
ui_step "v1.1.0 (Alt+V, Up)"               eval 'ui_key alt+v; ui_sync; ui_key Up'
ui_expect_new "size M5x16, to update" "the change waits"
ui_step "Show Changes (Alt+S)"             ui_key alt+s
ui_expect_new "Library changes: ISO 4762 M5x16: version v1.0.0 -> v1.1.0" "what changes"
ui_expect_new "M5x16: dk 8.5 mm -> 8.7 mm" "the wider head"
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_focus_main
ui_expect_new "Updated library parts: ISO 4762 M5x16: version v1.0.0 (" "updated"
ui_expect_volume "Body ISO 4762 M5x16 ([^)]*): volume 560.146 -> \([0-9.]*\) mm3" 573.55034687 "the screw at v1.1.0"
ui_mark
ui_step "Library Parts again"              ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_step "size M6 x 20"                     eval 'ui_key alt+v; ui_sync; ui_key Tab; xdotool type --delay 80 "M6";
  ui_sync; ui_key Tab; xdotool type --delay 80 "20"'
ui_expect_new "Library size M6x20" "the size chosen"
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_focus_main
ui_expect_new "Updated library parts: ISO 4762 M5x16: size M5x16 -> M6x20" "another size"
ui_expect_volume "ISO 4762 M6x20 ([^)]*): volume [0-9.]* -> \([0-9.]*\) mm3" 966.3332369294 "the M6x20 screw"
ui_mark
ui_step "Library Parts: the parts list"    ui_command "Library Parts"
ui_focus_dialog '^Library Parts$'
ui_expect_new "Parts list row: 1 x ISO 4762 M6x20 (mitcad-parts v1.1.0 (" "the parts list's screw"
ui_expect_new "Parts list row: 1 x ISO 4032 M5 (mitcad-parts v1.1.0 (" "the parts list's nut"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 5. Community Library"
ui_mark
ui_step "Community Library (command search)" ui_command "Community Library"
ui_focus_dialog '^Community Library$'
ui_expect_new "Library search '': 2 components, 1 libraries, 1 hidden by the licence" "the unlicensed library hidden"
ui_step "with items without a licence (Alt+W)" ui_key alt+w
ui_expect_new "Library search '': 2 components, 2 libraries, 0 hidden by the licence" "shown when asked"
ui_step "search bracket"                   eval 'ui_key alt+s; ui_type_text "bracket"'
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

echo "--- 6. A broken source: one warning (Fetch All and Update, mitcad#317)"
BROKEN=$WORK/no-such-library
ui_mark
ui_step "Libraries (command search)"       ui_command "Libraries..."
ui_focus_dialog '^Libraries$'
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "a folder that is no library"      eval 'ui_type_text "$BROKEN"; ui_key Return'
ui_focus_dialog '^Libraries$'
ui_expect_new "Library source added: $BROKEN" "the broken source added"
ui_step "Fetch All (Alt+F)"                ui_key alt+f
ui_expect_new "Library fetch failed: $BROKEN: " "its fetch fails" 30
ui_expect_new "Library fetch of 4 source(s): 1 failed" "the others fetched" 30
ui_expect_new "Library source $LIB (on): Mitcad parts (mitcad-parts); versions v1.1.0" \
  "the sources shown again"
ui_focus_dialog '^Fetch Libraries$'
ui_step "close the warning (Return)"       ui_key Return
ui_focus_dialog '^Libraries$'
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "search 4762"                      xdotool type --delay 40 "4762"
ui_expect_new "Library browser selected mitcad-parts/iso4762: version v1.1.0 (" "the screw selected"
ui_mark
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_expect_new "Library browser update: 4 enabled source(s)" "Update fetches every enabled source"
ui_expect_new "Library fetch failed: $BROKEN: " "the broken one fails" 30
ui_expect_new "Library fetch of 4 source(s): 1 failed" "one failure" 30
ui_expect_new "Library search '4762': 1 components" "the search ran again"
ui_expect_new "Library browser kept the selection component|" "the selection kept"
ui_focus_dialog '^Fetch Libraries$'
ui_step "close the warning (Return)"       ui_key Return
ui_focus_dialog '^Insert from Library$'
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 7. The part picker in Insert from Library (mitcad#418)"
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_expect_new "Part picker opened: " "the results as the picker's grid"
ui_expect_new "Part picker in this design: " "the design's library parts read"
ui_expect_new "mitcad-parts/iso4762 x1" "the inserted screw in this design, once"
ui_mark
ui_step "the group Screws (Alt+O, S)"      eval 'ui_key alt+o; ui_sync; ui_type_ahead "S"'
ui_expect_new "Part picker group General fasteners/Screws: 1 parts, first mitcad-parts/iso4762" "only the screws"
ui_mark
ui_step "search 4032 in Screws"            eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "4032"'
ui_expect_new "Library search '4032': 1 components" "the search"
ui_expect_new "Part picker group General fasteners/Screws: 0 parts, first -" "the nut is not a screw"
ui_step "clear the search"                 eval 'ui_key ctrl+a; ui_key BackSpace'
ui_mark
ui_step "All parts (Alt+O, A)"             eval 'ui_key alt+o; ui_sync; ui_type_ahead "A"'
ui_expect_new "Part picker group All parts: " "every part"
ui_step "search hexagon nut"               eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "hexagon nut"'
ui_expect_new "Library browser selected mitcad-parts/iso4032: version " "the nut chosen in the grid"
ui_mark
ui_step "star it (Ctrl+D)"                 ui_key ctrl+d
ui_expect_new "Part picker star iso4032 on" "the nut starred"
ui_expect_new "Part picker tiles kept: " "a star changes only the star: the tiles stay (mitcad#517)"
ui_expect_new "; current mitcad-parts/iso4032" "the selection stays on the starred nut"
ui_mark
ui_step "clear the search"                 eval 'ui_key ctrl+a; ui_key BackSpace'
ui_expect_new "Library search '': " "every part again"
ui_expect_new "first mitcad-parts/iso4032" "the starred nut first"
ui_mark
ui_step "Favourites (Alt+O, F)"            eval 'ui_key alt+o; ui_sync; ui_type_ahead "F"'
ui_expect_new "Part picker group Favourites: 1 parts, first mitcad-parts/iso4032" "Favourites has the nut"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Part picker chose mitcad-parts/iso4032" "chosen in the picker"
# The nut copied in 3. has the name: the new component is "(1)".
ui_expect_new "Inserted component ISO 4032 M5 (1) (linked) from mitcad-parts " "the nut inserted"
ui_mark
ui_step "Insert from Library again"        ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_expect_new "mitcad-parts/iso4032 x2" "the nut in this design after the placement: the copy and this one"
ui_mark
ui_step "In this design (Alt+O, I)"        eval 'ui_key alt+o; ui_sync; ui_type_ahead "I"'
ui_expect_new "Part picker group In this design: " "the design's parts as a group"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 8. The groups as a tree (mitcad#527)"
# Two libraries more: Wood (a parent by id) with its Beams, and a screw
# made for the slot in two groups, Slot profile/Screws and General
# fasteners/Screws (the parts library's group of that name); the other's
# Wood/Sheets in the same Wood.
WOOD=$WORK/wood
SHEETS=$WORK/sheets
mkdir -p "$WOOD/beams" "$WOOD/slot-screws" "$SHEETS/sheets"
cp "$ROOT/tools/cli/tests/v1_boss.mitcad" "$WOOD/beams/beam.mitcad"
cp "$ROOT/tools/cli/tests/v1_boss.mitcad" "$WOOD/slot-screws/slot-screw.mitcad"
cp "$ROOT/tools/cli/tests/v1_boss.mitcad" "$SHEETS/sheets/plywood.mitcad"
cat > "$WOOD/mitcad-library.json" <<'EOF'
{"format": "mitcad-library", "version": 1, "id": "test-wood", "name": "Test wood", "license": "MIT",
 "categories": [{"id": "wood", "name": "Wood"}, {"id": "beams", "name": "Beams", "parent": "wood"},
                {"id": "slot-screws", "name": "Slot profile/Screws"},
                {"id": "general-screws", "name": "General fasteners/Screws"}],
 "components": [
   {"id": "beam", "path": "beams/beam.mitcad", "name": "Beam test", "category": "beams"},
   {"id": "slot-screw", "path": "slot-screws/slot-screw.mitcad", "name": "Slot screw test",
    "category": ["slot-screws", "general-screws"]}]}
EOF
cat > "$SHEETS/mitcad-library.json" <<'EOF'
{"format": "mitcad-library", "version": 1, "id": "test-sheets", "name": "Test sheets", "license": "MIT",
 "categories": [{"id": "sheets", "name": "Wood/Sheets"}],
 "components": [{"id": "plywood", "path": "sheets/plywood.mitcad", "name": "Plywood test", "category": "sheets"}]}
EOF
for TREE_LIB in "$WOOD" "$SHEETS"; do
  printf 'MIT\n' > "$TREE_LIB/LICENSE"
  "$CLI" library check "$TREE_LIB" > /dev/null || ui_fail "the library $TREE_LIB has errors"
  g "$TREE_LIB" init -q
  g "$TREE_LIB" add -A
  g "$TREE_LIB" commit -q -m "Parts"
done
ui_mark
ui_step "Libraries (command search)"       ui_command "Libraries..."
ui_focus_dialog '^Libraries$'
for url in "$WOOD" "$SHEETS"; do
  ui_step "Add URL (Alt+A)"                ui_key alt+a
  ui_focus_dialog '^Add Library$'
  ui_step "the URL"                        eval 'ui_type_text "$url"; ui_key Return'
  ui_focus_dialog '^Libraries$'
done
ui_expect_new "Library source added: $SHEETS" "the two sources added"
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_mark
ui_step "Update (Alt+U)"                   ui_key alt+u
ui_expect_new "Library fetched: library Test wood (test-wood) from $WOOD" "the wood library fetched" 30
ui_expect_new "Library fetched: library Test sheets (test-sheets) from $SHEETS" "the sheets fetched" 30
ui_expect_new "source(s): 1 failed" "only the broken source fails" 30
ui_focus_dialog '^Fetch Libraries$'
ui_step "close the warning (Return)"       ui_key Return
ui_focus_dialog '^Insert from Library$'
ui_expect_new ", Wood/Sheets" "the groups as a tree, the other library's Sheets in the same Wood"
GROUPS_TREE=$(grep "^Part picker groups: " "$UI_LOG" | tail -1 | sed 's/^Part picker groups: //')
for group in "Wood" "Wood/Beams" "Wood/Sheets" "Slot profile" "Slot profile/Screws" "General fasteners" \
             "General fasteners/Screws" "General fasteners/Nuts"; do
  [[ ", $GROUPS_TREE, " == *", $group, "* ]] || ui_fail "no group $group in '$GROUPS_TREE'"
done
echo "ok   the groups as a tree: $GROUPS_TREE"
ui_mark
ui_step "the group Wood (Alt+O, W)"        eval 'ui_key alt+o; ui_sync; ui_type_ahead "W"'
ui_expect_new "Part picker group Wood: 2 parts" "Wood lists its subgroups' parts, of both libraries"
ui_mark
# Typed apart from the W above (ui_type_ahead): Qt would look for "WSlot".
ui_step "Slot profile's Screws (Alt+O, Slot, Down)" \
  eval 'ui_key alt+o; ui_sync; ui_type_ahead "Slot"; ui_sync; ui_key Down'
ui_expect_new "Part picker group Slot profile/Screws: 1 parts, first test-wood/slot-screw" \
  "the slot screw in Slot profile"
ui_mark
ui_step "General fasteners' Screws (Alt+O, General, Down)" \
  eval 'ui_key alt+o; ui_sync; ui_type_ahead "General"; ui_sync; ui_key Down'
ui_expect_new "Part picker group General fasteners/Screws: 2 parts" "the slot screw there too, beside the ISO screw"
ui_mark
ui_step "search slot screw"                eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "slot screw"'
# Only the slot screw is left, current (the browser logs a selection only
# when it changes: it may be current already); the star names it.
ui_expect_new "Part picker group General fasteners/Screws: 1 parts, first test-wood/slot-screw" "the slot screw found"
ui_mark
ui_step "star it (Ctrl+D)"                 ui_key ctrl+d
ui_expect_new "Part picker star slot-screw on" "the slot screw starred"
ui_step "clear the search"                 eval 'ui_key ctrl+a; ui_key BackSpace'
ui_mark
ui_step "Favourites (Alt+O, F)"            eval 'ui_key alt+o; ui_sync; ui_type_ahead "F"'
ui_expect_new "Part picker group Favourites: 2 parts" "the nut and the slot screw"
TILES=$(grep "^Part picker tiles: " "$UI_LOG" | tail -1)
[ "$(grep -o 'test-wood/slot-screw' <<< "$TILES" | wc -l)" -eq 1 ] ||
  ui_fail "the slot screw is not in Favourites once: '$TILES'"
echo "ok   a part in two groups is one tile in Favourites"
ui_mark
ui_step "fold Wood (Alt+O, W, Left)"       eval 'ui_key alt+o; ui_sync; ui_type_ahead "W"; ui_sync; ui_key Left'
ui_expect_new "Part picker group Wood folded" "Wood folded"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "Insert from Library again"        ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_expect_new "Wood (folded)" "the fold remembered"
GROUPS_TREE=$(grep "^Part picker groups: " "$UI_LOG" | tail -1)
[[ $GROUPS_TREE != *"Wood/Beams"* ]] || ui_fail "a folded group's subgroups are shown: '$GROUPS_TREE'"
ui_mark
ui_step "unfold it (Alt+O, W, Right)"      eval 'ui_key alt+o; ui_sync; ui_type_ahead "W"; ui_sync; ui_key Right'
ui_expect_new "Part picker group Wood unfolded" "Wood unfolded"
ui_step "close (Escape)"                   ui_key Escape
ui_focus_main

echo "--- 9. The Standard filter (mitcad#949)"
# The generator's socket head cap screws name their rows' standards: with
# Standard = DIN 912 the lengths are those of DIN 912 (M6x8, which ISO 4762
# lacks, and not the non-standard M6x6), and the part inserted is named by
# that standard.
ui_mark
ui_step "Insert from Library (command search)" ui_command "Insert from Library"
ui_focus_dialog '^Insert from Library$'
ui_step "All parts (Alt+O, A)"             eval 'ui_key alt+o; ui_sync; ui_type_ahead "A"'
ui_mark
ui_step "search 4762"                      eval 'ui_key alt+s; ui_sync; xdotool type --delay 40 "4762"'
ui_expect_new "Library browser selected mitcad-parts/iso4762: version v1.1.0 (" "the socket head cap screw"
ui_mark
ui_step "Standard DIN 912 (Alt+D)"         eval 'ui_key alt+d; ui_sync; ui_type_ahead "DIN"'
ui_expect_new "Library filter: Standard DIN 912, Supplier All: " "the Standard filter narrows the rows"
ui_mark
# Size M6: the field before Length, which is before Linked.
ui_step "size M6 (Alt+L, Shift+Tab twice)" eval 'ui_key alt+l; ui_sync; ui_key shift+Tab; ui_sync; ui_key shift+Tab;
  ui_sync; xdotool type --delay 80 "M6"'
ui_expect_new "Library Length choices (Standard DIN 912, Supplier All): " "the lengths of M6 narrowed"
LENGTHS=$(grep "^Library Length choices (Standard DIN 912, Supplier All): " "$UI_LOG" | tail -1 | sed 's/^.*): //')
[[ ", $LENGTHS, " == *", 8, "* ]] || ui_fail "DIN 912's M6x8 is not offered: '$LENGTHS'"
[[ ", $LENGTHS, " != *", 6, "* ]] || ui_fail "the non-standard M6x6 is offered with DIN 912: '$LENGTHS'"
[[ $LENGTHS == "8, "* ]] || ui_fail "M6x8 is not DIN 912's shortest M6: '$LENGTHS'"
echo "ok   the lengths of M6 with DIN 912: $LENGTHS"
ui_mark
# The first length (8): Home, since a typed 8 would look on from the
# current length to 80.
ui_step "length 8 (Tab, Home)"             eval 'ui_key Tab; ui_sync; ui_key Home'
ui_expect_new "Library size M6x8" "M6x8 chosen"
ui_expect_new "Library browser row M6x8: designation DIN 912 M6x8" "the designation follows the standard"
ui_step "Insert (Alt+I)"                   ui_key alt+i
ui_focus_main
ui_expect_new "Inserted component DIN 912 M6x8 (linked) from mitcad-parts v1.1.0" "the part named by DIN 912"

ui_stop_app
ui_finish "component libraries and the community library"
