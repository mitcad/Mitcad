#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccessGroup.cpp app/MainWindowNative.cpp
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/NativeMacWindow.hpp tools/cli
# The AppKit toolbar's quick-access group (mitcad#1113) on a saved plate
# that mitcad-cli makes (tools/ui-scripts/native-macos-quick-access.mitcad-ui):
# the default buttons after the workspace tabs; Save disabled for the
# unchanged design with its reason in the tooltip, enabled after a change;
# View › Look At checked in the group's ⋮ menu adds a button; Reset to
# Default; a narrow window folds the group into its ⋮; Hide Quick-Access
# Toolbar in its menu hides the whole group (mitcad#1189).
# Usage: tools/ui-macos-native-quick-access-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_SHOTS=$UI_OUT
mkdir -p "$MITCAD_TEST_WORK"
# A 200 x 200 x 10 mm plate centred on the origin.
cat > "$MITCAD_TEST_WORK/plate.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/plate.json" --save "$MITCAD_TEST_WORK/plate.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-quick-access.mitcad-ui \
  ui_run_app native-quick-access --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/plate.mitcad"
ui_expect_log '^TestDriver: done' 'the AppKit quick-access group: buttons, Save, Look At, reset, folding'
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: the AppKit quick-access group'
