# SPDX-License-Identifier: MIT
# check-all sources: app/files core/sketch/src/point_file.rs core/sketch/src/point_fit.rs core/model-api/src/sketch.rs tools/ui-scripts/points.mitcad-ui
# Insert Points (mitcad#2232) in the build VM, as tools/ui-points-test.sh on
# Linux: naca2412.dat is written here from the NACA 4-digit formula (a
# Selig file of 121 points, 5 decimals), and the shared input script
# tools/ui-scripts/points.mitcad-ui inserts it on a new design's XY plane
# as a fitted spline closed across its blunt trailing edge and extrudes
# the selected profile.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-points-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-points'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# NACA 2412 (camber 0.02 at 0.4, thickness 0.12), 61 stations per surface
# spaced closer at both edges, from the upper trailing edge round the
# leading edge to the lower one.
$invariant = [Globalization.CultureInfo]::InvariantCulture
function Station([double]$x, [bool]$upper) {
  $yt = 0.6 * (0.2969 * [Math]::Sqrt($x) - 0.1260 * $x - 0.3516 * $x * $x + 0.2843 * [Math]::Pow($x, 3) - 0.1015 * [Math]::Pow($x, 4))
  if ($x -lt 0.4) {
    $yc = 0.02 / 0.16 * (0.8 * $x - $x * $x); $slope = 0.04 / 0.16 * (0.4 - $x)
  } else {
    $yc = 0.02 / 0.36 * (0.2 + 0.8 * $x - $x * $x); $slope = 0.04 / 0.36 * (0.4 - $x)
  }
  $theta = [Math]::Atan($slope)
  if ($upper) {
    $px = $x - $yt * [Math]::Sin($theta); $py = $yc + $yt * [Math]::Cos($theta)
  } else {
    $px = $x + $yt * [Math]::Sin($theta); $py = $yc - $yt * [Math]::Cos($theta)
  }
  return ' ' + $px.ToString('F5', $invariant) + '  ' + $py.ToString('F5', $invariant)
}
$lines = New-Object System.Collections.Generic.List[string]
$lines.Add('NACA 2412')
$n = 61
for ($k = $n - 1; $k -ge 0; $k--) { $lines.Add((Station (0.5 * (1 - [Math]::Cos([Math]::PI * $k / ($n - 1)))) $true)) }
for ($k = 1; $k -lt $n; $k++) { $lines.Add((Station (0.5 * (1 - [Math]::Cos([Math]::PI * $k / ($n - 1)))) $false)) }
[IO.File]::WriteAllLines((Join-Path $work 'naca2412.dat'), $lines)
if ($lines.Count -ne 122) { Ui-Fail "naca2412.dat: $($lines.Count - 1) points, not 121" }
Write-Host 'ok   naca2412.dat written from the NACA 4-digit formula'

# The script types the path with forward slashes, as the log shows it.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/points.mitcad-ui'
Ui-StartApp 'points' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Insert Points driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Insert Points driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Insert Points of an airfoil file and Extrude of its profile'
Ui-StopApp
Write-Host 'PASS: Insert Points'
