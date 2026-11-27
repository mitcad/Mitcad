#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/CutTargetsPanel core/model/src/features/extrude.rs
# Real primary-Qt Join/New Body conversion, dormant scope and persistence.
# Usage: bash tools/ui-macos-cut-conversion-test.sh <app> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-cut-conversion}"
UI_TIMEOUT=150
cli=${MITCAD_UI_CLI:-$(dirname "$UI_APP")/../../../../tools/cli/mitcad-cli}
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
for phase in direct fresh; do
  "$cli" run "$here/ui-scripts/cut-conversion.json" --save "$MITCAD_TEST_WORK/cut-conversion.mitcad" > "$UI_OUT/setup-$phase.log" 2>&1 || exit 1
  MITCAD_TEST_INPUT=$here/ui-scripts/cut-conversion-$phase.mitcad-ui ui_run_app "$phase" \
    --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/cut-conversion.mitcad"
  ui_expect_log 'TestDriver: done' "$phase Cut: conversion-time checklist and geometry"
  if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
done
for operation in Join NewBody; do
  if [ "$operation" = Join ]; then
    export MITCAD_CONVERSION_START_LABEL=Join
    export MITCAD_CONVERSION_START_COUNT=2
    export MITCAD_CONVERSION_START_VOLUME=8640
  else
    export MITCAD_CONVERSION_START_LABEL='New Body'
    export MITCAD_CONVERSION_START_COUNT=3
    export MITCAD_CONVERSION_START_VOLUME=8800
  fi
  "$cli" run "$here/ui-scripts/cut-conversion.json" --save "$MITCAD_TEST_WORK/cut-conversion.mitcad" > "$UI_OUT/setup-$operation.log" 2>&1 || {
    cat "$UI_OUT/setup-$operation.log" >&2
    exit 1
  }
  for phase in cut-conversion cut-conversion-reopen; do
    MITCAD_TEST_INPUT=$here/ui-scripts/$phase.mitcad-ui ui_run_app "$operation-$phase" \
      --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/cut-conversion.mitcad"
    ui_expect_no_log 'TestDriver: FAIL' "$operation/$phase: assertions passed"
    ui_expect_log 'TestDriver: done' "$operation/$phase: completed"
    if [ "$UI_FAILURES" -gt 0 ]; then
      echo "FAIL: cut conversion; evidence in $UI_OUT"
      exit 1
    fi
  done
  # Verify persistence independently of the panel and its preview cache.
  python3 - "$MITCAD_TEST_WORK/cut-conversion.mitcad" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    feature = next(f for f in json.load(source)['features'] if f['uid'] == 'F4')
assert feature['operation'] == 'cut', feature
scope = feature['cut_scope']
assert scope['context'] == {'component': 'C0', 'occurrence': ''}, scope
assert scope['targets']['type'] == 'automatic', scope
assert scope['targets']['excluded'] == [{'component': 'C1', 'occurrence': 'O1'}], scope
PY
  if [ $? -ne 0 ]; then
    ui_fail "$operation: saved scope differs from checklist"
    exit 1
  fi
done
for policy in legacy explicit; do
  python3 - "$here/ui-scripts/cut-conversion.json" "$MITCAD_TEST_WORK/local.json" "$policy" <<'PY'
import json, sys
with open(sys.argv[1], encoding='utf-8') as source:
    script = json.load(source)
definition = {'type': 'extrude', 'profiles': [{'sketch': 'F3', 'region':
    'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'}],
    'extent': {'type': 'distance', 'distance': 50}, 'operation': 'cut'}
if sys.argv[3] == 'explicit':
    definition['operation'] = 'join'
    definition['participants'] = ['F1.b0']
script.append({'cmd': 'add_feature', 'def': definition})
with open(sys.argv[2], 'w', encoding='utf-8') as target:
    json.dump(script, target)
PY
  if [ $? -ne 0 ]; then exit 1; fi
  "$cli" run "$MITCAD_TEST_WORK/local.json" --save "$MITCAD_TEST_WORK/local.mitcad" > "$UI_OUT/setup-$policy.log" 2>&1 || exit 1
  MITCAD_TEST_INPUT=$here/ui-scripts/cut-conversion-local.mitcad-ui ui_run_app "$policy" \
    --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/local.mitcad"
  ui_expect_log 'TestDriver: done' "$policy: local policy preserved"
  if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
done
echo "PASS: Qt cut conversion; logs and captures in $UI_OUT"
