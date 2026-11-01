# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/framework/FloatingLayout.cpp app/MainWindow.cpp app/browser/BrowserController.cpp tools/cli
# Input-driven Windows UI test of the message band and the message history
# (mitcad#1042) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-message-band-test.sh does on Linux, on a block
# whose second fillet fails (r 50):
#   1. Floating: a click on the failures summary shows Fillet2's error in
#      the band right above the timeline, as wide as it; it is still shown
#      8 s later; its close button turns it into the stub of the latest
#      message, which keeps the band's row.
#   2. The error again, then Extrude (E): the next command closes it.
#   3. Esc cancels Extrude: the hint "Command canceled." fades after about
#      6 s and leaves its stub.
#   4. A click on the stub opens the history (child widget of the window,
#      not a popup: popups close at once in session 0): the newest message
#      first, the error below it; a click on the error's row selects Fillet2
#      without saying its error again. Esc closes the history.
#   5. Docked: the status bar's history button opens the same history.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-message-band-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-message-band'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up; Fillet1 (F3) r 1 on a
# vertical edge; Fillet2 (F4) r 50 fails.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'block.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 1}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c3[c2,c4])|F2:side(c4[c3,c1])}"], "radius": 50}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Expect-Line "regex" "description" [seconds]: a line since the mark
# matches; the last one is returned.
function Expect-Line([string]$Pattern, [string]$Description, [int]$Seconds = 6) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $found = $null
    foreach ($line in @(Ui-NewLines)) { if ($line -match $Pattern) { $found = $line } }
    if ($found) { Write-Host "ok   $Description"; return $found }
    Start-Sleep -Milliseconds 100
  } while ($clock.ElapsedMilliseconds -lt $Seconds * 1000)
  Ui-Fail "${Description}: no line since the mark matches '$Pattern'"
}

# Not-Since "regex" "description": no line since the mark matches.
function Not-Since([string]$Pattern, [string]$Description) {
  foreach ($line in @(Ui-NewLines)) { if ($line -match $Pattern) { Ui-Fail "${Description}: '$line'" } }
  Write-Host "ok   $Description"
}

# Select-FromHistory "description": the history open (since the mark), a
# click on Fillet2's error row selects Fillet2 and closes it, without its
# error said again.
function Select-FromHistory([string]$Where) {
  Expect-Line '^Message history: \d+ entries\r?$' "${Where}: the history opened" | Out-Null
  $historyRow = Expect-Line '^Message history row \d+: error Fillet2: .* at \d+,\d+\r?$' "${Where}: Fillet2's error is in it"
  $rowText = ($historyRow.TrimEnd("`r") -replace ' at \d+,\d+$', '')
  Ui-Mark
  Ui-Step 'click Fillet2''s error'        { Ui-ClickLogged $rowText }
  Ui-ExpectNew 'Message history: selected F4' "${Where}: the row is about Fillet2"
  Ui-ExpectNew 'Revealed Fillet2' "${Where}: Fillet2 selected"
  Ui-ExpectNew 'Message history: closed' "${Where}: the history closed"
  Ui-Sync
  Not-Since '^Message band: error Fillet2' "${Where}: the error is not said again"
}

Ui-StartApp 'message-band' @('--chrome=floating', '--open', $design)
Ui-ExpectLog 'Failed features: Fillet2: ' 'Fillet2 failed'
Ui-ExpectLog 'Failures button at' 'the failures summary is placed'

Write-Host '--- 1. Floating: an error in the band above the timeline stays until it is closed'
Ui-Mark
Ui-Step 'click the failures summary'     { Ui-ClickLogged 'Failures button' }
Ui-ExpectNew 'Revealed Fillet2' 'the summary went to Fillet2'
Expect-Line '^Message band full: error Fillet2: ' 'the band shows the error' | Out-Null
# The band was placed when Fillet2's error first showed on opening, before
# the mark, and a placement is logged only when it changes: the newest
# placement in the whole log.
$placed = $null
foreach ($line in @(Ui-LogLines)) { if ($line -match '^Message band placed: ') { $placed = $line } }
if ($placed -notmatch '^Message band placed: full ') { Ui-Fail "the band is placed: '$placed'" }
Write-Host 'ok   the band is placed'
if ($placed -notmatch ': full (-?\d+),(-?\d+) (\d+)x(\d+), row (\d+), timeline (-?\d+),(-?\d+) (\d+)x(\d+)') {
  Ui-Fail "the band's place: '$placed'"
}
$band = @([int]$Matches[1], [int]$Matches[2], [int]$Matches[3], [int]$Matches[4])
$row = [int]$Matches[5]
$timeline = @([int]$Matches[6], [int]$Matches[7], [int]$Matches[8], [int]$Matches[9])
if ($band[0] -ne $timeline[0] -or $band[2] -ne $timeline[2]) { Ui-Fail "the band is not as wide as the timeline: '$placed'" }
Write-Host "ok   as wide as the timeline ($($band[2]) px)"
if ($band[1] + $band[3] -gt $timeline[1]) { Ui-Fail "the band is not above the timeline: '$placed'" }
if ($row -lt $timeline[3]) { Ui-Fail "the band's row is lower than the timeline: '$placed'" }
Write-Host "ok   right above the timeline, its row $row px"
Ui-ExpectLog 'Message band close at' 'the close button is placed'
Start-Sleep -Seconds 8
Ui-Sync
Not-Since '^Message band stub: ' 'the error is still shown 8 s later'
Ui-Mark
Ui-Step 'close the message (x)'          { Ui-ClickLogged 'Message band close' }
Expect-Line '^Message band stub: error Fillet2: .* \(closed\)\r?$' 'closed: the stub of the error' | Out-Null
$stub = Expect-Line '^Message band placed: stub ' 'the stub is placed'
if ($stub -notmatch ", row $row, timeline ") { Ui-Fail "the stub does not keep the band's row: '$stub'" }
Write-Host 'ok   the stub keeps the band''s row: the cards above do not move'

Write-Host '--- 2. The next command closes an error'
Ui-Mark
Ui-Step 'click the failures summary'     { Ui-ClickLogged 'Failures button' }
Expect-Line '^Message band full: error Fillet2: ' 'the error shown again' | Out-Null
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started'
Expect-Line '^Message band stub: error Fillet2: .* \(next command\)\r?$' 'the next command closed the error' | Out-Null

Write-Host '--- 3. A hint fades after about 6 s and leaves its stub'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'
Expect-Line '^Message band full: hint Command canceled\.\r?$' 'the hint in the band' | Out-Null
$shown = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds 3
Ui-Sync
Not-Since '^Message band stub: ' 'the hint is still shown 3 s later'
Expect-Line '^Message band stub: hint .* \(faded\)\r?$' 'the hint faded to its stub' 12 | Out-Null
$faded = [int]($shown.ElapsedMilliseconds / 1000)
if ($faded -lt 4 -or $faded -gt 10) { Ui-Fail "the hint faded after $faded s, not about 6 s" }
Write-Host "ok   faded after about $faded s"
Ui-ExpectLog 'Message stub at' 'the stub is placed'

Write-Host '--- 4. The history: newest first; a feature''s row selects it'
Ui-Mark
Ui-Step 'click the stub'                 { Ui-ClickLogged 'Message stub' }
Expect-Line '^Message history row 1: hint Command canceled\. at ' 'the newest message is row 1' | Out-Null
$errorRow = Expect-Line '^Message history row \d+: error Fillet2: ' 'Fillet2''s error is in it'
if ($errorRow -notmatch '^Message history row (\d+):' -or [int]$Matches[1] -le 1) { Ui-Fail "the error is not below the newer hint: '$errorRow'" }
Write-Host 'ok   the error below the newer hint'
Select-FromHistory 'Floating'
Ui-Mark
Ui-Step 'click the stub'                 { Ui-ClickLogged 'Message stub' }
Ui-ExpectNew 'Message history: ' 'the history opened again'
Ui-Step 'close it (Esc)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Message history: closed' 'Esc closed it'

Write-Host '--- 5. Docked: the status bar''s history button'
Ui-StartApp 'message-band-docked' @('--chrome=docked', '--open', $design)
Ui-ExpectLog 'Failures button at' 'the failures summary is placed'
Ui-ExpectLog 'Message history button at' 'the history button is in the status bar'
Ui-Mark
Ui-Step 'click the failures summary'     { Ui-ClickLogged 'Failures button' }
Ui-ExpectNew 'Status error: Fillet2: ' 'the status line says it as an error'
Expect-Line '^Message band: error Fillet2: ' 'the message is recorded' | Out-Null
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'click the history button'       { Ui-ClickLogged 'Message history button' }
Select-FromHistory 'Docked'

Ui-Finish 'Windows UI message band test'
