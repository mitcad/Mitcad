#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/commands/DerivedPartCommands.cpp app/framework/TestDriver.cpp app/library tools/cli
# check-all sources: core/model/src/features/edge_frame.rs core/model-build core/model-api/src tools/ui-timber-lib.sh
# check-all sources: tools/ui-scripts/timber-edge.mitcad-ui tools/ui-scripts/timber-edge.json
# Timber's Along an edge (mitcad#1814) in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/timber-edge.mitcad-ui on a plate that
# mitcad-cli makes (tools/ui-scripts/timber-edge.json), with the timber
# library of tools/ui-timber-lib.sh (a 98 x 48 member): a member along the
# plate's long edge, flush and then moved by Inset, Start, End and Lift; the
# plate made shorter, the member follows with its offsets kept; a round edge
# is refused in words. As tools/ui-timber-edge-test.sh (Linux) and
# tools/ui-windows-timber-edge-test.ps1 (Windows).
# Usage: bash tools/ui-macos-timber-edge-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-timber-edge}"
# The timber library (tools/ui-timber-lib.sh), made with this mitcad-cli.
UI_CLI=$2
# shellcheck source=tools/ui-timber-lib.sh
. "$here/ui-timber-lib.sh"
timber_library ui-macos-timber-edge
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: the timber library was not made; evidence in $UI_OUT"
  exit 1
fi
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
design=$WORK/timber-edge.mitcad
"$CLI" run "$here/ui-scripts/timber-edge.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/timber-edge.mitcad-ui ui_run_app timber-edge \
  --ui=qt --no-mcp --no-recovery --no-native-dialogs --open "$design"
ui_expect_log '^Inspect Timber: along an edge, length 1800[.]00 mm, flat; end A at -?0[.]00, 49[.]00, 33[.]00; ' \
  'the member along the long edge, 1800 long, flush on the top face'
ui_expect_log 'length along an edge, 1730[.]00 mm; end A at 50[.]00, 59[.]00, 38[.]00' \
  'Inset 10, Start 50, End 20, Lift 5: placed where they say'
ui_expect_log '^TestDriver: done' 'the plate 1500 long: the member 1430 long, its offsets kept; a round edge refused'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Timber along an edge; evidence in $UI_OUT"
  exit 1
fi
echo 'PASS: Timber along an edge'
