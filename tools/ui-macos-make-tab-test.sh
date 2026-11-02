#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/MainWindowMake.cpp app/MainWindowNative.cpp app/print/MakeBodyCard.cpp
# The Make tab (mitcad#1049) on macOS, in Qt and in AppKit: the tabs Solid,
# Build and Make; the moved commands started from it (Qt: the Make tab's
# buttons; AppKit: the native actions of its toolbar) and cancelled: Print
# Analysis, Send to Slicer (once 3D Print) and the Annotate group's
# Dimension; Edit Materials… and Edit Printers and Processes… open their
# windows. Qt: Body2 selected on the Make tab shows its card with Walls 5
# set on the body, and the Browser marks Body2.
# tools/ui-make-tab-test.sh (Linux) and tools/ui-windows-make-tab-test.ps1
# do the rest of the Make tab.
# Usage: tools/ui-macos-make-tab-test.sh <app> <cli> <output> [qt|native|both]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory> [qt|native|both]" >&2
  exit 2
fi
cli=$2
case ${4:-both} in
  qt) frontends=qt ;; native) frontends=native ;; both) frontends='qt native' ;; *) exit 2 ;;
esac
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$MITCAD_TEST_WORK/make.json" << EOF
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[-40,0],"width":30,"height":20},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F1","region":"$R"}],
    "extent":{"type":"distance","distance":5},"operation":"new_body"}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F3","corner":[0,0],"width":40,"height":30},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F3","region":"$R"}],
    "extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Standard","values":{"layer_height":0.2,"perimeters":2}},
  {"cmd":"set_property","owner":"C0","set":"Print Printer","key":"profile","value":"bench"},
  {"cmd":"set_property","owner":"C0","set":"Print Process","key":"profile","value":"standard"},
  {"cmd":"set_property","owner":"F4.b0","set":"Print Process","key":"perimeters","value":5}
]
EOF
"$cli" run "$MITCAD_TEST_WORK/make.json" --save "$MITCAD_TEST_WORK/make.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }

# A command of the Make tab: Qt from its button there, AppKit by the
# native action its toolbar runs.
make_command() {
  if [ "$frontend" = native ]; then echo "native-action $1";
  else echo "click-logged '^Ribbon button $1 at '"; fi
}
cancel() {
  if [ "$frontend" = native ]; then echo 'native-pointer-click commandCancel';
  else echo "button '^Cancel\$'"; fi
}

for frontend in $frontends; do
  for scheme in light dark; do
  script=$UI_OUT/make-tab-$frontend-$scheme.mitcad-ui
  {
    echo '# SPDX-License-Identifier: MIT'
    echo 'wait-idle'
    if [ "$frontend" = native ]; then
      echo "expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'"
      echo "native-appearance '\${MITCAD_TEST_COLOR_SCHEME}'"
      echo 'native-resize 1280 820'
      echo 'native-choice native.workspaces Make'
    else
      echo 'window-resize 1280 820'
    fi
    echo 'expect-ribbon-labels Solid Build Make'
    if [ "$frontend" = qt ]; then
      echo 'mark'
      echo "click-logged '^Ribbon tab MAKE at '"
      echo "expect-log 'Ribbon tab shown: MAKE'"
      echo "expect-log 'Ribbon MAKE/ANNOTATE at '"
      echo "expect-log 'Browser Make markers: Root/Bodies/Body2: Own print settings: Walls'"
    fi
    for command in inspect.print_analysis:'Print Analysis' make.print3d:'Send to Slicer' make.dimension:'Dimension'; do
      echo 'mark'
      make_command "${command%%:*}"
      echo "expect-log 'Command ${command#*:} started'"
      cancel
      echo "expect-log 'Command ${command#*:} cancelled'"
    done
    echo 'mark'
    make_command solid.materials
    echo "expect-log 'Materials opened'"
    echo 'mark'
    make_command make.printers
    echo "expect-log 'Printers and processes opened'"
    if [ "$frontend" = qt ]; then
      echo 'focus-main'
      echo 'mark'
      echo "click-logged 'Browser Root/Bodies/Body2'"
      echo "expect-log 'printer Bench \\(component Root\\); process Standard \\(component Root\\); own: Walls = 5 \\(body Body2\\)'"
    fi
    # The picture for the ticket (light and dark, Qt and AppKit).
    echo "screenshot '$MITCAD_TEST_WORK/make-tab-$frontend-$scheme.png'"
  } > "$script"
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$script \
    ui_run_app "make-tab-$frontend-$scheme" --ui="$frontend" --no-recovery --no-mcp \
      --open "$MITCAD_TEST_WORK/make.mitcad"
  ui_expect_log '^TestDriver: done' "$frontend/$scheme: the Make tab, its commands and the body card"
  for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "$frontend/$scheme: no $problem"
  done
  [ -s "$MITCAD_TEST_WORK/make-tab-$frontend-$scheme.png" ] || ui_fail "$frontend/$scheme: no picture"
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
  done
done
echo 'PASS: the Make tab in Qt and AppKit'
