#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowContextMenu.cpp app/framework/TestDriver.cpp tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-delete-selection.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/delete-feature-selection.mitcad-ui
"$cli" run "$here/ui-scripts/delete-feature-selection.json" --save "$work/selection.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/selection.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'selection delete driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'selection deletion and atomic Undo completed'
questions=$(grep -c '^Delete C, Source: also Dependent?$' "$UI_LOG" || true)
[ "$questions" = 2 ] || { ui_fail "expected one question per attempt, found $questions"; exit 1; }
ui_finish 'UI feature selection Delete'
