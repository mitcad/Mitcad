#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp app/files/MainWindowLinks.cpp
# check-all sources: app/browser/BrowserController.cpp app/MainWindowContextMenu.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/link_parts.rs core/model/src/document/components.rs
# check-all sources: core/model/src/document/link_updates.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/link-parts.mitcad-ui tools/ui-scripts/link-parts-source.json
# check-all sources: tools/ui-scripts/link-parts-holder.json tools/ui-scripts/link-parts-outside.mitcad-ui
# check-all sources: core/model/src/document/link_copies.rs
# Linked parts of a design (mitcad#2130, GitHub mitcad/Mitcad#479) through
# the real UI, the shared input script tools/ui-scripts/link-parts.mitcad-ui
# (also run by tools/ui-windows-link-parts-test.ps1 and
# tools/ui-macos-link-parts-test.sh). mitcad-cli writes the design to link
# parts of (Bracket placed twice, and Board) and an empty design beside it,
# which the app opens. Insert Component > Linked > Choose parts... with
# Bracket:1 ticked links its body alone; Choose Linked Parts... from the
# linked component's browser row adds Bracket:2, one undo step, and Undo
# takes it back. A second run (Linux only, mitcad#2137) inserts the design
# from outside the holder's project: Copy into the Project and Link with
# Choose parts... ticked, Cancel in the parts list copies nothing, and
# Bracket:1 ticked links the copy's Bracket:1 alone.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that the path can be typed.
# Usage: tools/ui-link-parts-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-link-parts.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/link-parts-source.json" --save "$work/link-parts-source.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design to link parts of"; exit 1; }
"$cli" run "$here/ui-scripts/link-parts-holder.json" --save "$work/holder.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the holder"; exit 1; }
# The script with the linked design's path.
sed "s|@SOURCE@|$work/link-parts-source.mitcad|" "$here/ui-scripts/link-parts.mitcad-ui" > "$work/link-parts.mitcad-ui"
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$work/link-parts.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/holder.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "link parts driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Insert Component: linked, chosen parts' 'Linked > Choose parts... chosen'
ui_expect_log 'Linked parts of link-parts-source: Bracket:1' 'the insert links Bracket:1 alone'
ui_expect_log 'Choose Parts: O1, O2' 'Choose Linked Parts... ticks Bracket:2 too'
ui_expect_log 'TestDriver: done' 'linking chosen parts of a design'

# A design outside the holder's project (mitcad#2137): Copy into the
# Project and Link with Choose parts... ticked; Cancel in the parts list
# copies nothing, then Bracket:1 ticked links the copy's Bracket:1 alone
# (tools/ui-scripts/link-parts-outside.mitcad-ui). The holder in a project
# of its own, the design beside the project.
project=$work/project
mkdir -p "$project/.mitcad" "$project/designs"
printf '{"format": "mitcad-project", "version": 1}\n' > "$project/.mitcad/project.json"
"$cli" run "$here/ui-scripts/link-parts-holder.json" --save "$project/designs/holder.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the holder in a project"; exit 1; }
sed "s|@SOURCE@|$work/link-parts-source.mitcad|" "$here/ui-scripts/link-parts-outside.mitcad-ui" \
  > "$work/link-parts-outside.mitcad-ui"
export MITCAD_TEST_INPUT=$work/link-parts-outside.mitcad-ui
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$project/designs/holder.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "link parts outside the project driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Insert Component outside the project: copy into the project and link, chosen parts' \
  'Copy into the Project and Link > Choose parts... chosen'
ui_expect_log 'Choose Parts: canceled' 'the parts list canceled'
ui_expect_log 'Insert Component: copied link-parts-source.mitcad into the project as parts/link-parts-source.mitcad' \
  'copied under its own name: the canceled insert left no copy'
ui_expect_log 'Linked parts of link-parts-source: Bracket:1' 'the copy linked with Bracket:1 alone'
ui_expect_log 'TestDriver: done' 'linking chosen parts of a design outside the project'
copies=$(ls "$project/parts")
[ "$copies" = "link-parts-source.mitcad" ] || { ui_fail "the project's parts folder holds: $copies"; exit 1; }
cmp -s "$work/link-parts-source.mitcad" "$project/parts/link-parts-source.mitcad" ||
  { ui_fail "the copy differs from the file"; exit 1; }
echo "ok   one copy in the project's parts folder, the file's text"
ui_finish 'UI linked parts of a design'
