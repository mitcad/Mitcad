# SPDX-License-Identifier: MIT
# check-all sources: app/framework/AppSettings.cpp app/framework/DocumentSession.cpp
# check-all sources: app/view/ViewController.cpp tools/cli
# Input-driven Windows UI test of Preferences > General > Default units
# for new designs (mitcad#1036) in the build VM (tools/ui-windows-lib.ps1
# says how the input is posted), as the step "Default units for new
# designs" of tools/ui-units-test.sh does on Linux:
#   - The untitled design starts in mm (the default). The preference set to
#     in (Alt+U, Down x3, OK) leaves the open design in mm.
#   - New (Ctrl+N) makes a design in inches, not modified: the browser's
#     Units row shows in; a rectangle typed 2 x 1 is 2 x 1 in, an extrude's
#     distance typed 1 is an inch (a 2 x 1 x 1 in body).
#   - A new start: the untitled design is in inches. A design in mm made by
#     mitcad-cli and opened keeps mm.
#   - The preference back to mm (Up x3, OK): New is in mm again.
# The test's settings are its own (Ui-Init), so no other test sees the
# preference; it ends at mm all the same.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-default-units-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-default-units'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Popups close at once in session 0: Preferences' General page and Create
# Sketch by keys of the test's own.
Ui-Init $App $Out $QtBin @{ 'tools.preferences.general' = 'F8'; 'sketch.create' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'

# A design in millimetres to open while new designs are in inches.
$modelScript = Join-Path $work 'metric.json'
$metric = Join-Path $work 'metric.mitcad'
Set-Content -Encoding ascii $modelScript '[{"cmd": "sketch.create"}]'
$made = & $cli run $modelScript --save $metric
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $metric)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Expect-Units unit description: the browser's Design Settings opened, its
# Units row logged since the mark with the unit.
function Expect-Units([string]$Unit, [string]$Description) {
  Ui-Step 'select Design Settings'          { Ui-ClickLogged 'Browser Root/Design Settings' }
  Ui-Step 'expand it (Right)'               { Ui-Key 'Right' }
  Ui-ExpectNew "Browser Root/Design Settings/Units: $Unit at" $Description
}

# New-DesignUnits unit keys: Preferences > General (F8) > Default units for
# new designs (Alt+U) moved with the keys, OK; the choice saved.
function New-DesignUnits([string]$Unit, [string[]]$Keys) {
  Ui-Mark
  Ui-Step 'Preferences: General (F8)'       { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: General, ' 'Preferences on its General page'
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Default units for new designs (Alt+U)' { Ui-Key 'alt+u' }
  Ui-Step "choose $Unit ($($Keys -join ' '))" { Ui-Key $Keys }
  Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: default units for new designs $Unit" "new designs are to be in $Unit"
  if (-not (Select-String -Quiet -Path $ini -Pattern "^newDesigns=$Unit$")) {
    Ui-Fail "units/newDesigns is not $Unit in the settings"
  }
  Write-Host "ok   the setting is $Unit"
}

Write-Host '--- New designs in inches'
Ui-StartApp 'default-units'
Ui-Mark
Expect-Units 'mm' 'the untitled design starts in millimetres by default'
New-DesignUnits 'in' @('Down', 'Down', 'Down')
if (@(Ui-NewLines | Where-Object { $_.Contains('Design Settings/Units: in') }).Count -gt 0) {
  Ui-Fail "the preference changed the open design's units"
}
Write-Host 'ok   the open design stays in millimetres'
Ui-Mark
Ui-Step 'New (Ctrl+N)'                     { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New design in in' 'New makes a design in inches'
Ui-ExpectNew 'Opened the new design with Component1 active' 'the new design is open' 20
Ui-WaitIdle
$title = [UiInput]::Title($script:UiWindow)
if ($title -ne 'Untitled - MitCad') { Ui-Fail "the new design is modified: '$title'" }
Write-Host 'ok   the new design is not modified'
Expect-Units 'in' 'the browser shows Units: in'
Ui-Step 'create sketch on XY (F7)'         { Ui-CreateSketch 'F7' 'xy' }
Ui-Step 'rectangle tool (R)'               { Ui-Key 'r' }
Ui-Step 'first corner at the origin'       { Ui-SketchClick 0 0 }
Ui-Step 'width 2 (Tab) height 1'           { Ui-Type '2'; Ui-Key 'Tab'; Ui-Type '1' }
Ui-Step 'confirm (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 50.8 x 25.4 mm at (0, 0)' '2 x 1 typed are 2 x 1 in'
Ui-Step 'finish sketch (Ctrl+Enter)'       { Ui-Key 'ctrl+Return' }
Ui-ExpectLog 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'extrude (E)'                      { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the new profile'
Ui-Step 'type the distance 1'              { Ui-Type '1' }
Ui-ExpectNew 'Extrude Distance: 1 = 1 in' "the extrude's distance is in inches" 30
Ui-Step 'confirm (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (New Body), distance ' 'extrusion built' 30
Ui-ExpectVolume 'New body \S+ \(F2\.b0\): volume ([0-9.]+) mm3' 32774.128 '2 x 1 x 1 in (50.8 x 25.4 x 25.4 mm)'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- A new start and an opened design'
Ui-StartApp 'default-units-start'
Ui-Mark
Expect-Units 'in' 'the untitled design at the start is in inches'
Ui-StartApp 'default-units-open' @('--open', $metric)
Ui-ExpectLog 'metric.mitcad' 'the design in millimetres opened'
Ui-Mark
Expect-Units 'mm' 'an opened design keeps its millimetres'

Write-Host '--- Back to millimetres'
New-DesignUnits 'mm' @('Up', 'Up', 'Up')
Ui-Mark
Ui-Step 'New (Ctrl+N)'                     { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New design in mm' 'New makes a design in millimetres again'
Ui-ExpectNew 'Opened the new design with Component1 active' 'the new design is open' 20
Ui-WaitIdle
Expect-Units 'mm' 'the browser shows Units: mm'

Ui-Finish 'UI Windows default units test'
