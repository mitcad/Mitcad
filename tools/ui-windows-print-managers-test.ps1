# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserController.cpp app/MainWindow.cpp app/files/MainWindowPrint.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs
# check-all sources: core/model-api/src/print_profiles.rs tools/cli
# Input-driven Windows UI test of the print managers (mitcad#1135) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-print-managers-test.sh does on Linux, on a design of one box in a
# project. My library is the test's own folder (MITCAD_PRINT_LIBRARY_DIR)
# with a process Bench, the project's library a process Draft, the design
# its own process Mine, and another design of the project uses Draft.
#   1. Printers... and Processes... (shortcuts of the test's) open two
#      windows of their own, both open at once; the Processes manager lists
#      This design, This project, My library and Built-in, each with its
#      count.
#   2. Draft of This project: the editor says it is edited in the project's
#      library, which every design of the project uses; renamed Draft Fine,
#      its file has the name, and the other design of the project sees the
#      change and takes it (mitcad-cli).
#   3. The Materials manager: the built-in PLA Plastic is read only, its
#      Duplicate asks where (This design, This project, My library), My
#      library chosen: its file there.
#   4. Tools has no My Library...
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-print-managers-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-print-managers'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.printers' = 'F8'; 'make.processes' = 'F7'; 'solid.materials' = 'F9' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The user's library of materials and profiles in the test's own folder.
$library = Join-Path $work 'print-library'
$env:MITCAD_PRINT_LIBRARY_DIR = $library
$project = Join-Path $work 'bench-project'
$projectLibrary = Join-Path $project 'print-library'
# The project's folder as the JSON scripts name it.
$projectJson = $project.Replace('\', '/')

# Ui-ExpectItem path field expected description: a field of a library
# item's file (`material.name`, `profile.name`).
function Ui-ExpectItem([string]$Path, [string]$Field, [string]$Expected, [string]$Description) {
  if (-not (Test-Path $Path)) { Ui-Fail "${Description}: no $Path" }
  $value = Get-Content -Raw $Path | ConvertFrom-Json
  foreach ($part in $Field.Split('.')) { $value = $value.$part }
  if ("$value" -ne $Expected) { Ui-Fail "${Description}: $Field is '$value', expected '$Expected'" }
  Write-Host "ok   $Description"
}

# Ui-LoggedAfter "text": the last line since the mark with the text.
function Ui-LoggedAfter([string]$Text) {
  return Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) } | Select-Object -Last 1
}

# Run-Cli name script [args]: mitcad-cli run of a script written to
# <work>\<name>.json, failing the test when it fails.
function Run-Cli([string]$Name, [string]$Content, [string[]]$Arguments) {
  $path = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $path $Content
  $made = & $Cli run $path @Arguments
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ($Name): $($made -join ' / ')" }
}

# The project, My library's Bench and the project's Draft.
$made = & $Cli project init $project --no-history
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli project init: $($made -join ' / ')" }
$text = @"
[
  {"cmd": "create_print_profile", "library": "user", "kind": "process", "name": "Bench",
   "values": {"layer_height": 0.28}},
  {"cmd": "create_print_profile", "library": "project", "project": "$projectJson", "kind": "process", "name": "Draft",
   "values": {"layer_height": 0.2}}
]
"@
Run-Cli 'libraries' $text @('--save', (Join-Path $work 'scratch.mitcad'))
$draft = Join-Path $projectLibrary 'print\processes\draft.json'
if (-not (Test-Path (Join-Path $library 'print\processes\bench.json'))) { Ui-Fail 'Bench is not in My library' }
if (-not (Test-Path $draft)) { Ui-Fail "Draft is not in the project's library" }
# The design: one box and its own process Mine.
$design = Join-Path $project 'box.mitcad'
$text = @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Mine", "choose": true}
]
'@
Run-Cli 'model' $text @('--save', $design)
# Another design of the project that uses Draft (its copy).
$other = Join-Path $project 'other.mitcad'
$text = @"
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "draft",
   "library": "project", "project": "$projectJson"}
]
"@
Run-Cli 'other' $text @('--save', $other)

# Click-InWindow "text": a click on a logged place of the window
# Ui-FocusDialog chose. The places are logged in the main window's
# coordinates, as the others are, but a click posted to the main window
# stays in the main window's widgets: this one goes to the dialog, at the
# place moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ManagerClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InWindow([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object ManagerClient+Point
  $dialog = New-Object ManagerClient+Point
  [ManagerClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [ManagerClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

Ui-StartApp 'print-managers' @('--open', $design)

Write-Host '--- 1. Printers... and Processes...: two managers, the four groups'
Ui-Mark
Ui-Step 'Printers (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Printers opened' 'the Printers manager opened'
Ui-ExpectNew 'Printers groups: This design 0 | This project 0 | My library 0 | Built-in ' 'the four groups, each with its count'
# The Printers manager is the active window now: the main window's
# shortcut works once the main window is the active one again, as a click
# on it would make it (ui_focus_main on Linux). Posted input never
# activates a window.
Ui-FocusMain
Ui-Activate
Ui-Mark
Ui-Step 'Processes (F7)'                 { Ui-Key 'F7' }
Ui-ExpectNew 'Processes opened' 'the Processes manager opened'
Ui-ExpectNew 'Processes groups: This design 1 | This project 1 | My library 1 | Built-in ' 'the four groups with their counts'
Ui-ExpectNew "Processes row: process/mine Mine (design), the design's" "This design: Mine, the design's process"
Ui-ExpectNew 'Processes row: process/draft Draft (project)' 'This project: Draft'
Ui-ExpectNew 'Processes row: process/bench Bench (user)' 'My library: Bench'
Ui-FocusDialog '^Printers$'
Ui-FocusDialog '^Processes$'
Write-Host 'ok   the Printers and the Processes managers are two windows, open at once'

Write-Host "--- 2. This project's Draft: edited in the project's library, the other design takes it"
Ui-ExpectLog 'Processes project item Draft at ' 'Draft has its place'
Ui-Mark
Ui-Step 'Draft'                          { Click-InWindow 'Processes project item Draft' }
Ui-ExpectNew 'Profile selected: process Draft (project)' 'Draft is selected'
Ui-ExpectNew "Processes editing: Editing: this project's library" 'the editor says where it is edited'
Ui-ExpectNew 'every design of the project uses it' 'and whom it reaches'
Ui-ExpectNew 'Processes name at ' 'its name has its place'
Ui-Mark
Ui-Step 'its name'                       { Click-InWindow 'Processes name'; Start-Sleep -Milliseconds 200; Ui-Key 'ctrl+a'; Ui-Type 'Draft Fine'; Ui-Key 'Return' }
Ui-ExpectNew 'Profile Draft (project) name = Draft Fine' 'renamed where it lives'
Ui-ExpectItem $draft 'profile.name' 'Draft Fine' "the project's library has the new name"
$text = @"
[
  {"expect": {"query": {"query": "library_items", "kind": "process", "project": "$projectJson", "by_place": true},
              "result": {"items": [{"place": "design", "id": "draft", "name": "Draft", "changed": true},
                                   {"place": "project", "id": "draft", "name": "Draft Fine"},
                                   {"place": "user", "id": "bench", "name": "Bench"}]}}},
  {"cmd": "update_from_print_library", "kind": "process", "id": "draft", "project": "$projectJson"},
  {"expect": {"query": {"query": "print_profiles"}, "result": [{"id": "draft", "name": "Draft Fine"}]}}
]
"@
Run-Cli 'check' $text @('--open', $other)
Write-Host 'ok   the other design of the project sees Draft Fine and takes it'
Ui-FocusDialog '^Processes$'
Ui-Step 'close Processes (Esc)'          { Ui-Key 'Escape' }
Ui-FocusDialog '^Printers$'
Ui-Step 'close Printers (Esc)'           { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Activate

Write-Host '--- 3. Materials: a built-in one is read only, its Duplicate asks where'
Ui-Mark
Ui-Step 'Edit Materials (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Materials opened' 'the Materials manager opened'
Ui-ExpectNew 'Materials groups: This design 0 | This project 0 | My library 0 | Built-in ' 'the four groups'
Ui-ExpectNew 'Materials search at ' 'the search above the list'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'search PLA Plastic'             { Click-InWindow 'Materials search'; Start-Sleep -Milliseconds 200; Ui-Type 'PLA Plastic' }
Ui-ExpectNew 'Materials mitcad item PLA Plastic at ' "Built-in's PLA has its place"
Ui-Mark
Ui-Step 'PLA Plastic'                    { Click-InWindow 'Materials mitcad item PLA Plastic' }
Ui-ExpectNew 'Material selected: PLA Plastic (mitcad)' 'PLA is selected'
Ui-ExpectNew 'Materials editing: Built-in: read only. Duplicate makes a copy you can edit.' 'read only, the way on named'
Ui-ExpectNew 'Materials header_duplicate at ' 'Duplicate beside Built-in'
Ui-Mark
Ui-Step 'Duplicate'                      { Click-InWindow 'Materials header_duplicate' }
Ui-ExpectNew 'Materials duplicate menu: This design | This project | My library' 'Duplicate asks where, This design first'
Ui-Step 'My library'                     { Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Return' }
Ui-ExpectNew 'Material duplicated: PLA Plastic as ' 'a copy made'
$line = Ui-LoggedAfter 'Material duplicated: PLA Plastic as '
if ($line -notmatch ' as (\S+) in (\S+)$') { Ui-Fail "no id in: $line" }
$id = $Matches[1]
if ($Matches[2] -ne 'user') { Ui-Fail "the copy went to '$($Matches[2])', not My library" }
Ui-ExpectItem (Join-Path $library "materials\$id.json") 'material.name' 'PLA Plastic (copy)' 'its file in My library'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }
Ui-ExpectNew 'Materials closed' 'closed'

Write-Host '--- 4. Tools: no My Library...'
Ui-FocusMain
Ui-Mark
Ui-Step 'the Tools menu (Alt+T)'         { Ui-Key 'alt+t' }
Ui-ExpectNew 'Tools menu: ' 'the Tools menu opened'
$tools = Ui-LoggedAfter 'Tools menu: '
if ($tools.Contains('My Library')) { Ui-Fail "the Tools menu still has My Library: $tools" }
Write-Host "ok   the Tools menu has no My Library"
Ui-Step 'close it (Escape)'              { Ui-Key 'Escape' }

Ui-Finish 'UI Windows print managers test'
