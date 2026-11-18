#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Move on a component hidden with its parent (mitcad#1995, GitHub
# mitcad/Mitcad#430) in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/move-hidden-parent.mitcad-ui (also run by
# tools/ui-move-hidden-parent-test.sh and
# tools/ui-windows-move-hidden-parent-test.ps1) on the reported design of
# the private test files (MITCAD_MITCAD_REPORTED, github-430; never
# committed), unpacked into the test's folder: the component inserted
# under the hidden Fairlead:1, picked in the Browser, is moved 40 mm along
# X by Move/Copy. Skipped (77) without the design.
# Usage: bash tools/ui-macos-move-hidden-parent-test.sh <app> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
zip=$(find "${MITCAD_MITCAD_REPORTED:-/nonexistent}/github-430" -maxdepth 1 -name '*.zip' -type f 2> /dev/null | sort | head -1)
if [ -z "$zip" ]; then
  echo "SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-430) is not here"
  exit 77
fi
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
ui_init "$1" "${2:-ui-macos-move-hidden-parent}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK/reported"
unzip -o -q "$zip" -d "$MITCAD_TEST_WORK/reported" || {
  echo "FAIL: the reported design could not be unpacked" >&2
  exit 1
}
design=$(find "$MITCAD_TEST_WORK/reported" -name '*.mitcad' -type f | sort | head -1)
if [ -z "$design" ]; then
  echo "FAIL: no design in the reported zip" >&2
  exit 1
fi
MITCAD_TEST_INPUT=$here/ui-scripts/move-hidden-parent.mitcad-ui ui_run_app move-hidden-parent \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log ': hidden with Fairlead:1$' 'the rows in Fairlead:1 are hidden with it'
ui_expect_log '^Move/Copy Objects: 1 component \[component C3 in O1/O2/O3\]' 'Move/Copy takes the hidden component'
ui_expect_log '^Moved 1 components \(' 'OK moves it'
ui_expect_log 'TestDriver: done' 'every step passed'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: move hidden parent; evidence in $UI_OUT"
  exit 1
fi
echo 'PASS: Move of a component hidden with its parent'
