# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFilamentChanges.cpp app/files/MainWindowPrint.cpp app/print/FilamentChangeDialog.cpp
# check-all sources: app/view/FilamentChangeOverlay.cpp app/MainWindowContextMenu.cpp core/model tools/cli
# check-all sources: app/files/MainWindowPlates.cpp app/framework/NoteRow.cpp app/framework/CommandSession.cpp
# Send to Slicer's Filament changes (mitcad#1643) in the build VM, as
# tools/ui-filament-changes-test.sh on Linux: the shared steps
# tools/ui-scripts/filament-changes.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/filament-changes.json): a change added at a typed
# height, the view's print colours and ring, Undo and Redo, a change at a
# picked construction plane and its Remove, the normal look after the
# panel, and Filament Change Here... on the plane's context menu; the design
# saved keeps the change at the plane by its reference.
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
# The design keeps the typed change and the one at Plane1 by its reference.
$saved = Get-Content -Raw $design | ConvertFrom-Json
$events = @($saved.filament_changes[0].changes | ForEach-Object { if ($null -ne $_.at) { $_.at } else { $_.height } })
if ($events.Count -ne 2 -or [double]$events[0] -ne 12 -or $events[1] -ne 'F3') {
  Ui-Fail "the saved schedule: $($events -join ', '), expected 12, F3"
}
Write-Host 'ok   the design keeps the change at 12 mm and the one at Plane1'
$own = @($saved.filament_changes | Where-Object { $_.plate -eq 'plate1' })
if ($own.Count -ne 1 -or $own[0].changes.Count -ne 3 -or -not ("$($own[0].changes[2].at.face)".StartsWith('F2:end('))) {
  Ui-Fail "Plate 1's schedule: $($own | ConvertTo-Json -Depth 6 -Compress)"
}
Write-Host "ok   Plate 1 keeps its own schedule with the change at the box's top"
Ui-StopApp
Write-Host "PASS: Send to Slicer's filament changes"
