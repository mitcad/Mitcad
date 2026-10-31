# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowVisibility.cpp app/browser app/projects tools/cli
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# Input-driven Windows UI test of the sketch's own origin and axes in a new
# design, whose Origin folder's light bulb is off (mitcad#739), in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux test is tools/ui-sketch-origin-test.sh:
#   1. A new design starts with the design's origin hidden in the view and
#      the Origin folder's light bulb off. Its first view is framed and
#      logged ("View: new design framed, scale <mm> mm per view height at
#      <w>x<h>", mitcad#1000); the window resized smaller, larger (within
#      the screen) and taller than wide, maximized and restored (where the
#      window station maximizes it) and back keeps that scale ("View:
#      resized to ...").
#   2. Create Sketch on XY, XZ and YZ: the origin planes shown for the pick
#      only; in the sketch the design's origin is hidden again, while the
#      sketch draws its origin in the view and its x and y axes across it
#      ("Sketch axes drawn:"), where the sketch view puts the sketch's
#      (0, 0) and its x and y directions ("Sketch view").
#   3. The Line tool clicked a few pixels beside the origin starts the line
#      at the origin exactly (the snap). The XZ and YZ sketches get a line
#      away from the origin instead, so that they are kept (an empty new
#      sketch is taken back when it is finished).
#   4. After each sketch the Origin folder's light bulb is still off and
#      was never switched ("Visibility Root/Origin" is never logged).
#   5. Editing an existing sketch (the XZ one) draws its origin and axes
#      too, and its origin is snapped to.
#   6. New Design in a project (mitcad#844; the project made with
#      mitcad-cli): the new design opens with its Component1 active, Create
#      Sketch shows the origin planes, XY clicked in the view starts the
#      sketch in Component1, and the light bulb stays off.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-origin-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-origin'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7' } -StepPause 0

$hidden = "Design origin hidden in the view; the Origin folder's light bulb off"
$shown = "Design origin shown in the view; the Origin folder's light bulb off"

# Expect-OriginOff "description": within six seconds the last "Design
# origin hidden|shown in the view; ..." line says the design's origin is
# hidden with the light bulb off, and the Origin folder's light bulb was
# never switched. The match is case-sensitive and names the visibility line
# only: Ui-LastMatch (Select-String) ignores case, and a sketch edit's start
# logs "Sketch origin at ...; the design origin at x ... in the sketch",
# which came after the visibility line on Windows.
function Expect-OriginOff([string]$Description) {
  $last = $null
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LogLines | Where-Object { $_ -cmatch 'Design origin (hidden|shown) in the view; ' } |
      Select-Object -Last 1
    $last = if ("$line" -cmatch '(Design origin (hidden|shown) in the view; .*)$') { $Matches[1] } else { $null }
    if ($last -eq $hidden) { break }
    Start-Sleep -Milliseconds 200
  }
  if ($last -ne $hidden) { Ui-Fail "${Description}: the last origin line is '$last', expected '$hidden'" }
  $switched = Ui-LogLines | Where-Object { $_.Contains('Visibility Root/Origin') } | Select-Object -Last 1
  if ($switched) { Ui-Fail "${Description}: the Origin folder's light bulb was switched: '$switched'" }
  Write-Host "ok   ${Description}: the design's origin hidden, the Origin folder's light bulb off"
}

# Along ax ay bx by dx dy ox oy: the line a-b runs in the direction d and
# passes within two pixels of the point o.
function Along([double]$Ax, [double]$Ay, [double]$Bx, [double]$By, [double]$Dx, [double]$Dy,
               [double]$Ox, [double]$Oy) {
  $lx = $Bx - $Ax
  $ly = $By - $Ay
  $length = [Math]::Sqrt($lx * $lx + $ly * $ly)
  $d = [Math]::Sqrt($Dx * $Dx + $Dy * $Dy)
  if ($length -lt 50 -or $d -lt 1 -or ($lx * $Dx + $ly * $Dy) / ($length * $d) -lt 0.999) { return $false }
  $off = (($Ox - $Ax) * $ly - ($Oy - $Ay) * $lx) / $length
  return [Math]::Abs($off) -le 2
}

# Axes-Agree "axes" "view": the logged origin is in the view where the
# sketch view puts the sketch's (0, 0) (within two pixels), and the x and y
# axes are drawn across the view through it, each along the view's
# direction of the sketch's x or y.
function Axes-Agree([string]$Axes, [string]$View) {
  $pattern = '^origin at (-?\d+),(-?\d+) in the view; x axis (-?\d+),(-?\d+) to (-?\d+),(-?\d+); ' +
             'y axis (-?\d+),(-?\d+) to (-?\d+),(-?\d+)$'
  if ($Axes -notmatch $pattern) { return $false }
  $a = @(1..10 | ForEach-Object { [double]$Matches[$_] })
  $v = @($View -split '[ ,]' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
  if ($v.Count -ne 6) { return $false }
  if (($a[0] - $v[0]) * ($a[0] - $v[0]) + ($a[1] - $v[1]) * ($a[1] - $v[1]) -gt 4) { return $false }
  if (-not (Along $a[2] $a[3] $a[4] $a[5] ($v[2] - $v[0]) ($v[3] - $v[1]) $a[0] $a[1])) { return $false }
  return Along $a[6] $a[7] $a[8] $a[9] ($v[4] - $v[0]) ($v[5] - $v[1]) $a[0] $a[1]
}

# Expect-Axes "description": within six seconds the last "Sketch axes
# drawn:" line since the mark agrees with the last "Sketch view" line.
function Expect-Axes([string]$Description) {
  $axes = $null
  $view = $null
  for ($i = 0; $i -lt 30; $i++) {
    Ui-Sync
    $line = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Sketch axes drawn: ') } |
      Select-Object -Last 1
    $axes = if ($line -and $line -match 'Sketch axes drawn: (.*)$') { $Matches[1] } else { $null }
    $view = Ui-LastMatch 'Sketch view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
    if ($axes -and $view -and (Axes-Agree $axes $view)) {
      Write-Host "ok   ${Description}: $axes"
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the sketch's origin and axes are not drawn where the view puts them: '$axes' ($view)"
}

# Line-FromOrigin x y "description": the Line tool from a click four pixels
# right of and three above the sketch's origin to the sketch point (x, y):
# the line starts at the origin exactly.
function Line-FromOrigin([double]$X, [double]$Y, [string]$Description) {
  Ui-Mark
  Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
  Ui-ExpectNew 'Sketch tool Line' 'line tool active'
  $o = Ui-SketchAt 0 0
  Ui-Step 'click beside the origin'        { Ui-Click ($o[0] + 4) ($o[1] - 3) }
  Ui-Step "to ($X, $Y)"                    { Ui-SketchClick $X $Y }
  Ui-ExpectNew 'from (0, 0) to' "${Description}: the line starts at the origin (snapped)"
  Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
}

# Free-Line "description": a line from the sketch point (20, 10) to
# (40, 30), away from the origin and the axes, so that the sketch is kept
# when it is finished (an empty new sketch is taken back,
# MainWindow::finishSketch) and its origin stays unmade for a later snap.
function Free-Line([string]$Description) {
  Ui-Mark
  Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
  Ui-ExpectNew 'Sketch tool Line' 'line tool active'
  Ui-Step 'from (20, 10)'                  { Ui-SketchClick 20 10 }
  Ui-Step 'to (40, 30)'                    { Ui-SketchClick 40 30 }
  Ui-ExpectNew 'Added line ' "${Description}: a line away from the origin"
  Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
}

Write-Host "--- A new design: the origin hidden, the Origin folder's light bulb off"
Ui-StartApp 'sketch-origin'
Ui-ExpectLog $hidden 'a new design starts with its origin hidden'

Write-Host "--- A new design's framing kept when the window is resized (mitcad#1000)"
# The window resized as a user would drag its frame (the outer size),
# maximized and restored.
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class FramingTestWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy,
                                                               uint flags);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr hwnd, int command);
    [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hwnd);
    const uint SWP_NOMOVE = 0x2, SWP_NOZORDER = 0x4, SWP_NOACTIVATE = 0x10;

    public static bool Resize(IntPtr hwnd, int width, int height) {
        return SetWindowPos(hwnd, IntPtr.Zero, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
    public static int[] Size(IntPtr hwnd) {
        Rect rect;
        if (!GetWindowRect(hwnd, out rect)) { return null; }
        return new int[] { rect.Right - rect.Left, rect.Bottom - rect.Top };
    }
    [DllImport("user32.dll")] static extern bool SystemParametersInfo(uint action, uint param, out Rect rect, uint ini);
    // The primary screen's work area (SPI_GETWORKAREA): its size without
    // the taskbar.
    public static int[] WorkArea() {
        Rect rect;
        if (!SystemParametersInfo(0x30, 0, out rect, 0)) { return new int[] { 1024, 768 }; }
        return new int[] { rect.Right - rect.Left, rect.Bottom - rect.Top };
    }
    public static void Maximize(IntPtr hwnd) { ShowWindow(hwnd, 3); } // SW_MAXIMIZE
    public static void Restore(IntPtr hwnd) { ShowWindow(hwnd, 9); }  // SW_RESTORE
}
'@

Ui-ExpectLog 'View: new design framed, scale ' "a new design's first view is logged"
$framed = Ui-LastMatch 'View: new design framed, scale ([0-9.]+) mm'
if (-not $framed) { Ui-Fail 'no scale in the new design''s framing line' }
$framedScale = [double]::Parse($framed, [Globalization.CultureInfo]::InvariantCulture)

# Expect-Framing "description" [-Optional]: within six seconds a "View:
# resized to" line since the mark, with the new design's scale per view
# height (within the logged 0.001 mm). -Optional: no line is no failure
# (a window station that does not maximize the window).
function Expect-Framing([string]$Description, [switch]$Optional) {
  $line = $null
  for ($i = 0; $i -lt 30; $i++) {
    Ui-Sync
    $line = Ui-NewLines | Where-Object { $_.Contains('View: resized to ') } | Select-Object -Last 1
    if ($line) { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not $line) {
    if ($Optional) { Write-Host "skip ${Description}: the window's size did not change"; return }
    Ui-Fail "${Description}: no 'View: resized to' line since the mark"
  }
  if ("$line" -notmatch 'scale ([0-9.]+) mm per view height') { Ui-Fail "${Description}: no scale in '$line'" }
  $scale = [double]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Abs($scale - $framedScale) -gt 0.001) {
    Ui-Fail "${Description}: '$line', expected the new design's scale $framed mm per view height"
  }
  Write-Host "ok   ${Description}: $line"
}

$size = [FramingTestWindow]::Size($script:UiWindow)
if (-not $size) { Ui-Fail 'the window''s size is not known' }
# Every size within the screen's work area: Windows keeps a window within
# about the screen's size (its largest tracking size), so in session 0's
# 1024x768 desktop the window, which asks for more at the start, cannot grow
# and a larger size changed nothing. Smaller first, then as large as the
# work area allows, then narrow and as high as the work area (the view
# taller than wide; a window narrower than that is held at its minimum
# width, which changed nothing); maximized and restored from that narrow
# window, so that each step changes the view's size.
$area = [FramingTestWindow]::WorkArea()
$fit = @([Math]::Min($size[0], $area[0]), [Math]::Min($size[1], $area[1]))
Ui-Mark
Ui-Step 'a smaller window'                 { if (-not [FramingTestWindow]::Resize($script:UiWindow, $fit[0] - 300, $fit[1] - 150)) { Ui-Fail 'the window was not resized' } }
Expect-Framing 'a smaller window'
Ui-Mark
Ui-Step 'a larger window'                  { if (-not [FramingTestWindow]::Resize($script:UiWindow, $fit[0], $fit[1])) { Ui-Fail 'the window was not resized' } }
Expect-Framing 'a larger window'
Ui-Mark
Ui-Step 'a window taller than wide'        { if (-not [FramingTestWindow]::Resize($script:UiWindow, $fit[0] - 300, $fit[1])) { Ui-Fail 'the window was not resized' } }
Expect-Framing 'a window taller than wide'
Ui-Mark
Ui-Step 'the window maximized'             { [FramingTestWindow]::Maximize($script:UiWindow) }
$zoomed = [FramingTestWindow]::IsZoomed($script:UiWindow)
Expect-Framing 'the window maximized' -Optional:(-not $zoomed)
Ui-Mark
Ui-Step 'the window restored'              { [FramingTestWindow]::Restore($script:UiWindow) }
Expect-Framing 'the window restored' -Optional:(-not $zoomed)
Ui-Mark
Ui-Step "the window's size again"          { if (-not [FramingTestWindow]::Resize($script:UiWindow, $size[0], $size[1])) { Ui-Fail 'the window was not resized' } }
Expect-Framing "the window's size again"
# After maximize and restore in session 0 the main window's shortcuts no
# longer fire (F7 was logged and started nothing, also after a click on
# it), so the sketches start in a new design of their own.
Ui-StartApp 'sketch-origin-sketches'

foreach ($plane in @('xy', 'xz', 'yz')) {
  $name = $plane.ToUpperInvariant()
  Write-Host "--- Create Sketch on ${name}: the sketch's origin and axes drawn (mitcad#739)"
  Ui-Mark
  Ui-Step "create sketch on $name (F7)"    { Ui-CreateSketch 'F7' $plane }
  Ui-ExpectNew $shown "${name}: the origin planes shown for the pick"
  Ui-ExpectNew 'Sketch view' "${name}: the sketch view is logged"
  Expect-Axes $name
  Expect-OriginOff "$name in the sketch"
  if ($plane -eq 'xy') { Line-FromOrigin 30 20 'XY' } else { Free-Line $name }
  Ui-Mark
  Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
  Ui-ExpectNew 'Sketch finished' "${name}: the sketch finished"
  Ui-WaitIdle
  Expect-OriginOff "$name after the sketch"
}

Write-Host '--- Editing an existing sketch draws its origin and axes too (mitcad#739)'
Ui-ExpectLog 'Timeline Sketch2 at' 'the timeline is laid out'
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F2 (Sketch2)' 'the XZ sketch is edited again' 30
Ui-WaitIdle
Expect-Axes 'the edited XZ sketch'
Expect-OriginOff 'the edited XZ sketch'
Line-FromOrigin -30 20 'the edited XZ sketch'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the edited sketch finished'
Ui-WaitIdle
Expect-OriginOff 'after the edit'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- New Design in a project: Component1 active, the origin planes for the pick (mitcad#844)'
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$projects = Join-Path $script:UiOut 'projects'
Remove-Item -Recurse -Force $projects -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $projects | Out-Null
# The Projects panel lists the test's own projects, not the user's.
$env:MITCAD_PROJECTS_DIR = $projects
$project = Join-Path $projects 'Shop'
$output = & $cli project create $project --author 'Windows Tester <windows@example.invalid>' --design base.mitcad 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli project create: $($output -join ' / ')" }
Ui-StartApp 'sketch-origin-project' @('--open', (Join-Path $project 'base.mitcad'))
Ui-ExpectLog 'base.mitcad' 'a design of the project opened'
Ui-Mark
Ui-Step 'new design (Ctrl+N)'            { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New Design: in ' 'a design of the current project, not an untitled one'
Ui-ExpectNew 'Projects panel: naming a new design in ' 'named in the Projects panel' 20
Ui-Step 'take its name (Enter)'          { Ui-Key 'Return' }
Ui-ExpectNew 'New Design: made ' 'made as a version' 20
Ui-ExpectNew 'Opened the new design with Component1 active' 'the new design opens with Component1 active' 20
if (-not (Test-Path (Join-Path $project 'Design1.mitcad'))) { Ui-Fail 'the new design is not in the project' }
Ui-WaitIdle
Expect-OriginOff 'the new design'
Ui-CreateSketch 'F7' 'xy'
Ui-ExpectNew 'Create Sketch: origin planes shown' 'the origin planes shown for the pick'
Ui-ExpectNew $shown 'the design''s origin shown for the pick, its light bulb off'
Ui-ExpectNew 'Sketch Sketch1 is in Component1' 'XY clicked in the view starts the sketch in Component1'
Expect-OriginOff 'the new design''s sketch'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the new design''s sketch finished'
Ui-WaitIdle
Expect-OriginOff 'after the new design''s sketch'

Ui-Finish 'UI Windows sketch origin test'
