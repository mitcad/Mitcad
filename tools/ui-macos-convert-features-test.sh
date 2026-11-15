#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConvertToFeatures.cpp app/browser/BrowserController.cpp app/MainWindowContextMenu.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli core/model/src/document/convert_derived.rs core/model-api/src/convert_derived.rs
# Convert to Features (mitcad#1815) on macOS, through the test driver
# (tools/ui-scripts/convert-features.mitcad-ui, the same script as the
# Linux and Windows tests'): a library of one synthetic timber member made
# here (convert-features-member.json and convert-features-library.json,
# fetched from a local git repository), the member placed from it and
# joined to a grounded plate (convert-features.json); Convert to Features
# from the member's row menu, its sketch and extrusion in the timeline, the
# section's width changed, the joint holding.
# Usage: tools/ui-macos-convert-features-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-convert-features}"
UI_TIMEOUT=300
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
export MITCAD_LIBRARIES_DIR=$files/cache
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
lib=$files/test-timber
"$cli" library init "$lib" --id test-timber --name "Test timber" --license MIT > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
mkdir -p "$lib/dimensioned-timber"
"$cli" run "$here/ui-scripts/convert-features-member.json" \
  --save "$lib/dimensioned-timber/timber-convert-test.mitcad" >> "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
cp "$here/ui-scripts/convert-features-library.json" "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test part"
g "$lib" tag v1.0.0
"$cli" library fetch "$lib" >> "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
sed "s|LIBRARY_URL|$lib|" "$here/ui-scripts/convert-features.json" > "$files/convert-features.json"
"$cli" run "$files/convert-features.json" --save "$files/convert-features.mitcad" >> "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/convert-features.mitcad-ui \
  ui_run_app convert-features --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$files/convert-features.mitcad"
ui_expect_log 'TestDriver: done' 'the member converted from its row menu, its section edited, the joint holding'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
