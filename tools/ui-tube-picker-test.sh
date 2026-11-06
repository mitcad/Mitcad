#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library
# check-all sources: core/model/src/build core/model-build core/model-api/src core/vcs/src/library tools/cli
# check-all sources: tools/libraries/make-profile-library.py tools/libraries/library_previews.py tools/libraries/library_tags.py tools/libraries/profiles/steel-tubes.json
# The ASTM A500 tubes in BUILD's part picker (mitcad#1372, GitHub
# mitcad/Mitcad#159: a search for A500 found nothing in a library without
# them). The parts library's square and rectangular A500 tubes, made here by
# tools/libraries/make-profile-library.py from tools/libraries/profiles
# (--only, as the parts library makes them since 2.11.0), fetched from a
# local git repository:
#   1. Profile, the part picker from the panel's Component tile: every row
#      of both tubes a tile.
#   2. The search A500 lists them all.
#   3. A500 3x3x3/16: the square tube HSS 3x3x3/16 alone; A500 2x2x1/8
#      lists the square tube HSS 2x2x1/8; A500 4x2x3/16 the rectangular
#      tube HSS 4x2x3/16 alone.
# The Windows test is tools/ui-windows-tube-picker-test.ps1, with the same
# steps.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-tube-picker-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-tube-picker.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_LIBRARIES_DIR=$WORK/cache
LIB=$WORK/mitcad-parts

# The A500 tubes of the parts library (model parts: no supplier's files).
mkdir -p "$WORK/supplier"
python3 "$ROOT/tools/libraries/make-profile-library.py" --cli "$CLI" --source "$WORK/supplier" --out "$LIB" \
  --only hss-square-a500,hss-rect-a500 --work "$WORK" --version 1.0.0 --no-render --commit \
  > "$WORK/generator.log" 2>&1 || { cat "$WORK/generator.log"; ui_fail "the generator failed"; }
"$CLI" library fetch "$LIB" > "$WORK/fetch.log" 2>&1 || { cat "$WORK/fetch.log"; ui_fail "the library fetch failed"; }

# search TEXT: the picker's search (Alt+S) typed over with TEXT. RESULT is
# what the picker logged for it, the first "Part picker group <group>: <n>
# parts, first <tile>" after its "Part picker search '<TEXT>'" line from
# "<n> parts" on, TILES the tiles it then listed ("Part picker tiles: ").
type_search() { ui_key alt+s; ui_key ctrl+a; xdotool type --delay 40 "$1"; }
search() {
  local start after
  ui_mark
  ui_step "search '$1'"                    type_search "$1"
  ui_expect_new "Part picker search '$1'" "the picker's search '$1'"
  RESULT=""
  TILES=""
  for _ in $(seq 1 50); do
    start=$(grep -nF "Part picker search '$1'" "$UI_LOG" | tail -1 | cut -d: -f1)
    after=$(tail -n +"$((start + 1))" "$UI_LOG")
    RESULT=$(grep -m1 -E '^Part picker group .*: [0-9]+ parts, first ' <<< "$after" |
      sed 's/^Part picker group .*: \([0-9]* parts, first \)/\1/' || true)
    TILES=$(grep -m1 '^Part picker tiles: ' <<< "$after" || true)
    [ -n "$RESULT" ] && [ -n "$TILES" ] && return 0
    sleep 0.2
  done
  ui_fail "no group or tiles logged after the search '$1'"
}

ui_start_display
ui_start_app --no-native-dialogs

echo "--- 1. Profile's part picker"
ui_mark
ui_step "Profile (command search)"         ui_command "Profile"
ui_expect_new "Command Profile started" "Profile started"
ui_expect_new "BUILD parts profile: " "Profile lists the library's tubes"
ui_mark
ui_step "the part picker (the Component tile)" ui_click_logged "Panel Profile input part"
ui_expect_new "Profile: Component button" "the Component tile opens the picker"
ui_expect_new "Part picker opened: " "the part picker"
TUBES=$(sed -n 's/^Part picker opened: \([0-9]*\) parts for build.profile$/\1/p' "$UI_LOG" | tail -1)
[ -n "$TUBES" ] && [ "$TUBES" -gt 100 ] ||
  ui_fail "the picker opened with '${TUBES:-no}' parts, not the A500 tubes' rows"
echo "ok   $TUBES tiles, a row each"
ui_focus_dialog '^Component Picker - Profile$'

echo "--- 2. The search A500"
search "A500"
[[ $RESULT == "$TUBES parts, first mitcad-parts/hss-"* ]] ||
  ui_fail "A500 lists '$RESULT', not every one of the $TUBES tubes"
echo "ok   A500 lists all $TUBES tubes"

echo "--- 3. HSS 3x3, 2x2 and a rectangular one"
search "A500 3x3x3/16"
[ "$RESULT" = "1 parts, first mitcad-parts/hss-square-a500 (3x3x3/16)" ] ||
  ui_fail "A500 3x3x3/16 lists '$RESULT', not the square tube HSS 3x3x3/16 alone"
echo "ok   A500 3x3x3/16: HSS 3x3x3/16"
search "A500 2x2x1/8"
[[ $TILES == *"mitcad-parts/hss-square-a500 (2x2x1/8)"* ]] ||
  ui_fail "A500 2x2x1/8 lists '$TILES', without the square tube HSS 2x2x1/8"
echo "ok   A500 2x2x1/8: HSS 2x2x1/8 among $RESULT"
search "A500 4x2x3/16"
[ "$RESULT" = "1 parts, first mitcad-parts/hss-rect-a500 (4x2x3/16)" ] ||
  ui_fail "A500 4x2x3/16 lists '$RESULT', not the rectangular tube HSS 4x2x3/16 alone"
echo "ok   A500 4x2x3/16: HSS 4x2x3/16"
ui_step "close the picker (Escape)"        ui_key Escape
ui_stop_app

ui_finish "UI tube picker test"
