# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ExpressionCheck.hpp
# check-all sources: app/framework/MessageLog.cpp app/framework/MessageBand.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowParameterEdits.cpp app/framework/TestDriver.cpp
# check-all sources: core/expr core/model-api/src/formula_help.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/parameter-error-put-right.mitcad-ui
# check-all sources: tools/ui-scripts/parameter-error-put-right.json
# A parameter error put right leaves the message band, and expression
# errors in words (mitcad#2181), on Windows: the shared input script
# tools/ui-scripts/parameter-error-put-right.mitcad-ui as
# tools/ui-parameter-error-put-right-test.sh runs it on Linux. The
# Parameters window's empty row checks the expression while it is typed,
# a refused Expression cell says it in words in the band, the missing
# parameter added puts that error right, and the add again offers the near
# miss and adds the parameter.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameter-error-put-right'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'put-right.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameter-error-put-right.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'parameter error put right fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameter-error-put-right.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'parameter-error-put-right' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'parameter error put right driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "parameter error put right driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Message band cleared (put right)' 'the error put right leaves the band'
Ui-ExpectLog 'TestDriver: done' 'a parameter error put right leaves the band'
Ui-StopApp
Write-Host 'PASS: parameter error put right'
