#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/framework/Command.hpp app/framework/Command.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionScript.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/exchange tools/cli
# check-all sources: tools/ui-scripts/move-imported-assembly.mitcad-ui tools/ui-scripts/move-imported-assembly.json
# check-all sources: tools/ui-scripts/move-imported-assembly-source.json
# Move/Copy, Move Object Components, on an imported STEP assembly
# (mitcad#1891) through the real UI, the shared input script
# tools/ui-scripts/move-imported-assembly.mitcad-ui (also run by
# tools/ui-windows-move-imported-assembly-test.ps1 and
# tools/ui-macos-move-imported-assembly-test.sh). mitcad-cli writes a STEP
# assembly that places a part twice
# (tools/ui-scripts/move-imported-assembly-source.json) and imports it into
# a design with a body and a component in the root
# (tools/ui-scripts/move-imported-assembly.json): the import's own
# component, placed in the root, places the part's component twice. A
# click on a body of the part takes the import's occurrence in the root,
# not the part's nested one, which the model refuses; 40 mm along X moves
# the import only, and one undo puts it back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-imported-assembly-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-move-imported-assembly.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
# Both scripts name their files relative to the work folder.
(cd "$work" && "$cli" run "$here/ui-scripts/move-imported-assembly-source.json") > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the STEP assembly"; exit 1; }
(cd "$work" && "$cli" run "$here/ui-scripts/move-imported-assembly.json" --save "$work/move-imported-assembly.mitcad") \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/move-imported-assembly.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/move-imported-assembly.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "move imported assembly driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Move/Copy Objects: 1 component [component ' "a click on the imported part's body takes the import's occurrence"
ui_expect_log 'Moved 1 components (' "the import's occurrence moved"
ui_expect_log 'TestDriver: done' "Move/Copy of an imported STEP assembly"
ui_finish 'UI Move/Copy of an imported STEP assembly'
