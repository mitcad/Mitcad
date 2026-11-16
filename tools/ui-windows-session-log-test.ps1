# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SessionLog.cpp app/framework/HangWatchdog.cpp app/report/ReportCenter.cpp
# check-all sources: tools/ui-scripts/session-log.mitcad-ui tools/ui-scripts/session-log-ended.mitcad-ui
# The session log, the hang marker and the notice of an unexpected end
# (mitcad#1896) in the build VM, as tools/ui-session-log-test.sh on Linux:
# the shared steps tools/ui-scripts/session-log.mitcad-ui (a few commands,
# a test hang of 3 s with MITCAD_HANG_WATCHDOG_MS=1000, killed) and
# session-log-ended.mitcad-ui (the notice, Open Log Folder, a clean end),
# then the files of the test's own log folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-session-log-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-session-log'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$logs = Join-Path $script:UiOut 'session-logs'
Remove-Item -Recurse -Force $logs -ErrorAction SilentlyContinue
$env:MITCAD_DEBUG_LOG_DIR = $logs
$env:MITCAD_TEST_LOG_URLS = '1'
$env:MITCAD_TEST_STEP_MS = '100'
Remove-Item Env:MITCAD_SESSION_LOG -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_DEBUG_LOG -ErrorAction SilentlyContinue

# Run-Script name script "description": MitCad with the script until it ends.
function Run-Script([string]$Name, [string]$Script, [string]$Description) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp')
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${Description}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Description}: the driver failed (exit code $exitCode)"
  }
  $text = "$(Ui-LogText)"
  if (-not ($text.Contains('TestDriver: done') -or $text.Contains('TestDriver: kill') -or $text.Contains('TestDriver: quit ('))) {
    Ui-Fail "${Description}: not to its end"
  }
  Write-Host "ok   $Description"
}

function File-Text([string]$Path) {
  $stream = [System.IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
  try { $reader = New-Object System.IO.StreamReader $stream; return $reader.ReadToEnd() } finally { $stream.Dispose() }
}

function File-Has([string]$Pattern, [string]$Description, [string]$Path) {
  if (-not ((File-Text $Path) -split "`r?`n" | Where-Object { $_ -cmatch $Pattern })) {
    Ui-Fail "${Description}: no line matching '$Pattern' in $Path"
  }
  Write-Host "ok   $Description"
}

Write-Host '--- The first run: its log, a few commands, a hang of 3 s, killed'
$env:MITCAD_HANG_WATCHDOG_MS = '1000'
Remove-Item Env:MITCAD_UNEXPECTED_END_NOTICE -ErrorAction SilentlyContinue
Run-Script 'session-log' 'session-log.mitcad-ui' 'the first run'
Ui-StopApp
$files = @(Get-ChildItem -Path $logs -Filter 'session-*.log')
if ($files.Count -ne 1) { Ui-Fail "not one session log in ${logs}: $($files.Name -join ', ')" }
$first = $files[0].FullName
if (((File-Text $first) -split "`r?`n")[0] -ne 'MitCad session log') { Ui-Fail 'the file does not begin with its header' }
Write-Host "ok   one session log, with its header: $($files[0].Name)"
File-Has 'info +Open Log Folder: ' 'Open Log Folder''s line' $first
File-Has 'Create Sketch: select a plane' 'Create Sketch''s line' $first
File-Has 'warning +\[hang watchdog\] UI not responding for \d+ s \(last action: test step freeze-ui 3000 ms' `
  'the hang''s line, from the watchdog''s thread' $first
File-Has 'info +UI responding again after \d+\.\d s$' 'how long it lasted' $first
if ((File-Text $first) -match '(?m)^--- The session ended') { Ui-Fail 'a killed session has a clean end' }
$lock = [System.IO.Path]::ChangeExtension($first, '.lock')
if (-not (Test-Path $lock)) { Ui-Fail "the killed session's lock is gone: $lock" }
Write-Host 'ok   no clean end; its lock left'

Write-Host '--- The second run: the notice of the unexpected end, a clean end'
Remove-Item Env:MITCAD_HANG_WATCHDOG_MS -ErrorAction SilentlyContinue
$env:MITCAD_UNEXPECTED_END_NOTICE = '1'
Run-Script 'session-log-ended' 'session-log-ended.mitcad-ui' 'the second run'
Ui-ExpectLog "Previous session ended unexpectedly: $([System.IO.Path]::GetFileNameWithoutExtension($first))" `
  'the first run named as ended unexpectedly'
Ui-StopApp
$second = @(Get-ChildItem -Path $logs -Filter 'session-*.log' | Where-Object { $_.FullName -ne $first })
if ($second.Count -ne 1) { Ui-Fail 'no second session log' }
$last = @((File-Text $second[0].FullName) -split "`r?`n" | Where-Object { $_ }) | Select-Object -Last 1
if (-not $last.StartsWith('--- The session ended: ')) { Ui-Fail "the second run's last line: '$last'" }
Write-Host "ok   the second run's clean end: $last"
$locks = @(Get-ChildItem -Path $logs -Filter 'session-*.lock')
if ($locks.Count -ne 0) { Ui-Fail "locks left: $($locks.Name -join ', ')" }
Write-Host 'ok   no lock left'
Remove-Item Env:MITCAD_UNEXPECTED_END_NOTICE -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue

Write-Host 'PASS: UI Windows session log, hang marker and unexpected end'
