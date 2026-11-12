# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp core/model-api/src/components.rs tools/cli
# Input-driven Windows UI test of a design as it opens (mitcad#426) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux test is the last part of tools/ui-browser-collapse-test.sh:
#   1. A design saved with Bracket:1 active (mitcad-cli) opens with Bracket
#      active, as saved (mitcad#1678), unmodified and no undo step; it fits
#      the browser and opens expanded.
#   2. A design of 16 components, each with a body (52 rows, more than the
#      browser shows in the 1028 x 749 window of session 0), opens with the
#      root open and its children in sight, folded; an arrow opens one.
#   3. Collapse All there keeps the root open, its children in sight.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-open-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-open'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin -StepPause 0

# Place-Valid "Browser <path>": the row's place was logged and has not gone
# out of sight since (BrowserPanel::logLayout).
function Place-Valid([string]$Text) {
  $last = "$(Ui-LogLines | Where-Object { $_.Contains("$Text at ") -or $_.EndsWith("$Text out of sight") } |
    Select-Object -Last 1)"
  return ($last.Length -gt 0) -and -not $last.EndsWith(' out of sight')
}

# Since-Mark "text": the lines logged since the mark that contain the text.
function Since-Mark([string]$Text) {
  return @(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) })
}

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# Bracket (C1) with Cylinder1 (F1), left active when saved.
$activeScript = Join-Path $work 'active.json'
$active = Join-Path $work 'active.mitcad'
Set-Content -Encoding ascii $activeScript @(
  '[',
  '  {"cmd": "create_component", "name": "Bracket"},',
  '  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 10, "height": 10, "operation": "new_body"}},',
  '  {"expect": {"query": {"query": "components"}, "result": {"active": "C1"}}}',
  ']')
$output = & $cli run $activeScript --save $active
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $active)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
if (-not (Select-String -Quiet -Path $active -Pattern '"active_component": *"C1"')) { Ui-Fail 'the file does not store Bracket active' }

# Part1..Part16, each a component with a cylinder of its own: 4 rows of the
# root and 3 of each part (its row, Bodies, the body).
$largeScript = Join-Path $work 'large.json'
$large = Join-Path $work 'large.mitcad'
$lines = @('[')
for ($i = 1; $i -le 16; $i++) {
  $lines += '  {"cmd": "create_component", "name": "Part' + $i + '", "transform": {"translation": [' + ($i * 20) + ', 0, 0]}},'
  $lines += '  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 10, "height": 10, "operation": "new_body"}},'
  $lines += '  {"cmd": "activate_component", "component": "Root"},'
}
$lines += '  {"expect": {"bodies": 16}}'
$lines += ']'
Set-Content -Encoding ascii $largeScript $lines
$output = & $cli run $largeScript --save $large
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $large)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Write-Host '--- 1. Saved with Bracket active: opened with Bracket active (mitcad#1678)'
Ui-StartApp 'browser-open-active' @('--open', $active)
Ui-ExpectLog 'Opened with C1 active, as saved (unmodified)' 'Bracket is active as saved, the design unmodified'
Ui-ExpectLog 'Browser opened expanded: ' 'a small design opens expanded'
if (Ui-LogLines | Where-Object { $_.Contains('Browser opened folded') }) { Ui-Fail 'the small design opened folded' }
Ui-ExpectLog 'Browser Root/Bracket:1/Bodies/Body1 at' "Bracket's rows are open"
Ui-Sync
# The empty design of the start had its Component1 active: the view's
# active component is Bracket's now.
Ui-ExpectLog 'View inactive ghosts: 0 bodies (active Bracket:1)' 'the view has Bracket active'
if (Ui-LogLines | Where-Object { $_.StartsWith('Opened with the root active') }) { Ui-Fail 'the design opened with the root active' }
Write-Host 'ok   not with the root active'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
# A moment for an undo step that should not come, as the Linux test waits.
Start-Sleep -Milliseconds 1000
Ui-Sync
if ((Since-Mark 'Undo: Activate').Count -gt 0) { Ui-Fail 'the activation is an undo step' }
if ((Since-Mark 'Activated ').Count -gt 0) { Ui-Fail 'a component was activated by hand' }
Write-Host 'ok   no undo step'

Write-Host '--- 2. A design longer than the browser opens folded'
Ui-StartApp 'browser-open-large' @('--open', $large)
Ui-ExpectLog 'Browser opened folded: ' 'the long tree opens folded'
Ui-Sync
Ui-ExpectLog 'Browser Root/Part1:1 at' "the root's children are in sight"
foreach ($row in @('Root/Part1:1', 'Root/Part2:1', 'Root/Named Views', 'Root/Origin')) {
  if (-not (Place-Valid "Browser $row")) { Ui-Fail "$row is not in sight" }
  if (-not (Place-Valid "Browser arrow $row")) { Ui-Fail "$row has no arrow in sight" }
}
Write-Host 'ok   the first parts and the folders are in sight with their arrows'
if (Place-Valid 'Browser Root/Part1:1/Bodies') { Ui-Fail 'Part1 is open' }
Write-Host 'ok   Part1 is folded'
Ui-Mark
Ui-Step 'open Part1'                     { Ui-ClickLogged 'Browser arrow Root/Part1:1' }
Ui-ExpectNew 'Browser Root/Part1:1/Bodies at' 'its Bodies folder is in sight'
if ((Since-Mark 'Browser Root/Part1:1/Bodies/Body').Count -gt 0) { Ui-Fail "Part1's Bodies folder is open" }
Write-Host 'ok   and folded'

Write-Host '--- 3. Collapse All keeps the root open'
Ui-Mark
Ui-Step "right-click Part1's Bodies"     { Ui-ClickLogged 'Browser Root/Part1:1/Bodies' 3 }
Ui-ExpectNew 'Collapse All' 'the menu has Collapse All'
# Collapse All is the menu's last entry.
Ui-Step 'choose Collapse All'            { Ui-Key 'Up'; Ui-Key 'Return' }
Ui-ExpectNew 'Browser collapsed all' 'Collapse All ran'
Ui-ExpectNew 'Browser Root/Part1:1/Bodies out of sight' 'Part1 folded again'
if (-not (Place-Valid 'Browser Root/Part1:1')) { Ui-Fail "the root's children left sight" }
if (-not (Place-Valid 'Browser Root')) { Ui-Fail 'the top row is gone' }
Write-Host 'ok   the root stays open, its children in sight'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows browser open test'
