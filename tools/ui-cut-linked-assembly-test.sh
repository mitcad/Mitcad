#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/Selection.cpp app/browser app/files/MainWindowLinks.cpp
# check-all sources: core/model/src/assembly_cut.rs core/model/src/features/extrude_scope.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/cut-linked-assembly.mitcad-ui tools/ui-scripts/cut-linked-assembly-part.json
# check-all sources: tools/ui-scripts/cut-linked-assembly-holder.json tools/ui-scripts/cut-linked-assembly-check.json
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/HoleCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: core/model/src/features/revolve.rs core/model/src/features/hole.rs
# Extrude Cut, Revolve Cut and Hole in an assembly on a linked part
# (mitcad#2131, mitcad#2160) through the real UI, the shared input script
# tools/ui-scripts/cut-linked-assembly.mitcad-ui (also run by
# tools/ui-windows-cut-linked-assembly-test.ps1 and
# tools/ui-macos-cut-linked-assembly-test.sh): a block linked from
# plate.mitcad, and in the root a square over it and a rectangle beside the
# Z axis.
#   - Extrude: Affected components lists the linked occurrence, left out of
#     the automatic scope but checkable; checked, the scope turns explicit
#     and the row says it is machined in this assembly only.
#   - OK cuts the occurrence: its Browser row says it is machined, the
#     linked block itself stays as it was; Undo and Redo.
#   - Revolve Cut about the Z axis: the same list in the Revolve panel,
#     checked, machined here, OK; a Hole through the occurrence edited in
#     the Hole panel's list.
#   - Saved: the occurrence shows the three cuts, 3362.920 mm3
#     (cut-linked-assembly-check.json), and plate.mitcad is the file it was.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-linked-assembly-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-cut-linked-assembly.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work
export MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/cut-linked-assembly-part.json" --save "$work/plate.mitcad" > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli: the part'
}
# The holder made in the part's folder: the link is plate.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/cut-linked-assembly-holder.json" --save "$work/holder.mitcad") \
  > "$work/cli.log" 2>&1 || {
  cat "$work/cli.log"
  ui_fail 'mitcad-cli: the holder'
}
cp "$work/plate.mitcad" "$work/plate-written.mitcad"
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/cut-linked-assembly.mitcad-ui
ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/holder.mitcad"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'cut-linked-assembly: driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'cut-linked-assembly: the shared steps'
(cd "$work" && "$cli" run "$here/ui-scripts/cut-linked-assembly-check.json" --open "$work/holder.mitcad") \
  > "$work/check.log" 2>&1 || {
  cat "$work/check.log"
  ui_fail 'the saved assembly does not show the machined occurrence'
}
echo "ok   saved with plate:1 machined by the three cuts, 3362.920 mm3 shown"
cmp -s "$work/plate.mitcad" "$work/plate-written.mitcad" || ui_fail 'plate.mitcad changed'
echo "ok   plate.mitcad is the file it was"
ui_finish 'UI cuts (Extrude, Revolve, Hole) on a linked part in an assembly'
