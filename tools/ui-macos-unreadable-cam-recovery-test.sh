#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp
# check-all sources: core/model/src/file core/model-api/src/lib.rs core/model-api/src/classes.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui tools/ui-scripts/unreadable-cam-recovery.mitcad-ui
# check-all sources: tools/ui-scripts/unreadable-cam-file.py tools/ui-scripts/edit-in-place-part.json
# A design whose CAM section this MitCad cannot read and with a field a
# newer MitCad wrote, recovered from an autosave (mitcad#2587) on macOS,
# through the test driver, as tools/ui-unreadable-cam-recovery-test.sh on
# Linux: the autosaved text has neither, so the recovered design keeps them
# from its file and Save over it asks first, as for the design opened from
# it. The design is a cube made by mitcad-cli, its CAM and the newer field
# added by tools/ui-scripts/unreadable-cam-file.py. Three runs of the
# application, as a script cannot start it again:
#   1. tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui: opened
#      with both warnings, changed, autosaved, the session killed.
#   2. tools/ui-scripts/unreadable-cam-recovery.mitcad-ui: recovered, it
#      warns again; an agent's save (MCP) is refused; Save asks, Cancel; the
#      file is as it was, byte for byte (shasum). Killed once it autosaved.
#   3. The same script: recovered again; Save and Drop Them writes the file
#      without the CAM and the newer field, with the change.
# Exit status: 0 passed, 1 failed, 2 wrong usage, 77 skipped (no window
# server).
# Usage: tools/ui-macos-unreadable-cam-recovery-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-unreadable-cam-recovery}"
UI_TIMEOUT=240
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
files=$here/ui-scripts/unreadable-cam-file.py
work=$UI_OUT/files
rm -rf "$work"
mkdir -p "$work"
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=70
# Autosave every 2 s for the test (into the run's own folder, ui_init).
export MITCAD_TEST_AUTOSAVE_SECONDS=2
design=$work/cam.mitcad
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  echo 'FAIL: mitcad-cli: the design'
  exit 1
}
python3 -I "$files" add "$design" newer || { echo 'FAIL: the CAM section and the newer field were not added'; exit 1; }
features=$(python3 -I "$files" features "$design") || { echo 'FAIL: the design cannot be read'; exit 1; }
before=$(shasum -a 256 < "$design")

# run <description> <log name> <script> <app arguments>...: the script to
# its end, which kills the application (exit code 0). The test ends at the
# first failed run, as the next one needs its autosave.
run() {
  local description=$1 name=$2 script=$3
  shift 3
  echo "--- $description"
  MITCAD_TEST_INPUT=$here/ui-scripts/$script ui_run_app "$name" --ui=qt --no-mcp --no-native-dialogs "$@"
  ui_expect_no_log '^TestDriver: FAIL' "$name: no failed driver assertion"
  ui_expect_log '^TestDriver: kill' "$name: the script ran to its end"
  [ "$UI_FAILURES" = 0 ] || exit 1
}

# same_file description: the design's file is as it was, byte for byte.
same_file() {
  if [ "$(shasum -a 256 < "$design")" = "$before" ]; then
    ui_pass "the design's file is as it was, byte for byte"
  else
    ui_fail "$1 wrote the design's file"
  fi
}

run '1. Opened, changed, autosaved, killed' recovery-change unreadable-cam-recovery-change.mitcad-ui \
  --no-recovery --open "$design"
same_file 'the session'
[ "$UI_FAILURES" = 0 ] || exit 1

# Started again without --no-recovery: the killed session is offered.
MITCAD_TEST_ANSWER=Cancel MITCAD_TEST_ANSWERED=cancelled \
  run "2. Recovered: the agent's save refused, Save asks, Cancel" recovery-cancel unreadable-cam-recovery.mitcad-ui
same_file "Cancel or the agent's save"
# (Anchored: the driver's trace of its steps quotes the script's lines.)
if grep -Eq '^Saved ' "$UI_LOG_FILE"; then ui_fail 'the recovered design was saved'; else ui_pass 'the recovered design was not saved'; fi
[ "$UI_FAILURES" = 0 ] || exit 1

MITCAD_TEST_ANSWER='Save and Drop Them' MITCAD_TEST_ANSWERED=save \
  run '3. Recovered again: Save and Drop Them' recovery-drop unreadable-cam-recovery.mitcad-ui
ui_expect_log '^Saved .*/cam[.]mitcad$' "saved over the design's file"
if python3 -I "$files" dropped "$design" $((features + 1)); then
  ui_pass "the design's file after Save and Drop Them"
else
  ui_fail "the design's file after Save and Drop Them"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: UI macOS recovery of a design whose CAM cannot be read'
