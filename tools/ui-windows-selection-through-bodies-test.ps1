# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/PickContext.cpp app/framework/TestDriver.cpp tools/cli
# The selection through the bodies (mitcad#1956) in the build VM, as
# tools/ui-selection-through-bodies-test.sh on Linux: the shared steps
# tools/ui-scripts/selection-through-bodies.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/selection-through-bodies.json): the
# profile a block hides, selected with no command active, and a face of the
# block turned away drawn through it; with Selection Through Bodies off,
# neither.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-selection-through-bodies-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-selection-through-bodies'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'through.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/selection-through-bodies.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/selection-through-bodies.mitcad-ui'
Ui-StartApp 'selection-through-bodies' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'selection through bodies driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "selection through bodies driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Selection through the bodies: profile F2/' 'the buried profile drawn through the block'
Ui-ExpectLog 'Selection through the bodies: face F1.b0/' 'the face turned away drawn through the block'
Ui-ExpectLog 'Selection through the bodies: off' 'Selection Through Bodies off: neither'
Ui-ExpectLog 'TestDriver: done' 'the selection through the bodies'
Ui-StopApp
Write-Host 'PASS: selection through bodies'
