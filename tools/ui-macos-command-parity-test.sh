#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/commands/SweepCommands.cpp app/commands/InspectCommands.cpp
# check-all sources: app/framework/CommandPanel app/platform/macos/NativeForms app/platform/macos/NativeMacWindow app/framework/TestDriver
# Recent declarative command additions in Qt and AppKit, light and dark:
# All Edges, Sweep Distance 1/2, and Section Axis/Angle/miss warnings.
# Synthetic fixtures, actual input, previews, committed geometry and undo.
# Usage: tools/ui-macos-command-parity-test.sh <app> <cli> <output> [qt|native|both]
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
for model in box sweep section; do
  "$cli" run "$here/ui-scripts/command-parity-$model.json" --save "$MITCAD_TEST_WORK/$model.mitcad" \
    > "$UI_OUT/setup-$model.log" 2>&1 || { cat "$UI_OUT/setup-$model.log" >&2; exit 1; }
done

# Append equivalent ordinary controls to this frontend's driver script.
action() {
  if [ "$frontend" = native ]; then echo "native-action $1"; else echo "command $1"; fi
}
input_click() {
  if [ "$frontend" = native ]; then echo "native-pointer-click input_$2";
  else echo "click-logged 'Panel $1 input $2'"; fi
}
field() {
  echo 'mark'
  if [ "$frontend" = native ]; then echo "native-pointer-click input_$1";
  else echo "focus-widget input_$1"; fi
  echo 'key Ctrl+a'
  echo "type '$2'"
  echo 'key Tab'
  echo "expect-input-text $1 '^$2\$'"
}
visible() {
  if [ "$frontend" = native ]; then echo "expect-native-visible input_$1 $2";
  else echo "expect-widget-property input_$1 visible $2"; fi
}
ok_enabled() {
  if [ "$frontend" = native ]; then echo "expect-native-enabled commandOk $1";
  else echo "expect-widget-property commandOk enabled $1"; fi
}
confirm() {
  if [ "$frontend" = native ]; then echo 'native-pointer-click commandOk';
  else echo "button '^$1\$'"; fi
}
cancel() {
  if [ "$frontend" = native ]; then echo 'native-pointer-click commandCancel';
  else echo "button '^Cancel\$'"; fi
}
header() {
  echo '# SPDX-License-Identifier: MIT'
  echo 'wait-idle'
  if [ "$frontend" = native ]; then
    echo "expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'"
    echo "native-appearance '\${MITCAD_TEST_COLOR_SCHEME}'"
    echo 'native-resize 1280 820'
  else
    echo 'window-resize 1280 820'
  fi
  action view.fit
}

for frontend in $frontends; do
  for scheme in light dark; do
    # All Edges switches body/edge inputs in both dress-up commands. The
    # fillet is committed and undone; the chamfer preview is cancelled.
    script=$UI_OUT/all-edges-$frontend-$scheme.mitcad-ui
    {
      header
      echo 'expect-model-bodies 1 48000'
      echo 'capture-undo-depth before'
      action solid.fillet
      echo "expect-log 'Command Fillet started'"
      input_click Fillet sets.0.all_edges
      echo "expect-log 'Fillet: All Edges = on'"
      visible sets.0.body true
      visible sets.0.edges false
      input_click Fillet sets.0.all_edges
      visible sets.0.body false
      visible sets.0.edges true
      input_click Fillet sets.0.all_edges
      input_click Fillet sets.0.body
      if [ "$frontend" = native ]; then
        echo 'native-expand native.modelTree'
        echo 'native-select native.model.body:F2.b0@'
      else
        echo "click-logged 'Browser Root/Bodies/Body1'"
      fi
      echo "expect-log 'Fillet Body: 1 body'"
      field sets.0.radius '30 deg'
      ok_enabled false
      echo 'expect-model-bodies 1 48000'
      echo 'expect-undo-depth ${before} 0'
      field sets.0.radius '1 mm'
      echo "expect-log 'Preview Fillet: ok' 30"
      ok_enabled true
      echo "screenshot '$MITCAD_TEST_WORK/all-edges-$frontend-$scheme.png'"
      confirm OK
      echo "expect-log 'Added fillet on all edges' 30"
      echo 'expect-model-bodies 1 47898.33035271402'
      echo 'expect-undo-depth ${before} 1'
      action edit.undo
      echo 'expect-model-bodies 1 48000'
      echo 'expect-undo-depth ${before} 0'
      action solid.chamfer
      input_click Chamfer sets.0.all_edges
      visible sets.0.body true
      visible sets.0.edges false
      input_click Chamfer sets.0.body
      if [ "$frontend" = native ]; then
        echo 'native-expand native.modelTree'
        echo 'native-select native.model.body:F2.b0@'
      else
        echo "click-logged 'Browser Root/Bodies/Body1'"
      fi
      # A real change requests a new preview even when transient keystrokes
      # are coalesced. Re-entering the default 1 mm may be deduplicated.
      field sets.0.distance '2 mm'
      echo "expect-log 'Preview Chamfer: ok' 30"
      ok_enabled true
      cancel
      echo 'expect-model-bodies 1 48000'
      echo 'expect-undo-depth ${before} 0'
    } > "$script"

    # Edit the saved whole-path sweep: the profile lies inside the path,
    # so both distances are visible. Refuse 2, then keep half the path
    # after the profile with nothing before it (12.5 mm of cylinder).
    script=$UI_OUT/sweep-$frontend-$scheme.mitcad-ui
    {
      header
      echo 'expect-model-bodies 1 5890.486225480862'
      echo 'capture-undo-depth before'
      if [ "$frontend" = native ]; then echo 'native-activate native.history.F3';
      else echo "dblclick-logged 'Timeline Sweep1'"; fi
      echo "expect-log 'Editing F3 with Sweep'"
      echo "expect-log 'Sweep: the profile lies inside an open path; Distance 2 shown'"
      visible fraction true
      visible fraction2 true
      # Editing a persisted feature can roll the timeline back first. That
      # temporary setup is not an invalid field's action; compare the
      # refusal with the active edit, and the commit with the original file.
      echo 'capture-undo-depth editing'
      field fraction '2'
      echo "expect-log 'Preview Sweep: failed: .*fraction.*between 0 and 1' 30"
      ok_enabled false
      echo 'expect-undo-depth ${editing} 0'
      field fraction2 '0'
      field fraction '0.5'
      echo "expect-log 'Preview Sweep: ok' 30"
      ok_enabled true
      echo "screenshot '$MITCAD_TEST_WORK/sweep-$frontend-$scheme.png'"
      confirm OK
      echo "expect-log 'Edited F3' 30"
      echo 'expect-model-bodies 1 981.7477042468104'
      echo 'expect-undo-depth ${before} 1'
      action edit.undo
      echo 'expect-model-bodies 1 5890.486225480862'
      echo 'expect-undo-depth ${before} 0'
    } > "$script"

    # YZ through a cube centred on Z; 45 degrees without an axis is refused.
    # Picking Z makes the diagonal cut; offsets outside the cube must show
    # the two explicit miss warnings and keep the underlying body intact.
    script=$UI_OUT/section-$frontend-$scheme.mitcad-ui
    {
      header
      echo 'expect-model-bodies 1 1000'
      echo 'capture-undo-depth before'
      action inspect.section
      if [ "$frontend" = native ]; then
        echo "click-logged 'Datum yz'"
      else
        # Commands do not reveal hidden origins. Pick the actual browser
        # row, as the Move/Copy parity workflow does, rather than an old
        # viewport position now covered by the body.
        echo 'mark'
        echo "click-logged '^Browser arrow Root/Origin at '"
        echo 'wait-idle 60'
        echo "expect-log '^Browser Root/Origin/YZ at '"
        echo "click-logged '^Browser Root/Origin/YZ at '"
      fi
      echo "expect-log 'Section Analysis Faces/Plane: 1 plane'"
      echo "expect-log 'Inspect Section Analysis: area 100, length 40' 30"
      visible axis true
      visible angle true
      field angle '45 deg'
      echo "expect-log 'Inspect Section Analysis: failed: Select an axis for the angle.'"
      echo 'expect-model-bodies 1 1000'
      echo 'expect-undo-depth ${before} 0'
      input_click 'Section Analysis' axis
      if [ "$frontend" = native ]; then
        echo "click-logged 'Datum z'"
      else
        echo "click-logged '^Browser Root/Origin/Z at '"
      fi
      echo "expect-log 'Section Analysis Axis: 1 axis'"
      echo "expect-log 'Inspect Section Analysis: area 141\.42' 30"
      echo 'expect-shown-body F2.b0 - 500'
      echo "screenshot '$MITCAD_TEST_WORK/section-angle-$frontend-$scheme.png'"
      field offset '30 mm'
      echo "expect-log 'Inspect Section Analysis: The plane is beyond the bodies: it cuts nothing.' 30"
      echo 'expect-shown-body F2.b0 - 1000'
      if [ "$frontend" = native ]; then
        echo "expect-native-text commandResult 'The plane is beyond the bodies: it cuts nothing.'"
      else echo 'expect-widget-property commandResult visible true'; fi
      echo "screenshot '$MITCAD_TEST_WORK/section-miss-$frontend-$scheme.png'"
      field offset '-30 mm'
      echo "expect-log 'Inspect Section Analysis: The plane is beyond the bodies: it cuts them all away.' 30"
      echo 'expect-shown-body F2.b0 - 0'
      field offset '0 mm'
      echo "expect-log 'Inspect Section Analysis: area 141\.42' 30"
      if [ "$frontend" = native ]; then confirm Close;
      else echo "button '^OK\$' commandOk"; fi
      echo "expect-log 'Kept Section Analysis as Section1' 30"
      echo 'expect-model-bodies 1 1000'
      echo 'expect-undo-depth ${before} 1'
      action edit.undo
      echo 'expect-model-bodies 1 1000'
      echo 'expect-shown-body F2.b0 - 1000'
      echo 'expect-undo-depth ${before} 0'
    } > "$script"

    for phase in all-edges sweep section; do
      rm -rf "$MITCAD_SETTINGS_DIR"
      mkdir -p "$MITCAD_SETTINGS_DIR"
      case $phase in all-edges) model=box ;; *) model=$phase ;; esac
      MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=70 \
        MITCAD_TEST_INPUT=$UI_OUT/$phase-$frontend-$scheme.mitcad-ui \
        ui_run_app "$phase-$frontend-$scheme" --ui="$frontend" --no-recovery --no-mcp \
          --open "$MITCAD_TEST_WORK/$model.mitcad"
      ui_expect_log '^TestDriver: done' "$phase/$frontend/$scheme: all input, geometry and undo assertions passed"
      for problem in 'TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' 'Native Mac canvas:'; do
        ui_expect_no_log "$problem" "$phase/$frontend/$scheme: no $problem"
      done
      case $phase in
        section) shots='section-angle section-miss' ;;
        *) shots=$phase ;;
      esac
      for shot in $shots; do
        [ -s "$MITCAD_TEST_WORK/$shot-$frontend-$scheme.png" ] || ui_fail "$phase/$frontend/$scheme: missing $shot capture"
      done
      if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
    done
  done
done
echo 'PASS: recent declarative command controls agree in Qt and AppKit, light and dark'
