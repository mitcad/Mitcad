# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# The Bead command (mitcad#2645) in the build VM, as
# tools/ui-sheet-bead-test.sh on Linux: the shared steps
# tools/ui-scripts/sheet-bead.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/sheet-bead.json): a round bead pressed along a sketch
# line on a plate's top face, opened in Bead and changed to a trapezoid,
# then pressed into the face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-bead-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-bead'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-bead.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-bead.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-bead.mitcad-ui'
Ui-StartApp 'sheet-bead' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet bead driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet bead driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added Bead as F5' 'the bead was added'
Ui-ExpectLog 'Editing F5 with Bead' 'the bead opened in Bead'
Ui-ExpectLog 'TestDriver: done' 'a bead made, changed to a trapezoid and pressed into the face'
Ui-StopApp
Write-Host 'PASS: a bead made, changed to a trapezoid and pressed into the face'
