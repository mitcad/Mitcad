#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of key sequences in Keyboard Shortcuts and of the Keyboard and
# Mouse Overview's cheat sheet (mitcad#2095): a one-off visual check, not a
# recurring test (its name is not ui-*-test.sh, so no check picks it up).
# In the Docked and the Floating layout, on the light and the dark theme
# (Qt's GTK platform theme), tools/ui-scripts/shortcut-sequences-look.mitcad-ui
# saves into <dir>:
#   keyboard-shortcuts-<look>.png          Keyboard Shortcuts, G, L given to Keyboard and Mouse Overview
#   keyboard-shortcuts-all-<look>.png      every row at the default size (mitcad#2096)
#   keyboard-shortcuts-smallest-<look>.png every row at the smallest size
#   keyboard-shortcuts-extrude-<look>.png  Extrude's row with its default E
#   keyboard-shortcuts-clash-<look>.png    the message of E, X clashing with E
#   overview-<look>.png                    the Keyboard and Mouse Overview with its buttons
#   cheat-sheet-<look>-<page>.png          the cheat sheet's pages (pdftoppm, 100 dpi)
#   cheat-sheet-search-<look>-<page>.png   the cheat sheet of the search "Sketch"
# <look> is docked-light, docked-dark, floating-light or floating-dark.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-shortcut-sequences-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-shortcut-sequences-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME
# A folder of this run's own: the save dialog would ask to replace a file of
# an earlier run.
work=$(mktemp -d "$DIR/files.XXXXXX")
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/shortcut-sequences-look.mitcad-ui
"$cli" run "$here/ui-scripts/shortcut-sequences.json" --save "$DIR/sequences.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; exit 1; }
ui_start_display
for chrome in docked floating; do
  for scheme in light dark; do
    export MITCAD_LOOK=$chrome-$scheme MITCAD_TEST_COLOR_SCHEME=$scheme
    ui_start_app --ui=qt --no-mcp --no-native-dialogs --chrome=$chrome --open "$DIR/sequences.mitcad"
    for _ in $(seq 1 900); do
      kill -0 "$UI_RUNNER" 2> /dev/null || break
      sleep 0.2
    done
    cp "$UI_LOG" "$DIR/look-$MITCAD_LOOK.log"
    grep -q 'TestDriver: done' "$UI_LOG" || ui_fail "the pictures of $MITCAD_LOOK were not all taken"
    for pdf in "$work/cheat-sheet-$MITCAD_LOOK.pdf" "$work/cheat-sheet-search-$MITCAD_LOOK.pdf"; do
      pdftoppm -r 100 -png "$pdf" "$DIR/$(basename "${pdf%.pdf}")"
      cp "$pdf" "$DIR/"
    done
    cp "$work"/*-"$MITCAD_LOOK".png "$DIR/"
    echo "ok   $MITCAD_LOOK"
  done
done
