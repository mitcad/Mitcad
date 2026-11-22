# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp geometry/src/loft_merge.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# The Loft panel's Tangent Edges (mitcad#2162) in the build VM, as
# tools/ui-loft-tangent-edges-test.sh on Linux: the shared steps
# tools/ui-scripts/loft-tangent-edges.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/loft-tangent-edges.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-loft-tangent-edges-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-loft-tangent-edges'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'loft-tangent-edges.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/loft-tangent-edges.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/loft-tangent-edges.mitcad-ui'
Ui-StartApp 'loft-tangent-edges' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'loft tangent edges driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "loft tangent edges driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added loft (New Body) through 2 sections' 'the circle-to-slot loft builds with Tangent Edges Merge'
Ui-ExpectLog 'TestDriver: done' 'the loft has one side face and its caps'
Ui-StopApp
Write-Host 'PASS: the Loft panel shows Tangent Edges with Merge, one side face from a circle to a slot'
