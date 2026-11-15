# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/ManipulatorOverlay.cpp app/MainWindowJoints.cpp tools/cli
# A command panel set aside hides its handles (mitcad#1844) in the build
# VM, as tools/ui-joint-suspended-handles-test.sh on Linux: the shared
# steps tools/ui-scripts/joint-suspended-handles.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/joint-suspended-handles.json):
# Joint's Animate sets its panel aside, its handles hidden and a drag where
# one was moving nothing; Esc brings them back and a real drag turns it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-joint-suspended-handles-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-joint-suspended-handles'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'joint-suspended-handles.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/joint-suspended-handles.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/joint-suspended-handles.mitcad-ui'
Ui-StartApp 'joint-suspended-handles' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'joint suspended handles driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "joint suspended handles driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Joint handles hidden while set aside' 'the handles hidden while Animate plays'
Ui-ExpectLog 'TestDriver: done' 'the set-aside panel''s handles hidden, back on resume'
Ui-StopApp
Write-Host 'PASS: a set-aside panel''s handles'
