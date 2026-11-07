# SPDX-License-Identifier: MIT
# check-all sources: app/files app/WindowManager.cpp app/MainWindow.cpp app/MainWindowContextMenu.cpp app/browser/BrowserController.cpp app/framework/MessageBand.cpp tools/cli
# Linked designs while the holder is open (mitcad#1416), the steps of
# tools/ui-linked-design-test.sh (tools/ui-scripts/linked-design.mitcad-ui):
# the notice with Open Linked Design on an edit of a linked component (not
# on selecting it), the linked design opened in a tab of its own from the
# notice and the browser's context menu, "part.mitcad changed" with Update
# when the part is saved in its tab, Update and Update Link each one undo
# step. Here in the Docked layout: the buttons are the status bar's.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-linked-design'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/linked-design-part.json') --save (Join-Path $work 'part.mitcad')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'linked design part fixture creation failed' }
# The holder made in the part's folder: the link is part.mitcad.
Push-Location $work
try {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/linked-design-holder.json') --save (Join-Path $work 'holder.mitcad')
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'linked design holder fixture creation failed' }
} finally {
  Pop-Location
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/linked-design.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'linked-design' @('--ui=qt', '--no-mcp', '--no-recovery', '--open', (Join-Path $work 'holder.mitcad'))
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(360000)) { Ui-Fail 'linked design driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "linked design driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: quit' 'linked design notice, Open Linked Design, the changed-file notice and Update Link'
Ui-StopApp
Write-Host 'PASS: linked designs'
