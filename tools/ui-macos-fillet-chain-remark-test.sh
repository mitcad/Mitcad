#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A fillet's short tangent chain is a remark, not a warning (mitcad#1914),
# in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/fillet-chain-remark.mitcad-ui (also run by
# tools/ui-fillet-chain-remark-test.sh and
# tools/ui-windows-fillet-chain-remark-test.ps1) on the reported design of
# the private test files (MITCAD_MITCAD_REPORTED, fillet-chain-note/; never
# committed), copied into the test's folder: its accepted Fillet12 marks
# nothing on the Timeline; edited again, the panel points the short chain
# out as information. Skipped (77) without the design. Its first step waits
# for the view as long as the start-up open takes under load (mitcad#1933).
# Usage: bash tools/ui-macos-fillet-chain-remark-test.sh <app> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
reported=${MITCAD_MITCAD_REPORTED:-}/fillet-chain-note
if [ -z "${MITCAD_MITCAD_REPORTED:-}" ] || [ ! -d "$reported" ]; then
  echo "SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/fillet-chain-note) is not here"
  exit 77
fi
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
ui_init "$1" "${2:-ui-macos-fillet-chain-remark}"
UI_TIMEOUT=1200
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
cp -R "$reported" "$MITCAD_TEST_WORK/reported" || {
  echo "FAIL: the reported design could not be copied" >&2
  exit 1
}
design=$(find "$MITCAD_TEST_WORK/reported" -name '*.mitcad' -type f | sort | head -1)
if [ -z "$design" ]; then
  echo "FAIL: no design in the reported folder" >&2
  exit 1
fi
MITCAD_TEST_INPUT=$here/ui-scripts/fillet-chain-remark.mitcad-ui ui_run_app fillet-chain-remark \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'Preview Fillet: ok, remark: ' 'the panel points the short chain out'
ui_expect_log 'Panel Fillet message: information: ' 'as information, not a warning'
ui_expect_log 'TestDriver: done' 'nothing on the timeline after OK, the remark in the open panel'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Fillet chain remark; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: a fillet's short tangent chain is a remark in the open panel, nothing on the timeline"
