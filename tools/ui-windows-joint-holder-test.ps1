# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJointHolder.cpp app/browser/JointHolderQuestion.cpp app/commands/JointCommands.cpp app/view/ViewController.cpp tools/cli
# Joint's picks under another component than the active one (mitcad#1856,
# GitHub mitcad/Mitcad#344) in the build VM, as tools/ui-joint-holder-test.sh
# on Linux: the shared steps tools/ui-scripts/joint-holder.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/joint-holder.json): with
# the roller sub-assembly active, a Joint from a roller part to the frame
# asks whether to make it in the root, which holds both picks; Make it in
# mounts the roller whole, Don't ask again sets the preference, and
# Preferences > Assembly sets it back to Ask; a selection made before Joint
# asks too (mitcad#1864).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-joint-holder-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-joint-holder'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'joint-holder.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/joint-holder.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/joint-holder.mitcad-ui'
Ui-StartApp 'joint-holder' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(360000)) { Ui-Fail 'joint holder driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "joint holder driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Joint component asked: The two picks are in Roller and in Frame.' 'the question names the picks'' components'
Ui-ExpectLog 'Joint component answer: make it in ' 'Make it in the root'
Ui-ExpectLog 'Preferences: when the picks are in another component holder' 'Don''t ask again set the preference'
Ui-ExpectLog 'Preferences: when the picks are in another component ask' 'Preferences set it back to Ask'
Ui-ExpectLog 'Joint component answer: keep Roller' 'asked again, Keep Roller'
Ui-ExpectLog 'Joint component of the selection made before it: 2 pick(s)' 'the selection made before Joint is asked about'
Ui-ExpectLog 'TestDriver: done' 'the joint goes into the component that holds its picks'
Ui-StopApp
Write-Host 'PASS: the component that holds the joint''s picks'
