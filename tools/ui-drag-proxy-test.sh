#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSessionProxy.cpp app/framework/DragProxy.cpp
# check-all sources: app/commands/ExtrudeDragProxy.cpp app/view/ProvisionalMesh.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/ManipulatorOverlay.cpp app/OcctViewer.cpp
# check-all sources: app/framework/Numbers.cpp
# check-all sources: core/model/src/document.rs core/model-api/src/lib.rs tools/cli
# Extrude's provisional drag presentation (mitcad#636) through the real UI,
# with held drags of the distance's arrow: 180 mouse moves 16 ms apart
# (xdotool on the hidden Xvfb display, through the X server), a pause of
# six ticks half-way, then the release. On the ticket's synthetic designs,
# which mitcad-cli makes:
#   - box: a 120 x 100 x 20 mm block and a 20 x 10 mm rectangle at (50, 45)
#     on a plane on its top; holes: the same block with 83 through holes of
#     4 mm (a body of 94 faces with the boss). Extrude joins the rectangle,
#     5 mm, then its arrow is dragged up, once as the exact preview takes
#     and once with every preview held back 500 ms
#     (MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=500): the proxy follows the
#     dragged value while the exact preview computes ("Drag proxy Extrude
#     distance: <inputs>, <proxy updates>, <exact previews>, <frames>"),
#     the exact preview of the last value replaces it after the release,
#     and OK commits that value as Join; undo and redo.
#   - reach: the rectangle on a plane 10 mm over the block, 5 mm down: New
#     Body. Dragged down into the block, the proxy shows the tool going in
#     while the panel keeps New Body; the exact preview after the release
#     makes it Cut, and OK commits a Cut of the last value. The dragged
#     negative values ("-17.5 mm") bring no "= value" line under the
#     field (mitcad#994).
#   - placed: the block and the rectangle in a component turned 90 degrees
#     about Z and moved 60 mm along X, its boss's Join edited: the exact
#     tool is drawn where the component is (no longer at its own
#     coordinates), and the proxy over it.
#   - Escape and Exit right after a release, while the exact preview
#     computes: the proxy goes with the command, nothing is added.
# Each drag's line is printed: inputs (snapped values), proxy updates and
# their interval, exact previews and theirs, frames drawn (submitted, not
# fenced) and the time held.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drag-proxy-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-drag-proxy.XXXXXX")
# UI_KEEP_WORK=1 keeps the designs and logs.
trap 'ui_cleanup; [ -n "${UI_KEEP_WORK:-}" ] || rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
export MITCAD_LOG_TIMING=1

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# new_line "regex": the first line since the mark that matches.
new_line() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -m1 -E -- "$1"; }

# near a b tolerance description
near() {
  awk -v a="$1" -v b="$2" -v t="$3" 'BEGIN { exit !((a - b) <= t && (b - a) <= t) }' ||
    ui_fail "$4: $1, not $2"
  echo "ok   $4"
}

# held_drag "logged text" dx dy: a held left drag from the last "<text> at
# x,y" by (dx, dy) pixels in 180 moves 16 ms apart, six ticks' pause
# half-way, then the release.
held_drag() {
  local fx fy i steps=180 args=() start end
  read -r fx fy <<< "$(ui_logged_at "$1")"
  ui_apart "$fx" "$fy"
  args=(mousemove "$fx" "$fy" mousedown 1)
  for ((i = 1; i <= steps; i++)); do
    [ "$i" = $((steps / 2)) ] && args+=(sleep 0.096)
    args+=(mousemove $((fx + $2 * i / steps)) $((fy + $3 * i / steps)) sleep 0.016)
  done
  start=$(date +%s%N)
  xdotool "${args[@]}" mouseup 1
  end=$(date +%s%N)
  HELD_MS=$(((end - start) / 1000000))
}

# drag_summary label [min]: checks and prints the drag's line (at least
# min inputs, default 20); sets INPUTS,
# PROXIES, EXACTS.
drag_summary() {
  local line
  ui_expect_new " proxy updates (every " "$1: the drag's counts logged" 20 > /dev/null
  line=$(new_line "Drag proxy Extrude distance: [0-9]+ inputs, ")
  [[ $line =~ :\ ([0-9]+)\ inputs,\ ([0-9]+)\ proxy\ updates\ \(every\ ([0-9.]+)\ ms\ median,\ ([0-9.]+)\ ms\ p95\),\ ([0-9]+)\ exact\ previews\ \(every\ ([0-9.]+)\ ms\ median,\ ([0-9.]+)\ ms\ p95\),\ ([0-9]+)\ frames\ in\ ([0-9]+)\ ms\;\ proxy\ update\ ([0-9.]+)\ ms\ median,\ ([0-9.]+)\ ms\ max ]] ||
    ui_fail "$1: drag line not understood: $line"
  INPUTS=${BASH_REMATCH[1]} PROXIES=${BASH_REMATCH[2]} EXACTS=${BASH_REMATCH[5]}
  local frames=${BASH_REMATCH[8]} held=${BASH_REMATCH[9]}
  echo "Drag cadence $1: 180 moves sent in ${HELD_MS} ms (xdotool); held ${held} ms in the app;" \
    "${INPUTS} snapped inputs; ${PROXIES} proxy updates (interval median ${BASH_REMATCH[3]} ms, p95 ${BASH_REMATCH[4]} ms);" \
    "${EXACTS} exact previews shown (interval median ${BASH_REMATCH[6]} ms, p95 ${BASH_REMATCH[7]} ms);" \
    "${frames} frames submitted; proxy update ${BASH_REMATCH[10]} ms median, ${BASH_REMATCH[11]} ms max"
  [ "$INPUTS" -ge "${2:-20}" ] || ui_fail "$1: only $INPUTS inputs delivered"
}

# The designs.
circles=""
regions=""
n=0
for x in 8 18 28 38 48 58 68 78 88 98 108; do
  for y in 8 18 28 38 48 58 68 78 88; do
    if [ "$x" -gt 45 ] && [ "$x" -lt 80 ] && [ "$y" -gt 35 ] && [ "$y" -lt 70 ]; then
      continue
    fi
    n=$((n + 1))
    circles+="{\"cmd\": \"sketch.add_circle\", \"sketch\": \"F3\", \"center\": [$x, $y], \"diameter\": 4},"
    regions+="${regions:+, }{\"sketch\": \"F3\", \"region\": \"r{c$((2 * n - 1))}\"}"
  done
done
[ "$n" -eq 83 ] || ui_fail "83 holes, not $n"
BLOCK='{"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 100},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "'$R'"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}'
cat > "$WORK/holes.json" << EOF
[
  $BLOCK,
  {"cmd": "sketch.create"},
  $circles
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [$regions],
    "start": {"type": "offset", "offset": -5}, "extent": {"type": "distance", "distance": 30}, "operation": "cut"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F5"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [50, 45], "width": 20, "height": 10},
  {"expect": {"body": "Body1", "faces": 89}}
]
EOF
cat > "$WORK/box.json" << EOF
[
  $BLOCK,
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [50, 45], "width": 20, "height": 10},
  {"expect": {"body": "Body1", "faces": 6}}
]
EOF
cat > "$WORK/reach.json" << EOF
[
  $BLOCK,
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [50, 45], "width": 20, "height": 10}
]
EOF
# Board, turned 90 degrees about Z and 60 mm along X: a 40 x 30 x 10 block
# and a 10 x 10 boss 5 mm high joined on its top, at (5, 5) in Board.
cat > "$WORK/placed.json" << EOF
[
  {"cmd": "create_component", "name": "Board",
   "transform": {"translation": [60, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": 1.5707963267948966}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 10}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [5, 5], "width": 10, "height": 10},
  {"cmd": "add_feature", "name": "Board boss", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "join", "participants": ["F2.b0"]}}
]
EOF
for design in holes box reach placed; do
  "$CLI" run "$WORK/$design.json" --save "$WORK/$design.mitcad" > "$WORK/cli-$design.log" 2>&1 ||
    { cat "$WORK/cli-$design.log"; ui_fail "mitcad-cli $design"; }
done

ui_start_display

# join_case design delay label: Extrude joins the rectangle 5 mm; a held
# drag of its arrow up; the exact last value; OK, undo, redo.
join_case() {
  local design=$1 delay=$2 label=$3 value line
  echo "=== $label"
  MITCAD_TEST_RECOMPUTE_DELAY_MS=$delay ui_start_app --open "$WORK/$design.mitcad"
  ui_step "fit (F6)"                       ui_key F6
  ui_expect_log "Bodies shown: F2.b0" "$label: the block shown" > /dev/null
  ui_mark
  ui_step "extrude (E)"                    ui_key e
  ui_expect_new "Extrude: automatic Operation = Join" "$label: Join onto the block" 30
  ui_step "distance 5"                     ui_type_in "Panel Extrude input distance" "5"
  ui_wait_idle
  ui_expect_new "Preview Extrude: ok" "$label: previewed" 30
  ui_expect_log "Preview tool at 60.000,50.000,22.500" "$label: the tool where the boss is"
  ui_expect_log "Manipulator Extrude distance at " "$label: the distance's arrow" > /dev/null
  ui_focus_main
  ui_mark
  held_drag "Manipulator Extrude distance" 0 -180
  ui_expect_new "Manipulator Extrude distance dragged to " "$label: released" 20
  value=$(new_line "Manipulator Extrude distance dragged to " | sed 's/.* dragged to //')
  drag_summary "$label"
  ui_expect_new "Drag proxy Extrude: shown for distance " "$label: the proxy shown during the drag"
  line=$(new_line "Drag proxy Extrude: shown for distance ")
  [[ $line =~ at\ 60\.000,50\.000, ]] || ui_fail "$label: the proxy is not over the boss: $line"
  [ "$PROXIES" -gt "$EXACTS" ] || ui_fail "$label: fewer proxy updates ($PROXIES) than exact previews ($EXACTS)"
  if [ "$delay" != 0 ]; then
    # Every exact preview takes over 500 ms: the proxy evolves meanwhile.
    [ "$PROXIES" -ge 10 ] || ui_fail "$label: only $PROXIES proxy updates while the exact previews took 500 ms"
    ui_expect_new "Drag proxy Extrude: the exact preview of $value mm replaced it " \
      "$label: the exact preview of the last value replaces the proxy" 30
  fi
  ui_wait_idle
  ui_expect_new "Preview Extrude: ok" "$label: the last value previewed" 30
  ui_focus_main
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Added extrude (Join), distance $value" "$label: OK commits the last dragged value $value mm" 30
  no_new "Drag proxy Extrude: shown" "$label: no proxy after OK"
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: " "$label: undone"
  ui_step "redo (Ctrl+Shift+Z)"            ui_key ctrl+shift+z
  ui_expect_new "Redo: " "$label: redone"
  no_new "Drag proxy" "$label: nothing provisional on undo and redo"
  grep -q "faces that could not be meshed" "$UI_LOG" && ui_fail "$label: faces failed to mesh"
  grep -qE "The document was read while a job computes it|OCCT view event failed" "$UI_LOG" &&
    ui_fail "$label: the model or the view was used while a job computed"
  [ -z "${UI_KEEP_WORK:-}" ] || cp "$UI_LOG" "$WORK/${label//[^a-z0-9]/-}.app.log"
  ui_stop_app
}

# UI_DRAG_SKIP_JOIN=1 leaves the four Join drags out (while working on the
# others).
if [ -z "${UI_DRAG_SKIP_JOIN:-}" ]; then
  join_case box 0 "box"
  join_case holes 0 "83 holes"
  join_case box 0,preview=500 "box, previews held 500 ms"
  join_case holes 0,preview=500 "83 holes, previews held 500 ms"
fi
if [ "${UI_DRAG_ONLY:-}" = join ]; then
  echo "PASS: drag proxy (the Join drags only)"
  exit 0
fi

echo "=== Reach: New Body becomes Cut with the distance"
MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=500 ui_start_app --open "$WORK/reach.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_step "distance -5"                      ui_type_in "Panel Extrude input distance" "-5"
ui_wait_idle
ui_expect_new "Preview Extrude: ok" "previewed 5 mm down" 30
no_new "Extrude: automatic Operation" "5 mm down from 10 mm over the block: New Body"
ui_expect_log "Manipulator Extrude distance at " "the distance's arrow" > /dev/null
ui_focus_main
ui_mark
# About 0.5 mm a pixel: 40 pixels take the tool 15 to 25 mm down, from
# z = 30 into the block's top at z = 20 but not through it.
held_drag "Manipulator Extrude distance" 0 40
ui_expect_new "Manipulator Extrude distance dragged to " "released" 20
value=$(new_line "Manipulator Extrude distance dragged to " | sed 's/.* dragged to //')
awk -v v="$value" 'BEGIN { exit !(v < -10 && v > -30) }' || ui_fail "the drag did not end in the block: $value"
drag_summary "reach" 4
ui_expect_new "Drag proxy Extrude: shown for distance " "the proxy shows the tool going down"
ui_expect_new "Extrude: automatic Operation = Cut" "an exact preview going into the block makes it Cut" 30
ui_wait_idle
ui_expect_new "Drag proxy Extrude: the exact preview of $value mm replaced it " "the exact preview replaces the proxy" 30
# The drag wrote "-17.5 mm" and the like in the field: no "= -17.5 mm"
# line came under it, so the panel kept its height (mitcad#994). (The
# exact preview of the last value above came after its evaluation.)
no_new "Panel Extrude distance hint:" "no '= value' line under the dragged negative distance"
# Each operation the panel was given came with an exact preview that had
# just been computed, none from the proxy.
tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  awk '/Computing preview done/ { done = 1 }
       /Extrude: automatic Operation = / { if (!done) bad = 1; done = 0 }
       END { exit bad }' || ui_fail "an operation was chosen without an exact preview"
echo "ok   the operations came from exact previews:" \
  "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -oE "automatic Operation = [A-Za-z ]+" | sort | uniq -c | tr -s ' ' | tr '\n' ';')"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added extrude (Cut), distance $value" "OK commits the Cut of the last value $value mm" 30
ui_stop_app

echo "=== Placed: a Join of a component turned and moved"
ui_start_app --open "$WORK/placed.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_double_click_expect "Timeline Board boss" " with Extrude" "editing Board boss"
ui_wait_idle
ui_expect_new "Preview Extrude: ok" "the edit previewed" 30
# (5..15, 5..15) in Board: turned about Z to (-15..-5, 5..15), then 60 mm
# along X: x 45..55, y 5..15; z 10..15.
ui_expect_new "Preview tool at 50.000,10.000,12.500" "the exact tool drawn where Board puts it"
ui_focus_main
ui_mark
held_drag "Manipulator Extrude distance" 0 -120
ui_expect_new "Manipulator Extrude distance dragged to " "released" 20
value=$(new_line "Manipulator Extrude distance dragged to " | sed 's/.* dragged to //')
drag_summary "placed"
line=$(new_line "Drag proxy Extrude: shown for distance ")
[[ $line =~ at\ 50\.000,10\.000,([0-9.]+)$ ]] || ui_fail "the proxy is not where Board puts the tool: $line"
ui_wait_idle
z=$(awk -v v="$value" 'BEGIN { printf "%.3f", 10 + v / 2 }')
ui_expect_log "Preview tool at 50.000,10.000,$z" "the exact tool of the last value $value mm where Board puts it"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited " "the edit committed" 30
ui_stop_app

echo "=== Escape and Exit while the exact preview computes"
MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=1500 ui_start_app --open "$WORK/box.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_step "distance 5"                       ui_type_in "Panel Extrude input distance" "5"
ui_wait_idle
ui_expect_new "Preview Extrude: ok" "previewed" 30
ui_focus_main
ui_mark
held_drag "Manipulator Extrude distance" 0 -100
ui_expect_new "Drag proxy Extrude: shown for distance " "the proxy shown"
xdotool key Escape
ui_expect_new "Drag proxy Extrude: hidden: the command was cancelled" "Escape takes the proxy away" 30
ui_wait_idle
no_new "Added extrude" "nothing added"
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_step "distance 5"                       ui_type_in "Panel Extrude input distance" "5"
ui_wait_idle
ui_expect_new "Preview Extrude: ok" "previewed again" 30
ui_focus_main
ui_mark
held_drag "Manipulator Extrude distance" 0 -100
ui_expect_new "Drag proxy Extrude: shown for distance " "the proxy shown again"
xdotool key ctrl+q
for _ in $(seq 1 100); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "Mitcad did not exit"
wait "$UI_RUNNER" || ui_fail "exit code $?"
UI_RUNNER=""
grep -qiE "crash|Segmentation" "$UI_LOG" && ui_fail "a crash on Exit"
echo "ok   Exit with the proxy shown"
echo "PASS: drag proxy"
