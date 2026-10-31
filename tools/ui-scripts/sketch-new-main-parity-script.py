#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Real canvas events and ordinary shared command controls, without model edits."""
import sys

frontend, scenario = sys.argv[1:]
native = frontend == 'native'
lines = ['# SPDX-License-Identifier: MIT', 'wait-idle 30', 'window-resize 1400 900', 'focus-main']
if native:
    lines += ["native-appearance '${MITCAD_TEST_COLOR_SCHEME}'"]


def add(*steps):
    lines.extend(steps)


def capture(name):
    add(f"capture-sketch F2 '${{MITCAD_TEST_WORK}}/{name}.json'")


def roundtrip(prefix, depth):
    add('key Escape', f'expect-undo-depth ${{baseline}} {depth}', 'key Ctrl+z',
        f'expect-undo-depth ${{baseline}} {depth - 1}')
    capture(prefix + '-undo')
    add('key Ctrl+Shift+z', f'expect-undo-depth ${{baseline}} {depth}')
    capture(prefix + '-redo')


def choice(label, keys):
    add(f"native-pointer-choice input_filter '{label}' {keys} Return" if native else
        f"qt-choice input_filter '{label}'")


def finish_caption():
    if native:
        # Inspect what AppKit actually draws, not a cached button title.
        add('expect-native-style paletteFinish caption')


if scenario == 'measure':
    def measure_enabled(identifier, enabled):
        add(f"expect-native-enabled {identifier} {str(enabled).lower()}" if native else
            f"expect-widget-property {identifier} enabled {str(enabled).lower()}")

    def measure_press(identifier, label, place):
        if native:
            add(f"expect-native-text {identifier} '^{label}$'", f'native-pointer-click {identifier}')
        else:
            add(f"expect-widget-property {identifier} text '{label}'", f"click-logged '^{place} at '")

    # A history row's context menu, and the history's menu at the end of
    # its header (mitcad#1010).
    def row_menu(entry):
        add(f"native-context-menu measureHistory.row.1 '{entry}'" if native else
            f"context-menu-logged '^Measure history row 1 at ' '{entry}'")

    def history_menu(entry):
        if native:
            add(f"native-popup measureHistoryMenu '{entry}'")
        else:
            add("click-logged '^Measure history menu at '", f"popup '{entry}'")

    # The history is folded as the panel opens.
    def unfold_history():
        add('native-pointer-click measureHistoryToggle' if native else "click-logged '^Measure history at '",
            "expect-log '^Measure history opened$'")

    # The left edge's two corners stay outside the floating command card.
    # Selection 2 is optional, so focus its actual input before the pick.
    add('command view.top', 'command view.fit', 'expect-viewport', 'expect-model-bodies 1 252000',
        "capture-view-image '${MITCAD_TEST_WORK}/initial-view.png'",
        'capture-undo-depth baseline', 'mark', 'command inspect.measure',
        "expect-log 'Command Measure started'",
        "click-logged '^Sketch entity F1/p5 at '", "expect-log 'Measure Selection 1: 1 vertex'",
        'native-pointer-click input_b' if native else "click-logged '^Panel Measure input b at '",
        "click-logged '^Sketch entity F1/p8 at '", "expect-log 'Measure Selection 2: 1 vertex'",
        "expect-log 'distance 70'", "expect-log 'Measure Keep as Dimension enabled'",
        "expect-log '^Measure history row 1: .*: Distance 70 mm$'")
    measure_enabled('measureKeep', True)
    # One line a row: its number, its main value and the picks' kinds.
    if native:
        add("expect-native-text measureHistory.row.1 '^1[.] +70 mm +Vertex ↔ Vertex$'")
    else:
        add('expect-widget-property measureHistoryRows count 1')
    add("screenshot-screen '${MITCAD_TEST_WORK}/measure-body-corners.png'", 'mark')
    measure_press('measureKeep', 'Keep as Dimension', 'Measure Keep as Dimension')
    add("expect-log '^Measure Keep as Dimension clicked$'", "expect-log '^Annotation kept: Distance1$'",
        "expect-log '^Annotations: Distance1 distance$'", 'expect-undo-depth ${baseline} 1',
        'mark', 'key Escape', 'wait-idle 30', 'expect-undo-depth ${baseline} 1',
        "expect-no-log '^Annotations: none$'", 'expect-model-bodies 1 252000', 'mark',
        'command inspect.measure', "expect-log 'Command Measure started'",
        "expect-log '^Measure history row 1: .*: Distance 70 mm$'")
    # History belongs to the document, but its presenter and selection are
    # recreated. Restore the actual row before using its menu.
    unfold_history()
    if native:
        add("expect-native-text measureHistory.row.1 '^1[.] +70 mm +Vertex ↔ Vertex$'",
            'native-pointer-click measureHistory.row.1', 'expect-native-selection measureHistoryRows 1')
    else:
        add('expect-widget-property measureHistoryRows count 1',
            "click-logged '^Measure history row 1 at '",
            'expect-widget-property measureHistoryRows currentRow 0')
    add("expect-log '^Measure history showed row 1$'", "expect-log 'distance 70'")
    add('mark')
    row_menu('Copy')
    add("expect-log '^Measure history copied row 1: 1 .*Distance 70 mm$'", 'mark')
    history_menu('Copy All')
    add("expect-log '^Measure history copied 1 rows: 1 .*Distance 70 mm$'", 'mark')
    row_menu('Keep as Dimension')
    add("expect-log '^Measure history Keep as Dimension clicked: row 1$'", "expect-log '^Annotation kept: Distance2$'",
        "expect-log '^Annotations: Distance1 distance \\| Distance2 distance$'",
        'expect-undo-depth ${baseline} 2', 'mark', 'key Escape', 'wait-idle 30',
        'expect-undo-depth ${baseline} 2', "expect-no-log '^Annotations: none$'",
        'command edit.undo', 'expect-undo-depth ${baseline} 1',
        "expect-log '^Annotations: Distance1 distance$'", 'mark', 'command edit.undo',
        'expect-undo-depth ${baseline} 0', "expect-log '^Annotations: none$'", 'mark',
        'command inspect.measure', "expect-log 'Command Measure started'")
    choice('Sketch Curves and Points', 'Down Down')
    add("expect-log 'Measure: Selection Filter = Sketch Curves and Points'",
        "click-logged '^Sketch entity F1/p5 at '", "expect-log 'Measure Selection 1: 1 sketch point'",
        'native-pointer-click input_sketch_b' if native else "click-logged '^Panel Measure input sketch_b at '",
        "click-logged '^Sketch entity F1/p8 at '", "expect-log 'Measure Selection 2: 1 sketch point'",
        "expect-log 'distance 70'",
        "expect-log 'Measure Keep as Dimension disabled: Sketch geometry cannot be kept as an annotation'")
    measure_enabled('measureKeep', False)
    add('expect-log "^Measure Keep as Dimension disabled: Sketch geometry cannot be kept as an annotation: '
        'pick the body\'s corners or edges$"')
    add("screenshot-screen '${MITCAD_TEST_WORK}/measure-sketch-points.png'", 'mark')
    choice('Bodies', 'Up')
    add("expect-log 'Measure: Selection Filter = Bodies'", "click-logged '^Body F2.b0 at '",
        "expect-log 'Measure Body 1: 1 body'", "expect-log 'Measure Keep as Dimension enabled'")
    measure_enabled('measureKeep', True)
    add("screenshot-screen '${MITCAD_TEST_WORK}/measure-body.png'", 'mark')
    history_menu('Clear')
    add("expect-log '^Measure history Clear clicked$'", "expect-log '^Measure history cleared$'",
        "expect-log '^Measure history: 0 rows$'")
    measure_enabled('measureHistoryMenu', False)
    if native:
        add('expect-native-visible measureHistoryRows false')
    else:
        add('expect-widget-property measureHistoryRows count 0')
    add('key Escape', 'wait-idle 30', 'mark', 'command inspect.measure',
        "expect-log 'Command Measure started'", "expect-log '^Measure history: 0 rows$'")
    measure_enabled('measureKeep', False)
    add('key Escape', 'wait-idle 30',
        'expect-model-bodies 1 252000', 'expect-undo-depth ${baseline} 0')
else:
    add('expect-model-bodies 1 1000', 'native-activate native.history.F2' if native else
        "dblclick-logged '^Timeline Sketch1 at '", "expect-log 'Editing sketch F2 \\(Sketch1\\)'",
        'command view.fit', 'expect-viewport',
        "capture-view-image '${MITCAD_TEST_WORK}/initial-view.png'", 'capture-undo-depth baseline')
    capture('before')
    finish_caption()
    if scenario == 'rounded':
        for index, (command, x, y, width, height, radius) in enumerate([
                ('sketch.rectangle_rounded', -60, -20, 40, 30, 3),
                ('sketch.rectangle_rounded_center', 35, 0, 40, 20, 2)], 1):
            add('mark', 'command ' + command, f'sketch-click {x} {y}', f'type {width}',
                'key Tab', f'type {height}', 'key Tab', f'type {radius}',
                f'expect-widget-property sketchField_radius text {radius}')
            finish_caption()
            add(
                "screenshot-screen '${MITCAD_TEST_WORK}/rounded-fields-" + str(index) + ".png'",
                'key Return', f"expect-log 'Added rounded rectangle {width} x {height} mm, corner radius {radius} mm'")
            capture('first' if index == 1 else 'after')
            roundtrip('first' if index == 1 else 'action', index)
    elif scenario == 'fillet':
        add('sketch-click 30 10', 'sketch-click 50 25 ctrl', 'sketch-click 30 40 ctrl',
            'sketch-click 10 25 ctrl', 'expect-selection 4 0', 'mark', 'command sketch.fillet',
            "expect-log 'Preview Sketch Fillet: ok'",
            'native-pointer-click input_radius' if native else "click-logged 'Panel Sketch Fillet input radius at '",
            'key Ctrl+a', 'type 3', 'key Tab', 'focus-main', 'key Return',
            "expect-log 'Sketch fillet of .*radius 3'")
        capture('after')
        roundtrip('action', 1)
    elif scenario == 'symmetry':
        add('sketch-click -30 24', 'sketch-click -27 -11 ctrl', 'expect-selection 2 0', 'mark',
            'command sketch.constraint.symmetric', "expect-log 'Symmetry takes the selection as its picks: c'",
            'sketch-click 8 0', "expect-log 'Added constraint symmetric k'")
        capture('after')
        roundtrip('action', 1)
    elif scenario == 'first-dimension':
        add("expect-log '^Sketch Sketch1: 7 DOF$'", 'sketch-click 15 10',
            'expect-selection 1 0', 'mark', 'key d', 'sketch-click 15 4',
            "expect-log 'Sketch value editor at'", "type '100 mm'", 'key Return',
            "expect-log 'Added dimension \\S+ length \\S+ = 100 mm'",
            "expect-log 'Scaled the sketch by 10 to its first dimension'",
            "expect-never '^Added driven dimension'")
        capture('after')
        roundtrip('action', 1)
    else:
        raise SystemExit('unknown scenario: ' + scenario)
    add('command view.fit')
    if scenario == 'rounded':
        add(r"expect-sketch-dimension-clear '^30\.00$'")
    add("capture-view-image '${MITCAD_TEST_WORK}/sketch-result-scene.png'")
    finish_caption()
    add("screenshot-screen '${MITCAD_TEST_WORK}/sketch-result.png'",
        'native-pointer-click paletteFinish' if native else 'command sketch.finish',
        "expect-log 'Sketch finished'", 'expect-model-bodies 1 1000')
add("menu 'File/Save'", "expect-log '^Saved '")
print('\n'.join(lines))
