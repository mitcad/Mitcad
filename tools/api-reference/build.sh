#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Builds the generated API reference (mitcad#2606): static HTML and
# Markdown pages of the model API from core/model-api/src/commands.md, the
# commands' JSON Schemas and the names model_reference lists. Needs no
# network beyond what cargo needs to build (with --offline nothing is
# fetched).
#
#   tools/api-reference/build.sh [<folder>] [--offline]
#       writes the pages into <folder> (default build/api-reference) and
#       the report of what the cleaning left out beside it
#       (<folder>.report.txt); fails when a page is not clean, or when
#       <folder> holds files that are not pages of this reference (the
#       folder is published as it is: give a new or empty one at a
#       release, mitcad#2636).
#
# See tools/api-reference/README.md.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
folder="$repo/build/api-reference"
offline=()
for argument in "$@"; do
  case "$argument" in
    --offline) offline=(--offline) ;;
    -*) echo "unknown option $argument" >&2; exit 2 ;;
    *) folder="$argument" ;;
  esac
done
mkdir -p "$(dirname "$folder")"
folder="$(cd "$(dirname "$folder")" && pwd)/$(basename "$folder")"

"${CARGO:-cargo}" run --quiet --locked ${offline[@]+"${offline[@]}"} \
  --manifest-path "$repo/core/Cargo.toml" \
  -p mitcad-mcp --example api_reference -- "$folder"
