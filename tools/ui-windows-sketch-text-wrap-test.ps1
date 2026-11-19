# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# A sketch text drawn in a frame wraps to the frame's width (mitcad#2029)
# on Windows, the shared input script
# tools/ui-scripts/sketch-text-wrap.mitcad-ui as
# tools/ui-sketch-text-wrap-test.sh runs it on Linux: a frame dragged with
# the Text tool and a long text typed, its lines within the frame; Edit
# Text's Height, a frame corner dragged, Wrap in Frame off and on, and a
# line break typed in Edit Text's Text box; the saved sketch has the text
# in two lines, wrapped.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-text-wrap'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'text.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-text-wrap.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch text wrap fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-text-wrap.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-text-wrap' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch text wrap driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch text wrap driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a text wrapped in its frame, rewrapped and typed in two lines'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-text-wrap-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the saved text is not wrapped in two lines' }
Ui-StopApp
Write-Host 'PASS: sketch text wrapped in its frame'
