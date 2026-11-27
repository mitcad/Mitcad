#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/browser core/model/src/assembly_cut.rs
# check-all sources: core/model/src/features/extrude_scope.rs tools/cli tools/ui-scripts/cut-linked-assembly.mitcad-ui
# Extrude Cut, Revolve Cut and Hole in an assembly on a linked part
# (mitcad#2131, mitcad#2160) on macOS, the same shared script as the Linux
# and Windows tests' (tools/ui-scripts/cut-linked-assembly.mitcad-ui) on a
# holder linking plate.mitcad, both made by mitcad-cli: the Affected
# components list of each panel lists the linked occurrence, checked it is
# machined in this assembly only, OK cuts it; the saved holder checked with
# cut-linked-assembly-check.json and the part's file as it was written.
# Usage: tools/ui-macos-cut-linked-assembly-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cut-linked-assembly}"
UI_TIMEOUT=480
work=$UI_OUT/files
mkdir -p "$work"
"$2" run "$here/ui-scripts/cut-linked-assembly-part.json" --save "$work/plate.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The holder made in the part's folder: the link is plate.mitcad.
(cd "$work" && "$2" run "$here/ui-scripts/cut-linked-assembly-holder.json" --save "$work/holder.mitcad") \
  >> "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
cp "$work/plate.mitcad" "$UI_OUT/plate-written.mitcad"
MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/cut-linked-assembly.mitcad-ui \
  ui_run_app cut-linked-assembly --ui=qt --no-recovery --no-mcp --open "$work/holder.mitcad"
ui_expect_log 'TestDriver: done' 'cut-linked-assembly: the shared steps'
ui_expect_no_log '^TestDriver: FAIL' 'cut-linked-assembly: no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
if (cd "$work" && "$2" run "$here/ui-scripts/cut-linked-assembly-check.json" --open "$work/holder.mitcad") \
  > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved assembly (cut-linked-assembly-check.json)'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved assembly (cut-linked-assembly-check.json)'
  exit 1
fi
if cmp -s "$work/plate.mitcad" "$UI_OUT/plate-written.mitcad"; then
  ui_pass 'plate.mitcad is the file it was'
else
  ui_fail 'plate.mitcad changed'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
