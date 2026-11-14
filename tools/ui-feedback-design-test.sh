#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/report app/MainWindow.cpp tools/cli
# check-all sources: core/model/src/document/design_package.rs core/model-api/src/design_package.rs
# check-all sources: core/model/src/document/components.rs core/model/src/file/mod.rs core/zip
# Help > Send Feedback with Include the design (mitcad#1795, GitHub
# mitcad/Mitcad#328), through the real UI, on a holder design in a project
# (a folder with .mitcad/project.json) that links a block design in the
# project's parts folder (synthetic designs that mitcad-cli makes). No
# browser and no file manager open: MITCAD_TEST_LOG_URLS=1 (ui-test-lib.sh)
# logs the addresses instead.
#   1. The form with a screenshot and Include the design: Preview makes the
#      zip in `reports` and shows its size; Send makes it again, opens its
#      folder and the issue form, whose body names the zip; the last step
#      says to attach it or e-mail it to testfile@mitcad.org. The zip has
#      both designs at their paths in the project, the project's marker,
#      the report's Markdown, the screenshot and the log, and nothing of
#      the project's other design.
#   2. The zip unpacked opens as a project again: the holder loads and
#      follows its link to the block there.
#   3. A zip larger than an issue takes (the limit lowered for the test,
#      MITCAD_TEST_ATTACHMENT_LIMIT): the preview and the last step say to
#      e-mail it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-feedback-design-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-feedback-design.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
REPORTS=$XDG_DATA_HOME/Mitcad/Mitcad/reports
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
TRACKER=https://tracker.test/mitcad/issues
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[reports]\nissueUrl=%s\n' "$TRACKER" > "$SETTINGS"

PROJECT=$WORK/project
HOLDER=$PROJECT/designs/holder.mitcad
mkdir -p "$PROJECT/.mitcad" "$PROJECT/designs" "$PROJECT/parts"
printf '{"format": "mitcad-project", "version": 1}\n' > "$PROJECT/.mitcad/project.json"

cat > "$WORK/block.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$PROJECT/parts/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (block)"; }
# Another design of the project, which the holder does not link.
"$CLI" run "$WORK/block.json" --save "$PROJECT/designs/unrelated.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (unrelated)"; }
cat > "$WORK/holder.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [50, 0],
    "length": 20, "width": 20, "height": 2, "operation": "new_body"}},
  {"cmd": "insert_component", "path": "../parts/block.mitcad", "base": "$PROJECT/designs"}
]
EOF
"$CLI" run "$WORK/holder.json" --save "$HOLDER" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (holder)"; }
grep -qF '"path": "../parts/block.mitcad"' "$HOLDER" || ui_fail "the holder does not link the block"

# last_url: the last address the app would have opened.
last_url() { grep -o 'Open URL (test, not opened): .*' "$UI_LOG" | tail -1 | sed 's/^Open URL (test, not opened): //'; }
# url_part url title|body: a query parameter, decoded.
url_part() {
  python3 -I -c 'import sys, urllib.parse as u; print(u.parse_qs(u.urlsplit(sys.argv[1]).query)[sys.argv[2]][0])' "$1" "$2"
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
  ui_fail "fewer than $1 addresses logged"
}
# zip_names zip: the names in a zip, one per line.
zip_names() { python3 -I -c 'import sys, zipfile; print("\n".join(zipfile.ZipFile(sys.argv[1]).namelist()))' "$1"; }
# zip_text zip name: a file's text in a zip.
zip_text() { python3 -I -c 'import sys, zipfile; print(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]).decode())' "$1" "$2"; }
# send_feedback summary: Send Feedback from the command search, the summary,
# a screenshot, Include the design, then Preview.
send_feedback() {
  ui_mark
  ui_step "Send Feedback (command search)"   ui_command "Send Feedback"
  ui_focus_dialog '^Send Feedback$'
  ui_expect_new "Send Feedback: the form shows" "the form shows"
  ui_step "summary"                          xdotool type --delay 20 "$1"
  ui_step "the screenshot on"                ui_key Tab Tab Tab Tab space
  ui_expect_new "Feedback screenshot " "the screenshot of the window"
  ui_step "Include the design (Alt+G)"       ui_key alt+g
  ui_expect_new "Feedback design on" "Include the design on"
  ui_step "Preview (Enter)"                  ui_key Return
  ui_focus_dialog '^Review Report$'
  ui_expect_new "Feedback form: kind bug, summary '$1'" "the form's content"
  ui_expect_new ", design on" "the form asks for the design"
}

ui_start_display

echo "--- 1. Send Feedback with Include the design"
ui_start_app --open "$HOLDER"
ui_expect_log "Opened $HOLDER" "the holder opened in its project"
ui_wait_idle
send_feedback "Link lost"
ui_expect_new "Report design zip: $REPORTS/Bug-Link-lost-" "the zip made for the preview"
ui_expect_new "Report section design: Bug-Link-lost-" "the preview shows the zip with its size"
PREVIEWED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep '^Report section design: ')
expect_not_in "$PREVIEWED" "over the attachment limit" "small enough to attach"
ui_mark
ui_step "Send (Alt+S)"                     ui_key alt+s
ui_expect_new "Report sent: 'Bug: Link lost'" "sent"
ui_expect_new "Report design zip shown: $REPORTS/Bug-Link-lost-" "the zip made again and shown"
ui_expect_new "Open URL (test, not opened): file://$REPORTS" "its folder opened in the file manager"
ui_expect_new "Report design zip note: The design is packed as $REPORTS/Bug-Link-lost-" "the last step names the zip"
ui_expect_new "Attach the zip to the issue by dragging it into the comment box, or e-mail it to testfile@mitcad.org and write its name in the issue." \
  "the last step says to attach it or e-mail it"
wait_url 2
URL=$(last_url)
expect_in "$URL" "$TRACKER/new?title=Bug%3A%20Link%20lost&body=" "the issue form opens last"
BODY=$(url_part "$URL" body)
expect_in "$BODY" "### Design" "the body has the design's line"
expect_in "$BODY" "A zip of the design was made for this report: \`Bug-Link-lost-" "the body names the zip"
expect_not_in "$BODY" "$REPORTS" "the body has the zip's name only"
expect_not_in "$BODY" "holder" "no design name in the body"
ZIPS=$(find "$REPORTS" -name 'Bug-Link-lost-*.zip')
[ "$(wc -l <<< "$ZIPS")" = 1 ] && [ -f "$ZIPS" ] || ui_fail "one zip expected in $REPORTS: '$ZIPS'"
ZIP=$ZIPS
echo "ok   one zip: $(basename "$ZIP")"
ls "$REPORTS"/preview-*.png > /dev/null 2>&1 && ui_fail "the preview's screenshot file is left in $REPORTS"
NAMES=$(zip_names "$ZIP")
for NAME in designs/holder.mitcad parts/block.mitcad .mitcad/project.json mitcad-report/report.md \
  mitcad-report/screenshot.png mitcad-report/log.txt; do
  grep -qxF "$NAME" <<< "$NAMES" || ui_fail "$NAME is not in the zip: $NAMES"
done
echo "ok   the zip has both designs, the project's marker, the report, the screenshot and the log"
expect_not_in "$NAMES" "unrelated" "nothing of the project's other design"
expect_not_in "$NAMES" ".mitcad/local" "no per-user state of the project"
REPORT_MD=$(zip_text "$ZIP" mitcad-report/report.md)
expect_in "$REPORT_MD" "### Diagnostics" "the report's Markdown"
expect_in "$REPORT_MD" "A zip of the design was made for this report: \`$(basename "$ZIP")\`" "the Markdown names the zip"
expect_in "$(zip_text "$ZIP" mitcad-report/log.txt)" "Opened $HOLDER" "the log of this run"
python3 -I -c 'import sys, zipfile; d=zipfile.ZipFile(sys.argv[1]).read("mitcad-report/screenshot.png"); sys.exit(d[:8] != b"\x89PNG\r\n\x1a\n")' "$ZIP" ||
  ui_fail "the zip's screenshot is not a PNG"
echo "ok   the screenshot is a PNG"
ui_focus_dialog '^Report$'
ui_step "the last step (Enter)"            ui_key Return
ui_focus_main
ui_stop_app

echo "--- 2. The zip unpacked opens as a project again"
UNPACKED=$WORK/unpacked
python3 -I -m zipfile -e "$ZIP" "$UNPACKED" || ui_fail "the zip does not unpack"
[ -f "$UNPACKED/.mitcad/project.json" ] || ui_fail "the unpacked folder is no project"
# The limit an issue takes lowered, for step 3 on this start.
MITCAD_TEST_ATTACHMENT_LIMIT=100 ui_start_app --open "$UNPACKED/designs/holder.mitcad"
ui_expect_log "Opened $UNPACKED/designs/holder.mitcad" "the unpacked holder opened"
ui_wait_idle
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/block:1 at" "the linked block is there"
grep -qF "link is not followed" "$UI_LOG" && ui_fail "opening says a link is not followed: $(grep -F 'link is not followed' "$UI_LOG")"
echo "ok   the link to the block is followed in the unpacked project"

echo "--- 3. Larger than an issue takes: e-mail it"
send_feedback "Too large"
ui_expect_new "Report section design: Bug-Too-large-" "the preview shows the zip"
ui_expect_new ", over the attachment limit" "the preview says it is too large to attach"
ui_mark
ui_step "Send (Alt+S)"                     ui_key alt+s
ui_expect_new "Report design zip shown: " "the zip shown"
ui_expect_new "larger than the 25 MB an issue takes: e-mail it to testfile@mitcad.org and write its name in the issue." \
  "the last step says to e-mail it"
ui_focus_dialog '^Report$'
ui_step "the last step (Enter)"            ui_key Return
ui_focus_main
ui_stop_app

ui_finish "UI feedback with the design"
