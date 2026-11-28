#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindowMcp.cpp app/framework/TestDriverCommands.cpp app/framework/TestDriverModel.cpp app/framework/TestDriverPointer.cpp
# check-all sources: core/model/src/file core/model/src/cam core/model-api/src/lib.rs core/model/src/document/edit_in_place.rs tools/cli
# check-all sources: tools/ui-scripts/unreadable-cam-in-place.mitcad-ui tools/ui-scripts/unreadable-cam-file.py
# check-all sources: tools/ui-scripts/edit-in-place-part.json tools/ui-scripts/edit-in-place-holder.json
# Edit in Place of a linked design whose CAM section this MitCad cannot
# read (mitcad#2587, after mitcad#2581), the shared input script
# tools/ui-scripts/unreadable-cam-in-place.mitcad-ui (also run by
# tools/ui-macos-unreadable-cam-in-place-test.sh and
# tools/ui-windows-unreadable-cam-in-place-test.ps1): an assembly links
# part.mitcad, a cube whose CAM has two setups with one id; Edit in Place
# changes it, an agent's save (MCP) is refused and Save asks first.
#   1. Cancel: the part's file is as it was, byte for byte.
#   2. Save and Drop Them: written without the CAM, with the change.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-unreadable-cam-in-place-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-unreadable-cam-in-place.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/unreadable-cam-in-place.mitcad-ui
files=$here/ui-scripts/unreadable-cam-file.py
part=$work/part.mitcad
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$part" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the part"; exit 1; }
python3 -I "$files" add "$part" || { ui_fail "the CAM section was not added"; exit 1; }
# The holder made in the part's folder: the link is part.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/edit-in-place-holder.json" --save "$work/holder.mitcad") \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail "mitcad-cli: the holder"; exit 1; }
features=$(python3 -I "$files" features "$part") || { ui_fail "the part cannot be read"; exit 1; }
before=$(sha256sum < "$part")
ui_start_display

# run <name> <button> <logged answer>: the script with that answer to Save's
# question, to its end.
run() {
  echo "--- $1"
  export MITCAD_TEST_ANSWER=$2 MITCAD_TEST_ANSWERED=$3
  ui_start_app --ui=qt --no-mcp --open "$work/holder.mitcad"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: quit' "$1: Edit in Place, the agent's save refused, Save asked"
}

run "1. Cancel" Cancel cancelled
[ "$(sha256sum < "$part")" = "$before" ] || ui_fail "Cancel or the agent's save wrote the part's file"
echo "ok   the part's file is as it was, byte for byte"
# (Anchored: the driver's trace of its steps quotes the script's lines.)
grep -Eq '^Edit in Place: saved ' "$UI_LOG" && ui_fail "the part was saved"

run "2. Save and Drop Them" 'Save and Drop Them' save
grep -Eq '^Edit in Place: saved .*/part[.]mitcad$' "$UI_LOG" || ui_fail "the part was not saved"
echo "ok   the part saved"
python3 -I "$files" dropped "$part" $((features + 1)) || ui_fail "the part's file after Save and Drop Them"
ui_finish 'UI Edit in Place of a design whose CAM cannot be read'
