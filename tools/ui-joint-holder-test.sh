#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJointHolder.cpp app/MainWindowActivate.cpp app/browser/JointHolderQuestion.cpp
# check-all sources: app/browser/JointHolderQuestion.hpp app/commands/JointCommands.cpp app/view/ViewController.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionVisibility.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/joints.rs core/model-api/src/joints.rs tools/cli
# check-all sources: tools/ui-scripts/joint-holder.mitcad-ui tools/ui-scripts/joint-holder.json
# check-all sources: tools/ui-scripts/joint-holder-reported.mitcad-ui
# Joint's picks under another component than the active one (mitcad#1856,
# GitHub mitcad/Mitcad#344) through the real UI, the shared input script
# tools/ui-scripts/joint-holder.mitcad-ui (also run by the Windows and
# macOS wrappers) on a design that mitcad-cli makes
# (tools/ui-scripts/joint-holder.json), like the reported one: a roller
# sub-assembly with its own joint and rigid group beside a grounded frame,
# the roller active. A Joint from a roller part to the frame asks whether
# to make it in the root, which holds both picks; Make it in mounts the
# roller whole, the roller staying active. Don't ask again sets the
# preference (no question the next time); Preferences > Assembly sets it
# back to Ask, and Keep Roller keeps the joint in the roller, which refuses
# the frame's face. Last, Tube's and Frame's faces selected before Joint
# starts get the same question before the panel opens (mitcad#1864).
#
# Then, when the private test files have it (MITCAD_MITCAD_REPORTED,
# github-344), the reported design itself: with its roller active, a Joint
# from the roller's tube to a shaft of the frame asks, and Make it in mounts
# the roller, also when both are selected before Joint starts
# (tools/ui-scripts/joint-holder-reported.mitcad-ui).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-holder-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-joint-holder.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/joint-holder.mitcad-ui
"$cli" run "$here/ui-scripts/joint-holder.json" --save "$work/joint-holder.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/joint-holder.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'joint holder driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Joint component asked: The two picks are in Roller and in Frame.' 'the question names the picks'"'"' components'
ui_expect_log 'Joint component answer: make it in ' 'Make it in the root'
ui_expect_log 'Panel Joint notice: Joint goes into ' 'the panel says where the joint goes'
ui_expect_log ', which holds the picks' 'OK made the joint in the root'
ui_expect_log 'Preferences: when the picks are in another component holder' "Don't ask again set the preference"
ui_expect_log 'Joint component by the preference: ' 'the next joint asked nothing'
ui_expect_log 'Preferences: when the picks are in another component ask' 'Preferences set it back to Ask'
ui_expect_log 'Joint component answer: keep Roller' 'asked again, Keep Roller'
ui_expect_log 'Panel Joint error: Select geometry inside Roller, the component the joint goes into.' \
  'kept in Roller, the frame'"'"'s face is refused'
ui_expect_log 'Joint component of the selection made before it: 2 pick(s)' 'the selection made before Joint is asked about'
ui_expect_log 'TestDriver: done' 'the joint goes into the component that holds its picks'
UI_RUNNER=""

# The reported design (GitHub mitcad/Mitcad#344; under
# MITCAD_MITCAD_REPORTED, never committed).
REPORTED_ZIP=""
if [ -n "${MITCAD_MITCAD_REPORTED:-}" ]; then
  REPORTED_ZIP=$(find "$MITCAD_MITCAD_REPORTED/github-344" -maxdepth 1 -name '*.zip' 2> /dev/null | sort | head -1)
fi
if [ -n "$REPORTED_ZIP" ] && command -v unzip > /dev/null; then
  echo "--- The reported design of GitHub #344: the roller mounted from its own activation"
  mkdir -p "$work/reported"
  unzip -o -q "$REPORTED_ZIP" -d "$work/reported" || ui_fail "the reported design's zip"
  REPORTED=$(find "$work/reported" -name '*.mitcad' | sort | head -1)
  [ -n "$REPORTED" ] || ui_fail "no design in the reported zip"
  export MITCAD_TEST_INPUT=$here/ui-scripts/joint-holder-reported.mitcad-ui
  UI_WINDOW_WAIT=180 ui_start_app --ui=qt --no-mcp --open "$REPORTED"
  for _ in $(seq 1 2100); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the reported design'"'"'s driver timed out'; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'Joint component asked: ' 'asked for the reported roller'
  ui_expect_log 'Joint component answer: make it in ' 'Make it in the component that holds both'
  ui_expect_log ', which holds the picks' 'the roller mounted by a joint in the root'
  ui_expect_log 'Joint component of the selection made before it: 2 pick(s)' 'asked for the tube and the shaft selected before Joint'
  ui_expect_log 'TestDriver: done' 'the reported mount succeeds through the question'
  UI_RUNNER=""
else
  echo "skip the reported design of GitHub #344 (MITCAD_MITCAD_REPORTED, unzip)"
fi
ui_finish 'UI the component that holds the joint'"'"'s picks'
