# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-extend-to-meet.mitcad-ui tools/ui-scripts/sketch-extend-to-meet.json
# Extend to Meet (mitcad#2178) on Windows, the shared input script
# tools/ui-scripts/sketch-extend-to-meet.mitcad-ui as
# tools/ui-sketch-extend-to-meet-test.sh runs it on Linux: two lines
# previewed and brought to one corner, then a line and an arc with two
# results, Tab stepping the preview to the second and the click making it.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-extend-to-meet'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'meet.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-extend-to-meet.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch extend to meet fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-extend-to-meet.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-extend-to-meet' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch extend to meet driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch extend to meet driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'two lines and a line and an arc extended to meet, Tab stepping to the second result'
Ui-StopApp
Write-Host 'PASS: sketch Extend to Meet'
