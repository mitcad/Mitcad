# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/commands/CommandSupport.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/construct-offset-sketch.mitcad-ui tools/ui-scripts/construct-offset-sketch.json
# Offset Plane takes a sketch (mitcad#2167) in the build VM, as
# tools/ui-construct-offset-sketch-test.sh on Linux: the shared steps
# tools/ui-scripts/construct-offset-sketch.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/construct-offset-sketch.json): the
# sketch picked in the Browser and its profile clicked in the view are each
# taken as the sketch's plane, and OK makes the plane 15 mm above it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construct-offset-sketch-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construct-offset-sketch'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'construct-offset-sketch.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/construct-offset-sketch.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/construct-offset-sketch.mitcad-ui'
Ui-StartApp 'construct-offset-sketch' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'offset plane on a sketch driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "offset plane on a sketch driver failed (exit code $exitCode)"
}
Ui-ExpectLog "Construction plane: the sketch of F1 taken as its sketch's plane" 'the sketch picked in the Browser was taken'
Ui-ExpectLog "Construction plane: a profile of F1 taken as its sketch's plane" 'the profile clicked in the view was taken'
Ui-ExpectLog 'TestDriver: done' 'Offset Plane on a sketch'
$added = Ui-LogLines | Where-Object { $_ -match 'Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 15[)], normal [(]0, 0, 1[)]' }
if (-not $added) { Ui-Fail 'OK did not make the plane 15 mm above the sketch' }
Ui-StopApp
Write-Host 'PASS: Offset Plane takes a sketch picked in the Browser and its profile clicked in the view'
