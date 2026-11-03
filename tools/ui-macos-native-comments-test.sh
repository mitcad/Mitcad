#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments app/view/CommentPinOverlay
# check-all sources: app/platform/macos app/platform/NativeTestInput.hpp app/framework/TestDriver app/agent
# Comments in AppKit (mitcad#1120): a real face pick and native editor,
# numbered pin, complete native rows and actions, agent address command,
# filters, reply/reopen/delete, the chat toggle and navigation pin toggle.
# The synthetic block, settings and scripted assistant belong to this run.
# Usage: tools/ui-macos-native-comments-test.sh <app> <mitcad-cli> <test_chat_agent> <output>
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> <test_chat_agent> <output>" >&2
  exit 2
fi
cli=$2
agent=$3
[ -x "$cli" ] && [ -x "$agent" ] || { echo "FAIL: build mitcad-cli and test_chat_agent" >&2; exit 2; }
ui_init "$1" "$4"
UI_TIMEOUT=${UI_NATIVE_TIMEOUT:-300}
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_CHAT_DIR=$UI_ROOT/chat-history
export MITCAD_CHAT_AGENT_DIR=$UI_ROOT/chat-agent
export MITCAD_TEST_ASSISTANT_EXECUTABLES=$UI_ROOT/clients
export MITCAD_ASSISTANT_TEST_WORK=$MITCAD_TEST_WORK
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_TEST_ASSISTANT_EXECUTABLES"
cp "$agent" "$MITCAD_TEST_ASSISTANT_EXECUTABLES/claude"
chmod 755 "$MITCAD_TEST_ASSISTANT_EXECUTABLES/claude"
: > "$MITCAD_TEST_WORK/claude-signed-in"
cat > "$MITCAD_TEST_WORK/model.json" <<'JSON'
[
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F1","corner":[0,0],"width":30,"height":20},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F1","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],"extent":{"type":"distance","distance":10},"operation":"new_body"}}
]
JSON
for scheme in light dark; do
  rm -rf "$MITCAD_SETTINGS_DIR"
  mkdir -p "$MITCAD_SETTINGS_DIR"
  model=$MITCAD_TEST_WORK/comments-$scheme.mitcad
  "$cli" run "$MITCAD_TEST_WORK/model.json" --save "$model" > "$UI_OUT/setup-$scheme.log" 2>&1 || {
    cat "$UI_OUT/setup-$scheme.log" >&2
    exit 1
  }
  export MITCAD_TEST_SHOTS=$UI_OUT/$scheme
  mkdir -p "$MITCAD_TEST_SHOTS"
  MITCAD_TEST_APPEARANCE=$scheme MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-comments.mitcad-ui \
    ui_run_app "comments-$scheme" --ui=native --no-recovery --mcp-folder "$MITCAD_TEST_WORK" --open "$model"
  ui_expect_log '^Frontend: native \(--ui\)' "$scheme: AppKit was selected explicitly"
  ui_expect_log '^TestDriver: done' "$scheme: native comments workflow completed"
  native_clicks=$(grep -c '^Native viewport pointer:' "$UI_LOG_FILE" || true)
  [ "$native_clicks" -eq 2 ] || ui_fail "$scheme: expected actual AppKit face and pin clicks, found $native_clicks"
  ui_expect_log '^Comment C1: resolved' "$scheme: the native Resolve action changed the model"
  ui_expect_log '^Comment C1 reply 1: user: checked native reply' "$scheme: native reply editor committed"
  ui_expect_log '^Chat: Comments not attached' "$scheme: native chat toggle changed its shared state"
  for problem in '^TestDriver: FAIL' 'Unable to simultaneously satisfy constraints' \
      'Native Mac canvas:' 'OCCT view event failed:' 'Recompute failed'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  for shot in pin panel addressed compact chat; do
    [ -s "$MITCAD_TEST_SHOTS/$shot.png" ] || ui_fail "$scheme: missing $shot capture"
  done
  [ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
done
echo "PASS: native Comments: face add, pins, full rows/actions, filters, chat, VoiceOver labels; $UI_OUT"
