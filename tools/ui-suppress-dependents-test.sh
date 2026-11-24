#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/framework/TestDriver.cpp core/model/src/document.rs core/model-api/src/lib.rs tools/cli
# Suppress Features asks about the features that use it (mitcad#2314):
# Cancel, Suppress Only This, Suppress All with one Undo, and Unsuppress
# without a question (tools/ui-scripts/suppress-dependents.mitcad-ui).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-suppress-dependents.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/suppress-dependents.mitcad-ui
"$cli" run "$here/ui-scripts/suppress-dependents.json" --save "$work/chain.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/chain.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'suppress dependents driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Cancel, Suppress Only This, Suppress All, Undo and Unsuppress completed'
questions=$(grep -c '^Suppress Block: also Hole, Round?$' "$UI_LOG" || true)
[ "$questions" = 4 ] || { ui_fail "expected one question per Suppress, found $questions"; exit 1; }
ui_finish 'UI Suppress with dependents'
