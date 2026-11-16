# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/framework/NoticeLevel.cpp app/framework/TestDriver.cpp
# check-all sources: core/import-base/src/report_status.rs core/import-replay/src/report.rs core/ffi/src/lib.rs core/tests
# The Import Report's status on every row and its one-line verdict
# (mitcad#1893) in the build VM, as tools/ui-import-report-status-test.sh
# on Linux: the shared input scripts
# tools/ui-scripts/import-report-status.mitcad-ui and
# import-report-status-iam.mitcad-ui on the files test_exchange writes:
# --open of designed.ipt (--write-ipt-design), whose verdict says
# everything came in, every row OK; --open of top.iam (--write-iam), one of
# whose files is missing: one file could not be imported, its row an
# error; the filter shows the error rows alone, then every row again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-import-report-status-test.ps1 -App build\dev\mitcad.exe [-TestExchange build\dev\core\test_exchange.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$TestExchange = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-import-report-status'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
if (-not $TestExchange) { $TestExchange = Join-Path (Split-Path $script:UiApp) 'test_exchange.exe' }
if (-not (Test-Path $TestExchange)) { Ui-Fail "no $TestExchange" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$part = Join-Path $work 'designed.ipt'
& $TestExchange --write-ipt-design $part
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the part with a design was not written' }
& $TestExchange --write-iam (Join-Path $work 'iam')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the test assemblies were not written' }
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'

# Runs the application with a shared script and waits for its driver.
function Run-Script([string]$Name, [string]$Script, [string]$Open, [string]$What) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', '--open', $Open)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(480000)) { Ui-Fail "$What driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$What driver failed (exit code $exitCode)"
  }
}

Run-Script 'import-report-status' 'import-report-status.mitcad-ui' $part 'the part'
Ui-ExpectLog 'TestDriver: done' 'designed.ipt: everything came in, every row OK, the filter'
Ui-StopApp
Run-Script 'import-report-status-iam' 'import-report-status-iam.mitcad-ui' (Join-Path $work 'iam\top.iam') 'the assembly'
Ui-ExpectLog 'TestDriver: done' 'top.iam: one file could not be imported, its row an error, the filter'
Ui-StopApp
Write-Host 'PASS: the Import Report: a status per row and a verdict'
