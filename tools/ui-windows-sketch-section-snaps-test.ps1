# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model/src/document/plane_geometry/section.rs
# check-all sources: core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-section-snaps.mitcad-ui tools/ui-scripts/sketch-section-snaps.json
# The drawing tools snap to the centres of circular body edges and to where
# the sketch plane cuts the bodies (mitcad#2324, GitHub mitcad/Mitcad#579)
# in the build VM, as tools/ui-sketch-section-snaps-test.sh on Linux: the
# shared steps tools/ui-scripts/sketch-section-snaps.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sketch-section-snaps.json: a
# cylinder and a sphere, Sketch1 on a plane parallel to the cylinder's caps
# through both): a line drawn to the cylinder's axis ends on the centre of
# its caps' projected circle, one drawn to the sphere's centre on the centre
# of the sphere's linked section; each is one undo step; the line's end
# follows the section when the sphere moves.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-section-snaps-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-section-snaps'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'section.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-section-snaps.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-section-snaps.mitcad-ui'
Ui-StartApp 'sketch-section-snaps' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch section snaps driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch section snaps driver failed (exit code $exitCode)"
}
Ui-ExpectLog ', the projected center of E{' 'a line end held on the projected centre of the cylinder caps'
Ui-ExpectLog ', the section center of F2:' 'a line end held on the centre of the sphere section'
Ui-ExpectLog 'TestDriver: done' 'lines to a projected centre and a section centre, undone whole; the end follows the moved sphere'
Ui-StopApp
Write-Host 'PASS: sketch section snaps'
