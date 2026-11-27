#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/files/MainWindowFiles app/files/MainWindowAutosave app/files/Recovery app/framework/CommandSearch app/framework/Ribbon app/framework/WindowStyle app/framework/TestDriverChrome tools/ui-scripts/naming.mitcad-ui
# Current control labels keep their stable model and command identifiers.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-naming}"
for chrome in floating docked; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$chrome
  export MITCAD_TEST_SHOTS=$UI_OUT/$chrome
  mkdir -p "$MITCAD_SETTINGS_DIR" "$MITCAD_TEST_SHOTS"
  MITCAD_TEST_INPUT=$here/ui-scripts/naming.mitcad-ui \
    ui_run_app "$chrome" --ui=qt --chrome="$chrome" --no-recovery --no-mcp --demo
  ui_expect_log 'TestDriver: done' "$chrome: renamed controls retain stable behavior"
  ui_expect_log '^Policy dialog Root: part;' "$chrome: Component preset keeps its stored id"
  ui_expect_no_log 'TestDriver: FAIL line ' "$chrome: naming assertions passed"
  [ "$UI_FAILURES" = 0 ] || exit 1
done
echo "PASS: ribbon naming/accessibility, Design Settings, Recover Designs and Component policy; evidence in $UI_OUT"
