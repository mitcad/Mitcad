#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model core/model-base core/model-api core/ffi core/cpp geometry/src/sketch_ref.cpp tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
fixture=${UI_FACE_PROFILE_FIXTURE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/test_face_profiles}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-face-profiles.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
"$fixture" --write-fixture "$WORK" "$here/ui-scripts" || exit 1
ui_start_display
for scenario in face-bounded-profiles face-bounded-whole-face face-bounded-open-face; do
  case "$scenario" in
    face-bounded-profiles) design=face-profiles ;;
    face-bounded-open-face) design=open-face-profiles ;;
    *) design=whole-face ;;
  esac
  export MITCAD_TEST_INPUT=$WORK/$scenario.mitcad-ui
  ui_start_app --ui=qt --no-mcp --open "$WORK/$design.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$scenario driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'TestDriver: done' "$scenario completed"
  ui_stop_app
done
ui_finish 'UI solid/open face-bounded and whole-face profile picking'
