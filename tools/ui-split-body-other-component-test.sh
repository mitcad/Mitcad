#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp app/commands/SplitToolLinks.cpp app/commands/SplitToolLinks.hpp
# check-all sources: app/commands/CommandSupport.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/split_body.rs core/model/src/features/face_refs.rs tools/cli
# check-all sources: tools/ui-scripts/split-body-other-component.mitcad-ui tools/ui-scripts/split-body-other-component.json
# Split Body with a face of another component as the Splitting Tool
# (mitcad#2299, GitHub mitcad/Mitcad#588) through the real UI, the shared
# input script tools/ui-scripts/split-body-other-component.mitcad-ui (also
# run by tools/ui-windows-split-body-other-component-test.ps1 and
# tools/ui-macos-split-body-other-component-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/split-body-other-component.json): with
# B active, B's block split by the top face of A's block, picked where A:1
# shows it, builds in B without an activation; Undo takes it back; an edit
# shows the tool face where A:1 shows it; moving A:1 moves the cut.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-split-body-other-component-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-split-body-other-component.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/split-body-other-component.mitcad-ui
"$cli" run "$here/ui-scripts/split-body-other-component.json" --save "$work/split-body-other-component.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/split-body-other-component.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'split body other component driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Split Body Splitting Tool: 1 face [face F2:end(' 'the top face of A'"'"'s block picked as the tool'
ui_expect_log 'Added split body of 1 body' 'OK added the split in B'
ui_expect_log 'Editing F5 with Split Body' 'the split edited'
ui_expect_log 'TestDriver: done' 'a face of another component splits the body, follows its occurrence'
# Anchored: the driver echoes its own script lines (its expect-never
# checks the refusal's words).
grep -q '^Activated ' "$UI_LOG" && ui_fail 'a component was activated'
echo "ok   nothing activated"
ui_finish 'UI Split Body with a face of another component'
