# SPDX-License-Identifier: MIT
# check-all sources: app/commands/EnvelopeCommands.cpp app/commands/CreateCommands.cpp app/browser
# check-all sources: core/model/src/document/envelope.rs core/model/src/features/envelope core/model/src/joints.rs
# check-all sources: core/model/src/features/envelope.rs core/model-api/src/components.rs tools/cli
# Input-driven Windows UI test of the Envelope command and of an occurrence
# shown by its envelope on its own (mitcad#1276) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted; the context menu
# stays open in session 0 and takes the keys, as in
# tools/ui-windows-envelope-test.ps1), on designs that mitcad-cli makes:
# a cell with a component Machine:1 (O1) of two boxes (F1.b0, a 200 mm
# cube, and F2.b0 beside it), and a layout with a machine of three bodies
# linked twice (Machine:1, O1, and Machine:2, O2, 400 mm apart; the base
# feature F1 holds F1.b0-F1.b2).
#   1. Docked: Envelope (its test shortcut) with Machine:1 picked in the
#      browser, Automatic Detail Size off and 20 mm: previewed, OK adds
#      Envelope1 (F3) in the root, its body F3.b0 shown with the boxes.
#   2. Envelope1 edited: the panel opens with Machine:1 and 20 mm; 30 mm,
#      OK. Undo takes the edit back, a second undo the envelope.
#   3. Floating: Envelope on Machine:1 with the automatic detail size, OK.
#   4. Show as Envelope... on Machine:2's row (its menu also offers Show
#      All as Envelope...) at 20 mm: Machine:2 alone shows one body (a
#      component of its own, Machine Envelope:2), Machine:1 keeps its
#      three. Show in Full on its row: the three bodies in both again.
#      Undo: Machine:2 by its envelope again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-envelope-command-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-envelope-command'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.envelope' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# Run-Cli script design: mitcad-cli saves the script's design; a failure
# ends the test at once with its output (as in
# tools/ui-windows-component-combine-test.ps1).
function Run-Cli([string]$Script, [string]$Design) {
  $ErrorActionPreference = 'Continue'
  $output = & $Cli run $Script --save $Design 2>&1
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $Design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
}

# The cell: Machine:1 (O1) with Box1 (F1) and Box2 (F2); the root active.
$cellScript = Join-Path $work 'cell.json'
$cell = Join-Path $work 'cell.mitcad'
Set-Content -Encoding ascii $cellScript @'
[
  {"cmd": "create_component", "name": "Machine"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 200, "width": 200, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [250, 0], "length": 50, "width": 200, "height": 100, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
'@
Run-Cli $cellScript $cell

# The machine of three bodies and the layout that links it twice.
$machineScript = Join-Path $work 'machine.json'
$machine = Join-Path $work 'machine.mitcad'
Set-Content -Encoding ascii $machineScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 200, "width": 200, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [250, 0], "length": 50, "width": 200, "height": 100, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-100, 0], "length": 60, "width": 200, "height": 60, "operation": "new_body"}}
]
'@
Run-Cli $machineScript $machine
$layoutScript = Join-Path $work 'layout.json'
$layout = Join-Path $work 'layout.mitcad'
Set-Content -Encoding ascii $layoutScript ((@'
[
  {"cmd": "insert_component", "path": "MACHINE", "name": "Machine"},
  {"cmd": "copy_occurrence", "occurrence": "Machine:1", "transform": {"translation": [0, 400, 0]}}
]
'@) -creplace 'MACHINE', ($machine -replace '\\', '/'))
Run-Cli $layoutScript $layout

# Menu-Entries: the entries of the context menu logged last (the labels'
# ellipsis as three dots).
$ellipsis = [string][char]0x2026
function Menu-Entries {
  return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ' | ForEach-Object { $_.Replace($ellipsis, '...') })
}
# Expect-Entry "entry" $true|$false: whether the context menu has it.
function Expect-Entry([string]$Entry, [bool]$Present) {
  $entries = Menu-Entries
  if (($entries -contains $Entry) -ne $Present) {
    Ui-Fail "'$Entry' $(if ($Present) { 'is not' } else { 'is' }) in the context menu: $($entries -join ' | ')"
  }
  Write-Host "ok   the context menu $(if ($Present) { 'has' } else { 'has no' }) $Entry"
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step $Entry {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Row-Menu "row": the context menu of an occurrence's browser row.
function Row-Menu([string]$Row) {
  Ui-FocusMain
  Ui-Mark
  Ui-Step "right-click $Row"              { Ui-ClickLogged "Browser Root/$Row" 3 }
  Ui-ExpectNew 'Context menu: ' 'the row''s menu'
}
# Expect-Shown @("body in occurrence", ...) "description": the view shows
# exactly these bodies, in any order (the last "Bodies shown: " line).
function Expect-Shown([string[]]$Bodies, [string]$Description) {
  $wanted = ($Bodies | Sort-Object) -join ', '
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $shown = (@("$(Ui-LastMatch '^Bodies shown: (.*)$')" -split ', ') | Sort-Object) -join ', '
    if ($shown -eq $wanted) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 10000)
  Ui-Fail "${Description}: the view shows '$shown', not '$wanted'"
}
# Expect-ShownWith "body" $true|$false "description": whether the view
# shows the body (a body of the root without " in ").
function Expect-ShownWith([string]$Body, [bool]$Present, [string]$Description) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $shown = @("$(Ui-LastMatch '^Bodies shown: (.*)$')" -split ', ')
    if (($shown -contains $Body) -eq $Present) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 10000)
  Ui-Fail "${Description}: the view shows '$($shown -join ', ')'"
}

$boxes = @('F1.b0 in O1', 'F2.b0 in O1')

Ui-StartApp 'envelope-command' @('--open', $cell)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Machine:1 at' 'the browser lists Machine:1'
Expect-Shown $boxes 'the machine''s two boxes shown'

Write-Host '--- 1. Docked: Envelope of Machine:1 at 20 mm'
Ui-Mark
Ui-Step 'envelope (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Command Envelope started' 'the panel opened' 30
Ui-Mark
Ui-Step 'pick Machine:1 in the browser'  { Ui-ClickLogged 'Browser Root/Machine:1' }
Ui-ExpectNew 'Envelope Source: 1 component' 'Machine:1 picked as the source'
Ui-Mark
Ui-Step 'Automatic Detail Size off'      { Ui-ClickLogged 'Panel Envelope input automatic' }
Ui-Step 'detail size 20'                 { Ui-TypeIn 'Panel Envelope input detail' '20' }
Ui-ExpectNew 'Preview Envelope: ok' 'previewed' 60
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added envelope of 1 component in O1, detail size 20 mm' 'the envelope added' 60
Ui-ExpectLog 'Timeline Envelope1 at ' 'Envelope1 in the timeline'
Expect-Shown ($boxes + 'F3.b0') 'the envelope shown beside the boxes'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 2. Envelope1 edited to 30 mm, then undone'
Ui-Mark
Ui-Step 'edit Envelope1 (double-click)'  { Ui-DoubleClickLogged 'Timeline Envelope1' }
Ui-ExpectNew 'Editing F3 with Envelope' 'the envelope opens for editing' 30
Ui-Mark
Ui-Step 'detail size 30'                 { Ui-TypeIn 'Panel Envelope input detail' '30' }
Ui-ExpectNew 'Preview Envelope: ok' 'the edit previewed' 60
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F3' 'the envelope edited' 60
Ui-FocusMain
Ui-Mark
Ui-Step 'undo the edit'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'the edit undone'
Expect-ShownWith 'F3.b0' $true 'the envelope still shown'
Ui-Mark
Ui-Step 'undo the envelope'              { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'the envelope undone'
Expect-Shown $boxes 'only the boxes shown again'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 3. Floating: Envelope of Machine:1 at the automatic detail size'
Ui-StartApp 'envelope-command-floating' @('--open', $cell, '--chrome=floating')
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Machine:1 at' 'the browser lists Machine:1 in the floating layout'
Ui-Mark
Ui-Step 'envelope (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Command Envelope started' 'the panel opened in the floating layout' 30
Ui-Mark
Ui-Step 'pick Machine:1 in the browser'  { Ui-ClickLogged 'Browser Root/Machine:1' }
Ui-ExpectNew 'Envelope Source: 1 component' 'Machine:1 picked as the source'
Ui-ExpectNew 'Preview Envelope: ok' 'previewed' 60
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added envelope of 1 component in O1, detail size automatic' 'the envelope added' 60
Expect-Shown ($boxes + 'F3.b0') 'the envelope shown beside the boxes'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 4. Machine:2 alone by its envelope, then in full'
$first = @('F1.b0 in O1', 'F1.b1 in O1', 'F1.b2 in O1')
$second = @('F1.b0 in O2', 'F1.b1 in O2', 'F1.b2 in O2')
Ui-StartApp 'envelope-occurrence' @('--open', $layout)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Machine:2 at' 'the browser lists Machine:2'
Expect-Shown ($first + $second) 'both machines shown in full'
Row-Menu 'Machine:2'
Expect-Entry 'Show as Envelope...' $true
Expect-Entry 'Show All as Envelope...' $true
Expect-Entry 'Show in Full' $false
Menu-Choose 'Show as Envelope...'
Ui-Step 'detail size 20 (Enter)' {
  Ui-FocusDialog '^Show as Envelope$'
  Ui-Key 'ctrl+a'
  Ui-Type '20'
  Ui-Key 'Return'
}
Ui-FocusMain
Ui-ExpectNew 'Shown Machine:2 as its envelope: detail size 20 mm, 1 body(ies)' 'Machine:2 shown by its envelope' 60
# The base feature of the component made for it: F2.
Expect-Shown ($first + 'F2.b0 in O2') 'Machine:2 one body, Machine:1 its three'
Ui-ExpectLog 'Browser Root/Machine Envelope:2 at' 'Machine:2 placed by a component of its own'

Row-Menu 'Machine Envelope:2'
Expect-Entry 'Show in Full' $true
Menu-Choose 'Show in Full'
Ui-ExpectNew 'Shown Machine Envelope:2 in full: 3 body(ies)' 'Machine:2 shown in full' 60
Expect-Shown ($first + $second) 'both machines in full again'

Ui-FocusMain
Ui-Mark
Ui-Step 'undo'                           { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'one undo'
Expect-Shown ($first + 'F2.b0 in O2') 'Machine:2 by its envelope again'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows envelope command'
