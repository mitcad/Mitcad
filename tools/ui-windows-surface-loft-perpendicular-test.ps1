# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp geometry/src/loft.cpp geometry/src/loft_skin.cpp core/model/src/features/loft.rs tools/cli
# Surface Loft's Start and End Condition (mitcad#2345) in the build VM, as
# tools/ui-surface-loft-perpendicular-test.sh on Linux: the shared steps
# tools/ui-scripts/surface-loft-perpendicular.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/surface-loft-perpendicular.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-surface-loft-perpendicular-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-surface-loft-perpendicular'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'surface-loft-perpendicular.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/surface-loft-perpendicular.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/surface-loft-perpendicular.mitcad-ui'
Ui-StartApp 'surface-loft-perpendicular' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'surface loft perpendicular driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "surface loft perpendicular driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added surface loft through 2 sections' 'the surface loft builds with Perpendicular to Face at both ends'
Ui-ExpectLog 'TestDriver: done' 'the surface is one side face'
Ui-StopApp
Write-Host 'PASS: Surface Loft with Perpendicular to Face at both ends, one side face'
