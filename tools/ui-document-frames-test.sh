#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/DocumentFrame.cpp app/DocumentFrame.hpp app/WindowManager.cpp app/WindowManager.hpp app/framework/TestDriverDocuments.cpp tools/cli
# check-all sources: tools/ui-scripts/multi-document-frames-script.py
# Document tabs on Linux (mitcad#1172): the portable document frame's
# scenario (tools/ui-scripts/multi-document-frames-script.py) in Floating
# and Docked chrome, then Close All from a tab's context menu. With one
# design the frame shows no tab bar and lays the window out as a single
# document window; a second design shows the tabs, and they go again when
# one design is left.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-document-frames-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-document-frames.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_STEP_MS=100

# Three synthetic designs of 6000, 120 and 27 mm3.
fixture() {
  local name=$1 length=$2 width=$3 height=$4
  printf '[{"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0], "length": %s, "width": %s, "height": %s, "operation": "new_body"}}]\n' \
    "$length" "$width" "$height" > "$work/$name.json"
  "$cli" run "$work/$name.json" --save "$work/$name.mitcad" > "$work/cli-$name.log" 2>&1 ||
    { cat "$work/cli-$name.log"; ui_fail "mitcad-cli $name"; exit 1; }
}

# run name script-mode [app options]: one scenario until the app exits.
run() {
  local name=$1 mode=$2
  shift 2
  export MITCAD_TEST_WORK=$work/$name
  mkdir -p "$MITCAD_TEST_WORK"
  fixture first 10 20 30
  cp "$work/first.mitcad" "$MITCAD_TEST_WORK/first.mitcad"
  fixture second 4 5 6
  cp "$work/second.mitcad" "$MITCAD_TEST_WORK/second.mitcad"
  fixture third 3 3 3
  cp "$work/third.mitcad" "$MITCAD_TEST_WORK/third.mitcad"
  export MITCAD_TEST_INPUT=$work/$name.mitcad-ui
  python3 "$here/ui-scripts/multi-document-frames-script.py" "$MITCAD_TEST_WORK" "$mode" > "$MITCAD_TEST_INPUT" || exit 1
  ui_start_app --ui=qt --no-mcp --no-recovery "$@" --open "$MITCAD_TEST_WORK/first.mitcad"
  for _ in $(seq 1 1800); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the document frame driver timed out"; cat "$UI_LOG"; exit 1; }
  wait "$UI_RUNNER"
  UI_RUNNER=""
  if grep -Eq '^TestDriver: FAIL([[:space:]]|$)' "$UI_LOG"; then
    grep -E '^TestDriver: FAIL' "$UI_LOG"
    ui_fail "$name: a document frame step failed"
    exit 1
  fi
}

# expect_line regex description: a line of the ended app's log.
expect_line() {
  if grep -Eq -- "$1" "$UI_LOG"; then echo "ok   $2"; else ui_fail "$2: '$1' not in the log"; fi
}

ui_start_display
for chrome in floating docked; do
  echo "--- Three designs as tabs, $chrome"
  run "frames-$chrome" full --chrome="$chrome"
  expect_line '^TestDriver: (done|quit)' "$chrome: the document frame scenario completed"
  expect_line '^TestDriver: hidden document quiet: .*0 frameSwapped events' "$chrome: a hidden tab presents no frames"
done
echo "--- Close All from a tab"
run frames-close-all close-all
expect_line '^TestDriver: observed Close All: 3 accepted document closes, empty process registry$' \
  'Close All closed all three designs'
ui_finish 'UI document frames'
