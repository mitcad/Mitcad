#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/DebugLog.cpp app/framework/DebugLog.hpp app/mcp/LogRing.cpp app/mcp/LogRing.hpp
# check-all sources: app/MainWindowDebugLog.cpp app/main.cpp app/view/ViewController.cpp app/files/ProjectIndicator.cpp
# The debug log (mitcad#269) through the real UI:
#   1. --debug-log <file>: the file begins with its header (the version, the
#      build, the system, the graphics, the command line, the design, the
#      switches) and the start-up's lines from before it started; a sketch
#      command's own log lines follow in the file's format (the time in ms,
#      the level, the text); the project indicator says so.
#   2. Help > Open Log Folder: the file's folder (logged, not opened).
#   3. Preferences > Diagnostics, without a restart: the switch off stops
#      the file at once (its last line says so, what follows is not in it);
#      on again starts a new file in the log folder, with the lines kept
#      in memory from before, and the setting is saved.
#   4. The next start writes a log by the setting; the project indicator's
#      Turn Off Debug Log stops it and clears the setting.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-debug-log-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-debug-log.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/run.log
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
unset MITCAD_DEBUG_LOG MITCAD_DEBUG_LOG_DIR

# A line of the file: "2026-10-14 09:30:12.345 debug    text".
TIME='^[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]{3} '

# file_has "regex" description [file]: the file has a matching line (up to
# 3 s: it is written at least once a second).
file_has() {
  local file=${3:-$FILE}
  for _ in $(seq 1 15); do
    grep -qE -- "$1" "$file" 2> /dev/null && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line matching '$1' in $file"
}

# expect_graphics file description: the header's graphics are the renderer
# the 3D view logged ("OpenGL renderer: <name>"; the header has
# "Graphics: <name>, OpenGL <version>"), also when the log started before
# the view (the file waits for them).
expect_graphics() {
  local renderer="" line=""
  for _ in $(seq 1 25); do
    renderer=$(sed -n 's/^OpenGL renderer: //p' "$UI_LOG" | head -1)
    line=$(grep -m1 '^Graphics: ' "$1" 2> /dev/null)
    if [ -n "$renderer" ]; then
      case $line in
        "Graphics: $renderer, OpenGL "?*) echo "ok   $2: $line"; return ;;
      esac
    fi
    sleep 0.2
  done
  ui_fail "$2: '$line', the view's renderer is '$renderer'"
}

# count_in "text" file: how many lines of the file have the text.
count_in() { grep -cF -- "$1" "$2" 2> /dev/null || true; }

# preferences_debug_log description: Preferences on its Diagnostics page,
# Write a debug log toggled (Alt+W), OK.
preferences_debug_log() {
  ui_step "Preferences: Diagnostics (search)" ui_command "Preferences: Diagnostics"
  ui_expect_new "Preferences opened: Diagnostics, " "Preferences on its Diagnostics page"
  ui_focus_dialog '^Preferences$'
  ui_step "$1 (Alt+W)"                     ui_key alt+w
  sleep 0.3 # the check box toggles a moment after its key (animateClick)
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
}

# indicator_choose "entry": the project indicator's menu, an entry picked
# with the arrow keys (from the logged "Project indicator menu: a | b").
indicator_choose() {
  local entries index=0 found="" item
  ui_mark
  ui_step "the indicator's menu"         ui_click_logged "Project indicator"
  ui_expect_new "Project indicator menu: " "the menu" > /dev/null
  entries=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Project indicator menu: " | tail -1 |
    sed 's/.*Project indicator menu: //')
  IFS='|' read -ra items <<< "$entries"
  for item in "${items[@]}"; do
    index=$((index + 1))
    item=${item# }
    item=${item% }
    if [ "${item//…/...}" = "${1//…/...}" ]; then
      found=$index
      break
    fi
  done
  [ -n "$found" ] || ui_fail "'$1' is not in the indicator's menu: $entries"
  xdotool key --delay 80 --repeat "$found" Down
  ui_key Return
  ui_sync
}

ui_start_display

echo "--- --debug-log <file>: the header and the start-up"
ui_start_app --debug-log "$FILE"
ui_expect_log "Debug log on: $FILE (--debug-log)" "the debug log started"
[ -f "$FILE" ] || ui_fail "no file $FILE"
file_has '^MitCad debug log$' "the header, once the view knows its graphics"
[ "$(head -1 "$FILE")" = "MitCad debug log" ] || ui_fail "the file does not begin with its header: $(head -1 "$FILE")"
echo "ok   the file begins with its header"
file_has '^Version: MitCad [0-9]' "the version"
file_has '^Build: (release|debug), ' "the build"
file_has '^System: ' "the system"
expect_graphics "$FILE" "the GPU and renderer in the header of a log started before the view"
file_has "^Command line: .*--debug-log .*run\\.log" "the command line"
file_has '^Design: none' "no design at the start"
file_has '^Switches: debug log \(--debug-log\), .*MITCAD_LOG_PICKS=1' "the switches"
file_has '^Started: .*, file 1$' "the start"
file_has '^--- The newest [0-9]+ lines from before the debug log started:$' "the lines from before"
file_has "${TIME}info +Style: " "the start-up's lines from before it started"
file_has "${TIME}info +Debug log on: .*run\\.log \\(--debug-log\\)$" "its own first line"
file_has "${TIME}info +OpenGL renderer: " "the renderer, once the view started"
ui_expect_log "Project indicator: " "the project indicator"
grep "Project indicator: " "$UI_LOG" | tail -1 | grep -qF "debug log on" ||
  ui_fail "the project indicator does not say debug log on: $(grep "Project indicator: " "$UI_LOG" | tail -1)"
echo "ok   the project indicator says debug log on"

echo "--- A sketch command's lines in the file"
ui_create_sketch
file_has "${TIME}debug +Create Sketch: select a plane$" "Create Sketch's own line"
file_has "${TIME}debug +Sketch started on xy, origin " "the sketch started"
ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
file_has "${TIME}debug +Sketch finished$" "the sketch finished"

echo "--- Help > Open Log Folder"
ui_mark
ui_step "Open Log Folder (search)"       ui_command "Open Log Folder"
ui_expect_new "Open Log Folder: $WORK" "the log file's folder"
ui_expect_new "Open URL (test, not opened): file://$WORK" "the folder opened"

echo "--- Preferences > Diagnostics: off at once"
ui_mark
preferences_debug_log "Write a debug log off"
ui_expect_new "Preferences: debug log off" "the switch turned off"
ui_expect_new "Debug log off: $FILE" "the log stopped"
tail -1 "$FILE" | grep -qE "${TIME}info +Debug log off: .*run\\.log$" ||
  ui_fail "the file's last line is not its end: $(tail -1 "$FILE")"
echo "ok   the file's last line says it stopped"
grep "Project indicator: " "$UI_LOG" | tail -1 | grep -qF "debug log on" &&
  ui_fail "the project indicator still says debug log on"
echo "ok   the project indicator no longer says it"
grep -q '^debugLog=' "$CONF" && ui_fail "the switch off left a setting"
echo "ok   no setting"
ui_create_sketch
[ "$(count_in "Sketch started on xy" "$FILE")" = 1 ] || ui_fail "a sketch started after the stop is in the file"
echo "ok   nothing after the stop in the file"

echo "--- On again: a new file in the log folder, without a restart"
ui_mark
preferences_debug_log "Write a debug log on"
ui_expect_new "Preferences: debug log on" "the switch turned on"
ui_expect_new "Debug log on: " "a new debug log"
NEW=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Debug log on: \(.*\) (Preferences)$/\1/p' | tail -1)
case $NEW in
  "$XDG_DATA_HOME"/*/logs/mitcad-[0-9]*-[0-9]*-[0-9]*.log) echo "ok   in the log folder: $NEW" ;;
  *) ui_fail "the new log is not mitcad-<date>-<time>-<pid>.log in the log folder: '$NEW'" ;;
esac
[ "$(head -1 "$NEW")" = "MitCad debug log" ] || ui_fail "the new file does not begin with its header"
file_has '^Switches: debug log \(Preferences\)' "switched on in Preferences" "$NEW"
expect_graphics "$NEW" "the GPU and renderer in the header"
file_has '^--- The newest [0-9]+ lines from before the debug log started:$' "the lines kept from before" "$NEW"
[ "$(count_in "Sketch started on xy" "$NEW")" = 2 ] ||
  ui_fail "the sketches started before are not in the lines kept from before"
echo "ok   the sketch started while it was off is in the new file"
grep -q '^debugLog=true' "$CONF" || ui_fail "the switch is not saved"
echo "ok   the switch saved"
ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_mark
ui_step "Open Log Folder (search)"       ui_command "Open Log Folder"
ui_expect_new "Open Log Folder: $(dirname "$NEW")" "the log folder"
file_has "${TIME}info +Open Log Folder: $(dirname "$NEW")$" "a new line in the new file" "$NEW"
ui_stop_app

echo "--- The next start: on by the setting; off in the project indicator"
ui_start_app
ui_expect_log "Debug log on: " "the setting starts the log"
grep -q "Debug log on: .* (Preferences)$" "$UI_LOG" || ui_fail "not started by the setting"
LATEST=$(sed -n 's/^Debug log on: \(.*\) (Preferences)$/\1/p' "$UI_LOG" | tail -1)
[ "$LATEST" != "$NEW" ] || ui_fail "the next start wrote into the same file"
echo "ok   a file of its own: $LATEST"
indicator_choose "Turn Off Debug Log"
ui_expect_new "Debug log turned off from the project indicator" "turned off in the indicator"
ui_expect_new "Debug log off: $LATEST" "the log stopped"
grep -q '^debugLog=true' "$CONF" && ui_fail "the setting stayed on"
echo "ok   the setting cleared"

ui_finish "UI debug log test"
