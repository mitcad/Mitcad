# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.cpp app/MainWindowJobs.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp app/framework/TestDriverModel.cpp tools/cli
# Edit in Place (mitcad#2321), the steps of tools/ui-edit-in-place-test.sh
# (tools/ui-scripts/edit-in-place.mitcad-ui): a linked part edited inside
# the assembly that holds it, its document taken from its open tab, a
# sketch on another component's face frozen with its notice, an extrusion
# of it, Save writing the part's file, Finish returning to the holder with
# the part's new body as one undo step, the part's tab showing the same
# edit, the holder's step undone and taken again with Update, and Esc
# finishing. Here in the Docked layout: the buttons are the status bar's.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-edit-in-place'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edit-in-place-part.json') --save (Join-Path $work 'part.mitcad')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'edit in place part fixture creation failed' }
# The holder made in the part's folder: the link is part.mitcad.
Push-Location $work
try {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edit-in-place-holder.json') --save (Join-Path $work 'holder.mitcad')
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'edit in place holder fixture creation failed' }
} finally {
  Pop-Location
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/edit-in-place.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'edit-in-place' @('--ui=qt', '--no-mcp', '--no-recovery', '--open', (Join-Path $work 'holder.mitcad'))
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(360000)) { Ui-Fail 'edit in place driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "edit in place driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: quit' 'Edit in Place from a tab, a frozen pick, Save, Finish, Undo with Update, and Esc'
Ui-StopApp
Write-Host 'PASS: Edit in Place'
