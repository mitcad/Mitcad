# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Application.cpp app/framework/ParameterField.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# Keys while a pasted component is still computed (mitcad#1360, GitHub
# mitcad/Mitcad#156, a crash of Windows 0.47.0) in the build VM, as
# tools/ui-paste-busy-keys-test.sh on Linux: the shared steps
# tools/ui-scripts/paste-busy-keys.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/paste-busy-keys.json): Copy and Paste New of a
# component open Move/Copy; while its preview computes, Shift and Up in the
# X Distance field neither read the document nor end the application, Up
# steps 10 mm to 11 mm, and OK places the copy once the job has ended.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-paste-busy-keys-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-paste-busy-keys'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'paste-busy-keys.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/paste-busy-keys.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/paste-busy-keys.mitcad-ui'
Ui-StartApp 'paste-busy-keys' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'paste busy keys driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "paste busy keys driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Value field Move/Copy X Distance: Up stepped 10 mm to 11 mm' 'Up steps the distance while the preview computes'
Ui-ExpectLog ' at (51, 0, 0)' 'the copy is placed once the job has ended'
Ui-ExpectLog 'TestDriver: done' 'keys while a pasted component computes'
Ui-StopApp
Write-Host 'PASS: keys while a pasted component computes'
