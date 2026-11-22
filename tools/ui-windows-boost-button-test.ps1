# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BoostButton.cpp app/framework/BoostButton.hpp app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/framework/Ribbon.cpp app/MainWindow.cpp app/view/ViewController.cpp app/framework/Theme.cpp app/icons/boost.svg app/report/ReportCenter.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/boost-button.mitcad-ui tools/ui-scripts/boost-button.json app/framework/DiscordButton.cpp app/framework/DiscordButton.hpp app/icons/community.svg app/resources/layout/default.json third_party/brand-marks/discord-mark.svg app/view/PreferencesRestore.cpp
# The Boost development button (mitcad#1990) in the build VM, as
# tools/ui-boost-button-test.sh on Linux: the shared steps
# tools/ui-scripts/boost-button.mitcad-ui on a box
# (tools/ui-scripts/boost-button.json), in a 1024 x 768 window, in the
# Floating layout (the top bar's part titleBoost, left of the indicators)
# and in the Docked one (the tabs' row's right end), each with a fresh
# profile: the button shown, a click logs the Ko-fi link (the tests' URL
# log, MITCAD_TEST_LOG_URLS: no browser), its x hides it and saves it,
# Preferences > Display > Show the Boost development button brings it
# back; the Discord button right of it (titleDiscord, mitcad#2123): a
# click logs MitCad's Discord link, it stays while Boost is hidden, Help >
# MitCad on Discord logs the same link; its own x hides only it and saves
# it, Preferences > Display > Show the Discord button brings it back
# (mitcad#2159). The pictures (also both pills in each theme,
# boost-button-<layout>-dark.png and -light.png) are kept in -Out's
# pictures folder.
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
  Ui-ExpectLog 'Discord link opened: https://mitcad.org/discord' "${layout}: the Discord button and Help open the Discord link"
  Ui-ExpectLog 'Open URL (test, not opened): https://mitcad.org/discord' "${layout}: the Discord link only logged in the test"
  $discordOpened = @(Ui-LogLines | Where-Object { $_ -eq 'Discord link opened: https://mitcad.org/discord' })
  if ($discordOpened.Count -lt 3) {
    Ui-Fail "${layout}: the Discord link not opened three times (the button, the button without Boost, Help)"
  }
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
  # The Discord button right of it, left of the indicator (mitcad#2123),
  # from the parts logged last while Boost showed.
  $shown = Ui-LastMatch '^((Floating cards|Docked top bar): .*; titleBoost .*)$'
  $partX = {
    param([string]$Line, [string]$Name)
    $entry = @(($Line -split '; ') | Where-Object { $_ -match "^$Name " })
    if ($entry.Count -eq 0) { return $null }
    return [int]((($entry[0] -split ' ')[1] -split ',')[0])
  }
  $boostX = & $partX $shown 'titleBoost'
  $discordX = & $partX $shown 'titleDiscord'
  $indicatorX = & $partX $shown 'titleIndicator'
  if ($null -eq $boostX -or $null -eq $discordX -or $null -eq $indicatorX) {
    Ui-Fail "no titleBoost, titleDiscord or titleIndicator in the top bar: '$shown'"
  }
  if ($boostX -ge $discordX -or $discordX -ge $indicatorX) {
    Ui-Fail "the Discord button is not between the Boost button and the project indicator: '$shown'"
  }
  # Still there while Boost is hidden: the parts logged last between its x
  # and Preferences showing it again.
  $hidden = $null
  $on = $false
  foreach ($line in @(Ui-LogLines)) {
    if ($line -eq 'Boost button hidden') { $on = $true }
    elseif ($line -eq 'Preferences: Boost development button shown') { $on = $false }
    elseif ($on -and $line -match '^(Floating cards|Docked top bar): ') { $hidden = $line }
  }
  if ($null -eq $hidden) { Ui-Fail "${layout}: no top bar parts logged while Boost was hidden" }
  if ($hidden -match '; titleBoost ') { Ui-Fail "${layout}: titleBoost in the top bar after its x: '$hidden'" }
  if ($hidden -notmatch '; titleDiscord ') { Ui-Fail "${layout}: no titleDiscord in the top bar while Boost is hidden: '$hidden'" }
  Write-Host "ok   ${layout}: the Discord button right of the Boost button, there without it too"
  # Its own x (mitcad#2159): hidden and saved, Boost left; the parts logged
  # last between it and Preferences showing it again.
  Ui-ExpectLog 'Discord button hidden' "${layout}: the Discord x hides it"
  Ui-ExpectLog 'Preferences: Discord button shown' "${layout}: Preferences shows the Discord button again"
  $gone = $null
  $back = $null
  $on = $false
  $after = $false
  foreach ($line in @(Ui-LogLines)) {
    if ($line -eq 'Discord button hidden') { $on = $true }
    elseif ($line -eq 'Preferences: Discord button shown') { $on = $false; $after = $true }
    elseif ($line -match '^(Floating cards|Docked top bar): ') {
      if ($on) { $gone = $line }
      if ($after) { $back = $line }
    }
  }
  if ($null -eq $gone) { Ui-Fail "${layout}: no top bar parts logged while the Discord button was hidden" }
  if ($gone -match '; titleDiscord ') { Ui-Fail "${layout}: titleDiscord in the top bar after its x: '$gone'" }
  if ($gone -notmatch '; titleBoost ') { Ui-Fail "${layout}: the Discord x hid the Boost button too: '$gone'" }
  if ($null -eq $back) { Ui-Fail "${layout}: no top bar parts logged after Preferences showed the Discord button" }
  $boostX = & $partX $back 'titleBoost'
  $discordX = & $partX $back 'titleDiscord'
  if ($null -eq $boostX -or $null -eq $discordX -or $boostX -ge $discordX) {
    Ui-Fail "${layout}: the Discord button not back right of the Boost button after Preferences: '$back'"
  }
  Write-Host "ok   ${layout}: the Discord x hides only it, Preferences brings it back"
  foreach ($name in @('boost-button', 'boost-button-dark', 'boost-button-light')) {
    $picture = Join-Path $env:MITCAD_TEST_WORK "$name.png"
    if (-not (Test-Path $picture) -or (Get-Item $picture).Length -eq 0) { Ui-Fail "no picture $name.png ($layout)" }
  }
  # Each theme's own picture (mitcad#2176): the steps checked the Browser's
  # theme before each; the two files differ.
  foreach ($theme in @('dark', 'light')) {
    $checked = @(Ui-LogLines | Where-Object { $_ -match "TestDriver: widget theme modelBrowserTree .*: $theme`$" })
    if ($checked.Count -eq 0) { Ui-Fail "${layout}: the window not checked in the $theme theme before its picture" }
  }
  $darkHash = (Get-FileHash (Join-Path $env:MITCAD_TEST_WORK 'boost-button-dark.png')).Hash
  $lightHash = (Get-FileHash (Join-Path $env:MITCAD_TEST_WORK 'boost-button-light.png')).Hash
  if ($darkHash -eq $lightHash) { Ui-Fail "${layout}: boost-button-dark.png is the same picture as boost-button-light.png" }
  Write-Host "ok   ${layout}: the dark and the light picture differ"
  Copy-Item (Join-Path $env:MITCAD_TEST_WORK 'boost-button.png') (Join-Path $pictures "boost-button-$layout.png")
  Copy-Item (Join-Path $env:MITCAD_TEST_WORK 'boost-button-dark.png') (Join-Path $pictures "boost-button-$layout-dark.png")
  Copy-Item (Join-Path $env:MITCAD_TEST_WORK 'boost-button-light.png') (Join-Path $pictures "boost-button-$layout-light.png")
}
Write-Host "ok   the pictures in $pictures"
Ui-StopApp
Write-Host 'PASS: the Boost development button opens its page, hides by its x and comes back from Preferences'
