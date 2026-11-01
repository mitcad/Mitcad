# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/CMakeLists.txt app/report/ReportCenter.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/user-guide.mitcad-ui
# Help > User Guide (mitcad#1032) in the build VM, Docked and Floating,
# through the test driver (tools/ui-scripts/user-guide.mitcad-ui, the same
# script as tools/ui-user-guide-test.sh's): the entry in the menu bar's
# Help menu and the command by its id each would open the user guide on
# GitHub, at the release's tag or main (MITCAD_TEST_LOG_URLS=1: logged, not
# opened).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-user-guide-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-user-guide'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_LOG_URLS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/user-guide.mitcad-ui'
$guide = '(?m)Open URL \(test, not opened\): https://github\.com/mitcad/Mitcad/blob/(main|v[0-9]+\.[0-9]+\.[0-9]+)/docs/user-guide\.md\r?$'

foreach ($chrome in @('docked', 'floating')) {
  Ui-StartApp "user-guide-$chrome" @('--no-mcp', '--demo', "--chrome=$chrome")
  # The script ends with `stay`: done, or a failed step ends the app.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if ((Ui-CountLog 'TestDriver: (done|FAIL)') -gt 0 -or $script:UiProcess.HasExited) { break }
    Start-Sleep -Milliseconds 200
  } while ($clock.ElapsedMilliseconds -lt 60000)
  if ((Ui-CountLog 'TestDriver: done') -eq 0) {
    Ui-Fail "the test script failed (${chrome}): $(Ui-LastLineWith 'TestDriver: FAIL')"
  }
  $opened = Ui-CountLog $guide
  if ($opened -ne 2) { Ui-Fail "${chrome}: the guide's address logged $opened times, not 2" }
  Write-Host "ok   ${chrome}: Help > User Guide and the command opened the user guide"
  Ui-StopApp
}
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Write-Host 'PASS: user guide'
