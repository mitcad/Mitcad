#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/DocumentSnapshot.cpp app/browser/DocumentSnapshot.hpp
# check-all sources: core/model/src/document/operation_follow.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/new-component-fillets.mitcad-ui tools/ui-scripts/new-component-fillets.json
# An extrude edited from Join to New Component takes its fillet along
# (mitcad#2185) through the real UI, the shared input script
# tools/ui-scripts/new-component-fillets.mitcad-ui (also run by
# tools/ui-windows-new-component-fillets-test.ps1 and
# tools/ui-macos-new-component-fillets-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/new-component-fillets.json): a block with a joined
# boss and a fillet on the boss's edge, in a component. Edited to New
# Component, no feature fails and the timeline marks the extrude and the
# fillet in the new component's colour at once; undo and the edit back to
# Join bring the component's colour back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-new-component-fillets-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-new-component-fillets.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/new-component-fillets.mitcad-ui
export MITCAD_TEST_WORK=$work
"$cli" run "$here/ui-scripts/new-component-fillets.json" --save "$work/new-component-fillets.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/new-component-fillets.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "new component fillets driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Timeline feature colors: ' "the timeline logs its colour marks"
# The application's own line, at the start of its line: the driver's logged
# step (expect-never 'Recompute failed|…') only names it.
grep -q '^Recompute failed:' "$UI_LOG" && ui_fail "a feature failed"
ui_expect_log 'TestDriver: done' "the fillet followed the new component"
ui_finish 'UI New Component takes the fillets along'
