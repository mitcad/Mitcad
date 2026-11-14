#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFilamentChanges.cpp app/files/MainWindowPrint.cpp app/print/FilamentChangeDialog.cpp
# check-all sources: app/view/FilamentChangeOverlay.cpp app/MainWindowContextMenu.cpp app/framework/TestDriver.cpp
# check-all sources: app/files/MainWindowPlates.cpp app/framework/NoteRow.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model tools/cli tools/ui-scripts/filament-changes.mitcad-ui tools/ui-scripts/filament-changes.json
# Send to Slicer's Filament changes (mitcad#1643, mitcad#1800) through the
# real UI, by the shared input script
# tools/ui-scripts/filament-changes.mitcad-ui (also run by
# tools/ui-windows-filament-changes-test.ps1) on a design mitcad-cli makes
# (tools/ui-scripts/filament-changes.json): no Change At row in the panel;
# in Filament Change a change by a typed height, a change by a plane and a
# pause by a face picked into its To Object field, and the edit of each
# (opened in the way it was placed); the view's print colours and ring,
# Remove, Undo and Redo, the normal look after the panel, and Filament
# Change Here… on the plane's and the face's context menu, which opens the
# window with it picked. The design saved keeps the changes at the plane
# and the face by their references.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-filament-changes-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
command -v jq > /dev/null || { echo "jq is required" >&2; exit 2; }
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-filament-changes.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
design=$work/filament-changes.mitcad
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/filament-changes.mitcad-ui
"$cli" run "$here/ui-scripts/filament-changes.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'filament changes driver timed out'; exit 1; }
wait "$UI_RUNNER" || { tail -n 80 "$UI_LOG" >&2; ui_fail 'the driver failed'; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "Send to Slicer's filament changes"
# The design keeps the edited change and the one at Plane1 by its reference.
# (+ 0: a plain number, however the file writes it.)
events=$(jq -c '[.filament_changes[0].changes[] | (if .height then .height + 0 else .at end)]' "$design") ||
  ui_fail "cannot read $design"
[ "$events" = '[14,"F3"]' ] || ui_fail "the saved schedule: $events, expected [14,\"F3\"]"
echo "ok   the design keeps the change at 14 mm and the one at Plane1"
plate=$(jq -c '[.filament_changes[] | select(.plate == "plate1") | .changes[] | (if .height then .height + 0 elif (.at | type) == "object" then .at.face else .at end)]' "$design") ||
  ui_fail "cannot read $design"
case $plate in
  '[14,"F3","F2:end('*')"]') echo "ok   Plate 1 keeps its own schedule with the change at the box's top" ;;
  *) ui_fail "Plate 1's schedule: $plate" ;;
esac
ui_finish "UI Send to Slicer's filament changes"
