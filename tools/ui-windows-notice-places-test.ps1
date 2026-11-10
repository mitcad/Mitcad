# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp app/framework/CommandMessage.cpp app/framework/MessageBand.cpp app/framework/TestDriver.cpp app/framework/CommandSession.cpp geometry/src/dressup_check.cpp tools/cli tools/ui-scripts/notice-places-region.mitcad-ui tools/ui-scripts/notice-places-region.json
# Notice places (mitcad#1327) in the build VM, as
# tools/ui-notice-places-test.sh on Linux: the shared steps
# tools/ui-scripts/notice-places.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/notice-places.json): a rim's rounding that runs
# onto the rounding of the block's front edge warns in plain words, and the pointer over the
# warning paints its places in the view on the Timeline item, in the
# message band and in the command panel's message while it is edited;
# moving away clears them, as does another message in the hovered one's
# place (mitcad#1609); a hover while a preview computes reads nothing from
# the design and paints once it has ended (mitcad#1600). Then
# tools/ui-scripts/notice-places-region.mitcad-ui on
# notice-places-region.json: a sketch region's place in a component moved
# by 100 mm painted where the component is shown (mitcad#1609). The
# screenshots (notice-places-*.png, in the output folder's shots) show the
# highlight.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-notice-places-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-notice-places'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
$shots = Join-Path $script:UiOut 'shots'
New-Item -ItemType Directory -Force $work | Out-Null
New-Item -ItemType Directory -Force $shots | Out-Null
$design = Join-Path $work 'notice-places.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/notice-places.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$region = Join-Path $work 'notice-places-region.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/notice-places-region.json') --save $region
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the region design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_SHOTS = $shots
# The app on a design with an input script (tools/ui-scripts/<name>.mitcad-ui),
# until its driver is done.
function Run-Script([string]$Name, [string]$Design) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $Design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "$Name driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$Name driver failed (exit code $exitCode)"
  }
}
Run-Script 'notice-places' $design
Ui-ExpectLog 'Notice hover highlighted: timeline: ' 'the Timeline item paints its places'
Ui-ExpectLog 'Notice hover highlighted: message band: ' 'the message band paints its places'
Ui-ExpectLog 'Notice hover highlighted: panel: ' 'the command panel''s message paints its places'
Ui-ExpectLog 'Panel Fillet message hovered: 0 places' 'the hint of no preview paints no warning''s places'
Ui-ExpectLog '(drawn when the job ends)' 'hovered while a preview computes, nothing is read from the design'
Ui-ExpectLog 'TestDriver: done' 'the notice places are painted on hover'
# A sketch region of a moved component (mitcad#1609).
Run-Script 'notice-places-region' $region
Ui-ExpectLog 'Notice hover bounds: x 70.0 to 130.0, ' 'a moved component''s region is painted where it is shown'
Ui-ExpectLog 'TestDriver: done' 'the region is painted on hover'
Ui-StopApp
Write-Host 'PASS: notice places are painted on hover'
