# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# A sketch text moved after it is made (mitcad#1911) on Windows, the shared
# input script tools/ui-scripts/sketch-move-text.mitcad-ui as
# tools/ui-sketch-move-text-test.sh runs it on Linux: a text placed with
# the Text tool, moved 10 mm with Move/Copy, dragged onto a point and
# placed by Edit Text's Position, its anchor read back in Edit Text each
# time; the saved sketch has the text where the Position put it.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-move-text'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'text.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-text.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch text move fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-text.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-move-text' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch text move driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch text move driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a text moved with Move/Copy, dragged and placed by its Position'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-text-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the saved text is not where Edit Text placed it' }
Ui-StopApp
Write-Host 'PASS: sketch text moved'
