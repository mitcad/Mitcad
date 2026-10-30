#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos app/platform/NativeDialogs.hpp app/platform/NativeTestInput.hpp app/library/PartPicker
# check-all sources: app/files/LibraryDialogs app/framework/TestDriver app/framework/TestDriverDialogs.cpp tools/libraries/make-fastener-library.py app/library/SupplierPartsDialog
# Native part catalogue (mitcad#813): Insert from Library and Community
# Library as pictured tiles with the groups as a source list, search,
# favourites, tile size, the hover card, insertion by pointer and keyboard,
# the screw's suppliers' articles in their table (mitcad#979), and
# usable bounds at normal and narrow sizes, in light and dark
# appearance. A local synthetic library of two pictured parts (a socket
# screw and a nut, drawn previews); no network requests.
# Usage: tools/ui-macos-native-library-test.sh <app> <mitcad-cli> <output>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=${UI_NATIVE_TIMEOUT:-240}
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_STYLE_LIBRARY=$UI_OUT/library
export MITCAD_LIBRARIES_DIR=$UI_OUT/library-cache
mkdir -p "$MITCAD_TEST_WORK"
if [ ! -f "$MITCAD_STYLE_LIBRARY/mitcad-library.json" ]; then
  if ! python3 "$here/libraries/make-fastener-library.py" --cli "$cli" --out "$MITCAD_STYLE_LIBRARY" \
      --sizes M5 --standards iso4762,iso4032 --version 1.0.0 --no-render --commit > "$UI_OUT/setup.log" 2>&1; then
    cat "$UI_OUT/setup.log"
    exit 1
  fi
fi
if ! "$cli" library fetch "$MITCAD_STYLE_LIBRARY" --libraries "$MITCAD_LIBRARIES_DIR" --json > "$UI_OUT/fetch.log" 2>&1; then
  cat "$UI_OUT/fetch.log"
  exit 1
fi
for scheme in light dark; do
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-library.mitcad-ui \
    ui_run_app "library-$scheme" --ui=native --no-recovery
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: the native AppKit shell was used"
  ui_expect_log 'TestDriver: done' "$scheme: the native library catalogue workflow completed"
  ui_expect_log "Part picker chose mitcad-parts/iso4032" "$scheme: Return on the grid inserted the nut"
  ui_expect_log "Part picker chose mitcad-parts/iso4762" "$scheme: a double click inserted the screw"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
      'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  for shot in insert insert-narrow community supplier-articles; do
    [ -s "$MITCAD_TEST_WORK/$shot-$scheme.png" ] || ui_fail "$scheme: missing $shot capture"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then
    ui_log_tail
    exit 1
  fi
done
echo "PASS: native library catalogue: pictured tiles, groups, search, favourites, tile size, insertion, bounds; $UI_OUT"
