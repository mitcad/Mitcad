#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp app/commands/SplitToolLinks.cpp app/commands/SplitToolLinks.hpp
# check-all sources: core/model/src/features/split_body.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/split-body-other-component.mitcad-ui tools/ui-scripts/split-body-other-component.json
# Split Body with a face of another component as the Splitting Tool
# (mitcad#2299, GitHub mitcad/Mitcad#588) in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/split-body-other-component.mitcad-ui on the
# design mitcad-cli makes (tools/ui-scripts/split-body-other-component.json):
# with B active, B's block split by the top face of A's block, picked where
# A:1 shows it, builds in B without an activation; Undo takes it back; an
# edit shows the tool face where A:1 shows it; moving A:1 moves the cut.
# As tools/ui-split-body-other-component-test.sh (Linux) and
# tools/ui-windows-split-body-other-component-test.ps1 (Windows).
# Usage: bash tools/ui-macos-split-body-other-component-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-split-body-other-component}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/split-body-other-component.json" \
  --save "$MITCAD_TEST_WORK/split-body-other-component.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/split-body-other-component.mitcad-ui \
  ui_run_app split-body-other-component --ui=qt --no-recovery --no-mcp \
  --open "$MITCAD_TEST_WORK/split-body-other-component.mitcad"
# ui_expect_log takes an extended regular expression here.
ui_expect_log '^Split Body Splitting Tool: 1 face \[face F2:end\(' 'the top face of A'"'"'s block picked as the tool'
ui_expect_log '^Added split body of 1 body' 'OK added the split in B'
ui_expect_log 'TestDriver: done' 'a face of another component splits the body, follows its occurrence'
ui_expect_no_log 'TestDriver: FAIL' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Split Body with a face of another component; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: Split Body with a face of another component; evidence in $UI_OUT"
