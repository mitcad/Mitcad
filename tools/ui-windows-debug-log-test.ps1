# SPDX-License-Identifier: MIT
# check-all sources: app/framework/DebugLog.cpp app/framework/DebugLog.hpp app/mcp/LogRing.cpp app/mcp/LogRing.hpp
# check-all sources: app/MainWindowDebugLog.cpp app/main.cpp app/view/ViewController.cpp app/files/ProjectIndicator.cpp
# check-all sources: app/framework/WaitCursor.cpp app/framework/WaitCursor.hpp app/MainWindowJobs.cpp
# Input-driven Windows UI test of the debug log in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. --debug-log <file>: the file begins with its header (the version, the
#      build, the system, the graphics, the command line, the design, the
#      switches) and the start-up's lines from before it started.
#   2. Create Sketch by its default shortcut Alt+S (mitcad#267; no default
#      shortcut in conflict): its own log lines in the file, in the file's
#      format (the time in ms, the level, the text).
#   3. Help > Open Log Folder: the file's folder (logged, not opened).
#   4. Preferences > Diagnostics, without a restart: the switch off stops
#      the file at once, on again starts a new file in the log folder with
#      the lines kept from before; the setting is saved, and the next start
#      writes a log by it until the switch is turned off again (mitcad#269).
#   5. The wait cursor (mitcad#268), in the debug log too: a wait nested in
#      a wait (a test hook) with its progress dialog, every "on" with its
#      "off", none left once idle; a cursor left on (a test hook) is
#      cleared once idle, with a warning.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-debug-log-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-debug-log'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# No shortcut for Create Sketch: its default Alt+S starts it. Popups close
# at once in session 0, so the Diagnostics page and Open Log Folder get
# keys of the test's own.
Ui-Init $App $Out $QtBin @{ 'tools.preferences.diagnostics' = 'F8'; 'help.open_log_folder' = 'F9';
  'test.nested_wait' = 'F11'; 'test.leave_wait_cursor' = 'F12' } -StepPause 0

$work = Join-Path $script:UiOut 'debug-log'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$file = Join-Path $work 'run.log'
# The log folder of the test's own; addresses logged, not opened.
$folder = Join-Path $work 'logs'
$env:MITCAD_DEBUG_LOG_DIR = $folder
$env:MITCAD_TEST_LOG_URLS = '1'
# The wait cursor's test hooks (mitcad#268): Nested Wait (Test), Leave Wait
# Cursor On (Test).
$env:MITCAD_TEST_WAIT_CURSOR = '1'
Remove-Item Env:MITCAD_DEBUG_LOG -ErrorAction SilentlyContinue
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'

# A line of the file: "2026-10-14 09:30:12.345 debug    text".
$time = '^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} '

function File-Lines([string]$Path) {
  if (-not (Test-Path $Path)) { return @() }
  $stream = [System.IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
  try { $reader = New-Object System.IO.StreamReader $stream; $text = $reader.ReadToEnd() } finally { $stream.Dispose() }
  return $text -split "`r?`n"
}

# File-Has "regex" "description" [file]: a matching line, within 3 s (the
# file is written at least once a second).
function File-Has([string]$Pattern, [string]$Description, [string]$Path = $file) {
  for ($i = 0; $i -lt 15; $i++) {
    if (@(File-Lines $Path | Where-Object { $_ -cmatch $Pattern }).Count -gt 0) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: no line matching '$Pattern' in $Path"
}

# Expect-Graphics file "description": the header's graphics are the renderer
# the 3D view logged ("OpenGL renderer: <name>"; the header has
# "Graphics: <name>, OpenGL <version>"), also when the log started before
# the view (the file waits for them).
function Expect-Graphics([string]$Path, [string]$Description) {
  $line = $null
  $renderer = $null
  for ($i = 0; $i -lt 25; $i++) {
    $renderer = Ui-LastMatch '^OpenGL renderer: (.*)$'
    $line = [string](File-Lines $Path | Where-Object { $_.StartsWith('Graphics: ') } | Select-Object -First 1)
    if ($renderer -and $line.StartsWith("Graphics: $renderer, OpenGL ")) { Write-Host "ok   ${Description}: $($line.TrimEnd())"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: '$line', the view's renderer is '$renderer'"
}

function File-Count([string]$Text, [string]$Path) {
  return @(File-Lines $Path | Where-Object { $_.Contains($Text) }).Count
}

# Preferences-DebugLog "description": the Diagnostics page (F8), Write a
# debug log toggled (Alt+W), OK.
function Preferences-DebugLog([string]$Description) {
  Ui-Step 'Preferences: Diagnostics (F8)'   { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: Diagnostics, ' 'Preferences on its Diagnostics page'
  Ui-FocusDialog '^Preferences$'
  Ui-Step "$Description (Alt+W)"            { Ui-Key 'alt+w' }
  # The check box toggles a moment after its key (animateClick), as the
  # Linux test waits; nothing is logged for it, and Enter clicks OK at once.
  Start-Sleep -Milliseconds 300
  Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
  Ui-FocusMain
}

# Wait-CursorBalanced "description": every "Wait cursor on: <job> (depth N)"
# has its "Wait cursor off: <job> (depth N)" after it, and none was left on.
function Wait-CursorBalanced([string]$Description) {
  $open = @{}
  foreach ($line in Ui-LogLines) {
    if ($line -cmatch '^Wait cursor on: (.*)$') {
      $open[$Matches[1]] = [int]$open[$Matches[1]] + 1
    } elseif ($line -cmatch '^Wait cursor off: (.*)$') {
      $open[$Matches[1]] = [int]$open[$Matches[1]] - 1
      if ($open[$Matches[1]] -lt 0) { Ui-Fail "${Description}: off before on: $line" }
    }
  }
  if ($open.Count -eq 0) { Ui-Fail "${Description}: no wait cursor logged" }
  foreach ($key in @($open.Keys)) {
    if ($open[$key] -ne 0) { Ui-Fail "${Description}: $($open[$key]) unbalanced: $key" }
  }
  $left = @(Ui-LogLines | Where-Object { $_.StartsWith('Wait cursor left on after') })
  if ($left.Count -gt 0) { Ui-Fail "${Description}: $($left[0])" }
  Write-Host "ok   ${Description}: $($open.Count) wait cursors, each restored once"
}

function Indicator-Line {
  return [string](Ui-LogLines | Where-Object { $_.StartsWith('Project indicator: ') } | Select-Object -Last 1)
}

Write-Host '--- --debug-log <file>: the header and the start-up'
Ui-StartApp 'debug-log' @('--debug-log', $file)
Ui-ExpectLog "Debug log on: $file (--debug-log)" 'the debug log started'
File-Has '^MitCad debug log$' 'the header, once the view knows its graphics'
$first = @(File-Lines $file)[0]
if ($first -ne 'MitCad debug log') { Ui-Fail "the file does not begin with its header: '$first'" }
Write-Host 'ok   the file begins with its header'
File-Has '^Version: MitCad \d' 'the version'
File-Has '^Build: (release|debug), ' 'the build'
File-Has '^System: ' 'the system'
Expect-Graphics $file 'the GPU and renderer in the header of a log started before the view'
File-Has '^Command line: .*--debug-log .*run\.log' 'the command line'
File-Has '^Design: none' 'no design at the start'
File-Has '^Switches: debug log \(--debug-log\), .*MITCAD_LOG_PICKS=1' 'the switches'
File-Has '^Started: .*, file 1$' 'the start'
File-Has '^--- The newest \d+ lines from before the debug log started:$' 'the lines from before'
File-Has "${time}info +Style: " "the start-up's lines from before it started"
File-Has "${time}info +Debug log on: .*run\.log \(--debug-log\)$" 'its own first line'
File-Has "${time}info +OpenGL renderer: " 'the renderer, once the view started'
if (-not (Indicator-Line).Contains('debug log on')) { Ui-Fail "the project indicator does not say debug log on: $(Indicator-Line)" }
Write-Host 'ok   the project indicator says debug log on'

Write-Host '--- Create Sketch by Alt+S (mitcad#267); its lines in the file'
if ((Ui-LogLines) | Where-Object { $_.Contains('Conflicting saved shortcuts') }) { Ui-Fail 'the default shortcuts conflict' }
Write-Host 'ok   no default shortcut in conflict'
Ui-Step 'create sketch on XY (Alt+S)'     { Ui-CreateSketch 'alt+s' 'xy' }
File-Has "${time}debug +Create Sketch: select a plane$" "Create Sketch's own line"
File-Has "${time}debug +Sketch started on xy, origin " 'the sketch started'
Ui-Step 'finish the sketch (Ctrl+Enter)'  { Ui-Key 'ctrl+Return' }
File-Has "${time}debug +Sketch finished$" 'the sketch finished'

Write-Host '--- The wait cursor (mitcad#268): a wait nested in a wait, with its progress dialog'
# Nested Wait (Test): a job of two seconds; 200 ms into its wait, while that
# holds the cursor, a second wait for it starts inside the first one's loop
# and shows the progress dialog after its own 400 ms.
Ui-Mark
Ui-Step 'nested wait (F11)'                { Ui-Key 'F11' }
Ui-ExpectNew 'Wait cursor on: nested wait test (depth 1)' "the wait's cursor"
Ui-ExpectNew 'Wait cursor on: nested wait test (nested) (depth 2)' "the nested wait's cursor"
Ui-ExpectNew 'Progress dialog shown: nested wait test (nested)' "the nested wait's progress dialog"
Ui-ExpectNew 'Wait cursor off: nested wait test (nested) (depth 2)' 'its cursor gone with the dialog'
Ui-ExpectNew 'Computing nested wait test done in' 'the job done' 10
Ui-ExpectNew 'Wait cursor off: nested wait test (depth 1)' "the first wait's cursor gone at its end"
Ui-ExpectNew 'Wait cursor at idle: none set' 'no wait cursor left once idle'
Ui-FocusMain
Wait-CursorBalanced 'the nested waits'
File-Has "${time}debug +Wait cursor on: nested wait test \(nested\) \(depth 2\)$" 'the nested wait in the debug log'
File-Has "${time}debug +Wait cursor off: nested wait test \(depth 1\)$" 'its end in the debug log'

Write-Host '--- The wait cursor (mitcad#268): one left on is cleared once idle'
Ui-Mark
Ui-Step 'leave a wait cursor on (F12)'     { Ui-Key 'F12' }
Ui-ExpectNew 'Wait cursor left on for a test: test hook' 'an override cursor set and left (the test hook)'
Ui-ExpectNew 'Wait cursor left on after test hook: cleared' 'cleared once idle'
Ui-ExpectNew 'Wait cursor at idle: none set' 'none left'
File-Has "${time}warning +Wait cursor left on after test hook: cleared$" 'the warning in the debug log'

Write-Host '--- Help > Open Log Folder'
Ui-Mark
Ui-Step 'Open Log Folder (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew "Open Log Folder: $work" "the log file's folder"
Ui-ExpectNew 'Open URL (test, not opened): file:///' 'the folder opened'

Write-Host '--- Preferences > Diagnostics: off at once'
Ui-Mark
Preferences-DebugLog 'Write a debug log off'
Ui-ExpectNew 'Preferences: debug log off' 'the switch turned off'
Ui-ExpectNew "Debug log off: $file" 'the log stopped'
$last = @(File-Lines $file | Where-Object { $_ }) | Select-Object -Last 1
if ($last -notmatch "${time}info +Debug log off: .*run\.log$") { Ui-Fail "the file's last line is not its end: '$last'" }
Write-Host "ok   the file's last line says it stopped"
if ((Indicator-Line).Contains('debug log on')) { Ui-Fail 'the project indicator still says debug log on' }
Write-Host 'ok   the project indicator no longer says it'
Ui-Step 'create sketch on XY (Alt+S)'     { Ui-CreateSketch 'alt+s' 'xy' }
if ((File-Count 'Sketch started on xy' $file) -ne 1) { Ui-Fail 'a sketch started after the stop is in the file' }
Write-Host 'ok   nothing after the stop in the file'
Ui-Step 'finish the sketch (Ctrl+Enter)'  { Ui-Key 'ctrl+Return' }

Write-Host '--- On again: a new file in the log folder, without a restart'
Ui-Mark
Preferences-DebugLog 'Write a debug log on'
Ui-ExpectNew 'Preferences: debug log on' 'the switch turned on'
Ui-ExpectNew 'Debug log on: ' 'a new debug log'
$new = Ui-LastMatch '^Debug log on: (.*) \(Preferences\)$'
if (-not $new -or (Split-Path $new) -ne $folder -or (Split-Path $new -Leaf) -notmatch '^mitcad-\d{8}-\d{6}-\d+\.log$') {
  Ui-Fail "the new log is not mitcad-<date>-<time>-<pid>.log in ${folder}: '$new'"
}
Write-Host "ok   in the log folder: $new"
if (@(File-Lines $new)[0] -ne 'MitCad debug log') { Ui-Fail 'the new file does not begin with its header' }
File-Has '^Switches: debug log \(Preferences\)' 'switched on in Preferences' $new
Expect-Graphics $new 'the GPU and renderer in the header'
File-Has '^--- The newest \d+ lines from before the debug log started:$' 'the lines kept from before' $new
if ((File-Count 'Sketch started on xy' $new) -ne 2) { Ui-Fail 'the sketches started before are not in the lines kept from before' }
Write-Host 'ok   the sketch started while it was off is in the new file'
if (-not (Select-String -Path $ini -Pattern '^debugLog=true' -Quiet)) { Ui-Fail 'the switch is not saved' }
Write-Host 'ok   the switch saved'
Ui-Mark
Ui-Step 'Open Log Folder (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew "Open Log Folder: $folder" 'the log folder'
File-Has "${time}info +Open Log Folder: $([regex]::Escape($folder))$" 'a new line in the new file' $new
Ui-StopApp

Write-Host '--- The next start: on by the setting, off again in Preferences'
Ui-StartApp 'debug-log-next'
Ui-ExpectLog 'Debug log on: ' 'the setting starts the log'
$latest = Ui-LastMatch '^Debug log on: (.*) \(Preferences\)$'
if (-not $latest) { Ui-Fail 'not started by the setting' }
if ($latest -eq $new) { Ui-Fail 'the next start wrote into the same file' }
Write-Host "ok   a file of its own: $latest"
Ui-Mark
Preferences-DebugLog 'Write a debug log off'
Ui-ExpectNew "Debug log off: $latest" 'the log stopped'
if (Select-String -Path $ini -Pattern '^debugLog=true' -Quiet) { Ui-Fail 'the setting stayed on' }
Write-Host 'ok   the setting cleared'

Ui-Finish 'UI Windows debug log test'
