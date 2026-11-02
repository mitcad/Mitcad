# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowPrint.cpp app/files/Print3d.cpp app/files/SlicerProcesses.cpp
# check-all sources: app/framework/CommandSession.cpp tools/cli
# Input-driven Windows UI test of Send to Slicer's Use the open slicer
# window (mitcad#1162) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), as tools/ui-slicer-window-test.sh does on Linux,
# with fake slicers: a small program compiled here (Add-Type) that logs
# each start with the files it was given and stays running as a "window"
# (it shows none) until the test ends it; started with --single-instance
# while another of its windows runs, it hands the files over (logs
# "forwarded") and exits, as PrusaSlicer does.
#   1. The box on (the default): two sends of one design start one
#      window, with --single-instance, and the second hands it the file;
#      one window runs.
#   2. Another design (Save As) starts a window of its own, without the
#      switch (another window of the program runs).
#   3. The box off (clicked, remembered): every send starts a new window.
#   4. After a restart the box is still off; turned on again, a send of
#      a design whose window this session did not start starts one.
#   5. A program without a hand-over ("Fake slicer"): the second send
#      starts nothing, reuses the window (it has none to bring to the
#      front) and says that the file was updated.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-slicer-window-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-slicer-window'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.print3d' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The app's temporary folder of the test's own.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp

# The fake slicers: a "window" logs "<pid> window: <arguments>", a start
# that hands over "<pid> forwarded to <pid>: <arguments>"; each ends when
# the stop file appears (or after ten minutes, should the test fail).
$fake = Join-Path $script:UiOut 'fake'
if (Test-Path $fake) {
  New-Item -ItemType File -Force (Join-Path $fake 'stop') | Out-Null
  Start-Sleep -Milliseconds 500
}
Remove-Item -Recurse -Force $fake -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $fake 'windows') | Out-Null
$startsLog = Join-Path $fake 'starts.log'
Set-Content -Encoding ascii $startsLog @()
$fakeSlicer = Join-Path $fake 'fake-slicer.exe'
$otherSlicer = Join-Path $fake 'other-slicer.exe'
Add-Type -OutputType WindowsApplication -OutputAssembly $fakeSlicer -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading;

public static class FakeSlicer {
  static void Log(string dir, string line) {
    for (int i = 0; i < 50; i++) {
      try {
        File.AppendAllText(Path.Combine(dir, "starts.log"), line + "\r\n");
        return;
      } catch (IOException) {
        Thread.Sleep(100);
      }
    }
  }

  public static int Main(string[] args) {
    Process self = Process.GetCurrentProcess();
    string exe = self.MainModule.FileName;
    string dir = Path.GetDirectoryName(exe);
    string windows = Path.Combine(dir, "windows");
    string text = string.Join(" ", args);
    if (Array.IndexOf(args, "--single-instance") >= 0) {
      foreach (Process other in Process.GetProcessesByName(Path.GetFileNameWithoutExtension(exe))) {
        if (other.Id != self.Id && File.Exists(Path.Combine(windows, other.Id.ToString()))) {
          Log(dir, self.Id + " forwarded to " + other.Id + ": " + text);
          return 0;
        }
      }
    }
    string mine = Path.Combine(windows, self.Id.ToString());
    File.WriteAllText(mine, "");
    Log(dir, self.Id + " window: " + text);
    for (int i = 0; i < 3000 && !File.Exists(Path.Combine(dir, "stop")); i++) {
      Thread.Sleep(200);
    }
    File.Delete(mine);
    return 0;
  }
}
'@
if (-not (Test-Path $fakeSlicer)) { Ui-Fail 'the fake slicer was not compiled' }
Copy-Item $fakeSlicer $otherSlicer

# Set-PrintSettings name program [reuse]: the slicer in the settings, in
# place of the [print] group the app saved.
function Set-PrintSettings([string]$Name, [string]$Program, [string]$Reuse = '') {
  $ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
  $kept = @()
  $skip = $false
  foreach ($line in @(Get-Content $ini)) {
    if ($line -match '^\[') { $skip = ($line -eq '[print]') }
    if (-not $skip) { $kept += $line }
  }
  $kept += @('[print]', "slicerName=$Name", "slicerProgram=$($Program -replace '\\', '/')", 'format=3mf')
  if ($Reuse) { $kept += "reuseWindow=$Reuse" }
  Set-Content -Encoding ascii $ini $kept
}
Set-PrintSettings 'PrusaSlicer (fake)' $fakeSlicer

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'plate.mitcad'
$other = Join-Path $work 'other.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Fake-Windows: how many fake slicer windows run.
function Fake-Windows {
  $count = 0
  foreach ($file in @(Get-ChildItem (Join-Path $fake 'windows') -File -ErrorAction SilentlyContinue)) {
    if (Get-Process -Id ([int]$file.Name) -ErrorAction SilentlyContinue) { $count++ }
  }
  return $count
}
function Expect-Windows([int]$Count, [string]$Description) {
  $now = Fake-Windows
  if ($now -ne $Count) { Ui-Fail "${Description}: $now slicer windows run, expected $Count" }
  Write-Host "ok   $Description"
}
# Started-Pid: the pid of the last "Send to Slicer: started ..." line.
function Started-Pid { return Ui-LastMatch 'Send to Slicer: started .* \(pid (\d+)\)' }
# Starts: the fake slicers' log lines.
function Starts { return @(Get-Content $startsLog -ErrorAction SilentlyContinue) }
# Expect-Start "regex" "description": waits for a line of the fakes' log.
function Expect-Start([string]$Pattern, [string]$Description) {
  for ($i = 0; $i -lt 100; $i++) {
    if (Starts | Where-Object { $_ -match $Pattern }) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: no '$Pattern' in the fake slicer's log: $((Starts) -join ' / ')"
}
# Send: Send to Slicer (F8), then Send (Enter).
function Send {
  Ui-Mark
  Ui-Step 'Send to Slicer (F8)'          { Ui-Key 'F8' }
  Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
  Ui-Step 'send (Enter)'                 { Ui-Key 'Return' }
}

# The fake slicers end with the test, also when it fails (Ui-Fail exits).
try {
Ui-StartApp 'slicer-window' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'

Write-Host '--- 1. The box on: one window, the second file handed over'
Send
Ui-ExpectNew '3D Print open slicer window: on' 'the box is on by default'
Ui-ExpectNew 'Send to Slicer: started PrusaSlicer (fake) (pid ' 'a slicer window started' 30
$first = Started-Pid
Expect-Start "^$first window: --single-instance .*\\plate\.3mf$" 'the first window took --single-instance and the file'
Send
Ui-ExpectNew "Send to Slicer: reused PrusaSlicer (fake) window of plate (pid $first): the file handed over" `
  'the second send reused the window' 30
Ui-ExpectNew '3D Print last line: Sent 1 body to the PrusaSlicer (fake) window of plate.' 'the last line says so'
Expect-Start "^\d+ forwarded to ${first}: --single-instance .*\\plate\.3mf$" 'the window was handed the file'
Expect-Windows 1 'one slicer window runs'

Write-Host '--- 2. Another design starts its own window'
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'         { Ui-Key 'ctrl+shift+s' }
Ui-FocusDialog '^Save As$'
Ui-Step 'the path'                       { Ui-Key 'ctrl+a'; Ui-Type $other; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Saved ' 'saved as another design'
Send
Ui-ExpectNew 'Send to Slicer: started PrusaSlicer (fake) (pid ' 'the other design''s window started' 30
$second = Started-Pid
if ($second -eq $first) { Ui-Fail 'the other design reused the first design''s window' }
Expect-Start "^$second window: .*\\other\.3mf$" 'the other design''s window took its file'
if (Starts | Where-Object { $_ -match "^$second window: --single-instance" }) {
  Ui-Fail 'the other design''s window took --single-instance while another window runs'
}
Write-Host 'ok   without --single-instance: another window of the program runs'
Expect-Windows 2 'two slicer windows run'

Write-Host '--- 3. The box off: a new window every time'
Ui-Mark
Ui-Step 'Send to Slicer (F8)'            { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-Step 'Use the open slicer window'     { Ui-ClickLogged 'Panel Send to Slicer input reuse' }
Ui-ExpectNew '3D Print open slicer window: off' 'the box turned off'
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Send to Slicer: started PrusaSlicer (fake) (pid ' 'a new window although one is open' 30
$third = Started-Pid
Expect-Start "^$third window: .*\\other\.3mf$" 'the new window took the file'
Send
Ui-ExpectNew '3D Print open slicer window: off' 'the box stays off'
Ui-ExpectNew 'Send to Slicer: started PrusaSlicer (fake) (pid ' 'another new window' 30
Expect-Start "^$(Started-Pid) window: " 'the fourth window started'
Expect-Windows 4 'four slicer windows run'
Ui-StopApp

Write-Host '--- 4. Remembered after a restart; a window of another session is not reused'
$ini = Get-Content (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini')
if (-not ($ini -contains 'reuseWindow=false')) { Ui-Fail "the box off is not remembered: $($ini -join ' / ')" }
Write-Host 'ok   the box off in the settings'
Ui-StartApp 'slicer-window-again' @('--open', $other)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'
Ui-Mark
Ui-Step 'Send to Slicer (F8)'            { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-ExpectNew '3D Print open slicer window: off' 'the box still off'
Ui-Step 'Use the open slicer window'     { Ui-ClickLogged 'Panel Send to Slicer input reuse' }
Ui-ExpectNew '3D Print open slicer window: on' 'the box turned on'
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Send to Slicer: started PrusaSlicer (fake) (pid ' 'the windows the last session started are not reused' 30
Expect-Start "^$(Started-Pid) window: " 'the fifth window started'
Expect-Windows 5 'five slicer windows run'
Ui-StopApp

Write-Host '--- 5. A program without a hand-over: the window reused, the file updated'
New-Item -ItemType File -Force (Join-Path $fake 'stop') | Out-Null
for ($i = 0; $i -lt 50 -and (Fake-Windows) -gt 0; $i++) { Start-Sleep -Milliseconds 200 }
Expect-Windows 0 'the windows ended'
Remove-Item -Force (Join-Path $fake 'stop')
Set-PrintSettings 'Fake slicer' $otherSlicer 'true'
Ui-StartApp 'slicer-window-other' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'
Send
Ui-ExpectNew 'Send to Slicer: started Fake slicer (pid ' 'a window started' 30
$fourth = Started-Pid
Expect-Start "^$fourth window: .*\\plate\.3mf$" 'the window took the file'
if (Starts | Where-Object { $_ -match "^$fourth window: --single-instance" }) {
  Ui-Fail 'a program without a hand-over took --single-instance'
}
Send
Ui-ExpectNew "Send to Slicer: reused Fake slicer window of plate (pid $fourth): " 'the window reused' 30
Ui-ExpectNew '3D Print last line: Fake slicer is already open with plate: the file was updated' `
  'the last line says that the file was updated'
Start-Sleep -Milliseconds 1000
$plateWindows = @(Starts | Where-Object { $_ -match ' window: .*\\plate\.3mf$' }).Count
if ($plateWindows -ne 2) { Ui-Fail "a send started another window: $((Starts) -join ' / ')" }
if (Starts | Where-Object { $_ -match "forwarded to ${fourth}:" }) { Ui-Fail 'the program was started again to hand the file over' }
Write-Host 'ok   nothing started'
Expect-Windows 1 'one slicer window runs'
if (-not (Test-Path (Join-Path $printTemp 'mitcad-print\plate\plate.3mf'))) { Ui-Fail 'the file was not written again' }
Write-Host 'ok   the file rewritten in place'
New-Item -ItemType File -Force (Join-Path $fake 'stop') | Out-Null

Ui-Finish 'UI Windows slicer window test'
} finally {
  New-Item -ItemType File -Force (Join-Path $fake 'stop') | Out-Null
}
