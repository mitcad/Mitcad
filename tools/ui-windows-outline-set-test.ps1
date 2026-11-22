# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/ProfilePicks.cpp tools/cli
# The outline set (mitcad#2194, GitHub mitcad/Mitcad#545) in the build VM, as
# tools/ui-outline-set-test.sh on Linux: the shared steps
# tools/ui-scripts/outline-set.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/outline-set.json): a plate's holes unfilled but picked,
# Extrude's start and whole sketch picks giving the plate and its island,
# and a sketch whose closed curves cross said in the panel.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-outline-set-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-outline-set'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'outline-set.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/outline-set.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/outline-set.mitcad-ui'
Ui-StartApp 'outline-set' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'outline set driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "outline set driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Show profiles, Extrude starts and whole sketch picks follow the outline set'
Ui-StopApp
Write-Host 'PASS: outline set of nested and crossing sketch regions'
