# SPDX-License-Identifier: MIT
# Screenshots of the whole main window in the Docked chrome, on the light and
# the dark theme, for comparing the window's look before and after a change
# (mitcad#251). Not a test: it fails only when Mitcad does not get through
# its script. Runs mitcad.exe with Mesa's software OpenGL (MITCAD_MESA_DIR)
# in SSH's session 0, as tools/ui-windows-test.ps1 does, with the in-process
# test driver (MITCAD_TEST_INPUT, app/framework/TestDriver.hpp), which grabs
# the window's widgets with the 3D view composed in (the native title bar is
# not in the image).
#
# For each theme (MITCAD_TEST_COLOR_SCHEME, framework/Diagnostics.hpp) the
# demo block opens with the Projects panel shown; Body1 is clicked in the
# Browser, then Extrude1 in the Timeline, so that the selection shows in the
# Browser and the Timeline:
#   window-<theme>-selected.png   the selection
#   window-<theme>-extrude.png    then Extrude started: the Command panel
# Only steps that every build since the Projects panel has are used, so the
# script takes the same pictures of an older build.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-window-shots.ps1 -App <mitcad.exe> -Out <dir> [-QtBin dir] [-Scale factor]
# Qt's DLLs are not copied next to the app; -QtBin defaults to the Qt of
# the build directory's CMakeCache.txt.
#
# The window is 1280 x 800 (main.cpp), but the screen of SSH's session 0 is
# 1024 x 768, and Windows keeps a window within it (its largest tracking
# size, 1044 x 788): the window came out about 1040 px wide, too narrow for
# the ribbon, which the tool bar then hid behind its extension button. Qt's
# scale factor (QT_SCALE_FACTOR, -Scale, 0.75 by default) makes the screen
# 1365 x 1024 logical pixels, so the window keeps its 1280 x 800 and the
# ribbon shows; the images are 0.75 of that in pixels.
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-window-shots'),
  [string]$QtBin = '',
  [double]$Scale = 0.75)

$ErrorActionPreference = 'Stop'
$App = (Resolve-Path $App).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$failures = 0

if (-not $QtBin) {
  $cache = Join-Path (Split-Path $App) 'CMakeCache.txt'
  $qtDir = if (Test-Path $cache) { Select-String -Path $cache -Pattern '^Qt6_DIR:PATH=(.*)' } else { $null }
  if ($qtDir) { $QtBin = Join-Path $qtDir.Matches[0].Groups[1].Value '..\..\..\bin' }
}
if (-not $QtBin -or -not (Test-Path (Join-Path $QtBin 'Qt6Core.dll'))) {
  Write-Host "FAIL: Qt's bin directory not found; pass -QtBin"
  exit 1
}
$env:Path = (Resolve-Path $QtBin).Path + ';' + $env:Path
if (-not (Test-Path (Join-Path (Split-Path $App) 'opengl32.dll'))) {
  Write-Host "FAIL: no opengl32.dll next to $App. Build Mesa (tools\dev-env\setup-mesa-windows.ps1)"
  Write-Host '      and configure with -DMITCAD_MESA_DIR=C:/dev/mesa/bin.'
  exit 1
}

# Invoke-Mitcad name arguments environment: runs the app until it exits and
# returns its log (Qt messages go to stderr with QT_FORCE_STDERR_LOGGING).
function Invoke-Mitcad([string]$Name, [string[]]$Arguments, [hashtable]$Environment) {
  $info = New-Object System.Diagnostics.ProcessStartInfo $App
  $info.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.EnvironmentVariables['QT_FORCE_STDERR_LOGGING'] = '1'
  # Folders of the run's own, not the user's.
  $info.EnvironmentVariables['MITCAD_AUTOSAVE_DIR'] = Join-Path $Out "$Name-autosave"
  $info.EnvironmentVariables['MITCAD_CRASH_DIR'] = Join-Path $Out "$Name-crashes"
  $info.EnvironmentVariables['MITCAD_RESULT_STORE'] = 'off'
  $info.EnvironmentVariables['MITCAD_NO_UPDATE_CHECK'] = '1'
  foreach ($variable in $Environment.GetEnumerator()) {
    $info.EnvironmentVariables[$variable.Key] = $variable.Value
  }
  $process = [System.Diagnostics.Process]::Start($info)
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit(180000)) {
    $process.Kill()
    Write-Host "FAIL: ${Name}: Mitcad did not exit within 180 s"
    $script:failures++
  }
  $process.WaitForExit()
  $log = $stdout.Result + $stderr.Result
  Set-Content -Encoding utf8 (Join-Path $Out "$Name.log") $log
  if ($process.ExitCode -ne 0) {
    Write-Host "FAIL: ${Name}: exit code $($process.ExitCode); the log: $(Join-Path $Out "$Name.log")"
    $log -split "`r?`n" | Where-Object { $_ -match 'TestDriver: FAIL' } | ForEach-Object { Write-Host "     $_" }
    $script:failures++
  }
  return $log
}

foreach ($theme in @('light', 'dark')) {
  Write-Host "--- The $theme theme"
  $name = "window-$theme"
  # Settings of the run's own: the Projects panel shown, its projects in an
  # empty folder of the run's own.
  $settings = Join-Path $Out "$name-settings"
  Remove-Item -Recurse -Force $settings -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $settings 'Mitcad') | Out-Null
  Set-Content -Encoding ascii (Join-Path $settings 'Mitcad\Mitcad.ini') @('[projects]', 'panel=true', 'panelTopBar=true')
  $projects = Join-Path $Out "$name-projects"
  Remove-Item -Recurse -Force $projects -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force $projects | Out-Null
  $selected = (Join-Path $Out "$name-selected.png") -replace "'", "''"
  $extrude = (Join-Path $Out "$name-extrude.png") -replace "'", "''"
  foreach ($shot in @($selected, $extrude)) { Remove-Item -Force ($shot -replace "''", "'") -ErrorAction SilentlyContinue }
  $driverScript = Join-Path $Out "$name.script"
  @"
# The theme asked for once the window shows (main.cpp), then the panels.
expect-ever 'Asking the platform for the $theme colour scheme' 30
expect-ever 'Browser Root/Bodies/Body1 at' 30
expect-ever 'Timeline Extrude1 at' 30
sleep 1500
# The body's row in the Browser, then Extrude1 in the Timeline, which adds
# the feature's row to the Browser's selection ("Browser selected: ...,
# Root/Sketches/Sketch1/Extrude1"; BrowserPanel::logSelection).
mark
click-logged 'Browser Root/Bodies/Body1 at'
expect-log 'Browser selected: .*Body1|Selected: 1 body'
sleep 500
mark
click-logged 'Timeline Extrude1 at'
expect-log 'Browser selected: .*Extrude1|Timeline selected: F2'
sleep 1500
screenshot '$selected'
mark
command solid.extrude
expect-log 'Command Extrude started|Editing F2 with Extrude'
sleep 2000
screenshot '$extrude'
"@ | Set-Content -Encoding ascii $driverScript
  $log = Invoke-Mitcad $name @('--no-recovery', '--chrome=docked', '--demo') @{
    'MITCAD_TEST_INPUT' = $driverScript; 'MITCAD_TEST_COLOR_SCHEME' = $theme
    'MITCAD_SETTINGS_DIR' = $settings; 'MITCAD_PROJECTS_DIR' = $projects
    'QT_SCALE_FACTOR' = $Scale.ToString([Globalization.CultureInfo]::InvariantCulture) }
  # What the window's style and frame logged (builds since mitcad#251).
  $log -split "`r?`n" | Where-Object { $_ -match '^(Style:|Window style:|Window frame:|View area )' } |
    Select-Object -Unique | ForEach-Object { Write-Host "     $_" }
  # The ribbon's groups are logged only while they show (Ribbon::logLayout):
  # hidden behind the tool bar's extension button, the window is too narrow.
  if ($log -match 'Ribbon SOLID/CREATE at [0-9]+,[0-9]+') { Write-Host "ok   the ribbon shows ($theme)" }
  else { Write-Host "FAIL: the ribbon's groups do not show ($theme): try a smaller -Scale"; $failures++ }
  foreach ($shot in @($selected, $extrude)) {
    $path = $shot -replace "''", "'"
    if (Test-Path $path) { Write-Host "ok   $path" }
    else { Write-Host "FAIL: no screenshot $path"; $failures++ }
  }
}

Write-Host "Screenshots and logs: $Out"
if ($failures -gt 0) { Write-Host "FAIL: window screenshots ($failures failures)"; exit 1 }
Write-Host 'PASS: window screenshots'
