# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFilamentChanges.cpp app/files/MainWindowPrint.cpp app/print/FilamentChangeDialog.cpp
# check-all sources: app/view/FilamentChangeOverlay.cpp app/MainWindowContextMenu.cpp core/model tools/cli
# check-all sources: app/files/MainWindowPlates.cpp app/framework/NoteRow.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# Send to Slicer's Filament changes (mitcad#1643, mitcad#1800) in the build
# VM, as tools/ui-filament-changes-test.sh on Linux: the shared steps
# tools/ui-scripts/filament-changes.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/filament-changes.json): no Change At row in the
# panel; in Filament Change a change by a typed height, a change by a plane
# and a pause by a face picked into its To Object field, and the edit of
# each; the view's print colours and ring, Remove, Undo and Redo, the
# normal look after the panel, and Filament Change Here... on the plane's
# and the face's context menu; the design saved keeps the changes at the
# plane and the face by their references.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-filament-changes-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-filament-changes'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'filament-changes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/filament-changes.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/filament-changes.mitcad-ui'
Ui-StartApp 'filament-changes' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'filament changes driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "filament changes driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "Send to Slicer's filament changes"
# The design keeps the edited change and the one at Plane1 by its reference.
$saved = Get-Content -Raw $design | ConvertFrom-Json
$events = @($saved.filament_changes[0].changes | ForEach-Object { if ($null -ne $_.at) { $_.at } else { $_.height } })
if ($events.Count -ne 2 -or [double]$events[0] -ne 14 -or $events[1] -ne 'F3') {
  Ui-Fail "the saved schedule: $($events -join ', '), expected 14, F3"
}
Write-Host 'ok   the design keeps the change at 14 mm and the one at Plane1'
$own = @($saved.filament_changes | Where-Object { $_.plate -eq 'plate1' })
if ($own.Count -ne 1 -or $own[0].changes.Count -ne 3 -or -not ("$($own[0].changes[2].at.face)".StartsWith('F2:end('))) {
  Ui-Fail "Plate 1's schedule: $($own | ConvertTo-Json -Depth 6 -Compress)"
}
Write-Host "ok   Plate 1 keeps its own schedule with the change at the box's top"
Ui-StopApp
Write-Host "PASS: Send to Slicer's filament changes"
