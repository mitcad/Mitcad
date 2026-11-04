# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSection.cpp app/framework/SectionPreview.cpp app/browser/BrowserPanel.cpp tools/cli
# A section that shows a body whole because its clip failed is reported
# (mitcad#1230) in the build VM, as tools/ui-section-clip-report-test.sh
# on Linux: the shared steps tools/ui-scripts/section-clip-report.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/section-clip-report.json), with
# MITCAD_TEST_SECTION_CLIP_FAIL=F2.b0 making Body1's clips fail: the
# message band's warning, the history's error and the Analysis folder
# row's warning style and tooltip; hiding the analysis withdraws both; the
# Section Analysis panel's section reports it too, and its plane past the
# body gives the panel's status line text; an analysis that cuts no shown
# body says so in its row's tooltip.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-section-clip-report-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-section-clip-report'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'clip-report.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/section-clip-report.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/section-clip-report.mitcad-ui'
$env:MITCAD_TEST_SECTION_CLIP_FAIL = 'F2.b0'
Ui-StartApp 'section-clip-report' @('--ui=qt', '--no-mcp', '--open', $design)
Remove-Item Env:MITCAD_TEST_SECTION_CLIP_FAIL
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'section clip report driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "section clip report driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a failed clip reported in the band and the analysis row, withdrawn when hidden'
Ui-StopApp
Write-Host 'PASS: a section clip that failed is reported'
