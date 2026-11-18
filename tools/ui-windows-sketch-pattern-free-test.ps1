# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-pattern-free.mitcad-ui tools/ui-scripts/sketch-pattern-free.json
# Deleting, trimming and freeing sketch patterns (mitcad#2007, GitHub
# mitcad/Mitcad#432) on Windows, the shared input script
# tools/ui-scripts/sketch-pattern-free.mitcad-ui as
# tools/ui-sketch-pattern-free-test.sh runs it on Linux: a deleted copy stays
# deleted and Edit Pattern still works, Trim on a copy frees the pattern with
# a note, and Free Pattern in the context menu frees another.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-pattern-free'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pattern-free.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-free.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch pattern free fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-free.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-pattern-free' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch pattern free driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch pattern free driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a deleted copy stays out, Trim frees a pattern, Free Pattern frees another'
Ui-StopApp
Write-Host 'PASS: sketch pattern copies deleted, trimmed and freed'
