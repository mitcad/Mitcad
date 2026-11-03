# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialProperties.cpp app/print/MaterialsDialog.cpp app/view/ViewController.cpp
# check-all sources: core/model/src/material.rs core/model/src/material_units.rs core/model/src/material_exchange.rs
# check-all sources: core/model/src/material_dataset.rs core/model/src/document/material_import.rs
# check-all sources: core/model-api/src/materials.rs third_party/material-properties tools/cli
# Input-driven Windows UI test of materials' mechanical and thermal
# properties (mitcad#1213) in the build VM (tools/ui-windows-lib.ps1 says
# how the input is posted), as tools/ui-material-properties-test.sh does on
# Linux, in an inch design of one box with a material of its own, Shop
# steel.
#   1. Edit Materials: Shop steel's Young's modulus typed in psi, the
#      design's unit, shown in psi again and kept in MPa (the saved file).
#   2. New > Browse Material Dataset...: "6061 t6" searched, Aluminum 6061,
#      T6 picked, its values shown in psi, created in This design with its
#      values and source, and saved.
#   3. Import...: a CSV of three materials, one of them not valid: two
#      imported, the invalid row reported with its line and reason.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-material-properties-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-material-properties'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.materials' = 'F9' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'
$design = Join-Path $work 'box.mitcad'

# Run-Cli name script [args]: mitcad-cli run of a script written to
# <work>\<name>.json, failing the test when it fails.
function Run-Cli([string]$Name, [string]$Content, [string[]]$Arguments) {
  $path = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $path $Content
  $made = & $Cli run $path @Arguments
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ($Name): $($made -join ' / ')" }
}

# Saved-Material name: the saved design's material of that name.
function Saved-Material([string]$Name) {
  $file = Get-Content -Raw $design | ConvertFrom-Json
  $found = @($file.materials | Where-Object { $_.name -eq $Name })
  if ($found.Count -ne 1) { Ui-Fail "the saved design has no material $Name" }
  return $found[0]
}

# The design: inches, one box, its own material Shop steel.
$text = @'
[
  {"cmd": "set_units", "length": "in"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-0.5, -0.5],
    "length": 1, "width": 1, "height": 1, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop_steel", "name": "Shop steel", "density": 7.85, "class": "metal"}
]
'@
Run-Cli 'model' $text @('--save', $design)
# A CSV of three materials, the third not valid (a negative yield strength),
# in metric units the file states.
$csv = Join-Path $work 'materials.csv'
Set-Content -Encoding ascii $csv @(
  'name,class,density [g/cm3],yield_strength [MPa],poissons_ratio',
  'Brass CZ121,metal,8.5,300,0.34',
  'Bronze PB1,metal,8.8,250,',
  'Broken,metal,8.5,-3,')

# Click-InWindow "text": a click on a logged place of the window
# Ui-FocusDialog chose. The places are logged in the main window's
# coordinates, as the others are, but a click posted to the main window
# stays in the main window's widgets: this one goes to the dialog, at the
# place moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MaterialsClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InWindow([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object MaterialsClient+Point
  $dialog = New-Object MaterialsClient+Point
  [MaterialsClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [MaterialsClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

Ui-StartApp 'material-properties' @('--open', $design)

Write-Host "--- 1. Shop steel's Young's modulus typed in psi, the inch design's unit"
Ui-Mark
Ui-Step 'Edit Materials (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Materials opened' 'the Materials manager opened'
Ui-ExpectNew 'Materials design item Shop steel at ' 'Shop steel is listed in This design'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Shop steel'                     { Click-InWindow 'Materials design item Shop steel' }
Ui-ExpectNew 'Material selected: Shop steel (design)' 'Shop steel is selected'
Ui-ExpectNew "Materials properties: Shop steel: Young's modulus not given | Poisson's ratio not given | Yield strength not given" 'its values are not given, never 0'
Ui-ExpectNew 'Materials property youngs_modulus at ' "Young's modulus has its field"
Ui-Mark
Ui-Step "Young's modulus 30000000"       {
  Click-InWindow 'Materials property youngs_modulus'
  Start-Sleep -Milliseconds 200
  Ui-Key 'ctrl+a'
  Ui-Type '30000000'
  Ui-Key 'Return'
}
Ui-ExpectNew 'Material Shop steel (design) youngs_modulus = 30000000 psi' 'typed in psi'
Ui-ExpectNew "Materials properties: Shop steel: Young's modulus 30000000 psi | Poisson's ratio not given" 'shown in psi again'

Write-Host '--- 2. Browse Material Dataset...: Aluminum 6061, T6 into This design'
Ui-ExpectLog 'Materials new at ' 'New has its place'
Ui-Mark
Ui-Step 'New'                            { Click-InWindow 'Materials new' }
Ui-ExpectNew 'Materials new menu: Metal | General Plastic | FDM Plastic | Resin | Other | Browse Material Dataset' 'New ends with Browse Material Dataset...'
Ui-Step 'Browse Material Dataset...'     { Ui-Key 'Down', 'Down', 'Down', 'Down', 'Down', 'Down', 'Return' }
Ui-ExpectNew 'Material dataset opened: 119 materials' 'the dataset opened, offline'
Ui-FocusDialog '^Material Dataset$'
Ui-Mark
Ui-Step 'search 6061 t6'                 { Ui-Type '6061 t6' }
Ui-ExpectNew 'Material dataset search: 6061 t6, 1 shown' 'the search finds the material and grade'
Ui-ExpectNew 'Material dataset item Aluminum 6061, T6 at ' 'Aluminum 6061, T6 is listed'
Ui-Mark
Ui-Step 'Aluminum 6061, T6'              { Click-InWindow 'Material dataset item Aluminum 6061, T6' }
Ui-ExpectNew 'Material dataset selected: Aluminum 6061, T6: Density: 0.098 lb/in' 'its density in lb/in3'
Ui-ExpectNew "Young's modulus: 10300000 psi | Poisson's ratio: 0.33 | Yield strength: 35000 psi" 'its values in psi'
Ui-Mark
Ui-Step 'Create (Enter)'                 { Ui-Key 'Return' }
Ui-FocusDialog '^Materials$'
Ui-ExpectNew 'Materials dataset menu: This design' 'Create asks where, This design first'
Ui-Step 'This design'                    { Ui-Key 'Down', 'Return' }
Ui-ExpectNew 'Material created from the dataset: Aluminum 6061, T6 (aluminum/6061/t6) as ' 'made in the design'
Ui-ExpectNew "Materials properties: Aluminum 6061, T6: Young's modulus 10300000 psi | Poisson's ratio 0.33 | Yield strength 35000 psi" 'its values filled in, in psi'
Ui-ExpectNew 'Materials source: material-properties dataset, 6061, T6' 'its source and grade kept'
Ui-FocusMain
Ui-Activate
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
# Kept in the model's units: 30 000 000 psi is 206 842.7 MPa.
$steel = Saved-Material 'Shop steel'
if ([math]::Abs($steel.properties.youngs_modulus - 206842.7188) -gt 0.01) {
  Ui-Fail "Shop steel's modulus is $($steel.properties.youngs_modulus) MPa, not 206842.72"
}
Write-Host "ok   Shop steel's modulus kept in MPa"
$aluminum = Saved-Material 'Aluminum 6061, T6'
if ($aluminum.reference.material -ne '6061' -or $aluminum.reference.grade -ne 'T6' -or $aluminum.class -ne 'metal') {
  Ui-Fail "the dataset's material lacks its source: $($aluminum | ConvertTo-Json -Compress -Depth 5)"
}
if ([math]::Round($aluminum.properties.yield_strength) -ne 241) {
  Ui-Fail "its yield strength is $($aluminum.properties.yield_strength) MPa, not 241"
}
Write-Host "ok   the dataset's material saved with its source, grade and values"

Write-Host '--- 3. Import...: a CSV, one invalid row reported'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Import...'                      { Click-InWindow 'Materials import_materials' }
Ui-Step 'the CSV' {
  Ui-FocusDialog '^Import Materials$'
  Ui-Key 'ctrl+a'
  Ui-Type $csv
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
}
Ui-FocusDialog '^Materials$'
Ui-ExpectNew 'Materials import_materials menu: This design' 'Import asks where'
Ui-Step 'This design'                    { Ui-Key 'Down', 'Return' }
Ui-ExpectNew 'Materials import: materials.csv: added 2, replaced 0, kept 0, 1 not valid' 'two imported, one not'
Ui-ExpectNew 'Materials import row 4 Broken: Yield strength must be greater than zero, got -3' 'the invalid row named with its line and reason'
Ui-ExpectNew 'Brass CZ121 (design)' 'Brass CZ121 is listed in This design'
$rows = @(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Materials row: ') -and $_.Contains(' Broken (design)') })
if ($rows.Count -gt 0) { Ui-Fail "the invalid row was imported: $($rows[0])" }
Write-Host 'ok   the invalid row was not imported'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Ui-Finish 'UI Windows material properties test'
