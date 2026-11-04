#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialsDialog.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/library_items.rs core/model/src/print_files/mod.rs
# check-all sources: core/model-api/src/materials.rs core/model/src/print_library.rs
# A My library material whose file lacks its own color (mitcad#1242), on a
# private Xvfb display: the test driver's script
# tools/ui-scripts/library-material-own-colour.mitcad-ui (shared with the
# Windows test; it says what it does: the Materials window renames the
# item, the new name listed) on My library made from
# tools/ui-scripts/library-material-own-colour/, no design open. Then the
# item's file: the new name and its own color made again and carried.
# Usage: tools/ui-library-material-own-colour-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
WORK=$(mktemp -d /tmp/mitcad-ui-library-material-own-colour.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# My library in the test's own folder: the item without its own color.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
mkdir -p "$MITCAD_PRINT_LIBRARY_DIR"
cp -R "$here/ui-scripts/library-material-own-colour/." "$MITCAD_PRINT_LIBRARY_DIR/"
ITEM=$MITCAD_PRINT_LIBRARY_DIR/materials/generic-tpu-x.json
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/library-material-own-colour.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'library material own color driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' 'the Materials window renamed the item and listed the new name'

# expect_item filter expected description: a jq filter on the item's file.
expect_item() {
  local actual
  actual=$(jq -c "$1" "$ITEM") || ui_fail "$3: cannot read $ITEM"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}
expect_item '.material.name' '"Shop TPU"' "the file has the new name"
expect_item '[.material.appearance, .appearance.id, .appearance.name]' \
  '["generic_tpu_x_colour","generic_tpu_x_colour","Shop TPU color"]' "the file carries its own color made again"
echo 'PASS: library material own color'
