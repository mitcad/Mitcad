# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp tools/cli
# The BOM Structure and the Bill of Materials' structured and parts-only
# views (mitcad#1985) in the build VM, as tools/ui-bom-structure-test.sh on
# Linux: the shared steps tools/ui-scripts/bom-structure.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/bom-structure.json: a box
# and a subassembly holding another box twice): the subassembly's BOM
# Structure set to Inseparable, Phantom and Reference, and both views
# changing with it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-bom-structure-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-bom-structure'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'bom-structure.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/bom-structure.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/bom-structure.mitcad-ui'
Ui-StartApp 'bom-structure' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'BOM Structure driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "BOM Structure driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Build BOM item:   2.1 Rail x 2 (Normal)' 'the structured view'
Ui-ExpectLog 'Build BOM item: 1 Base x 3 (Normal)' 'identical parts grouped in Parts only'
Ui-ExpectLog 'Build BOM item: 2 Frame x 1 (Inseparable)' 'an Inseparable subassembly'
Ui-ExpectLog 'Build BOM item: 2 Rail x 2 (Normal)' "a Phantom's children a level up"
Ui-ExpectLog 'Build BOM item: - Frame x 0 (Reference)' 'a Reference without a number'
Ui-ExpectLog 'TestDriver: done' 'every step of the BOM Structure passed'
Ui-StopApp
Write-Host 'PASS: BOM Structure'
