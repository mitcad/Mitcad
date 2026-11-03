# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser/DocumentSnapshot.cpp app/browser/DocumentSnapshot.hpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# The sketch of a component shown only at its second occurrence
# (mitcad#1194) on Windows, the shared input script
# tools/ui-scripts/hidden-occurrence-sketch.mitcad-ui as
# tools/ui-hidden-occurrence-sketch-test.sh runs it on Linux: Plate's first
# occurrence lies in a hidden assembly, its second at the root is shown.
# Editing Plate's sketch puts it at the shown occurrence and draws its
# curves there with its dimensions; the hole's diameter set to 12 in its
# value editor changes the plate (the saved design checked).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-hidden-occurrence-sketch'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/hidden-occurrence-sketch.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'hidden occurrence sketch fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/hidden-occurrence-sketch.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'hidden-occurrence-sketch' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'hidden occurrence sketch driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "hidden occurrence sketch driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the sketch's curves drawn at the shown occurrence, its hole's diameter edited"
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/hidden-occurrence-sketch-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved plate's hole is not 12 in diameter" }
Ui-StopApp
Write-Host 'PASS: sketch of a component shown at its second occurrence'
