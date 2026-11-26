# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp core/model/src/features/shell.rs geometry/src/faceops.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/shell-own-thickness.mitcad-ui tools/ui-scripts/shell-own-thickness.json
# Shell with faces of their own thickness (mitcad#2443) in the build VM, as
# tools/ui-shell-own-thickness-test.sh on Linux: the shared steps
# tools/ui-scripts/shell-own-thickness.mitcad-ui on the box that mitcad-cli
# makes (tools/ui-scripts/shell-own-thickness.json). Shell with the top
# removed, 2 mm inside, the front in an Own Thickness row at 5 mm, then
# reopened and the row edited to 4 mm, with the volumes the script works out.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-shell-own-thickness-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-shell-own-thickness'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'shell-own-thickness.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/shell-own-thickness.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/shell-own-thickness.mitcad-ui'
Ui-StartApp 'shell-own-thickness' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'shell-own-thickness driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "shell-own-thickness driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Shell: added face_thicknesses row 1' 'the Own Thickness row was added'
Ui-ExpectLog 'Added shell removing 1 face(s), 1 face(s) at their own thickness' 'the shell was added with the row'
Ui-ExpectLog 'TestDriver: done' 'Shell with faces of their own thickness (mitcad#2443): every step of the script'
Ui-StopApp
Write-Host 'PASS: Shell with faces of their own thickness (mitcad#2443)'
