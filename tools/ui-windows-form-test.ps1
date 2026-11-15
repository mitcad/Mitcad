# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/view/FormCageOverlay.cpp app/MainWindowForm.cpp core/subdiv core/model-api/src/form.rs tools/ui-scripts/form.mitcad-ui
# Form and Edit Form (mitcad#1757, mitcad#1810) on Mesa's software
# renderer, as tools/ui-form-test.sh does on Linux: the shared steps
# tools/ui-scripts/form.mitcad-ui in an empty design (a Form from a box
# cage, opened from the timeline; a face picked in the view and moved,
# then extruded, an edge creased semi-sharp and symmetry about the picked
# YZ plane set, each one undo step whose body is a solid; a vertex dragged
# in the view; Close keeps the edits; a Form with Operation Cut cuts a
# box).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-form-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-form'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/form.mitcad-ui'
Ui-StartApp 'form' @('--ui=qt', '--no-mcp')
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(400000)) { Ui-Fail 'Form driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Form driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Form edit Move in Form1: done' 'a face of the cage moved, one undo step'
Ui-ExpectLog 'Form edit Extrude in Form1: done' 'the face extruded'
Ui-ExpectLog '"sharpness":1.5' 'an edge creased semi-sharp'
Ui-ExpectLog 'Form edit Symmetry in Form1: done' 'symmetry about the picked plane set'
Ui-ExpectLog 'halves equal about the symmetry plane' 'the symmetric Form''s halves are equal'
Ui-ExpectLog 'Form edit Drag in Form1: done' 'a vertex dragged in the view, one undo step'
Ui-ExpectLog 'Closed Edit Form after 4' 'Close kept the edits'
Ui-ExpectLog 'Form cut: body ' 'a Form cut from a box'
Ui-ExpectLog 'TestDriver: done' 'a box cage edited into a smooth solid'
Ui-StopApp
Write-Host 'PASS: Form and Edit Form'
