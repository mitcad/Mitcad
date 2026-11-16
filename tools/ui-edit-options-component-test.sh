#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/library/EditOptionsDialog.cpp app/library/OptionChooser.cpp app/library/PartOptions.cpp
# check-all sources: app/browser/BrowserController.cpp core/model/src/document/component_options.rs
# check-all sources: core/model-api/src/configurations.rs core/model-api/src/component_options.rs
# check-all sources: core/vcs/src/library tools/cli
# check-all sources: tools/ui-scripts/edit-options-component.mitcad-ui tools/ui-scripts/edit-options-component.json
# check-all sources: tools/ui-scripts/edit-options-component-plate.json
# Edit Options of an inserted library part (mitcad#1929) through the real
# UI, the shared input script tools/ui-scripts/edit-options-component.mitcad-ui
# (also run by tools/ui-windows-edit-options-component-test.ps1 and
# tools/ui-macos-edit-options-component-test.sh), with a library of one
# synthetic plate with options made here (mitcad-cli) and fetched from a
# local git repository, and a design that inserts it: Width changed in
# Edit Options is previewed without a refusal, and OK keeps it in one undo
# step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-edit-options-component-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-edit-options-component.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_LIBRARIES_DIR=$work/cache
lib=$work/test-options
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }

# The library: the plate with its options, tagged and fetched.
"$cli" library init "$lib" --id test-options --name "Test options" --license MIT > /dev/null || ui_fail 'library init'
"$cli" run "$here/ui-scripts/edit-options-component-plate.json" --save "$work/plate.mitcad" \
  > "$work/plate.log" 2>&1 || { cat "$work/plate.log"; ui_fail 'mitcad-cli: the plate'; }
"$cli" library add "$lib" "$work/plate.mitcad" --id plate --name Plate --designation Plate --category plates \
  > /dev/null || ui_fail 'library add plate'
g "$lib" add -A
g "$lib" commit -q -m 'Test part'
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" > /dev/null || ui_fail 'library fetch'

# The design that inserts it.
sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/edit-options-component.json" > "$work/design.json"
"$cli" run "$work/design.json" --save "$work/edit-options-component.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail 'mitcad-cli: the design'; }

export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/edit-options-component.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/edit-options-component.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Edit Options driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Edit Options option Width chosen: 40 mm' 'Width changed in Edit Options'
ui_expect_log 'Edit Options of Plate: ' 'OK kept the options with one command'
ui_expect_log 'TestDriver: done' 'the preview took the value and OK changed the plate in one undo step'
ui_finish 'UI Edit Options of a component'
