# SPDX-License-Identifier: MIT
# check-all sources: app/browser core/model/src/document/envelope.rs core/model/src/features/envelope
# check-all sources: core/model/src/features/envelope.rs core/model-api/src/components.rs tools/cli
# Input-driven Windows UI test of a linked component shown by its envelope
# (mitcad#1271) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted; the context menu stays open in session 0 and takes the
# keys, as in tools/ui-windows-move-copy-menu-test.ps1), on designs that
# mitcad-cli makes: a machine of three bodies (a 200 mm frame with a
# 100 mm tunnel under it and two blocks beside it, 40 and 50 mm away) and
# a layout with the machine inserted linked (Machine:1, O1; its base
# feature F1 holds the three bodies F1.b0-F1.b2).
#   1. Machine:1's browser row offers Show as Envelope... and no Show in
#      Full; Show as Envelope... with a detail size of 20 mm: the layout
#      shows one body of the machine (its envelope, the three pieces
#      further apart than 20 mm in one body) instead of three.
#   2. Its row then offers Show in Full: the three bodies again.
#   3. Undo: the envelope again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-envelope-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-envelope'),
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

# The machine: Box1 (F1) 200 x 200 x 200 with the tunnel cut under it
# (Box2, F2), Box3 (F3) and Box4 (F4) beside it.
$machineScript = Join-Path $work 'machine.json'
$machine = Join-Path $work 'machine.mitcad'
Set-Content -Encoding ascii $machineScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 200, "width": 200, "height": 200, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [50, 0], "length": 100, "width": 200, "height": 150, "operation": "cut", "participants": ["F1.b0"]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [250, 0], "length": 50, "width": 200, "height": 100, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-100, 0], "length": 60, "width": 200, "height": 60, "operation": "new_body"}}
]
'@
$made = & $Cli run $machineScript --save $machine
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $machine)) { Ui-Fail "mitcad-cli (machine): $($made -join ' / ')" }

$layoutScript = Join-Path $work 'layout.json'
$layout = Join-Path $work 'layout.mitcad'
Set-Content -Encoding ascii $layoutScript ((@'
[
  {"cmd": "insert_component", "path": "MACHINE", "name": "Machine"}
]
'@) -creplace 'MACHINE', ($machine -replace '\\', '/'))
$made = & $Cli run $layoutScript --save $layout
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $layout)) { Ui-Fail "mitcad-cli (layout): $($made -join ' / ')" }

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
# Row-Menu: the context menu of Machine:1's browser row.
function Row-Menu {
  Ui-FocusMain
  Ui-Mark
  Ui-Step 'right-click Machine:1'        { Ui-ClickLogged 'Browser Root/Machine:1' 3 }
  Ui-ExpectNew 'Context menu: ' 'the row''s menu'
}
# Expect-Shown "bodies" "description": the view shows exactly these bodies
# (the last "Bodies shown: " line).
function Expect-Shown([string]$Bodies, [string]$Description) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $shown = Ui-LastMatch '^Bodies shown: (.*)$'
    if ($shown -eq $Bodies) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 10000)
  Ui-Fail "${Description}: the view shows '$shown', not '$Bodies'"
}

$full = 'F1.b0 in O1, F1.b1 in O1, F1.b2 in O1'

Ui-StartApp 'envelope' @('--open', $layout)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Machine:1 at' 'the browser lists Machine:1'
Expect-Shown $full 'the machine''s three bodies shown'

Write-Host '--- 1. Show as Envelope... at 20 mm'
Row-Menu
Expect-Entry 'Show as Envelope...' $true
Expect-Entry 'Show in Full' $false
Menu-Choose 'Show as Envelope...'
Ui-Step 'detail size 20 (Enter)' {
  Ui-FocusDialog '^Show as Envelope$'
  Ui-Key 'ctrl+a'
  Ui-Type '20'
  Ui-Key 'Return'
}
Ui-FocusMain
Ui-ExpectNew 'Shown Machine:1 as its envelope: detail size 20 mm, 1 body(ies)' 'the machine shown by its envelope' 60
Expect-Shown 'F1.b0 in O1' 'one body of the machine shown: its envelope'

Write-Host '--- 2. Show in Full'
Row-Menu
Expect-Entry 'Show as Envelope...' $true
Expect-Entry 'Show in Full' $true
Menu-Choose 'Show in Full'
Ui-ExpectNew 'Shown Machine:1 in full: 3 body(ies)' 'the machine shown in full' 60
Expect-Shown $full 'the three bodies shown again'

Write-Host '--- 3. Undo: the envelope again'
Ui-FocusMain
Ui-Mark
Ui-Step 'undo'                           { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'one undo'
Expect-Shown 'F1.b0 in O1' 'the envelope shown again'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows envelope'
