# SPDX-License-Identifier: MIT
# Windows UI test for the build VM: runs mitcad.exe with Mesa's software
# OpenGL (llvmpipe, see tools/dev-env/setup-mesa-windows.ps1 and
# MITCAD_MESA_DIR) and checks its screenshots:
#   1. --demo: the demo block is drawn with the body colours.
#   2. --open of a project file with a fillet: recomputed and drawn.
#   3. the same file with --set d3=35: the block is drawn taller.
# The app renders into an offscreen framebuffer and --screenshot reads it
# back, so this works over SSH, in session 0 without a desktop.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
# Qt's DLLs are not copied next to the app; -QtBin defaults to the Qt of
# the build directory's CMakeCache.txt.
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows'),
  [string]$QtBin = '')

$ErrorActionPreference = 'Stop'
$App = (Resolve-Path $App).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path
$failures = 0

function Pass([string]$Message) { Write-Host "ok   $Message" }
function Fail([string]$Message) { Write-Host "FAIL: $Message"; $script:failures++ }

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

# Pixel statistics of a screenshot. A "face" pixel lies inside a uniformly
# coloured area (thin grid lines and edges drop out) and is clearly darker
# than the light background; face pixels are grouped by brightness, and
# every group covering at least 1.5 % of the image counts as a face shade.
# The orientation cube in the upper right corner is left out.
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

public class Shade {
    public int Luma, Pixels, R, G, B;
}

public class ShotStats {
    public int Width, Height, Background, FacePixels;
    public List<Shade> Shades = new List<Shade>();
    public double Percent(int pixels) { return 100.0 * pixels / (Width * Height); }
}

public static class Shot {
    const int Bucket = 8;

    public static ShotStats Analyze(string path) {
        using (Bitmap bitmap = new Bitmap(path)) {
            int w = bitmap.Width, h = bitmap.Height;
            BitmapData data = bitmap.LockBits(new Rectangle(0, 0, w, h), ImageLockMode.ReadOnly,
                                              PixelFormat.Format24bppRgb);
            int stride = data.Stride;
            byte[] px = new byte[stride * h];
            Marshal.Copy(data.Scan0, px, 0, px.Length);
            bitmap.UnlockBits(data);

            ShotStats stats = new ShotStats();
            stats.Width = w;
            stats.Height = h;
            // The background is a vertical gradient: per row, the median
            // brightness of the rightmost pixels, which the fitted body
            // leaves free (grid lines are thin).
            int[] background = new int[h];
            for (int y = 0; y < h; y++) {
                List<int> edge = new List<int>();
                for (int x = Math.Max(0, w - 9); x < w; x++) edge.Add(Luma(px, stride, x, y));
                edge.Sort();
                background[y] = edge[edge.Count / 2];
            }
            int[] sorted = (int[])background.Clone();
            Array.Sort(sorted);
            stats.Background = sorted[h / 2];

            int cube = Math.Min(w, h) / 3;
            long[,] sums = new long[256 / Bucket + 1, 4];
            for (int y = 2; y < h - 2; y++) {
                for (int x = 2; x < w - 2; x++) {
                    if (x > w - cube && y < cube) continue;
                    if (!Uniform(px, stride, x, y)) continue;
                    int luma = Luma(px, stride, x, y);
                    if (luma > background[y] - 12) continue;
                    int i = y * stride + 3 * x;
                    int bucket = luma / Bucket;
                    sums[bucket, 0]++;
                    sums[bucket, 1] += px[i + 2];
                    sums[bucket, 2] += px[i + 1];
                    sums[bucket, 3] += px[i];
                    stats.FacePixels++;
                }
            }
            for (int bucket = sums.GetLength(0) - 1; bucket >= 0; bucket--) {
                long n = sums[bucket, 0];
                if (n * 1000 < 15L * w * h) continue;
                Shade shade = new Shade();
                shade.Luma = bucket * Bucket + Bucket / 2;
                shade.Pixels = (int)n;
                shade.R = (int)(sums[bucket, 1] / n);
                shade.G = (int)(sums[bucket, 2] / n);
                shade.B = (int)(sums[bucket, 3] / n);
                stats.Shades.Add(shade);
            }
            return stats;
        }
    }

    static int Luma(byte[] px, int stride, int x, int y) {
        int i = y * stride + 3 * x;
        return (px[i + 2] * 299 + px[i + 1] * 587 + px[i] * 114) / 1000;
    }

    static bool Uniform(byte[] px, int stride, int x, int y) {
        int c = y * stride + 3 * x;
        int[] offsets = { -2 * 3, 2 * 3, -2 * stride, 2 * stride };
        foreach (int offset in offsets) {
            for (int k = 0; k < 3; k++) {
                if (Math.Abs(px[c + k] - px[c + offset + k]) > 4) return false;
            }
        }
        return true;
    }
}
'@

# Invoke-Mitcad name arguments...: runs the app until it exits and returns
# its log (Qt messages go to stderr with QT_FORCE_STDERR_LOGGING).
function Invoke-Mitcad([string]$Name, [string[]]$Arguments, [hashtable]$Environment = @{}) {
  $info = New-Object System.Diagnostics.ProcessStartInfo $App
  $info.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.EnvironmentVariables['QT_FORCE_STDERR_LOGGING'] = '1'
  # Autosave's recovery folder of the test's own, not the user's.
  $info.EnvironmentVariables['MITCAD_AUTOSAVE_DIR'] = Join-Path $Out 'autosave'
  # Crash reports of the test's own (mitcad#62): none offered from the user's.
  $info.EnvironmentVariables['MITCAD_CRASH_DIR'] = Join-Path $Out 'crashes'
  # The store of computed results (P7d) off, unless the run turns it on.
  $info.EnvironmentVariables['MITCAD_RESULT_STORE'] = 'off'
  # No update checks (mitcad#9): the tests make no network requests.
  $info.EnvironmentVariables['MITCAD_NO_UPDATE_CHECK'] = '1'
  foreach ($variable in $Environment.GetEnumerator()) {
    $info.EnvironmentVariables[$variable.Key] = $variable.Value
  }
  $process = [System.Diagnostics.Process]::Start($info)
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit(120000)) {
    $process.Kill()
    Fail "${Name}: Mitcad did not exit within 120 s"
  }
  $process.WaitForExit()
  $log = $stdout.Result + $stderr.Result
  Set-Content -Encoding utf8 (Join-Path $Out "$Name.log") $log
  if ($process.ExitCode -ne 0) { Fail "${Name}: exit code $($process.ExitCode)"; Write-Host $log }
  return $log
}

function Expect-Log([string]$Log, [string]$Pattern, [string]$Description) {
  if ($Log -match $Pattern) { Pass $Description } else { Fail "${Description}: '$Pattern' not in the log" }
}

# Check-Shot name: checks a screenshot for the body and returns its stats.
function Check-Shot([string]$Name) {
  $path = Join-Path $Out "$Name.png"
  if (-not (Test-Path $path)) { Fail "${Name}: no screenshot"; return $null }
  $stats = [Shot]::Analyze($path)
  $shades = ($stats.Shades | ForEach-Object {
    '{0} rgb({1},{2},{3}) {4:0.0}%' -f $_.Luma, $_.R, $_.G, $_.B, $stats.Percent($_.Pixels) }) -join ', '
  Write-Host ("     {0}: {1}x{2}, background {3}, faces {4:0.0}%: {5}" -f $Name, $stats.Width,
    $stats.Height, $stats.Background, $stats.Percent($stats.FacePixels), $shades)
  if ($stats.Width -lt 300 -or $stats.Height -lt 200) { Fail "${Name}: image too small" }
  elseif ($stats.Background -lt 200) { Fail "${Name}: background is not the light gradient" }
  elseif ($stats.Percent($stats.FacePixels) -lt 20) { Fail "${Name}: no body in the image" }
  elseif ($stats.Shades.Count -lt 2) { Fail "${Name}: fewer than two lit faces" }
  elseif ($stats.Shades | Where-Object { $_.B -lt $_.R -or $_.B - $_.R -gt 40 }) {
    Fail "${Name}: faces are not the bluish grey body colour"
  } else { Pass "${Name}: body drawn with $($stats.Shades.Count) face shades" }
  return $stats
}

# Side faces relative to the top face (the brightest shade).
function Side-Ratio($Stats) {
  $top = $Stats.Shades[0].Pixels
  $sides = ($Stats.Shades | Select-Object -Skip 1 | Measure-Object -Property Pixels -Sum).Sum
  return $sides / $top
}

Write-Host '--- Demo block'
$log = Invoke-Mitcad 'demo' @('--demo', '--screenshot', (Join-Path $Out 'demo.png'))
if ($log -match 'OpenGL renderer: (.*)') { $renderer = $Matches[1].Trim() } else { $renderer = '' }
if ($renderer -like 'llvmpipe*') { Pass "OpenGL renderer: $renderer" }
else { Fail "OpenGL renderer is '$renderer', expected Mesa's llvmpipe" }
$null = Check-Shot 'demo'

Write-Host '--- Open a project file, then change a parameter'
$file = Join-Path $Out 'block.mitcad'
@'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 },
    { "name": "d3", "value": 20.0 }, { "name": "d4", "value": 3.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 0 },
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude1", "role": "side", "index": 2 },
      { "extrude": "Extrude1", "role": "side", "index": 3 } ], "radius": "d4" }
  ]
}
'@ | Set-Content -Encoding ascii $file
$log = Invoke-Mitcad 'block20' @('--open', $file, '--screenshot', (Join-Path $Out 'block20.png'))
Expect-Log $log 'Opened .*block\.mitcad' 'opened with --open'
Expect-Log $log 'Recomputed 3 feature\(s\)' 'sketch, extrude and fillet recomputed'
if ($log.Contains('Recompute failed')) { Fail 'a recompute failed' }
$low = Check-Shot 'block20'

$log = Invoke-Mitcad 'block35' @('--open', $file, '--set', 'd3=35', '--screenshot', (Join-Path $Out 'block35.png'))
Expect-Log $log 'Recomputed 2 feature\(s\)' 'd3=35 recomputed extrude and fillet'
$high = Check-Shot 'block35'
if ($low -and $high -and $low.Shades.Count -ge 2 -and $high.Shades.Count -ge 2) {
  $lowRatio = Side-Ratio $low
  $highRatio = Side-Ratio $high
  $message = 'side faces / top face {0:0.00} -> {1:0.00}' -f $lowRatio, $highRatio
  if ($highRatio -gt 1.3 * $lowRatio) { Pass "block drawn taller with d3=35: $message" }
  else { Fail "block not drawn taller with d3=35: $message" }
}

# The result store (P7d): the first open stores the extrusion and the fillet
# (however quick), the second takes them from the store.
$results = Join-Path $Out 'results'
Remove-Item -Recurse -Force $results -ErrorAction SilentlyContinue
$store = @{ 'MITCAD_RESULT_STORE' = $results; 'MITCAD_RESULT_STORE_MIN_MS' = '0' }
$log = Invoke-Mitcad 'store1' @('--open', $file, '--screenshot', (Join-Path $Out 'store1.png')) $store
Expect-Log $log 'Result store: restored 0, evaluated 3; stored 2 ' 'the first open stored two results'
$log = Invoke-Mitcad 'store2' @('--open', $file, '--screenshot', (Join-Path $Out 'store2.png')) $store
Expect-Log $log 'Result store: restored 2, evaluated 1; stored 0 ' 'the second open took them from the store'
Expect-Log $log 'Recomputed 1 feature\(s\)' 'only the sketch computed'
$stored = Check-Shot 'store2'
if ($low -and $stored -and $stored.Shades.Count -eq $low.Shades.Count) {
  Pass 'the stored block is drawn as the computed one'
}

Write-Host "Screenshots and logs: $Out"
if ($failures -gt 0) { Write-Host "FAIL: UI Windows test ($failures failures)"; exit 1 }
Write-Host 'PASS: UI Windows test'
