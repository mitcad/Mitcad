#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Native cross-component cut, exclusions, preview, edit, undo and persistence.
# Usage: bash tools/ui-macos-cut-components-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cut-components}"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
"$2" run "$here/ui-scripts/cut-components.json" --save "$MITCAD_TEST_WORK/cut-components.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
for phase in cut-components cut-components-reopen; do
  MITCAD_TEST_INPUT=$here/ui-scripts/$phase.mitcad-ui ui_run_app "$phase" \
    --ui=native --no-recovery --open "$MITCAD_TEST_WORK/cut-components.mitcad"
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$phase: real native UI"
  ui_expect_log 'TestDriver: done' "$phase: workflow completed"
  # TestDriver echoes expect-never patterns; only diagnostic lines are failures.
  if grep -Eq '^TestDriver: FAIL|^OCCT view event failed|^Native Mac canvas:' "$UI_LOG_FILE"; then
    ui_fail "$phase: runtime or assertion failure"
  else
    ui_pass "$phase: geometry, input and view assertions passed"
  fi
  if [ "$UI_FAILURES" -gt 0 ]; then
    echo "FAIL: cut components; evidence in $UI_OUT"
    exit 1
  fi
done
"$2" run "$here/ui-scripts/cut-components-legacy.json" --save "$MITCAD_TEST_WORK/cut-components-legacy.mitcad" > "$UI_OUT/legacy-setup.log" 2>&1 || {
  cat "$UI_OUT/legacy-setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/cut-components-legacy.mitcad-ui ui_run_app legacy \
  --ui=native --no-recovery --open "$MITCAD_TEST_WORK/cut-components-legacy.mitcad"
ui_expect_log 'TestDriver: done' 'legacy: workflow completed'
if grep -Eq '^TestDriver: FAIL|^OCCT view event failed|^Native Mac canvas:' "$UI_LOG_FILE"; then
  ui_fail 'legacy: runtime or assertion failure'
else
  ui_pass 'legacy: local cut scope and view assertions passed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: legacy cut scope; evidence in $UI_OUT"
  exit 1
fi
for shot in cut-components-hover cut-components-preview cut-components-reopened; do
  if [ ! -s "$MITCAD_TEST_WORK/$shot.png" ]; then
    echo "FAIL: missing $shot.png"
    exit 1
  fi
done
echo "PASS: cut components; logs and screenshots in $UI_OUT"
