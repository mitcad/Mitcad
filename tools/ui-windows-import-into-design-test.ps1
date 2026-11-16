# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/Dialogs.cpp app/framework/TestDriver.cpp app/WindowManager.cpp
# check-all sources: core/model-api/src/lib.rs core/ffi/src/ipt_import.rs core/ipt core/tests tools/cli
# check-all sources: tools/ui-scripts/import-into-design.mitcad-ui tools/ui-scripts/import-into-design.json
# Design files of other formats into the open design as components
# (mitcad#1894) in the build VM, as tools/ui-import-into-design-test.sh on
# Linux: the shared input script tools/ui-scripts/import-into-design.mitcad-ui
# on a design with content (tools/ui-scripts/import-into-design.json, a
# 20 mm cube) and the two .ipt test parts of test_exchange (--write-ipt,
# --write-ipt-design): File > Import of both at once into this design (one
# Where to Import, each a component named after its file with its history,
# side by side, one undo step, each file's report), Undo taking both out,
# and a third import as a new design in a tab of its own.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-import-into-design-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe -TestExchange build\dev\test_exchange.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$TestExchange,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-import-into-design'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'assembly.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/import-into-design.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
& $TestExchange --write-ipt (Join-Path $work 'part.ipt')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the test part was not written' }
& $TestExchange --write-ipt-design (Join-Path $work 'designed.ipt')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the designed test part was not written' }

# The script types the paths with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/import-into-design.mitcad-ui'

Ui-StartApp 'import-into-design' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(420000)) { Ui-Fail 'the import driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the import driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'two .ipt parts imported into the design as components in one undo step; a third as a new design'
Ui-StopApp
Write-Host 'PASS: import into this design'
