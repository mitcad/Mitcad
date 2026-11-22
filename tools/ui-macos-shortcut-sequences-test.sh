#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ShortcutDialog.cpp app/framework/ShortcutBindings.hpp app/framework/CommandRegistry.cpp
# check-all sources: app/view/ShortcutOverview.cpp app/view/ViewController.cpp app/build/ListPages.cpp
# check-all sources: app/printing/PaperPrint.cpp app/framework/TestDriver.cpp app/platform/macos/MacChrome.mm tools/cli
# check-all sources: tools/ui-scripts/shortcut-sequences.mitcad-ui tools/ui-scripts/shortcut-sequences-menu.mitcad-ui
# check-all sources: tools/ui-scripts/shortcut-sequences.json
# Key sequences in Keyboard Shortcuts and the Keyboard and Mouse Overview's
# cheat sheet (mitcad#2095) in the Qt frontend on macOS, as
# tools/ui-shortcut-sequences-test.sh on Linux: the shared steps
# tools/ui-scripts/shortcut-sequences.mitcad-ui; then
# tools/ui-scripts/shortcut-sequences-menu.mitcad-ui: the Help menu's
# Keyboard and Mouse Overview with G, L shows the sequence in its title
# (Qt's " (G, L)"), has no key equivalent, and runs on G, L, not on G alone.
# Usage: tools/ui-macos-shortcut-sequences-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-shortcut-sequences-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=200
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
export MITCAD_TEST_PRINT_PDF=$MITCAD_TEST_WORK/printed.pdf
# Each key press and where it went (0.69.0-rc3: the log of one run ended in
# the middle of a type step, and the app exited 1 without its FAIL line).
export MITCAD_LOG_KEYS=1
design=$MITCAD_TEST_WORK/sequences.mitcad
"$cli" run "$here/ui-scripts/shortcut-sequences.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run shortcut-sequences.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/shortcut-sequences.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app shortcut-sequences --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Keyboard Shortcuts recording ended by Enter$' 'Enter ends a recording and keeps G, L'
ui_expect_log '^Keyboard Shortcuts conflict: E \(Extrude\) is the start of E, X \(Keyboard and Mouse Overview\)' \
  'OK names the prefix clash in words'
ui_expect_log '^Shortcut overview PDF written: .*MitCad keyboard shortcuts[.]pdf \([0-9]+ page\(s\), A4\)$' \
  'Save as PDF… wrote the file it offered'
ui_expect_log '^Printed Keyboard and Mouse Overview: [0-9]+ page\(s\) to ' 'Print… printed the cheat sheet'
for file in "$MITCAD_TEST_WORK/MitCad keyboard shortcuts.pdf" "$MITCAD_TEST_PRINT_PDF"; do
  if [ "$(head -c 5 "$file" 2> /dev/null)" = "%PDF-" ]; then ui_pass "$(basename "$file") is a PDF"
  else ui_fail "$file is no PDF"; fi
done
ui_expect_log '^TestDriver: done' 'every step of the key sequences passed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed key sequence assertion'
MITCAD_TEST_INPUT=$here/ui-scripts/shortcut-sequences-menu.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app shortcut-sequences-menu --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Keyboard Shortcuts key: Keyboard and Mouse Overview \| .* = G, L$' 'G, L given to the menu command'
ui_expect_log '^Shortcut overview: [0-9]+ command shortcuts$' 'G, L runs the menu command'
ui_expect_log '^TestDriver: done' 'the menu bar shows and runs the sequence'
ui_expect_no_log 'TestDriver: FAIL' 'no failed menu bar assertion'
if [ "$UI_FAILURES" != 0 ]; then
  # A full disk ends the log while the app still runs (its FAIL line lost).
  df -h "$UI_OUT" | sed 's/^/     | /'
  exit 1
fi
echo 'PASS: Qt shortcut sequences'
