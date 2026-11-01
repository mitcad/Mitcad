# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/TablesPage.cpp app/framework/FormulaEditor.cpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterSuggest.cpp core/model/src/tables.rs
# Input-driven Windows UI test of tables in Edit Parameters (mitcad#667,
# docs/table-lookups.md) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), on the demo block (60 x 40 x 20 mm: d1, d2, d3), as
# tools/ui-tables-test.sh does on Linux, by the keyboard (the clicks of the
# helpers go to the main window, and the dialog is a window of its own):
#   1. Edit Parameters (a shortcut of the test's), the Tables tab
#      (Alt+T), New (Alt+N): the table Screws, a Key column of texts and a
#      Value column in mm with one empty row.
#   2. Cells typed: M4 7, Add Row (Alt+W), M5; abc in Value is refused and
#      stays red with the model's reason; 8.5 is taken.
#   3. The Parameters tab (Alt+P), d3 found by typing its name, Edit
#      Formula (Alt+F): XLOOKUP('M5'; Screws.Key; Screws. completes the
#      table's columns, V and Tab put Value in, Ctrl+Enter: the block
#      8.5 mm high.
#   4. The Tables tab: d3 uses the table; M5's Value 10 makes the block
#      10 mm high.
#   5. Import CSV... (Alt+I): a file with M5's Value 9, previewed (d3 from
#      10 mm to 9 mm) and imported; the block 9 mm high; Undo takes the
#      import back in one step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-tables-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-tables'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.parameters' = 'F7' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$csv = Join-Path $work 'Screws.csv'
[IO.File]::WriteAllText($csv, "Key [Text],Value [mm]`r`nM4,7`r`nM5,9`r`nM6,10`r`n")

# The last change of the block's volume logged since the mark.
function Expect-Volume([double]$Expected, [string]$Description) {
  $volume = $null
  for ($i = 0; $i -lt 75 -and $null -eq $volume; $i++) {
    foreach ($line in @(Ui-NewLines)) {
      if ($line -match 'Body Body1 \(F2\.b0\): volume [0-9.]* -> ([0-9.]*) mm3') { $volume = $Matches[1] }
    }
    if ($null -eq $volume) { Start-Sleep -Milliseconds 200 }
  }
  if ($null -eq $volume) { Ui-Fail "${Description}: no volume logged" }
  $v = [double]::Parse($volume, [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Pow($v - $Expected, 2) -ge [Math]::Pow(1e-6 * $Expected, 2) + 1e-4) {
    Ui-Fail "${Description}: volume $v mm3, expected $Expected"
  }
  Write-Host "ok   ${Description}: $v mm3"
}

Ui-StartApp 'tables' @('--demo')

Write-Host '--- 1. A new table'
Ui-Mark
Ui-Step 'Edit Parameters (F7)'          { Ui-Key 'F7' }
Ui-ExpectNew 'Parameters dialog opened' 'the dialog opened'
Ui-FocusDialog '^Parameters$'
Ui-Mark
Ui-Step 'the Tables tab (Alt+T)'          { Ui-Key 'alt+t' }
Ui-ExpectNew 'Parameters tab shown: Tables' 'the Tables tab'
Ui-ExpectNew 'Tables: none' 'no tables yet'
Ui-Mark
Ui-Step 'New (Alt+N)'                     { Ui-Key 'alt+n' }
Ui-ExpectNew 'New Table dialog opened: Table1' 'New Table asks for a name'
Ui-FocusDialog '^New Table$'
Ui-Step 'Screws (Enter)'                  { Ui-Key 'ctrl+a'; Ui-Type 'Screws'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws created' 'the table made'
Ui-ExpectNew 'Table Screws columns: Key [Text], Value [mm]' 'a key column and a value column'
Ui-ExpectNew 'Table Screws rows: [[null,null]]' 'one empty row'

Write-Host '--- 2. Cells typed, a wrong one refused'
Ui-FocusDialog '^Parameters$'
Ui-Mark
Ui-Step 'M4 (Enter)'                      { Ui-Type 'M4'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 1 Key: M4' 'a text cell'
Ui-Mark
Ui-Step 'the next cell: 7 (Enter)'        { Ui-Key 'Right'; Ui-Type '7'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 1 Value: 7' 'a number cell'
Ui-ExpectNew 'Table Screws rows: [["M4",7]]' 'the row as the model keeps it'
Ui-Mark
Ui-Step 'Add Row (Alt+W)'                 { Ui-Key 'alt+w' }
Ui-ExpectNew 'Table Screws rows: [["M4",7],[null,null]]' 'a new empty row after it'
Ui-Mark
Ui-Step 'its Key: M5 (Enter)'             { Ui-Key 'Left'; Ui-Type 'M5'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 2 Key: M5' 'the new row''s key'
Ui-Mark
Ui-Step 'its Value: abc (Enter)'          { Ui-Key 'Right'; Ui-Type 'abc'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 2 Value: abc refused: ' 'a text in a column of numbers is refused'
Ui-ExpectNew 'Table Screws refused cells: row 2 Value: abc (' 'it stays in its cell, red, with the reason'
Ui-Mark
Ui-Step '8.5 (Enter)'                     { Ui-Type '8.5'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 2 Value: 8.5' 'a number taken'
Ui-ExpectNew 'Table Screws rows: [["M4",7],["M5",8.5]]' 'the table'
Ui-ExpectNew 'Table Screws refused cells: none' 'nothing refused any more'

Write-Host '--- 3. A dimension through XLOOKUP, typed in the Formula Editor'
Ui-Mark
Ui-Step 'the Parameters tab (Alt+P)'      { Ui-Key 'alt+p' }
Ui-ExpectNew 'Parameters tab shown: Parameters' 'the Parameters tab'
Ui-Step 'go to d3 (type its name)'        { Ui-Type 'd3' }
Start-Sleep -Milliseconds 800
Ui-Mark
Ui-Step 'Edit Formula (Alt+F)'            { Ui-Key 'alt+f' }
Ui-FocusDialog '^Formula Editor$'
Ui-ExpectNew 'Formula editor OK at' 'the Formula Editor opened'
Ui-Mark
Ui-Step 'the lookup up to Screws.'        { Ui-Key 'ctrl+a'; Ui-Type "XLOOKUP('M5'; Screws.Key; Screws." }
Ui-ExpectNew 'Formula completions for "Screws.": Key, Value' 'the table''s columns offered'
Ui-Mark
Ui-Step 'V'                               { Ui-Type 'V' }
Ui-ExpectNew 'Formula completions for "Screws.V": Value' 'filtered by what is typed'
Ui-Step 'Tab'                             { Ui-Key 'Tab' }
Ui-ExpectNew "Formula completion inserted Value: XLOOKUP('M5'; Screws.Key; Screws.Value" 'Value put in'
Ui-Mark
Ui-Step ') and OK (Ctrl+Enter)'           { Ui-Type ')'; Ui-Key 'ctrl+Return' }
Ui-FocusDialog '^Parameters$'
Ui-ExpectNew "XLOOKUP('M5'; Screws.Key; Screws.Value) = 8.5 mm" 'd3 looks its value up'
Expect-Volume 20400 'the block 60 x 40 x 8.5'

Write-Host '--- 4. A cell edited: the block follows'
Ui-Mark
Ui-Step 'the Tables tab (Alt+T)'          { Ui-Key 'alt+t' }
Ui-ExpectNew 'Table Screws used by: d3' 'the table says d3 uses it'
Ui-Mark
Ui-Step 'M5''s Value: 10 (Enter)'         { Ui-Type '10'; Ui-Key 'Return' }
Ui-ExpectNew 'Table Screws row 2 Value: 10' 'the cell changed'
Ui-ExpectNew 'Table Screws changed parameters: d3' 'd3 follows the cell'
Expect-Volume 24000 'the block 60 x 40 x 10'

Write-Host '--- 5. Import CSV...'
Ui-Mark
Ui-Step 'Import CSV... (Alt+I)'           { Ui-Key 'alt+i' }
Ui-FocusDialog '^Import Table from CSV$'
Ui-Step 'type the path (Enter)'           { Ui-Key 'ctrl+a'; Ui-Type $csv; Start-Sleep -Milliseconds 300; Ui-Key 'Return' }
Ui-FocusDialog '^Import Table$'
Ui-ExpectNew 'Table import preview of Screws.csv as Screws: ' 'the preview, for the table of the file''s name'
Ui-ExpectNew 'Table import column Value [mm]' 'the file''s columns'
Ui-ExpectNew 'Table import changes d3: 10 mm -> 9 mm' 'd3 would change'
if (@(Ui-NewLines | Where-Object { $_.Contains('Body Body1 (F2.b0): volume') }).Count -gt 0) {
  Ui-Fail 'the preview changed the block'
}
Ui-Mark
Ui-Step 'Import (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Imported table Screws from Screws.csv: ' 'imported'
Ui-ExpectNew 'Table Screws rows: [["M4",7],["M5",9],["M6",10]]' 'the file''s rows'
Expect-Volume 21600 'the block 60 x 40 x 9'
Ui-FocusDialog '^Parameters$'
Ui-Step 'close (Esc)'                     { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                   { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Import Table Screws' 'the import was one undo step'
Expect-Volume 24000 '10 mm high again'
if ("$(Ui-LogText)".Contains('Recompute failed')) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows tables test'
