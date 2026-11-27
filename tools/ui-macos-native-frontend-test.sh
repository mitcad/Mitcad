#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/main.cpp app/framework/ChromeStyle app/view/ViewController.cpp
# The frontend at start-up in a build with the AppKit frontend (mitcad#1056):
# Qt by default; Settings › General's native macOS interface starts AppKit
# at the next start; --ui and --chrome override it for a run; the start-up's
# Frontend: line names the frontend and what chose it. Part of the release's
# native smoke subset (label app.ui-macos-native-smoke).
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-native-frontend}"
UI_TIMEOUT=180
cat > "$UI_OUT/block.mitcad" <<'DESIGN'
{
  "format": "mitcad", "version": 1,
  "parameters": [
    {"name": "d1", "value": 60}, {"name": "d2", "value": 40},
    {"name": "d3", "value": 20}
  ],
  "features": [
    {"type": "sketch", "name": "Sketch1", "shapes": [
      {"type": "rectangle", "corner": [0, 0], "width": "d1", "height": "d2"}]},
    {"type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body"}
  ]
}
DESIGN
scripts=$here/ui-scripts
# run name frontend-pattern script [arguments]: one start of MitCad with the
# design, which must end its script and name its frontend in the
# Frontend: line.
run() {
  local name=$1 frontend=$2 script=$3
  shift 3
  MITCAD_TEST_INPUT=$scripts/$script \
    ui_run_app "$name" --no-recovery --no-mcp "$@" --open "$UI_OUT/block.mitcad"
  ui_expect_log "^Frontend: $frontend\$" "$name: Frontend: $frontend"
  ui_expect_log 'TestDriver: done' "$name: script completed"
  ui_expect_no_log 'TestDriver: FAIL' "$name: assertions passed"
  # Anchored: the test driver echoes each script step into the log, and the
  # scripts' own checks name the AppKit shell's line.
  case $frontend in
    native*) ui_expect_log '^Native UI: AppKit shell with embedded OCCT viewport' "$name: the AppKit frontend started" ;;
    *)
      if grep -Eq '^Native UI: AppKit shell' "$UI_LOG_FILE"; then
        ui_fail "$name: the AppKit frontend started instead of Qt"
      else
        ui_pass "$name: the Qt frontend started"
      fi
      ;;
  esac
}
# No preference and no --ui: Qt. The script turns the preference on.
run default 'qt \(default\)' native-frontend-choose.mitcad-ui
# The next start: AppKit, by the preference, with the design open; it quits.
run preference 'native \(preference\)' native-frontend-native.mitcad-ui
# --chrome chooses a Qt layout, so Qt for this run.
run chrome 'qt \(--chrome\)' native-frontend-qt.mitcad-ui --chrome=docked
# --ui=qt overrides the preference; Settings shows it on, and the script
# turns it off again.
run ui-qt 'qt \(--ui\)' native-frontend-clear.mitcad-ui --ui=qt
# --ui=native without the preference.
run ui-native 'native \(--ui\)' native-frontend-native.mitcad-ui --ui=native
# Off again: Qt by default.
run cleared 'qt \(default\)' native-frontend-qt.mitcad-ui
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: Qt by default, AppKit by the preference, --ui and --chrome for a run; logs: $UI_OUT"
