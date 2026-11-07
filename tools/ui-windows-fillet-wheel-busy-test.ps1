# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# The mouse wheel over Fillet's Radius while its preview computes
# (mitcad#1407, GitHub mitcad/Mitcad#162) in the build VM, as
# tools/ui-fillet-wheel-busy-test.sh on Linux: the shared steps
# tools/ui-scripts/fillet-wheel-busy.mitcad-ui on the cylinder that
# mitcad-cli makes (tools/ui-scripts/fillet-wheel-busy.json): Fillet on its
# top edge, Radius 3 mm, then wheel notches over the field faster than the
# preview computes: each notch steps the field, nothing reads the document,
# the application goes on, the last value (6 mm) is evaluated and previewed
# once the job ends, and OK makes that fillet.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-wheel-busy-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-wheel-busy'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet-wheel-busy.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fillet-wheel-busy.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-wheel-busy.mitcad-ui'
Ui-StartApp 'fillet-wheel-busy' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'fillet wheel busy driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "fillet wheel busy driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Value field Fillet Radius: wheel stepped 4 mm to 5 mm' 'the wheel steps the radius while the preview computes'
Ui-ExpectLog 'Fillet Radius: 6 mm = 6 mm' 'the last value is evaluated once the job has ended'
Ui-ExpectLog 'Added Fillet' 'OK makes the fillet'
Ui-ExpectLog 'TestDriver: done' 'the wheel over Fillet Radius while its preview computes'
Ui-StopApp
Write-Host 'PASS: the wheel over Fillet Radius while its preview computes'
