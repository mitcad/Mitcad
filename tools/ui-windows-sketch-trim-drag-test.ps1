# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverSketch.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-trim-drag.mitcad-ui tools/ui-scripts/sketch-trim-drag.json
# Trim's drag (mitcad#2524) on Windows, the shared input script
# tools/ui-scripts/sketch-trim-drag.mitcad-ui as
# tools/ui-sketch-trim-drag-test.sh runs it on Linux: a drag that starts on
# a curve trims nothing while the button is held and Esc drops it, a drag
# from empty space trims three pieces as one undo step that one Undo takes
# back, a plain click trims one piece, and with Trim ended a left drag
# selects with a window.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-trim-drag'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'trim-drag.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-trim-drag.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch trim drag fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-trim-drag.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-trim-drag' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch trim drag driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch trim drag driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Trim drag trimmed 3 of 3 pieces as one step' 'a drag from empty space trims three pieces'
Ui-ExpectLog 'TestDriver: done' 'the drag, its Esc, its Undo, a click and a window selection after Trim'
Ui-StopApp
Write-Host 'PASS: sketch Trim drag'
