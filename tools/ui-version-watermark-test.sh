#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/VersionWatermark.cpp app/view/VersionWatermark.hpp app/MainWindow.cpp
# check-all sources: app/view/ViewController.cpp app/framework/FloatingLayout.cpp app/OcctViewer.cpp
# MitCad's version in the main window (mitcad#1209, app/COMMANDS.md "The
# view", the version watermark), on the demo block:
#   - The watermark's text is "MitCad <version>" (the version --version
#     prints); the window title ends in " - MitCad <version>".
#   - Docked: the watermark sits in the view's lower right (the timeline is
#     a dock below the view). A click at its place reaches the view: the
#     selection (Body1, from the browser) is cleared.
#   - Preferences > General > Show the version in the view (Alt+E) off: the
#     watermark goes at once (the screen at its place changes), the setting
#     is saved, the title keeps the version.
#   - The view's picture (--screenshot, the view's framebuffer) has no
#     watermark: the region where it shows is the same with the preference
#     on and off.
#   - Floating: the watermark sits at the view's right, just above the
#     timeline's card.
# tools/ui-windows-version-watermark-test.ps1 does the same on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-version-watermark-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-watermark.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf

VERSION=$("$UI_APP" --version 2> /dev/null | sed -n 's/^MitCad //p')
[ -n "$VERSION" ] || ui_fail "no version from $UI_APP --version"
TEXT="MitCad $VERSION"

# placed [log]: the watermark's last place in the log (default UI_LOG), as
# "bx by bw bh vx vy vw vh": its box and the view's, in the window's
# coordinates.
placed() {
  grep -F "Version watermark '$TEXT' at " "${1:-$UI_LOG}" | tail -1 |
    sed -n 's/.*: box \(-*[0-9]*\),\(-*[0-9]*\) \([0-9]*\)x\([0-9]*\), view \(-*[0-9]*\),\(-*[0-9]*\) \([0-9]*\)x\([0-9]*\)$/\1 \2 \3 \4 \5 \6 \7 \8/p'
}

# expect_title: the window title ends in the version.
expect_title() {
  local title
  title=$(xdotool getwindowname "$UI_WINDOW" 2> /dev/null)
  [[ $title == *" - MitCad $VERSION" ]] || ui_fail "the window title '$title' does not end in ' - MitCad $VERSION'"
  echo "ok   the window title ends in the version: $title"
}

# inside bx by bw bh vx vy vw vh: the box is inside the view, at most 24
# pixels from its right edge.
inside() {
  [ "$1" -ge "$5" ] && [ "$2" -ge "$6" ] && [ $(($1 + $3)) -le $(($5 + $7)) ] && [ $(($2 + $4)) -le $(($6 + $8)) ] ||
    ui_fail "the watermark ($1,$2 $3x$4) is not inside the view ($5,$6 $7x$8)"
  [ $(($5 + $7 - $1 - $3)) -le 24 ] || ui_fail "the watermark ($1,$2 $3x$4) is not at the view's right edge ($5,$6 $7x$8)"
  [ "$3" -gt 20 ] && [ "$4" -gt 6 ] || ui_fail "the watermark is only $3x$4"
}

ui_start_display

echo "--- 1. Docked: the text, the title and the place"
ui_start_app --demo
ui_expect_log "Version watermark '$TEXT' at " "the watermark shows '$TEXT'"
expect_title
ui_sync
read -r bx by bw bh vx vy vw vh <<< "$(placed)"
[ -n "$vh" ] || ui_fail "no place of the watermark logged"
inside "$bx" "$by" "$bw" "$bh" "$vx" "$vy" "$vw" "$vh"
[ $((vy + vh - by - bh)) -le 24 ] || ui_fail "the watermark ($bx,$by ${bw}x$bh) is not at the view's bottom ($vx,$vy ${vw}x$vh)"
echo "ok   in the view's lower right: $bx,$by ${bw}x$bh in $vx,$vy ${vw}x$vh"

echo "--- 2. A click at its place reaches the view"
ui_mark
ui_step "select Body1 in the browser"    ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Selected: 1 body" "Body1 selected"
ui_mark
ui_step "click the watermark"            ui_click_logged "Version watermark '$TEXT'"
ui_expect_new "Selected: nothing" "the click on the watermark is the view's: the selection cleared"

echo "--- 3. The preference turns it off"
# The pointer away from the watermark's place, which the dumps compare.
xdotool mousemove $((X + 4)) $((Y + 4))
ui_sync
xwd -root -silent > "$WORK/on.xwd" || ui_fail "no screen dump"
ui_mark
ui_step "Preferences: General (search)"  ui_command "Preferences: General"
ui_focus_dialog '^Preferences$'
ui_step "Show the version in the view (Alt+E)" ui_key alt+e
ui_step "OK (Enter)"                     ui_key Return
ui_focus_main
ui_expect_new "Preferences: show the version in the view off" "the preference turned off"
ui_expect_new "Version watermark hidden: preference" "the watermark goes at once"
grep -q '^showVersion=false$' "$SETTINGS" || ui_fail "view/showVersion is not false in the settings"
echo "ok   the setting is saved"
ui_sync
xwd -root -silent > "$WORK/off.xwd" || ui_fail "no screen dump"
json=$(python3 "$STATS" "$WORK/off.xwd" $((X + bx * UI_SCALE)) $((Y + by * UI_SCALE)) $((bw * UI_SCALE)) \
  $((bh * UI_SCALE)) "$WORK/on.xwd") || ui_fail "no statistics of the watermark's place"
changed=$(sed -n 's/.*"changed": \([0-9]*\).*/\1/p' <<< "$json")
[ "${changed:-0}" -gt 10 ] || ui_fail "the screen at the watermark's place did not change: $json"
echo "ok   the watermark was on the screen and is gone ($changed pixels)"
expect_title
ui_stop_app

echo "--- 4. The view's picture has no watermark"
# shot name: the demo block's --screenshot (the view's framebuffer) with
# its log, in the Docked layout as above.
shot() {
  (cd "$WORK" && "$UI_APP" --chrome=docked --no-recovery --demo --screenshot "$WORK/$1.png" > "$WORK/$1.log" 2>&1) ||
    { tail -5 "$WORK/$1.log"; ui_fail "the screenshot run ($1) failed"; }
  file "$WORK/$1.png" | grep -q 'PNG image data, [1-9]' || ui_fail "no screenshot ($1)"
}
shot off
grep -q "^Version watermark hidden: preference" "$WORK/off.log" || ui_fail "the watermark was not off in the run without it"
sed -i 's/^showVersion=false$/showVersion=true/' "$SETTINGS"
shot on
read -r bx by bw bh vx vy vw vh <<< "$(placed "$WORK/on.log")"
[ -n "$vh" ] || ui_fail "the watermark did not show in the run with it"
json=$(python3 "$STATS" --png-changed "$WORK/on.png" "$WORK/off.png" $(((bx - vx) * UI_SCALE)) \
  $(((by - vy) * UI_SCALE)) $((bw * UI_SCALE)) $((bh * UI_SCALE))) || ui_fail "the pictures could not be compared"
pixels=$(sed -n 's/.*"pixels": \([0-9]*\).*/\1/p' <<< "$json")
changed=$(sed -n 's/.*"changed": \([0-9]*\).*/\1/p' <<< "$json")
[ "${pixels:-0}" -gt 0 ] || ui_fail "the watermark's place is outside the picture: $json"
[ "${changed:-1}" = 0 ] || ui_fail "the view's picture differs where the watermark shows: $json"
echo "ok   the picture is the same with and without the watermark at its place ($pixels pixels)"

echo "--- 5. Floating: above the timeline"
ui_start_app --demo --chrome=floating
ui_expect_log "Version watermark '$TEXT' at " "the watermark shows in the Floating layout"
expect_title
ui_sync
read -r bx by bw bh vx vy vw vh <<< "$(placed)"
[ -n "$vh" ] || ui_fail "no place of the watermark logged"
inside "$bx" "$by" "$bw" "$bh" "$vx" "$vy" "$vw" "$vh"
cards=$(grep "^Floating cards: " "$UI_LOG" | tail -1)
read -r tx ty tw th <<< "$(sed -n 's/.*; timelineDock \([0-9]*\),\([0-9]*\) \([0-9]*\)x\([0-9]*\).*/\1 \2 \3 \4/p' <<< "$cards")"
[ -n "$th" ] || ui_fail "no timeline card logged: $cards"
# Above the message band's row too while it shows a message (mitcad#1042).
floor=$ty
band=$(sed -n 's/.*; messageBandRow [0-9]*,\([0-9]*\) .*/\1/p' <<< "$cards")
[ -n "$band" ] && [ "$band" -lt "$floor" ] && floor=$band
[ $((by + bh)) -le "$floor" ] || ui_fail "the watermark ($bx,$by ${bw}x$bh) reaches the timeline or the message band (from y $floor)"
[ $((floor - by - bh)) -le 40 ] || ui_fail "the watermark ($bx,$by ${bw}x$bh) is not just above the timeline or the message band (y $floor)"
[ $((bx + bw)) -le $((tx + tw)) ] || ui_fail "the watermark ($bx,$by ${bw}x$bh) is right of the timeline ($tx,$ty ${tw}x$th)"
echo "ok   just above the timeline: $bx,$by ${bw}x$bh, the timeline at $tx,$ty ${tw}x$th${band:+, the message band row at y $band}"
ui_stop_app

ui_finish "Version watermark test"
