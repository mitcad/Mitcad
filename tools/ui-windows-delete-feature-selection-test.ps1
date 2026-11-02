# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowContextMenu.cpp app/framework/TestDriver.cpp tools/cli
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-delete-selection'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'selection.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/delete-feature-selection.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'selection fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/delete-feature-selection.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'delete-selection' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'selection delete driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "selection delete driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'collection and view Delete, dependency question and atomic Undo'
$questions = @(Select-String -Path $script:UiLog -Pattern '^Delete C, Source: also Dependent\?$').Count
if ($questions -ne 2) { Ui-Fail "expected one question per attempt, found $questions" }
Ui-StopApp
Write-Host 'PASS: feature selection Delete'
