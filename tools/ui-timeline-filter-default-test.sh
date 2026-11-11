#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/TimelineWidget.hpp app/browser/BrowserController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/timeline-filter-default.mitcad-ui tools/ui-scripts/timeline-filter-default-reopen.mitcad-ui
# check-all sources: tools/ui-scripts/timeline-filter-default.json
# The timeline filter's default and saved choice (mitcad#1662) through the
# real UI, the shared input scripts tools/ui-scripts/timeline-filter-default
# .mitcad-ui and timeline-filter-default-reopen.mitcad-ui (also run by
# tools/ui-windows-timeline-filter-default-test.ps1), on a design of two
# components that mitcad-cli makes (tools/ui-scripts/timeline-filter-default
# .json), in the test's own settings:
#   - The first start: Active Component with its subcomponents, every
#     feature listed with the root active; Base activated shows its and
#     Pin's features at once, the filter button highlighted; one named
#     component is not saved; All Components is.
#   - The window closed and started again: All Components.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-filter-default-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-timeline-filter-default.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/timeline-filter-default.json" --save "$work/timeline-filter.mitcad" || exit 1
ui_start_display

# run_script name: the app with the shared script ui-scripts/<name>.mitcad-ui
# on the design, until the script ends it.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --open "$work/timeline-filter.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
}

echo "--- The first start: Active Component with its subcomponents"
run_script timeline-filter-default
ui_expect_log 'Timeline filter: Active Component (Root) with Subcomponents: 4 of 4 features' \
  'the timeline starts at Active Component: every feature with the root active'
ui_expect_log 'Timeline filter: Active Component (Base) with Subcomponents: 3 of 4 features' \
  'Base activated: its features and its subcomponent'"'"'s'
ui_expect_log 'Timeline filter: Base with Subcomponents: 3 of 4 features' 'one named component shown'
ui_expect_log 'Timeline filter saved: all, subcomponents' 'All Components saved'
ui_expect_log 'TestDriver: done' 'the first start'"'"'s script ran to its end'
grep -q '^Timeline filter saved: active' "$UI_LOG" && ui_fail 'the default was saved without a choice'
echo "ok   only the user's choice was saved"

echo "--- The window started again: All Components"
run_script timeline-filter-default-reopen
ui_expect_log 'Timeline filter: all components' 'the next start takes the saved All Components'
ui_expect_log 'Timeline filter saved: active, subcomponents' 'Active Component chosen again and saved'
ui_expect_log 'TestDriver: done' 'the second start'"'"'s script ran to its end'
ui_finish 'UI timeline filter default'
