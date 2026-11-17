# SPDX-License-Identifier: MIT
# check-all sources: app/gravity app/MainWindowGravity.cpp app/commands/GravityCommands.cpp app/MainWindow.cpp
# check-all sources: app/MainWindowContextMenu.cpp app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/browser/DocumentSnapshot.cpp app/framework/DocumentSession.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model-api/src/gravity.rs core/model/src/gravity.rs core/model/src/document/gravity.rs
# check-all sources: core/model-api/src/flex_hang.rs core/model/src/document/flex_hang.rs core/model-base/src/flex_drape.rs
# check-all sources: core/ffi/src/kernel/flex_drape.rs core/cpp/bridge/flex_drape.cpp geometry/src/flex_drape.cpp
# check-all sources: tools/ui-scripts/gravity.mitcad-ui tools/ui-scripts/gravity.json tools/cli
# check-all sources: app/flex/FlexRope.cpp app/flex/FlexEdit.cpp core/model/src/features/flex.rs
# Let It Hang and its panel's direction of gravity (mitcad#1713) in the
# build VM, as tools/ui-gravity-test.sh on Linux: the shared steps
# tools/ui-scripts/gravity.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/gravity.json: a plate, a step on it, a plank above the
# step's edge and a hose over them, no direction of gravity), in the
# Docked and in the Floating layout: Let It Hang from the hose's context
# menu asks for a direction; the plate's top face picked, the hose falls
# from its ends, its middle onto the plank where it rests lying on it; its
# middle node pinned and let go; the damping changed; dragged by its middle
# and let go; OK keeps its shape with the direction as one undo step, Undo
# takes both back; from the ribbon again, the length longer and its end
# let go, Esc puts it back and takes the direction back; once more, Clear
# Gravity in the panel, a face again, Esc. The marker only while the
# panel is open; no row of gravity in the browser.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-gravity-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-gravity'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/gravity.mitcad-ui'

# Where the hose rested before it was kept (the last "Hang rest F4" line
# with its lowest point): lying on the plank (three of its points or more
# on a body) and nowhere through the plate (its lowest point at least the
# plate's top, 10, and the hose's radius, 3, high). Fallen through the
# plank it would touch nothing.
function Check-Rest([string]$Chrome) {
  # The rest lines before the kept shape, at the starts of lines (not the
  # driver's echo of the script's wait-log, which has the same text).
  $text = "$(Ui-LogText)"
  $locked = [regex]::Match($text, '(?m)^Hang locked F4: ')
  if (-not $locked.Success) { Ui-Fail "no kept hose logged ($Chrome)" }
  $before = [regex]::Matches($text.Substring(0, $locked.Index), '(?m)^Hang rest F4: [^\r\n]*')
  if ($before.Count -eq 0) { Ui-Fail "no resting hose logged ($Chrome)" }
  $line = $before[$before.Count - 1].Value
  $match = [regex]::Match($line, 'touching ([0-9]+), .*, lowest ([-0-9.]+) along ')
  if (-not $match.Success) { Ui-Fail "the rest line cannot be read ($Chrome): $line" }
  $culture = [Globalization.CultureInfo]::InvariantCulture
  $touching = [int]::Parse($match.Groups[1].Value, $culture)
  $lowest = [double]::Parse($match.Groups[2].Value, $culture)
  if ($touching -lt 3 -or $lowest -lt 12.9) {
    Ui-Fail "the hose does not lie on the plank ($Chrome): $line"
  }
  Write-Host "ok   the hose lies on the plank, $touching points on it, its lowest point at $lowest ($Chrome)"
}

foreach ($chrome in @('docked', 'floating')) {
  Write-Host "--- Let It Hang in the $chrome layout"
  $design = Join-Path $work "gravity-$chrome.mitcad"
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/gravity.json') --save $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
  Ui-StartApp "gravity-$chrome" @('--ui=qt', '--no-mcp', "--chrome=$chrome", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(360000)) { Ui-Fail "the gravity driver timed out ($chrome)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "the gravity driver failed ($chrome, exit code $exitCode)"
  }
  Ui-ExpectLog 'Hang panel opened' "Let It Hang opened its panel from the hose's context menu ($chrome)"
  Ui-ExpectLog 'Preview Let It Hang: not built: Pick a planar face' "without a direction the panel asks for one ($chrome)"
  Ui-ExpectLog 'Gravity direction: 0.000,0.000,-1.000 from face' "gravity pulls against the plate's top face ($chrome)"
  Ui-ExpectLog 'Gravity marker: arrow ' "the marker shows the direction while the panel is open ($chrome)"
  Ui-ExpectLog 'Hang started F4 (Hose 6): ' "the hose let hang once the direction was given ($chrome)"
  Ui-ExpectLog "Hang press beside F4: the view's" "a press beside the hose is the view's ($chrome)"
  Ui-ExpectLog 'Hang pin n2 pinned' "a click on the middle node pinned it ($chrome)"
  Ui-ExpectLog 'Hang pin n2 let go' "another click let it go ($chrome)"
  Ui-ExpectLog 'Hang damping 0.800' "the damping changed in the panel ($chrome)"
  Ui-ExpectLog 'Hang drag of F4 started at ' "a press on the hose grabbed it ($chrome)"
  Ui-ExpectLog 'Hang drag of F4 released after ' "the release let it settle ($chrome)"
  Ui-ExpectLog 'Hang locked F4: nodes n1,' "OK kept its shape ($chrome)"
  Ui-ExpectLog 'Let It Hang: its steps merged into Edit Flex1' "the direction and the shape are one undo step ($chrome)"
  Check-Rest $chrome
  Ui-ExpectLog 'Gravity marker: none' "the marker went with the panel ($chrome)"
  Ui-ExpectLog 'Undo: Edit Flex1' "one undo step takes them back ($chrome)"
  Ui-ExpectLog 'Hang length 200.000 mm' "the length changed in the panel ($chrome)"
  Ui-ExpectLog 'Hang ends pinned free' "End pinned cleared let its end go ($chrome)"
  Ui-ExpectLog 'Hang cancelled: F4 put back' "Esc put it back ($chrome)"
  Ui-ExpectLog 'Let It Hang: 1 direction change(s) taken back' "Esc took the direction back ($chrome)"
  Ui-ExpectLog 'Gravity cleared' "Clear Gravity in the panel cleared the direction ($chrome)"
  Ui-ExpectLog 'Let It Hang: 3 direction change(s) taken back' "Esc took every direction change back ($chrome)"
  Ui-ExpectLog 'TestDriver: done' "the script ran to its end ($chrome)"
  Ui-StopApp
}
Write-Host 'PASS: UI Windows gravity'
