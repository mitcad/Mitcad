# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/SectionHatch.cpp app/view/SectionHatch.hpp app/MainWindowSection.cpp tools/cli
# The hatch of a large body's section caps lies in the section plane
# (mitcad#2146) in the build VM, as the shared part of
# tools/ui-section-caps-test.sh on Linux: the steps
# tools/ui-scripts/section-caps.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/section-caps.json): a 2000 x 1000 x 500 mm block
# cut across by Section Analysis at the YZ plane moved 1300, then by one
# turned 30 degrees about Z and flipped. The viewer's section look names
# the hatch's largest distance from the plane: 0 within 0.000001 mm.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-section-caps-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-section-caps'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'section-caps.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/section-caps.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/section-caps.mitcad-ui'
Ui-StartApp 'section-caps' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'section caps driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "section caps driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the large body''s hatch in the section plane, across and turned'
Ui-StopApp
Write-Host 'PASS: the section caps'' hatch lies in the plane'
