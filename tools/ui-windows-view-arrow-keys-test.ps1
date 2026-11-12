# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/CameraTurn.hpp app/OcctViewer.cpp tools/cli
# The arrow keys turn the view (mitcad#1211) in the build VM, as
# tools/ui-view-arrow-keys-test.sh on Linux: the shared steps
# tools/ui-scripts/view-arrow-keys.mitcad-ui on the block that mitcad-cli
# makes (tools/ui-scripts/view-arrow-keys.json): with the keyboard in the
# 3D view the keys turn the camera by 15 degrees, the zoom kept, and 7,
# 1 and Home show the isometric, front and home views (mitcad#1684); with
# the keyboard in Extrude's value field they leave the view as it is and 7
# types the digit.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-arrow-keys-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-arrow-keys'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/view-arrow-keys.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the block was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-arrow-keys.mitcad-ui'
Ui-StartApp 'view-arrow-keys' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'arrow keys driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "arrow keys driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the arrow keys, Home and 1 to 7 move the view, not from a value field'
Ui-StopApp
Write-Host 'PASS: arrow keys turn the view'
