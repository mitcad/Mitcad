# SPDX-License-Identifier: MIT
# check-all sources: app/render app/view tools/cli
# Input-driven Windows UI test of the rendered view seen from below the
# ground (mitcad#320) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted); the Linux test is tools/ui-render-ground-test.sh. A red
# 40 mm box with the ground's shadows on (the ground is Cycles' shadow
# catcher, whose matte would leave only the background from below): in the
# orthographic and then the perspective view the camera orbits under the
# ground (Shift + middle drags) and back above it; the worker leaves the
# ground out below and builds it again above ("render-ground off|on"), and
# every view renders the box: its frames' opaque pixels (what the frame's
# alpha covers) are a good part of the view, not none.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-render-ground-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-render-ground'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.rendered' = 'F8'; 'view.orthographic' = 'F3'; 'view.perspective' = 'F11';
  'view.iso' = 'F12' } -StepPause 0
# On the CPU, few samples: the test checks what the frames cover.
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[render]', 'device=cpu')
$samples = 8
$env:MITCAD_RENDER_SAMPLES = "$samples"

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# A red 40 mm box (F1.b0) on the XY plane; the ground (shadows) is on by
# default, under the box.
$script = Join-Path $work 'box.json'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -20],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F1.b0", "appearance": "paint_red"}
]
'@
$design = Join-Path $work 'box.mitcad'
$output = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# Last-View: the sequence number of the last view sent to the worker.
function Last-View { return Ui-LastMatch '^Render view (\d+): ' }

# Expect-Box "description": the last view gets all its samples, and its last
# frame's opaque pixels cover at least a twentieth of the view (the box;
# "covers 0,0 0,0" when only the background shows).
function Expect-Box([string]$Description) {
  $view = $null
  for ($i = 0; $i -lt 300; $i++) {
    $view = Last-View
    if ($view -and (Ui-LogLines | Where-Object { $_ -match "^Render view ${view}: $samples samples in" })) { break }
    if (Ui-Crashed) { Ui-Fail "${Description}: the app crashed" }
    Start-Sleep -Milliseconds 200
  }
  if (-not (Ui-LogLines | Where-Object { $_ -match "^Render view ${view}: $samples samples in" })) {
    Ui-Fail "${Description}: view $view did not get its $samples samples"
  }
  $line = ''
  for ($i = 0; $i -lt 50 -and -not $line; $i++) {
    $line = "$(Ui-LogLines | Where-Object { $_ -match "^Render frame view $view \d+x\d+ samples $samples( denoised)? covers " } |
      Select-Object -Last 1)"
    if (-not $line) { Start-Sleep -Milliseconds 200 }
  }
  if ($line -notmatch ' covers (-?\d+),(-?\d+) (-?\d+),(-?\d+)$') {
    Ui-Fail "${Description}: no frame of view $view with all samples shown"
  }
  $area = ([int]$Matches[3] - [int]$Matches[1]) * ([int]$Matches[4] - [int]$Matches[2])
  $covers = "$($Matches[1]),$($Matches[2]) $($Matches[3]),$($Matches[4])"
  $size = (Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ }
  if ($area -le $size[2] * $size[3] / 20) {
    Ui-Fail "${Description}: the frame covers $covers of a $($size[2]) x $($size[3]) view: the box is not rendered"
  }
  Write-Host "ok   ${Description}: view $view covers $covers"
}

# Camera-Z: the z of the camera's last logged direction (from the eye to the
# model; above 0 while it looks up).
function Camera-Z {
  $z = Ui-LastMatch '^Camera direction -?[0-9.]+ -?[0-9.]+ (-?[0-9.]+) up'
  if ($null -eq $z) { return 0.0 }
  return [double]::Parse($z, [Globalization.CultureInfo]::InvariantCulture)
}

# Orbit-Drag share: one orbit, Shift + middle drag down the view by that
# share of its height in percent (negative: up), then the camera at rest.
function Orbit-Drag([int]$Share) {
  $from = Ui-ViewAt 50 (50 - $Share / 2)
  $to = Ui-ViewAt 50 (50 + $Share / 2)
  Ui-Mark
  [UiInput]::Hold($script:UiWindow, $false, $true, $false)
  try {
    Ui-Drag $from[0] $from[1] $to[0] $to[1] 2
  } finally {
    [UiInput]::Unhold($script:UiWindow)
  }
  Ui-Sync
  Ui-ExpectNew 'Camera direction ' 'the camera came to rest' 10 | Out-Null
}

# Orbit-Until off|on "description": orbits 30 % of the view's height at a
# time (the other way when a drag turned the camera the wrong way), until
# the worker logs the ground off (under it) or on (above it).
$script:Drag = 30
function Orbit-Until([string]$Want, [string]$Description) {
  $mark = (Ui-LogLines).Count
  $text = "Render worker: render-ground $Want"
  for ($i = 0; $i -lt 10; $i++) {
    if ((Ui-LogLines | Select-Object -Skip $mark) | Where-Object { $_.Contains($text) }) { break }
    $before = Camera-Z
    Orbit-Drag $script:Drag
    $after = Camera-Z
    # Under the ground the camera looks up (z grows), above it down.
    if (($Want -eq 'off' -and $after -lt $before) -or ($Want -eq 'on' -and $after -gt $before)) {
      $script:Drag = -$script:Drag
    }
    Start-Sleep -Milliseconds 500
  }
  if (-not ((Ui-LogLines | Select-Object -Skip $mark) | Where-Object { $_.Contains($text) })) {
    Ui-Fail "${Description}: the worker did not turn the ground $Want (camera direction z $(Camera-Z))"
  }
  Write-Host "ok   ${Description}: the ground $Want (camera direction z $(Camera-Z))"
}

Ui-StartApp 'render-ground' @('--open', $design)
Ui-ExpectLog 'box.mitcad' 'the box opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

foreach ($projection in @(@{ Name = 'Orthographic'; Key = 'F3' }, @{ Name = 'Perspective'; Key = 'F11' })) {
  $name = $projection.Name
  Write-Host "--- ${name}: under the ground and back"
  Ui-Step $name                          { Ui-Key $projection.Key }
  Ui-Step 'Isometric View (F12)'         { Ui-Key 'F12' }
  if (-not (Ui-LogLines | Where-Object { $_.Contains('Rendered view on') })) {
    Ui-Step 'View > Rendered (F8)'       { Ui-Key 'F8' }
    Ui-ExpectLog 'Render worker ready: Cycles' 'the worker runs Cycles' 30
  }
  Ui-ExpectLog 'Render environment studio, background view, ground shadows' 'the ground''s shadows are on'
  Expect-Box "$name, above the ground"
  Orbit-Until 'off' "$name, orbited under the ground"
  Expect-Box "$name, from below the ground"
  Orbit-Until 'on' "$name, orbited above the ground again"
  Expect-Box "$name, above the ground again"
}
if (Ui-LogLines | Where-Object { $_.Contains('Render worker error') }) { Ui-Fail 'the worker reported an error' }

Ui-Finish 'UI Windows render ground test'
