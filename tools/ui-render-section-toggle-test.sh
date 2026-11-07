#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render app/MainWindowSection.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/render-section-toggle.mitcad-ui tools/ui-scripts/render-section-toggle.json
# The render worker while a section analysis is shown and hidden in the
# rendered view (mitcad#1432), through the real UI: the shared input script
# tools/ui-scripts/render-section-toggle.mitcad-ui (also run by
# tools/ui-windows-render-section-toggle-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/render-section-toggle.json). Cycles
# renders on the CPU (Embree) with more samples than the test waits for,
# so every change reaches the worker while it traces; the worker must not
# stop, and it deletes the meshes it left only after its render thread has
# built the scene without them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-section-toggle-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-render-section-toggle.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/render-section-toggle.mitcad-ui
export MITCAD_RENDER_SAMPLES=100000
mkdir -p "$UI_CONFIG/Mitcad"
printf '[render]\ndevice=cpu\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
"$cli" run "$here/ui-scripts/render-section-toggle.json" --save "$work/render-section-toggle.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/render-section-toggle.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'render section toggle driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the section analysis shown and hidden twelve times while the worker rendered'
grep -q '^Render worker stopped' "$UI_LOG" && ui_fail 'the render worker stopped'
ui_finish 'UI render section toggle'
