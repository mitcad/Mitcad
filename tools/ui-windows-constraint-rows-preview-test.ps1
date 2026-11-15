# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/constraint-rows-preview.mitcad-ui tools/ui-scripts/constraint-rows-preview.json
# Assembly Constraint's preview with more than one row (mitcad#1813) in the
# build VM, as tools/ui-constraint-rows-preview-test.sh on Linux: the
# shared steps tools/ui-scripts/constraint-rows-preview.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/constraint-rows-preview.json):
# row 1 complete is previewed; Add Constraint leaves the new row out of the
# preview, which still places the block by row 1 and says what the row
# needs; the second row complete places the block by both rows, drawn
# once, with its picks where the preview puts it; OK adds the set as
# previewed.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-constraint-rows-preview-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-constraint-rows-preview'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'constraint-rows-preview.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/constraint-rows-preview.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/constraint-rows-preview.mitcad-ui'
Ui-StartApp 'constraint-rows-preview' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'constraint rows preview driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "constraint rows preview driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Assembly Constraint moves O2' 'row 1 previewed: the block moved'
Ui-ExpectLog 'Preview Assembly Constraint without its incomplete rows (rows row 2: Select geometry 1.): ok' 'the empty row left out, row 1 still previewed'
Ui-ExpectLog 'Preview Assembly Constraint without its incomplete rows (rows row 2: Select geometry 2.): ok' 'the row with only its Geometry 1 left out, row 1 still previewed'
Ui-ExpectLog 'Assembly Constraint picks drawn where the preview places them: 2' 'the picks on the block where the preview puts it'
Ui-ExpectLog 'Added constraint Constraint1: align, align placed, moved O2' 'OK added the set'
Ui-ExpectLog 'TestDriver: done' 'the rows'' preview'
Ui-StopApp
Write-Host 'PASS: Assembly Constraint previews its complete rows and draws a moved part once'
