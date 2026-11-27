#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MaterialCommands app/commands/MaterialBinding app/browser/LibraryPicker app/browser/ConfigurationBindingsPage app/platform/NativePicker app/platform/macos/NativePicker app/platform/macos/NativeForms app/platform/macos/NativeDialogs app/platform/NativeTestInput app/framework/TestDriver
# The Material and Appearance tools (mitcad#1059, mitcad#1066): their fx
# with Varies With and a row per value, the pickers (the AppKit frontend's
# popover under the tile), the fx off, three bodies, a face bound to an
# option added in Configurations, and Configurations' fourth tab, through
# real pointer, menu and table input, writes and undo, in both themes.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory> qt|native" >&2
  exit 2
fi
cli=$2
frontend=$4
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
ui_init "$1" "$3"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_ROOT/libraries
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_LIBRARIES_DIR"
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  model=$MITCAD_TEST_WORK/bindings.mitcad
  "$cli" run "$here/ui-scripts/bindings-parity.json" --save "$model" > "$UI_OUT/setup-$scheme.log" 2>&1 || {
    cat "$UI_OUT/setup-$scheme.log" >&2
    exit 1
  }
  script=$UI_OUT/bindings-$frontend-$scheme.mitcad-ui
  python3 "$here/ui-scripts/bindings-parity-script.py" "$frontend" > "$script" || exit 1
  args=(--ui="$frontend" --no-recovery --no-mcp --open "$model")
  [ "$frontend" != qt ] || args+=(--no-native-dialogs)
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=90 MITCAD_TEST_INPUT=$script \
    ui_run_app "$frontend-$scheme" "${args[@]}"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme: the tools' fx and pickers and the Bindings workflows completed"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:' 'OCCT view event failed:'; do
    ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
  done
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
  python3 "$here/ui-scripts/bindings-parity-verify.py" theme "$model" "$MITCAD_TEST_WORK" "$frontend" "$scheme"
  [ "$?" -eq 0 ] || exit 1
  cp "$model" "$MITCAD_TEST_WORK/bindings-$frontend-$scheme.mitcad"
done

# The fx without options, an option added in Configurations and a picked
# face use a separate no-options model. Run these once per frontend; the
# tools and the primary binding workflow above are already captured in
# both appearances.
rm -rf "$MITCAD_SETTINGS_DIR"
mkdir -p "$MITCAD_SETTINGS_DIR"
model=$MITCAD_TEST_WORK/bindings-open.mitcad
"$cli" run "$here/ui-scripts/bindings-parity-open-values.json" --save "$model" > "$UI_OUT/setup-open-values.log" 2>&1 || {
  cat "$UI_OUT/setup-open-values.log" >&2
  exit 1
}
script=$UI_OUT/bindings-open-$frontend-light.mitcad-ui
python3 "$here/ui-scripts/bindings-parity-open-values-script.py" "$frontend" > "$script" || exit 1
args=(--ui="$frontend" --no-recovery --no-mcp --open "$model")
[ "$frontend" != qt ] || args+=(--no-native-dialogs)
MITCAD_TEST_COLOR_SCHEME=light MITCAD_TEST_STEP_MS=90 MITCAD_TEST_INPUT=$script \
  ui_run_app "$frontend-open-values-light" "${args[@]}"
ui_expect_log '^TestDriver: done' "$frontend: no options, Add Option, open values and face workflows completed"
for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:' 'OCCT view event failed:'; do
  ui_expect_no_log "$problem" "$frontend/open-values: no $problem"
done
[ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
python3 "$here/ui-scripts/bindings-parity-verify.py" open-values "$model" "$UI_LOG_FILE" "$MITCAD_TEST_WORK" "$frontend"
[ "$?" -eq 0 ] || exit 1
cp "$model" "$MITCAD_TEST_WORK/bindings-open-$frontend-light.mitcad"
echo "PASS: the Material and Appearance tools and Bindings tab ($frontend, light/dark plus supplementary face/open-values workflow)"
