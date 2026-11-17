# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FormCommands.cpp app/commands/FormCage.cpp app/view/FormCageOverlay.cpp app/MainWindowForm.cpp app/MainWindow.cpp app/framework/OpeningEdges.cpp core/subdiv core/model-api/src/form.rs tools/ui-scripts/form-fill-hole.mitcad-ui
# A Form's opening (mitcad#1941) on Mesa's software renderer, as
# tools/ui-form-fill-hole-test.sh does on Linux: the shared steps
# tools/ui-scripts/form-fill-hole.mitcad-ui in an empty design (a Form from
# a box cage, a face of its top deleted in Edit Form; Patch picks the
# opening by one free edge, the whole loop; Fill Hole from the view's
# context menu on a picked edge of the opening closes the cage again).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-form-fill-hole-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-form-fill-hole'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/form-fill-hole.mitcad-ui'
Ui-StartApp 'form-fill-hole' @('--ui=qt', '--no-mcp')
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'Form Fill Hole driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Form Fill Hole driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Form edit Delete Faces in Form1: done' 'a face of the Form deleted, the cage open'
Ui-ExpectLog 'Patch Profiles or Edges: 1 edge' 'Patch takes the Form''s opening by one free edge'
Ui-ExpectLog 'Context menu: Fill Hole | Bridge' 'the context menu of Edit Form offers Fill Hole and Bridge'
Ui-ExpectLog 'Form edit Fill Hole in Form1: done' 'Fill Hole from the context menu, one undo step'
Ui-ExpectLog 'Form edit Form1: cage of 26 vertices, 48 edges, 24 faces, closed' 'the cage closed again'
Ui-ExpectLog 'TestDriver: done' 'a Form''s opening picked by Patch and filled'
Ui-StopApp
Write-Host 'PASS: Form Fill Hole'
