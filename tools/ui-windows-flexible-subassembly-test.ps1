# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/browser/BrowserPanel.cpp app/MainWindowJoints.cpp core/model/src/joints.rs tools/cli
# check-all sources: app/browser/DocumentHost.hpp core/model/src/document/flexible.rs core/model/src/document/joints.rs core/model-api/src/joints.rs
# A flexible subassembly (mitcad#1793) in the build VM, as
# tools/ui-flexible-subassembly-test.sh on Linux: the shared steps
# tools/ui-scripts/flexible-subassembly.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/flexible-subassembly.json): Hinge:1 is
# made Flexible from its row's menu and its inner Arm dragged along the
# Hinge's own joint, for Hinge:1 alone; Hinge:2's Arm stays; then Joint1
# is animated inside Hinge:1 from its row's menu (mitcad#1805).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flexible-subassembly-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-flexible-subassembly'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'flexible-subassembly.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/flexible-subassembly.json') --save $design
if (-not (Test-Path $design)) { Ui-Fail 'flexible subassembly fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/flexible-subassembly.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'flexible-subassembly' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'flexible subassembly driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "flexible subassembly driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Hinge:1 made flexible, its arm dragged and animated for it alone'
Ui-StopApp
Write-Host 'PASS: a flexible subassembly moves its own joints in its parent'
