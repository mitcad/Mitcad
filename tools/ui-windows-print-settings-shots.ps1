# SPDX-License-Identifier: MIT
# Screenshots of the print settings UI (mitcad#655) on the light theme, for a
# reviewer to judge. Not a test: a shot fails only when Mitcad does not get
# through the steps that lead to it. Runs mitcad.exe with Mesa's software
# OpenGL (MITCAD_MESA_DIR) in SSH's session 0 with the in-process test
# driver (MITCAD_TEST_INPUT, app/framework/TestDriver.hpp), as
# tools/ui-windows-window-shots.ps1 does: `screenshot` grabs the main window
# with the 3D view composed in, `screenshot-dialog` the window that
# focus-dialog chose; neither has the native title bar. Every shot follows
# an expect-log that proves its window is there, so a missing window fails
# loudly instead of giving an empty picture.
#
# mitcad-cli makes the data first: a design of three boxes with a printer
# of its own (Bench X 0.4 nozzle), the process Shop Fine 0.12 and the
# filament materials Mount PLA (Body2, Body3) and Shop PETG (Body1, which
# also sets its own Walls); the process and Shop PETG saved to the user's
# print library of the run's own (MITCAD_PRINT_LIBRARY_DIR), and Shop PETG
# then changed there from another design, which the window asks about; and
# a Bambu Studio project (3MF) of another printer and process to import.
#
# Each group of shots is a run of its own (its own settings with 3MF as the
# format, projects and autosave folders, a copy of the design), so that a
# failed step costs only that run's shots. Floating chrome unless named:
#   01-3dprint-panel.png        3D Print (3MF) after Import from slicer...
#                               of the project: the Printer and Process
#                               rows, the line beside the button, Details...
#   02-import-details.png       Details...: the Import Details window
#   03-process-quality.png      Edit processes..., the Process picker's
#                               last row (mitcad#1134): the Processes
#                               manager on the process, its first tab
#                               (Quality)
#   04-process-strength.png     the Strength tab (the tab bar from the
#                               search field by Tab, then Right)
#   05-process-advanced.png     the Strength tab with Advanced on
#   06-process-search.png       the search 'jerk'
#   07-printer-editor.png       Edit printers... in the Printer picker:
#                               the Printers manager on the printer
#   08-materials.png            Edit Materials, Mount PLA selected in its
#                               list, its 3D printing section open
#   09-material-tool.png        The Material tool with Body2 and Body3
#                               picked (mitcad#1059): the panel's picker
#                               button and what the bodies are made of
#   10-property-sets-print.png  Property Sets of Body1 scrolled to its end:
#                               Print settings (effective)
#   11-changed-elsewhere.png    Edit Materials, Shop PETG selected: Shop
#                               PETG, asked about once: "Shop PETG was
#                               changed in another design."
#   12-3dprint-docked.png       3D Print (3MF) in the Docked chrome
#   13-printers-processes.png   Processes... (MAKE), opened
#                               on the design's process Shop Fine 0.12
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-print-settings-shots.ps1 -App <mitcad.exe> [-Cli <mitcad-cli.exe>] -Out <dir> [-QtBin dir] [-Scale factor]
# -Cli defaults to the build's tools\cli\mitcad-cli.exe; Qt's DLLs are not
# copied next to the app, -QtBin defaults to the Qt of the build directory's
# CMakeCache.txt. -Scale is Qt's scale factor (QT_SCALE_FACTOR, 0.75 by
# default), as in tools/ui-windows-window-shots.ps1: session 0's screen is
# 1024 x 768, too small for the window's 1280 x 800 logical pixels, which a
# scale of 0.75 makes room for; the images are 0.75 of that in pixels.
#
# Not reachable from here: a context menu (the driver's right click is a
# plain mouse event, which opens none), so Material... of the Browser's
# menu is reached as Edit Materials with the bodies selected, which opens
# the same window for them.
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-print-settings-shots'),
  [string]$QtBin = '',
  [double]$Scale = 0.75)

$ErrorActionPreference = 'Stop'
$App = (Resolve-Path $App).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', 'tools\cli\mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $App) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) {
  Write-Host "FAIL: no mitcad-cli.exe beside $App; pass -Cli"
  exit 1
}
$Cli = (Resolve-Path $Cli).Path
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

$shots = @('01-3dprint-panel.png', '02-import-details.png', '03-process-quality.png', '04-process-strength.png',
           '05-process-advanced.png', '06-process-search.png', '07-printer-editor.png', '08-materials.png',
           '09-material-tool.png', '10-property-sets-print.png', '11-changed-elsewhere.png', '12-3dprint-docked.png',
           '13-printers-processes.png')
foreach ($shot in $shots) { Remove-Item -Force (Join-Path $Out $shot) -ErrorAction SilentlyContinue }
$saved = New-Object System.Collections.Generic.List[string]
$failed = New-Object System.Collections.Generic.List[string]

# --- The data, made with mitcad-cli

$work = Join-Path $Out 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The user's library of materials and profiles in the run's own folder, for
# mitcad-cli and the app alike.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'
New-Item -ItemType Directory -Force $env:MITCAD_PRINT_LIBRARY_DIR | Out-Null

# Invoke-Cli name script arguments: `mitcad-cli run` of a JSON script;
# whether it succeeded.
function Invoke-Cli([string]$Name, [string]$Json, [string[]]$Arguments) {
  $file = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $file $Json
  $output = & $Cli run $file @Arguments
  if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: mitcad-cli ($Name), exit code ${LASTEXITCODE}:"
    $output | ForEach-Object { Write-Host "     $_" }
    return $false
  }
  return $true
}

Write-Host '--- The data'
# The design: three boxes, a printer of its own, the process, two filament
# materials. Body1 is Shop PETG, Body2 and Body3 Mount PLA.
$base = Join-Path $work 'base.mitcad'
$designJson = @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-50, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-15, -15],
    "length": 30, "width": 30, "height": 12, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -8],
    "length": 16, "width": 16, "height": 32, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "id": "bench-x", "name": "Bench X 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]], "max_print_height": 250,
              "nozzle_diameter": 0.4, "printer_settings_id": {"orcaslicer": "Bench X 0.4 nozzle"}}},
  {"cmd": "create_print_profile", "kind": "process", "id": "shop-fine", "name": "Shop Fine 0.12",
   "values": {"layer_height": 0.12, "first_layer_height": 0.2, "seam_position": "aligned",
              "perimeters": 3, "top_solid_layers": 5, "bottom_solid_layers": 4,
              "fill_density": 20, "fill_pattern": "grid", "external_perimeter_speed": 60,
              "brim_width": 3}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile", "value": "bench-x"},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "shop-fine"},
  {"cmd": "create_material", "id": "mount-pla", "name": "Mount PLA", "based_on": "pla",
   "filament": {"temperature": 210, "bed_temperature": 60}},
  {"cmd": "create_material", "id": "shop-petg", "name": "Shop PETG", "based_on": "pla", "density": 1.27,
   "filament": {"filament_type": "PETG", "temperature": 240, "bed_temperature": 80}},
  {"cmd": "set_body_material", "uid": "F1.b0", "material": "shop-petg"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "mount-pla"},
  {"cmd": "set_body_material", "uid": "F3.b0", "material": "mount-pla"}
]
'@
$made = Invoke-Cli 'design' $designJson @('--save', $base)
if (-not $made -or -not (Test-Path $base)) {
  Write-Host 'FAIL: the design could not be made; no screenshots'
  exit 1
}
Write-Host 'ok   the design'
$design = $base

# Body1's own Walls, which Property Sets shows as the body's (10).
$own = Join-Path $work 'own.mitcad'
$ownJson = @'
[
  {"cmd": "set_property", "owner": "F1.b0", "set": "Print Process", "key": "perimeters", "value": 4}
]
'@
if (Invoke-Cli 'own' $ownJson @('--open', $design, '--save', $own)) {
  $design = $own
  Write-Host "ok   Body1's own Walls"
} else {
  Write-Host "     going on without Body1's own Walls"
}

# The process and Shop PETG into the user's library (version 1); the
# design's copies name them as their origin.
$library = Join-Path $work 'library.mitcad'
$libraryJson = @'
[
  {"cmd": "save_to_print_library", "library": "user", "kind": "process", "id": "shop-fine"},
  {"cmd": "save_to_print_library", "library": "user", "kind": "material", "id": "shop-petg"}
]
'@
# Shop PETG edited in the library from another design: version 2 there,
# the design's copy stays at 1, and the window asks about it (11).
$newerJson = @'
[
  {"cmd": "edit_material", "library": "user", "id": "shop-petg", "filament": {"temperature": 245}}
]
'@
if (Invoke-Cli 'library' $libraryJson @('--open', $design, '--save', $library)) {
  $design = $library
  Write-Host 'ok   the process and Shop PETG in the user library'
  if (Invoke-Cli 'newer' $newerJson @('--save', (Join-Path $work 'newer.mitcad'))) {
    Write-Host 'ok   Shop PETG changed in the user library'
  } else {
    Write-Host '     going on without the change: 11-changed-elsewhere.png will fail'
  }
} else {
  Write-Host '     going on without the user library: 11-changed-elsewhere.png will fail'
}

# The slicer's project to import (01, 02): a Bambu Studio 3MF of two boxes
# of two materials, with a printer and a process of its own (its own
# names, so that nothing of it meets the design's).
$project = Join-Path $work 'plate.3mf'
$projectJson = @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Bench Mini 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [180, 0], [180, 180], [0, 180]], "max_print_height": 180,
              "nozzle_diameter": 0.4, "printer_settings_id": {"bambu_studio": "Bench Mini 0.4 nozzle"}}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Extra Fine", "values": {"layer_height": 0.08}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bench-mini-0-4-nozzle"},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "extra-fine"},
  {"cmd": "create_material", "id": "plate-pla", "name": "Plate PLA", "based_on": "pla",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "create_material", "id": "plate-abs", "name": "Plate ABS", "based_on": "abs",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "set_component_material", "component": "C0", "material": "plate-pla"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "plate-abs"},
  {"cmd": "export", "path": "PROJECT", "dialect": "bambu_studio"}
]
'@
$projectJson = $projectJson -creplace 'PROJECT', ($project -replace '\\', '/')
$null = Invoke-Cli 'plate' $projectJson @('--save', (Join-Path $work 'plate.mitcad'))
if (Test-Path $project) { Write-Host 'ok   the Bambu Studio project' }
else { Write-Host '     no project to import: 01 and 02 will fail' }

# --- The runs

$runs = Join-Path $Out 'runs'

# Invoke-Mitcad name arguments environment: runs the app until it exits;
# its log (Qt messages go to stderr with QT_FORCE_STDERR_LOGGING) and exit
# code.
function Invoke-Mitcad([string]$Name, [string]$Dir, [string[]]$Arguments, [hashtable]$Environment) {
  $info = New-Object System.Diagnostics.ProcessStartInfo $App
  $info.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.EnvironmentVariables['QT_FORCE_STDERR_LOGGING'] = '1'
  # Folders of the run's own, not the user's.
  $info.EnvironmentVariables['MITCAD_AUTOSAVE_DIR'] = Join-Path $Dir 'autosave'
  $info.EnvironmentVariables['MITCAD_CRASH_DIR'] = Join-Path $Dir 'crashes'
  $info.EnvironmentVariables['MITCAD_RESULT_STORE'] = 'off'
  $info.EnvironmentVariables['MITCAD_NO_UPDATE_CHECK'] = '1'
  foreach ($variable in $Environment.GetEnumerator()) {
    $info.EnvironmentVariables[$variable.Key] = $variable.Value
  }
  $process = [System.Diagnostics.Process]::Start($info)
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  $timedOut = $false
  if (-not $process.WaitForExit(240000)) {
    $process.Kill()
    $timedOut = $true
  }
  $process.WaitForExit()
  $log = $stdout.Result + $stderr.Result
  Set-Content -Encoding utf8 (Join-Path $Dir "$Name.log") $log
  return @{ Log = $log; ExitCode = $process.ExitCode; TimedOut = $timedOut }
}

# Invoke-Shots name chrome shots parts: one run of the app with the design
# and the driver script (its parts one after the other); which of its shots
# were saved, and for the others the step that failed.
function Invoke-Shots([string]$Name, [string]$Chrome, [string[]]$Shots, [string[]]$Parts) {
  Write-Host "--- $Name ($($Shots -join ', '))"
  $dir = Join-Path $runs $Name
  Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $dir 'settings\Mitcad') | Out-Null
  New-Item -ItemType Directory -Force (Join-Path $dir 'projects') | Out-Null
  # 3D Print opens with 3MF (its Printer and Process rows); Advanced off,
  # the Materials window's 3D printing section open.
  Set-Content -Encoding ascii (Join-Path $dir 'settings\Mitcad\Mitcad.ini') @(
    '[print]', 'format=3mf', 'advancedSettings=false')
  # A copy of the design of the run's own: nothing of one run (a lock, an
  # autosave) reaches the next.
  $copy = Join-Path $dir 'design.mitcad'
  Copy-Item $design $copy
  $driverScript = Join-Path $dir "$Name.script"
  Set-Content -Encoding ascii $driverScript ($Parts -join "`r`n")
  $result = Invoke-Mitcad $Name $dir @('--no-recovery', '--no-native-dialogs', '--no-mcp', "--chrome=$Chrome", '--open', $copy) @{
    'MITCAD_TEST_INPUT' = $driverScript; 'MITCAD_TEST_COLOR_SCHEME' = 'light'
    'MITCAD_SETTINGS_DIR' = (Join-Path $dir 'settings'); 'MITCAD_PROJECTS_DIR' = (Join-Path $dir 'projects')
    'MITCAD_SHOTS_DIR' = $Out; 'MITCAD_SHOTS_PROJECT' = $project
    'QT_SCALE_FACTOR' = $Scale.ToString([Globalization.CultureInfo]::InvariantCulture) }
  # Why the run stopped: the driver's failed step, with its script line.
  $why = ''
  $fail = $result.Log -split "`r?`n" | Where-Object { $_ -match 'TestDriver: FAIL line ([0-9]+): ' } | Select-Object -Last 1
  if ($fail) {
    $number = [int]([regex]::Match($fail, 'FAIL line ([0-9]+)').Groups[1].Value)
    $lines = Get-Content $driverScript
    $step = if ($number -ge 1 -and $number -le $lines.Count) { $lines[$number - 1].Trim() } else { '' }
    $why = ($fail -replace '^.*TestDriver: ', '') + $(if ($step) { " [$step]" } else { '' })
  } elseif ($result.TimedOut) {
    $why = 'Mitcad did not exit within 240 s'
  } elseif ($result.ExitCode -ne 0) {
    $why = "exit code $($result.ExitCode)"
  }
  foreach ($shot in $Shots) {
    if (Test-Path (Join-Path $Out $shot)) {
      Write-Host "ok   $shot"
      $script:saved.Add($shot)
    } else {
      $reason = if ($why) { $why } else { 'no file, though the run ended without a failed step' }
      Write-Host "FAIL: $shot ($reason); the log: $(Join-Path $dir "$Name.log")"
      $script:failed.Add("$shot ($Name): $reason")
    }
  }
}

# Each script starts once the window shows the light theme and the
# design's bodies in the Browser.
$start = @'
expect-ever 'Asking the platform for the light colour scheme' 30
expect-ever 'Browser Root/Bodies/Body1 at' 60
expect-ever 'Browser Root/Bodies/Body3 at' 30
sleep 1500
'@

# 01, 02: 3D Print (3MF from the settings), Import from slicer... through
# Qt's own file dialog (--no-native-dialogs): the path typed in its name
# field and Open clicked (Return could go to the field's completer), then
# Details....
$panel = @'
mark
command make.print3d
expect-log '3D Print panel: .*format 3mf' 10
expect-log 'Panel Send to Slicer action import\.0 at' 10
mark
click-logged 'Panel Send to Slicer action import\.0'
focus-dialog '^Import from Slicer$' 20
focus-widget fileNameEdit
key ctrl+a
type '${MITCAD_SHOTS_PROJECT}'
button '^Open$'
focus-main
expect-log '3D Print import: plate.3mf imported' 60
expect-log 'Panel Send to Slicer action import.1 at' 10
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\01-3dprint-panel.png'
mark
click-logged 'Panel Send to Slicer action import.1'
expect-log 'Import details opened: ' 10
focus-dialog '^Import Details$'
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\02-import-details.png'
key Escape
expect-log 'Import details closed' 10
'@
Invoke-Shots 'panel' 'floating' @('01-3dprint-panel.png', '02-import-details.png') @($start, $panel)

# The Processes manager on the process (mitcad#1135): Edit processes...,
# the Process picker's last row (no pencil beside the row, mitcad#1134).
$openProcess = @'
mark
command make.print3d
expect-log 'Panel Send to Slicer input process at' 10
click-logged 'Panel Send to Slicer input process'
expect-log 'Process picker opened: ' 10
focus-dialog '^Process Picker$'
type 'Edit processes'
expect-log 'Process picker search: Edit processes, first Edit processes' 10
key Return
expect-log 'Process picker chose: Edit processes' 10
expect-log 'Processes opened' 10
expect-log 'Profile selected: process Shop Fine 0\.12' 10
focus-dialog '^Processes$'
expect-log 'Processes settings search at' 10
expect-log 'Processes settings tab quality at' 10
sleep 1000
'@

# 03, 06: its first tab, then the search.
$process = @'
screenshot-dialog '${MITCAD_SHOTS_DIR}\03-process-quality.png'
mark
focus-widget settingsSearch
type jerk
expect-log 'Processes settings field default_jerk at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\06-process-search.png'
'@
Invoke-Shots 'process' 'floating' @('03-process-quality.png', '06-process-search.png') @($start, $openProcess, $process)

# 04, 05: the Strength tab, the tab bar reached from the search field by
# Tab (past Advanced) and Right; then Advanced on.
$strength = @'
focus-widget settingsSearch
mark
key Tab
key Tab
key Right
expect-log 'Processes settings field perimeters at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\04-process-strength.png'
mark
button '^Advanced$'
expect-setting print/advancedSettings true
expect-log 'Processes settings field \S+ at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\05-process-advanced.png'
'@
Invoke-Shots 'process-strength' 'floating' @('04-process-strength.png', '05-process-advanced.png') @($start, $openProcess, $strength)

# 07: Edit printers... in the Printer picker (mitcad#1134, mitcad#1135).
$printer = @'
mark
command make.print3d
expect-log 'Panel Send to Slicer input printer at' 10
click-logged 'Panel Send to Slicer input printer'
expect-log 'Printer picker opened: ' 10
focus-dialog '^Printer Picker$'
type 'Edit printers'
expect-log 'Printer picker search: Edit printers, first Edit printers' 10
key Return
expect-log 'Printer picker chose: Edit printers' 10
expect-log 'Printers opened' 10
expect-log 'Profile selected: printer Bench X 0\.4 nozzle' 10
focus-dialog '^Printers$'
expect-log 'Printers settings search at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\07-printer-editor.png'
'@
Invoke-Shots 'printer' 'floating' @('07-printer-editor.png') @($start, $printer)

# 08: Edit Materials, Mount PLA chosen in its list (the window assigns
# nothing since mitcad#1059), the print settings' first tab.
$materials = @'
mark
command solid.materials
expect-log '^Materials opened$' 10
expect-log 'Materials item Mount PLA at' 10
focus-dialog '^Materials$'
mark
dialog-click-logged 'Materials item Mount PLA at'
expect-log 'Material selected: Mount PLA \(design\)' 10
expect-log 'Material Mount PLA value ' 10
expect-log 'Materials settings tab filament at' 10
mark
focus-widget settingsTabs
key Right
expect-log 'Materials settings search at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\08-materials.png'
'@
Invoke-Shots 'materials' 'floating' @('08-materials.png') @($start, $materials)

# 09: Body2 and Body3 (Ctrl+click) selected, the Material tool for them
# (mitcad#1059): the picker button and what they are made of.
$tool = @'
mark
click-logged 'Browser Root/Bodies/Body2 at'
expect-log 'Selected: 1 bod' 10
mark
click-logged 'Browser Root/Bodies/Body3 at' left ctrl
expect-log 'Selected: 2 bod' 10
mark
command solid.material
expect-log 'Panel Material bodies note: ' 10
expect-log 'Panel Material input material at' 10
sleep 1000
screenshot-screen '${MITCAD_SHOTS_DIR}\09-material-tool.png'
key Escape
'@
Invoke-Shots 'material-tool' 'floating' @('09-material-tool.png') @($start, $tool)

# 10: Property Sets of Body1, made taller and taken to its last row (End),
# where the print settings are; a print row's cell in sight proves it.
$propertySets = @'
mark
click-logged 'Browser Root/Bodies/Body1 at'
expect-log 'Selected: 1 bod' 10
mark
command view.property_sets
expect-log 'Property Sets opened for F1\.b0' 10
expect-log 'Property sets row: print Print Process/' 10
focus-dialog '^Property Sets$'
dialog-resize 960 860
sleep 500
key End
sleep 1000
expect-log 'Property sets print Print (Printer|Process|Filament)/\S+ value at' 10
screenshot-dialog '${MITCAD_SHOTS_DIR}\10-property-sets-print.png'
'@
Invoke-Shots 'property-sets' 'floating' @('10-property-sets-print.png') @($start, $propertySets)

# 11: Edit Materials, Shop PETG (Body1's) chosen in its list: another design
# changed it in My library, asked about once.
$changed = @'
mark
command solid.materials
expect-log '^Materials opened$' 10
expect-log 'Materials item Shop PETG at' 10
focus-dialog '^Materials$'
dialog-click-logged 'Materials item Shop PETG at'
expect-log 'Material selected: Shop PETG \(user\)' 10
expect-log 'Materials changed elsewhere: Shop PETG' 10
focus-dialog '^Changed in Another Design$'
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\11-changed-elsewhere.png'
'@
Invoke-Shots 'changed-elsewhere' 'floating' @('11-changed-elsewhere.png') @($start, $changed)

# 12: 3D Print (3MF) in the Docked chrome.
$docked = @'
mark
command make.print3d
expect-log '3D Print panel: .*format 3mf' 10
expect-log 'Panel Send to Slicer action import\.0 at' 10
expect-log 'Panel Send to Slicer input process at' 10
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\12-3dprint-docked.png'
'@
Invoke-Shots 'docked' 'docked' @('12-3dprint-docked.png') @($start, $docked)

# 13: Processes... from its own command (MAKE, mitcad#1135): the manager
# opens on the design's process.
$profiles = @'
mark
command make.processes
expect-log 'Processes opened' 10
expect-log 'Profile selected: process Shop Fine 0\.12' 10
focus-dialog '^Processes$'
expect-log 'Processes settings tab quality at' 10
sleep 1000
screenshot-dialog '${MITCAD_SHOTS_DIR}\13-printers-processes.png'
'@
Invoke-Shots 'profiles' 'floating' @('13-printers-processes.png') @($start, $profiles)

Write-Host '--- Summary'
Write-Host "Saved ($($saved.Count) of $($shots.Count)):"
foreach ($shot in $saved) { Write-Host "  $(Join-Path $Out $shot)" }
if ($failed.Count -gt 0) {
  Write-Host "Failed ($($failed.Count)):"
  foreach ($shot in $failed) { Write-Host "  $shot" }
}
Write-Host "Screenshots: $Out; logs and driver scripts: $runs"
if ($failed.Count -gt 0) { Write-Host 'FAIL: print settings screenshots'; exit 1 }
Write-Host 'PASS: print settings screenshots'
