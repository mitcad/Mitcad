# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp tools/cli
# The BUILD lists as text (mitcad#1641) in the build VM, as
# tools/ui-build-lists-text-test.sh on Linux: the shared steps
# tools/ui-scripts/build-lists-text.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/build-lists-text.json: three components, one with
# a long name): the Cut List's Copy as CSV puts rows ending in CRLF on the
# clipboard and Save as CSV... writes the same; Copy as Text puts the tab's
# tables there as aligned plain text with CRLF line ends, and Save as
# Text... writes exactly that text; the Bill of Materials likewise.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-lists-text-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build-lists-text'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
# The files of an earlier run: the save dialogs would ask to replace them.
$saved = @('cut-list.csv', 'cut-list.txt', 'bill-of-materials.txt')
foreach ($name in $saved) { Remove-Item -Force (Join-Path $work $name) -ErrorAction SilentlyContinue }
$design = Join-Path $work 'build-lists-text.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/build-lists-text.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/build-lists-text.mitcad-ui'
Ui-StartApp 'build-lists-text' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'BUILD lists as text driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "BUILD lists as text driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Build list copied as CSV: ' 'Copy as CSV copied the cut list'
Ui-ExpectLog 'Build list saved as CSV: ' 'Save as CSV wrote the cut list'
Ui-ExpectLog 'Build list copied as text: ' 'Copy as Text copied the tables'
Ui-ExpectLog 'Build list saved as text: ' 'Save as Text wrote the tables'
Ui-ExpectLog 'TestDriver: done' 'the clipboard and the saved files hold the same text, every line ending in CRLF'
Ui-StopApp
# The saved files outside the application: CRLF line ends only.
foreach ($name in $saved) {
  $path = Join-Path $work $name
  if (-not (Test-Path $path)) { Ui-Fail "$name was not written" }
  $bytes = [System.IO.File]::ReadAllBytes($path)
  if ($bytes.Length -lt 2 -or $bytes[$bytes.Length - 2] -ne 13 -or $bytes[$bytes.Length - 1] -ne 10) {
    Ui-Fail "$name does not end in CRLF"
  }
  for ($i = 0; $i -lt $bytes.Length; $i++) {
    if ($bytes[$i] -eq 10 -and ($i -eq 0 -or $bytes[$i - 1] -ne 13)) { Ui-Fail "$name has a line ending in LF alone" }
  }
  Write-Host "ok   ${name}: every line ends in CRLF"
}
$text = [System.IO.File]::ReadAllText((Join-Path $work 'cut-list.txt'), [System.Text.Encoding]::UTF8)
if (-not $text.Contains("`r`n       1  A component with a rather long name`r`n")) {
  Ui-Fail 'cut-list.txt has no aligned row of the component with the long name'
}
Write-Host 'ok   cut-list.txt: the long name in its padded column'
Write-Host 'PASS: BUILD lists as text'
