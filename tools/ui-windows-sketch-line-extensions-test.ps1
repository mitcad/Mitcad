# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/solver core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-line-extensions.mitcad-ui tools/ui-scripts/sketch-line-extensions.json
# Resting the pointer on a sketch line acquires it, and a drawing tool
# snaps onto its extension and to where two acquired lines' extensions
# meet (mitcad#2035, GitHub mitcad/Mitcad#433) in the build VM, as
# tools/ui-sketch-line-extensions-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-line-extensions.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-line-extensions.json): a line
# ended on a vertical line's extension is held there by a Coincident and
# follows the line when it moves; a rectangle's corner where two acquired
# lines' extensions meet is held on both; with Snap to line extensions
# unchecked nothing is acquired.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-line-extensions-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-line-extensions'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'lines.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-line-extensions.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-line-extensions.mitcad-ui'
Ui-StartApp 'sketch-line-extensions' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch line extensions driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch line extensions driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'on the extension of c2' 'a line end held on a line extension'
Ui-ExpectLog 'on the extension of c8' 'a rectangle corner held where two line extensions meet'
Ui-ExpectLog 'TestDriver: done' 'line extensions acquired, snapped to, followed and turned off'
Ui-StopApp
Write-Host 'PASS: sketch line extensions'
