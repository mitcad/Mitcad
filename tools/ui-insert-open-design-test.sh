#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/insert-open-design-script.py tools/ui-scripts/insert-open-design-first.json tools/ui-scripts/insert-open-design-second.json tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-insert-open-design.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$work/workflow.mitcad-ui
python3 "$here/ui-scripts/insert-open-design-script.py" qt > "$MITCAD_TEST_INPUT" || exit 1
for design in first second; do
  "$cli" run "$here/ui-scripts/insert-open-design-$design.json" --save "$work/$design.mitcad" || exit 1
done
ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/first.mitcad"
for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'open-design insertion driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: quit' 'open-design selection, saved-state insertion, Save/Cancel and independent undo'
ui_finish 'UI insert an open design'
