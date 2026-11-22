# SPDX-License-Identifier: MIT
# check-all sources: app/files/DrawingDialog.cpp app/files/DrawingSettings.cpp app/files/MainWindowDrawing.cpp
# check-all sources: core/model/src/drawing core/model-drawing/src core/model-api/src/drawing.rs geometry/src/drawing.cpp tools/cli
# Input-driven Windows UI test of File > Export Drawing (mitcad#471) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), on a
# design mitcad-cli makes: a 120 x 80 x 10 plate with four through holes of
# 8.5 (Body1, F2.b0) and a 120 x 20 x 30 bar beside it (Body2, F4.b0).
#   1. Export Drawing (a shortcut of the test's) opens its dialog with the
#      defaults (PDF, A4, the scale to fit); a typed .svg file makes the
#      format SVG; the sheet's drop-down turned to A3 (its wheel: the list
#      cannot open in session 0); OK writes the drawing: 3 views of the 2
#      bodies on A3 at 1:1, as the log says, with the views, the plate's
#      size and the holes' diameter in the file.
#   2. The dialog opens again with the choices kept (SVG, A3).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'file.export_drawing' = 'F9' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'drawing.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 80},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F2:end(RECT)",
                  "points": [[10, 10, 10], [110, 10, 10], [10, 70, 10], [110, 70, 10]]},
    "diameter": 8.5, "extent": {"type": "through_all"}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [0, 90], "width": 120, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F4", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Dialog-At "name": where the dialog logged an input, in its own window.
function Dialog-At([string]$Name) { return Ui-LoggedAt "Drawing dialog $Name" }

Ui-StartApp 'drawing' @('--open', $design)
Ui-ExpectLog 'drawing.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: F2.b0' 'the bodies shown'

Write-Host '--- 1. Export Drawing: SVG on A3'
$svg = Join-Path $work 'drawing.svg'
Ui-Mark
Ui-Step 'export drawing (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Drawing dialog opened: format pdf, sheet A4, scale fit' 'the dialog opened with the defaults'
Ui-ExpectNew 'Drawing dialog OK at' 'the dialog laid out'
Ui-Step 'focus the dialog'               { Ui-FocusDialog '^Export Drawing$' }
Ui-Step 'type the SVG file'              { Ui-Key 'ctrl+a'; Ui-Type $svg }
Ui-ExpectNew 'Drawing dialog: format = svg' 'the format follows the file'
Ui-Step 'sheet A3 (wheel)' {
  $at = Dialog-At 'sheet'
  [UiInput]::Move($script:UiTarget, $at[0], $at[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Wheel($script:UiTarget, $at[0], $at[1], -1)
}
Ui-ExpectNew 'Drawing dialog: sheet = A3' 'A3 chosen'
Ui-Step 'OK' {
  $at = Dialog-At 'OK'
  Ui-Apart $script:UiTarget $at[0] $at[1]
  [UiInput]::Move($script:UiTarget, $at[0], $at[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiTarget, 0x201, $at[0], $at[1], 1)
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($script:UiTarget, 0x202, $at[0], $at[1], 0)
}
Ui-FocusMain
Ui-ExpectNew "Drawing dialog: svg A3 to $svg" 'the choices taken'
Ui-ExpectNew 'Drawing kept: D1 Drawing1' 'the drawing kept in the design (mitcad#707)'
Ui-ExpectNew "Exported ${svg}: drawing, 3 views, 2 bodies in 2 placements, A3, 1:1" 'the drawing written' 60
if (-not (Test-Path $svg)) { Ui-Fail 'no SVG file' }
$text = [System.IO.File]::ReadAllText($svg, [System.Text.Encoding]::UTF8)
$views = ([regex]::Matches($text, '<g class="view"')).Count
if ($views -ne 3) { Ui-Fail "the SVG has $views views" }
foreach ($want in @('>120</text>', ">4x $([char]0xD8)8.5 THRU</text>", '<g class="hole-row" id="A4">', 'width="420mm"')) {
  if (-not $text.Contains($want)) { Ui-Fail "the SVG lacks $want" }
}
Write-Host 'ok   the SVG holds the three views, the plate''s width, the holes'' diameter and table'

Write-Host '--- 2. The choices kept'
Ui-Mark
Ui-Step 'export drawing (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Drawing dialog opened: format svg, sheet A3, scale fit' 'the dialog opened with the kept choices'
Ui-Step 'cancel (Esc)'                   { Ui-FocusDialog '^Export Drawing$'; Ui-Key 'Escape'; Ui-FocusMain }

Ui-Finish 'UI Windows drawing test'
