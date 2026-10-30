# SPDX-License-Identifier: MIT
"""Texture folder width, middle elision, selection and removal (mitcad#988)."""
import sys

native = sys.argv[1] == 'native'
lines = ['wait-idle', 'expect-model-bodies 1 48000', 'command tools.preferences.textures',
         "focus-dialog '^Settings$'"]

def add(*steps):
    lines.extend(steps)

def capture(name):
    add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{name}-${{MITCAD_TEST_COLOR_SCHEME}}-content.png'",
        f"screenshot-screen '${{MITCAD_TEST_WORK}}/{name}-${{MITCAD_TEST_COLOR_SCHEME}}-own-windows.png'")

def close():
    add('key Ctrl+w', ("expect-native-dialog-gone" if native else "expect-dialog-gone") + " '^Settings$'", 'focus-main')

if native:
    add('native-dialog-resize 1200 700',
        'expect-native-style textureFolders/0/0 single-line,untruncated,full-row,elide-middle,in-view',
        'expect-native-style textureFolders/1/0 single-line,truncated,full-row,elide-middle,in-view',
        'native-pointer-click textureFolders/0/0', 'expect-native-enabled textureFolderRemove true')
else:
    add('dialog-resize 1200 700', 'expect-widget-property textureFolders horizontalScrollBarPolicy ScrollBarAlwaysOff', 'focus-widget textureFolders', 'key Home', 'key Down', 'key Up',
        'expect-widget-property textureFolderRemove enabled true')
capture('folders-wide')
if native:
    add('native-dialog-resize 950 650',
        'expect-native-style textureFolders/0/0 single-line,untruncated,full-row,elide-middle,in-view',
        'expect-native-style textureFolders/1/0 single-line,truncated,full-row,elide-middle,in-view')
else:
    add('dialog-resize 950 650')
capture('folders-narrow')
# The shared selection handler removes only the selected folder, not its files.
add('native-pointer-click textureFolderRemove' if native else "button '^Remove$'")
close()
add('command tools.preferences.textures', "focus-dialog '^Settings$'")
if native:
    add('expect-native-style textureFolders/0/0 single-line,truncated,full-row,elide-middle,in-view')
add("expect-setting textures/allowNetwork false")
capture('folders-reopened')
close()
add('expect-model-bodies 1 48000')
print('# SPDX-License-Identifier: MIT\n' + '\n'.join(lines))
