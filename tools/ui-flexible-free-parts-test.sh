#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/MainWindowJoints.cpp
# check-all sources: core/model/src/joints.rs core/model/src/document/flexible.rs core/model/src/document/joints.rs
# check-all sources: core/model/src/assembly.rs core/model-api/src/joints.rs tools/cli
# check-all sources: tools/ui-scripts/flexible-free-parts.json tools/ui-scripts/flexible-free-parts.mitcad-ui
# Flexible on a subassembly without joints of its own (mitcad#2016)
# through the real UI, the shared input script
# tools/ui-scripts/flexible-free-parts.mitcad-ui (also run by
# tools/ui-windows-flexible-free-parts-test.ps1 and
# tools/ui-macos-flexible-free-parts-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/flexible-free-parts.json): Gantry:1 is made
# Flexible from its row's menu and its free Joiner dragged along the
# root's slider, alone; Gantry:1's Carriage and Gantry:2 stay.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flexible-free-parts-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-flexible-free-parts.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/flexible-free-parts.mitcad-ui
"$cli" run "$here/ui-scripts/flexible-free-parts.json" --save "$work/flexible-free-parts.mitcad" ||
  { ui_fail 'mitcad-cli could not make the design'; exit 1; }
[ -f "$work/flexible-free-parts.mitcad" ] || { ui_fail 'the design was not made'; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/flexible-free-parts.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'flexible free parts driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Gantry:1 made flexible, its free Joiner dragged along the root'"'"'s joint alone'
ui_finish 'UI a subassembly without joints is flexible and its free part drags alone'
