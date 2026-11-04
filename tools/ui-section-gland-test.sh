#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSection.cpp app/framework/SectionPreview.cpp app/framework/TestDriver.cpp
# check-all sources: core/cpp/bridge/analysis.cpp geometry/analysis/src/section.cpp geometry/src/thread.cpp
# check-all sources: tools/cli tools/ui-scripts/section-gland.mitcad-ui tools/ui-scripts/section-gland.json
# A cable gland body cut in its profile's plane (mitcad#1230), through the
# real UI: the shared input script tools/ui-scripts/section-gland.mitcad-ui
# (also run by tools/ui-windows-section-gland-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/section-gland.json): a body revolved
# from profiles in the XY plane with modelled M45x3 internal and M30x2
# external threads, Section Analysis at the XY plane (Distance 0, through
# the axis) and 1 mm off it. Both cut the body with caps; no clip fails.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-gland-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-section-gland.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/section-gland.mitcad-ui
"$cli" run "$here/ui-scripts/section-gland.json" --save "$work/gland.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/gland.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'section gland driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the threaded gland cut with caps in its profile plane and 1 mm off it'
ui_finish 'UI section gland'
