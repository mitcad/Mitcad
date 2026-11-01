#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Ordinary macOS merge/release checks through the profiling test runner.
# Usage: tools/test-macos.sh --build build/macos-release [-R REGEX ...]
set -euo pipefail
build=""
mode=--schedule
jobs=4
args=("$@")
while [ "$#" -gt 0 ]; do
  case $1 in
    --build) build=${2:?--build needs a directory}; shift ;;
    --measure) mode=--measure; jobs=1 ;;
  esac
  shift
done
[ -n "$build" ] || { echo 'test-macos: --build DIR is required' >&2; exit 2; }
runner=$build/core/mitcad-testrun
[ -x "$runner" ] || { echo "test-macos: build $runner first" >&2; exit 2; }
# Aqua GUI tests hold macos_desktop: one GUI at a time, while independent
# tests may overlap. The shell harness also locks across separate runs.
# Measure mode defaults to one test at a time.
# macOS rusage records CPU and the largest process's RSS, without a hard
# per-test memory limit. Profiles live outside the disposable build tree.
exec "$runner" "$mode" -j "$jobs" --memory-budget 85% --cpu-gate 0.9 \
  --sensitive 'app.ui-*' --sensitive-max 1 --sensitive-load 0.7 \
  --limit-factor 0 "${args[@]}"
