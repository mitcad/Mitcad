# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/framework/CommandSessionVisibility.cpp app/framework/TestDriver.cpp tools/cli
# Browser eyes while a command is open (mitcad#1263) in the build VM, as
# tools/ui-browser-eyes-mid-command-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-eyes-mid-command.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/browser-eyes-mid-command.json):
# Section1's eye during a modelled Thread turns the section on, the panel
# stays open and its preview is clipped at the plane, OK adds the thread
# and Undo takes back the thread before the eye; Esc after Section1's eye
# cancels Thread and keeps the section, while Delete on its row still waits
# for the command; during Fillet the other body's eye and the filleted
# body's own keep the selection and OK adds the fillet (Undo: the fillet,
# then each eye).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-eyes-mid-command-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-eyes-mid-command'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-eyes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-eyes-mid-command.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/browser-eyes-mid-command.mitcad-ui'
Ui-StartApp 'browser-eyes-mid-command' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
# The modelled thread's previews and OK take the longest: up to 9 minutes.
if (-not $script:UiProcess.WaitForExit(540000)) { Ui-Fail 'browser eyes driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "browser eyes driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Visibility Root/Analysis/Section1: shown (during Thread)' 'Section1 turned on during Thread'
Ui-ExpectLog 'Section preview ready at ' 'the thread''s preview clipped at the section'
Ui-ExpectLog 'Added modeled thread M10x1.5 6g on 1 face(s)' 'OK added the thread with the panel''s inputs'
Ui-ExpectLog 'Command Thread cancelled' 'Esc cancelled Thread after the eye'
Ui-ExpectLog 'Visibility Root/Bodies/Body2: hidden (during Fillet)' 'the filleted body hidden during Fillet'
Ui-ExpectLog 'Added fillet on 4 edge(s)' 'OK added the fillet'
Ui-ExpectLog 'TestDriver: done' 'the eyes work while a command is open'
Ui-StopApp
Write-Host 'PASS: Browser eyes work while a command is open'
