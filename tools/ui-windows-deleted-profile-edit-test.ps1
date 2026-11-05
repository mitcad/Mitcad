# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model core/model-api tools/cli
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-deleted-profile-edit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'deleted-profile.mitcad'
# The script's last step leaves Extrude1 failed on purpose; the design is
# what counts.
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/deleted-profile-edit.json') --save $design
if (-not (Test-Path $design)) { Ui-Fail 'deleted profile fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/deleted-profile-edit.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'deleted-profile-edit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'deleted profile edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "deleted profile edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the edit leaves the deleted profile out and takes the new one'
Ui-StopApp
Write-Host 'PASS: editing an extrusion whose profile was deleted'
