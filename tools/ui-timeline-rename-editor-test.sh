#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/TimelineWidget.hpp app/browser/BrowserController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/timeline-rename-editor.mitcad-ui tools/ui-scripts/timeline-rename-editor.json
# The timeline's rename editor is never cut short (mitcad#1742, GitHub
# mitcad/Mitcad#304) through the real UI, the input script
# tools/ui-scripts/timeline-rename-editor.mitcad-ui on a design of thirty
# sketches that mitcad-cli makes (tools/ui-scripts/timeline-rename-editor
# .json), in the Floating and the Docked layout:
#   - The last item renamed with a name of 30 characters in a wide window,
#     where the strip ends just after it: every place of the editor lies
#     wholly inside the visible timeline and is at least as wide as its
#     text.
#   - In a window 820 px wide, the timeline scrolled to its end: the last
#     item's editor opens with the 30 characters, inside the visible
#     timeline and at least as wide as them, moved left against the
#     visible right edge.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-rename-editor-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-timeline-rename-editor.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
"$cli" run "$here/ui-scripts/timeline-rename-editor.json" --save "$work/rename-editor.mitcad" || exit 1
ui_start_display
export MITCAD_TEST_INPUT=$here/ui-scripts/timeline-rename-editor.mitcad-ui
name='Rounded corner of the last arm'

# check_editor layout: every logged place of the editor inside the visible
# timeline and at least as wide as its text; the place after the second
# Rename against the visible right edge.
check_editor() {
  local layout=$1 lines second
  lines=$(grep -c '^Timeline rename editor at ' "$UI_LOG")
  [ "$lines" -ge 2 ] || ui_fail "$layout: the editor's place was logged $lines times"
  awk -v layout="$layout" '
    /^Renaming Rounded corner of the last arm in the timeline$/ { second = 1; first = 1; next }
    /^Timeline rename editor at / {
      # Timeline rename editor at X,Y WxH in the visible VWxVH, text N wide
      split($5, at, ","); split($6, size, "x"); split($10, visible, "x"); text = $12
      sub(/,$/, "", visible[2])
      x = at[1] + 0; y = at[2] + 0; w = size[1] + 0; h = size[2] + 0
      vw = visible[1] + 0; vh = visible[2] + 0
      if (x < 0 || y < 0 || x + w > vw || y + h > vh) {
        printf "FAIL: %s: the editor %d,%d %dx%d leaves the visible timeline %dx%d\n", layout, x, y, w, h, vw, vh
        bad = 1
      }
      if (w < text + 0) {
        printf "FAIL: %s: the editor is %d wide, its text %d\n", layout, w, text
        bad = 1
      }
      if (second && first) {
        first = 0; seen = 1
        if (x + w != vw) {
          printf "FAIL: %s: the last item'"'"'s editor ends at %d, not at the visible right edge %d\n", layout, x + w, vw
          bad = 1
        }
        if (text + 0 < 100) {
          printf "FAIL: %s: the editor opened with a text %d wide, not the 30 characters\n", layout, text
          bad = 1
        }
      }
    }
    END {
      if (!seen) { printf "FAIL: %s: no editor place after the second Rename\n", layout; bad = 1 }
      exit bad
    }' "$UI_LOG" || ui_fail "$layout: the rename editor was cut short or left the timeline"
  echo "ok   $layout: the editor inside the visible timeline, as wide as its text ($lines places)"
}

for layout in floating docked; do
  echo "--- $layout: renaming the last item"
  ui_start_app --ui=qt --no-mcp --chrome=$layout --open "$work/rename-editor.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$layout: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log "Renamed Sketch30 to $name" "$layout: the last item renamed with 30 characters"
  ui_expect_log 'TestDriver: done' "$layout: the script ran to its end"
  check_editor "$layout"
done
ui_finish 'UI timeline rename editor'
