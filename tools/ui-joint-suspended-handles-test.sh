#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionHold.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/MainWindowJoints.cpp app/commands/JointCommands.cpp
# check-all sources: tools/cli tools/ui-scripts/joint-suspended-handles.mitcad-ui
# A command panel set aside hides its handles (mitcad#1844) through the
# real UI, the shared input script
# tools/ui-scripts/joint-suspended-handles.mitcad-ui (also run by the
# Windows and macOS wrappers) on a design that mitcad-cli makes
# (tools/ui-scripts/joint-suspended-handles.json): Joint's Animate sets
# its panel aside, its handles hidden and a drag where one was moving
# nothing; Esc brings them back and a real drag turns the joint.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-suspended-handles-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-joint-suspended-handles.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/joint-suspended-handles.mitcad-ui
"$cli" run "$here/ui-scripts/joint-suspended-handles.json" --save "$work/joint-suspended-handles.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/joint-suspended-handles.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'joint suspended handles driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Joint handles hidden while set aside' 'the handles hidden while Animate plays'
ui_expect_log 'TestDriver: done' "the set-aside panel's handles hidden, back on resume"
ui_finish "UI a set-aside panel's handles"
