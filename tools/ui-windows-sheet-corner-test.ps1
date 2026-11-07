# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp core/model/src/sheet core/model/src/features/sheet_folded.rs tools/cli
# The Flange panel's Corner (mitcad#1410), the steps of
# tools/ui-sheet-flange-corner-test.sh: a 1 mm sheet, a 100 x 10 base with
# Outside flanges of Inner Length 10 on both long edges and a third
# closing the 10 x 10 section with an open corner (mitcad-cli, as
# cli.sheet_flange_corner):
#   1. The closing flange edited from the timeline loads its Corner: Open
#      hides Measured (a corner's length is the inside dimension), Hem is
#      hidden; OK keeps the body, 3400 + 225 pi mm3.
#   2. Corner: Half Open in the panel: the wall 0.5 mm over the first
#      wall's end, which the 0.00001 mm seam cuts back, 4156.857 mm3.
#   3. Saved and read back by mitcad-cli: `corner` half_open, length mode
#      inner, the body valid with its volume.
#   4. Corner: None shows Measured again; Esc leaves the flange as it is.
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-sheet-test.ps1 does.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-corner'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$workLogged = $work -replace '\\', '/'
$pi = [Math]::PI

$model = Join-Path $work 'model.json'
$design = Join-Path $work 'corner.mitcad'
Set-Content -Encoding ascii $model @'
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":1,"bend_radius":1,"allowance":{"type":"k_factor","k":0.44},"reference_side":"front","bend_relief":{"type":"round","width":1,"depth":2},"corner_relief":{"type":"round","width":1,"depth":2}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[0,0],"width":100,"height":10},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}]}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c1[c4,c2]"}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside"}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c3[c2,c4]"}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside"}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F5","slot":0},"curve":{"type":"generated","feature":"F5","slot":51}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside","corner":"open"}},
  {"expect":{"feature":"F6","status":"ok","body":"F3.b0","volume":4106.8583470577035}}
]
'@
$made = & $Cli run $model --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Whether the last layout of the Flange panel shows an input.
function Panel-Shown([string]$Name) {
  $line = Ui-LastLineWith "Panel Flange input $Name at"
  return ($null -ne $line) -and -not $line.Contains('(hidden)')
}

function Edit-Closing {
  Ui-Mark
  Ui-Step 'edit the closing flange (double-click)' { Ui-DoubleClickLogged 'Timeline SheetFlange3' }
  Ui-ExpectNew 'Editing F6 with Flange' 'the closing flange opens for editing'
  Ui-ExpectNew 'Panel Flange input corner at' 'the panel is laid out'
  Ui-Sync
}

Write-Host '--- The open corner loads and is kept'
Ui-StartApp 'sheet-corner' @('--open', $design)
Ui-ExpectLog "Opened $workLogged/corner.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-ExpectLog 'Timeline SheetFlange3 at' 'the closing flange is in the timeline'
Edit-Closing
if (-not (Panel-Shown 'corner')) { Ui-Fail 'Corner is hidden' }
if (Panel-Shown 'length_mode') { Ui-Fail 'Measured is shown: the flange did not load its corner' }
if (Panel-Shown 'hem') { Ui-Fail 'Hem is shown beside a corner' }
Write-Host 'ok   Corner: Open loaded, Measured and Hem hidden'
Ui-Mark
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F6' 'the open corner kept' 30
Ui-Sync
if ((Ui-NewLines) -match 'Body \S+ \(F3\.b0\): volume') { Ui-Fail 'the body changed: the panel did not keep the open corner' }
Write-Host 'ok   the open corner kept'

Write-Host '--- Corner: Half Open'
Edit-Closing
Ui-Mark
Ui-Step 'Corner: Half Open'                 { Ui-WheelLogged 'Panel Flange input corner' -1 }
Ui-ExpectNew 'Preview Flange: ok' 'the half open corner previews' 30
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F6' 'the half open corner made' 30
Ui-ExpectVolume 'Body \S+ \(F3\.b0\): volume [0-9.]+ -> ([0-9.]+) mm3' (3450 + 225 * $pi - 0.001) 'the closing wall 0.5 mm over the first wall, the seam cut back'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                     { Ui-Key 'ctrl+s' }
Ui-ExpectNew "Saved $workLogged/corner.mitcad" 'saved'

Write-Host '--- The saved corner read back'
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect":{"query":{"query":"feature","uid":"F6"},"result":{"status":"ok","def":{"corner":"half_open","length":{"mode":"inner"},"position":"outside"}}}},
  {"expect":{"body":"F3.b0","volume":4156.857347057704}},
  {"expect":{"query":{"query":"sheet_state","body":"F3.b0"},"result":{"status":"valid"}}}
]
'@
$read = & $Cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved corner: $($read -join ' / ')" }
Write-Host 'ok   the saved half open corner'

Write-Host '--- The half open corner loads; Corner: None shows Measured'
Edit-Closing
if (Panel-Shown 'length_mode') { Ui-Fail 'Measured is shown: the half open corner did not load' }
Ui-Step 'Corner: Open'                      { Ui-WheelLogged 'Panel Flange input corner' 1 }
Ui-Sync
Ui-Step 'Corner: None'                      { Ui-WheelLogged 'Panel Flange input corner' 1 }
Ui-Sync
if (-not (Panel-Shown 'length_mode')) { Ui-Fail 'Measured is hidden without a corner' }
if (-not (Panel-Shown 'hem')) { Ui-Fail 'Hem is hidden without a corner' }
Write-Host 'ok   Corner: None shows Measured and Hem'
Ui-Mark
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Flange cancelled' 'the edit cancelled'

Ui-Finish 'Windows sheet flange corner UI'
