# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/SmallCirclesPreference.cpp app/view/ViewTessellation.hpp
# check-all sources: app/framework/DisplayMeshes.cpp app/framework/DocumentSession.cpp app/MainWindowDisplayMeshes.cpp
# check-all sources: app/OcctViewer.cpp app/main.cpp geometry/io/src/mesh.cpp tools/cli
# Input-driven Windows UI test of Preferences > Display > Small circles
# (mitcad#879) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), the counterpart of tools/ui-small-circles-test.sh, on a plate
# that mitcad-cli makes: 60 x 40 x 3 with a hole of 2 mm through it.
#   1. The app starts with the view's small circles Adaptive (the default).
#   2. Preferences on its Display page, Small circles (Alt+M) Full Detail
#      (Down), OK: kept in the settings, and the open design's shown body
#      is meshed again ("1 bodies meshed again with small circles full
#      detail"), with more triangles than in Adaptive.
#   3. The app again: it starts with Full Detail; Preferences back to
#      Adaptive (Up) meshes the body again with fewer triangles.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-small-circles-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-small-circles'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.preferences.display' = 'Shift+F8' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 3}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 20], "diameter": 2},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design 2>&1
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'

# Small-Circles key logged: Preferences on its Display page, Small circles
# (Alt+M) moved by the key, OK.
function Small-Circles([string]$Key, [string]$Logged) {
  Ui-Mark
  Ui-Step 'Preferences: Display (Shift+F8)' { Ui-Key 'shift+F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' 'Preferences on its Display page'
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Small circles (Alt+M)'         { Ui-Key 'alt+m' }
  Ui-ExpectNew 'Preferences small circles focused: ' 'the keyboard in Small circles'
  Ui-Step "key $Key"                      { Ui-Key $Key }
  Ui-Step 'OK (Enter)'                    { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew $Logged "the preference saved ($Logged)"
}

# Remeshed choice: waits for the body meshed again with the choice; its
# triangles.
function Remeshed([string]$Choice) {
  Ui-ExpectNew "1 bodies meshed again with small circles $Choice in " "the shown body meshed again ($Choice)" 60
  $triangles = Ui-LastMatch "meshed again with small circles $Choice in [0-9]+ ms \(([0-9]+) triangles\)"
  if (-not $triangles) { Ui-Fail "no triangles logged for $Choice" }
  return [int]$triangles
}

Write-Host '--- Adaptive by default'
Ui-StartApp 'small-circles-adaptive' @('--open', $design)
Ui-ExpectLog 'Small circles: adaptive' "the view's small circles adaptive at the start"
Ui-ExpectLog 'Bodies shown: ' 'the plate shown'
Ui-Sync

Write-Host '--- Full Detail'
Small-Circles 'Down' 'Preferences: small circles full'
if (-not (Select-String -Quiet -Pattern '^smallCircles=full' -Path $ini)) { Ui-Fail 'Full Detail not kept in the settings' }
$full = Remeshed 'full detail'
Write-Host "ok   $full triangles in Full Detail"
if (Ui-LogLines | Where-Object { $_ -match 'meshing again with small circles .* cancelled' }) {
  Ui-Fail 'the meshing again was cancelled'
}
Ui-StopApp

Write-Host '--- Full Detail kept; Adaptive again'
Ui-StartApp 'small-circles-full' @('--open', $design)
Ui-ExpectLog 'Small circles: full' 'the app starts with Full Detail'
Ui-ExpectLog 'Bodies shown: ' 'the plate shown'
Ui-Sync
Small-Circles 'Up' 'Preferences: small circles adaptive'
if (-not (Select-String -Quiet -Pattern '^smallCircles=adaptive' -Path $ini)) { Ui-Fail 'Adaptive not kept in the settings' }
$adaptive = Remeshed 'adaptive'
Write-Host "ok   $adaptive triangles in Adaptive"
if ($adaptive -ge $full) { Ui-Fail "Adaptive ($adaptive triangles) not fewer than Full Detail ($full)" }
Write-Host 'ok   Adaptive has fewer triangles than Full Detail'

Ui-Finish 'UI Windows small circles test'
