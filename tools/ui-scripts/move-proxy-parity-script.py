#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Real browser selection, motion choices, geometry picks and held handles."""
import math
import re
import shlex
import sys

frontend, case = sys.argv[1:]
native = frontend == "native"
face = case == "face-ring"
# Pegs start on the plate's z=0 plane: their first 4 mm overlap the plate,
# leaving 26 mm of each 30 mm extrusion above it (64 × pi × 4² × 26).
volume = 48000 if face else 46000 + 31124 * math.pi
lines = ["# SPDX-License-Identifier: MIT", "wait-idle 60", "window-resize 1400 900", "focus-main",
         "command view.fit", "expect-viewport", f"expect-model-bodies {1 if face else 3} {volume:.12f}",
         "capture-undo-depth baseline", "test-manipulator-feedback"]
if native:
    lines += ["native-appearance '${MITCAD_TEST_COLOR_SCHEME}'"]


def add(*steps):
    lines.extend(steps)


def choice(kind):
    add(f"{'native-choice' if native else 'qt-choice'} input_type {shlex.quote(kind)}", "wait-idle 60")


def field(identifier, value):
    add(f"native-pointer-click input_{identifier}" if native else f"focus-widget input_{identifier}",
        "key Ctrl+a", f"type '{value}'", "key Tab")


def pick_input(identifier):
    add(f"native-pointer-click input_{identifier}" if native else
        f"click-logged 'Panel {'Move Face' if face else 'Move/Copy'} input {identifier} at '")


def capture(prefix, uid, occurrence):
    add(f"capture-shown-body {prefix} {uid} {occurrence}")


def same(prefix, uid, occurrence):
    values = " ".join("${" + prefix + "_" + key + "}" for key in
                      ["volume", "min_x", "min_y", "min_z", "max_x", "max_y", "max_z"])
    add(f"expect-shown-body {uid} {occurrence} {values}")


uid, occurrence = ("F2.b0", "-") if face or case == "body-arrow" else ("F6.b0", "O2")
capture("original", uid, occurrence)
if face:
    region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
    top = "F2:end(" + region + ")"
    edge = "E{" + top + "|F2:side(c1[c4,c2])}"
    add("command solid.move_face", "expect-log '^Command Move Face started$'",
        "click-pick " + shlex.quote("face F2.b0/" + re.escape(top)),
        "expect-log 'Move Face Faces: 1 face'")
    choice("Rotate")
    # A valid, small tilt before the ring is dragged. Neighbouring faces
    # must extend/shorten; this command deliberately has no rigid proxy.
    field("angle", "5 deg")
    pick_input("axis")
    add("click-pick " + shlex.quote("edge F2.b0/" + re.escape(edge)),
        "expect-log 'Move Face Axis: 1 edge'", "wait-idle 60", "expect-log 'Preview Move Face: ok'")
    command, key, dx, dy = "Move Face", "angle", 12, 4
else:
    if native:
        add("native-expand native.modelTree", "native-select native.model." +
            ("body:F2.b0@" if case == "body-arrow" else "component@O2"))
    elif case == "body-arrow":
        # Startup already expands this fixture's body folder. Wait for
        # the shown body row instead of folding it with its disclosure.
        # Selecting a Browser row returns keyboard focus to the canvas.
        add("expect-ever '^Browser Root/Bodies/Body1 at '",
            "click-logged '^Browser Root/Bodies/Body1 at '",
            "expect-log '^Selected: 1 body \\[body F2.b0\\]$'")
    else:
        add("click-logged 'Browser Root/Pegs:1 at '")
    add("expect-selection 1 0", "mark", "command solid.move", "expect-log '^Command Move/Copy started$'")
    if case == "body-arrow":
        add("expect-log 'Move/Copy Objects: 1 body'", "wait-idle 60", "expect-log 'Preview Move/Copy: ok'")
        key, dx, dy = "tx", 120, 0
    else:
        add("expect-log 'Panel Move/Copy choices: move_type=components'")
        if case == "rotate-ring":
            choice("Rotate")
            add("expect-log 'Move/Copy: Move Type = Rotate'")
            # Z from the browser's Origin folder: a command shows the hidden
            # origin no more (mitcad#1030), and its row picks it as a click
            # in the view would, without an undo step of a light bulb.
            if native:
                add("native-expand native.modelTree", "native-select native.model.origin:z")
            else:
                add("mark", "click-logged '^Browser arrow Root/Origin at '", "wait-idle 60",
                    "expect-log '^Browser Root/Origin/Z at '", "click-logged '^Browser Root/Origin/Z at '")
            add("expect-log 'Move/Copy Axis: 1 axis'")
            key, dx, dy = "angle", 70, 40
        else:
            choice("Point to Position")
            add("expect-log 'Move/Copy: Move Type = Point to Position'")
            pick_input("point")
            add("click-pick '/E\\{F8:end\\(r\\{c'", "expect-log 'Move/Copy Point: 1 edge'")
            key, dx, dy = "px", 120, 0
        add("wait-idle 60", "expect-log 'Preview Move/Copy: ok'")
    command = "Move/Copy"

add("focus-main", "mark", f"drag-from '^Manipulator {command} {key} at ' {dx} {dy}", "wait-idle 60",
    f"capture dragged '^Manipulator {command} {key} dragged to ([+-]?[0-9.]+)$'")
# Ring logs are radians, while their visible input is in degrees. The saved
# expression is checked against the exact dragged radians by the verifier.
add(f"expect-input-text {key} '^[+-]?[0-9.]+(°| deg)?$'" if key == "angle" else
    f"expect-input-text {key} '^${{dragged:re}}( mm)?$'", f"expect-log 'Preview {command}: ok'")
capture("preview", uid, occurrence)
add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/{case}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}-preview.png'",
    "mark", "native-pointer-click commandOk" if native else "button '^OK$'",
    "expect-log '^Moved faces:' 60" if face else "expect-log '^Moved 1 ' 60", "wait-idle 60",
    "expect-undo-depth ${baseline} 1")
same("preview", uid, occurrence)
if not face:
    add(f"expect-model-bodies 3 {volume:.12f}")
add("menu 'File/Save'", "expect-log '^Saved '", "mark", "command edit.undo", "wait-idle 60",
    "expect-undo-depth ${baseline} 0", f"expect-model-bodies {1 if face else 3} {volume:.12f}")
same("original", uid, occurrence)
add("command edit.redo", "wait-idle 60", "expect-undo-depth ${baseline} 1")
same("preview", uid, occurrence)
add("expect-never 'The document was read while a job computes it|OCCT view event failed[:]|Recompute fail[e]d|Native Mac canvas[:]'")
print("\n".join(lines))
