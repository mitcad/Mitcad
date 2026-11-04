# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Offset where corner arcs vanish (mitcad#1234) on Windows, the
# shared input script tools/ui-scripts/sketch-offset-vanishing.mitcad-ui as
# tools/ui-sketch-offset-vanishing-test.sh runs it on Linux: a hexagon with
# 2 mm corner arcs, Offset opened on a side with Chain Selection on
# previews at 5 mm and at 2 mm inward without an error, the note naming the
# corner arcs that vanish, and OK keeps the offset.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-offset-vanishing'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'offset.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-vanishing.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch Offset fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-vanishing.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-offset-vanishing' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch Offset driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Offset driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Offset previews and keeps a hexagon whose corner arcs vanish'
Ui-StopApp
Write-Host 'PASS: sketch Offset where corner arcs vanish'
