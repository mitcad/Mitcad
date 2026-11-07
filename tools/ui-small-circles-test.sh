#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/SmallCirclesPreference.cpp app/view/ViewTessellation.hpp
# check-all sources: app/framework/DisplayMeshes.cpp app/framework/DocumentSession.cpp app/MainWindowDisplayMeshes.cpp
# check-all sources: app/OcctViewer.cpp app/main.cpp geometry/io/src/mesh.cpp tools/cli
# Small circles (mitcad#879) through the real UI, on a plate that
# mitcad-cli makes: 60 x 40 x 3 with a hole of 2 mm through it.
#   1. The app starts with the view's small circles Adaptive (the default).
#   2. Preferences > Display > Small circles: Full Detail (kept in the
#      settings); the open design's shown body is meshed again ("1 bodies
#      meshed again with small circles full detail"), with more triangles
#      than in Adaptive.
#   3. The app again: it starts with Full Detail; Preferences back to
#      Adaptive meshes the body again with fewer triangles than in Full
#      Detail.
# tools/ui-windows-small-circles-test.ps1 is the Windows counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-small-circles-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-small-circles.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/plate.mitcad
CONF=$UI_CONFIG/Mitcad/Mitcad.conf

cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 3}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 20], "diameter": 2},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# small_circles keys "logged line": Preferences > Display, Small circles
# (Alt+M) moved by the keys, OK.
small_circles() {
  local logged=${*: -1}
  ui_mark
  ui_step "preferences, Display (search)"  ui_command "Preferences: Display"
  ui_focus_dialog '^Preferences$'
  ui_step "Small circles (Alt+M)"          ui_key alt+m
  ui_expect_new "Preferences small circles focused: " "the keyboard in Small circles"
  for key in "${@:1:$#-1}"; do
    ui_step "key $key"                     ui_key "$key"
  done
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "$logged" "${logged#Preferences: }"
}

# remeshed choice: waits for the body meshed again with `choice` and prints
# its triangles (only them on standard output).
remeshed() {
  local line
  ui_expect_new "1 bodies meshed again with small circles $1 in " "the shown body meshed again ($1)" 60 >&2
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "meshed again with small circles $1 in [0-9]* ms ([0-9]* triangles)" | tail -1)
  sed -E 's/.*\(([0-9]+) triangles\)/\1/' <<< "$line"
}

ui_start_display

echo "--- Adaptive by default"
ui_start_app --open "$FILE"
ui_expect_log "Small circles: adaptive" "the view's small circles adaptive at the start"
ui_expect_log "Bodies shown: " "the plate shown"
ui_sync

echo "--- Full Detail"
small_circles Down "Preferences: small circles full"
grep -q "^smallCircles=full" "$CONF" || ui_fail "Full Detail not kept in the settings"
FULL=$(remeshed "full detail")
[ -n "$FULL" ] || ui_fail "no triangles logged for Full Detail"
echo "ok   $FULL triangles in Full Detail"
grep -q "Display meshes of .*meshing again with small circles .* cancelled" "$UI_LOG" && ui_fail "the meshing again was cancelled"
ui_stop_app

echo "--- Full Detail kept; Adaptive again"
ui_start_app --open "$FILE"
ui_expect_log "Small circles: full" "the app starts with Full Detail"
ui_expect_log "Bodies shown: " "the plate shown"
ui_sync
small_circles Up "Preferences: small circles adaptive"
grep -q "^smallCircles=adaptive" "$CONF" || ui_fail "Adaptive not kept in the settings"
ADAPTIVE=$(remeshed adaptive)
[ -n "$ADAPTIVE" ] || ui_fail "no triangles logged for Adaptive"
echo "ok   $ADAPTIVE triangles in Adaptive"
[ "$ADAPTIVE" -lt "$FULL" ] || ui_fail "Adaptive ($ADAPTIVE triangles) not fewer than Full Detail ($FULL)"
echo "ok   Adaptive has fewer triangles than Full Detail"
ui_stop_app

ui_finish "UI small circles test"
