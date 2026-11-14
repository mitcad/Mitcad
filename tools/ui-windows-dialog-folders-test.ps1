# SPDX-License-Identifier: MIT
# Input-driven Windows UI test of the file dialogs' folders per group
# (mitcad#498), as tools/ui-dialog-folders-test.sh does on Linux:
#   1. On the demo block: Save As into one folder (the Open group), Export
#      a STEP file into another, Import a copy of it from a third; each
#      dialog's first start is the design's folder.
#   2. Import again starts in the import folder, Export in the export
#      folder, Open in the design's; a cancelled dialog remembers nothing.
#   3. The app killed and started again with no design: Import, Export and
#      Open each start in their own remembered folder.
# Qt's own file dialog (--no-native-dialogs); the app logs where a group's
# dialog starts ("Dialog folder import: <folder>") and what it remembers.
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-test.ps1 does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-dialog-folders-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-dialog-folders'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Popups such as the File menu close at once in session 0: shortcuts.
Ui-Init $App $Out $QtBin @{ 'file.export' = 'F9'; 'file.import' = 'F11' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
$designs = Join-Path $work 'designs'
$exports = Join-Path $work 'exports'
$imports = Join-Path $work 'imports'
New-Item -ItemType Directory -Force $designs, $exports, $imports | Out-Null
# The app logs folders with forward slashes.
function Logged([string]$Path) { return $Path -replace '\\', '/' }

# A path typed into a file dialog's focused field, then Enter.
function Type-Into-Dialog([string]$Title, [string]$Path) {
  Ui-FocusDialog $Title
  Ui-Key 'ctrl+a'
  Ui-Type $Path
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
  Ui-FocusMain
}

# The dialog closed with Escape, the keys back to the main window.
function Cancel-Dialog([string]$Title) {
  Ui-FocusDialog $Title
  Ui-Key 'Escape'
  Ui-FocusMain
}

Write-Host '--- Save As, Export and Import into folders of their own'
Ui-StartApp 'first' @('--demo')
$design = Join-Path $designs 'block.mitcad'
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'         { Ui-Key 'ctrl+shift+s' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Save As$' $design }
Ui-ExpectNew "Saved $(Logged $design)" 'saved through the dialog'
Ui-ExpectNew "Dialog folder open remembered: $(Logged $designs)" "the Open group remembers the design's folder"

$step = Join-Path $exports 'block.step'
Ui-Mark
Ui-Step 'export (F9)'                    { Ui-Key 'F9' }
Ui-ExpectNew "Dialog folder export: $(Logged $designs)" "Export starts in the design's folder at first"
Ui-Step 'type the path'                  { Type-Into-Dialog '^Export$' $step }
Ui-ExpectNew "Exported ${step}: step, 1 body" 'exported'
Ui-ExpectNew "Dialog folder export remembered: $(Logged $exports)" 'the Export group remembers its folder'

$part = Join-Path $imports 'part.step'
Copy-Item $step $part
Ui-Mark
Ui-Step 'import (F11)'                   { Ui-Key 'F11' }
Ui-ExpectNew "Dialog folder import: $(Logged $designs)" "Import starts in the design's folder at first"
Ui-Step 'type the path'                  { Type-Into-Dialog '^Import$' $part }
# The design has content: asked where the file goes (mitcad#1629).
Ui-ExpectNew 'Import place dialog opened: part.step' 'asked where the file goes'
Ui-Step 'into a new component (Enter)'   { Ui-FocusDialog '^Where to Import$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Imported part.step as' 'imported'
Ui-ExpectNew "Dialog folder import remembered: $(Logged $imports)" 'the Import group remembers its folder'
Ui-Step 'undo the import (Ctrl+Z)'       { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Import part.step' 'undone: the design as saved'

Write-Host '--- Each group starts in its own folder'
Ui-Mark
Ui-Step 'import again (F11)'             { Ui-Key 'F11' }
Ui-ExpectNew "Dialog folder import: $(Logged $imports)" 'Import starts in the import folder'
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Import$' }
Ui-Mark
Ui-Step 'export again (F9)'              { Ui-Key 'F9' }
Ui-ExpectNew "Dialog folder export: $(Logged $exports)" 'Export starts in the export folder, with a design open'
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Export$' }
Ui-Mark
Ui-Step 'open (Ctrl+O)'                  { Ui-Key 'ctrl+o' }
Ui-ExpectNew "Dialog folder open: $(Logged $designs)" 'Open keeps its own folder'
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Open$' }
Ui-Mark
Ui-Step 'import, cancelled (F11)'        { Ui-Key 'F11'; Cancel-Dialog '^Import$' }
if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Dialog folder import remembered') }) {
  Ui-Fail 'a cancelled dialog remembered a folder'
}
Write-Host 'ok   a cancelled dialog remembers nothing'

Write-Host '--- After a restart, with no design'
Ui-StartApp 'restarted'
Ui-Mark
Ui-Step 'import (F11)'                   { Ui-Key 'F11' }
Ui-ExpectNew "Dialog folder import: $(Logged $imports)" 'Import still starts in the import folder'
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Import$' }
Ui-Mark
Ui-Step 'export (F9)'                    { Ui-Key 'F9' }
Ui-ExpectNew "Dialog folder export: $(Logged $exports)" 'Export still starts in the export folder'
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Export$' }
Ui-Mark
Ui-Step 'open (Ctrl+O)'                  { Ui-Key 'ctrl+o' }
Ui-ExpectNew "Dialog folder open: $(Logged $designs)" "Open still starts in the design's folder"
Ui-Step 'cancel it (Escape)'             { Cancel-Dialog '^Open$' }

Ui-Finish 'UI Windows dialog folders test'
