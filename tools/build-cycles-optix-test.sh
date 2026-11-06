#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# tools/dev-env/build-cycles.sh --optix (mitcad#1332, docs/rendering.md
# "Building an OptiX worker yourself") refuses, before it fetches or builds
# anything:
#   - a folder without include/optix.h, and one that is not there (the
#     script never downloads the OptiX SDK);
#   - --optix without a folder;
#   - the ordinary render prefix as the OptiX worker's prefix.
# No SDK and no network: the script stops at its argument checks, and no
# prefix is made. The ctest repo.build_cycles_optix_args on Linux.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-optix-args.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/sdk/include" "$WORK/home"

fail() {
  echo "FAIL: $*"
  exit 1
}

# check description text arguments...: the script exits with status 2 and
# says `text`, and makes no prefix.
check() {
  local what=$1 text=$2 out status
  shift 2
  out=$(HOME="$WORK/home" MITCAD_RENDER_DEPS="$WORK/ordinary" bash "$ROOT/tools/dev-env/build-cycles.sh" "$@" 2>&1)
  status=$?
  [ "$status" -eq 2 ] || fail "$what: exit status $status, not 2: $out"
  grep -qF -- "$text" <<< "$out" || fail "$what: no \"$text\" in: $out"
  for prefix in "$WORK/ordinary" "$WORK/home/mitcad-render-deps" "$WORK/home/mitcad-render-deps-optix"; do
    [ ! -e "$prefix" ] || fail "$what: the prefix $prefix was made"
  done
  echo "ok   $what"
}

check "a folder without include/optix.h" "has no include/optix.h" --optix "$WORK/sdk"
check "a folder that is not there" "has no include/optix.h" --optix "$WORK/no-such-sdk"
check "--optix without a folder" "--optix needs the folder of the OptiX SDK" --optix
# A header in place (an empty file of the test's own, no SDK file): the
# ordinary prefix is refused all the same. The script builds for x86-64
# Linux only and says so first elsewhere.
if [ "$(uname -s)" = Linux ] && [ "$(uname -m)" = x86_64 ]; then
  : > "$WORK/sdk/include/optix.h"
  check "the ordinary prefix (MITCAD_RENDER_DEPS)" "is the ordinary render prefix" --optix "$WORK/sdk" "$WORK/ordinary"
  check "the ordinary prefix (the default)" "is the ordinary render prefix" \
    --optix "$WORK/sdk" "$WORK/home/mitcad-render-deps"
fi
echo "PASS: build-cycles.sh --optix checks its arguments"
