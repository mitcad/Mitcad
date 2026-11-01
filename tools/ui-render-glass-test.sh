#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/commands/MaterialCommands.cpp app/browser/LibraryPicker.cpp app/framework/Appearances.cpp
# Glass given to an imported body in the rendered view (mitcad#319,
# docs/rendering.md): an STL cube whose triangles are wound inwards (the
# file's own triangulation, as an imported body keeps it), imported into a
# design by mitcad-cli, without the ground:
#   - The rendered view turns the cube's triangles round to face outwards
#     ("Render mesh ...: 12 of 12 triangles turned").
#   - The Appearance tool assigns Glass - Clear while the view renders: the
#     body's material is swapped live (logged with its mesh reused), and
#     the cube renders as glass at once: not black, as bright as the same
#     design renders when it is opened again with the glass.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-glass-test.sh
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-render-glass.XXXXXX")
FILE=$WORK/cube.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_RENDER_SAMPLES=16
mkdir -p "$UI_CONFIG/Mitcad"
printf '[render]\ndevice=cpu\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

# A 40 mm cube (corners 0 and 40) as an ASCII STL file, each triangle wound
# clockwise seen from outside (inwards).
corner() { echo "$((40 * ($1 & 1))) $((40 * ($1 >> 1 & 1))) $((40 * ($1 >> 2 & 1)))"; }
{
  echo "solid inward"
  # The six sides as corner numbers (bit 0: x, bit 1: y, bit 2: z), each
  # counter-clockwise seen from outside, then written the other way round.
  for side in "0 2 3 1" "4 5 7 6" "0 1 5 4" "2 6 7 3" "0 4 6 2" "1 3 7 5"; do
    read -r a b c d <<< "$side"
    for triangle in "$a $c $b" "$a $d $c"; do
      echo "  facet normal 0 0 0"
      echo "    outer loop"
      for k in $triangle; do
        echo "      vertex $(corner "$k")"
      done
      echo "    endloop"
      echo "  endfacet"
    done
  done
  echo "endsolid inward"
} > "$WORK/inward.stl"
cat > "$WORK/model.json" << EOF
[
  {"cmd": "import_file", "path": "$WORK/inward.stl"},
  {"cmd": "set_render_settings", "ground": {"shadows": false}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# brightness_after mark description: once the view's frames after the log
# line <mark> reach all samples and stop coming, the brightness of what the
# last one covers ("Render brightness view ...").
brightness_after() {
  local line last frames
  for _ in $(seq 1 300); do
    tail -n +$(($1 + 1)) "$UI_LOG" | grep -qE "^Render brightness view [0-9]+ samples $MITCAD_RENDER_SAMPLES: " && break
    ui_crashed && ui_fail "$2: the app crashed"
    sleep 0.2
  done
  last=-1
  for _ in $(seq 1 20); do
    frames=$(tail -n +$(($1 + 1)) "$UI_LOG" | grep -c "^Render brightness view ")
    [ "$frames" = "$last" ] && break
    last=$frames
    sleep 1
  done
  line=$(tail -n +$(($1 + 1)) "$UI_LOG" | grep -E "^Render brightness view [0-9]+ samples $MITCAD_RENDER_SAMPLES: " | tail -1)
  [ -n "$line" ] || ui_fail "$2: no frame with all $MITCAD_RENDER_SAMPLES samples"
  sed -n 's/.*: \([0-9.]*\) over \([0-9]*\) pixels$/\1 \2/p' <<< "$line"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "the imported cube opened"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Bodies/" "the cube is in the browser"
ROW=$(grep -oE "^Browser Root/Bodies/[^/]+ at" "$UI_LOG" | head -1 | sed 's/ at$//')
echo "     the cube's row: $ROW"

echo "--- Rendered: the cube's triangles turned outwards"
ui_mark
ui_step "View > Rendered"                ui_command "Rendered"
ui_expect_new "Render worker ready: Cycles" "the worker runs Cycles" 30
ui_expect_new ": 12 of 12 triangles turned to face outwards" "the inward triangles turned round"
BODY=$(grep -oE "^Render mesh [^:]+: 12 of 12 triangles" "$UI_LOG" | tail -1 | sed 's/^Render mesh \([^:]*\):.*/\1/')
read -r opaque _ <<< "$(brightness_after "$UI_MARK" "the cube rendered")"
echo "ok   the cube rendered, brightness $opaque"

echo "--- Glass - Clear assigned while rendering"
ui_step "select the cube's row"          ui_click_logged "$ROW"
ui_mark
ui_step "Appearance (search)"            ui_command "Appearance"
ui_expect_new "Appearance Bodies: 1 body [body $BODY]" "the tool is for the cube"
ui_step "the picker: Glass - Clear"      ui_picker_choose "Panel Appearance input value" "Appearance picker" "Glass - Clear"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance glass_clear for 1 body(ies)" "assigned"
ui_expect_new "Render material swap $BODY: default (transmission 0, opacity 1) -> glass_clear (transmission 1" \
  "the live material swap is logged"
ui_expect_new "; mesh reused (the worker holds it)" "with the cube's mesh reused"
ui_expect_new "changed $BODY;" "the worker changed the cube's material"
applied=$(grep -n "changed $BODY;" "$UI_LOG" | tail -1 | cut -d: -f1)
read -r live livePixels <<< "$(brightness_after "$applied" "the glass rendered")"
echo "     live glass: brightness $live over $livePixels pixels"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_log "Saved $FILE" "saved with the glass"

echo "--- The design opened again"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "opened again"
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "View > Rendered"                ui_command "Rendered"
ui_expect_new "Render body $BODY: appearance glass_clear" "the cube goes as glass"
read -r reopened reopenedPixels <<< "$(brightness_after "$UI_MARK" "the reopened glass rendered")"
echo "     reopened glass: brightness $reopened over $reopenedPixels pixels"
awk -v l="$live" -v r="$reopened" 'BEGIN {
  d = l - r; if (d < 0) d = -d
  exit !(l > 0.05 && r > 0.05 && d <= 0.3 * r + 0.02) }' ||
  ui_fail "the glass assigned live (brightness $live) does not render as the reopened design's ($reopened)"
echo "ok   the glass renders at once as after opening the design again ($live and $reopened)"
grep -q "Render worker error" "$UI_LOG" && ui_fail "the worker reported an error"

ui_finish "glass on an imported body in the rendered view"
