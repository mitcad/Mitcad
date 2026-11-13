# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveComponent.cpp app/browser core/model/src/document
# check-all sources: core/model-api/src/components.rs tools/cli app/files/SaveComponentTarget.cpp
# Input-driven Windows UI test of Save Component as Design (mitcad#1057) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-save-component-test.sh does on Linux, on a design that mitcad-cli
# makes: A:1 (a 10 mm cube at x = 100), Box:1 (a cube and a sketch with a
# linked projection of A's top face) and part:1 (a design inserted linked).
#   1. Box:1's browser row has Save Component as Design... right after
#      Create Drawing...; A:1's has it too; the root's row and the linked
#      part:1's do not.
#   2. Box:1: the dialog lists the projection of A:1, frozen; Save... with
#      the proposed name writes Box.mitcad in the design's folder, without
#      the link to A:1.
#   3. Box:1 again, Save... over the design's own file by a name that
#      differs only in case (BENCH.mitcad), the replace question answered
#      Yes: refused with a warning, bench.mitcad byte-identical
#      (mitcad#1148).
#   4. Box:1 again with Replace with a link to the new file: Box2.mitcad
#      written and Box:1 linked to it; its row then has no Save Component
#      as Design... (a linked component).
#   5. A:1 with Replace with a link, the file given outside the design's
#      folder: the dialog says the link would not be followed; Save into
#      the Project and Link (Enter) writes parts\A.mitcad beside the
#      design and links it, nothing outside (mitcad#1739).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-save-component-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-save-component'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The app logs paths with forward slashes.
$workLogged = $work -replace '\\', '/'
# The entries' texts, their ellipsis one character (kept out of this file's
# bytes: Windows PowerShell reads a file without a BOM as ANSI).
$ellipsis = [string][char]0x2026
$entry = 'Save Component as Design' + $ellipsis
$drawing = 'Create Drawing' + $ellipsis

$partScript = Join-Path $work 'part.json'
Set-Content -Encoding ascii $partScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 5, "width": 5, "height": 5, "operation": "new_body"}}
]
'@
$made = & $Cli run $partScript --save (Join-Path $work 'part.mitcad')
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli (part): $($made -join ' / ')" }

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'bench.mitcad'
# The folder the inserted design is found in. A placeholder of its own and
# a case-sensitive Replace: -replace ignores case and once also replaced the
# key "base", which mitcad-cli refused (mitcad#1071).
$base = $workLogged
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "create_component", "name": "A", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "create_component", "name": "Box", "transform": {"translation": [0, 50, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.project", "sketch": "F3", "source": "F1:top", "body": "F1.b0",
    "linked": true, "occurrence": "O1"},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "insert_component", "path": "part.mitcad", "base": "@BASE@",
    "transform": {"translation": [0, -50, 0]}}
]
'@).Replace('@BASE@', $base)
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Menu-Entries: the entries of the context menu logged last.
function Menu-Entries { return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ') }
function Expect-Has([string]$Entry) {
  if (@(Menu-Entries) -notcontains $Entry) { Ui-Fail "'$Entry' is not in the context menu: $((Menu-Entries) -join ' | ')" }
  Write-Host "ok   the context menu has $Entry"
}
function Expect-Lacks([string]$Entry) {
  if (@(Menu-Entries) -contains $Entry) { Ui-Fail "'$Entry' is in the context menu: $((Menu-Entries) -join ' | ')" }
  Write-Host "ok   the context menu has no $Entry"
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step 'choose it' {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Row-Menu "Browser Root/...": the context menu of a browser row.
function Row-Menu([string]$Row) {
  Ui-FocusMain
  Ui-Mark
  Ui-Step "right-click $Row"             { Ui-ClickLogged $Row 3 }
  Ui-ExpectNew 'Context menu: ' "the menu of $Row opened"
}
function Close-Menu {
  Ui-Step 'close the menu (Esc)'         { Ui-Key 'Escape' }
  Ui-Step 'clear the selection (Esc)'    { Ui-Key 'Escape' }
}

Ui-StartApp 'save-component' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Box:1 at' 'the browser lists Box:1'

Write-Host '--- 1. Only the rows of the design''s own components have it'
Row-Menu 'Browser Root/Box:1'
Expect-Has $entry
$entries = Menu-Entries
$at = [array]::IndexOf($entries, $drawing)
if ($at -lt 0 -or $entries[$at + 1] -ne $entry) { Ui-Fail "$entry does not follow $($drawing): $($entries -join ' | ')" }
Write-Host "ok   it follows $drawing"
Close-Menu
Row-Menu 'Browser Root/A:1'
Expect-Has $entry
Close-Menu
Row-Menu 'Browser Root'
Expect-Lacks $entry
Close-Menu
Ui-ExpectLog 'Browser Root/part:1 at' 'the browser lists the linked part:1'
Row-Menu 'Browser Root/part:1'
Expect-Lacks $entry
Close-Menu

Write-Host '--- 2. Box:1 as a design: the projection of A:1 listed and frozen'
Row-Menu 'Browser Root/Box:1'
Ui-Mark
Menu-Choose $entry
Ui-ExpectNew 'Save Component as Design dialog opened: Box; references: ' 'the dialog opened'
Ui-ExpectNew 'its projection of A:1, frozen at its current geometry' 'the projection of A:1 is listed, frozen'
Ui-FocusDialog '^Save Component as Design$'
Ui-Mark
Ui-Step 'Save... (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Save Component as Design dialog: replace with a link: no' 'not replaced'
Ui-Step 'the proposed name (Enter)' {
  Ui-FocusDialog '^Save Design File$'
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew "Saved component Box as design $workLogged/Box.mitcad: 1 reference(s), replaced: no" 'written under the component''s name in the design''s folder' 60
$saved = Join-Path $work 'Box.mitcad'
if (-not (Test-Path $saved)) { Ui-Fail 'Box.mitcad was not written' }
$text = Get-Content -Raw $saved
if (-not $text.Contains('"type": "sketch"')) { Ui-Fail 'Box.mitcad has no sketch' }
if ($text.Contains('"source": "O1"')) { Ui-Fail 'Box.mitcad still links to A:1' }
Write-Host 'ok   the new design holds the sketch, its projection unlinked'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 3. Not over the design''s own file by a name that differs only in case (mitcad#1148)'
$designBytes = (Get-FileHash $design).Hash
Row-Menu 'Browser Root/Box:1'
Ui-Mark
Menu-Choose $entry
Ui-ExpectNew 'Save Component as Design dialog opened: Box; references: ' 'the dialog opened'
Ui-FocusDialog '^Save Component as Design$'
Ui-Mark
Ui-Step 'Save... (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Save Component as Design dialog: replace with a link: no' 'not replaced'
Ui-FocusDialog '^Save Design File$'
# The file dialog's overwrite question has the dialog's title.
$fileDialog = Ui-Windows '^Save Design File$'
Ui-Step 'the design''s own file in capitals' {
  Ui-Key 'ctrl+a'
  Ui-Type (Join-Path $work 'BENCH.mitcad')
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
}
Ui-FocusDialog '^Save Design File$' -Except $fileDialog
Ui-Step 'replace it (Alt+Y)'             { Ui-Key 'alt+y' }
Ui-ExpectNew "Save Component as Design refused: " 'refused as the design''s own file' 20
Ui-FocusDialog '^Save Component as Design$'
Ui-Step 'close the warning (Enter)'      { Ui-Key 'Return' }
Ui-FocusMain
if (-not (Ui-NewLines | Where-Object { $_.Contains("is this design's own file") })) {
  Ui-Fail "not refused as the design's own file: $((Ui-NewLines) -join ' / ')"
}
if (Ui-NewLines | Where-Object { $_.Contains('Saved component Box as design') }) { Ui-Fail 'Box was saved over the design' }
if ((Get-FileHash $design).Hash -ne $designBytes) { Ui-Fail 'bench.mitcad changed' }
Write-Host 'ok   bench.mitcad is byte-identical'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 4. Replace with a link to the new file'
Row-Menu 'Browser Root/Box:1'
Ui-Mark
Menu-Choose $entry
Ui-ExpectNew 'Save Component as Design dialog opened: Box; references: ' 'the dialog opened'
Ui-FocusDialog '^Save Component as Design$'
# The dialog made the active window before its mnemonic: Qt takes a window
# shortcut such as Alt+R only in the active window, and posted input never
# activates one. Beside other tests the dialog was not active yet when the
# key came: the app logged "R with alt", the box stayed unchecked, and
# Enter (a key of the dialog itself) saved without the link. Activation
# alone still lost the key now and then (0.57.0 and 0.59.0-rc3), so the box
# is checked before Enter: Alt+R again, after activating again, until the
# app logs it checked.
function Check-Replace {
  $checked = $false
  for ($try = 1; $try -le 4 -and -not $checked; $try++) {
    [UiInput]::Note = ''
    $active = [UiInput]::Activate($script:UiTarget)
    if ([UiInput]::Note) { Write-Host "note: activating the dialog: $([UiInput]::Note)" }
    if (-not $active) { Write-Host 'note: the dialog is not the active window' }
    Ui-SyncOr 300
    $before = Ui-CountLog 'replace with a link checked: yes'
    Ui-Key 'alt+r'
    $deadline = (Get-Date).AddSeconds(3)
    while ((Get-Date) -lt $deadline -and (Ui-CountLog 'replace with a link checked: yes') -le $before) {
      Start-Sleep -Milliseconds 100
    }
    $checked = (Ui-CountLog 'replace with a link checked: yes') -gt $before
    if (-not $checked) { Write-Host "note: Alt+R did not check the box (try $try)" }
  }
  if (-not $checked) { Ui-Fail 'Alt+R never checked Replace with a link' }
}
Check-Replace
Ui-Mark
Ui-Step 'Replace with a link (Alt+R), Save... (Enter)' { Ui-Key 'Return' }
Ui-ExpectNew 'Save Component as Design dialog: replace with a link: yes' 'replaced'
Ui-Step 'another file' {
  Ui-FocusDialog '^Save Design File$'
  Ui-Key 'ctrl+a'
  Ui-Type (Join-Path $work 'Box2.mitcad')
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew "Saved component Box as design $workLogged/Box2.mitcad: 1 reference(s), replaced: yes" 'written and replaced by a link' 60
if (-not (Test-Path (Join-Path $work 'Box2.mitcad'))) { Ui-Fail 'Box2.mitcad was not written' }
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Row-Menu 'Browser Root/Box:1'
Expect-Lacks $entry
Close-Menu

Write-Host '--- 5. Replace with a link to a file outside the design''s folder: said before saving (mitcad#1739)'
$outside = Join-Path $script:UiOut 'elsewhere'
Remove-Item -Recurse -Force $outside -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $outside | Out-Null
Row-Menu 'Browser Root/A:1'
Ui-Mark
Menu-Choose $entry
Ui-ExpectNew 'Save Component as Design dialog opened: A; references: ' 'the dialog opened'
Ui-FocusDialog '^Save Component as Design$'
Check-Replace
Ui-Mark
Ui-Step 'Save... (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Save Component as Design dialog: replace with a link: yes' 'replaced'
Ui-Step 'a file outside the design''s folder' {
  Ui-FocusDialog '^Save Design File$'
  Ui-Key 'ctrl+a'
  Ui-Type (Join-Path $outside 'A.mitcad')
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
}
Ui-ExpectNew 'Save Component as Design outside the project: A.mitcad (folder); choices Save into the Project and Link | Save There and Link | Cancel' 'the dialog offers the parts folder, the file as asked and Cancel' 20
Ui-ExpectNew "Save Component as Design outside the project text: A.mitcad is outside this design's folder, so a link to it is not followed: changes to the file will not come in. Save it into the parts folder beside this design and link that file, or save it where you chose and link it as it is." 'the dialog says in words why and what to do'
Ui-Step 'Save into the Project and Link (Enter)' {
  Ui-FocusDialog '^Save Component as Design$'
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew 'Save Component as Design outside the project: save into the project and link' 'the parts folder chosen'
Ui-ExpectNew 'Save Component as Design: saving into the project as parts/A.mitcad' 'into the parts folder beside the design'
Ui-ExpectNew "Saved component A as design $workLogged/parts/A.mitcad: 0 reference(s), replaced: yes" 'written beside the design and replaced by a link' 60
if (-not (Test-Path (Join-Path $work 'parts\A.mitcad'))) { Ui-Fail 'A.mitcad was not written into the parts folder' }
if (Test-Path (Join-Path $outside 'A.mitcad')) { Ui-Fail 'A.mitcad was written outside the design''s folder too' }
Write-Host 'ok   the file is in the parts folder only'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Row-Menu 'Browser Root/A:1'
Expect-Lacks $entry
Close-Menu

Ui-Finish 'UI Windows save component test'
