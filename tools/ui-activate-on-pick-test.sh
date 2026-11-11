#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowActivate.cpp app/MainWindow.cpp app/MainWindow.hpp app/browser/ActivateOnPick.cpp
# check-all sources: app/browser/ActivateOnPick.hpp app/framework/CommandSession.cpp app/framework/CommandSession.hpp
# check-all sources: app/framework/CommandSessionVisibility.cpp app/framework/CommandPanel.cpp app/framework/CommandPresenter.hpp
# check-all sources: app/commands/FilletCommands.cpp app/framework/TestDriver.cpp core/model/src/document/components.rs
# check-all sources: tools/ui-scripts/activate-on-pick.mitcad-ui tools/ui-scripts/activate-on-pick.json tools/cli
# A feature that changes the picked body activates its component
# (mitcad#1634) through the real UI, the shared input script
# tools/ui-scripts/activate-on-pick.mitcad-ui (also run by
# tools/ui-windows-activate-on-pick-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/activate-on-pick.json): with Base active, Fillet
# on an edge of Lid's block activates Lid without a question, the panel
# says where the fillet goes, OK adds it, and Undo takes back the fillet,
# then the activation; Esc after the activation leaves Lid active; an edge
# of each component gives the refusal and the Activate Lid button, which
# keeps Lid's edge.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-activate-on-pick-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-activate-on-pick.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/activate-on-pick.mitcad-ui
"$cli" run "$here/ui-scripts/activate-on-pick.json" --save "$work/activate-on-pick.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/activate-on-pick.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'activate on pick driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Activated Lid for Fillet (picked edge ' 'Fillet on Lid'"'"'s edge activated Lid'
ui_expect_log 'Panel Fillet notice: Fillet goes into Lid (now active).' 'the panel says where the fillet goes'
ui_expect_log 'Added fillet on 1 edge' 'OK added the fillet in Lid'
ui_expect_log 'Command Fillet cancelled' 'Esc cancelled Fillet after the activation'
ui_expect_log 'Panel Fillet offers: Activate Lid (drops 1)' 'an edge of each component offers Activate Lid'
ui_expect_log 'Panel Fillet notice: Fillet goes into Lid (now active). 1 pick of another component dropped.' \
  'the Activate button kept Lid'"'"'s edge'
ui_expect_log 'TestDriver: done' 'the picked body'"'"'s component is activated'
# Lid activated three times for Fillet: by the first pick twice, by the
# button once; never by a question (anchored: the driver echoes its own
# script lines).
activated=$(grep -c '^Activated Lid for Fillet ' "$UI_LOG")
[ "$activated" -eq 3 ] || ui_fail "Lid activated $activated times for Fillet, expected 3"
echo "ok   Lid activated 3 times for Fillet"
grep -q '^Activate component asked' "$UI_LOG" && ui_fail 'a question was asked'
echo "ok   nothing asked"
ui_finish 'UI Activate on pick'
