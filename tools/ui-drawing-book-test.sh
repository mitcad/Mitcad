#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings/DrawingBookDialog.cpp app/printing app/files/MainWindowFiles.cpp
# check-all sources: core/model/src/document/drawing_book.rs core/model-api/src/drawing_book.rs core/model/src/file/v2.rs tools/cli
# check-all sources: tools/ui-scripts/drawing-book.mitcad-ui tools/ui-scripts/drawing-book-reopen.mitcad-ui tools/ui-scripts/drawing-book-a.json tools/ui-scripts/drawing-book-b.json tools/ui-scripts/drawing-book-b-gone.json tools/ui-scripts/drawing-book-main.json
# File › Drawing Book… (mitcad#2448, GitHub mitcad/Mitcad#687) through the
# real UI on a main design that links a.mitcad twice and b.mitcad once,
# each with a drawing named Drawing1 (a's of two sheets, and an Extra):
#   - the dialog offers a's drawings once and b's; both Drawing1 are
#     checked apart, b's design moved first, the cover page checked and a
#     title typed; Save is one undo step and File › Save writes the book;
#   - opened again it shows the same choice, and Print… prints the cover,
#     b's sheet and a's two sheets to a PDF (four pages, Extra left out),
#     the linked files unchanged;
#   - b saved again without its drawing and the main design opened again:
#     the book kept its order and cover page, b's Drawing1 is named as gone
#     and Print… waits until it is unchecked.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-book-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-book.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_PRINT_PDF=$work/book.pdf
"$cli" run "$here/ui-scripts/drawing-book-a.json" --save "$work/a.mitcad" || exit 1
"$cli" run "$here/ui-scripts/drawing-book-b.json" --save "$work/b.mitcad" || exit 1
# The main design made in the linked designs' folder: the links are a.mitcad
# and b.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/drawing-book-main.json" --save "$work/main.mitcad") || exit 1
a_before=$(cat "$work/a.mitcad")
b_before=$(cat "$work/b.mitcad")

# ui_run_script <script> <what>: the app on main.mitcad with the driver's
# script, until it quits.
ui_run_script() {
  export MITCAD_TEST_INPUT=$1
  ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/main.mitcad"
  for _ in $(seq 1 1800); do
    kill -0 "$UI_RUNNER" 2>/dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail "$2: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$2"; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: quit' "$2"
}

ui_start_display

echo "--- The book chosen, saved and printed"
ui_run_script "$here/ui-scripts/drawing-book.mitcad-ui" 'the drawing book chosen, saved and printed'
pages=$(grep -aoE '/Type */Page([^s]|$)' "$MITCAD_TEST_PRINT_PDF" | wc -l | tr -d ' ')
[ "$pages" = 4 ] || ui_fail "the book's PDF has $pages pages, not 4 (the cover, b's sheet, a's two)"
echo "ok   the PDF has the cover and three sheets"
grep -q '"drawing_book"' "$work/main.mitcad" || ui_fail "main.mitcad keeps no drawing book"
grep -q '"cover": true' "$work/main.mitcad" || ui_fail "main.mitcad keeps no cover page"
grep -q '"title": "Project book"' "$work/main.mitcad" || ui_fail "main.mitcad keeps no book title"
echo "ok   main.mitcad keeps the book"
[ "$(cat "$work/a.mitcad")" = "$a_before" ] || ui_fail "printing the book changed a.mitcad"
[ "$(cat "$work/b.mitcad")" = "$b_before" ] || ui_fail "printing the book changed b.mitcad"
echo "ok   the linked designs are unchanged"

echo "--- b saved without its drawing: named before printing"
"$cli" run "$here/ui-scripts/drawing-book-b-gone.json" --save "$work/b.mitcad" || exit 1
ui_run_script "$here/ui-scripts/drawing-book-reopen.mitcad-ui" 'the drawing gone named before printing'

ui_finish 'UI drawing book'
