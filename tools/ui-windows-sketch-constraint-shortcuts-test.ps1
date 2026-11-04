# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/commands/SketchConstraintKeys.hpp app/sketch
# check-all sources: app/framework/CommandRegistry.cpp app/framework/TestDriver.cpp core/sketch tools/cli
# The sketch constraints' Shift keys (mitcad#1229) on Windows, the shared
# input script tools/ui-scripts/sketch-constraint-shortcuts.mitcad-ui as
# tools/ui-sketch-constraint-shortcuts-test.sh runs it on Linux: two lines
# selected, Shift+P adds Parallel; a line selected, Shift+V makes it
# vertical; Shift+P typed into a dimension's value field types a capital P
# and adds no constraint.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-constraint-shortcuts'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'constraints.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-constraint-shortcuts.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch constraint keys fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-constraint-shortcuts.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-constraint-shortcuts' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch constraint keys driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch constraint keys driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Shift+P adds Parallel, Shift+V Vertical, Shift+P in a value field types P'
Ui-StopApp
Write-Host 'PASS: sketch constraint Shift keys'
