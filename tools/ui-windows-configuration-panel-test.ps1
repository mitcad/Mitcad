# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationBar.cpp app/browser/MainWindowConfigurationPanel.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp app/MainWindow.cpp tools/cli
# Input-driven Windows UI test of the Configuration panel and the left
# column (mitcad#1020) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), in the Floating layout, on two designs mitcad-cli makes:
# a 10 mm cube without options, and a design with three range options
# (SizeX, SizeY, Height) whose cube is patterned 6 x 5 (30 bodies):
#   - without options: no Configuration card;
#   - with them: the Configuration card above the browser's, the bar's
#     choosers in it; Root/Bodies opened, the rows scroll and the
#     browser's card reaches the timeline's (less the gap);
#   - the boundary between the two cards dragged 40 px up: 40 px from the
#     Configuration card to the browser's, remembered, the same after a
#     restart; a double-click on it gives the automatic split again;
#   - the Configuration card collapsed by its chevron: its height to the
#     browser's card; expanded again.
# The Linux counterpart is tools/ui-configuration-panel-test.sh (also the
# Docked layout).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-configuration-panel-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-configuration-panel'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$plainScript = Join-Path $work 'plain.json'
$plain = Join-Path $work 'plain.mitcad'
Set-Content -Encoding ascii $plainScript @"
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$region"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"expect": {"bodies": 1}}
]
"@
$binsScript = Join-Path $work 'bins.json'
$bins = Join-Path $work 'bins.mitcad'
Set-Content -Encoding ascii $binsScript @"
[
  {"cmd": "add_parameter", "name": "SizeX", "unit": "mm", "expression": "42 mm",
   "min": "42 mm", "max": "252 mm", "step": "42 mm"},
  {"cmd": "add_parameter", "name": "SizeY", "unit": "mm", "expression": "42 mm",
   "min": "42 mm", "max": "252 mm", "step": "42 mm"},
  {"cmd": "add_parameter", "name": "Height", "unit": "mm", "expression": "21 mm",
   "min": "7 mm", "max": "84 mm", "step": "7 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$region"}],
    "extent": {"type": "distance", "distance": "Height"}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "rectangular_pattern",
    "objects": {"type": "bodies", "bodies": ["F2.b0"]},
    "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 6, "distance": 20},
    "direction2": {"axis": {"type": "origin", "axis": "y"}, "quantity": 5, "distance": 20},
    "distance_type": "spacing"}},
  {"cmd": "set_configurations", "configurations": {
    "options": ["SizeX", "SizeY", "Height"], "closed": false, "rows": [], "rules": []}},
  {"expect": {"bodies": 30}}
]
"@
foreach ($pair in @(@($plainScript, $plain), @($binsScript, $bins))) {
  $made = & $cli run $pair[0] --save $pair[1]
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $pair[1])) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
}

# Card-Rect name: @(x, y, w, h) of a card in the last "Floating cards:"
# line (main window coordinates), $null when it is not shown.
function Card-Rect([string]$Name) {
  $line = Ui-LastLineWith 'Floating cards:' -Start
  $m = [regex]::Match("$line", "; $Name (\d+),(\d+) (\d+)x(\d+)")
  if (-not $m.Success) { return $null }
  return @([int]$m.Groups[1].Value, [int]$m.Groups[2].Value, [int]$m.Groups[3].Value, [int]$m.Groups[4].Value)
}

function Near([int]$A, [int]$B) { return [Math]::Abs($A - $B) -le 3 }

# Wait-For description { condition }: the condition holds within 10 s.
function Wait-For([string]$Description, [scriptblock]$Condition) {
  for ($i = 0; $i -lt 50; $i++) {
    if (& $Condition) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "$Description (cards: $(Ui-LastLineWith 'Floating cards:' -Start))"
}

function Inside($Point, $Rect) {
  if ($null -eq $Point -or $null -eq $Rect) { return $false }
  return $Point[0] -ge $Rect[0] -and $Point[0] -lt $Rect[0] + $Rect[2] -and
    $Point[1] -ge $Rect[1] -and $Point[1] -lt $Rect[1] + $Rect[3]
}

function Logged-Place([string]$Text) {
  $m = [regex]::Match("$(Ui-LastLineWith "$Text at " -Start)", ' at (\d+),(\d+)$')
  if (-not $m.Success) { return $null }
  return @([int]$m.Groups[1].Value, [int]$m.Groups[2].Value)
}

# Browser-ReachesTimeline: the browser's card ends where the timeline's card
# starts, less the gap; above the message band's row while it has one
# (mitcad#1042).
function Browser-ReachesTimeline {
  $b = Card-Rect 'browserDock'
  $t = Card-Rect 'messageBandRow'
  if ($null -eq $t) { $t = Card-Rect 'timelineDock' }
  return $null -ne $b -and $null -ne $t -and (Near ($b[1] + $b[3]) ($t[1] - 8))
}

function Heights-Are([int]$Config, [int]$Browser) {
  $c = Card-Rect 'configurationDock'
  $b = Card-Rect 'browserDock'
  return $null -ne $c -and $null -ne $b -and (Near $c[3] $Config) -and (Near $b[3] $Browser)
}

# Start-Bins: the design with options, Root/Bodies opened (it opens folded
# when the 30 rows do not fit).
function Start-Bins([string]$Name) {
  Ui-StartApp $Name @('--chrome=floating', '--open', $bins)
  Ui-ExpectLog 'Configuration bar: SizeX = 42 mm' 'the bar shows the options' 20
  Ui-ExpectLog 'Browser arrow Root/Bodies at' "the Bodies folder's arrow"
  Ui-WaitIdle
  Ui-Sync
  # The row's last state: logged in sight before the design was folded,
  # then out of sight.
  if (-not "$(Ui-LastLineWith 'Browser Root/Bodies/Body1 ' -Start)".Contains(' at ')) {
    Ui-Mark
    Ui-Step 'open Bodies'                { Ui-ClickLogged 'Browser arrow Root/Bodies' }
    Ui-ExpectNew 'Browser Root/Bodies/Body1 at' "Body1's row"
  }
}

Write-Host '--- Without options: no Configuration card'
Ui-StartApp 'plain' @('--chrome=floating', '--open', $plain)
Wait-For "the browser's card is placed" { $null -ne (Card-Rect 'browserDock') }
Ui-WaitIdle
Ui-Sync
if ($null -ne (Card-Rect 'configurationDock')) { Ui-Fail 'a Configuration card for a design without options' }
Write-Host 'ok   no Configuration card'

Write-Host '--- With options: the Configuration card above the browser''s, the bar in it'
Start-Bins 'bins'
Ui-ExpectLog 'Configuration panel: shown' 'the panel shows with the options'
Wait-For "the Configuration card is above the browser's" {
  $c = Card-Rect 'configurationDock'
  $b = Card-Rect 'browserDock'
  $null -ne $c -and $null -ne $b -and $c[0] -eq $b[0] -and ($c[1] + $c[3]) -le $b[1]
}
Wait-For "the bar's choosers are in the Configuration card" {
  $c = Card-Rect 'configurationDock'
  (Inside (Logged-Place 'Configuration bar SizeX') $c) -and (Inside (Logged-Place 'Configuration bar SizeY') $c) -and
    (Inside (Logged-Place 'Configuration bar Height') $c)
}
Wait-For 'the 30 body rows scroll' {
  $m = [regex]::Match("$(Ui-LastLineWith 'Browser scroll range ' -Start)", 'range (\d+)$')
  $m.Success -and [int]$m.Groups[1].Value -gt 0
}
Wait-For "the browser's card reaches the timeline's (less the gap)" { Browser-ReachesTimeline }
Ui-WaitIdle
Ui-Sync
$config0 = (Card-Rect 'configurationDock')[3]
$browser0 = (Card-Rect 'browserDock')[3]
Write-Host "ok   at the start: Configuration $config0 px, browser $browser0 px high"

Write-Host '--- The boundary dragged 40 px up, remembered'
$split = Ui-LoggedAt 'Floating split configurationDock/browserDock'
Ui-Mark
Ui-Step 'drag the boundary up'           { Ui-Drag $split[0] $split[1] $split[0] ($split[1] - 40) }
Ui-ExpectNew 'Floating split remembered: configurationDock ' 'the split is remembered'
Wait-For "40 px from the Configuration card to the browser's" { Heights-Are ($config0 - 40) ($browser0 + 40) }
$remembered = Ui-LastMatch 'Floating split remembered: (.*)$'
$config1 = (Card-Rect 'configurationDock')[3]
$browser1 = (Card-Rect 'browserDock')[3]
Ui-StopApp

Start-Bins 'bins-again'
Ui-ExpectLog "Floating split restored: $remembered" 'the split is read at the start' 20
Wait-For 'the cards are as high as before the restart' { Heights-Are $config1 $browser1 }

Write-Host '--- A double-click on the boundary: the automatic split'
Ui-Mark
Ui-Step 'double-click the boundary'      { Ui-DoubleClickLogged 'Floating split configurationDock/browserDock' }
Ui-ExpectNew 'Floating split: automatic' "the user's split is forgotten"
Wait-For 'the heights of the start again' { Heights-Are $config0 $browser0 }

Write-Host '--- The Configuration card collapsed and expanded'
Ui-Mark
Ui-Step 'collapse the Configuration card' { Ui-ClickLogged 'Configuration panel chevron' }
Ui-ExpectNew 'Configuration panel: collapsed, remembered' 'collapsed, remembered'
Wait-For "its height goes to the browser's card" {
  $c = Card-Rect 'configurationDock'
  $b = Card-Rect 'browserDock'
  $null -ne $c -and $null -ne $b -and $c[3] -lt 40 -and $b[3] -ge ($browser0 + $config0 - $c[3] - 3)
}
Ui-Mark
Ui-Step 'expand the Configuration card'  { Ui-ClickLogged 'Configuration panel chevron' }
Ui-ExpectNew 'Configuration panel: expanded, remembered' 'expanded, remembered'
Wait-For 'the heights of the start again' { Heights-Are $config0 $browser0 }

Ui-Finish 'UI Windows configuration panel test'
