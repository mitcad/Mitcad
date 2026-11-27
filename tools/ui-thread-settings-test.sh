#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp app/commands/HoleSizes.hpp
# check-all sources: app/framework/CommandSession.cpp app/framework/RememberedValues.cpp app/framework/Command.hpp
# check-all sources: core/model/src/features/thread.rs tools/cli
# Thread starts from the last thread's settings on a face of the same size
# (mitcad#1298), through the real UI, with MITCAD_REMEMBER_PANEL_VALUES=1
# (the test library turns it off) and settings of the test's own. On a
# block 40 x 20 x 16 (F1, F2: Body1, F2.b0) with two Ø8.5 holes (the tap
# drill of M10x1.5: F4 at (-12, 0), F6 at (0, 0)) and a Ø6.8 one (M8x1.25:
# F8 at (12, 0)):
#   - Nothing kept yet: the first Ø8.5 hole suggests M10x1.5 6H, as
#     before; made Left Hand, Modeled, not Full Length, 12 mm long, OK
#     keeps the panel's settings with Ø8.5 internal.
#   - Thread again, Chamfer Ends checked before the second Ø8.5 hole is
#     picked: the last thread's settings are restored but Chamfer Ends,
#     which stays as the user set it. Cancelled (keeps nothing).
#   - Thread again on the second Ø8.5 hole: the panel shows Left Hand,
#     Modeled, Length 12 mm; OK makes the same thread as the first (the
#     same volume taken out of the block).
#   - Thread on the Ø6.8 hole: the kept settings are not used (Ø6.8
#     differs); the size is suggested (M8x1.25 6H) and the rest are the
#     defaults (a cosmetic thread).
# The Windows counterpart is the Thread part of
# tools/ui-windows-remembered-values-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-thread-settings-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-thread-settings.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
HOLE_A='F4:side(c1)'
HOLE_B='F6:side(c1)'
HOLE_C='F8:side(c1)'
D8='Ø8.5 internal'
D6='Ø6.8 internal'

cat > "$WORK/holes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -10], "width": 40, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 16}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [-12, 0], "diameter": 8.5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [0, 0], "diameter": 8.5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F7", "center": [12, 0], "diameter": 6.8},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}}
]
EOF
"$CLI" run "$WORK/holes.json" --save "$WORK/holes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli holes"; }

# removed "what": REMOVED, the volume the last feature since the mark took
# out of the block ("Body Body1 (F2.b0): volume <before> -> <after> mm3").
# (Not in a subshell, so that ui_fail ends the test.)
removed() {
  local line=""
  for _ in $(seq 1 150); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Body Body1 (F2.b0): volume [0-9.]* -> [0-9.]* mm3" | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$1: no volume logged"
  REMOVED=$(echo "$line" | awk '{ printf "%.6f\n", $5 - $7 }')
}

# restored_line: RESTORED, what the last restore since the mark set
# ("Panel Thread restored: direction=left, modeled=on, ..."), which must
# hold the first thread's Left Hand, Modeled, not Full Length and 12 mm.
restored_line() {
  local setting
  ui_expect_new "Panel Thread restored: " "what was restored logged"
  RESTORED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Panel Thread restored: " | tail -1)
  RESTORED=${RESTORED#*restored: }
  for setting in "direction=left" "modeled=on" "full_length=off" "length=12 mm"; do
    case ", $RESTORED," in
      *", $setting,"*) ;;
      *) ui_fail "$setting not restored: $RESTORED" ;;
    esac
  done
}

# not_new "text" "what": the text is not in the log since the mark.
not_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

ui_start_display
# The kept settings are on for this test; the test library's settings
# folder is the test's own, so nothing is kept when it starts.
export MITCAD_REMEMBER_PANEL_VALUES=1
ui_start_app --open "$WORK/holes.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "=== The first Ø8.5 hole: M10x1.5 6H, Left Hand, Modeled, 12 mm long"
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g, direction=right" \
  "the panel starts from the defaults"
ui_expect_new "Pick face F2.b0/$HOLE_A at" "the first hole can be picked"
ui_mark
ui_step "pick the first Ø8.5 hole"         ui_click_pick "face F2.b0/$HOLE_A"
ui_expect_new "Thread Faces: 1 face [face $HOLE_A of F2.b0" "the hole picked"
ui_expect_new "Preview Thread: ok" "M10x1.5 6H suggested and previewed"
not_new "settings of the last" "nothing kept yet, so nothing restored or compared"
ui_mark
ui_step "Direction: Left Hand"             ui_choose "Panel Thread input direction" 1
ui_expect_new "Thread: Direction = Left Hand" "left-handed"
ui_step "Modeled"                          ui_click_logged "Panel Thread input modeled"
ui_expect_new "Thread: Modeled = on" "modelled"
ui_step "Full Length off"                  ui_click_logged "Panel Thread input full_length"
ui_expect_new "Thread: Full Length = off" "not the full length"
ui_step "length 12"                        ui_type_in "Panel Thread input length" "12"
ui_expect_new "Thread Length: 12 = 12 mm" "12 mm long"
ui_expect_new "Preview Thread: ok" "the modelled thread previewed" 120
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added modeled thread M10x1.5 6H on 1 face(s)" "M10x1.5 6H modelled in the first hole" 120
ui_expect_new "Panel Thread: settings kept with $D8" "its settings kept with the hole's size"
removed "the first thread"
first=$REMOVED
echo "ok   the first thread took $first mm3 out of the block"
ui_wait_idle

echo "=== A setting changed before the pick stays the user's"
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Command Thread started" "Thread started"
ui_expect_new "Pick face F2.b0/$HOLE_B at" "the second hole can be picked"
ui_step "Chamfer Ends"                     ui_click_logged "Panel Thread input chamfer"
ui_expect_new "Thread: Chamfer Ends = on" "chamfered before the pick"
ui_mark
ui_step "pick the second Ø8.5 hole"        ui_click_pick "face F2.b0/$HOLE_B"
ui_expect_new "Thread Faces: 1 face [face $HOLE_B of F2.b0" "the hole picked"
ui_expect_new "Panel Thread: settings of the last $D8 thread restored" "the last thread's settings restored"
ui_expect_new "Panel Thread left as changed: chamfer" "Chamfer Ends left as the user set it"
restored_line
restored=$RESTORED
case "$restored" in
  *"chamfer="*) ui_fail "Chamfer Ends restored: $restored" ;;
esac
echo "ok   restored but Chamfer Ends: $restored"
ui_expect_new "Preview Thread: ok" "the restored thread previewed" 120
ui_focus_main
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Thread cancelled" "cancelled"
not_new "Panel Thread: settings kept" "Cancel keeps nothing"
ui_wait_idle

echo "=== The second Ø8.5 hole starts from the first thread's settings"
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g, direction=right" \
  "the panel starts from the defaults before a face is known"
ui_expect_new "Panel Thread checks: " "its checks logged"
ui_expect_new "Pick face F2.b0/$HOLE_B at" "the second hole can be picked"
ui_mark
ui_step "pick the second Ø8.5 hole"        ui_click_pick "face F2.b0/$HOLE_B"
ui_expect_new "Thread Faces: 1 face [face $HOLE_B of F2.b0" "the hole picked"
ui_expect_new "Panel Thread: settings of the last $D8 thread restored" "the last Ø8.5 thread's settings restored"
restored_line
restored=$RESTORED
echo "ok   Left Hand, Modeled, not Full Length, 12 mm restored: $restored"
ui_expect_new "Thread Length: 12 mm = 12 mm" "the panel's Length is 12 mm"
not_new "Panel Thread left as changed" "nothing changed before the pick"
ui_expect_new "Preview Thread: ok" "the restored thread previewed" 120
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added modeled thread M10x1.5 6H on 1 face(s)" "M10x1.5 6H modelled in the second hole" 120
removed "the second thread"
second=$REMOVED
awk -v a="$first" -v b="$second" 'BEGIN { exit !((a - b) ^ 2 < 1e-6) }' ||
  ui_fail "the second thread took $second mm3, the first $first mm3"
echo "ok   the same thread: $second mm3 taken out, as by the first"
ui_wait_idle

echo "=== The Ø6.8 hole: the size suggested, the rest the defaults"
ui_mark
ui_step "thread (search)"                  ui_command "Thread"
ui_expect_new "Command Thread started" "Thread started"
ui_expect_new "Pick face F2.b0/$HOLE_C at" "the Ø6.8 hole can be picked"
ui_mark
ui_step "pick the Ø6.8 hole"               ui_click_pick "face F2.b0/$HOLE_C"
ui_expect_new "Thread Faces: 1 face [face $HOLE_C of F2.b0" "the hole picked"
ui_expect_new "Panel Thread: settings of the last $D8 thread not used: $D6 differs" "the Ø8.5 settings not used"
not_new "Panel Thread restored" "nothing restored"
ui_expect_new "Preview Thread: ok" "previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added cosmetic thread M8x1.25 6H on 1 face(s)" "M8x1.25 6H suggested, cosmetic by default" 60
ui_expect_new "Panel Thread: settings kept with $D6" "the Ø6.8 thread's settings kept now"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app
export MITCAD_REMEMBER_PANEL_VALUES=0
ui_finish "UI thread settings test"
