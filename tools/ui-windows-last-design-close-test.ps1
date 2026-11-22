# SPDX-License-Identifier: MIT
# check-all sources: app/WindowManager.cpp app/DocumentFrame.cpp app/MainWindowStart.cpp app/files/MainWindowFiles.cpp app/framework/AppSettings.cpp app/view/ViewController.cpp app/framework/TestDriverDocuments.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/last-design-close.mitcad-ui tools/ui-scripts/last-design-close-restart.mitcad-ui tools/ui-scripts/last-design-close-tab.mitcad-ui tools/ui-scripts/panel-message-top.json
# Closing the last design (mitcad#2142) in the build VM, as
# tools/ui-last-design-close-test.sh on Linux, on a cube that mitcad-cli
# makes: the shared steps tools/ui-scripts/last-design-close.mitcad-ui
# (Preferences > General > Quit MitCad when the last design is closed off,
# the default: File > Close of the only design shows the Start page and the
# process runs on; Open and New Design start from there; the preference
# turned on), last-design-close-restart.mitcad-ui after a restart (still
# on; File > Close of the last design quits, with the session log's end
# line) and last-design-close-tab.mitcad-ui in the Docked and the Floating
# layout (the only design's tab close button: the Start page with the
# preference off, quitting with it on).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-last-design-close-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-last-design-close'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'part.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
Remove-Item Env:MITCAD_SESSION_LOG -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_DEBUG_LOG -ErrorAction SilentlyContinue

function File-Text([string]$Path) {
  $stream = [System.IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
  try { $reader = New-Object System.IO.StreamReader $stream; return $reader.ReadToEnd() } finally { $stream.Dispose() }
}

# One of the shared scripts, the app started on the design in that layout,
# with the settings the runs share and a session log folder of its own.
# Ends 'done': the driver runs to its end; 'quit': MitCad quits before it.
function Run-Script([string]$Name, [string]$Chrome, [string]$Ends) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  $logs = Join-Path $script:UiOut "logs-$Name-$Chrome"
  Remove-Item -Recurse -Force $logs -ErrorAction SilentlyContinue
  $env:MITCAD_DEBUG_LOG_DIR = $logs
  Ui-StartApp "$Name-$Chrome" @('--ui=qt', '--no-mcp', "--chrome=$Chrome", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "the driver timed out ($Name, $Chrome)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "MitCad failed ($Name, $Chrome, exit code $exitCode)"
  }
  $text = "$(Ui-LogText)"
  if ($text -match '(?m)^TestDriver: FAIL') { Ui-Fail "a failed driver step ($Name, $Chrome)" }
  if ($Ends -eq 'done') {
    Ui-ExpectLog 'TestDriver: done' "the shared steps ran to their end ($Name, $Chrome)"
    Ui-ExpectLog 'Last design closed: Start page' "the last design closed to the Start page ($Name, $Chrome)"
    if ($text.Contains('Document windows closed: empty process registry')) {
      Ui-Fail "MitCad quit after the last design closed ($Name, $Chrome)"
    }
    Write-Host "ok   MitCad ran on with no design open ($Name, $Chrome)"
  } else {
    if ($text.Contains('TestDriver: done')) { Ui-Fail "the driver reached its end: MitCad did not quit ($Name, $Chrome)" }
    Ui-ExpectLog 'Last design closed: quitting' "the last design closed with the preference on ($Name, $Chrome)"
    Ui-ExpectLog 'Document windows closed: empty process registry' "MitCad quit ($Name, $Chrome)"
    $sessions = @(Get-ChildItem -Path $logs -Filter 'session-*.log' -ErrorAction SilentlyContinue)
    if ($sessions.Count -lt 1) { Ui-Fail "no session log in $logs ($Name, $Chrome)" }
    $last = @((File-Text $sessions[0].FullName) -split "`r?`n" | Where-Object { $_ }) | Select-Object -Last 1
    if (-not $last.StartsWith('--- The session ended: ')) { Ui-Fail "the session log's last line: '$last' ($Name, $Chrome)" }
    Write-Host "ok   the session ended cleanly ($Name, $Chrome)"
  }
  Ui-StopApp
}

Run-Script 'last-design-close' 'floating' 'done'
Run-Script 'last-design-close-restart' 'floating' 'quit'
Run-Script 'last-design-close-tab' 'docked' 'quit'
Run-Script 'last-design-close-tab' 'floating' 'quit'
Remove-Item Env:MITCAD_DEBUG_LOG_DIR -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Write-Host 'PASS: the last design closes to the Start page, or quits MitCad with the preference on'
