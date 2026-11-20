# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BoostButton.cpp app/framework/BoostButton.hpp app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/framework/Ribbon.cpp app/MainWindow.cpp app/view/ViewController.cpp app/framework/Theme.cpp app/icons/boost.svg app/report/ReportCenter.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/boost-button.mitcad-ui tools/ui-scripts/boost-button.json
# The Boost development button (mitcad#1990) in the build VM, as
# tools/ui-boost-button-test.sh on Linux: the shared steps
# tools/ui-scripts/boost-button.mitcad-ui on a box
# (tools/ui-scripts/boost-button.json), in a 1024 x 768 window, in the
# Floating layout (the top bar's part titleBoost, left of the indicators)
# and in the Docked one (the tabs' row's right end), each with a fresh
# profile: the button shown, a click logs the Ko-fi link (the tests' URL
# log, MITCAD_TEST_LOG_URLS: no browser), its x hides it and saves it,
# Preferences > Display > Show the Boost development button brings it
# back. The pictures are kept in -Out's pictures folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-boost-button-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-boost-button'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$pictures = Join-Path $script:UiOut 'pictures'
New-Item -ItemType Directory -Force $pictures | Out-Null
$design = Join-Path $work 'boost.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/boost-button.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
# The link is logged, never opened in a browser.
$env:MITCAD_TEST_LOG_URLS = '1'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/boost-button.mitcad-ui'
foreach ($layout in @('floating', 'docked')) {
  Write-Host "=== $layout"
  # A fresh profile for each layout.
  Set-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[shortcuts]')
  $env:MITCAD_TEST_WORK = Join-Path $work $layout
  New-Item -ItemType Directory -Force $env:MITCAD_TEST_WORK | Out-Null
  Ui-StartApp "boost-button-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "boost button driver timed out ($layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "boost button driver failed ($layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'Boost button shown' "${layout}: the button shown"
  Ui-ExpectLog 'Boost link opened: https://ko-fi.com/mitcad' "${layout}: a click opens the Ko-fi page"
  Ui-ExpectLog 'Open URL (test, not opened): https://ko-fi.com/mitcad' "${layout}: only logged in the test"
  Ui-ExpectLog 'Boost button hidden' "${layout}: the x hides it"
  Ui-ExpectLog 'Preferences: Boost development button shown' "${layout}: Preferences shows it again"
  Ui-ExpectLog 'TestDriver: done' "${layout}: the Boost development button"
  # In the top bar, left of the project indicator, in both layouts
  # (mitcad#2082).
  $cards = Ui-LastMatch '^((Floating cards|Docked top bar): .*)$'
  $boost = @(($cards -split '; ') | Where-Object { $_ -match '^titleBoost ' })
  $indicator = @(($cards -split '; ') | Where-Object { $_ -match '^titleIndicator ' })
  if ($boost.Count -eq 0 -or $indicator.Count -eq 0) { Ui-Fail "no titleBoost or titleIndicator in the top bar: '$cards'" }
  $boostX = [int]((($boost[0] -split ' ')[1] -split ',')[0])
  $indicatorX = [int]((($indicator[0] -split ' ')[1] -split ',')[0])
  if ($boostX -ge $indicatorX) { Ui-Fail "the Boost button is not left of the project indicator: '$cards'" }
  Write-Host "ok   ${layout}: the Boost button in the top bar, left of the project indicator"
  $picture = Join-Path $env:MITCAD_TEST_WORK 'boost-button.png'
  if (-not (Test-Path $picture) -or (Get-Item $picture).Length -eq 0) { Ui-Fail "no picture boost-button.png ($layout)" }
  Copy-Item $picture (Join-Path $pictures "boost-button-$layout.png")
}
Write-Host "ok   the pictures in $pictures"
Ui-StopApp
Write-Host 'PASS: the Boost development button opens its page, hides by its x and comes back from Preferences'
