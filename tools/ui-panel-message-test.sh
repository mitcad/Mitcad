#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework tools/cli
# A command panel's errors in its narrow page (mitcad#198), through the
# real UI, in the Docked and the Floating layout, editing a 20 mm cube's
# Extrude:
#   - a long unknown name typed as its distance: the note under the field
#     has a word far wider than the panel, and the panel stays as wide as
#     its page (wider, its right side with the fields and OK and Cancel
#     would be cut off);
#   - made Symmetric, a distance of -5: the preview's error in the
#     message, within the page's width; 10: the message hidden, its room
#     given back; -5 again: the error shown again. (A distance of 0 only
#     waits for a distance, as a hint, mitcad#2481.)
# Long words in the message itself and many lines of errors, bounded and
# scrolling, are in the unit tests (app/tests/command_message.cpp).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-message-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-panel-message.XXXXXX)
FILE=$WORK/cube.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Letters only, so that the expression is one name.
LONG=anUnknownParameterWhoseNameIsFarWiderThanTheCommandPanelAndHasNoSpaceInItAnywhereAtAll

cat > "$WORK/cube.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cube.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# fit_after text [pattern]: FIT is the last line about the panel's width
# ("Panel Extrude width W of P" from its layout, "Panel Extrude message
# <shown>, panel width W of P" when its message or a note changed; or what
# the pattern matches) after the last line with the text, waited for.
FIT_LINE="Panel Extrude .*width [0-9]* of [0-9]*"
MESSAGE_LINE="Panel Extrude message .*width [0-9]* of [0-9]*"
fit_after() {
  local from
  FIT=""
  for _ in $(seq 1 30); do
    from=$(grep -nF -- "$1" "$UI_LOG" | tail -1 | cut -d: -f1)
    [ -n "$from" ] && FIT=$(tail -n +$((from + 1)) "$UI_LOG" | grep -- "${2:-$FIT_LINE}" | tail -1)
    [ -n "$FIT" ] && return 0
    sleep 0.2
  done
  ui_fail "no width of the panel logged after '$1'"
}

# panel_fits description: the panel in FIT is not wider than its page (the
# scroll area's viewport, which never scrolls sideways).
panel_fits() {
  local width page
  read -r width page <<< "$(sed 's/.*width \([0-9]*\) of \([0-9]*\).*/\1 \2/' <<< "$FIT")"
  [ -n "$width" ] && [ -n "$page" ] || ui_fail "$1: no width in '$FIT'"
  [ "$width" -le "$page" ] || ui_fail "$1: the panel is $width px wide, its page $page px"
  echo "ok   $1 ($width of $page px)"
}

# message_is shown|hidden description: what FIT says of the message.
message_is() {
  case "$1:$FIT" in
  shown:*"message hidden"* | hidden:*"message "[0-9]*) ui_fail "$2: '$FIT'" ;;
  *"message "*) echo "ok   $2" ;;
  *) ui_fail "$2: no message in '$FIT'" ;;
  esac
}

edit_extrude() {
  echo "--- $1"
  ui_step "fit (F6)"                         ui_key F6
  ui_expect_log "Timeline Extrude1 at" "the timeline is logged"
  ui_double_click_expect "Timeline Extrude1" "Editing F2 with Extrude" "editing Extrude1"
  ui_expect_new "Panel Extrude input distance at" "its panel"
  fit_after "Editing F2 with Extrude"
  panel_fits "$1: the panel as it opens"
  ui_wait_idle

  # Symmetric, so that a negative distance is the model's refusal (a
  # distance of 0 only waits for a distance, as a hint, mitcad#2481).
  ui_mark
  ui_step "Symmetric"                        ui_choose_text "Panel Extrude input direction" "Symmetric"
  ui_expect_new "Extrude: Direction = Symmetric" "Symmetric chosen"
  ui_expect_new "Preview Extrude: ok" "the symmetric 20 mm previewed"
  ui_wait_idle

  ui_mark
  ui_step "a long unknown name"              ui_type_in "Panel Extrude input distance" "$LONG"
  ui_expect_new "Extrude Distance: $LONG is invalid" "the name is refused, under the field"
  fit_after "Extrude Distance: $LONG is invalid"
  panel_fits "$1: a long name in the note"

  ui_mark
  ui_step "distance -5"                      ui_type_in "Panel Extrude input distance" "-5"
  ui_expect_new "Preview Extrude: failed" "a symmetric distance of -5 fails"
  fit_after "Preview Extrude: failed" "$MESSAGE_LINE"
  message_is shown "the preview's error in the message"
  panel_fits "$1: the preview's error"

  ui_mark
  ui_step "distance 10"                      ui_type_in "Panel Extrude input distance" "10"
  ui_expect_new "Preview Extrude: ok" "10 mm previewed"
  ui_expect_new "Panel Extrude message hidden" "the error gone, its room given back"
  FIT=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -- "$MESSAGE_LINE" | tail -1)
  message_is hidden "no message after the error"
  panel_fits "$1: no message"

  ui_mark
  ui_step "distance -5 again"                ui_type_in "Panel Extrude input distance" "-5"
  ui_expect_new "Preview Extrude: failed" "-5 fails again"
  fit_after "Preview Extrude: failed" "$MESSAGE_LINE"
  message_is shown "the error shown again"
  panel_fits "$1: the error again"

  ui_mark
  ui_step "cancel (Esc)"                     ui_key Escape
  ui_expect_new "Command Extrude cancelled" "the edit cancelled"
}

ui_start_display
ui_start_app --open "$FILE"
edit_extrude "Docked"
ui_stop_app
ui_start_app --open "$FILE" --chrome=floating
edit_extrude "Floating"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI panel message test"
