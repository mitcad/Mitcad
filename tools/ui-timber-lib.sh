# SPDX-License-Identifier: MIT
# What the TIMBER group's UI tests share (mitcad#520: tools/ui-timber-test.sh,
# ui-timber-wall-test.sh, ui-timber-floor-roof-test.sh; each declares this
# file in its "check-all sources" lines): a small library of synthetic
# parts made here with mitcad-cli (a 48 x 98 timber member and a wood screw;
# no supplier's files), fetched from a local git repository, and the
# helpers that read the panels' logs and click the places they log.
# Sourced after ui-test-lib.sh.
#
#   timber_library NAME [nail]
#                         the work folder build/NAME.XXXXXX (WORK, removed
#                         at the end), the library made and fetched (LIB,
#                         MITCAD_LIBRARIES_DIR), CLI; with "nail" a ring
#                         nail too (TIMBER_WOOD: the wood fasteners, 1 or 2)
#
# The helpers take the command whose panel logs the lines (Timber, Wall,
# Floor / Roof): "Inspect <command>: ...", "View hint <command>: ...".

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
# frame NAME ORIGIN Z X: a mount frame (docs/libraries.md, "BUILD parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
# member W T: a timber member W wide (along X) and T thick (along Y),
# centred on the Z axis, extruded by L from z = 0, with its end frames and
# a face frame on each side (docs/libraries.md, "Timber parts").
member() {
  local hw ht
  hw=$(awk -v s="$1" 'BEGIN { print s / 2 }')
  ht=$(awk -v s="$2" 'BEGIN { print s / 2 }')
  printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},\n'
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-%s, -%s], "width": %s, "height": %s},\n' \
    "$hw" "$ht" "$1" "$2"
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame face_xp "[$hw, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame face_xn "[-$hw, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame face_yp "[0, $ht, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame face_yn "[0, -$ht, 0]" "[0, -1, 0]" "[0, 0, 1]"
}
design() { # design FILE: the script on stdin saved as a design
  local script=$WORK/$(basename "$1" .mitcad).json
  cat > "$script"
  mkdir -p "$(dirname "$1")"
  "$CLI" run "$script" --save "$1" > /dev/null || ui_fail "mitcad-cli run $(basename "$script") failed"
}

timber_library() {
  mkdir -p "$ROOT/build"
  WORK=$(mktemp -d "$ROOT/build/$1.XXXXXX")
  trap 'ui_cleanup; rm -rf "$WORK"' EXIT
  export MITCAD_LIBRARIES_DIR=$WORK/cache
  LIB=$WORK/test-timber
  "$CLI" library init "$LIB" --id test-timber --name "Test timber" --license MIT > /dev/null ||
    ui_fail "library init"
  { member 98 48
    printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/dimensioned-timber/timber-48-test.mitcad"
  # A wood screw: a 5 x 5 shank 70 long along +Z, its axis frame at the head.
  { printf '[\n  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 5, "width": 5, "height": 70, "operation": "new_body"}},\n'
    frame axis "[2.5, 2.5, 0]" "[0, 0, 1]" "[1, 0, 0]"
    printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/fasteners/wood-screw-test.mitcad"
  # With "nail" (mitcad#521): a ring nail too, a 3 x 3 shank 90 long along
  # +Z, its axis frame at the head.
  local nail_entry=""
  TIMBER_WOOD=1
  if [ "${2:-}" = nail ]; then
    { printf '[\n  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
      "corner": [0, 0], "length": 3, "width": 3, "height": 90, "operation": "new_body"}},\n'
      frame axis "[1.5, 1.5, 0]" "[0, 0, 1]" "[1, 0, 0]"
      printf '  {"expect": {"bodies": 1}}\n]\n'; } | design "$LIB/fasteners/ring-nail-test.mitcad"
    nail_entry=',
    {"id": "ring-nail-test", "path": "fasteners/ring-nail-test.mitcad", "name": "Ring nail test",
     "category": "fasteners", "designation": "Ring nail test 3.1x90",
     "build": {"role": "fastener", "wood": {"kind": "nail", "diameter": "3.1 mm", "length": "90 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}]}}'
    TIMBER_WOOD=2
  fi
  cat > "$LIB/mitcad-library.json" <<EOF
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "Synthetic BUILD parts of the timber UI tests.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "dimensioned-timber", "name": "Dimensioned timber"}, {"id": "fasteners", "name": "Fasteners"}],
  "components": [
    {"id": "timber-48-test", "path": "dimensioned-timber/timber-48-test.mitcad", "name": "Timber 48x98 test",
     "category": "dimensioned-timber", "designation": "Timber 48x98 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "98 mm", "height": "48 mm"}, "nominal": {"width": "98 mm", "height": "48 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm",
                  "stock": ["3000 mm", "3600 mm", "4200 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}},
    {"id": "wood-screw-test", "path": "fasteners/wood-screw-test.mitcad", "name": "Wood screw test",
     "category": "fasteners", "designation": "Wood screw test 5x70",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "countersunk", "diameter": "5 mm", "length": "70 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}]}}${nail_entry}
  ]
}
EOF
  g "$LIB" add -A
  g "$LIB" commit -q -m "Test parts"
  g "$LIB" tag v1.0.0
  "$CLI" library fetch "$LIB" > /dev/null || ui_fail "library fetch"
}

# expect_inspect CMD TEXT... DESCRIPTION: an "Inspect CMD:" line since the
# mark has every TEXT.
expect_inspect() {
  local cmd=$1 description=${!#} line text ok
  shift
  for _ in $(seq 1 50); do
    while IFS= read -r line; do
      ok=1
      for text in "${@:1:$#-1}"; do
        [[ $line == *"$text"* ]] || ok=0
      done
      [ $ok = 1 ] && { echo "ok   $description"; return; }
    done < <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect $cmd: ")
    sleep 0.2
  done
  ui_fail "$description: no 'Inspect $cmd:' line since the mark with: ${*:1:$#-1}"
}
# off_view: the mouse out of the view, over the ribbon above its middle
# (window coordinates of the view: "View area x y w h").
off_view() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  xdotool mousemove $((X + (vx + vw / 2) * UI_SCALE)) $((Y + vy / 2 * UI_SCALE))
}
# mid_view: the mouse in the middle of the view.
mid_view() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  [ -n "$area" ] || ui_fail "the app did not log its view area"
  read -r _ _ vx vy vw vh <<< "$area"
  xdotool mousemove $((X + (vx + vw / 2) * UI_SCALE)) $((Y + (vy + vh / 2) * UI_SCALE))
}
# no_inspect CMD DESCRIPTION: a moment after the mouse moved, no "Inspect
# CMD:" line since the mark: the hover runs no placement and no inspection
# (mitcad#508).
no_inspect() {
  sleep 1.5
  ui_sync
  ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Inspect $1: " ||
    ui_fail "$2: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect $1: " | head -1)"
  echo "ok   $2"
}
# last_end_a CMD: where the last "Inspect CMD:" line puts the preview's end
# A ("; end A at x, y, z").
last_end_a() { grep -F "Inspect $1: " "$UI_LOG" | tail -1 | sed -n 's/.*; end A at \([^;]*\);.*/\1/p'; }
# no_ghost CMD TEXT DESCRIPTION: a moment after Place, no "Inspect CMD:
# TEXT" line since the mark: nothing previewed at the default place on top
# of what was just placed (mitcad#513).
no_ghost() {
  sleep 1.5
  ui_sync
  ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Inspect $1: $2" ||
    ui_fail "$3: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect $1: $2" | head -1)"
  echo "ok   $3"
}
# end_a_since CMD: the places of the preview's end A in the "Inspect CMD:"
# lines since the mark, each once.
end_a_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect $1: " | sed -n 's/.*; end A at \([^;]*\);.*/\1/p' | sort -u
}
# moved_once CMD WAS DESCRIPTION: after a click, the "Inspect CMD:" lines
# since the mark put the preview's end A elsewhere than WAS, all at one
# place: the preview moved once, to the click (mitcad#508).
moved_once() {
  local places=""
  for _ in $(seq 1 30); do
    places=$(end_a_since "$1")
    [ -n "$places" ] && break
    sleep 0.2
  done
  # A moment more: a preview still following the mouse would add places.
  sleep 1
  ui_sync
  places=$(end_a_since "$1")
  [ -n "$places" ] || ui_fail "$3: no 'Inspect $1:' line with end A since the mark"
  [ "$(printf '%s\n' "$places" | wc -l)" -eq 1 ] || ui_fail "$3: end A at more than one place: $(tr '\n' ' ' <<< "$places")"
  [ "$places" != "$2" ] || ui_fail "$3: end A still at $2"
  echo "ok   $3: end A at $places"
}

# new_bodies: the bodies made since the mark, separated by spaces.
new_bodies() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^New body .* (\(F[0-9]*\.b[0-9]*\)): volume.*/\1/p' |
    sort -u | tr '\n' ' '
}
# largest_body: the body of the largest volume made since the mark (a
# wall's plate).
largest_body() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n -E 's/^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$/\2 \1/p' |
    sort -g | tail -1 | cut -d' ' -f2
}
# largest_bodies: every body of the largest volume made since the mark (a
# wall's two plates), separated by spaces.
largest_bodies() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n -E 's/^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$/\2 \1/p' |
    sort -g | awk '{ v[NR] = $1; b[NR] = $2 } END { for (i = 1; i <= NR; i++) if (v[i] == v[NR]) print b[i] }' |
    tr '\n' ' '
}
# pickable BODIES: the first of these bodies a click picks a face of now
# (a "Pick face" place after the last "Pick places:"); empty for none.
pickable() {
  local start body
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  for body in $1; do
    if tail -n +"${start:-1}" "$UI_LOG" | grep -q "^Pick face $body/"; then
      echo "$body"
      return
    fi
  done
}

# wall_faces TEXT: "x y" of each face of Wall 1's bodies ($WALL1) whose
# name has TEXT that a click on Start picks now (after the last "Pick
# places:"), sorted left to right.
wall_faces() {
  local start line
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | while IFS= read -r line; do
    if [[ $line =~ ^Pick\ face\ (F[0-9]+\.b[0-9]+)/(.*)\ at\ ([0-9]+),([0-9]+)$ ]] &&
      [[ " $WALL1 " == *" ${BASH_REMATCH[1]} "* && ${BASH_REMATCH[2]} == *"$1"* ]]; then
      echo "${BASH_REMATCH[3]} ${BASH_REMATCH[4]}"
    fi
  done | sort -n
}

# stud_bodies: the 2304 mm studs (98 x 48: 10838016 mm3) made since the
# mark, separated by spaces.
stud_bodies() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n -E 's/^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$/\2 \1/p' |
    awk '$1 > 10838015 && $1 < 10838017 { print $2 }' | sort -u | tr '\n' ' '
}

# stud_faces BODIES: "x y body" of each side face of these bodies a click
# on Start picks now (after the last "Pick places:"), sorted left to right.
stud_faces() {
  local start line
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | while IFS= read -r line; do
    if [[ $line =~ ^Pick\ face\ (F[0-9]+\.b[0-9]+)/(.*)\ at\ ([0-9]+),([0-9]+)$ ]] &&
      [[ " $1 " == *" ${BASH_REMATCH[1]} "* && ${BASH_REMATCH[2]} == *":part(F2:side("* ]]; then
      echo "${BASH_REMATCH[3]} ${BASH_REMATCH[4]} ${BASH_REMATCH[1]}"
    fi
  done | sort -n
}

# stud_face BODIES WHICH: "x y" of a stud's side face (stud_faces): WHICH
# "first" the leftmost face of all (the leftmost stud's front face), "last"
# the leftmost face of the rightmost stud (its front face, not its end's),
# "middle" the rightmost face of the middle stud, the studs ordered by
# their rightmost face (the view from the front right: +X and +Y to the
# right, a stud's +X face right of its -Y face).
stud_face() {
  stud_faces "$1" | awk -v which="$2" '
    { if (!($3 in lo) || $1 < lo[$3]) { lo[$3] = $1; ly[$3] = $2 }
      if (!($3 in hi) || $1 > hi[$3]) { hi[$3] = $1; hy[$3] = $2 } }
    END { for (b in hi) print hi[b], hy[b], lo[b], ly[b] }' | sort -n | awk -v which="$2" '
    { r[NR] = $0 }
    END {
      if (NR == 0) exit
      if (which == "first") { split(r[1], f, " "); print f[3], f[4] }
      else if (which == "last") { split(r[NR], f, " "); print f[3], f[4] }
      else { split(r[int((NR + 1) / 2)], f, " "); print f[1], f[2] } }'
}

# start_picks CMD: the view fitted, the places a click on CMD's Start picks
# logged.
start_picks() {
  ui_step "fit the view (F6)"              ui_key F6
  ui_step "$1's Start"                     ui_click_logged "Panel $1 input start"
}

# click_place "X Y": a click where a pick place was logged.
click_place() {
  local px py
  read -r px py <<< "$1"
  [ -n "$py" ] || ui_fail "no place to click"
  xdotool mousemove $((X + px * UI_SCALE)) $((Y + py * UI_SCALE)) click 1
}

# start_wall: the BUILD tab and Wall (command search) in the app started
# last: its picker's member and the wood screw for its junctions.
start_wall() {
  ui_mark
  ui_step "the BUILD tab"                  ui_click_logged "Ribbon tab BUILD"
  ui_expect_new "Ribbon BUILD/TIMBER at" "its TIMBER group (mitcad#520)"
  ui_mark
  ui_step "Wall (command search)"          ui_command "Wall"
  ui_expect_new "Command Wall started" "Wall started"
  ui_expect_new "BUILD parts timber: 1 (0 in the design)" "Wall's picker: the member"
  ui_expect_new "BUILD parts wood: ${TIMBER_WOOD:-1} (0 in the design)" "the library's wood fasteners for the wall's junctions"
}

# wall_from NAME "X Y" LENGTH TEXT CORNER: a wall started by a click at a
# place on Wall 1, LENGTH mm long (typed): the panel names where it starts
# (TEXT) and the Corner (CORNER), and the wall is placed so.
wall_from() {
  ui_mark
  ui_step "the start on Wall 1"            click_place "$2"
  ui_expect_new "View hint Wall: Click where the wall ends, or press Enter: Length square to the wall clicked" \
    "the end next, else Length square to the wall"
  ui_mark
  ui_step "$3 mm long"                     ui_type_in "Panel Wall input frame_length" "$3"
  expect_inspect Wall "frame wall $1, wall $3 × 2400 mm" "; $1: $4" "; corner $5" "$1 previewed: $4, corner $5"
  ui_mark
  ui_step "Place (Enter)"                  ui_key Return
  ui_expect_new "Placed frame $1 (wall, lead C" "$1 placed" 60
  ui_expect_new "Frame junction: $1: $4" "$1 placed: $4"
}
