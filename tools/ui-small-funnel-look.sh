#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A small funnel's rounded rim (mitcad#1624): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# The funnel of tools/ui-scripts/small-funnel.json (or the design given as
# the second argument), shaded with its edges on the dark background, from
# the cameras of tools/ui-small-funnel-look.py; it saves the view as
#   <dir>/funnel-whole.png    the funnel from above at a slant
#   <dir>/funnel-rim.png      the rim's outer rounding where it is the
#                             outline, close up
#   <dir>/funnel-outlet.png   the outlet's near edge from 26 degrees
# and the app's log as <dir>/look.log.
#
# What to look for: the rim's rounding meets the background in a smooth
# curve, with no notches or saw teeth along it. At the outlet the open
# tube shows the background through it as a narrow lens at the outlet's
# near edge, in the background's colour and with its grid lines: that is
# the view through the tube and out of its open end (the shell leaves both
# ends open), not a mark on the faces.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-small-funnel-look.sh <dir> [design.mitcad]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-small-funnel-look.sh <dir> [design.mitcad]}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT

design=${2:-}
if [ -z "$design" ]; then
  design=$DIR/funnel.mitcad
  "$cli" run "$here/ui-scripts/small-funnel.json" --save "$design" || exit 1
fi
export MITCAD_MCP_DISCOVERY_DIR=$DIR/discovery
ui_start_display
ui_start_app --mcp-folder "$DIR" --open "$design"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_wait_idle
python3 -I "$here/ui-small-funnel-look.py" "$cli" "$DIR" || ui_fail "the pictures were not taken (above)"
cp -f "$UI_LOG" "$DIR/look.log"
ui_crashed && ui_fail "the application crashed"
ui_stop_app
ui_finish "Small funnel look pictures in $DIR"
