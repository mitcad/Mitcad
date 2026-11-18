# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/SketchCurveRows.cpp app/MainWindowBrowserCurves.cpp
# check-all sources: tools/ui-scripts/surface-patch-browser-curve.mitcad-ui app/framework/TestDriver.cpp
# Patch takes a sketch curve for its boundary from the Browser (mitcad#1978,
# GitHub mitcad/Mitcad#407), in the build VM: the shared steps
# tools/ui-scripts/surface-patch-browser-curve.mitcad-ui (also run by
# tools/ui-surface-patch-browser-curve-test.sh and
# tools/ui-macos-surface-patch-browser-curve-test.sh) on the reported design
# of the private test files (MITCAD_MITCAD_REPORTED,
# github-407/surfacing_bug.zip; never committed), unpacked into the test's
# folder: while Patch's boundary is active the Browser lists the sketches'
# curves; Patch takes two free edges in the view and the sketch line from
# its Browser row, and OK adds the patch.
# Skipped (77) without the design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-surface-patch-browser-curve-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-surface-patch-browser-curve'),
  [string]$QtBin = '')
$reportedRoot = $env:MITCAD_MITCAD_REPORTED
$zip = if ($reportedRoot) { Join-Path $reportedRoot 'github-407/surfacing_bug.zip' } else { '' }
if (-not $zip -or -not (Test-Path -LiteralPath $zip -PathType Leaf)) {
  Write-Host 'SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-407) is not here'
  exit 77
}
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$unpacked = Join-Path $work 'reported'
Expand-Archive -LiteralPath $zip -DestinationPath $unpacked -Force
$design = Get-ChildItem -LiteralPath $unpacked -Recurse -File -Filter '*.mitcad' | Sort-Object FullName | Select-Object -First 1
if (-not $design) { Ui-Fail 'no design in the reported zip' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/surface-patch-browser-curve.mitcad-ui'
Ui-StartApp 'surface-patch-browser-curve' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(900000)) { Ui-Fail 'surface patch browser curve driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "surface patch browser curve driver failed (exit code $exitCode)"
}
Ui-ExpectLog ' sketches, ' 'the Browser lists the sketches'' curves during Patch'
Ui-ExpectLog 'Notice hover highlighted: browser curve: sketch_curve c42 ' 'the curve''s row paints it under the pointer'
Ui-ExpectLog 'Patch Profiles or Edges: 2 edges, 1 sketch curve' 'two free edges and the line from the Browser are taken'
Ui-ExpectLog 'Added patch of 3 items' 'OK adds the patch'
Ui-ExpectLog 'Browser sketch curves: none' 'the curve rows go with the command'
Ui-ExpectLog 'TestDriver: done' 'every step passed'
Ui-StopApp
Write-Host 'PASS: patch with a sketch curve picked in the Browser'
