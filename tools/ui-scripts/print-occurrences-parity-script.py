#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Use actual selection, Send and bed face choices for bounded 3MF cases."""
import re
import shlex
import sys

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820", "focus-main",
         "expect-model-bodies 1 1000", "expect-selection 0 0",
         "expect-setting print/slicerName 'Fixture slicer'",
         "expect-setting print/slicerProgram '${MITCAD_TEST_WORK}/record-slicer.py'",
         "expect-setting print/format 3mf"]


def add(*steps):
    lines.extend(steps)


def select(kind, path, label):
    if native:
        add("native-expand native.modelTree", f"native-select native.model.{kind}@{path}")
    else:
        add(f"click-logged {shlex.quote('Browser Root/' + label + ' at ')}")
    add("expect-selection 1 0")


def send(number, selected, count, on_faces, answer=None):
    stage = "escape" if answer == "escape" else str(number)
    bodies = f"^Send to Slicer Bodies: {count} " + ("body" if count == 1 else "bodies")
    add(f"capture-undo-depth beforeSend{number}", "mark", "command make.print3d",
        "expect-log '^Command Send to Slicer started$'",
        f"expect-log '^3D Print panel: 1 {selected}: Pin; format 3mf, refinement high, slicer Fixture slicer'",
        f"expect-log {shlex.quote(bodies)}")
    # The panel must confirm the recording program before Send: Folder
    # only would open an external file manager and is a fixture failure.
    add("native-pointer-click commandOk" if native else "button '^Send$'")
    if answer:
        question = "Not sent yet: Pair:1 has 2 print bed faces. Click the green one to put on the bed."
        add("expect-log '^3D Print bed face to choose for the file: 2 faces: Pin F1:top in Pair:1/Pin:1; Pin F1:top in Pair:1/Pin:2$'",
            f"expect-log {shlex.quote('^Panel Send to Slicer asks: ' + re.escape(question) + '$')}",
            "expect-log '^Send to Slicer marks: face F1.b0/F1:top in O1/O2, face F1.b0/F1:top in O1/O3$'",
            f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.started' {number - 1}",
            f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.json' {number - 1}",
            "expect-no-log '^3D Print: started |^3D Print: [0-9]+ bodies as '")
        if native:
            add("expect-native-visible input_bed_face true", "expect-native-enabled input_bed_face true",
                "expect-native-text input_bed_face '^Select$'", "expect-native-enabled commandOk false",
                f"expect-native-text note_bed_question {shlex.quote('^' + re.escape(question) + '$')}",
                "expect-native-visible 'native.action.note|bed_assembled|0' true",
                "expect-native-text 'native.action.note|bed_assembled|0' '^Send as assembled$'",
                "expect-native-visible commandMessage false")
        else:
            add("expect-widget-property select_bed_face text Select", "expect-widget-property commandOk enabled false",
                f"expect-log {shlex.quote('^Panel Send to Slicer note bed_question: ' + re.escape(question) + '$')}",
                "expect-widget-property input_bed_question visible true",
                "expect-widget-property commandMessage visible false")
        add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/print-bed-choice-{stage}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'", "mark")
        if answer == "button":
            add("native-pointer-button '^Send as assembled$'" if native else "button '^Send as assembled$'",
                "expect-log '^3D Print bed face of the file: kept as assembled$'")
        elif answer == "escape":
            # Use the real AppKit key-equivalent/responder route, including
            # the native form's Cancel button; a direct action misses it.
            add("native-key-event Escape" if native else "key Escape",
                "expect-log '^3D Print bed face of the file: not asked any more \\(Esc\\)$'",
                "expect-log '^Send to Slicer marks: none$'", "wait-idle",
                f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.started' {number - 1}",
                f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.json' {number - 1}",
                "expect-no-log '^3D Print: started |^3D Print: [0-9]+ bodies as |^Done Send to Slicer$|^Command Send to Slicer cancelled$'",
                "expect-model-bodies 1 1000")
            if native:
                add("expect-native-visible input_bed_face false", "expect-native-visible note_bed_question false",
                    "expect-native-visible 'native.action.note|bed_assembled|0' false",
                    "expect-native-visible commandOk true", "expect-native-enabled commandOk true")
            else:
                add("expect-widget-property input_bed_face visible false",
                    "expect-widget-property input_bed_question visible false",
                    "expect-widget-property commandOk visible true", "expect-widget-property commandOk enabled true")
            add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/print-bed-escape-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'",
                "mark", "native-key-event Escape" if native else "key Escape",
                "expect-log '^Command Send to Slicer cancelled$'", "wait-idle", "focus-main",
                "expect-model-bodies 1 1000", f"expect-undo-depth ${{beforeSend{number}}} 0",
                f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.started' {number - 1}",
                f"expect-files '${{MITCAD_TEST_WORK}}/receipts' '*.json' {number - 1}",
                "expect-no-log '^3D Print: started |^3D Print: [0-9]+ bodies as |^Done Send to Slicer$'")
            return
        else:
            add("click-pick 'occurrence face F1.b0/F1:top in O1/O3 at'",
                "expect-log '^3D Print bed face of the file: Pin F1:top in Pair:1/Pin:2$'")
    add("expect-log '^3D Print: started .*record-slicer.py ' 30",
        f"expect-file '${{MITCAD_TEST_WORK}}/receipts/{number}.json'", "wait-idle", "focus-main")
    status = f"Sent {count} " + ("body" if count == 1 else "bodies") + " to Fixture slicer."
    if answer == "face":
        status += " Pair:1 on Pin (Pair:1/Pin:2)'s print bed face."
    elif on_faces:
        status += f" {on_faces} " + ("body on its print bed face." if on_faces == 1 else "bodies on their print bed faces.")
    if native:
        add(f"expect-native-text native.status {shlex.quote('^' + re.escape(status) + '$')}")
    else:
        add(f"expect-widget-property messageBand message {shlex.quote(status)}")
    add("expect-model-bodies 1 1000", f"expect-undo-depth ${{beforeSend{number}}} 0",
        f"screenshot-screen '${{MITCAD_TEST_WORK}}/print-occurrences-{number}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
    if answer:
        add("expect-log '^View highlight places: none$'", "expect-no-log '^Command Send to Slicer cancelled$'")
        if native:
            add("expect-native-visible input_bed_face false", "expect-native-visible commandMessage false")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")

# Only Pin:2's body is selected. The other occurrence must not be exported.
select("body:F1.b0", "O1/O3", "Pair:1/Pin:2/Bodies/Pin")
send(1, "selected", 1, 0)
add("menu 'File/Save'", "expect-log '^Saved '", "mark",
    "open-file '${MITCAD_TEST_WORK}/bed.mitcad'", "wait-idle", "focus-main", "key Escape",
    "expect-selection 0 0", "expect-model-bodies 1 1000")

# No selection means both visible placements. Their top normals differ,
# so each requires an independent placement onto its print bed face.
send(2, "visible", 2, 2)

# Selecting the parent component sends the same two bodies as assembled:
# one build object and both original occurrence transforms, no bed turn.
add("command view.iso", "command view.fit")
select("component", "O1", "Pair:1")
send(3, "selected", 2, 0, "button")

# Escape ends only the question, sends nothing and keeps the panel open.
# A second Escape cancels the panel and preserves model/history state.
select("component", "O1", "Pair:1")
send(4, "selected", 2, 0, "escape")

# The second occurrence's top faces a different way and is on the outside
# of the whole assembly. Pick its real green overlay: both placements turn
# together onto that face, remaining one assembled build object.
select("component", "O1", "Pair:1")
send(4, "selected", 2, 1, "face")
add("menu 'File/Save'", "expect-log '^Saved '",
    "expect-never 'Recompute failed|Native Mac canvas[:]|OCCT view event failed'")
print("\n".join(lines))
