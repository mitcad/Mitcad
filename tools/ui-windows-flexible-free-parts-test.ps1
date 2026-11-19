# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowJoints.cpp core/model/src/joints.rs tools/cli
# check-all sources: core/model/src/document/flexible.rs core/model/src/document/joints.rs core/model-api/src/joints.rs
# Flexible on a subassembly without joints of its own (mitcad#2016) in the
# build VM, as tools/ui-flexible-free-parts-test.sh on Linux: the shared
# steps tools/ui-scripts/flexible-free-parts.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/flexible-free-parts.json): Gantry:1
# is made Flexible from its row's menu and its free Joiner dragged along
# the root's slider, alone; Gantry:1's Carriage and Gantry:2 stay.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flexible-free-parts-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-flexible-free-parts'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'flexible-free-parts.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/flexible-free-parts.json') --save $design
if (-not (Test-Path $design)) { Ui-Fail 'flexible free parts fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/flexible-free-parts.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'flexible-free-parts' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'flexible free parts driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "flexible free parts driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Gantry:1 made flexible, its free Joiner dragged along the root''s joint alone'
Ui-StopApp
Write-Host 'PASS: a subassembly without joints is flexible and its free part drags alone'
