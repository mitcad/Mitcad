# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli
# check-all sources: core/3mf core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# 3MF import (mitcad#2156) in the build VM, as tools/ui-3mf-import-test.sh
# on Linux: mitcad-cli writes plate.3mf with MitCad's 3MF export
# (tools/ui-scripts/3mf-import.json: Plate placed once, Pin placed twice);
# the shared input script tools/ui-scripts/3mf-import.mitcad-ui then opens
# it (a mesh body and a component of two occurrences, nothing asked) and
# imports it with File > Import into a new design (Insert Mesh asks no
# unit), finding the same bodies and component.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-3mf-import-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-3mf-import'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The file, written in the folder the script's relative path is in.
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/3mf-import.json') 2>&1
  $status = $LASTEXITCODE
} finally {
  Pop-Location
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: plate.3mf: $($output -join ' / ')" }
$plate = Join-Path $work 'plate.3mf'
$bytes = [IO.File]::ReadAllBytes($plate)
if ($bytes.Length -lt 2 -or [Text.Encoding]::ASCII.GetString($bytes, 0, 2) -ne 'PK') {
  Ui-Fail 'plate.3mf is no zip package'
}
Write-Host 'ok   plate.3mf written by the 3MF export'

# The script types the path with forward slashes, as the log shows it.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/3mf-import.mitcad-ui'
Ui-StartApp '3mf-import' @('--ui=qt', '--no-mcp', '--open', $plate)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3MF import driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3MF import driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Open and File > Import of plate.3mf: a mesh body and a component of two occurrences'
Ui-StopApp
Write-Host 'PASS: 3MF import'
