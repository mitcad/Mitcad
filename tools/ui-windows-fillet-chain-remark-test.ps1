# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/browser/DocumentSnapshot.cpp core/model-api/src/lib.rs geometry/src/dressup.cpp
# check-all sources: tools/ui-scripts/fillet-chain-remark.mitcad-ui app/framework/TestDriver.cpp app/main.cpp
# A fillet's short tangent chain is a remark, not a warning (mitcad#1914),
# in the build VM: the shared steps tools/ui-scripts/fillet-chain-remark.mitcad-ui
# (also run by tools/ui-fillet-chain-remark-test.sh and
# tools/ui-macos-fillet-chain-remark-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, fillet-chain-note/; never
# committed), copied into the test's folder: its accepted Fillet12 marks
# nothing on the Timeline; edited again, the panel points the short chain
# out as information. Skipped (77) without the design. Its first step waits
# for the view as long as the start-up open takes under load (mitcad#1933).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-chain-remark-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-chain-remark'),
  [string]$QtBin = '')
$reportedRoot = $env:MITCAD_MITCAD_REPORTED
if (-not $reportedRoot -or -not (Test-Path -LiteralPath (Join-Path $reportedRoot 'fillet-chain-note') -PathType Container)) {
  Write-Host 'SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/fillet-chain-note) is not here'
  exit 77
}
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$copy = Join-Path $work 'reported'
Copy-Item -LiteralPath (Join-Path $reportedRoot 'fillet-chain-note') -Destination $copy -Recurse -Force
$design = Get-ChildItem -LiteralPath $copy -Recurse -File -Filter '*.mitcad' | Sort-Object FullName | Select-Object -First 1
if (-not $design) { Ui-Fail 'no design in the reported folder' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-chain-remark.mitcad-ui'
Ui-StartApp 'fillet-chain-remark' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(1200000)) { Ui-Fail 'fillet chain remark driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "fillet chain remark driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Fillet: ok, remark: ' 'the panel points the short chain out'
Ui-ExpectLog 'Panel Fillet message: information: ' 'as information, not a warning'
Ui-ExpectLog 'TestDriver: done' 'nothing on the timeline after OK, the remark in the open panel'
Ui-StopApp
Write-Host 'PASS: a fillet''s short tangent chain is a remark in the open panel, nothing on the timeline'
