# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/MainWindowJoints.cpp app/framework/TestDriver.cpp core/model/src/document.rs tools/cli tools/ui-scripts/joint-edit-empty-offset.mitcad-ui tools/ui-scripts/joint-edit-empty-offset.json
# A joint edited with its Offset emptied while typing (mitcad#1860, GitHub
# mitcad/Mitcad#347) in the build VM, as
# tools/ui-joint-edit-empty-offset-test.sh on Linux: the shared steps
# tools/ui-scripts/joint-edit-empty-offset.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/joint-edit-empty-offset.json): the
# edited joint's preview keeps the joint and the rigid group after it, an
# emptied Offset moves nothing in the view (the last good preview stays,
# OK is refused), and a new value moves the group's members together and
# OK commits it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-joint-edit-empty-offset-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-joint-edit-empty-offset'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'joint-edit-empty-offset.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/joint-edit-empty-offset.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/joint-edit-empty-offset.mitcad-ui'
Ui-StartApp 'joint-edit-empty-offset' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'joint edit empty offset driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "joint edit empty offset driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Joint: the last good preview kept' 'the emptied Offset leaves the last good preview drawn'
Ui-ExpectLog 'Joint: OK refused' 'OK refused while the Offset is empty'
Ui-ExpectLog 'Edited F4' 'the new Offset committed'
Ui-ExpectLog 'TestDriver: done' 'a joint edited with its Offset emptied'
Ui-StopApp
Write-Host 'PASS: a joint edited with its Offset emptied moves nothing'
