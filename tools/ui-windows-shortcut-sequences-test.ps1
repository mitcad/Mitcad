# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ShortcutDialog.cpp app/view/ShortcutOverview.cpp app/view/ViewController.cpp app/build/ListPages.cpp tools/cli
# Key sequences in Keyboard Shortcuts and the Keyboard and Mouse Overview's
# cheat sheet (mitcad#2095) in the build VM, as
# tools/ui-shortcut-sequences-test.sh on Linux: the shared steps
# tools/ui-scripts/shortcut-sequences.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/shortcut-sequences.json): G, L recorded
# for a command (ended by the pause, kept by Enter, the old key kept by Esc)
# and saved; G alone starts nothing in the 3D view, G then L starts it; the
# overview's Save as PDF... and Print... (to MITCAD_TEST_PRINT_PDF's file); E, X
# for another command makes OK say that E (Extrude) is its start.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-shortcut-sequences-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-shortcut-sequences'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
# The files of an earlier run: the save dialog would ask to replace them.
$pdf = Join-Path $work 'MitCad keyboard shortcuts.pdf'
$printed = Join-Path $work 'printed.pdf'
Remove-Item -Force $pdf, $printed -ErrorAction SilentlyContinue
$design = Join-Path $work 'sequences.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/shortcut-sequences.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_PRINT_PDF = $printed
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/shortcut-sequences.mitcad-ui'
Ui-StartApp 'shortcut-sequences' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Shortcut sequences driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Shortcut sequences driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Keyboard Shortcuts recording ended by Enter' 'Enter ends a recording and keeps G, L'
Ui-ExpectLog 'Keyboard Shortcuts saved' 'G, L saved for Keyboard and Mouse Overview'
Ui-ExpectLog 'is the start of E, X (Keyboard and Mouse Overview)' 'OK names the prefix clash in words'
Ui-ExpectLog 'Shortcut overview PDF written: ' 'Save as PDF... wrote the file it offered'
foreach ($file in @($pdf, $printed)) {
  if (-not (Test-Path $file)) { Ui-Fail "$file was not written" }
  $head = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes($file), 0, 5)
  if ($head -ne '%PDF-') { Ui-Fail "$file is no PDF" }
}
Write-Host 'ok   the cheat sheet saved and printed as PDF'
Ui-ExpectLog 'Printed Keyboard and Mouse Overview: ' 'Print... printed the cheat sheet'
Ui-ExpectLog 'TestDriver: done' 'every step of the key sequences passed'
Ui-StopApp
Write-Host 'PASS: Shortcut sequences'
