#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework app/view tools/cli
# The Browser's Rows background in the Floating layout (mitcad#419), through
# the real UI, on a 200 x 200 mm plate zoomed in until it fills the view:
#   - Floating by default (mitcad#429); the Browser starts on its card (set
#     in the settings: Rows is the default on Linux, mitcad#429).
#   - Preferences > Display > Browser background: Rows, applied at once and
#     saved (appearance/browserBackground); a place beside the rows, inside
#     the card's old rectangle, is logged.
#   - A click there selects the plate's face behind the rows; a click on a
#     row still selects its body; a middle drag beside the rows orbits.
#   - The card follows its rows (mitcad#425; 30 sketches more, Sketches
#     folded at the start, no Projects card): folding Bodies shrinks it,
#     opening it grows it, and the rows do not scroll while it ends above
#     the timeline; opening Sketches grows it down to the top of the
#     message band's row above the timeline (mitcad#1042; the timeline's
#     top without a message) less the gap, the rows scroll, and the
#     scroll bar is at the card's left, left of the rows.
#   - Back to Card at once: the place beside the rows is gone, and a click
#     there is the card's again (the plate is not picked).
# The Windows counterpart is tools/ui-windows-browser-rows-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-rows-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-browser-rows.XXXXXX)
FILE=$WORK/plate.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0); and 30
# empty sketches more, so that the Browser's rows overflow with Sketches
# open (mitcad#425).
{
  cat << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
JSON
  for _ in $(seq 1 30); do echo '  , {"cmd": "sketch.create"}'; done
  echo ']'
} > "$WORK/plate.json"
"$CLI" run "$WORK/plate.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The middle button orbits (the drag beside the rows). No Projects card:
# the Browser's card starts below the floating rows and has the room down
# to the timeline. The Browser on its card, for the choice of Rows (the
# default on Linux, mitcad#429).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[view]\nnavigationScheme=middle-orbit\n[projects]\npanel=false\n[appearance]\nbrowserBackground=card\n' \
  > "$UI_CONFIG/Mitcad/Mitcad.conf"

# count_new "text": how many lines logged since the mark contain the text.
count_new() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -- "$1"; }

# arrow "path": a click on the row's disclosure arrow.
arrow() {
  # Apart from a click there a moment ago (Bodies folds and opens in turn),
  # which Qt would make a double-click.
  local at
  at=$(ui_logged_at "Browser arrow $1") || exit 1
  ui_apart $at
  xdotool mousemove $at click 1
}

# set_background card|rows Up|Down: Preferences > Display, the Browser
# background one entry up or down (Alt+B), OK.
set_background() {
  ui_mark
  ui_step "Preferences: Display (search)"  ui_command "Preferences: Display"
  ui_expect_new "Preferences browser background at" "Display has the Browser background"
  ui_focus_dialog '^Preferences$'
  ui_step "Browser background: $1"         ui_key alt+b "$2"
  ui_step "OK"                             ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: browser background $1" "the choice is saved"
  ui_expect_new "Browser background: $1" "the Browser follows at once"
  grep -q "^browserBackground=$1" "$UI_CONFIG/Mitcad/Mitcad.conf" ||
    ui_fail "appearance/browserBackground is not $1 in the settings"
  echo "ok   the setting is $1"
}

# card_rect name: "x y width height" of the card in the last "Floating
# cards:" line (window coordinates).
card_rect() {
  grep '^Floating cards: ' "$UI_LOG" | tail -1 | tr ';' '\n' | sed 's/^ *//' | grep "^$1 " |
    sed -n 's/^[^ ]* \(-\{0,1\}[0-9]*\),\(-\{0,1\}[0-9]*\) \([0-9]*\)x\([0-9]*\)$/\1 \2 \3 \4/p'
}

# column_floor: where the left column ends, plus the gap: the top of the
# message band's row (mitcad#1042), or of the timeline while no message
# was shown.
column_floor() {
  local top
  read -r _ top _ _ <<< "$(card_rect messageBandRow)"
  [ -n "$top" ] || read -r _ top _ _ <<< "$(card_rect timelineDock)"
  echo "$top"
}

# browser_height_becomes lt|gt height description: waits until the
# Browser card's height is less or greater than the height.
browser_height_becomes() {
  local x y w h
  for _ in $(seq 1 25); do
    read -r x y w h <<< "$(card_rect browserDock)"
    [ -n "$h" ] && [ "$h" -"$1" "$2" ] && { echo "ok   $3 ($2 -> $h)"; return 0; }
    sleep 0.2
  done
  ui_fail "$3: the Browser card is $h high, not $1 $2"
}

# scroll_range: the rows' last logged vertical scroll range.
scroll_range() { grep -o '^Browser scroll range [0-9]*$' "$UI_LOG" | tail -1 | grep -o '[0-9]*$'; }

ui_start_display
# The default layout: Floating (mitcad#429).
UI_DEFAULT_CHROME=1 ui_start_app --open "$FILE"
ui_expect_log "Chrome: floating (the platform's default)" "Floating by default"
ui_expect_log "Opened $FILE" "the plate is open"
ui_expect_log "Browser background: card" "the Browser starts on its card (the setting)"
# Its 30 sketches do not fit: the design opens folded below the root
# (mitcad#426); Bodies opened again for Body1's row.
ui_expect_log "Browser opened folded: " "the design opens folded"
# Sketch1's row is not in sight: "out of sight" when the rows were logged
# open before the fold, never logged when the fold came before the
# panel's first layout (which of the two depends on the start's timing).
ui_sync
for _ in $(seq 1 30); do
  ui_place_valid "Browser Root/Sketches/Sketch1" || break
  sleep 0.2
done
ui_place_valid "Browser Root/Sketches/Sketch1" && ui_fail "Sketch1's row is still in sight after the fold"
ui_place_valid "Browser Root/Sketches" || ui_fail "the Sketches folder is not in sight"
echo "ok   the sketches folded away"
ui_expect_log "Browser arrow Root/Bodies at" "the Bodies folder's arrow"
ui_mark
ui_step "open Bodies"                    arrow "Root/Bodies"
ui_expect_new "Browser Root/Bodies/Body1 at" "Body1's row"

echo "--- The plate fills the view"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
# Zoomed in about the plate's middle (the body's logged place), which stays
# put: the plate grows on every side and fills the view, also behind the
# Browser. About the view's middle it grows mostly away from the Browser:
# the plate sits in the free area right of the Browser's card.
ui_sync
ui_step "zoom in at the plate's middle"  xdotool mousemove $(ui_logged_at "Body F2.b0") click --repeat 14 --delay 60 4
ui_wait_idle

echo "--- Rows"
set_background rows Down
ui_expect_log "Browser view beside the rows at" "a place beside the rows is logged"
read -r BX BY <<< "$(ui_logged_at "Browser view beside the rows")"
ui_mark
ui_step "click beside the rows"          ui_click_logged "Browser view beside the rows"
ui_expect_new "Selected: 1 face [face F2:" "the click reached the view: the plate's face behind the rows"
ui_mark
ui_step "click Body1's row"              ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Selected: 1 body [body F2.b0]" "a row's box still takes the click"
[ -n "$SHOT" ] && ui_capture "$SHOT"

ui_mark
read -r BX BY <<< "$(ui_logged_at "Browser view beside the rows")"
ui_apart "$BX" "$BY"
xdotool mousemove "$BX" "$BY" mousedown 2
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove $((BX + 15 * i)) $((BY + 8 * i))
  sleep 0.05
done
xdotool mouseup 2
ui_sync
ui_expect_new "Camera direction" "a middle drag beside the rows orbits" 8
last=$(grep "Camera direction" "$UI_LOG" | tail -1)
case "$last" in *"direction 0 0 -1 "*) ui_fail "the drag did not turn the view: $last" ;; esac
echo "ok   orbited to: ${last#*Camera }"
[ "$(count_new "Selected: ")" = 0 ] || ui_fail "the drag selected something"
echo "ok   the drag selected nothing"

echo "--- The card grows with its rows, then scrolls; the scroll bar at the left (mitcad#425)"
read -r _ _ _ H0 <<< "$(card_rect browserDock)"
ui_mark
ui_step "fold Bodies"                    arrow "Root/Bodies"
ui_expect_new "Browser Root/Bodies/Body1 out of sight" "Body1 folded away"
browser_height_becomes lt "$H0" "the card shrinks as a row folds"
read -r _ _ _ H1 <<< "$(card_rect browserDock)"
ui_mark
ui_step "open Bodies"                    arrow "Root/Bodies"
ui_expect_new "Browser Root/Bodies/Body1 at" "Body1 in sight again"
browser_height_becomes gt "$H1" "the card grows as a row opens"
read -r _ BTOP _ BH <<< "$(card_rect browserDock)"
TTOP=$(column_floor)
[ $((BTOP + BH)) -lt $((TTOP - 8)) ] || ui_fail "the card reaches the timeline ($BTOP + $BH, timeline at $TTOP)"
[ "$(scroll_range)" = 0 ] || ui_fail "the rows scroll ($(scroll_range)) before the card reaches the timeline"
ui_place_valid "Browser scroll bar" && ui_fail "a scroll bar before the card reaches the timeline"
echo "ok   no scroll bar while the rows fit"
ui_mark
ui_step "open Sketches"                  arrow "Root/Sketches"
ui_expect_new "Browser Root/Sketches/Sketch1 at" "the sketches in sight"
for _ in $(seq 1 25); do
  read -r _ BTOP _ BH <<< "$(card_rect browserDock)"
  TTOP=$(column_floor)
  [ $((BTOP + BH)) = $((TTOP - 8)) ] && break
  sleep 0.2
done
[ $((BTOP + BH)) = $((TTOP - 8)) ] ||
  ui_fail "the card ends at $((BTOP + BH)), not at the timeline's top less the gap $((TTOP - 8))"
echo "ok   the card grew down to the timeline ($((BTOP + BH)), the timeline at $TTOP)"
ui_expect_new "Browser scroll range " "the rows' range logged"
for _ in $(seq 1 25); do
  [ "$(scroll_range)" -gt 0 ] && ui_place_valid "Browser scroll bar" && break
  sleep 0.2
done
[ "$(scroll_range)" -gt 0 ] || ui_fail "the rows do not scroll"
ui_place_valid "Browser scroll bar" || ui_fail "no scroll bar once the rows overflow"
read -r SX _ <<< "$(ui_logged_at "Browser scroll bar")"
read -r RX _ <<< "$(ui_logged_at "Browser Root")"
[ "$SX" -lt "$RX" ] || ui_fail "the scroll bar at x $SX is not left of the rows (Root at $RX)"
echo "ok   the rows scroll ($(scroll_range)); the scroll bar at the card's left ($SX, Root's name at $RX)"

echo "--- Back to Card"
set_background card Up
ui_expect_new "Browser view beside the rows out of sight" "the place beside the rows is gone"
ui_mark
ui_step "click where the rows left room" xdotool mousemove "$BX" "$BY" click 1
sleep 0.5
# The card's tree may select the row there; the plate behind is not picked.
[ "$(count_new "Selected: 1 face")" = 0 ] || ui_fail "the click went past the card to the view"
echo "ok   the card takes the click again"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI browser rows test"
