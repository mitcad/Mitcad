#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ShortcutDialog.cpp app/framework/ShortcutBindings.hpp app/framework/CommandRegistry.cpp
# check-all sources: app/view/ShortcutOverview.cpp app/view/ViewController.cpp app/build/ListPages.cpp
# check-all sources: app/printing/PaperPrint.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/shortcut-sequences.mitcad-ui tools/ui-scripts/shortcut-sequences.json
# check-all sources: tools/ui-scripts/shortcut-sequences-driver.mitcad-ui app/framework/SettingsTransfer.cpp
# Key sequences in Keyboard Shortcuts and the Keyboard and Mouse Overview's
# cheat sheet (mitcad#2095) through the real UI, the shared input script
# tools/ui-scripts/shortcut-sequences.mitcad-ui (also run by
# tools/ui-windows-shortcut-sequences-test.ps1 and
# tools/ui-macos-shortcut-sequences-test.sh): G, L recorded for a command
# (ended by the pause, kept by Enter, the old key kept by Esc) and saved; G
# alone starts nothing in the 3D view, G then L starts the command; the
# overview's Save as PDF… writes the file it offers and Print… prints the
# same pages to MITCAD_TEST_PRINT_PDF's file; E, X for another command
# makes OK say that E (Extrude) is its start, with its row selected.
# mitcad#2096: the Command column's room at the default and the smallest
# size, the overview's and the cheat sheet's captions as the toolbar shows
# them, and the driver dropping a started sequence at a `type` step and in
# another window (tools/ui-scripts/shortcut-sequences-driver.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-shortcut-sequences-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-shortcut-sequences.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/shortcut-sequences.mitcad-ui
export MITCAD_TEST_PRINT_PDF=$work/printed.pdf
"$cli" run "$here/ui-scripts/shortcut-sequences.json" --save "$work/sequences.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sequences.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Shortcut sequences driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
# The Command column's room (mitcad#2096): at least what its longest name
# needs, at the default size and at the smallest.
columns=$(grep -o 'Keyboard Shortcuts columns: Command [0-9]* (needs [0-9]*), .*' "$UI_LOG")
[ "$(printf '%s\n' "$columns" | grep -c .)" -ge 2 ] || ui_fail "Keyboard Shortcuts logged no columns at both sizes"
printf '%s\n' "$columns" | awk '{ gsub(/[(),]/, " "); if ($5 + 0 < $7 + 0) bad = bad $0 "\n" } END { printf "%s", bad }' |
  grep . && ui_fail "the Command column is narrower than its longest name needs"
echo "ok   the Command column has the room its names need"
ui_expect_log 'Keyboard Shortcuts recording ended by Enter' 'Enter ends a recording and keeps G, L'
ui_expect_log 'Keyboard Shortcuts saved' 'G, L saved for Keyboard and Mouse Overview'
ui_expect_log 'is the start of E, X (Keyboard and Mouse Overview)' 'OK names the prefix clash in words'
pdf="$work/MitCad keyboard shortcuts.pdf"
ui_expect_log "Shortcut overview PDF written: $pdf (" 'Save as PDF… wrote the file it offered'
[ "$(head -c 5 "$pdf")" = "%PDF-" ] || ui_fail "$pdf is no PDF"
echo "ok   the cheat sheet is a PDF"
ui_expect_log "Printed Keyboard and Mouse Overview: " 'Print… printed the cheat sheet'
[ "$(head -c 5 "$work/printed.pdf")" = "%PDF-" ] || ui_fail "the printed cheat sheet is no PDF"
echo "ok   the printed cheat sheet is a PDF"
# The PDFs' text is read with poppler's pdftotext (poppler-utils,
# tools/dev-env/install-packages.sh); without it those checks are skipped.
pdf_text=0
command -v pdftotext > /dev/null && pdf_text=1
# The same rows on the same pages (mitcad#2096: they broke one row apart).
if [ "$pdf_text" = 1 ]; then
  diff <(pdftotext "$pdf" - | tr '\f' '#' | tr -s ' \n') <(pdftotext "$work/printed.pdf" - | tr '\f' '#' | tr -s ' \n') > /dev/null ||
    ui_fail "the saved and the printed cheat sheet break their pages apart"
  echo "ok   the saved and the printed cheat sheet have the same pages"
else
  echo "skip the cheat sheets' pages compared: no pdftotext (apt-get install poppler-utils)"
fi
# The overview's and the cheat sheet's captions name the tab and group as
# the toolbar shows them (mitcad#2096).
ui_expect_log 'Shortcut overview groups: ' 'the overview logs its group captions'
grep -q 'Shortcut overview groups: .*Solid › Create' "$UI_LOG" ||
  ui_fail "the overview has no Solid › Create caption"
grep -E 'Shortcut overview groups: .*[A-Z]{3,} / ' "$UI_LOG" && ui_fail "an overview caption is in the layout's capitals"
echo "ok   the overview's captions are as the toolbar shows them"
if [ "$pdf_text" = 1 ]; then
  pdftotext "$pdf" - | grep -q 'Solid › Create' || ui_fail "the cheat sheet has no Solid › Create caption"
  echo "ok   the cheat sheet's captions are as the toolbar shows them"
else
  echo "skip the cheat sheet's captions: no pdftotext (apt-get install poppler-utils)"
fi
ui_expect_log 'TestDriver: done' 'every step of the key sequences passed'

# The driver drops a started sequence at a `type` step and in another window
# (mitcad#2096), with G, L saved above.
export MITCAD_TEST_INPUT=$here/ui-scripts/shortcut-sequences-driver.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$work/sequences.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the driver sequence steps timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
[ "$(grep -c '^TestDriver: key sequence so far: G$' "$UI_LOG")" = 2 ] || ui_fail "G did not start a sequence twice"
ui_expect_log 'TestDriver: key sequence dropped: G (typed text)' 'a type step drops a started sequence'
ui_expect_log 'TestDriver: key sequence dropped: G (another window)' 'a key in another window drops it'
ui_expect_log 'TestDriver: done' 'every step of the driver sequences passed'
ui_finish 'UI Shortcut sequences'
