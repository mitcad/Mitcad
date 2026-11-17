# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp geometry/src/loft_skin.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# A loft from a circle to a square through a rail tied to Intersect points,
# and a loose rail's miss in words (mitcad#1944), in the build VM, as
# tools/ui-loft-rail-intersect-test.sh on Linux: the shared steps
# tools/ui-scripts/loft-rail-intersect.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/loft-rail-intersect.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-loft-rail-intersect-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-loft-rail-intersect'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'loft-rail-intersect.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/loft-rail-intersect.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/loft-rail-intersect.mitcad-ui'
Ui-StartApp 'loft-rail-intersect' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'loft rail intersect driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "loft rail intersect driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added loft (New Body) through 2 sections' 'the loft through the rail tied to Intersect points builds'
Ui-ExpectLog 'rail 1 misses the second profile by 0.3 mm' 'the loose rail is refused in words'
Ui-ExpectLog 'TestDriver: done' 'the miss message paints its places, without names or coordinates'
Ui-StopApp
Write-Host 'PASS: a loft through a rail tied to Intersect points, a loose rail refused in words'
