#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/framework/GlassCard app/framework/FloatingLayout app/framework/TestDriver app/framework/Ribbon
# Actual native tool-window bounds as well as requested slots (mitcad#681).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <cli> [output directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-projects-cards}"
export MITCAD_LIBRARIES_DIR=$UI_ROOT/library-cache
sed '/^collapse-card /d' "$here/ui-scripts/macos-projects-cards.mitcad-ui" > "$UI_OUT/docked.mitcad-ui"
cat >> "$UI_OUT/docked.mitcad-ui" <<'DRIVER'
# A VM's 2x workspace can leave just 394 logical points for the window's
# frame. Exercise that allowance in this process, including the real native
# titlebar margins, without changing the physical display resolution.
fit-main-frame 916 394
expect-main-frame 916 394
expect-ribbon-labels Solid Build Make
expect-projects-layout ${MITCAD_TEST_PROJECT_COUNT}
focus-widget projectsTree
key Home Right
expect-projects-layout ${MITCAD_TEST_PROJECT_COUNT}
key End
expect-projects-layout ${MITCAD_TEST_PROJECT_COUNT}
expect-widget-property ribbonScrollLeft enabled false
focus-widget ribbonScrollRight
key Space
expect-widget-property ribbonScrollLeft enabled true
expect-ribbon-labels Solid Build Make
focus-widget ribbonScrollLeft
key Space
expect-widget-property ribbonScrollLeft enabled false
expect-ribbon-labels Solid Build Make
screenshot ${MITCAD_TEST_SHOTS}/short-frame.png
# Below the width of all group captions, overflow must work through ordinary
# keyboard input and leave every complete group menu reachable.
fit-main-frame 500 394
expect-main-frame 500 394
expect-ribbon-labels Solid Build Make
expect-widget-property ribbonScrollLeft enabled false
focus-widget ribbonScrollRight
key Space
expect-widget-property ribbonScrollLeft enabled true
expect-ribbon-labels Solid Build Make
focus-widget ribbonScrollLeft
key Space
expect-widget-property ribbonScrollLeft enabled false
expect-ribbon-labels Solid Build Make
screenshot ${MITCAD_TEST_SHOTS}/ribbon-overflow.png
DRIVER
for chrome in floating docked; do
  template=$here/ui-scripts/macos-projects-cards.mitcad-ui
  [ "$chrome" = docked ] && template=$UI_OUT/docked.mitcad-ui
  { echo 'command view.projects'; cat "$template"; } > "$UI_OUT/empty-$chrome.mitcad-ui"
done
for count in 0 1 24; do
  if [ "$count" != 0 ]; then
    start=1
    [ "$count" = 24 ] && start=2
    for ((i=start; i<=count; i++)); do
      folder=$(printf '%s/Project-%02d' "$MITCAD_PROJECTS_DIR" "$i")
      if [ "$i" = 1 ]; then
        # A local bare repository stands in for the Cloud: no network.
        git init -q --bare "$UI_ROOT/remote.git"
        "$cli" project create "$folder" --author 'UI Tester <ui@example.invalid>' --design base.mitcad \
          --url "$UI_ROOT/remote.git" >> "$UI_OUT/setup.log" 2>&1 || exit 1
        "$cli" project settings "$folder" --set '{"local":{"name":"Project-01"}}' \
          >> "$UI_OUT/setup.log" 2>&1 || exit 1
        "$cli" lock take "$folder/base.mitcad" --author 'UI Tester <ui@example.invalid>' \
          >> "$UI_OUT/setup.log" 2>&1 || exit 1
        "$cli" lock status "$folder" >> "$UI_OUT/setup.log" 2>&1 || exit 1
      else
        "$cli" project create "$folder" --author 'UI Tester <ui@example.invalid>' --design base.mitcad \
          >> "$UI_OUT/setup.log" 2>&1 || exit 1
      fi
      "$cli" project mkdir "$folder" parts >> "$UI_OUT/setup.log" 2>&1 || exit 1
      "$cli" project new-design "$folder" parts/bracket.mitcad >> "$UI_OUT/setup.log" 2>&1 || exit 1
    done
  fi
  export MITCAD_TEST_PROJECT_COUNT=$count
  for chrome in floating docked; do
    if [ "$chrome" = docked ]; then
      export MITCAD_TEST_INPUT=$UI_OUT/docked.mitcad-ui
    else
      export MITCAD_TEST_INPUT=$here/ui-scripts/macos-projects-cards.mitcad-ui
    fi
    for scale in 1 2; do
      schemes=light
      [ "$count" = 1 ] && schemes='light dark'
      for scheme in $schemes; do
        export MITCAD_SETTINGS_DIR=$UI_ROOT/settings-$count-$chrome-${scale}x-$scheme
        mkdir -p "$MITCAD_SETTINGS_DIR"
        if [ "$count" = 0 ]; then
          export MITCAD_TEST_INPUT=$UI_OUT/empty-$chrome.mitcad-ui
        else
          # Shown at the start, as for a user who turned it on (mitcad#1020:
          # hidden until then, also with projects configured), in the
          # macOS settings file nocodo.fi/Mitcad.ini.
          mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
          printf '[projects]\npanel=true\n' > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
        fi
        export MITCAD_TEST_SHOTS=$UI_OUT/$count-$chrome-${scale}x-$scheme
        mkdir -p "$MITCAD_TEST_SHOTS"
        QT_SCALE_FACTOR=$scale MITCAD_TEST_COLOR_SCHEME=$scheme \
          ui_run_app "$count-$chrome-${scale}x-$scheme" --ui=qt --chrome="$chrome" --no-recovery --no-mcp
        ui_expect_log 'TestDriver: done' "$count projects, $chrome, ${scale}x, $scheme: layout completed"
        ui_expect_no_log 'TestDriver: FAIL' "$count projects, $chrome, ${scale}x, $scheme: no overlap or clipping"
      done
    done
  done
done
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: Projects and Browser slots and actual bounds, 0/1/24 projects, Floating/Docked, 1x/2x"
