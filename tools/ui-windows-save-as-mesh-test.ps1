# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveAsMesh.cpp app/files/SaveAsMesh.cpp app/files/Print3d.cpp
# check-all sources: app/MainWindowContextMenu.cpp app/browser core/model/src/exchange core/3mf geometry/io tools/cli
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp
# Input-driven Windows UI test of Save As Mesh (mitcad#734) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-save-as-mesh-test.sh does on Linux, on a design that mitcad-cli
# makes: Plate (F1.b0, 40 x 20 x 10, its top the print bed face), Lid
# (F2.b0, 20 x 20 x 5 at x = 60, in its own component Lid:1) and Skin
# (F3.b0, a box with its bottom unstitched: a surface body).
#   1. Plate's browser row: Save As Mesh... is the menu's last entry before
#      Collapse All; 3MF on its print bed face with the proposed name
#      Plate.3mf, turned over and centred on the plate, in millimetres.
#   2. The component Lid:1's row: no print bed face to offer; STL (ASCII) in
#      inches, proposed Lid.stl (the component's name).
#   3. The root's row: Skin left out; OBJ, a file per body, Custom with a
#      1 mm longest edge, into a folder: Plate.obj and Lid.obj.
#   4. Skin's row: Save As Mesh... disabled with the reason.
#   5. File > Export remembers (mitcad#1164): Plate selected, the dialog
#      starts as STEP named Plate in the last folder; 3MF with refinement
#      High chosen and written. Lid selected next: the dialog starts with
#      3MF, High and Lid's name, and Enter writes Lid.3mf.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-save-as-mesh-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-save-as-mesh'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'file.export' = 'F9' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$meshes = Join-Path $work 'meshes'
New-Item -ItemType Directory -Force $meshes | Out-Null
# The app logs paths with forward slashes.
$workLogged = $work -replace '\\', '/'
# The entry's text, its ellipsis one character (kept out of this file's
# bytes: Windows PowerShell reads a file without a BOM as ANSI).
$entry = 'Save as Mesh' + [char]0x2026

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'bench.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [60, 0],
    "length": 20, "width": 20, "height": 5, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 40],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "unstitch", "body": "F3.b0", "faces": ["F3:bottom"]}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Lid"},
  {"cmd": "rename_body", "uid": "F3.b0", "name": "Skin"},
  {"cmd": "set_print_bed_face", "uid": "F1.b0", "face": "F1:top"},
  {"cmd": "components_from_bodies", "bodies": ["F2.b0"]}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Menu-Entries: the entries of the context menu logged last.
function Menu-Entries { return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ') }
# Expect-Last "entry": the browser row menu's last entry before Collapse
# All, which ends every row's menu (mitcad#312).
function Expect-Last([string]$Entry) {
  $entries = Menu-Entries
  if ($entries.Count -lt 2 -or $entries[-2] -ne $Entry -or $entries[-1] -ne 'Collapse All') {
    Ui-Fail "'$Entry' is not the context menu's last entry before Collapse All: $($entries -join ' | ')"
  }
  Write-Host "ok   the context menu ends with $Entry, then Collapse All"
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step 'choose it' {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Row-Menu "Browser Root/...": the context menu of a browser row.
function Row-Menu([string]$Row) {
  Ui-FocusMain
  Ui-Mark
  Ui-Step "right-click $Row"             { Ui-ClickLogged $Row 3 }
  Ui-ExpectNew 'Context menu: ' "the menu of $Row opened"
}
# Save-Dialog: Save as Mesh... chosen, its dialog given the keyboard.
function Save-Dialog {
  Ui-Mark
  Menu-Choose $entry
  Ui-ExpectNew 'Save As Mesh dialog opened: ' 'the dialog opened'
  Ui-FocusDialog '^Save as Mesh$'
}
# Accept-File "title regex": the file dialog's proposed name taken.
function Accept-File([string]$Title) {
  Ui-FocusDialog $Title
  Ui-Key 'Return'
  Ui-FocusMain
}

Ui-StartApp 'save-as-mesh' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Plate at' 'the browser lists Plate'

Write-Host '--- 1. A body on its print bed face, as 3MF'
Row-Menu 'Browser Root/Bodies/Plate'
Expect-Last $entry
Save-Dialog
Ui-ExpectNew 'Save As Mesh dialog opened: bodies Plate; print bed face available' 'Plate with its bed face offered'
Ui-Mark
Ui-Step 'on its print bed face (Alt+B), OK' { Ui-Key 'alt+b', 'Return' }
Ui-ExpectNew 'Save As Mesh dialog: 3mf, mm, one file, medium, on bed' '3MF, millimetres, on the bed'
Ui-Step 'the proposed name (Enter)'      { Accept-File '^Save Mesh$' }
Ui-ExpectNew "Saved as mesh $workLogged/Plate.3mf: 3mf, mm, one file, medium, on bed, 1 body(ies)" 'written under the body''s name' 30
Ui-ExpectNew ('Save As Mesh bed Plate.3mf: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | ' +
  '0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000') 'turned over onto its top, centred on the plate'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $work 'Plate.3mf'))
try {
  $reader = New-Object IO.StreamReader ($zip.GetEntry('3D/3dmodel.model').Open())
  $text = $reader.ReadToEnd()
  $reader.Dispose()
} finally { $zip.Dispose() }
if (-not $text.Contains('<model unit="millimeter"')) { Ui-Fail 'the 3MF file is not in millimetres' }
Write-Host 'ok   the 3MF file is in millimetres'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 2. A component as text STL in inches'
Ui-ExpectLog 'Browser Root/Lid:1 at' 'the browser lists Lid:1'
Row-Menu 'Browser Root/Lid:1'
Expect-Last $entry
Save-Dialog
Ui-ExpectNew 'Save As Mesh dialog opened: bodies Lid; print bed face unavailable (No body has a print bed face)' 'the component''s body, no print bed face'
Ui-Mark
Ui-Step 'STL (ASCII) (Alt+F, Down Down)' { Ui-Key 'alt+f', 'Down', 'Down' }
Ui-Step 'inches (Alt+U, Down x3)'        { Ui-Key 'alt+u', 'Down', 'Down', 'Down' }
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Save As Mesh dialog: stl ascii, in, one file, medium' 'text STL in inches'
Ui-Step 'the proposed name (Enter)'      { Accept-File '^Save Mesh$' }
Ui-ExpectNew "Saved as mesh $workLogged/Lid.stl: stl ascii, in, one file, medium, 1 body(ies)" 'written under the component''s name' 30
$invariant = [Globalization.CultureInfo]::InvariantCulture
$xs = Get-Content (Join-Path $work 'Lid.stl') | ForEach-Object {
  if ($_ -match '^\s*vertex\s+(\S+)') { [double]::Parse($Matches[1], $invariant) } }
$maxX = ($xs | Measure-Object -Maximum).Maximum
if ([Math]::Abs($maxX - 80.0 / 25.4) -gt 1e-4) { Ui-Fail "Lid's largest x in the file is $maxX, expected 80 mm in inches" }
Write-Host 'ok   the numbers are in inches'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 3. The whole design, OBJ, a file per body, a longest edge'
Row-Menu 'Browser Root'
Expect-Last $entry
Save-Dialog
Ui-ExpectNew 'Save As Mesh dialog opened: bodies Plate, Lid; left out Skin' 'both solids, the surface body left out'
Ui-Mark
Ui-Step 'OBJ (Alt+F, Down)'              { Ui-Key 'alt+f', 'Down' }
Ui-Step 'millimetres (Alt+U, Up x3)'     { Ui-Key 'alt+u', 'Up', 'Up', 'Up' }
Ui-Step 'a file per body (Alt+S, Down)'  { Ui-Key 'alt+s', 'Down' }
Ui-Step 'Custom (Alt+R, Down Down)'      { Ui-Key 'alt+r', 'Down', 'Down' }
Ui-Step 'longest edge 1 mm (Alt+E, Up)'  { Ui-Key 'alt+e', 'Up' }
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Save As Mesh dialog: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm' 'OBJ, a file per body, a 1 mm longest edge'
Ui-Step 'the folder' {
  Ui-FocusDialog '^Save Meshes in Folder$'
  Ui-Key 'ctrl+a'
  Ui-Type $meshes
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew "Saved as mesh $workLogged/meshes/Plate.obj: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm, 1 body(ies)" 'Plate''s file named after it' 60
Ui-ExpectNew "Saved as mesh $workLogged/meshes/Lid.obj: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm, 1 body(ies)" 'Lid''s file named after it' 60
$faces = @(Get-Content (Join-Path $meshes 'Plate.obj') | Where-Object { $_.StartsWith('f ') }).Count
if ($faces -le 1000) { Ui-Fail "Plate.obj has $faces triangles: its edges were not split to 1 mm" }
Write-Host "ok   Plate's mesh split to the longest edge ($faces triangles)"
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 4. A surface body: disabled with the reason'
Row-Menu 'Browser Root/Bodies/Skin'
Ui-ExpectNew 'Save As Mesh disabled: Surface and empty bodies are not saved as meshes' 'disabled, the reason given'
Expect-Last $entry
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }

Write-Host '--- 5. File > Export remembers the type and the refinement, names the body (mitcad#1164)'
Ui-FocusMain
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'click Plate''s row'             { Ui-ClickLogged 'Browser Root/Bodies/Plate' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Plate' 'Plate selected'
Ui-Mark
Ui-Step 'export (F9)'                    { Ui-Key 'F9' }
Ui-ExpectNew ", type step-ap214, refinement medium, file $workLogged/meshes/Plate.step" 'STEP at first, the selected body''s name in the last folder'
Ui-Mark
Ui-Step '3MF (Alt+T, Down x7), High (Alt+R, Down), OK' {
  Ui-FocusDialog '^Export$'
  Ui-Key 'alt+t'
  foreach ($i in 1..7) { Ui-Key 'Down' }
  Ui-Key 'alt+r', 'Down', 'Return'
  Ui-FocusMain
}
Ui-ExpectNew "Export dialog: 3MF for 3D printing (*.3mf) to $workLogged/meshes/Plate.3mf, refinement high" '3MF with refinement High, the name''s extension follows the type'
Ui-ExpectNew "Exported $workLogged/meshes/Plate.3mf: 3mf, 1 body" 'Plate written as 3MF' 60
if (-not (Test-Path (Join-Path $meshes 'Plate.3mf'))) { Ui-Fail 'no Plate.3mf' }
Ui-ExpectLog 'Browser Root/Lid:1/Bodies/Lid at' 'the browser lists Lid''s body in Lid:1'
Ui-Mark
Ui-Step 'click Lid''s row'               { Ui-ClickLogged 'Browser Root/Lid:1/Bodies/Lid' }
Ui-ExpectNew 'Browser selected: Root/Lid:1/Bodies/Lid' 'Lid selected'
Ui-Mark
Ui-Step 'export again (F9)'              { Ui-Key 'F9' }
Ui-ExpectNew ", type 3mf, refinement high, file $workLogged/meshes/Lid.3mf" 'the next export starts with 3MF, High and the selected body''s name'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-FocusDialog '^Export$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew "Export dialog: 3MF for 3D printing (*.3mf) to $workLogged/meshes/Lid.3mf, refinement high" 'taken as proposed'
Ui-ExpectNew "Exported $workLogged/meshes/Lid.3mf: 3mf, 1 body" 'Lid written as 3MF' 60
if (-not (Test-Path (Join-Path $meshes 'Lid.3mf'))) { Ui-Fail 'no Lid.3mf' }
Write-Host 'ok   both files written'

Ui-Finish 'UI Windows save as mesh test'
