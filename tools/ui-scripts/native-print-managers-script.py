#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Native manager input; library fixtures are created by the macOS wrapper."""
import sys

project = sys.argv[1] == 'project'
lines = ['# SPDX-License-Identifier: MIT', 'wait-idle', 'window-resize 1280 820', 'native-appearance light',
         "expect-ever '^Frontend: native '",
         "expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'"]


def add(*steps):
    lines.extend(steps)


def groups(design=True):
    # NativeControlIdentifierRole identifies the actual first-column cell;
    # these ordinary selection rows have no separate .row view identifier.
    for place, title in [('design', 'This design'), ('project', 'This project'),
                         ('user', 'My library'), ('mitcad', 'Built-in')]:
        identifier = 'printManager.group.' + place
        if not design and place in ('design', 'project'):
            add(f'expect-native-present {identifier} false')
        else:
            count = '1' if design and place != 'mitcad' else '[1-9][0-9]*'
            add(f"expect-native-text {identifier} '^{title} \\({count}\\)$'")


def close(title):
    add('native-pointer-click printManagerClose', f"expect-native-dialog-gone '^{title}$'", 'focus-main')


def manager_bounds(table):
    # Managers keep their action row inside the body, not a dialog-button
    # footer. Check the real controls for clipping and pointer obstruction;
    # a scrolling table needs an exposed viewport, not every row at once.
    add(f'expect-native-visible {table} true')
    for identifier, width in [('printManagerSearch', 100),
                              ('slicerImportButton', 80), ('printManagerNew', 35),
                              ('printManagerDuplicate', 55), ('printManagerDelete', 40),
                              ('printManagerClose', 40)]:
        add(f'expect-native-bounds {identifier} {width}')
    if project:
        add('expect-native-bounds slicerImportInto 80')
    else:
        add('expect-native-visible slicerImportInto false')


if project:
    # Both windows must remain independently focusable while open together.
    add('expect-model-bodies 1 1000', 'focus-main', 'command make.printers', "focus-dialog '^Printers$'")
    groups()
    add("screenshot-dialog '${MITCAD_TEST_WORK}/printers-groups.png'", 'focus-main',
        'command make.processes', "focus-dialog '^Processes$'")
    groups()
    add("focus-dialog '^Printers$'", "focus-dialog '^Processes$'",
        'native-pointer-click printManagerClose', "focus-dialog '^Printers$'", 'native-pointer-click printManagerClose',
        'focus-main')

for kind, title, setting, value in [('printer', 'Printers', 'nozzle_diameter', '0.6'),
                                    ('process', 'Processes', 'layer_height', '0.16')]:
    command = {'printer': 'make.printers', 'process': 'make.processes'}[kind]
    add('focus-main', 'mark', f'command {command}', f"focus-dialog '^{title}$'",
        f"expect-log '^{title} opened{'' if project else ' without a design'}$'")
    groups(project)
    manager_bounds('printProfilesList')
    if project:
        add("native-choice slicerImportInto 'This project'",
            "expect-native-text slicerImportInto '^This project$'",
            'expect-native-enabled slicerImportButton true')
    if not project:
        add('native-type printManagerSearch Library',
            f"native-pointer-click printManager.item.user/library-{kind}",
            "expect-native-text printManagerEditing '^Editing: My library — every design of yours can use it$'",
            'expect-native-present printManagerUse false',
            'native-pointer-click printManagerNew',
            "expect-native-focus printProfileName '.+'", 'expect-native-style printProfileName selected',
            "screenshot-dialog '${MITCAD_TEST_WORK}/" + kind + "-no-design.png'")
        close(title)
        continue
    add('mark', 'native-type printManagerSearch Library',
        f"expect-log '^{title} groups: This design 0 \\| This project 0 \\| My library 1 \\| Built-in 0$'",
        "expect-native-text printManager.group.user '^My library \\(1\\)$'",
        "expect-native-text printManager.group.project '^This project \\(0\\)$'",
        f"expect-native-text printManager.item.user/library-{kind} '^Library {kind}$'",
        "native-type printManagerSearch ''", 'mark',
        f'native-pointer-click printManager.item.project/project-{kind}',
        "expect-native-text printManagerEditing \"^Editing: this project's library — every design of the project uses it$\"",
        'expect-native-focus printProfilesList',
        'key F2', f"expect-native-focus printProfileName '^Project {kind}$'",
        'expect-native-style printProfileName selected', f"type 'Edited {kind}'", 'key Return',
        f"expect-log '^Profile Project {kind} \\(project\\) name = Edited {kind}$'",
        f"expect-native-text printProfileName '^Edited {kind}$'",
        "native-type settingsSearch '" + ('nozzle diameter' if kind == 'printer' else 'layer height') + "'",
        f'expect-native-visible printSetting_{setting} true',
        f"native-type printSetting_{setting} '{value}'", 'key Return',
        f"expect-log '^Profile Edited {kind} \\(project\\) {setting} = {value.replace('.', '[.]')} mm$'",
        f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{kind}-project-edit.png'")
    if kind == 'printer':
        # These three captures intentionally have the same window size.
        # Compare actual native geometry after the real drag and after a
        # settings rebuild; retain all captures for the orchestrator's QA.
        # Give the settings row more room: its label, fixed-width editor
        # and reset action already occupy the right pane's reading width.
        add("screenshot-dialog '${MITCAD_TEST_WORK}/printers-divider-before.png'",
            'native-pointer-drag printManagerSplit.divider.0 0.5 -14',
            'expect-native-split-retained printManagerSplit.divider.0',
            "screenshot-dialog '${MITCAD_TEST_WORK}/printers-divider-after-drag.png'",
            'native-pointer-click printManager.item.user/library-printer', 'mark',
            'native-pointer-click printManagerUse',
            "expect-log '^Profile used in this design: printer Library printer$'",
            # Its preset changes the design dialect. SettingsForm::setKeys
            # replaces its rows, exercising an actual presenter rebuild.
            "expect-log '^Native presenter structure rebuild: Printers '",
            'native-pointer-click printManager.item.project/project-printer',
            "expect-native-text printProfileName '^Edited printer$'",
            'expect-native-visible printSetting_nozzle_diameter true',
            'expect-native-split-retained printManagerSplit.divider.0',
            "screenshot-dialog '${MITCAD_TEST_WORK}/printers-divider-after-rebuild.png'")
    add('mark', "native-pointer-popup printManagerCopyTo 'My library'",
        f"expect-log '^Profile copied to user: Edited {kind}$'",
        "expect-native-text printManagerEditing '^Editing: My library — every design of yours can use it$'",
        'native-type printManagerSearch Builtin',
        f'native-pointer-click printManager.item.mitcad/builtin-{kind}',
        "expect-native-text printManagerEditing '^Built-in: read only[.] Duplicate makes a copy you can edit[.]$'",
        'expect-native-style printProfileName readonly', 'expect-native-enabled printManagerDelete false',
        'expect-native-present printManagerCopyTo false', 'mark',
        f"native-pointer-popup printManagerBuiltInDuplicate 'My library' '${{MITCAD_TEST_WORK}}/{kind}-duplicate-where.png'",
        f"expect-log '^{title} duplicate menu: This design \\| This project \\| My library$'",
        f"expect-log '^Profile duplicated: Builtin {kind} as .* in user$'",
        "expect-native-focus printProfileName '.+'", 'expect-native-style printProfileName selected',
        "native-type printManagerSearch ''", 'mark',
        "native-pointer-popup printManagerNew 'This project'",
        f"expect-log '^{title} new menu: This design \\| This project \\| My library$'",
        f"expect-log '^Profile created: {kind} .* in project$'",
        "expect-native-focus printProfileName '.+'", 'expect-native-style printProfileName selected',
        'native-pointer-click printManagerUse',
        f"expect-log '^Profile used in this design: {kind} .*'")
    close(title)

if project:
    # Range validation stays beside the actual native setting. A refused
    # value does not replace the project profile; a corrected value commits.
    add('focus-main', 'command make.processes', "focus-dialog '^Processes$'",
        "native-type printManagerSearch 'Edited process'",
        'native-pointer-click printManager.item.project/project-process',
        "native-type settingsSearch 'infill density'", 'mark',
        "native-type printSetting_fill_density '250'", 'key Return',
        "expect-log '^Setting fill_density refused:'",
        "expect-native-text printSetting_fill_density '^250(?:[.]0*)?$'",
        "native-type printSetting_fill_density '25'", 'key Return',
        "expect-log '^Profile Edited process \\(project\\) fill_density = 25 %$'")
    close('Processes')

add('focus-main', 'mark', 'command solid.materials', "focus-dialog '^Materials$'")
groups(project)
manager_bounds('materialsList')
if project:
    add('native-type printManagerSearch Design', 'native-pointer-click material.design/design-material',
        "expect-native-text printManagerEditing '^Editing: this design only$'",
        'expect-native-focus materialsList', 'key F2',
        "expect-native-focus materialName '^Design material$'", 'expect-native-style materialName selected',
        "type 'Edited material'", 'key Return',
        "expect-log '^Material Design material \\(design\\) name = Edited material$'",
        "expect-native-text materialName '^Edited material$'",
        "expect-native-text materialAppearances/0/0 '^Paint - Red$'",
        'expect-native-style materialAppearances/0/0 image',
        "expect-native-text materialAppearances/0/1 '^Default$'",
        'native-pointer-click material.appearance.paint_blue',
        'native-pointer-click materialAppearanceDefault',
        "expect-log '^Material Edited material \\(design\\) appearance = paint_blue$'",
        "expect-native-text materialAppearances/0/0 '^Paint - Blue$'",
        "expect-native-text materialAppearances/0/1 '^Default$'",
        "screenshot-dialog '${MITCAD_TEST_WORK}/materials-swatches.png'", 'mark',
        'native-pointer-click material.appearance.paint_blue', 'key Delete',
        "expect-log '^Material Edited material \\(design\\) appearances = paint_red$'",
        "expect-native-text materialAppearances/0/0 '^Paint - Red$'", 'mark',
        'native-pointer-click material.appearance.paint_red', 'key Backspace',
        "expect-log '^Material Edited material \\(design\\) appearances = none$'",
        "expect-native-text materialAppearances/0/0 '^None$'",
        'expect-native-enabled materialAppearanceRemove false')
else:
    add('expect-native-present printManagerUse false')
add('native-type printManagerSearch PLA', 'native-pointer-click material.mitcad/pla',
    "expect-native-text printManagerEditing '^Built-in: read only[.] Duplicate makes a copy you can edit[.]$'",
    'expect-native-style materialName readonly', 'expect-native-enabled printManagerDelete false',
    'expect-native-enabled materialAppearanceAdd false', 'mark',
    ("native-pointer-popup printManagerBuiltInDuplicate 'My library' '${MITCAD_TEST_WORK}/materials-duplicate-where.png'"
     if project else 'native-pointer-click printManagerBuiltInDuplicate'),
    "expect-log '^Material duplicated: PLA Plastic as .* in user$'",
    "expect-native-focus materialName '.+'", 'expect-native-style materialName selected',
    "expect-native-text printManagerEditing '^Editing: My library — every design of yours can use it$'")
if project:
    add("expect-log '^Materials duplicate menu: This design \\| This project \\| My library$'")
close('Materials')
print('\n'.join(lines))
