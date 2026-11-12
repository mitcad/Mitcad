#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SelectionScope.cpp app/framework/SelectionScope.hpp app/framework/SelectionFilterPanel.cpp app/framework/SelectionFilterPanel.hpp app/MainWindowSelection.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/PickContext.cpp app/sketch app/framework/CommandSession.cpp app/framework/Selection.cpp
# Sketch selection filters (mitcad#217) through the real UI, on a small
# synthetic design: Box1 (F1.b0, 40 x 20 x 10 on XY), Box2 (F2.b0) standing
# in front of the middle of Box1's front bottom edge as the default
# isometric view sees it, Box3 (F3.b0) further off, Sketch1 (F4) with a
# circle on XY, Plane1 (F5) 20 above XY and Sketch2 (F6) on it: a
# rectangle, a construction line and Box3's top face projected.
#   1. Editing Sketch2 starts with the filter's defaults; the Selection
#      Filter control is in the SKETCH tab.
#   2. Seen isometric, a window dragged to the left over everything selects
#      Sketch2's ordinary, construction and projected curves together, and
#      no body edges, faces, vertices, profiles, dimensions or Sketch1's
#      geometry.
#   3. A body edge in sight is selected by a click, one behind faces not;
#      Shift+click selects the hidden one (the hover shows it while Shift is
#      down, not after), Ctrl+Shift+click adds, Shift+click alone replaces.
#      An edge partly hidden is selected where it is in sight, not where it
#      is hidden.
#   3b. Sketch Dimension (mitcad#866): Box1's edges are below Sketch2's
#      plane; the tool does not take the edge behind faces, and takes the
#      edge in sight as what it projects to.
#   4. Visible edges a cross: a window dragged to the right takes the edges
#      in sight, also the partly hidden one, but not the hidden one.
#      Visible edges empty: a click does not take them, Shift+click does.
#      Dimensions and constraints a cross: a window takes the dimensions.
#      Reset to Defaults.
#   5. Project: its own rows; a click where an edge is hidden takes the face
#      in front, Shift+click the hidden edge; a row changed while it runs;
#      Cancel and OK bring back the sketch's rows exactly as they were.
#   6. Outside sketch mode the SOLID tab's filters decide again (a window
#      keeps one kind: the profiles); editing again starts with the
#      defaults.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-select-test.sh
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-select.XXXXXX)
FILE=$WORK/select.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# Where a click hits each body edge, in sight and behind faces, once the
# camera rests in sketch mode ("Sketch pick edge F1.b0/<edge> visible at x,y").
export MITCAD_LOG_EDGE_PLACES=1

# Seen from the front, right and above (the isometric view), a point of
# Box1's front bottom edge between x = 11 and 29 lies behind Box2. Box1 is
# long enough that the edge has a stretch in sight at each end (about 28 px
# at the view's fit) with room for a place that a click hits it alone, clear
# of the corner's edge and of Box2's, whatever the view's size.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [23, -12],
    "length": 10, "width": 8, "height": 13, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [70, 0],
    "length": 10, "width": 10, "height": 5, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F4", "center": [-40, -10], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F5"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [40, 30], "width": 10, "height": 10},
  {"cmd": "sketch.add_line", "sketch": "F6", "start": [40, 50], "end": [60, 50], "construction": true},
  {"cmd": "sketch.project", "sketch": "F6", "source": "F3:top", "body": "F3.b0"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

DEFAULTS="sketch-geometry=cross, construction-geometry=cross, projected-geometry=cross, annotations=square,\
 profiles=square, visible-edges=square, occluded-edges=empty, faces=empty, vertices=empty, bodies=empty,\
 components=empty, reference-geometry=empty"

# selected: the last "Selected:" line since the mark.
selected() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Selected: " | tail -1; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# distinct line regex: how many different matches of the regex the line has.
distinct() { grep -oE "$2" <<< "$1" | sort -u | wc -l; }

# edge_with visible|occluded|both [text]: an edge of Box1 whose places
# logged last are all in sight, all behind faces, or both (and whose name
# has the text: "bottom" for an edge of the bottom face, which projects
# onto the sketch plane as a line).
edge_with() {
  local start
  start=$(grep -n "Sketch pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" |
    sed -n 's/.*Sketch pick edge F1\.b0\/\([^ ]*\) \(visible\|occluded\) at [0-9]*,[0-9]*$/\1 \2/p' |
    awk -v want="$1" -v text="${2:-}" '{ seen[$1] = seen[$1] " " $2 }
      END {
        for (e in seen) {
          v = index(seen[e], "visible") > 0; o = index(seen[e], "occluded") > 0
          if (text != "" && index(e, text) == 0) {
            continue
          }
          if ((want == "visible" && v && !o) || (want == "occluded" && o && !v) || (want == "both" && v && o)) {
            print e; exit
          }
        }
      }'
}

# place edge visible|occluded: the screen position logged for it.
place() { ui_logged_at "Sketch pick edge F1.b0/$1 $2"; }

click_at() { xdotool mousemove "$1" "$2" click 1; }
shift_click_at() { xdotool mousemove "$1" "$2" keydown shift click 1 keyup shift; }
ctrl_shift_click_at() { xdotool mousemove "$1" "$2" keydown ctrl keydown shift click 1 keyup shift keyup ctrl; }

# cycle row: one click on the row in the Selection Filter control.
cycle() {
  ui_mark
  ui_click_logged "Selection filter button"
  ui_sync
  ui_expect_new "Selection filter row $1 at" "the control lists $1" > /dev/null
  ui_click_logged "Selection filter row $1"
  ui_sync
  ui_key Escape
  ui_sync
}

# window_over left|right: a window over the whole view, dragged to the left
# (from the lower right: also what it crosses) or to the right.
window_over() {
  if [ "$1" = left ]; then
    ui_view_drag 1 97 97 3 3
  else
    ui_view_drag 1 3 3 97 97
  fi
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                           ui_key F6

echo "--- 1. Editing Sketch2"
ui_expect_log "Timeline Sketch2 at" "the timeline is logged"
ui_step "edit Sketch2 (double-click)"        ui_double_click_logged "Timeline Sketch2"
ui_expect_log "Editing sketch F6 (Sketch2)" "Sketch2 is edited"
ui_expect_log "Selection filter sketch defaults: $DEFAULTS" "the sketch filter starts with its defaults"
ui_expect_log "Selection filter button at" "the Selection Filter control is in the SKETCH tab"

echo "--- 2. A window over the isometric view"
ui_mark
ui_step "isometric view (search)"          ui_command "Isometric View"
ui_expect_new "Camera direction -0.577 0.577 -0.577" "seen from the front, right and above"
ui_expect_new "Sketch pick edge F1.b0/" "where the clicks hit Box1's edges is logged"
ui_sync # the whole list
VIS=$(edge_with visible)
OCC=$(edge_with occluded bottom)
BOTH=$(edge_with both)
[ -n "$VIS" ] || ui_fail "no edge of Box1 wholly in sight"
[ -n "$OCC" ] || ui_fail "no bottom edge of Box1 wholly behind faces"
[ -n "$BOTH" ] || ui_fail "no edge of Box1 partly behind Box2"
echo "ok   edges: $VIS in sight, $OCC hidden, $BOTH partly hidden"

ui_mark
ui_step "window to the left over everything" window_over left
ui_expect_new "Selected: " "the window selected"
LINE=$(selected)
[ "$(distinct "$LINE" 'sketch curve c[0-9]+ of F6')" = 9 ] ||
  ui_fail "the window took $(distinct "$LINE" 'sketch curve c[0-9]+ of F6') of Sketch2's 9 curves: $LINE"
[ "$(distinct "$LINE" 'sketch point p[0-9]+ of F6')" -ge 1 ] || ui_fail "no sketch points: $LINE"
if grep -qE 'edge |face |vertex |profile |dimension |constraint |body | of F4([];]|$)' <<< "$LINE"; then
  ui_fail "the window took more than Sketch2's geometry: $LINE"
fi
echo "ok   ordinary, construction and projected curves together, nothing else"

echo "--- 3. Clicks on body edges"
ui_mark
ui_step "click the edge in sight"            click_at $(place "$VIS" visible)
ui_expect_new "Selected: 1 edge [edge $VIS of F1.b0]" "a click takes a body edge in sight"
ui_mark
ui_step "click the hidden edge"              click_at $(place "$OCC" occluded)
ui_expect_new "Selected: nothing" "a click does not take an edge behind faces"
ui_mark
ui_step "pointer on the hidden edge"         xdotool mousemove $(place "$OCC" occluded)
ui_step "Shift down"                         xdotool keydown shift
ui_expect_new "Selection filter widened (Shift)" "Shift widens the filter"
ui_expect_new "Sketch hover: edge $OCC of F1.b0" "the hover shows the hidden edge with Shift"
ui_mark
ui_step "Shift up"                           xdotool keyup shift
ui_expect_new "Selection filter as set" "Shift released"
ui_expect_new "Sketch hover: nothing" "the hover goes back to the filter"
ui_mark
ui_step "Shift+click the hidden edge"        shift_click_at $(place "$OCC" occluded)
ui_expect_new "Selected: 1 edge [edge $OCC of F1.b0]" "Shift+click takes the hidden edge"
ui_mark
ui_step "Ctrl+Shift+click the edge in sight" ctrl_shift_click_at $(place "$VIS" visible)
ui_expect_new "Selected: 2 edges" "Ctrl+Shift+click adds"
ui_mark
ui_step "Shift+click the edge in sight"      shift_click_at $(place "$VIS" visible)
ui_expect_new "Selected: 1 edge [edge $VIS of F1.b0]" "Shift+click alone selects, it does not add"
ui_mark
ui_step "click the partly hidden edge in sight" click_at $(place "$BOTH" visible)
ui_expect_new "Selected: 1 edge [edge $BOTH of F1.b0]" "the part in sight is clickable"
ui_mark
ui_step "click its hidden part"              click_at $(place "$BOTH" occluded)
ui_expect_new "Selected: nothing" "the hidden part is not"

echo "--- 3b. Sketch Dimension out of the plane: in sight only (mitcad#866)"
ui_mark
ui_step "sketch dimension (D)"               ui_key d
ui_expect_new "Sketch tool Sketch Dimension" "the tool started"
ui_step "pointer on the hidden edge"         xdotool mousemove $(place "$OCC" occluded)
ui_expect_new "Sketch body geometry out of the plane:" "the edges below the sketch read"
ui_expect_new "Computing body geometry done in" "read off the UI thread, as a job of its own (mitcad#1656)"
ui_step "click the hidden edge"              click_at $(place "$OCC" occluded)
ui_sync
since_mark "Sketch Dimension picked the body edge $OCC " && ui_fail "the tool took an edge behind faces"
echo "ok   an edge behind faces is not taken"
ui_mark
ui_step "sketch dimension again (D)"         ui_key d
ui_expect_new "Sketch tool Sketch Dimension" "the tool started afresh"
ui_step "click the edge in sight"            click_at $(place "$VIS" visible)
ui_expect_new "Sketch Dimension picked the body edge $VIS of F1.b0" "an edge in sight is taken"
ui_expect_new "out of the plane: projected as a " "as what it projects to (a line, or a point seen end on)"
ui_step "drop the pick (Esc)"                ui_key Escape
ui_sync
ui_step "end the tool (Esc)"                 ui_key Escape
ui_sync
since_mark "Projected the body" && ui_fail "a pick projected the edge"
echo "ok   nothing projected"

echo "--- 4. The rows"
ui_step "visible edges: cross"               cycle visible-edges
ui_expect_new "Selection filter sketch visible-edges cross:" "visible edges a cross"
ui_mark
ui_step "window to the right over everything" window_over right
ui_expect_new "Selected: " "the window selected"
LINE=$(selected)
grep -qF "edge $VIS of F1.b0" <<< "$LINE" || ui_fail "the window missed the edge in sight: $LINE"
grep -qF "edge $BOTH of F1.b0" <<< "$LINE" || ui_fail "the window missed the partly hidden edge: $LINE"
if grep -qF "edge $OCC of F1.b0" <<< "$LINE"; then
  ui_fail "the window took the hidden edge: $LINE"
fi
[ "$(distinct "$LINE" 'sketch curve c[0-9]+ of F6')" = 9 ] || ui_fail "the window missed sketch curves: $LINE"
echo "ok   edges in sight with the sketch, the hidden edge left out"

ui_step "visible edges: empty"               cycle visible-edges
ui_expect_new "Selection filter sketch visible-edges empty:" "visible edges empty"
ui_mark
ui_step "click a sketch curve"               ui_sketch_click 45 30
ui_expect_new "Selected: 1 sketch curve" "a sketch curve selected"
ui_mark
ui_step "click the edge in sight"            click_at $(place "$VIS" visible)
ui_expect_new "Selected: nothing" "an empty row takes no click"
ui_mark
ui_step "Shift+click it"                     shift_click_at $(place "$VIS" visible)
ui_expect_new "Selected: 1 edge [edge $VIS of F1.b0]" "Shift+click takes it all the same"

ui_step "dimensions and constraints: cross"  cycle annotations
ui_expect_new "Selection filter sketch annotations cross:" "dimensions and constraints a cross"
ui_mark
ui_step "window to the left over everything" window_over left
ui_expect_new "Selected: " "the window selected"
LINE=$(selected)
grep -qE 'dimension k[0-9]+ of F6' <<< "$LINE" || ui_fail "the window took no dimensions: $LINE"
echo "ok   the window took the dimensions"

ui_mark
ui_step "open the control"                   ui_click_logged "Selection filter button"
ui_sync
ui_expect_new "Selection filter reset at" "Reset to Defaults is offered"
ui_step "Reset to Defaults"                  ui_click_logged "Selection filter reset"
ui_expect_new "Selection filter sketch reset: $DEFAULTS" "the defaults again"
ui_step "close the control"                  ui_key Escape

echo "--- 5. Project"
ui_step "visible edges: cross"               cycle visible-edges
ui_expect_new "Selection filter sketch visible-edges cross:" "the sketch's rows changed before Project"
CHANGED=${DEFAULTS/visible-edges=square/visible-edges=cross}
ui_mark
ui_step "Project (P)"                        ui_key p
ui_expect_new "Command Project started" "Project started"
ui_expect_new "Selection filter project defaults: visible-edges=cross, occluded-edges=empty, faces=square,\
 vertices=square" "Project's own rows"
ui_mark
ui_step "click where the edge is hidden"     click_at $(place "$OCC" occluded)
ui_expect_new "Project Geometry: 1 face" "the click takes the face in front"
ui_mark
ui_step "Shift+click the hidden edge"        shift_click_at $(place "$OCC" occluded)
ui_expect_new "Project Geometry: 1 face, 1 edge" "Shift+click takes the hidden edge as a source"
ui_expect_new "edge $OCC of F1.b0]" "... the hidden edge"
ui_step "occluded edges: square"             cycle occluded-edges
ui_expect_new "Selection filter project occluded-edges square:" "a row of Project's changed while it runs"
ui_mark
ui_step "cancel (Esc)"                       ui_key Escape
ui_expect_new "Command Project cancelled" "Project cancelled"
ui_expect_new "Selection filter sketch restored: $CHANGED" "Cancel brings back the sketch's rows as they were"

ui_mark
ui_step "Project (P)"                        ui_key p
ui_expect_new "Selection filter project defaults:" "Project starts with its defaults again"
ui_step "Shift+click the hidden edge"        shift_click_at $(place "$OCC" occluded)
ui_expect_new "Project Geometry: 1 edge [edge $OCC of F1.b0]" "the hidden edge as the source"
ui_mark
ui_step "OK (Enter)"                         ui_key Return
ui_expect_new "Projected 1 item(s)" "projected"
ui_expect_new "Selection filter sketch restored: $CHANGED" "OK brings back the sketch's rows as they were"

echo "--- 6. Leaving and editing again"
ui_mark
ui_step "finish the sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch left"
ui_mark
ui_step "window to the left over everything" window_over left
ui_expect_new "Selected: " "the window selected"
LINE=$(selected)
grep -q "profile" <<< "$LINE" || ui_fail "outside sketch mode the window took no profiles: $LINE"
if grep -qE 'sketch curve|sketch point|edge |face ' <<< "$LINE"; then
  ui_fail "outside sketch mode the window kept more than one kind: $LINE"
fi
echo "ok   outside sketch mode the SOLID tab's filters and one kind per window again"
ui_mark
ui_step "clear the selection (Esc)"          ui_key Escape
ui_mark
ui_step "edit Sketch2 again (double-click)"  ui_double_click_logged "Timeline Sketch2"
ui_expect_new "Selection filter sketch defaults: $DEFAULTS" "editing again starts with the defaults"
ui_step "finish the sketch (Ctrl+Enter)"     ui_key ctrl+Return

ui_finish "sketch selection filters"
