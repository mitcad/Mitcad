#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/AppearancePicker app/print/SettingField app/print/MaterialsDialog app/platform/NativeDialogs app/platform/macos/NativeDialogs app/platform/macos/NativeColorField app/platform/NativePicker app/platform/macos/NativePicker app/framework/TestDriver app/platform/NativeTestInput
# The Materials window's colours (mitcad#1055: a filament's colour is its
# appearance): a colour a command gave a material became its appearance, a
# chosen appearance stays, none leaves it none, as the list of the
# material's appearances shows them (the default marked in its row,
# mitcad#1083); Add…'s popover of the appearance library opened and closed
# without a change; light and dark pictures of the window and the popover.
# The AppKit frontend shows the list as a native table and the popover as
# the tools' picker popover under Add… (mitcad#1068); there Add… also
# chooses Paint - Red in it, then Make Default and Remove.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 4 ] || { [ "$3" != qt ] && [ "$3" != native ]; }; then
  echo "usage: $0 <app> <cli> qt|native <new-output>" >&2
  exit 2
fi
cli=$2
frontend=$3
ui_init "$1" "$4"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR"
"$cli" run "$here/ui-scripts/material-color-parity.json" --save "$MITCAD_TEST_WORK/colors.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
python3 "$here/ui-scripts/material-color-parity-script.py" "$frontend" > "$UI_OUT/colors.mitcad-ui" || exit 1
for scheme in light dark; do
  export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$scheme
  mkdir -p "$MITCAD_SETTINGS_DIR"
  # Qt's supported test option keeps its QColorDialog controllable through
  # the actual Qt buttons. Native uses the production AppKit colour panel.
  if [ "$frontend" = qt ]; then
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$UI_OUT/colors.mitcad-ui \
      ui_run_app "$scheme" --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/colors.mitcad"
  else
    export MITCAD_TEST_PICKER_ANIMATIONS=1
    # Separate pointer steps need an AppKit event-loop turn, while the
    # original popover's close animation is still in progress.
    MITCAD_TEST_STEP_MS=50 MITCAD_TEST_SETTLE_MS=0 \
      MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$UI_OUT/colors.mitcad-ui \
      ui_run_app "$scheme" --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/colors.mitcad"
  fi
  ui_expect_log 'TestDriver: done' "$frontend/$scheme: explicit, inherited and unset appearances shown, the picker closed"
  if [ "$frontend" = native ]; then
    ui_expect_log '^Native picker animations: on$' 'the actual NSPopover lifecycle ran with animations enabled'
  fi
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:' 'OCCT view event failed:'; do
    ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
  done
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
done
echo "PASS: $frontend Materials appearances and the appearance picker in light/dark; $UI_OUT"
