# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/framework/NoteRow.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp core/model core/3mf core/zip tools/cli
# Input-driven Windows UI test of a material's filament preset per slicer
# (mitcad#1062) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), as tools/ui-filament-preset-test.sh does on Linux, on
# a box whose material (Shop PETG HF, made in MitCad) names no slicer
# preset, with a fake slicer (a PowerShell script that writes the
# arguments it was started with) of no known dialect:
#   1. 3D Print (a shortcut of the test's) with 3MF: the Materials row
#      warns that Shop PETG HF names no Bambu Studio filament preset (the
#      design's slicer).
#   2. The row's pencil opens Materials on the body's material; on the
#      Filament tab the search finds the Slicer presets, a row per slicer;
#      Bambu Studio's typed and Enter: one edit, the name kept for Bambu
#      Studio. Closed, the row's warning is gone.
#   3. Send (Enter): the 3MF's project settings name the filament by the
#      preset (filament_settings_id) and inherit from it (inherits_group).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-filament-preset-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-filament-preset'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.print3d' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The user's library of materials and profiles in the test's own folder.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'
$warning = '(warning: Shop PETG HF names no Bambu Studio filament preset'

# The app's temporary folder of the test's own, and a fake slicer in the
# settings.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp
$slicerArgs = Join-Path $script:UiOut 'slicer-args.txt'
Remove-Item -Force $slicerArgs, "$slicerArgs.part" -ErrorAction SilentlyContinue
$slicerScript = Join-Path $script:UiOut 'fake-slicer.ps1'
Set-Content -Encoding ascii $slicerScript (@'
[IO.File]::WriteAllLines('ARGS.part', [string[]]$args)
Move-Item -Force 'ARGS.part' 'ARGS'
'@ -creplace 'ARGS', $slicerArgs)
$powershell = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fake slicer', "slicerProgram=$powershell", 'format=3mf',
  "slicerArguments=-NoProfile, -ExecutionPolicy, Bypass, -File, $($slicerScript -replace '\\', '/')")

# A 20 mm box of Shop PETG HF, a material of the design's own that names
# no slicer preset.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'box.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop-petg-hf", "name": "Shop PETG HF", "based_on": "pla",
   "filament": {"filament_type": "PETG", "temperature": 250}},
  {"cmd": "set_body_material", "uid": "F1.b0", "material": "shop-petg-hf"}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli (the design): $($made -join ' / ')" }

# Click-InDialog "text": a click on a logged place of the Materials window.
# The places are logged in the main window's coordinates, as the others
# are, but a click posted to the main window stays in the main window's
# widgets: this one goes to the window Ui-FocusDialog chose, at the place
# moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PresetClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InDialog([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object PresetClient+Point
  $dialog = New-Object PresetClient+Point
  [PresetClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [PresetClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

# The project settings of the 3MF sent.
function Project-Settings {
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $printTemp 'mitcad-print\box\box.3mf'))
  try {
    $reader = New-Object IO.StreamReader ($zip.GetEntry('Metadata/project_settings.config').Open())
    $text = $reader.ReadToEnd()
    $reader.Dispose()
  } finally { $zip.Dispose() }
  return $text | ConvertFrom-Json
}

Ui-StartApp 'filament-preset' @('--open', $design)

Write-Host '--- 1. 3D Print: the Materials row warns of the material without a preset'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-ExpectNew '; format 3mf, ' 'the panel starts with 3MF'
Ui-ExpectNew $warning 'the row warns that Shop PETG HF names no Bambu Studio preset'
Ui-ExpectNew 'Panel Send to Slicer action materials.0 at ' 'its pencil has its place'

Write-Host '--- 2. Materials'' Filament tab: Bambu Studio''s preset typed'
Ui-PanelReveal 'Panel Send to Slicer input materials'
Ui-Mark
Ui-Step 'the Materials row''s pencil'    { Ui-ClickLogged 'Panel Send to Slicer action materials.0' }
Ui-ExpectNew 'Materials opened for bodies F1.b0' 'Materials opened for the body printed'
Ui-ExpectNew 'Material selected: Shop PETG HF (design)' 'on the body''s material'
Ui-ExpectNew 'Materials settings tab filament at ' 'the print tabs beside General'
Ui-ExpectNew 'Materials close at ' 'Close has its place'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'the Filament tab'               { Click-InDialog 'Materials settings tab filament' }
Ui-ExpectNew 'Materials settings search at ' 'Search settings'
Ui-Mark
Ui-Step 'search preset'                  { Click-InDialog 'Materials settings search'; Ui-Type 'preset' }
Ui-ExpectNew 'Materials settings field filament_settings_id.bambu_studio at ' 'Bambu Studio''s preset row'
Ui-ExpectNew 'Materials settings field filament_settings_id.prusaslicer at ' 'PrusaSlicer''s, a row per slicer'
Ui-Step 'Bambu Studio''s preset'          { Click-InDialog 'Materials settings field filament_settings_id.bambu_studio' }
Ui-Mark
Ui-Step 'type the name, Enter'           { Ui-Key 'ctrl+a'; Ui-Type 'Generic PETG HF'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Shop PETG HF (design) filament_settings_id = Bambu Studio: Generic PETG HF' `
  'Enter saved it, for Bambu Studio'
Ui-Step 'close'                          { Click-InDialog 'Materials close' }
Ui-FocusMain
Ui-ExpectNew 'Panel Send to Slicer note materials: ' 'the Materials row again'
$last = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.Contains('Panel Send to Slicer note materials: ') } | Select-Object -Last 1
if ("$last".Contains('(warning:')) { Ui-Fail "the row still warns: $last" }
Write-Host 'ok   the row warns no more'

Write-Host '--- 3. Send: the 3MF names the filament by the preset and inherits from it'
Remove-Item -Force $slicerArgs -ErrorAction SilentlyContinue
Ui-Mark
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew '3D Print: print settings for bambu_studio, filaments 1' 'Bambu Studio''s settings, one filament' 30
for ($i = 0; $i -lt 100 -and -not (Test-Path $slicerArgs); $i++) { Start-Sleep -Milliseconds 200 }
if (-not (Test-Path $slicerArgs)) { Ui-Fail 'the slicer was not started' }
Write-Host 'ok   the slicer got the 3MF'
$settings = Project-Settings
$ids = @($settings.filament_settings_id) -join '|'
if ($ids -ne 'Generic PETG HF') { Ui-Fail "filament_settings_id: '$ids', expected 'Generic PETG HF'" }
Write-Host 'ok   the filament is named by the preset'
$inherits = @($settings.inherits_group) -join '|'
if ($inherits -ne '|Generic PETG HF|') { Ui-Fail "inherits_group: '$inherits', expected '|Generic PETG HF|'" }
Write-Host 'ok   the filament inherits from it'

Ui-Finish 'UI Windows filament preset test'
