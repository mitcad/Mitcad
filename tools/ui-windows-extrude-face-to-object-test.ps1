# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/ffi/src/kernel/face_extrude.rs
# check-all sources: core/cpp/bridge/face_extrude.cpp geometry/src/face_extrude.cpp geometry/src/extrude.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-face-to-object.mitcad-ui tools/ui-scripts/extrude-face-to-object.json
# Extrude of a body face up to another face of the same body (mitcad#2033)
# in the build VM, as tools/ui-extrude-face-to-object-test.sh on Linux: the
# shared steps tools/ui-scripts/extrude-face-to-object.mitcad-ui on the
# stepped block that mitcad-cli makes (tools/ui-scripts/extrude-face-to-object.json):
# the lower step's top is the profile, Extent To Object takes the higher
# step's top (To Selected Face), the automatic Join builds and OK adds it:
# one level body of 72000 mm3.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-face-to-object-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-face-to-object'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'stepped.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-face-to-object.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the stepped block was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-face-to-object.mitcad-ui'
Ui-StartApp 'extrude-face-to-object' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'extrude of a face to an object driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "extrude of a face to an object driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Extrude Profiles: 1 face [face F2:end(' 'the lower top is the profile'
Ui-ExpectLog 'Extrude Object: 1 face [face F4:end(' 'the higher top of the same body is the object'
Ui-ExpectLog 'Added extrude (Join), distance to object, of a face' 'the Join up to the face added'
Ui-ExpectLog 'TestDriver: done' 'Extrude of a face up to a face of its own body'
Ui-StopApp
Write-Host 'PASS: extrude of a body face up to an object'
