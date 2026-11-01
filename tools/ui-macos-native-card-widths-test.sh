#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeCardWidths.mm app/platform/macos/NativeCardWidths.hpp
# check-all sources: app/platform/macos/NativeWindowFit.mm app/platform/macos/NativeWindowFit.hpp
# check-all sources: app/platform/macos/NativeMacWindow.mm app/platform/macos/NativeDialogs.mm app/platform/macos/NativeTestInput.mm
# check-all sources: app/MainWindowNative.cpp app/browser/AppearanceDialog.cpp app/framework/FitToScreen.cpp
# check-all sources: app/framework/TestDriverWindowFit.cpp app/projects tools/cli
# Native side card widths and secondary windows within the visible screen
# (mitcad#818), with a project and a body of long synthetic names: pointer
# drags of the Projects, Browser and inspector edges, their minimums, the
# free canvas between the sides, a compact window, the widths after a
# relaunch in the native and the Qt frontend, and Appearances within a lower
# visible screen frame with its buttons visible, in light and dark.
# Usage: tools/ui-macos-native-card-widths-test.sh <app> <mitcad-cli> <output>
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
if command -v launchctl > /dev/null; then
  session=$(launchctl managername 2> /dev/null || true)
  if [ "$session" = System ] && [ "${MITCAD_UI_REQUIRE_GUI:-0}" != 1 ]; then
    echo "SKIP: no window server in the System session"
    exit 77
  fi
fi

# NSWindow keeps the document window's frame in Cocoa defaults, outside the
# isolated settings, and native-resize changes it: restore it on exit.
card_domain=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$(dirname "$(dirname "$UI_APP")")/Info.plist")
card_frame_key='NSWindow Frame MitcadNativeDocumentWindow'
card_frame_present=0
card_frame=""
if card_frame=$(defaults read "$card_domain" "$card_frame_key" 2> /dev/null); then
  card_frame_present=1
fi
card_cleanup() {
  ui_cleanup
  if [ "$card_frame_present" = 1 ]; then
    defaults write "$card_domain" "$card_frame_key" -string "$card_frame"
  else
    defaults delete "$card_domain" "$card_frame_key" > /dev/null 2>&1 || true
  fi
}
trap 'card_test_exit=$?; trap - EXIT; set +e; card_cleanup; exit "$card_test_exit"' EXIT
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"

# Names longer than the default card widths show (synthetic data only).
"$cli" project create "$MITCAD_PROJECTS_DIR/Synthetic project with a long name" \
  --author 'UI Tester <ui@example.invalid>' --design base.mitcad > "$UI_OUT/setup.log" 2>&1
cat > "$MITCAD_TEST_WORK/long-names.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -15],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Synthetic body with a deliberately long name for width checks"}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/long-names.json" --save "$MITCAD_TEST_WORK/long-names.mitcad" \
  >> "$UI_OUT/setup.log" 2>&1
# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then, also with projects configured). On
# macOS the private settings are nocodo.fi/Mitcad.ini (the organization
# domain, as tools/ui-macos-test.sh notes).
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '\n[projects]\npanel=true\n' >> "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"

# run_phase <script suffix> <description> <arguments>...
run_phase() {
  local suffix=$1 description=$2
  shift 2
  MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-card-widths$suffix.mitcad-ui \
    ui_run_app "card-widths$suffix" "$@" || true
  ui_expect_log 'TestDriver: done' "$description"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
                 'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "card-widths$suffix: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
}
run_phase "" 'native card widths dragged, limited and remembered; Appearances within the screen' \
  --ui=native --no-recovery --open "$MITCAD_TEST_WORK/long-names.mitcad"
run_phase -restored 'native card widths restored after a relaunch' \
  --ui=native --no-recovery --open "$MITCAD_TEST_WORK/long-names.mitcad"
run_phase -qt 'the Floating Qt Projects card takes the shared width' --ui=qt --no-recovery --no-mcp
for shot in card-widths-default card-widths-wide card-widths-compact card-widths-dark appearances \
            appearances-compact appearances-compact-dark; do
  if [ ! -s "$MITCAD_TEST_WORK/$shot.png" ]; then
    ui_fail "missing capture $shot.png"
  fi
done
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: native card widths and windows within the screen; logs and captures: $UI_OUT"
  exit 1
fi
echo "PASS: native card widths and windows within the screen; logs and captures: $UI_OUT"
