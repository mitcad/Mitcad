# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/DocumentSession.cpp app/framework/TestDriver.cpp tools/cli
# A panel's preview held while the model finishes another job shows the
# busy mark (mitcad#2118) in the build VM, as
# tools/ui-panel-busy-waiting-test.sh on Linux: the shared steps
# tools/ui-scripts/panel-busy-waiting.mitcad-ui on the cube that mitcad-cli
# makes (tools/ui-scripts/panel-message-top.json), Docked and then
# Floating: Extrude1 edited, its Distance 7 mm, whose preview's display
# refinement the test's worker gate holds; a Distance of 9 mm typed
# meanwhile waits for it with the busy mark beside OK and Cancel shown, and
# once the job is let go its preview follows and the mark hides.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-panel-busy-waiting-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-panel-busy-waiting'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'panel-busy-waiting.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/panel-busy-waiting.mitcad-ui'

foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "panel-busy-waiting-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${layout}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Preview Extrude: refining the display meshes' "${layout}: the preview's refinement, another job"
  Ui-ExpectLog 'Preview Extrude: waits for the model' "${layout}: the next preview waits for it"
  Ui-ExpectLog 'Panel Extrude busy mark shown after ' "${layout}: the busy mark shown while it waits"
  Ui-ExpectLog 'Panel Extrude busy: Waiting for the design to finish computing' "${layout}: its text says what it waits for"
  Ui-ExpectLog 'Panel Extrude busy mark hidden after ' "${layout}: and hidden once the preview has ended"
  Ui-ExpectLog 'TestDriver: done' "${layout}: a preview held by another job shows the busy mark"
  Ui-StopApp
}
Write-Host 'PASS: a preview held by another job shows the busy mark'
