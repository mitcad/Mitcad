#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/browser/BrowserPanel.cpp app/browser/DocumentSnapshot.cpp
# check-all sources: app/MainWindowJoints.cpp core/model/src/document/link_tree.rs core/model/src/document/components.rs
# check-all sources: core/model/src/document/flexible.rs core/model/src/document/joints.rs core/model/src/joints.rs
# check-all sources: core/model/src/document/link_updates.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/linked-flexible.mitcad-ui tools/ui-scripts/linked-flexible-source.json
# check-all sources: tools/ui-scripts/linked-flexible-holder.json
# A design linked with its components made Flexible (mitcad#2209, GitHub
# mitcad/Mitcad#533) through the real UI, the shared input script
# tools/ui-scripts/linked-flexible.mitcad-ui (also run by
# tools/ui-windows-linked-flexible-test.ps1 and
# tools/ui-macos-linked-flexible-test.sh). mitcad-cli writes the design (a
# Key sliding along x on a grounded Base) and a design beside it that links
# it whole, which the app opens: Flexible is offered on the linked
# sub-assembly's Browser row, and a drag moves the Key along the design's
# own slider for that occurrence alone.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-linked-flexible-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-linked-flexible.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/linked-flexible-source.json" --save "$work/linked-flexible-source.mitcad" \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design to link'; exit 1; }
# The holder with the linked design's path.
sed "s|@SOURCE@|$work/linked-flexible-source.mitcad|" "$here/ui-scripts/linked-flexible-holder.json" \
  > "$work/linked-flexible-holder.json"
"$cli" run "$work/linked-flexible-holder.json" --save "$work/holder.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the holder'; exit 1; }
[ -f "$work/holder.mitcad" ] || { ui_fail 'the holder was not made'; exit 1; }
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/linked-flexible.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/holder.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'linked flexible driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Slider:1 made flexible, its Key dragged along the design'"'"'s slider alone'
ui_finish 'UI a design linked with its components is flexible and its slider part drags'
