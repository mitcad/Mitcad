#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSession.hpp
# check-all sources: app/MainWindowJoints.cpp app/commands/JointCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document.rs core/model/src/document/auto_operation.rs core/model-api/src/lib.rs
# check-all sources: tools/ui-scripts/joint-edit-empty-offset.mitcad-ui tools/ui-scripts/joint-edit-empty-offset.json
# A joint edited with its Offset emptied while typing (mitcad#1860, GitHub
# mitcad/Mitcad#347) through the real UI, the shared input script
# tools/ui-scripts/joint-edit-empty-offset.mitcad-ui (also run by
# tools/ui-windows-joint-edit-empty-offset-test.ps1 and
# tools/ui-macos-joint-edit-empty-offset-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/joint-edit-empty-offset.json): the
# edited joint's preview keeps the joint and the rigid group after it, an
# emptied Offset moves nothing in the view (the last good preview stays,
# the panel says what is wrong, OK is refused), and a new value typed then
# moves the rigid group's members together and OK commits it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-edit-empty-offset-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-joint-edit-empty-offset.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/joint-edit-empty-offset.mitcad-ui
"$cli" run "$here/ui-scripts/joint-edit-empty-offset.json" --save "$work/joint-edit-empty-offset.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/joint-edit-empty-offset.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'joint edit empty offset driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview Joint: the last good preview kept' 'the emptied Offset leaves the last good preview drawn'
ui_expect_log 'Joint: OK refused' 'OK refused while the Offset is empty'
ui_expect_log 'Edited F4' 'the new Offset committed'
ui_expect_log 'TestDriver: done' 'a joint edited with its Offset emptied'
ui_finish 'UI a joint edited with its Offset emptied'
