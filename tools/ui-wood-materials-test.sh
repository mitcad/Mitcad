#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialProperties.cpp app/print/MaterialsDialog.cpp app/print/PrintLibrary.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/material.rs core/model/src/material_dataset.rs core/model/data/wood_materials.json core/model/src/document/materials.rs
# check-all sources: core/model-base/src/material_class.rs core/model-base/src/appearance.rs core/model-api/src/materials.rs
# The bundled dataset's woods and panels in the class Wood (mitcad#1552),
# on a private Xvfb display: the test driver's script
# tools/ui-scripts/wood-materials.mitcad-ui (shared with the Windows test;
# it says what it does: the Materials window's New menu with Wood, Browse
# Material Dataset… with its two sources, "oak" found in the class Wood
# with its values and source, and Wood - Oak created into the design in
# the Materials window's Wood group with its look) on a design mitcad-cli
# makes from tools/ui-scripts/wood-materials.json. The pictures of the
# dataset and the Materials window stay in build/ui-wood-materials-pictures
# (UI_PICTURES).
# Usage: tools/ui-wood-materials-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
ROOT=$(cd "$here/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-wood-materials.XXXXXX")
PICTURES=${UI_PICTURES:-$ROOT/build/ui-wood-materials-pictures}
mkdir -p "$PICTURES"
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
"$CLI" run "$here/ui-scripts/wood-materials.json" --save "$WORK/wood.mitcad" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  exit 1
}
# The user's library in the test's own folder: no materials of the user's.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
export MITCAD_TEST_WORK=$PICTURES
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/wood-materials.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$WORK/wood.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail "wood materials driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'TestDriver: done' "oak found in the class Wood and created as a Wood material of the design"
echo "PASS: wood materials (pictures in $PICTURES)"
