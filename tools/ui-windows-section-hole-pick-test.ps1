# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp core/model/src/document/analyses.rs core/model/src/features/geom_ref.rs tools/cli
# Section Analysis through a pick (mitcad#1769) in the build VM, as
# tools/ui-section-hole-pick-test.sh on Linux, on a design mitcad-cli makes:
# a 40 x 40 x 10 mm block (Body1, F2.b0) with a 20 mm hole along Z through
# its middle (Extrude2, F4; the hole's round face F4:side(c1), its axis
# x = 20, y = 20).
#   1. Two faces selected (the hole's and the top): Section Analysis
#      starts with nothing picked and shows no section.
#   2. The hole's round face selected, then Section Analysis: it starts
#      with the face as its plane and shows the section at once, through
#      the hole's axis, its turn starting at XZ (normal +Y): the block cut
#      across the hole, two 10 x 10 mm pieces (area 200, outline 80).
#   3. OK keeps it as Section1, the face as its plane.
#   4. Section1 edited: its Angle 45 degrees turns the plane about the
#      hole's axis without an Axis (a ring about the hole's axis), through
#      the block's diagonal: (40 sqrt(2) - 20) x 10 = 365.685 mm^2. OK keeps
#      it; the saved file has the face, the angle and no axis.
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-workflow-test.ps1 does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-section-hole-pick-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-section-hole-pick'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'inspect.section' = 'Shift+F4' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'hole.mitcad'
$hole = 'F4:side(c1)'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 20], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Through-Axis nx ny nz: the last section shown since the mark has that
# normal and contains the hole's axis (x = 20, y = 20, along Z): its normal
# has no Z part and its origin lies on a plane through the axis.
function Through-Axis([double]$Nx, [double]$Ny, [double]$Nz) {
  $line = @(Ui-NewLines) | Where-Object { $_.Contains('Section analysis at ') } | Select-Object -Last 1
  if (-not $line) { Ui-Fail 'no section shown' }
  $plane = ($line.Substring($line.IndexOf('Section analysis at ') + 20) | ConvertFrom-Json).plane
  $o = @($plane.origin)
  $n = @($plane.normal)
  $want = @($Nx, $Ny, $Nz)
  for ($i = 0; $i -lt 3; $i++) {
    if ([Math]::Abs([double]$n[$i] - $want[$i]) -gt 1e-6) { Ui-Fail "the section's normal is not ($Nx, $Ny, $Nz): $line" }
  }
  $off = ([double]$o[0] - 20) * [double]$n[0] + ([double]$o[1] - 20) * [double]$n[1]
  if ([Math]::Abs([double]$n[2]) -gt 1e-9 -or [Math]::Abs($off) -gt 1e-6) {
    Ui-Fail "the section is not through the hole's axis: $line"
  }
  Write-Host "ok   the section through the hole's axis, normal ($Nx, $Ny, $Nz)"
}

Ui-StartApp 'section-hole-pick' @('--open', $design)
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the block shown'

# Section Analysis once for the log's pick places of the faces; cancelled.
Ui-Mark
Ui-Step 'section analysis (Shift+F4)'       { Ui-Key 'shift+F4' }
Ui-ExpectNew 'Command Section Analysis started' 'Section Analysis started'
Ui-ExpectNew "Pick face F2.b0/$hole at" "the hole's round face can be picked"
Ui-ExpectNew 'Pick face F2.b0/F2:end(' 'the top face can be picked'
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Section Analysis cancelled' 'cancelled'

Write-Host '--- 1. Two faces selected: nothing picked'
Ui-Mark
Ui-Step "select the hole's face"            { Ui-ClickPick "face F2.b0/$hole" }
Ui-Step 'Ctrl+click the top face' {
  $at = Ui-PickAt 'face F2.b0/F2:end('
  Ui-ClickWith 'ctrl' $at[0] $at[1]
}
Ui-ExpectNew 'Selected: 2 faces' 'both faces selected'
Ui-Mark
Ui-Step 'section analysis (Shift+F4)'       { Ui-Key 'shift+F4' }
Ui-ExpectNew 'Command Section Analysis started' 'Section Analysis started'
Ui-WaitIdle
Ui-Sync
$since = @(Ui-NewLines)
if ($since | Where-Object { $_.Contains('Section Analysis Faces/Plane: 1 ') }) {
  Ui-Fail 'an item of two selected was taken as the plane'
}
if ($since | Where-Object { $_.Contains('Inspect Section Analysis: area ') }) { Ui-Fail 'a section was shown' }
Write-Host 'ok   started with nothing picked, no section'
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Section Analysis cancelled' 'cancelled'
Ui-Step 'clear the selection (Esc)'         { Ui-Key 'Escape' }

Write-Host "--- 2. The hole's face selected, then Section Analysis"
Ui-Mark
Ui-Step "select the hole's face"            { Ui-ClickPick "face F2.b0/$hole" }
Ui-ExpectNew "Selected: 1 face [face $hole of F2.b0" "the hole's face selected"
Ui-Mark
Ui-Step 'section analysis (Shift+F4)'       { Ui-Key 'shift+F4' }
Ui-ExpectNew 'Command Section Analysis started' 'Section Analysis started'
Ui-ExpectNew "Section Analysis Faces/Plane: 1 face [face $hole of F2.b0" 'the selected face is its plane'
Ui-ExpectNew 'Section analysis at ' 'the section shown at once'
Ui-ExpectNew 'Inspect Section Analysis: area 200, length 80' `
  "the block cut through the hole's axis: two 10 x 10 mm pieces" 12
Through-Axis 0 1 0

Write-Host '--- 3. Kept'
Ui-Mark
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Kept Section Analysis as Section1' 'kept in the document'
Ui-ExpectNew 'Browser Root/Analysis/Section1 at' 'listed in the Analysis folder'

Write-Host "--- 4. Its Angle edited: turned about the hole's axis"
Ui-Mark
Ui-Step 'edit Section1 (double-click)'      { Ui-DoubleClickLogged 'Browser Root/Analysis/Section1' }
Ui-ExpectNew 'Editing analysis Section1 with Section Analysis' 'its panel opens'
Ui-ExpectNew "Section Analysis Faces/Plane: 1 face [face $hole of F2.b0" "with the hole's face"
Ui-ExpectNew 'Manipulator Section Analysis angle at' "a ring about the hole's axis, without an Axis"
Ui-Mark
Ui-Step '45 degrees'                        { Ui-TypeIn 'Panel Section Analysis input angle' '45' }
Ui-ExpectNew 'Inspect Section Analysis: area 365.685' "turned to the block's diagonal: 365.685 mm^2" 12
Through-Axis -0.7071067811865476 0.7071067811865476 0
if (@(Ui-NewLines) | Where-Object { $_.Contains('Select an axis for the angle.') }) {
  Ui-Fail 'the panel asked for an axis'
}
Write-Host 'ok   no axis asked for'
Ui-Mark
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Edited analysis Section1' 'the turned section kept'
Ui-Step 'save (Ctrl+S)'                     { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$kept = @((Get-Content -Raw $design | ConvertFrom-Json).analyses)[0]
if ($kept.plane.face -ne $hole -or [Math]::Abs([double]$kept.angle - [Math]::PI / 4) -gt 1e-9 -or
    $null -ne $kept.PSObject.Properties['axis']) {
  Ui-Fail "the file's section: $($kept | ConvertTo-Json -Compress)"
}
Write-Host "ok   saved with the hole's face as its plane, the angle and no axis"

Ui-Finish 'UI Windows section hole pick test'
