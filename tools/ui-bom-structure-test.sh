#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/bom_structure.rs core/model-api/src/libraries.rs
# check-all sources: tools/ui-scripts/bom-structure.mitcad-ui tools/ui-scripts/bom-structure.json tools/cli
# The BOM Structure and the Bill of Materials' structured and parts-only
# views (mitcad#1985) through the real UI, the shared input script
# tools/ui-scripts/bom-structure.mitcad-ui (also run by
# tools/ui-windows-bom-structure-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/bom-structure.json: a box and a subassembly holding
# another box twice): the subassembly's BOM Structure set to Inseparable,
# Phantom and Reference, and both views changing with it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-bom-structure-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-bom-structure.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/bom-structure.mitcad-ui
"$cli" run "$here/ui-scripts/bom-structure.json" --save "$work/bom-structure.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/bom-structure.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'BOM Structure driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Build BOM item:   2.1 Rail x 2 (Normal)' 'the structured view'
ui_expect_log 'Build BOM item: 1 Base x 3 (Normal)' 'identical parts grouped in Parts only'
ui_expect_log 'Build BOM item: 2 Frame x 1 (Inseparable)' 'an Inseparable subassembly'
ui_expect_log 'Build BOM item: 2 Rail x 2 (Normal)' 'a Phantom'"'"'s children a level up'
ui_expect_log 'Build BOM item: - Frame x 0 (Reference)' 'a Reference without a number'
ui_expect_log 'TestDriver: done' 'every step of the BOM Structure passed'
ui_finish 'UI BOM Structure'
