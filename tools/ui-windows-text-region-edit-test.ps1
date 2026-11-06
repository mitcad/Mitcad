# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/framework/Command.cpp
# check-all sources: core/model-api/src/sketch.rs core/sketch/src/text.rs core/sketch/src/code.rs tools/cli
# Input-driven Windows UI test of Edit Text from inside a letter
# (mitcad#1369, GitHub mitcad/Mitcad#158) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-text-region-edit-test.sh on Linux, on the same sketch: "Io" in
# the bundled Droid Sans, 20 mm high from (0, 0) without kerning, and a QR
# code of "https://mitcad.org" with 2 mm modules from (30, 0). Zoomed in, a
# right-click inside the I's stroke, the o's stroke and the o's counter
# selects a region, not the outline: its context menu offers Edit Text,
# which opens on the text; a double-click there opens it too; Edit Text
# from the counter changes the text's height. The code's finder pattern's
# dark centre (37, 7) and the light ring round it (33, 7) offer and open
# Edit QR Code (no Edit Text). The points (sketch mm) are worked out in the
# Linux test's header.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-text-region-edit-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-text-region-edit'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$sketchJson = Join-Path $work 'sketch.json'
$design = Join-Path $work 'text.mitcad'
Set-Content -Encoding ascii $sketchJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_text", "sketch": "F1", "text": "Io", "at": [0, 0], "height": 20, "no_kerning": true},
  {"cmd": "sketch.add_code", "sketch": "F1", "text": "https://mitcad.org", "at": [30, 0], "module": 2}
]
'@
$made = & $cli run $sketchJson --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# In-View x y: the sketch point (mm) is in the 3D view, clear of its edges.
function In-View([double]$Sx, [double]$Sy) {
  $area = Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)'
  if (-not $area) { Ui-Fail 'the app did not log its view area' }
  $v = $area -split ' ' | ForEach-Object { [int]$_ }
  $p = Ui-SketchAt $Sx $Sy
  if ($p[0] -le $v[0] + 20 -or $p[0] -ge $v[0] + $v[2] - 20 -or $p[1] -le $v[1] + 20 -or $p[1] -ge $v[1] + $v[3] - 20) {
    Ui-Fail "the sketch point ($Sx, $Sy) is not in the view"
  }
}

# Zoom-InAt x y steps: a fit, then wheel steps in about the sketch point (mm).
function Zoom-InAt([double]$Sx, [double]$Sy, [int]$Steps) {
  Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
  Ui-Sync
  Start-Sleep -Milliseconds 500
  In-View $Sx $Sy
  $p = Ui-SketchAt $Sx $Sy
  Ui-Mark
  [UiInput]::Move($script:UiWindow, $p[0], $p[1], 0)
  Start-Sleep -Milliseconds 80
  for ($step = 1; $step -le $Steps; $step++) {
    Ui-Step "zoom in at ($Sx, $Sy), step $step" { [UiInput]::Wheel($script:UiWindow, $p[0], $p[1], 1) }
  }
  Ui-ExpectNew 'Sketch view ' 'the view zoomed in'
  Ui-Sync
  In-View $Sx $Sy
}

# Menu-Choose "entry": the entry of the context menu logged last, chosen
# with the keys, as the menu opens with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step "choose $Entry" {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}

# RightClick-Region x y command field: a right-click at a sketch point (mm)
# selects the region there; the context menu offers the command, which
# opens on the region's text. Cancelled. Sets $script:Region and
# $script:Picked (the text), as the helpers' output would mix with a
# returned value.
function RightClick-Region([double]$Sx, [double]$Sy, [string]$Command, [string]$Field) {
  $p = Ui-SketchAt $Sx $Sy
  Ui-Mark
  Ui-Step "right-click at ($Sx, $Sy)"      { Ui-Click $p[0] $p[1] 3 }
  Ui-ExpectNew 'Context menu: ' "the context menu at ($Sx, $Sy)"
  $selected = @(Ui-NewLines | Where-Object { $_ -match '^Selected: 1 profile \[profile (.*) of F1\]' }) | Select-Object -Last 1
  if (-not $selected -or $selected -notmatch '^Selected: 1 profile \[profile (.*) of F1\]') {
    Ui-Fail "the right-click at ($Sx, $Sy) selected no profile of F1"
  }
  $region = $Matches[1]
  Write-Host "ok   the right-click selected the region $region"
  Menu-Choose $Command
  Ui-ExpectNew "Panel $Command input " "the $Command panel"
  Ui-ExpectNew "$Command ${Field}: 1 sketch curve [sketch curve " 'on a text'
  $line = @(Ui-NewLines | Where-Object { $_.StartsWith("$Command ${Field}: 1 sketch curve [sketch curve ") }) | Select-Object -Last 1
  if ($line -notmatch '\[sketch curve (t\d+) of F1\]') { Ui-Fail "$Command did not take a text for the region $region" }
  $text = $Matches[1]
  Write-Host "ok   $Command opened on $text"
  Ui-Mark
  Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
  Ui-ExpectNew "Command $Command cancelled" "$Command cancelled"
  $script:Region = $region
  $script:Picked = $text
}

# DoubleClick-Region x y text command: a double-click at a sketch point
# (mm) opens the command on the region's text; under load the clicks may
# come as two single clicks, so it is tried once more. Cancelled.
function DoubleClick-Region([double]$Sx, [double]$Sy, [string]$Text, [string]$Command) {
  $p = Ui-SketchAt $Sx $Sy
  for ($attempt = 1; $attempt -le 2; $attempt++) {
    Ui-Mark
    Ui-WaitIdle
    Ui-DoubleClick $p[0] $p[1]
    Ui-Sync
    if (Ui-Crashed) { Ui-Fail "crashed during the double-click at ($Sx, $Sy)" }
    for ($i = 0; $i -lt 15; $i++) {
      if (@(Ui-NewLines | Where-Object { $_.Contains("Double-click on ${Text}: $Command") }).Count -gt 0) {
        Write-Host "ok   the double-click at ($Sx, $Sy) opened $Command on $Text"
        Ui-ExpectNew 'Panel ' 'its panel'
        Ui-Mark
        Ui-Step 'cancel (Esc)'             { Ui-Key 'Escape' }
        Ui-ExpectNew ' cancelled' 'cancelled'
        return
      }
      Start-Sleep -Milliseconds 200
    }
    Write-Host "note: no 'Double-click on ${Text}: $Command' after the double-click at ($Sx, $Sy) (attempt $attempt)"
    Start-Sleep -Milliseconds 600
  }
  Ui-Fail "the double-click at ($Sx, $Sy) did not open $Command on $Text"
}

Ui-StartApp 'text-region-edit' @('--open', $design)
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'      { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch edited' 30
Ui-ExpectNew 'Sketch view ' 'its view placed'

Write-Host '--- The text: inside the I''s stroke and the o''s stroke and counter'
Zoom-InAt 8 8 3
RightClick-Region 3.389 7.139 'Edit Text' 'Text'
$text = $script:Picked
$stem = $script:Region
RightClick-Region 12.549 5.361 'Edit Text' 'Text'
if ($script:Picked -ne $text) { Ui-Fail "the o's counter gave $($script:Picked), not the text $text" }
$counter = $script:Region
if ($counter -eq $stem) { Ui-Fail "the I and the o's counter are one region ($stem)" }
RightClick-Region 8.799 5.381 'Edit Text' 'Text'
if ($script:Picked -ne $text) { Ui-Fail "the o's stroke gave $($script:Picked), not the text $text" }
if ($script:Region -eq $counter) { Ui-Fail "the o's stroke and counter are one region ($counter)" }
DoubleClick-Region 12.549 5.361 $text 'sketch.edit_text'
DoubleClick-Region 3.389 7.139 $text 'sketch.edit_text'

Write-Host '--- Edit Text from the counter changes that text'
$p = Ui-SketchAt 12.549 5.361
Ui-Mark
Ui-Step 'right-click the counter'          { Ui-Click $p[0] $p[1] 3 }
Ui-ExpectNew 'Context menu: ' 'the context menu'
Menu-Choose 'Edit Text'
Ui-ExpectNew "Edit Text Text: 1 sketch curve [sketch curve $text of F1]" "Edit Text on $text"
Ui-Step 'height 21'                        { Ui-TypeIn 'Panel Edit Text input height' '21' }
Ui-ExpectNew 'Preview Edit Text: ok' 'previewed'
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew "Edited text ${text}: `"Io`", Droid Sans, left" 'the text edited'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'the edit undone'

Write-Host '--- The QR code: its finder pattern''s dark centre and the light ring round it'
Zoom-InAt 35 7 4
RightClick-Region 37 7 'Edit QR Code' 'QR Code'
$code = $script:Picked
if ($code -eq $text) { Ui-Fail "the code's region gave the text $text" }
RightClick-Region 33 7 'Edit QR Code' 'QR Code'
if ($script:Picked -ne $code) { Ui-Fail "the light ring gave $($script:Picked), not the code $code" }
if ("$(Ui-LastMatch '^Context menu: (.*)$')".Contains('Edit Text')) { Ui-Fail 'a code''s region offers Edit Text' }
Write-Host 'ok   a code''s region offers no Edit Text'
DoubleClick-Region 37 7 $code 'sketch.edit_code'
DoubleClick-Region 33 7 $code 'sketch.edit_code'

Ui-Step 'finish sketch (Ctrl+Enter)'       { Ui-Key 'ctrl+Return' }
Ui-ExpectLog 'Sketch finished' 'sketch finished'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows text region edit test'
