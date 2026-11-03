# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.hpp app/framework/TestDriver.cpp tools/cli
# Actual document list selection, saved-state insertion, source Save/Cancel,
# independent undo and self-insertion exclusion through the shared UI driver.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-insert-open-design'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
foreach ($design in @('first', 'second')) {
  & $Cli run (Join-Path $PSScriptRoot "ui-scripts/insert-open-design-$design.json") --save (Join-Path $work "$design.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "open-design $design fixture creation failed" }
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $work 'workflow.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
$generatedSteps = @(& python (Join-Path $PSScriptRoot 'ui-scripts/insert-open-design-script.py') qt)
if ($LASTEXITCODE -ne 0) { Ui-Fail 'open-design script generation failed' }
[System.IO.File]::WriteAllLines($env:MITCAD_TEST_INPUT, [string[]]$generatedSteps, [System.Text.UTF8Encoding]::new($false))
Ui-StartApp 'insert-open-design' @('--ui=qt', '--no-mcp', '--no-recovery', '--open', (Join-Path $work 'first.mitcad'))
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(360000)) { Ui-Fail 'open-design insertion driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "open-design insertion driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: quit' 'open-design selection, saved-state insertion, Save/Cancel and independent undo'
Ui-StopApp
Write-Host 'PASS: insert an open design'
