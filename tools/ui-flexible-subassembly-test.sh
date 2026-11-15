#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/browser/BrowserPanel.cpp app/browser/DocumentSnapshot.cpp
# check-all sources: app/MainWindowJoints.cpp core/model/src/joints.rs core/model/src/document/flexible.rs tools/cli
# check-all sources: app/browser/DocumentHost.hpp core/model/src/document/joints.rs core/model-api/src/joints.rs
# check-all sources: tools/ui-scripts/flexible-subassembly.json tools/ui-scripts/flexible-subassembly.mitcad-ui
# A flexible subassembly (mitcad#1793) through the real UI, the shared
# input script tools/ui-scripts/flexible-subassembly.mitcad-ui (also run by
# tools/ui-windows-flexible-subassembly-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/flexible-subassembly.json): Hinge:1 is
# made Flexible from its row's menu and its inner Arm dragged along the
# Hinge's own joint, for Hinge:1 alone; Hinge:2's Arm stays; then Joint1
# is animated inside Hinge:1 from its row's menu (mitcad#1805).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flexible-subassembly-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-flexible-subassembly.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/flexible-subassembly.mitcad-ui
"$cli" run "$here/ui-scripts/flexible-subassembly.json" --save "$work/flexible-subassembly.mitcad" ||
  { ui_fail 'mitcad-cli could not make the design'; exit 1; }
[ -f "$work/flexible-subassembly.mitcad" ] || { ui_fail 'the design was not made'; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/flexible-subassembly.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'flexible subassembly driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Hinge:1 made flexible, its arm dragged and animated for it alone'
ui_finish 'UI a flexible subassembly moves its own joints in its parent'
