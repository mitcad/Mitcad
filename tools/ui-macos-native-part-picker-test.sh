#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos app/platform/NativeDialogs.hpp app/platform/NativeTestInput.hpp app/library/PartPicker
# check-all sources: app/commands/BuildCommands.cpp app/framework/Dialogs.cpp app/framework/TestDriver tools/cli
# Native Component Picker (mitcad#819): BUILD Profile's real Part tile opens the
# shared picker in an AppKit utility panel with its pictured grid and
# groups, never a Qt window; a click chooses the part while the keyboard
# stays with the command; pinning keeps it over the command's end and the
# next Profile takes it over; open-table options below the grid keep their
# cascade, text-field commit and command choice; Close and Command-W return the keyboard to
# the document, where the command goes on; the focused accessibility
# element stays on a live tile and, after a cancelled command, on the canvas
# (mitcad#1025, mitcad#1026); light and dark appearance.
# A local synthetic library of two pictured slot profiles, made here with
# mitcad-cli (no supplier's files, no network).
# Usage: tools/ui-macos-native-part-picker-test.sh <app> <mitcad-cli> <output>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 3 ] || [ $# -gt 4 ] || { [ $# -eq 4 ] && [ "$4" != --prepare-only ]; }; then
  echo "usage: $0 <app> <mitcad-cli> <output> [--prepare-only]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=${UI_NATIVE_TIMEOUT:-240}
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_LIBRARIES_DIR=$UI_OUT/library-cache
lib=$UI_OUT/test-profiles
rm -rf "$lib" "$MITCAD_LIBRARIES_DIR" "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
setup=$UI_OUT/setup.log
: > "$setup"
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" >> "$setup" 2>&1; }

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# frame NAME ORIGIN Z X: a mount frame (docs/libraries.md, "BUILD parts").
frame() {
  printf '  {"cmd": "add_feature", "name": "mount:%s", "def": {"type": "joint_origin", "geometry": "origin",
    "frame_override": {"origin": %s, "z_axis": %s, "x_axis": %s}}},\n' "$1" "$2" "$3" "$4"
}
# profile SIZE: a square bar of SIZE mm centred on the Z axis, L long, with
# the end and slot frames of a slot profile.
profile() {
  local half
  half=$(awk -v s="$1" 'BEGIN { print s / 2 }')
  printf '[\n  {"cmd": "add_parameter", "name": "L", "expression": "500 mm"},\n'
  cat <<'EOF'
  {"cmd": "add_parameter", "name": "Grade", "expression": "'A'", "values": ["'A'", "'B'"]},
  {"cmd": "add_parameter", "name": "Offset", "expression": "8 mm", "min": "0 mm", "max": "10 mm", "step": "1 mm"},
EOF
  printf '  {"cmd": "sketch.create", "plane": "xy"},\n'
  printf '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-%s, -%s], "width": %s, "height": %s},\n' \
    "$half" "$half" "$1" "$1"
  printf '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "%s"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},\n' "$RECT"
  frame end_a "[0, 0, 0]" "[0, 0, -1]" "[1, 0, 0]"
  printf '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin",
    "geometry": {"body": "F2.b0", "face": "F2:end(%s)"}}},\n' "$RECT"
  frame "slot_x+_1" "[$half, 0, 0]" "[1, 0, 0]" "[0, 0, 1]"
  frame "slot_x-_1" "[-$half, 0, 0]" "[-1, 0, 0]" "[0, 0, 1]"
  frame "slot_y+_1" "[0, $half, 0]" "[0, 1, 0]" "[0, 0, 1]"
  frame "slot_y-_1" "[0, -$half, 0]" "[0, -1, 0]" "[0, 0, 1]"
  cat <<'EOF'
  {"cmd": "set_configurations", "configurations": {"options": ["Grade", "Offset"], "closed": false,
    "default": "Standard", "rows": [{"name": "Standard", "values": {"Grade": "'A'", "Offset": "8 mm"}}],
    "rules": [{"name": "Grade B offset", "when": "Grade == 'B'", "then": [{"option": "Offset", "max": "5 mm"}]}]}},
  {"expect": {"bodies": 1}}
]
EOF
}
design() { # design SIZE FILE
  local script=$UI_OUT/p$1-test.json
  profile "$1" > "$script"
  mkdir -p "$(dirname "$2")"
  "$cli" run "$script" --save "$2" >> "$setup" 2>&1
}
component() { # component SIZE: the manifest entry of the profile
  cat <<EOF
    {"id": "p$1-test", "path": "profiles/p$1-test.mitcad", "name": "Profile $1 test", "category": "profiles",
     "preview": "previews/p$1-test.png", "supplier": "Test supplier", "part_number": "P$1-T",
     "designation": "Profile $1 test L={L}",
     "build": {"role": "profile", "system": {"type": "I", "slot": 5, "grid": 20},
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "$1 mm", "height": "$1 mm"},
       "slot": {"depth": "6 mm", "opening": "5.2 mm", "inner_width": "11 mm"}, "mass_per_metre": "0.4 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}}
EOF
}
if ! "$cli" library init "$lib" --id test-profiles --name "Test profiles" --license MIT >> "$setup" 2>&1 ||
   ! design 20 "$lib/profiles/p20-test.mitcad" || ! design 30 "$lib/profiles/p30-test.mitcad"; then
  cat "$setup"
  exit 1
fi
# The parts' previews: a light picture with a darker section of the size,
# as a library's drawn previews are (PNG, written by the guest's Python).
mkdir -p "$lib/previews"
if ! python3 - "$lib/previews" >> "$setup" 2>&1 <<'PY'
import struct
import sys
import zlib


def png(path, side, section):
    rows = []
    low, high = (side - section) // 2, (side + section) // 2
    for y in range(side):
        row = bytearray()
        for x in range(side):
            inside = low <= x < high and low <= y < high
            row += bytes((96, 104, 120) if inside else (236, 238, 242))
        rows.append(b"\x00" + bytes(row))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0)
    with open(path, "wb") as out:
        out.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
                  + chunk(b"IEND", b""))


png(sys.argv[1] + "/p20-test.png", 128, 48)
png(sys.argv[1] + "/p30-test.png", 128, 72)
PY
then
  cat "$setup"
  exit 1
fi
{
  cat <<'EOF'
{
  "format": "mitcad-library", "version": 1, "id": "test-profiles", "name": "Test profiles",
  "description": "Synthetic BUILD profiles of the native part picker test.", "library_version": "1.0.0",
  "license": "MIT", "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "slot", "name": "Slot profile"}, {"id": "profiles", "name": "Profiles", "parent": "slot"}],
  "components": [
EOF
  component 20
  printf '    ,\n'
  component 30
  printf '  ]\n}\n'
} > "$lib/mitcad-library.json"
g "$lib" add -A
g "$lib" commit -q -m "Test profiles"
g "$lib" tag v1.0.0
if ! "$cli" library fetch "$lib" >> "$setup" 2>&1; then
  cat "$setup"
  exit 1
fi
# A design with only its root: Profile places without a component policy.
printf '[]\n' > "$MITCAD_TEST_WORK/empty.json"
if ! "$cli" run "$MITCAD_TEST_WORK/empty.json" --save "$MITCAD_TEST_WORK/start.mitcad" >> "$setup" 2>&1; then
  cat "$setup"
  exit 1
fi
# The integrated acceptance audit reuses these exact configurable, pictured
# production BUILD fixtures without launching this test's GUI workflows.
if [ "${4:-}" = --prepare-only ]; then
  echo "Prepared synthetic configurable BUILD profiles; $UI_OUT"
  exit 0
fi
for scheme in light dark; do
  MITCAD_TEST_COLOR_SCHEME=$scheme MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-part-picker.mitcad-ui \
    ui_run_app "picker-$scheme" --ui=native --no-recovery --open "$MITCAD_TEST_WORK/start.mitcad"
  ui_expect_log 'Native UI: AppKit shell with embedded OCCT viewport' "$scheme: the native AppKit shell was used"
  ui_expect_log 'TestDriver: done' "$scheme: the native Component Picker workflow completed"
  ui_expect_log 'Part picker chose test-profiles/p30-test' "$scheme: a click on a tile chose the part"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
      'Unable to simultaneously satisfy constraints'; do
    ui_expect_no_log "$problem" "$scheme: no $problem"
  done
  [ -s "$MITCAD_TEST_WORK/picker-$scheme.png" ] || ui_fail "$scheme: missing picker capture"
  if [ "$UI_FAILURES" -gt 0 ]; then
    ui_log_tail
    exit 1
  fi
done
echo "PASS: native Component Picker: production BUILD entry, pictured grid, focus, pin, close and reopen; $UI_OUT"
