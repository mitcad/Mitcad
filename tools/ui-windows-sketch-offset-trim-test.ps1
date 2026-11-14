# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/UserText.hpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# Trim on an offset's curve (mitcad#1755, GitHub mitcad/Mitcad#314) on
# Windows, the shared input script tools/ui-scripts/sketch-offset-trim.mitcad-ui
# as tools/ui-sketch-offset-trim-test.sh runs it on Linux: a line and an arc
# offset outward, Trim clicked on the arc copy beyond a line across it cuts
# the copy there and says that the offset is no longer linked to its
# source.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-offset-trim'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'offset-trim.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-trim.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch Trim on an offset fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-trim.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-offset-trim' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch Trim on an offset driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Trim on an offset driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Trim cuts an offset copy and says it is no longer linked to its source'
Ui-StopApp
Write-Host 'PASS: sketch Trim on an offset copy'
