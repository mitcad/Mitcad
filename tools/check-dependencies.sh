#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Checks the dependencies against the licence policy (docs/development.md):
#   1. The Rust dependencies against core/deny.toml: known vulnerabilities
#      (fetches the RustSec advisory database), licences, banned crates and
#      sources.
#   2. The native libraries the build fetches from their pinned sources and
#      links in (cmake/Keychain.cmake: QtKeychain, BSD-3-Clause): their
#      licence texts in the build's fetched sources (MITCAD_BUILD_DIR,
#      default build/dev; skipped before a configure has fetched them).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
(cd "$ROOT/core" && cargo deny --locked check)

BUILD=${MITCAD_BUILD_DIR:-$ROOT/build/dev}
# licence name file phrases...: the file has every phrase and no GPL text.
licence() {
  local name=$1 file=$2
  shift 2
  if [ ! -f "$file" ]; then
    echo "native: $name: $file not there (configure the build first): skipped"
    return
  fi
  for phrase in "$@"; do
    grep -qF -- "$phrase" "$file" || { echo "native: $name: '$phrase' is not in $file" >&2; exit 1; }
  done
  if grep -qiE 'GNU (Lesser |Library )?General Public License' "$file"; then
    echo "native: $name: $file is a GPL licence" >&2
    exit 1
  fi
  echo "native: $name: ok"
}
licence "QtKeychain (BSD-3-Clause)" "$BUILD/_deps/qtkeychain-src/COPYING" \
  "Redistribution and use in source and binary forms" \
  "Redistributions in binary form must reproduce the above copyright" \
  "may not be used to" \
  "endorse or promote products derived from this software without"
