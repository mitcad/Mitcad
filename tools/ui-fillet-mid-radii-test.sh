#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ListLineLayout.cpp app/framework/ListLineLayout.hpp
# check-all sources: app/framework/CommandPanel.cpp app/commands/FilletCommands.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/fillet-mid-radii.mitcad-ui tools/ui-scripts/fillet-mid-radii.json
# Fillet's Mid Radii in a narrow Command panel (mitcad#2421, GitHub
# mitcad/Mitcad#671) through the real UI: the shared input script
# tools/ui-scripts/fillet-mid-radii.mitcad-ui on a stepped block that
# mitcad-cli makes (tools/ui-scripts/fillet-mid-radii.json), in the
# Floating and the Docked layout at Text size 100%, then in both at Text
# size 150%. A Variable fillet with two Mid Radii: each row's Set,
# Position and Radius stacked under the row's name, every field wide
# enough for a short value and no label cut; the fields typed through with
# Tab; an invalid Radius's error under its field, whole. No Fillet panel's
# input is ever logged clipped.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-fillet-mid-radii-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-mid-radii.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-mid-radii.mitcad-ui
# Room for the panel at Text size 150% without scrolling it.
UI_SCREEN_HEIGHT=1080
"$cli" run "$here/ui-scripts/fillet-mid-radii.json" --save "$work/fillet-mid-radii.mitcad" || exit 1
ui_start_display

# mid_radii name [app arguments]: the script's steps in one layout, on a
# fresh copy of the design.
mid_radii() {
  local name=$1
  shift
  echo "=== Mid Radii, $name"
  cp "$work/fillet-mid-radii.mitcad" "$work/layout.mitcad" || { ui_fail "$name: the design could not be copied"; return; }
  ui_start_app --ui=qt --no-mcp "$@" --open "$work/layout.mitcad"
  for _ in $(seq 1 1200); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the Mid Radii driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'Panel Fillet row mid.1 stacked' "$name: the second Mid Radius row is stacked"
  ui_expect_log 'Panel Fillet Tab: mid.0.radius -> mid.1.set' "$name: Tab goes from the first row's Radius to the second row's Set"
  ui_expect_log 'is invalid: ' "$name: the invalid Radius is refused under its field"
  ui_expect_log 'TestDriver: done' "$name: the Mid Radii fields readable and typed"
  # No label, field or note of the Fillet panel cut at any time.
  if grep -E '^Panel Fillet inputs clipped: ' "$UI_LOG" | sort -u | sed 's/^/     /' | grep .; then
    ui_fail "$name: an input of the Fillet panel is clipped"
  else
    echo "ok   $name: no input of the Fillet panel clipped"
  fi
}

mid_radii "Floating" --chrome=floating
mid_radii "Docked" --chrome=docked
# A larger Text size (Preferences › Display › Text size): the panel and its
# text both larger.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '\n[appearance]\ntextSize=150\n' >> "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
mid_radii "Floating, Text size 150%" --chrome=floating
mid_radii "Docked, Text size 150%" --chrome=docked
ui_finish 'UI fillet mid radii'
