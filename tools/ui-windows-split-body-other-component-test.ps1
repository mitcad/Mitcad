# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp app/commands/SplitToolLinks.cpp core/model/src/features/split_body.rs app/framework/TestDriver.cpp tools/cli
# Split Body with a face of another component as the Splitting Tool
# (mitcad#2299, GitHub mitcad/Mitcad#588) in the build VM, as
# tools/ui-split-body-other-component-test.sh on Linux: the shared steps
# tools/ui-scripts/split-body-other-component.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/split-body-other-component.json): with
# B active, B's block split by the top face of A's block, picked where A:1
# shows it, builds in B without an activation; Undo takes it back; an edit
# shows the tool face where A:1 shows it; moving A:1 moves the cut.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-split-body-other-component-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-split-body-other-component'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'split-body-other-component.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/split-body-other-component.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/split-body-other-component.mitcad-ui'
Ui-StartApp 'split-body-other-component' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'split body other component driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "split body other component driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Split Body Splitting Tool: 1 face [face F2:end(' 'the top face of A''s block picked as the tool'
Ui-ExpectLog 'Added split body of 1 body' 'OK added the split in B'
Ui-ExpectLog 'Editing F5 with Split Body' 'the split edited'
Ui-ExpectLog 'TestDriver: done' 'a face of another component splits the body, follows its occurrence'
Ui-StopApp
Write-Host 'PASS: Split Body with a face of another component'
