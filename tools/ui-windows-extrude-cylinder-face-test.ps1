# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/sketch/SketchSurface.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/developed.rs tools/cli
# check-all sources: tools/ui-scripts/extrude-cylinder-face.mitcad-ui tools/ui-scripts/extrude-cylinder-face.json
# Extrude of a sketch on a cylinder's side with the automatic operation
# (mitcad#1908, GitHub mitcad/Mitcad#362) in the build VM, as
# tools/ui-extrude-cylinder-face-test.sh on Linux: the shared steps
# tools/ui-scripts/extrude-cylinder-face.mitcad-ui on the design that
# mitcad-cli makes as reported (tools/ui-scripts/extrude-cylinder-face.json:
# a cylinder of radius 150 in a component, a rectangle and a circle
# sketched on its side): the picked profiles are highlighted on the face,
# the distance arrow starts there, the automatic Join builds and OK adds it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-cylinder-face-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-cylinder-face'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cylinder.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-cylinder-face.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the cylinder and its sketch were not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-cylinder-face.mitcad-ui'
Ui-StartApp 'extrude-cylinder-face' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'extrude on a cylinder driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "extrude on a cylinder driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'profile F3/r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]} in O1 at 13' "the rectangle's highlight on the cylinder's side"
Ui-ExpectLog 'Extrude: automatic Operation = Join' 'the New Body preview turned Join'
Ui-ExpectLog 'Added extrude (Join)' 'the Join of the profiles on the face added'
Ui-ExpectLog 'TestDriver: done' 'Extrude of a sketch on a cylinder with the automatic Join'
Ui-StopApp
Write-Host 'PASS: extrude of a sketch on a cylindrical face'
