#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The part picker's group counts under the filters (mitcad#1015): a
# one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). It makes two libraries, the
# parts library's socket head cap screws and hexagon nuts (M5, with their
# suppliers' articles) with the generator and a small library of its own
# whose Wood/Beams and Wood/Sheets have no supplier, fetches them, opens
# Insert from Library and saves pictures on the light and the dark theme
# (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME):
#   <dir>/picker-groups-<theme>-all.png       no filter: every group with
#                                             its count
#   <dir>/picker-groups-<theme>-filtered.png  Wood chosen, then Supplier
#                                             Würth: Wood greyed with (0),
#                                             its empty subgroups hidden,
#                                             the fasteners with Würth's
#                                             counts
# The app's log as <dir>/look-<theme>.log, with the "Part picker groups:"
# lines (the last dump stays as <dir>/dialog.xwd).
#
# What to look for: each group row ends with its count (Favourites with
# none greyed); under Würth only All parts and General fasteners with its
# Screws and Nuts are left besides the chosen Wood, which is greyed with
# (0) (also while selected) and readable on both themes; the details
# panel says that Wood has no parts with these filters.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-picker-groups-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-picker-groups-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_LIBRARIES_DIR=$DIR/cache
LIB=$DIR/fasteners
WOOD=$DIR/wood
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The screws and nuts of size M5 with their suppliers' articles.
if [ ! -f "$LIB/mitcad-library.json" ]; then
  python3 "$ROOT/tools/libraries/make-fastener-library.py" --cli "$CLI" --out "$LIB" --sizes M5 \
    --standards iso4762,iso4032 --version 1.0.0 --no-render --commit > "$DIR/generator.log" 2>&1 ||
    { cat "$DIR/generator.log"; ui_fail "the generator failed"; }
fi
# Wood with its Beams and Sheets, of no supplier.
if [ ! -f "$WOOD/mitcad-library.json" ]; then
  mkdir -p "$WOOD/beams" "$WOOD/sheets"
  cp "$ROOT/tools/cli/tests/v1_boss.mitcad" "$WOOD/beams/beam.mitcad"
  cp "$ROOT/tools/cli/tests/v1_boss.mitcad" "$WOOD/sheets/plywood.mitcad"
  cat > "$WOOD/mitcad-library.json" <<'EOF'
{"format": "mitcad-library", "version": 1, "id": "look-wood", "name": "Look wood", "license": "MIT",
 "categories": [{"id": "wood", "name": "Wood"}, {"id": "beams", "name": "Beams", "parent": "wood"},
                {"id": "sheets", "name": "Sheets", "parent": "wood"}],
 "components": [
   {"id": "beam", "path": "beams/beam.mitcad", "name": "Beam look", "category": "beams"},
   {"id": "plywood", "path": "sheets/plywood.mitcad", "name": "Plywood look", "category": "sheets"}]}
EOF
  printf 'MIT\n' > "$WOOD/LICENSE"
  g "$WOOD" init -q
  g "$WOOD" add -A
  g "$WOOD" commit -q -m "Wood"
fi
for library in "$LIB" "$WOOD"; do
  "$CLI" library fetch "$library" --libraries "$MITCAD_LIBRARIES_DIR" --json > "$DIR/fetch.log" 2>&1 ||
    { cat "$DIR/fetch.log"; ui_fail "the library fetch of $library failed"; }
done

# shot name: Insert from Library's window as <dir>/picker-groups-<name>.png,
# the mouse away (no hover, no tooltip).
shot() {
  local window
  window=$(xdotool search --onlyvisible --name '^Insert from Library$' 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window Insert from Library"
  xdotool mousemove 5000 5000
  ui_wait_idle
  ui_sync "$window"
  sleep 2
  xwd -id "$window" -silent > "$DIR/dialog.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/dialog.xwd" "$DIR/picker-groups-$1.png"
  echo "ok   $DIR/picker-groups-$1.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --demo
  ui_expect_log "Asking the platform for the $theme colour scheme" "the $theme colour scheme asked for"
  ui_expect_log "Window style: $theme, " "the window's style is $theme"
  ui_mark
  ui_step "Insert from Library (command search)" ui_command "Insert from Library"
  ui_focus_dialog '^Insert from Library$'
  ui_expect_new "Part picker groups: " "the groups logged"
  ui_expect_new "Library browser selected " "the parts listed"
  grep "^Part picker groups: " "$UI_LOG" | tail -1
  shot "$theme-all"
  ui_mark
  ui_step "the group Wood (Alt+O, W)"        eval 'ui_key alt+o; ui_sync; ui_type_ahead "W"'
  ui_expect_new "Part picker group Wood: 2 parts" "Wood chosen"
  ui_mark
  ui_step "Supplier Würth (Alt+R, W)"        eval 'ui_key alt+r; ui_sync; ui_type_ahead "W"'
  ui_expect_new "Part picker supplier wurth" "the Supplier filter on Würth"
  ui_expect_new "Part picker groups: " "the groups under the filter"
  ui_expect_new "Library browser empty: No parts in Wood with this search and these filters." \
    "the details say why Wood is empty"
  grep "^Part picker groups: " "$UI_LOG" | tail -1
  shot "$theme-filtered"
  # The Supplier filter is kept in the settings: All again for the next
  # theme.
  ui_mark
  ui_step "All suppliers (Alt+R, All)"       eval 'ui_key alt+r; ui_sync; ui_type_ahead "All"'
  ui_expect_new "Part picker supplier all" "the Supplier filter off"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Part picker group counts look pictures in $DIR"
