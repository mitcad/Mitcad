#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp
# check-all sources: core/model/src/file core/model/src/cam core/model-api/src/lib.rs core/model-api/src/classes.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui tools/ui-scripts/unreadable-cam-recovery.mitcad-ui
# check-all sources: tools/ui-scripts/unreadable-cam-file.py tools/ui-scripts/edit-in-place-part.json
# A design whose CAM section this MitCad cannot read and with a field a
# newer MitCad wrote, recovered from an autosave (mitcad#2587, after
# mitcad#2581 and mitcad#1132): the autosaved text has neither, so the
# recovered design keeps them from its file and Save over it asks first,
# as for the design opened from it. The shared input scripts (also run by
# tools/ui-macos-unreadable-cam-recovery-test.sh and
# tools/ui-windows-unreadable-cam-recovery-test.ps1):
#   1. tools/ui-scripts/unreadable-cam-recovery-change.mitcad-ui: opened
#      with both warnings, changed, autosaved, the session killed.
#   2. tools/ui-scripts/unreadable-cam-recovery.mitcad-ui: recovered, it
#      warns again; an agent's save (MCP) is refused; Save asks, Cancel; the
#      file is as it was, byte for byte. Killed once it autosaved.
#   3. The same script: recovered again; Save and Drop Them writes the file
#      without the CAM and the newer field, with the change.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-unreadable-cam-recovery-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-unreadable-cam-recovery.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
# Autosave every 2 s for the test.
export MITCAD_TEST_AUTOSAVE_SECONDS=2
files=$here/ui-scripts/unreadable-cam-file.py
design=$work/cam.mitcad
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$design" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; }
python3 -I "$files" add "$design" newer || ui_fail "the CAM section and the newer field were not added"
features=$(python3 -I "$files" features "$design") || ui_fail "the design cannot be read"
before=$(sha256sum < "$design")
ui_start_display

# run <name> <script> <app arguments>...: the script to its end, which kills
# the application.
run() {
  local name=$1 script=$2
  shift 2
  echo "--- $name"
  export MITCAD_TEST_INPUT=$here/ui-scripts/$script
  ui_start_app --ui=qt --no-mcp "$@"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$name: the driver timed out"
  wait "$UI_RUNNER" || ui_fail "$name: the driver failed"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: kill' "$name: the script ran to its end"
}

run "1. Opened, changed, autosaved, killed" unreadable-cam-recovery-change.mitcad-ui --open "$design"
[ "$(sha256sum < "$design")" = "$before" ] || ui_fail "the session wrote the design's file"

export MITCAD_TEST_ANSWER=Cancel MITCAD_TEST_ANSWERED=cancelled
UI_RECOVERY=1 run "2. Recovered: the agent's save refused, Save asks, Cancel" unreadable-cam-recovery.mitcad-ui
[ "$(sha256sum < "$design")" = "$before" ] || ui_fail "Cancel or the agent's save wrote the design's file"
echo "ok   the design's file is as it was, byte for byte"
# (Anchored: the driver's trace of its steps quotes the script's lines.)
grep -Eq '^Saved ' "$UI_LOG" && ui_fail "the recovered design was saved"

export MITCAD_TEST_ANSWER='Save and Drop Them' MITCAD_TEST_ANSWERED=save
UI_RECOVERY=1 run "3. Recovered again: Save and Drop Them" unreadable-cam-recovery.mitcad-ui
grep -Eq '^Saved .*/cam[.]mitcad$' "$UI_LOG" || ui_fail "the recovered design was not saved over its file"
echo "ok   saved over the design's file"
python3 -I "$files" dropped "$design" $((features + 1)) || ui_fail "the design's file after Save and Drop Them"
ui_finish 'UI recovery of a design whose CAM cannot be read'
