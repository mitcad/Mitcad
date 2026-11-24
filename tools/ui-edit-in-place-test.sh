#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.cpp app/MainWindowJobs.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp app/framework/TestDriverModel.cpp
# check-all sources: core/model/src/document/edit_in_place.rs core/model/src/document/save_component.rs core/model/src/document/link_updates.rs core/model-api/src/edit_in_place.rs core/ffi/src/edit_in_place.rs tools/cli
# check-all sources: tools/ui-scripts/edit-in-place.mitcad-ui tools/ui-scripts/edit-in-place-part.json tools/ui-scripts/edit-in-place-holder.json
# Edit in Place (mitcad#2321): a linked part edited inside the assembly that
# holds it, its document taken from its open tab, a sketch on another
# component's face frozen with its notice, an extrusion of it, Save writing
# the part's file, Finish returning to the holder with the part's new body
# as one undo step, the part's tab showing the same edit, and Esc finishing
# (tools/ui-scripts/edit-in-place.mitcad-ui).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-edit-in-place.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/edit-in-place.mitcad-ui
"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$work/part.mitcad" || exit 1
# The holder made in the part's folder: the link is part.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/edit-in-place-holder.json" --save "$work/holder.mitcad") || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/holder.mitcad"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'edit in place driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: quit' 'Edit in Place from a tab, a frozen pick, Save, Finish, Undo with Update, and Esc'
ui_finish 'UI Edit in Place'
