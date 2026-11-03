# SPDX-License-Identifier: MIT
# check-all sources: app/printing app/files/MainWindowFiles.cpp app/sketch/SketchModel.cpp
# Input-driven Windows UI test of File > Print... (mitcad#1089) in the build
# VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-paper-print-test.sh does on Linux. The system's print dialog is
# replaced by a PDF file (MITCAD_TEST_PRINT_PDF), which the test reads:
#   - With nothing to print, Print is disabled ("Print available: nothing")
#     and Ctrl+P opens nothing.
#   - In an empty sketch it stays disabled (mitcad#1156: no blank page);
#     with a 300 x 200 mm rectangle typed in it is enabled, and Ctrl+P
#     opens the Print Sketch panel (A4 by Home in Paper: 1:1 on two pages in
#     one row, portrait), which is cancelled (Escape).
#   - Print... (Enter) prints the two tiles to the PDF: two A4 pages.
#   - Fit to page (Alt+F): one A4 landscape page.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-paper-print-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-paper-print'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Popups close at once in session 0: Create Sketch by a key of the test's own.
Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7' } -StepPause 0
$pdf = Join-Path $script:UiOut 'printed.pdf'
Remove-Item -Force $pdf -ErrorAction SilentlyContinue
$env:MITCAD_TEST_PRINT_PDF = $pdf

# The PDF's text (its page dictionaries are plain text).
function Pdf-Text {
  if (-not (Test-Path $pdf)) { Ui-Fail "no PDF $pdf" }
  return [IO.File]::ReadAllText($pdf, [Text.Encoding]::GetEncoding(28591))
}
function Pdf-Pages { return ([regex]::Matches((Pdf-Text), '/Type\s*/Page(?![A-Za-z])')).Count }
# The pages in A4 portrait or landscape: their media boxes within a point.
function Pdf-A4Pages([string]$Orientation) {
  $count = 0
  foreach ($m in [regex]::Matches((Pdf-Text), '/MediaBox\s*\[\s*0\s+0\s+([0-9.]+)\s+([0-9.]+)\s*\]')) {
    $w = [double]::Parse($m.Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
    $h = [double]::Parse($m.Groups[2].Value, [Globalization.CultureInfo]::InvariantCulture)
    if ($Orientation -eq 'landscape') { $t = $w; $w = $h; $h = $t }
    if ($w -gt 594 -and $w -lt 597 -and $h -gt 841 -and $h -lt 843) { $count++ }
  }
  return $count
}
# The panel's choices as logged last, when it opened or changed.
function Panel-State { return Ui-LastMatch '^Print panel(?:: | opened: [^,]*, )(.*)$' }

Ui-StartApp 'paper-print'

Write-Host '--- Nothing to print'
Ui-ExpectLog 'Print available: nothing' 'Print is disabled with nothing to print'
Ui-Mark
Ui-Step 'Print (Ctrl+P)'                   { Ui-Key 'ctrl+p' }
Ui-SyncOr 500
if (@(Ui-NewLines | Where-Object { $_.Contains('Print panel opened') }).Count -gt 0) {
  Ui-Fail 'Print opened its panel with nothing to print'
}
Write-Host 'ok   Ctrl+P does nothing'

Write-Host '--- A sketch: empty, nothing to print; drawn in, enabled; the panel opened and cancelled'
Ui-Mark
Ui-Step 'create sketch on XY (F7)'         { Ui-CreateSketch 'F7' 'xy' }
Ui-Step 'Print (Ctrl+P) in the empty sketch' { Ui-Key 'ctrl+p' }
Ui-SyncOr 500
if (@(Ui-NewLines | Where-Object { $_.Contains('Print available: sketch') -or $_.Contains('Print panel opened') }).Count -gt 0) {
  Ui-Fail 'Print is available in an empty sketch (a blank page, mitcad#1156)'
}
Write-Host 'ok   an empty sketch has nothing to print'
Ui-Step 'rectangle tool (R)'               { Ui-Key 'r' }
Ui-Step 'first corner at the origin'       { Ui-SketchClick 0 0 }
Ui-Step 'width 300 (Tab) height 200'       { Ui-Type '300'; Ui-Key 'Tab'; Ui-Type '200' }
Ui-Step 'confirm (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 300 x 200 mm at (0, 0)' 'a 300 x 200 rectangle'
Ui-ExpectNew 'Print available: sketch' 'Print is enabled in the sketch with its rectangle'
Ui-Step 'end the tool (Esc)'               { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Print (Ctrl+P)'                   { Ui-Key 'ctrl+p' }
Ui-FocusDialog '^Print Sketch$'
Ui-ExpectNew 'Print panel opened: Sketch' 'the Print Sketch panel'
Ui-ExpectNew ', 1:1, ' 'at 1:1 to start with'
Ui-Step 'Paper: A4 (Alt+P, Home)'          { Ui-Key @('alt+p', 'Home') }
Ui-SyncOr 500
$state = Panel-State
if ($state -ne '304 x 204 mm, 1:1, A4 portrait, 2 page(s), 1 x 2') {
  Ui-Fail "the panel is not at two A4 pages in one row: $state"
}
Write-Host "ok   two A4 pages, one row, portrait: $state"
Ui-Mark
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Print panel cancelled' 'the panel cancelled'
if (Test-Path $pdf) { Ui-Fail 'the cancelled panel printed' }

Write-Host '--- 1:1: two tiles'
Ui-Mark
Ui-Step 'Print (Ctrl+P)'                   { Ui-Key 'ctrl+p' }
Ui-FocusDialog '^Print Sketch$'
Ui-ExpectNew 'Print panel opened: Sketch1, 304 x 204 mm, 1:1, A4 portrait, 2 page(s), 1 x 2' 'the panel again, with the paper chosen'
Ui-Step 'Print (Enter)'                    { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew "Print dialog skipped (test): printing to $pdf" "the test's PDF in place of the dialog"
Ui-ExpectNew 'Printed Sketch1: 2 page(s), 1:1, 1 x 2 to ' 'two pages printed' 20
if ((Pdf-Pages) -ne 2) { Ui-Fail "the PDF has $(Pdf-Pages) pages, not 2" }
if ((Pdf-A4Pages 'portrait') -ne 2) { Ui-Fail "the PDF has $(Pdf-A4Pages 'portrait') A4 portrait pages, not 2" }
Write-Host 'ok   the PDF has two A4 pages'

Write-Host '--- Fit to page: one page'
Ui-Mark
Ui-Step 'Print (Ctrl+P)'                   { Ui-Key 'ctrl+p' }
Ui-FocusDialog '^Print Sketch$'
Ui-ExpectNew 'Print panel opened: Sketch1' 'the panel'
Ui-Step 'Fit to page (Alt+F)'              { Ui-Key 'alt+f' }
Ui-ExpectNew 'Print panel: 304 x 204 mm, fit, A4 landscape, 1 page(s), 1 x 1, scale 0.911' 'one page, landscape, the picture at 91 %'
Ui-Step 'Print (Enter)'                    { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Printed Sketch1: 1 page(s), fit, 1 x 1 to ' 'one page printed' 20
if ((Pdf-Pages) -ne 1) { Ui-Fail "the PDF has $(Pdf-Pages) pages, not 1" }
if ((Pdf-A4Pages 'landscape') -ne 1) { Ui-Fail "the PDF's page is not A4 landscape" }
Write-Host 'ok   the PDF has one A4 landscape page'

Ui-Finish 'Windows UI paper print test'
