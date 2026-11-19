# SPDX-License-Identifier: MIT
# check-all sources: app/render app/MainWindowRender.cpp app/framework/Ribbon.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/render-workspace.mitcad-ui tools/ui-scripts/render-workspace.json
# The Render workspace (mitcad#2013) in the build VM, as
# tools/ui-render-workspace-test.sh on Linux: the shared steps
# tools/ui-scripts/render-workspace.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/render-workspace.json), in the Floating layout
# (the toolbar panel's workspaces, mitcad#1990). Render chosen in the
# workspace selector opens the workspace with the live view, its stages
# as the tab row itself with Render and Cancel at its right and the stage's
# actions under it as the toolbar's groups, each stage its own list at the left and card at the right; Light's list chooses the environment's or a light's settings,
# an appearance goes to a body, two shots are saved, renamed and looked
# through, and Render All Shots renders both with 4 samples. Cycles renders
# on the CPU. A picture of each stage in a 1024 x 768 window
# (render-look.png, render-light.png, render-frame.png, render-output.png)
# is kept in -Out's pictures folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-render-workspace-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-render-workspace'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[render]', 'device=cpu')
$env:MITCAD_RENDER_SAMPLES = '16'
$env:MITCAD_TEST_STEP_MS = '80'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/render-workspace.mitcad-ui'
$pictures = Join-Path $script:UiOut 'pictures'
New-Item -ItemType Directory -Force $pictures | Out-Null
$env:MITCAD_TEST_PICTURES = $pictures

# The shared script in one layout, on a fresh design.
function Run-Layout([string]$Layout) {
  $work = Join-Path $script:UiOut "files-$Layout"
  New-Item -ItemType Directory -Force $work | Out-Null
  $design = Join-Path $work 'render-workspace.mitcad'
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/render-workspace.json') --save $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail "the design was not made ($Layout)" }
  $env:MITCAD_TEST_WORK = $work
  Ui-StartApp "render-workspace-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail "render workspace driver timed out ($Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "render workspace driver failed ($Layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the Render workspace ($Layout)"
  Ui-ExpectLog 'Ribbon tab RENDER: its own row' "the stages as the tab row ($Layout)"
  Ui-ExpectLog 'Render live pause at ' "Pause in the Live group ($Layout)"
  Ui-ExpectLog 'Render groups: Live | Pick | Look' "Look's groups after the Live group ($Layout)"
  Ui-ExpectLog 'Render groups: Live | Output' "Output's group after the Live group ($Layout)"
  Ui-ExpectLog 'Render panels: Shots to Render | Output' "each stage its own list and card ($Layout)"
  Ui-ExpectLog 'Render settings field environment.strength at ' "the environment's settings docked ($Layout)"
  Ui-ExpectLog 'Render all shots done: 2 of 2' "both shots rendered ($Layout)"
  foreach ($shot in 'Shot1', 'Hero') {
    $image = Join-Path $work "render-workspace-$shot.png"
    if (-not (Test-Path $image) -or (Get-Item $image).Length -eq 0) { Ui-Fail "no image of $shot ($Layout)" }
  }
  foreach ($picture in @('look', 'light', 'frame', 'output')) {
    $path = Join-Path $pictures "render-$picture.png"
    if (-not (Test-Path $path) -or (Get-Item $path).Length -eq 0) { Ui-Fail "no picture render-$picture.png in $pictures" }
  }
  Write-Host "ok   the pictures in $pictures"
  Ui-StopApp
}

Run-Layout 'floating'
Write-Host 'PASS: the Render workspace: stages, docked environment and lights, shots and Render All Shots'
