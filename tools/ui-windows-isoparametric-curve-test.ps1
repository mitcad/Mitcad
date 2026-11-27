# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp core/model/src/features/isoparametric_curve.rs geometry/src/iso_curve.cpp app/framework/ModelShapes.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/isoparametric-curve.mitcad-ui tools/ui-scripts/isoparametric-curve.json
# Isoparametric Curve (mitcad#2522) in the build VM, as
# tools/ui-isoparametric-curve-test.sh on Linux: the shared steps
# tools/ui-scripts/isoparametric-curve.mitcad-ui on the cylinder that
# mitcad-cli makes (tools/ui-scripts/isoparametric-curve.json). The u curve
# through the clicked side face, three v curves evenly spaced, the cylinder
# made taller and the first curve following it, the first curve reopened.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-isoparametric-curve-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-isoparametric-curve'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'isoparametric-curve.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/isoparametric-curve.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/isoparametric-curve.mitcad-ui'
Ui-StartApp 'isoparametric-curve' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'isoparametric-curve driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "isoparametric-curve driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added isoparametric curve, U direction, through the picked point' 'the u curve was added through the click'
Ui-ExpectLog 'Added isoparametric curve, V direction, 3 evenly spaced' 'three v curves were added'
Ui-ExpectLog 'TestDriver: done' 'Isoparametric Curve (mitcad#2522): every step of the script'
Ui-StopApp
Write-Host 'PASS: Isoparametric Curve (mitcad#2522)'
