# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-glyph-clear.mitcad-ui tools/ui-scripts/sketch-glyph-clear.json
# A constraint symbol keeps clear of the curves (mitcad#1594) on Windows,
# the shared input script tools/ui-scripts/sketch-glyph-clear.mitcad-ui as
# tools/ui-sketch-glyph-clear-test.sh runs it on Linux: two close parallel
# lines with a Parallel between them; both of its symbols are in the glyph
# log, a click on the upper line's middle selects the line, not the
# Parallel whose second symbol covered it, and a click on that symbol
# selects the Parallel.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-glyph-clear-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-glyph-clear'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'glyph-clear.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-glyph-clear.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the sketch glyph clear design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-glyph-clear.mitcad-ui'
Ui-StartApp 'sketch-glyph-clear' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch glyph clear driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch glyph clear driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Constraint k12 glyph at ' "k12's symbols are logged"
foreach ($line in @('c6', 'c3')) {
  $found = Ui-LogLines | Where-Object { $_ -match "^Constraint k12 glyph at [0-9]+,[0-9]+ \(parallel by $line\)$" }
  if (-not $found) { Ui-Fail "k12's symbol beside $line is not logged" }
}
Write-Host "ok   both of k12's symbols are logged"
Ui-ExpectLog 'TestDriver: done' 'a click on the upper line selects it, the moved symbol the Parallel'
Ui-StopApp
Write-Host 'PASS: sketch glyph clear of curves'
