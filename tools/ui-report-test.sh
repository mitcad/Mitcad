#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Feedback and error reports (mitcad#61, mitcad#62) through the real UI.
# No browser opens: MITCAD_TEST_LOG_URLS=1 (ui-test-lib.sh) logs the
# prefilled issue form's address instead, and the test reads it.
#   1. Help > Send Feedback (from the command search): the form, a cropped
#      screenshot of the window, the preview with the home folder and the
#      computer's name masked, a section left out and one edited there;
#      Send opens the tracker of the settings (reports/issueUrl) with the
#      title and the body as edited, and saves the screenshot as cropped.
#   2. An internal error: a crash inside the geometry kernel turned into an
#      error (MITCAD_TEST_OCCT_CRASH=fillet, a design with a fillet) is
#      offered at once; the preview has the error and the recent actions;
#      Send opens the form with the duplicate key.
#   3. A crash of the application (MITCAD_TEST_CRASH=model-worker: on the
#      model's worker thread, where OCCT's handlers are installed): the
#      process dies with SIGSEGV and leaves a crash report; the next start
#      offers it, the preview has the stack with the crashing function;
#      Send opens the form; the report is marked offered.
#   4. A crash of the import worker (MITCAD_TEST_CRASH=import-worker): the
#      import fails, the report is offered at once (a worker of this run),
#      Cancel in the preview sends nothing; "Do not offer error reports
#      again" turns the offers off in the settings.
#   5. With the offers off, a crash report found at the start is marked
#      offered without a question.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-report-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d /tmp/mitcad-ui-report.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# The crashes are on purpose: no core files.
ulimit -c 0
CRASHES=$XDG_DATA_HOME/Mitcad/Mitcad/crashes
REPORTS=$XDG_DATA_HOME/Mitcad/Mitcad/reports
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
TRACKER=https://tracker.test/mitcad/issues
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[reports]\nissueUrl=%s\n' "$TRACKER" > "$SETTINGS"

# The crashes and kernel errors here are wanted: the app counts as crashed
# only when it is gone or gdb stopped it.
ui_crashed() { grep -q "^Program received signal" "$UI_LOG" || ! kill -0 "$UI_RUNNER" 2> /dev/null; }

# last_url: the last address the app would have opened.
last_url() { grep -o 'Open URL (test, not opened): .*' "$UI_LOG" | tail -1 | sed 's/^Open URL (test, not opened): //'; }
# url_part url title|body: a query parameter, decoded.
url_part() {
  python3 -c 'import sys, urllib.parse as u; print(u.parse_qs(u.urlsplit(sys.argv[1]).query)[sys.argv[2]][0])' "$1" "$2"
}
# expect_in text needle description / expect_not_in text needle description
expect_in() {
  grep -qF -- "$2" <<< "$1" || ui_fail "$3: '$2' missing"
  echo "ok   $3"
}
expect_not_in() {
  if grep -qF -- "$2" <<< "$1"; then ui_fail "$3: '$2' is there"; fi
  echo "ok   $3"
}
# wait_url count: waits until the app logged that many addresses.
wait_url() {
  for _ in $(seq 1 50); do
    [ "$(grep -c 'Open URL (test, not opened): ' "$UI_LOG")" -ge "$1" ] && return 0
    sleep 0.2
  done
  ui_fail "no issue form address logged"
}
# png_size file: "width height" of a PNG file.
png_size() { python3 -c 'import struct, sys; d=open(sys.argv[1],"rb").read(24); print(*struct.unpack(">II", d[16:24]))' "$1"; }

ui_start_display

echo "--- 1. Help > Send Feedback"
ui_start_app --demo
SECRET="$HOME/designs/secret-bracket.f3d"
HOST=$(hostname)
ui_mark
ui_step "Send Feedback (command search)"   ui_command "Send Feedback"
ui_focus_dialog '^Send Feedback$'
ui_expect_new "Send Feedback: the form shows" "the form shows"
ui_step "summary"                          xdotool type --delay 20 "Fillet fails"
ui_step "description"                      eval 'ui_key Tab; xdotool type --delay 10 "It failed on $SECRET at $HOST."'
ui_step "contact"                          eval 'ui_key Tab; xdotool type --delay 10 "tester@example.org"'
ui_step "the screenshot on"                ui_key Tab Tab space
ui_expect_new "Feedback screenshot 1280x800" "the screenshot of the window"
# Crop: a drag over the right half of the image.
read -r IX IY <<< "$(ui_logged_at "Feedback screenshot image")"
read -r _ _ _ _ IW IH <<< "$(grep -o 'Feedback screenshot image size [0-9]* [0-9]*' "$UI_LOG" | tail -1)"
ui_step "crop the screenshot"              eval 'xdotool mousemove $((IX + 10)) $((IY - IH / 4)) mousedown 1;
  for i in 1 2 3 4 5; do xdotool mousemove $((IX + 10 + i * (IW / 2 - 20) / 5)) $((IY - IH / 4 + i * (IH / 2) / 5)); sleep 0.05; done;
  xdotool mouseup 1'
ui_expect_new "Report screenshot cropped to" "the screenshot cropped"
CROP=$(grep -o 'Report screenshot cropped to [0-9]*x[0-9]*' "$UI_LOG" | tail -1 | sed 's/.* //')
ui_step "Preview (Enter)"                  ui_key Return
ui_focus_dialog '^Review Report$'
ui_expect_new "Feedback form: kind bug, summary 'Fillet fails'" "the form's content"
ui_expect_new "Report preview: Bug: Fillet fails" "the preview's title"
ui_expect_new "Report section diagnostics: Mitcad " "the diagnostics in the preview"
ui_expect_new "Report section contact: tester@example.org" "the contact address, not masked"
ui_expect_new "Report section screenshot: $CROP" "the cropped screenshot in the preview"
PREVIEW=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Report section description: ')
expect_in "$PREVIEW" "It failed on <path>/<file>.f3d at" "the design's path masked in the preview"
expect_not_in "$PREVIEW" "secret-bracket" "the design's name masked"
expect_not_in "$PREVIEW" "$HOME" "the home folder masked"
if [ ${#HOST} -ge 3 ] && [ "$HOST" != localhost ]; then
  expect_in "$PREVIEW" "at <host>." "the computer's name masked"
fi
ui_step "leave out the diagnostics"        ui_click_logged "Report section diagnostics"
ui_expect_new "Report section diagnostics left out" "the diagnostics left out"
ui_step "edit the description"             eval 'ui_click_logged "Report text description"; ui_key ctrl+End;
  xdotool type --delay 10 " Edited in the preview."'
ui_step "Send (Alt+S)"                     ui_key alt+s
ui_expect_new "Report sent: 'Bug: Fillet fails', sections description, contact" "sent without the diagnostics"
ui_expect_new "Report screenshot saved: $REPORTS/screenshot-" "the screenshot saved"
wait_url 1
URL=$(last_url)
expect_in "$URL" "$TRACKER/new?title=Bug%3A%20Fillet%20fails&body=" "the tracker of the settings, the title"
BODY=$(url_part "$URL" body)
expect_in "$BODY" "### Description" "the description in the body"
expect_in "$BODY" "It failed on <path>/<file>.f3d at" "the masked path in the body"
expect_in "$BODY" "Edited in the preview." "the preview's edit in the body"
expect_in "$BODY" "### Contact" "the contact in the body"
expect_in "$BODY" "tester@example.org" "the contact address in the body"
expect_not_in "$BODY" "### Diagnostics" "no diagnostics in the body"
expect_not_in "$BODY" "secret-bracket" "no design name in the body"
expect_not_in "$URL" "$(python3 -c 'import sys, urllib.parse as u; print(u.quote(sys.argv[1], safe=""))' "$HOME")" \
  "no home folder in the address"
SHOT=$(grep -o "Report screenshot saved: [^ ]*" "$UI_LOG" | tail -1 | sed 's/^Report screenshot saved: //')
[ -f "$SHOT" ] || ui_fail "the screenshot file $SHOT is missing"
[ "$(png_size "$SHOT" | tr ' ' x)" = "$CROP" ] || ui_fail "the screenshot is $(png_size "$SHOT"), not $CROP"
echo "ok   the screenshot file is the cropped one ($CROP)"
ui_focus_dialog '^Report$'
ui_step "the note on the screenshot (Enter)" ui_key Return
ui_focus_main
ui_stop_app

echo "--- 2. An internal error: a crash in the geometry kernel"
# Without gdb, which would stop at the crash that OCCT catches.
UI_GDB=0 MITCAD_TEST_OCCT_CRASH=fillet ui_start_app --open "$ROOT/tools/cli/tests/v1_boss.mitcad"
ui_expect_log "Internal error (geometry kernel): " "the kernel's crash is an internal error"
ui_focus_dialog '^Error Report$'
ui_expect_log "Error report offered (error): An internal error occurred:" "the report offered at once"
KEY=$(grep -o 'Error report offered (error): .*; key mitcad-[0-9a-f]*' "$UI_LOG" | tail -1 | sed 's/.*key mitcad-//')
[ ${#KEY} = 12 ] || ui_fail "no duplicate key: '$KEY'"
ui_mark
ui_step "Review Report (Enter)"            ui_key Return
ui_focus_dialog '^Review Report$'
ui_expect_new "Report section error: Internal error in the geometry kernel: | " "the error in the preview"
ui_expect_new "SIGSEGV 'segmentation violation' detected" "the kernel's message"
ui_expect_new "Report section actions: " "the recent actions"
ui_expect_new "Report key: mitcad-$KEY" "the duplicate key in the preview"
ui_step "Send (Alt+S)"                     ui_key alt+s
wait_url 1
URL=$(last_url)
expect_in "$(url_part "$URL" title)" "Internal error: " "the title"
expect_in "$(url_part "$URL" title)" "[mitcad-$KEY]" "the key in the title"
BODY=$(url_part "$URL" body)
expect_in "$BODY" "duplicate key \`mitcad-$KEY\`" "the key in the body"
expect_in "$BODY" "### Recent actions" "the recent actions in the body"
expect_in "$BODY" "### Diagnostics" "the diagnostics in the body"
ui_focus_main
ui_stop_app

echo "--- 3. A crash of the application, offered at the next start"
UI_LOG_BEFORE=$UI_LOG
UI_LOG=$WORK/crash.log
MITCAD_TEST_CRASH=model-worker timeout 120 "$UI_APP" --no-recovery --demo > "$UI_LOG" 2>&1
STATUS=$?
[ "$STATUS" = 139 ] || { cat "$UI_LOG"; ui_fail "the app ended with $STATUS, not SIGSEGV (139)"; }
echo "ok   the app crashed (SIGSEGV)"
ui_expect_log "Mitcad crashed: SIGSEGV; crash report: $CRASHES/crash-" "the crash report written"
REPORT=$(ls "$CRASHES"/*.crash)
grep -q '^process: app$' "$REPORT" || ui_fail "the report's process"
grep -q 'mitcad5crash8crashNow' "$REPORT" || ui_fail "the crashing function is not in the report's stack"
grep -q 'dispatch_occt_signal' "$REPORT" || ui_fail "the crash did not come through OCCT's handlers"
echo "ok   the report: the application, the stack through OCCT's handlers to the crash"
UI_LOG=$UI_LOG_BEFORE
: > "$UI_LOG"
ui_start_app --demo
ui_focus_dialog '^Error Report$'
ui_expect_log "Error report offered (crash): Mitcad quit unexpectedly the last time it ran (SIGSEGV)." \
  "the crash offered at the next start"
ui_mark
ui_step "Review Report (Enter)"            ui_key Return
ui_focus_dialog '^Review Report$'
ui_expect_new "Report preview: Crash: SIGSEGV in mitcad::crash::crashNow [mitcad-" "the title names the crash"
ui_expect_new "Report section stack: " "the stack in the preview"
ui_expect_new "mitcad: mitcad::crash::crashNow() + 0x" "the crashing function in the stack"
ui_expect_new "Report section error: Mitcad " "what happened"
expect_not_in "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Report section stack: ')" "$ROOT" \
  "the build folder masked in the stack"
ui_step "Send (Alt+S)"                     ui_key alt+s
wait_url 1
BODY=$(url_part "$(last_url)" body)
expect_in "$BODY" "### Stack" "the stack in the body"
expect_in "$BODY" "mitcad::crash::crashNow()" "the crashing function in the body"
[ -z "$(ls "$CRASHES"/*.crash 2> /dev/null)" ] && [ -n "$(ls "$CRASHES"/*.crash.offered)" ] ||
  ui_fail "the report was not marked offered"
echo "ok   the report is marked offered"
ui_focus_main
ui_stop_app

echo "--- 4. A crash of the import worker, offered at once"
printf 'not a design' > "$WORK/part.f3d"
: > "$UI_LOG"
MITCAD_TEST_CRASH=import-worker ui_start_app --open "$WORK/part.f3d"
ui_focus_dialog '^Import$'
ui_expect_log "Mitcad crashed: SIGSEGV; crash report: $CRASHES/" "the import failed with the worker's crash"
ui_step "the import's error (Enter)"       ui_key Return
ui_focus_dialog '^Error Report$'
ui_expect_log "Error report offered (worker-crash): The import process quit unexpectedly (SIGSEGV)." \
  "the worker's crash offered at once"
ui_mark
ui_step "no more offers"                   ui_click_logged "Error report no more offers"
ui_step "Review Report (Alt+R)"            ui_key alt+r
ui_focus_dialog '^Review Report$'
ui_expect_new "Error reports: no more offers" "no more offers"
ui_expect_new "Report preview: Crash of the import process: SIGSEGV in mitcad::crash::crashNow" "the title"
ui_expect_new "Report section error: Mitcad 0" "what happened"
ui_expect_new "(import process, " "the import process named"
ui_step "Cancel (Esc)"                     ui_key Escape
ui_expect_new "Report not sent: the preview was cancelled" "nothing sent"
[ "$(grep -c 'Open URL (test, not opened): ' "$UI_LOG")" = 0 ] || ui_fail "an address was opened"
echo "ok   no issue form opened"
grep -q '^offerErrors=false$' "$SETTINGS" || ui_fail "the setting reports/offerErrors is not off"
echo "ok   the offers are off in the settings"
ui_focus_main
ui_stop_app

echo "--- 5. Offers off: a crash report is kept, not offered"
OFFERED=$(ls "$CRASHES"/*.crash.offered | head -1)
cp "$OFFERED" "$CRASHES/crash-1-1.crash"
: > "$UI_LOG"
ui_start_app --demo
ui_expect_log "Error report not offered (turned off): Mitcad" "not offered"
[ -f "$CRASHES/crash-1-1.crash.offered" ] || ui_fail "the report was not marked offered"
echo "ok   marked offered"
ui_stop_app
ui_finish "ui-report-test"
