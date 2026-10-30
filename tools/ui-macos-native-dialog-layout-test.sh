#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos app/platform/NativeTestInput.hpp app/platform/NativeDialogs.hpp app/framework/TestDriver app/framework/SettingsWindow app/browser/ParametersDialog app/files/ProjectDialogs app/files/ProjectSettings app/view/NavigationPreferences.hpp app/files/LibraryDialogs app/render/RenderImageDialog
# Native dialog sizing contract and macOS button order (mitcad#812, #817),
# in light and dark appearance: synthetic presenters, then Parameters, New
# Project, Settings > Navigation (with its section icons, mitcad#971),
# Insert from Library, Render Image and Library Parts at normal and narrow
# sizes, with real pointer and key input.
# Usage: tools/ui-macos-native-dialog-layout-test.sh <app> [out directory]
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-native-dialog-layout}"
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

for appearance in light dark; do
  MITCAD_TEST_APPEARANCE=$appearance MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-dialog-layout.mitcad-ui \
    ui_run_app "$appearance" --ui=native --no-recovery
  ui_expect_log 'TestDriver: done' "$appearance: native dialog layout workflow completed"
  ui_expect_log 'Native bounds: native .navigationTrackpadSpeed' "$appearance: slider bounds recorded"
  ui_expect_log 'Native button order: .*Close .*Insert' "$appearance: Insert from Library button bounds recorded"
  # A form can keep every field visible by opening much taller than its
  # content. Compare the initial New Project height with the shared form's
  # own size, allowing AppKit's different control metrics and window chrome.
  if python3 - "$UI_LOG_FILE" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
presenter = re.search(r"New Project dialog: layout .*\(dialog height (\d+),", text)
native = re.search(r"Native bounds: native 'projectFolder'.* in \d+x(\d+)", text)
if not presenter or not native:
    raise SystemExit("FAIL: missing New Project size measurements")
shared_height = int(presenter.group(1))
native_height = int(native.group(1))
if native_height > shared_height * 1.25:
    raise SystemExit(f"FAIL: New Project height {native_height} exceeds shared form height {shared_height} by over 25%")
print(f"PASS: New Project compact height {native_height}, shared form {shared_height}")
PY
  then
    :
  else
    ui_fail "$appearance: New Project opens at its content height"
  fi
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:' \
    'OCCT view event failed' 'New Project: created'; do
    ui_expect_no_log "$problem" "$appearance: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then
    ui_log_tail
    exit 1
  fi
done
echo "PASS: native dialog field and slider widths, labels, focus, button order and keys in light and dark; $UI_OUT"
