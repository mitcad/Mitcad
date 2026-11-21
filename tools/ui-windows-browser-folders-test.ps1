# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_folders.rs tools/cli tools/ui-scripts/browser-folders.mitcad-ui
# Folders in the Browser (mitcad#2103) in the build VM, as
# tools/ui-browser-folders-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-folders.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/browser-folders.json): components and bodies into
# folders from the row menu and by dragging, the folder's eye, Rename,
# Ungroup and Undo; the saved design checked with
# browser-folders-check.json.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-folders-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-folders'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-folders.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-folders.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$name = 'browser-folders'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${name}: driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "${name}: driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
Ui-StopApp
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-folders-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved design does not pass browser-folders-check.json" }
Write-Host 'PASS: components and bodies into folders and out, the folder eye, Rename, Ungroup and Undo, kept with the design'
