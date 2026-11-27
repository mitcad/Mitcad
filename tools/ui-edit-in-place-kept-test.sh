#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowEditInPlace.cpp app/files/MainWindowFiles.cpp app/files/MainWindowProjects.cpp app/MainWindow.cpp app/WindowManager.cpp
# check-all sources: tools/ui-scripts/edit-in-place-kept.mitcad-ui tools/ui-scripts/edit-in-place-part.json
# check-all sources: tools/ui-scripts/edit-in-place-part-newer.json tools/ui-scripts/edit-in-place-holder.json tools/cli
# Edit in Place's kept edits (mitcad#2527) never go over the linked design's
# own saves unasked (tools/ui-scripts/edit-in-place-kept.mitcad-ui): the part
# edited in place without a tab and Finish keep unsaved edits for the
# holder's Save; opening part.mitcad in a tab then asks about them first
# (Don't Save), the tab's change is saved, and the holder's Save leaves the
# tab's file as it was saved (compared byte for byte here, and read again in
# a new tab). Kept edits again, the file changed on disk meanwhile (here,
# with mitcad-cli): the holder's Save asks (Don't Save) and the file keeps
# what it has. All geometry is synthetic.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-edit-in-place-kept-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-edit-in-place-kept.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/edit-in-place-kept.mitcad-ui

# wait_for pattern seconds: the line in the application's log.
wait_for() {
  local _
  for _ in $(seq 1 $(($2 * 10))); do
    grep -aqE "$1" "$UI_LOG" && { echo "ok   $1"; return 0; }
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.1
  done
  cat "$UI_LOG" >&2
  ui_fail "'$1' is not in the log after $2 s"
  exit 1
}

# wait_count pattern count seconds: as wait_for, at least `count` times.
wait_count() {
  local _
  for _ in $(seq 1 $(($3 * 10))); do
    [ "$(grep -acE "$1" "$UI_LOG")" -ge "$2" ] && { echo "ok   $1 ($2 times)"; return 0; }
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.1
  done
  cat "$UI_LOG" >&2
  ui_fail "'$1' is not $2 times in the log after $3 s"
  exit 1
}

"$cli" run "$here/ui-scripts/edit-in-place-part.json" --save "$work/part.mitcad" || exit 1
# The holder made in the part's folder: the link is part.mitcad.
(cd "$work" && "$cli" run "$here/ui-scripts/edit-in-place-holder.json" --save "$work/holder.mitcad") || exit 1
# The part as another program will have written it (two cubes, 2000 mm3).
"$cli" run "$here/ui-scripts/edit-in-place-part-newer.json" --save "$work/newer.mitcad" || exit 1

ui_start_display
ui_start_app --ui=qt --no-mcp --no-recovery --open "$work/holder.mitcad"

echo "--- 1. The tab's save stays on disk after the holder's Save"
wait_for '^Saved .*/part[.]mitcad$' 180
# The tab's saved design, as the script waits for it.
cp "$work/part.mitcad" "$work/tab-saved.tmp" && mv "$work/tab-saved.tmp" "$work/tab-saved.mitcad"
wait_for '^Saved .*/holder[.]mitcad$' 120
if cmp -s "$work/part.mitcad" "$work/tab-saved.mitcad"; then
  echo "ok   part.mitcad is as its tab saved it after the holder's Save"
else
  ui_fail "the holder's Save wrote part.mitcad over the tab's save"
  exit 1
fi

echo "--- 2. The file changed on disk while edits made in place are kept"
wait_count '^Finish Edit in Place C[0-9]+: changed, unsaved; ' 2 180
cp "$work/newer.mitcad" "$work/part.tmp" && mv -f "$work/part.tmp" "$work/part.mitcad" || exit 1
: > "$work/part-changed"
echo "ok   part.mitcad changed on disk"

for _ in $(seq 1 1800); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'edit in place kept driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: quit' 'Kept edits asked about before the tab opens, and before a write over a changed file'
if cmp -s "$work/part.mitcad" "$work/newer.mitcad"; then
  echo "ok   part.mitcad keeps what it had on disk after Don't Save"
else
  ui_fail "the holder's Save wrote part.mitcad over the file changed on disk"
fi
ui_finish 'UI Edit in Place kept edits'
