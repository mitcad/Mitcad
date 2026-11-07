#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The rounding specs of the .f3d corpora (mitcad#107, regenerated in
# mitcad#1398): every fillet or chamfer whose definition the geometry kernel
# fails on during an import, with its input body, its definition (edges by
# name, sizes) and the history's stored bodies before and after it, written
# by the import's dressup dump (MITCAD_IMPORT_DRESSUPS,
# core/import-replay/src/dressups.rs), then indexed by
# tools/f3d-rounding-specs-index.py. Kept outside the repository: the specs
# are derived from the designs.
#
#   tools/f3d-rounding-specs.sh BUILD OUT [--jobs N] [--time-limit S]
#
# BUILD is a build folder with core/test_f3d_import (a release build: the
# import runs as in a coverage run). Each file of the corpora under
# MITCAD_F3D_CORPUS, MITCAD_F3D_MODELS and MITCAD_F3D_BACKUP (those set) is
# imported once per content (the first 8 hex digits of its SHA-256, the
# designs' content id; OUT/files.tsv), N at a time (default 3), in a child
# process of its own, with the dump in OUT/specs/<content id>/ and the
# import's report in OUT/reports/<content id>*.json. A file already done
# (OUT/done/<content id>) is not imported again, so a stopped run goes on
# where it was. At the end OUT/index.tsv lists every spec and
# OUT/summary.txt counts them by class.
#
# Memory: keep N times one import's peak (about 6 GB for the largest
# designs) within what the machine allows, e.g. run the script inside a
# memory-capped scope.

set -u

usage() {
  echo "usage: $0 BUILD OUT [--jobs N] [--time-limit S]" >&2
  exit 2
}

# One file (a child of the run): --one BUILD OUT TIME_LIMIT "ID<tab>CORPUS<tab>PATH".
if [ "${1:-}" = "--one" ]; then
  build=$2 out=$3 limit=$4
  IFS=$'\t' read -r id corpus relative <<<"$5"
  [ -e "$out/done/$id" ] && exit 0
  dir=${!corpus}
  mkdir -p "$out/specs/$id"
  start=$(date +%s)
  # The import stops a design at the time limit; the process is killed
  # well after it, so that no file stops the run.
  MITCAD_IMPORT_DRESSUPS="$out/specs/$id" timeout -k 30 $((limit * 3 + 120)) \
    "$build/core/test_f3d_import" --corpus-file "$dir/$relative" --id "$id" --max-items 1000000 \
    --time-limit "$limit" --hang-limit "$limit" --reports "$out/reports" \
    >"$out/logs/$id.txt" 2>&1
  status=$?
  printf '%s\t%s\t%s\t%s\n' "$id" "$corpus" "$status" "$(($(date +%s) - start))" >"$out/done/$id"
  exit 0
fi

[ $# -ge 2 ] || usage
build=$(cd "$1" && pwd) || usage
out=$2
shift 2
jobs=3
limit=300
while [ $# -gt 0 ]; do
  case "$1" in
    --jobs) jobs=$2; shift 2 ;;
    --time-limit) limit=$2; shift 2 ;;
    *) usage ;;
  esac
done
[ -x "$build/core/test_f3d_import" ] || { echo "no $build/core/test_f3d_import" >&2; exit 1; }
mkdir -p "$out/specs" "$out/reports" "$out/done" "$out/logs" || exit 1
out=$(cd "$out" && pwd)

# The files, once per content id, in the corpora's order.
: >"$out/files.tsv.new"
for corpus in MITCAD_F3D_CORPUS MITCAD_F3D_MODELS MITCAD_F3D_BACKUP; do
  dir=${!corpus:-}
  if [ -z "$dir" ] || [ ! -d "$dir" ]; then
    echo "$corpus: not set or missing, left out" >&2
    continue
  fi
  (cd "$dir" && find -L . -type f \( -name '*.f3d' -o -name '*.f3z' \) -print0 | sort -z |
    xargs -0 -r sha256sum) |
    while read -r sum path; do
      printf '%s\t%s\t%s\n' "${sum:0:8}" "$corpus" "${path#./}"
    done >>"$out/files.tsv.new"
done
awk -F '\t' '!seen[$1]++' "$out/files.tsv.new" >"$out/files.tsv"
echo "$(wc -l <"$out/files.tsv") files (distinct contents)" >&2

self=$(cd "$(dirname "$0")" && pwd)/$(basename "$0")
tr '\n' '\0' <"$out/files.tsv" |
  xargs -0 -r -n 1 -P "$jobs" bash "$self" --one "$build" "$out" "$limit"
python3 -I "$(dirname "$self")/f3d-rounding-specs-index.py" "$out"
