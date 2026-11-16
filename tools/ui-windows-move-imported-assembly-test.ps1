# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/framework/Command.hpp app/framework/Command.cpp app/framework/CommandSession.cpp app/framework/CommandSessionScript.cpp app/framework/TestDriver.cpp core/model/src/exchange tools/cli tools/ui-scripts/move-imported-assembly.mitcad-ui tools/ui-scripts/move-imported-assembly.json tools/ui-scripts/move-imported-assembly-source.json
# Move/Copy, Move Object Components, on an imported STEP assembly
# (mitcad#1891) in the build VM, as tools/ui-move-imported-assembly-test.sh
# on Linux: the shared steps tools/ui-scripts/move-imported-assembly.mitcad-ui
# on the design mitcad-cli makes in the work folder (a STEP assembly that
# places a part twice, tools/ui-scripts/move-imported-assembly-source.json,
# imported into a design with a body and a component in the root,
# tools/ui-scripts/move-imported-assembly.json). A click on a body of the
# imported part takes the import's occurrence in the root, not the part's
# nested one; 40 mm along X moves the import only, and one undo puts it
# back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-imported-assembly-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-imported-assembly'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'move-imported-assembly.mitcad'
# Both scripts name their files relative to the work folder.
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-imported-assembly-source.json') 2>&1
  $status = $LASTEXITCODE
  if ($status -eq 0) {
    $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-imported-assembly.json') --save $design 2>&1
    $status = $LASTEXITCODE
  }
} finally {
  Pop-Location
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: the design was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-imported-assembly.mitcad-ui'
Ui-StartApp 'move-imported-assembly' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'move imported assembly driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "move imported assembly driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Move/Copy Objects: 1 component [component ' "a click on the imported part's body takes the import's occurrence"
Ui-ExpectLog 'Moved 1 components (' "the import's occurrence moved"
Ui-ExpectLog 'TestDriver: done' 'Move/Copy of an imported STEP assembly'
Ui-StopApp
Write-Host 'PASS: Move/Copy of an imported STEP assembly moves the import''s occurrence'
