# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/DocumentHost.hpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/MainWindowJobs.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/ffi/src/kernel/mod.rs core/ffi/src/kernel/abandon.rs
# check-all sources: tools/ui-scripts/drawing-large-view.mitcad-ui tools/ui-scripts/drawing-large-view.json
# A large design's drawing (mitcad#1997) in the build VM, as
# tools/ui-drawing-large-view-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-large-view.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/drawing-large-view.json: 36 cylinders and a drawing
# of them), each projection of the jobs held 3 s: the drawing opened is drawn
# in a job with its note, a new scale shows the old lines scaled while the
# command projects, a base view's placing cancelled adds nothing and placed
# again is projected once inside its command, a model change draws the sheet
# in a job.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-large-view-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-large-view'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cylinders.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-large-view.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-large-view.mitcad-ui'
$env:MITCAD_TEST_RECOMPUTE_DELAY_MS = 'add_drawing_view=3000,edit_drawing_view=3000,drawing sheet=3000'
Ui-StartApp 'drawing-large-view' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(200000)) { Ui-Fail 'large view driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "large view driver failed (exit code $exitCode)"
}
Remove-Item Env:MITCAD_TEST_RECOMPUTE_DELAY_MS -ErrorAction SilentlyContinue
Ui-ExpectLog 'Drawing note: Drawing the views' 'the opened sheet drawn in a job, with its note'
Ui-ExpectLog 'Drawing views updating: 1 by 0.5' 'the old lines scaled while the new scale is projected'
Ui-ExpectLog 'Computing add_drawing_view cancelled after ' 'the placing cancelled'
Ui-ExpectLog 'TestDriver: done' 'placed once, cancelled without a view, drawn in jobs after a model change'
Ui-StopApp
Write-Host 'PASS: drawing of a large design: jobs, progress, cancel and the old lines scaled'
