#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# AppKit workflow test, driven in process without Accessibility permission.
# Usage: tools/ui-macos-native-test.sh <app> [out directory] [workflow|cancel|secondary|sketch|panels|toolbar|gestures|history|context-menus|replacement|autoselect|point-pattern|navigation-bar]
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"

if [ $# -lt 1 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> [out directory] [workflow|cancel|secondary|sketch|panels|toolbar|gestures|history|context-menus|replacement|autoselect|point-pattern|navigation-bar]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-native}"
scenario=${3:-workflow}
# A scenario that models in the root starts from an empty design without a
# stored policy (written below), not from a new design, whose active Component1
# would take the features (mitcad#220, mitcad#421).
plain_design=0
case "$scenario" in
  workflow) script=native-macos.mitcad-ui ;;
  familiar) script=native-macos-familiar.mitcad-ui ;;
  cancel)
    script=native-macos-cancel.mitcad-ui
    export MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=1500,add_feature=1500,edit_feature=1500
    ;;
  secondary)
    script=native-macos-secondary.mitcad-ui
    # A failed smoke may already have created its Local project. Remove
    # only this scenario's fixture so the Create action is rerunnable.
    rm -rf "$UI_OUT/files/NativeProject" "$UI_OUT/files/NativeCloudProject" \
      "$UI_OUT/files/CancelledProject" "$UI_OUT/files/native-cloud.git"
    git init -q --bare "$UI_OUT/files/native-cloud.git" || exit 1
    ;;
  sketch)
    script=native-macos-sketch.mitcad-ui
    export MITCAD_TEST_COLOR_SCHEME=${MITCAD_TEST_COLOR_SCHEME:-light}
    case "$MITCAD_TEST_COLOR_SCHEME" in
      light|dark) ;;
      *) echo "native sketch color scheme must be light or dark" >&2; exit 2 ;;
    esac
    ;;
  autoselect) script=native-macos-autoselect.mitcad-ui ;;
  point-pattern) script=native-macos-point-pattern.mitcad-ui ;;
  panels) script=native-macos-panels.mitcad-ui ;;
  toolbar) script=native-macos-toolbar.mitcad-ui ;;
  gestures) script=native-macos-gestures.mitcad-ui ;;
  context-menus)
    script=native-macos-context-menus.mitcad-ui
    export MITCAD_LOG_ORIENTATION_CUBE=1
    ;;
  history)
    script=native-macos-history.mitcad-ui
    export MITCAD_TEST_RECOMPUTE_DELAY_MS=0,set_marker=800
    # The root's boxes, its timeline search and the hybrid-assembly question.
    plain_design=1
    ;;
  replacement) script=native-macos-replacement.mitcad-ui ;;
  # The navigation bar at the bottom of the view (mitcad#1099).
  navigation-bar) script=native-macos-navigation-bar.mitcad-ui ;;
  *) echo "unknown native scenario: $scenario" >&2; exit 2 ;;
esac
UI_TIMEOUT=${UI_NATIVE_TIMEOUT:-240}
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"

if command -v launchctl > /dev/null; then
  session=$(launchctl managername 2> /dev/null || true)
  if [ "$session" = System ] && [ "${MITCAD_UI_REQUIRE_GUI:-0}" != 1 ]; then
    echo "SKIP: no window server in the System session"
    exit 77
  fi
fi

{
  # Cocoa owns toolbar customizations independently of isolated QSettings.
  # Preserve this test's configuration, including its original absence.
  UI_TOOLBAR_DOMAIN=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$(dirname "$(dirname "$UI_APP")")/Info.plist") || exit 1
  UI_TOOLBAR_CONFIG_KEY='NSToolbar Configuration com.mitcad.toolbar.familiar.v1'
  UI_TOOLBAR_CONFIG_PRESENT=0
  UI_TOOLBAR_CONFIG=""
  if UI_TOOLBAR_CONFIG=$(defaults read "$UI_TOOLBAR_DOMAIN" "$UI_TOOLBAR_CONFIG_KEY" 2> /dev/null); then
    UI_TOOLBAR_CONFIG_PRESENT=1
  fi
  ui_toolbar_cleanup() {
    ui_cleanup
    if [ "$UI_TOOLBAR_CONFIG_PRESENT" = 1 ]; then
      defaults write "$UI_TOOLBAR_DOMAIN" "$UI_TOOLBAR_CONFIG_KEY" "$UI_TOOLBAR_CONFIG"
    else
      defaults delete "$UI_TOOLBAR_DOMAIN" "$UI_TOOLBAR_CONFIG_KEY" > /dev/null 2>&1 || true
    fi
  }
  trap ui_toolbar_cleanup EXIT
  defaults delete "$UI_TOOLBAR_DOMAIN" "$UI_TOOLBAR_CONFIG_KEY" > /dev/null 2>&1 || true
}

if [ "$scenario" = toolbar ]; then
  rm -rf "$MITCAD_TEST_WORK/ToolbarProject" "$MITCAD_TEST_WORK/ToolbarOtherProject"
  MITCAD_TEST_INPUT=$here/ui-scripts/$script \
    ui_run_app toolbar --ui=native --no-recovery
  ui_expect_log 'TestDriver: done' 'familiar toolbar defaults and customized status stayed removed'
  MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-toolbar-restored.mitcad-ui \
    ui_run_app toolbar-restored --ui=native --no-recovery
  ui_expect_log 'TestDriver: done' 'customized toolbar survived application relaunch'
elif [ "$scenario" = panels ]; then
  # NSWindow's frame uses Cocoa defaults, independently of the isolated
  # QSettings directory. Restore this one preference when the test exits.
  UI_PANEL_DOMAIN=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$(dirname "$(dirname "$UI_APP")")/Info.plist") || exit 1
  UI_PANEL_FRAME_KEY='NSWindow Frame MitcadNativeDocumentWindow'
  UI_PANEL_FRAME_PRESENT=0
  UI_PANEL_FRAME=""
  if UI_PANEL_FRAME=$(defaults read "$UI_PANEL_DOMAIN" "$UI_PANEL_FRAME_KEY" 2> /dev/null); then
    UI_PANEL_FRAME_PRESENT=1
  fi
  ui_panels_cleanup() {
    ui_toolbar_cleanup
    if [ "$UI_PANEL_FRAME_PRESENT" = 1 ]; then
      defaults write "$UI_PANEL_DOMAIN" "$UI_PANEL_FRAME_KEY" -string "$UI_PANEL_FRAME"
    else
      defaults delete "$UI_PANEL_DOMAIN" "$UI_PANEL_FRAME_KEY" > /dev/null 2>&1 || true
    fi
  }
  trap ui_panels_cleanup EXIT
  for mode in native no-native; do
    panel_args=(--ui=native --no-recovery)
    if [ "$mode" = no-native ]; then panel_args+=(--no-native-dialogs); fi
    MITCAD_TEST_INPUT=$here/ui-scripts/$script \
      ui_run_app "panels-$mode" "${panel_args[@]}"
    ui_expect_log 'TestDriver: done' "native panels completed ($mode)"
    MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-frame-restored.mitcad-ui \
      ui_run_app "frame-restored-$mode" "${panel_args[@]}"
    ui_expect_log 'TestDriver: done' "native frame restored after relaunch ($mode)"
  done
elif [ "$scenario" = replacement ]; then
  # shellcheck source=tools/ui-macos-replacement-fixture.sh
  . "$here/ui-macos-replacement-fixture.sh"
  if ! ui_replacement_prepare; then
    cat "$UI_OUT/replacement-setup.log" 2> /dev/null || true
    exit 1
  fi
  # The coordinator must not inherit the parent test's cleanup trap.
  (trap - EXIT INT TERM; ui_replacement_coordinate) > "$UI_OUT/replacement-fixture.log" 2>&1 &
  UI_REPLACEMENT_PID=$!
  trap 'ui_replacement_cleanup; ui_toolbar_cleanup' EXIT
  MITCAD_TEST_INPUT=$here/ui-scripts/$script \
    ui_run_app native --ui=native --no-recovery --open "$MITCAD_TEST_REPLACEMENT/ada/part.mitcad" --set d3=27
  touch "$MITCAD_TEST_REPLACEMENT/app-finished"
  if ! wait "$UI_REPLACEMENT_PID"; then
    cat "$UI_OUT/replacement-fixture.log"
    ui_fail 'replacement fixture or retained recovery state verification failed'
  fi
  UI_REPLACEMENT_PID=""
elif [ "$plain_design" = 1 ]; then
  printf '{"format": "mitcad", "version": 2, "parameters": [], "features": []}\n' \
    > "$MITCAD_TEST_WORK/root-design.mitcad"
  MITCAD_TEST_INPUT=$here/ui-scripts/$script \
    ui_run_app native --ui=native --no-recovery --open "$MITCAD_TEST_WORK/root-design.mitcad"
else
  MITCAD_TEST_INPUT=$here/ui-scripts/$script \
    ui_run_app native --ui=native --no-recovery
fi
ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' 'native AppKit shell was used'
ui_expect_log 'TestDriver: done' 'native workflow reached its end'
if [ "$scenario" = workflow ]; then
  ui_expect_log 'New body .*volume 20000\.000 mm3' 'native form created the expected box'
  ui_expect_log 'volume 20000\.000 -> 10000\.000 mm3' 'native history edit changed the model'
  if [ ! -s "$MITCAD_TEST_WORK/native-macos.png" ]; then
    ui_fail 'native composed screenshot was not saved'
  fi
elif [ "$scenario" = cancel ]; then
  ui_expect_log 'volume 12000\.000 -> 13000\.000 mm3' 'latest async definition was committed'
elif [ "$scenario" = sketch ]; then
  ui_expect_log 'volume 20250\.000 mm3' 'native precision fields created the expected extrusion'
elif [ "$scenario" = secondary ]; then
  if [ -e "$MITCAD_TEST_WORK/CancelledProject" ]; then
    ui_fail 'cancelling New Project created files or settings'
  fi
  if [ -n "$(git -C "$MITCAD_TEST_WORK/NativeCloudProject" ls-files .mitcad/local)" ]; then
    ui_fail 'per-computer creation settings were added to version history'
  fi
  ui_expect_log 'Added parameter nativeWidth = 30 mm' 'native parameter sheet changed the document'
  ui_expect_log 'Appearances dialog opened' 'native constrained appearances list was opened'
  ui_expect_no_log 'Unable to simultaneously satisfy constraints' 'native secondary windows have no contradictory layout constraints'
  if grep -Eq '^Report sent:|^Report on the clipboard:' "$UI_LOG_FILE"; then
    ui_fail 'report preview did not stay local'
  fi
elif [ "$scenario" = replacement ]; then
  ui_expect_log 'Imported exchange.step as' 'a successful native Open imported into a separate document'
  ui_expect_log 'Autosaved exchange:' 'the untitled imported document has its own recovery data'
fi
if grep -Eq '^TestDriver: FAIL|^Native Mac canvas:|^OCCT view event failed' "$UI_LOG_FILE"; then
  ui_fail 'native input or rendering failed'
fi
echo "Logs and files: $UI_OUT"
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: native macOS UI workflow ($UI_FAILURES failures)"
  exit 1
fi
echo 'PASS: native macOS UI workflow'
