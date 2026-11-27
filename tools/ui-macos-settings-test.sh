#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view app/files/MainWindowFiles.cpp app/framework/CachePreferences app/framework/SettingsWindow app/framework/CommandRegistry app/framework/TestDriver
# Primary Qt macOS Settings, cache editing and tool-window menu dispatch.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-settings}"
cat > "$UI_OUT/block.mitcad" <<'EOF'
{
  "format": "mitcad", "version": 1,
  "parameters": [
    {"name": "d1", "value": 60}, {"name": "d2", "value": 40},
    {"name": "d3", "value": 20}
  ],
  "features": [
    {"type": "sketch", "name": "Sketch1", "shapes": [
      {"type": "rectangle", "corner": [0, 0], "width": "d1", "height": "d2"}]},
    {"type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body"}
  ]
}
EOF
MITCAD_TEST_INPUT=$here/ui-scripts/macos-settings.mitcad-ui \
  ui_run_app settings --ui=qt --no-recovery --no-mcp --open "$UI_OUT/block.mitcad"
ui_expect_no_log 'TestDriver: FAIL' 'settings assertions passed'
ui_expect_log 'TestDriver: done' 'settings script completed'
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: macOS Qt Settings; logs: $UI_OUT"
