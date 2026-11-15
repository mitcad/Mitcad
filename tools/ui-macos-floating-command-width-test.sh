#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/FloatingLayout.cpp app/framework/UiSizes.hpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchPalette.cpp tools/ui-scripts/floating-command-width.mitcad-ui
# The Floating layout's Command card at the largest Text size and Icon size
# (mitcad#1846): the shared input script
# tools/ui-scripts/floating-command-width.mitcad-ui of
# tools/ui-floating-command-width-test.sh in the Qt frontend, MitCad started
# with appearance/textSize and appearance/iconSize 200.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-floating-command-width-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=180
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[appearance]\ntextSize=200\niconSize=200\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
MITCAD_TEST_INPUT=$here/ui-scripts/floating-command-width.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app floating-command-width --ui=qt --no-recovery --no-mcp --chrome=floating
ui_expect_log '^TestDriver: done' 'the card at 200 %, the palette whole in it'
ui_expect_no_log 'TestDriver: FAIL' 'no failed Command card assertion'
width=$(grep -E '^Floating cards: ' "$UI_LOG_FILE" | grep -oE '; commandDock -?[0-9]+,-?[0-9]+ [0-9]+x' | tail -n 1 |
  grep -oE ' [0-9]+x$' | tr -d ' x')
if [ -n "$width" ] && [ "$width" -ge 600 ]; then
  ui_pass "the command card is $width px wide (at least 600)"
else
  ui_fail "the command card is ${width:-?} px wide at 200 %, not at least 600"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Floating Command card width'
