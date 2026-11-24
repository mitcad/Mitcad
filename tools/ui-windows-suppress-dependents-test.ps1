# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/framework/TestDriver.cpp core/model/src/document.rs core/model-api/src/lib.rs tools/cli
# Suppress Features asks about the features that use it (mitcad#2314):
# Cancel, Suppress Only This, Suppress All with one Undo, and Unsuppress
# without a question (tools/ui-scripts/suppress-dependents.mitcad-ui).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-suppress-dependents'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'chain.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/suppress-dependents.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'suppress dependents fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/suppress-dependents.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'suppress-dependents' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'suppress dependents driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "suppress dependents driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Cancel, Suppress Only This, Suppress All, Undo and Unsuppress'
$questions = @(Select-String -Path $script:UiLog -Pattern '^Suppress Block: also Hole, Round\?$').Count
if ($questions -ne 4) { Ui-Fail "expected one question per Suppress, found $questions" }
Ui-StopApp
Write-Host 'PASS: Suppress with dependents'
