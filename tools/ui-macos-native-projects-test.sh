#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Native Projects: bounded rows, disclosure, model updates and real relaunches.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
trap 'project_test_exit=$?; trap - EXIT; set +e; ui_cleanup; exit "$project_test_exit"' EXIT
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
project="$MITCAD_PROJECTS_DIR/Project-01"
"$cli" project create "$project" --author 'UI Tester <ui@example.invalid>' --design base.mitcad \
  > "$UI_OUT/setup.log" 2>&1
"$cli" project mkdir "$project" parts >> "$UI_OUT/setup.log" 2>&1
"$cli" project new-design "$project" parts/bracket.mitcad >> "$UI_OUT/setup.log" 2>&1
for i in $(seq 1 12); do
  name=$(printf 'design-%02d.mitcad' "$i")
  "$cli" project new-design "$project" "$name" >> "$UI_OUT/setup.log" 2>&1
done
run_phase() {
  MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-projects-$1.mitcad-ui \
    ui_run_app "$1" --ui=native --no-recovery || true
  ui_expect_log 'TestDriver: done' "$1: native Projects workflow completed"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed'; do
    ui_expect_no_log "$problem" "$1: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
}
run_phase one
run_phase restored
run_phase hidden
for i in $(seq 2 24); do
  project=$(printf '%s/Project-%02d' "$MITCAD_PROJECTS_DIR" "$i")
  "$cli" project create "$project" --author 'UI Tester <ui@example.invalid>' --design base.mitcad \
    >> "$UI_OUT/setup.log" 2>&1
done
run_phase many
echo "PASS: native Projects rows, disclosure, opening designs, scrolling and visibility persistence"
