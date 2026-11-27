# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/commands/CommandSupport.cpp app/framework/ManipulatorOverlay.cpp app/framework/TestDriver.cpp tools/cli geometry/src/datum.cpp geometry/src/faceops.cpp core/model/src/features/offset_face.rs
# Press Pull's Offset on a curved face (mitcad#2509) in the build VM, as
# tools/ui-press-pull-curved-offset-test.sh on Linux: the shared steps
# tools/ui-scripts/press-pull-curved-offset.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/press-pull-curved-offset.json): on a
# tapered Extrude's side the arrow starts on the face along its outward
# normal; dragged, the preview is ok and OK commits a body that grew; the
# dragged distance typed gives the same body, 0.5 mm typed the grown
# frustum's volume.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-press-pull-curved-offset-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-press-pull-curved-offset'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'press-pull-curved-offset.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/press-pull-curved-offset.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/press-pull-curved-offset.mitcad-ui'
Ui-StartApp 'press-pull-curved-offset' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'press pull curved offset driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "press pull curved offset driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Press Pull made offset face, distance ' 'the side offset by the dragged arrow'
Ui-ExpectLog 'Press Pull made offset face, distance 0.5' 'the side offset by 0.5 mm typed'
Ui-ExpectLog 'TestDriver: done' 'the arrow on the curved face dragged and typed'
Ui-StopApp
Write-Host 'PASS: Press Pull Offset on a curved face'
