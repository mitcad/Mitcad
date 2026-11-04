#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSection.cpp app/framework/SectionPreview.cpp app/framework/MessageBand.cpp
# check-all sources: app/framework/MessageLog.cpp app/browser/BrowserPanel.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/section-clip-report.mitcad-ui tools/ui-scripts/section-clip-report.json
# A section that shows a body whole because its clip failed is reported
# (mitcad#1230), through the real UI: the shared input script
# tools/ui-scripts/section-clip-report.mitcad-ui (also run by
# tools/ui-windows-section-clip-report-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/section-clip-report.json), with
# MITCAD_TEST_SECTION_CLIP_FAIL=F2.b0 making Body1's clips fail. The
# message band shows the warning once, the history the error, the
# Analysis folder's row its warning style and tooltip; hiding the analysis
# withdraws both; the Section Analysis panel's section reports it too, and
# its plane past the body gives the panel's status line text; an analysis
# whose plane cuts no shown body says so in its row's tooltip.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-clip-report-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-section-clip-report.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/section-clip-report.mitcad-ui
export MITCAD_TEST_SECTION_CLIP_FAIL=F2.b0
"$cli" run "$here/ui-scripts/section-clip-report.json" --save "$work/clip-report.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/clip-report.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'section clip report driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'a failed clip reported in the band and the analysis row, withdrawn when hidden'
ui_finish 'UI section clip report'
