# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/RibbonLabels.cpp app/view/ViewController.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/ui-scripts/ribbon-labels.mitcad-ui app/main.cpp
# Preferences > Display > Toolbar labels (mitcad#1936, GitHub
# mitcad/Mitcad#356) in the build VM, as tools/ui-ribbon-labels-test.sh on
# Linux: the shared steps tools/ui-scripts/ribbon-labels.mitcad-ui in the
# Floating and the Docked layout. Labels under icons by default in both
# (the toolbar panel, mitcad#1990, the Docked layout's too since
# mitcad#2082); Labels under
# icons makes the rows higher and puts Sketch, Extrude, Revolve and Hole
# under the SOLID tab's Create buttons, Create and Modify stay whole in a
# wide window and the groups fold from the right in a narrow one; Icons
# only again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-ribbon-labels-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-ribbon-labels'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/ribbon-labels.mitcad-ui'

# The shared script in one layout.
function Run-Layout([string]$Layout) {
  # The option's index without a choice: Labels under icons in both
  # layouts (the toolbar panel's, mitcad#2082).
  $env:MITCAD_TEST_LABELS_DEFAULT = '1'
  # Without the choice the run before saved (its last step chooses Icons
  # only), so that the default shows.
  $ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
  if (Test-Path $ini) {
    Set-Content -Encoding ascii $ini (Get-Content $ini | Where-Object { $_ -notmatch '^ribbonLabels=' })
  }
  Ui-StartApp "ribbon-labels-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail "ribbon labels driver timed out ($Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "ribbon labels driver failed ($Layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the ribbon's labels ($Layout)"
  Ui-ExpectLog 'Ribbon group SOLID/CREATE labels: Sketch, Extrude, Revolve, Hole' "the Create group's labels ($Layout)"
  Ui-StopApp
}

Run-Layout 'floating'
Run-Layout 'docked'
Write-Host 'PASS: the ribbon shows its labels under the icons and folds its groups from the right'
