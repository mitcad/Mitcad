#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeDialogs.mm app/platform/macos/NativeTableCells app/platform/macos/NativeTestInput.mm app/platform/NativeDialogs.hpp app/platform/MacParameters.hpp app/browser/ParametersDialog app/files/LibraryDialogs app/framework/Theme app/framework/TestDriver
# Native AppKit tables (mitcad#814), in light and dark appearance: the demo
# block's Parameters refuse an invalid unit committed with Return and with
# Tab while the dialog stays open with the refused cell and its reason in
# red; Escape discards, F2 edits, a correction succeeds; section rows,
# read-only cells and the shared column sizes at normal and narrow sizes;
# a synthetic library's long name and path on single elided lines.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
# The log says what ends a native cell edit and who takes the focus.
export MITCAD_LOG_NATIVE_EDITS=1
mkdir -p "$MITCAD_TEST_WORK"

# A synthetic fastener library with a long name, in folders with long
# names: the Libraries table's Source and Library cells. Nothing remote.
export MITCAD_STYLE_LIBRARY=$UI_OUT/a-deliberately-long-synthetic-folder-for-native-table-elision/another-nested-folder-with-a-long-name/synthetic-library
export MITCAD_LIBRARIES_DIR=$UI_OUT/library-cache
if [ ! -f "$MITCAD_STYLE_LIBRARY/mitcad-library.json" ]; then
  rm -rf "$MITCAD_STYLE_LIBRARY"
  python3 "$here/libraries/make-fastener-library.py" --cli "$cli" --out "$MITCAD_STYLE_LIBRARY" \
    --sizes M5 --standards iso4762,iso4032 --version 1.0.0 --no-render > "$UI_OUT/setup.log" 2>&1 || {
    cat "$UI_OUT/setup.log" >&2
    exit 1
  }
  sed -i '' 's/"name": "Mitcad parts",/"name": "Synthetic fasteners with a deliberately long library name for elision checks",/' \
    "$MITCAD_STYLE_LIBRARY/mitcad-library.json"
  if ! grep -q 'deliberately long library name' "$MITCAD_STYLE_LIBRARY/mitcad-library.json"; then
    echo "FAIL: the synthetic library's name was not replaced" >&2
    exit 1
  fi
  identity=(-c user.name='Mitcad developers' -c user.email=devs@mitcad.org)
  { git init -q -b main "$MITCAD_STYLE_LIBRARY" &&
    git -C "$MITCAD_STYLE_LIBRARY" add -A &&
    git -C "$MITCAD_STYLE_LIBRARY" "${identity[@]}" commit -q -m 'Synthetic fasteners 1.0.0' &&
    git -C "$MITCAD_STYLE_LIBRARY" "${identity[@]}" tag -a v1.0.0 -m 'Synthetic fasteners 1.0.0'; } \
    >> "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
fi
"$cli" library fetch "$MITCAD_STYLE_LIBRARY" --libraries "$MITCAD_LIBRARIES_DIR" --json > "$UI_OUT/fetch.log" 2>&1 || {
  cat "$UI_OUT/fetch.log" >&2
  exit 1
}

for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR" && mkdir -p "$MITCAD_SETTINGS_DIR"
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TABLES_SCHEME=$scheme \
    MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-tables.mitcad-ui \
    ui_run_app "tables-$scheme" --ui=native --demo --no-recovery --no-mcp
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: the native AppKit shell was used"
  ui_expect_log 'TestDriver: done' "$scheme: native table workflow reached its end"
  ui_expect_log 'Parameter d3 expression: 30 deg refused:' "$scheme: Return committed the refused unit"
  ui_expect_log 'Parameter d3 expression: 45 deg refused:' "$scheme: Tab committed the refused unit"
  ui_expect_log 'Parameter d3 expression: 25 mm = ' "$scheme: the correction was accepted"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
                 'Parameter d3 expression: 99 mm'; do
    ui_expect_no_log "$problem" "$scheme: no '$problem'"
  done
  for shot in parameters-normal parameters-narrow parameters-return parameters-tab libraries-normal libraries-narrow; do
    [ -s "$MITCAD_TEST_WORK/$shot-$scheme.png" ] || ui_fail "$scheme: missing $shot capture"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then
    ui_log_tail
    exit 1
  fi
done
echo "Logs and captures: $UI_OUT"
echo "PASS: native table semantics, cell edit keys and column sizing in both appearances"
