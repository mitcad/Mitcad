# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowExplode.cpp app/motion/ExplodeViewDialog.cpp app/commands/ExplodeCommands.cpp app/browser app/drawings app/framework/TestDriver.cpp core/model-api/src/explode.rs tools/cli tools/ui-scripts/explode-view.mitcad-ui
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp core/model-api/src/analysis.rs
# Exploded views (mitcad#2113) in the build VM, as
# tools/ui-explode-view-test.sh on Linux: the shared steps
# tools/ui-scripts/explode-view.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/explode-view.json): Auto Explode, a step by
# Explode Step's handle, the steps played back and forth, the Browser's
# row, a drawing view with the explode view; Measure on the exploded
# bodies and a step along a picked edge (mitcad#2328); the saved design
# checked with explode-view-check.json.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-explode-view-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-explode-view'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'explode-view.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/explode-view.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$name = 'explode-view'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(420000)) { Ui-Fail "${name}: driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "${name}: driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
Ui-StopApp
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/explode-view-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved design does not pass explode-view-check.json" }
Write-Host 'PASS: an explode view made by Auto Explode and a handle, played, shown from the Browser and in a drawing view, kept with the design'
