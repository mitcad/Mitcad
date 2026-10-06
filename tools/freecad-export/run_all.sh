#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Makes the FreeCAD reference data with every installed FreeCAD version
# (install-freecad.sh), in the isolated Linux distro only:
#
#   - each model of models/ saved as <models>/<version>/<id>.FCStd with its
#     dump <id>.json (run_model.py, dump.py);
#   - the examples bundled with each version copied to
#     <examples>/<version>/ with their dumps;
#   - the enumeration tables of each version, <models>/<version>/enums.json
#     (enums.py), merged into <models>/enums.json (merge_enums.py), the
#     source of core/freecad/data/enums.json.
#
# FreeCAD runs with its user interface on a hidden X display (xvfb-run), so
# that the documents keep their view data (GuiDocument.xml), and with a
# configuration folder of its own per version (<FREECAD_ROOT>/home). The
# output folders hold only the documents and their dumps (a failed model
# keeps its log). .FCStd files are never committed to the repository; the
# tests read them from MITCAD_FCSTD_CORPUS.
#
# Usage: tools/freecad-export/run_all.sh [--no-examples] [version...]
#   versions: those under ~/freecad (default: all)
#   FREECAD_ROOT (default ~/freecad), FCSTD_MODELS (default ~/fcstd-models),
#   FCSTD_EXAMPLES (default ~/fcstd-examples), MODELS (model ids to make)
set -uo pipefail

TOOLS=$(cd "$(dirname "$0")" && pwd)
FREECAD_ROOT=${FREECAD_ROOT:-$HOME/freecad}
MODELS_OUT=${FCSTD_MODELS:-$HOME/fcstd-models}
EXAMPLES_OUT=${FCSTD_EXAMPLES:-$HOME/fcstd-examples}
examples=1
if [ "${1:-}" = --no-examples ]; then
  examples=0
  shift
fi
versions=("$@")
if [ ${#versions[@]} -eq 0 ]; then
  for dir in "$FREECAD_ROOT"/*/squashfs-root; do
    [ -d "$dir" ] && versions+=("$(basename "$(dirname "$dir")")")
  done
fi
[ ${#versions[@]} -gt 0 ] || { echo "no FreeCAD under $FREECAD_ROOT (install-freecad.sh)" >&2; exit 1; }

# freecad <version> <script>: FreeCAD with its user interface on a hidden
# display, its own configuration (next to the installation); the script
# ends it (os._exit).
freecad() {
  local version=$1 script=$2
  local home="$FREECAD_ROOT/home/$version"
  mkdir -p "$home"
  FREECAD_USER_HOME="$home" timeout 600 xvfb-run -a -s "-screen 0 1280x1024x24" \
    "$FREECAD_ROOT/$version/squashfs-root/AppRun" freecad "$script" < /dev/null > "$home/last.log" 2>&1
}

failed=0
for version in "${versions[@]}"; do
  out="$MODELS_OUT/$version"
  mkdir -p "$out"
  echo "== FreeCAD $version"
  for model in "$TOOLS"/models/*.py; do
    id=$(basename "$model" .py)
    if [ -n "${MODELS:-}" ] && ! [[ " $MODELS " == *" $id "* ]]; then
      continue
    fi
    # The model's files and its variant (_prelude.py: variant("changed", …)).
    rm -f "$out/$id".* "$out/${id}_changed".*
    MODEL="$model" OUT="$out" TOOLS="$TOOLS" freecad "$version" "$TOOLS/run_model.py"
    result=$(cat "$out/$id.status" 2> /dev/null || echo "failed: no status (see $out/$id.log)")
    printf '  %-24s %s\n' "$id" "$result"
    # The log and the status stay only when the model failed.
    case $result in
      ok | skipped*) rm -f "$out/$id.log" "$out/$id.status" ;;
      *) failed=1 ;;
    esac
  done
  ENUMS_OUT="$out/enums.json" freecad "$version" "$TOOLS/enums.py"
  [ -s "$out/enums.json" ] && echo "  enums.json" || { echo "  enums.json failed"; failed=1; }
  if [ $examples -eq 1 ]; then
    shipped="$FREECAD_ROOT/$version/squashfs-root/usr/share/examples"
    mkdir -p "$EXAMPLES_OUT/$version"
    for file in "$shipped"/*.FCStd; do
      [ -f "$file" ] || continue
      name=$(basename "$file" .FCStd)
      cp "$file" "$EXAMPLES_OUT/$version/"
      FCSTD="$EXAMPLES_OUT/$version/$name.FCStd" JSON="$EXAMPLES_OUT/$version/$name.json" \
        freecad "$version" "$TOOLS/dump.py"
      [ -s "$EXAMPLES_OUT/$version/$name.json" ] && r=ok || { r=failed; failed=1; }
      printf '  example %-16s %s\n' "$name" "$r"
    done
  fi
done

# The tables of every version made so far, not only of this run's.
inputs=()
for table in "$MODELS_OUT"/*/enums.json; do
  [ -s "$table" ] && inputs+=("$table")
done
if [ ${#inputs[@]} -gt 0 ]; then
  "$FREECAD_ROOT/${versions[-1]}/squashfs-root/AppRun" python "$TOOLS/merge_enums.py" \
    "$MODELS_OUT/enums.json" "${inputs[@]}" < /dev/null && echo "merged: $MODELS_OUT/enums.json"
fi
exit $failed
