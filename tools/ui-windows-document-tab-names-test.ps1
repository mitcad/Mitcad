# SPDX-License-Identifier: MIT
# check-all sources: app/DocumentFrame.cpp app/DocumentFrame.hpp app/framework/TitleBar.cpp app/framework/TestDriverDocuments.cpp tools/cli tools/ui-scripts/document-tab-names.mitcad-ui tools/ui-scripts/document-tab-names.json
# The document tabs' names in the Floating top bar (mitcad#2060) in the
# build VM, as tools/ui-document-tab-names-test.sh on Linux: the shared
# steps tools/ui-scripts/document-tab-names.mitcad-ui with designs named
# Bracket, Untitled and a 60-character name: the first two shown whole,
# shown or not, the long one elided. The picture is kept in -Out.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-document-tab-names-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-document-tab-names'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'Bracket.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/document-tab-names.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
Copy-Item $design (Join-Path $work 'A design with a name far too long for its tab in the top bar.mitcad')
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/document-tab-names.mitcad-ui'
Ui-StartApp 'document-tab-names' @('--ui=qt', '--no-mcp', '--no-recovery', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'document tab names driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "document tab names driver failed (exit code $exitCode)"
}
Ui-ExpectLog "Document tabs shown: 'Bracket' 'A design" 'Bracket shown whole beside the long name'
Ui-ExpectLog 'TestDriver: done' 'the document tabs show short names whole and elide the long one'
Ui-StopApp
Write-Host 'PASS: the document tabs show short names whole and elide the long one'
