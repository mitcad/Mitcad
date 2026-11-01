# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/ModelShapes.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowActivate.cpp core/model/src/features/construction tools/cli
# Input-driven Windows UI test of a construction plane of the active
# component on a face of another component's occurrence, both placed by a
# turn and a move (mitcad#1029), in the build VM (tools/ui-windows-lib.ps1
# says how the input is posted); the Linux test is
# tools/ui-construct-occurrence-test.sh. Motor:1 (O1) is turned 90 degrees
# about Z and moved by (100, 0, 0): its 30 x 20 x 10 block's top face
# (z = 10 in Motor) lies at z = 10 in the design too. Frame:1 (O2) is
# turned 90 degrees about X and moved by (0, 50, 0): a point q of the
# design is (qx, qz, 50 - qy) in Frame.
#   1. Frame active, Offset Plane (F7) on Motor's top face: the pick asks
#      whether to activate Motor:1 (mitcad#960), answered Keep in Frame:1.
#      The preview is drawn where the finished plane is shown ("Preview
#      datum in the design" against the Construct command's "Added ...; in
#      the design: ..."): 10 mm above the face, z = 20 with the normal
#      (0, 0, 1) in the design, y = 20 with the normal (0, 1, 0) in Frame.
#   2. The plane edited from the timeline: its preview there too, 12 mm
#      above the face; cancelled.
#   3. The design saved: the plane is Frame's, linked to Motor's body from
#      O1 to O2, and moving Motor:1 15 mm up (mitcad-cli) moves it 15 mm
#      along Frame's y.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construct-occurrence-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construct-occurrence'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'construct.offset_plane' = 'F7' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Motor (C1, O1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: F2.b0.
# Frame (C2, O2): Sketch2 (F3) 20 x 20, Extrude2 (F4) 5 up: F4.b0, clear of
# Motor's block in the design. The plane made in the test is F5.
$modelScript = Join-Path $work 'turned.json'
$design = Join-Path $work 'turned.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "create_component", "name": "Motor", "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Frame", "transform": [[1, 0, 0, 0], [0, 0, -1, 50], [0, 1, 0, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
'@ -creplace 'RECT', $rect)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

$invariant = [Globalization.CultureInfo]::InvariantCulture

# The last line logged since the mark that contains the text.
function New-Line([string]$Text) {
  $line = Ui-LogLines | Select-Object -Skip ($script:UiMark + 1) | Where-Object { $_.Contains($Text) } |
    Select-Object -Last 1
  if (-not $line) { Ui-Fail "'$Text' not in the log since the mark" }
  return "$line"
}

# Part "origin (x, y, z), normal (a, b, c)" origin|normal: the three numbers.
function Part([string]$Text, [string]$Name) {
  if ($Text -notmatch "$Name \(([^)]*)\)") { Ui-Fail "no $Name in '$Text'" }
  return , @($Matches[1] -split ',\s*' | ForEach-Object { [double]::Parse($_.Trim(), $invariant) })
}

# Within: each coordinate within the tolerance of the expected one; NaN
# expects any value.
function Within([double[]]$Got, [double[]]$Want, [double]$Tolerance, [string]$Description) {
  for ($i = 0; $i -lt 3; $i++) {
    if (-not [double]::IsNaN($Want[$i]) -and [Math]::Abs($Got[$i] - $Want[$i]) -gt $Tolerance) {
      Ui-Fail "${Description}: ($($Got -join ', ')), expected ($($Want -join ', ')) within $Tolerance"
    }
  }
  Write-Host "ok   ${Description}: ($($Got -join ', '))"
}
$any = [double]::NaN

# The question whether to activate a picked face's component (mitcad#960),
# which the other tests turn off.
$env:MITCAD_ASK_ACTIVATE_COMPONENT = '1'
Ui-StartApp 'construct-occurrence' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: ' 'the bodies shown'
Ui-Mark
Ui-Step 'activate Frame'                 { Ui-ClickLogged 'Browser radio Root/Frame:1' }
Ui-ExpectNew 'Activated Frame' 'Frame is active'

Write-Host '--- 1. Offset Plane in Frame on Motor:1''s top face, kept in Frame:1'
Ui-Mark
Ui-Step 'offset plane (F7)'              { Ui-Key 'F7' }
Ui-ExpectNew 'Pick face F2.b0/F2:end(' 'Motor''s top face can be picked'
Ui-Step 'pick Motor''s top face'          { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Activate component asked: Motor:1 (active ' 'asked whether to activate Motor:1'
Ui-FocusDialog '^Activate Component$'
Ui-Step 'keep in Frame:1 (Alt+K)'        { Ui-Key 'alt+k' }
Ui-FocusMain
Ui-ExpectNew 'Activate component answer: keep in ' 'kept in Frame'
Ui-ExpectNew 'Offset Plane Plane: 1 face' 'the face picked'
Ui-ExpectNew 'Preview Offset Plane: ok' 'previewed' 30
Ui-ExpectNew 'Preview datum in the design: origin (' 'the preview''s plane logged'
$preview = New-Line 'Preview datum in the design: '
$preview = $preview.Substring($preview.IndexOf('Preview datum in the design: ') + 'Preview datum in the design: '.Length)
Within (Part $preview 'origin') @($any, $any, 20) 1e-6 'the preview 10 mm above the face in the design'
Within (Part $preview 'normal') @(0, 0, 1) 1e-6 'the preview''s normal the face''s'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added plane Plane1: origin (' 'Plane1 added' 30
$added = New-Line 'Added plane Plane1: '
$at = $added.IndexOf('; in the design: ')
if ($at -lt 0) { Ui-Fail "Plane1 is not logged in the design: $added" }
$own = $added.Substring(0, $at)
$own = $own.Substring($own.IndexOf('Added plane Plane1: ') + 'Added plane Plane1: '.Length)
$shown = $added.Substring($at + '; in the design: '.Length)
if ($shown -ne $preview) { Ui-Fail "the preview ($preview) is not where Plane1 is shown ($shown)" }
Write-Host "ok   the preview where Plane1 is shown: $shown"
Within (Part $own 'origin') @($any, 20, $any) 1e-6 'Plane1 20 mm along Frame''s y'
Within (Part $own 'normal') @(0, 1, 0) 1e-6 'Plane1''s normal Frame''s y'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 2. Plane1 edited from the timeline: its preview where it is shown'
Ui-Mark
Ui-Step 'double-click Plane1'            { Ui-DoubleClickLogged 'Timeline Plane1' }
Ui-ExpectNew 'Editing F5 with Offset Plane' 'editing Plane1'
Ui-Step 'type 12'                        { Ui-TypeIn 'Panel Offset Plane input distance' '12' }
Ui-ExpectNew 'Preview Offset Plane: ok' 'previewed' 30
Ui-ExpectNew 'Preview datum in the design: origin (' 'the edited preview''s plane logged'
$edited = New-Line 'Preview datum in the design: '
$was = Part $shown 'origin'
Within (Part $edited 'origin') @($was[0], $was[1], ($was[2] + 2)) 1e-4 'the edited preview 2 mm above Plane1 in the design'
Within (Part $edited 'normal') @(0, 0, 1) 1e-6 'the edited preview''s normal'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Offset Plane cancelled' 'the edit cancelled'

Write-Host '--- 3. Saved: Plane1 linked to Motor''s body; Motor:1 moved 15 mm up'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$local = Part $own 'origin'
$moved = '{0}, {1}, {2}' -f $local[0].ToString('R', $invariant), ($local[1] + 15).ToString('R', $invariant),
  $local[2].ToString('R', $invariant)
$movedScript = Join-Path $work 'moved.json'
Set-Content -Encoding ascii $movedScript (@'
[
  {"expect": {"query": {"query": "feature", "uid": "F5"},
    "result": {"status": "ok", "component": "C2",
               "def": {"links": {"F2.b0": {"source": "O1", "target": "O2"}}}}}},
  {"cmd": "set_occurrence_transform", "occurrence": "O1",
    "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 15]]},
  {"expect": {"query": {"query": "datum", "uid": "F5"},
    "result": {"origin": [MOVED], "normal": [0, 1, 0]}, "tolerance": 1e-4}}
]
'@ -creplace 'MOVED', $moved)
$checked = & $cli run $movedScript --open $design
if ($LASTEXITCODE -ne 0) {
  Ui-Fail "Plane1 is not linked to Motor's body, or does not follow it: $($checked -join ' / ')"
}
Write-Host "ok   Plane1 is Frame's, linked from O1 to O2, and follows Motor:1 to ($moved)"
if (Ui-Crashed) { Ui-Fail 'the application crashed' }

Ui-Finish 'UI Windows construct occurrence test'
