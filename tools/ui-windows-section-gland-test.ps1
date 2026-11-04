# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSection.cpp core/cpp/bridge/analysis.cpp geometry/analysis/src/section.cpp tools/cli
# A cable gland body cut in its profile's plane (mitcad#1230) in the build
# VM, as tools/ui-section-gland-test.sh on Linux: the shared steps
# tools/ui-scripts/section-gland.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/section-gland.json): a body revolved from
# profiles in the XY plane with modelled M45x3 internal and M30x2 external
# threads, Section Analysis at the XY plane (Distance 0, through the axis)
# and 1 mm off it. Both cut the body with caps; no clip fails.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-section-gland-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-section-gland'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'gland.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/section-gland.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/section-gland.mitcad-ui'
Ui-StartApp 'section-gland' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'section gland driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "section gland driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the threaded gland cut with caps in its profile plane and 1 mm off it'
Ui-StopApp
Write-Host 'PASS: the threaded gland cut in its profile plane'
