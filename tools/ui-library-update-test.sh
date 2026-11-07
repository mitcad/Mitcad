#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/files app/library core/vcs/src/library core/ffi/src/libraries.rs tools/cli
# check-all sources: tools/libraries/make-profile-library.py tools/libraries/library_previews.py tools/libraries/library_tags.py tools/libraries/profiles/steel-tubes.json
# Update brings a library's newest tag to BUILD's part picker without a
# restart (mitcad#1423, GitHub mitcad/Mitcad#172: the versions were sorted
# as texts, so v2.10.0 and v2.11.0 came before v2.9.0, and after Update the
# picker still read v2.9.0). The parts library's ASTM A500 tubes, made here
# by tools/libraries/make-profile-library.py and published to a local bare
# repository:
#   1. v2.9.0 with the rectangular A500 tube only: Library Sources with the
#      bare repository in place of the default sources; Update in Browse
#      Components fetches it, and the log names v2.9.0 as the version the
#      pickers read. Profile's part picker reads mitcad-parts v2.9.0, and
#      A500 4x2x3/16 finds the rectangular tube.
#   2. v2.10.0 with the square A500 tube too, pushed with its tag while the
#      application runs: Update again; the log names v2.10.0 first and as
#      the version the pickers read. Profile's picker, opened again, reads
#      v2.10.0 and lists more rows, and A500 3x3x3/16 finds the square tube
#      HSS 3x3x3/16.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-library-update-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-library-update.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/mitcad-parts
BARE=$WORK/parts-remote.git
mkdir -p "$WORK/supplier"

# make_version ids version: the generator's library of these tubes (model
# parts: no supplier's files) in $LIB, recorded and tagged v<version>.
make_version() {
  python3 "$ROOT/tools/libraries/make-profile-library.py" --cli "$CLI" --source "$WORK/supplier" --out "$LIB" \
    --only "$1" --work "$WORK" --version "$2" --no-render --commit \
    > "$WORK/generator-$2.log" 2>&1 || { cat "$WORK/generator-$2.log"; ui_fail "the generator failed for $2"; }
}

# update version: Update in Browse Components (Insert from Library opens
# it on Installed): the library fetched from the bare repository with
# v<version> its newest version, the one the pickers read.
update() {
  ui_mark
  ui_step "Insert from Library (command search)" ui_command "Insert from Library"
  ui_focus_dialog '^Browse Components$'
  ui_mark
  ui_step "Update (Alt+U)"                 ui_key alt+u
  ui_expect_new "Library browser update: 1 enabled source(s)" "Update fetches the bare repository"
  ui_expect_new "Library fetched: library Mitcad parts (mitcad-parts) from $BARE; versions v$1 (" \
    "v$1 listed as the newest version" 60
  ui_expect_new "; the pickers read v$1 (" "the log names v$1 as the version the pickers read"
  ui_expect_new "Library fetch of 1 source(s): 0 failed" "no failures"
  ui_step "close (Escape)"                 ui_key Escape
  ui_focus_main
}

# picker version: Profile, then its part picker from the panel's Component
# tile, reading mitcad-parts at v<version>; ROWS the parts it opened with.
picker() {
  ui_mark
  ui_step "Profile (command search)"       ui_command "Profile"
  ui_expect_new "Command Profile started" "Profile started"
  ui_expect_new "BUILD parts profile read from mitcad-parts v$1 (" "Profile reads mitcad-parts v$1"
  ui_mark
  ui_step "the part picker (the Component tile)" ui_click_logged "Panel Profile input part"
  ui_expect_new "Profile: Component button" "the Component tile opens the picker"
  ui_expect_new "Part picker opened: " "the part picker"
  ROWS=$(sed -n 's/^Part picker opened: \([0-9]*\) parts for build.profile$/\1/p' "$UI_LOG" | tail -1)
  [ -n "$ROWS" ] && [ "$ROWS" -gt 0 ] || ui_fail "the picker opened with '${ROWS:-no}' parts"
  echo "ok   $ROWS tiles"
  ui_focus_dialog '^Component Picker - Profile$'
}

# close_picker: the picker closed (Escape), then Profile (Escape).
close_picker() {
  ui_step "close the picker (Escape)"      ui_key Escape
  ui_focus_main
  ui_step "close Profile (Escape)"         ui_key Escape
  ui_sync
}

# search TEXT: the picker's search (Alt+S) typed over with TEXT. RESULT is
# what the picker logged for it, the first "Part picker group <group>: <n>
# parts, first <tile>" after its "Part picker search '<TEXT>'" line from
# "<n> parts" on (as tools/ui-tube-picker-test.sh reads it).
type_search() { ui_key alt+s; ui_key ctrl+a; xdotool type --delay 40 "$1"; }
search() {
  local start after
  ui_mark
  ui_step "search '$1'"                    type_search "$1"
  ui_expect_new "Part picker search '$1'" "the picker's search '$1'"
  RESULT=""
  for _ in $(seq 1 50); do
    start=$(grep -nF "Part picker search '$1'" "$UI_LOG" | tail -1 | cut -d: -f1)
    after=$(tail -n +"$((start + 1))" "$UI_LOG")
    RESULT=$(grep -m1 -E '^Part picker group .*: [0-9]+ parts, first ' <<< "$after" |
      sed 's/^Part picker group .*: \([0-9]* parts, first \)/\1/' || true)
    [ -n "$RESULT" ] && return 0
    sleep 0.2
  done
  ui_fail "no group logged after the search '$1'"
}

make_version hss-rect-a500 2.9.0
git clone -q --bare "$LIB" "$BARE" || ui_fail "the bare repository was not made"

ui_start_display
ui_start_app --no-native-dialogs

echo "--- 1. v2.9.0: Library Sources, Update, Profile's picker"
ui_mark
ui_step "Library Sources (command search)" ui_command "Library Sources"
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library source https://github.com/mitcad/parts.git (on): not fetched" "the default sources listed"
ui_step "remove the defaults (Alt+R twice)" eval 'ui_key alt+r; ui_sync; ui_key alt+r'
ui_expect_new "Library source removed: https://github.com/mitcad/community-index.git" "the defaults removed"
ui_step "Add URL (Alt+A)"                  ui_key alt+a
ui_focus_dialog '^Add Library$'
ui_step "the bare repository"              eval 'ui_type_text "$BARE"; ui_key Return'
ui_focus_dialog '^Library Sources$'
ui_expect_new "Library source added: $BARE" "the bare repository added"
ui_step "close (Alt+C)"                    ui_key alt+c
ui_focus_main
update 2.9.0
picker 2.9.0
FIRST=$ROWS
search "A500 4x2x3/16"
[ "$RESULT" = "1 parts, first mitcad-parts/hss-rect-a500 (4x2x3/16)" ] ||
  ui_fail "A500 4x2x3/16 lists '$RESULT', not the rectangular tube HSS 4x2x3/16 alone"
echo "ok   A500 4x2x3/16: HSS 4x2x3/16"
close_picker

echo "--- 2. v2.10.0 pushed: Update again, Profile's picker reads it"
make_version hss-square-a500,hss-rect-a500 2.10.0
git -C "$LIB" push -q "$BARE" main v2.10.0 || ui_fail "v2.10.0 was not pushed"
update 2.10.0
picker 2.10.0
[ "$ROWS" -gt "$FIRST" ] || ui_fail "the picker lists $ROWS parts after Update, not more than the $FIRST of v2.9.0"
echo "ok   $ROWS tiles, $FIRST before"
search "A500 3x3x3/16"
[ "$RESULT" = "1 parts, first mitcad-parts/hss-square-a500 (3x3x3/16)" ] ||
  ui_fail "A500 3x3x3/16 lists '$RESULT', not the square tube HSS 3x3x3/16 alone"
echo "ok   A500 3x3x3/16: HSS 3x3x3/16, new in v2.10.0"
close_picker
ui_stop_app

ui_finish "UI library update test"
