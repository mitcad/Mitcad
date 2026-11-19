# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/flat_pack.rs core/model/src/features/finger_joint.rs core/model-api/src/sheet_process.rs core/model-api/src/cut_sheets.rs app/framework/TestDriver.cpp tools/cli tools/ui-scripts/flat-pack.mitcad-ui tools/ui-scripts/flat-pack.json
# Flat Pack (mitcad#1939) in the build VM, as tools/ui-flat-pack-test.sh on Linux: the
# shared steps tools/ui-scripts/flat-pack.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/flat-pack.json). A box picked in the browser becomes six finger-jointed plates on a cut sheet, taken back by one undo.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flat-pack-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-flat-pack'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'flat-pack.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/flat-pack.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/flat-pack.mitcad-ui'
Ui-StartApp 'flat-pack' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'flat-pack driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "flat-pack driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Flat pack: six plates of 6' 'the flat pack was made'
Ui-ExpectLog 'TestDriver: done' 'Flat Pack (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Flat Pack (mitcad#1939)'
