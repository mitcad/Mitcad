# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowActivate.cpp app/browser/ActivateOnPick.cpp app/framework/CommandSessionVisibility.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp tools/cli app/commands/GearCommands.cpp app/commands/PlaceInputs.cpp
# A feature that changes the picked body activates its component
# (mitcad#1634) in the build VM, as tools/ui-activate-on-pick-test.sh on
# Linux: the shared steps tools/ui-scripts/activate-on-pick.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/activate-on-pick.json):
# with Base active, Fillet on an edge of Lid's block activates Lid without
# a question, the panel says where the fillet goes, OK adds it, and Undo
# takes back the fillet, then the activation; Esc after the activation
# leaves Lid active; an edge of each component gives the refusal and the
# Activate Lid button, which keeps Lid's edge. Gear on the top face of
# Lid's block (a part placed by a pick, mitcad#1752) activates Lid as well,
# and OK adds the gear in Lid.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-activate-on-pick-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-activate-on-pick'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'activate-on-pick.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/activate-on-pick.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/activate-on-pick.mitcad-ui'
Ui-StartApp 'activate-on-pick' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'activate on pick driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "activate on pick driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Activated Lid for Fillet (picked edge ' 'Fillet on Lid''s edge activated Lid'
Ui-ExpectLog 'Panel Fillet notice: Fillet goes into Lid (now active).' 'the panel says where the fillet goes'
Ui-ExpectLog 'Added fillet on 1 edge' 'OK added the fillet in Lid'
Ui-ExpectLog 'Command Fillet cancelled' 'Esc cancelled Fillet after the activation'
Ui-ExpectLog 'Panel Fillet offers: Activate Lid (drops 1)' 'an edge of each component offers Activate Lid'
Ui-ExpectLog 'Panel Fillet notice: Fillet goes into Lid (now active). 1 pick of another component dropped.' 'the Activate button kept Lid''s edge'
Ui-ExpectLog 'Activated Lid for Gear (picked face ' 'Gear on Lid''s face activated Lid'
Ui-ExpectLog 'Panel Gear notice: Gear goes into Lid (now active).' 'the panel says where the gear goes'
Ui-ExpectLog 'Added gear ' 'OK added the gear in Lid'
Ui-ExpectLog 'TestDriver: done' 'the picked body''s component is activated'
Ui-StopApp
Write-Host 'PASS: A feature on another component''s body activates that component'
