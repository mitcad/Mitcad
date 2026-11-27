#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession app/framework/DocumentSession app/framework/ManipulatorOverlay app/framework/TestDriver app/MainWindowJobs
# Real held mouse input keeps flowing while one exact preview owns the model.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
ui_init "$1" "$3"
cli=$2
frontend=${4:-qt}
UI_TIMEOUT=240
# One-pixel held positions also exercise motion within a snap band. The
# geometry-aware handle step can be finer than the old zoom-only step, so
# 48 positions over 180 pixels may change the snapped value on every move.
# Keep the full drag and all continuous/semantic/preview/history assertions.
drag_steps=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
for fixture in box curved; do
  python3 "$here/ui-scripts/drag-preview-fixture.py" "$fixture" > "$UI_OUT/setup.json"
  "$cli" run "$UI_OUT/setup.json" --save "$MITCAD_TEST_WORK/drag.mitcad" > "$UI_OUT/setup-$fixture.log" 2>&1
  cat > "$UI_OUT/drag.mitcad-ui" <<'DRIVER'
wait-idle 60
window-resize 1400 990
command view.front
command view.fit
expect-viewport
test-manipulator-feedback
capture-undo-depth initial
dblclick-logged '^Timeline Drag boss at '
expect-log 'Editing .* with Extrude'
wait-idle 60
focus-main
mark
drag-from '^Manipulator Extrude distance at ' 0 -180
wait-idle 60
capture distance '^Manipulator Extrude distance dragged to ([0-9.]+)$'
expect-input-text distance '^${distance:re}( mm)?$'
button '^OK$'
expect-log 'Edited F[35]' 60
expect-undo-depth ${initial} 1
menu 'File/Save'
expect-log 'Saved .*drag\.mitcad' 30
command edit.undo
expect-undo-depth ${initial} 0
command edit.redo
expect-undo-depth ${initial} 1
expect-never 'The document was read while a job computes it|OCCT view event failed|Recompute failed|Manipulator Extrude .*ended by a dialog'
DRIVER
  if [ "$frontend" = native ]; then
    boss=F3
    [ "$fixture" != curved ] || boss=F5
    python3 - "$UI_OUT/drag.mitcad-ui" "$boss" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
path.write_text(path.read_text().replace("dblclick-logged '^Timeline Drag boss at '",
    "expect-native-visible native.historyCard true\nnative-activate native.history." + sys.argv[2])
    .replace("button '^OK$'", "native-click commandOk"))
PY
  fi
  # The controlled delay exceeds the modal threshold even on a simple box.
  # Six held ticks make the pause shorter than a running preview, then input
  # resumes without cancellation or a backlog of intermediate field actions.
  # At the final position the button stays down until a useful exact solid
  # is installed. The driver polls without blocking or adding move events.
  MITCAD_LOG_TIMING=1 MITCAD_TEST_STEP_MS=16 MITCAD_TEST_SETTLE_MS=0 \
    MITCAD_TEST_DRAG_STEPS=$drag_steps MITCAD_TEST_DRAG_PAUSE_TICKS=6 \
    MITCAD_TEST_DRAG_WAIT_PREVIEW=1 \
    MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=500 MITCAD_TEST_INPUT=$UI_OUT/drag.mitcad-ui \
    ui_run_app "$fixture" --ui="$frontend" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/drag.mitcad"
  ui_expect_log '^TestDriver: done' "$fixture: all held-drag and history assertions completed"
  ui_expect_no_log 'Progress dialog shown: preview' "$fixture: preview never blocks input"
  ui_expect_no_log 'Computing preview cancelled' "$fixture: numeric motion never cancels useful work"
  if ! python3 - "$UI_LOG_FILE" "$MITCAD_TEST_WORK/drag.mitcad" "$drag_steps" <<'PY'
import json, re, sys
text = open(sys.argv[1], encoding="utf-8").read()
steps = int(sys.argv[3])
assert "Held drag: begin\n" in text and "Held drag: end," in text, "missing completed held drag"
gesture = text.split("Held drag: begin\n", 1)[1].split("Held drag: end,", 1)[0]
assert "Preview installed: Extrude, dragging=1" in gesture, "no useful solid before release"
assert "Held drag: exact preview available before release" in gesture, "no completed final held wait"
assert "Preview numeric replaced:" in gesture, "did not replace pending numeric motion"
assert not re.search(r"pending= [2-9]", gesture), "more than one pending distance"
assert "Preview Extrude: intermediate numeric result" in gesture, "no intermediate solid installed"
assert "Held drag: resumed" in gesture
feedback = re.search(r"Manipulator feedback Extrude distance: moves=(\d+) semantic=(\d+) visual=(\d+) paints=(\d+)", text)
assert feedback, "missing delivered/semantic/visual/paint counters"
moves, semantic, visual, paints = map(int, feedback.groups())
assert moves == steps and visual > semantic > 0 and paints > 0, feedback[0]
delivered = re.search(r"Held drag: end, delivered= (\d+) elapsed_ms= (\d+)", text)
assert delivered and int(delivered[1]) == steps
distance = float(re.search(r"Manipulator Extrude distance dragged to ([0-9.]+)", text)[1])
design = json.load(open(sys.argv[2], encoding="utf-8"))
feature = next(f for f in design["features"] if f["name"] == "Drag boss")
expression = feature["extent"]["distance"]
expression = next((p["expression"] for p in design["parameters"] if p["name"] == expression), expression)
assert abs(float(str(expression).split()[0]) - distance) < 1e-6, feature
print("Held drag evidence:", "events", delivered[1], "elapsed_ms", delivered[2],
      "numeric_replacements", gesture.count("Preview numeric replaced:"),
      "evaluated_pending", gesture.count("Preview numeric drained:"),
      "solid_installs", gesture.count("Preview installed:"), "committed_mm", distance,
      "semantic_changes", semantic, "visual_updates", visual, "overlay_paints", paints)
PY
  then
    ui_fail "$fixture: held-drag evidence failed (see $UI_LOG_FILE)"
  fi
  [ "$UI_FAILURES" = 0 ] || exit 1
done
echo "PASS: nonblocking held drag, latest numeric value and exact committed history ($frontend)"
