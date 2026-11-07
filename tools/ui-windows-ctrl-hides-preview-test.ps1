# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreviewHold.cpp app/framework/CommandSessionHold.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# Ctrl held alone hides a command's preview while its inputs are picked
# (mitcad#1421) in the build VM, as tools/ui-ctrl-hides-preview-test.sh on
# Linux: the shared steps tools/ui-scripts/ctrl-hides-preview.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/ctrl-hides-preview.json):
# a new Extrude on three circles hides its bodies while Ctrl is held, a
# fourth circle Ctrl+clicked then joins and the release draws the extrude of
# all four; Ctrl+Z and Ctrl+S show it again at once, a Ctrl+scroll keeps it
# hidden until the release; Fillet1 edited
# from the timeline gains an edge while Ctrl hides its rounding.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-ctrl-hides-preview-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-ctrl-hides-preview'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'ctrl-hides-preview.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/ctrl-hides-preview.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/ctrl-hides-preview.mitcad-ui'
Ui-StartApp 'ctrl-hides-preview' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Ctrl hides preview driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Ctrl hides preview driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview hidden (Ctrl held)' 'Ctrl held alone hides the preview'
Ui-ExpectLog 'Extrude Profiles: 4 profiles' 'a circle Ctrl+clicked while the preview is hidden joins Profiles'
Ui-ExpectLog 'Preview shown (Ctrl released)' 'the release shows the preview again'
Ui-ExpectLog 'Preview shown (a key pressed)' 'a shortcut with Ctrl shows the preview again at once'
Ui-ExpectLog 'Added extrude (New Body)' 'OK adds the extrude of four circles'
Ui-ExpectLog 'Fillet Edges: 2 edges' 'an edge Ctrl+clicked while the rounding is hidden joins the edited fillet'
Ui-ExpectLog 'Edited F3' 'OK keeps the edited fillet'
Ui-ExpectLog 'TestDriver: done' 'Ctrl hides the preview while the inputs are picked'
Ui-StopApp
Write-Host 'PASS: Ctrl hides the preview while the inputs are picked'
