#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/Dialogs.cpp app/framework/TestDriver.cpp app/WindowManager.cpp
# check-all sources: core/model-api/src/lib.rs core/ffi/src/ipt_import.rs core/ipt core/tests tools/cli
# check-all sources: tools/ui-scripts/import-into-design.mitcad-ui tools/ui-scripts/import-into-design.json
# Design files of other formats into the open design as components
# (mitcad#1894) through the real UI, the shared input script
# tools/ui-scripts/import-into-design.mitcad-ui (also run by
# tools/ui-windows-import-into-design-test.ps1 and
# tools/ui-macos-import-into-design-test.sh), on a design with content
# (tools/ui-scripts/import-into-design.json, a 20 mm cube) and two .ipt test
# parts that test_exchange writes (--write-ipt: a cube and a cylinder,
# aluminium; --write-ipt-design: a parameter, a sketch and an extrusion):
#   1. File > Import of both at once: Where to Import asks once, Into This
#      Design: each a component named after its file, with its history,
#      the second beside the first; the Browser lists both, the timeline
#      their bands; one undo step; each file's import report.
#   2. Undo takes both out: the cube alone again.
#   3. File > Import of one part, A New Design: a new tab with the part,
#      this design as it was.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-import-into-design-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
test_exchange=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-import-into-design.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/import-into-design.mitcad-ui

"$cli" run "$here/ui-scripts/import-into-design.json" --save "$work/assembly.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
"$test_exchange" --write-ipt "$work/part.ipt" > "$work/parts.log" 2>&1 &&
  "$test_exchange" --write-ipt-design "$work/designed.ipt" >> "$work/parts.log" 2>&1 ||
  { cat "$work/parts.log"; ui_fail "test_exchange: the test parts"; exit 1; }
ui_start_display

ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/assembly.mitcad"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the import driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "the import driver failed"; exit 1; }
ui_expect_log 'TestDriver: done' \
  'two .ipt parts imported into the design as components in one undo step; a third as a new design'
ui_finish 'UI import into this design'
