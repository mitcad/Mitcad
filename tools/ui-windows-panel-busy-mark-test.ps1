# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/panel-busy-mark.mitcad-ui tools/cli
# A preview being computed shown without resizing the panel (mitcad#1907,
# GitHub mitcad/Mitcad#361) in the build VM, as
# tools/ui-panel-busy-mark-test.sh on Linux: the shared steps
# tools/ui-scripts/panel-busy-mark.mitcad-ui on the cube that mitcad-cli
# makes (tools/ui-scripts/panel-message-top.json), every feature a preview
# evaluates slowed to 1 s, Docked and then Floating: Extrude1 edited, its
# distance arrow dragged up; the busy mark beside OK and Cancel shows and
# then hides, the message line stays hidden, and every busy line from the
# edit's first preview on gives the panel the same size and needed height
# ("Panel Extrude busy ..., panel WxH, needs H").
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-panel-busy-mark-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-panel-busy-mark'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'panel-busy-mark.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/panel-busy-mark.mitcad-ui'
# Every feature a preview evaluates 1 s longer, so that the previews
# outlast the busy mark's delay (Diagnostics.hpp).
$env:MITCAD_TEST_RECOMPUTE_DELAY_MS = '0,preview=1000'

foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "panel-busy-mark-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${layout}: the busy mark driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the busy mark driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Manipulator Extrude distance dragged to ' "${layout}: the distance's arrow dragged"
  Ui-ExpectLog 'Panel Extrude busy mark shown after ' "${layout}: the busy mark shown while the preview computes"
  Ui-ExpectLog 'Panel Extrude busy mark hidden after ' "${layout}: and hidden once it has ended"
  Ui-ExpectLog 'TestDriver: done' "${layout}: a preview being computed resizes nothing"
  # The busy lines from the end of the edit's first preview to the panel's
  # end (Esc).
  $all = @(Ui-LogLines)
  $from = -1
  for ($i = 0; $i -lt $all.Count; $i++) {
    if ($all[$i] -match '^Panel Extrude busy (mark hidden|ended) after ') { $from = $i; break }
  }
  if ($from -lt 0) { Ui-Fail "${layout}: the edit's first preview did not end" }
  $lines = @()
  for ($i = $from; $i -lt $all.Count; $i++) {
    if ($all[$i] -match '^Command Extrude cancelled') { break }
    if ($all[$i] -match '^Panel Extrude busy') { $lines += $all[$i] }
  }
  if (-not ($lines | Where-Object { $_ -match '^Panel Extrude busy mark shown after ' })) {
    Ui-Fail "${layout}: the busy mark did not show during the drag"
  }
  $sizes = @($lines | ForEach-Object {
      if ($_ -match ', (panel [0-9]+x[0-9]+, needs [0-9]+)$') { $Matches[1] }
    } | Sort-Object -Unique)
  if ($sizes.Count -ne 1) {
    Ui-Fail "${layout}: the panel's size changed while previews computed: $($sizes -join '; ')"
  }
  Write-Host "ok   ${layout}: $($lines.Count) busy lines, all with $($sizes[0])"
  Ui-StopApp
}
Remove-Item Env:MITCAD_TEST_RECOMPUTE_DELAY_MS -ErrorAction SilentlyContinue
Write-Host 'PASS: a preview being computed shown without resizing the panel'
