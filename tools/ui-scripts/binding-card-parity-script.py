#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise shared binding-card identities and actions through macOS controls."""
import re
import shlex
import sys

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820", "focus-main",
         "command view.iso", "command view.fit", "expect-model-bodies 4 26000",
         "capture-undo-depth beforeCards", "expect-ever '^Browser fx badge Root/Bodies/Body1: fx 3$'",
         "expect-ever '^Browser fx badge Root/Board:1: fx 1$'"]


def add(*steps):
    lines.extend(steps)


def expected(text):
    return "expect-log " + shlex.quote("^" + re.escape(text) + "$")


def capture(state):
    add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/binding-card-{state}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")


def body_card():
    add("focus-main", "mark")
    if native:
        add("native-select native.model.body:F2.b0@", "native-pointer-click native.model.fx.body:F2.b0@")
    else:
        add("click-logged '^Browser fx badge Root/Bodies/Body1 at '")
    add(expected("Bindings card shown for Body1 (pinned)"),
        expected("Bindings card Body1 follows; now: Color = Black, Holes = No"),
        expected("Bindings card line Appearance: Color: Black → Paint - Black ●, Red → Paint - Red"),
        expected("Bindings card line Shown: when Holes = No · shown now"),
        expected("Bindings card line Face 1 (side): Color: Black → Paint - White ●, Red → Paint - Yellow"),
        "expect-log '^Bindings card line Material: '")
    if native:
        add("expect-native-text bindingsCardTitle '^Body1 follows$'",
            "expect-native-text bindingsCardNow '^now: Color = Black, Holes = No$'",
            "expect-native-visible bindingsCardEdit_body:F2.b0:appearance true",
            "expect-native-visible bindingsCardEdit_body:F2.b0:material true",
            "expect-native-visible bindingsCardEdit_body:F2.b0:shown true",
            "expect-native-visible 'bindingsCardEdit_body:F2.b0:face:F2:side(c1[c4,c2])' true",
            "expect-native-visible bindingsCardAll true")
    else:
        add("focus-dialog '^Body1 follows$'",
            "expect-widget-property bindingsCardTitle text 'Body1 follows'",
            "expect-widget-property bindingsCardNow text 'now: Color = Black, Holes = No'")


def close_card():
    add("mark", "native-key-event Escape" if native else "key Escape",
        expected("Bindings card closed"), "focus-main")
    if native:
        add("expect-native-visible bindingsCardTitle false")


def edit_body(property_name, key, title, target_log=None):
    body_card()
    add("mark")
    if native:
        add("native-pointer-click " + shlex.quote("bindingsCardEdit_" + key))
    else:
        add("dialog-click-logged " + shlex.quote("^Bindings card Edit " + re.escape(property_name) + " at "))
    add(expected(f"Bindings card Edit {property_name} of Body1"),
        "focus-dialog " + shlex.quote("^" + re.escape(title) + "$"))
    if target_log:
        add("expect-log " + shlex.quote(target_log))
    add("native-key-event Escape" if native else "key Escape",
        ("expect-native-dialog-gone " if native else "expect-dialog-gone ") +
        shlex.quote("^" + re.escape(title) + "$"), "focus-main", "expect-model-bodies 4 26000",
        "expect-undo-depth ${beforeCards} 0")


def edit_body_tool(property_name, key, target, follows=True):
    # An appearance binding's Edit… starts the Appearance tool on its
    # target with the binding shown (mitcad#1059); Cancel changes nothing.
    body_card()
    add("mark")
    if native:
        add("native-pointer-click " + shlex.quote("bindingsCardEdit_" + key))
    else:
        add("dialog-click-logged " + shlex.quote("^Bindings card Edit " + re.escape(property_name) + " at "))
    add(expected(f"Bindings card Edit {property_name} of Body1"), expected(f"Appearance tool for {target}"))
    if follows:
        add("expect-log '^Appearance tool: the targets follow '")
    add("focus-main")
    capture("tool-" + re.sub(r"[^a-z]+", "-", property_name.lower()).strip("-"))
    add("native-click commandCancel" if native else "key Escape",
        "expect-log '^Command Appearance cancelled'", "focus-main", "expect-model-bodies 4 26000",
        "expect-undo-depth ${beforeCards} 0")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'", "native-expand native.modelTree",
        "native-select native.model.body:F2.b0@",
        "expect-native-text native.model.fx.body:F2.b0@ '^fx 3$'",
        # A real cursor is necessary: shared followPointer reads QCursor.
        # Direct mouseEntered or the synthetic Qt hover helper cannot prove it.
        "native-system-pointer native.modelTree", "sleep 100", "mark",
        # The name's real NSTextField is distinct from its neighbouring
        # fx pill. Waiting beyond the hover delay must not open a card.
        "native-system-pointer body:F2.b0@", "sleep 700",
        "expect-no-log '^Bindings card shown for '",
        "expect-native-visible bindingsCardTitle false", "mark",
        "native-system-pointer native.model.fx.body:F2.b0@", "sleep 700",
        expected("Bindings card shown for Body1 (hover)"),
        "expect-native-visible bindingsCardTitle true", "sleep 450",
        "expect-native-visible bindingsCardTitle true")
    capture("hover")
    # The hover popover covers the tree's centre. Move to the exposed
    # timeline search button to leave both the badge and the card.
    add("mark", "native-system-pointer native.history.search", "sleep 450",
        expected("Bindings card closed"), "expect-native-visible bindingsCardTitle false")
else:
    # The root's Bodies opens expanded, Body1's badge in sight (its arrow
    # would fold it away); later steps fold it to bring Board:1 in sight.
    add("expect-ever '^Browser arrow Root/Bodies at '",
        "expect-ever '^Browser fx badge Root/Bodies/Body1 at '")

body_card()
capture("body")
close_card()

if native:
    body_card()
    add("mark", "native-pointer-click native.model.component@", expected("Bindings card closed"),
        "expect-native-visible bindingsCardTitle false", "focus-main",
        "expect-model-bodies 4 26000", "expect-undo-depth ${beforeCards} 0")

edit_body_tool("Appearance", "body:F2.b0:appearance", "F2.b0")
edit_body_tool("Face 1 (side)", "body:F2.b0:face:F2:side(c1[c4,c2])", "F2.b0 F2:side(c1[c4,c2])", follows=False)
edit_body("Shown", "body:F2.b0:shown", "Show Body1 When")

add("mark")
if native:
    add("native-select native.model.component@O1", "expect-native-text native.model.fx.component@O1 '^fx 1$'",
        "native-pointer-click native.model.fx.component@O1")
else:
    # Collapse the three root bodies again so Board:1 is back in sight;
    # its old folded-layout position must not be reused after expansion.
    add("mark", "click-logged '^Browser arrow Root/Bodies at '",
        "expect-log '^Browser fx badge Root/Board:1 at '", "mark",
        "click-logged '^Browser fx badge Root/Board:1 at '")
add(expected("Bindings card shown for Board:1 (pinned)"),
    expected("Bindings card line Shown: when Color ≠ Red · shown now"))
capture("occurrence")
close_card()

add("mark")
if native:
    add("expect-native-text native.history.fx.F4 '^fx 1$'", "native-pointer-click native.history.fx.F4")
else:
    add("click-logged '^Timeline fx Box2 at '")
add(expected("Bindings card shown for Box2 (pinned)"),
    expected("Bindings card line Suppressed: when Holes = Yes · not suppressed now"))
capture("feature")
add("mark")
if native:
    add("native-pointer-click bindingsCardAll")
else:
    add("focus-dialog '^Box2 follows$'", "dialog-click-logged '^Bindings card all bindings at '")
add(expected("Bindings card all bindings"), "focus-dialog '^Configurations$'",
    expected("Configurations page Bindings"), "expect-log '^Configurations bindings status: 6 bindings, 1 warning$'")
if native:
    # Body2's warning fill must retain readable actual text in both themes.
    # Check every column, including the property/value and warning icon cell.
    for column in range(4):
        add(f"expect-native-style configurationsBindings/3/{column} readable-fill,in-view")
capture("all")
add("native-key-event Escape" if native else "key Escape",
    "expect-native-dialog-gone '^Configurations$'" if native else "expect-dialog-gone '^Configurations$'",
    "focus-main", "expect-model-bodies 4 26000", "expect-undo-depth ${beforeCards} 0",
    "mark", "menu 'File/Save'", "expect-log '^Saved '",
    "expect-never 'Native Mac canvas[:]|OCCT view event fail[e]d|Recompute failed|Follow Option'")
print("\n".join(lines))
