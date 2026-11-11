# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dModel.cpp app/sketch/Sketch3dEdit.cpp app/sketch/Sketch3dController.cpp app/MainWindowSketch3d.cpp app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp app/framework/TestDriver.cpp tools/cli
# The 3D Sketch Palette's rows in words (mitcad#1652) in the build VM, as
# tools/ui-sketch3d-palette-words-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch3d-palette-words.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch3d-palette-words.json): the
# reference curve's Linked references row reads "Reference curve 1 (an
# edge of Box1); included in 3D", no shown text has an id, a topological
# name or a coordinate, the pointer over the row paints the reference curve
# and the edge it follows, and leaving the row clears them.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-palette-words-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d-palette-words'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sketch3d-palette-words.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch3d-palette-words.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch3d-palette-words.mitcad-ui'
Ui-StartApp 'sketch3d-palette-words' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3D sketch palette words driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3D sketch palette words driver failed (exit code $exitCode)"
}
Ui-ExpectLog '| shown: Reference curve 1 (an edge of Box1); included in 3D' 'the reference curve''s row reads in words, its source by its feature''s name'
Ui-ExpectLog '3D sketch palette hover: c' 'the pointer over the row was logged'
Ui-ExpectLog 'Notice hover highlighted: 3D Sketch Palette: sketch_curve ' 'the row''s reference curve and its edge were painted'
Ui-ExpectLog '3D sketch palette hover: none' 'leaving the row ended it'
Ui-ExpectLog 'TestDriver: done' 'the rows are words and paint what they name'
Ui-StopApp
Write-Host 'PASS: The 3D Sketch Palette''s rows are words and paint what they name'
