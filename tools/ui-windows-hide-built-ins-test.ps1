# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/Dialogs.cpp core/model/src/print_library.rs core/model/src/document/library_items.rs
# check-all sources: tools/cli tools/libraries/print-library
# Input-driven Windows UI test of hidden built-ins (mitcad#1270) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-hide-built-ins-test.sh does on Linux. The base library is a
# local one of the test's own (MITCAD_LIBRARIES_DIR, recorded with git)
# with two of MitCad's built-in printers, Bambu Lab A1 (0.4 nozzle) and
# Bambu Lab A1 mini (0.4 nozzle), from tools\libraries\print-library; My
# library is the test's own folder (MITCAD_PRINT_LIBRARY_DIR); the hidden
# ids are a setting of the test's own settings (MITCAD_SETTINGS_DIR).
#   1. The Printers manager on a box: Bambu Lab A1 selected, Hide: gone
#      from the list (Built-in 1), the setting names it.
#   2. Send to Slicer: the Printer box lists Bambu Lab A1 mini, not it.
#   3. Show hidden lists it grayed; its row's context menu Unhide restores
#      it; the context menu's Hide hides it again.
#   4. A design that uses it (its copy, made with mitcad-cli) still shows
#      it as This design's printer, and its Printer box lists it.
#   5. With a printer in My library the Printers manager opens with
#      Built-in folded.
# Skipped where git is not installed (the base library cannot be fetched).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-hide-built-ins-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-hide-built-ins'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.printers' = 'F8'; 'make.print3d' = 'F7' } -StepPause 0
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; the base library cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows hide built-ins test (skipped)'
  exit 0
}
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
# The user's library and the fetched libraries in the test's own folders.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'libraries'
New-Item -ItemType Directory -Force $env:MITCAD_PRINT_LIBRARY_DIR, $env:MITCAD_LIBRARIES_DIR | Out-Null
$settings = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
# 3MF, whose Printer row is enabled.
Add-Content -Encoding ascii $settings @('[print]', 'format=3mf')
$a1 = 'Bambu Lab A1 (0.4 nozzle)'
$mini = 'Bambu Lab A1 mini (0.4 nozzle)'

# Run-Cli name script [args]: mitcad-cli run of a script written to
# <work>\<name>.json, failing the test when it fails.
function Run-Cli([string]$Name, [string]$Content, [string[]]$Arguments) {
  $path = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $path $Content
  $made = & $Cli run $path @Arguments
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ($Name): $($made -join ' / ')" }
}

# Hidden-Setting: the setting's hidden printers ('' without any).
function Hidden-Setting {
  if (-not (Test-Path $settings)) { return '' }
  $inGroup = $false
  foreach ($line in Get-Content $settings) {
    if ($line -match '^\[(.*)\]$') { $inGroup = $Matches[1] -eq 'printLibrary'; continue }
    if ($inGroup -and $line -match '^hidden_printer=(.*)$') { return $Matches[1] }
  }
  return ''
}

# Ui-LoggedAfter "text": the last line since the mark with the text.
function Ui-LoggedAfter([string]$Text) {
  return Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) } | Select-Object -Last 1
}

# The base library: MitCad's component library id with two of its
# printers, recorded with git and fetched as MitCad fetches it.
$base = Join-Path $work 'base-library'
$output = & $Cli library init $base --id mitcad-parts --name 'Print fixtures' --license MIT
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library init: $($output -join ' / ')" }
$printers = Join-Path $base 'print\printers'
New-Item -ItemType Directory -Force $printers | Out-Null
$source = Join-Path $PSScriptRoot 'libraries\print-library\print\printers'
Copy-Item (Join-Path $source 'bambu-lab-a1.json'), (Join-Path $source 'bambu-lab-a1-mini.json') $printers
$manifestPath = Join-Path $base 'mitcad-library.json'
$manifest = Get-Content -Raw $manifestPath | ConvertFrom-Json
$manifest | Add-Member -Force -NotePropertyName 'library_version' -NotePropertyValue '1.0.0'
$manifest | Add-Member -Force -NotePropertyName 'printers' -NotePropertyValue @(
  [ordered]@{ id = 'bambu-lab-a1'; path = 'print/printers/bambu-lab-a1.json' },
  [ordered]@{ id = 'bambu-lab-a1-mini'; path = 'print/printers/bambu-lab-a1-mini.json' })
# Without a byte order mark, as the CLI writes it.
[System.IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 10))
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'Built-in printers'), @('tag', 'v1.0.0'))) {
  $output = & $git.Source -C $base -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
$output = & $Cli library fetch $base
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library fetch: $($output -join ' / ')" }

# The designs: a box, and a box whose printer is Bambu Lab A1 (its copy).
$design = Join-Path $work 'box.mitcad'
Run-Cli 'box' @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}}
]
'@ @('--save', $design)
$used = Join-Path $work 'used.mitcad'
Run-Cli 'used' @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bambu-lab-a1", "library": "base"}
]
'@ @('--save', $used)

# Click-InWindow "text" [button]: a click on a logged place of the window
# Ui-FocusDialog chose (as tools/ui-windows-print-managers-test.ps1 does):
# the places are in the main window's coordinates.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class HideClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InWindow([string]$Text, [int]$Button = 1) {
  $at = Ui-LoggedAt $Text
  $main = New-Object HideClient+Point
  $dialog = New-Object HideClient+Point
  [HideClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [HideClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y $Button } finally { $script:UiWindow = $window }
}

# Menu-Choose "entry": the context menu's entry chosen with the keys, as
# the menu opens with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step "choose $Entry" {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}

# Open-Printers: the Printers manager (F8) from the main window.
function Open-Printers {
  Ui-FocusMain
  Ui-Activate
  Ui-Step 'Printers (F8)'                { Ui-Key 'F8' }
  Ui-ExpectNew 'Printers opened' 'the Printers manager opened'
}

# Printer-Options: the Printer box's options as Send to Slicer (F7) lists them.
function Printer-Options {
  Ui-FocusMain
  Ui-Activate
  Ui-Mark
  Ui-Step 'Send to Slicer (F7)'          { Ui-Key 'F7' }
  Ui-ExpectNew 'Panel Send to Slicer options printer: ' "the Printer box's options"
  return "$(Ui-LoggedAfter 'Panel Send to Slicer options printer: ')"
}

Ui-StartApp 'hide-built-ins' @('--open', $design)

Write-Host '--- 1. Printers: Bambu Lab A1 hidden'
Ui-Mark
Open-Printers
Ui-ExpectNew 'Printers groups: This design 0 | My library 0 | Built-in 2' 'the two built-in printers'
Ui-ExpectNew 'Printers folded: none' "no printer of the user's: Built-in open"
Ui-ExpectNew "Printers mitcad item $a1 at " 'Bambu Lab A1 has its place'
Ui-FocusDialog '^Printers$'
Ui-Mark
Ui-Step 'Bambu Lab A1'                   { Click-InWindow "Printers mitcad item $a1" }
Ui-ExpectNew "Profile selected: printer $a1 (mitcad)" 'Bambu Lab A1 selected'
Ui-ExpectNew 'Printers hide at ' 'Hide offered for a built-in printer'
Ui-Mark
Ui-Step 'Hide'                           { Click-InWindow 'Printers hide' }
Ui-ExpectNew "Printers hidden: $a1" 'hidden'
Ui-ExpectNew 'Printers groups: This design 0 | My library 0 | Built-in 1' 'gone from the list'
Ui-ExpectNew "Printers row: printer/bambu-lab-a1 $a1 (mitcad), hidden" 'its row hidden'
Ui-ExpectNew 'Printers show_hidden at ' 'Show hidden appears'
if ((Hidden-Setting) -ne 'bambu-lab-a1') { Ui-Fail "the setting names '$(Hidden-Setting)', not bambu-lab-a1" }
Write-Host "ok   the user's setting names it"
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Write-Host '--- 2. Send to Slicer: the Printer box without it'
$options = Printer-Options
if ($options.Contains($a1)) { Ui-Fail "the Printer box still lists ${a1}: $options" }
if (-not $options.Contains($mini)) { Ui-Fail "the Printer box does not list ${mini}: $options" }
Write-Host "ok   the Printer box lists $mini, not $a1"
Ui-Mark
Ui-Step 'close the panel (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed'

Write-Host '--- 3. Show hidden, Unhide and Hide in the context menu'
Ui-Mark
Open-Printers
Ui-ExpectNew 'Printers show_hidden at ' 'Show hidden under the list'
Ui-FocusDialog '^Printers$'
Ui-Mark
Ui-Step 'Show hidden'                    { Click-InWindow 'Printers show_hidden' }
Ui-ExpectNew 'Printers show hidden: on' 'Show hidden on'
Ui-ExpectNew 'Printers groups: This design 0 | My library 0 | Built-in 2' 'listed again'
Ui-ExpectNew "Printers grayed: $a1" 'grayed'
Ui-ExpectNew "Printers mitcad item $a1 at " 'its place in the list'
Ui-Mark
Ui-Step 'its context menu'               { Click-InWindow "Printers mitcad item $a1" 3 }
Ui-ExpectNew 'Context menu: Duplicate | Unhide' 'Duplicate and Unhide'
Menu-Choose 'Unhide'
Ui-ExpectNew "Printers unhidden: $a1" 'unhidden'
Ui-ExpectNew 'Printers grayed: none' 'no longer grayed'
if (Hidden-Setting) { Ui-Fail "the setting still names '$(Hidden-Setting)'" }
Write-Host 'ok   the setting names none'
Ui-ExpectNew "Printers mitcad item $a1 at " 'listed'
Ui-Mark
Ui-Step 'its context menu'               { Click-InWindow "Printers mitcad item $a1" 3 }
Ui-ExpectNew 'Context menu: Duplicate | Hide' 'Duplicate and Hide'
Menu-Choose 'Hide'
Ui-ExpectNew "Printers hidden: $a1" 'hidden again'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Write-Host '--- 4. A design that uses it keeps it as its own'
Ui-StartApp 'hide-built-ins-used' @('--open', $used)
Ui-Mark
Open-Printers
Ui-ExpectNew 'Printers groups: This design 1 | My library 0 | Built-in 1' "This design's copy, the built-in hidden"
Ui-ExpectNew "$a1 (design), the design's" "This design's printer"
Ui-FocusDialog '^Printers$'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }
$options = Printer-Options
if (-not $options.Contains($a1)) { Ui-Fail "the Printer box does not list the design's ${a1}: $options" }
Write-Host "ok   the Printer box lists the design's $a1"
Ui-Mark
Ui-Step 'close the panel (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed'

Write-Host '--- 5. A printer of My library: Built-in folded'
Ui-StopApp
Run-Cli 'library' @'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}}
]
'@ @('--save', (Join-Path $work 'scratch.mitcad'))
Ui-StartApp 'hide-built-ins-own' @('--open', $design)
Ui-Mark
Open-Printers
Ui-ExpectNew 'Printers groups: This design 0 | My library 1 | Built-in 1' "My library's Bench X"
Ui-ExpectNew 'Printers folded: Built-in' 'Built-in starts folded'
Ui-ExpectNew 'Printers user item Bench X 0.4 nozzle at ' "My library's printer in sight"
Ui-Sync
if (@(Ui-NewLines | Where-Object { $_.Contains('Printers mitcad item ') }).Count -gt 0) {
  Ui-Fail 'a built-in row is in sight while Built-in is folded'
}
Write-Host 'ok   no built-in row in sight'
Ui-FocusDialog '^Printers$'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Ui-Finish 'UI Windows hide built-ins test'
