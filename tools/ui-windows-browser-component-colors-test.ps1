# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/ComponentColors.hpp app/browser/TimelineWidget.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/browser-component-colors.mitcad-ui tools/ui-scripts/browser-component-colors.json
# The Browser's component color bars (mitcad#1876) in the build VM, as
# tools/ui-browser-component-colors-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-component-colors.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/browser-component-colors.json), in
# the Floating layout (the Linux test runs the Docked one too): every
# component, occurrence and body row logs its component's timeline color,
# the root's rows the section caps' orange, both placements of a
# component the same; the colors follow a rename, a move into another
# component, a new component and its undo, and stay in the dark theme.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-component-colors-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-component-colors'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-component-colors.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-component-colors.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/browser-component-colors.mitcad-ui'
Ui-StartApp 'browser-component-colors' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'browser component colors driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "browser component colors driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Browser colors: Root #f29e1a, ' "the root's rows in the default orange"
Ui-ExpectLog 'Root/Base:1/Bolt:2 #' "the moved occurrence's rows keep its color"
Ui-ExpectLog 'TestDriver: done' 'the component color bars'
Ui-StopApp
Write-Host 'PASS: the Browser rows carry their components'' timeline colors'
