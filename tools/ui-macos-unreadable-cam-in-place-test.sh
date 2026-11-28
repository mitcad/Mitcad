#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp app/framework/TestDriverPointer.cpp
# check-all sources: core/model/src/file core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-in-place.mitcad-ui tools/ui-scripts/unreadable-cam-file.py
# check-all sources: tools/ui-scripts/edit-in-place-part.json tools/ui-scripts/edit-in-place-holder.json
# Edit in Place of a linked design whose CAM section this MitCad cannot
# read (mitcad#2587) on macOS, through the test driver, as
# tools/ui-unreadable-cam-in-place-test.sh on Linux: the shared steps
# tools/ui-scripts/unreadable-cam-in-place.mitcad-ui on an assembly that
# links part.mitcad, a cube whose CAM has two setups with one id (made by
# mitcad-cli and tools/ui-scripts/unreadable-cam-file.py). Edit in Place
# changes it, an agent's save (MCP) is refused and Save asks first:
#   1. Cancel: the part's file is as it was, byte for byte (shasum).
#   2. Save and Drop Them: written without the CAM, with the change.
# Exit status: 0 passed, 1 failed, 2 wrong usage, 77 skipped (no window
# server).
# Usage: tools/ui-macos-unreadable-cam-in-place-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-unreadable-cam-in-place}"
UI_TIMEOUT=300
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
files=$here/ui-scripts/unreadable-cam-file.py
work=$UI_OUT/files
rm -rf "$work"
mkdir -p "$work"
part=$work/part.mitcad
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$part" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  echo 'FAIL: mitcad-cli: the part'
  exit 1
}
python3 -I "$files" add "$part" || { echo 'FAIL: the CAM section was not added'; exit 1; }
# The holder made in the part's folder: the link is part.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/edit-in-place-holder.json" --save "$work/holder.mitcad") \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  echo 'FAIL: mitcad-cli: the holder'
  exit 1
}
features=$(python3 -I "$files" features "$part") || { echo 'FAIL: the part cannot be read'; exit 1; }
before=$(shasum -a 256 < "$part")

# run <name> <button> <logged answer>: the script with that answer to Save's
# question, to its end.
run() {
  echo "--- $1"
  MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=70 MITCAD_TEST_ANSWER=$2 MITCAD_TEST_ANSWERED=$3 \
    MITCAD_TEST_INPUT=$here/ui-scripts/unreadable-cam-in-place.mitcad-ui \
    ui_run_app "$4" --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$work/holder.mitcad"
  ui_expect_no_log '^TestDriver: FAIL' "$1: no failed driver assertion"
  ui_expect_log '^TestDriver: quit' "$1: Edit in Place, the agent's save refused, Save asked"
}

run '1. Cancel' Cancel cancelled in-place-cancel
if [ "$(shasum -a 256 < "$part")" = "$before" ]; then
  ui_pass "the part's file is as it was, byte for byte"
else
  ui_fail "Cancel or the agent's save wrote the part's file"
fi
# (Anchored: the driver's trace of its steps quotes the script's lines.)
if grep -Eq '^Edit in Place: saved ' "$UI_LOG_FILE"; then ui_fail 'the part was saved'; else ui_pass 'the part was not saved'; fi
[ "$UI_FAILURES" = 0 ] || exit 1

run '2. Save and Drop Them' 'Save and Drop Them' save in-place-drop
ui_expect_log '^Edit in Place: saved .*/part[.]mitcad$' 'the part saved'
if python3 -I "$files" dropped "$part" $((features + 1)); then
  ui_pass "the part's file after Save and Drop Them"
else
  ui_fail "the part's file after Save and Drop Them"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: UI macOS Edit in Place of a design whose CAM cannot be read'
