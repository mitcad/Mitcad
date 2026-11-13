# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/files/RemotePreferences.cpp app/files/RemotePreferences.hpp
# check-all sources: app/files/Print3d.cpp core/vcs/src/remote/cli.rs
# check-all sources: app/view/NavigationPreferences.hpp app/framework/PreferencesPageLayout.hpp
# Input-driven Windows UI test of Preferences' size (mitcad#690) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), with git
# and without it, as tools/ui-preferences-test.sh does on Linux:
#   1. git found: the Cloud page names it.
#   2. MITCAD_GIT naming a file that is not there: the Cloud page says so.
#   3. No git on PATH nor where Git for Windows installs itself (git not
#      installed): the Cloud page's long message, which once made the
#      window wider than the screen, wraps.
# Each time Preferences opens on its Cloud page: the size it asks for (its
# largest page's; each page's own size logged, mitcad#1751)
# and its minimum size fit a 1366 x 768 screen with a title
# bar and a taskbar, it is resizable, its frame lies within the screen's
# available area, and the page list is as wide as its entries; on its
# General page (Home in the page list) the minimum is that page's, and it
# fits too. Once, Preferences on its Navigation page and on its Cloud page
# (mitcad#1394), each made as low as it goes, then 160 pixels taller: the
# page fills the window's height (its last line ends within
# the bottom margin of the area the window gives it), and its list (the
# bindings table, the known brokers) takes the height the page has to
# spare, at least its 138 or 110 pixels in the lowest window; pictures of
# both heights (navigation-tall.png, navigation-min.png, cloud-tall.png,
# cloud-min.png) in the output folder's shots.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preferences-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preferences'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The dialog resized as a user would drag its frame (the outer size; Windows
# keeps it at least the dialog's minimum).
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class PreferencesTestWindow {
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy,
                                                               uint flags);
    const uint SWP_NOMOVE = 0x2, SWP_NOZORDER = 0x4, SWP_NOACTIVATE = 0x10;

    public static bool Resize(IntPtr hwnd, int width, int height) {
        return SetWindowPos(hwnd, IntPtr.Zero, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }

    [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out RECT rect);

    // The window's outer height (0 when it is not known).
    public static int Height(IntPtr hwnd) {
        RECT r;
        return GetWindowRect(hwnd, out r) ? r.Bottom - r.Top : 0;
    }
}
'@

# Popups close at once in session 0: the Cloud and Navigation pages by keys
# of the test's own.
Ui-Init $App $Out $QtBin @{ 'tools.preferences.cloud' = 'F8'; 'tools.preferences.navigation' = 'F9' } -StepPause 0

$work = Join-Path $script:UiOut 'preferences'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
Remove-Item Env:MITCAD_GIT -ErrorAction SilentlyContinue

# Since-Match "regex": the match of the last line logged since the mark.
function Since-Match([string]$Pattern) {
  $lines = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
  $found = $null
  foreach ($line in $lines) {
    $match = [regex]::Match($line, $Pattern)
    if ($match.Success) { $found = $match }
  }
  return $found
}

# Fits what width height: within 1366 x 690, what a 1366 x 768 screen shows
# below a title bar and above a taskbar (mitcad#25).
function Fits([string]$What, [int]$Width, [int]$Height) {
  if ($Width -le 0 -or $Width -gt 1366 -or $Height -le 0 -or $Height -gt 690) {
    Ui-Fail "Preferences: $What $Width x $Height, more than a 1366 x 768 screen shows"
  }
  Write-Host "ok   Preferences: $What $Width x $Height fits a 1366 x 768 screen"
}

# Expect-Cloud "regex" "description": the Cloud page's line about the git
# program ("Preferences: cloud: <text>") since the mark matches.
function Expect-Cloud([string]$Pattern, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    $match = Since-Match '^Preferences: cloud: (.*)$'
    if ($match) {
      $text = $match.Groups[1].Value
      if ($text -notmatch "^$Pattern") { Ui-Fail "${Description}: the Cloud page says '$text'" }
      Write-Host "ok   ${Description}: $text"
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the Cloud page's git line is not in the log"
}

# Preferences-Fit "Cloud page regex" "description": Preferences opened on
# its Cloud page (F8), the Cloud page's text about git, the window's sizes
# and frame, then its General page, then closed.
function Preferences-Fit([string]$Pattern, [string]$Description) {
  Ui-Mark
  Ui-Step 'Preferences: Cloud (F8)'        { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: Cloud, ' 'Preferences on its Cloud page'
  Expect-Cloud $Pattern $Description
  Ui-ExpectNew 'Preferences page list ' 'Preferences shown'
  $asked = Since-Match '^Preferences opened: Cloud, (\d+) x (\d+)$'
  if (-not $asked) { Ui-Fail 'the size Preferences asks for is not logged' }
  Fits 'the size asked for' ([int]$asked.Groups[1].Value) ([int]$asked.Groups[2].Value)
  # Each page's own size is logged (mitcad#1751), so a failure of the
  # check above names the page; the dialog itself sizes by its largest page.
  $pageSizes = @(Ui-LogLines | Select-Object -Skip $script:UiMark |
                 ForEach-Object { [regex]::Match($_, '^Preferences page size: (.+), (\d+) x (\d+)$') } |
                 Where-Object { $_.Success })
  if (-not ($pageSizes | Where-Object { $_.Groups[1].Value -eq 'Cloud' })) {
    Ui-Fail "the size Preferences asks for with its Cloud page is not logged"
  }
  $minimum = Since-Match '^Preferences minimum (\d+) x (\d+), ([a-z]+)$'
  if (-not $minimum) { Ui-Fail "Preferences' minimum size is not logged" }
  Fits 'the minimum size' ([int]$minimum.Groups[1].Value) ([int]$minimum.Groups[2].Value)
  if ($minimum.Groups[3].Value -ne 'resizable') { Ui-Fail "Preferences cannot be resized ($($minimum.Groups[3].Value))" }
  Write-Host 'ok   Preferences can be resized'
  $frame = Since-Match '^Preferences frame (\d+) x (\d+) at (-?\d+),(-?\d+) on a screen of (\d+) x (\d+) at (-?\d+),(-?\d+)$'
  if (-not $frame) { Ui-Fail "Preferences' frame is not logged" }
  $v = @(1..8 | ForEach-Object { [int]$frame.Groups[$_].Value })
  $width = $v[0]; $height = $v[1]; $x = $v[2]; $y = $v[3]
  $screenWidth = $v[4]; $screenHeight = $v[5]; $screenX = $v[6]; $screenY = $v[7]
  if ($x -lt $screenX -or $y -lt $screenY -or $x + $width -gt $screenX + $screenWidth -or
      $y + $height -gt $screenY + $screenHeight) {
    Ui-Fail ("Preferences' frame $width x $height at $x,$y is not within the screen's " +
             "$screenWidth x $screenHeight at $screenX,$screenY")
  }
  if ($width -gt 1366) { Ui-Fail "Preferences' frame is $width wide, more than a 1366 x 768 screen" }
  Write-Host "ok   Preferences' frame $width x $height at $x,$y lies within the screen ($screenWidth x $screenHeight)"
  $list = Since-Match '^Preferences page list (\d+) wide, its entries (\d+)$'
  if (-not $list -or [int]$list.Groups[2].Value -le 0 -or [int]$list.Groups[1].Value -lt [int]$list.Groups[2].Value) {
    Ui-Fail "the page list does not show its entries whole: $(if ($list) { $list.Value } else { 'not logged' })"
  }
  Write-Host "ok   the page list ($($list.Groups[1].Value)) shows its entries whole ($($list.Groups[2].Value))"
  Ui-FocusDialog '^Preferences$'
  Ui-Mark
  Ui-Step 'the General page (Home)'        { Ui-Key 'Home' }
  Ui-ExpectNew 'Preferences page: General, minimum ' 'Preferences on its General page'
  $general = Since-Match '^Preferences page: General, minimum (\d+) x (\d+)$'
  Fits "the General page's minimum size" ([int]$general.Groups[1].Value) ([int]$general.Groups[2].Value)
  Ui-Step 'close it (Escape)'              { Ui-Key 'Escape' }
  Ui-FocusMain
}

# Page-Layout "window" "line": the page's last layout line since the mark,
# "<line> N high, its last line G above the bottom of its area A high", its
# last line within that area's bottom margin; the list's height and the
# area's.
function Page-Layout([string]$Window, [string]$Line) {
  $page = Since-Match ('^' + [regex]::Escape($Line) +
                       ' (\d+) high, its last line (-?\d+) above the bottom of its area (\d+) high$')
  if (-not $page) { Ui-Fail "'$Line' in the $Window window is not logged" }
  $list = [int]$page.Groups[1].Value
  $gap = [int]$page.Groups[2].Value
  $area = [int]$page.Groups[3].Value
  if ($gap -lt 0 -or $gap -gt 32) {
    Ui-Fail "${Line}: in the $Window window the page ends $gap pixels above the bottom of its $area pixels"
  }
  Write-Host "ok   ${Line}: in the $Window window the page ends $gap pixels above the bottom of its $area pixels, the list $list high"
  return , @($list, $area)
}

# Page-Shot shot "window" area: the picture the app saved of the window at
# the page area's height (MITCAD_TEST_PREFERENCES_SHOTS), kept as
# <shot>-<window>.png for the ticket.
function Page-Shot([string]$Shot, [string]$Window, [int]$Area) {
  $saved = Join-Path $shots "$Shot-$Area.png"
  if (-not (Test-Path $saved) -or (Get-Item $saved).Length -eq 0) { Ui-Fail "no picture $saved" }
  $kept = Join-Path $shots "$Shot-$Window.png"
  Copy-Item -Force $saved $kept
  Write-Host "ok   picture $kept"
}

# Page-Fills Page key "line" shot least: Preferences opened on the page (by
# its key), made as low as Windows lets it be (its minimum with the frame),
# then 160 pixels taller (mitcad#1394), then closed: the page's list
# ("line") takes the height the page has to spare, at least `least` pixels
# in the lowest window, and the page fills the area the window gives it;
# pictures <shot>-min.png and <shot>-tall.png.
function Page-Fills([string]$Page, [string]$Key, [string]$Line, [string]$Shot, [int]$Least) {
  Ui-Mark
  Ui-Step "Preferences: $Page ($Key)"      { Ui-Key $Key }
  Ui-ExpectNew "Preferences opened: $Page, " "Preferences on its $Page page"
  Ui-ExpectNew 'Preferences minimum ' 'Preferences shown'
  Ui-ExpectNew "$Line " "the $Page page laid out"
  $frame = Since-Match '^Preferences frame (\d+) x (\d+) at '
  if (-not $frame) { Ui-Fail "Preferences' frame is not logged" }
  $frameWidth = [int]$frame.Groups[1].Value
  Ui-FocusDialog '^Preferences$'
  $dialog = $script:UiTarget
  # Lower than any minimum: Windows keeps the window at its own. The page
  # logs a new line only when the height changed (not when Preferences
  # opened that low): the last one since it opened is the lowest window's,
  # once the sync has waited for the page's watched timer.
  Ui-Step 'Preferences as low as it goes'  {
    if (-not [PreferencesTestWindow]::Resize($dialog, $frameWidth, 100)) { Ui-Fail 'Preferences was not resized' }
  }
  Ui-Sync
  $low = Page-Layout 'lowest' $Line
  Page-Shot $Shot 'min' $low[1]
  $lowOuter = [PreferencesTestWindow]::Height($dialog)
  Ui-Mark
  Ui-Step 'Preferences 160 pixels taller'  {
    if (-not [PreferencesTestWindow]::Resize($dialog, $frameWidth, $lowOuter + 160)) { Ui-Fail 'Preferences was not resized' }
  }
  if ([PreferencesTestWindow]::Height($dialog) -le $lowOuter) {
    Ui-Fail "Preferences stays $lowOuter pixels high (outer) when asked for $($lowOuter + 160)"
  }
  Ui-ExpectNew "$Line " "the $Page page laid out in the taller window"
  Ui-Sync
  $tall = Page-Layout 'taller' $Line
  Page-Shot $Shot 'tall' $tall[1]
  if ($tall[1] -le $low[1]) {
    Ui-Fail "the $Page page's area is $($tall[1]) pixels high in the taller window, $($low[1]) in the lowest"
  }
  if ($low[0] -lt $Least) { Ui-Fail "$Line is $($low[0]) pixels high in the lowest window, less than $Least" }
  if ($tall[0] -le $low[0] -or $tall[0] - $low[0] -lt $tall[1] - $low[1] - 2) {
    Ui-Fail ("$Line grew from $($low[0]) to $($tall[0]) pixels while its page grew from " +
             "$($low[1]) to $($tall[1])")
  }
  Write-Host "ok   $Line takes the page's spare height: $($low[0]) pixels lowest, $($tall[0]) in the taller window"
  Ui-Step 'close it (Escape)'              { Ui-Key 'Escape' }
  Ui-FocusMain
}

Write-Host '--- 1. git found'
# The pictures of the pages that fill the window's height (mitcad#1394).
$shots = Join-Path $script:UiOut 'shots'
New-Item -ItemType Directory -Force $shots | Out-Null
$env:MITCAD_TEST_PREFERENCES_SHOTS = $shots
try {
  Ui-StartApp 'preferences-git'
} finally {
  Remove-Item Env:MITCAD_TEST_PREFERENCES_SHOTS -ErrorAction SilentlyContinue
}
Preferences-Fit 'git [0-9][^ ]* at ' 'the Cloud page names the git program found'
Page-Fills 'Navigation' 'F9' 'Navigation page: bindings table' 'navigation' 138
Page-Fills 'Cloud' 'F8' 'Cloud page: known brokers list' 'cloud' 110

Write-Host '--- 2. MITCAD_GIT naming a file that is not there'
$env:MITCAD_GIT = Join-Path $work 'no-such-git.exe'
Ui-StartApp 'preferences-mitcad-git'
Remove-Item Env:MITCAD_GIT
Preferences-Fit 'the git program set for Mitcad \(MITCAD_GIT\) is unavailable: ' 'the Cloud page says it is not there'

Write-Host '--- 3. No git on PATH nor where Git for Windows installs itself'
# Mitcad started with a PATH of only the folders without git.exe, and the
# folders where Git for Windows installs itself pointing at an empty one;
# the test's own environment back as it was once it runs.
$saved = @{}
foreach ($name in @('Path', 'ProgramFiles', 'ProgramW6432', 'LOCALAPPDATA')) {
  $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
}
$empty = Join-Path $work 'no-git-here'
New-Item -ItemType Directory -Force $empty | Out-Null
$env:Path = (@($env:Path -split ';' | Where-Object { $_ -and -not (Test-Path (Join-Path $_ 'git.exe')) })) -join ';'
$env:ProgramFiles = $empty
$env:ProgramW6432 = $empty
$env:LOCALAPPDATA = $empty
try {
  Ui-StartApp 'preferences-no-git'
} finally {
  foreach ($name in $saved.Keys) { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
}
Preferences-Fit 'git is not installed: ' 'the Cloud page says git is not installed'

Ui-Finish 'UI Windows preferences test'
