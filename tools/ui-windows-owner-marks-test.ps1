# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/framework/TestDriver.cpp core/model core/model-api tools/cli
# What is picked in the view marks its owners' rows in the browser and its
# feature in the timeline, without selecting them (mitcad#1704), and the
# folders on the way to them, also in a placed component (mitcad#1792): the shared
# script tools/ui-scripts/owner-marks.mitcad-ui (tools/ui-owner-marks-test.sh
# on Linux) in the Docked layout, then in the Floating one.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-owner-marks'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'owner-marks.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/owner-marks.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'owner marks fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/owner-marks.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
foreach ($chrome in @('docked', 'floating')) {
  $env:MITCAD_TEST_CHROME = $chrome
  Ui-StartApp "owner-marks-$chrome" @('--ui=qt', '--no-mcp', "--chrome=$chrome", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "owner marks driver timed out ($chrome)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "owner marks driver failed ($chrome, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "a face and a profile mark their owners and folders, Delete acts on the face ($chrome)"
  Ui-StopApp
}
Write-Host 'PASS: owner marks of the view selection'
