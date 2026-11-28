# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowInvertSelection.cpp app/OcctViewer.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindow.cpp app/resources/layout/default.json app/framework/ShortcutDialog.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/invert-selection.mitcad-ui tools/ui-scripts/invert-selection.json
# Invert Selection (mitcad#2647) in the build VM, as
# tools/ui-invert-selection-test.sh on Linux: the shared steps
# tools/ui-scripts/invert-selection.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/invert-selection.json): F7 bound in Keyboard
# Shortcuts; bodies inverted (the hidden one not taken), the faces of an
# isolated body, a sketch's lines (by the key and the view's context menu),
# and Interference's Bodies input.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-invert-selection-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-invert-selection'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'invert-selection.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/invert-selection.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/invert-selection.mitcad-ui'
Ui-StartApp 'invert-selection' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(170000)) { Ui-Fail 'invert selection driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "invert selection driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Invert Selection: 1 face to 5 faces' 'the faces of the isolated body inverted'
Ui-ExpectLog 'Invert Selection in Interference Bodies: nothing to 2 bodies' 'the input took the shown bodies'
Ui-ExpectLog 'TestDriver: done' 'bodies, faces, sketch curves and a command input inverted'
Ui-StopApp
Write-Host 'PASS: invert selection'
