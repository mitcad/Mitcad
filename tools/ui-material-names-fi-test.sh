#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Appearances.cpp app/framework/Appearances.hpp app/print/PrintLibrary.cpp
# check-all sources: app/print/PrintLibrary.hpp app/print/MaterialsDialog.cpp app/commands/MaterialCommands.cpp
# check-all sources: app/browser/LibraryPicker.cpp app/framework/Language.cpp app/translations
# check-all sources: tools/ui-scripts/material-names-fi.mitcad-ui tools/ui-scripts/material-names-fi.json
# The built-in materials' names in Finnish (mitcad#1689), on a private Xvfb
# display: MitCad started with --language fi runs the test driver's script
# tools/ui-scripts/material-names-fi.mitcad-ui (shared with the Windows
# test; it says what it does: the Material tool's tile "Oletus (Teräs)",
# its picker listing the built-in materials in Finnish and finding them by
# their English names too, Alumiini given to the box, the Materials window
# listing Teräs) on a design mitcad-cli makes from
# tools/ui-scripts/material-names-fi.json. Then the saved design: the body's
# material is aluminum, and no Finnish name was stored.
# Usage: tools/ui-material-names-fi-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
ROOT=$(cd "$here/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-material-names-fi.XXXXXX")
FILE=$WORK/box.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
"$CLI" run "$here/ui-scripts/material-names-fi.json" --save "$FILE" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  ui_fail "mitcad-cli could not make the design"
}
# The user's library in the test's own folder: no materials of the user's.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
export MITCAD_TEST_WORK=$WORK
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/material-names-fi.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --language=fi --open "$FILE"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && ui_fail "the material names driver timed out"
wait "$UI_RUNNER" || ui_fail "the material names driver failed: $(grep -E 'TestDriver' "$UI_LOG" | tail -8)"
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "the built-in materials listed and chosen in Finnish"

# The saved design: the body's material by its id, the names stored as the
# model gives them (English), none of the Finnish ones the interface showed.
actual=$(jq -c '.bodies[0].material' "$FILE") || ui_fail "cannot read $FILE"
[ "$actual" = '"aluminum"' ] || ui_fail "the body's material is $actual, expected \"aluminum\""
echo "ok   the box is aluminum in the saved design"
if grep -qE 'Alumiini|Teräs|Oletus' "$FILE"; then
  ui_fail "a Finnish material name was stored: $(grep -oE 'Alumiini|Teräs|Oletus' "$FILE" | sort -u | tr '\n' ' ')"
fi
echo "ok   no Finnish material name stored in the design"

ui_finish 'UI material names in Finnish test'
