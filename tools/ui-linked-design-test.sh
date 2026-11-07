#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp app/framework/MessageBand.cpp app/framework/MessageLog.cpp
# check-all sources: core/model/src/document/link_updates.rs core/model/src/document/components.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/linked-design.mitcad-ui tools/ui-scripts/linked-design-part.json tools/ui-scripts/linked-design-holder.json
# Linked designs while the holder is open (mitcad#1416): the notice with
# Open Linked Design on an edit of a linked component (not on selecting it),
# the linked design opened in a tab of its own from the notice and from the
# browser's context menu, the message band's "part.mitcad changed" with
# Update when the part is saved in its tab, Update and the context menu's
# Update Link each one undo step (tools/ui-scripts/linked-design.mitcad-ui).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-linked-design.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/linked-design.mitcad-ui
"$cli" run "$here/ui-scripts/linked-design-part.json" --save "$work/part.mitcad" || exit 1
# The holder made in the part's folder: the link is part.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/linked-design-holder.json" --save "$work/holder.mitcad") || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/holder.mitcad"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'linked design driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: quit' 'linked design notice, Open Linked Design, the changed-file notice and Update Link'
ui_finish 'UI linked designs'
