#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DogboneCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/features/dogbone.rs geometry/src/dogbone.cpp tools/cli
# Dogbone (mitcad#1011) through the real UI, on a design mitcad-cli makes: a
# block 80 x 60 x 20 (Box1, F1, Body1) with a pocket 40 x 20, 10 deep (Box2,
# F2): 88000 mm3. Dogbone, the block picked in the browser, OK with the
# panel's defaults (a 6.35 mm tool, 0.1 mm extra, diagonal): four reliefs at
# the pocket's inside corners, each the tool's circle outside the pocket
# (6.388881 mm2, 10 deep; tools/cli/tests/dogbone.json works it out), and
# Dogbone1 in the timeline. The Windows counterpart is
# tools/ui-windows-dogbone-test.ps1 (ctest app.ui-windows-dogbone).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-dogbone-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-dogbone.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 80, "width": 60, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box",
    "plane": {"type": "fixed", "origin": [20, 20, 10], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
    "length": 40, "width": 20, "height": 15, "operation": "cut", "participants": ["F1.b0"]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/model.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli model"; }

ui_start_display
ui_start_app --open "$WORK/model.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the block shown"

echo "--- Dogbone on the pocket's body"
ui_mark
ui_step "Dogbone"                          ui_command "Dogbone"
ui_expect_new "Command Dogbone started" "the panel opened" 30
ui_step "pick Body1"                       ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Dogbone Corners: 1 body [body F1.b0]" "the block picked" 30
ui_expect_new "Preview Dogbone: ok" "the reliefs previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added dogbone (Diagonal), tool diameter 6.35" "the dogbone added" 30
ui_expect_volume "Body Body1 (F1.b0): volume 88000.000 -> \([0-9.]*\) mm3" 87744.444755 "four reliefs cut"
ui_expect_log "Timeline Dogbone1 at" "Dogbone1 in the timeline"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI dogbone test"
