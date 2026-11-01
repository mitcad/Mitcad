# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveComponent.cpp app/browser core/model/src/document
# check-all sources: core/model-api/src/components.rs tools/cli
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
#   3. Box:1 again with Replace with a link to the new file: Box2.mitcad
#      written and Box:1 linked to it; its row then has no Save Component
#      as Design... (a linked component).
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

Write-Host '--- 3. Replace with a link to the new file'
Row-Menu 'Browser Root/Box:1'
Ui-Mark
Menu-Choose $entry
Ui-ExpectNew 'Save Component as Design dialog opened: Box; references: ' 'the dialog opened'
Ui-FocusDialog '^Save Component as Design$'
Ui-Mark
Ui-Step 'Replace with a link (Alt+R), Save... (Enter)' { Ui-Key 'alt+r', 'Return' }
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

Ui-Finish 'UI Windows save component test'
