# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/RecentDesigns.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp tools/cli
# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125) in the build VM, as
# tools/ui-start-page-test.sh on Linux: the shared steps
# tools/ui-scripts/start-page*.mitcad-ui on three designs that mitcad-cli
# makes (tools/ui-scripts/start-page.json), in three starts of MitCad:
#   1. start-page-fixture: the three opened, changed and saved (each save
#      writes its preview), no Start page at launch;
#   2. start-page with MITCAD_START_PAGE=1: the Start tab at launch with the
#      three designs and their previews, the star and Pinned, the search,
#      Show start page at launch turned off, a card opens its design, New
#      Design gives an empty design in the default units (inches);
#   3. start-page-off: MitCad starts on an empty design, as before.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-start-page-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-start-page'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
foreach ($name in 'a', 'b', 'c') {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/start-page.json') --save (Join-Path $work "start-$name.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "start-$name.mitcad was not made" }
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# One start of MitCad with the shared steps; the app ends with them.
function Run-Script([string]$Name, [string]$Script, [string[]]$Arguments = @()) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name (@('--ui=qt', '--no-mcp') + $Arguments)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: the steps"
}

$env:MITCAD_START_PAGE = '0'
Run-Script 'start-page-fixture' 'start-page-fixture.mitcad-ui' @('--open', (Join-Path $work 'start-a.mitcad'))
foreach ($name in 'a', 'b', 'c') {
  Ui-ExpectLog "Design preview saved: start-$name.mitcad (" "start-$name.mitcad saved with its preview"
}

$env:MITCAD_START_PAGE = '1'
Run-Script 'start-page' 'start-page.mitcad-ui'
Ui-ExpectLog 'Start page at launch: shown' 'the Start tab at launch'
Ui-ExpectLog 'Start page recent designs (all): start-c.mitcad (preview) | start-b.mitcad (preview) | start-a.mitcad (preview)' `
  'the three designs with their previews, newest first'
Ui-ExpectLog 'Start page pinned: start-b.mitcad' 'the star pins a design'
Ui-ExpectLog 'Start page recent designs (pinned): start-b.mitcad (preview)' 'Pinned shows the pinned design alone'
Ui-ExpectLog 'Start page recent designs (all, "start-a"): start-a.mitcad (preview)' 'the search finds a design'
Ui-ExpectLog 'Start page chosen: open start-c.mitcad' 'a card opens its design'
Ui-ExpectLog 'Start page at launch: off' 'Show start page at launch turned off'
Ui-ExpectLog 'New design in in' 'New Design: an empty design in the default units'

Run-Script 'start-page-off' 'start-page-off.mitcad-ui'
Ui-ExpectLog 'Start page at launch: off (Show start page at launch)' 'off: no Start page at launch'
Remove-Item Env:MITCAD_START_PAGE -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-StopApp
Write-Host 'PASS: the Start tab: recent designs with previews, pins, search, New Design and the launch choice'
