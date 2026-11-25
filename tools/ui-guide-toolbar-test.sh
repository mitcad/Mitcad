#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials/GuideStep.cpp app/tutorials/GuideStep.hpp app/tutorials/TutorialGuide.cpp
# check-all sources: app/tutorials/TutorialGuide.hpp app/tutorials/MainWindowGuide.cpp
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/framework/TestDriverCommands.cpp
# check-all sources: app/framework/TestDriver.cpp app/MainWindowMcp.cpp tools/cli
# check-all sources: tools/ui-scripts/guide-toolbar.mitcad-ui tools/ui-scripts/workspaces.json
# A tutorial's guide steps on the toolbar's own places (mitcad#2370, GitHub
# mitcad/Mitcad#630; docs/tutorials.md "Guide steps") through the real UI,
# the input script tools/ui-scripts/guide-toolbar.mitcad-ui, once in the
# Floating and once in the Docked layout, on a box with one drawing that
# mitcad-cli makes (tools/ui-scripts/workspaces.json): the workspace
# selector outlined and opening it completing its step; choosing Design
# completing the next, with its list open and Design shown, and at once
# with Design shown and the list closed; Make's entry outlined in the open
# list, Escape closing the list and not the step, Make chosen; the Surface
# tab outlined and a click on it completing its step; the timeline outlined
# until the step is ended. Here the outlines are checked against the places
# the toolbar logs: the selector's and the tab's centres lie in their
# steps' outlined rectangles, Design's entry in its step's, and the
# timeline's outline is a wide bar.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-guide-toolbar-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-guide-toolbar.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/guide-toolbar.mitcad-ui
"$cli" run "$here/ui-scripts/workspaces.json" --save "$work/guide.mitcad" || exit 1
ui_start_display

# guide_covers <log> <step> <place regex>: "ok ..." when the first outline
# the step logged in the top window's coordinates ("Guide: step <step>
# outline at x,y wxh") holds the centre of the place logged last before it
# ("... at cx,cy", the same coordinates).
guide_covers() {
  awk -v step="$2" -v place="$3" '
    $0 ~ place { split($NF, c, ","); cx = c[1]; cy = c[2] }
    index($0, "Guide: step " step " outline at ") == 1 && !found {
      found = 1
      split($(NF - 1), p, ","); split($NF, s, "x")
      ok = cx != "" && cx + 0 >= p[1] + 0 && cx + 0 <= p[1] + s[1] && cy + 0 >= p[2] + 0 && cy + 0 <= p[2] + s[2]
      print (ok ? "ok" : "no") " centre " cx "," cy " in " $(NF - 1) " " $NF
    }
    END { if (!found) print "no outline logged" }' "$1"
}

for layout in floating docked; do
  ui_start_app --ui=qt --no-mcp --chrome=$layout --open "$work/guide.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the guide driver timed out ($layout)"
  wait "$UI_RUNNER" || ui_fail "the guide driver failed ($layout): $(grep -E 'TestDriver' "$UI_LOG" | tail -8)"
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "the toolbar's guide steps ($layout)"
  ui_expect_log 'Guide: step selector done: the workspace selector opened' "opening the selector completes its step ($layout)"
  ui_expect_log 'Guide: step design done: the workspace DESIGN chosen' "choosing Design, the one shown, in the open list ($layout)"
  ui_expect_log 'Guide: step design-again done: the workspace DESIGN chosen' "Design shown already: done at once ($layout)"
  ui_expect_log 'Guide: step make done: the workspace MAKE chosen' "choosing Make after Escape closed the list ($layout)"
  ui_expect_log 'Guide: step surface done: the tab SURFACE activated' "a click on the Surface tab ($layout)"
  ui_expect_log 'Guide: step timeline left (ended)' "the timeline step ended by the page ($layout)"
  for check in "selector|^Ribbon workspace at |the workspace selector" \
               "design|^Ribbon workspace entry DESIGN at |Design's entry in the open list" \
               "surface|^Ribbon tab SURFACE at |the Surface tab"; do
    IFS='|' read -r step place what <<< "$check"
    result=$(guide_covers "$UI_LOG" "$step" "$place")
    [ "${result%% *}" = ok ] || ui_fail "$what is not outlined ($layout): $result"
    echo "ok   $what outlined ($layout): ${result#ok }"
  done
  timeline=$(grep -m 1 -E '^Guide: step timeline outline at -?[0-9]+,-?[0-9]+ [0-9]+x[0-9]+$' "$UI_LOG")
  size=${timeline##* }
  [ -n "$timeline" ] && [ "${size%x*}" -gt "${size#*x}" ] || ui_fail "the timeline is not outlined as a bar ($layout): '$timeline'"
  echo "ok   the timeline outlined ($layout): ${timeline#*outline at }"
  cp -f "$UI_LOG" "$work/$layout.log"
done
ui_finish 'UI guide steps on the toolbar'
