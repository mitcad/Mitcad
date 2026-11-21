# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp app/framework/ManipulatorOverlay.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/construct-reverse-normal.mitcad-ui tools/ui-scripts/construct-reverse-normal.json
# Offset Plane's Reverse Normal and signed Distance (mitcad#2048, GitHub
# mitcad/Mitcad#443) in the build VM, as
# tools/ui-construct-reverse-normal-test.sh on Linux: the shared steps
# tools/ui-scripts/construct-reverse-normal.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/construct-reverse-normal.json): the
# Distance tooltip says how to reach the other side, the plane's flip input
# is Reverse Normal, toggling it turns the preview's normal marker over,
# and Distance -10 mm puts the plane on the other side of the face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construct-reverse-normal-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construct-reverse-normal'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'construct-reverse-normal.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/construct-reverse-normal.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/construct-reverse-normal.mitcad-ui'
Ui-StartApp 'construct-reverse-normal' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'reverse normal driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "reverse normal driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview marker direction Offset Plane normal: 0.000,0.000,1.000' 'the normal marker points up'
Ui-ExpectLog 'Offset Plane: Reverse Normal = on' 'the plane''s flip input is Reverse Normal'
Ui-ExpectLog 'Preview marker direction Offset Plane normal: 0.000,0.000,-1.000' 'Reverse Normal turned the marker over'
Ui-ExpectLog 'Added plane Plane' 'OK added the plane'
Ui-ExpectLog 'TestDriver: done' 'Reverse Normal and a negative distance'
$added = Ui-LogLines | Where-Object { $_ -match 'Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 20[)], normal [(]0, 0, -1[)]' }
if (-not $added) { Ui-Fail 'Distance -10 mm did not put the plane on the other side of the face' }
Ui-StopApp
Write-Host 'PASS: Offset Plane Reverse Normal turns the normal over; a negative distance reaches the other side'
