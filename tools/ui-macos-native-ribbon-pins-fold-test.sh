#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/RibbonFit.cpp app/platform/macos/NativeMacWindow.mm
# check-all sources: app/platform/NativeMacWindow.hpp app/MainWindowNative.cpp app/platform/macos/NativeTestInput.mm
# check-all sources: tools/ui-scripts/native-macos-ribbon-pins-fold.mitcad-ui
# The AppKit ribbon's pinned buttons fold first in a narrow window
# (mitcad#1357), as the Qt ribbon's do (mitcad#1111,
# tools/ui-sketch-create-menu-test.sh): Sweep pinned into Create, the
# window narrowed step by step with the in-process native resize (no
# system input; tools/ui-scripts/native-macos-ribbon-pins-fold.mitcad-ui).
# The log says the order: when Sweep's button folds ("Ribbon pinned folded
# SOLID/CREATE: solid.sweep", the Qt ribbon's line) the groups on its
# right, Modify among them, have collapsed ("Native ribbon collapsed
# SOLID: ..."), Create has not, so Extrude and Create's other own buttons
# are still in its row; wide again, Sweep's button is back.
# Usage: tools/ui-macos-native-ribbon-pins-fold-test.sh <app> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 2 ]; then
  echo "usage: $0 <app> <output directory>" >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# NSWindow's frame uses Cocoa defaults, independently of the isolated
# QSettings directory: the resized window's frame is not left to the next
# test.
frame_domain=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$(dirname "$(dirname "$UI_APP")")/Info.plist") || exit 1
frame_key='NSWindow Frame MitcadNativeDocumentWindow'
frame_present=0
frame=""
if frame=$(defaults read "$frame_domain" "$frame_key" 2> /dev/null); then frame_present=1; fi
ui_frame_cleanup() {
  ui_cleanup
  if [ "$frame_present" = 1 ]; then
    defaults write "$frame_domain" "$frame_key" -string "$frame"
  else
    defaults delete "$frame_domain" "$frame_key" > /dev/null 2>&1 || true
  fi
}
trap ui_frame_cleanup EXIT

MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-ribbon-pins-fold.mitcad-ui \
  ui_run_app native-ribbon-pins-fold --ui=native --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'Sweep pinned into Create, the window narrowed and wide again'

# The first fold of Sweep's button and the groups collapsed at that moment
# (logged before the folds of the same layout).
fold=$(grep -n -E '^Ribbon pinned folded SOLID/CREATE: solid\.sweep$' "$UI_LOG_FILE" | head -n 1 | cut -d: -f1)
if [ -z "$fold" ]; then
  ui_fail "Sweep's pinned button never folded in the narrowed window"
else
  state=$(head -n "$fold" "$UI_LOG_FILE" | grep -E '^Native ribbon collapsed SOLID: ' | tail -n 1)
  collapsed=", ${state#Native ribbon collapsed SOLID: }, "
  case "$collapsed" in
    *", CREATE, "*) ui_fail "Create collapsed before (or with) Sweep's button folded: $state" ;;
    *", MODIFY, "*) ui_pass "Sweep's button folded while Create kept Extrude and its own buttons ($state)" ;;
    *) ui_fail "Sweep's button folded before the groups on its right collapsed: ${state:-no collapsed groups logged}" ;;
  esac
fi
ui_expect_log '^Ribbon pinned folded SOLID/CREATE: none$' "wide again, Sweep's button is back"
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
  ui_expect_no_log "$problem" "no $problem"
done
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo "PASS: the AppKit ribbon's pinned buttons fold before their group"
