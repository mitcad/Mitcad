# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/framework/EnterRepeat.cpp app/framework/ShortcutDialog.cpp tools/cli
# The command Repeat's own key (mitcad#2052) in the build VM, as
# tools/ui-repeat-key-test.sh on Linux: the shared steps
# tools/ui-scripts/repeat-key.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/repeat-key.json): with no key bound Space does nothing;
# Repeat's row in Keyboard Shortcuts has no key and takes Space; after an
# Extrude, Space in the 3D view repeats it (`Repeat Extrude (Space)`), not
# while its panel is open, not in a sketch, not with the browser's tree
# focused, and typed into a rename it is a space.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-repeat-key-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-repeat-key'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'repeat.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/repeat-key.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/repeat-key.mitcad-ui'
Ui-StartApp 'repeat-key' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Repeat key driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Repeat key driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Keyboard Shortcuts key: Repeat | 3D View = Space' 'Space given to Repeat in Keyboard Shortcuts'
Ui-ExpectLog 'Repeat Extrude (Space)' 'Space in the 3D view repeats Extrude'
Ui-ExpectLog 'TestDriver: done' 'the view repeats by Space; panels, sketches and the browser keep it'
Ui-StopApp
Write-Host 'PASS: Repeat key'
