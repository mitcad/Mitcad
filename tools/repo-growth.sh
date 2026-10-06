#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Measures how a git repository grows when a project file is saved again and
# again (P12a): for each model, three repositories get the model and then
# SAVES commits that each change one parameter (its expression and value):
#   v2      one file with the B-rep data inside (base64),
#   v3      version 3 with the B-rep data in the project's store, zlib files,
#   v3raw   the same with the B-rep files uncompressed (measurement only;
#           Mitcad does not read it).
# Printed per model: the file sizes, the loose objects after the saves (as a
# library without packing, such as gitoxide, leaves them: count, bytes, disk
# use) and the size after `git gc` (packs with deltas). Needs git and
# python3; MITCAD_CLI is the mitcad-cli to convert with.
#
# Usage: tools/repo-growth.sh <work dir> <model.mitcad>...
set -euo pipefail
WORK=${1:?usage: tools/repo-growth.sh <work dir> <model.mitcad>...}
shift
CLI=${MITCAD_CLI:-build/dev/tools/cli/mitcad-cli}
SAVES=${SAVES:-50}
CLI=$(realpath "$CLI")
mkdir -p "$WORK"
WORK=$(realpath "$WORK")

# Changes the first parameter with a number as its expression, or else the
# first feature's name; save $2 of the file $1.
edit() {
  python3 - "$1" "$2" <<'EOF'
import re, sys
path, save = sys.argv[1], int(sys.argv[2])
text = open(path, encoding="utf-8").read()
m = re.search(r'"expression": "(-?[0-9]+(?:\.[0-9]+)?)( ?[a-z]*)"', text)
if m:
    number = float(m.group(1)) + save * 0.25
    text = text[:m.start(1)] + repr(number) + text[m.end(1):]
    v = re.compile(r'"value": (-?[0-9.eE+-]+)').search(text, m.end())
    if v:
        text = text[:v.start(1)] + repr(number) + text[v.end(1):]
else:
    n = re.search(r'"name": "([^"]*)"', text[text.index('"features"'):])
    start = text.index('"features"') + n.start(1)
    text = text[:start] + n.group(1).split(" #")[0] + f" #{save}" + text[start + len(n.group(1)):]
open(path, "w", encoding="utf-8").write(text)
EOF
}

# The B-rep files of a version 3 project uncompressed, the file's
# references marked so.
uncompress() {
  python3 - "$1" <<'EOF'
import os, sys, zlib
root = sys.argv[1]
for folder, _, files in os.walk(os.path.join(root, ".mitcad", "brep")):
    for name in files:
        path = os.path.join(folder, name)
        data = zlib.decompress(open(path, "rb").read())
        open(path[: -len(".zlib")], "wb").write(data)
        os.remove(path)
model = os.path.join(root, "model.mitcad")
text = open(model, encoding="utf-8").read()
open(model, "w", encoding="utf-8").write(text.replace('"compression": "zlib",\n', '"compression": "none",\n'))
EOF
}

objects() { # repository: loose count, apparent bytes, disk KiB
  local count bytes disk
  count=$(git -C "$1" count-objects -v | sed -n 's/^count: //p')
  disk=$(git -C "$1" count-objects -v | sed -n 's/^size: //p')
  bytes=$(find "$1/.git/objects" -type f -not -path '*/pack/*' -not -path '*/info/*' -printf '%s\n' |
    awk '{ s += $1 } END { print s + 0 }')
  echo "$count $bytes $disk"
}

printf '%-14s %-6s %10s %10s %6s %11s %9s %10s\n' model kind file brep loose loose_bytes disk_KiB packed_KiB
for model in "$@"; do
  name=$(basename "$model" .mitcad)
  if ! grep -q '"expression": "-\?[0-9]' "$model" && ! grep -q '"uid": "F' "$model"; then
    echo "$name: no parameter or feature to change, left out"
    continue
  fi
  for kind in v2 v3 v3raw; do
    repo="$WORK/$name-$kind"
    rm -rf "$repo"
    mkdir -p "$repo"
    git -C "$repo" init -q -b main
    git -C "$repo" config user.name "Mitcad measurement"
    git -C "$repo" config user.email "measure@example.invalid"
    git -C "$repo" config gc.auto 0
    if [ "$kind" = v2 ]; then
      "$CLI" convert "$model" "$repo/model.mitcad" --format v2 > /dev/null
    else
      "$CLI" project init "$repo" --no-history > /dev/null
      "$CLI" convert "$model" "$repo/model.mitcad" --format v3 > /dev/null
      [ "$kind" = v3raw ] && uncompress "$repo"
    fi
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "First version"
    for ((i = 1; i <= SAVES; i++)); do
      edit "$repo/model.mitcad" "$i"
      git -C "$repo" commit -q -a -m "Save $i"
    done
    file=$(stat -c %s "$repo/model.mitcad")
    brep=0
    if [ -d "$repo/.mitcad/brep" ]; then
      brep=$(find "$repo/.mitcad/brep" -type f -printf '%s\n' | awk '{ s += $1 } END { print s + 0 }')
    fi
    read -r count bytes disk < <(objects "$repo")
    git -C "$repo" gc -q
    packed=$(git -C "$repo" count-objects -v | sed -n 's/^size-pack: //p')
    printf '%-14s %-6s %10s %10s %6s %11s %9s %10s\n' "$name" "$kind" "$file" "$brep" "$count" "$bytes" "$disk" "$packed"
  done
done
