#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/printing app/files/MainWindowFiles.cpp app/sketch/SketchModel.cpp
# File › Print… (mitcad#1089) of a sketch through the real UI, printing to a
# PDF file instead of the system's dialog (MITCAD_TEST_PRINT_PDF):
#   - With nothing to print, Print is disabled ("Print available: nothing")
#     and Ctrl+P opens nothing.
#   - In an empty sketch it stays disabled (mitcad#1156: no blank page);
#     with a 300 x 200 mm rectangle typed in it is enabled, and Ctrl+P
#     opens the Print Sketch panel (1:1, A4 by Home in Paper: two pages in
#     one row, portrait), which is cancelled (Escape).
#   - Dimensions checked make the picture larger; unchecked again.
#   - Print… (Enter) prints the two tiles to the PDF: two A4 pages.
#   - Fit to page (Alt+F): one page, landscape; printed, the PDF one page.
#   - The sketch finished: nothing to print again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-paper-print-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-paper-print.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
PDF=$WORK/printed.pdf
export MITCAD_TEST_PRINT_PDF=$PDF

# The pages of the PDF in A4 portrait or landscape: their media boxes in
# points as Qt writes them, within a point.
a4_pages() {
  grep -aoE '/MediaBox *\[ *0 +0 +[0-9.]+ +[0-9.]+ *\]' "$PDF" |
    sed -E 's/.*\[ *0 +0 +([0-9.]+) +([0-9.]+) *\]/\1 \2/' |
    awk -v want="$1" '{ w = $1; h = $2
      if ((want == "portrait" && w > 594 && w < 597 && h > 841 && h < 843) ||
          (want == "landscape" && h > 594 && h < 597 && w > 841 && w < 843)) n++ }
      END { print n + 0 }'
}
pdf_pages() { grep -aoE '/Type */Page([^s]|$)' "$PDF" | wc -l | tr -d ' '; }
# The panel's choices as logged last, when it opened or changed: ": 304 x
# 204 mm, 1:1, A4 portrait, 2 page(s), 1 x 2".
panel_state() {
  grep -E '^Print panel(:| opened:) ' "$UI_LOG" | tail -1 | sed -E 's/^Print panel opened: [^,]*, /: /; s/^Print panel: /: /'
}

ui_start_display
ui_start_app

echo "--- Nothing to print"
ui_expect_log "Print available: nothing" "Print is disabled with nothing to print"
ui_mark
ui_step "Print (Ctrl+P)"                   ui_key ctrl+p
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Print panel opened"; then
  ui_fail "Print opened its panel with nothing to print"
fi
echo "ok   Ctrl+P does nothing"

echo "--- A sketch: empty, nothing to print; drawn in, enabled; the panel opened and cancelled"
ui_mark
ui_step "create sketch on XY"              ui_create_sketch xy
ui_step "Print (Ctrl+P) in the empty sketch" ui_key ctrl+p
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Print available: sketch|Print panel opened"; then
  ui_fail "Print is available in an empty sketch (a blank page, mitcad#1156)"
fi
echo "ok   an empty sketch has nothing to print"
ui_step "rectangle tool (R)"               ui_key r
ui_step "first corner at the origin"       ui_sketch_click 0 0
ui_step "width 300 (Tab) height 200"       eval 'xdotool type --delay 40 300; ui_key Tab; xdotool type --delay 40 200'
ui_step "confirm (Enter)"                  ui_key Return
ui_expect_log "Added rectangle 300 x 200 mm at (0, 0)" "a 300 x 200 rectangle"
ui_expect_new "Print available: sketch" "Print is enabled in the sketch with its rectangle"
ui_step "end the tool (Esc)"               ui_key Escape
ui_mark
ui_step "Print (Ctrl+P)"                   ui_key ctrl+p
ui_focus_dialog '^Print Sketch$'
ui_expect_new "Print panel opened: Sketch" "the Print Sketch panel"
ui_expect_new ", 1:1, " "at 1:1 to start with"
ui_step "Paper: A4 (Alt+P, Home)"          ui_key alt+p Home
ui_sync
state=$(panel_state)
case "$state" in
*": 304 x 204 mm, 1:1, A4 portrait, 2 page(s), 1 x 2") echo "ok   two A4 pages, one row, portrait: $state" ;;
*) ui_fail "the panel is not at two A4 pages in one row: $state" ;;
esac
ui_mark
ui_step "Dimensions (Alt+D)"               ui_key alt+d
ui_expect_new "Print panel: " "the dimensions change the picture"
state=$(panel_state)
case "$state" in
*": 304 x 204 mm,"*) ui_fail "the dimensions left the picture as it was: $state" ;;
*) echo "ok   with the dimensions: $state" ;;
esac
ui_step "Dimensions off (Alt+D)"           ui_key alt+d
ui_sync
ui_mark
ui_step "cancel (Escape)"                  ui_key Escape
ui_focus_main
ui_expect_new "Print panel cancelled" "the panel cancelled"
[ ! -e "$PDF" ] || ui_fail "the cancelled panel printed"

echo "--- 1:1: two tiles"
ui_mark
ui_step "Print (Ctrl+P)"                   ui_key ctrl+p
ui_focus_dialog '^Print Sketch$'
ui_expect_new "Print panel opened: Sketch1, 304 x 204 mm, 1:1, A4 portrait, 2 page(s), 1 x 2" \
  "the panel again, with the paper chosen"
ui_step "Print (Enter)"                    ui_key Return
ui_focus_main
ui_expect_new "Print dialog skipped (test): printing to $PDF" "the test's PDF in place of the dialog"
ui_expect_new "Printed Sketch1: 2 page(s), 1:1, 1 x 2 to $PDF" "two pages printed" 20
[ "$(head -c 5 "$PDF")" = "%PDF-" ] || ui_fail "$PDF is no PDF"
[ "$(pdf_pages)" = 2 ] || ui_fail "the PDF has $(pdf_pages) pages, not 2"
[ "$(a4_pages portrait)" = 2 ] || ui_fail "the PDF has $(a4_pages portrait) A4 portrait pages, not 2"
echo "ok   the PDF has two A4 pages"

echo "--- Fit to page: one page"
ui_mark
ui_step "Print (Ctrl+P)"                   ui_key ctrl+p
ui_focus_dialog '^Print Sketch$'
ui_expect_new "Print panel opened: Sketch1" "the panel"
ui_step "Fit to page (Alt+F)"              ui_key alt+f
ui_expect_new "Print panel: 304 x 204 mm, fit, A4 landscape, 1 page(s), 1 x 1, scale 0.911" \
  "one page, landscape, the picture at 91 %"
ui_step "Print (Enter)"                    ui_key Return
ui_focus_main
ui_expect_new "Printed Sketch1: 1 page(s), fit, 1 x 1 to $PDF" "one page printed" 20
[ "$(pdf_pages)" = 1 ] || ui_fail "the PDF has $(pdf_pages) pages, not 1"
[ "$(a4_pages landscape)" = 1 ] || ui_fail "the PDF's page is not A4 landscape"
echo "ok   the PDF has one A4 landscape page"

echo "--- The sketch finished: nothing to print"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_new "Print available: nothing" "Print is disabled again"

ui_finish "UI paper print test"
