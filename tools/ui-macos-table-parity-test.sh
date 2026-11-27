#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TablesPage app/platform/macos/NativeDialogs app/platform/macos/NativeTableCells app/platform/NativeDialogs.hpp
# The same synthetic parameter table in Qt and AppKit, in both themes:
# real column-name and unit edits, refusal, cell correction, dependent
# geometry and one-step undo. Run by the macOS orchestrator, serially.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/table-parity.json" --save "$MITCAD_TEST_WORK/table.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

for frontend in qt native; do
  for scheme in light dark; do
    model=$MITCAD_TEST_WORK/table-$frontend-$scheme.mitcad
    cp "$MITCAD_TEST_WORK/table.mitcad" "$model"
    rm -rf "$MITCAD_SETTINGS_DIR"
    mkdir -p "$MITCAD_SETTINGS_DIR"
    script=$UI_OUT/table-$frontend-$scheme.mitcad-ui
    cat > "$script" <<'DRIVER'
# SPDX-License-Identifier: MIT
wait-idle
expect-model-bodies 1 48000
command solid.parameters
focus-dialog '^Parameters$'
DRIVER
    if [ "$frontend" = native ]; then
      cat >> "$script" <<'DRIVER'
expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'
native-appearance '${MITCAD_TEST_COLOR_SCHEME}'
native-choice parametersTabs Tables
expect-native-bounds tablesColumnName_1 60
expect-native-bounds tablesColumnName_2 60
expect-native-bounds tablesColumnUnit_2 60
native-pointer-click tablesColumnName_2
DRIVER
    else
      cat >> "$script" <<'DRIVER'
dialog-click-logged 'Parameters tab Tables'
focus-widget tablesColumnName_2
DRIVER
    fi
    cat >> "$script" <<'DRIVER'
mark
key Ctrl+a
type Height
key Return
expect-log 'Table Sizes column 2 renamed: Value -> Height'
focus-dialog '^Parameters$'
wait-idle
expect-model-bodies 1 48000
DRIVER
    if [ "$frontend" = native ]; then
      cat >> "$script" <<'DRIVER'
expect-native-text tablesColumnUnit_2 '^mm$'
native-pointer-choice tablesColumnUnit_2 cm Down Return
expect-log '^Native pointer choice: tablesColumnUnit_2 .*menu tracked$'
expect-native-text tablesColumnUnit_2 '^cm$'
DRIVER
    else
      cat >> "$script" <<'DRIVER'
focus-widget tablesColumnUnit_2
key Space
key Down
key Return
DRIVER
    fi
    cat >> "$script" <<'DRIVER'
expect-log 'Table Sizes column Height unit: mm -> cm'
expect-log 'Table Sizes columns: Key \[Text\], Height \[cm\]'
wait-idle
expect-model-bodies 1 48000
capture-undo-depth beforeInvalid
mark
DRIVER
    if [ "$frontend" = native ]; then
      cat >> "$script" <<'DRIVER'
# Header/unit commits rebuild the shared table and select only its current
# cell. The actual native row highlight must survive that refresh, without
# expanding the presenter's selection to a whole row.
expect-native-style table.Sizes.0.0 row-selected,source-single-cell,in-view
DRIVER
      echo 'native-click table.Sizes.1.1' >> "$script"
    else
      echo "dialog-dblclick-logged 'Tables cell 2 2'" >> "$script"
    fi
    cat >> "$script" <<'DRIVER'
key Ctrl+a
type abc
key Return
expect-log 'Table Sizes row 2 Height: abc refused:'
wait-idle
focus-dialog '^Parameters$'
expect-model-bodies 1 48000
expect-undo-depth ${beforeInvalid} 0
DRIVER
    cat >> "$script" <<DRIVER
screenshot-dialog '$MITCAD_TEST_WORK/table-refused-$frontend-$scheme.png'
screenshot-screen '$MITCAD_TEST_WORK/table-refused-$frontend-$scheme-own-windows.png'
DRIVER
    if [ "$frontend" = native ]; then
      cat >> "$script" <<'DRIVER'
expect-native-style table.Sizes.1.1 error,not-editing,row-selected,source-single-cell,in-view
expect-native-text table.Sizes.1.1 '^abc$'
native-click table.Sizes.1.1
DRIVER
    else
      echo "dialog-dblclick-logged 'Tables cell 2 2'" >> "$script"
    fi
    cat >> "$script" <<'DRIVER'
mark
key Ctrl+a
type 2.5
key Return
expect-log 'Table Sizes row 2 Height: 2\.5'
wait-idle
focus-dialog '^Parameters$'
expect-model-bodies 1 60000
expect-undo-depth ${beforeInvalid} 1
DRIVER
    if [ "$frontend" = native ]; then
      cat >> "$script" <<'DRIVER'
expect-native-style table.Sizes.1.1 normal,not-editing,row-selected,source-single-cell,in-view
expect-native-text table.Sizes.1.1 '^2\.5$'
native-dialog-size 900 500
expect-native-bounds tablesColumnName_2 60
expect-native-bounds tablesColumnUnit_2 60
DRIVER
    fi
    cat >> "$script" <<DRIVER
screenshot-dialog '$MITCAD_TEST_WORK/table-$frontend-$scheme.png'
button '^OK$'
focus-main
command edit.undo
wait-idle
expect-model-bodies 1 48000
mark
menu 'File/Save'
expect-log 'Saved .*table-.*\.mitcad'
DRIVER
    MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$script \
      ui_run_app "table-$frontend-$scheme" --ui="$frontend" --no-recovery --no-mcp --open "$model"
    ui_expect_log 'TestDriver: done' "$frontend/$scheme: complete table workflow"
    for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'Unable to simultaneously satisfy constraints'; do
      ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
    done
    [ -s "$MITCAD_TEST_WORK/table-$frontend-$scheme.png" ] || ui_fail "$frontend/$scheme: missing table capture"
    [ -s "$MITCAD_TEST_WORK/table-refused-$frontend-$scheme.png" ] || ui_fail "$frontend/$scheme: missing refusal capture"
    [ -s "$MITCAD_TEST_WORK/table-refused-$frontend-$scheme-own-windows.png" ] || ui_fail "$frontend/$scheme: missing composed refusal capture"
    if [ "$UI_FAILURES" -gt 0 ]; then
      ui_log_tail
      exit 1
    fi
    python3 "$here/ui-scripts/table-parity-verify.py" "$model" || exit 1
  done
done
echo "PASS: Qt/AppKit table headers, unit conversion, refusal, dependent geometry and undo in both themes"
