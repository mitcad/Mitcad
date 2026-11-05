#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/MainWindowSection.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionVisibility.cpp
# check-all sources: app/framework/TestDriver.cpp app/commands/HoleCommands.cpp app/commands/FilletCommands.cpp
# check-all sources: core/model/src/document/analyses.rs
# check-all sources: tools/ui-scripts/browser-eyes-mid-command.mitcad-ui tools/ui-scripts/browser-eyes-mid-command.json tools/cli
# Browser eyes while a command is open (mitcad#1263) through the real UI,
# the shared input script tools/ui-scripts/browser-eyes-mid-command.mitcad-ui
# (also run by tools/ui-windows-browser-eyes-mid-command-test.ps1) on a
# design that mitcad-cli makes (tools/ui-scripts/browser-eyes-mid-command.json):
# Section1's eye during a modelled Thread turns the section on, the panel
# stays open and its preview is clipped at the plane, OK adds the thread
# and Undo takes back the thread before the eye; Esc after Section1's eye
# cancels Thread and keeps the section, while Delete on its row still waits
# for the command; during Fillet the other body's eye and the filleted
# body's own keep the selection and OK adds the fillet (Undo: the fillet,
# then each eye).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-eyes-mid-command-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-browser-eyes-mid-command.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/browser-eyes-mid-command.mitcad-ui
"$cli" run "$here/ui-scripts/browser-eyes-mid-command.json" --save "$work/browser-eyes.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/browser-eyes.mitcad"
# The modelled thread's previews and OK take the longest: up to 9 minutes.
for _ in $(seq 1 2700); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'browser eyes driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Visibility Root/Analysis/Section1: shown (during Thread)' 'Section1 turned on during Thread'
ui_expect_log 'Section preview ready at ' "the thread's preview clipped at the section"
ui_expect_log 'Added modeled thread M10x1.5 6g on 1 face(s)' 'OK added the thread with the panel'"'"'s inputs'
ui_expect_log 'Command Thread cancelled' 'Esc cancelled Thread after the eye'
ui_expect_log 'Visibility Root/Bodies/Body2: hidden (during Fillet)' 'the filleted body hidden during Fillet'
ui_expect_log 'Added fillet on 4 edge(s)' 'OK added the fillet'
ui_expect_log 'TestDriver: done' 'the eyes work while a command is open'
ui_finish 'UI Browser eyes while a command is open'
